use std::io;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use env_logger;
use ldk_node;
use ldk_node::bip39::Mnemonic;
use ldk_node::bitcoin::secp256k1::PublicKey;
use ldk_node::bitcoin::Network;
use ldk_node::lightning::ln::msgs::SocketAddress;
use ldk_node::LogLevel;
use log::{error, info};
use tokio::signal;
use tokio::task;
use url::Url;

mod node_tasks;
mod periodic_blocking_task;
mod scb;

use periodic_blocking_task::StopHandle;
use scb::EncodedChannelMonitorBackup;

#[derive(Parser, Debug)]
struct Args {
    /// Seed phrase. If you do not provide a value, you will be prompted to enter it.
    #[arg(short = 's', long)]
    seed: Option<Mnemonic>,

    /// Passphrase for the seed phrase. If you do not provide a value, you will be prompted to enter it.
    /// Pass an empty string for no passphrase.
    #[arg(short = 'p', long)]
    passphrase: Option<String>,

    /// Path to the Alby Hub static channel backup file.
    #[arg(short = 'b', long)]
    backup_file: String,

    /// LDK network.
    #[arg(short = 'n', long, default_value = "bitcoin")]
    ldk_network: Network,

    /// Esplora server URL.
    #[arg(long, default_value = "https://electrs.getalbypro.com")]
    esplora_server: Url,

    /// Gossip source URL.
    #[arg(long)]
    gossip_source: Option<Url>,

    /// Enable verbose output. Specify once for debug level, twice for trace level.
    #[arg(short = 'v', action = clap::ArgAction::Count)]
    verbosity: u8,
}

fn setup_logging(verbosity: u8) {
    let level = match verbosity {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(level)).init();
}

fn prompt(p: &str) -> String {
    println!("{}", p);
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().to_string()
}

fn prompt_parse<T>(p: &str) -> T
where
    T: FromStr,
{
    loop {
        match prompt(p).parse::<T>() {
            Ok(v) => break v,
            Err(_) => error!("failed to parse input, try again"),
        }
    }
}

async fn run(args: &Args) -> Result<()> {
    let scb =
        scb::load_scb(&args.backup_file).context("failed to load static channel backup file")?;

    let mnemonic = args
        .seed
        .clone()
        .unwrap_or_else(|| prompt_parse("Enter seed phrase:"));
    let passphrase = args
        .passphrase
        .clone()
        .or_else(|| Some(prompt("Enter passphrase (empty for none):")))
        .and_then(|p| if p.is_empty() { None } else { Some(p) });
    if passphrase.is_none() {
        info!("no passphrase provided");
    }

    let ldk_log_level = match args.verbosity {
        0 => LogLevel::Info,
        1 => LogLevel::Debug,
        _ => LogLevel::Trace,
    };

    let config = ldk_node::Config {
        log_level: ldk_log_level,
        ..Default::default()
    };

    let mut builder = ldk_node::Builder::from_config(config);
    builder
        .set_entropy_bip39_mnemonic(mnemonic, passphrase)
        .set_network(args.ldk_network)
        .set_storage_dir_path("./ldk_data".to_string())
        .set_esplora_server(args.esplora_server.to_string())
        .set_liquidity_source_lsps2(
            SocketAddress::from_str("52.88.33.119:9735").unwrap(),
            PublicKey::from_str(
                "031b301307574bbe9b9ac7b79cbe1700e31e544513eae0b5d7497483083f99e581", // Olympus LSP
            )
            .unwrap(),
            None,
        )
        .restore_encoded_channel_monitors(
            scb.monitors
                .into_iter()
                .map(EncodedChannelMonitorBackup::into)
                .collect(),
        );

    if let Some(gossip_source) = args.gossip_source.as_ref() {
        builder.set_gossip_source_rgs(gossip_source.to_string());
    }

    let node = Arc::new(builder.build().context("failed to instantiate LDK node")?);

    node.start().context("failed to start LDK node")?;

    node.sync_wallets()
        .context("failed to perform initial wallet synchronization")?;

    for ch in scb.channels {
        let pkey = PublicKey::from_str(&ch.peer_id).context(format!(
            "bad static channel backup: invalid peer ID: {}",
            ch.peer_id
        ))?;
        let peer_addr = SocketAddress::from_str(&ch.peer_socket_address).map_err(|e| {
            anyhow!(
                "bad static channel backup: invalid peer address {}: {:?}",
                ch.peer_socket_address,
                e
            )
        })?;
        if let Err(e) = node.connect(pkey, peer_addr, true) {
            error!("failed to connect to peer {}: {}", ch.peer_id, e);
        } else {
            info!(
                "connected to peer {} {}",
                ch.peer_socket_address, ch.peer_id
            );
        }
    }

    node.force_close_all_channels_without_broadcasting_txn();

    let stop = Arc::new(StopHandle::new());

    let node_task = node_tasks::spawn_node_event_loop_task(node.clone(), stop.clone());
    let balance_task = node_tasks::spawn_balance_task(node.clone(), stop.clone());
    let sync_task = node_tasks::spawn_wallet_sync_task(node.clone(), stop.clone());

    info!("press Ctrl-C to stop the node");

    tokio::select! {
        _ = signal::ctrl_c() => stop.stop(),
        _ = stop.wait() => {}
    }

    info!("stopping node");

    info!("waiting for node task to finish");
    node_task.wait().await.context("node task failed")?;

    info!("waiting for balance task to finish");
    balance_task.wait().await.context("balance task failed")?;

    info!("waiting for sync task to finish");
    sync_task.wait().await.context("sync task failed")?;

    info!("stopping node");
    task::spawn_blocking(move || node.stop().context("failed to stop LDK node"))
        .await
        .context("node stop task failed")??;

    info!("done");

    Ok(())
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    setup_logging(args.verbosity);

    if let Err(e) = run(&args).await {
        if args.verbosity == 0 {
            error!("{:#}", e);
            error!("run with -v for detailed error information");
        } else {
            error!("{:?}", e);
        }
    }
}
