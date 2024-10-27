use std::collections::HashSet;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use anyhow::Context;
use ldk_node::KeyValue;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct StaticChannelBackup {
    pub channels: Vec<ChannelBackup>,
    pub monitors: Vec<EncodedChannelMonitorBackup>,
}

impl StaticChannelBackup {
    pub fn channel_ids(&self) -> HashSet<String> {
        self.channels.iter().map(|c| c.channel_id.clone()).collect()
    }
}

#[derive(Deserialize, Debug)]
pub struct EncodedChannelMonitorBackup {
    pub key: String,

    #[serde(with = "hex")]
    pub value: Vec<u8>,
}

impl From<EncodedChannelMonitorBackup> for KeyValue {
    fn from(backup: EncodedChannelMonitorBackup) -> Self {
        KeyValue {
            key: backup.key,
            value: backup.value,
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct ChannelBackup {
    pub channel_id: String,
    pub peer_id: String,
    pub peer_socket_address: String,
}

pub fn load_scb<P>(path: P) -> anyhow::Result<StaticChannelBackup>
where
    P: AsRef<Path>,
{
    serde_json::from_reader(BufReader::new(
        File::open(path).context("failed to open SCB file")?,
    ))
    .context("failed to parse SCB file")
}
