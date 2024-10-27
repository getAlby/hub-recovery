use std::collections::HashSet;
use std::fs::File;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct State {
    /// Identifiers of channels for which the closure request has been sent.
    force_closed: HashSet<String>,
}

impl State {
    pub fn new() -> Self {
        Self {
            force_closed: HashSet::new(),
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

    pub fn get_force_closed_channels(&self) -> &HashSet<String> {
        &self.force_closed
    }

    pub fn set_force_closed_channels(&mut self, force_closed: HashSet<String>) {
        self.force_closed = force_closed;
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}
