# Alby hub recovery

This is a simple tool to recover funds from channels in a static channel backup
file. It reconnects to the peer nodes and force closes all open channels, then
waits for the sweeping transactions to confirm.

## Quick start

1. Download the latest release.
2. Copy the channel backup file to the same directory as the tool and rename it
   to `channel-backup.json`.
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
