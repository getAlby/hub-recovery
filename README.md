# Alby Hub Recovery

This is a simple tool to recover funds from channels in a static channel backup file. It reconnects to your channel peers, asks them to force-close all channels, and sweeps your funds back to your on-chain wallet.

[Learn more about Alby Hub backups](https://guides.getalby.com/user-guide/alby-hub/backups-and-recover).

## Attention

> Before proceeding: If you are a subscriber with your Alby Hub on Alby Cloud, please contact Alby Support to check if VSS is enabled for your account. VSS (Virtual Static Storage) allows you to recover your channels along with the funds. This feature is enabled by default for all Cloud Hubs created after December 9, 2024 (Hub version `1.11.1`).  
> If your Hub was created after this date, simply start a new Hub, go to **Advanced > Import Recovery Phrase**, and recover your channels without force-closing them. You don’t need this guide or the recovery tool.  

If VSS is not enabled or you’re using a self-hosted/free Hub, follow these steps:

> **Important:** Always run the recovery tool in a **new, empty folder** that contains only the tool and your channel backup file. See [Running the recovery again](#running-the-recovery-again).

## Quick Start

1. Download the latest release.

2. Create a new, empty folder (for example `hub-recovery-1`) and move the tool into it.

3. Choose one of these options:  

   **A) If you have an Alby Account:**  
   Download the channel backup file from <https://getalby.com/backups/> and place it in the new folder, next to the tool.

   **B) If you do not have an Alby Account:**  
   Copy the channel backup file into the new folder, next to the tool, and rename it to `channel-backup.json`.  
   You can find this file in your Alby Hub directory at `WORK_DIR/ldk/static_channel_backups`.  
   Refer to the `WORK_DIR` for your operating system here:  
   <https://github.com/adrg/xdg?tab=readme-ov-file#xdg-base-directory>  

   **Important:** Most users should choose option A. Option B is for advanced users without an Alby Account.

4. Make sure your Alby Hub is **not running**, then launch the tool from the new folder and follow the on-screen instructions.

5. Once the recovery process starts, the application will periodically display the wallet balance. Keep it running until it finishes. If you have to stop it (with `Ctrl+C`), see [Running the recovery again](#running-the-recovery-again) before starting it again.

6. The application will exit automatically when the recovery process is complete.

## Usage

### Windows Users

- Download the `hub-recovery-windows-x86_64.exe` file from the releases page: <https://github.com/getAlby/hub-recovery/releases>  
- Create a new, empty folder and move the tool and your channel backup file into it.  
- Double-click the tool to execute it.  
- Follow the instructions in the terminal window.

### Linux Users

- Download the `hub-recovery-linux-*` file from the releases page: <https://github.com/getAlby/hub-recovery/releases>  
- Create a new, empty folder and move the tool and your channel backup file into it, for example:

  ```bash
  mkdir ~/hub-recovery-1
  mv ~/Downloads/hub-recovery-linux-* ~/Downloads/channel-backup.* ~/hub-recovery-1/
  ```

- Open the terminal and navigate to the new folder with the `cd` command:

  ```bash
  cd ~/hub-recovery-1
  ```

- Make the file executable by running:

  ```bash
  chmod +x hub-recovery-linux-*
  ```

- Run the tool from the terminal:

  ```bash
  ./hub-recovery-linux-*
  ```

- Follow the on-screen instructions.

### macOS Users

- Download the `hub-recovery-macos` file from the releases page: <https://github.com/getAlby/hub-recovery/releases>  
- Create a new, empty folder and move the tool and your channel backup file into it, for example:

  ```bash
  mkdir ~/hub-recovery-1
  mv ~/Downloads/hub-recovery-macos ~/Downloads/channel-backup.* ~/hub-recovery-1/
  ```

- Open the terminal and navigate to the new folder with the `cd` command:

  ```bash
  cd ~/hub-recovery-1
  ```

- To allow the tool to run, remove the quarantine attribute by running:

  ```bash
  xattr -d com.apple.quarantine ./hub-recovery-macos
  ```

- Run the tool from the terminal:

  ```bash
  ./hub-recovery-macos
  ```

- Follow the on-screen instructions.

### Additional Notes

- The tool stores its data (`ldk_data`, `hub-recovery.state`) next to the tool, and its log file (`hub-recovery.log`) in the folder you run it from. Keep the folder after the recovery until you have verified that the funds are fully recovered.
- If you run the tool and it says "0 sats claimable" and then immediately exits with "Recovery completed successfully", wait 5 minutes and try again in a **new folder** (see [Running the recovery again](#running-the-recovery-again)).
- Until your channel peer has closed the channel on-chain, "Claimable" shows your channel balance **at the time the backup was made**. The amount you actually recover can be lower (for example if you made payments after the backup). The real amount is shown once the closing transaction has confirmed.
- Your funds are available when the "Spendable" balance is near the "Pending sweep" balance. Note that LDK will stay in "Pending Sweep" for many blocks, even though your funds are actually recovered.
- The recovery process may take anywhere from a few hours to up to two weeks, depending on network conditions and the number of open channels.
- It is recommended to wait until funds are fully recovered before starting the Alby Hub again. However, if you do start it before recovery is complete, the Alby Hub may not recognize the new UTXOs. In that case, perform a full re-scan via **Settings > Debug Tools > Reset Router > All**.
- After your funds are recovered, you can start a new Hub using the same seed phrase. Although the best practice is once the funds are recovered, move on to a brand new Alby Hub with its own new seed phrase and move your funds there.

#### Version Compatibility

*Channel backups are dependent on LDK version which is not always backward-compatible.*

- hub-recovery v0.3.0: Channel backups created in Alby Hub v1.21.2 +
- hub-recovery v0.2.1: Channel backups created in Alby Hub < v1.21.2 **see warning below**.

**Warning:** v0.2.2 and older allow the tool to be restarted in the same folder, which can lose your funds (see [How the recovery works](#how-the-recovery-works)). If you use one of these versions, always use a new folder for every run.

### Build From Source Code

***(For Advanced Users Only)***  

You can build the tool directly from the source code on macOS or Linux by running:

```bash
cargo build --release
```

The binary will be stored in `target/release/hub-recovery`.

Do not run the tool inside `target/release`. Copy the binary into a new, empty folder together with your static channel backup file renamed to `channel-backup.json`, then start the recovery process from that folder:

```bash
mkdir ~/hub-recovery-1
cp target/release/hub-recovery ~/hub-recovery-1/
cp /path/to/channel_backup.json ~/hub-recovery-1/channel-backup.json
cd ~/hub-recovery-1
./hub-recovery
```

Alternatively, specify the backup file path with the `-b` option:

```bash
./hub-recovery -b /path/to/channel_backup.json
```


## While Running the Tool

The tool will prompt for your seed phrase. Avoid entering the seed phrase as a command line argument to prevent it from being stored in shell history.

Once started, the tool will periodically display your wallet balance. After all funds are swept, the tool will exit. You can stop the tool with `Ctrl+C`, but never start it again in the same folder; see [Running the recovery again](#running-the-recovery-again).

For all available options, run:

```bash
hub-recovery -h
```

## Running the recovery again

If you stopped the tool, it crashed, your computer restarted, or you were asked to try again, **do not** start it again in the same folder. Instead:

1. Make sure no other copy of the recovery tool is running.
2. Create a new, empty folder (for example `hub-recovery-2`).
3. Copy **only** the recovery tool and your channel backup file into the new folder. Do not copy `ldk_data`, `hub-recovery.state` or any other file from the previous folder.
4. Run the recovery tool from the new folder, as described in [Usage](#usage).

Keep the previous folder: its files (especially `hub-recovery.log`) can help if you need to contact support. Funds that were already recovered by a previous run are in your on-chain wallet and will show up in the new run too.

The tool refuses to start in a folder that contains `ldk_data` or `hub-recovery.state` from a previous run. It never changes or deletes those files.

## How the recovery works

***(Technical details)***

A static channel backup contains, for every channel, the peer's node ID and address and an LDK **channel monitor**. The channel monitor is a snapshot of the channel's state **at the time the backup was made**, usually when the channel was opened. After every payment the channel state moves on, so the monitor in the backup is normally **outdated**: its commitment transaction (our version of the channel state, which our node can sign and publish) has since been **revoked**. If our node ever publishes that revoked commitment transaction, the peer can claim the entire channel balance with a penalty transaction.

The recovery therefore never publishes our own commitment transaction. Instead, it gets the peer to close the channel with the peer's **latest** state:

1. The tool starts an LDK node from your seed phrase, restores the channel monitors from the backup, and starts with an **empty channel manager**: the node does not know about any open channels.
2. It connects to each peer. The peer sends `channel_reestablish` for the channel. Because the channel is unknown, LDK replies with a deliberately invalid `channel_reestablish`, which tells the peer that we have lost our channel state.
3. The peer force-closes the channel by publishing **its own latest commitment transaction**.
4. Our output in the peer's commitment transaction pays to a key derived from your seed phrase and has no penalty path, so it is safe to claim even though our backup is outdated. The restored channel monitor recognises the output and the tool sweeps it to your on-chain wallet.

This is only safe when LDK starts **without any channel manager data**. When the tool runs, LDK saves a channel manager that has no channels. If the tool is started again in the same folder, LDK reloads that channel manager, finds channel monitors without a matching channel and, to protect a normal node, force-closes them by publishing **our** commitment transaction from the monitor. That transaction is the outdated, revoked state from the backup, and the peer can take the whole channel balance with a penalty transaction. This happens:

- if the peer was unreachable during the first run and never closed the channel, or
- if the peer did close the channel, but its closing transaction was not yet confirmed when the tool was started again: our outdated commitment transaction can replace it in the mempool because it pays a higher fee.

For this reason the tool must be run in a **fresh folder every time**, and it refuses to start in a folder that has already been used.

What the recovery relies on:

- **The peer force-closes the channel.** There is no guarantee that it does: the peer is not obliged to act on our `channel_reestablish`. It is in the peer's own interest (otherwise its share of the channel stays locked as well), and standard Lightning node software force-closes automatically. If the peer is offline, the funds stay in the channel until the peer comes back online and closes it; run the tool again (in a new folder) to try again.
- **The peer closes with its latest state.** Because our backup is outdated, our node cannot detect or penalize a peer that publishes one of its own *older* commitment transactions from after the backup was made.
- **The backup contains no pending outbound payments.** If a payment was in flight when the backup was made, LDK publishes our outdated commitment transaction once that payment times out, even on a first run. Alby Hub creates backups when a channel is opened, so this is rare.

### Need Help?

Reach out to our support at <https://getalby.com/help> , here to assist! 😊

### Mutinynet

You can test the tool on Mutinynet with the following command (in a new, empty folder): `./hub-recovery-linux-x86_64  -n signet --esplora-server https://mutinynet.com/api -v`

See [AGENTS.md](AGENTS.md) for a complete end-to-end test on Mutinynet.
