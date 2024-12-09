use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

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
use url::Url;

mod balance;
mod scb;
mod state;

use scb::EncodedChannelMonitorBackup;
use state::{ChannelState, State};

const LDK_DIR: &str = "./ldk_data";
const LOG_FILE: &str = "hub-recovery.log";
const STATE_FILE: &str = "hub-recovery.state";
const DEFAULT_SCB_FILE: &str = "channel-backup.json";
const DEFAULT_SCB_ENCRYPTED_FILE: &str = "channel-backup.enc";

#[derive(Parser, Debug)]
struct Args {
    /// Seed phrase. If you do not provide a value, you will be prompted to enter it.
    #[arg(short = 's', long)]
    seed: Option<Mnemonic>,

    /// Path to the Alby Hub static channel backup file.
    #[arg(short = 'b', long)]
    backup_file: Option<String>,

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

    /// Use the current working directory for local data instead of the
    /// directory where the executable is located.
    #[arg(long)]
    use_workdir: bool,

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
            Err(_) => println!("Incorrect input, try again"),
        }
    }
}

fn parse_peer_address(s: &str) -> Result<SocketAddress> {
    SocketAddress::from_str(s).map_err(|e| {
        anyhow!(
            "bad static channel backup: invalid peer address {}: {:?}",
            s,
            e
        )
    })
}

fn get_scb_path<P: AsRef<Path>>(dir: P, arg: Option<&str>) -> PathBuf {
    let dir = dir.as_ref();

    if let Some(p) = arg {
        return dir.join(p);
    }

    let detected_default = if let Ok(true) = dir.join(DEFAULT_SCB_FILE).try_exists() {
        Some(DEFAULT_SCB_FILE)
    } else if let Ok(true) = dir.join(DEFAULT_SCB_ENCRYPTED_FILE).try_exists() {
        Some(DEFAULT_SCB_ENCRYPTED_FILE)
    } else {
        None
    };

    let prompt_str = match detected_default {
        Some(p) => format!(
            "Enter static channel backup filename (press enter to use default filename: \"{}\"):",
            p
        ),
        None => "Enter static channel backup file name:".to_string(),
    };

    loop {
        let p = prompt(&prompt_str);
        if p.trim().is_empty() {
            if let Some(d) = detected_default {
                break dir.join(d);
            } else {
                println!("No filename provided, please try again");
                continue;
            }
        }

        let path = PathBuf::from(&p);
        if path.try_exists().unwrap_or(false) {
            break path;
        } else if dir.join(&p).try_exists().unwrap_or(false) {
            break dir.join(&p);
        } else {
            println!("File {} not found, please try again", p);
        }
    }
}

fn run<P: AsRef<Path>>(args: &Args, dir: P) -> Result<()> {
    let dir = dir.as_ref();
    let mut state = State::try_load(dir.join(STATE_FILE))
        .context("failed to load recovery state")?
        .unwrap_or_default();

    if !state.is_empty() {
        println!("Recovery process is in progress.");
        loop {
            let s =
                prompt("Hit Enter to resume recovery. Type NEW to start the process from scratch.");
            if s.trim().is_empty() {
                break;
            } else if s.trim().to_lowercase() == "new" {
                reset_recovery(dir)?;
                state = State::default();
                break;
            } else {
                println!("Invalid input, try again");
            }
        }
    }

    let first_run = state.is_empty();

    let mnemonic = args.seed.clone().unwrap_or_else(|| {
        const SAMPLE: &str =
            "hotel obvious agent lecture gadget evil jealous keen fragile before damp clarify";
        let prompt = format!("Enter recovery phrase (12 words, e.g.: {}):", SAMPLE);
        prompt_parse(&prompt)
    });

    let scb_path = get_scb_path(dir, args.backup_file.as_deref());

    let scb = scb::load_scb_guess_type(scb_path, &mnemonic)
        .context("failed to load static channel backup file")?;

    if state.is_empty() {
        info!("initializing recovery state");
        scb.channels.iter().for_each(|ch| {
            state.set_channel_state(&ch.peer_id, &ch.channel_id, ChannelState::Pending);
        });
        state
            .save(dir.join(STATE_FILE))
            .context("failed to save recovery state")?;
    } else if state.get_all_channel_ids() != scb.channel_ids() {
        // If the channels in SCB don't match channels in the recovery state
        // file, it is likely that the recovery process has been restarted
        // with a different static channel backup file. We do not allow that.
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
        .set_esplora_server(
            args.esplora_server
                .to_string()
                .trim_end_matches('/')
                .to_string(),
        )
        .set_liquidity_source_lsps2(
            SocketAddress::from_str("52.88.33.119:9735").unwrap(),
            PublicKey::from_str(
                "031b301307574bbe9b9ac7b79cbe1700e31e544513eae0b5d7497483083f99e581", // Olympus LSP
            )
            .unwrap(),
            None,
        );

    if first_run {
        builder.restore_encoded_channel_monitors(
            scb.monitors
                .into_iter()
                .map(EncodedChannelMonitorBackup::into)
                .collect(),
        );
    }

    let node = Arc::new(builder.build().context("failed to instantiate LDK node")?);

    node.start().context("failed to start LDK node")?;

    println!("Synchronizing wallets...");
    if let Err(e) = node.sync_wallets() {
        error!("failed to perform initial wallet synchronization: {:?}", e);
        eprintln!("Failed to synchronize wallets: {:#}", e);
    }

    let mut connected_peers = HashSet::new();
    let mut failed_peers = HashSet::new();

    println!("Connecting to peers...");
    for ch in &scb.channels {
        if connected_peers.contains(&ch.peer_id) {
            continue;
        }

        let pkey = PublicKey::from_str(&ch.peer_id).context(format!(
            "bad static channel backup: invalid peer ID: {}",
            ch.peer_id
        ))?;
        let peer_addr = parse_peer_address(&ch.peer_socket_address)?;
        if let Err(e) = node.connect(pkey, peer_addr, true) {
            error!("failed to connect to peer {}: {}", ch.peer_id, e);
            failed_peers.insert(ch.peer_id.clone());
        } else {
            info!(
                "connected to peer {} {}",
                ch.peer_socket_address, ch.peer_id
            );
            connected_peers.insert(ch.peer_id.clone());
        }
    }

    if !failed_peers.is_empty() {
        println!("Failed to connect to the following peers:");
        for peer in failed_peers {
            println!("  {}", peer);
        }
        println!("Please check the logs for details.");
    }

    if state.has_pending_channels() {
        println!("Force-closing channels...");
        node.force_close_all_channels_without_broadcasting_txn();
    } else {
        println!("Resuming recovery");
    }

    // For all newly connected peers, update their channels' state.
    for ch in scb.channels.iter() {
        if connected_peers.contains(&ch.peer_id)
            && state
                .get_channel_state(&ch.peer_id, &ch.channel_id)
                .unwrap_or(ChannelState::Pending)
                == ChannelState::Pending
        {
            state.set_channel_state(
                &ch.peer_id,
                &ch.channel_id,
                ChannelState::ForceCloseInitiated,
            );
        }
    }

    state
        .save(dir.join(STATE_FILE))
        .context("failed to save recovery state")?;

    println!("Waiting for channel recovery to complete. This may take a while...");
    println!("It is safe to interrupt this program by pressing Ctrl-C. You can resume it later to check recovery status.");
    let (tx, rx) = mpsc::channel();
    ctrlc::set_handler(move || tx.send(()).expect("Could not send signal on channel."))
        .expect("Error setting Ctrl-C handler");

    let mut last_balance = Instant::now();
    let mut last_sync = Instant::now();
    loop {
        if rx.try_recv().is_ok() {
            println!("Stopping...");
            break;
        }

        let now = Instant::now();

        if now.duration_since(last_balance).as_secs() >= 3 {
            if balance::check_and_print_balances(&node, &scb.channels) == 0 {
                info!("no more pending funds, stopping the node");
                println!("Recovery completed successfully");
                break;
            }
            last_balance = now;
        }

        if now.duration_since(last_sync).as_secs() >= 4 {
            info!("syncing wallets");
            if let Err(e) = node.sync_wallets() {
                error!("failed to sync wallets: {:?}", e);
            } else {
                info!("wallets synced");
            }
            last_sync = now;
        }

        loop {
            match node.next_event() {
                Some(event) => {
                    info!("event: {:?}", event);
                    node.event_handled();
                }
                None => break,
            }
        }

        thread::sleep(Duration::from_millis(100));
    }

    info!("stopping node");
    node.stop().context("failed to stop LDK node")?;
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

fn get_local_dir(use_cwd: bool) -> Result<PathBuf> {
    match use_cwd {
        true => Ok(std::env::current_dir().context("failed to get current working directory")?),
        false => get_own_dir(),
    }
}

fn main() {
    let args = Args::parse();

    setup_logging(args.verbosity).unwrap();

    let local_dir = match get_local_dir(args.use_workdir) {
        Ok(d) => d,
        Err(e) => {
            error!("failed to get own directory: {:?}", e);
            return;
        }
    };

    if args.reset_recovery {
        if let Err(e) = reset_recovery(&local_dir) {
            error!("failed to reset recovery state: {:?}", e);
            eprintln!("Failed to reset recovery state: {:#}", e);
            eprintln!("To reset the recovery state manually, delete the following:");
            eprintln!("  {}", STATE_FILE);
            eprintln!("  {}", LDK_DIR);
            return;
        }
    }

    if let Err(e) = run(&args, &local_dir) {
        error!("recovery failed: {:?}", e);

        eprintln!(
            "Recovery failed; error: {:#} (see the {} file for details)",
            e, LOG_FILE
        );
    }
}
