# Testimonial program (LEZ 0.4)

The on-chain half of the LP-0021 **Testimonial mini-app**: "a standard LEZ program that
stores each testimonial on-chain", each with custom text, an optional username and the unique
identifier of the λPrize submission it vouches for. It is built so an evaluator can check the
**adoption** criteria (150 testimonials from 150 distinct accounts, over 2+ months with 30+ a
month, each carrying the submission's identifier and an on-chain timestamp) **from chain data
alone, with plain sequencer reads**: no indexer, no server of ours, no trust in our client.

| Path | What |
|---|---|
| `core/` | All program logic (`testimonial_core`): records, PDAs, the handler, the list walk and the adoption summary. Host tests. |
| `methods/`, `methods/guest/` | The risc0 guest (`logos_testimonial`), one line: `actor_entrypoint!(testimonial_core::handle)`. |
| `engine-test/` | The real guest ELF on LEZ v0.4.0-rc1's real state machine (`V03State`). `forwarder/` is a test-only guest that relays messages, to prove relayed posts are refused. `testimonial-imageid` writes the ELF and prints its image ID. |
| `client/` | `testimonial`: `post`, `list` (with per-month counts and the adoption check), `show`, `address`. |

## Design

### Accounts

`TST` is the program's account ID (its loader header). Every address below is a PDA of `TST`
by SPEL's `compute_pda` rule: each seed component is 32 bytes (strings zero-padded, account IDs
and `[u8; 32]` raw), several are combined as `sha256(c1 ‖ c2 ‖ …)`, and the address is
`AccountId::for_public_pda(TST, seed)`. There are no integer seeds (logos-co/spel#300).

| Account | Address (SPEL notation) | Holds |
|---|---|---|
| Submission head | `pda[const "testimonial/v1/head", arg sha256(submission)]` | `Head` |
| Testimonial | `pda[const "testimonial/v1/node", arg sha256(submission), account author]` | `Testimonial` |

The submission identifier enters the seeds as its sha256, so identifiers can be longer than
one 32-byte component; the identifier itself is stored in both records.

There is no admin, no settings account, no fee to us and no moderation. Nothing can be edited
or deleted: a testimonial's text and time are fixed once written.

### Records (borsh)

```rust
struct Head {
    schema: u8,              // 1
    submission: String,      // the identifier, as given
    count: u64,              // testimonials = distinct author accounts
    newest: Option<AccountId>,
    first_at: u64,           // ms; `at` of the first and of the latest testimonial
    last_at: u64,
}
struct Testimonial {
    schema: u8,              // 1
    submission: String,
    author: AccountId,       // the account that signed it
    seq: u64,                // position in the submission's list, from 0
    at: u64,                 // ms since the epoch, bound to the block's time (below)
    username: Option<String>,// display name, NOT proven
    text: String,
    prev: Option<AccountId>, // the testimonial before this one
}
```

Worst case (every field at its limit): `Testimonial` 1,215 bytes, `Head` 126 bytes (pinned by
`worst_case_record_sizes`).

### Limits (checked on chain)

| Field | Rule |
|---|---|
| submission | 1-64 bytes of `a-z 0-9 . _ : / -`, starting with a letter or digit. Lowercase only, so look-alike identifiers can't split one submission's list. Exact match. |
| username | optional; 1-32 bytes UTF-8, no control characters, no space at either end |
| text | 1-1024 bytes UTF-8, not only whitespace, no control characters except `\n` and `\t` |
| per account | one testimonial per (submission, author account) |

### The instruction and what it does

A transaction sends `Message::Instruction(Post { submission, username, text, now })` to the
**author's own actor** `(author, TST)` and declares two more actors: the submission head and the
author's testimonial account (`actors_to_declare`). No admission evidence is needed: the
program sends each effect with the PDA's seed (`SyncMessage::with_pda_seeds`), so it admits
its own accounts.

1. **Root (`(author, TST)`)**: requires `from == None` (the transaction's own root message) and
   `is_authorized` (the author signed). Validates the fields, takes the author from the
   receiver's account ID (it is never an argument), bounds the transaction's timestamp to
   `[now, now + 10 min)`, and sends `Effect::Index` to the head.
2. **Head**: accepts it only from the author's own actor; numbers the testimonial
   (`seq = count`), links it after the current newest (`prev = newest`), stores
   `count + 1` and the new newest, and sends `Effect::Write` to the testimonial's account.
3. **Testimonial account**: accepts it only from the head, only at the address derived from
   (submission, author), and only if empty. A second post by the same account fails here and
   the whole transaction is refused. Emits `testimonial::Posted { submission, author, seq, at }`
   (selector `sha256("testimonial::Posted")[..8]`) for anyone who does run an indexer.

Effects are accepted only from this program's own actors (`from_own_program`), so neither a
transaction nor another program can write a head or a testimonial directly.

**Why root-only.** In LEZ 0.4 a signature authorizes the signer in every program a transaction
reaches. Without the `from == None` check, a program the author called for something else could
relay a `Post` and publish a testimonial in their name. The engine test `t05` shows the relay
being refused.

**Concurrency.** Every address a post touches is a function of its own arguments; the counter
and the link are read at execution time by the head actor, not predicted by the client. Posts in
the same block form one list (engine test `t02`).

### Time

A LEZ 0.4 program cannot read the block's time, so the client passes `now` (it reads the
chain's clock account) and the program returns a timestamp validity window `[now, now + 10 min)`.
The sequencer applies a transaction only in a block whose timestamp is inside every window
(`validated_state_diff/mod.rs`, `OutOfValidityWindow`). So the stored `at` is the author's
claim, **provably no later than the block's time and at most 10 minutes before it**. A
testimonial can't be backdated by more than 10 minutes or dated in the future. Per-month counts
use `at` in UTC; only a post in the last 10 minutes of a month can fall in the month before its
block's.

### Private authors: refused with the stock wallet, on purpose

Tried live (see Status). A private account can run the root step inside a proof: it is
authorized at `(author, TST)` and the root sees its real ID. But when a private actor sends a
message to a public actor, LEZ 0.4 shows the receiver the address the sender *presents*
(`SenderPresentation`, `execution_state.rs`), and the rc1 wallet presents every private sender
under a **fresh random alias per message** (`AccountManager::presenter`). The head requires the
sender to be the author's own ID, so the post is refused:

```
Guest panicked: index: only the author's own actor indexes a testimonial
```

(now reworded to name the alias). This is the behaviour we want: if the head accepted the
alias as the author, one private account could post again and again as "new" accounts and the
"150 distinct accounts" count would mean nothing. A wallet that presents a private sender
canonically would pass, but then the private account's ID is published next to the
testimonial, and evaluators would still see no activity history for it. Public authors are
the supported path; the client refuses a private `--author` up front rather than spend minutes
proving a post that will be refused (`TESTIMONIAL_TRY_PRIVATE=1` tries anyway).

## Verifying the adoption criteria from chain data

Everything below uses only the sequencer's `getAccountView` (and optionally `getAccount`),
which every LEZ 0.4 sequencer serves.

1. Derive the head from the program ID and the submission identifier:
   `H = for_public_pda(TST, sha256(pad32("testimonial/v1/head") ‖ sha256(submission)))`
   (`testimonial address --submission <id>` prints it).
2. `getAccountView({account_id: H, program_account_id: TST})` → `Head`. `count` is both the
   number of testimonials and the number of distinct author accounts.
3. Walk `newest`, then each `prev`, with `getAccountView` on each (`count` reads). For each
   record check: it decodes as a `Testimonial` with `schema = 1` and this `submission`; its
   address equals `pda(TST, submission, author)`, which makes each author appear once; and
   `seq` runs from `count - 1` down to 0 with the list ending at `None`. `walk` in
   `core/src/lib.rs` does exactly this, and the client prints the result.
4. Group `at` by UTC calendar month: the per-month counts. The bar (`summarize`): at least 150
   testimonials from 150 distinct accounts, in at least 2 months with 30 or more each.
5. Optional: `getAccount(author)` gives each author's current nonce, a hint of activity beyond
   the testimonial (`testimonial list --nonces`). Judging prior activity properly needs the
   author's transaction history, which only an indexer (`getTransactionsByAccount`) or a block
   scan provides.

Trust assumptions: the program's code (its image ID is in the loader header at `TST`; rebuild
from this source to compare), and the sequencer's answers. The submission identifier is a
plain string anyone can post under; the program proves *who* posted, not that they are real
users, so manufactured accounts remain the evaluators' call.

```sh
testimonial --rpc <sequencer> --program <TST> list --submission lp-0021/names-wallet
testimonial --rpc <sequencer> --program <TST> list --submission lp-0021/names-wallet --json --nonces
```

## Build and test

Guests build with the local RISC Zero toolchain (`rzup`, risc0 3.0.5), no Docker. On the Mac
mini: `export PATH=$HOME/.cargo/bin:$HOME/.risc0/bin:/opt/homebrew/bin:$PATH`.

```sh
cargo test -p testimonial_core                                   # host logic, fast
cargo test --release -p testimonial_engine_test -- --nocapture   # real guest, real rc1 engine
cargo build --release -p testimonial_client                      # the `testimonial` binary
cargo run --release -p testimonial_engine_test --bin testimonial-imageid -- testimonial.bin
```

## Deploy

Deploy the guest through the LEZ program loader with any 0.4 wallet, e.g. the `wallet` CLI or
`names wallet`: create a header account and enough segment accounts, then

```sh
wallet program-loader deploy --elf testimonial.bin --header <new account> --segments <new accounts…> --payer <funded account>
```

The header account is `TST`. The program has no configuration step: the first post to a
submission creates its head. Keep the header key safe; it is the upgrade key, and upgrading
must keep reading `schema = 1` records. Consider deploying with `--immutable` once settled.

## Cost

| Path | Kind | Worst case at 500 req/s |
|---|---|---|
| Posting | an on-chain transaction the author pays for | $0 to us |
| Listing / verifying | the reader's own sequencer RPC calls | $0 to us |

No Worker, server or database of ours is involved.

## Status (2026-11-03)

- **Host tests:** `cargo test -p testimonial_core`: 11 pass (handler, guards, limits, sizes, the
  list walk against tampered state, months and the adoption bar over 150 posts).
- **Engine tests** on LEZ v0.4.0-rc1 (real guest ELF, real signatures, no admission evidence):
  8 pass. A post costs about 54,600 cycles and 533 transaction bytes; a worst-case post (every
  field at its limit) 98,709 cycles and 1,604 bytes. `t08` posts 150 testimonials from 150
  accounts over two months and the client's own `walk` + `summarize` read them back as meeting
  the bar.
- **Live, on a throwaway local rc1 sequencer** (stopped afterwards): deployed through the loader
  with `lezreg wallet program-loader deploy` (4 segments; the final image, a 356,828-byte ELF,
  has image ID `69d838aa…fd44`), posted from two public accounts with `testimonial post`, the
  duplicate refused before sending, a second submission kept separate, and `testimonial list`
  (plain `getAccountView` reads, 5 for two testimonials) printed the list, per-month counts and
  author nonces. A post simulated at about 56,400 cycles (gas limit 84,585).
- **Private author:** tried on the first image (`e4fceabf…8f97`, identical but for the panic
  text): shielding took about 11 minutes of proving, then the post was refused in the proof's
  simulation with the alias error above. The client now refuses a private author up front.
- Not yet: deploy on testnet 0.4 (not launched); the Basecamp mini-app on the Wallet Provider
  SDK, which builds the same `Post` and declares the same three actors.
