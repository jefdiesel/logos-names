# Encoding changes since 36d2953 (for the CLI, module and JS clients)

Base: registry `36d2953` (branch `registry-04-full`) and lez_nft v3 `145c447`.
Now: registry branch `registry-04-full`, NFT branch `v3-lez04`. Both programs pass
their unit tests and the two-program flow in `tools/engine-test` on the LEZ v0.4.0
engine.

## Registry (`names_core`): no wire changes

Every instruction, message, account layout and PDA seed is byte-for-byte as in
36d2953 and `SPEC.md`. What changed for clients:

1. **Register before Configure is refused** with `the registry has no settings`.
   Before, an unconfigured registry registered names for free and minted no NFT.
2. **`register_generation(slot_id, owner, now, secret)` and
   `claim_generation(slot_id, owner, now)` are now public.** A client needs them
   to compute the token a Register or Claim mints:
   `token_id = nft_token_id(slot_id, register_generation(slot_id, owner_tag(slot_id, signer), now, secret))`.
   `now` must be the exact `now` the Register carries.
3. **Actors.** These come from the real engine run, not from the encodings.

   | Transaction | Public actors to declare (besides the signer's own) | PDA evidence |
   |---|---|---|
   | Register (fee mode) | record, page, settings, commitment, signer's native balance, treasury's native balance, **`(nft_token_address(nft, token_id), nft)`**, **`(lez_nft::config_address(nft), nft)`**; a lapsed re-registration also declares the **old** token `(nft_token_address(nft, old_token_id), nft)` | record, page, commitment, and `Pda { program: nft, seed: nft_token_seed(token_id) }` for the new token |
   | Claim | same NFT pair as Register: the new token and the NFT config, plus the old token if it overwrites one | new token seed under `nft` |
   | List (`display_name: Some`) | record, `market_bucket_address(bucket_of(slot))` | the market bucket seed the first time |
   | Buy | record, page, settings, market bucket (if `in_market`), token, signer's, `pay_to`'s and treasury's native balances | — |
   | Transfer | record, page, settings, token (plus the market bucket, site bucket and site header when they are listed) | — |
   | SetPage / SetRecord with `Upsert` or `Remove` | record (and page for SetPage), `site_bucket_address(bucket, page)`, `site_dir_header_address` | the site bucket and header seeds the first time |
   | CreateSubname / RevokeSubname | parent record, subname record, subname page | the subname seeds |

   The NFT config actor is the only new one. lez_nft checks on every linked Mint
   that the registry is an authorised program in its Config.

## lez_nft v3 (vs 145c447): it now takes the registry's format

| Item | 145c447 | Now |
|---|---|---|
| `LinkedAuthority` | `program ‖ target ‖ record_actor ‖ name_target(NameTarget)` (129/161 B) | `program(32) ‖ authority_actor(32) ‖ target(32) ‖ record_actor(32)` (128 B), the same as `names_core::nft::LinkedAuthority` |
| `NameTarget` | top-level / subname enum | **removed** (NFTs are for top-level names only) |
| Who may send `AuthoritySync` | `(linked.record_actor, linked.program)` | **`(linked.authority_actor, linked.program)`**, i.e. the registry's settings PDA |
| `AuthorityOp::Mint` (`02 00`) | `… linked ‖ collection: Option<(id,u32)> ‖ content ‖ expires` | `token_id ‖ owner ‖ linked ‖ content ‖ expires` (no collection). The program sets `collection = Some((lns_collection_id(linked.program), 0))` itself |
| `AuthorityOp::Unshield` (`02 05`) | `token_id ‖ deed_account` | `token_id` |
| `AuthorityOp::RefreshDeed` (`02 06`) | `token_id ‖ deed_account ‖ expires` | renamed **`RefreshShielded`**: `token_id ‖ expires` |
| `AuthorityOp::SetFrozen` (`02 07`) | — | unchanged (the registry never sends it) |
| `NftRecord` | `… shielded ‖ expires ‖ frozen` | adds **`deed_account: Option<AccountId>`** as the last field (`00` or `01 ‖ 32 B`), set by Shield and cleared by Unshield (also for unlinked tokens; their `Instruction::Unshield` must name the stored account) |
| Hook to the registry (`Message::NftTransferHook`) | `03 ‖ NameTarget ‖ token_id ‖ old ‖ new ‖ now` (138 B) | `03 ‖ id(32) ‖ token_id ‖ old ‖ new ‖ now(u64 LE)` (137 B), `id = linked.target` |
| CLI `info` JSON | `linked.name_target` | `linked.authority_actor`; adds `deed_account` |

The NFT-side transfer of a linked token (`lez_nft::Instruction::Transfer` with
`linked_target: Some(slot)` and `now: Some(t)`) declares: token PDA, registry record,
registry page, registry settings (plus the market bucket, site bucket and site header
when they are listed).

## Deploy order (matters once)

1. Deploy both programs.
2. lez_nft `Configure { admin, authorities: [{ program: REGISTRY, collection_id: Some(lns_collection_id(REGISTRY)) }] }`.
   This reserves the LNS collection id, so nobody can `CreateCollection` with it
   and forge LNS deeds. Do it before anyone else can.
3. Registry `Configure { …, nft_program: NFT }`, signed by the registry program's
   own account. `nft_program` can never change after this.

The byte-for-byte agreement is checked in `tools/engine-test/src/crosscheck.rs`
(encode with one crate, decode with the other). The same hex for `Mint` is pinned
in both repos' unit tests.
