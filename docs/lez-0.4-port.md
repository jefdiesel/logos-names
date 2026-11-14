# Porting Names to LEZ 0.4

Branch `lez-0.4`, pinned to `v0.4.0-rc1`; branch `rc3-port` moves it to `v0.4.0-rc3` (`bafd07be`, below). The 0.3 deployment stays live until the 0.4 testnet launches.

## rc1 to rc3

What LEZ `v0.4.0-rc3` changed for Names, and what the port did:

- **Clock.** One clock account, `/LEZ/ClockProgramAccount/0000000` (rc1 had three; Names read `…/0000001`). The CLI, the module (`registry.rs`), the web app (`register.js`), the testimonial client and app, and SPEC §8 read the new one. The tick is the last transaction of each block, so the time read is the previous block's: still never ahead of the block a transaction lands in, so the `[now, now + 10 min)` window is unchanged. `AssertTimestamp` is gone; the registry never sent it (its time check is the transaction's validity window, which the sequencer checks against the block it builds).
- **Loader.** Headers and segments are stored as a tagged `LoaderEntry`; the CLI reads a header with `ProgramHeader::from_loader_actor_state`. The loader charges 1 gas per byte it reads, so a deploy costs about the program's size in gas; deploys go through the LEZ wallet, which measures it.
- **Wallet.** It measures each public transaction's gas by simulating it, declares the measured gas plus 25%, prices `max_fee` at the fee headroom, and refuses to send a transaction whose simulation reverts. `private_fee_headroom_blocks` became `fee_headroom_blocks` (the old key still reads). `names wallet` takes rc3's new `--allow-revert`, `--gas-limit`, `--max-fee` and `--tip`, and prints a dry run's report.
- Unchanged: the record layout, ids, PDAs, the commit/reveal rules, the claims format, the public transaction format, risc0 3.0.5 and the toolchain. The guest's image id changes because it links rc3's `lee_core`.

Checked on the Mac mini on 2026-11-13 against a standalone rc3 sequencer (`--features standalone,prove`):

- Registry guest, local (non-Docker) build: rc1 `7d0caf7f…5f4742` (459,464 bytes), rc3 `00ad4571…6dd2dd` (459,496 bytes). The Docker build CI makes (`ci.yml`, job `guest`) was not run here; its id is unverified until CI runs on this branch.
- `cargo test --release --workspace`: 267 passed, 0 failed. `wallet-tokens`: 20 passed. Module (`basecamp-module/rust-lib`, `cargo test --lib`): 321 passed, 2 ignored. JS: resolver 28, app settings 11, claims 4, lez-tx 23, shortlist 4, `basecamp/logic.test.mjs` and the testimonial app's logic test pass.
- `scripts/wallet-e2e.sh`: PASS (`docs/wallet/e2e-rc3-wallet.log`): deploy, deposit mode, a public registration (commit, 15-minute wait, reveal), the locked name, payments, tokens, testimonial, fees, a drop and a restart, a name handed on and back.
- A private registration (owner a private account: shield, private commit, private reveal) passed in RISC0 dev mode only (`docs/wallet/e2e-rc3-private-devmode.log`): fake receipts, accepted by a sequencer run with `RISC0_DEV_MODE=1`. It checks the transactions, fees, clock and windows on rc3, not the proofs.
- With this branch's CLI and its module package (`nix build .#lgx-portable`, names 0.15.3): `lp22-zones-e2e.sh` on two rc3 sequencers 57 PASS, 0 FAIL; the four solo-node Logos Blockchain e2es `chain-ipc` 43/0, `chain-accounts` 30/0, `chain-fee` 23/0, `lp22-inscribe` 32/0 (`docs/wallet/e2e-rc3-*.log`; chain-ipc's version check needs the package in a folder named `…-lgx-<version>`, which a first run lacked).
- The module itself on rc3 (`basecamp-module/tools/try-changes.py` over logosctl, this branch's package and CLI; `docs/wallet/e2e-rc3-module-try-changes.log`, rerun after the review fixes): 36 ok, 1 FAIL. Registering with `auto` twice, receiving privately, entries, profile, page, transfer, release and re-registering all took effect, and `auto` made three distinct private accounts, none shared. Five calls (three `set_text`, `set_profile`, the second `release`) came back from logosctl as `RPC_FAILED` ("callModuleMethod … RPC call failed") although each change landed; the one FAIL is a `resolve` read that failed the same way while waiting. Reusing an account on a retried reveal is covered by unit tests only.

### Known limits

- **Private registration is proven only in RISC0 dev mode.** No real rc3 proof has been run: not the circuit's memory, not whether a real proof (10.5–11.5 min on the Mac mini) still lands inside the 10-minute window with the proof lead tuned on rc1, not the sequencer accepting real receipts for this image id. No private payment to a name and no `names receive` has run on rc3.
- **A public commit or reveal at clock 0, or with a skewed local clock, reverts and pays its gas limit.** The clock reads 0 at block 1 of a fresh chain, and the CLI takes it as is (the module refuses 0); if the clock can't be read, the CLI uses this machine's time less a minute. Neither the CLI's simulation nor the wallet's checks the validity window. A private one is dropped free. Deploying and configuring a registry takes several blocks, so the first case doesn't arise in practice.
- **The locked-name list's root is compiled into the guest** (`core/src/lib.rs`, `9778bed4…`): changing the list means a new guest, a new image id and a redeploy.
- **Web registration is off on this line** (`WEB_REGISTRATION = false` in `gateway/public/app/app.js`): the web app's list is the old one (root `66c75f76…`, 1.6 MB), and a web reveal would fail on chain after the commit and the wait. Registration from the web moves to its own page.
- `receive_privately(name, "auto")` makes a new private account on each call, with no cap: a retried failure leaves unused accounts in the wallet (off chain).
- A payment from a public account leaves a permanent public recovery note at the name's account (the first one).
- **A module call can report failure for a change that landed.** In the try-changes run, logosctl answered `RPC_FAILED` on five of about thirty calls (and on one read), intermittently; the chain showed every change. The app should read the name back before telling the person a change failed. Cause not found (Logos Core IPC, not the module refusing).
- A claim's claimed `now` can be up to 10 minutes early, so a claim can land up to 10 minutes after `claims_until` (from before the port).

## What changed in the program

LEZ 0.4 replaces shards and `plan`/`apply` with actors and messages. The port keeps the same split:

- A transaction is one `Message::Instruction` to the signer's own actor of the registry.
- `instruct` derives every address itself and sends each check or change as a sync `Message::Effect`.
- `handle` accepts an effect only from this program's own actors; `apply` is unchanged.
- Native transfers are sync messages to the payer's balance actor. Their credit is an async message, which runs in the same transaction when the account paid is declared.

Clients must:

- declare every actor a transaction reaches, including the native balance of whoever is paid;
- list `PublicAccountEvidence::Pda` for every account the transaction might create (`name_seeds`, `commitment_seed`, `settings_seed`), or the engine refuses with `UnadmittedPublicActor`;
- pass `treasury` on `Register` and `Renew` in fee mode, since there is no account list any more.

## Carrying names over

The reset would otherwise drop every name, so a holder can claim theirs for free on 0.4:

1. `names snapshot` (branch `snapshot-0.3`) scans the 0.3 chain and writes every registry account with its data. The chain knows no names, so it is keyed by account.
2. The claims list is one leaf per record: `claim_leaf(old record address, owner tag)`.
3. The admin sets `claims_root`, `claims_until` and `claims_program` with `Configure`, with no redeploy.
4. A holder's client derives the old record address from the name and sends `Instruction::Claim` with its proof, signing with the same account. Public account IDs derive the same way in 0.3 and 0.4, so the owner tag matches. Private account IDs do not: restoring the same recovery phrase on 0.4 gave the same npk but a different ID, so a privately held 0.3 name cannot be claimed. The admin can place those names for free instead; `jef` and `logos` take this path.

The claim needs no deposit and no commit step, because only the holder's account can use the proof.

Decisions to keep in mind:

- A claim overwrites a registration that wasn't itself claimed, until `claims_until`. The registrant's deposit stays in the name's account and goes to the claimant on release. This makes snapshot names safe from being taken early, but someone can lose a deposit by registering one, so the client should warn during the claim period.
- A holder who released a name after the snapshot can still claim it during the period.
- `claimed` stops a second claim after a transfer.

## The board

A 0.4 actor sees only its own state, so the board can no longer read registry records. The registry answers `Message::RequireHolder { tag }`, which refuses unless the tag holds the record. The board guest and the CLI are both built with `LOGOS_BOARD_REGISTRY` set to the registry's ID.

## Basecamp

The module keeps building a name's transactions, but the `names` CLI signs them, through a hidden `names module` command (`accounts`, `keys`, `send`, `restore`). This replaces `lez_core`:

- No `lez_core` for 0.4 is released. Open PR #72 on `logos-execution-zone-module` adds one, but its `send_generic_public_transaction` resolves each account to `Public` or `PublicNoSign`, so it can't pass a PDA seed. `Commit`, `Register` and `Claim` create accounts, so they would fail with `UnadmittedPublicActor` and be charged.
- The CLI also tries each change first and sizes its gas, which the module can't: it has no engine.
- People who keep keys in the LEZ Wallet app can't sign here; the CLI's wallet is the only one used.

`names module restore` exists because `names wallet restore-keys` keeps only accounts it finds on the chain, and on a new chain that is none. It moves the old `storage.json` and `names.json` aside as `*.lez-0.3`, and derives 16 public accounts and a new private one.

## Status

- [x] Registry ported; 99 core tests pass.
- [x] Real guest builds for 0.4.
- [x] End-to-end on the real engine (`tools/engine-test`). Public accounts only.
- [x] Snapshot command; `names claim` and `names claims-root`.
- [x] CLI and board ported; public flows, fee mode, claims and the board run on a local 0.4 sequencer.
- [x] Resolver, encoder, signer, MCP server and SPEC.md ported; JS-built transactions run on the real engine.
- [x] Module ported: reads, changes, `cli_claim`, `cli_restore_wallet`, signing through the CLI. Its plugin builds with nix.
- [x] Module: 94 unit tests pass; the plugin loads and `try-changes.py` passes in full against a local 0.4 sequencer. `cli_claim` took back a name there.
- [ ] `names module` (`accounts`, `keys`, `send`, `restore`) compiles; not yet run.
- [ ] App (`Main.qml`) ported: wallet restore, claim warning and claiming, raised private fees. Passes `basecamp/logic.test.mjs`; never run in Basecamp.
- [x] Web: reading the account back, chain clock, Claim screen and proofs from a static snapshot. JS tests pass, with stubbed RPC.
- [x] Web run in a browser against a local 0.4 sequencer: wallet, register (commit, wait, claim), turning on messages, Claim screen including the page. The passkey prompt was stubbed, so a real passkey and chat between two people are still untested. `PAGE_GAS` in `claims.js` is an unmeasured allowance.
- [x] Web registration builds its length proof in the browser from `gateway/public/app/shortlist.bin`, a static file checked against `SHORTLIST_ROOT`. Proofs match `shortlist serve` byte for byte. If the list grows to all 1-4 character names (about 60 MB) this needs shards instead of one file.
- [ ] A snapshot must be published at `gateway/public/claims/lez-0.3.json`; the web app and `cli_claim` fetch it from there. It isn't committed, since it is rerun just before the reset.
- [ ] A page of exactly 40 sealed bytes also parses as an empty record, so it adds a harmless extra leaf. The CLI and the web app treat it the same way, so roots agree.
- [ ] Private flows: private board post and private pay failed on the fee payer's balance, undiagnosed. Low priority until mainnet.
- [ ] Not yet run on chain: `transfer`, `clear-page`, `bridge`, `register` without `--wait`, and others listed in the port report.
- [ ] The faucet must send a new account's public key as evidence, and pay at least 12,000,000: the app now needs that much to register privately, and the faucet gave 2,000,000. It lives outside this repo.
- [ ] Before deploying: `DEFAULT_PROGRAM`, `DEFAULT_BOARD`, `REGISTRY`, `CHAINS`, `register.js`, MCP and `lez-tx-send.mjs` defaults still point at 0.3, as do the module's `DEFAULT_REGISTRY`, `DEFAULT_SEQUENCER` and `FAUCET_URL`.
- [ ] Publish a 0.4 `names` release before the app: `cli_install` runs `install.sh`, which fetches the published CLI, and that one has no `names module`.
- [ ] `names module restore` assumes 0.4 derives public accounts in the same order as 0.3. Not checked against a real 0.3 wallet.
- [ ] `cli_status` starts the CLI twice each time it is asked; with a sequencer that doesn't answer each can wait 20 seconds. Worth caching.
- [ ] The app offers a claim only on a name that reads as unregistered, so a holder can't yet claim over someone's 0.4 registration from Basecamp. The web form and `names claim` can.
- [ ] Deploy on testnet 0.4 when it launches. Rerun the snapshot just before the reset.
