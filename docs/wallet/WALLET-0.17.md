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
- **The bridge is a setting, not a build constant.** The bridge program the Wallet reads on LEZ
  0.4 (its Config lists the bridged tokens) lives in config.json under `lez04.bridge_program`.
  The default is `JECKBkvoc2e3dC18Wm6zZJEC2BGMwUwLsdZiQzYTnUx7`, today's. Change it with the
  module's `set_bridge_program(<id>)`, or `""` to go back to the default:
  `logosctl call names set_bridge_program str:<id>` in a session, or the Names app. The read
  goes to LEZ 0.4's configured sequencer (`lez04.sequencer`). The module holds no vault and no
  relay endpoint: those are the bridge app's.

## What works where

See the report table (evidence: tests and runs on the Mac mini, 2026-11-14).

## Install

`/Volumes/exos162/logos/wallet-next/install/` on the Mini, not run by the worker:
1. `realtest-0.17.sh`: the read test on copies of the user's config and wallet (the conductor
   runs it, with the user's knowledge);
2. `install-0.17.sh --dry-run`, then `install-0.17.sh`;
3. `rollback-0.17.sh`: back to 0.16.0, the build installed now.
