use std::collections::{HashMap, HashSet};
use std::ops::Not;

use ldk_node::lightning::ln::types::ChannelId;
use ldk_node::{LightningBalance, Node, PendingSweepBalance};
use log::info;

use crate::scb::ChannelBackup;

fn get_ln_balance_channel_amount(balance: &LightningBalance) -> (ChannelId, u64) {
    match balance {
        LightningBalance::ClaimableOnChannelClose {
            channel_id,
            amount_satoshis,
            ..
        } => (*channel_id, *amount_satoshis),
        LightningBalance::ClaimableAwaitingConfirmations {
            channel_id,
            amount_satoshis,
            ..
        } => (*channel_id, *amount_satoshis),
        LightningBalance::ContentiousClaimable {
            channel_id,
            amount_satoshis,
            ..
        } => (*channel_id, *amount_satoshis),
        LightningBalance::MaybeTimeoutClaimableHTLC {
            channel_id,
            amount_satoshis,
            ..
        } => (*channel_id, *amount_satoshis),
        LightningBalance::MaybePreimageClaimableHTLC {
            channel_id,
            amount_satoshis,
            ..
        } => (*channel_id, *amount_satoshis),
        LightningBalance::CounterpartyRevokedOutputClaimable {
            channel_id,
            amount_satoshis,
            ..
        } => (*channel_id, *amount_satoshis),
    }
}

fn get_pending_sweep_balance_amount(amount: &PendingSweepBalance) -> (Option<ChannelId>, u64) {
    match amount {
        PendingSweepBalance::PendingBroadcast {
            channel_id,
            amount_satoshis,
            ..
        } => (*channel_id, *amount_satoshis),
        PendingSweepBalance::BroadcastAwaitingConfirmation {
            channel_id,
            amount_satoshis,
            ..
        } => (*channel_id, *amount_satoshis),
        PendingSweepBalance::AwaitingThresholdConfirmations {
            channel_id,
            amount_satoshis,
            ..
        } => (*channel_id, *amount_satoshis),
    }
}

pub fn check_and_print_balances(node: &Node, scb_channels: &[ChannelBackup]) -> u64 {
    let channels = node.list_channels();
    let balances = node.list_balances();

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
        .filter_map(|(channel_id, amount)| channel_ids.contains(channel_id).not().then(|| *amount))
        .reduce(|total, amount| total + amount)
        .unwrap_or(0);

    let pending_by_channel: Vec<_> = balances
        .pending_balances_from_channel_closures
        .iter()
        .map(get_pending_sweep_balance_amount)
        .collect();

    let pending_sweep = pending_by_channel
        .iter()
        .map(|(_, amount)| *amount)
        .reduce(|total, amount| total + amount)
        .unwrap_or(0);

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

    if !claimable_by_channel.is_empty() {
        println!("  Claimable:");
        for (channel_id, amount) in claimable_by_channel {
            let (peer_id, funding_tx) = backup_by_channel
                .get(&hex::encode(&channel_id.0))
                .map(|backup| (backup.peer_id.to_string(), backup.funding_tx_id.to_string()))
                .unwrap_or_else(|| ("<unknown>".to_string(), "<unknown>".to_string()));
            println!(
                "    {} sats from node {}, funding tx {}",
                amount, peer_id, funding_tx
            );
        }
    }

    if !pending_by_channel.is_empty() {
        println!("  Pending sweep:");
        for (channel_id, amount) in pending_by_channel {
            if channel_id.is_none() {
                println!("    {} sats (channel unknown)", amount);
                continue;
            }

            let channel_id = hex::encode(channel_id.unwrap().0);

            let (peer_id, funding_tx) = backup_by_channel
                .get(&channel_id)
                .map(|backup| (backup.peer_id.to_string(), backup.funding_tx_id.to_string()))
                .unwrap_or_else(|| ("<unknown>".to_string(), "<unknown>".to_string()));
            println!(
                "    {} sats from node {}, funding tx {}",
                amount, peer_id, funding_tx
            );
        }
    }

    println!();

    claimable + pending_sweep
}
