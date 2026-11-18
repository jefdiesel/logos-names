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
| 701 | loadCliBalance | askLater | cli_balance | poll: keeps the balance shown (15 s timer) |
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
| 1839 | loadCliStatus | askLater | cli_status | poll: keeps the last status (a busy read is never "no wallet", so never Welcome); 15 s |
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
