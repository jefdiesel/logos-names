# Names Wallet: the Wallet Provider SDK

Any Logos Basecamp app can use the person's Names Wallet through the `names`
core module: ask to see accounts, read their balances, and propose LEZ
payments and program calls. The person approves every one in the Names app.
The app never sees a key, and nothing is signed or sent until the person
approves.

This page is the API reference, with a worked example of
connect → account selection → approval → transaction in QML and in C++. Two
complete reference apps built on it, [Testimonial](../../apps/testimonial/README.md)
and [Faucet](../../apps/faucet/README.md), are the longer worked examples.

## How it works

```
your app ──propose──▶ names module ──queue──▶ Names app (approval sheet)
    ▲                       │                        │
    └──── request_status ◀──┴──── approve / reject ◀─┘
                            │
                            ├─ transfer: names CLI job (`names pay`, `auth-transfer send`)
                            ├─ call:     names CLI (`names module send`) tries, signs, sends
                            ├─ private call: names CLI simulates (`module private-call plan`),
                            │                then proves and sends (`module private-call send`)
                            ├─ secret:   names CLI derives the app's values (`module app-secret`)
                            └─ inscribe: the person's own Logos Blockchain node signs and sends
```

1. **Connect.** Your app calls `request_access`. The person sees your app's
   name and picks which accounts you may see.
2. **Read.** `granted_accounts` and `account_balances` answer at once, and
   only about the accounts you were given.
3. **Propose.** `propose_transfer` or `propose_call` queues a request. The
   person sees an account choice and then exactly what will be sent or signed.
4. **Wait.** Poll `request_status` until `state` is no longer `pending` or
   `running`.

Your app is identified by the name Basecamp gives its credential: the `name`
in your `metadata.json`. You can't pass a different name, because the module
reads your identity from the call itself (`current_caller()`). That name is
what the person sees in every prompt.

The Names app must be installed and open to answer requests. A request that
nobody answers expires after 10 minutes.

**Taking the person to Names and back.** After a call that leaves a request
`pending`, bring the Names app forward with Basecamp's own navigation intent,
`basecamp.apps.launch` (Basecamp 0.3.2 and later; the shell answers it, so no
chooser appears). Declare it in your `metadata.json`, as an object:
`"uses": [{ "intent": "basecamp.apps.launch" }]`, or the request fails
`not_declared`. Then:

```qml
if (logos.request) logos.request("basecamp.apps.launch", { app: "names_ui" }, function(res) {})
```

When the person approves or rejects, Names shows your app again by the same
intent, if your app is an installed Basecamp app (`ui_qml`) and no other
request is waiting. A request made by a module, not an app, stays in Names.
Keep polling `request_status`: coming back is navigation only, not the answer.

**Zones.** The wallet can hold several LEZ zones (chains) and uses one at a
time. Balances are those of the zone in use. A transfer or a call is asked for
the zone in use at that moment and can be approved only there: the person sees
the zone's name on the sheet, and if they have switched since, they are told to
switch back or reject. Your view of the request carries `zone` (`home`, or the
zone's channel) and `zone_channel` (64 hex, `null` if not known yet), never what
the person calls the zone. `get_config` tells you where you are before you ask:
`sequencer` and `token_program` are the zone in use's, `zone` is
`{"id", "channel", "home"}`, and `names_sequencer` is where names are read
(always home's). The same account ID is another account on another zone, so
compare `zone_channel` with the chain your app is built for. A payment to a
name needs the home zone in use; elsewhere, ask to pay an account ID. Access
(`request_access`) is per app and account, not per zone.

## Setup

Add the module as a dependency in your app's `metadata.json`:

```json
"dependencies": [
  { "name": "names", "version": ">=0.11.0" }
]
```

`0.11.0` stands for the first `names` release that includes the Wallet
Provider SDK. Every method returns JSON text:

```json
{ "success": true,  "value": <answer>, "error": null }
{ "success": false, "value": null,     "error": "why, in plain words" }
```

`request_access_with(reason, options_json)` is `request_access` with options, a JSON object.
The only option is `{"account_hint": "Public/<id>"}` (or `Private/<id>`): the account the access
is for, which the sheet ticks first when the wallet holds it. It only pre-ticks: the person still
chooses, and nothing is given without their tick. A hint the wallet doesn't hold is dropped, and
an unknown field or a malformed hint is refused. `"{}"` is the same as `request_access(reason)`.
Check `get_config().features` for `"request_access_with"` first, and fall back to
`request_access(reason)` on an older Names module. `features` is in the reply's `value`
(`{"success": true, "value": {…, "features": [...]}}`); builds before 3434018 showed it only to the
Names app, so another app never saw it there.

Adding an account to the access an app already has: check `get_config().features` for `"grant_add_account"`.
With it, `request_access_with` naming an `account_hint` the app wasn't given
asks to add that one account: `{request: "<id>", state: "pending"}`, a sheet that offers only that
account. Approved, it is added and the accounts given before stay; Not now ends the request
`rejected` with an `error` the app can show, and the access is as it was. A hint the app already
sees answers `{request: null, state: "approved"}` as before; a hint the wallet doesn't hold is
refused ("the Names wallet doesn't hold Public/<id>…").

`request_access` with no wallet on the network in use is refused at once ("the Names wallet has
no wallet on LEZ 0.4 yet; …"); ask again once the person has created it. A token source
(`propose_token_source`) naming a bridge program or one of the wallet's own programs is refused.

A call can also answer `{ "success": false, "busy": true, "value": null, "error": "The wallet
is busy; still working." }` (a method that answers a bool answers `false`). The module runs one
call at a time, and a call that arrives while another is waiting on a long step (a Logos
Blockchain node call, up to 15 s for a read and 180 s for a proof; a Program Registry lookup) is
answered busy at once. **Every app must retry on busy.**

- **`busy: true` ⇒ nothing happened; retry after a short wait** (a second or two, then a little
  longer each time). The answer comes from the module's generated dispatch before any module
  code runs: no request is created, nothing is stored, the caller is not even read. No module
  code ever answers `busy: true` after it has changed anything, so proposing again after busy
  can never make a second request.
- **No answer at all (a timeout) ⇒ the call may have been taken:** read `request_status` (or
  your pending state) before proposing again.
- Only `busy` tells it apart from a refusal; a refusal (`success: false`, no `busy`) is final for
  that input.

Accounts are written the way the LEZ wallet writes them: `Public/<base58 id>`
or `Private/<base58 id>`. Amounts are decimal text in base units, because
balances are 128-bit numbers.

## Methods for any app

| Method | What it does | Value |
|---|---|---|
| `request_access(reason)` | Asks to see some of the person's accounts. `reason` (up to 280 characters) is shown as your own words. If you already have access, nobody is asked. If a request is already waiting, you get that one back. | a request (below); if you're already connected, `{request: null, state: "approved", accounts}` |
| `request_access_with(reason, options_json)` | `request_access` with `{"account_hint": "Public/<id>"}`: the account the sheet ticks first (see above). | as `request_access` |
| `granted_accounts()` | The accounts you were given. Asks nobody. | `{connected, accounts: [{account, account_kind}]}`; `account_kind` is `public` or `private` |
| `account_balances()` | The LEZ balance of each account you were given. Reads run in the background and the answer comes at once with the last balance read. Ask again for a newer one; the chain is read at most every 15 s. | `[{account, account_kind, balance, updating, error}]`; `balance` is decimal text, or null until first read |
| `propose_transfer(asset, amount, destination)` | Asks to send `amount` of `asset` to `destination`. `asset` is `LEZ`, the native token (empty also means LEZ), or a token of the wallet's token program, written as its definition's account ID in base58 (see [Token transfers](#token-transfers)). `destination` is a name such as `alice`, or `Public/<id>`. The person picks the account it comes from. Checked when you call it. | a request |
| `propose_call(program_id, accounts_json, instruction_hex, summary)` | Asks to run a program, signed by an account the person picks (details below). | a request |
| `propose_private_call(account, call_json, summary)` | Asks for a privacy-preserving transaction from one of the private accounts you were given, across up to 4 programs (see [Private calls](#private-calls)). | a request |
| `declare_programs(programs_json, reason)` | Names the programs (1 to 8 IDs, a JSON array) you will ask the person to sign calls to that someone else pays for. The person approves it once (see [Calls someone else pays for](#calls-someone-else-pays-for)). | a request |
| `propose_relayed_call(account, call_json, summary)` | Asks the wallet to sign, as one of the public accounts you were given, a call whose fee another account pays (a relay's). The wallet hands the signed transaction back to you and sends nothing. LEZ 0.4 only. | a request; once approved, `transaction` and `hash` |
| `resign_relayed_call(request_id)` | Signs your approved relayed call again, without asking the person, when only the fee payer's nonce has moved (your relay had another transaction in flight). Same account and account nonce, program, bytes, actors, payer, gas limit, tip and max fee; at most 3 times within 10 minutes of the approval. A refusal carries `code`: `unchanged` (send what you have), `landed_or_used` (your account's nonce moved: the call may have landed; check before proposing again), `ask_again` (propose it again) or `refused`. | the request, with the new `transaction` and `hash` |
| `app_secret(account, scope, context_hex, tag, first, count)` | Values the wallet derives for your app from one of your accounts; the first use of each (account, scope) is approved by the person (see [App secrets](#app-secrets)). | `{granted: false, request}` or `{granted: true, account, scope, tag, first, values, imported}` |
| `propose_token_source(token_program, definitions_json)` | Asks to list a token program's definitions, with names, symbols and decimals, in the person's holdings (see [Token sources](#token-sources)). Needs no access first. | a request |
| `token_sources()` | The token sources the person accepted. | `[{token_program, definitions, app, since}]` |
| `propose_inscription(text)` | Asks to inscribe a testimonial on the Logos Blockchain (not the LEZ zone), paid from one of the person's node's accounts, which they pick. Needs no access first (see [Inscriptions](#inscriptions-on-the-logos-blockchain)). | a request |
| `get_config()` | The network part of the Names configuration (`sequencer`, `names_sequencer`, `registry`, `token_program`, `zone`, `names_network`, `module_version`) and `features`, what this build can do. Since 0.17.0 in the usual envelope; earlier versions answered a plain object. | `{sequencer, …}` |
| `request_status(request_id)` | How your request stands. Only the app that made a request can read it. | a request |
| `cancel_request(request_id)` | Withdraws your request while it's pending. | a request, `state: "cancelled"` |
| `drop_access()` | Gives up the access you were given. | `{revoked}` |
| `request_nft_access`, `granted_nfts`, `nft_membership`, `propose_nft_transfer`, … | NFTs: a capability of their own; see [NFTs](#nfts-lp-0001). | |

You must call `request_access` before `propose_transfer` or `propose_call`.
Otherwise they fail with `ask for access first (request_access), then propose it`.
`propose_inscription` is the exception: it names no account and reads nothing
of the person's, so it needs no access.

### A request

```json
{
  "request": "9f3c…(32 hex)",
  "kind": "connect | transfer | call | inscribe | private_call | secret | token_source",   // NFT requests have their own (nft_list, nft_transfer)
  "state": "pending | approved | running | done | failed | rejected | expired | cancelled",
  "created": 1793735537880, "expires": 1793736137880, "decided": null,
  "account": "Public/…",          // the account the person chose (transfer, call; inscribe: a chain key, 64 hex)
  "accounts": ["Public/…"],       // connect: all the accounts you now have
  "asset": "LEZ", "asset_kind": "native", "amount": "1000", "destination": "alice",   // transfer
  "token_name": null,             // transfer of a token: its definition's name
  "program": "<base58>",          // call
  "zone": "home", "zone_channel": "<64 hex>",   // transfer, call: the zone it was asked for
  "tx": "<hash>",                 // call, inscribe: the transaction sent
  "chain": "logos", "submission": "lp-0022:names-wallet", "text": "…",   // inscribe
  "fee": "1086", "channel": "<64 hex>", "msg_id": "<64 hex>",            // inscribe, once done
  "programs": ["<base58>"], "check": "running | ready | failed", "phase": "plan | simulate | prove | submit | confirm",
  "message_len": 6595,                                                   // private call
  "scope": "RFP022/deposit",                                             // secret
  "token_program": "<base58>",                                           // token source
  "error": null                   // failed: why, without the wallet's details
}
```

| `state` | Meaning |
|---|---|
| `pending` | Waiting for the person |
| `approved` | Access given (`connect`) |
| `running` | A transfer is being proven and sent: about 4 minutes from a private account, one block from a public one |
| `done` | Included in a block and not reverted. A call also gives `tx`, its hash. An inscription is `done` when the person's node has taken it: `tx` is its hash, and it is final a few minutes later |
| `failed` | `error` says why, in general terms. The wallet's own message, which may mention the person's balances, goes only to the Names app. For an inscription the error is always `the inscription may not have been sent`, and `account` is the key that was to pay: the node took it under another hash, or didn't answer after it was handed over, so it may still land. Don't ask again before the person has looked at their node's history |
| `rejected` / `expired` / `cancelled` | Nothing was signed or sent |

Times are in milliseconds. A decided request stays readable for a day.

### Program calls

`propose_call(program_id, accounts_json, instruction_hex, summary)`:

- `program_id`: the program's ID in base58.
- `instruction_hex`: the message the program's transaction carries, in hex,
  at most 2,048 bytes. On LEZ 0.4 this is `borsh(Message::Instruction(..))`,
  as your program defines it.
- `accounts_json`: the actors the call reaches, as a JSON array of at most 16
  entries, in the order your program expects. The account the person chooses
  always comes first, as its own actor of your program (its data for your
  program). You don't list it. Each entry is one of:
  - a public account ID (base58): your program's data on that account;
  - `{"seed": "<64 hex>"}`: one of your program's own accounts (a PDA). The
    seed is the evidence that lets the call create it. You may also give
    `"account"`, which must match the seed;
  - `{"account": "<id>", "balance": true}`: that account's LEZ balance rather
    than your program's data (it may be combined with `seed`);
  - `{"account": "signer", "balance": true}`: the chosen account's LEZ. Only
    with this entry can your program move the person's LEZ.
- `summary`: what your app says the call does, at most 280 characters. The
  prompt shows it as your own words, unchecked.

The prompt does not rely on your summary. It says what the call can do,
worked out from what it reaches:
- The chosen account signs, and your program may change the data it keeps on
  that account.
- Your program may move the chosen account's LEZ, up to all of it, when you
  listed `signer` for its balance. Otherwise the prompt says that the call
  can't move that account's LEZ, and only the fee is paid from it.
- Each other account is described as data, as your program's own account
  (made if it is new), or as a LEZ balance.

The prompt shows the program's label and source-verification status from the
Program Registry (`program_registry`, `program_label`). When the Program
Registry isn't installed, the prompt says so. It warns when the source isn't
verified or has changed since it was registered, when the program can be
upgraded, and when the program is the Names registry itself. For any of these
warnings, the person must tick an extra box before Approve works.

Calls and payments both go through the **names CLI's wallet**. An approved
call goes the same way as the names module's own registry changes on LEZ 0.4:
the module hands it to `names --program=<your program> module send`. The CLI
reads your program's bytecode from the chain and runs the call against the
current state first. A call that would fail stops there, with nothing sent
and no fee paid. The same run sizes the gas limit. Then the CLI signs with the
chosen public account, sends the call and waits for its block. A call that
was included but reverted counts as failed. The gas is sized only at this
point, so the prompt shows no number for it. The answer to `approve_request`
waits for the block, up to two minutes. The call can only reach your
program: the CLI loads one program's bytecode per call.

### Token transfers

`propose_transfer` also moves tokens of a token program (fungible tokens other
than LEZ, `docs/wallet/TOKENS.md`). Write the token as its definition's account
ID:

```qml
names("propose_transfer", [demoTokenId, "250", "alice"], function(v) { requestId = v.request; poll.start() })
```

- The person's wallet must have a token program set (Names, Tokens). None runs
  on the public testnet yet; until one does, a token transfer fails with
  `this wallet can't send tokens yet: it has no token program set`.
- The token must be one of that program's: the module reads its definition
  when you ask, and a token it can't find fails with
  `this wallet's token program has no such token`.
- `amount` is in the token's base units, up to 2^128 - 1.
- The request has `asset_kind: "token"`, and `token_name`, the name the token's
  definition gives it (its maker's words, with control and direction-changing
  characters removed).
- The prompt reads "send N <token name> from <account> to <destination>",
  with the token's ID, the token program's label and source-verification
  status from the Program Registry, and any warnings (unverified or changed
  program, no Program Registry, a token without a name). As for a program
  call, a warning needs an extra tick before Approve works.
- A transfer from the person's private account, or to a name (whose tokens go
  to its private receiving keys and wait there until its holder receives
  them), is proven on their computer: `running` lasts 7-12 minutes. From the
  public account to `Public/<id>` it takes one block.
- It is sent only with the token program the prompt named: if the person
  changes their token program in between, approving it fails and it stays
  pending, so they reject it and you can ask again.

### NFTs (LP-0001)

NFTs of Logos's reference NFT program ([NFT.md](NFT.md)) have two capabilities of their own,
each asked for, shown and revoked separately from account access (`request_access`). Neither
gives the other.

| Method | What it does | Value |
|---|---|---|
| `request_nft_access(reason)` | Asks to see which NFTs some accounts hold. The person ticks the accounts (private ones only if they tick them). The grant is shown as "which NFTs these accounts hold, with their collections and metadata". | a request (`kind: "nft_list"`), or `{request: null, state: "approved", accounts}` when already given |
| `granted_nfts()` | The NFTs of the accounts you were given: `{accounts, nfts: [{program, account, account_kind, definition, token, print_balance, collection}]}`. A public account is written `Public/<id>`; a private one only as a handle made for your app alone, `Private/~<40 hex>`, never its ID. `token` is `master` or `copy` (the program has no serial number); `collection` is `{definition, name, printable_supply, standard, uri, creators}`, or null when it can't be read. Answers busy until the wallet's first read is back. | as said |
| `nft_membership(program_id, definition)` | Whether those accounts hold an NFT of that collection: `{member, count}`. A plain answer about the accounts you were given, not a private proof. | as said |
| `request_nft_transfers(reason)` | Asks to be allowed to propose NFT transfers, a capability apart from the list. | a request (`kind: "nft_transfers"`), or `{request: null, state: "approved"}` |
| `propose_nft_transfer(program_id, from, destination, summary)` | Asks to send the one NFT `from` holds under that program (a copy, or the master with all its print rights) to an LNS name or `Public/<id>`. `from` is an account (or a handle) you were given for the list, and you need `request_nft_transfers` too. Someone's private receiving keys are refused: the wallet can't check that their account is ready. | a request (`kind: "nft_transfer"`) |
| `nft_request_status(request_id)` / `cancel_nft_request(request_id)` | How your NFT request stands / withdraw it while pending. | `{request, kind, state, program, from, destination, tx, error}` |
| `drop_nft_access()` | Gives up the NFT list. | `{revoked}` |

- `program_id` must be in the person's list for the network in use (every deployment of the NFT
  program is a different program; the person adds them under Collectibles).
- A transfer request is approved on the same sheet the Names app shows for its own Send: the
  collection, from, to, the asset leaving, what becomes public, the fee, the program's Program
  Registry status and warnings (an extra tick), and for a proven one the private account that
  pays. Approval starts it only with that sheet's digest, so an app's request and the person's
  own Send take the same two steps. There are no collection-wide approvals.
- States: `pending`, `running` (being sent: one block from a public account; 7-12 minutes when
  proven), `done` (with `tx`), `failed` (with one of the plain refusals, never the wallet's own
  text), `rejected`, `expired`, `cancelled`. Same limits as other requests (4 waiting per app,
  30 an hour, 10 minutes to answer).
- Approval waits for the Program Registry's answer about who publishes your app, as for every
  other request; a failed publisher check needs the person's extra tick.
- Names app only: `pending_nft_requests()`, `approve_nft_request(id, accounts, digest,
  fee_payer)`, `reject_nft_request(id)`, `nft_grants()` (`[{app, accounts, since, capability,
  discloses}]`), `revoke_nft_capability(app, capability)` and `revoke_nft_access(app)`.
  "Remove access" (`revoke_app`) takes every NFT capability too, and cancels the app's
  waiting NFT requests.
- The Names app's Collectibles tab says "· N waiting" on every page while a request waits.

### Membership proofs (LP-0001)

A third capability, `membership_proof`, needs no account access and gives none
(spec `~/logos/LP0001-OWNERSHIP-PROOF-SPEC.md` §4). UNRUN: no gate is pinned on any network
yet, so every proof ends `failed` with `GATE_NOT_PINNED` and nothing is sent.

| Method | Value |
|---|---|
| `request_membership_proof(request_json)`: `{program, collection, purpose, nonce (64 hex, yours), ttl (ms, default 1 h, at most 24 h)}`; on-chain use (`onchain`) is refused for now | `{request, state: "awaiting-approval"}` at once |
| `membership_proof_status(request_id)` | `{request, kind, state, collection, status}`; `status.state` is `awaiting-approval`, `checking`, `proving` (with `elapsed_seconds`), `sending`, `waiting-for-block`, `ready` (with `proof: {gate, slot, not_before, expires_at, tx_hash}`), `failed` (with `code` and `plain_message`), `rejected`, `expired` or `cancelled` |
| `cancel_membership_proof(request_id)` | until sending |

- Errors (`code`): `NOT_A_MEMBER`, `FEE_RESERVE`, `ACCOUNT_BUSY`, `OUT_OF_WINDOW`,
  `PROVER_OUT_OF_MEMORY`, `RPC_UNREACHABLE`, `GATE_NOT_PINNED`, `UNEXPECTED_OUTCOME`.
- The wallet fills in the context's audience from your app's identity; you can't set it.
- Nothing returns an account, a balance, a holding, a commitment or a nullifier.

### Inscriptions on the Logos Blockchain

`propose_inscription(text)` asks the wallet to write a testimonial on the Logos
Blockchain itself, as a channel inscription. Everything else in this SDK
happens on the LEZ zone.

```qml
names("propose_inscription", [text], function(v) { requestId = v.request; poll.start() })
```

- `text` is 1 to 1,024 bytes of UTF-8 with something in it besides spaces, no
  control characters except newline and tab, and no character that turns the
  text around or shows nothing (bidi marks and overrides, zero-width space,
  BOM, line separators and the like; the emoji and script joiners are fine).
  It is checked when you call.
- You name no account. In the Names app the person sees your app's name and
  publisher, the words, that an inscription is public and permanent, the
  accounts their own node holds with their balances, and the fee their node
  asks for it from the account they tick. Nothing is held or sent before they
  approve.
- What is inscribed is `{"submission":"lp-0022:names-wallet","text":"…"}`, on
  the paying account's own channel
  (`sha256("lp-0022:testimonial:" || its inscription key)`), signed by an
  Ed25519 inscription key the wallet keeps for that account. The only thing
  that moves is the fee; the node may fund at most 100,000 for it, and an
  inscription that would cost more is refused when its fee is shown.
- Approving sends it at once. `done` gives `account` (the key that paid), `tx`,
  `fee` (what it paid), `channel` and `msg_id`.
- The person needs a running Logos Blockchain node in Basecamp with an account
  that holds enough for the fee. If the node is stopped, or refuses the
  inscription before it is sent, the request stays pending and the person is
  told why; they can still reject it.
- The queue limits below apply as for any request.

### Private calls

`propose_private_call(account, call_json, summary)` asks the wallet to build, prove and send a
privacy-preserving transaction from `account` (`Private/<id>`, one you were given). It can reach
several programs in one transaction, which `propose_call` can't. `call_json`:

```json
{
  "programs": ["<root program>", "<other program>", "…"],     // 1-4, the root's first
  "accounts": [                                               // up to 16 actors besides the root
    { "actor": "account", "program": "<id or native>", "openings": true },  // the account's own actor
    { "account": "<base58>", "program": "<id or native>" },               // a public account, never signed
    { "seed": "<64 hex>", "of": "<program>", "program": "<id or native>" } // a PDA of `of`
  ],
  "message": "<hex>",                       // the root message, at most 16,384 bytes
  "openings": ["<64 hex>"],                 // one-time recipients of the account, 0-4
  "fee_payer": "account",                   // "account" (it pays its own fee) or "wallet" (any private account)
  "declared": {                             // what you say the call moves
    "debits":  [{ "asset": "LEZ | <token definition>", "amount": "…" }],
    "credits": [{ "asset": "LEZ | <token definition>", "amount": "…" }],
    "max_fee": "…"
  },
  "reveal_account": false                   // true only if the call publishes the account's ID on purpose
}
```

- The root actor is the account's own actor of `programs[0]`; you don't list it. `program`
  defaults to the root program. `"native"` is the LEZ balance.
- The person's account is the only private account a call reaches; the wallet signs nothing
  public in it. A one-time address `account.blinded(opening)` that you declare is received
  inside the proof, so a credit to it lands in the account and the address is never linked to it.
- **Nothing you write decides what the person sees.** The names CLI simulates the call twice
  when you ask: the wallet's own dry run of the exact transaction, and a replay of it with LEE's
  executor that shows what each account ends with. `check` is `running` meanwhile (seconds to a
  minute), then `ready` or `failed`. The sheet shows what the simulation found: what the account
  receives and pays, per asset; the fee (about, and at most); every public account's change; the
  programs with their Program Registry status and whether your app used them before; what
  becomes public and that the account's ID doesn't; proving time and memory; then your summary,
  marked as your words.
- **Refused, whatever the person clicks:** a debit of LEZ or of a token beyond what you
  declared; a change to data the account already keeps for a program the wallet can't value; a
  fee that can exceed your `max_fee`, or paid from another account when you said `account`; the
  account's ID in public state or events unless you set `reveal_account`; anything the
  simulation can't run or read.
- **Approving** starts a wallet job that simulates again and stops unless the effects are those
  the person approved, then proves on the person's computer (about 11 minutes and 9.5 GB for a
  bridge claim on an Apple M4 16 GB, 4 threads), sends and waits for the block. `state` is
  `running` with `phase`; `done` gives `tx`. You may `cancel_request` it until `phase` is
  `submit` (a cancel late in proving may come after it was sent: the request says so); the
  module can also pause and resume it (`private_call_control`, Names app only; no buttons in
  the app yet).
- One wallet job runs at a time: a call approved while another job runs stays pending, and the
  person is told why.

### App secrets

`app_secret(account, scope, context_hex, tag, first, count)` gives your app values derived from
one of the person's accounts that you were given:

```text
m_i   = HKDF-SHA256(ikm = the account's key, salt = "logos-names/app-secret/v1",
                    info = "logos-names/app-secret/v1/info" | u8 len(app) | app | u8 len(scope) | scope
                           | u16be len(context) | context | account id | u64be i)
value = SHA256(tag | m_i)        for i in first .. first + count
```

- **Scope `RFP022/exit` names an Ethereum address.** Its context must be the LEZ Bridge's exit
  layout, `cfgId(32) | token(20) | u256(denom) | ethDest(20) | u256(maxRelayFee)` (136 bytes);
  anything else is refused, never granted. For an `ethDest` the person hasn't approved for your app
  and that account, the answer is `{granted: false, request}` even when the scope is granted: the
  sheet shows the address in full, the four words the wallet makes itself (the bridge page's
  `logos-bridge/withdraw-to/v1` words, when the relay fee is 0) and the bridge (v4 to v7 by their
  Config), and needs a tick. Approved addresses are kept per (app, account) in the network's wallet
  folder (`app-secret-recipients.json`, no secret), and outlive taking the grant back. The Names
  app's own Bridge to Ethereum approves the address the person reads back and ticks at step 2
  (`approve_exit_recipient`, Names app only), so its own send-backs ask nothing more.
- `app` is your module's name as Basecamp gives it, never an argument: another app gets other
  values. `m_i` never leaves the wallet; you get `SHA256(tag | m_i)` for a tag of your choice
  (1-32 printable ASCII). `scope` is 1-64 printable ASCII, `context_hex` up to 256 bytes, `count`
  1-64.
- The same call gives the same values after the person restores the wallet from its recovery
  phrase (accounts made again in the same order are the same accounts). An account **imported**
  from a key (the wallet holds its key, no key tree of the phrase has it) gets values from the
  phrase too: `ikm` is the private key tree root's secret spending key `R`, under salt
  `logos-names/app-secret/imported/v1` and info prefix `logos-names/app-secret/imported/v1/info`
  (the rest of `info` as above, the account's id in it), so a wallet restored from the phrase
  alone makes them again (see [BRIDGE-SDK.md](BRIDGE-SDK.md) §1). The answer then carries
  `imported: true`. The CLI decides by key-tree membership and names the scheme
  (`names module app-secret` prints `scheme`; `names module account-scheme` answers it alone);
  the module refuses an imported account's values from a tool that names none.
- The first call for an (account, scope) returns `{granted: false, request}`: the person sees
  your app, the account, the scope, and that you get values from which the account's key can't
  be recovered and with which nothing can be spent. Poll `request_status`; once `approved`, call
  again. The person can take a grant back in Settings.
- At most 120 calls an hour per app; each runs the names CLI (about a second).
- Example, the RFP-022 bridge: the deposit master `s` of deposit `i` is `m_i` with scope
  `RFP022/deposit` and context `cfgId | token | u256(denom)`; asking with tag `RFP022/ck` gives
  the claim secret `k = SHA256("RFP022/ck" | s)`, while `s` stays in the wallet.

### Token sources

`propose_token_source(token_program, definitions_json)` asks the person to list a token
program's tokens with their holdings. `definitions_json` is `[{definition, name, symbol,
decimals, origin}]`, up to 16; the names are your words, shown with the definition IDs. The
program must speak the wallet's token messages (`wallet-tokens/`; the RFP-022 bridge's token
program does). Once approved, the Names app lists them as "Bridged tokens" with their decimals,
and can send them through the same path as its own tokens.

### Worked example: a private bridge claim

The RFP-022 bridge mini-app claims a Sepolia deposit to the person's private account `V`,
minted to a one-time address of `V` and sponsored from the bridge's float:

```qml
// 1. The claim secret, from the wallet (asked once per account and scope).
names("app_secret", [V, "RFP022/deposit", cfgId + token + u256(denom), "RFP022/ck", index, 1], function(v) {
    if (!v.granted) { requestId = v.request.request; poll.start(); return }
    k = v.values[0]                                   // the deposit was made with this k
})
// 2. The claim: ClaimPlan's accounts, in order (rfp-022 lez-bridge cli/src/main.rs).
var call = {
    programs: [bridge, tokenProgram, attestProgram],
    accounts: [
        { seed: anchorSeed, of: attestProgram, program: attestProgram },
        { seed: nullifierSeed, of: bridge },
        { seed: configSeed, of: bridge },
        { account: dUSDC, program: tokenProgram },
        { actor: "account", program: tokenProgram, openings: true },
        { seed: configSeed, of: bridge, program: "native" },
        { actor: "account", program: "native", openings: true }
    ],
    message: claimAttestedHex,                        // borsh(Message::Instruction(ClaimAttested {..}))
    openings: [sha256("RFP022/alias" + k)],
    fee_payer: "account",                             // paid from the sponsored credit
    declared: { credits: [{ asset: dUSDC, amount: "100000000" }], max_fee: "20000000" }
}
names("propose_private_call", [V, JSON.stringify(call), "Claim 100 dUSDC from Sepolia"], function(v) {
    requestId = v.request; poll.start()              // check → ready, then the person approves
})
```

The person sees the sheet in [BRIDGE-SDK.md](BRIDGE-SDK.md) §2: 100 dUSDC and 20,000,000 LEZ to
`V`, nothing leaving it but the fee, the float's and the supply's public changes, the one-time
address, and that `V`'s ID appears in nothing public. `scripts/bridge-claim-call.py` builds this
call from a demo run's frozen material.

### Calls someone else pays for

On LEZ 0.4 a public transaction's fee payer is named in the transaction and signs it; it need
not be the account that makes the call (`lee` `FeeDeclaration`). So an account that holds no
LEZ, even one never used on the chain, can make a public call when another account (a relay)
pays: the account signs, the relay adds its own signature as fee payer and submits it. The
wallet does the first half: it signs as the person's account and gives the transaction back to
your app. **The relay sends it and pays the fee**; the wallet can't see what happens to the transaction
afterwards.

1. **Declare your programs once:** `declare_programs(programs_json, reason)`, a JSON array of 1
   to 8 program IDs. The person approves the list. A relayed call to any other program is
   refused before anyone sees it, and Settings takes the list back with the app's access.
2. **Propose the call:** `propose_relayed_call(account, call_json, summary)`. `account` is one of
   the public accounts you were given (`Public/<id>`). `call_json`:

   ```json
   {
     "program": "<root program ID>",
     "message": "<hex: the root call's message bytes, at most 16,384 bytes>",
     "accounts": [
       { "account": "<id>", "program": "<program ID, or native>" },
       { "seed": "<64 hex>", "of": "<program ID>", "program": "<program ID>", "new": true }
     ],
     "fee": { "payer": "Public/<the relay's payer>", "gas_limit": 120000, "tip": "0", "max_fee": "900000" },
     "privacy": "public_call"
   }
   ```

   - The root actor is `account` under `program`; `accounts` are the other actors the call
     declares (at most 16), in order; `program` defaults to the root program; `native` is an
     account's LEZ balance. A program account (`seed` + `of`) the call creates is marked
     `"new": true`, which adds its admission evidence.
   - Each entry has `account` **or** `seed`, never both: the wallet works out a program
     account's address from its seed and refuses an entry that also gives an address. (Unlike
     `propose_call` above, and unlike the names CLI's own `module public-call` input, which
     accept an address beside a seed when it matches.)
   - Every program named must be one you declared. The wallet's own Names registry (either
     network's) and its token program are refused, at declaration and in a call, as the root, as
     a declared actor's program or as a program account's owner. The programs that hold the
     person's bridged tokens on LEZ 0.4 (the token program each configured bridge's Config names;
     the wallet knows up to 4 bridges) are refused as the root only: a call to a bridge may still
     declare them. The wallet reads them from the chain, also keeps every one it has read and
     knows the default bridge's built in; if it can't read any one bridge's Config, it signs
     nothing. The token program of a token source an app added is refused as the root too. Each rule is applied again at approval and at re-sign.
   - **One exception: a burn to send tokens back to Ethereum** (feature `relayed_bridge_burn`).
     A bridge's token program may be the root of exactly one shape: `BurnWithNotice` (variant
     10, 145 bytes: definition, amount u128 LE, notify account and program, a 32-byte memo E)
     of one denomination of a token registered in the Config of a bridge whose vault releases by
     proof (built in: bridge v4 `BKXe3q5J…JBf`, v5 `CuV1ctZ2…DMA`, v6 `4PxaLAfP…QWed` and v7
     `CdijfhGn…TM93`; v2 and v3 share their token program and are refused; v4 to v7 share it too, and
     each bridge's Config lists only its own wrapped definitions, so a token burns only toward its
     own bridge), notified to that bridge's Config PDA, declaring exactly
     `[{account: <definition>, program: <token program>}, {seed: <Config seed>, of: <bridge>,
     program: <bridge>}]` (not `new`), with privacy `bridge_public_burn`, burns not frozen. The
     wallet reads that Config at proposal, sheet, approval and re-sign (failing closed); its own
     build must reach only the root and those two accounts, give no LEZ balance and create
     nothing. Both programs are declared as usual. Anything else to that root stays refused. The
     sheet then shows what is burned (amount and token from the Config), the bridge, the exit
     commitment and "This destroys the wrapped tokens on Logos and can't be undone; the ETH is
     released on Ethereum after the bridge's steps." The Ethereum address is not in the burn (only
     E), so the wallet can't check it: name it in your `summary`, which the sheet labels as your
     words. Shape and vector: rfp-022 `modules/NAMES-SEND-BACK.md` §7, `public-burn-v1.json`.
   - `fee` is the fee declaration exactly as signed: the payer, the gas limit, the tip and the
     max fee. The payer must not be the signing account, nor any account of the person's wallet
     (that is an ordinary call, `propose_call`), and the call must not declare an actor of the
     payer's account.
   - `privacy` picks the sheet's privacy line from a fixed list; it is never free text:
     - `public_call` (the default): "Public: this account, the program it runs, the bytes it sends
       and what it changes, and the account that paid the fee."
     - `bridge_public_claim`: "A public claim publishes the claim secret and links the Sepolia
       deposit to this account."
     - `bridge_public_burn` (the burn above only): "A public burn publishes this account, the
       amount and an exit commitment; the Ethereum address stays hidden until the release."
3. **The wallet builds it** at once (names CLI `module public-call plan`: the account's and the
   payer's nonces from the chain, no proof, a second or two). A call it can't build is refused
   with the reason, and nobody is asked. `request_status` gives `check: "ready"`.
4. **The person sees a sheet made from that build**, never from your words: your app's name, the
   network (LEZ 0.4 · bridge test network), the program as `unknown program <id>` with what the
   Program Registry says of it (the wallet doesn't decode the message bytes, and a warning says
   so: the person ticks it before approving), the account that signs, who pays the fee and its caps, each other
   account reached (its LEZ balance or its data), the program accounts it creates, the SHA-256 of
   exactly what is signed, the privacy line, that the wallet sends nothing and can't see what
   happens afterwards, and then your `summary`, labelled as your own words. Every text that
   comes from an app or the chain is shown as plain text, one line, with control, bidi and
   zero-width characters removed. A build that differs from the request in any field is refused.
5. **Once approved**, the wallet builds the call again and signs it only if it is still the call the
   person saw (the same hash: an account or payer used since gives another one). `request_status`
   then gives `state: "done"`, `transaction` (the half-signed `PublicTransaction`, hex of its borsh
   bytes) and `hash`. The request is used up: approving it again is refused, and the transaction
   can be submitted once on this chain (rc3 binds no chain id: after a reset of the test network,
   or on another LEZ 0.4 chain where the account's nonce comes back to what it was, the payer
   could submit it again).
6. **If the relay says it is busy** (its payer's nonce moved), call `resign_relayed_call(request)`:
   the wallet builds the call again and signs it only if nothing but the payer's nonce changed
   (at most 3 times, within 10 minutes of the approval). A refusal's `code` says what to do:
   `unchanged` (the payer's nonce hasn't moved; send what you have), `landed_or_used` (your
   account's nonce moved: the call may have landed; check the chain first), `ask_again` (propose
   it again) or `refused`. A re-sign is also `ask_again` when the wallet has seen the bridge test
   network reset since 30 s before the call was asked (`bridge-net.json`'s `reset_at`; a read stamps it up to 25 s early): a
   new build after a reset is on a chain the person never saw the call on.

**What the account's signature covers (LEZ v0.4.0-rc3).** The SHA-256 of the whole borsh message
under `/LEE/v0.3/Message/Public/`: the declared actors and signing accounts, the root call and its
bytes, the fee declaration (payer, gas limit, tip, max fee), both signers' nonces, and the
admission evidence. So the relay can't give the transaction another payer, other caps, another
byte or another actor without breaking the account's signature. It names the account's current
nonce, so on this chain it lands at most once and dies with the account's next transaction. rc3
binds no chain id and carries no validity window: after a reset of the test network, or on another
LEZ 0.4 chain where the account's nonce comes back to what it was when it signed, the payer could
submit it again. The request itself expires after 10 minutes like every request.

**What stays unbound, and why.** The wallet refuses a re-sign after a reset it has seen (step 6).
It can't do more on rc3, because the signature carries no chain id: (1) a transaction already
signed and handed back stays good on any LEZ 0.4 chain where both signers' nonces match, and the
wallet can't take a signature back; (2) the wallet knows of a reset only once it has read the
bridge test network since (its balances read, at most every 30 s, while asked for), so a reset no
read has seen yet doesn't stop a re-sign (the account-nonce rule still does when the new chain's
nonce differs); (3) the reset record follows the bridge test network's endpoint, not the zone a
call was asked in (`asked_here` checks the zone and its configured channel); (4) a missing or
unreadable record means no reset known. Binding the chain into what is signed waits for LEZ. Checked by the names CLI's test `a_half_signed_call_verifies_and_binds_its_payer`
(vector `cli/tests/vectors/public-call-v1.json`). The full argument, statement by statement with
its test, is in [basecamp-module/rust-lib/SIGNING.md](../../basecamp-module/rust-lib/SIGNING.md).

### Worked example: a relayed public bridge claim

The RFP-022 bridge claims a Sepolia deposit to a public account `C` that holds no LEZ; its
relay pays the fee (rfp-022 `lez-bridge/cli/src/relay.rs`, `public-demo/relayer/relayer.py`).
The bridge mini-app does the bridge's part and the wallet only signs:

```qml
// Once: the programs this app will ask the person to sign calls to.
names("declare_programs", [JSON.stringify([bridge]), "To claim your bridge deposits"], function(v) { requestId = v.request; poll.start() })
// The relay's payer and caps, from the chain's endpoint (the bridge's own JSON-RPC methods).
//   bridge_relayInfo -> {payer, max_gas, max_fee}
// The claim: ClaimPlan's accounts in order after the root (relay.rs `build`), the message
// borsh(Message::Instruction(ClaimAttested{...})) the bridge builds from the Sepolia logs.
var call = {
    program: bridge,
    message: claimHex,
    accounts: claimActors,                          // [{account|seed+of, program}], no payer actor
    fee: { payer: "Public/" + info.payer, gas_limit: info.max_gas, tip: "0", max_fee: String(info.max_fee) },
    privacy: "bridge_public_claim"
}
names("propose_relayed_call", ["Public/" + C, JSON.stringify(call), "Claim 100 dUSDC from Sepolia"], function(v) {
    requestId = v.request; poll.start()             // check: ready, then the person approves
})
// request_status(requestId) -> {state: "done", transaction, hash}
// then: bridge_relayClaim [transaction] on the chain's endpoint; the relay checks it, co-signs, submits.
```

What the bridge mini-app still does itself: read the deposit leaves from the Sepolia vault's logs
and its proof, build the claim message exactly as `relay.rs` does, pick the gas limit and max fee
within the relay's caps (`bridge_relayInfo`), and hand the signed transaction to
`bridge_relayClaim`. The wallet never reads Sepolia, never talks to the relay and never decodes
the bridge's message: the sheet names the bridge program by its ID and registry label and shows
the message's hash.

### Limits

| Limit | Value |
|---|---|
| Requests waiting, per app / in all | 4 / 32 |
| Requests per app per hour | 30 |
| Time a request waits for an answer | 10 minutes |
| Accounts an app can be given | 8 |
| Apps with access | 64 |
| `reason`, `summary` | 280 characters; control and bidi characters are removed |
| Actors a call reaches besides the signer's own / its message | 16 / 2,048 bytes |
| A private call's programs / actors besides its root / message / one-time addresses | 4 / 16 / 16,384 bytes / 4 |
| `app_secret` values per call / calls per hour / grants per app | 64 / 120 / 16 |
| A relayed call's actors besides its root / message / declared programs per app | 16 / 16,384 bytes / 8 |
| Token sources / definitions each | 16 / 16 |

Requests and grants are kept in `~/.config/logos-names/provider.json`. The
file is mode 0600 and written atomically, so a grant survives a restart and a
crash never leaves a half-written file.

## Methods for the Names app

Only the Names app (`names_ui`) and the host may call these. Any other app
gets `only the Names app can do this`.

| Method | Value |
|---|---|
| `pending_requests()` | the waiting connect, transfer and call requests (not inscriptions), oldest first, with everything the prompt shows and `choices`: `[{account, account_kind, pays, label, granted}]`. A transfer or a call also has `zone`, `zone_name`, `zone_in_use` (false: it can't be approved until the person switches to that zone) and `zone_removed`. A payment comes from the wallet's main public or private account (`pays`). A call can be signed by any of its public accounts; those beyond the main one are read in the background and can appear a few seconds later |
| `approve_request(request_id, account)` | approves with one of the request's `choices` (for `connect`, one or more separated by commas). A transfer starts as a CLI job, and its `job` is in the value. If the wallet is busy, the request stays pending and the error says so |
| `reject_request(request_id)` | the rejected request, of any kind |
| `pending_inscriptions()` | the waiting inscription requests, oldest first: `{id, app, publisher, says, submission, text, created, expires, choices}`. `choices` is `[{key, balance, label?, title?}]`, the accounts the person's node holds, named as the Blockchain tab names them and without an empty account they hid. If the node can't be asked, `choices` is empty and `node` says why |
| `approve_inscription(request_id, from_key)` | inscribes it from that account and answers when the node has taken it (`done`) or may have (`failed`). Refused before anything is sent (the key isn't one the node holds, the node is stopped, the account can't pay the fee): the request stays pending and the error says why |
| `chain_inscribe(from_key, text, send)` | with `send` false, the plan the sheet shows: `fee_estimate`, the node's own fee for this inscription from this account, asked in a way that holds none of the account's funds (`null` when the node can't say; an error when it is above the 100,000 a send pays) |
| `app_grants()` | `[{app, accounts, since}]` |
| `revoke_app(app)` | `{revoked}`; also its secret grants and remembered programs |
| `secret_grants()` / `revoke_secret(app, account, scope)` | `[{app, account, scope, since}]` / `{revoked}` |
| `private_call_control(request_id, action)` | `pause`, `resume` or `cancel` an approved private call until it is being sent |
| `token_source_holdings()` | `[{token_program, app, holdings: [{account, account_kind, definition, name, symbol, decimals, origin, balance}], updating, error}]` |
| `cli_token_send_source(token_program, definition, amount, to, from_private, to_public)` | a token send job, as `cli_token_send`, for a token source's token |
| `remove_token_source(token_program)` | `{removed}` |
| (in `pending_requests`) a private call's `sheet` | `{lines, public_changes, privacy, programs, warnings, refusals, refused, digest}`; Approve works only when `check` is `ready` and `refused` is false |

## Worked example: a tip jar

Throughout this example, `tipjar_ui` is a Basecamp `ui_qml` app with `names`
in its dependencies. It connects, shows the balance of the account it was
given, and asks to tip `alice` 1,000 LEZ.

### QML

```qml
import QtQuick

Item {
    id: tipjar
    property string requestId: ""
    property string status: ""
    property var accounts: []

    // Every names call answers {success, value, error} as JSON text.
    function names(method, args, done) {
        logos.callModuleAsync("names", method, args, function(reply) {
            var r = JSON.parse(reply)
            if (r.success) done(r.value)
            else status = r.error
        }, 20000)
    }

    // 1. Connect. The person picks the accounts in the Names app.
    function connect() {
        names("request_access", ["Show your balance and send tips"], function(v) {
            if (v.state === "approved") { accounts = v.accounts; return }
            requestId = v.request
            status = "Approve in the Names app…"
            poll.start()
        })
    }

    // 2. Propose a transfer. The person picks the account it comes from.
    function tip() {
        names("propose_transfer", ["LEZ", "1000", "alice"], function(v) {
            requestId = v.request
            status = "Approve the tip in the Names app…"
            poll.start()
        })
    }

    // 3. Wait for the answer.
    Timer {
        id: poll
        interval: 2000
        repeat: true
        onTriggered: names("request_status", [tipjar.requestId], function(v) {
            if (v.state === "pending" || v.state === "running") return
            poll.stop()
            if (v.kind === "connect" && v.state === "approved") tipjar.accounts = v.accounts
            tipjar.status = v.state === "done" ? "Tip sent from " + v.account
                          : v.state === "approved" ? "Connected"
                          : "Not sent: " + (v.error || v.state)
        })
    }

    // Balances: answered at once, refreshed in the background.
    function balances(done) { names("account_balances", [], done) }
}
```

Use `x["public"]` rather than `x.public` in QML. Some Qt builds refuse a
reserved word after a dot, so this API uses `account_kind` throughout.

### C++ (a `ui_qml` app with a C++ backend)

The module builder generates a typed wrapper for each dependency from the
module's interface (`names_api.h`), with the method and parameter names as
written above. Calls return the reply as `QString` JSON. The async variants
pass that `QString` to your callback. Check the generated header for the
exact signatures in your build.

```cpp
#include <QJsonDocument>
#include <QJsonObject>

// Returns the reply's "value", or an empty object and the error in `error`.
static QJsonObject valueOf(const QString& reply, QString* error) {
    const QJsonObject r = QJsonDocument::fromJson(reply.toUtf8()).object();
    if (!r.value("success").toBool()) { *error = r.value("error").toString(); return {}; }
    return r.value("value").toObject();
}

void TipJar::connectWallet() {
    QString error;
    const QJsonObject v = valueOf(modules().names.request_access("Show your balance and send tips"), &error);
    if (!error.isEmpty()) { emit failed(error); return; }
    if (v.value("state").toString() == "approved") { emit connected(v.value("accounts").toArray()); return; }
    m_request = v.value("request").toString();   // poll request_status(m_request) on a timer
}

void TipJar::tip() {
    QString error;
    const QJsonObject v = valueOf(modules().names.propose_transfer("LEZ", "1000", "alice"), &error);
    if (!error.isEmpty()) { emit failed(error); return; }
    m_request = v.value("request").toString();
    m_poll.start(2000);
}

void TipJar::onPoll() {   // connected to m_poll's timeout
    QString error;
    const QJsonObject v = valueOf(modules().names.request_status(m_request), &error);
    const QString state = v.value("state").toString();
    if (state == "pending" || state == "running") return;
    m_poll.stop();
    if (state == "done") emit tipped(v.value("account").toString());
    else emit failed(error.isEmpty() ? v.value("error").toString(state) : error);
}
```

### A program call

The same flow applies to a contract call. For example, the testimonial program
(`testimonial/`) keeps each submission's list in a head account and each
testimonial in an account of its own, both derived from seeds. The person's
account signs and pays only the fee, so the call doesn't list `signer`:

```qml
names("propose_call", [
    testimonialProgramId,                                  // base58
    JSON.stringify([ { seed: headSeedHex },                // the submission's head (made if new)
                     { seed: nodeSeedHex } ]),             // this author's testimonial account
    messageHex,                                            // borsh(Message::Instruction(Post {..}))
    "Post a testimonial: “Names Wallet is great”"
], function(v) { requestId = v.request; poll.start() })
```

`state` becomes `done` with `tx` once the call is in a block and has not
reverted. A call the program refuses fails when the CLI tries it, before
anything is sent.

When an address in the call depends on who signs (here the testimonial
account is derived from the author), your app has to know the signer before it
proposes: let the person pick one of the granted public accounts in your app,
build the call for it, and tell them to choose the same account in the Names
sheet. If they choose another, the CLI's dry run refuses the call and nothing
is sent or charged.

### What the person sees

1. A sheet over the Names app with your app's name, as Basecamp gives it, and
   how long the request has left.
2. **Account selection.** For access, the Names wallet's accounts, as
   checkboxes. For a payment, its main public or private account. For a call,
   its public accounts. Accounts your app already has are marked.
3. **What will be sent or signed.** For a payment, the asset, amount and
   destination; for a token, also the token's name and ID and the token
   program's label and verification. For a call, the program's label and ID, its source
   verification, what the call can do, any warnings, the data size, the gas
   line, and your summary marked as your own words.
4. **Approve** or **Reject**. In Settings, under Connected apps, the person can
   take back any app's access.

## Worked examples: the reference mini-apps

Two complete Basecamp apps use only this SDK for everything that touches the
wallet. Their READMEs walk through connect → account selection → approval →
transaction with the code that does it, and record how they were tested:

- [Testimonial](../../apps/testimonial/README.md) (`apps/testimonial/`): a
  program call (`propose_call`) whose message and PDA seeds are built in
  JavaScript and checked against the program's Rust encoding, then a list read
  back from the chain with per-month counts. Its "Logos Blockchain" choice
  inscribes the testimonial there instead (`propose_inscription`).
- [Faucet](../../apps/faucet/README.md) (`apps/faucet/`): access and account
  selection, then a request to a disclosed, optional faucet service, with its
  rate limits explained and the balance watched with `account_balances`.

**Network access in a `ui_qml` app.** Basecamp gives a QML view no network (its
engine has a deny-all network access manager), so `XMLHttpRequest` can't reach
a sequencer or any other service. Wallet actions go through this SDK. For plain
reads or another HTTP service, give the app a small C++ backend (a universal
`ui_qml` module with a `.rep` file); both mini-apps show one.

## Security model

- **Identity is the caller, not an argument.** `current_caller()` names the
  calling module. A caller that can't be named is refused, and so is a module
  that names itself `host`.
- **Only the Names app decides.** `pending_requests`, `approve_request`,
  `pending_inscriptions`, `approve_inscription`,
  `reject_request`, `app_grants` and `revoke_app` refuse every app except
  `names_ui` and the host. This is the same check that guards the module's
  paying and signing methods (`cli_pay`, `cli_pay_account`, `wallet` and
  others), and those methods stay closed to other apps.
- **Nothing is signed before approval.** The only code that starts a payment
  or a signature for a request is `approve_request`. It runs only for the
  Names app, only on a pending request, and only with an account from that
  request's own `choices`. For a call, the account must also be one the names
  wallet holds when the person approves. A rejected, cancelled or expired
  request can never be approved. A failed call is never retried, because on
  LEZ 0.4 a failed transaction is still charged, and a send that timed out may
  still have gone through.
- **Private state is shown only with approval.** An app sees only the
  accounts the person ticked for it. It gets a private account's balance only
  if the person gave it that private account. A failed payment tells the app
  only that it failed.
- **Keys never leave their wallet.** Payments and calls both run in the names
  CLI, which signs with its own keys. The names module never holds a key for
  funds: an inscription's fee is paid by the person's node from its own
  keystore. The module holds only the inscription keys (one per chain account,
  mode 0600), which sign what an account's channel says and can move nothing.
- **An inscription needs no grant, and gets nothing.** The app that asks
  learns nothing until the person approves; then it learns the one account
  that paid (or was to pay, when the outcome is `failed`). The words are shown whole before approval, an app whose publisher
  can't be verified needs an extra tick, and a request that failed after it
  was handed to the node (taken under another hash, or no answer in time) is
  never offered again: its notes count as spent and the account inscribes
  nothing else until it lands or 15 minutes pass.
- **Prompts can't be spoofed by text.** Your `reason` and `summary` lose
  control, bidi-override and zero-width characters. The app shows them as
  plain text, labelled as your words.
- **Bounded.** The queue has limits per app and in total, a per-hour cap,
  expiry, and size limits, so an app can't flood the person or the disk.
- **A private call is shown as it is, not as the app says.** Its sheet comes from the wallet's
  simulation; a debit beyond what the app declared is refused; the job proves only a call whose
  effects still equal the approved ones, starting from the states it simulated. What the privacy
  check covers: the public state and events the call leaves, not the bytes of messages between
  programs.
- **App secrets are one-way.** An app gets `SHA256(tag | m)`; `m` and the account's key stay in
  the names CLI's wallet.

## Compatibility

`propose_private_call`, `app_secret`, token sources and the private-call controls need the
`names` module and names CLI from the `bridge-sdk` branch (module 0.15.3 today; it would ship
as the next minor, so ask for `"version": ">=0.16.0"` once it is out; the version bump is left
to the release). Against an older module these
methods don't exist: check with a call and fall back to telling the person to update the Names
app.
