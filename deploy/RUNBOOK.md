# LNS Registry v8 + `lez_nft` v3 — Monday Launch Runbook (Official LEZ v0.4 Testnet)

> **One-shot deployment.** `names_core` v8 and `lez_nft` v3 are deployed **once** to the official LEZ v0.4 testnet. Several critical values are **FIXED FOREVER** at compile time or on the initial `Configure` transaction. Follow every step in order; do not skip checks.

---

## 0. Security & Key Handling Rules

- **NEVER** put private keys, recovery phrases, or wallet passwords into `deploy/params.toml`, `deploy/state/*`, or git.
- All signing is performed at run time by existing wallet tooling using `LEE_WALLET_HOME_DIR` and `LEE_WALLET_PASSWORD` (leave `LEE_WALLET_PASSWORD` unset to be prompted interactively on a TTY).
- `deploy/params.toml` and `deploy/state/deploy-state.json` hold **public account IDs and program IDs only**.
- `deploy/params.toml` should stay local (or reviewed carefully before committing). `deploy.sh` refuses to run if it detects any secret-like key name inside `params.toml`.

---

## 1. Pre-Monday Preparation (Before Endpoint Is Announced)

### 1.1 Prerequisite: Ensure `repo/cli` and `../lez-nft` are built for v8 / v3
1. Confirm `repo/cli` has been updated to v8 (needed by `post-deploy.sh` for v8 `register` / `transfer` / `resolve`; note that `deploy.sh` already sends exact 411-byte v8 `Configure` via `names module send`, so `deploy.sh` works even with `--use-cli-configure` off).
2. Build reproducible RISC-V guests (Docker / `cargo risczero`) for both programs:
   - Registry v8 (`names_core` v8):
     ```bash
     cargo risczero build --manifest-path methods/guest/Cargo.toml
     ```
   - Tied NFT (`lez_nft` v3):
     ```bash
     cargo risczero build --manifest-path ../lez-nft/methods/guest/Cargo.toml
     ```
3. Build release CLIs:
   ```bash
   cargo build --release -p names_cli
   cargo build --release --manifest-path ../lez-nft/Cargo.toml
   ```
4. Verify `shortlist/shortlist.bin` is present (`60,892,524` bytes = `1,845,228` leaves × 33 bytes) and matches `shortlist/root.hex` (`9778bed49005fe7401d25685a27d9cf3e681fa0582b562336cdb8ee9661d990c`).

### 1.2 Pin Expected Image IDs
1. Copy `deploy/expected-image-ids.example.txt` → `deploy/expected-image-ids.txt`.
2. Fill in the exact 64-hex `registry_image_id` and `nft_image_id` (and optionally `registry_elf_sha256` / `nft_elf_sha256`) from step 1.1.
3. Configure `[tooling].imageid_cmd` in `params.toml` (or export `IMAGEID_CMD`) so `deploy.sh` computes the image ID directly from each ELF on disk and compares it against `expected-image-ids.txt`.

### 1.3 Prepare `deploy/params.toml`
1. Copy `deploy/params.example.toml` → `deploy/params.toml`.
2. Leave `confirmed_by_jef = false` until Monday morning final review.
3. Verify the `[immutable]` section (already pre-filled with Jef's decided constants; `deploy.sh` cross-checks every line against `repo/core/src/` and `repo/shortlist/root.hex`):
   - `record_version = 8`
   - `shortlist_root = "9778bed49005fe7401d25685a27d9cf3e681fa0582b562336cdb8ee9661d990c"` (`shortlist_depth = 21`, `1,845,228` leaves)
   - `sale_fee_bps = 250` (2.5% = 1/40 to treasury; 0 on plain transfers)
   - `grace_days = 30`
   - `premium_days = 21` (daily halving to 0)
   - `public_wait_minutes = 5` / `private_wait_minutes = 15` / `commit_max_age_hours = 24`
   - `term_days = [30, 365, 1825]` (+ permanent = `u64::MAX`)
4. Verify `[registry]` defaults (all admin-adjustable later):
   - `mode = "fee"`
   - `tier_5      = [2_000_000, 20_000_000, 80_000_000, 400_000_000]`
   - `tier_6_7    = [  500_000,  5_000_000, 20_000_000, 100_000_000]`
   - `tier_8_plus = [  100_000,  1_000_000,  4_000_000,  20_000_000]`
   - `premium_start = 4_000_000_000`
   - `[registry.claims] enabled = false`
5. Fill in `[[held_names]]` (`jef`, `logos`, `web`, `android`, plus any additional names Jef decides to place on day 1) once target v0.4 account IDs exist.

### 1.4 Run Local Dry-Run Verification
```bash
./deploy/test-dry-run.sh
```
Optionally pass `SEQ_BIN` and `SEQ_CONFIG` (LEZ `networks/local.json`) to test `deploy.sh --dry-run` against a real local standalone sequencer and confirm **zero** transactions are sent.

---

## 2. Monday Launch Procedure

### Step 1 — Obtain Official Endpoint & Fund Deployer Wallet
1. Record the official LEZ v0.4 testnet sequencer RPC URL and channel ID (`getChannelId`).
2. Point your deployer wallet's `$LEE_WALLET_HOME_DIR/wallet_config.json` at the new RPC URL (`sequencer.sequencer_addr`).
3. Create/select the v0.4 public accounts in your wallet:
   - `deployer_payer` (fund from testnet faucet — needs enough for two multi-segment program deploys + header gas + treasury admission; budget ≥ 100M LEZ headroom)
   - `nft_admin` (public account in deployer wallet)
   - `admin` (public account in deployer wallet)
   - `treasury` (public account — **note below on `admin == treasury`**)
   - Plus non-admin smoke accounts (`smoke_public_owner` public, `smoke_private_owner` private, funded with ≥ 10M LEZ each) in a separate smoke wallet if desired.

> **Important fee-mode note (`core/src/lib.rs:Effect::RequirePrice`):**
> In `mode = "fee"`, only `treasury` itself pays 0 fee on registration.
> - **Option A (recommended for launch day):** Set `admin == treasury` initially (matching v5 live). Admin registers `jef`, `logos`, `web`, `android` permanent for 0 fee (gas only). Afterward, rotate `treasury` to a dedicated treasury account via a second `Configure` if desired.
> - **Option B:** Set `admin != treasury` from the start, and fund `admin` with enough LEZ to pay the tier fees for all held names (e.g. 3 × 400M + 100M = 1.3B LEZ for `jef`/`logos`/`web`/`android` permanent; the fees immediately land in `treasury`).

4. Fill `[network].rpc_url`, `[network].expected_channel_id`, `[accounts].*`, `[[held_names]].owner`, and `[post_deploy].smoke_*_owner` in `deploy/params.toml`.

**Check after Step 1:**
```bash
python3 deploy/lib.py rpc-probe "$LEZ_RPC_URL" "$EXPECTED_CHANNEL"
```
Confirm `last_block_id` is advancing and `channel_id` matches the official testnet announcement.

---

### Step 2 — Full Dry-Run Against Official Testnet
Keep `confirmed_by_jef = false` and run:
```bash
./deploy/deploy.sh --params deploy/params.toml --rpc "$LEZ_RPC_URL" --dry-run
```
**Check after Step 2:**
- `params.toml` validates cleanly (only expected warning is `confirmed_by_jef is NOT true`).
- Both guest ELF image IDs match `deploy/expected-image-ids.txt`.
- Sequencer probe succeeds; `channel_id` matches; `deployer_payer` balance is non-zero.
- `wallet_config.json` matches `$LEZ_RPC_URL`.
- Review every printed transaction, account ID, price cell, `premium_start`, `shortlist_root`, and the 411-byte `Configure` Borsh hex.
- Verify **0 transactions** were sent (`deployer_payer` nonce unchanged).

Also preview post-deploy:
```bash
./deploy/post-deploy.sh --params deploy/params.toml --rpc "$LEZ_RPC_URL" --dry-run
```

---

### Step 3 — Jef Sign-Off (`confirmed_by_jef = true`)
Jef reviews the dry-run output — especially every **FIXED FOREVER** line — and sets:
```toml
confirmed_by_jef = true
```
in `deploy/params.toml`.

---

### Step 4 — Live Deploy & Configure (`deploy.sh`)
```bash
./deploy/deploy.sh --params deploy/params.toml --rpc "$LEZ_RPC_URL"
```
Type `DEPLOY` at the final confirmation prompt.

What `deploy.sh` executes in exact order:
1. Re-validates `params.toml` (`confirmed_by_jef = true`) and compile-time immutables.
2. Re-verifies ELF image IDs against `expected-image-ids.txt` and `shortlist.bin` size.
3. Probes sequencer, verifies `channel_id`, `wallet_config.json`, and wallet health.
4. Deploys `lez_nft` v3 via `names wallet program-loader deploy` → saves `nft_program_id` to `deploy/state/deploy-state.json` and verifies header on chain.
5. Deploys LNS Registry v8 via `names wallet program-loader deploy` → saves `registry_program_id` and verifies header on chain.
6. Funds `registry_program_id` (`registry_gas_fund`) so it can sign initial `Configure`, funds `nft_program_id` (`nft_header_gas_fund`), and funds `treasury` (`treasury_init_fund`) if unadmitted.
7. Sends `lez_nft::Instruction::Configure` authorizing `registry_program_id` and reserving `lns_collection_id(registry)`.
8. Sends initial 411-byte `names_core::Instruction::Configure` signed by `registry_program_id` — **permanently binding `Settings.nft_program = nft_program_id`**.
9. Reads back on-chain `Settings` PDA via `lib.py verify-settings` and asserts every field matches `params.toml` bit-for-bit. Writes `deploy/state/deployed-ids.env`.

**Check after Step 4:**
- `deploy.sh` exits `0` with `DEPLOY COMPLETE & VERIFIED`.
- `deploy/state/deploy-state.json` and `deploy/state/deployed-ids.env` exist (`0600`) and contain both new program IDs.
- **Back up the deployer wallet** (especially the header keys if you plan to keep upgrade authority, and the `admin` / `nft_admin` / `treasury` keys).

---

### Step 5 — Post-Deploy Held Names, E2E Smoke Tests & NFT Link Check (`post-deploy.sh`)
```bash
./deploy/post-deploy.sh --params deploy/params.toml --state deploy/state/deploy-state.json
```
*(Expect ~35–45 minutes total if running all phases: 4 held names × 5m public wait + 5m public smoke + 15m private smoke + local RISC0 proof. Use `--skip-private-smoke` only if running private smoke from another machine with `r0vm`, then run it immediately afterward!)*

**Check after Step 5:**
1. **Held names:** `jef`, `logos`, `web`, `android` (and any extras) are registered and transferred; `lib.py inspect-record` reports `owner_matches: true` for each target owner.
2. **Public E2E smoke:** `smoke_public_name` registered after 5-min wait, `names resolve` succeeds, and `treasury` balance increased by `100,000` LEZ (`tier_8_plus[0]`).
3. **Private E2E smoke:** `smoke_private_name` registered after 15-min wait with local RISC0 proof, `NameRecord.owner` matches `owner_tag(slot_id, smoke_private_owner)`, and `treasury` balance increased by `100,000` LEZ.
4. **NFT lockstep check:** `lib.py verify-nft-link` exits `0` confirming `NameRecord.nft_token_id == NftRecord.token_id`, `owner`, `expires`, `shielded`, and all 4 `LinkedAuthority` fields match.
5. **Client config printout:** Copy the Phase 5 output for Step 6.

---

### Step 6 — Update Every Client's Program IDs

Update each client in order (verify against the Phase 5 output of `post-deploy.sh`):

| Client / Service | Exact File(s) & Symbols to Update | Notes |
|---|---|---|
| **CLI (`names`)** | `repo/cli/src/main.rs`: `DEFAULT_RPC`, `DEFAULT_PROGRAM`, `DEFAULT_NFT` (v8), `RETIRED_PROGRAMS` | Rebuild, tag, publish release binaries (`scripts/publish-cli.sh` / `cli-release.yml`) |
| **Names Wallet / Basecamp Module** | `repo/basecamp-module/rust-lib/src/lib.rs`: `DEFAULT_SEQUENCER`, `DEFAULT_REGISTRY`, `DEFAULT_NFT` (v8), `RETIRED_REGISTRIES` | Rebuild native `.dylib`/`.so` module & update `metadata.json` version if bumping |
| **Web App (`nameswallet.app`)** | `repo/gateway/public/app/app.js` (`CHAINS`, `DEFAULTS`), `repo/gateway/public/app/register.js` (default `program`, `_rpc`, `SHORTLIST_ROOT`/`SHORTLIST_DEPTH`) | Redeploy static site (`nameswallet.app` + gateway `/app`) |
| **Gateway (`lnsweb.online`)** | `repo/gateway/public/index.html` + M2 host `serve.mjs` env (`LOGOS_NAMES_RPC`, `LOGOS_NAMES_PROGRAM`, `LOGOS_NFT_PROGRAM`) | Restart gateway service on M2; verify `https://jef.lnsweb.online` once Jef publishes a page |
| **MCP Server** | `repo/mcp/server.js`: `DEFAULT_RPC`, `DEFAULT_PROGRAM` | Restart / publish updated MCP package |
| **Android App** | External Android repo config: `LNS_REGISTRY_PROGRAM_ID`, `LNS_NFT_PROGRAM_ID`, `LEZ_RPC_URL`, `LEZ_CHANNEL_ID`, `LNS_SHORTLIST_ROOT` | Build & test signed APK against live testnet |
| **LNS Board** | `repo/board/core/src/lib.rs`: build-time env `LOGOS_BOARD_REGISTRY=<64-hex registry ID>`, then deploy board guest and update `DEFAULT_BOARD` in `repo/cli/src/board.rs` | Must be rebuilt & redeployed **after** the registry has its final program ID |
| **Docs & Spec** | `repo/SPEC.md` §11, `repo/README.md`, `repo/docs/INTEGRATING.md` | Record live program IDs, image IDs, and launch date |

---

## 3. Rollback Matrix (What CAN vs. CANNOT Be Rolled Back)

### CAN Be Changed / Rolled Back After Launch (No Redeploy Needed)
| Parameter / Action | How to Change | Notes |
|---|---|---|
| **Price table (`[3][4]`)** | `Instruction::SetPrice { tier, term, price }` or `Instruction::Configure` signed by `admin` | Immediate; affects future registrations & renewals only |
| **Re-release `premium_start`** | `Instruction::SetPremium { premium_start }` or `Instruction::Configure` signed by `admin` | Set to `0` to disable re-release premium immediately |
| **Pricing mode (`fee` vs `deposit`) & `deposits`** | `Instruction::Configure` signed by `admin` | Takes effect for new registrations |
| **`treasury` account** | `Instruction::Configure` signed by `admin` | Ensure new `treasury` is admitted on chain first! |
| **`admin` account** | `Instruction::Configure` signed by current `admin` | **Danger:** verify you control the new admin key first |
| **Legacy `claims_*` window** | `Instruction::Configure` signed by `admin` | Can enable/disable/extend claims later |
| **`lez_nft` authorized programs / `nft_admin`** | `lez_nft::Instruction::Configure` signed by `nft_admin` | Never revoke the live LNS registry once names are minted |
| **Client configs (web, gateway, wallet, CLI, Android)** | Revert config & redeploy clients | Always safe as long as on-chain state hasn't been announced publicly |

### CANNOT Be Rolled Back (Irreversible Once Executed)
| Item | Why It Cannot Be Rolled Back | Mitigation / Only Recovery |
|---|---|---|
| **`Settings.nft_program` binding** | Enforced immutable by `Effect::WriteSettings` after the first `Configure` lands | Verify `NFT_PROGRAM_ID` in dry-run & state file before Step 9. If bound to wrong program: **abandon registry & deploy a brand-new registry program** before publishing IDs to clients |
| **Compile-time constants** (`SHORTLIST_ROOT`, `SALE_FEE_BPS = 250`, `GRACE_MS = 30d`, `PREMIUM_DAYS = 21`, `5m/15m` waits, `TERM_LENGTHS_MS`, `VERSION = 8`) | Baked into the RISC-V guest ELF image ID | Cross-checked automatically by `deploy.sh` Step 1 & Step 2. Changing requires rebuilding ELF + program-loader upgrade (if header key retained) or full redeploy |
| **Transfer of a held name (`jef`, `logos`, `web`, `android`) to wrong account ID** | Once transferred, only the recipient's private key can transfer or release it (`admin` cannot seize a registered name) | Triple-check every `[[held_names]].owner` ID before running `post-deploy.sh`; `lib.py` already rejects blank, all-zero, and single-case mangled IDs |
| **Losing `admin` or `nft_admin` private key** | No recovery backdoor exists in `names_core` or `lez_nft` | Back up deployer wallet immediately after Step 4; verify signatures before rotating `admin` |
| **Public announcement of program IDs** | Once third-party users register names and mint tied NFTs, moving to a new registry requires migration / legacy claim snapshot | Do NOT publish program IDs to clients until Steps 4 & 5 pass 100% |

---

## 4. Troubleshooting & Recovery

- **Mid-deploy RPC timeout or disconnect:**
  Simply re-run the exact same `deploy.sh` command with the same `--state` file. `deploy.sh` checks `deploy-state.json` and resumes from the first incomplete step without deploying duplicate programs.
- **`UnadmittedPublicActor` on first paid registration:**
  `treasury` has never been admitted on chain. Send `1,000,000` LEZ to `treasury` (`names wallet auth-transfer send`) and retry.
- **Initial `Configure` fails with gas / insufficient balance:**
  `registry_program_id` itself signs initial `Configure`. Fund `Public/<registry_program_id>` with additional LEZ and re-run `deploy.sh` (it will skip Steps 5–8 and retry Step 9).
- **`lez_nft` rejects mint with unauthorized authority:**
  Verify Step 8 (`lez_nft::Instruction::Configure`) authorized `registry_program_id` with `collection_id = Some(lns_collection_id(&registry))`. Re-run `lez_nft` Configure as `nft_admin` (still mutable).
- **Private smoke test times out / `r0vm` missing:**
  Verify `r0vm --version` is on `PATH` and matches the LEZ v0.4 RISC0 toolchain version. You can re-run just Phase 3 with:
  ```bash
  ./deploy/post-deploy.sh --params deploy/params.toml --state deploy/state/deploy-state.json \
    --skip-held --skip-public-smoke --skip-nft-check
  ```
