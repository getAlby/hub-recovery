use std::sync::Arc;
use std::time::Duration;

use ldk_node::Node;
use log::info;

use crate::periodic_blocking_task::{PeriodicBlockingTask, StopHandle};

pub fn spawn_wallet_sync_task(node: Arc<Node>, stop: Arc<StopHandle>) -> PeriodicBlockingTask {
    PeriodicBlockingTask::spawn(Duration::from_secs(4), stop, move || {
        info!("syncing wallets");
        node.sync_wallets()?;
        info!("wallets synced");

        Ok(())
    })
}
