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
4. A holder's client derives the old record address from the name and sends `Instruction::Claim` with its proof, signing with the same account. Account IDs derive the same way in 0.3 and 0.4, so the owner tag matches.

The claim needs no deposit and no commit step, because only the holder's account can use the proof.

Decisions to keep in mind:

- A claim overwrites a registration that wasn't itself claimed, until `claims_until`. The registrant's deposit stays in the name's account and goes to the claimant on release. This makes snapshot names safe from being taken early, but someone can lose a deposit by registering one, so the client should warn during the claim period.
- A holder who released a name after the snapshot can still claim it during the period.
- `claimed` stops a second claim after a transfer.

## Status

- [x] Registry ported; 98 core tests pass.
- [x] Real guest builds for 0.4.
- [x] End-to-end on the real engine (`tools/engine-test`): configure, commit, register, deposit, transfer, release and refund, claim, and refused attacks. Public accounts only.
- [x] Snapshot command.
- [ ] Private holders: the same flows through a privacy-preserving transaction. **Not tested, and claims for private holders depend on it.**
- [ ] CLI (`cli/`): transaction layer against the 0.4 wallet API. 23 errors in `main.rs` at last count.
- [ ] Board: same port. Out of the workspace on this branch for now.
- [ ] Basecamp module and app, web app and `lez-tx.js`, gateway, MCP server, resolver.
- [ ] SPEC.md: new wire format and vectors.
- [ ] Deploy on a local 0.4 chain, then on testnet 0.4 when it launches.
