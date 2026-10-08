# Agent instructions

## Building

```bash
cargo build --release   # binary: target/release/hub-recovery
cargo test
```

## End-to-end test on Mutinynet (happy path)

This test checks that the recovery tool recovers funds from an **outdated** static channel backup (SCB), which is what users normally have. It uses a real Alby Hub on Mutinynet (signet), driven through the Alby Hub agent skill. Mutinynet coins have no value, and blocks come about every 30 seconds.

A human is needed to fund the hub from the Mutinynet faucet (<https://faucet.mutinynet.com>).

### Rules

- **Never run the recovery tool twice in the same folder.** Each run needs a new, empty folder that contains only the binary and the SCB. Restarting in a used folder can publish an outdated commitment transaction that the peer penalizes; see "How the recovery works" in the README. The tool refuses to start in a used folder.
- Never run the hub and the recovery tool at the same time. They use the same seed.
- Do not print the mnemonic. Follow the skill's rules about recovery phrase files and tokens.
- Keep every folder (hub data, SCB copy, recovery folder) until the test is evaluated.

### 1. Install the skill and start a hub

```bash
mkdir -p /tmp/hub-recovery-e2e && cd /tmp/hub-recovery-e2e
npx -y skills add getAlby/hub-skill -y
```

Read `.agents/skills/alby-hub/SKILL.md` and its `references/` (especially `mutinynet.md` and `other-installation-options.md`). Then:

- Download the latest `albyhub-Server-Linux-x86_64.tar.bz2` release into a `hub/` folder, verify it against the release `manifest.txt`, and extract it there.
- Create `hub/.env`:

  ```
  NETWORK=signet
  MEMPOOL_API=https://mutinynet.com/api
  LDK_ESPLORA_SERVER=https://mutinynet.com/api
  WORK_DIR=.
  ```

- Start `./bin/albyhub` from `hub/` in the background. The manual binary listens on port 8080, so set `HUB_URL=http://localhost:8080` for `@getalby/hub-cli`.
- Run `setup` and `start --save` with a throwaway password, then wait until `get-info` reports `"running": true` and `"network": "signet"`.

### 2. Fund the hub and open a channel

1. `get-onchain-address`. Ask the human to send **500,000 sats** from the faucet (or use l402 to buy access with a few real sats), then wait until the hub's `get-balances` shows them as spendable.
2. `get-channel-suggestions`: use the signet entry **"Mutinynet Faucet"** (`paymentMethod: onchain`). Run `connect-peer` with its pubkey and host, then `open-channel --amount-sats 400000` (private).
3. Poll `list-channels` until the channel is `active`.

### 3. Take the outdated backup, then make a payment

1. **As soon as the channel is active, and before any payment**, copy the SCB the hub wrote to `hub/ldk/static_channel_backups/<timestamp>.json` into a separate folder and keep it read-only. This is the outdated backup.
2. Pay 10% of `lightning.totalSpendableSat` to the lightning address `refund@lnurl.mutinynet.com`. The CLI cannot pay lightning addresses, so resolve it with LNURL-pay:

   ```bash
   CB=$(curl -s https://lnurl.mutinynet.com/.well-known/lnurlp/refund | jq -r .callback)
   PR=$(curl -s "$CB?amount=<amount_in_msat>" | jq -r .pr)
   npx -y @getalby/hub-cli pay-invoice "$PR"
   ```

   Check that the payment state is `settled`. The SCB is now outdated.
3. Run `backup-mnemonic --output <file>` while the hub is still running.
4. Shut the hub down completely: run `stop`, then terminate the `albyhub` process. Verify that no hub process is left and that ports 8080 and 9735 are free.

### 4. Run the recovery once

```bash
mkdir /tmp/hub-recovery-e2e/recovery-1
cp target/release/hub-recovery /tmp/hub-recovery-e2e/recovery-1/hub-recovery-linux-x86_64
cp <outdated SCB>.json /tmp/hub-recovery-e2e/recovery-1/channel-backup.json
cd /tmp/hub-recovery-e2e/recovery-1
./hub-recovery-linux-x86_64 -n signet --esplora-server https://mutinynet.com/api \
  -b channel-backup.json -s "$(tr -s ' \n' ' ' < <mnemonic file>)" < /dev/null > recovery.stdout 2>&1
```

Run it in the background and leave it running until it exits by itself. Do not restart it.

### 5. Expected result

- `hub-recovery.log` shows `connected to peer` and `Sending bogus ChannelReestablish for unknown channel`.
- The funding output (`https://mutinynet.com/api/tx/<funding_txid>/outspend/<vout>`) is spent by the **peer's latest** commitment transaction: one output for the peer (the amount of the 10% payment) and one for us (the rest of our balance), plus anchors.
- The log has **no** `Queueing monitor update to ensure missing channel` and no `Got broadcast of latest holder commitment` line. We must never broadcast our own commitment.
- A sweep transaction moves our output to the on-chain wallet a few blocks later.
- The tool prints `Recovery completed successfully` and exits.
- We recover about 90% of the channel amount, minus on-chain fees: roughly 1,000 sats for the commitment fee and anchors (paid by us as the channel opener) and roughly 100 sats for the sweep. Example: a 400,000-sat channel with a 39,534-sat payment recovered 359,411 sats.

### 6. Restore the mnemonic in a new hub

Check that the recovered funds are in the wallet a user would restore. The recovery tool must have exited first.

1. Start a **new** hub in a new folder (same `.env` as step 1, no data copied from the first hub).
2. Import the mnemonic during setup, then start the hub:

   ```bash
   npx -y @getalby/hub-cli setup --password <throwaway password> --mnemonic "$(tr -s ' \n' ' ' < <mnemonic file>)"
   npx -y @getalby/hub-cli start --password <throwaway password> --save
   ```

3. Wait until `get-info` reports `"running": true`.

Expected:

- The node ID from `get-node-connection-info` matches `node_id` in the SCB.
- `get-balances` shows `onchain.spendableSat` equal to the final balance printed by the recovery tool ("Your on-chain wallet now holds … spendable sats"): the on-chain change from opening the channel plus the swept channel funds. A full rescan is not needed.
- `list-channels` is empty.

Shut this hub down when done.
