use std::sync::Arc;
use std::time::Duration;

use ldk_node::Node;
use log::info;

use crate::periodic_blocking_task::{PeriodicBlockingTask, StopHandle};

pub fn spawn_node_event_loop_task(node: Arc<Node>, stop: Arc<StopHandle>) -> PeriodicBlockingTask {
    PeriodicBlockingTask::spawn(Duration::from_millis(100), stop, move || {
        if let Some(event) = node.next_event() {
            info!("event: {:?}", event);
            node.event_handled();
        }

        Ok(())
    })
}
