# Busy answers in the Names app (0.17.0): every names call, once

The names module answers busy at once when a call arrives while another call waits on a
different module (the Logos Blockchain node, the Program Registry): the generated scaffold's
try_lock (`basecamp-module/rust-lib/scaffold_patch.rs`). **Busy means the call was not taken:
nothing was done.** The app sorts each reply with `Logic.replyKind`. The module's busy envelope
becomes `moduleBusy` ("The wallet is busy; still working."). No answer at all (a host timeout, or
the app's own patience running out) becomes `moduleNoAnswer` ("No answer from the wallet yet: it
may still be working on it. Look before trying again."), because the call may have been taken.
Polls and reads ask again after either one (`waitAgain`). Actions show the line; a busy action is
safe to press again.

Since `names-stall-2` (2026-11-18), the reads the app polls while a page is open never wait on
the network or the names CLI on the module's thread (`basecamp-module/rust-lib/src/served.rs`):
`wallet`, `cli_balance`, `activity`'s balance and receipt reads, `cli_accounts`, `cli_status`'s
CLI probes, `cli_mine`'s first read, `mine`, `keep`, `cli_fee`, `settings`, `faucet_info`,
`zone_list`'s channel read, the relayed-call sheet's bridge Configs, and, for the Names app only,
`resolve`, `page`, `page_view`, `avatar` and `deposit` (other apps still read those on the spot)
are read on a thread of their own and answered with the last read (`read_at`; a read that failed
since says `stale`, `as_of` and `warning`, which `Logic.readNote` shows; `updating` while a newer
one is under way, and the balance, accounts and wallet loaders ask again for it). Before the first
read is back (600 ms at most; also when a kept read is past its age) such a call answers busy, and
the app asks again: the name lookups (`read`, the board's, `keep`) now do too. A change to the
wallet (a send logged, a job ended, an account made or renamed) drops the kept reads of it. `faucet_claim` answers `code: "pending"` while the faucet is asked, and
the app asks again for its answer (never a second ask of the faucet).

Since `names-stall-3` (2026-11-19), also as Names opens on a Basecamp that has the Logos
Blockchain node loaded: the node reads the app polls (`chain_status`, `chain_accounts`,
`chain_stake`, `chain_claim` without sending, `chain_tx`, `activity`'s settling,
`pending_inscriptions`' accounts) are asked without waiting (`node::polled`, an asynchronous
ask from the call's thread, the answer kept 5 s, shown 30 s at most while a newer one is asked),
and answer busy before the node's first answer. The module keeps one client per module for the
life of the host (`peer.rs`, `client`), so the node's object is acquired once, and its first ask
may wait 1 s until one was sent without acquiring. The CLI's `--version` (asked on LEZ 0.4 before
any CLI run) and `file_sha256` (the shipped bridge, ~10 MB) are read on a thread of their own:
the polls answer busy meanwhile, any other call says so in words (busy promises nothing was
done). A burst of calls waits 400 ms in all for first reads (`served::WAIT_BUDGET`), not 600 ms
each, and a read already under way is never waited for again. A send, a claim, an account made
or renamed and an inscription still wait for the node, and drop what the polls kept.

Since `names-hang-fix` (2026-11-18), a module that isn't loaded is never waited for, so it can't
make the wallet busy (`basecamp-module/rust-lib/src/peer.rs`). The module finds out from its
endpoint, which takes microseconds and needs no client. A chain method then answers at once with
`code: "module_not_loaded"` and "…isn't running…": the node-off line, with Open the Blockchain
app. A node that is loaded but gave no answer is not asked again for 30 s. The Program Registry
and package_manager are asked without waiting. Busy is left only for a call that arrives while
another one waits on a module that is loaded (a send being proven, say).

The table lists every site in `basecamp/qml/Main.qml` that calls the names module, with what it
does on busy. It was generated from the source at the commit that adds this file.

- **askLater**: asynchronous. The failure handler receives the message.
- **ask / normalized**: synchronous. They throw the message.
- **call**: raw, used only for get_config and the five bool setters the app calls.

## The bool methods

There are 8: set_registry, set_sequencer, set_wallet_files, set_cli, set_cli_wallet,
set_token_program, set_bridge_program and set_bridge_programs. On busy each one answers `false`,
the same as a refusal. The app calls five of them, and each reads `false` as "not saved" and
keeps the old setting:
- set_registry (saveConfig)
- set_wallet_files (saveWalletFiles)
- set_cli and set_cli_wallet (saveCliSettings)
- set_token_program (saveTokenProgram)

The app never calls set_sequencer, set_bridge_program or set_bridge_programs; only other tools
do.

## The sites

| Line | Function | Call | Method | On busy |
|---|---|---|---|---|
| 456 | take | call | get_config | config: busy has no `sequencer`, so it is not taken; configRetry asks again in 2 s |
| 472 | saveConfig | call | set_registry | bool: busy is `false`: "not saved", the old setting stays; press again |
| 513 | loadZones | askLater | zone_list | keeps what is shown; asked again in 5 s (againWhenFree) |
| 527 | zoneAsk | askLater | method | action: says "The wallet is busy; still working." (nothing was done; press again). No answer at all says "No answer from the wallet yet…" instead |
| 614 | loadDeposit | askLater | deposit | keeps what is shown; asked again in 5 s (againWhenFree) |
| 637 | saveWalletFiles | call | set_wallet_files | bool: as set_registry |
| 653 | saveCliSettings | call | set_cli | bool: as set_registry |
| 657 | saveCliSettings | call | set_cli_wallet | bool: as set_registry |
| 676 | loadCliMine | askLater | cli_mine | poll: keeps what is shown; its timer asks again (15 s; anon_name and board_likes: best effort) |
| 691 | loadCliMine | askLater | anon_name | poll: keeps what is shown; its timer asks again (15 s; anon_name and board_likes: best effort) |
| 692 | loadCliMine | askLater | board_likes | poll: keeps what is shown; its timer asks again (15 s; anon_name and board_likes: best effort) |
| 701 | loadCliBalance | askLater | cli_balance | keeps the balance shown (nothing shown: "Reading…"); asked again in 5 s (againWhenFree) |
| 753 | loadBridgeNet | askLater | bridge_net_balances | poll: shows the busy line under the balances it keeps; 30 s timer |
| 772 | bridgeNetWatch | askLater | bridge_net_watch | action: says "The wallet is busy; still working." (nothing was done; press again). No answer at all says "No answer from the wallet yet…" instead |
| 791 | setWalletNet | askLater | set_names_network | action: says "The wallet is busy; still working." (nothing was done; press again). No answer at all says "No answer from the wallet yet…" instead |
| 847 | loadAccounts | askLater | cli_accounts | keeps what is shown; asked again in 5 s (againWhenFree) |
| 868 | accountChange | askLater | method | action: says "The wallet is busy; still working." (nothing was done; press again). No answer at all says "No answer from the wallet yet…" instead |
| 902 | loadFee | askLater | cli_fee | quote dropped, asked again when the sheet asks |
| 938 | chainAccountNew | askLater | chain_account_new | action: says "The wallet is busy; still working." (nothing was done; press again). No answer at all says "No answer from the wallet yet…" instead; its follow-up chain_accounts read only logs |
| 944 | chainAccountNew | askLater | chain_accounts | action: says "The wallet is busy; still working." (nothing was done; press again). No answer at all says "No answer from the wallet yet…" instead; its follow-up chain_accounts read only logs |
| 970 | loadChain | askLater | chain_status | keeps the node's state, keys, stake and claim plan; chainRetry reads again in 5 s |
| 973 | loadChain | askLater | chain_accounts | keeps the node's state, keys, stake and claim plan; chainRetry reads again in 5 s |
| 978 | loadChain | askLater | chain_stake | keeps the node's state, keys, stake and claim plan; chainRetry reads again in 5 s |
| 983 | loadChain | askLater | chain_claim | keeps the node's state, keys, stake and claim plan; chainRetry reads again in 5 s |
| 1032 | chainCheck | askLater | chain_send | action: says "The wallet is busy; still working." (nothing was done; press again). No answer at all says "No answer from the wallet yet…" instead |
| 1051 | chainSend | askLater | chain_send | action: says "The wallet is busy; still working." (nothing was done; press again). No answer at all says "No answer from the wallet yet…" instead |
| 1068 | chainPoll | askLater | chain_tx | job poll: next tick asks again |
| 1087 | chainClaim | askLater | chain_claim | action: says "The wallet is busy; still working." (nothing was done; press again). No answer at all says "No answer from the wallet yet…" instead |
| 1213 | loadActivity | askLater | activity | poll: keeps what is shown; its timer asks again (30 s) |
| 1465 | loadWatched | askLater | watched_tokens | keeps what is shown; asked again in 5 s (againWhenFree) |
| 1491 | addToken | askLater | watch_token | action: says "The wallet is busy; still working." (nothing was done; press again). No answer at all says "No answer from the wallet yet…" instead |
| 1506 | removeToken | askLater | watch_token | action: says "The wallet is busy; still working." (nothing was done; press again). No answer at all says "No answer from the wallet yet…" instead |
| 1515 | keyOnName | ask | resolve | settle read after a sent change: busy doesn't use up a try; next tick asks again |
| 1676 | bridgeRun | askLater | chain_bridge | action: says "The wallet is busy; still working." (nothing was done; press again). No answer at all says "No answer from the wallet yet…" instead |
| 1717 | bridgeSplit | askLater | chain_split | action: says "The wallet is busy; still working." (nothing was done; press again). No answer at all says "No answer from the wallet yet…" instead; its tx poll: next tick |
| 1740 | bridgeSplit | askLater | chain_tx | action: says "The wallet is busy; still working." (nothing was done; press again). No answer at all says "No answer from the wallet yet…" instead; its tx poll: next tick |
| 1839 | loadCliStatus | askLater | cli_status | keeps the last status (a busy read is never "no wallet", so never Welcome; nothing yet: "Reading your wallet…"); asked again in 5 s (againWhenFree) |
| 1862 | installCli | askLater | cli_install | action: says "The wallet is busy; still working." (nothing was done; press again). No answer at all says "No answer from the wallet yet…" instead |
| 1893 | checkInstall | askLater | job | job poll: next tick asks again |
| 1918 | createWallet | askLater | cli_create_wallet | action: says "The wallet is busy; still working." (nothing was done; press again). No answer at all says "No answer from the wallet yet…" instead (and the module refuses over an existing store) |
| 1940 | restoreWallet | askLater | cli_restore_wallet | action: says "The wallet is busy; still working." (nothing was done; press again). No answer at all says "No answer from the wallet yet…" instead (and the module refuses over a wallet that opens) |
| 1955 | loadClaims | askLater | settings | keeps what is shown; asked again in 5 s (againWhenFree) |
| 1971 | claimName | askLater | cli_claim | action: says "The wallet is busy; still working." (nothing was done; press again). No answer at all says "No answer from the wallet yet…" instead |
| 1983 | checkClaim | askLater | job | job poll: next tick asks again |
| 2007 | getFreeTokens | askLater | cli_faucet | action: says "The wallet is busy; still working." (nothing was done; press again). No answer at all says "No answer from the wallet yet…" instead |
| 2031 | moveToPrivate | askLater | cli_shield | action: says "The wallet is busy; still working." (nothing was done; press again). No answer at all says "No answer from the wallet yet…" instead |
| 2049 | claimPayments | askLater | cli_receive | action: says "The wallet is busy; still working." (nothing was done; press again). No answer at all says "No answer from the wallet yet…" instead |
| 2062 | checkReceive | askLater | job | job poll: next tick asks again |
| 2080 | checkShield | askLater | job | job poll: next tick asks again |
| 2149 | loadMine | askLater | mine | logs; the reveal driver and loadCliMine read again |
| 2183 | revealDue | askLater | register_name | reveal driver: logs, reads My names again, tries again on its 30 s tick (logic test) |
| 2252 | loadWallet | askLater | wallet | keeps the wallet shown; walletRetry asks again (busy is never "no wallet") |
| 2299 | find | ask | reverse | throws busy; the caller's catch says it (nothing done) |
| 2340 | checkRegister | normalized | normalize | throws busy; the caller's catch says it (nothing done) |
| 2345 | checkRegister | askLater | resolve | throws busy; the caller's catch says it (nothing done) |
| 2387 | loadAuthorPosts | askLater | board_posts_of | best effort: ignored; asked again on the next poll or use |
| 2411 | read | ask | resolve | throws busy; the caller's catch says it (nothing done) (normalize via `normalized()`, never "[object Object]") |
| 2416 | read | normalized | normalize | throws busy; the caller's catch says it (nothing done) (normalize via `normalized()`, never "[object Object]") |
| 2431 | read | ask | page_view | throws busy; the caller's catch says it (nothing done) (normalize via `normalized()`, never "[object Object]") |
| 2436 | read | askLater | keep | throws busy; the caller's catch says it (nothing done) (normalize via `normalized()`, never "[object Object]") |
| 2457 | change | askLater | method | action: says "The wallet is busy; still working." (nothing was done; press again). No answer at all says "No answer from the wallet yet…" instead |
| 2525 | cliChange | askLater | cliMethod | action: says "The wallet is busy; still working." (nothing was done; press again). No answer at all says "No answer from the wallet yet…" instead |
| 2582 | checkCliJob | askLater | job | job poll: next tick asks again |
| 2667 | registerName | askLater | register_name | action: says "The wallet is busy; still working." (nothing was done; press again). No answer at all says "No answer from the wallet yet…" instead |
| 2702 | receivePrivately | ask | resolve | settle read after a sent change: busy doesn't use up a try; next tick asks again |
| 2710 | clearAddresses | ask | resolve | settle read after a sent change: busy doesn't use up a try; next tick asks again |
| 2720 | saveText | ask | resolve | settle read after a sent change: busy doesn't use up a try; next tick asks again |
| 2757 | saveProfile | ask | resolve | settle read after a sent change: busy doesn't use up a try; next tick asks again |
| 2771 | savePage | ask | page | settle read after a sent change: busy doesn't use up a try; next tick asks again |
| 2781 | transferName | ask | resolve | settle read after a sent change: busy doesn't use up a try; next tick asks again |
| 2788 | releaseName | ask | resolve | settle read after a sent change: busy doesn't use up a try; next tick asks again |
| 2876 | previewPage | askLater | page_preview | keeps what is shown; asked again in 5 s (againWhenFree) |
| 2935 | loadAvatar | askLater | avatar | keeps what is shown; asked again in 5 s (againWhenFree) |
| 3102 | setMain | askLater | set_main_name | action: says "The wallet is busy; still working." (nothing was done; press again). No answer at all says "No answer from the wallet yet…" instead |
| 3120 | makeAnon | askLater | anon_name | best effort: ignored; asked again on the next poll or use |
| 3189 | payName | normalized | normalize | stops the payment with the busy line: the "your own name" check (normalize) is never skipped (S-4) |
| 3201 | payName | ask | recipient | stops the payment with the busy line: the "your own name" check (normalize) is never skipped (S-4) |
| 3314 | boardLoad | askLater | method | keeps the posts; boardRetry asks again |
| 3329 | boardLoad | askLater | board_follows | keeps the posts; boardRetry asks again |
| 3339 | boardLookUp | askLater | resolve | best effort: ignored; asked again on the next poll or use |
| 3351 | boardLookUp | askLater | avatar | best effort: ignored; asked again on the next poll or use |
| 3376 | boardStart | askLater | method | action: says "The wallet is busy; still working." (nothing was done; press again). No answer at all says "No answer from the wallet yet…" instead |
| 3391 | boardCheckJob | askLater | job | job poll: next tick asks again |
| 3453 | chatLoad | askLater | dm_names | best effort: ignored; asked again on the next poll or use (10 s) |
| 3454 | chatLoad | askLater | chat_names | best effort: ignored; asked again on the next poll or use (10 s) |
| 3456 | chatLoad | askLater | resolve | best effort: ignored; asked again on the next poll or use (10 s) |
| 3497 | chatAddressOf | normalized | normalize | says busy and stops (never "That isn't a name.") |
| 3510 | chatAddressOf | askLater | resolve | says busy and stops (never "That isn't a name.") |
| 3517 | chatAddressOf | askLater | set_chat_name | says busy and stops (never "That isn't a name.") |
| 3588 | chatStart | askLater | set_dm_name | best effort: ignored; asked again on the next poll or use |
| 3615 | chatPublish | ask | resolve | settle read after a sent change: busy doesn't use up a try; next tick asks again |
| 3636 | loadRunningJobs | askLater | running_jobs | best effort: ignored; asked again on the next poll or use |
| 3676 | boardSpellCheck | askLater | spell_check | best effort: ignored; asked again on the next poll or use |
| 3699 | boardFollow | askLater | board_follow | action: says "The wallet is busy; still working." (nothing was done; press again). No answer at all says "No answer from the wallet yet…" instead |
| 3810 | loadTokens | askLater | token_source_holdings | poll: keeps what is shown; its timer asks again (3 s) |
| 3817 | loadTokens | askLater | cli_token_holdings | poll: keeps what is shown; its timer asks again (3 s) |
| 3825 | loadTokens | askLater | cli_token_pending | poll: keeps what is shown; its timer asks again (3 s) |
| 3835 | saveTokenProgram | call | set_token_program | bool: as set_registry |
| 3900 | startTokenJob | askLater | method | action: says "The wallet is busy; still working." (nothing was done; press again). No answer at all says "No answer from the wallet yet…" instead |
| 3915 | tokenCheckJob | askLater | job | job poll: next tick asks again |
| 3977 | loadAppRequests | askLater | pending_requests | poll: keeps the sheet; 3 s timer |
| 4005 | loadAppGrants | askLater | app_grants | keeps what is shown; asked again in 5 s (againWhenFree) |
| 4099 | answerAppRequest | askLater | method | approve/reject: says busy on the sheet; the request stays pending (busy means not taken) and the list is read again; press again |
| 4130 | revokeApp | askLater | revoke_app | action: says "The wallet is busy; still working." (nothing was done; press again). No answer at all says "No answer from the wallet yet…" instead |
| 4203 | loadInscribeRequests | askLater | pending_inscriptions | poll: keeps the sheet; 3 s timer |
| 4248 | loadInscribePlan | askLater | chain_inscribe | plan cleared, no error shown (the busy module is not the account's problem); read again on the sheet's next 3 s poll |
| 4289 | answerInscribeRequest | askLater | approve | busy: says busy, stays pending; no answer: "No answer yet: the inscription may still be on its way…" |
| 5808 | next | askLater | set_dm_name | best effort: ignored; asked again on the next poll or use |

## Every call and how long it can hold the module's thread (names-stall-3, 2026-11-19)

The module answers one call at a time on the host's main thread, and Basecamp gives up on a call
after about 20 s. A call that waits holds every call behind it, so this lists every call the
module exposes (`NamesModule`), the longest it can hold that thread, and why. "Measured" is from
the unit test `every_call_the_module_exposes_holds_the_thread_how_long` (lib.rs), with the node
answering after 3 s, a names CLI taking 3 s and an endpoint 4 s. A call that fails its checks
early shows near 0 there, so the worst case comes from the code's own bounds. That test fails if a
read (not a press) holds the thread a second or waits on the network, the CLI or another module.

- A **read** never waits: what it needs is read on a thread of its own (`served.rs`, or an ask of
  the node, `node::polled`), and it answers busy until the first answer is back.
- A **press** (a change the person or an app asked for) may wait, by design and within the bound
  given. The app shows a working line at once (`say(…, "working")`, `chainBusy`, `bridgeBusy`,
  `registerChecking`). A wait on the call's thread can't be cancelled: the host delivers no other
  call until it ends, so a cancel could only arrive after it.
- Since names-stall-3 the app asks again by itself when a plan or a lookup is answered busy
  (`againWhileBusy`, every 1.5 s, up to 20 times): Check on the Logos Blockchain send
  (`chain_send` plan), the bridge plan (`chain_bridge` plan), Find by account (`reverse`) and Pay
  a name (`recipient`).

| Call | Worst case on the module's thread | Why | Measured |
|---|---|---|---|
| `account_balances` | < 50 ms. Local files and memory only. |  | 0 ms |
| `activity` | < 50 ms. Local files and memory only. |  | 0 ms |
| `anon_name` | < 50 ms. Local files and memory only. |  | 0 ms |
| `app_grants` | < 50 ms. Local files and memory only. |  | 0 ms |
| `app_note` | < 50 ms. Local files and memory only. |  | 0 ms |
| `app_secret` | 60 s. | Another app's call (the LEZ Bridge): the names CLI derives the values (`SECRET_PATIENCE`), about 1 s; up to 20 s more when it asks whether an imported account (`SCHEME_PATIENCE`). Kept on the call's thread on purpose: the bridge waits 20 s for it and reads a busy answer as a failure. Rate-limited per app and hour. | 3052 ms |
| `approve_inscription` | 15 s per node read (`node::QUICK`); 180 s while the node proves a send or claim (`node::PROVING`). By design: a press; nothing else is answered meanwhile. |  | 0 ms |
| `approve_request` | 12 s for a relayed call (the CLI's plan, `PUBLIC_PLAN_PATIENCE`) plus 600 ms for a bridge's Config not read within its bound; otherwise local. | A press. | 0 ms |
| `avatar` | As `served`; for the Names app only. Another app's call reads on the spot: up to 10 s per registry request (`REQUEST_CAP`) plus about 1 s of Argon2 for a name not seen yet. |  | 1 ms |
| `backup_check` | < 50 ms. Local files and memory only. |  | 0 ms |
| `backup_make` | < 100 ms. Checks, then the names CLI runs as a job; the app polls `job`. |  | 0 ms |
| `backup_open` | < 50 ms. Local files and memory only. |  | 0 ms |
| `backup_restore` | < 100 ms. Checks, then the names CLI runs as a job; the app polls `job`. |  | 0 ms |
| `backup_status` | < 50 ms. Local files and memory only. |  | 2 ms |
| `board_feed` | 600 ms at most, 400 ms for a whole burst (`served::WAIT_BUDGET`). Read on a thread of its own; busy until the first read is back. |  | 0 ms |
| `board_follow` | < 50 ms. Local files and memory only. |  | 9 ms |
| `board_follows` | < 50 ms. Local files and memory only. |  | 0 ms |
| `board_hide` | < 100 ms. Checks, then the names CLI runs as a job; the app polls `job`. |  | 0 ms |
| `board_like` | < 100 ms. Checks, then the names CLI runs as a job; the app polls `job`. |  | 0 ms |
| `board_likes` | < 50 ms. Local files and memory only. |  | 0 ms |
| `board_post` | < 100 ms. Checks, then the names CLI runs as a job; the app polls `job`. |  | 0 ms |
| `board_posts_of` | 600 ms at most, 400 ms for a whole burst (`served::WAIT_BUDGET`). Read on a thread of its own; busy until the first read is back. |  | 0 ms |
| `board_thread` | 600 ms at most, 400 ms for a whole burst (`served::WAIT_BUDGET`). Read on a thread of its own; busy until the first read is back. |  | 0 ms |
| `board_toggle_like` | < 100 ms. Checks, then the names CLI runs as a job; the app polls `job`. |  | 0 ms |
| `bridge_net_balances` | < 50 ms. Local files and memory only. |  | 0 ms |
| `bridge_net_watch` | < 50 ms. Local files and memory only. |  | 8 ms |
| `cancel_request` | < 50 ms. Local files and memory only. |  | 0 ms |
| `chain_account_label` | 15 s per node read (`node::QUICK`); 180 s while the node proves a send or claim (`node::PROVING`). By design: a press; nothing else is answered meanwhile. |  | 3016 ms |
| `chain_account_new` | 15 s per node read (`node::QUICK`); 180 s while the node proves a send or claim (`node::PROVING`). By design: a press; nothing else is answered meanwhile. |  | 3006 ms |
| `chain_account_remove` | 15 s per node read (`node::QUICK`); 180 s while the node proves a send or claim (`node::PROVING`). By design: a press; nothing else is answered meanwhile. |  | 0 ms |
| `chain_accounts` | About 0. The node is asked without waiting (`node::polled`, kept 5 s); busy before its first answer. The first ask after the host starts may hold the thread up to 1 s while the host acquires the node's object (`FIRST_ASK_WAIT`, then held 30 s if it did). |  | 0 ms |
| `chain_bridge (send false)` | As the node polls, plus a name's key and LEZ's version, channel and the account's key read on a thread of their own (busy until back). The app asks again by itself every 1.5 s (up to 20 times). |  | 0 ms |
| `chain_bridge (send true)` | 15 s per node read (`node::QUICK`); 180 s while the node proves a send or claim (`node::PROVING`). By design: a press; nothing else is answered meanwhile. Plus LEZ's version and channel (10 s and 15 s) and the CLI's accounts on LEZ 0.4 (20 s), read afresh. |  |  |
| `chain_claim (send false)` | As the node polls, plus a name's key and LEZ's version, channel and the account's key read on a thread of their own (busy until back). The app asks again by itself every 1.5 s (up to 20 times). |  | 0 ms |
| `chain_claim (send true)` | 15 s per node read (`node::QUICK`); 180 s while the node proves a send or claim (`node::PROVING`). By design: a press; nothing else is answered meanwhile. |  |  |
| `chain_inscribe (send false)` | As the node polls, plus a name's key and LEZ's version, channel and the account's key read on a thread of their own (busy until back). The app asks again by itself every 1.5 s (up to 20 times). |  | 0 ms |
| `chain_inscribe (send true)` | 15 s per node read (`node::QUICK`); 180 s while the node proves a send or claim (`node::PROVING`). By design: a press; nothing else is answered meanwhile. |  |  |
| `chain_recipient` | < 50 ms. Local files and memory only. (a key; a name is read as `chain_send`'s plan reads it on a press) |  | 0 ms |
| `chain_send (send false)` | As the node polls, plus a name's key and LEZ's version, channel and the account's key read on a thread of their own (busy until back). The app asks again by itself every 1.5 s (up to 20 times). |  | 0 ms |
| `chain_send (send true)` | 15 s per node read (`node::QUICK`); 180 s while the node proves a send or claim (`node::PROVING`). By design: a press; nothing else is answered meanwhile. |  |  |
| `chain_split (send false)` | As the node polls, plus a name's key and LEZ's version, channel and the account's key read on a thread of their own (busy until back). The app asks again by itself every 1.5 s (up to 20 times). |  | 0 ms |
| `chain_split (send true)` | 15 s per node read (`node::QUICK`); 180 s while the node proves a send or claim (`node::PROVING`). By design: a press; nothing else is answered meanwhile. |  |  |
| `chain_stake` | About 0. The node is asked without waiting (`node::polled`, kept 5 s); busy before its first answer. The first ask after the host starts may hold the thread up to 1 s while the host acquires the node's object (`FIRST_ASK_WAIT`, then held 30 s if it did). |  | 0 ms |
| `chain_status` | About 0. The node is asked without waiting (`node::polled`, kept 5 s); busy before its first answer. The first ask after the host starts may hold the thread up to 1 s while the host acquires the node's object (`FIRST_ASK_WAIT`, then held 30 s if it did). |  | 0 ms |
| `chain_tx` | About 0. The node is asked without waiting (`node::polled`, kept 5 s); busy before its first answer. The first ask after the host starts may hold the thread up to 1 s while the host acquires the node's object (`FIRST_ASK_WAIT`, then held 30 s if it did). |  | 0 ms |
| `chat_names` | < 50 ms. Local files and memory only. |  | 0 ms |
| `clear_addresses` | 20 s for the CLI's accounts (`wallet::accounts`) plus 10 s per registry request, then the send. Wallet-files mode only; the app shows "Asking the wallet to sign…" at once. |  | 4001 ms |
| `cli_account_label` | < 50 ms. Local files and memory only. |  | 0 ms |
| `cli_account_new` | < 100 ms. Checks, then the names CLI runs as a job; the app polls `job`. |  | 0 ms |
| `cli_account_use` | < 100 ms. Checks, then the names CLI runs as a job; the app polls `job`. |  | 0 ms |
| `cli_accounts` | 600 ms at most, 400 ms for a whole burst (`served::WAIT_BUDGET`). Read on a thread of its own; busy until the first read is back. |  | 31 ms |
| `cli_add_name` | < 100 ms. Checks, then the names CLI runs as a job; the app polls `job`. |  | 11 ms |
| `cli_balance` | 600 ms at most, 400 ms for a whole burst (`served::WAIT_BUDGET`). Read on a thread of its own; busy until the first read is back. |  | 129 ms |
| `cli_bridge` | 120 s. | The CLI's bridge run (`BRIDGE_PATIENCE`). The Names app no longer calls it (it bridges with `chain_bridge`); tools do. | 3080 ms |
| `cli_bridge_to` | 120 s. | As `cli_bridge`. | 3093 ms |
| `cli_claim` | < 100 ms. Checks, then the names CLI runs as a job; the app polls `job`. |  | 0 ms |
| `cli_create_wallet` | < 100 ms. Checks, then the names CLI runs as a job; the app polls `job`. |  | 0 ms |
| `cli_faucet` | 30 s. | The Names faucet (LEZ 0.3) is asked on the call's thread; a press. Not measured (a real URL). |  |
| `cli_fee` | 600 ms at most, 400 ms for a whole burst (`served::WAIT_BUDGET`). Read on a thread of its own; busy until the first read is back. |  | 8 ms |
| `cli_install` | 20 s on LEZ 0.3 (an existing CLI's `--version`); on LEZ 0.4 busy-free (checked on its own thread). | A press. Not measured (it would download). |  |
| `cli_install_bundled` | < 50 ms. Local files and memory only. |  | 0 ms |
| `cli_mine` | 600 ms at most, 400 ms for a whole burst (`served::WAIT_BUDGET`). Read on a thread of its own; busy until the first read is back. |  | 405 ms |
| `cli_pay` | < 100 ms. Checks, then the names CLI runs as a job; the app polls `job`. |  | 22 ms |
| `cli_pay_account` | < 100 ms. Checks, then the names CLI runs as a job; the app polls `job`. |  | 0 ms |
| `cli_pay_public` | < 100 ms. Checks, then the names CLI runs as a job; the app polls `job`. |  | 0 ms |
| `cli_profile` | < 100 ms. Checks, then the names CLI runs as a job; the app polls `job`. |  | 0 ms |
| `cli_receive` | < 100 ms. Checks, then the names CLI runs as a job; the app polls `job`. |  | 0 ms |
| `cli_receive_all` | < 100 ms. Checks, then the names CLI runs as a job; the app polls `job`. |  | 0 ms |
| `cli_receive_privately` | < 100 ms. Checks, then the names CLI runs as a job; the app polls `job`. |  | 0 ms |
| `cli_register` | < 100 ms. Checks, then the names CLI runs as a job; the app polls `job`. |  | 13 ms |
| `cli_restore_wallet` | 20 s. | The CLI is asked whether a wallet already opens there (it refuses to restore over one); a press. | 3085 ms |
| `cli_set_page` | < 100 ms. Checks, then the names CLI runs as a job; the app polls `job`. |  | 0 ms |
| `cli_set_text` | < 100 ms. Checks, then the names CLI runs as a job; the app polls `job`. |  | 0 ms |
| `cli_shield` | < 100 ms. Checks, then the names CLI runs as a job; the app polls `job`. |  | 0 ms |
| `cli_status` | < 50 ms. Local files and memory only. |  | 0 ms |
| `cli_token_holdings` | < 50 ms. Local files and memory only. |  | 0 ms |
| `cli_token_info` | < 50 ms. Local files and memory only. |  | 0 ms |
| `cli_token_pending` | < 50 ms. Local files and memory only. |  | 0 ms |
| `cli_token_receive` | < 100 ms. Checks, then the names CLI runs as a job; the app polls `job`. |  | 0 ms |
| `cli_token_send` | < 100 ms. Checks, then the names CLI runs as a job; the app polls `job`. |  | 20 ms |
| `cli_token_send_source` | < 100 ms. Checks, then the names CLI runs as a job; the app polls `job`. |  | 0 ms |
| `cli_transfer` | 10 s. | Before the job, the registry is asked whether the new owner's account was ever used (`REQUEST_CAP`); a press. | 4001 ms |
| `declare_programs` | < 50 ms. Local files and memory only. |  | 0 ms |
| `deposit` | As `served`; for the Names app only. Another app's call reads on the spot: up to 10 s per registry request (`REQUEST_CAP`) plus about 1 s of Argon2 for a name not seen yet. |  | 0 ms |
| `dm_names` | < 50 ms. Local files and memory only. |  | 0 ms |
| `drop_access` | < 50 ms. Local files and memory only. |  | 13 ms |
| `faucet_claim` | 600 ms (first ask), then about 0. | The faucet is asked on a thread of its own; the app asks again for the answer. | 401 ms |
| `faucet_info` | 600 ms at most, 400 ms for a whole burst (`served::WAIT_BUDGET`). Read on a thread of its own; busy until the first read is back. |  | 1 ms |
| `file_sha256` | 600 ms at most, 400 ms for a whole burst (`served::WAIT_BUDGET`). Read on a thread of its own; busy until the first read is back. |  | 2 ms |
| `get_config` | < 50 ms. Local files and memory only. |  | 2 ms |
| `granted_accounts` | < 50 ms. Local files and memory only. |  | 0 ms |
| `job` | < 50 ms. Local files and memory only. |  | 0 ms |
| `keep` | 600 ms at most, 400 ms for a whole burst (`served::WAIT_BUDGET`). Read on a thread of its own; busy until the first read is back. |  | 0 ms |
| `main_name` | < 50 ms. Local files and memory only. |  | 0 ms |
| `make_anon` | < 100 ms. Checks, then the names CLI runs as a job; the app polls `job`. |  | 0 ms |
| `make_public` | < 100 ms. Checks, then the names CLI runs as a job; the app polls `job`. |  | 0 ms |
| `mine` | 600 ms at most, 400 ms for a whole burst (`served::WAIT_BUDGET`). Read on a thread of its own; busy until the first read is back. |  | 5 ms |
| `normalize` | < 50 ms. Local files and memory only. |  | 0 ms |
| `on_context_ready` | < 50 ms. Local files and memory only. | Once, as the module starts: nothing is read. |  |
| `page` | As `served`; for the Names app only. Another app's call reads on the spot: up to 10 s per registry request (`REQUEST_CAP`) plus about 1 s of Argon2 for a name not seen yet. |  | 11 ms |
| `page_preview` | < 50 ms. Local files and memory only. |  | 0 ms |
| `page_view` | As `served`; for the Names app only. Another app's call reads on the spot: up to 10 s per registry request (`REQUEST_CAP`) plus about 1 s of Argon2 for a name not seen yet. |  | 3 ms |
| `pending_inscriptions` | < 50 ms. Local files and memory only. |  | 0 ms |
| `pending_requests` | < 50 ms. Local files and memory only. |  | 1 ms |
| `private_call_control` | < 50 ms. Local files and memory only. |  | 0 ms |
| `propose_call` | < 50 ms. Local files and memory only. |  | 8 ms |
| `propose_inscription` | < 50 ms. Local files and memory only. |  | 9 ms |
| `propose_private_call` | < 50 ms. Local files and memory only. |  | 0 ms |
| `propose_relayed_call` | 12 s (the CLI's plan) plus 600 ms for a Config. | Another app's press; it waits 75 s. | 0 ms |
| `propose_token_source` | < 50 ms. Local files and memory only. |  | 0 ms |
| `propose_transfer` | < 50 ms. Local files and memory only. |  | 9 ms |
| `receive_privately` | < 50 ms. Local files and memory only. |  | 0 ms |
| `recipient` | As `served`; for the Names app only. Another app's call reads on the spot: up to 10 s per registry request (`REQUEST_CAP`) plus about 1 s of Argon2 for a name not seen yet. |  | 183 ms |
| `register_name` | 20 s for the CLI's accounts (`wallet::accounts`) plus 10 s per registry request, then the send. Wallet-files mode only; the app shows "Asking the wallet to sign…" at once. |  | 3064 ms |
| `reject_request` | < 50 ms. Local files and memory only. |  | 0 ms |
| `release` | 20 s for the CLI's accounts (`wallet::accounts`) plus 10 s per registry request, then the send. Wallet-files mode only; the app shows "Asking the wallet to sign…" at once. |  | 4001 ms |
| `remove_token_source` | < 50 ms. Local files and memory only. |  | 11 ms |
| `request_access` | < 50 ms. Local files and memory only. |  | 0 ms |
| `request_access_with` | < 50 ms. Local files and memory only. |  | 0 ms |
| `request_status` | < 50 ms. Local files and memory only. |  | 0 ms |
| `resign_relayed_call` | 12 s (the CLI's plan) + 12 s (its sign). | Another app's press. | 0 ms |
| `resolve` | As `served`; for the Names app only. Another app's call reads on the spot: up to 10 s per registry request (`REQUEST_CAP`) plus about 1 s of Argon2 for a name not seen yet. |  | 42 ms |
| `reveal_name` | < 50 ms. Local files and memory only. |  | 0 ms |
| `reverse` | As `served`; for the Names app only. Another app's call reads on the spot: up to 10 s per registry request (`REQUEST_CAP`) plus about 1 s of Argon2 for a name not seen yet. |  | 0 ms |
| `revoke_app` | < 50 ms. Local files and memory only. |  | 10 ms |
| `revoke_secret` | < 50 ms. Local files and memory only. |  | 8 ms |
| `running_jobs` | < 50 ms. Local files and memory only. |  | 0 ms |
| `secret_grants` | < 50 ms. Local files and memory only. |  | 0 ms |
| `set_account` | 20 s for the CLI's accounts (`wallet::accounts`) plus 10 s per registry request, then the send. Wallet-files mode only; the app shows "Asking the wallet to sign…" at once. |  | 4075 ms |
| `set_bridge_program` | < 50 ms. Local files and memory only. |  | 8 ms |
| `set_bridge_programs` | < 50 ms. Local files and memory only. |  | 8 ms |
| `set_chat_name` | < 50 ms. Local files and memory only. |  | 9 ms |
| `set_cli` | < 50 ms. Local files and memory only. |  | 15 ms |
| `set_cli_wallet` | < 50 ms. Local files and memory only. |  | 10 ms |
| `set_dm_name` | < 50 ms. Local files and memory only. |  | 8 ms |
| `set_main_name` | < 50 ms. Local files and memory only. |  | 7 ms |
| `set_names_network` | < 50 ms. Local files and memory only. |  | 17 ms |
| `set_page` | 20 s for the CLI's accounts (`wallet::accounts`) plus 10 s per registry request, then the send. Wallet-files mode only; the app shows "Asking the wallet to sign…" at once. |  | 4001 ms |
| `set_primary` | 20 s for the CLI's accounts (`wallet::accounts`) plus 10 s per registry request, then the send. Wallet-files mode only; the app shows "Asking the wallet to sign…" at once. |  | 4001 ms |
| `set_private_keys` | < 50 ms. Local files and memory only. |  | 0 ms |
| `set_profile` | 20 s for the CLI's accounts (`wallet::accounts`) plus 10 s per registry request, then the send. Wallet-files mode only; the app shows "Asking the wallet to sign…" at once. |  | 3061 ms |
| `set_registry` | < 50 ms. Local files and memory only. |  | 0 ms |
| `set_sequencer` | 5 s. | The new sequencer is asked its channel (`CHANNEL_WAIT`); a press, from tools only (the app never calls it). | 24 ms |
| `set_text` | 20 s for the CLI's accounts (`wallet::accounts`) plus 10 s per registry request, then the send. Wallet-files mode only; the app shows "Asking the wallet to sign…" at once. |  | 4001 ms |
| `set_token_program` | < 50 ms. Local files and memory only. |  | 8 ms |
| `set_wallet_files` | < 50 ms. Local files and memory only. |  | 9 ms |
| `settings` | 600 ms at most, 400 ms for a whole burst (`served::WAIT_BUDGET`). Read on a thread of its own; busy until the first read is back. |  | 0 ms |
| `spell_check` | About 200 ms. | The word list is checked in memory (the first call loads it). | 200 ms |
| `token_source_holdings` | < 50 ms. Local files and memory only. |  | 0 ms |
| `token_sources` | < 50 ms. Local files and memory only. |  | 0 ms |
| `transfer` | 20 s for the CLI's accounts (`wallet::accounts`) plus 10 s per registry request, then the send. Wallet-files mode only; the app shows "Asking the wallet to sign…" at once. |  | 4001 ms |
| `wallet` | 600 ms at most, 400 ms for a whole burst (`served::WAIT_BUDGET`). Read on a thread of its own; busy until the first read is back. |  | 0 ms |
| `watch_token` | < 50 ms. Local files and memory only. |  | 0 ms |
| `watched_tokens` | < 50 ms. Local files and memory only. |  | 0 ms |
| `zone_add` | 5 s. | The new zone's sequencer is asked its channel (`CHANNEL_WAIT`); a press. | 0 ms |
| `zone_list` | < 50 ms. Local files and memory only. |  | 25 ms |
| `zone_remove` | < 50 ms. Local files and memory only. |  | 0 ms |
| `zone_rename` | < 50 ms. Local files and memory only. |  | 0 ms |
| `zone_sequencer` | 10 s. | Home's channel when not known yet, then the new address's (`CHANNEL_WAIT` each); a press. | 0 ms |
| `zone_use` | < 50 ms. Local files and memory only. |  | 0 ms |


### What to watch in the owner's Basecamp log after installing this build

The log has the names host's warnings (`[logos] [names] Warning: …`) and Basecamp's own lines. The
names host's debug lines do not reach it. Read it from the `Module loaded: names` line on:

1. **The stall is gone if** none of these pairs appear in the first minute: an unprefixed
   `RemoteLogosObject: async callMethod timed out` followed by `qml: names_ui: <call>: No answer
   from the wallet yet…`. Each such pair is a call that waited past Basecamp's 20 s. Busy lines
   are expected and fine: `qml: names_ui: CLI balance: The wallet is busy; still working.`
2. **The node, when slow or silent:** at most one `[names] Warning: RemoteLogosObject: async
   callMethod timed out` about 1 s after the Names app opens (the first ask's short wait), then
   at most one every 30 s or so (each ask may wait 15 s; a module that gave no answer is left
   alone for 30 s). Many of these in a row would mean the hold isn't working.
3. **The node's object, acquired once (the client kept per module):** a
   `[names] Warning: RemoteTransportConnection: Timeout waiting for replica: "blockchain_module"`
   means acquiring it held the names thread for the ask's wait (1 s for the first ask). It may
   appear once after the host starts, or after the node is reloaded. Seen every few seconds, it
   would mean the client isn't being kept. Then report it with the timestamps.
4. `RemoteTransportConnection: no listener at … Is the module loaded?` must never appear for
   blockchain_module from names: a node that isn't loaded is known from its endpoint first
   (`peer.rs`).
5. `[names] Warning: RemoteLogosObject: callRemoteMethod timed out` (the synchronous form) should
   now appear only after a press on the node (a send, a claim, an account made or renamed, an
   inscription), never while Names only opens and polls.
6. **Timings:** `qml: names_ui: wallet has N funded account(s)` should follow the Names app's
   opening by about the time one names CLI run takes on that wallet. No `app_grants`,
   `backup_status`, `pending_requests`, `pending_inscriptions` or `the shipped lez_bridge.lgx
   can't be checked` line should say "No answer".


### What a kept read never does (names-stall-3 review, 2026-11-19)

- **No failure kept long:** a node read that failed, or a transaction the node hasn't stored
  yet (null), is asked again after 5 s (`node::KEEP`). Only a real block or transaction is kept
  for the hour.
- **Nothing decided from a read still on its way:** a Logos send stays "sending" until the node
  has answered (never "unknown" from a busy read). A cut-off deposit stays "sending" until its walk
  down the chain has found it (or walked all 600 blocks); the walk goes on by itself between
  polls (`node::deposit_walk`), so it settles with its hash and fee.
- **No read from before a change comes back after it:** the CLI's 15 s balance cache and its slow
  reads (`BALANCE_GEN`), the background reads (`BoardRead::reading`), the node's kept answers and
  the deposit walks (`node::forget`'s generation) drop an answer started before the change.
- **After a claim, a burn or a faucet grant:** the bridge network's balances are read afresh
  (`bridge_net_balances` with `{"accounts": […], "fresh": true}`, and on every approval). "Now
  holds" is said only from a read made after the claim; the card asks again every 3 s for up to
  2 min until there is one.
- **The short first ask (1 s):** it bounds the host acquiring the node's object. Running out says
  nothing about the node: it neither holds the module nor is kept as "didn't answer", and the next
  ask waits the full 15 s.
- **Bridge to Ethereum submits the amount it shows:** before planning, it reads the bridge's
  amounts again and sends the index that list gives the shown amount. If that list no longer has
  it, nothing is done and the person chooses again. A change of bridge drops the amounts held,
  and a new send-back reads them afresh. "Change" shows whenever there is another amount or
  another account (public or private) to choose.
