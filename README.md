# Alby Hub Recovery

This is a simple tool to recover funds from channels in a static channel backup file. It reconnects to peer nodes, force-closes all open channels, and waits for the sweeping transactions to confirm.

Learn more about Alby Hub backups here: https://guides.getalby.com/user-guide/alby-account-and-browser-extension/alby-hub/backups

### Attention
> Before proceeding: If you are a subscriber with your Alby Hub on Alby Cloud, please contact Alby Support to check if VSS is enabled for your account. VSS (Virtual Static Storage) allows you to recover your channels along with the funds. This feature is enabled by default for all Hubs created after December 9, 2024 (Hub version `1.11.1`).  
> If your Hub was created after this date, simply start a new Hub, go to **Advanced > Import Recovery Phrase**, and recover your channels without force-closing them. You don’t need this guide or the recovery tool.  

If VSS is not enabled or you’re using a self-hosted/free Hub, follow these steps:

## Quick Start

1. Download the latest release.

2. Choose one of these options:  

   **A) If you have an Alby Account:**  
   Download the channel backup file from https://getalby.com/backups/ and place it in the same directory as the tool.

   **B) If you do not have an Alby Account:**  
   Copy the channel backup file to the same directory as the tool and rename it to `channel-backup.json`.  
   You can find this file in your Alby Hub directory at `WORK_DIR/ldk/static_channel_backups`.  
   Refer to the `WORK_DIR` for your operating system here:  
   https://github.com/adrg/xdg?tab=readme-ov-file#xdg-base-directory  

   **Important:** Most users should choose option A. Option B is for advanced users without an Alby Account.

3. Launch the tool and follow the on-screen instructions.

4. Once the recovery process starts, the application will periodically display the wallet balance. It is safe to interrupt the process with `Ctrl+C` and restart it later.

5. The application will exit automatically when the recovery process is complete.

## Usage

### Windows Users
- Download the `hub-recovery-windows-x86_64.exe` file from the releases page: https://github.com/getAlby/hub-recovery/releases  
- Move it to the same folder as your channel-backup file and double-click to execute.  
- Follow the instructions in the terminal window.

### Linux Users
- Download the `hub-recovery-linux-*` file from the releases page: https://github.com/getAlby/hub-recovery/releases  
- Move it to the same folder as your channel-backup file.  
- Open the terminal and navigate to the folder where you downloaded the recovery tool and backup file. You can do this with the `cd` command, for example:

  ```bash
  cd ~/Downloads
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
- Download the `hub-recovery-macos` file from the releases page: https://github.com/getAlby/hub-recovery/releases  
- Move it to the same folder as your channel-backup file.  
- Open the terminal and navigate to the folder where you downloaded the recovery tool and backup file. You can do this with the `cd` command, for example:

  ```bash
  cd ~/Downloads
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
- It is safe to shut down and restart the tool if necessary. Simply rerun it when ready.
- The recovery process may take anywhere from a few hours to up to two weeks, depending on network conditions and the number of open channels.
- It is recommended to wait until funds are fully recovered before starting the Alby Hub again. However, if you do start it before recovery is complete, the Alby Hub may not recognize the new UTXOs. In that case, perform a full re-scan via **Settings > Debug Tools > Reset Router > All**.
- After your funds are recovered, you can start a new Hub using the same seed phrase. Although the best practice is once the funds are recovered, move on to a brand new Alby Hub with its own new seed phrase and move your funds there.


### Build From Source Code
***(For Advanced Users Only)***  

You can build the tool directly from the source code on macOS or Linux by running:

```bash
$ cargo build --release
```

The binary will be stored in `target/release/hub-recovery`.

To recover funds from a static channel backup file, rename the file to
`channel-backup.json` and place it in the same directory as the binary. Then start the recovery process:

```bash
$ ./hub-recovery
```

Alternatively, specify the backup file path with the `-b` option:

```bash
$ ./hub-recovery -b /path/to/channel_backup.json
```


## While Running the Tool
The tool will prompt for your seed phrase. Avoid entering the seed phrase as a command line argument to prevent it from being stored in shell history.

Once started, the tool will periodically display your wallet balance. After all funds are swept, the tool will exit. You can safely interrupt the process with `Ctrl+C` and restart later if needed.

For all available options, run:

```bash
$ hub-recovery -h
```


### Need Help?:
Reach out to our support at https://getalby.com/help , here to assist! 😊

### Mutinynet
You can test the tool on Mutinynet with the following command: `./hub-recovery-linux-x86_64  -n signet --esplora-server https://mutinynet.com/api -v`
