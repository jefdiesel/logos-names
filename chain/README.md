# The Logos Names test chain

A standalone LEZ v0.3.0 chain that keeps the registry usable while the public
LEZ testnet is halted (it stopped at block 13,982 on 2026-10-07). It runs on
one machine: a sequencer, a faucet, and a tunnel the gateway reaches them
through. Nothing here is anchored to the Logos Blockchain yet, so the chain is
only as durable as that machine.

| Piece | Where | What |
|---|---|---|
| Sequencer | `127.0.0.1:3040` | `sequencer_service` from LEZ `v0.3.0`, built with `--features standalone` |
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
3. Start the sequencer with `RISC0_SERVER_PATH` pointing at `bin/r0vm`. Without
   it, genesis fails with `ProgramExecutionFailed("No such file or directory")`.
   `r0vm` 3.0.5 comes from `rzup install cargo-risczero 3.0.5`.
4. `./deploy.sh` deploys and configures the registry and registers `jef` and
   `logos`. Change `PAYER` in it to the registry operator's account.
5. Start `faucet.py`, then the tunnel, and deploy the gateway with
   `CHAIN_ORIGIN` set (see `scripts/publish-cli.sh`).
