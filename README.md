# Alby hub recovery

This is a simple tool to recover funds from channels in a static channel backup
file. It reconnects to the peer nodes and force closes all open channels, then
waits for the sweeping transactions to confirm.

Learn more about Alby Hub backups here: https://guides.getalby.com/user-guide/alby-account-and-browser-extension/alby-hub/backups


## Quick start (Alby Account)
> Before continuing, please contact Alby Support to check if VSS is enabled for your account (Enabled for Alby Cloud users who subscribed to Alby Hub on or after version `1.11.1`). If so, you can start a new hub -> advanced -> import recovery phrase and recover your channels without having to force close them.

1. Download the latest release.
2. Download the channel backup file from https://getalby.com/backups/1 to the same directory as the tool.
3. Launch the tool and follow the instructions.
4. Once the recovery process starts, the application will print the wallet
   balance periodically. It is safe to interrupt the application with `Ctrl+C`
   and restart it later.
5. When the recovery process is complete, the application will exit.

## Quick start (No Alby Account)

1. Download the latest release.
2. Copy the channel backup file to the same directory as the tool and rename it
   to `channel-backup.json`. You can find it in your Alby Hub `WORK_DIR`/ldk/static_channel_backups. You can find the `WORK_DIR` for your operating system [here](https://github.com/adrg/xdg?tab=readme-ov-file#xdg-base-directory).
3. Launch the tool and follow the instructions.
4. Once the recovery process starts, the application will print the wallet
   balance periodically. It is safe to interrupt the application with `Ctrl+C`
   and restart it later.
5. When the recovery process is complete, the application will exit.

## Usage

The app can be built from source with:

```bash
$ cargo build --release
```

The resulting binary will be stored in `target/release/hub-recovery`.

To recover funds from a static channel backup file, rename the file to
`channel-backup.json` and place it in the same directory as the binary. Start
recovery by launching the application:

```bash
$ ./hub-recovery
```

Alternatively, the path to the channel backup file can be specified with the
`-b` option:

```bash
$ ./hub-recovery -b /path/to/channel_backup.json
```

The tool will prompt for the seed phrase. It is also possible to specify the
seed phrase as a command line argument. However, this is discouraged as the seed
phrase will be stored in the shell history.

After the tool is started, it will periodically print wallet balance. As soon as
all funds are swept from the channels, the tool will exit. It is safe to
interrupt it with `Ctrl+C` and restart later.

For all available options, run:

```bash
$ hub-recovery -h
```
