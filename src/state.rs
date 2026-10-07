use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Copy, Clone, PartialEq)]
pub enum ChannelState {
    Pending,
    ForceCloseInitiated,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct State {
    /// Map of channel states by peer ID.
    by_peer: HashMap<String, HashMap<String, ChannelState>>,
}

impl State {
    pub fn new() -> Self {
        Self {
            by_peer: HashMap::new(),
        }
    }

    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let f = File::create(path).context("failed to create state file")?;
        serde_json::to_writer(f, self)?;
        Ok(())
    }

    pub fn has_pending_channels(&self) -> bool {
        self.by_peer
            .values()
            .any(|v| v.values().any(|&s| s == ChannelState::Pending))
    }

    pub fn get_channel_state(&self, peer: &str, channel_id: &str) -> Option<ChannelState> {
        self.by_peer
            .get(peer)
            .and_then(|v| v.get(channel_id))
            .cloned()
    }

    pub fn set_channel_state(&mut self, peer: &str, channel_id: &str, state: ChannelState) {
        self.by_peer
            .entry(peer.to_string())
            .or_insert_with(HashMap::new)
            .insert(channel_id.to_string(), state);
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}
