# LNS registry v8 + lez_nft v3: Monday deploy on the official LEZ v0.4 testnet

Branch `lns-04-release`. Rehearsed end to end on a local LEZ v0.4.0 chain with
`deploy/rehearse.sh`; step-by-step results in `deploy/rehearsal/results.txt` (what passed,
what did not, and the kit edits the run needed).

## Monday checklist

**Jef provides** (nothing here is in git; ids are public account ids, never keys):

| What | Where it goes |
|---|---|
| Official LEZ v0.4 testnet RPC URL, when Logos publishes it | `params.toml` `[network].rpc_url` |
| Its channel id (`getChannelId` on that RPC) | `params.toml` `[network].expected_channel_id` |
| The upgrade-key wallet folder (new, mode 700; password kept apart) | `[keys].upgrade_wallet_dir` and `LEE_WALLET_HOME_DIR` |
| Deployer payer, admin, treasury, NFT admin (`Public/…` ids) | `[accounts]` `deployer_payer`, `admin`, `treasury`, `nft_admin` |
| Owners of the held names `jef`, `logos`, `web`, `android` | `[[held_names]].owner` (empty = stays with the admin) |
| Admin funds for the held names: each is placed permanent at the 1-5 character price (400 M today), 1.6 B for all four (it goes to the treasury) | the admin account, before `post-deploy.sh` |
| A funded smoke wallet and its public account | `SMOKE_WALLET`, `[post_deploy].smoke_public_owner` |
| `true` for `admin`, `treasury`, `nft_admin`, `rpc_url`, `upgrade_wallet_dir`, `image_ids` | `[confirmed_by_jef]` |

**We do before Monday:** the reproducible (Docker) guest builds of both programs and their ids
in `deploy/expected-image-ids.txt` (only the `.example` exists; the rehearsal used non-Docker
builds), and the `names`, `lez-nft`, `lns_imageid` binaries on the Mini (section 1). The
v0.4.0 tools use risc0-zkvm 3.1.0 and the names CLI proves through an external `r0vm`, which
must be 3.1.0: the Mini's guarded `r0vm` points at 3.0.5 (`REAL=` in `~/.cargo/bin/r0vm`).
Install 3.1.0 (`rzup install r0vm 3.1.0`) and point the guard at it, keeping the guard (Jef's
call; any proof the CLI makes on the Mini, such as a private registration, needs it).

**Commands, in order** (on the Mini; `<wallet>` = the upgrade-key wallet folder):

```
deploy/test-dry-run.sh
```
```
LEE_WALLET_HOME_DIR=<wallet> LOGOS_NAMES_RPC=<official rpc> names wallet account new public
```
(fund that payer from the faucet, put its id in `[accounts].deployer_payer`)
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

**Still placeholder today:** every `PLACEHOLDER` in `params.toml` (`rpc_url`,
`upgrade_wallet_dir`, the four `[accounts]`), `expected_channel_id` (empty), the held-name
owners, the `[post_deploy]` smoke wallet/owners, `deploy/expected-image-ids.txt` (missing),
and the old registry id `38jEfVV1…h2gq` in every client default.

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

1. **Reproducible guest builds.** The rehearsal used local (non-Docker) builds:
   registry image `e93931586973b7c24e0990c19c13ced7a250cd2b1b409650746381d395b95501`,
   lez_nft image `5a47f6b61b45f1e8dd59808f00f0e351baf327a868ec5185115b46d761cdbf3e`.
   Build both with the Docker reproducible build (`cargo risczero build` on
   `methods/guest` in each repo), put the ids in `deploy/expected-image-ids.txt`
   (`registry_image_id = …`, `nft_image_id = …`), and point `[artifacts]` at those ELFs.
   `deploy.sh --send` refuses if the ELFs' ids differ from that file.
2. **Binaries** for the deploy machine (Mini or Colab, never the laptop disk):
   `names` (this repo, `cargo build --release -p names_cli`), `lez-nft`
   (`../lez-nft`, branch `v3-lez04`, `cargo build --release -p lez_nft_cli`) and
   `lns_imageid` (`deploy/imageid`, `cargo build --release`). Set `[tooling]` or
   `NAMES_CLI` / `NFT_CLI` / `IMAGEID`.
3. **Offline checks:** `deploy/test-dry-run.sh` (16 checks, no chain needed).
4. **Rehearsal** (optional, ~35 min): see section 5.

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
page, a subname, and an in-place upgrade with the header key. Results go to
`deploy/rehearsal/results.txt` (`--from STEP` appends to it; `--fresh` wipes the whole folder,
including that file, so copy it out first). Needs `r0vm` 3.1.0 on the machine (the sequencer
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
