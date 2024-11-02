use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use ldk_node;
use ldk_node::bip39::Mnemonic;
use ldk_node::bitcoin::secp256k1::PublicKey;
use ldk_node::bitcoin::Network;
use ldk_node::lightning::ln::msgs::SocketAddress;
use ldk_node::LogLevel;
use log::{error, info, LevelFilter};
use log4rs::append::file::FileAppender;
use log4rs::config::{Appender, Config, Root};
use log4rs::encode::pattern::PatternEncoder;
use tokio::signal;
use tokio::task;
use url::Url;

mod node_tasks;
mod periodic_blocking_task;
mod scb;
mod state;

use periodic_blocking_task::StopHandle;
use scb::EncodedChannelMonitorBackup;
use state::State;

const LDK_DIR: &str = "./ldk_data";
const LOG_FILE: &str = "hub-recovery.log";
const STATE_FILE: &str = "hub-recovery.state";
const DEFAULT_SCB_FILE: &str = "channel-backup.json";

#[derive(Parser, Debug)]
struct Args {
    /// Seed phrase. If you do not provide a value, you will be prompted to enter it.
    #[arg(short = 's', long)]
    seed: Option<Mnemonic>,

    /// Path to the Alby Hub static channel backup file.
    #[arg(short = 'b', long, default_value = DEFAULT_SCB_FILE)]
    backup_file: String,

    /// LDK network.
    #[arg(short = 'n', long, default_value = "bitcoin")]
    ldk_network: Network,

    /// Esplora server URL.
    #[arg(long, default_value = "https://electrs.getalbypro.com")]
    esplora_server: Url,

    /// Reset local recovery state.
    ///
    /// WARNING: the recovery process will start from scratch. All the existing
    /// recovery state will be lost.
    #[arg(long)]
    reset_recovery: bool,

    /// Enable verbose output. Specify once for debug level, twice for trace level.
    #[arg(short = 'v', action = clap::ArgAction::Count)]
    verbosity: u8,
}

fn setup_logging(verbosity: u8) -> Result<()> {
    let level = match verbosity {
        0 => LevelFilter::Info,
        1 => LevelFilter::Debug,
        _ => LevelFilter::Trace,
    };

    let logfile = FileAppender::builder()
        .encoder(Box::new(PatternEncoder::new(
            "[{d(%Y-%m-%dT%H:%M:%S%.3f%z)} {l} {t}] {m}{n}",
        )))
        .build(LOG_FILE)
        .context("failed to create log file appender")?;

    let config = Config::builder()
        .appender(Appender::builder().build("logfile", Box::new(logfile)))
        .build(Root::builder().appender("logfile").build(level))
        .context("failed to create log configuration")?;

    log4rs::init_config(config).context("failed to initialize log4rs")?;

    Ok(())
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

async fn run<P: AsRef<Path>>(args: &Args, dir: P) -> Result<()> {
    let dir = dir.as_ref();
    let mut state = State::try_load(dir.join(STATE_FILE))
        .context("failed to load recovery state")?
        .unwrap_or_default();

    let mnemonic = args
        .seed
        .clone()
        .unwrap_or_else(|| prompt_parse("Enter seed phrase:"));

    let scb = scb::load_scb_guess_type(dir.join(&args.backup_file), &mnemonic)
        .context("failed to load static channel backup file")?;

    // Compare the list of channels from SCB with the list of channels from
    // the state file. If the channels don't match, it is likely that
    // the recovery process has been restarted with a different static channel
    // backup file. We do not allow that.
    if !state.get_force_closed_channels().is_empty()
        && state.get_force_closed_channels() != &scb.channel_ids()
    {
        error!("static channel backup file has changed; cannot proceed with the recovery");
        println!("The recovery process has already been initiated with a different static channel backup file.");
        println!("Please specify the same backup file to resume recovery.");
        println!("To recover channels from a different backup file, restart the app with the --reset-recovery flag.");
        println!("WARNING: this will reset the recovery state and start the recovery process from scratch.");
        return Err(anyhow!(
            "static channel backup file does not match the stored state"
        ));
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
        .set_entropy_bip39_mnemonic(mnemonic, None)
        .set_network(args.ldk_network)
        .set_storage_dir_path(
            dir.join(LDK_DIR)
                .to_str()
                .ok_or(anyhow!("invalid LDK path"))?
                .to_string(),
        )
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

    let node = Arc::new(builder.build().context("failed to instantiate LDK node")?);

    node.start().context("failed to start LDK node")?;

    println!("Synchronizing wallets...");
    node.sync_wallets()
        .context("failed to perform initial wallet synchronization")?;

    let mut channels = HashSet::new();

    println!("Connecting to peers...");
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

        channels.insert(ch.channel_id);
    }

    if state.get_force_closed_channels().is_empty() {
        println!("Forcing close all channels...");
        node.force_close_all_channels_without_broadcasting_txn();

        state.set_force_closed_channels(channels);
        state
            .save(dir.join(STATE_FILE))
            .context("failed to save recovery state")?;
    } else {
        println!("Resuming recovery");
    }

    let stop = Arc::new(StopHandle::new());

    let node_task = node_tasks::spawn_node_event_loop_task(node.clone(), stop.clone());
    let balance_task = node_tasks::spawn_balance_task(node.clone(), stop.clone());
    let sync_task = node_tasks::spawn_wallet_sync_task(node.clone(), stop.clone());

    println!("Waiting for channel recovery to complete. This may take a while...");
    println!("It is safe to interrupt this program by pressing Ctrl-C. You can resume it later to check recovery status.");

    tokio::select! {
        _ = signal::ctrl_c() => stop.stop(),
        _ = stop.wait() => {}
    }

    println!("Stopping...");

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

fn ignore_not_found(e: io::Error) -> io::Result<()> {
    match e.kind() {
        io::ErrorKind::NotFound => Ok(()),
        _ => Err(e),
    }
}

fn reset_recovery<P: AsRef<Path>>(dir: P) -> Result<()> {
    let dir = dir.as_ref();
    std::fs::remove_file(dir.join(STATE_FILE))
        .or_else(ignore_not_found)
        .context("failed to delete recovery state file")?;
    std::fs::remove_dir_all(dir.join(LDK_DIR))
        .or_else(ignore_not_found)
        .context("failed to delete LDK data directory")?;
    Ok(())
}

fn get_own_dir() -> Result<PathBuf> {
    Ok(std::env::current_exe()
        .context("failed to get own executable path")?
        .parent()
        .context("failed to get own executable directory")?
        .to_path_buf())
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    setup_logging(args.verbosity).unwrap();

    let own_dir = match get_own_dir() {
        Ok(d) => d,
        Err(e) => {
            error!("failed to get own directory: {:?}", e);
            return;
        }
    };

    if args.reset_recovery {
        if let Err(e) = reset_recovery(&own_dir) {
            error!("failed to reset recovery state: {:?}", e);
            eprintln!("Failed to reset recovery state: {:#}", e);
            eprintln!("To reset the recovery state manually, delete the following:");
            eprintln!("  {}", STATE_FILE);
            eprintln!("  {}", LDK_DIR);
            return;
        }
    }

    if let Err(e) = run(&args, &own_dir).await {
        error!("recovery failed: {:?}", e);

        eprintln!(
            "Recovery failed; error: {:#} (see the {} file for details)",
            e, LOG_FILE
        );
    }
}
