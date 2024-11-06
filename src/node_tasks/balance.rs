use std::collections::HashSet;
use std::ops::Not;
use std::sync::Arc;
use std::time::Duration;

use ldk_node::lightning::ln::ChannelId;
use ldk_node::{LightningBalance, Node, PendingSweepBalance};
use log::info;

use crate::periodic_blocking_task::{PeriodicBlockingTask, StopHandle};

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

fn get_pending_sweep_balance_amount(amount: &PendingSweepBalance) -> u64 {
    match amount {
        PendingSweepBalance::PendingBroadcast {
            amount_satoshis, ..
        } => *amount_satoshis,
        PendingSweepBalance::BroadcastAwaitingConfirmation {
            amount_satoshis, ..
        } => *amount_satoshis,
        PendingSweepBalance::AwaitingThresholdConfirmations {
            amount_satoshis, ..
        } => *amount_satoshis,
    }
}

pub fn spawn_balance_task(node: Arc<Node>, stop: Arc<StopHandle>) -> PeriodicBlockingTask {
    let stop_clone = stop.clone();

    PeriodicBlockingTask::spawn(Duration::from_secs(3), stop, move || {
        let channels = node.list_channels();
        let balances = node.list_balances();

        let channel_ids = channels
            .iter()
            .map(|c| c.channel_id)
            .collect::<HashSet<_>>();

        let claimable = balances
            .lightning_balances
            .iter()
            .filter_map(|b| {
                let (channel_id, amount) = get_ln_balance_channel_amount(b);
                channel_ids.contains(&channel_id).not().then(|| amount)
            })
            .reduce(|total, amount| total + amount)
            .unwrap_or(0);

        let pending_sweep = balances
            .pending_balances_from_channel_closures
            .iter()
            .map(get_pending_sweep_balance_amount)
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
        println!();

        if claimable + pending_sweep == 0 {
            info!("no more pending funds, stopping the node");
            println!("Recovery completed successfully");
            stop_clone.stop();
        }

        Ok(())
    })
}
