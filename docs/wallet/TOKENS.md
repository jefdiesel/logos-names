# Names Wallet: token-program assets on LEZ 0.4

Part of the LP-0021 plan (`docs/wallet/PLAN.md`, row "Token program"). Researched 2026-11-03
against LEZ `v0.4.0-rc1` (commit 1806e873, read at `~/logos/upstream/lez-0.4-rc1`) and
lez-programs `main` at 6674d78 (2026-10-28). Paths below are relative to those two trees
unless they start with a repo name.

## The short answer

1. **No token program runs on LEZ 0.4 yet.** lez-programs has no branch, tag or open PR
   that targets 0.4, and LEZ 0.4 itself ships none. The reference program is written for
   the 0.2.x account model, which 0.4 removed. Porting it is a rewrite of every handler, not
   a dependency bump.
2. **That blocks the prize criterion as written.** LP-0021 lists "Designing or implementing
   the token program itself" as out of scope: "the wallet integrates with the
   existing/reference implementation" (LP-0021.md, Scope, Out of Scope). Until Logos
   publishes a 0.4 token program, there is nothing to integrate with, and a token program
   of our own would not count.
3. **The wallet side is not blocked.** A 0.4 token program shaped like LEZ's own native
   token lets the stock `wallet` crate move it publicly and privately with the code it
   already uses for native transfers. `wallet-tokens/` in this branch is a working
   prototype of that program and its client (results below), so Names can build the
   screens and commands now, behind an interface, and point them at the reference program
   when it lands.

## 1. Which lez-programs version targets LEZ 0.4

None.

- `main` (6674d78, 2026-10-28) pins `lee_core`/`lee` to LEZ `v0.2.5-rc2`
  (`Cargo.toml:43-44`); the token guest pins `nssa_core` to `v0.2.5-rc2` and SPEL
  `spel-framework` rev c6f73ce (`programs/token/methods/guest/Cargo.toml:59-60`).
- Tags: `v0.1.0`, `v1.0.0`, `v2.0.0`, `v2.0.1`. `v2.0.1` (2026-09-09) pins LEZ `v0.2.4`;
  `DEPLOYMENTS.md` lists only "Testnet legacy (v0.2.4)" (`DEPLOYMENTS.md:5`), built from
  `v2.0.1` (`DEPLOYMENTS.md:12`).
- All 231 branches, scanned for the LEZ pin in their `Cargo.toml` on 2026-11-03: 1 on
  `v0.3.0` (`chore/upgrade-to-lez-0.3.0-rc`, last commit 2026-10-12 "WIP: LEZ v0.3.0
  upgrade"), the rest on `v0.2.x`, `v0.2.0-rc*` or an older rev. None on 0.4.
  (`chore/lez-core-0.4.3` is about the Basecamp `lez_core` module 0.4.3, and pins LEZ
  `v0.2.4`.)
- No open PR in lez-programs mentions 0.4. SPEL (`logos-co/spel`), which the token program
  is written in, has no 0.4 branch either: `main` was last updated 2026-10-13 for LEZ
  `v0.2.5-rc2` compatibility.
- LEZ 0.4 itself ships no general token program. `lez/programs/` holds bridge,
  bridge_lock, clock, cross_zone_*, fee, ping_*, program_loader, sequencer_stake and
  `wrapped_token`. `wrapped_token` is the bridge's destination token: only the cross-zone
  inbox may mint (`lez/programs/wrapped_token/core/src/lib.rs:1-4`, `src/main.rs:22-24`)
  and it has no transfer message at all (`core/src/lib.rs:33-86`).

### Why the reference program does not port by a version bump

The reference is written against the 0.2.x model, where a program receives the accounts
of a transaction with their whole state and returns state diffs and chained calls:

- `#[lez_program]` handlers take `AccountWithMetadata` for every account and return
  `AccountStateDiff`s (`programs/token/methods/guest/src/bin/token.rs:14-34`,
  `programs/token/src/transfer.rs:7-21,109-116`).
- A transfer reads the **recipient's** data to check its definition
  (`programs/token/src/transfer.rs:17-27`); ownership is `program_owner`
  (`programs/ata/src/transfer.rs:16-19`); the ATA program moves tokens with a
  `ChainedCall` carrying PDA seeds (`programs/ata/src/transfer.rs:67-74`).

LEZ 0.4 has none of these. A program is an actor handler: it receives one message for one
actor (an account paired with the program) and sees only that actor's own state
(`lee/state_machine/core/src/program/mod.rs:373-385`), and it answers with a new state for
that actor plus sync and async messages to other actors (`program/mod.rs:428-438`). Every
handler of the token program has to be re-cut into messages between actors. The
`spel04-shim` in `~/logos/lez-program-registry` keeps SPEL's attribute syntax on 0.4, but
it states the same limit: "Only the root actor carries `pre_state` and `is_authorized`: a
0.4 guest cannot see any other actor's state" (`spel04-shim/src/lib.rs`, doc of
`AccountWithMetadata`), so it helps with the IDL, not with the logic.

What a port takes: about the size of LEZ's `native_token.rs` (317 lines with tests) plus a
definition and mint authority, i.e. what `wallet-tokens/core` is (one file, 20 tests).
The real cost is agreeing on the account model (next section), because wallets, the AMM
and the ATA program all depend on it.

## 2. The LEZ 0.4 account model a token program lives in

- An account is a nonce and a map from program to that program's state on the account
  (`lee/state_machine/core/src/account.rs:95-98,131-133`). The native balance is just the
  state of the native token program, `NATIVE_TOKEN_PROGRAM_ID = [0; 32]`
  (`native_token.rs:9`, `account.rs:156-158`).
- An **actor** is `(account_id, program_account_id)` (`account.rs:194-197`). One account
  has one state per program, so a token program has exactly one slot per account.
- A program lives at whatever account its deployer chose; the same bytecode deployed twice
  is two programs (`program/mod.rs:510-524`). Deploy is segments plus a header through
  the program loader (`lez/wallet/src/program_facades/program_loader.rs:125-136`; segment
  size 96 KiB, `lez/programs/program_loader/core/src/lib.rs:24`).
- `ReceiveInput.is_authorized` is true when the receiver's account signed or its owning
  program authorized it with a PDA seed (`program/mod.rs:379-380`); `from` is `None` for the
  transaction's root message (`program/mod.rs:376-378`); `from_own_program()` tells whether
  the sender is an actor of the same program (`program/mod.rs:388-393`).
- PDAs: `for_public_pda(program, seed)` (`program/mod.rs:152-167`); private PDAs also bind
  the owner's npk/vpk (`program/mod.rs:210-231`).
- A public account a transaction reaches for the first time must be admitted: by a PDA
  seed in the message that reaches it, or by `PublicAccountEvidence::Key`/`Pda` in the
  transaction (`lee/state_machine/src/transaction_message.rs:16-19`); otherwise execution
  fails with `UnadmittedPublicActor` (`lee/state_machine/core/src/execution_state.rs:1190-1210`).

### The native token is the template

`native_token.rs` is a complete fungible token in this model:

- message `Transfer { to, amount }` / `Credit(amount)` / `ReadState` (`native_token.rs:12-16`);
- `Transfer` is sent by the transaction to the sender's own actor, which must be authorized
  (`native_token.rs:90-99`), debits it, and sends the credit as an **async** message to the
  recipient's actor (`native_token.rs:68-70`);
- `Credit` is accepted only from the same program (`native_token.rs:71-74`);
- zero is empty state (`native_token.rs:41-47`).

The async credit is what makes privacy work: a public recipient declared in the
transaction is credited in the same transaction; a private recipient gets a pending
message it receives later (`lez/wallet/src/program_facades/mod.rs:25-47`).

### Prototype token program (`wallet-tokens/core/src/lib.rs`)

The prototype ports the reference's fungible semantics into that shape:

| | reference (0.2.x) | prototype (0.4) |
|---|---|---|
| definition | own account, `TokenDefinition::Fungible { name, total_supply, metadata_id, authority }` (`programs/token/core/src/lib.rs:143-162`) | same enum and fields, as the token program's state on a fresh account that signs its creation |
| holding | one account per token, `TokenHolding::Fungible { definition_id, balance }` (`core/src/lib.rs:186-201`), found by `program_owner` | the holder's own actor state: `TokenState::Holdings(BTreeMap<definition, u128>)` |
| transfer | `Transfer { amount_to_transfer }` over (sender holding, recipient holding) (`core/src/lib.rs:34`) | `Transfer { definition, to, amount }` to the sender's actor; async `Credit { definition, amount }` to `to` |
| mint | `Mint` (definition signs) or `MintWithAuthority` (authority signs) (`core/src/lib.rs:86,97`) | `Mint { definition, to, amount }` to the authority's actor, forwarded as sync `MintFrom` to the definition (the `wrapped_token` pattern, `lez/programs/wrapped_token/src/main.rs:193-197`); or to the definition when it is its own authority |
| burn, set authority | `Burn`, `SetAuthority*` | `Burn` (holder debit + sync `BurnFrom` to the definition), `SetAuthority` (like mint) |
| ATA | `compute_ata_seed(token_program, owner, definition)`, ATA = `for_public_pda(ata_program, seed)` (`programs/ata/core/src/lib.rs:66-88`) | not needed: the holder's actor *is* the deterministic per-(owner, token program) location; all definitions share it |

Wire format: Borsh, tag byte = variant index, append-only: `Transfer` 0,
`NewFungibleDefinition` 1, `Mint` 2, `Burn` 3, `SetAuthority` 4, `Credit` 5, `MintFrom` 6,
`BurnFrom` 7, `SetAuthorityFrom` 8. User messages are root-only (`from == None`) and need
`is_authorized`; the four internal ones are accepted only from the program's own actors.

Root-only on purpose: an actor reached from another program has `is_authorized` from
whatever its signer signed for that program, not for this call (the same "ambient
authority" `wrapped_token` closes, `lez/programs/wrapped_token/core/src/lib.rs:98-103`). The
native token does accept such calls (`native_token.rs:203-223`). The reference port will
have to decide this for composability (AMM vaults); the prototype refuses.

Trade-offs of the holdings-map design, for whoever does the reference port:

- Good: one read lists everything an account holds; private balances live in the private
  note, exactly like the native balance; the wallet's native transfer path works unchanged.
- Bad: anyone can credit dust of a new token to anyone, growing their state. The prototype
  caps a holder at 32 kinds (`MAX_HOLDINGS`), which bounds the state but lets a spammer
  fill a victim's slots. The reference avoids this by making holdings separate accounts
  (and LP-0014 ATAs). An opt-in step ("open a holding") or per-(owner, definition) PDAs
  would fix it at the cost of an extra transaction.
- Private notes are padded to 512 bytes, leaving 427 for actor states
  (`lez/wallet/src/account_manager.rs:30-35`); about seven token kinds fit before a private
  account's note becomes recognizable by length (the wallet warns, `lez/wallet/src/lib.rs:1551-1556`).

## 3. How a wallet lists a user's holdings

- Public account: one read, `get_account_view(Actor::new(account, token_program))`, then
  `data.actor_state(token_program)` (`lez/wallet/src/program_facades/mod.rs:49-63`; the
  facade helper is `pub(crate)`, so call `WalletCore::get_account_view` directly,
  `lez/wallet/src/lib.rs:957`).
- Private account: `wallet.private_account_state(id)` / `get_account_private(id)` after
  `sync_to_latest_block()` (`lez/wallet/src/program_facades/mod.rs:64-70`,
  `lez/wallet/src/lib.rs:1136-1145`).
- Then decode `TokenState` and list `(definition, balance)`; read each definition's state
  for its name and supply.

No token list or index is needed: the wallet already knows its accounts. For contrast, the
reference's AMM module lists holdings by scanning the wallet's accounts for ones whose
`program_owner` is the token program (`modules/amm/ffi/src/api/token_holdings.rs:1-22`,
`modules/amm/ffi/src/api/holding.rs:13-21`), and LP-0014 ATAs need the mint list to derive
addresses.

Pending private credits are separate: `owned_pending_messages()` returns messages one of
this wallet's keys opens (`lez/wallet/src/lib.rs:985-988`); filter on
`body.to.program_account_id == token_program` to show "incoming, not yet received".

## 4. Building and signing transfers with the 0.4 `wallet` crate

Everything goes through `WalletCore::send_tx_with_pre_check(accounts, root, message,
programs, cross_messages, async_delivery, fee_payer, pre_check)`
(`lez/wallet/src/lib.rs:1286-1323`). It sends a plain public transaction when every
account is public and nothing is pending; otherwise it proves the transaction locally and
sends a privacy-preserving one (`lib.rs:1302-1322`).

The native facade is the recipe (`lez/wallet/src/program_facades/native_token_transfer/mod.rs:18-61`):

1. `accounts[0]` = the sender's actor under the program, authorized:
   `sender.select_program_actor_state(program)` (`account_manager.rs:215-223`).
2. The recipient, by `credit_destination` (`program_facades/mod.rs:25-47`, `pub(crate)`,
   so Names copies its 20 lines):
   - public, or a private account this wallet owns: declare its actor, unauthorized
     (`AccountMention::receiving`, `account_manager.rs:255-261`), so the async credit runs in
     the same transaction;
   - someone else's private account (`AccountIdentity::PrivateForeign { npk, vpk, kind }`,
     `account_manager.rs:56-60`): not declared; `seal_destination` (private sender) or
     `async_destination` (public sender) adds the recipient material, and the credit is
     published as a pending message (`lib.rs:1350-1395`), with `promotions.listed_only = true`.
   - a public account never used yet: `AccountIdentity::PublicForeign(public_key)`, which
     supplies the admission evidence (the stock CLI's `--to-pk`,
     `lez/wallet/src/cli/programs/native_token_transfer.rs:38-40`).
3. `programs`: a `ProgramCatalog` holding the token program, needed to prove private
   execution; `ProgramLoader(&wallet).program(id)` reads the deployed bytecode
   (`program_facades/program_loader.rs:229`). The native program is protocol code and
   needs none (`lib.rs:2271-2273`).
4. `message` = `borsh(Transfer { definition, to, amount })`.

Signing is the wallet's: public signers sign the message hash with their keys; private
accounts are authorized inside the proof (`lib.rs:1783-1799`). A private transaction pays
its fee in-proof from a private account: `fee_payer` if given, else the wallet's choice
(`lib.rs:1449-1454,1513-1515`, `account_manager.rs:755-777`), so every private token send
needs a private account holding native tokens.

### Receiving a private credit

`receive_pending_message_with(pending, accounts, programs, ..)` executes the published
credit as a proven transaction; it loads every declared program it lacks from the chain
(`lez/wallet/src/lib.rs:2211-2310`, programs at `2271-2288`). The stock CLI already does
this generically: `wallet pending list` / `wallet pending receive --position N`
(`lez/wallet/src/cli/pending.rs:21-34`).

Unlike native credits, token credits are never swept automatically into a later private
transaction: the sweep only picks messages whose `native_credit()` is `Some`
(`lez/wallet/src/lib.rs:139-148,1941-1952`). Names needs an explicit "receive" step (or
auto-receive on sync) for tokens.

## 5. What ran (prototype)

Standalone crate `wallet-tokens/` (own `[workspace]`): `core` (program logic, host tests),
`methods` (guest built with `risc0_build::embed_methods`), `cli` (`wallet-tokens`, on the
stock `wallet` crate, tag `v0.4.0-rc1`). Built and run on the Mac mini under
`/Volumes/exos162/logos/dev/wallet-tokens-agent`, against a throwaway standalone rc1
sequencer on 127.0.0.1:3180 (stopped afterwards). The CLI refuses any non-loopback
sequencer, because the wallet's built-in default is the public testnet
(`lez/wallet/src/config.rs:62-64`).

RUN-RESULTS

## 6. Recommended integration for Names

Keep tokens behind one Rust module in the CLI (`cli/src/tokens.rs`) that takes the token
program's ID and a small codec trait (encode transfer/mint, decode holdings and
definition). The prototype's codec ships first; the reference 0.4 program's replaces it
when it exists. The wallet plumbing (accounts, credit destination, proving, finishing,
receiving) does not change between them as long as the reference keeps the native-token
shape; if it keeps per-token holding accounts instead, only the "where is the balance" and
"which accounts to declare" parts change.

CLI (all accept `--program`, defaulting per network like `DEFAULT_PROGRAM`):

| command | does |
|---|---|
| `names token list [--account A]` | holdings of every wallet account (public from chain, private from notes after a sync) with token names; pending incoming private credits listed separately |
| `names token info <definition>` | name, supply, mint authority |
| `names token send <definition> <amount> --to <name|account|keys> [--from A]` | `<name>` resolves like `names pay`: to the name's public account, or with `--private` to its published npk/vpk (a pending credit); `--from` private proves locally |
| `names token receive` | receives pending token credits to this wallet's private accounts |
| `names token create <name> <supply> [--mintable]`, `names token mint` | for testing and the demo; not needed by the prize |

Module (`basecamp-module`), same shape as the existing `cli_pay` / `cli_balance` methods,
signing through the hidden `names module` command:

- `cli_token_holdings(account?) -> [{account, private, definition, name, balance}]`
- `cli_token_pending() -> [{position, definition, amount, to}]`
- `cli_token_send(definition, amount, to, from, private) -> {tx, block}`
- `cli_token_receive() -> [{tx, block}]`
- `cli_token_info(definition) -> {name, supply, authority}`

For the provider SDK and approval sheet (PLAN.md, provider agent): a token transfer
decodes to "send <amount> <token name> from <account> to <destination>" from the
`Transfer` message and the definition's name, so the approval prompt can show asset,
amount and destination instead of bytes.

## 7. Open questions

- Will Logos port the token program to 0.4 before testnet 0.4 launches, and with which
  model: per-token holding accounts like 0.2.x (then likely PDAs, since 0.4 actors cannot
  read a second account), or holdings in the holder's own actor like the native token? Who
  to ask: lez-programs maintainers. (Agents must not open issues there; the user can.)
- Does SPEL get a 0.4 backend? If yes, lez-programs follows it; if not, every program
  there is rewritten by hand.
- Composability: should a token transfer accept a call from another program (AMM vaults,
  ATA-like programs), and how is ambient authority avoided?
- Dust spam into holdings maps (section 2).
- Token metadata (name, symbol, decimals) has no decimals field in the reference
  definition; the wallet needs one to display amounts.

## 8. Blockers

1. No 0.4 reference token program (section 1); the prize excludes writing our own.
2. Testnet 0.4 is not live, so nothing can be checked against the canonical zone.
3. Private sends need a private account with native tokens to pay the in-proof fee.
   `docs/lez-0.4-port.md` records private Names flows failing on the fee payer's balance;
   see section 5 for what the token run showed.
