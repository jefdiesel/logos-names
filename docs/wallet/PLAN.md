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
| Blockchain assets: own, send, receive | Done: `names chain balance/send/tx`, the app's Blockchain tab; send to a key or a name's `blockchain` entry. Chain over IPC: `scripts/chain-ipc-e2e.sh` runs the module's `chain_*` through logosctl against the real blockchain_module 0.3.4 on a solo node with no peers (`docs/wallet/e2e-chain-ipc.log`, the 0.15.0 run of 2026-11-06: 34 PASS, 0 FAIL, plus the session stop; the run of 5b37e29 on 2026-11-08 added a bridge deposit section and passed 43, its log was not kept in the repo). It also covers the refusals a verifier found: a hash never sent fails with the node's "no record", a send is still followed after `module unload/load names` (sends kept in `chain-sent.json`), `1,5` is refused rather than read as 15, and a stopped node is named by every method. Upstream: logosctl 0.3.2 turns bare digits into a JSON number, and the host refuses the call before names runs, so logosctl amounts need a comma (`3,000`); `get_transaction` covers only the mempool plus about 10 min, so it is no chain index |
| Staking: stake, unstake, claim | Done as Cryptarchia has it: no stake op (aged notes on the node's keys are the stake), unstake = send off the node's keys, `names chain claim` / Claim. SDP (blend) stake is not wired: the module has no withdrawal call. Chain over IPC: `chain_stake` equals the node's aged notes and vouchers. A claim with nothing to claim is a plain refusal (e2e log). A claim with vouchers was checked on a solo node across the 12:14 UTC epoch boundary on 2026-11-05 (verifier's `v-stake/driver.sh`, log on the Mini at `dev/chain-ipc/v-stake/driver.log`): no Blend peers needed, the claim answered in 0.3 s, and its tx 504090bb was in the block at slot 15600021. **One voucher per claim**: 1656 → 1655 vouchers, 38 claimed, fee 2028 (input 5000, change 2972) from the LeaderFunding key, which must hold funds first. names now reports `claimed`, `fee` and a `warning`. Upstream: the module's `leader_claim()` takes no `max_tx_fee` and has no fee estimate, so a claim can cost 53× its reward and nothing can refuse it in advance |
| Bridging, chain → zone | Done through Basecamp's own node: `chain_split` makes a note of exactly the amount, `chain_bridge` deposits it on LEZ's channel (`channel_deposit_with_notes`), the app's Bridge screen runs both; `names bridge` (CLI, node HTTP API) and Get tokens as before. **On the real testnet:** 2026-11-06 11:01 EST the Mini's Basecamp node split off 10,000,000 (tx e9156c80, fee 2,298) and bridged it (tx ff6b5485) on channel 0101…01 to `Public/9W9huNeAds11pdGpMijrdhTkSQ8upNudRnrZJbNNWfoW`, which held 1 and holds 10,000,001 on 2026-11-09 (`getAccountBalance` on testnet.lez.logos.co). On a solo node: a deposit against a stand-in for the sequencer's `getChannelId`, its fee, landing and the double-bridge refusal (5b37e29). Pending/settled: the tx is followed to `in_block`, then the arrival is the destination's credit (activity); the deposit receipt PDA is not read yet, and the time from block to LEZ credit was not recorded |
| Bridging, zone → chain | Blocked upstream: the LEZ bridge guest panics "Withdraws are disabled" in rc1 and on dev |
| Multi-zone, per-zone sequencer | To do, after the fix/* merge (they rewrite lib.rs, chain.rs, activity.rs). Design: a zone is known by its channel (`getChannelId`), not its URL: a URL whose channel matches the current zone is a sequencer switch that keeps the wallet as it is; another channel is another zone. Each zone has its own LEZ wallet home, since `storage.json` holds `last_synced_block` and private account data that belong to one chain (LEZ `lez/wallet/src/storage/persistent.rs`); the keys are the same on every zone. A zone = `{name, channel, sequencer, wallet, registry}`; `cli_setup()` builds from the current zone, names resolve on the home zone, the bridge uses the current zone's channel. Old configs become zone 0, and the flat `sequencer`/`registry`/`cli_wallet` fields keep being written so a rollback reads them. Activity marks move from per-sequencer to per-zone (`marks_are_per_sequencer`). Test: the official testnet plus a standalone sequencer on the Mini, spun down after |
| Multiple blockchain accounts | Listing done (the node's keys); making a key needs `generate_key`, whose arguments differ between the module's lidl and its client |
| SDK across chain and zones | To do: `propose_transfer` etc. for the blockchain |
| Testimonial + faucet on the blockchain | To do |
| Approval prompts for stake/bridge calls | To do; source verification (LP-0023) is in |
| Gas on blockchain transactions | Estimate before, fee paid after. Every `chain_send`/`chain_split`/`chain_bridge`/`chain_claim` plan carries `fee_about`, the node's own fee for that transaction (its `wallet_fund_tx`, asked with stand-ins of the key's notes and no funding key, because asked with real notes it reserves them: on a solo node a second plan got "available=0" and the send after it was refused); `cannot_pay_fee` when the key's notes can't pay it with its fee; a claim plan warns when one claim's fee exceeds its reward. Shown in the app's confirm step, the bridge plan and the Stake page, and in `names chain send` plans (`/wallet/fund`). `scripts/chain-fee-e2e.sh` on a solo node: estimate = actual for a send of one input (834) and two (866), a split (834), the CLI plan (834), a deposit (1,614); a claim is estimated (1,599) but no claim was compared. Fee paid: read from the stored transaction as before. Gas prices in `names chain status` |
| E2E against a real node in CI | Chain over IPC on the Mini (`scripts/chain-ipc-e2e.sh`, solo node, no peers); not in CI: the wallet e2e there runs only a sequencer |
