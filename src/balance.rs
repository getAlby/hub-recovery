use std::collections::{BTreeMap, HashMap, HashSet};

use ldk_node::bitcoin::{Network, Txid};
use ldk_node::lightning::chain::channelmonitor::ANTI_REORG_DELAY;
use ldk_node::lightning::ln::types::ChannelId;
use ldk_node::{LightningBalance, Node, PendingSweepBalance};
use log::info;

use crate::scb::ChannelBackup;

/// Placeholder used when a channel from the node's balances has no matching
/// entry in the static channel backup file.
const UNKNOWN: &str = "<unknown>";

/// What a channel with an unresolved lightning balance is currently waiting
/// for.
///
/// The amounts LDK reports for these balances come from the restored channel
/// monitor, which only knows the channel state at the time the backup was made.
/// They are wrong whenever the channel was used after the backup (until the
/// output reaches the sweeper), so they are never shown.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ChannelStatus {
    /// The channel's close transaction has not appeared on-chain yet.
    AwaitingChannelClose,
    /// The channel was closed on-chain; our output becomes sweepable once the
    /// chain reaches this height.
    AwaitingMaturity(u32),
    /// Resolving through some other pending on-chain resolution.
    Other,
}

fn get_ln_balance_channel_status(balance: &LightningBalance) -> (ChannelId, ChannelStatus) {
    match balance {
        LightningBalance::ClaimableOnChannelClose { channel_id, .. } => {
            (*channel_id, ChannelStatus::AwaitingChannelClose)
        }
        LightningBalance::ClaimableAwaitingConfirmations {
            channel_id,
            confirmation_height,
            ..
        } => (*channel_id, ChannelStatus::AwaitingMaturity(*confirmation_height)),
        LightningBalance::ContentiousClaimable { channel_id, .. } => {
            (*channel_id, ChannelStatus::Other)
        }
        LightningBalance::MaybeTimeoutClaimableHTLC {
            channel_id,
            claimable_height,
            ..
        } => (*channel_id, ChannelStatus::AwaitingMaturity(*claimable_height)),
        LightningBalance::MaybePreimageClaimableHTLC { channel_id, .. } => {
            (*channel_id, ChannelStatus::Other)
        }
        LightningBalance::CounterpartyRevokedOutputClaimable { channel_id, .. } => {
            (*channel_id, ChannelStatus::Other)
        }
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

    // Channels whose balance has not been handed to the sweeper yet, with the
    // least advanced status of their balances. Any remaining lightning balance
    // entry means the channel has not fully resolved on-chain yet, so these
    // must not be used to conclude that the recovery is complete.
    let mut unresolved: BTreeMap<String, ChannelStatus> = BTreeMap::new();
    for (channel_id, status) in balances
        .lightning_balances
        .iter()
        .map(get_ln_balance_channel_status)
        .filter(|(channel_id, _)| !channel_ids.contains(channel_id))
    {
        let entry = unresolved
            .entry(hex::encode(channel_id.0))
            .or_insert(status);
        *entry = (*entry).min(status);
    }

    let not_closed: Vec<_> = unresolved
        .iter()
        .filter(|(_, status)| **status == ChannelStatus::AwaitingChannelClose)
        .map(|(channel_id, _)| channel_id)
        .collect();
    let closing = unresolved.len() - not_closed.len();

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

    let recovery_complete = unresolved.is_empty() && pending_by_channel.is_empty();

    let mut pending_str = pending_sweep.to_string();
    if !not_closed.is_empty() {
        pending_str += &format!(
            " + unknown amount from {} channel(s) not closed yet (total channel size: {})",
            not_closed.len(),
            total_channel_size(not_closed.iter().map(|id| backup_by_channel.get(*id)))
        );
    }
    if closing > 0 {
        pending_str += &format!(
            " + unknown amount from {} closed channel(s) still confirming",
            closing
        );
    }

    info!(
        "balances: spendable: {}, reserved: {}, pending from channel closures: {}",
        balances.spendable_onchain_balance_sats,
        balances.total_anchor_channels_reserve_sats,
        pending_str
    );

    println!("Balances (sats):");
    println!(
        "  Spendable: {}; total: {}; reserved: {}",
        balances.spendable_onchain_balance_sats,
        balances.total_onchain_balance_sats - balances.total_anchor_channels_reserve_sats,
        balances.total_anchor_channels_reserve_sats
    );
    println!("  Pending from channel closures: {}", pending_str);

    if pending_sweep > 0 || !unresolved.is_empty() {
        println!("    (these sats may not appear in your on-chain balance yet; keep this tool running until nothing is pending)");
    }

    if !unresolved.is_empty() {
        println!("  Channels being closed:");
        for (channel_id, status) in &unresolved {
            let status_note = match status {
                ChannelStatus::AwaitingChannelClose => {
                    " (waiting for the channel to be closed on-chain)".to_string()
                }
                ChannelStatus::AwaitingMaturity(height) => format!(
                    " (channel closed; your share will be shown in {} blocks)",
                    height.saturating_sub(current_height)
                ),
                ChannelStatus::Other => " (resolving on-chain)".to_string(),
            };

            let backup = backup_by_channel.get(channel_id);
            let size = match backup.and_then(|b| b.channel_size) {
                Some(size) => format!("{}-sat channel", size),
                None => "channel (size unknown)".to_string(),
            };
            let (peer_id, funding_tx) = backup
                .map(|backup| (backup.peer_id.to_string(), backup.funding_tx_id.to_string()))
                .unwrap_or_else(|| (UNKNOWN.to_string(), UNKNOWN.to_string()));
            println!(
                "    {} with node {}, funding tx {}{}",
                size,
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

/// Sum of the channel sizes from the backup, e.g. "200000 sats". If the size
/// of any channel is unknown (older backups), only the known part is shown.
fn total_channel_size<'a>(channels: impl Iterator<Item = Option<&'a ChannelBackup>>) -> String {
    let mut total = 0;
    let mut unknown = 0;
    for size in channels.map(|c| c.and_then(|c| c.channel_size)) {
        match size {
            Some(size) => total += size,
            None => unknown += 1,
        }
    }
    match (total, unknown) {
        (_, 0) => format!("{} sats", total),
        (0, _) => "unknown".to_string(),
        _ => format!("{} sats + {} channel(s) of unknown size", total, unknown),
    }
}
