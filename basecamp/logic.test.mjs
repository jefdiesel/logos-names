// Checks the view's own logic (qml/logic.mjs, which qml/Main.qml imports), since the QML
// engine cannot run in CI. Run with: node basecamp/logic.test.mjs
import { atLeast, someAmount, entryShown, amountRead, amountProblem, amountDigits, AMOUNT_MAX, CHAIN_AMOUNT_MAX, jobProblem, tokenToProblem, tokenSendProves, tokenShown, feeEstimate, feePaid, accountTitle, accountsShown, publisherLine, durationWords, keyShown, chainToProblem, stakeLines, rewardsLine, grouped, addDigits, walletAssets, subDigits, walletGroups, stakeTable, activityLine, activityFor, agoShown, readNote, chainKeyTitle, chainKeySub, chainUsable, chainLaterNote } from "./qml/logic.mjs";
import { readFileSync } from "node:fs";
import { cliOutdatedLine, replyKind, revealNext, revealLine, mergeCliStatus, requestTickKey, normalizedReply, noWalletSaid, onNet, preTicked, sameNetAnswer, askAgainAfter, REVEAL_REFUSED_RETRY_MS } from "./qml/logic.mjs";
import { bridgeReply, claimStage, claimAccountProblem, claimBridge, claimJobFor, runningClaimJob, claimPhases, claimFailure, claimErrorTitle, claimInput, otherDeployment } from "./qml/logic.mjs";
import { publicAccounts, decimalShown, bridgeNetStatus, bridgeNetRows, bridgeNetShown, publicIdProblem, NET_03, NET_04, mergedAccounts } from "./qml/logic.mjs";

let failures = 0;
const check = (label, got, want) => {
  const ok = JSON.stringify(got) === JSON.stringify(want);
  if (!ok) failures++;
  console.log((ok ? "ok   " : "FAIL ") + label + (ok ? "" : `\n   got  ${JSON.stringify(got)}\n   want ${JSON.stringify(want)}`));
};
// Balances and deposits are decimal text, past what a JS number holds exactly.
check("atLeast more digits", atLeast("10", "9"), true);
check("atLeast fewer digits", atLeast("9", "10"), false);
check("atLeast equal", atLeast("1000000", "1000000"), true);
check("atLeast leading zeros", [atLeast("007", "7"), atLeast("7", "008")], [true, false]);
check("atLeast past 2^53", atLeast("100000000000000000000", "99999999999999999999"), true);
check("atLeast just short", atLeast("99999999999999999998", "99999999999999999999"), false);
check("someAmount", ["1", "000", "0", "", null, undefined, "10"].map(someAmount), [true, false, false, false, false, false, true]);
// A long unbroken value (a key, a CID) is shortened in the middle; text and links are not.
check("entryShown key", entryShown("a".repeat(30) + "b".repeat(30)), "aaaaaaaaaaaa…bbbbbbbb");
check("entryShown short", entryShown("short value"), "short value");
check("entryShown sentence", entryShown("a bio that runs on for quite a while, longer than forty"), "a bio that runs on for quite a while, longer than forty");
check("entryShown link", entryShown("https://example.com/" + "x".repeat(40)), "https://example.com/" + "x".repeat(40));

// The Logos Blockchain: keys, durations and the stake, as chain_stake gives it.
const KEY = "66ecf4bccaa13e4e03c3d5c0e3ebecb945c51bbf815e92727a9a4efa6b39a81b";
check("durationWords", [30, 600, 113980, 400000].map(durationWords), ["30 s", "10 min", "31.7 h", "4.6 days"]);
check("keyShown", keyShown(KEY), "66ecf4bc…a81b");
check("grouped", [grouped("34002373886"), grouped(999), grouped("")], ["34,002,373,886", "999", ""]);

// A read the sequencer only partly answered, or a list kept from before, says so beside its
// value (cli-mine-stale-list-unmarked-during-outage, balance-accounts-zeroed-with-success-on-partial-failure).
check("readNote whole read", readNote({ success: true, value: [], error: null }), "");
check("readNote failure", readNote({ success: false, value: null, error: "could not reach the sequencer" }), "");
check("readNote no reply", [readNote(null), readNote(undefined)], ["", ""]);
check("readNote partial", readNote({ success: true, value: {}, partial: true, unread: ["Public/x"], warning: "The sequencer didn't give the balance of Public/CCVd…pKnk (it answered HTTP 503 instead of a reading), so its balance shows as unknown. Try again shortly." }),
  "The sequencer didn't give the balance of Public/CCVd…pKnk (it answered HTTP 503 instead of a reading), so its balance shows as unknown. Try again shortly.");
check("readNote stale", readNote({ success: true, value: [], stale: true, as_of: 1, warning: "Your names as read 3 min ago. The sequencer didn't answer." }), "Your names as read 3 min ago. The sequencer didn't answer.");
check("readNote stale without words", readNote({ success: true, value: [], stale: true }), "This may be out of date.");
check("chainTo good", [KEY, "0x" + KEY, "alice", "@Alice", "alice.lez"].map((t) => chainToProblem(t)), ["", "", "", "", ""]);
check("chainTo short key", chainToProblem(KEY.substr(0, 60)), "A Logos Blockchain key is 64 hex characters; that one has 60.");
check("chainTo empty", chainToProblem("  "), "Enter a key or a name.");
check("stakeLines one key", stakeLines({ staked: "34002373886", notes: 4538, keys: [KEY], epoch: 7, next_epoch_in_s: 113980 }), [
  "34,002,373,886 staked in 4538 notes on 66ecf4bc…a81b.",
  "Epoch 7; the next starts in about 31.7 h. New tokens join the stake at an epoch boundary, up to two epochs after they arrive.",
]);
check("stakeLines no epoch length", stakeLines({ staked: "0", notes: 0, keys: [], epoch: 7, next_epoch_in_s: null })[1], "Epoch 7. New tokens join the stake at an epoch boundary, up to two epochs after they arrive.");
check("rewardsLine", [rewardsLine({ vouchers: 0 }), rewardsLine({ vouchers: 2, claimable: "26280" }), rewardsLine({ vouchers: 1722, claimable: "65436", reward_per_voucher: "38" })], ["Nothing to claim. Each block your node leads earns a reward to claim here.", "26,280 to claim from 2 blocks your node led.", "65,436 to claim from 1722 blocks your node led. Each claim cashes one block's reward (38) and pays its own fee, which can be more than that."]);

// Every amount field reads amounts as the names CLI and the module do, from the same
// table (amount-comma-stripped-in-bridge-token-send: "1,5" was sent as 15).
const amounts = JSON.parse(readFileSync(new URL("../basecamp-module/rust-lib/src/amounts.json", import.meta.url), "utf8"));
const misread = amounts.accept.filter(([typed, want]) => JSON.stringify(amountRead(typed)) !== JSON.stringify({ digits: want, problem: "" }))
  .concat(amounts.refuse.filter(([typed, want]) => JSON.stringify(amountRead(typed)) !== JSON.stringify({ digits: "", problem: want })));
check("amounts.json read as the CLI and the module read it (" + (amounts.accept.length + amounts.refuse.length) + " cases)", misread, []);
check("amount null", amountRead(null), { digits: "", problem: "Enter an amount, such as 1,000." });
check("amount helpers", [amountDigits("1,000"), amountDigits("1,5"), amountProblem("1,000"), amountProblem("1,5")], ["1000", "", "", "1,5 isn't a whole number: commas go only between groups of three digits, such as 1,000."]);
check("amount Logos Blockchain max", [amountDigits("18,446,744,073,709,551,615", CHAIN_AMOUNT_MAX), amountProblem("18446744073709551616", CHAIN_AMOUNT_MAX)], ["18446744073709551615", "18446744073709551616 is more than an amount can be."]);
check("amount max", amountDigits(AMOUNT_MAX), AMOUNT_MAX);
const ok = (problem) => problem === "";
// transfer-failure-text-is-last-cause-line: a failed job told its last cause line.
const downLog = "Error: failed to read the account from the sequencer\n\nCaused by:\n    0: client error (Connect)\n    1: tcp connect error\n    2: Connection refused (os error 61)\n";
check("jobProblem", [jobProblem({ problem: "The payment didn't go through: boom", log_tail: downLog }, "x"), jobProblem({ log_tail: downLog }, "It didn't go through"), jobProblem({ log_tail: "killed\n" }, "It didn't go through"), jobProblem({}, "It didn't go through")],
  ["The payment didn't go through: boom", "It didn't go through: failed to read the account from the sequencer", "It didn't go through: killed", "It didn't go through: it stopped without saying why"]);
const id = "CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk";
check("tokenTo good", ["ada", "Jef.lez", "@ada", "Public/" + id, "Private/" + id].map((t) => ok(tokenToProblem(t))), [true, true, true, true, true]);
check("tokenTo bad", ["", id, "Public/nope", "Public/" + id + "/x", "a b", "$(id)"].map((t) => ok(tokenToProblem(t))), [false, false, false, false, false, false]);
check("tokenTo bare id says how", tokenToProblem(id).includes("Public/"), true);
check("tokenSendProves", [tokenSendProves(true, "Public/" + id), tokenSendProves(false, "Public/" + id), tokenSendProves(false, "ada"), tokenSendProves(false, "Private/" + id)], [true, false, true, true]);
check("tokenShown", [tokenShown("Demo"), tokenShown(""), tokenShown(null), tokenShown("  ")], ["Demo", "unnamed token", "unnamed token", "unnamed token"]);

// Qt's QML parser on Linux refuses `x.public` ("Expected token 'identifier'") and the whole
// app then fails to load, so reserved words are read as x["public"]. Strings are blanked
// first, so a word inside text doesn't count.
const reserved = /\.(public|private|protected|package|interface|implements|static|let|yield|enum|class|const|export|import|extends|super)\b/;
// Fees: what the quote and a finished job's `fee` say, as the app shows them.
const quote = { public_transfer: { about: "8000", at_most: "18000" }, private_transfer: { about: "1234567", at_most: "1234567" } };
check("feeEstimate public", feeEstimate(quote, false), "about 8,000 (at most 18,000 held while it runs)");
check("feeEstimate private, no headroom", feeEstimate(quote, true), "about 1,234,567");
check("feeEstimate private, headroom", feeEstimate({ private_transfer: { about: "5", at_most: "9" } }, true), "about 5 (at most 9 if fees climb before it lands)");
check("feeEstimate missing", [feeEstimate(null, false), feeEstimate({}, true), feeEstimate({ public_transfer: { about: "x", at_most: "1" } }, false)], ["", "", ""]);
check("feePaid", [feePaid({ paid: "7950" }), feePaid({ paid: null }), feePaid(null)], ["Fee paid: 7,950.", "", ""]);
// Accounts: private first, each kind's default leading it.
const rows = [
  { account: "Public/a", kind: "public", label: null, default: false },
  { account: "Private/b", kind: "private", label: "Savings", default: false },
  { account: "Public/c", kind: "public", label: "Main", default: true },
  { account: "Private/d", kind: "private", label: " ", default: true },
];
check("accountsShown order", accountsShown(rows).map((r) => r.account), ["Private/d", "Private/b", "Public/c", "Public/a"]);
// Nicknames never title an account: only a registered name does (walletGroups).
check("accountTitle", rows.map(accountTitle), ["Public account", "Private account", "Public account", "Private account"]);
check("accountsShown not a list", accountsShown(null), []);
// The approval sheet's publisher line (the Program Registry's verify_app).
check("publisher verified", publisherLine({ status: "verified", publisher: "forumer" }), { text: "forumer ✓ verified publisher", tone: "good" });
// S-4: normalize answers plain text; a busy answer is never a name ("[object Object]").
check("normalize replies", ["jef", "", JSON.stringify({ success: false, busy: true, value: null, error: "The wallet is busy; still working." }), undefined, "Module not connected", "{\"error\":\"timeout\"}"].map((r) => { const a = normalizedReply(r); return a.kind + ":" + a.value; }),
  ["ok:jef", "ok:", "busy:null", "busy:null", "missing:null", "busy:null"]);
// Busy (not taken) is told apart from no answer (may have been taken).
check("busy not taken vs no answer", [JSON.stringify({ success: false, busy: true, value: null, error: "The wallet is busy; still working." }), "{\"error\":\"timeout\"}", undefined].map((r) => replyKind(r).kind + ":" + (replyKind(r).notTaken === true)),
  ["busy:true", "busy:false", "busy:false"]);
// Welcome only when the module says there is no wallet; a wallet with no name reaches the Wallet.
check("no wallet said", [null, undefined, {}, { wallet: true, public: "A" }, { wallet: true }, { wallet: false }, { wallet: false, old_wallet: true }].map(noWalletSaid),
  [false, false, false, false, false, true, true]);
// S-A: a public account read on LEZ 0.3 is never carried into a LEZ 0.4 answer, which can't read its own.
check("cli status across networks", mergeCliStatus(onNet({ wallet: true, public: "Public/A03" }, "0.3"), onNet({ wallet: true, public: null }, "0.4"))["public"], null);
{
  // A late answer: asked on LEZ 0.4, the person switched to LEZ 0.3, then the 0.4 answer arrives.
  let shown = null, net = "0.4";
  const askedOn = net;
  const done = (value) => { if (sameNetAnswer(askedOn, net)) shown = value; };
  net = "0.3";
  done({ wallet: true, public: "Public/A04" });
  check("late answer after a switch is dropped", shown, null);
  check("an answer on the same network is shown", sameNetAnswer("0.3", "0.3"), true);
}
check("cli status same network", mergeCliStatus(onNet({ wallet: true, public: "Public/A04" }, "0.4"), onNet({ wallet: true, public: null }, "0.4"))["public"], "Public/A04");
{
  const one = [{ account: "Public/A", granted: true }, { account: "Public/B", granted: false }];
  const two = [{ account: "Public/A", granted: true }, { account: "Public/B", granted: true }];
  check("pre-ticks", [
    preTicked({ kind: "programs", choices: one }, false),
    preTicked({ kind: "programs", choices: two }, false),
    preTicked({ kind: "token_source", choices: two }, false),
    preTicked({ kind: "programs", choices: one, checking: true }, false),
    preTicked({ kind: "programs", choices: one }, true),
    preTicked({ kind: "connect", choices: [{ account: "Public/B", granted: false }] }, false),
    preTicked({ kind: "connect", choices: one, checking: true }, false),
    preTicked({ kind: "transfer", account: "Public/B", choices: two }, false),
    preTicked({ kind: "transfer", choices: one }, false),
  ], [["Public/A"], [], [], [], [], [], [], ["Public/B"], []]);
}
{
  // The 0.4 card: the wallet's own account first with Copy; a LEZ 0.3 one view only, no Copy; one
  // the 0.4 wallet also holds (a restored phrase) once, as its own.
  const A04 = "Public/" + "A".repeat(43), B03 = "Public/" + "B".repeat(43), C = "Public/" + "C".repeat(43);
  const read = { accounts: [{ account: B03, found: true, native: "5" }, { account: C, found: true, native: "7" }, { account: A04, found: false }] };
  const rows = bridgeNetRows(read, [A04, C]);
  check("0.4 card order", rows.map((r) => r.account), [C, A04, B03]);
  check("0.4 card copy", rows.map((r) => r.copyable), [true, true, false]);
  check("0.4 card view only title", rows[2].title.startsWith("LEZ 0.3 account ") && rows[2].title.endsWith("view only: your LEZ 0.4 wallet can't sign for it"), true);
  check("0.4 card own once", rows.filter((r) => r.account === C).length, 1);
  check("0.4 card no wallet yet: all view only", bridgeNetRows(read, []).map((r) => r.viewOnly), [true, true, true]);
  check("0.3 card unchanged", bridgeNetRows(read).map((r) => r.copyable), [true, true, true]);
}
{
  // SF-A: a refused reveal (alice's commitment never landed) doesn't block bob's behind it.
  const pending = [{ name: "alice", ready_at: 1000 }, { name: "bob", ready_at: 1000 }];
  check("first due", revealNext(pending, 5000, 0, 200000, {}).name, "alice");
  const refused = { alice: 5000 + REVEAL_REFUSED_RETRY_MS };
  check("refused one waits, the next goes", revealNext(pending, 5000, 0, 200000, refused).name, "bob");
  check("asked again after its wait", revealNext(pending, 5001 + REVEAL_REFUSED_RETRY_MS, 0, 200000, refused).name, "alice");
  // SF-D: a busy or unanswered `mine` is asked again; a refusal isn't.
  check("mine asked again", ["busy", "none", "refused"].map((m) => askAgainAfter(m, "busy", "none")), [true, true, false]);
}
{
  // Claim a bridge deposit: the lez_bridge module's replies (Basecamp 0.3.2 encodes them twice).
  const twice = (v) => JSON.stringify(JSON.stringify(v));
  check("bridge reply ok", bridgeReply(twice({ success: true, value: { a: 1 }, error: null })), { ok: true, value: { a: 1 } });
  check("bridge reply error", bridgeReply(twice({ success: false, value: null, error: "no" })).kind, "error");
  check("bridge reply kinds", ["{\"error\":\"timeout\"}", "{\"error\":\"Module not connected\"}", "{\"error\":\"Method not found\"}", "", undefined].map((r) => bridgeReply(r).kind),
    ["busy", "starting", "missing", "busy", "busy"]);
  const V2 = "JECKBkvoc2e3dC18Wm6zZJEC2BGMwUwLsdZiQzYTnUx7", V3 = "4eaeK3yiqATXhpCEYEFMMtjUn1bS7HLxPwfrH31mXCvr";
  const labels = [[V2, "bridge v2"], [V3, "bridge v3"]];
  const ok = { ok: true, id: "rc-1", deposit: { account: "Public/A", leaf_index: 7, leaves: 9, amount_text: "0.1 ETH" }, call: { program: V3 }, checks: [] };
  // Every stage the screen has.
  check("stages", [
    claimStage({ missing: true }),
    claimStage({}),
    claimStage({ checking: true }),
    claimStage({ check: { ok: false, error: { code: "already_claimed" } } }),
    claimStage({ check: ok, accountProblem: "not in wallet" }),
    claimStage({ check: ok, bridgeProblem: "not read" }),
    claimStage({ check: ok }),
    claimStage({ check: ok, asking: true }),
    claimStage({ check: ok, needs: { needs: "connect" } }),
    claimStage({ check: ok, uncertain: { needs: "uncertain" } }),
    claimStage({ check: ok, job: { state: "running" } }),
    claimStage({ check: ok, job: { state: "done" } }),
    claimStage({ check: ok, job: { state: "failed" } }),
  ], ["missing", "start", "checking", "refused", "elsewhere", "wrong_bridge", "ready", "asking", "needs", "uncertain", "running", "done", "failed"]);
  // The account must be in this LEZ 0.4 wallet; the sentence says how a LEZ 0.3 one gets there.
  check("account in wallet", claimAccountProblem("Public/A", [{ account: "Public/A" }]), "");
  const away = claimAccountProblem("Public/B", [{ account: "Public/A" }]);
  check("account elsewhere", away.startsWith("This claim file pays Public/B, which isn't in your LEZ 0.4 wallet") && away.includes("LEZ 0.3 wallet") && away.includes("import-account-04.sh"), true);
  // Which bridge, and whether this wallet reads it.
  check("bridge v3 read", claimBridge(V3, [V2, V3], labels), { label: "bridge v3", problem: "" });
  check("bridge not read", claimBridge(V2, [V3], labels).problem.startsWith("This claim file is for bridge v2"), true);
  check("other deployment", [otherDeployment("bridge-test-network-v3"), otherDeployment("bridge-test-network")], ["bridge-test-network", "bridge-test-network-v3"]);
  // A claim already running or done for this file is found, never started twice.
  const jobs = [{ id: "j2", state: "done", account: "Public/A", leaf_index: 7 }, { id: "j1", state: "running", check: "rc-0" }];
  check("job for this file", claimJobFor(jobs, ok).id, "j2");
  check("job by check id", claimJobFor([{ id: "j3", check: "rc-1", state: "running" }], ok).id, "j3");
  check("no job for another leaf", claimJobFor([{ id: "j4", state: "running", account: "Public/A", leaf_index: 8 }], ok), null);
  check("running job on reopen", runningClaimJob(jobs).id, "j1");
  // Progress.
  check("phases relay", claimPhases({ state: "running", phase: "relay" }).map((p) => (p.done ? "d" : p.now ? "n" : "-")).join(""), "dn-");
  check("phases done", claimPhases({ state: "done", phase: "done" }).every((p) => p.done), true);
  // Busy (nothing sent: Claim again) vs no answer (a request may be waiting: look first).
  check("busy again", claimFailure("Your Names Wallet is busy; nothing was sent; try again.").again, true);
  const silent = claimFailure("Your Names Wallet didn't answer in time (x). Nothing was sent; try again.");
  check("no answer: look first", [silent.again, silent.line.includes("Look at the confirmation")], [false, true]);
  check("module busy", claimFailure("still answering an earlier call").again, true);
  check("error titles", [claimErrorTitle("already_claimed"), claimErrorTitle("other_vault"), claimErrorTitle("zzz")], ["Already claimed", "This deposit went to another vault", "The claim can't go on"]);
  check("claim input", ["", "{\"format\":\"logos-bridge-claim\"}", "{}", "/tmp/c.json", "file:///c.json", "hello"].map((t) => claimInput(t).kind + (claimInput(t).problem ? "!" : "")),
    ["empty", "json", "json!", "path", "path", "other!"]);
  // The access sheet ticks only the claim's account when the bridge asks, if the wallet has it.
  const access = { kind: "connect", app: "lez_bridge", choices: [{ account: "Public/M" }, { account: "Public/A" }] };
  check("claim account ticked", preTicked(access, false, "Public/A"), ["Public/A"]);
  check("hint not in wallet", preTicked(access, false, "Public/Z"), []);
  check("module hint", preTicked(Object.assign({ account_hint: "Public/M" }, access), false, ""), ["Public/M"]);
}
check("publisher checking", publisherLine({ status: "checking" }), { text: "Checking who publishes it…", tone: "plain" });
{
  // M-2: the ticks are set again whenever what the sheet knows changes, not only per request.
  const checking = { id: "r1", publisher: { status: "checking" }, checking: true };
  const fake = { id: "r1", publisher: { status: "mismatch" }, checking: false };
  const warned = { id: "r1", publisher: { status: "verified" }, checking: false, sheet: { warnings: ["unverified"] } };
  check("tick key changes on status", [requestTickKey(checking) !== requestTickKey(fake), requestTickKey(fake) !== requestTickKey(warned), requestTickKey(fake) === requestTickKey({ ...fake })], [true, true, true]);
  check("tick key of nothing", requestTickKey(null), "");
}
check("publisher copy", publisherLine({ status: "mismatch", publisher: "forumer" }).tone, "warn");
check("publisher unknown name", publisherLine({ status: "no_such_name", publisher: "x" }).text, "Claims x, which isn't a registered name.");
check("publisher unclaimed", publisherLine({ status: "unclaimed" }).tone, "plain");
check("publisher none", [publisherLine(null), publisherLine({ status: "other" }), publisherLine("x")], [null, null, null]);
const source = readFileSync(new URL("./qml/Main.qml", import.meta.url), "utf8").split("\n");
const dotted = source
  .map((line, i) => [i + 1, line.replace(/"(\\.|[^"\\])*"|'(\\.|[^'\\])*'|\/\/.*$/g, '""')])
  .filter(([, line]) => reserved.test(line))
  .map(([n]) => n);
check("no reserved word after a dot in Main.qml", dotted, []);
// The Wallet screen's rows.
check("addDigits", [addDigits("999", "1"), addDigits("34002373886", "1191220464"), addDigits("99999999999999999999", "1"), addDigits("x", "1")], ["1000", "35193594350", "100000000000000000000", null]);
{
  const rows = walletAssets({ balance: "250000000", public: { account: "C", balance: "1250000" } },
    [{ account: "Private/a", account_kind: "private", definition: "D1", name: "Gold", balance: "40" }], ["D1", "D2"], { D2: "Silver" },
    [{ key: KEY, balance: "5" }, { key: KEY, balance: "7" }], false);
  check("walletAssets ids", rows.map(r => r.id), ["lez-private", "lez-public", "token-D1-private", "token-D2", "chain"]);
  check("walletAssets watched token", [rows[3].title, rows[3].balance, rows[3].where], ["Silver", "0", "None yet"]);
  check("walletAssets chain total", [rows[4].balance, rows[4].stakable], ["12", true]);
  const down = walletAssets(null, null, [], {}, null, true);
  check("walletAssets node down", down.map(r => [r.id, r.balance, r.where]), [["chain", null, "Node not running"]]);
  check("walletAssets nothing yet", walletAssets(undefined, null, [], {}, null, false), []);
}
// The Wallet's groups and the Staking table.
check("subDigits", [subDigits("1000", "1"), subDigits("210939029", "210939029"), subDigits("5", "6"), subDigits("100000000000000000000", "1")], ["999", "0", null, "99999999999999999999"]);
{
  const accounts = [
    { account: "Public/CCVd", kind: "public", label: null, balance: "38858810170", default: true },
    { account: "Private/yGrP", kind: "private", label: null, balance: "40599000", default: true },
    { account: "Private/HqRv", kind: "private", label: null, balance: "1000", default: false },
  ];
  const stake = { staked: "210939029", claimable: "0", vouchers: 0 };
  const g = walletGroups(null, accounts, null, [], {}, [{ key: KEY, balance: "210939029" }], false, stake);
  check("walletGroups ids", g.map(x => x.id), ["lez", "chain"]);
  check("walletGroups lez rows", g[0].rows.map(r => [r.title, r.canSend]), [["Private account", true], ["Private account", false], ["Public account", true]]);
  check("walletGroups lez total", g[0].total, "38899410170");
  check("walletGroups all staked", g[1].note, "1 key on your node · all staked");
  const more = walletGroups(null, accounts, null, [], {}, [{ key: KEY, balance: "215939029" }], false, stake);
  check("walletGroups not counted yet", more[1].note, "1 key on your node · 5,000,000 not counted yet");
  check("walletGroups fallback", walletGroups({ balance: "5", account: "a", public: { account: "b", balance: "7" } }, null, null, [], {}, null, false, null).map(x => [x.id, x.total]), [["lez", "12"]]);
  // cli_balance with no default private account: account and balance null, the public one still shown.
  check("walletGroups fallback no private", walletGroups({ balance: null, account: null, public: { account: "b", balance: "7" }, waiting: null }, null, null, [], {}, null, false, null).map(x => [x.id, x.total, x.rows.map(r => r.id)]), [["lez", "7", ["lez-public"]]]);
  // LP-0022 accounts: labels title the rows; one the node takes at its next start can't send yet.
  const NEW = "ab".repeat(32);
  const labelled = walletGroups(null, null, null, [], {}, [{ key: KEY, balance: "5", label: "Main", ready: true }, { key: NEW, balance: null, title: "names-1", label: "", ready: false }], false, null);
  check("walletGroups chain labels", labelled[0].rows.map(r => [r.title, r.sub, r.canSend, r.balance]),
    [["Main", "Public · " + keyShown(KEY), true, "5"], ["Account", "Public · abababab…abab · your node uses it from its next start", false, null]]);
  check("walletGroups chain later", [labelled[0].note, labelled[0].total], ["2 keys on your node · 1 from its next start", "5"]);
  check("chainKeyTitle", [chainKeyTitle({ key: KEY }), chainKeyTitle({ key: KEY, label: "  " }), chainKeyTitle({ key: KEY, label: "Savings", title: "names-2" })], ["Node key", "Node key", "Savings"]);
  check("chainUsable", [chainUsable(null), chainUsable([{ key: KEY, balance: "5" }, { key: NEW, balance: null, ready: false }, { key: "cd", balance: "0", ready: true }]).map(a => a.key)], [null, [KEY, "cd"]]);
  check("chainKeySub ready", chainKeySub({ key: KEY, ready: true }), "Public · " + keyShown(KEY));
  // The account page offers no address, Receive or Bridge for an account the node takes at its
  // next start (verify render wallet-chain-notready-page showed Address + Copy and Receive).
  check("walletGroups chain later rows", labelled[0].rows.map(r => r.later), [false, true]);
  check("chainLaterNote", [chainLaterNote().includes("next start"), chainLaterNote().includes("don't have anything sent to it")], [true, true]);
  check("walletGroups node down", walletGroups(null, null, null, [], {}, null, true, null).map(x => [x.id, x.total, x.note]), [["chain", null, "Node not running"]]);
  // A name is matched to the account its profile's npk points at.
  const npkAccounts = accounts.concat([{ account: "Private/Jef1", kind: "private", label: null, balance: "7", default: false, npk: "AB".repeat(32) }]);
  const withNames = walletGroups(null, npkAccounts, null, [], {}, null, false, null, { ["ab".repeat(32)]: ["jefdiesel"] });
  const jefRow = withNames[0].rows.find(r => r.account === "Private/Jef1");
  check("walletGroups name row", [jefRow.title, jefRow.name, jefRow.sub], ["jefdiesel", "jefdiesel", "Private · receives payments to jefdiesel"]);
  check("walletGroups names count", withNames[0].note, "4 accounts · 1 name");
  // The paying account is shown as you, by your main name's display name.
  const asYou = walletGroups(null, accounts, null, [], {}, null, false, null, {}, "jefdiesel", "Jef");
  // The paying account isn't the main name's: no name, no picture (only the account a name is paid into carries it).
  check("walletGroups paying account unnamed", [asYou[0].rows[0].title, asYou[0].rows[0].name], ["Private account", ""]);
  check("stakeTable", stakeTable(stake, [{ key: KEY, balance: "215939029" }]), { held: "215939029", staked: "210939029", waiting: "5000000", claimable: "0", vouchers: 0 });
}

{
  const now = 1792000000000;
  const KEY = "7a6223abde07c0c278d203973fd233ab3e47b2bfa53c8ed3d74d5f453a552a2a";
  const bridge = { kind: "bridge", chain: "logos", from: KEY, to: "Public/9W9huNeAds11pdGpMijrdhTkSQ8upNudRnrZJbNNWfoW", amount: "1000000", state: "in_block", at: now - 2 * 3600 * 1000 };
  check("activityLine bridge", activityLine(bridge, now), { title: "Bridge to LEZ 0.3", sub: "from key 7a6223ab…2a2a into Public 9W9huNeA…WfoW", amount: "1,000,000", sign: "-", state: "In a block · arrives in about 12 h", tone: "warn", ago: "2 h ago" });
  check("activityLine arrived", activityLine({ kind: "arrived", chain: "lez", to: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", amount: "5", state: "arrived", at: now }, now).sign, "+");
  check("activityLine split", activityLine({ kind: "split", chain: "logos", from: KEY, amount: "1000000", fee: "2018", state: "in_block", at: now }, now).title, "Made a note of exactly 1,000,000");
  check("activityLine failed pay", activityLine({ kind: "pay", chain: "lez", name: "ada", amount: "7", state: "failed", at: now }, now).tone, "bad");
  check("activityLine receive_privately", [activityLine({ kind: "receive_privately", chain: "lez", name: "xferecho", fee: { about: "400000", at_most: "900000", paid: "417376" }, state: "done", at: now }, now).title, activityLine({ kind: "receive_privately", chain: "lez", name: "xferecho", fee: { about: "400000", at_most: "900000", paid: "417376" }, state: "done", at: now }, now).sub], ["Private payments to xferecho", "on LEZ 0.3, new receiving keys · fee 417,376"]);
  check("activityLine bridge not followed", activityLine(Object.assign({}, bridge, { to_before: null, followed: false }), now).state, "In a block · its arrival on LEZ 0.3 can't be followed here");
  check("activityLine nothing claimed", activityLine({ kind: "receive", chain: "lez", claimed: false, state: "done", at: now }, now).title, "Nothing claimed");
  // A send whose module restarted mid-way, or whose job record was pruned: not "failed", not "done".
  const unsure = activityLine({ kind: "pay_account", chain: "lez", to: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", amount: "100", state: "unknown", at: now }, now);
  check("activityLine unknown", [unsure.state, unsure.tone], ["Outcome unknown · check the balance", "warn"]);
  // An arrival whose window had a fee the module couldn't know claims no figure, only a least one.
  const least = activityLine({ kind: "arrived", chain: "lez", to: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", amount: null, at_least: "95648", state: "arrived", at: now }, now);
  check("activityLine arrived at least", [least.amount, least.sign, least.sub], ["", "", "in Public CCVd76Tk…pKnk on LEZ 0.3 · at least 95,648"]);
  check("activityFor", activityFor([bridge, { kind: "pay", name: "ada" }], KEY).length, 1);
  // Zones (LP-0022): the same keys are the same LEZ account ID on every zone, so an account's
  // page lists the zone in use only; a Logos Blockchain key's page and a name's are not narrowed.
  {
    const ACC = "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk";
    const B = "0303030303030303030303030303030303030303030303030303030303030303";
    const rows = [
      { id: "h", kind: "pay_account", chain: "lez", from: ACC, amount: "1" },
      { id: "b", kind: "pay_account", chain: "lez", zone: B, from: ACC, amount: "2" },
      { id: "bh", kind: "bridge", chain: "logos", from: KEY, to: ACC, amount: "3" },
      { id: "bb", kind: "bridge", chain: "logos", zone: B, from: KEY, to: ACC, amount: "4" },
    ];
    const ids = (who, zone) => activityFor(rows, who, zone).map((e) => e.id);
    check("activityFor on home", ids(ACC, "home"), ["h", "bh"]);
    check("activityFor on a zone", ids(ACC, B), ["b", "bb"]);
    check("activityFor with no zone given", ids(ACC), ["h", "b", "bh", "bb"]);
    check("activityFor a chain key is every zone's", ids(KEY, B), ["bh", "bb"]);
    const line = (fields) => activityLine(Object.assign({ kind: "pay", chain: "lez", name: "ada", amount: "7", state: "done", at: now }, fields), now).sub;
    check("activityLine names the zone", [line({ zone: B, zone_name: "Games zone" }).endsWith(" · zone Games zone"), line({ zone_name: "Home" }).endsWith(" · zone Home"),
      line({ zone: B, zone_name: null }).endsWith(" · a zone removed since"), / zone /.test(line({}))], [true, true, true, false]);
  }
  check("agoShown", [agoShown(now - 5000, now), agoShown(now - 3 * 86400000, now)], ["just now", "3 d ago"]);
  check("agoShown in seconds", agoShown(Math.floor(now / 1000) - 7200, now), "2 h ago");
  // activity-public-send-says-proving: a public send in flight said "Proving…".
  const flying = (fields) => activityLine(Object.assign({ chain: "lez", state: "sending", to: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", amount: "1000", at: Math.floor(now / 1000) }, fields), now).state;
  check("activityLine public sends are sending", [flying({ kind: "pay_account", private: false }), flying({ kind: "token_send", from: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk" }), flying({ kind: "pay_account", private: true }), flying({ kind: "shield" }), flying({ kind: "receive" }), flying({ kind: "pay", from: "Private/yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z" }), activityLine({ kind: "send", chain: "logos", state: "sending", at: now }, now).state], ["Sending…", "Sending…", "Proving…", "Proving…", "Proving…", "Proving…", "Sending…"]);
  // activity-bridge-row-wording: 2 h, 3 d and 1090 d old all said "arrives in about 12 h".
  check("activityLine bridge overdue", [activityLine(bridge, now).state, activityLine(Object.assign({}, bridge, { at: Math.floor(now / 1000) - 259200 }), now).state, activityLine(Object.assign({}, bridge, { at: 1700000000 }), now).state], ["In a block · arrives in about 12 h", "In a block · should have arrived by now; check the balance", "In a block · should have arrived by now; check the balance"]);
  // single-bridge-false-arrival-on-foreign-credit-at-least-amount: bridges matched together
  // by one credit are hedged, one matched by its own credit is not.
  check("activityLine bridge arrived hedged", [activityLine(Object.assign({}, bridge, { state: "arrived" }), now).state, activityLine(Object.assign({}, bridge, { state: "arrived", maybe: true }), now).state], ["Arrived on LEZ 0.3", "A credit of the same total as your bridges reached LEZ"]);
  // late-or-expired-bridge-arrival-logged-twice / expired-bridge-takes-later-bridges-credit:
  // a bridge not credited within 2 days is overdue (still followed by its receipt), and one
  // credited after that arrives late, as one row.
  const overdue = activityLine(Object.assign({}, bridge, { state: "expired", followed: true }), now);
  check("activityLine bridge expired", [overdue.state, overdue.tone, activityLine(Object.assign({}, bridge, { state: "expired", followed: false }), now).state],
    ["Not credited on LEZ 0.3 after 2 days · still followed; check the balance", "warn", "Not seen on LEZ 0.3 after 2 days · check the balance"]);
  check("activityLine bridge arrived late", activityLine(Object.assign({}, bridge, { state: "arrived", late: true }), now).state, "Arrived on LEZ 0.3, late");
  // claim-entry-not-on-credited-accounts-page-and-shown-as-credit-on-payer: the module's
  // reply for a live 2-account claim (payer E3, two accounts credited 2,000,000 each).
  const claim = { all: true, amount: "4000000", at: 1794204085, chain: "lez", fee: { about: "5070616", at_most: "8239751", paid: "6972097" }, from: "Private/E3j7HPx3ZhoqRnB9BtMsGjWixbms3n9kamGKXKkjiuy5", id: "1794204085-receive", kind: "receive", received: { "Private/6MJ8W5vH7vTLPpCcnuA87YU3foxrx676CkzvtGHfBJXC": "2000000", "Private/9drrc1ARLeTXhHrgV8r54iNiWDD6Rj3yUMxQJTditnaK": "2000000" }, seq: 10, state: "done" };
  const onPage = (entry, who) => activityFor([entry], who).map((x) => { const l = activityLine(x, 1794205600000, who); return [l.title, l.sign + l.amount, l.sub]; });
  check("claim on a credited page", onPage(claim, "Private/9drrc1ARLeTXhHrgV8r54iNiWDD6Rj3yUMxQJTditnaK"), [["Claimed waiting payments", "+2,000,000", "on LEZ 0.3, into your private accounts"]]);
  check("claim on the other credited page", onPage(claim, "Private/6MJ8W5vH7vTLPpCcnuA87YU3foxrx676CkzvtGHfBJXC").length, 1);
  check("claim on the payer's page", onPage(claim, claim.from), [["Paid the fee to claim waiting payments", "-6,972,097", "on LEZ 0.3, into your other private accounts"]]);
  check("claim in the wallet's history", [activityLine(claim, 1794205600000).sign, activityLine(claim, 1794205600000).amount], ["+", "4,000,000"]);
  // The live 1-account claim (seq 9): its `to` is the credited account.
  const one = Object.assign({}, claim, { amount: "2000000", fee: { about: "5070616", at_most: "8239751", paid: "5070616" }, received: { "Private/6MJ8W5vH7vTLPpCcnuA87YU3foxrx676CkzvtGHfBJXC": "2000000" }, to: "Private/6MJ8W5vH7vTLPpCcnuA87YU3foxrx676CkzvtGHfBJXC" });
  check("one claim, payer and credited pages", [onPage(one, one.from)[0][1], onPage(one, one.to)[0][1]], ["-5,070,616", "+2,000,000"]);
  // A payer credited too: its own credit, with the fee it paid beside it.
  const both = Object.assign({}, claim, { received: { [claim.from]: "3000000" } });
  check("claim payer also credited", onPage(both, claim.from), [["Claimed waiting payments", "+3,000,000", "on LEZ 0.3, into your private accounts · fee 6,972,097"]]);
  // partial-claim-job-and-pages-say-nothing-true: 1 of 2 claims landed (R1), R2's credit waits,
  // the payer paid one claim's fee. Partly done on all three pages, each in its own words.
  const R1 = "Private/6MJ8W5vH7vTLPpCcnuA87YU3foxrx676CkzvtGHfBJXC", R2 = "Private/9drrc1ARLeTXhHrgV8r54iNiWDD6Rj3yUMxQJTditnaK";
  const partial = Object.assign({}, claim, { amount: "1000000", to: R1, received: { [R1]: "1000000" }, left: { [R2]: "1000000" }, fee: { about: "10141232", at_most: "16479502", paid: "5070616" }, state: "partial" });
  check("partial claim on the credited page", onPage(partial, R1), [["Claimed waiting payments", "+1,000,000", "on LEZ 0.3, into your private accounts · some still wait"]]);
  check("partial claim on the waiting page", onPage(partial, R2), [["Still waiting to be claimed", "1,000,000", "on LEZ 0.3, not claimed yet: claim it with Receive"]]);
  check("partial claim on the payer's page", onPage(partial, claim.from), [["Paid the fee to claim waiting payments", "-5,070,616", "on LEZ 0.3, into your other private accounts"]]);
  check("partial claim state", [activityLine(partial, 1794205600000).state, activityLine(partial, 1794205600000).tone], ["Partly done · see what's left", "warn"]);
  // incoming-payment-shown-as-outgoing-on-receiving-accounts-page: a payment into A showed
  // "Paid Public A ... -1,000" on A's own page.
  const A = "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", P = "Private/yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z";
  const paid = { kind: "pay_account", chain: "lez", from: P, to: A, amount: "1000", private: true, fee: { about: null, at_most: null, paid: "70000" }, state: "done", at: 1794200000 };
  check("payment on the payee's page", onPage(paid, A), [["Received from Private yGrPEbP3…fe4Z", "+1,000", "on LEZ 0.3"]]);
  check("payment on the payer's page", onPage(paid, P), [["Paid Public CCVd76Tk…pKnk", "-1,000", "on LEZ 0.3, from your private account · fee 70,000"]]);
  check("token on the payee's page", onPage({ kind: "token_send", chain: "lez", from: P, to: A, amount: "50", state: "done", at: 1794200000 }, A)[0].slice(0, 2), ["Received a token from Private yGrPEbP3…fe4Z", "+50"]);
  // A bridge in a block hasn't reached LEZ yet: no "+" until it arrives.
  check("bridge on the LEZ account's page", onPage(Object.assign({}, bridge, { to: A, fee: "100000" }), A)[0].slice(1), ["1,000,000", "from key 7a6223ab…2a2a into Public CCVd76Tk…pKnk"]);
  check("bridge arrived on the LEZ account's page", onPage(Object.assign({}, bridge, { to: A, state: "arrived" }), A)[0][1], "+1,000,000");
  // payee-page-failed-or-pending-payment-reads-received-plus: a payment that didn't land
  // read "Received from … +1,000 · Didn't go through" on the payee's page.
  const row = (fields) => { const l = activityLine(Object.assign({}, paid, fields), 1794205600000, A); return [l.title, l.sign + l.amount, l.state]; };
  check("payee page: sending, failed, unknown", [row({ state: "sending" }), row({ state: "failed" }), row({ state: "unknown" })], [
    ["Payment from Private yGrPEbP3…fe4Z", "1,000", "Proving…"],
    ["Payment from Private yGrPEbP3…fe4Z", "1,000", "Didn't go through"],
    ["Payment from Private yGrPEbP3…fe4Z", "1,000", "Outcome unknown · check the balance"]]);
  check("payee page: a token that failed", onPage({ kind: "token_send", chain: "lez", from: P, to: A, amount: "50", state: "failed", at: 1794200000 }, A)[0].slice(0, 2), ["Token payment from Private yGrPEbP3…fe4Z", "50"]);
  check("payee page: a failed bridge", onPage(Object.assign({}, bridge, { to: A, state: "failed" }), A)[0][1], "1,000,000");
  const failedClaim = Object.assign({}, claim, { state: "failed" });
  check("credited page: a failed claim", onPage(failedClaim, "Private/9drrc1ARLeTXhHrgV8r54iNiWDD6Rj3yUMxQJTditnaK"), [["Claim of waiting payments", "2,000,000", "on LEZ 0.3, into your private accounts"]]);
  check("wallet history: a failed claim and a failed send", [activityLine(failedClaim, 1794205600000).sign, activityLine(Object.assign({}, paid, { state: "failed" }), 1794205600000).sign], ["", ""]);
  check("payer page: a payment in flight still leaves", onPage(Object.assign({}, paid, { state: "sending" }), P)[0][1], "-1,000");
  check("Logos Blockchain send into a key: in a block is received", onPage({ kind: "send", chain: "logos", from: KEY, to: "39".repeat(32), amount: "700", state: "in_block", at: 1794200000 }, "39".repeat(32))[0].slice(0, 2), ["Received on the Logos Blockchain", "+700"]);
  // account-page-address-line-useless: the prefix ate the shown characters.
  check("keyShown account", [keyShown(A), keyShown(P), keyShown("Public/11111111111111111111111111111111")], ["Public/CCVd76Tk…pKnk", "Private/yGrPEbP3…fe4Z", "Public/11111111…1111"]);
  check("bridge on the key's page", onPage(Object.assign({}, bridge, { to: A }), bridge.from)[0][1], "-1,000,000");
}

// LP-0022 gas: the node's own estimate in a chain plan's confirm step (chain_fee.rs).
// Imported here, not on the import line above, which other branches also extend (imports are hoisted).
import { chainFeeLine, claimFeeLine } from "./qml/logic.mjs";
check("chainFeeLine estimate", chainFeeLine({ amount: "3000", fee_about: "834" }), "About 834, paid from the key's notes: your node's figure at its gas prices now. The fee paid is shown once it's sent.");
check("chainFeeLine no estimate", chainFeeLine({ amount: "3000", fee_about: null }), "Your node couldn't say in advance. The fee paid is shown once it's sent.");
check("chainFeeLine no plan", chainFeeLine(null), "");
check("chainFeeLine short", chainFeeLine({ amount: "1000", fee_about: null, cannot_pay_fee: true }), "That key's notes can't pay this amount and its fee: send a little less.");
// adv.py H4: the key's only note is a send's input still on its way; not "send less".
check("chainFeeLine in flight", chainFeeLine({ amount: "1000", fee_about: null, in_flight: true }), "Another transfer from this key is still waiting to land, and its notes can't be spent twice. Wait a minute for it to land, then try again.");
check("claimFeeLine warns", claimFeeLine({ reward_per_voucher: "38", fee_about: "1599", warning: "It costs more than it brings." }), "Each claim costs about 1,599 in fees and cashes one reward of 38. It costs more than it brings.");
check("claimFeeLine none", [claimFeeLine({ fee_about: null }), claimFeeLine(null)], ["", ""]);

// LP-0022 inscription: an app's request, approved in the Names app with a chain account.
import { inscribeTitle, inscribeChoice, inscribeFirstChoice, inscribeFee, inscribeCanApprove } from "./qml/logic.mjs";
{
  const K1 = "e3635f207984ae779cf76b5f20714b514373f61ff96260879fe0a6d71f2dce07", K2 = "39e16b432574571a6bcd8ee36e370589641bd9f35367f6f97a972453c46c3225";
  const request = { id: "r1", app: "testimonial_ui", text: "good wallet", choices: [{ key: K2, balance: "0" }, { key: K1, balance: "100000", label: "Savings" }] };
  check("inscribeTitle", [inscribeTitle(request), inscribeTitle(null)], ["testimonial_ui asks you to inscribe a testimonial on the Logos Blockchain", ""]);
  check("inscribeChoice", request.choices.map(inscribeChoice).concat([inscribeChoice({ key: K2, balance: "5", title: "names-1", label: "" })]), [
    { title: "Node key", sub: "39e16b43…3225 · holds 0" }, { title: "Savings", sub: "e3635f20…ce07 · holds 100,000" }, { title: "Account", sub: "39e16b43…3225 · holds 5" }]);
  // An empty account can't pay the fee: it isn't the one ticked.
  check("inscribeFirstChoice", [inscribeFirstChoice(request.choices), inscribeFirstChoice([{ key: K2, balance: "0" }]), inscribeFirstChoice([]), inscribeFirstChoice(undefined)], [K1, "", "", ""]);
  check("inscribeFee", [inscribeFee("", null, ""), inscribeFee(K1, null, ""), inscribeFee(K1, { fee_estimate: "1086" }, ""), inscribeFee(K1, { fee_estimate: null }, ""),
    inscribeFee(K1, null, "That key doesn't hold enough for this and its fee.")], [
    { text: "Choose the account that pays it", tone: "plain" }, { text: "Asking your node…", tone: "plain" },
    { text: "About 1,086, paid from this account: your node's figure at its gas prices now. The fee paid is shown once it's sent.", tone: "plain" },
    { text: "Your node couldn't say in advance. The fee paid is shown once it's sent.", tone: "plain" },
    { text: "That key doesn't hold enough for this and its fee.", tone: "bad" }]);
  const can = (over) => inscribeCanApprove(Object.assign({ request, chosen: K1, plan: { fee_estimate: "1086" }, problem: "", busy: false, risky: false, understood: false }, over));
  check("inscribeCanApprove", [can({}), can({ chosen: "" }), can({ plan: null }), can({ problem: "short" }), can({ busy: true }), can({ risky: true }), can({ risky: true, understood: true }),
    can({ plan: { fee_estimate: null } }), can({ request: Object.assign({}, request, { node: "Your Logos Blockchain node isn't running." }) }), can({ request: null })],
    [true, false, false, false, false, false, true, true, false, false]);
  check("history: an inscription", [activityLine({ kind: "inscribe", chain: "logos", from: K1, hash: "ab".repeat(32), fee: "1086", state: "in_block", at: 1794200000 }, 1794205600000)].map(l => [l.title, l.sub, l.amount, l.sign, l.state]),
    [["Inscribed a testimonial", "on the Logos Blockchain, from key e3635f20…ce07 · fee 1,086", "", "", "In a block"]]);
}

// The bridge test network (LEZ 0.4): accounts asked about, amounts with decimals, status lines.
{
  const OWNER = "CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk";
  const OTHER = "BVDcg8SbSdCTKzx4mJbSpVgEuaoPEMhxmmRwvVCGyBnD";
  check("net names", [NET_03.version + " " + NET_03.name, NET_04.version + " " + NET_04.name], ["LEZ 0.3 public Logos testnet", "LEZ 0.4 bridge test network"]);
  check("publicAccounts from both reads, once, no private",
    publicAccounts({ public: { account: OWNER, balance: "0" }, accounts: [{ account: "Public/" + OTHER, kind: "public" }, { account: "Private/" + OTHER, kind: "private" }] },
                   [{ account: "Public/" + OWNER, kind: "public" }]),
    ["Public/" + OWNER, "Public/" + OTHER]);
  check("publicAccounts nothing read", [publicAccounts(null, null), publicAccounts(undefined, [])], [[], []]);
  check("publicAccounts refuses junk", publicAccounts({ public: { account: "not an id" } }, [{ account: "Public/0OIl", kind: "public" }]), []);
  check("decimalShown", [decimalShown("100000000000000000", 18), decimalShown("123456789", 6), decimalShown("0", 6), decimalShown("1234567000000", 6), decimalShown("23786858", 0), decimalShown("1", 18), decimalShown("000100", 2)],
    ["0.1", "123.456789", "0", "1,234,567", "23,786,858", "0.000000000000000001", "1"]);
  check("decimalShown past 2^53", decimalShown("340282366920938463463374607431768211455", 18), "340,282,366,920,938,463,463.374607431768211455");
  const clock = (s) => "t" + s;
  const read = { channel: "b4a9", height: 1572, read_at: 100, bridge: true, token_problem: null, reset_at: null,
    tokens: [{ definition: "9ANd", symbol: "dUSDC", decimals: 6, name: "Wrapped dUSDC (demo)" }, { definition: "DbPG", symbol: "wETH", decimals: 18, name: "Wrapped Sepolia ETH (demo)" }],
    accounts: [{ account: "Public/" + OWNER, found: true, native: "23786858", other_tokens: 0,
                 tokens: [{ definition: "9ANd", symbol: "dUSDC", decimals: 6, name: "Wrapped dUSDC (demo)", amount: "0" }, { definition: "DbPG", symbol: "wETH", decimals: 18, name: "Wrapped Sepolia ETH (demo)", amount: "100000000000000000" }] },
               { account: "Public/" + OTHER, found: false, native: "0", other_tokens: 0, tokens: [] }] };
  check("bridgeNetStatus reading", bridgeNetStatus(null, clock), { line: "Reading the bridge test network…", tone: "plain", reset: "" });
  check("bridgeNetStatus never read, down", bridgeNetStatus({ read: null, error: "can't reach the bridge test network: it answered HTTP 502 instead of a reading" }, clock),
    { line: "Can't read the bridge test network: it answered HTTP 502 instead of a reading", tone: "bad", reset: "" });
  check("bridgeNetStatus fresh", bridgeNetStatus({ read, error: null }, clock), { line: "Updated t100 · block 1,572", tone: "plain", reset: "" });
  check("bridgeNetStatus stale", bridgeNetStatus({ read, error: "can't reach" }, clock).line, "Can't reach the bridge test network now. Showing what it said at t100.");
  check("bridgeNetStatus reset", bridgeNetStatus({ read: Object.assign({}, read, { reset_at: 90 }) }, clock).reset, "The bridge test network was reset at t90. What was held there before is gone; balances start again.");
  check("bridgeNetRows", bridgeNetRows(read).map((r) => [r.title, r.found, r.assets.map((a) => a.symbol + " " + a.amount)]),
    [["Public account CCVd76Tk…pKnk", true, ["LEZ 23,786,858", "dUSDC 0", "wETH 0.1"]], ["Public account BVDcg8Sb…yBnD", false, []]]);
  // A busy module is never an absent one.
  check("replyKind", [undefined, null, "", "Timed out", "{\"error\":\"call timed out\"}", "false",
      JSON.stringify({ success: true, value: 3 }), JSON.stringify(JSON.stringify({ success: false, error: "no" })), "No such method: mine", "module not found: names"].map((r) => replyKind(r).kind),
    ["busy", "busy", "busy", "busy", "busy", "busy", "ok", "error", "missing", "missing"]);
  // A wallet seen is never lost to a passing read: only a plain "no wallet" (wallet: false) changes it.
  {
    const seen = { wallet: true, public: "A", private: "P", cli: "/x" };
    check("mergeCliStatus keeps the public account a passing read missed", mergeCliStatus(seen, { wallet: true, public: null, private: null, cli: "/x" }).public, "A");
    check("mergeCliStatus: no answer keeps the last", mergeCliStatus(seen, undefined), seen);
    check("mergeCliStatus: the module saying no wallet is taken", mergeCliStatus(seen, { wallet: false, public: null }).wallet, false);
  }
  check("replyKind module busy", replyKind(JSON.stringify({ success: false, busy: true, value: null, error: "The wallet is busy; still working." })).kind, "busy");
  // The host bridge's own answers (LogosQmlBridge): no client or not reachable is missing; a timeout is busy.
  check("replyKind host answers", [JSON.stringify({ error: "Module not connected" }), JSON.stringify({ error: "Module not reachable", detail: "x" }),
      "Module not reachable yet", "Module not connected", JSON.stringify({ error: "timeout" })].map((r) => replyKind(r).kind),
    ["missing", "missing", "missing", "missing", "busy"]);
  // The reveal driver: a poll that fails (busy) mid-registration doesn't stop the reveal.
  {
    const due = [{ name: "m2-test", owner: "A", ready_at: 1000, stage: "committed" }];
    let askedAt = 0, sent = [], answers = ["busy", "busy-never", "ok"];
    const tick = (now) => {
      const item = revealNext(due, now, askedAt, 40000);
      if (item === null) return;
      askedAt = now;
      const answer = answers.shift();
      if (answer === "busy") askedAt = 0;          // a failed ask settles at once
      else if (answer === "ok") { sent.push(now); askedAt = 0 }
      // "busy-never": the host never answers; the ask is out until its patience runs out.
    };
    for (let now = 0; now <= 200000; now += 10000) tick(now);
    check("revealNext: before ready_at nothing; a failed ask and an unanswered one don't stop it", sent, [60000]);
  }
  check("revealLine", revealLine({ name: "a", display: "a", stage: "committed", ready_at: 900000, reveal_by: 86000000 }, 0, (ms) => String(Math.round(ms / 60000)) + "m"),
    "a: the reveal goes out at 15m (in 15 min); it can still be sent for 23 h.");
  check("revealLine near the end", revealLine({ name: "a", stage: "committed", ready_at: 0, reveal_by: 30 * 60000 }, 0, (ms) => String(Math.round(ms / 60000)) + "m"),
    "a: the reveal is due now and goes out while Names is open; it must be sent by 30m (30 min left).");
  check("cliOutdatedLine 0.3", cliOutdatedLine("0.3", "names 0.1.0"), "The names tool this app uses for LEZ 0.3 (names 0.1.0) is too old for that network as it is now, so creating a wallet there doesn't work with it. LEZ 0.4 is unaffected: switch to it above.");
  check("cliOutdatedLine no reset, no install", ["0.3", "0.4"].map((n) => /reset|Install/.test(cliOutdatedLine(n, ""))), [false, false]);
  check("bridgeNetRows two bridges", bridgeNetRows({ bridges: [{ program: "A" }, { program: "B" }], accounts: [{ account: "Public/" + OTHER, found: true, native: "0",
    tokens: [{ bridge: "JECKBkvoc2e3dC18Wm6zZJEC2BGMwUwLsdZiQzYTnUx7", bridge_label: "bridge v2", symbol: "wETH", name: "Wrapped ETH", amount: "1", decimals: 0 }, { bridge: "7".repeat(44), symbol: "wETH", amount: "2", decimals: 0 }] }] })[0].assets.map((a) => a.name),
    ["Native", "Wrapped ETH · bridge v2", "Bridged token · bridge 77777777…7777"]);
  check("bridgeNetRows one bridge unchanged", bridgeNetRows({ bridges: [{ program: "A" }], accounts: [{ account: "Public/" + OTHER, found: true, native: "0",
    tokens: [{ bridge: "A", symbol: "wETH", name: "Wrapped ETH", amount: "1", decimals: 0 }] }] })[0].assets.map((a) => a.name), ["Native", "Wrapped ETH"]);
  check("bridgeNetRows watched", bridgeNetRows({ accounts: [{ account: "Public/" + OTHER, found: false, watched: true }] }).map((r) => [r.title, r.watched]), [["Watched account BVDcg8Sb…yBnD", true]]);
  check("publicIdProblem", [publicIdProblem(OWNER), publicIdProblem(" Public/" + OWNER + " "), publicIdProblem(""), publicIdProblem("Private/" + OWNER), publicIdProblem("0OIl" + OWNER)],
    ["", "", "Paste a public account ID.", "A private account can't be read on the bridge test network yet.", "That isn't a public account ID: 32 to 44 letters and digits, without 0, O, I or l."]);
  {
    const rows = [{ account: "a", found: true }, { account: "b", found: false }, { account: "c", found: false, watched: true }, { account: "d", found: false }];
    check("bridgeNetShown folds the empty ones", ((v) => [v.shown.map((r) => r.account), v.empty])(bridgeNetShown(rows, false)), [["a", "c"], 2]);
    check("bridgeNetShown all", bridgeNetShown(rows, true).shown.length, 4);
    check("bridgeNetShown none", bridgeNetShown(null, false), { shown: [], empty: 0 });
  }
  check("bridgeNetRows none", [bridgeNetRows(null), bridgeNetRows({})], [[], []]);
  check("activityLine arrived on the bridge test network",
    ((l) => [l.title, l.sub, l.amount, l.sign, l.quiet, l.tone])(activityLine({ kind: "arrived", chain: "lez-0.4", network: "bridge-0.4", to: "Public/" + OWNER, amount: "100000000000000000", token: "DbPG", symbol: "wETH", decimals: 18, state: "arrived", at: 1 }, 2000)),
    ["Arrived", "in Public CCVd76Tk…pKnk on LEZ 0.4, the bridge test network", "0.1 wETH", "+", true, "plain"]);
  check("activityLine on LEZ 0.3 keeps its colours", ((l) => [l.quiet, l.tone])(activityLine({ kind: "arrived", chain: "lez", to: "Public/" + OWNER, amount: "5", state: "arrived", at: 1 }, 2000)).map((v) => v === undefined ? false : v), [false, "good"]);
  check("activityLine native on the bridge test network",
    ((l) => [l.sub, l.amount])(activityLine({ kind: "arrived", chain: "lez-0.4", to: "Public/" + OWNER, amount: "5", state: "arrived", at: 1 }, 2000)),
    ["in Public CCVd76Tk…pKnk on LEZ 0.4, the bridge test network", "5"]);
  // The names network on LEZ 0.4: its own entries say LEZ 0.4, LEZ 0.3's accounts are added once.
  check("activityLine paid a name while on LEZ 0.4",
    ((l) => [l.title, l.sub])(activityLine({ kind: "pay", chain: "lez", zone: "bridge-0.4", name: "lns-smoke-ourzerofour", amount: "1000", state: "done", at: 1 }, 2000)),
    ["Paid lns-smoke-ourzerofour", "on LEZ 0.4"]);
  check("activityLine paid a name on LEZ 0.3",
    ((l) => l.sub)(activityLine({ kind: "pay", chain: "lez", name: "ada", amount: "1000", state: "done", at: 1 }, 2000)), "on LEZ 0.3");
  check("mergedAccounts", mergedAccounts(["Public/a", "Public/b"], ["Public/b", "Public/c", 7]), ["Public/a", "Public/b", "Public/c"]);
  check("mergedAccounts empty", mergedAccounts(null, undefined), []);
}

{
  // Relayed calls: untrusted text is cleaned (the registry browser's cases), the sheet is read.
  const { clean, relayedSheet } = await import("./qml/logic.mjs");
  check("clean drops bidi, zero-width, controls; a break shows as ⏎", clean("1.0\u202E(unproven)\u200B\u2066x\u2069\n"), "1.0(unproven)x⏎");
  check("invisible characters dropped", clean("a\u00ADb\u180Ec\u2061d\u206Be\uFFFAf\uDB40\uDC41g"), "abcdefg");
  check("clean of nothing", [clean(null), clean(undefined)], ["", ""]);
  const sheet = relayedSheet({ lines: ["lez_bridge asks\u202E", "Network: LEZ 0.4 · bridge test network"], warnings: [], privacy: "A public claim publishes the claim secret and links the Sepolia deposit to this account.", after: "x", app_says: "Claim\nnow\u200B", refused: false, hash: "ab" });
  check("relayedSheet cleans every line", [sheet.lines[0], sheet.says, sheet.refused], ["lez_bridge asks", "Claim⏎now", false]);
  check("relayedSheet of nothing", relayedSheet(null).lines, []);
  check("activityLine for a re-sign",
    ((l) => [l.title, l.sub])(activityLine({ kind: "resign", chain: "lez", zone: "bridge-0.4", app: "lez_bridge", payer_nonce: "42", resigns: 1, state: "done", at: 1 }, 2000)),
    ["Signed again for lez_bridge", "on LEZ 0.4, without asking: only the payer's nonce had moved (payer nonce 42, re-sign 1 of 3)"]);
}

{
  // The LEZ Bridge, installed from inside Names.
  const { BUNDLED_BRIDGE, installedPackages, installedEntry, bundledPath, bridgeSetup, bridgeDiffers, installReply, intentAnswer, publisherWaitLine } = await import("./qml/logic.mjs");
  const me = { name: "names_ui", version: "0.17.0", installDir: "/Users/you/Library/Application Support/Logos/LogosBasecamp/plugins/names_ui" };
  const oldMod = { name: "lez_bridge", version: "0.1.0", hashes: { root: "a2c6c3117443a92f2a5b2a1a2267175f4f82ed209a3dae8e554461bd09df0d18" } };
  const oldUi = { name: "lez_bridge_ui", version: "0.1.0", hashes: { root: "788f2f0dde4bd7ae" } };
  const newMod = { name: "lez_bridge", version: "0.1.0", hashes: { root: BUNDLED_BRIDGE[0].root } };
  const newUi = { name: "lez_bridge_ui", version: "0.1.0", hashes: { root: BUNDLED_BRIDGE[1].root } };
  const list = [{ name: "package_manager" }, me];
  check("installedPackages: the list, as text, twice, in {success, value}",
    [installedPackages(list), installedPackages(JSON.stringify(list)), installedPackages(JSON.stringify(JSON.stringify(list))), installedPackages({ success: true, value: list })].map((l) => l && l.length),
    [2, 2, 2, 2]);
  check("installedPackages: anything not surely the list is null (never an install from it)",
    [installedPackages([]), installedPackages([{ name: "chat_ui" }]), installedPackages(""), installedPackages("x"), installedPackages(null), installedPackages({ error: "timeout" })],
    [null, null, null, null, null, null]);
  check("bundledPath beside the app's view", bundledPath(list, "lez_bridge.lgx"), me.installDir + "/qml/bundled/lez_bridge.lgx");
  check("bundledPath unknown folder", [bundledPath([{ name: "names_ui" }], "x.lgx"), bundledPath([{ name: "names_ui", installDir: "rel/dir" }], "x.lgx")], ["", ""]);
  check("bridgeSetup: list unread → unknown (manual line), unless the module answers",
    [bridgeSetup({ list: null }), bridgeSetup({ list: null, answers: false }), bridgeSetup({ list: null, answers: true })], ["unknown", "unknown", "ready"]);
  check("bridgeSetup: a package missing → absent", [bridgeSetup({ list }), bridgeSetup({ list: list.concat([newMod]) }), bridgeSetup({ list: list.concat([newUi]), answers: true })], ["absent", "absent", "absent"]);
  check("bridgeSetup: installed, not answering → idle; not asked yet → unknown",
    [bridgeSetup({ list: list.concat([newMod, newUi]), answers: false }), bridgeSetup({ list: list.concat([newMod, newUi]) })], ["idle", "unknown"]);
  check("bridgeSetup: running", bridgeSetup({ list: list.concat([newMod, newUi]), answers: true, checkWith: true }), "ready");
  check("bridgeSetup: older only when it lacks the new call AND is another build",
    [bridgeSetup({ list: list.concat([oldMod, oldUi]), answers: true, checkWith: false }),
     bridgeSetup({ list: list.concat([oldMod, oldUi]), answers: true, checkWith: true }),
     bridgeSetup({ list: list.concat([newMod, newUi]), answers: true, checkWith: false })],
    ["older", "ready", "ready"]);
  check("bridgeDiffers", [bridgeDiffers(list.concat([oldMod, newUi])), bridgeDiffers(list.concat([newMod, newUi])), bridgeDiffers(list)], [true, false, false]);
  check("installReply: success has a path and no error",
    installReply(JSON.stringify({ name: "lez_bridge", path: "/x/modules/lez_bridge", isCoreModule: true, signatureStatus: "unsigned" })),
    { ok: true, timeout: false, path: "/x/modules/lez_bridge", error: "" });
  check("installReply: an error", installReply({ name: "lez_bridge", path: "", error: "Only LGX packages are supported" }).ok, false);
  check("installReply: no reply is a timeout, not a failure", [installReply(undefined).timeout, installReply("").timeout, installReply({ error: "Timed out waiting" }).timeout], [true, true, true]);
  check("installReply: in {success, value}", installReply({ success: true, value: { path: "/p" } }).ok, true);
  check("intentAnswer", [intentAnswer({ ok: true }), intentAnswer({ ok: false, error: "cancelled" }), intentAnswer({ ok: false, error: "not_declared" }), intentAnswer(null)], ["ok", "cancelled", "not_declared", "other"]);
  check("claimStage: looking until the bridge module is heard from", [claimStage({ looking: true }), claimStage({ looking: true, missing: true }), claimStage({ looking: true, check: { ok: true } })], ["looking", "missing", "ready"]);
  check("publisherWaitLine counts down",
    [publisherWaitLine(1000, 1000, 15000), publisherWaitLine(1000, 9500, 15000), publisherWaitLine(1000, 16500, 15000)],
    ["Checking who publishes this app… about 15 s", "Checking who publishes this app… about 7 s", "Checking who publishes this app… a few more seconds"]);
}

{
  const { idShort, claimExtras, claimSteps, claimSummary, claimErrorLine } = await import("./qml/logic.mjs");
  const A = "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk";
  check("idShort", [idShort(A), idShort("short")], ["CCVd76…pKnk", "short"]);
  check("claimExtras: none / another account / no grant",
    [claimExtras([{ app: "lez_bridge", accounts: [A] }], A), claimExtras([{ app: "lez_bridge", accounts: ["Public/x"] }], A), claimExtras([], A), claimExtras([{ app: "lez_bridge", accounts: [{ account: A }] }], A)],
    [[], ["public_account"], ["connect", "programs"], []]);
  const view = (steps) => steps.map((x) => x.n + "/" + x.total + " " + x.state + " " + x.title + (x.line ? " — " + x.line : ""));
  check("steps: choose the file (4 steps)", view(claimSteps({ stage: "start", extras: [] })),
    ["1/4 now Choose the claim file", "2/4 later Check", "3/4 later Approve in your wallet", "4/4 later Claim"]);
  check("steps: checked, first claim (6 steps)", view(claimSteps({ stage: "ready", extras: ["connect", "programs"], summary: "0.1 ETH → your account CCVd76…pKnk (bridge v3)" })),
    ["1/6 done Choose the claim file — Claim file read", "2/6 now Checked ✓", "3/6 later Let the bridge see this account", "4/6 later Allow the bridge's programs", "5/6 later Approve in your wallet", "6/6 later Claim"]);
  check("steps: the connect sheet", view(claimSteps({ stage: "needs", needs: "connect", extras: ["connect", "programs"], summary: "S" }))[2], "3/6 now Let the bridge see this account");
  check("steps: an unexpected needs adds a step", claimSteps({ stage: "needs", needs: "programs", extras: [], summary: "S" }).map((x) => x.key), ["file", "check", "programs", "approve", "done"]);
  check("steps: approve, then sending, then done",
    [view(claimSteps({ stage: "running", job: { phase: "approve" }, extras: [] }))[2], view(claimSteps({ stage: "running", job: { phase: "relay" }, extras: [] }))[3], view(claimSteps({ stage: "done", job: { phase: "done", state: "done", block: 5249 }, extras: [] }))[3]],
    ["3/4 now Approve in your wallet", "4/4 now Claiming…", "4/4 done Done: claimed in block 5249"]);
  const { claimRetryAfter } = await import("./qml/logic.mjs");
  check("claimRetryAfter", [claimRetryAfter("not_final", { eta_s: 330 }), claimRetryAfter("not_final", {}), claimRetryAfter("root_not_attested"), claimRetryAfter("relay_cap")], [330, 0, 60, 0]);
  check("claimSummary", claimSummary({ ok: true, deposit: { amount_text: "0.1 ETH", account: A } }, "bridge v3"), "0.1 ETH → your account CCVd76…pKnk (bridge v3)");
  check("claimErrorLine", [claimErrorLine("relay_cap"), claimErrorLine("not_final", { eta_s: 330 }), claimErrorLine("relay_busy"), claimErrorLine("nonsense")],
    ["The bridge's relay has reached today's limit. Nothing was sent. Try again later.", "Not final on Ethereum yet: about 6 min.", "The relay is busy. Try again.", "The claim can't go on."]);
}

console.log(failures === 0 ? "ALL PASS" : failures + " FAILED");
process.exit(failures ? 1 : 0);
