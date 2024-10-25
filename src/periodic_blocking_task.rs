use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use log::error;
use tokio::sync::Notify;
use tokio::task::{self, JoinHandle};

pub struct StopHandle {
    notify: Notify,
    stopped: AtomicBool,
}

impl StopHandle {
    pub fn new() -> Self {
        Self {
            notify: Notify::new(),
            stopped: AtomicBool::new(false),
        }
    }

    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Relaxed);
        self.notify.notify_waiters();
    }

    pub fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::Relaxed)
    }

    pub async fn wait(&self) {
        self.notify.notified().await
    }
}

pub struct PeriodicBlockingTask {
    task: JoinHandle<()>,
}

impl PeriodicBlockingTask {
    pub fn spawn<F>(period: Duration, stop: Arc<StopHandle>, f: F) -> Self
    where
        F: Fn() -> Result<()> + Send + 'static,
    {
        let task = task::spawn_blocking(move || loop {
            if let Err(e) = f() {
                error!("task failed: {:?}", e);
            }

            thread::sleep(period);

            if stop.is_stopped() {
                break;
            }
        });

        Self { task }
    }

    pub async fn wait(self) -> Result<()> {
        self.task.await.context("task failed")
    }
}
