# Names Wallet × RFP-022 bridge: design (branch `bridge-sdk`)

Status: **built on branch `bridge-sdk` (from `lez-0.4` at `e466cc8`), not pushed, not installed.**
What lets a mini-app, first of all the RFP-022 Ethereum ↔ LEZ bridge, use the person's private
account through the Wallet Provider SDK (LP-0021), and makes bridged tokens show up in the wallet.
The API reference is in [PROVIDER-SDK.md](PROVIDER-SDK.md) ("App secrets", "Private calls",
"Token sources"); this page says why it is built this way.

## 1. App secrets (`app_secret`)

**Problem.** The bridge keeps one secret per deposit and per exit. It must come back when the
wallet is restored from its phrase (RFP-022 Functionality #9), and the deposit master `s` must
never appear in any transaction (`research/failsafe-design.md` §2.1: a reclaim after a
wind-down needs `s`, a claim only `k = SHA256("RFP022/ck" ‖ s)`).

**Chosen: the master stays in the wallet; the app gets one-way images.**

```text
ikm   = the account's 32-byte secret (public: signing key; private: authorization secret key)
prk   = HMAC-SHA256("logos-names/app-secret/v1", ikm)
info  = "logos-names/app-secret/v1/info" | u8 len(app) | app | u8 len(scope) | scope
        | u16be len(context) | context | account id (32) | u64be index
m     = HMAC-SHA256(prk, info | 0x01)                 never leaves the wallet
value = SHA256(tag | m)                               what the app gets (tag: 1-32 bytes)
```

- `app` is `current_caller()`'s module name, never an argument, so `lez_bridge` can't ask for
  `other_app`'s values. The account's key is behind HMAC, so no value gives it.
- For the bridge: scope `RFP022/deposit`, context `cfgId | token | u256(denom)`, index `i`:
  `m` is the bridge's `s`, and asking with tag `RFP022/ck` gives exactly `k`. The app can
  compute commitments, nullifiers, the one-time opening and recovery scans from `k`; it never
  holds `s`. Exits: scope `RFP022/exit`, tag `RFP022/ek` gives `k'`.
- **Why this and not raw secrets:** a leaked `k` can't steal (a claim mints only to the account
  the commitment binds, and that account's proof is the wallet's); a leaked `s` could reclaim
  to any Ethereum address after a wind-down. So the one value that moves money without the
  wallet stays in it. Generality is kept: any app gets a deterministic, restorable PRF over its
  own (scope, context, index), which is what key-derivation clients need.
- **Rejected:** (a) raw `m` to the app: simplest, but hands out the reclaim credential;
  (b) wallet-side bridge operations ("derive the commitment", "build the claim"): no secret
  leaves, but the wallet would carry the bridge's formats, versioned with it, and still could
  not stop the app from seeing `k` because the claim message the app builds contains `k`;
  (c) per-app random secrets stored by the wallet: not restorable from the phrase.
- **Not given yet:** `m` itself. The future reclaim proof (G4) needs `s` as a witness; either the
  wallet proves it (an operation) or a separate, explicit "export" approval is added. Open
  question for the user.
- **Restore (verified).** LEZ 0.4 `Storage::restore` (`lez/wallet/src/storage/mod.rs:105`)
  rebuilds both key trees from `SeedHolder::from_mnemonic(phrase, "")`; children come from
  HMAC-SHA512 of the parent (`key_protocol/.../keys_public.rs:23,77`, `keys_private.rs:27,56`),
  the private account's `ask` from the tree node (`keys_private.rs:65`), and new accounts take
  the next free slot in order (`find_next_slot_layered`). So an account made again in the same
  order has the same secret. The CLI test `a_wallet_restored_from_its_phrase_derives_the_same_values`
  makes a wallet, restores a second from its phrase, makes the same accounts and checks equal
  values (and unequal for a third wallet). Imported keys are refused (not in the trees).
  Caveat: `names module restore` makes 16 public and **1** private account; a bridge user with
  a second private account must make it again after restoring (open item: a `--private N`).
- **Approval.** The first call per (app, account, scope) returns `{granted: false, request}`;
  the Names app shows what the app gets and can't do; approving stores the grant in
  `provider.json` (`secret_grants`). Settings can take one grant or all of an app's back
  (`revoke_secret`, `revoke_app`). Bounds: 64 values a call, 120 calls an hour, 16 grants per app.

## 2. Private calls (`propose_private_call`)

**Inputs** (one JSON): `programs` (≤ 4, the root's first), `accounts` (≤ 16 actors besides the
root: the person's own account `{"actor": "account", "program", "openings"}`, a public account,
or a PDA by seed), `message` (≤ 16 KiB), `openings` (declared one-time recipients, ≤ 4),
`fee_payer` (`account` or `wallet`), `declared` (`debits`, `credits`, `max_fee`) and
`reveal_account`. The account is a private one the app was given.

**Message limit: 16 KiB.** The v2 demo's leaf-1 `ClaimAttested` message measured **6,595 bytes**
(`scripts/bridge-claim-call.py` on the frozen Sepolia proof: 32-sibling path 1,028, header RLP
682, 9 account + 3 storage proof nodes). A mainnet proof runs to ~12 account and ~8 storage
nodes of ≤ 532 bytes: ~12.5 KB worst case. 16 KiB covers it; 4 waiting requests per app stay
under 128 KiB of hex in `provider.json`.

**What the wallet checks before the person sees anything** (names CLI
`module private-call plan`, `cli/src/private_call.rs`):

1. The wallet's own dry run of exactly the transaction it would prove
   (`send_tx_with_pre_check` with the LEZ wallet in dry-run mode). Gives the public cycles, the
   fee at head, the reserve and the fee payer; a call the circuit would refuse stops here.
2. An **open replay** for the effects: LEE's own public executor
   (`ValidatedStateDiff::from_fee_settlement_invocation`, which takes an authorization set
   instead of signatures) runs the same root call on a scratch `V03State` holding the programs,
   the public accounts as the chain has them, and the private account as the wallet holds it
   (with its waiting credits added). Programs see the same pre-state, message, sender and
   authorization either way, so the replay changes each account as the proof will.
   Asynchronous messages to the account or its declared one-time addresses stay as
   publications in the replay; the proof receives them, so their credits (native `Credit`,
   token `Credit`) are added. Anything else sent to the account, or a replay that fails, is
   refused ("the wallet can't explain this call").
3. The report: the account's LEZ and token balances before/after, the data it keeps for other
   programs, every public account's change, the one-time addresses used, the events, whether
   the account's ID appears in the public state or events left behind, the fee, and a digest of
   the effects (deltas, not absolute values).

The upstream API offers no way to run a program from outside `lee` with an observer
(`Program::handle_message` is `pub(crate)`, the wallet's `AccountManager` is private), so the
replay goes through the public executor. Rejected: a forked `lee` (pinned git tag; would drift).

**Refusal rules** (module `provider_private::review`, pure and golden-tested):
- a debit of any asset the wallet can value (LEZ, tokens of known token programs) beyond what
  the app declared for it, to the base unit;
- a change to data the account already keeps for a program the wallet can't value (new data
  for a program, e.g. the bridge's claimer actor, is shown, not refused);
- a fee that can reach more than the declared `max_fee`, or paid from another account when the
  app asked the account to pay its own;
- the account's ID in public state or events, unless the app declared `reveal_account` (then
  a warning and an extra tick);
- a simulation of another account, unreadable balances, unknown rows, no fee.

Warnings (an extra tick before Approve): programs this app hasn't used before, no Program
Registry or an unverified, changed or upgradeable program, credits that differ from what the
app declared.

**The sheet** (Names app), all from the simulation, the app's words apart:
```
lez_bridge asks for a private transaction from Private/2if1EJ…6HUC
What it does (simulated by your wallet)
 • Private/2if1EJ…6HUC receives 100 (100,000,000 base units) dUSDC (FBqNAw…LQZJ).
 • Private/2if1EJ…6HUC receives 20,000,000 LEZ.
 • Nothing leaves your account except the fee.
 • Fee: about 5,723,160 LEZ, at most 9,300,135, paid inside the proof from your account; …
Programs   (each: label · ID · source verification · new for this app / used before)
Public changes
 • Public/<config>: LEZ 100,000,000 → 80,000,000
 • dUSDC (FBqNAw…LQZJ): supply 100,000,000 → 200,000,000
 • Public/<config>: JECKBk…nUx7's data changes
Privacy
 • Public: the programs it runs (3), the public accounts it reaches and the changes above, the fee reserve, and the time.
 • Public: the one-time address B8UnZG…i3wT and what it receives. Nobody can tell it is yours.
 • Private: Private/2if1…, its balance, and the message to it (proven, not shown).
 • Checked: your account's ID appears nowhere in what becomes public.
Data: 6,595 bytes, proven, not published
Proving runs on this computer: about 11 minutes and about 9.5 GB … pause or cancel until it is sent.
The app describes it as: "…" (its own words, not checked)
```
(Golden test `the_claim_sheet_says_what_the_simulation_shows`; the live e2e numbers are in
§6.) Limit of the privacy line, said in the docs: the wallet checks the public state and events
the call leaves, not the bytes of messages between programs.

**After approval** a wallet job (`names module private-call send --input <0600 file>`) plans
again and **refuses unless the digest equals the one the person approved**, checks in the
wallet's pre-check hook that the proof starts from the simulated account states, then proves,
sends and waits for the block. Phases print `phase: plan|simulate|prove|submit|confirm`; the
module follows them (`request_status.phase`). Pause (SIGSTOP), resume and cancel (TERM, then
KILL) act on the job's process group until `submit` (`private_call_control`; an app's
`cancel_request` too). One wallet job at a time, as for every job.

**Abuse cases.** A mini-app asking to prove a drain: the replay shows the debit, the sheet
shows "pays N", and the rule refuses it unless the app declared it (then the person sees exactly
that). A call that behaves differently later: digest and pre-state checks. A call that sends
value to an alias of the account and then spends it: messages to the account are only accepted
if they are credits the wallet reads. Flooding: the provider's queue limits (4 waiting per app,
30 an hour), 4 programs, 16 actors, 16 KiB. Another app's secrets or private calls: refused
by caller identity; private calls only from accounts granted to that app.

## 3. Bridged tokens

**The two token programs are wire-compatible** (`rfp-022/lez-bridge/token/src/lib.rs` is a copy of
`wallet-tokens/core/src/lib.rs`): the same `Message` tags 0-8 and fields, the same
`TokenDefinition`/`TokenState` layout, holdings on the holder's own actor, the same `Transfer`
flow (root signer, async `Credit`), the same limits (name 64, 32 holdings). Differences:

| | Names `wallet-tokens` | Bridge token |
|---|---|---|
| `MintBy { to, amount }` (tag 9): mint by any actor of the authority program | — | yes (the bridge mints through it) |
| `BurnWithNotice { definition, amount, notify: Actor, memo }` (tag 10) and `BurnNotice` | — | yes (exit burns) |
| Definition authority | an account | the bridge's **program** id |
| Decimals, symbol | none | none in the token; the bridge's `Config.tokens[i].decimals` |
| Image | `wallet-tokens` guest | own image (`633555cc…`), immutable |

So nothing must change on the bridge side, and the wallet's own token code reads and moves
bridged tokens when pointed at the bridge token program. What the wallet adds:
- **Token sources** (`propose_token_source(program, definitions)`): an app registers a token
  program and its definitions with name, symbol, decimals and origin; the person approves; the
  holdings view lists them as "Bridged tokens" with decimals (`token_source_holdings`), and
  `cli_token_send_source` moves them through the existing `names token send` path.
- Not done (the wallet can't start a redeem): `BurnWithNotice` needs the exit commitment, which
  is the bridge's flow; the bridge mini-app proposes it as a private call.

## 4. LP-0022: the Ethereum route beside Logos Blockchain ↔ zone (design only)

Today (`lp22-merge`, merged in `lez-0.4`): `zones.rs` keeps `{name, channel, sequencer, wallet,
token_program}` per zone; the Blockchain tab's `chain_bridge` moves LGB to a zone (deposit by
channel), and `names zone` manages the CLI's own list.

- **A zone carries its bridges.** Add `bridges: [{kind: "evm", name, evm_chain_id, vault,
  bridge_program, attest_program, attest_instance, token_program, cfg_id, eth_rpc?}]` to
  `Zone` (`basecamp-module/rust-lib/src/zones.rs`, with the CLI's `cli/src/zone.rs` sharing the
  file). `cfg_id` is checked against the bridge's on-chain config
  (`SHA256(CFG_TAG | u256(chain) | vault | lezChainId | zoneId | programId)`), so a wrong vault or
  chain id is caught before any deposit.
- **Discovery by LNS name.** A bridge publishes text records on its name (SPEC §7 style):
  `evm-bridge` = `<evm_chain_id>:<vault>`, `evm-bridge-program`, `evm-bridge-attest`
  (`<program>:<instance>`), `evm-bridge-token-program`, `evm-bridge-cfg`. `names zone add-bridge
  <name>` (CLI) and Settings → Zones → Add bridge read them, check `cfg_id` on chain, and show
  the vault, chain, programs (with Program Registry labels) for approval.
- **UI.** The Blockchain tab's "Move to zone" sheet gets a sibling "Ethereum" route per zone
  with a bridge: deposit (opens the bridge mini-app with the zone and bridge preselected),
  claims waiting, exits. The wallet groups show "Bridged tokens" under the zone. The CLI gets
  `names zone bridges [ZONE]`.
- **Files:** `zones.rs` (struct, load/save, limits), `cli/src/zone.rs` (commands), module
  `lib.rs` (`zone_add_bridge`, `zone_bridges`, and token sources keyed by zone), `Main.qml`
  (Settings → Zones, Blockchain tab), `docs/wallet/PLAN.md` (LP-0022 table).
- Token sources and secret grants are per zone in that design (the same program id on another
  zone is another program); today's token sources are kept wallet-wide and say their program.

## 5. What the bridge side must adapt (for the workers on `rfp-022`)

1. **Secrets:** derive `s` through the wallet: `app_secret(V, "RFP022/deposit", hex(cfgId | token
   | u256(denom)), "RFP022/ck", i, n)` gives `k_i`; exits `("RFP022/exit", cfgId | token |
   u256(denom) | ethDest | u256(maxRelayFee), "RFP022/ek")`. Release-address keys: a tag such as
   `RFP022/release-key` over scope `RFP022/release`. These replace `client/src/kdf.rs`'s
   `AccountRoot` (which reads the account key directly) when the bridge runs under the Names
   wallet; deposits made with the old KDF or raw secrets keep their old path.
2. **The CLI's own wallet** (`lez-bridge` with `LEE_WALLET_HOME_DIR`) can reproduce the wallet's
   values (same formula, `app = "lez_bridge"`) for `recover` from the phrase; add a
   `--names-app lez_bridge` mode to `recover.rs`, or let the module do recovery scans with
   `app_secret` batches (64 a call).
3. **Claims:** the module builds the claim exactly as `bridge-claim-call.py` does (ClaimPlan's
   mentions, in that order, as `accounts` entries; PDAs by seed; the token and native actors of
   the account with `openings: true`; `fee_payer: "account"` when sponsored) and calls
   `propose_private_call(V, call, summary)`, then polls `request_status`. It never proves itself.
   It should pass `declared.credits` (the denomination) and `declared.max_fee`.
4. **Burns:** `BurnWithNotice` as a private call from V: programs `[token, bridge]`, root actor
   token, accounts: the definition (token program), the bridge config PDA (bridge program),
   `declared.debits = [{asset: <definition>, amount: <denom>}]`.
5. **Token source:** on first run, `propose_token_source(token_program, [{definition, name,
   symbol, decimals (from Config.tokens[i].decimals), origin: "Sepolia 0x…"}])`.
6. **Errors to map:** `check: failed` carries the CLI's local diagnosis only to the Names app;
   the app sees `check` and `error` in general terms (as for every provider request).
7. **`modules/CLI-CONTRACT.md` §0.4 (secrets on argv)** no longer applies to the wallet path:
   no secret goes on any argv; the call goes to the CLI in a 0600 file or on stdin.

## 6. Tests

See PROVIDER-SDK.md's compatibility note and the branch's commit messages for what ran where.
