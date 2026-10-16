# Registry v5 plan

Decided with the maintainer on 2026-10-16. Nothing here is built yet.

## Name derivation

- **Argon2id** (v0x13, m = 64 MiB, t = 3, p = 1, 32-byte output) replaces
  PBKDF2-SHA256 at 100k iterations. A GPU dictionary of common names becomes
  about 50 times more expensive (about a minute per 5M names today, about 50
  minutes with Argon2id). Derivation stays on clients; the on-chain program never
  derives (the risc0 guest has no KDF).
- **One shared salt for testnet v5 and mainnet**, not one per registry. This lets
  a testnet name carry over to mainnet (a name's ID is the same on both). The cost
  is small: a dictionary of guessed names works on both networks, but they hold
  the same names anyway.
- New domain strings, all labelled `v5` consistently (today they say v2/v3 while
  the registry is v4.1), a new `VERSION`, and new SPEC section 10 vectors,
  cross-checked between the Rust `argon2` crate and the JS resolver.
- Every resolver changes: `seal/`, `cli/`, `basecamp-module/rust-lib/`,
  `resolver/names.js` (with `mcp/`), the Logos Kit `names.rs`, and the LEZ wallet
  PR's `names.rs`.

## Payment: deposits now, fees possible

- **Deposit mode (on now).** Registering locks a deposit in the registry program,
  and the name is held for as long as the deposit stays locked: there are no
  terms, renewals, expiry or grace period. Releasing the name returns the deposit
  to the holder and frees the name. A private holder's refund goes to their private account
  in the same private transaction. Nobody profits; tying up tokens is what stops
  squatting. This follows Status's username registrar (10 SNT, refundable).
  Consequence: a holder who loses their keys leaves the name and its deposit
  locked for good.
- **Fee mode (kept in the code, off).** The present fee-to-treasury path stays,
  with its terms, renewals and expiry, selected by a setting the admin can switch. It is isolated so it can be removed
  entirely.
- The admin (the maintainer for now) can change amounts and the mode, and can be
  locked later (admin set to an account nobody controls) or handed to the Logos
  Foundation. The code is MIT OR Apache-2.0.

## Name lengths

The chain never sees a name, so length rules are enforced with a fixed list: every
name of 1 to 3 characters (49,284 names) has its ID computed once and committed to
as a sorted Merkle tree whose root is built into the program. The list (about
1.6 MB) ships inside the CLI. A registration proves its ID is on the list (and
which length) or that it is not (two adjacent leaves, with sentinels at both
ends).

| Length | Rule |
|---|---|
| 1-2 characters (1,332 names) | Locked. Only the admin can release one, to an owner tag the recipient supplies. |
| 3 characters (47,952) | Higher deposit. |
| 4 or more | Normal deposit. |

Short names are guessable in seconds anyway, so the list reveals nothing new.
(4-character names were considered for a middle tier; it would have meant a 1.8M-ID,
~28 MB list for every client, so it was dropped.)

## Deposit amounts (testnet)

- Normal names: **100,000,000**. Three-character names: **2,000,000,000**.
- Both are admin settings, editable later. A change applies to new registrations
  only: each name's deposit sits in its own account (a PDA per name) and the holder
  gets back exactly what was locked.
- Mainnet amounts are set at mainnet launch, once the token's value is known.
- The admin places names without a deposit (nothing to refund on release): a locked name it releases, or a built-in one. `jef.lez` is placed this way at deploy: the admin registers it and transfers it to jef's private holder.
- On a transfer the deposit stays with the name: whoever holds it when it is
  released gets it back.
- Known gap: the commit step is free for private users, so commitment accounts can
  be spammed. That claims no names; not addressed in v5.

## Testnet to mainnet

- **Snapshot** of every testnet v5 name (ID, owner tag) before mainnet, committed
  as a Merkle root in the mainnet program.
- **Claim with the same keys.** The holder signs from the same account (a private
  holder inside a private proof); the program checks that `owner_tag(id, signer)`
  is in the snapshot. To verify before building: LEZ derives the same account from
  the same recovery phrase on testnet and mainnet.
- **A real mainnet deposit is required to claim.** Testnet tokens are free, so
  this is what keeps testnet squatting from carrying over.
- **30-day claim window** after mainnet launch; unclaimed names then open to
  anyone.
- No registration cutoff date on testnet.

## Order of work

1. The CLI and website changes already shipped (static site, `names open`,
   reveal retries, signed releases).
2. v5 program: deposits and refunds (fee mode, with terms and expiry, behind a
   setting), the length list and its proofs. Unit tests for each.
3. v5 derivation (Argon2id, shared salt) in every resolver, with new vectors.
4. Compute the short-name list; deploy v5 on the testnet; re-register `jef`
   and `logos`.
5. Snapshot and claim design for mainnet, built when mainnet is near.
