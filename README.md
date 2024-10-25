# Alby hub recovery

This is a simple tool to recover funds from channels in a static channel backup
file. It reconnects to the peer nodes and force closes all open channels, then
waits for the sweeping transactions to confirm.

## Usage

The app can be built from source with:

```bash
$ cargo build --release
```

The resulting binary will be stored in `target/release/hub-recovery`.

Recovery can be initiated by running the tool:

```bash
$ hub-recovery -b /path/to/channel_backup.json
```

The tool will prompt for the seed phrase and the passphrase. The latter is
optional; just hit Enter if no passphrase is needed.

After the tool is started, it will periodically print wallet balance. As soon as
all funds are swept from the channels, the tool will exit. It is safe to
interrupt it with `Ctrl+C` and restart later.

It is also possible to specify both the seed phrase and passphrase as command
line arguments. However, this is discouraged as the seed phrase will be stored
in the shell history.

For all available options, run:

```bash
$ hub-recovery -h
```
