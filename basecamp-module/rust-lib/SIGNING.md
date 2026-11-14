# Signing a call another account pays for (`propose_relayed_call`)

For the reviewer of the Names Wallet's one new signing path in 0.17.0. The code is
`src/provider_public.rs` and `approve_public` / `propose_relayed_call` in `src/lib.rs` (module),
and `cli/src/public_call.rs` (names CLI, which holds the keys and signs). Each statement names
the code that makes it true and the test that checks it. LEZ v0.4.0-rc3 (`bafd07be`).

## 1. What bytes the wallet signs, and where the sheet comes from

- The app hands the module a structured call: root program, message bytes (hex), declared
  actors, and a fee declaration `{payer, gas_limit, tip, max_fee}`. It can't hand over a
  transaction, a nonce or a signature: there is no field for one
  (`provider_public::relayed_call_request` refuses unknown fields; the CLI's `CallInput` has
  none).
- The names CLI builds the one transaction itself (`public_call.rs`, `CallInput::build` and
  `Built::message`). It is `lee::public_transaction::Message::new(root, bytes, actors, nonces,
  Some(fee), evidence)`, where:
  - `root` is the signing account's actor of the program;
  - `actors` are the root plus the declared ones;
  - `nonces` are the signing account's and the payer's current nonces, read from the chain by
    the CLI, never from the app;
  - `evidence` is a PDA admission only for entries marked `new`.
- The signature is `Signature::new(key, message.hash())`. `message.hash()` is SHA-256 over the
  domain `/LEE/v0.3/Message/Public/` and the message's borsh bytes (`lee`
  `transaction_message.rs` `hash_under`).
- **The sheet comes from that build, not from the app's words.** `module public-call plan`
  prints the build's own fields: the hash, the account, program and payer, the gas limit, tip
  and max fee, the nonces, the actors, the accounts it creates, the message's length and SHA-256
  (`Built::report`).
  - `provider_public::sheet` takes every line it shows from that report.
  - It refuses the request if any field of the report differs from the request.
  - The app's own text appears once, labelled "its own words, not checked", cleaned to one plain
    line.
  - Tests: `the_sheet_says_what_the_wallet_built_in_plain_words` and
    `a_build_that_differs_from_the_request_is_refused_never_shown_as_the_request` (9 mutated
    fields, each refused).

## 2. What the wallet refuses

| Refused | Where | Test |
|---|---|---|
| Anything but one public call (no raw transaction, no second root, no private account) | `relayed_call_request` (fields), `CallInput::build` (one `RootCall`) | `a_relayed_call_is_built_from_declared_programs_only`, `what_the_wallet_never_signs_is_refused` |
| A fee payer equal to the signing account | both | same two |
| A fee payer that is any account of the person's wallet | CLI `public_call_message` (the wallet holds its key) | e2e step "plan refuses a payer the wallet holds" |
| An actor of the payer's account in the call (the payer signs too, so a program would be authorized on it) | both | same two |
| A signing account that isn't the wallet's own | CLI (`get_account_public_signing_key` is none) and module (must be an account given to this app, a public one) | e2e |
| Nonces from the app | not possible: the CLI reads both | by construction |
| A program the app didn't declare and the person didn't approve (`declare_programs`) | `relayed_call_request`, again at approval | `a_relayed_call_is_built_from_declared_programs_only`, module flow test |
| LEZ 0.3 | module (`self.net`) | module flow test |
| Another app's request, or this one approved twice | provider store | module flow test |

## 3. Programs the wallet can't decode

The wallet decodes no program's message bytes in this path. Every program is shown as `unknown
program <id>`, with what the Program Registry says of it (or that it isn't there, or that the
registry isn't installed). The first warning on the sheet says the wallet can't decode what the
call tells the program to do and shows only the exact bytes' hash. Warnings need an extra tick
before Approve. Nothing is hidden, and nothing is refused silently for being undecodable. Test:
`the_sheet_says_what_the_wallet_built_in_plain_words` (line and warning).

## 4. What the half-signed transaction is good for

The account's signature covers the hash of the whole message:
- the declared actors and the accounts that sign;
- the root call: the program actor and every message byte;
- the fee declaration, with the payer, gas limit, tip and max fee;
- the nonces of both the account and the payer;
- the admission evidence.

So:
- **It is good for exactly that message, once.** Another payer, other caps, another nonce, or
  any other byte or actor gives another hash, and the account's signature no longer verifies.
  The account's nonce moves when the call lands, so the same transaction can't land twice.
- **The payer and the fee fields ARE covered.** No finding there.
- **rc3 public transactions carry no validity window.** An unused half-signed transaction stays
  valid until the account's next transaction. The request itself expires after 10 minutes, like
  every request.

Tests:
- `public_call::tests::a_half_signed_call_verifies_and_binds_its_payer` (vector
  `cli/tests/vectors/public-call-v1.json`): the half-signed transaction's signature verifies
  but the transaction isn't valid until the payer co-signs. Co-signed, its signers are exactly
  the account and the payer. Another payer, a max fee one higher, or the account's next nonce
  each break the signature.
- e2e `relayed-e2e.sh` on a throwaway rc3 chain (`docs/wallet/e2e-rc3-relayed-call.log`, 12 PASS,
  0 FAIL): the co-signed call applies once (block 11). Submitted again, it never lands: not in a
  block after 120 s, because its nonce is spent.

## 5. An app can't get a signature for a message other than the one shown

- **Request fields.** A request's fields are kept by the wallet (`provider.json`, mode 0600).
  The app can't change them after proposing.
- **At approval the module re-checks, in `approve_public`:**
  - it rebuilds the sheet and refuses a sheet that is refused;
  - it hashes the kept message bytes again and refuses unless they match the build's SHA-256;
  - it hands the CLI the call with `expect_hash`, the hash shown.
- **The CLI rebuilds and compares.** `public_call_sign` rebuilds the transaction from the chain's
  current nonces and signs only if its hash equals `expect_hash`. A message, actor, fee or
  payer changed in between, or an account or payer used in between, gives another hash and
  nothing is signed.

Tests:
- module test `a_relayed_call_is_built_shown_signed_and_handed_back_never_sent`: the kept message
  is changed after the sheet was shown; approval is refused and the CLI is never asked to sign.
  Then, put back, it signs once, with the shown hash.
- e2e step "sign refuses a hash that isn't the approved one", with the real CLI.

## 6. Signing the same call again when the payer's nonce moved

The relay may answer "busy, rebuild" when its payer has another transaction in flight. The
half-signed transaction names the payer's nonce, so it is dead once that nonce is used. **Before
0.17's `resign_relayed_call`, an app could not get the same call signed again without the person
approving a second time:** the request ends `done`, approving it again is refused, and a new
`propose_relayed_call` makes a new request and a new sheet.

`resign_relayed_call(request_id)` is the one exception, and it is narrow:
- **Who:** only the app that made the request, only a relayed call the person approved and the
  wallet signed, only on LEZ 0.4, and only while the app still has the program declared.
- **Limits:** within 10 minutes of the approval, at most 3 times (`RESIGN_WINDOW_MS`,
  `MAX_RESIGNS`).
- **What is compared:**
  - The kept bytes are hashed again against what was approved.
  - The CLI builds the call again from the chain's nonces.
  - `provider_public::resign_check` compares that build with the build the person approved
    (`approved_report`). Every field the sheet showed must be the same: the account, program,
    payer, gas limit, tip, max fee, actors, the accounts it creates, whether it reaches the
    account's LEZ balance, the message's length and SHA-256.
  - The set of signers must be the same, and so must the signing account's nonce. If that nonce
    moved, the approved call may have landed, so this is refused.
  - The payer's nonce must be newer than the last one signed.
  - Only then does the CLI sign the new hash, and it rebuilds and compares the hash once more as
    in §5.
- **Anything else needs a new approval** ("Propose it again"). That includes an unchanged payer
  nonce: the transaction already given is still the one to send.

Tests:
- `provider_public::tests::a_re_sign_is_allowed_only_for_a_newer_payer_nonce`: each of the 11
  fields changed is refused; so are the account's nonce moved, the payer's nonce unchanged or
  older, a third signer, and an unchanged hash.
- The module flow test `a_relayed_call_is_built_shown_signed_and_handed_back_never_sent`:
  - another app is refused;
  - an unchanged nonce is refused;
  - the max fee, the payer or the bytes changed are each refused;
  - the account's nonce moved is refused;
  - a newer payer nonce is signed again, 3 times, then refused;
  - past 10 minutes it is refused.

## What the wallet does not do

It sends nothing and pays nothing. It never talks to the relay and never reads Sepolia. It can't
see or limit what the app or the relay do with the signed transaction, beyond the fact that only
exactly these bytes can be submitted, once. The sheet says so.
