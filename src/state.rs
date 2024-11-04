use std::collections::{HashMap, HashSet};
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

    pub fn try_load<P: AsRef<Path>>(path: P) -> Result<Option<Self>> {
        let path = path.as_ref();
        if !path.try_exists().context("cannot access state file")? {
            return Ok(None);
        }

        let f = File::open(path).context("failed to open state file")?;
        let state = serde_json::from_reader(f)?;
        Ok(Some(state))
    }

    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let f = File::create(path).context("failed to create state file")?;
        serde_json::to_writer(f, self)?;
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.by_peer.is_empty()
    }

    pub fn has_pending_channels(&self) -> bool {
        self.by_peer
            .values()
            .any(|v| v.values().any(|&s| s == ChannelState::Pending))
    }

    pub fn get_all_channel_ids(&self) -> HashSet<String> {
        self.by_peer
            .iter()
            .flat_map(|(_, v)| v.keys().cloned())
            .collect()
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
