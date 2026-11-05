# Names Wallet: plan for λPrize LP-0021

Names grows into the LEZ wallet the prize asks for, instead of a new app: it already has the
wallet, keys in the names CLI, public and private accounts, payments, the faucet, and per-app
permissions in the module. One install (the Names app + the names module) stays one install.
Spec: lambda-prize `prizes/LP-0021.md` ($20,000, X-Large, first submission meeting every
criterion wins, adoption measured over 2+ months, so it can only finish 2+ months after
testnet 0.4 launches).

## Criteria map

| Criterion | Names today | Gap | Who |
|---|---|---|---|
| LEZ assets: own/send/receive native, public and private | `cli_pay` (to a name, privately or publicly), `cli_pay_account`, `cli_shield`, `cli_balance`, `wallet` | receive screen (show address/name to receive), history | me, later |
| Token program: fungible tokens, public and private | **CLI done**: `names token list/info/send/receive/create/mint` + `names module token …`, behind a codec trait; prototype 0.4 token program (`wallet-tokens/`) until an official one ships (none exists for 0.4; assumed coming). **Module, app and approval sheet done** (branch `module-tokens`, TOKENS.md section 10) | a live run through the app on a local chain | me |
| Multiple accounts, public and private, switch | **done**: Settings → Accounts lists every account with its balance, makes new public/private ones, renames them (nicknames kept in `names.json`: LEZ wallet labels can't be changed), and chooses which pays and registers (`names module account-list/new/label/use`, module `cli_accounts`, `cli_account_*`; changes wait while a CLI job runs). A never-used public account shows its key, which a sender needs on 0.4 | — | done |
| Wallet Provider SDK (connect, read, propose txs) | **done**: request_access, granted_accounts, account_balances, propose_transfer, propose_call, request_status; Names-only approve/reject/grants/revoke (`docs/wallet/PROVIDER-SDK.md`); calls signed through the names CLI | real call against a sequencer; run inside Basecamp | me |
| Selection + approval prompts naming app, account, asset, amount, destination; readable summary for program calls | **done**: approval sheet with registry label + verification, warnings needing an extra tick, and the network fee for transfers | — | done |
| Program source-verification status in the prompt | — | `program_registry` module (`program_label`) | done in lez-program-registry M6 (reads); wire in via the provider agent |
| Testimonial mini-app + an on-chain testimonial program with submission id | program + client merged (`testimonial/`, 8 engine tests, live post/list on rc1); **mini-app built** (`apps/testimonial/`, branch `mini-apps`: SDK-only wallet flow, JS payload checked against Rust vectors, posted live on rc1 through the module) | deploy at testnet launch; set the program ID in the app. Our submission id: `lp-0021:names-wallet` (fits the program's format, so the image doesn't change). Public authors only: a private sender shows a fresh alias per message, which would let one account post as many | me, after the provider SDK |
| Faucet mini-app built on the SDK (account choice, rate limits) | Welcome's "Get free tokens" (Names faucet); **mini-app built** (`apps/faucet/`, branch `mini-apps`: SDK access + account choice, configurable and disclosed faucet URL, rate limits with retry times) | target may change (lambda-prize PR #181): point the URL at it | me, after the SDK |
| Testnet 0.4 end to end | port done on the 0.4 branch, tested on local rc1 chains | testnet not launched | waits for launch |
| Module catalog via a fork of `logos-modules-release-base` + `logos-modules-release-action` | **done 2026-11-04**: [jefdiesel/logos-modules](https://github.com/jefdiesel/logos-modules), a fork released by the action: `names`, `names_ui`, `testimonial_ui`, `faucet_ui` (and the registry's `program_registry`), each for darwin-arm64, linux-amd64 and linux-arm64, unsigned; Basecamp adds `https://raw.githubusercontent.com/jefdiesel/logos-modules/main/logos-repo.json`. macOS legs build on a self-hosted Mac mini runner (GitHub's Apple-silicon runners never picked the jobs up) | bump the submodule and rerun at each release; sign releases | me |
| Gas estimate and gas used in the UI | **done**: every send prints `fee: about A, at most M (...)` before and `fee paid: P` after (LEZ 0.4 keeps no fee receipt, so it is measured from the payer's balance; formulas from LEZ's `fee_core`); the module puts it in each job's `fee`; the app shows the estimate on Pay and in approvals, and the fee paid when a job ends. `cli_fee` quotes the chain's fees. Program calls are sized when tried, just before sending | — | done |
| State survives restarts and network drops | **tested** (wallet-e2e.sh step 6): with the sequencer stopped, reads and sends fail at once with the reason while nicknames and the paying account still change; restarted on the same chain, every balance and setting is as it was and a transfer lands. Wallet files are the LEZ wallet's; module config atomic 0600 | — | done |
| No mandatory external services | **checked 2026-11-04** (every URL in the module, CLI and app): reads and sends go only to the sequencer the person chose (default the public LEZ testnet); the registry's bytecode is read from the chain. The rest is optional and named in the app before use: the Names faucet (`cli_faucet`, the faucet mini-app's URL is configurable), the 0.3 claims snapshot (only for claiming a name held before the 0.4 reset, a static file) and the one-time CLI installer (signed files, checked against the release key) | — | done |
| Remote backup only on Logos Storage, encrypted client-side | none (no backup) | optional encrypted backup | later |
| E2E tests on a standalone sequencer in CI | module tests; `try-changes.py` against a local chain | CI job like the registry's `e2e.yml` | me |
| README + SDK docs with the two mini-apps as worked examples | partial | write | me + provider agent |
| Adoption: 10 outside developers' Basecamp apps using the SDK; 150 testimonials from 150 accounts over 2 months (30+/month); 30 Discord + 30 X testimonials; macOS users | LNS users and integrators (Forumer, WhisperBox, Backpack, OneSecret) | outreach, from launch day | the user |

## Order

1. Now, in parallel: tokens, testimonial program, provider SDK + approval UI (agents).
2. Merge into `lez-0.4`, then: account management in the app, the two mini-apps on the SDK,
   gas display, e2e CI, docs.
3. Catalog fork (with the registry's modules), release.
4. Launch day of testnet 0.4: deploy the testimonial program, publish, start the adoption clock.

## Cost (zero marginal cost)

| Path | Kind | Worst case at 500 req/s |
|---|---|---|
| Wallet, module, SDK, mini-apps | run on the user's machine; talk to the user's sequencer and Storage node | $0 to us |
| Testimonial program | on chain; each testimonial is a transaction its author pays for | $0 to us |
| Docs, releases, catalog | GitHub Releases / static files | $0 |
| Names faucet | existing service on the M2 (fixed cost, rate-limited per account) | fixed; the faucet mini-app must also work with the official LEZ faucet so ours is optional |

No new Worker, server or database. Nothing is sent to a third party without opt-in.

## LP-0022: the Logos Blockchain and many zones

LP-0022 (draft, $10,000) builds on this wallet. Where each criterion stands:

| Criterion | State |
|---|---|
| Blockchain assets: own, send, receive | Done: `names chain balance/send/tx`, the app's Blockchain tab; send to a key or a name's `blockchain` entry |
| Staking: stake, unstake, claim | Done as Cryptarchia has it: no stake op (aged notes on the node's keys are the stake), unstake = send off the node's keys, `names chain claim` / Claim. SDP (blend) stake is not wired: the module has no withdrawal call |
| Bridging, chain → zone | Done before (`names bridge`, Get tokens); settled = the bridge's deposit receipt PDA, not shown yet |
| Bridging, zone → chain | Blocked upstream: the LEZ bridge guest panics "Withdraws are disabled" in rc1 and on dev |
| Multi-zone, per-zone sequencer | To do: a zone = its own LEZ wallet home + sequencer URL + channel; the registry resolves on a home zone |
| Multiple blockchain accounts | Listing done (the node's keys); making a key needs `generate_key`, whose arguments differ between the module's lidl and its client |
| SDK across chain and zones | To do: `propose_transfer` etc. for the blockchain |
| Testimonial + faucet on the blockchain | To do |
| Approval prompts for stake/bridge calls | To do; source verification (LP-0023) is in |
| Gas on blockchain transactions | Fee read from the stored transaction when it shows one; gas prices in `names chain status` |
| E2E against a real node in CI | To do: the wallet e2e runs only a sequencer |
