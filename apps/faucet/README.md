# Faucet (Basecamp mini-app)

Free LEZ testnet tokens, paid to an account the person picks from their Names Wallet. It is the
LP-0021 **Faucet mini-app** and a worked example of the
[Wallet Provider SDK](../../docs/wallet/PROVIDER-SDK.md): the wallet side (connect, account
selection, balances) goes through the `names` module's SDK, and the app never sees a key.

| | |
|---|---|
| Basecamp name | `faucet_ui` |
| Needs | the `names` module `>=0.11.0` and the Names app open to approve access |
| Default faucet | `https://lez.buggers.online/faucet`, the Names faucet ([`chain/faucet.py`](../../chain/faucet.py)), the same one the module's `cli_faucet` uses |
| Settings | the faucet URL (https, or http only on this computer) |

## An optional, disclosed service

The prize forbids undisclosed third-party services, so before anything is sent the app says,
in its first panel, which service it contacts (the URL and host, and who runs the default
one), that it sends only the chosen account ID, that the service sees the person's internet
address, and that nothing is sent until they press the button. It also says the faucet is not
needed: tokens can be bridged from the Logos Blockchain in the Names app, or someone can pay
the account.

The faucet target may change (lambda-prize PR #181 proposes an "open funding path"), so the URL
is a setting. Any faucet that speaks the same protocol works:

```
POST <url>  {"account": "<base58 public account ID>"}
  200 {"account", "amount", "tx"}                           sent (arrives within a block or two)
  4xx/5xx {"error": "<reason>"}  [Retry-After: <s|date>]    refused
GET  <url>  {"amount", "per_account"}                       what it gives (shown in the app)
```

## Worked example: connect → account selection → request → result

Asking a faucet signs nothing, so the person's approval here is the access grant: which
accounts this app may see. The steps are in [`src/qml/Main.qml`](src/qml/Main.qml).

### 1. Connect

```qml
names("request_access", ["Choose an account to receive free testnet tokens"], function(v) {
    if (v.state === "approved") return readGranted()
    requestId = v.request; poll.start()          // request_status every 2 s until approved
})
```

### 2. Account selection

```qml
names("granted_accounts", [], function(v) { granted = v.accounts })
names("account_balances", [], function(v) { balances = v })   // re-asked every 15 s, no faster
```

Only granted **public** accounts are offered (`Logic.fundable`): the faucet sends a public
transfer. A granted private account gets a note: have a public one paid, then move tokens to
private in the Names app.

### 3. Request

The QML can't reach the network (Basecamp's QML sandbox has a deny-all network access manager),
so the app's small backend ([`src/faucet_ui.rep`](src/faucet_ui.rep),
[`src/faucet_ui_backend.cpp`](src/faucet_ui_backend.cpp)) makes the HTTP call. It sends only
`{"account": "<id>"}`, refuses anything but https (or http to loopback), follows no redirects,
times out after a minute, and returns `{status, body, retry_after}`.

```qml
logos.watch(backend.requestFunds(faucetUrl, Logic.bare(chosen)), function(text) {
    var o = Logic.interpret(JSON.parse(text), Date.now(), lastGrant[chosen] || 0)
    // o = {ok, amount, tx, message, retryAt, retryText, final}
})
```

### 4. Result, and rate limits

| Faucet answer | What the app says | Try again |
|---|---|---|
| 200 `{tx}` | "The faucet sent 2,000,000 LEZ…", the tx, then watches `account_balances` until the balance grows | the button waits 24 h for that account |
| 429 "funded in the last day" | once a day per account | a day after the last grant this app saw, else "about a day after its last grant" |
| 429 "…from your address today" | once a day per internet address | "about a day after that grant" |
| 429 "daily limit" | all given out today | "in a few hours, or tomorrow" |
| `Retry-After` header (any status) | | exactly then; the button counts down |
| 409 busy / can't reach the chain, 422, 502-504 | the faucet's words | in a minute |
| 409 "empty" | suggests bridging instead | (final) |
| 400, 404 | not a public account / no faucet there | (final) |
| unreachable | check the connection and the address | in a minute |

The Names faucet sends no `Retry-After`, so its reasons are matched by text; the faucet only
remembers a grant once the tokens arrive, and says so if they never do.

## Build and test

```sh
node apps/faucet/logic.test.mjs        # URL policy, accounts, every faucet answer, waits, QML reserved words
# on the Mac mini (nix):
cd apps/faucet && nix build .#lgx-portable --accept-flake-config \
  --override-input names path:/abs/path/to/basecamp-module   # only when the copy isn't a git checkout
```

Results (2026-11-03): Node test all pass; `nix build .#lgx-portable` builds
`logos-faucet_ui-module.lgx`. The QML rendered offscreen (Qt 6.9.2, stand-in bridge) in six
states (start, connected, sent, limited, busy, settings with a refused URL) with no QML
warnings ([`../tools/render.py`](../tools/render.py)).

**Live against the real faucet code, locally:** `chain/faucet.py` ran on 127.0.0.1:3041 over a
throwaway rc1 sequencer (its state in a scratch home; stopped afterwards; the public faucet was
not called). Driving it exactly as the backend does and passing each answer through this app's
`Logic.interpret` ([`../tools/faucet-live.mjs`](../tools/faucet-live.mjs)):

```
describe {"amount":"2000000","per_account":"once a day"}
ask 0 C -> HTTP 200 {"amount":"2000000","tx":"109f6ee4…6436"}   app says: The faucet sent 2,000,000 LEZ. …
ask 1 C -> HTTP 409 busy sending                                app says: … | ask again in 1 min
ask 2 C -> HTTP 429 funded in the last day                      app says: … | ask again in 24 h (at 18:07)
ask 3 D -> HTTP 429 too many requests from your address today   app says: … | ask again about a day after that grant
```

C's balance on chain afterwards: 2,000,000. (The run first printed "in 23 h 60 min"; fixed and
covered by the Node test.)

Not tested inside a running Basecamp: the backend slot was exercised only through the stand-in
bridge; the same HTTP exchange was made from Node against the local faucet.
