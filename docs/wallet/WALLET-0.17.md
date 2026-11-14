# Names Wallet 0.17.0 for Basecamp (branch `wallet-next`)

One release candidate that joins the finished pieces. Module `names` 0.17.0, app `names_ui`
0.17.0, names CLI 0.17.0 (`names 0.17.0 (LEZ v0.4.0-rc3)`). Not pushed, not in the catalog; it
goes onto the user's Mac mini first (install plan below).

## What is in it

| Piece | How it came in |
|---|---|
| `rc3-port` fcc7486: LNS names on LEZ v0.4.0-rc3, reviewed; the eta icon | the base |
| The provider SDK, mini-apps, tokens, testimonial, accounts and gas, LP-0022 (zones, chain wallet, inscriptions) | already in `rc3-port` (`provider-sdk-0.4`, `tokens-0.4`, `names-tokens`, `module-tokens`, `mini-apps`, `testimonial-0.4`, `lp22/*`, `lp22-merge` are all ancestors) |
| `two-networks` d21a810: balances on LEZ 0.3 and LEZ 0.4, watch any public account, every view says its network | merged, no conflicts; it also brings `lez-0.4`'s 37f81a1 (the plain black eta) |
| `bridge-sdk` 3e8127f: private calls, app secrets, token sources | merged; `logic.mjs` and `render.py` conflicts kept both sides; one rc1→rc3 fix (rc3 returns the dry run's report instead of printing it) |
| The names network switch | new (below) |
| Calls someone else pays for (`declare_programs`, `propose_relayed_call`) | new; [SIGNING.md](../../basecamp-module/rust-lib/SIGNING.md) |
| Clock read on the public LEZ 0.3 | fix: since the rc3 port, settings and deposits didn't read there |

Not in: `pressure-fixes` (11 commits on `main`, partly ported to the 0.4 line already),
`snapshot-0.3`, `web-register(-04)`.

## The network switch (the site's rule)

- One switch, LEZ 0.3 · public Logos testnet | LEZ 0.4 · bridge test network. It moves the Wallet
  and the names together, so a balance, a name and a payment on one screen are on one chain.
- It is shown only where an action works on both networks: the Wallet, Names, Search and
  Register. Every other screen says the network it runs on ("Runs on …"), and an action that
  doesn't run on the network in use says so beside it.
- **LEZ 0.3** keeps config.json's flat fields exactly as before: registry
  `38jEfVV1…h2gq`, `https://testnet.lez.logos.co`, the CLI and wallet already set. A rollback
  reads them unchanged.
- **LEZ 0.4** keeps its settings under `lez04` in the same file:
  - registry `6Yxj5k8AiuBSuMAYVQFGrYKjJGUSSaWWgoB9D9XXN3gA`;
  - RPC `https://lez.nameswallet.app/` (Settings can set `http://127.0.0.1:3290` on the Mini);
  - its own CLI, which must say `(LEZ v0.4` in `--version`; a CLI found on the computer is
    never used;
  - its own wallet folder, `~/.config/logos-names/lez-0.4/wallet` (never LEZ 0.3's, never
    `~/.lee/wallet`);
  - its own jobs folder;
  - its home zone is called `bridge-0.4`, so its history and other apps' requests are never
    LEZ 0.3's.
- **Requests from other apps carry their network** and can be approved only on it.
- **The bridges are v2 and v3 by default.** With none set, the Wallet reads both:
  `JECKBkvoc2e3dC18Wm6zZJEC2BGMwUwLsdZiQzYTnUx7` (v2) and
  `4eaeK3yiqATXhpCEYEFMMtjUn1bS7HLxPwfrH31mXCvr` (v3, the fixed bridge). Their token programs
  (`3gF78HRJ…`, `5w6nTkft…`) are built in as refused roots. Balance rows say "bridge v2" or
  "bridge v3" (a label built into the module by program ID); another bridge shows its short ID.
- **On LEZ 0.4 a name is registered public-only**: no private account is made and no private
  receiving keys are published (private payments are off there).
- **An app asking for access with no wallet on the network** gets a plain refusal ("the Names
  wallet has no wallet on LEZ 0.4 yet…"), never a request left waiting behind the Welcome screen.
- **The bridges are a setting, not a build constant, and there may be several.** The bridge
  programs the Wallet reads on LEZ 0.4 (each Config lists that bridge's tokens) live in
  config.json under `lez04.bridge_programs`, a list of at most 4. The first is also written as
  `lez04.bridge_program`, which an earlier 0.17 build reads; a file with only
  `lez04.bridge_program` reads as a list of one. With none set, the default is
  `JECKBkvoc2e3dC18Wm6zZJEC2BGMwUwLsdZiQzYTnUx7`, today's.
  - Set the whole list with the module's `set_bridge_programs(<JSON array>)` (adding or removing
    a bridge is sending the new list; `[]` goes back to the default), or one bridge with
    `set_bridge_program(<id>)`: `logosctl call names set_bridge_programs str:'["<id>","<id>"]'`
    in a session. An install script writes `lez04.bridge_programs` into config.json itself.
    Names app or host only.
  - The balance view shows every bridge's tokens, each row naming its bridge when there are
    several; it is still one request per account. Every bridge's token program is refused as a
    relayed call's root: built in for the default bridge, kept once read, and read again at
    every check (SIGNING.md).
  - The read goes to LEZ 0.4's configured sequencer (`lez04.sequencer`). The module holds no
    vault and no relay endpoint: those are the bridge app's.

## What works where

Evidence from the Mac mini on 2026-11-14. "Run" means against the real network named; "tests"
means unit or flow tests with stand-ins; "untested" means not run in this release.

| Action | the public 0.3 (LEZ 0.3) | our 0.4 (LEZ 0.4) |
|---|---|---|
| Look up a name (resolve, recipient, page) | works: run (`jef`, `logos`) | works: run (`lns-smoke-ourzerofour`, through the public URL and 3290) |
| Registry settings, deposit quote, locked names | works: run, after the clock fix a45cf9d (it failed since the rc3 port) | works: run (deposit mode, 1,000,000; `qkz` locked) |
| Register from a public account (module `register_name`) | not on this network with the October CLI (it has no `names module`), as in 0.16.0 | works: run (`trial-53516`: commit, 15-minute wait, reveal, through the guarded CLI wrapper); try-changes: see below |
| Register privately (CLI, a proof) | unchanged path (October CLI), untested in this release | off in the app until real rc3 proofs run; dev mode only, on a throwaway chain |
| Change a name (entries, profile, page, transfer, release) | not with the October CLI, as in 0.16.0 | try-changes on our 0.4: see below |
| Pay a name's public account (`cli_pay_public`) | untested | writes e2e: see below |
| Pay a name's private keys / receive | unchanged path (October CLI), untested | off in the app until real rc3 proofs run; dev mode only, on a throwaway chain |
| Balances | unchanged (October CLI) | works: run (native 20,000,000 and the bridged dUSDC/wETH rows through `bridge_net`; the CLI's account list) |
| Tokens (the wallet's token program) | not on this network (no token program) | bridged tokens show (run); `names token` on our 0.4 untested |
| Bridge deposits | not on this network | view: works (run); public relayed claim: wallet method ready (`propose_relayed_call`, run on a throwaway rc3 chain), bridge side by its owner; private claim: proposes a private call, proof about 11 min and 9.5 GB on the claimer's machine, measured on rc1, not proved on rc3 |
| Messages | today: Logos chat module (address changes on every Basecamp start); Names Messages comes in 0.18.0 | same |
| Relayed calls (`propose_relayed_call`, `resign_relayed_call`) | not on this network | wallet method: unit and module flow tests (stand-in CLI), and the CLI's plan, sign, co-sign and busy/re-sign path run on a throwaway rc3 chain. The generated C++/QML interface of these two methods has not been driven through an lgx and logosctl. |
| Mini-apps (provider calls) | access, transfers: unchanged; public calls need `names module`, not with the October CLI | module flow tests (stand-in CLI); relayed calls run on a throwaway rc3 chain |

## Install

`/Volumes/exos162/logos/wallet-next/install/` on the Mini, not run by the worker:
1. `realtest-0.17.sh`: the read test on copies of the user's config and wallet (the conductor
   runs it, with the user's knowledge);
2. `install-0.17.sh --dry-run`, then `install-0.17.sh`;
3. `rollback-0.17.sh`: back to 0.16.0, the build installed now.

## What Basecamp needs, and the two lines side by side

- **No LEZ module.**
  - The names module doesn't load `lez_core` (the 0.3-line `lez_core` 0.5.0 stays installed for
    the LEZ Wallet app) and doesn't need `logos-execution-zone-module` 0.6.0.
  - It reads both chains with direct JSON-RPC, and signs through the names CLI that each
    network names.
- **Two CLIs, one install:**
  - the public 0.3: the CLI the config already names (on the Mini, the October build at
    `/Volumes/exos162/logos/dev/user-test/bin/names`, `names 0.1.0`);
  - our 0.4: `names 0.17.0 (LEZ v0.4.0-rc3)`, at the path set after switching. The module runs
    only a CLI whose `--version` says `(LEZ v0.4` there, never one found on the computer.
- **No r0vm beside the LEZ 0.4 CLI.**
  - The CLI is built with `local-prove` (in-process prover).
  - On the Mini it runs behind a wrapper: reads, plans, signing and public sends run directly;
    every other command runs under the Mini's watchdog (`guard.sh`) on 4 threads.
  - The app offers no private proof on LEZ 0.4 in 0.17.
- **Basecamp host:** 0.3.2. The module was loaded and driven over logosctl 0.3.2 in a scratch
  session, never the running Basecamp.

## Still missing as SDK underpinning for the private bridge (RFP-021/022)

| Gap | Who needs it |
|---|---|
| `app_secret` never gives out the deposit master `s`. A failsafe reclaim after a wind-down needs it as a witness: there is no wallet operation for it and no explicit export approval (BRIDGE-SDK.md §1). | the bridge mini-app's reclaim path (`lez_bridge`) |
| A private claim or burn (`propose_private_call`) has never been proven on rc3: about 11 min and 9.5 GB, measured on rc1. | `lez_bridge` private claims and exits |
| The Names app has no buttons to pause, resume or cancel a private call's job; the module method `private_call_control` exists. | the person using `lez_bridge` |
| `names module restore` makes 16 public accounts and 1 private one. App secrets of a second private account come back only after it is made again (no `--private N`). | `lez_bridge` recovery from the phrase |
| Token sources are keyed by token program, and the bridge read takes one bridge program. A second bridge on the same token program would replace the first's entry. | a second bridge instance (light-client attested) |
| rc3 public transactions carry no validity window, so a half-signed relayed call stays valid until the account's next transaction. The wallet can't revoke it, only refuse new signatures. | the bridge relay and mini-app |
| The wallet decodes no program's message bytes. A claim shows as "unknown program" with the bytes' hash, not as "claim N dUSDC from deposit i". | `lez_bridge` claims, for the person approving |

## Breaking change for other apps

`get_config` answers other apps in the usual `{success, value, error}` envelope since 0.17.0
(the network part only). From 0.16 it was a plain object for every caller. An app that parsed
the plain object must read `value` now. The Names app itself still gets the plain object.

## Known limits

- SF-1: clearing the bridge list (`set_bridge_programs("[]")`) while on LEZ 0.3 doesn't stick after
  a restart: the stale `lez04.bridge_programs` comes back from the parked LEZ 0.4 settings. It
  fails safe (the old list is read; its token programs stay refused as root).
- S-3: a request the wallet refused at approval stays pending, with its bytes, until it is rejected
  or expires; nothing can sign it while the rule that refused it holds.
- S-5: every `pending_requests` poll (every 3 s) reads each listed bridge's Config once per pending
  relayed call. With 4 bridges and 4 such calls that is about 53 reads per 10 s, over the filter
  proxy's 50: the reads then fail and the sheets show refused (availability only). With the
  default two bridges it is about 7 per 10 s per pending call.
- `bridge-token-programs.json` (beside config.json) only grows: every token program read from a
  bridge's Config (a check, the balance view, a list change) stays refused as a relayed call's
  root for good. A file that can't be read refuses every relayed call until it can; deleting it is
  the reset, and gives up the token programs of bridges no longer listed (the built-in v2 and v3
  ones stay).

- The names module runs one call at a time, on its one thread. A call that waits on another
  module (a Logos Blockchain node call, up to 15 s for a read and 180 s for a proof; a Program
  Registry lookup) waits in a nested event loop, and a names call delivered meanwhile used to wait
  forever on the module's lock: the freeze on the M2. The generated scaffold now takes that lock
  with try_lock (rust-lib/build.rs, scaffold_patch.rs), so that call is answered busy at once,
  before any module code runs, and nothing is done. The app shows "The wallet is busy; still
  working." and asks again; other apps must retry too (PROVIDER-SDK.md). Answers still go only to
  the caller. (0.17's first fix ran calls on worker threads, `concurrency: "multi"`; it was
  dropped because in that mode the host sends each answer to every process holding the module.)

- rc3 binds no chain id and carries no validity window. A half-signed relayed call is good once
  on its chain, but after a reset of the test network, or on another LEZ 0.4 chain where the
  account's nonce comes back to what it was when it signed, the payer could submit it again (it
  must co-sign). Raise a chain id or a
  distinct domain prefix upstream.
- rc3 public messages share their hash prefix with v0.3.1 (theoretical cross-version confusion,
  upstream).
- The wallet decodes no message bytes on the relayed path: the person trusts the app and the
  program, which the sheet says.
- The re-sign window uses the wall clock (bounded by the count of 3). A module restart doesn't
  end it.
