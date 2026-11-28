# LNS registry v8 + lez_nft v3: Monday deploy on the official LEZ v0.4 testnet

Branch `lns-04-release`. Rehearsed end to end on a local LEZ v0.4.0 chain with
`deploy/rehearse.sh`; step-by-step results in `deploy/rehearsal/results.txt` (what passed,
what did not, and the kit edits the run needed).

## Monday checklist

**Jef provides** (nothing here is in git; ids are public account ids, never keys). Paths in this
file use two env vars, set them on the deploy machine first: `LNS_DEPLOY_DIR` (the folder that
will hold the deployer wallet, e.g. on the Mini's external disk) and `SHORTLIST_BUILD_DIR` (where
the locked-name shards were built, section 4).

| What | Where it goes |
|---|---|
| Official LEZ v0.4 testnet RPC URL, when Logos publishes it | `params.toml` `[network].rpc_url` |
| Its channel id (`getChannelId` on that RPC) | `params.toml` `[network].expected_channel_id` |
| The upgrade-key wallet folder (new, mode 700; password kept apart) | `[keys].upgrade_wallet_dir` and `LEE_WALLET_HOME_DIR` |
| Deployer payer, admin, treasury, NFT admin (`Public/…` ids) | `[accounts]` `deployer_payer`, `admin`, `treasury`, `nft_admin` |
| Owners of the held names `jef`, `logos`, `web`, `android` | `[[held_names]].owner` (empty = stays with the admin) |
| Admin funds for the held names: each is placed permanent at its length tier's price (`jef`, `logos`, `web` 400 M each, `android` 100 M), 1.3 B for all four plus ~50 M fees (it goes to the treasury) | the admin account, before `post-deploy.sh` |
| A funded smoke wallet and its public account | `SMOKE_WALLET`, `[post_deploy].smoke_public_owner` |
| `true` for `admin`, `treasury`, `nft_admin`, `rpc_url`, `upgrade_wallet_dir`, `image_ids` | `[confirmed_by_jef]` |

**Before Monday, still open** (details in section 1):

1. **Reproducible image ids: not built yet.** Needs Docker (GitHub Actions, or the Dell when it
   is back); the agent's push of the build-only branch was refused, so Jef runs one of the two
   ways in section 1.1. Until `deploy/expected-image-ids.txt` exists, `deploy.sh --send` refuses
   a live deploy.
2. **r0vm 3.1.0 on the Mini, behind the guard** (Jef runs it; section 1.4). Every `names` call
   runs the registry program in r0vm before it sends (public calls too), and the v0.4.0 tools
   need r0vm 3.1.0; the Mini has only the guarded 3.0.5. Memory rule:
   `feedback-no-big-risc0-proofs-on-mini.md` (one proof at a time, mimo unloaded for a private
   proof, never `r0vm.real` directly).
3. **Binaries on the Mini:** `names`, `lez-nft`, `lns_imageid` (section 1.2).

**Commands, in order** (on the Mini; `<wallet>` = the upgrade-key wallet folder):

```
deploy/test-dry-run.sh
```
```
LEE_WALLET_HOME_DIR=<wallet> LOGOS_NAMES_RPC=<official rpc> names wallet account new public
```
(it creates the wallet and prints its recovery phrase: run it in a plain Terminal, not in an
agent session, with the output redirected to a mode-600 file kept with the wallet backup;
then fund that payer from the faucet and put its id in `[accounts].deployer_payer`)
```
LEE_WALLET_HOME_DIR=<wallet> deploy/deploy.sh --params deploy/params.toml
```
```
LEE_WALLET_HOME_DIR=<wallet> deploy/deploy.sh --params deploy/params.toml --send
```
(back up `<wallet>` now: it holds both upgrade keys)
```
LEE_WALLET_HOME_DIR=<wallet> deploy/post-deploy.sh --params deploy/params.toml --send
```
```
SMOKE_WALLET=<smoke wallet> deploy/post-deploy.sh --params deploy/params.toml --send --skip-held
```

**Then the program ids go into the clients** (section 3 lists every file): the names CLI
`DEFAULT_PROGRAM` and `DEFAULT_RPC` (today's default `https://testnet.lez.logos.co`: confirm it
or replace it with the official URL), the Names Wallet module `DEFAULT_REGISTRY` (+ version bump), the web app
`CHAINS`/`DEFAULTS` (no entry for the official chain yet), the gateway `DEFAULT_CONFIG` /
`lns.json`, the LNS browser, the Android app, the nameswallet site (a new
`site/networks/<channel>.json`), the MCP server and the board. Then host the locked-name
shards (section 4).

**Still placeholder today, and who provides it**

| Placeholder | Where | Who |
|---|---|---|
| Official LEZ v0.4 RPC URL | `params.toml` `[network].rpc_url` | Jef (from Logos) |
| Channel id of that chain | `[network].expected_channel_id` (empty) | Jef (`getChannelId` on it) |
| Upgrade-key wallet folder | `[keys].upgrade_wallet_dir`, `LEE_WALLET_HOME_DIR` | Jef (new folder, mode 700) |
| Deployer payer | `[accounts].deployer_payer` | Jef (made by the checklist's first command, funded ~200 M) |
| Admin, treasury, NFT admin ids | `[accounts].admin`, `treasury`, `nft_admin` | Jef |
| Owners of `jef`, `logos`, `web`, `android` | `[[held_names]].owner` (empty = stays with the admin) | Jef |
| Admin funds for the held names | the admin account, before `post-deploy.sh` | Jef: 1.3 B LEZ (`jef`, `logos`, `web` at the 1-5 tier's permanent 400 M each, `android` at the 6-7 tier's 100 M; measured in the clean rehearsal) plus ~50 M for fees. Not 1.6 B: `android` has 7 characters |
| Smoke wallet + its funded public account | `SMOKE_WALLET`, `[post_deploy].smoke_public_owner` | Jef (any funded test wallet, not the admin or treasury) |
| `deploy/expected-image-ids.txt` | missing | the Docker build (section 1.1): Jef starts it |
| The Docker-built ELFs on the Mini | `[artifacts].registry_elf`, `nft_elf` (or `NAMES_ELF`/`NFT_ELF`) | from the same build's artifact |
| r0vm 3.1.0 behind the guard | the Mini, `~/.risc0/extensions/v3.1.0-…` | Jef (section 1.4) |
| `true` for `admin`, `treasury`, `nft_admin`, `rpc_url`, `upgrade_wallet_dir`, `image_ids` | `[confirmed_by_jef]` | Jef |
| The old registry id `38jEfVV1…h2gq` in every client default | section 3 | us, after the deploy |

## 0. Rules

- No key, phrase or password goes into `params.toml`, a log, a commit or a chat. Keys live in
  the deployer wallet folder (`LEE_WALLET_HOME_DIR`); `lib.py` refuses a params file that has
  a key-like field.
- Both programs are **upgradeable** on testnet. Each program's id is its header account, and
  that account's key (in the deployer wallet) is its upgrade key. Back the wallet folder up
  (and its password, separately) right after step 3, before anything else.
- `nft_program` is written by the registry's first Configure and can never change. The
  compiled values (2.5% fee, 30-day grace, 21-day premium, terms 30d/1y/5y/permanent, waits
  5 min public / 15 min private, locked-list root `9778bed4…990c`, deed image
  `lns://deed/<name>`) are in the guest image: changing one means a new build and an upgrade.
- The first lez_nft Configure makes its signer the NFT admin. Run step 4 straight after the
  deploy, so nobody else configures it first.

## 1. Before Monday

1. **Reproducible guest builds.** Both `methods/build.rs` (this repo and lez_nft `v3-lez04`
   fabe6c7) build their guest in RISC Zero's builder `risczero/risc0-guest-builder:r0.1.97.0`
   pinned by digest `sha256:7ae0a27f…4468` when `RISC0_USE_DOCKER=1` (the rfp-022 recipe; a
   native build is unchanged). `deploy/repro-build.sh --nft-src <lez_nft tree> --out <dir>`
   builds both and writes `<dir>/expected-image-ids.txt` (both ids and both ELF sha256),
   the two ELFs and `SHA256SUMS`. Needs Docker on linux/amd64 (not Colab, not the Mini).
   Two ways to run it twice:
   - **GitHub Actions** (`.github/workflows/lns-repro-images.yml`: two fresh runners plus a
     compare job). The build-only branch is made, not pushed: commit `<BUILD_SHA>` = this
     branch + the lez_nft tree in `vendor/lez-nft-v3` (no CLAUDE.md/.claude). Pushing it makes
     the registry and lez_nft v3 source public (key scan done: clean). Jef, in this repo:
     ```
     git push origin <BUILD_SHA>:refs/heads/repro/lns-04-images
     ```
     ```
     gh run watch -R jefdiesel/logos-names $(gh run list -R jefdiesel/logos-names -b repro/lns-04-images -L 1 --json databaseId -q '.[0].databaseId')
     ```
     ```
     gh run download -R jefdiesel/logos-names -n lns-guests-a -D lns-guests-a
     ```
   - **The Dell** (when it is reachable): `deploy/repro-build.sh` twice into two folders, with
     `docker builder prune -af` between, then `diff` the two `expected-image-ids.txt`.

   Then copy that `expected-image-ids.txt` to `deploy/expected-image-ids.txt`, commit it, put
   the two ELFs on the Mini and point `[artifacts]` at them. `deploy.sh --send` refuses when an
   ELF's id or sha256 is not the file's (checked by `deploy/test-dry-run.sh` offline and by the
   rehearsal on a chain). The rehearsal's native ids (`e9393158…5501` registry, `5a47f6b6…3f3e`
   lez_nft) are not reproducible and must not go in that file.
2. **Binaries** for the deploy machine (the Mini; never the laptop disk):
   `names` (this repo, `cargo build --release -p names_cli`), `lez-nft`
   (lez_nft `v3-lez04`, `cargo build --release -p lez_nft_cli`) and
   `lns_imageid` (`deploy/imageid`, `cargo build --release`). Set `[tooling]` or
   `NAMES_CLI` / `NFT_CLI` / `IMAGEID`. The CLIs load the programs from the chain, so they
   need not be Docker builds.
3. **Offline checks:** `deploy/test-dry-run.sh` (16 checks, no chain needed).
4. **Rehearsal**: see section 5 (the clean run took <REHEARSAL_MIN> min).
5. **r0vm 3.1.0 on the Mini, guarded** (Jef runs these; nothing else on the Mini changes).
   Rules: memory `feedback-no-big-risc0-proofs-on-mini.md`. The guarded 3.0.5 stays where it
   is (`~/.cargo/bin/r0vm`, used by the rc3 stack); the names CLI finds rzup's copy for its own
   risc0 version (`~/.risc0/extensions/v3.1.0-…/r0vm`) before PATH, so that copy must be the
   guard wrapper, made in the same command as the install:
   ```
   ssh minim4 '~/.risc0/bin/rzup install r0vm 3.1.0 && E=$(ls -d ~/.risc0/extensions/v3.1.0-*/ | head -1) && E=${E%/} && mv "$E/r0vm" "$E/r0vm.real" && sed "s#^\(G=.*REAL=\).*#\1$E/r0vm.real#" ~/.risc0/extensions/v3.0.5-cargo-risczero-aarch64-apple-darwin/r0vm > "$E/r0vm" && chmod 755 "$E/r0vm" && grep REAL= "$E/r0vm" && "$E/r0vm" --version'
   ```
   Check: the `REAL=` line names `…v3.1.0-…/r0vm.real`, `--version` prints 3.1.0,
   `~/.cargo/bin/r0vm --version` still prints 3.0.5, and the first `names` call adds a
   `start … end` pair to `~/.local/r0vm-guard/r0vm.log`. A private registration on the Mini:
   one at a time, `mimo` unloaded first, free memory checked.


## 2. Monday, in order

1. Get the official endpoint and its channel id (`getChannelId`). Fill `[network].rpc_url`
   and `expected_channel_id` in `deploy/params.toml`.
2. Make the deployer wallet (a new folder, e.g. `$LNS_DEPLOY_DIR/wallet`,
   mode 700) and its payer account:
   ```
   LEE_WALLET_HOME_DIR=<wallet> names wallet account new public
   ```
   Fund the payer from the faucet. Measured in the rehearsal (local v0.4.0 chain, default
   fees): both deploys (14 segments), the registry header's 10 M Configure fund and a full
   in-place upgrade of the registry together took 55.4 M from the payer; 200 M is ample. The
   official chain's fees may differ: check one `--dry-run` quote first.
3. Fill `[accounts]`: `deployer_payer`, `admin`, `treasury`, `nft_admin` (public ids only),
   `[keys].upgrade_wallet_dir` (the wallet folder's path), and `[[held_names]].owner`.
   If the treasury is not in the deployer wallet and has never been used on the chain, export
   `TREASURY_PUBLIC_KEY=<its initial key>` (the `initial key` line of `names wallet account get --account-id Public/<id> --keys` in the wallet that holds it) (LEZ 0.4 rejects credits to an unseen account).
4. Jef sets `true` for `admin`, `treasury`, `nft_admin`, `rpc_url`, `upgrade_wallet_dir`,
   `image_ids` in `[confirmed_by_jef]` (and `prices` / `premium_start` / `held_names` if he
   wants to confirm the defaults; they stay admin-adjustable either way).
5. Dry run (sends nothing; prints every command):
   ```
   LEE_WALLET_HOME_DIR=<wallet> deploy/deploy.sh --params deploy/params.toml
   ```
6. Deploy and configure:
   ```
   LEE_WALLET_HOME_DIR=<wallet> deploy/deploy.sh --params deploy/params.toml --send
   ```
   Steps: accounts (headers + segments) → deploy lez_nft → deploy the registry → configure
   lez_nft (admin, authorise the registry, reserve `lns_collection_id(registry)`) → fund the
   registry header and configure the registry (signed by the header, binds `nft_program`) →
   read back (image ids, settings field by field, NFT config). It is resumable: rerun the same
   command after any stop. Output: `deploy/state/deployed-ids.env`.
7. **Back up the wallet folder now** (it holds both upgrade keys and the admin key).
8. Held names and a smoke registration:
   ```
   LEE_WALLET_HOME_DIR=<wallet> deploy/post-deploy.sh --params deploy/params.toml --send
   SMOKE_WALLET=<a funded test wallet> deploy/post-deploy.sh --params deploy/params.toml --send --skip-held
   ```
   Held names take ~6 minutes each (public commit + wait + reveal), one at a time.
9. Put the ids into every client (section 3), then publish the shards (section 4).

## 3. Where each client's program id goes

From `deploy/state/deployed-ids.env`: `LNS_REGISTRY_PROGRAM_ID`, `LNS_NFT_PROGRAM_ID`,
`LEZ_RPC_URL`. The NFT program id is read from the registry's settings by the CLI, the module
and the web app; only the deploy needs it.

| Client | File and symbol (registry id unless noted) |
|---|---|
| names CLI | `cli/src/main.rs` `DEFAULT_PROGRAM` (and the default RPC beside it); or `--program` / `LOGOS_NAMES_PROGRAM` |
| Names Wallet (Basecamp module) | `basecamp-module/rust-lib/src/lib.rs` `DEFAULT_REGISTRY`; test fixtures in `zones.rs` `REGISTRY` and `cli.rs`; then bump `MODULE_VERSION`, `metadata.json` and `neededModule` in `basecamp/qml/Main.qml` together |
| Web app | `gateway/public/app/app.js` `CHAINS` (add the official RPC → registry entry; none exists yet) and `DEFAULTS`; `register.js` default `program` in `registerPublic` and `setText`, and the `_rpc` default; `index.html` RPC radio choices; `wallet.js` `FAUCET_URL` |
| Gateway (`~/logos/lns-gateway`, branch `lns-04-subnames`) | `derive.mjs` `DEFAULT_CONFIG.registry` / `.sequencer`, overridden by env `LNS_REGISTRY` / `LNS_SEQUENCER_URL` (`gateway-config.mjs`); `lns.json` keys `registry`, `rpc`; the default tables in `README.md` and `GATEWAY.md`; `browser-logic/lns.js.txt` (copy of `lns-browser/src/lns.js`). Remove the old base-id fallback after the switch |
| LNS browser (`~/logos/lns-browser`) | `src/lns.js` `registry` |
| Android app (`~/logos/logos-node-android`) | `app/server.py` `NAMES_PROGRAM` (env `NAMES_PROGRAM`); `tools/msg-peer.mjs` `--program` default |
| Site (`~/logos/nameswallet-site`) | `site/apps/lns/lns.js`, `site/bridge/src/lib/lns.js`, `site/explorer/lns/lns.js` `LNS_PROGRAM`; `site/explorer/labels.js`; `site/lns/src/chain.js` `program`; `site/networks/<channel>.json` `registry` (a new file for the official channel); `apps-src/CLAIMS.md` |
| MCP server | `mcp/server.js` `DEFAULT_PROGRAM` |
| Board | `LOGOS_BOARD_REGISTRY` (64-hex form of the registry id) |

## 4. Locked-name shards

The web app proves a name is not locked from 256 static shards. Built from the real list
(root = core `SHORTLIST_ROOT`), they are on the Mini at
`$SHORTLIST_BUILD_DIR/lns-04-b/shortlist-shards/` (61 MB; only `manifest.json` is in git).
Host them at `/app/shortlist-shards/` next to the web app (static assets, no Worker). Rebuild:
`node gateway/app-src/build-shortlist-shards.mjs shortlist/shortlist.bin <out-dir>`.

## 5. Rehearsal (local chain)

```
LEZ_SRC=<LEZ v0.4.0 checkout with testnet/ kit> SEQ_BIN=… WALLET_BIN=… \
NAMES_CLI=… NFT_CLI=… IMAGEID=… NAMES_ELF=… NFT_ELF=… deploy/rehearse.sh --fresh
```
Boots a standalone sequencer on 127.0.0.1:3940 and runs: deploy.sh --send, a price change,
a public and a private registration side by side, the tied NFT check (collection and deed
metadata), list + buy with the 2.5% split, a `cid` site written the way `lns publish` writes it
(`names profile --cid`) and its sites-directory entry, `discover=off` and back on, an on-chain
page, a subname, an in-place upgrade with the header key, then `post-deploy.sh --send` with the
same params (the operator funds the admin; the four held names, `web` handed to a user and the
rest kept by the admin; a rerun that must skip them all; the smoke from `SMOKE_WALLET`), and
last the private registration's reveal. Before the real send it checks the image gate: a send
with a wrong registry ELF must refuse with nothing sent. `EXPECTED_IDS` (default
`deploy/expected-image-ids.txt`) is the gate's file; without one the rehearsal pins the given
ELFs' own ids and writes a WARN line. Every result line carries its elapsed time. Results go to
`deploy/rehearsal/results.txt` (`--from STEP` appends to it; a plain run truncates it and
`--fresh` wipes the whole folder, so copy the tracked file out first). Needs `r0vm` 3.1.0 on the machine (the sequencer
and the CLI both run guests through it).

The testnet kit (`jefdiesel/logos-execution-zone` branch `testnet-0.4`, still on rc3) needs two
edits before it boots LEZ v0.4.0 binaries (made by hand on the rehearsal machine; not yet in
the fork): in `testnet/up.sh` the import flag is `--secret` (was `--private-key`), and in
`testnet/sequencer/chain.sh` (and `testnet/docker/make-config.sh`) the config line
`c["cross_zone"] = None` must become `c.pop("cross_zone", None)` (v0.4.0 has no such field).
v0.4.0 also prints a public account's key as `initial key <hex>` and takes it as
`--to-initial-key` (rc3: `pk` / `--to-pk`); `deploy.sh` and `rehearse.sh` use the new form.

## 6. What can be undone

| Can change later (admin or header key) | Cannot |
|---|---|
| prices (`set-price`), premium (`set-premium`), treasury (`set-treasury`), admin, claims (Configure) | `nft_program` in the registry settings |
| the guest code, by `program-loader update` with the header key (testnet only) | a name handed to a wrong account (only its new holder can move it) |
| lez_nft authorities (its admin) | the program ids (= header accounts) |

## 7. If something stops

- Rerun the same command: every step reads the chain first and skips what is done.
- "header holds image X, expected Y": the ELF is not the one in `expected-image-ids.txt`.
- "treasury … is new to the chain": export `TREASURY_PUBLIC_KEY` (step 3 above).
- A deploy that wrote segments but no header: make new segment accounts (delete the `REG_SEG*`
  or `NFT_SEG*` lines from the state file) and rerun; segments are write-once.
