# The Logos Names test chain

A LEZ chain that keeps the registry usable while the public LEZ testnet is
halted (it stopped at block 13,982 on 2026-10-07). It runs on one machine: a
sequencer, a faucet, and a tunnel the gateway reaches them through.

Since 2026-10-10 it is **anchored to the Logos Blockchain**: the sequencer runs
LEZ `dev` at `4fba494` (the v0.3.1 candidate; v0.3.0 cannot talk to node 0.3.1)
and posts every block to its own channel,
`c20f23a3c56544c9cf063c94618af29457877182c1c102be55dbfd4e536ee66b`, through the
Logos node on the same machine (`node_url` `http://127.0.0.1:8081`). The node's
key in `funding_key` pays for each post, about 1-2k, or roughly 9M a day at one
block every 15 s. If the Logos Blockchain stops or relaunches, this chain stops
with it.

| Piece | Where | What |
|---|---|---|
| Sequencer | `127.0.0.1:3040` | `sequencer_service` from LEZ `dev` `4fba494` with `lez-wal-cap.patch`, normal (anchored) build |
| Faucet | `127.0.0.1:3041` | `faucet.py`: 50,000,000 to an account once a day |
| Registry | `CsiBzfERqtLz9dYV9dPWzFPMbaUcUagYeCmvvJVmCGcF` | the CI guest `542ae051…`, prices 10000/100000/400000 |
| Public | gateway `/rpc`, `/faucet` | the Worker forwards to the tunnel; `/rpc` passes reads and `sendTransaction` only |

## Setting it up

Everything lives in `~/lez-chain`:

```
bin/sequencer_service   bin/r0vm   bin/names
config.json             (from config.example.json, with fresh keys)
home/                   the chain's database
faucet-wallet/          the faucet and registry accounts' keys, mode 600
deploy.sh   faucet.py   logos_names-ci-542ae051.bin   jef-page.html
```

1. Make the wallet: `LEE_WALLET_HOME_DIR=~/lez-chain/faucet-wallet names wallet
   account new public` twice (faucet, then registry operator), saving the output
   to a mode-600 file: it holds the recovery phrase. Point its
   `wallet_config.json` at `http://127.0.0.1:3040/`.
2. Write `config.json` from `config.example.json`: put both accounts in
   `genesis`, a fresh random `signing_key`, and `home`. Never use the keys in
   LEZ's debug config; they are public.
3. For anchoring, set `bedrock_config.node_url` to the Logos node, `funding_key`
   to a key that node's wallet holds funds under, and a fresh random
   `channel_id`; the sequencer creates the channel on first start. Start the sequencer with `RISC0_SERVER_PATH` pointing at `bin/r0vm`. Without
   it, genesis fails with `ProgramExecutionFailed("No such file or directory")`.
   `r0vm` 3.0.5 comes from `rzup install cargo-risczero 3.0.5`.
4. `./deploy.sh` deploys and configures the registry and registers `jef` and
   `logos`. Change `PAYER` in it to the registry operator's account.
5. Start `faucet.py`, then the tunnel, and deploy the gateway with
   `CHAIN_ORIGIN` set (see `scripts/publish-cli.sh`).

## Disk: apply `lez-wal-cap.patch`

Unpatched, the sequencer's RocksDB keeps every write-ahead log: the tables
written a little each block never fill a memtable, so nothing lets old logs go
until RocksDB's default cap of several GB. On 2026-10-11 that reached 5.4 GB in
a day and was growing about 2 GB an hour, while the chain's real data was 36 MB.
The patch caps the logs at 256 MB (`set_max_total_wal_size`). A restart also
clears them, but takes about 10 minutes, since the sequencer re-reads its
channel's history from the node.
