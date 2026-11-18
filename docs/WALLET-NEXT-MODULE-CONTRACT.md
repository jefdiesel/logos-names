# What the Names app needs from the LEZ Bridge module (wallet-next, 2026-11-18)

For the bridge-module worker. The Names app (branch `wallet-next-1118`, `basecamp/qml/`) already has the
screens for these calls, built against stand-ins: Bridge to Ethereum's address step with saved addresses,
and Remove on the card's "Arriving from Ethereum" and "Bridging to Ethereum" rows (rfp-022
`proposal/WALLET-ROADMAP.md` items 1, 5, 7 and 8). The module has none of these calls yet. Everything below
is **designed only** (state 3); nothing here has run against a module or a chain.

The app reaches every call through one block, `PRIVATE_BRIDGE` in `basecamp/qml/logic.mjs`. It offers a
call only when `config().features` names its feature word. With an older module the screens fall back to
what exists today: `send_back_recipients` instead of the address book, and no Remove.

## Conventions (unchanged)

- Called as `logos.callModuleAsync("lez_bridge", method, [strings…], cb, timeoutMs)`. Every argument is a
  string. Every reply is `{success, value, error}`, encoded twice under Basecamp 0.3.2.
- `{success: false, busy: true}` means nothing was done. The app asks again after 2, 4 and 6 s.
- No reply carries a secret. None of these calls asks the Names Wallet for anything, signs anything or
  sends anything.
- None of these calls is polled. Each runs once per press, or once when a screen opens.

| Feature word | Calls |
|---|---|
| `address_book` | `address_book_list`, `address_book_save`, `address_book_remove` |
| `receive_remove` | `receive_remove` |
| `send_back_remove` | `send_back_remove` |

## 1. The address book

### What it holds

- **Scope: this wallet on this computer,** one list in the module's data directory
  (`<module data>/address_book.json`, mode 600). It is shared by every LEZ account, unlike
  `send_back_recipients`, which is per account.
- **Never on chain, never sent anywhere.** A nickname appears only in these three replies.
- **Each entry:** `address` (EIP-55 checksummed), `nickname`, `saved_at` (unix seconds), `last_used`
  (unix seconds of the newest send-back to it, or null).
- **At most 50 saved entries.**

### The list the app shows

- **Saved entries** (with a name), plus every address in `send_back_recipients` for any account that has no
  saved entry. The second kind come back with `nickname: ""` and `saved: false`; the app shows them as
  "Sent to before".
- **Order:** `last_used` newest first; never-used saved entries after them, by `saved_at` newest first.

### The four words

Each entry's words are the withdraw-to compare words for that address on the **deployment new send-backs
use now** (`config().deployment`):

- the payload `version 1 | chainId | vault | address | relayFeeChoice 0`;
- the same words `send_back_check(address)` gives for it.

The words depend on the vault, so compute them on every `address_book_list` call. Never store them, and
never carry them across a bridge change. The reply names the deployment they were made for.

### `address_book_list()`

Read only, no chain read. Reply:

```json
{ "addresses": [ { "address": "0x68B54942A37aE5ceBA34D47eFB8d33a293d3975d", "short": "0x68B5…975d",
                   "nickname": "main", "words": ["lens","mushroom","enjoy","orient"],
                   "words_text": "lens · mushroom · enjoy · orient",
                   "last_used": 1795000000, "saved_at": 1794900000, "saved": true } ],
  "deployment": "bridge-test-network-v5", "chain_id": 11155111, "max": 50 }
```

### `address_book_save(address, nickname)`

Write, local only.

**What it accepts:**
- **The address:** the same rules as `send_back_check`'s address route (NAMES-SEND-BACK.md §3.1a).
  - It must be EIP-55 checksummed: `bad_address:no_checksum`, `bad_address:bad_checksum`.
  - It must be `0x` + 40 hex digits: `bad_address:address`.
  - It must not be the zero address or the vault: `bad_address:recipient`.
- **The nickname:** trimmed, 1 to 24 characters, only `[A-Za-z0-9 ._-]` (`bad_nickname`).
- **One address per nickname,** compared without case: another address with that name is refused
  (`nickname_taken`).

**What it does:**
- Saving an address that is already saved renames it.
- A 51st entry is refused (`full`).

**Reply:** `{ok: true, entry: {address, nickname, …}, addresses: [… as address_book_list …]}`. A refusal is
`{ok: false, error: {code, message}}`, and nothing changes.

### `address_book_remove(address)`

Write, local only.

- Drops the saved entry. Also forgets the address from `send_back_recipients` for every account, so it is
  no longer listed at all (the person pressed Remove on it).
- Reply: `{ok: true, removed: bool, addresses: [...]}`. An address that wasn't there gives
  `removed: false` and is not an error.

### What the app does with it (already built)

- **Step 2 of Bridge to Ethereum is one box:**
  - It is always shown and empty at first; nothing is pre-filled.
  - The entries are listed under it as "Choose a previous address", showing name, short address and four
    words.
  - Picking an entry fills the box. The app then calls `send_back_check(address)` as today and shows the
    full address and its words. Nothing goes ahead without the tick "This is my address".
- **After a send-back's burn has gone out** to an address with no name: "Save to my addresses" with a short
  name, which calls `address_book_save`.
- **Settings → Bridge:** lists the entries with Remove, which calls `address_book_remove`.

## 2. `receive_remove(id)`: remove a receive row

**The safety rule the module enforces:** a receive is removed only when removing it cannot lose money.
The module checks this **itself, at the call**:
- It never relies on an earlier `receive_status`.
- It never relies on anything the app says.
- The check and the removal happen in the one call (the module runs `concurrency: single`), so there is
  no gap between them.

It removes the row in exactly two cases:

1. **`no_deposit`**: nothing was deposited for the row's commitment `C`. All of these must hold, on the
   **row's own deployment** (its vault, not the current one), read through the module's Sepolia RPC list
   at `latest`, not `finalized`, so that a fresh deposit counts:
   - the vault's `commitmentUsed(C)` is false;
   - no `Deposit` log carries `C`, and the logs read are complete: their count equals the vault's
     `depositCount()` at the same block, as every reader does (`check_deposits_complete`).
2. **`claimed`**: the deposit was claimed. The claim nullifier's PDA is spent on the row's LEZ endpoint
   (a fresh read, as `receive_status` makes), or this module recorded the claim's block itself.

Every other case **keeps the row** and says why:

| `code` | When |
|---|---|
| `deposited` | `C` is in the vault and the deposit is not claimed, whatever its stage (seen, final, claimable, claimable with the relay moved on). Never removed. |
| `unknown` | the row has no deployment (an older build, `action: "recover"`), so nothing can be checked |
| `unreachable` | any read failed, timed out, answered 429 past its retries, or came back short (logs vs `depositCount`) |
| `not_found` | there is no such row (already removed); the app treats it as gone |

**What removal does and doesn't do:**
- **It never frees the key index.** The row's `key_index` stays used for its (account, deployment, token,
  denomination), and the next request takes the next one, as today.
- **It keeps a tombstone** `{id, account, deployment, token, denom_index, key_index, commitment,
  removed_at}` in `positions.json`.
  - `recover(account, …)` still finds a deposit made to a removed code later (someone opened the old link)
    and brings the row back.
  - `receives()` does not list tombstones.
- **It doesn't touch the Names Wallet.** No `app_secret` call; no secret is needed.

**Reply** (always a success; `removed` says what happened):

```json
{ "removed": true, "id": "dep-…", "code": "no_deposit",
  "text": "No deposit was made for this code. Removed.",
  "checked": { "deployment": "bridge-test-network-v4", "vault": "0x…", "commitment": "0x…",
               "deposit_found": false, "claimed": false, "block": 12345678, "reads": { "sepolia": 3, "lez": 0 } } }
```

`text` is optional. The app has its own words for each `code` (`logic.mjs` `removeReply`) and shows
`text` when it is given.

**Known limit:** a deposit sent in the seconds before the press, and not yet in a block, is not seen. The
tombstone and `recover` cover it, and the app's line after a removal names no time.

## 3. `send_back_remove(id)`: remove a send-back row

**The safety rule:** removed only when no burn for this exit exists or can still appear, or the release
is done. The module checks this at the call, the same way as for receives. It removes the row in exactly
two cases:

1. **`released`**: `releaseNullifierSpent(nf_r)` is true on the row's own vault (a fresh Sepolia read).
2. **`never_burned`**: all of these hold.
   - **Nothing sent:** no `burn_tx` is recorded and no relay hand-over (`send_back_submit`) was made.
   - **Nothing with the wallet:**
     - a private send-back's `wallet_job` has ended (rejected, lapsed, cancelled, failed), asked of the
       wallet once now; an unanswered ask refuses with `unreachable`;
     - a public send-back has no proposal outstanding or unanswered: no `relay_uncertain` for it, and no
       pending `propose_relayed_call` request.
   - **Nothing on LEZ:** a fresh scan of the row's LEZ endpoint, from the block the plan was made at up to
     the tip, finds no transaction carrying the 32 bytes of `E` (the scan `send_back_status` and
     `send_back_burn` already make).

Every other case **keeps the row**:

| `code` | When |
|---|---|
| `in_flight` | a burn was sent or found (`burning`, `burned`, `burn_unclear`, `recorded`, `proving`, `proved`, `wrapped`, `releasable`), or a settlement runs (`settle_id`) |
| `with_wallet` | a burn proposal is waiting in the Names Wallet, or a proposal got no answer (`relay_uncertain`) |
| `unreachable` | a read failed, timed out, or the LEZ scan did not reach the tip |
| `not_found` | no such send-back |

**What removal does and doesn't do:**
- **It never reuses the exit index.** The counter for (account, context) is unchanged, so a later plan
  never makes the same `E` or `nf_r`.
- **It keeps a tombstone** `{id, account, deployment, token, denom_index, key_index, exit_commitment, nf_r,
  recipient, removed_at}`. A burn of that exit that later appears (it can't, by the rule above, but the
  rule is the module's to keep) can then be followed again.
- **It doesn't change the address book** or `send_back_recipients`.

**Reply:** the same shape as `receive_remove`. `checked` holds `{deployment, exit_commitment, burn_found,
wallet_job_state, released, lez_tip, reads}`.

## 4. Calls the address step uses that already exist (no change asked)

- `send_back_check(code_or_address)`. The app now binds its answer to the exact value in the box. A check
  answered for a value no longer there is dropped and the box is checked again.
- `send_back_plan(token, denom_index, account, code)`. The app calls it only after the tick, and only for
  the code `send_back_check` gave for the box's value now.
- `send_back_recipients(account)`. This is the fallback list when `address_book` is not in `features`.

## 5. Tests the module should carry

- **Address book:**
  - save, rename, `nickname_taken` (case-insensitive), `full` at 51, every `bad_address:*` and
    `bad_nickname`;
  - remove drops the address from `send_back_recipients` too;
  - the list merges recipients without a name;
  - the words follow `bridge_use` (the same address gives v4's words, then v5's);
  - the file is mode 600.
- **`receive_remove`:**
  - removed with no deposit;
  - kept when `commitmentUsed` is true but no log is found yet (`deposited`), and when the log count is
    short (`unreachable`);
  - removed once claimed;
  - kept with `unknown` for a row with no deployment;
  - the key index is not reused after removal (the next request takes index + 1);
  - `recover` brings a removed row back when a deposit appears;
  - read on the row's deployment after `bridge_use` moved on.
- **`send_back_remove`:**
  - removed when planned and never proposed;
  - kept with `with_wallet` while a wallet job runs, and after an unanswered relayed proposal;
  - kept with `in_flight` when the LEZ scan finds `E`, though nothing was recorded locally (the burn sent
    from another install);
  - removed once released;
  - the exit index is not reused.
- **Busy:** a call while another waits on Names answers busy and removes nothing.

## 6. Where this lives in the Names app

- **The adapter:** `basecamp/qml/logic.mjs` `PRIVATE_BRIDGE` and the feature words `PRIVATE_FEATURES`
  (`address_book`, `receive_remove`, `send_back_remove`).
- **The rules the screens follow:** `addressStep`, `addressBookRows`, `nicknameProblem`,
  `addressSaveOffered`, `addressSaveReply`, `removeOffered`, `removeReply`.
  - `removeOffered` only hides Remove where removing is clearly unsafe. The module's check is the
    authority.
- **Tests:** `basecamp/logic.test.mjs` (the wallet-next block).
- **Renders:** `basecamp/tools/render.py`, scenarios `sendback-address-*`, `sendback-approve-next`,
  `sendback-save*`, `card-remove*`, `settings-bridge-book`. Their stand-in answers follow this document.
