# Testimonial (Basecamp mini-app)

Post a testimonial for a λPrize submission on the [testimonial program](../../testimonial/README.md),
signed with your Names Wallet, and read a submission's testimonials with per-month counts.
It is the LP-0021 **Testimonial mini-app** and a worked example of the
[Wallet Provider SDK](../../docs/wallet/PROVIDER-SDK.md): everything that touches the wallet goes
through the `names` module's SDK, and the app never sees a key.

| | |
|---|---|
| Basecamp name | `testimonial_ui` (what the person sees in every Names prompt) |
| Needs | the `names` module `>=0.11.0` and the Names app open to approve |
| Defaults | submission `lp-0021:names-wallet`; sequencer = the one the Names Wallet uses (`get_config`) |
| Settings | sequencer, testimonial program ID (deployed when testnet 0.4 launches), submission ID |

## Worked example: connect → account selection → approval → transaction

All four steps are in [`src/qml/Main.qml`](src/qml/Main.qml); the bytes are built in
[`src/qml/logic.mjs`](src/qml/logic.mjs). Every `names` call answers
`{success, value, error}` as JSON text:

```qml
function names(method, args, done, failed) {
    logos.callModuleAsync("names", method, args, function(reply) {
        var r = JSON.parse(reply)
        if (r.success) done(r.value); else failed(r.error)
    }, 30000)
}
```

### 1. Connect

```qml
names("request_access", ["Post a testimonial signed by one of your public accounts"], function(v) {
    if (v.state === "approved") return readGranted()   // already connected
    follow(v, "connect")                               // poll request_status every 2 s
})
```

The Names app shows a sheet naming `testimonial_ui` and lets the person tick accounts. When
`request_status` says `approved`, the app reads what it was given:

```qml
names("granted_accounts", [], function(v) { granted = v.accounts })   // [{account, account_kind}]
```

### 2. Account selection (in the app, and it matters)

The app lists the granted **public** accounts and the person picks one. A private sender reaches
the program under a fresh alias per message, which the program refuses (one testimonial per
account would mean nothing otherwise), so private accounts aren't offered.

The pick matters because a testimonial lives at a PDA derived from its author:

```
head = pda(TST, sha256(pad32("testimonial/v1/head") ‖ sha256(submission)))
node = pda(TST, sha256(pad32("testimonial/v1/node") ‖ sha256(submission) ‖ author))
```

`propose_call` has to name `node` before the person chooses the signer in Names. So the app
fixes the author first, computes both seeds for that account, names it in the summary and in
the waiting screen ("Choose Public/… to sign"), and the person picks the same account in the
approval sheet. If they pick another, the wallet's dry run refuses the call before anything is
sent or charged (tested live, below).

### 3. Approval: `propose_call`

Before proposing, the app reads the author's `node` account (one testimonial per account: a
second one is refused here, not by a failed transaction) and the chain's clock, then builds the
call exactly as `testimonial/core` expects:

```js
// logic.mjs
var call = Logic.postCall(program, submission, username, text, now, author)
// call.instruction_hex = hex(borsh(Message::Instruction(Post { submission, username, text, now })))
//                      = 00 00 | u32 len + submission | 00, or 01 + u32 len + username | u32 len + text | u64 now
// call.accounts_json   = [{"seed":"<head seed>"},{"seed":"<node seed>"}]
```

```qml
names("propose_call", [call.program, call.accounts_json, call.instruction_hex,
                       Logic.postSummary(submission, author, text)], function(v) { follow(v, "call") })
```

`accounts_json` follows the SDK: the signer's own actor of the program comes first and is not
listed; then the submission's head and the author's testimonial account, each as `{"seed": hex}`
(the program's own accounts, made if new). There is no `signer` balance entry, so the sheet says
the call can't move the account's LEZ and only the fee is paid.

`now` is the chain's last block time, read from the clock program's account
(`getAccountView` on `4BdcjoXk…RWNU` under `2hwaRW8s…157Z`, both checked against Rust), or this
computer's time minus a minute if that read fails. The program only lets the post into a block
in `[now, now + 10 min)`, so the waiting screen counts that window down; a request also expires
after 10 minutes.

### 4. The transaction

```qml
Timer {
    id: poll; interval: 2000; repeat: true
    onTriggered: names("request_status", [requestId], function(v) {
        if (v.state === "pending" || v.state === "running") return
        poll.stop()
        if (v.state === "done") showTx(v.tx)     // included and not reverted
        else showWhy(v.state, v.error)           // failed / rejected / expired / cancelled
    })
}
```

`done` carries `tx`, the transaction hash, and the app shows it with the testimonial's address.
A `failed` call is never retried for the person: on LEZ 0.4 a sent call that fails still pays
its fee.

## The other target: an inscription on the Logos Blockchain (LP-0022)

"Where to write it" has a second choice, **Logos Blockchain**. It needs no connection, no
program ID and no username: the app calls `propose_inscription(text)` and follows the request.

```qml
names("propose_inscription", [body], function(v) { follow(v, "inscribe") })
```

In the Names app the person sees the words, that an inscription is public and permanent, the
accounts their own Logos Blockchain node holds, and the fee their node asks from the account
they tick; approving inscribes `{"submission":"lp-0022:names-wallet","text":"…"}` on that
account's own channel. `done` carries the account that paid, the transaction hash, the fee
paid and the channel, which the app shows. `failed` means it may still have been sent, so the
app says to look at the Names app's history before asking again. The text rules are the
wallet's (1 to 1,024 bytes, no control characters but newline and tab, and no character that
turns the text around or shows nothing) and `Logic.inscriptionProblem` checks the same ones
before asking. While the wallet sends an approved inscription the names module answers nothing
else for a minute or more; the app keeps asking for the status instead of giving up.

Inscriptions aren't listed in this app: Basecamp gives it no way to read the node, and each
account has its own channel. The transaction shows in the Names app's history.

## Reading the list

The list is read with plain sequencer reads, as `testimonial list` does: the head, then each
testimonial through its `prev` link, newest first, with the same checks as
`testimonial_core::walk` (each record at its own author's address, so each account appears once;
numbered `count-1` down to 0; no gaps). The app shows the count, the distinct accounts, the
per-month counts (UTC, by each testimonial's on-chain time) and whether the LP-0021 adoption bar
(150 from 150 accounts, 2+ months with 30+) is met.

**Why this app has a C++ backend.** Basecamp gives a `ui_qml` view no network: its QML engine
gets a deny-all network access manager (`logos-basecamp/app/restricted/QmlSandbox.cpp`), so
`XMLHttpRequest` can't reach a sequencer. The app therefore has a small universal backend
([`src/testimonial_ui.rep`](src/testimonial_ui.rep),
[`src/testimonial_ui_backend.cpp`](src/testimonial_ui_backend.cpp)) with one slot,
`accountView(sequencer, account, program)`, which posts `getAccountView` (http/https only, no
redirects, 15 s timeout, 256 KB cap) and returns the result. It holds no key and calls no
module. The QML calls it with `logos.watch(backend.accountView(…), ok, failed)`.

## Build and test

```sh
node apps/testimonial/logic.test.mjs                      # JS vs Rust vectors, the walk, limits, QML reserved words
cargo test -p testimonial_core --test js_vectors          # the vectors file vs testimonial_core
# on the Mac mini (nix):
cd apps/testimonial && nix build .#lgx-portable --accept-flake-config \
  --override-input names path:/abs/path/to/basecamp-module   # only when the copy isn't a git checkout
```

`python3 apps/testimonial/tools/render.py <qml binary> <out dir>` renders the page offscreen with
a stand-in for the Logos bridge (both targets, and an inscription waiting, done, failed, rejected
and refused), with the Logos design system's `qml` folder on `QML_IMPORT_PATH`.

`test/vectors.json` is written by `testimonial/core/tests/js_vectors.rs`
(`WRITE_VECTORS=1 cargo test -p testimonial_core --test js_vectors`) and checked by both that
test and the Node test: three cases (leading-zero IDs, no username, Unicode, a text over 255
bytes), each with the message hex, both seeds, both addresses, `accounts_json`, and a `Head` and
`Testimonial` record to decode.

Results (2026-11-03): Node test all pass; `cargo test -p testimonial_core` 11 + 1 pass;
`nix build .#lgx-portable` builds `logos-testimonial_ui-module.lgx` (plugin, replica factory, QML,
icon; links QtNetwork through `@rpath` like the official LEZ Wallet UI). The QML was rendered
offscreen with Qt 6.9.2 and a stand-in bridge in seven states (start, connected with a list,
pending, done, failed, settings, and a real `post()` run through the duplicate check) with no
QML warnings ([`../tools/render.py`](../tools/render.py)).

**Live, on a throwaway LEZ v0.4.0-rc1 sequencer** on 127.0.0.1 (stopped afterwards): the
testimonial program deployed through the loader; a `logosctl` session with this branch's
`names` module (pointed at the loopback chain and a scratch wallet) played both sides, as the
host is allowed to: the app's calls and the Names app's approvals. The payload came from this
app's `logic.mjs` ([`../tools/live.mjs`](../tools/live.mjs), [`../tools/live-flow.py`](../tools/live-flow.py)):

```
ok   request_access queues a connect request
ok   connect approved with F and B
ok   granted_accounts
     payload: now=1793747200587 (chain 1793747200587) instruction 96 bytes
     effects: … can't move the LEZ in the account you choose; only the fee is paid from it.
              Reads or writes …'s own account Public/BvhN5W… (made if it is new). (×2)
     approve_request -> state done (12s)
ok   post from F approved with F is done   tx e4493951…bfd6
ok   the app's list reads it back
     approve_request refused: the names CLI failed: Error: refused: Program violated execution rules
ok   a signer other than the picked account fails
ok   ... and nothing was sent (F's nonce unchanged)   nonce 7 -> 7
ok   post from B approved with B is done   tx 5b0585ed…f47c
ok   the list has both, newest first, per-month counts
ok   a second post from F is caught before proposing
ALL PASS
```

`testimonial list` (the Rust client) read the same two testimonials back.

Not tested inside a running Basecamp: the backend slot and `logos.watch` were exercised only
through the stand-in bridge (the Mini's Basecamp is the user's and is never restarted).
