# Names Wallet: the Wallet Provider SDK

Any Logos Basecamp app can use the person's Names Wallet through the `names`
core module: ask to see accounts, read their balances, and propose LEZ
payments and program calls. The person approves every one in the Names app.
The app never sees a key, and nothing is signed or sent until the person
approves.

This page is the API reference, with a worked example of
connect → account selection → approval → transaction in QML and in C++.

## How it works

```
your app ──propose──▶ names module ──queue──▶ Names app (approval sheet)
    ▲                       │                        │
    └──── request_status ◀──┴──── approve / reject ◀─┘
                            │
                            ├─ transfer: names CLI job (`names pay`, `auth-transfer send`)
                            └─ call:     names CLI (`names module send`) tries, signs, sends
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

Accounts are written the way the LEZ wallet writes them: `Public/<base58 id>`
or `Private/<base58 id>`. Amounts are decimal text in base units, because
balances are 128-bit numbers.

## Methods for any app

| Method | What it does | Value |
|---|---|---|
| `request_access(reason)` | Asks to see some of the person's accounts. `reason` (up to 280 characters) is shown as your own words. If you already have access, nobody is asked. If a request is already waiting, you get that one back. | a request (below); if you're already connected, `{request: null, state: "approved", accounts}` |
| `granted_accounts()` | The accounts you were given. Asks nobody. | `{connected, accounts: [{account, account_kind}]}`; `account_kind` is `public` or `private` |
| `account_balances()` | The LEZ balance of each account you were given. Reads run in the background and the answer comes at once with the last balance read. Ask again for a newer one; the chain is read at most every 15 s. | `[{account, account_kind, balance, updating, error}]`; `balance` is decimal text, or null until first read |
| `propose_transfer(asset, amount, destination)` | Asks to send `amount` of `asset` to `destination`. `asset` is `LEZ`, the native token (empty also means LEZ), or a token of the wallet's token program, written as its definition's account ID in base58 (see [Token transfers](#token-transfers)). `destination` is a name such as `alice`, or `Public/<id>`. The person picks the account it comes from. Checked when you call it. | a request |
| `propose_call(program_id, accounts_json, instruction_hex, summary)` | Asks to run a program, signed by an account the person picks (details below). | a request |
| `request_status(request_id)` | How your request stands. Only the app that made a request can read it. | a request |
| `cancel_request(request_id)` | Withdraws your request while it's pending. | a request, `state: "cancelled"` |
| `drop_access()` | Gives up the access you were given. | `{revoked}` |

You must call `request_access` before you propose anything. Otherwise
`propose_*` fails with `ask for access first (request_access), then propose it`.

### A request

```json
{
  "request": "9f3c…(32 hex)",
  "kind": "connect | transfer | call",
  "state": "pending | approved | running | done | failed | rejected | expired | cancelled",
  "created": 1793735537880, "expires": 1793736137880, "decided": null,
  "account": "Public/…",          // the account the person chose (transfer, call)
  "accounts": ["Public/…"],       // connect: all the accounts you now have
  "asset": "LEZ", "asset_kind": "native", "amount": "1000", "destination": "alice",   // transfer
  "token_name": null,             // transfer of a token: its definition's name
  "program": "<base58>",          // call
  "tx": "<hash>",                 // call: the transaction sent
  "error": null                   // failed: why, without the wallet's details
}
```

| `state` | Meaning |
|---|---|
| `pending` | Waiting for the person |
| `approved` | Access given (`connect`) |
| `running` | A transfer is being proven and sent: about 4 minutes from a private account, one block from a public one |
| `done` | Included in a block and not reverted. A call also gives `tx`, its hash |
| `failed` | `error` says why, in general terms. The wallet's own message, which may mention the person's balances, goes only to the Names app |
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

Requests and grants are kept in `~/.config/logos-names/provider.json`. The
file is mode 0600 and written atomically, so a grant survives a restart and a
crash never leaves a half-written file.

## Methods for the Names app

Only the Names app (`names_ui`) and the host may call these. Any other app
gets `only the Names app can do this`.

| Method | Value |
|---|---|
| `pending_requests()` | the waiting requests, oldest first, with everything the prompt shows and `choices`: `[{account, account_kind, pays, label, granted}]`. A payment comes from the wallet's main public or private account (`pays`). A call can be signed by any of its public accounts; those beyond the main one are read in the background and can appear a few seconds later |
| `approve_request(request_id, account)` | approves with one of the request's `choices` (for `connect`, one or more separated by commas). A transfer starts as a CLI job, and its `job` is in the value. If the wallet is busy, the request stays pending and the error says so |
| `reject_request(request_id)` | the rejected request |
| `app_grants()` | `[{app, accounts, since}]` |
| `revoke_app(app)` | `{revoked}` |

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

The same flow applies to a contract call. For example, take a testimonial
program that stores each testimonial in an account of its own, derived from a
seed. The person's account signs and pays only the fee, so the call doesn't
list `signer`:

```qml
names("propose_call", [
    testimonialProgramId,                                  // base58
    JSON.stringify([ { seed: testimonialSeedHex } ]),      // the program's account for this testimonial
    messageHex,                                            // borsh(Message::Instruction(..)) of your program
    "Post a testimonial: “Names Wallet is great”"
], function(v) { requestId = v.request; poll.start() })
```

`state` becomes `done` with `tx` once the call is in a block and has not
reverted. A call the program refuses fails when the CLI tries it, before
anything is sent.

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

## Security model

- **Identity is the caller, not an argument.** `current_caller()` names the
  calling module. A caller that can't be named is refused, and so is a module
  that names itself `host`.
- **Only the Names app decides.** `pending_requests`, `approve_request`,
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
  CLI, which signs with its own keys. The names module never holds a key.
- **Prompts can't be spoofed by text.** Your `reason` and `summary` lose
  control, bidi-override and zero-width characters. The app shows them as
  plain text, labelled as your words.
- **Bounded.** The queue has limits per app and in total, a per-hour cap,
  expiry, and size limits, so an app can't flood the person or the disk.
