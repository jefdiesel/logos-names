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

## Status

- [x] Registry ported; 99 core tests pass.
- [x] Real guest builds for 0.4.
- [x] End-to-end on the real engine (`tools/engine-test`). Public accounts only.
- [x] Snapshot command; `names claim` and `names claims-root`.
- [x] CLI and board ported; public flows, fee mode, claims and the board run on a local 0.4 sequencer.
- [x] Resolver, encoder, signer, MCP server and SPEC.md ported; JS-built transactions run on the real engine.
- [ ] Basecamp module and app.
- [ ] Web: Claim screen, reading the account back after registering, trying the app in a browser.
- [ ] Private flows: private board post and private pay failed on the fee payer's balance, undiagnosed. Low priority until mainnet.
- [ ] Not yet run on chain: `transfer`, `clear-page`, `bridge`, `register` without `--wait`, and others listed in the port report.
- [ ] The faucet must send a new account's public key as evidence. It lives outside this repo.
- [ ] Before deploying: `DEFAULT_PROGRAM`, `DEFAULT_BOARD`, `REGISTRY`, `CHAINS`, `register.js`, MCP and `lez-tx-send.mjs` defaults still point at 0.3.
- [ ] Deploy on testnet 0.4 when it launches. Rerun the snapshot just before the reset.
