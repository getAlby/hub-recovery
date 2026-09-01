use std::collections::{HashMap, HashSet};
use std::ops::Not;

use ldk_node::bitcoin::{Network, Txid};
use ldk_node::lightning::chain::channelmonitor::ANTI_REORG_DELAY;
use ldk_node::lightning::ln::types::ChannelId;
use ldk_node::{LightningBalance, Node, PendingSweepBalance};
use log::info;

use crate::scb::ChannelBackup;

/// Placeholder used when a channel from the node's balances has no matching
/// entry in the static channel backup file.
const UNKNOWN: &str = "<unknown>";

/// What a claimable lightning balance is currently waiting for.
enum ClaimableStatus {
    /// The channel's close transaction has not appeared on-chain yet. The
    /// restored channel monitor may be stale and report a 0 amount here; the
    /// real balance only becomes visible once the counterparty's commitment
    /// transaction confirms, so recovery is not complete while any balance is
    /// in this state.
    AwaitingChannelClose,
    /// The funds become sweepable once the chain reaches this height.
    AwaitingMaturity(u32),
    /// Claimable through some other pending on-chain resolution.
    Other,
}

fn get_ln_balance_channel_amount(balance: &LightningBalance) -> (ChannelId, u64, ClaimableStatus) {
    match balance {
        LightningBalance::ClaimableOnChannelClose {
            channel_id,
            amount_satoshis,
            ..
        } => (
            *channel_id,
            *amount_satoshis,
            ClaimableStatus::AwaitingChannelClose,
        ),
        LightningBalance::ClaimableAwaitingConfirmations {
            channel_id,
            amount_satoshis,
            confirmation_height,
            ..
        } => (
            *channel_id,
            *amount_satoshis,
            ClaimableStatus::AwaitingMaturity(*confirmation_height),
        ),
        LightningBalance::ContentiousClaimable {
            channel_id,
            amount_satoshis,
            ..
        } => (*channel_id, *amount_satoshis, ClaimableStatus::Other),
        LightningBalance::MaybeTimeoutClaimableHTLC {
            channel_id,
            amount_satoshis,
            claimable_height,
            ..
        } => (
            *channel_id,
            *amount_satoshis,
            ClaimableStatus::AwaitingMaturity(*claimable_height),
        ),
        LightningBalance::MaybePreimageClaimableHTLC {
            channel_id,
            amount_satoshis,
            ..
        } => (*channel_id, *amount_satoshis, ClaimableStatus::Other),
        LightningBalance::CounterpartyRevokedOutputClaimable {
            channel_id,
            amount_satoshis,
            ..
        } => (*channel_id, *amount_satoshis, ClaimableStatus::Other),
    }
}

/// Where a pending sweep currently is in its lifecycle. Once an output reaches
/// the sweeper there is no fixed number of blocks left to wait: the sweep
/// transaction is broadcast as soon as possible and then confirms at the pace
/// of the chain.
enum SweepStatus {
    /// The sweep transaction has not been broadcast yet.
    AwaitingBroadcast,
    /// The sweep transaction is waiting for its first confirmation.
    AwaitingConfirmation(Txid),
    /// The sweep transaction confirmed at the given height.
    Confirmed(u32, Txid),
}

fn get_pending_sweep_balance(
    amount: &PendingSweepBalance,
) -> (Option<ChannelId>, u64, SweepStatus) {
    match amount {
        PendingSweepBalance::PendingBroadcast {
            channel_id,
            amount_satoshis,
            ..
        } => (*channel_id, *amount_satoshis, SweepStatus::AwaitingBroadcast),
        PendingSweepBalance::BroadcastAwaitingConfirmation {
            channel_id,
            amount_satoshis,
            latest_spending_txid,
            ..
        } => (
            *channel_id,
            *amount_satoshis,
            SweepStatus::AwaitingConfirmation(*latest_spending_txid),
        ),
        PendingSweepBalance::AwaitingThresholdConfirmations {
            channel_id,
            amount_satoshis,
            confirmation_height,
            latest_spending_txid,
            ..
        } => (
            *channel_id,
            *amount_satoshis,
            SweepStatus::Confirmed(*confirmation_height, *latest_spending_txid),
        ),
    }
}

/// Returns the base URL for transaction links on a block explorer matching the
/// chain the node runs on. Esplora-style explorers (mempool.space,
/// mutinynet.com, blockstream.info) serve their API under an "/api" path of
/// their web UI, so an esplora server URL of that shape also tells us where the
/// web UI is; this notably makes links point to the right place on custom
/// signets. Other esplora servers (like the default one) fall back to
/// mempool.space for well-known networks.
pub fn explorer_tx_base(network: Network, esplora_server: &str) -> Option<String> {
    if let Some(base) = esplora_server.trim_end_matches('/').strip_suffix("/api") {
        return Some(format!("{}/tx/", base));
    }

    match network {
        Network::Bitcoin => Some("https://mempool.space/tx/".to_string()),
        Network::Testnet => Some("https://mempool.space/testnet/tx/".to_string()),
        Network::Signet => Some("https://mempool.space/signet/tx/".to_string()),
        _ => None,
    }
}

/// Returns a block explorer link for a transaction, so that users can check
/// its status on-chain (e.g. whether a channel has actually been force-closed,
/// or whether a sweep transaction has confirmed).
fn tx_link(explorer_tx_base: Option<&str>, tx_id: &str) -> String {
    if tx_id.is_empty() || tx_id == UNKNOWN {
        return tx_id.to_string();
    }

    match explorer_tx_base {
        Some(base) => format!("{}{}", base, tx_id),
        None => tx_id.to_string(),
    }
}

/// Prints the current balances and returns whether the recovery is complete,
/// i.e. all channels have been closed on-chain and all funds have been swept.
pub fn check_and_print_balances(
    node: &Node,
    explorer_tx_base: Option<&str>,
    scb_channels: &[ChannelBackup],
) -> bool {
    let channels = node.list_channels();
    let balances = node.list_balances();
    let current_height = node.status().current_best_block.height;

    let backup_by_channel: HashMap<_, _> = scb_channels
        .iter()
        .map(|c| (c.channel_id.to_string(), c.clone()))
        .collect();

    let channel_ids = channels
        .iter()
        .map(|c| c.channel_id)
        .collect::<HashSet<_>>();

    let claimable_by_channel: Vec<_> = balances
        .lightning_balances
        .iter()
        .map(get_ln_balance_channel_amount)
        .collect();

    let claimable = claimable_by_channel
        .iter()
        .filter_map(|(channel_id, amount, _)| {
            channel_ids.contains(channel_id).not().then(|| *amount)
        })
        .reduce(|total, amount| total + amount)
        .unwrap_or(0);

    // Any remaining lightning balance entry — even a 0-amount one — means the
    // channel has not fully resolved on-chain yet: the close transaction may
    // not have confirmed (a stale restored monitor reports 0 sats there), or
    // LDK is still waiting out the anti-reorg delay on a resolved output. The
    // amounts alone must not be used to conclude that the recovery is
    // complete.
    let unresolved_channels = claimable_by_channel
        .iter()
        .any(|(channel_id, _, _)| !channel_ids.contains(channel_id));

    // A sweep that confirmed ANTI_REORG_DELAY blocks ago is fully recovered:
    // the funds are already part of the on-chain balance. LDK keeps the sweeper
    // entry around for ~4 weeks after that (until the channel monitor is
    // archived), so it must not count towards the pending balance.
    let is_recovered = |confirmation_height: u32| {
        current_height.saturating_sub(confirmation_height) >= ANTI_REORG_DELAY
    };

    let pending_by_channel: Vec<_> = balances
        .pending_balances_from_channel_closures
        .iter()
        .map(get_pending_sweep_balance)
        .filter(|(_, _, status)| match status {
            SweepStatus::Confirmed(height, _) => !is_recovered(*height),
            _ => true,
        })
        .collect();

    let pending_sweep = pending_by_channel
        .iter()
        .map(|(_, amount, _)| *amount)
        .reduce(|total, amount| total + amount)
        .unwrap_or(0);

    let recovery_complete = !unresolved_channels && pending_by_channel.is_empty();

    info!(
        "balances: spendable: {}, reserved: {}, claimable: {}, pending sweep: {}",
        balances.spendable_onchain_balance_sats,
        balances.total_anchor_channels_reserve_sats,
        claimable,
        pending_sweep
    );

    println!("Balances (sats):");
    println!(
        "  Spendable: {}; total: {}; reserved: {}",
        balances.spendable_onchain_balance_sats,
        balances.total_onchain_balance_sats - balances.total_anchor_channels_reserve_sats,
        balances.total_anchor_channels_reserve_sats
    );
    println!(
        "  Pending from channel closures: {}",
        claimable + pending_sweep
    );

    if claimable + pending_sweep > 0 {
        println!("    (these sats may not appear in your on-chain balance yet; keep this tool running until this reaches 0)");
    }

    if !claimable_by_channel.is_empty() {
        println!("  Claimable:");
        for (channel_id, amount, status) in claimable_by_channel {
            // A restored channel monitor does not know the current channel
            // balance, so a reported amount of 0 means the amount is not (yet)
            // known rather than that there is nothing to recover.
            let (amount_str, status_note) = match status {
                ClaimableStatus::AwaitingChannelClose => (
                    match amount {
                        0 => "unknown amount".to_string(),
                        _ => format!("{} sats", amount),
                    },
                    " (waiting for the channel to be closed on-chain)".to_string(),
                ),
                ClaimableStatus::AwaitingMaturity(height) => {
                    let blocks = height.saturating_sub(current_height);
                    match amount {
                        // A closed channel with a 0-amount balance: the stale
                        // restored monitor does not know the real amount here.
                        // Any funds either already went directly to the
                        // on-chain wallet or are handed to the sweeper once
                        // the anti-reorg delay elapses.
                        0 => (
                            "channel closed".to_string(),
                            format!(
                                " (finalizing for {} more blocks; any recovered funds will then appear under \"Pending sweep\" or in the balances above)",
                                blocks
                            ),
                        ),
                        _ => (
                            format!("{} sats", amount),
                            match blocks {
                                0 => " (sweepable now)".to_string(),
                                _ => format!(" (sweepable in {} blocks)", blocks),
                            },
                        ),
                    }
                }
                ClaimableStatus::Other => (format!("{} sats", amount), String::new()),
            };

            let (peer_id, funding_tx) = backup_by_channel
                .get(&hex::encode(&channel_id.0))
                .map(|backup| (backup.peer_id.to_string(), backup.funding_tx_id.to_string()))
                .unwrap_or_else(|| (UNKNOWN.to_string(), UNKNOWN.to_string()));
            println!(
                "    {} from node {}, funding tx {}{}",
                amount_str,
                peer_id,
                tx_link(explorer_tx_base, &funding_tx),
                status_note
            );
        }
    }

    if !pending_by_channel.is_empty() {
        println!("  Pending sweep:");
        for (channel_id, amount, status) in pending_by_channel {
            let sweep_status = match status {
                SweepStatus::AwaitingBroadcast => " (preparing sweep transaction)".to_string(),
                SweepStatus::AwaitingConfirmation(txid) => format!(
                    " (sweep tx {} waiting for its first confirmation)",
                    tx_link(explorer_tx_base, &txid.to_string())
                ),
                SweepStatus::Confirmed(height, txid) => format!(
                    " (sweep tx {} has {}/{} confirmations)",
                    tx_link(explorer_tx_base, &txid.to_string()),
                    current_height.saturating_sub(height),
                    ANTI_REORG_DELAY
                ),
            };

            if channel_id.is_none() {
                println!("    {} sats (channel unknown){}", amount, sweep_status);
                continue;
            }

            let channel_id = hex::encode(channel_id.unwrap().0);

            let (peer_id, funding_tx) = backup_by_channel
                .get(&channel_id)
                .map(|backup| (backup.peer_id.to_string(), backup.funding_tx_id.to_string()))
                .unwrap_or_else(|| (UNKNOWN.to_string(), UNKNOWN.to_string()));
            println!(
                "    {} sats from node {}, funding tx {}{}",
                amount,
                peer_id,
                tx_link(explorer_tx_base, &funding_tx),
                sweep_status
            );
        }
    }

    println!();

    recovery_complete
}
