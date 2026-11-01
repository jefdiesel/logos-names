# Porting Names to LEZ 0.4

Branch `lez-0.4`, pinned to `v0.4.0-rc1`. The 0.3 deployment stays live until the 0.4 testnet launches.

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
- [ ] Module's unit tests and plugin not re-run since its `logos_module_install` entry point was restored. Without that entry point the plugin died on load, so **no module call has yet run on a host or on a chain**. Next: rebuild, load in a logosctl session with Basecamp's host, run `basecamp-module/tools/try-changes.py`.
- [ ] `names module` (`accounts`, `keys`, `send`, `restore`) compiles; not yet run.
- [ ] App (`Main.qml`) ported: wallet restore, claim warning and claiming, raised private fees. Passes `basecamp/logic.test.mjs`; never run in Basecamp.
- [x] Web: reading the account back, chain clock, Claim screen and proofs from a static snapshot. JS tests pass, with stubbed RPC.
- [ ] Web not run against a sequencer or in a browser. `PAGE_GAS` in `claims.js` is an unmeasured allowance.
- [ ] A snapshot must be published at `gateway/public/claims/lez-0.3.json`; the web app and `cli_claim` fetch it from there. It isn't committed, since it is rerun just before the reset.
- [ ] A page of exactly 40 sealed bytes also parses as an empty record, so it adds a harmless extra leaf. The CLI and the web app treat it the same way, so roots agree.
- [ ] Private flows: private board post and private pay failed on the fee payer's balance, undiagnosed. Low priority until mainnet.
- [ ] Not yet run on chain: `transfer`, `clear-page`, `bridge`, `register` without `--wait`, and others listed in the port report.
- [ ] The faucet must send a new account's public key as evidence, and pay at least 12,000,000: the app now needs that much to register privately, and the faucet gave 2,000,000. It lives outside this repo.
- [ ] Before deploying: `DEFAULT_PROGRAM`, `DEFAULT_BOARD`, `REGISTRY`, `CHAINS`, `register.js`, MCP and `lez-tx-send.mjs` defaults still point at 0.3, as do the module's `DEFAULT_REGISTRY`, `DEFAULT_SEQUENCER` and `FAUCET_URL`.
- [ ] Deploy on testnet 0.4 when it launches. Rerun the snapshot just before the reset.
