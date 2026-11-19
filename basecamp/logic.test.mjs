// Checks the view's own logic (qml/logic.mjs, which qml/Main.qml imports), since the QML
// engine cannot run in CI. Run with: node basecamp/logic.test.mjs
import { atLeast, someAmount, entryShown, amountRead, amountProblem, amountDigits, AMOUNT_MAX, CHAIN_AMOUNT_MAX, jobProblem, tokenToProblem, tokenSendProves, tokenShown, feeEstimate, feePaid, accountTitle, accountsShown, publisherLine, durationWords, keyShown, chainToProblem, stakeLines, rewardsLine, grouped, addDigits, walletAssets, subDigits, walletGroups, stakeTable, activityLine, activityFor, agoShown, readNote, chainKeyTitle, chainKeySub, chainUsable, chainLaterNote } from "./qml/logic.mjs";
import { readFileSync } from "node:fs";
import { SETTLE_CONSENT, settleNotOffered } from "./qml/logic.mjs";
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
// The CLI's log and the module's problem carry the chain's own words: cleaned before they show.
check("jobProblem cleans what the CLI printed", [jobProblem({ log_tail: "Error: paid ‮eman​ x\n" }, "It didn't go through"), jobProblem({ problem: "Failed: ⁦a⁩\u0007" }, "x"), jobProblem({ log_tail: "last ‮line" }, "y")],
  ["It didn't go through: paid eman x", "Failed: a", "y: last line"]);
const id ="CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk";
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
  // Review finding 4: the module's error words are classified raw, then cleaned and cut.
  check("bridge reply error cleaned", bridgeReply(twice({ success: false, value: null, error: "no\u202E <b>way</b>" })).error, "no bway/b");
  check("bridge reply missing, cleaned and still missing", ((r) => [r.kind, r.error])(bridgeReply(JSON.stringify({ error: "Method not found\u200B<i>" + "y".repeat(500) }))), ["missing", "Method not foundi" + "y".repeat(282) + "…"]);
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
  check("other deployment, newest first", [otherDeployment("bridge-test-network-v3"), otherDeployment("bridge-test-network"), otherDeployment("bridge-test-network-v5"), otherDeployment("bridge-test-network-v4")],
    ["bridge-test-network-v5", "bridge-test-network-v5", "bridge-test-network-v4", "bridge-test-network-v5"]);
  check("other deployment: the ones tried are skipped", [otherDeployment("bridge-test-network-v3", ["bridge-test-network-v5", "bridge-test-network-v4"]), otherDeployment("bridge-test-network", ["bridge-test-network-v5", "bridge-test-network-v4", "bridge-test-network-v3"])],
    ["bridge-test-network", ""]);
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
  check("no answer: look first, as a wait", [silent.again, silent.line.includes("If one shows on this page, answer it"), silent.kind], [false, true, "waiting"]);
  // The buttons the lines name are the ones on the screen (Continue), never a "Claim" that isn't there.
  check("busy names Continue, a wait", [claimFailure("Your Names Wallet is busy; nothing was sent; try again.").line.includes("Press Continue"), claimFailure("Your Names Wallet is busy").kind], [true, "waiting"]);
  check("no claim line names a missing Claim button", [claimFailure("Your Names Wallet is busy").line, silent.line].some((l) => /press Claim/i.test(l)), false);
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
  // Review finding 3: markup never survives the cleaner (and Hint & co. draw plain text, below).
  check("clean drops < > &", clean("<b>bold</b> &amp; <img src='x'>"), "bbold/b amp; img src='x'");
  // Finding 4: the LEZ Bridge's and its relay's words: cleaned and cut.
  const { bridgeText } = await import("./qml/logic.mjs");
  check("bridgeText: bidi, markup, breaks", bridgeText("paid \u202E000,01<br><font color='green'>Claim sent</font>\nnow"), "paid 000,01brfont color='green'Claim sent/font⏎now");
  check("bridgeText: 200 kB cut to 300", [bridgeText("x".repeat(200000)).length, bridgeText("x".repeat(200000)).endsWith("…")], [300, true]);
  check("bridgeText: nothing", [bridgeText(null), bridgeText(undefined), bridgeText("")], ["", "", ""]);
  check("bridgeText: a shorter cut", bridgeText("abcdef", 4), "abc…");
  const sheet = relayedSheet({ lines: ["lez_bridge asks\u202E", "Network: LEZ 0.4 · bridge test network"], warnings: [], privacy: "A public claim publishes the claim secret and links the Sepolia deposit to this account.", after: "x", app_says: "Claim\nnow\u200B", refused: false, hash: "ab" });
  check("relayedSheet cleans every line", [sheet.lines[0], sheet.says, sheet.refused], ["lez_bridge asks", "Claim⏎now", false]);
  check("relayedSheet of nothing", relayedSheet(null).lines, []);

  // A burn the wallet decoded (sheet.burn), toward each bridge it signs burns for (v4 to v7).
  // Only the burn this app's own Bridge to Ethereum screen shows, for the send-back on it (whose
  // address and words were read back and ticked at step 2), goes without a tick; any other app's
  // burn, or the LEZ Bridge's away from that screen, needs the tick, and shows where it goes.
  const { requestRisky, burnIsOwnSendBack, burnDestination, burnTickText } = await import("./qml/logic.mjs");
  const E = "0xe2ba7765d0fbf247044cb907a4c921c03ca316c6cfcb6323a6b5e2545ff3dfd5";
  const bridges = [["bridge v4", "BKXe3q5JcKeiSrKv9FUBjZw3AowbvYgyHZ3SYPgDxJBf"], ["bridge v5", "CuV1ctZ2qJG4wQcXP4TBudVGKzNmdoYT3DpP1veHnDMA"],
                   ["bridge v6", "4PxaLAfPZDvpfZaR8SsC2Y7pYMPNwPPGVGMpjEVPQWed"], ["bridge v7", "CdijfhGnf1NjyCvYTZcGijQZzZ6soNJQ75Piex1ZTM93"]];
  const burnOf = (app, label, program) => ({ id: "r9", kind: "relayed_call", app, account: "Public/5DNyon1w3jf4Ai2ANzLz1bYnrkeFwuoNRmTL48FTGGb7",
    sheet: { warnings: ["This destroys the wrapped tokens on Logos and can't be undone; the ETH is released on Ethereum after the bridge's steps."], refused: false,
             burn: { bridge: program, bridge_label: label, amount_text: "0.01", symbol: "wETH", exit: E } } });
  // `recorded`: the address this wallet read back and ticked at step 2 for that send-back (kept by
  // Names itself when it planned it); `shown`: the address the LEZ Bridge's row draws above the sheet.
  const ADDR = "0xE2C4857976b346F8F9b3a7be714Cd92d0bF68680";
  const own = { inline: true, view: "sendback", focus: "sb-e2ba7765d0fbf247", recorded: ADDR, shown: ADDR };
  for (const [label, program] of bridges) {
    const theirs = burnOf("other_app", label, program), bridgeAway = burnOf("lez_bridge", label, program), ours = burnOf("lez_bridge", label, program);
    check(`burn to ${label}: another app's needs the tick`, requestRisky(theirs, false, burnIsOwnSendBack(theirs, own)), true);
    check(`burn to ${label}: the LEZ Bridge's, away from Bridge to Ethereum, needs the tick`, requestRisky(bridgeAway, false, burnIsOwnSendBack(bridgeAway, { inline: false, view: "home", focus: "" })), true);
    check(`burn to ${label}: the one on this app's own Bridge to Ethereum screen goes without`, requestRisky(ours, false, burnIsOwnSendBack(ours, own)), false);
    check(`burn to ${label}: its destination is shown`, burnDestination(theirs), [
      "Asked by other_app.",
      `Bridge: ${label} (${program}).`,
      "Amount: 0.01 wETH, out of your account 5DNyon…GGb7.",
      "Ethereum address: not in the call. It is sealed in the exit commitment 0xe2ba7765…dfd5, which the wallet can't open, so the wallet can't show or check it."]);
  }
  const ours = burnOf("lez_bridge", "bridge v7", bridges[3][1]);
  check("own screen, another send-back on it: the tick", requestRisky(ours, false, burnIsOwnSendBack(ours, { ...own, focus: "sb-0000000000000000" })), true);
  check("own screen, not drawn there (a sheet over it): the tick", burnIsOwnSendBack(ours, { ...own, inline: false }), false);
  check("own screen, its address not drawn above (send-back row not read): the tick", burnIsOwnSendBack(ours, { ...own, shown: "" }), false);
  // Review 2026-11-19, finding 1: a send-back the LEZ Bridge lists but this wallet never planned
  // and ticked (another app planned it, to its own address), or one whose drawn address isn't the
  // one ticked, needs the tick.
  check("own screen, focus and address match but this wallet never ticked it: the tick", burnIsOwnSendBack(ours, { ...own, recorded: "" }), false);
  check("own screen, ticked another address than the one drawn: the tick", burnIsOwnSendBack(ours, { ...own, recorded: "0x68b5000000000000000000000000000000000975d" }), false);
  check("own screen, the same address in other case: no tick", burnIsOwnSendBack(ours, { ...own, recorded: ADDR.toLowerCase() }), true);
  check("own screen, no record object at all: the tick", burnIsOwnSendBack(ours, { inline: true, view: "sendback", focus: "sb-e2ba7765d0fbf247", address: true }), false);
  // Finding 2: whether a burn is the send-back in focus at all (decides inline vs the sheet over it).
  const { burnMatchesSendBack, isDecodedBurn } = await import("./qml/logic.mjs");
  check("burnMatchesSendBack", [burnMatchesSendBack(ours, "sb-e2ba7765d0fbf247"), burnMatchesSendBack(ours, "sb-0000000000000000"), burnMatchesSendBack(ours, ""), burnMatchesSendBack(burnOf("other_app", "bridge v7", bridges[3][1]), "sb-e2ba7765d0fbf247"), burnMatchesSendBack({ kind: "connect", app: "lez_bridge" }, "sb-e2ba7765d0fbf247")],
    [true, false, false, false, false]);
  check("isDecodedBurn", [isDecodedBurn(ours), isDecodedBurn({ kind: "relayed_call", sheet: {} }), isDecodedBurn(null)], [true, false, false]);
  // Exit keys for an address not approved (the module's sheet data, exit_recipient.rs).
  const { isExitAsk, exitShown, exitTickText, recipientApproved } = await import("./qml/logic.mjs");
  const exitAsk = { id: "r5", kind: "secret", app: "lez_bridge", account: "Private/AAA", scope: "RFP022/exit",
    exit: { recipient: "0x3c2f2c4baec708aee232379eb00e9ecfa1b351f8", bridge_label: "bridge v4", vault: "0xc2c6a059eab928966b24fa5549d441a3b17b54ed", chain_id: 11155111, words_text: "myth · nerve · session · until", words_why: null } };
  check("an exit ask always needs the tick", [requestRisky(exitAsk, false, false), isExitAsk(exitAsk), isExitAsk({ kind: "secret", scope: "RFP022/exit" })], [true, true, false]);
  check("an exit ask shows the address, words and bridge", exitShown(exitAsk), { halves: ["0x3c2f2c4baec708aee232", "379eb00e9ecfa1b351f8"], words: "myth · nerve · session · until", why: "",
    bridge: "Bridge: bridge v4 (vault 0xc2c6a059eab928966b24fa5549d441a3b17b54ed, Ethereum chain 11155111)." });
  check("an exit ask for an unknown bridge, no words", ((s) => [s.words, s.bridge])(exitShown({ ...exitAsk, exit: { recipient: exitAsk.exit.recipient, cfg_id: "0x1111111111111111111111", words_text: null, words_why: "can't make the four words" } })),
    ["", "Bridge: one this wallet doesn't know (configuration 0x111111111111…)."]);
  check("exit tick", [exitTickText(exitAsk), exitTickText({ ...exitAsk, exit: { ...exitAsk.exit, words_text: null } })],
    ["This is my Ethereum address: I compared it, and its four words, with where I'm sending to. Tokens sent back to it can't be taken back.",
     "This is my Ethereum address: I compared it with where I'm sending to. Tokens sent back to it can't be taken back."]);
  check("recipientApproved", [recipientApproved({ "Private/AAA": ["0x3c2f2c4baec708aee232379eb00e9ecfa1b351f8"] }, "AAA", "0x3C2F2C4BAEC708AEE232379EB00E9ECFA1B351F8"),
    recipientApproved({ "Private/AAA": ["0x3c"] }, "Private/BBB", "0x3c"), recipientApproved({}, "AAA", "0x3c"), recipientApproved(null, "AAA", "0x3c"), recipientApproved({ "Public/AAA": ["0x3c"] }, "Public/AAA", "")],
    [true, false, false, false, false]);
  // A focus-matching burn this wallet didn't tick: the address above is the LEZ Bridge's record, said so.
  check("the tick when the address above is the bridge's record", burnTickText(ours, true),
    "The address above is the LEZ Bridge's record of this send-back; this wallet didn't read it back or tick it. Tick only if it is your address; the burn can't be undone.");
  check("destination when the address above is the bridge's record", burnDestination(ours, true)[3],
    "Ethereum address: not in the call. It is sealed in the exit commitment 0xe2ba7765…dfd5; the address above is the LEZ Bridge's record of it, not checked by this wallet.");
  check("own screen, another app with the same exit: the tick", burnIsOwnSendBack(burnOf("other_app", "bridge v7", bridges[3][1]), own), false);
  check("an exit that isn't 32 bytes of hex matches no send-back", burnIsOwnSendBack({ ...ours, sheet: { ...ours.sheet, burn: { ...ours.sheet.burn, exit: "0xe2ba7765d0fbf247" } } }, own), false);
  check("a refused burn is no burn: its warnings decide", requestRisky({ ...ours, sheet: { ...ours.sheet, refused: true } }, false, true), true);
  check("a publisher check that failed always ticks", requestRisky(ours, true, true), true);
  check("the rest as before: an undecoded relayed call with a warning / a call without / a token with one / a payment",
    [requestRisky({ kind: "relayed_call", sheet: { warnings: ["can't decode"] } }, false, false), requestRisky({ kind: "call", warnings: [] }, false, false),
     requestRisky({ kind: "transfer", asset_kind: "token", warnings: ["no name"] }, false, false), requestRisky({ kind: "transfer", warnings: ["x"] }, false, false), requestRisky(null, false, false)],
    [true, false, true, false, false]);
  check("the tick names the app and says it can't be undone", burnTickText(burnOf("other_app", "bridge v6", bridges[2][1])),
    "The wallet can't check the Ethereum address. Tick only if you started this send-back in other_app and checked the address there; the burn can't be undone.");
  check("the bridge's own words for the tick", burnTickText(bridgeAwayOf()), "The wallet can't check the Ethereum address. Tick only if you started this send-back in the LEZ Bridge and checked the address there; the burn can't be undone.");
  function bridgeAwayOf() { return burnOf("lez_bridge", "bridge v4", bridges[0][1]) }
  check("destination text from the module is cleaned", burnDestination(burnOf("evil‮app", "bridge v4​", "BKXe\nx")).slice(0, 2), ["Asked by evilapp.", "Bridge: bridge v4 (BKXe⏎x)."]);
  check("no burn, no destination", [burnDestination(null), burnDestination({ kind: "relayed_call", sheet: {} })], [[], []]);
  // The sheet uses these rules, not a copy of its own.
  const sheetQml = readFileSync(new URL("./qml/Main.qml", import.meta.url), "utf8");
  check("Main.qml: appRequestRisky is Logic.requestRisky", /function appRequestRisky\(request\) \{\s*return Logic\.requestRisky\(/.test(sheetQml), true);
  check("Main.qml: the short sheet shows Logic.burnDestination", sheetQml.includes("Logic.burnDestination(sheetBody.request"), true);
  // Review finding 3: no text the app draws is read as markup unless it says so. The text
  // components default to plain text, and every Text element in Main.qml names its format.
  const blockOf = (src, at) => { let depth = 0, i = src.indexOf("{", at); const start = i; for (; i < src.length; i++) { if (src[i] === "{") depth++; else if (src[i] === "}" && --depth === 0) break } return src.slice(start, i + 1) };
  for (const name of ["Hint", "NumberLine", "Part", "WhyLine"]) {
    const at = sheetQml.indexOf("component " + name + ": Text {");
    check(`Main.qml: ${name} draws plain text`, at >= 0 && /textFormat: Text\.PlainText/.test(blockOf(sheetQml, at)), true);
  }
  const bare = [...sheetQml.matchAll(/(^|[^\w.])Text \{/gm)].map((m) => ({ line: sheetQml.slice(0, m.index + m[0].indexOf("Text")).split("\n").length, block: blockOf(sheetQml, m.index) }))
    // The address step's words (names-roadmap-2 changes that line; its plain format is in the
    // marked patch for that branch, LP0001-JOB-B-ROADMAP2.patch).
    .filter((t) => !/textFormat:/.test(t.block)).map((t) => t.line);
  check("Main.qml: every Text element names its format", bare, []);
  // Finding 1: Names keeps the address it ticked for each send-back it planned, and the exemption reads it.
  check("Main.qml: sbPlan records the ticked address", /plans\[id\] = ticked\s*sbOwnPlans = plans/.test(sheetQml), true);
  check("Main.qml: appBurnOwn passes the record", /recorded: sbOwnPlans\[sbFocus\]/.test(sheetQml), true);
  // The record outlives a restart: an address approved on the exit sheet or ticked at step 2 is
  // kept by the names module (exit_recipients), and step 2 approves it before the plan.
  check("Main.qml: appBurnOwn reads the kept approvals", sheetQml.includes("Logic.recipientApproved(sbApproved, sbFocusRow.account, sbFocusRow.recipient)"), true);
  check("Main.qml: step 2 approves the ticked address before the plan", /function sbContinue\(\)[\s\S]{0,600}approve_exit_recipient[\s\S]{0,400}sbPlan\(\)/.test(sheetQml), true);
  check("Main.qml: the short sheet shows an exit ask's address", sheetQml.includes("Logic.exitShown(sheetBody.request)"), true);
  // Finding 2: a burn for another send-back than the one in focus is never drawn inline under its address.
  check("Main.qml: appInline on Bridge to Ethereum needs the burn to match the focus", sheetQml.includes("!Logic.isDecodedBurn(appRequest) || Logic.burnMatchesSendBack(appRequest, sbFocus)"), true);
  check("Main.qml: the address block above the sheet is the burn's own", sheetQml.includes("visible: sendBackScreen.burning && root.sbFocusRow !== null && (!Logic.isDecodedBurn(root.appRequest) || Logic.burnMatchesSendBack(root.appRequest, root.sbFocus))"), true);

  check("activityLine for a re-sign",
    ((l) => [l.title, l.sub])(activityLine({ kind: "resign", chain: "lez", zone: "bridge-0.4", app: "lez_bridge", payer_nonce: "42", resigns: 1, state: "done", at: 1 }, 2000)),
    ["Signed again for lez_bridge", "on LEZ 0.4, without asking: only the payer's nonce had moved (payer nonce 42, re-sign 1 of 3)"]);
}

{
  // The LEZ Bridge, installed from inside Names.
  const { BUNDLED_BRIDGE, installedPackages, installedEntry, bundledPath, bridgeSetup, bridgeDiffers, installReply, intentAnswer, publisherWaitLine } = await import("./qml/logic.mjs");
  const me = { name: "names_ui", version: "0.17.0", installDir: "/Users/you/Library/Application Support/Logos/LogosBasecamp/plugins/names_ui" };
  const oldMod = { name: "lez_bridge", version: "0.1.0", hashes: { root: "a2c6c3117443a92f2a5b2a1a2267175f4f82ed209a3dae8e554461bd09df0d18" } };
  const oldUi = { name: "lez_bridge_app_unused", version: "0.1.0" };
  const newMod = { name: "lez_bridge", version: "0.1.0", hashes: { root: BUNDLED_BRIDGE[0].root } };
  const newUi = { name: "lez_bridge_app_unused", version: "0.1.0" };
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
  check("bridgeSetup: the module missing → absent (the bridge app doesn't count)", [bridgeSetup({ list }), bridgeSetup({ list: list.concat([newMod]) }), bridgeSetup({ list: list.concat([newUi]), answers: true })], ["absent", "unknown", "absent"]);
  check("bridgeSetup: installed, not answering → idle; not asked yet → unknown",
    [bridgeSetup({ list: list.concat([newMod, newUi]), answers: false }), bridgeSetup({ list: list.concat([newMod, newUi]) })], ["idle", "unknown"]);
  check("bridgeSetup: running", bridgeSetup({ list: list.concat([newMod, newUi]), answers: true, checkWith: true }), "ready");
  check("bridgeSetup: older only when it can't receive AND is another build",
    [bridgeSetup({ list: list.concat([oldMod, oldUi]), answers: true, features: [] }),
     bridgeSetup({ list: list.concat([oldMod, oldUi]), answers: true, features: ["receive", "receives"] }),
     bridgeSetup({ list: list.concat([newMod, newUi]), answers: true, features: [] }),
     bridgeSetup({ list: list.concat([oldMod, oldUi]), answers: true, checkWith: false })],
    ["older", "ready", "ready", "older"]);
  check("bridgeSetup: the module alone is enough (no bridge app)", bridgeSetup({ list: list.concat([newMod]), answers: true, features: ["receives"] }), "ready");
  check("stale token: its own kind", [replyKind("ModuleProxy: rejecting unauthorized call - auth token not recognized").kind, bridgeReply("auth token not recognized").kind], ["stale", "stale"]);
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
    ["1/4 now Choose the claim file", "2/4 later Check", "3/4 later Approve the claim", "4/4 later Claim"]);
  check("steps: checked, first claim (6 steps)", view(claimSteps({ stage: "ready", extras: ["connect", "programs"], summary: "0.1 ETH → your account CCVd76…pKnk (bridge v3)" })),
    ["1/6 done Choose the claim file — Claim file read", "2/6 now Checked", "3/6 later Let the bridge see this account", "4/6 later Allow the bridge's programs", "5/6 later Approve the claim", "6/6 later Claim"]);
  check("steps: the connect sheet", view(claimSteps({ stage: "needs", needs: "connect", extras: ["connect", "programs"], summary: "S" }))[2], "3/6 now Let the bridge see this account");
  check("steps: an unexpected needs adds a step", claimSteps({ stage: "needs", needs: "programs", extras: [], summary: "S" }).map((x) => x.key), ["file", "check", "programs", "approve", "done"]);
  check("steps: approve, then sending, then done",
    [view(claimSteps({ stage: "running", job: { phase: "approve" }, extras: [] }))[2], view(claimSteps({ stage: "running", job: { phase: "relay" }, extras: [] }))[3], view(claimSteps({ stage: "done", job: { phase: "done", state: "done", block: 5249 }, extras: [] }))[3]],
    ["3/4 now Approve the claim", "4/4 now Claiming…", "4/4 done Claimed"]);
  check("steps: the row's words", claimSteps({ first: "arrived", stage: "running", job: { phase: "approve" }, extras: [] }).map((x) => x.short + " " + x.state),
    ["Arrived done", "Checked done", "Approve now", "Claim later"]);
  const { claimRetryAfter } = await import("./qml/logic.mjs");
  check("claimRetryAfter", [claimRetryAfter("not_final", { eta_s: 330 }), claimRetryAfter("not_final", {}), claimRetryAfter("root_not_attested"), claimRetryAfter("relay_cap")], [330, 0, 150, 0]);
  const { claimIsWait } = await import("./qml/logic.mjs");
  check("claimIsWait", [claimIsWait("not_final"), claimIsWait("root_not_attested"), claimIsWait("relay_cap"), claimIsWait("already_claimed"), claimIsWait("bad_file")], [true, true, true, false, false]);
  check("steps: a received deposit, its keys first", claimSteps({ first: "arrived", stage: "needs", needs: "secret", extras: [] }).map((x) => x.state + " " + x.title),
    ["done Your deposit arrived", "now Allow the bridge's deposit keys", "later Check", "later Approve the claim", "later Claim"]);
  check("claimSummary", claimSummary({ ok: true, deposit: { amount_text: "0.1 ETH", account: A } }, "bridge v3"), "0.1 ETH → your account CCVd76…pKnk (bridge v3)");
  check("claimErrorLine", [claimErrorLine("relay_cap"), claimErrorLine("not_final", { eta_s: 330 }), claimErrorLine("relay_busy"), claimErrorLine("nonsense")],
    ["The bridge's relay can't pay this claim right now (its daily limit, or fees above what it pays). Nothing was sent. Try again later.", "Not final on Ethereum yet: about 6 min.", "The relay is busy. Try again.", "The claim can't go on."]);
}

{
  const { receiveTokens, receiveReply, receivesFrom, receiveProgress, receiveSteps, RECEIVE_FALLBACK } = await import("./qml/logic.mjs");
  check("receiveTokens from registry", receiveTokens({ tokens: [{ symbol: "ETH", eth_token: "0x0000000000000000000000000000000000000000", denoms: [{ index: 0, value: "10000000000000000", text: "0.01" }] }] }),
    [{ key: "0x0000000000000000000000000000000000000000", ethToken: "0x0000000000000000000000000000000000000000", symbol: "ETH", frozen: false, delisted: false, denoms: [{ index: 0, text: "0.01", value: "10000000000000000" }] }]);
  check("receiveTokens fallback", receiveTokens(null), RECEIVE_FALLBACK);
  check("receiveReply", [receiveReply({ needs: "secret" }).kind, receiveReply({ request: "logosdep1qq" }).kind, receiveReply({ deposit: {} }).kind], ["needs", "code", "old"]);
  check("receivesFrom keeps the unclaimed receives",
    receivesFrom({ receives: [{ id: "dep-1", symbol: "ETH", denom_text: "0.1", account: "A", account_kind: "public", stage: "deposited" }, { id: "dep-2", claimed: true }, { id: "dep-3", stage: "claimed" }] }),
    [{ id: "dep-1", symbol: "ETH", amount: "0.1", account: "A", account_kind: "public", stage: "deposited", compare: null, deployment: null }]);
  check("receivesFrom keeps the row's compare words", receivesFrom({ receives: [{ id: "d", compare: { words: ["a", "b", "c", "d"] } }] })[0].compare.words.length, 4);
  check("receivesFrom: the amount from denom and decimals", receivesFrom({ receives: [{ id: "d", symbol: "ETH", denom: "100000000000000000", decimals: 18 }] })[0].amount, "0.1");
  const { bridgeCan } = await import("./qml/logic.mjs");
  check("bridgeCan", [bridgeCan(["receive", "receives"], "receives"), bridgeCan(undefined, "receives"), bridgeCan([], "receive")], [true, false, false]);
  check("deposited: up to about N min, then any minute now", [receiveProgress({ stage: "deposited", final_in_min: 7 }).headline, receiveProgress({ stage: "deposited", final_in_min: 1 }).headline, receiveProgress({ stage: "deposited" }).card],
    ["Final on Sepolia in up to about 7 min.", "Any minute now.", "Deposited · final on Sepolia in up to about 20 min"]);
  check("receiveProgress: the card's words", ["requested", "deposited", "final", "claimable", "claimed", undefined].map((stage) => receiveProgress(stage ? { stage, final_in_min: 14, claimed_block: 812 } : null).card),
    ["Waiting for your deposit", "Deposited · final on Sepolia in up to about 14 min", "Final on Sepolia · arriving any minute now", "Arrived · ready to claim", "Claimed in block 812", "Checking…"]);
  check("receiveProgress: tones and Claim", ["requested", "deposited", "final", "claimable", "claimed", "unknown"].map((stage) => receiveProgress({ stage }).tone + (receiveProgress({ stage }).claim ? "+claim" : "")),
    ["wait", "wait", "wait", "ready+claim", "done", "plain"]);
  check("receiveProgress: seen on Ethereum", receiveProgress({ stage: "deposited", confirmations: 12, final_in_min: 9 }).detail, "Deposit seen on Sepolia · 12 confirmations.");
  const steps = (s) => receiveSteps(s).map((x) => x.n + "/" + x.total + " " + x.state + " " + x.title);
  check("receiveSteps", steps({ stage: "code", extras: ["public_account", "secret"] }),
    ["1/6 done Choose the amount and account", "2/6 done Give the bridge this account", "3/6 done Allow the bridge's deposit keys", "4/6 now Deposit on the page", "5/6 later Waiting for Ethereum", "6/6 later Claim"]);
  // The deposit seen: the deposit step completes by itself, the wait is the step, then Claim, then claimed.
  check("receiveSteps: deposited", steps({ stage: "deposited", extras: ["public_account", "secret"] }).slice(3), ["4/6 done Deposited", "5/6 now Waiting for Ethereum", "6/6 later Claim"]);
  check("receiveSteps: final", steps({ stage: "final", extras: [] }).slice(1), ["2/4 done Deposited", "3/4 now Waiting for Ethereum", "4/4 later Claim"]);
  check("receiveSteps: claimable", steps({ stage: "claimable", extras: ["secret"] }).slice(2), ["3/5 done Deposited", "4/5 done Waiting for Ethereum", "5/5 now Arrived: claim it"]);
  check("receiveSteps: claimed", steps({ stage: "claimed", claimedBlock: 6054, extras: [] }), ["1/4 done Choose the amount and account", "2/4 done Deposited", "3/4 done Waiting for Ethereum", "4/4 done Claimed"]);
  check("receiveSteps: waiting is the deposit step", [steps({ stage: "waiting", extras: [] })[1], steps({ stage: "checking", extras: [] })[1]], ["2/4 now Deposit on the page", "2/4 now Deposit on the page"]);
  check("receiveSteps: the row's words", receiveSteps({ stage: "deposited", extras: ["secret"] }).map((x) => x.short), ["Amount", "Keys", "Deposited", "Wait", "Claim"]);
}

{
  const { receiveRowsShown, receiveInProgress, receiveOnItsWay, receiveCheckLine, receiveBalanceLine, RECEIVE_CLAIMED_SHOWN_MS, bridgeNetShown } = await import("./qml/logic.mjs");
  const R = (id, amount) => ({ id, symbol: "ETH", amount: amount || "0.1", account: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk" });
  const list = [R("w1"), R("w2"), R("w3"), R("d1"), R("a1"), R("c1"), R("c2")];
  const st = { w1: { stage: "requested" }, w2: { stage: "requested" }, w3: { stage: "requested" }, d1: { stage: "deposited", final_in_min: 14 }, a1: { stage: "claimable" }, c1: { stage: "claimed", claimed_block: 9 }, c2: { stage: "claimed" } };
  const now = 1000000000;
  const shown = receiveRowsShown(list, st, { c1: now - 60000, c2: now - RECEIVE_CLAIMED_SHOWN_MS - 1 }, now, false);
  check("rows: arrived, on the way, just claimed, then two unused codes", [shown.rows.map((r) => r.id + " " + r.progress.card), shown.hidden],
    [["a1 Arrived · ready to claim", "d1 Deposited · final on Sepolia in up to about 14 min", "c1 Claimed in block 9", "w1 Waiting for your deposit", "w2 Waiting for your deposit"], 1]);
  check("rows: show all", receiveRowsShown(list, st, {}, now, true).rows.map((r) => r.id), ["a1", "d1", "w1", "w2", "w3"]);
  check("rows: never checked", receiveRowsShown([R("x")], {}, {}, now, false).rows[0].progress.card, "Checking…");
  check("in progress: the one on its way", receiveInProgress(list, st, {}), "d1");
  check("in progress: an unused old code never", receiveInProgress([R("w1")], { w1: { stage: "requested" } }, {}), "");
  check("in progress: the newest code made here", receiveInProgress([R("w1"), R("w2")], { w1: { stage: "requested" }, w2: { stage: "requested" } }, { w1: { made_at: 5 }, w2: { made_at: 9 } }), "w2");
  check("receiveOnItsWay", [receiveOnItsWay(R("d1"), st.d1), receiveOnItsWay(R("f"), { stage: "final" }), receiveOnItsWay(R("a1"), st.a1), receiveOnItsWay(R("w1"), st.w1)],
    ["You have a deposit on its way: 0.1 ETH · final on Sepolia in up to about 14 min", "You have a deposit on its way: 0.1 ETH · arriving any minute now", "Your 0.1 ETH deposit has arrived.", "Your code for 0.1 ETH is waiting for a deposit."]);
  check("receiveCheckLine", [receiveCheckLine(false, now - 3000, now + 57000, now, false), receiveCheckLine(false, now - 40000, now + 20000, now, false), receiveCheckLine(true, 0, 0, now, false),
    receiveCheckLine(false, now - 3000, now + 57000, now, true), receiveCheckLine(false, 0, now + 60000, now, false), receiveCheckLine(false, now - 61000, now - 1000, now, false)],
    ["Checked just now", "Checked 40 s ago", "Checking…", "Couldn't check just now", "Next check in 1 min", "Checked 61 s ago"]);
  check("receiveCheckLine: a gentle check in minutes", [receiveCheckLine(false, now - 3000, now + 147000, now, false), receiveCheckLine(false, now - 3000, 0, now, false), receiveCheckLine(false, now - 3000, now + 147000, now, true)],
    ["Checked just now · next check in 3 min", "Checked just now", "Couldn't check just now · next try in 3 min"]);
  const blocks = [{ account: "CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", found: true, assets: [{ symbol: "LEZ", amount: "0" }, { symbol: "ETH", amount: "0.1" }] }];
  check("receiveBalanceLine: wrapped symbol", receiveBalanceLine([{ account: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", assets: [{ symbol: "wETH", amount: "0.2" }] }], "CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", "ETH"), "CCVd76…pKnk now holds 0.2 wETH.");
  check("receiveBalanceLine", [receiveBalanceLine(blocks, "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", "ETH"), receiveBalanceLine([], "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", "ETH")],
    ["CCVd76…pKnk now holds 0.1 ETH.", "The balance of CCVd76…pKnk shows on the Wallet card once the network is read again."]);
  // The receiving account stays in view even when the network has nothing for it yet.
  const rows = [{ account: "AAA", found: true }, { account: "BBB", found: false }, { account: "CCC", found: false }];
  check("bridgeNetShown keeps a receive's account", [bridgeNetShown(rows, false, ["Public/BBB"]).shown.map((r) => r.account), bridgeNetShown(rows, false, ["Public/BBB"]).empty, bridgeNetShown(rows, false).empty],
    [["AAA", "BBB"], 1, 2]);
}

{
  // One state machine for every entry point: the claim screen, Receive and the LEZ 0.4 card call
  // bridgeSetupState with the same facts, so each shows the same line and button.
  const { bridgeSetupState, BUNDLED_BRIDGE } = await import("./qml/logic.mjs");
  const me = { name: "names_ui", installDir: "/x/plugins/names_ui" };
  const oldMod = { name: "lez_bridge", hashes: { root: "a2c6c311" } };
  const newMod = { name: "lez_bridge", hashes: { root: BUNDLED_BRIDGE[0].root } };
  const FEAT = ["receive", "public_receive", "receive_status", "receives"];
  const NEW = { features: FEAT, scaffold: "lez-bridge-scaffold-try-lock=3", build: { commit: BUNDLED_BRIDGE[0].build } };
  const RECEIVES_NO_FIX = { features: FEAT, scaffold: "lez-bridge-scaffold-try-lock=3" };
  const cases = [
    ["not installed", { list: [me], bundle: "ok" }, "install", "Install"],
    ["not installed, bundle bad", { list: [me], bundle: "bad" }, "manual", ""],
    ["installing", { list: [me], bundle: "ok", install: { phase: "installing", line: "Installing the LEZ Bridge module…" } }, "busy", ""],
    ["installed while Names open", { list: [me, newMod], bundle: "ok", install: { phase: "reopen" } }, "reopen", ""],
    ["installed, not running", { list: [me, newMod], bundle: "ok", answers: false }, "reopen", ""],
    ["installed, stuck", { list: [me, newMod], bundle: "ok", answers: false, stuck: true }, "stuck", "Open Settings"],
    ["older installed, stuck (frozen)", { list: [me, oldMod], bundle: "ok", answers: false, stuck: true }, "update", "Update"],
    ["older running", { list: [me, oldMod], bundle: "ok", answers: true, features: [] }, "update", "Update"],
    ["update installed, last step", { list: [me, newMod], bundle: "ok", answers: true, features: [], install: { phase: "unload" } }, "unload", "Open Settings"],
    ["new files, old module running", { list: [me, newMod], bundle: "ok", answers: true, features: [] }, "unload", "Open Settings"],
    ["ready", { list: [me, newMod], bundle: "ok", answers: true, features: FEAT, config: NEW }, "", ""],
    ["older that receives, no freeze fix", { list: [me, oldMod], bundle: "ok", answers: true, features: FEAT, config: RECEIVES_NO_FIX }, "update", "Update"],
    ["new files, pre-fix module running", { list: [me, newMod], bundle: "ok", answers: true, features: FEAT, config: RECEIVES_NO_FIX }, "unload", "Open Settings"],
    ["another build (build.commit differs)", { list: [me, oldMod], bundle: "ok", answers: true, features: FEAT, config: { features: FEAT, build: { commit: "1234567" } } }, "update", "Update"],
    ["the shipped build running, other files listed", { list: [me, oldMod], bundle: "ok", answers: true, features: FEAT, config: NEW }, "", ""],
    ["installed, not loaded in this tab", { list: [me, newMod], bundle: "ok", answers: false, notLoaded: true }, "reopen", ""],
    ["stale token", { list: [me, newMod], bundle: "ok", answers: false, stale: true, stuck: true }, "stale", ""],
    ["just updated, after reopen", { list: [me, newMod], bundle: "ok", answers: true, features: FEAT, config: NEW, done: "LEZ Bridge updated." }, "done", ""],
    ["unread list", { list: null, bundle: "ok" }, "", ""],
  ];
  for (const entry of ["claim", "receive", "card"]) {
    for (const [label, facts, state, button] of cases) {
      const got = bridgeSetupState(facts);
      check(`setup on ${entry}: ${label}`, [got.state, got.button], [state, button]);
    }
  }
}

{
  // A `needs` with no request id has no sheet to wait for: its words, with Try again.
  const { needsWithoutRequest } = await import("./qml/logic.mjs");
  check("needs with a request", needsWithoutRequest({ needs: "public_account", request: { request: "r1", state: "pending" } }), "");
  check("needs, no request, module words", needsWithoutRequest({ needs: "public_account", request: null, message: "Names refused: no wallet" }), "Names refused: no wallet");
  check("needs, request without id", needsWithoutRequest({ needs: "connect", request: { error: "the Names wallet doesn't hold Public/x" } }), "the Names wallet doesn't hold Public/x");
  check("needs, nothing said", needsWithoutRequest({ needs: "secret" }), "The LEZ Bridge needs an answer from this wallet but didn't ask for one.");
  check("not a needs", needsWithoutRequest({ deposit: {} }), "");
}

{
  // The shipped LEZ 0.4 tool that can't try a call first: one line with the most it can cost.
  const { uncheckedLine, FULL_TOOL_DOWNLOAD, BUNDLED_TOOL } = await import("./qml/logic.mjs");
  const bare = { bare: true, unchecked_max_fee: { commit: "626778", register: "4455000", change: "1770930" } };
  check("unchecked register", uncheckedLine(bare, ["commit", "register"]), "This computer can't check the call first. If the network refuses it, the fee (at most 5.08M) is still paid.");
  check("unchecked change", uncheckedLine(bare, ["change"]), "This computer can't check the call first. If the network refuses it, the fee (at most 1.77M) is still paid.");
  check("a tool that checks", uncheckedLine({ bare: false }, ["change"]), "");
  check("no tool status", uncheckedLine(null, ["change"]), "");
  check("download off for people", FULL_TOOL_DOWNLOAD, false);
  check("tool file", BUNDLED_TOOL.file, "names-lez04");
}

{
  const { backupLastLine, backupUncoveredLine, backupPasswordHint, backupNetworks, backupCheckedLine } = await import("./qml/logic.mjs");
  const clock = () => "14 Nov, 19:50";
  check("backupLastLine", [backupLastLine(null, clock), backupLastLine({ at: 1, networks: ["lez-0.3", "lez-0.4"] }, clock), backupLastLine({ at: 1, networks: ["lez-0.4"] }, clock)],
    ["Last backup: never", "Last backup: 14 Nov, 19:50 · both wallets", "Last backup: 14 Nov, 19:50 · the LEZ 0.4 wallet"]);
  check("backupUncoveredLine", [backupUncoveredLine({ networks: [{ uncovered: 0 }, { uncovered: 1 }] }), backupUncoveredLine({ networks: [{ uncovered: 0 }] })],
    ["This wallet has 1 account your recovery phrase doesn't cover. A backup file is the only way to recover it.", ""]);
  check("backupPasswordHint", [backupPasswordHint("", "", 10).ok, backupPasswordHint("short", "", 10).text, backupPasswordHint("long enough!", "long enoug", 10).text, backupPasswordHint("long enough!", "long enough!", 10).ok],
    [false, "Too short: at least 10 characters.", "The two don't match.", true]);
  const st = { networks: [{ id: "lez-0.3", has_wallet: true }, { id: "lez-0.4", has_wallet: true }] };
  check("backupNetworks", [backupNetworks("this", "lez-0.4", st), backupNetworks("both", "lez-0.4", st), backupNetworks("both", "lez-0.4", { networks: [{ id: "lez-0.4", has_wallet: true }] })], ["lez-0.4", "lez-0.3,lez-0.4", "lez-0.4"]);
  check("backupCheckedLine", [backupCheckedLine({ opens: true, networks: [{ matches: true, accounts: [1, 2, 3] }, { matches: true, accounts: [4, 5] }] }).text, backupCheckedLine({ opens: true, networks: [{ matches: false, accounts: [] }] }).ok],
    ["Backup checked: it opens with your password and holds these 5 accounts.", false]);
}

{
  const { backupCheckedLine, backupFoldersLine, dialogPath } = await import("./qml/logic.mjs");
  check("no networks is not checked", backupCheckedLine({ opens: true, networks: [] }).ok, false);
  check("couldn't compare", backupCheckedLine({ opens: true, networks: [{ checked: false, matches: false, accounts: [] }] }).text,
    "The backup opens, but it couldn't be compared with the wallet here. Make another one to be sure.");
  check("folders line", backupFoldersLine([{ id: "lez-0.3", folder: "/Users/you/.lee/wallet" }, { id: "lez-0.4", folder: "/Users/you/.config/logos-names/lez-0.4/wallet" }]),
    "From /Users/you/.lee/wallet (LEZ 0.3) and /Users/you/.config/logos-names/lez-0.4/wallet (LEZ 0.4).");
  check("dialog path with spaces", dialogPath("file:///Users/you/Names%20Wallet%20Backups/b.lnsbackup"), "/Users/you/Names Wallet Backups/b.lnsbackup");
}

{
  const { compareLine, compareWords, compareDigitsDetail } = await import("./qml/logic.mjs");
  const words = { words: ["maple", "tiger", "seven", "lamp"], first: "b95db8", last: "6b299d" };
  check("words line", compareLine(words), "Check the page shows these four words. If they differ, stop.");
  check("words", compareWords(words), "maple · tiger · seven · lamp");
  check("digits under details", compareDigitsDetail(words), "Commitment starts b95db8 and ends 6b299d.");
  check("older bridge", [compareLine({ first: "b95db8", last: "6b299d" }), compareWords({ first: "b95db8" })], ["The page must show a commitment starting b95db8 and ending 6b299d.", ""]);
  // The bridge 5cad169's words (rfp-022 f2fbdd9 web/deposit/test/vectors/compare-words-v1.json): the
  // app shows what the module returns, as it returns it; it never makes words itself.
  const VECTORS = [[["cream", "laugh", "load", "visual"], "cream · laugh · load · visual"], [["valley", "venue", "pyramid", "notable"], "valley · venue · pyramid · notable"], [["expire", "mimic", "burst", "echo"], "expire · mimic · burst · echo"]];
  check("compare vectors: words_text shown as given", VECTORS.map(([words, text]) => compareWords({ words, words_text: text, first: "000000", last: "000000" })), VECTORS.map(([, text]) => text));
  check("compare vectors: the same from the words alone", VECTORS.map(([words]) => compareWords({ words })), VECTORS.map(([, text]) => text));
}

{
  const { WHATS_NEW } = await import("./qml/logic.mjs");
  check("what's new: three short lines", [WHATS_NEW.length, WHATS_NEW.every((l) => l.length > 0 && l.length <= 110)], [3, true]);
}

{
  const { bridgesToAdd, BRIDGE_V4_PROGRAM, receiveHoldLine, receiveCodeRefused, RECEIVE_DEPLOYMENT, BUNDLED_BRIDGE } = await import("./qml/logic.mjs");
  const V2 = "JECKBkvoc2e3dC18Wm6zZJEC2BGMwUwLsdZiQzYTnUx7", V3 = "4eaeK3yiqATXhpCEYEFMMtjUn1bS7HLxPwfrH31mXCvr";
  const { BRIDGE_V5_PROGRAM, BRIDGE_V6_PROGRAM, BRIDGE_V7_PROGRAM } = await import("./qml/logic.mjs");
  check("v4 to v7 added to a saved list without them", bridgesToAdd([V2, V3]), [V2, V3, BRIDGE_V4_PROGRAM, "CuV1ctZ2qJG4wQcXP4TBudVGKzNmdoYT3DpP1veHnDMA", "4PxaLAfPZDvpfZaR8SsC2Y7pYMPNwPPGVGMpjEVPQWed", "CdijfhGnf1NjyCvYTZcGijQZzZ6soNJQ75Piex1ZTM93"]);
  check("v6 and v7 added to the saved list of the last build", bridgesToAdd([V2, V3, BRIDGE_V4_PROGRAM, BRIDGE_V5_PROGRAM]), [V2, V3, BRIDGE_V4_PROGRAM, BRIDGE_V5_PROGRAM, BRIDGE_V6_PROGRAM, BRIDGE_V7_PROGRAM]);
  check("nothing to add", [bridgesToAdd([V2, V3, BRIDGE_V4_PROGRAM, BRIDGE_V5_PROGRAM, BRIDGE_V6_PROGRAM, BRIDGE_V7_PROGRAM]), bridgesToAdd([]), bridgesToAdd(null), bridgesToAdd(["a", "b", "c", "d", "e", "f"])], [null, null, null, null]);
  check("v2, then v3 make room when full; never past six", [bridgesToAdd([V2, V3, BRIDGE_V4_PROGRAM, "d", "e"]), bridgesToAdd([V2, V3, BRIDGE_V4_PROGRAM, BRIDGE_V5_PROGRAM, "e", "f"]), bridgesToAdd(["a", "b", "c", "d"])],
    [[BRIDGE_V6_PROGRAM, BRIDGE_V7_PROGRAM, BRIDGE_V4_PROGRAM, "d", "e", BRIDGE_V5_PROGRAM], [BRIDGE_V6_PROGRAM, BRIDGE_V7_PROGRAM, BRIDGE_V4_PROGRAM, BRIDGE_V5_PROGRAM, "e", "f"], ["a", "b", "c", "d", BRIDGE_V4_PROGRAM, BRIDGE_V5_PROGRAM]]);
  check("Receive's deployment is v5", RECEIVE_DEPLOYMENT, "bridge-test-network-v5");
  const now = { build: { commit: BUNDLED_BRIDGE[0].build }, deployment: { name: "bridge-test-network-v4", bridge_program: BRIDGE_V4_PROGRAM } };
  check("no hold with the shipped bridge read", receiveHoldLine(now, [V2, V3, BRIDGE_V4_PROGRAM]), "");
  check("hold with an older bridge", receiveHoldLine({ build: { commit: "5cad169x" }, deployment: now.deployment }, [V2, V3]).startsWith("New codes come from the new LEZ Bridge"), true);
  check("hold with a bridge the wallet doesn't read", receiveHoldLine(now, [V2, V3]).startsWith("This wallet doesn't read"), true);
  check("no config, no hold", receiveHoldLine(null, []), "");
  check("refused: an older build", receiveCodeRefused("The deposit code of dep-1 can't be shown again: it was made by an older build of this module, which did not keep the vault it was for. Nothing was guessed").line,
    "This code can't be shown again: an older LEZ Bridge made it and didn't keep what's needed.");
  check("refused: other reasons, the reason kept for Details", [receiveCodeRefused("there is no deposit dep-9 on this computer").line, receiveCodeRefused("x").detail], ["This code can't be shown again.", "x"]);
}

{
  const { receiveProgress, receiveWhere, receiveRowsShown, receiveSteps, codeOlderVault, claimErrorTitle, receivesFrom } = await import("./qml/logic.mjs");
  // The bridge 0acf483: a receive it can't follow, and one arrived where the relay moved on.
  const lost = receiveProgress({ stage: "unknown", action: "recover", text: "It was made by an older build…" });
  check("a receive the bridge can't follow", [lost.stage, lost.card, lost.detail, lost.claim], ["lost", "Can't be followed here", "It was made by an older build…", false]);
  check("unknown without recover is still checking", receiveProgress({ stage: "unknown" }).stage, "checking");
  const moved = receiveProgress({ stage: "claimable", relay: false, text: "Arrived: 0.1 ETH is ready to claim on bridge v3 (an older vault)." });
  check("arrived, the relay moved on: no Claim", [moved.stage, moved.claim, moved.tone], ["noclaim", false, "plain"]);
  check("arrived with the relay: Claim", receiveProgress({ stage: "claimable", relay: true }).claim, true);
  const v3 = { id: "bridge-test-network-v3", label: "bridge v3", current: false };
  check("where: only when not the current vault", [receiveWhere({ deployment: v3 }, null), receiveWhere(null, { deployment: v3 }), receiveWhere({ deployment: Object.assign({}, v3, { current: true }) }, null), receiveWhere(null, null)], ["bridge v3", "bridge v3", "", ""]);
  const shown = receiveRowsShown([{ id: "a", symbol: "ETH", amount: "0.1", account: "A", deployment: v3 }], { a: { stage: "deposited", final_in_min: 5 } }, {}, 0, true).rows[0];
  check("card row says the older vault", shown.where, "bridge v3");
  check("steps: lost sits on Deposit, noclaim on Claim", [receiveSteps({ stage: "lost" }).find((x) => x.state === "now").key, receiveSteps({ stage: "noclaim" }).find((x) => x.state === "now").key], ["deposit", "claim"]);
  check("older-vault code", [codeOlderVault({ deployment: "bridge-test-network-v3" }), codeOlderVault({ deployment: "bridge-test-network-v5" }), codeOlderVault({}), codeOlderVault(null), codeOlderVault({ deployment: v3 }), codeOlderVault({ deployment: "bridge-test-network-v4" })], [true, false, false, false, true, true]);
  check("relay_other_bridge title", claimErrorTitle("relay_other_bridge"), "The relay no longer pays claims on this vault");
  check("receivesFrom keeps the deployment", receivesFrom({ receives: [{ id: "a", deployment: v3 }] })[0].deployment.label, "bridge v3");
}

{
  // 15 Nov, the owner's first v4 claim: the programs step's Approve stayed dim (two accounts shared
  // with the bridge, none ticked, the choice hidden), then "Try again" looped on the cancelled job.
  const { preTicked, sheetChoices, claimJobFor, claimErrorLine, addNameNotYours } = await import("./qml/logic.mjs");
  const two = { kind: "programs", app: "lez_bridge", choices: [{ account: "Public/AAA", granted: true }, { account: "Public/BBB", granted: true }, { account: "Public/CCC" }] };
  check("programs: the claim's account is ticked", preTicked(two, false, "BBB"), ["Public/BBB"]);
  check("programs: the claim's account with its prefix too", preTicked(two, false, "Public/BBB"), ["Public/BBB"]);
  check("programs: no hint, two shared: none (the sheet then shows the choice)", preTicked(two, false, ""), []);
  check("programs: one shared, no hint", preTicked({ kind: "programs", choices: [{ account: "Public/AAA", granted: true }] }, false, ""), ["Public/AAA"]);
  check("programs: an app that may be a fake: none", preTicked(two, true, "BBB"), []);
  check("programs: only the claim's account is offered", sheetChoices(two, "BBB").map((c) => c.account), ["Public/BBB"]);
  check("programs: without a claim, every account", sheetChoices(two, "").length, 3);
  check("other kinds keep their choices", sheetChoices({ kind: "connect", choices: two.choices }, "BBB").length, 3);
  const chk = { id: "rc-1", deposit: { account: "Public/BBB", leaf_index: 2 } };
  check("a cancelled job of this check is history", claimJobFor([{ id: "j1", check: "rc-1", state: "cancelled" }], chk), null);
  check("a failed one too", claimJobFor([{ id: "j1", check: "rc-1", state: "failed" }], chk), null);
  check("a running one is found", claimJobFor([{ id: "j2", check: "rc-1", state: "running" }, { id: "j1", check: "rc-1", state: "cancelled" }], chk).id, "j2");
  check("a done one is found", claimJobFor([{ id: "j3", check: "rc-1", state: "done" }], chk).id, "j3");
  check("wallet_rejected says which", [
    claimErrorLine("wallet_rejected", { message: "Nobody answered it in the Names app within 10 minutes; nothing was signed or sent." }),
    claimErrorLine("wallet_rejected", { message: "Cancelled; nothing was signed or sent." }),
    claimErrorLine("wallet_rejected", { message: "You rejected it in the Names app; nothing was signed or sent." }),
    claimErrorLine("wallet_rejected", {})],
    ["Nobody answered the approval within 10 minutes, so nothing was signed or sent.", "The approval was cancelled, so nothing was signed or sent.",
     "The approval was rejected, so nothing was signed or sent.", "The approval ended without a yes, so nothing was signed or sent."]);
  const { canApprove } = await import("./qml/logic.mjs");
  const progs = { kind: "programs", app: "lez_bridge", choices: two.choices, sheet: { warnings: ["no label"] } };
  check("programs: ticked box + the claim's account enables Approve", canApprove({ request: progs, chosen: preTicked(progs, false, "BBB"), risky: true, understood: true }), true);
  check("programs: the box not ticked: dim", canApprove({ request: progs, chosen: ["Public/BBB"], risky: true, understood: false }), false);
  check("programs: no account ticked: dim (the short sheet then shows the accounts)", canApprove({ request: progs, chosen: [], risky: true, understood: true }), false);
  check("relayed call: its account and a built sheet", canApprove({ request: { kind: "relayed_call", account: "Public/BBB", sheet: { warnings: [] } }, chosen: ["Public/BBB"], risky: false }), true);
  check("relayed call refused by the wallet: dim", canApprove({ request: { kind: "relayed_call", sheet: { refused: true } }, chosen: ["Public/BBB"] }), false);
  check("connect with the hint ticked", canApprove({ request: { kind: "connect", choices: two.choices }, chosen: preTicked({ kind: "connect", choices: two.choices, account_hint: "Public/BBB" }, false, ""), risky: false }), true);
  check("secret with its account", canApprove({ request: { kind: "secret", account: "Public/AAA", choices: [{ account: "Public/AAA" }] }, chosen: preTicked({ kind: "secret", account: "Public/AAA", choices: [{ account: "Public/AAA" }] }, false, ""), risky: false }), true);
  const { receiveBalanceLine } = await import("./qml/logic.mjs");
  const blocks = [{ account: "Public/3J29s1Wy62NYMRuTe6MWXQrpWkqUAkv9t6PqzHWF7CNA", assets: [{ symbol: "wETH", name: "Wrapped Sepolia ETH (demo) · bridge v2", amount: "0" }, { symbol: "wETH", name: "Wrapped Sepolia ETH (v4) · bridge v4", amount: "0.01" }] }];
  check("balance line: the bridge that holds it, not v2's 0", receiveBalanceLine(blocks, "Public/3J29s1Wy62NYMRuTe6MWXQrpWkqUAkv9t6PqzHWF7CNA", "ETH"), "3J29s1…7CNA now holds 0.01 wETH.");
  const { normalizedReply } = await import("./qml/logic.mjs");
  check("normalize through callModuleAsync (JSON-encoded once or twice)", [normalizedReply('"jef04"').value, normalizedReply(JSON.stringify(JSON.stringify("jef04"))).value, normalizedReply('""').value, normalizedReply("jef04").value, normalizedReply(JSON.stringify({ success: true, value: "jef04" })).value],
    ["jef04", "jef04", "", "jef04", "jef04"]);
  check("normalize: a real busy answer stays busy", normalizedReply(JSON.stringify({ success: false, busy: true, error: "busy" })).kind, "busy");
  check("Add: a name not yours", [addNameNotYours("Adding the name didn't finish: this wallet doesn't hold jef04, so it isn't added."), addNameNotYours("Adding the name didn't finish: Error: jef04 is not registered"), addNameNotYours("Adding the name didn't finish: couldn't reach the sequencer")], [true, true, false]);
}

// ── Send back to Ethereum ──
{
  const { sendBackOffer, sendBackAmounts, sendBackSymbol, sendBackCode, sendBackCodeTitle, sendBackProgress, sendBacksFrom, sendBackRowsShown, sendBackSteps, sendBackBurnHeld, clockShown, BRIDGE_V4_PROGRAM, bridgeNetRows } = await import("./qml/logic.mjs");
  const V3 = "4eaeK3yiqATXhpCEYEFMMtjUn1bS7HLxPwfrH31mXCvr";
  const weth = (bridge, base) => ({ symbol: "wETH", bridge, base, decimals: 18 });
  check("offered on v5 too", sendBackOffer(weth("CuV1ctZ2qJG4wQcXP4TBudVGKzNmdoYT3DpP1veHnDMA", "10000000000000000")), true);
  check("offered on v4 with a denomination held", [sendBackOffer(weth(BRIDGE_V4_PROGRAM, "10000000000000000")), sendBackOffer(weth(BRIDGE_V4_PROGRAM, "9999999999999999")), sendBackOffer(weth(V3, "100000000000000000")), sendBackOffer(null)], [true, false, false, false]);
  check("dUSDC on v4 from 100", [sendBackOffer({ symbol: "dUSDC", bridge: BRIDGE_V4_PROGRAM, base: "100000000" }), sendBackOffer({ symbol: "LEZ", bridge: BRIDGE_V4_PROGRAM, base: "999" })], [true, false]);
  check("amounts held only", sendBackAmounts(null, weth(BRIDGE_V4_PROGRAM, "150000000000000000")).map((d) => d.text + "@" + d.index), ["0.01@0", "0.1@1"]);
  check("amounts from the registry", sendBackAmounts({ tokens: [{ symbol: "ETH", denoms: [{ index: 0, value: "10000000000000000" }, { index: 1, value: "100000000000000000" }, { index: 2, value: "1000000000000000000" }] }] }, weth(BRIDGE_V4_PROGRAM, "1000000000000000000")).map((d) => d.text), ["0.01", "0.1", "1"]);
  check("a registry without values falls back", sendBackAmounts({ tokens: [{ symbol: "ETH", denoms: [] }] }, weth(BRIDGE_V4_PROGRAM, "10000000000000000")).map((d) => d.text), ["0.01"]);
  check("no amounts below the smallest", sendBackAmounts(null, weth(BRIDGE_V4_PROGRAM, "1")), []);
  check("symbol for the bridge", [sendBackSymbol("wETH"), sendBackSymbol("dUSDC"), sendBackSymbol("X")], ["ETH", "dUSDC", "X"]);
  const code = "logoswd1qyqqqqqqqqqzr7dm2wcgqnmxn3uq7c2yupx7qfguwl6zcw0d2ej8zhwpsa8x4a5xg9ws3xcqp";
  check("code from a link, spaces, capitals", [sendBackCode(code), sendBackCode("https://nameswallet.app/bridge/#" + code), sendBackCode("  " + code.slice(0, 20) + "\n" + code.slice(20) + " "), sendBackCode(code.toUpperCase()), sendBackCode("logosdep1abc"), sendBackCode("")], [code, code, code, code, "", ""]);
  check("code titles", ["other_vault", "other_chain", "bad_code:checksum", "x"].map(sendBackCodeTitle), ["This code is for another bridge vault", "This code is for another chain", "That is not a usable code", "The code was refused"]);
  const at = (s) => "T" + s;
  const stages = ["planned", "approve", "burning", "burned", "burn_unclear", "recorded", "proving", "proved", "wrapped", "releasable", "released", undefined];
  check("stage tones", stages.map((stage) => sendBackProgress(stage ? { stage } : null, 0, at).tone), ["ready", "ready", "wait", "wait", "problem", "ready", "wait", "wait", "wait", "ready", "done", "plain"]);
  check("stage actions", stages.map((stage) => sendBackProgress({ stage }, 0, at).action), ["burn", "", "", "", "check", "prove", "", "seal", "", "release", "", ""]);
  check("burned: the honest wait, no time promised", [sendBackProgress({ stage: "burned" }, 0, at).headline, sendBackProgress({ stage: "burned" }, 0, at).detail],
    ["Waiting for Ethereum to record it.", ""]);
  {
    // A v7 send-back (seq-sig gate): the LEZ Bridge's `text`/`wait` with `update` (rfp-022 V7-IN-WALLET.md §2, sendback.rs
    // gate_wait_text, word for word) are the waiting line; v4/v5 carry `text` but no `update`, and keep the generic wait.
    const window = { stage: "burned", update: { window_s: 600, pending: true, finalizable_at: 5000, block_id: 28000, covers: true, dead: false },
      text: "Burned. Waiting for the 10-minute contest window: Ethereum has the update that records this burn, and it can be applied when the window ends.",
      wait: { what: "the 10-minute contest window", until: 5000, seconds: 600 } };
    const w = sendBackProgress(window, 4400, at);
    check("v7 burned: the contest window is the wait", [w.stage, w.card, w.headline, w.tone, w.action, w.gate],
      ["burned", "Burned · waiting for the 10-minute contest window until T5000", "Waiting for the 10-minute contest window: Ethereum has the update that records this burn, and it can be applied when the window ends.", "wait", "", true]);
    const passed = { stage: "burned", update: { window_s: 600, pending: true, finalizable_at: 5000, block_id: 28000, covers: true, dead: false },
      text: "Burned. The contest window has passed: waiting for the update that records this burn to be applied on Ethereum.",
      wait: { what: "the update to be applied on Ethereum (its contest window has passed)", until: null, seconds: null } };
    const nothing = { stage: "burned", update: { window_s: 600, pending: false, dead: false },
      text: "Burned. Waiting for the next update on Ethereum: two of the bridge's signers sign its blocks, then a proof is made (by hand today, so hours).",
      wait: { what: "the next update on Ethereum (two of the bridge's signers sign, then a proof; by hand today)", until: null, seconds: null } };
    const dead = { stage: "burned", update: { window_s: 600, pending: false, dead: true },
      text: "Burned, but the bridge's update gate on Ethereum has stopped: it can't take updates any more. Nothing is lost on the bridge test network; ask the bridge's operator.",
      wait: { what: "an update the bridge's gate can no longer take (it has stopped)", until: null, seconds: null } };
    check("v7 burned: window passed, nothing staged, a stopped gate", [passed, nothing, dead].map((x) => { const p = sendBackProgress(x, 6000, at); return [p.card, p.headline, p.tone] }), [
      ["Burned · waiting for the update to be applied on Ethereum", "The contest window has passed: waiting for the update that records this burn to be applied on Ethereum.", "wait"],
      ["Burned · waiting for the next update on Ethereum", "Waiting for the next update on Ethereum: two of the bridge's signers sign its blocks, then a proof is made (by hand today, so hours).", "wait"],
      ["Burned · waiting for an update the bridge's gate can no longer take", "Burned, but the bridge's update gate on Ethereum has stopped: it can't take updates any more. Nothing is lost on the bridge test network; ask the bridge's operator.", "problem"]]);
    check("v7 burned: past the window's end no time is promised", sendBackProgress(window, 5000, at).card, "Burned · waiting for the 10-minute contest window");
    const v5 = sendBackProgress({ stage: "burned", text: "Burned. Waiting for Ethereum to record it: the bridge operator posts updates by hand today.", wait: { what: "the next update on Ethereum (the bridge operator, by hand today)", seconds: null }, update: null }, 0, at);
    check("v4/v5 burned: unchanged (no update, so the module's text is not used)", [v5.card, v5.headline, v5.gate], ["Burned · waiting for Ethereum to record it", "Waiting for Ethereum to record it.", undefined]);
    check("the card row carries the window too", sendBackRowsShown([{ id: "s7", amount: "0.01 ETH", account: "Public/A", recipient: "0xR", stage: "burned" }], { s7: window }, {}, 4400 * 1000)[0].progress.card,
      "Burned · waiting for the 10-minute contest window until T5000".replace("T5000", clockShown(5000)));
  }
  {
    // The hand steps are said once, in one place (SEND_BACK_BY_HAND), and the times come from one constant.
    const { SEND_BACK_TIMES, SEND_BACK_BY_HAND, sendBackByHand } = await import("./qml/logic.mjs");
    check("by hand: said for the stages that wait on it", ["burning", "burned", "recorded", "proving", "proved", "wrapped"].map(sendBackByHand), [false, true, false, false, true, false]);
    check("by hand: the one line names both hand steps", [/update/.test(SEND_BACK_BY_HAND), /wrapper/.test(SEND_BACK_BY_HAND)], [true, true]);
    check("no stage repeats the by-hand line", ["burned", "proved"].map((stage) => /by hand/.test(sendBackProgress({ stage }, 0, at).detail + sendBackProgress({ stage }, 0, at).headline)), [false, false]);
    check("times from one place", [sendBackProgress({ stage: "proving" }, 0, at).headline.includes(SEND_BACK_TIMES.proof), sendBackProgress({ stage: "proved" }, 0, at).headline.includes(SEND_BACK_TIMES.wrap), sendBackProgress({ stage: "burning" }, 0, at).headline.includes(SEND_BACK_TIMES.burn)], [true, true, true]);
    check("burn unclear: a problem with Check again, never 'look in the Names app'", (() => { const x = sendBackProgress({ stage: "burn_unclear", text: "Included in a block, but the account's balance did not go down: it may have been refused. Look in the Names app." }, 0, at); return [x.tone, x.action, /Names app/.test(x.detail)] })(), ["problem", "check", false]);
  }
  check("wrapped: releasable at the vault's time", [sendBackProgress({ stage: "wrapped", releasable_at: 5000 }, 4000, at).headline, sendBackProgress({ stage: "wrapped", releasable_at: 5000 }, 5000, at).headline], ["Releasable at T5000.", "Releasable any minute now."]);
  check("recorded: the earliest release under it", sendBackProgress({ stage: "recorded", releasable_at: 5000 }, 1000, at).detail, "Releasable at T5000 at the earliest (the vault's delay).");
  check("proof and wrapper times", [sendBackProgress({ stage: "proving" }, 0, at).headline, sendBackProgress({ stage: "proved" }, 0, at).headline], ["Making the release proof on this computer: about 20 s.", "Proof made. Next: its Ethereum wrapper (about 3 min once it is run), then the release."]);
  check("approved, the relay refused: its words, not a wait for approval", [sendBackProgress({ stage: "approve", note: "The bridge's relay refused the burn: cap. Nothing was sent." }, 0, at).headline, sendBackProgress({ stage: "approve", note: "x" }, 0, at).tone], ["Signed; the bridge's relay hasn't sent it yet and tries again at each check.", "wait"]);
  check("the endpoint busy is not a relay refusal", sendBackProgress({ stage: "approve", busy: true, note: "The bridge's endpoint is busy; trying again shortly." }, 0, at).headline, "Waiting for your approval.");
  const { rateLimited } = await import("./qml/logic.mjs");
  check("429 words are a wait", [rateLimited("HTTP 429 Too Many Requests"), rateLimited("can't reach the bridge test network: too many requests"), rateLimited("The bridge's endpoint is busy; trying again shortly."), rateLimited("no such receive")], [true, true, true, false]);
  check("clock is HH:MM", /^[0-2][0-9]:[0-5][0-9]$/.test(clockShown(1763208000)), true);
  check("send_backs rows", sendBacksFrom({ send_backs: [{ id: "sb-1", amount_text: "0.01 ETH", account: "Public/A", recipient: "0xAb", words_text: "a · b · c · d", stage: "burned", symbol: "ETH" }, { stage: "x" }] }),
    [{ id: "sb-1", amount: "0.01 ETH", account: "Public/A", recipient: "0xAb", words: "a · b · c · d", stage: "burned", symbol: "ETH" }]);
  const rows = sendBackRowsShown([{ id: "a", stage: "burned" }, { id: "b", stage: "released" }, { id: "c", stage: "released" }], { a: { stage: "recorded" } }, { b: 1000 }, 2000);
  check("card rows: under way, and received only for a while", rows.map((x) => x.id + ":" + x.progress.stage), ["a:recorded", "b:released"]);
  const now = (st) => sendBackSteps(st).find((x) => x.state === "now");
  check("steps follow the stage", [now({ stage: "choose" }).key, now({ stage: "address" }).key, now({ stage: "needs", needs: "secret" }).key, now({ stage: "planned", extras: ["secret"] }).key, now({ stage: "approve" }).key, now({ stage: "burned" }).key, now({ stage: "wrapped" }).key],
    ["source", "address", "secret", "burn", "burn", "back", "back"]);
  check("steps: a first-time question stays in the row", sendBackSteps({ stage: "burned", extras: ["secret"] }).map((x) => x.short + ":" + x.state), ["Account:done", "Address:done", "Keys:done", "Approved:done", "Ethereum:now"]);
  check("steps: received, all done", sendBackSteps({ stage: "released" }).map((x) => x.state), ["done", "done", "done", "done"]);
  check("steps: the last title", sendBackSteps({ stage: "released" })[3].title, "Received on Ethereum");
  check("names_update said plainly", [sendBackBurnHeld({ needs: "names_update" }), sendBackBurnHeld({ stage: "approve" })], ["This Names Wallet can't sign a burn yet: it needs a newer Names build. Nothing was signed or sent.", ""]);
  const read = { bridges: [{}, {}], accounts: [{ account: "Public/A", found: true, native: "0", tokens: [{ symbol: "wETH", amount: "100000000000000000", decimals: 18, bridge: BRIDGE_V4_PROGRAM, bridge_label: "bridge v4", definition: "D" }] }] };
  const asset = bridgeNetRows(read, null)[0].assets[1];
  const { sendBackSources } = await import("./qml/logic.mjs");
  const blocks = [
    { account: "Public/A", assets: [{ symbol: "LEZ", amount: "5" }, { symbol: "wETH", amount: "0.1", bridge: BRIDGE_V4_PROGRAM, base: "100000000000000000" }] },
    { account: "Public/B", assets: [{ symbol: "wETH", amount: "0.1", bridge: V3, base: "100000000000000000" }] },
    { account: "Public/C", watched: true, assets: [{ symbol: "wETH", amount: "1", bridge: BRIDGE_V4_PROGRAM, base: "1000000000000000000" }] },
    { account: "Public/D", viewOnly: true, assets: [{ symbol: "wETH", amount: "1", bridge: BRIDGE_V4_PROGRAM, base: "1000000000000000000" }] },
    { account: "Public/E", assets: [{ symbol: "dUSDC", amount: "100", bridge: BRIDGE_V4_PROGRAM, base: "100000000" }, { symbol: "wETH", amount: "0.001", bridge: BRIDGE_V4_PROGRAM, base: "1000000000000000" }] },
  ];
  check("sources: own accounts with a v4 denomination, nothing else", sendBackSources(blocks).map((x) => x.key), ["Public/A|sym:wETH", "Public/E|sym:dUSDC"]);
  check("sources: none", [sendBackSources([]).length, sendBackSources(null).length], [0, 0]);
  check("step 1 asks for the account and the amount", sendBackSteps({ stage: "choose" })[0].title, "Choose the account and the amount");
  check("card asset rows carry what Send back needs", [asset.bridge === BRIDGE_V4_PROGRAM, asset.base, asset.decimals, sendBackOffer(asset)], [true, "100000000000000000", 18, true]);
}

// ── Checks on demand (15 Nov): when asked, once on view, by itself only for jobs and short waits ──
{
  const { receiveAutoMode, sendBackAutoMode, bridgeNextCheck, CHECK_JOB_MS, CHECK_GENTLE_MS } = await import("./qml/logic.mjs");
  const now = 1_000_000;
  check("receive modes", ["requested", "deposited", "final", "claimable", "claimed", undefined].map((stage) => receiveAutoMode(stage ? { stage } : null)), ["none", "gentle", "gentle", "none", "none", "none"]);
  check("send-back modes", ["planned", "approve", "burning", "burned", "recorded", "proving", "proved", "releasable", "released", "burn_unclear"].map((stage) => sendBackAutoMode({ stage }).mode),
    ["none", "job", "job", "none", "none", "job", "none", "none", "none", "none"]);
  check("send-back: a relay refusal is not polled, a busy endpoint gently", [sendBackAutoMode({ stage: "approve", note: "refused" }).mode, sendBackAutoMode({ stage: "burned", busy: true, note: "busy" }).mode], ["none", "gentle"]);
  check("send-back: releasable time known", sendBackAutoMode({ stage: "wrapped", releasable_at: 2000 }), { mode: "at", at: 2005000 });
  const gentle = { mode: "gentle", focused: true }, job = { mode: "job", focused: false }, none = { mode: "none", focused: true };
  check("no timer when nothing shows", [bridgeNextCheck({ now, visible: false, frontmost: true, screen: "receive", items: [gentle, job] })], [0]);
  check("not in front: only a job goes on", [bridgeNextCheck({ now, visible: true, frontmost: false, screen: "receive", items: [gentle] }), bridgeNextCheck({ now, visible: true, frontmost: false, screen: "card", items: [job] })], [0, now + CHECK_JOB_MS]);
  check("the card checks by itself only for a job", [bridgeNextCheck({ now, visible: true, frontmost: true, screen: "card", items: [gentle, none] }), bridgeNextCheck({ now, visible: true, frontmost: true, screen: "card", items: [gentle, job] })], [0, now + CHECK_JOB_MS]);
  check("a waiting screen: gently, its own item only", [bridgeNextCheck({ now, visible: true, frontmost: true, screen: "receive", items: [gentle] }), bridgeNextCheck({ now, visible: true, frontmost: true, screen: "receive", items: [{ mode: "gentle", focused: false }] })], [now + CHECK_GENTLE_MS, 0]);
  check("an operator wait: nothing by itself", bridgeNextCheck({ now, visible: true, frontmost: true, screen: "sendback", items: [none] }), 0);
  check("a known time: once when it passes", [bridgeNextCheck({ now, visible: true, frontmost: true, screen: "sendback", items: [{ mode: "at", at: now + 600000, focused: true }] }), bridgeNextCheck({ now, visible: true, frontmost: true, screen: "sendback", items: [{ mode: "at", at: now - 1, focused: true }] })], [now + 600000, 0]);
  check("a job in progress still advances, quickly", bridgeNextCheck({ now, visible: true, frontmost: true, screen: "sendback", items: [{ mode: "job", focused: true }, gentle] }), now + CHECK_JOB_MS);
  check("gentle is 2-3 min, a job 5 s", [CHECK_GENTLE_MS >= 120000 && CHECK_GENTLE_MS <= 180000, CHECK_JOB_MS], [true, 5000]);
  // Check now works in every state: the line always offers it (Main.qml shows the button unless a
  // check runs), and asking is never refused by a mode: every stage maps to a mode, none throws.
  const stages = ["planned", "approve", "burning", "burned", "burn_unclear", "recorded", "proving", "proved", "wrapped", "releasable", "released", "x", undefined];
  const { checkButton } = await import("./qml/logic.mjs");
  check("Check now: pressable when idle, Checking… at once when pressed or while a check runs", [checkButton(false, false), checkButton(false, true), checkButton(true, false)],
    [{ label: "Check now", enabled: true }, { label: "Checking…", enabled: false }, { label: "Checking…", enabled: false }]);
  check("every stage has a mode", stages.every((stage) => ["none", "job", "gentle", "at"].indexOf(sendBackAutoMode({ stage }).mode) >= 0), true);
}

{
  // The uniform status lines (qml/STYLE.md): four kinds, waits never red.
  const { noticeKind, isWaitMessage, toneKind, setupKind, claimNext, approveWhy, receiveMakeWhy, STATUS_KINDS, claimErrorLine } = await import("./qml/logic.mjs");
  check("four kinds", STATUS_KINDS, ["working", "waiting", "problem", "done"]);
  const busy = "The wallet is busy; still working.";
  const noAnswer = "No answer from the wallet yet: it may still be working on it. Look before trying again.";
  const starting = "Starting… the wallet is still loading (up to half a minute after Basecamp opens).";
  check("busy, no answer and starting are waits, not problems", [noticeKind(busy, true), noticeKind(noAnswer, true), noticeKind(starting, true)], ["waiting", "waiting", "waiting"]);
  check("a real failure is a problem", noticeKind("That is not a program ID.", true), "problem");
  // sayRead says the starting line with `false` (say(shown, shown !== moduleStarting)): still a wait.
  check("starting said with false is a wait, not a green tick", noticeKind(starting, false), "waiting");
  check("a plain note has no kind", noticeKind("That account has no name.", "plain"), "");
  check("a plain result is done; a said kind is kept; nothing is nothing", [noticeKind("Added Gold to your wallet.", false), noticeKind("Sending 5 LEZ…", "waiting"), noticeKind("Asking the wallet to sign.", "working"), noticeKind("", true)], ["done", "waiting", "working", ""]);
  check("the bridge module busy and the 429 are waits", [isWaitMessage("The LEZ Bridge module is still answering an earlier call."), isWaitMessage("HTTP 429 Too Many Requests"), isWaitMessage("the relay refused")], [true, true, false]);
  check("tones", ["wait", "done", "problem", "ready", "plain"].map(toneKind), ["waiting", "done", "problem", "", ""]);
  check("setup states", ["failed", "stuck", "busy", "done", "install", "unload"].map(setupKind), ["problem", "problem", "waiting", "done", "", ""]);
  // A refusal no retry can change never offers Check again (it was a dead loop on "already claimed").
  check("already claimed: the way out", claimNext("already_claimed", false, true), { kind: "problem", label: "Back to the wallet", action: "back" });
  check("not final: a wait with Check now (the same words as everywhere)", claimNext("not_final", false, true), { kind: "waiting", label: "Check now", action: "check" });
  check("failed claim from an arrived deposit: Try again, not Choose file", claimNext("relay_refused", true, false), { kind: "problem", label: "Try again", action: "retry" });
  check("a bad file: choose another", claimNext("bad_file", false, true).label, "Choose another file");
  // relay_cap is also the bridge's answer for fees above what the relay pays (relayed.rs:684).
  check("relay_cap doesn't promise it is today's limit", /today's limit/.test(claimErrorLine("relay_cap")), false);
  check("reverted says it was included", claimErrorLine("reverted").includes("included"), true);
  check("rejected doesn't say who said no", /You said no|in your wallet/.test(claimErrorLine("wallet_rejected", { message: "You rejected it in the Names app; nothing was signed or sent." })), false);
  // A disabled Approve always says why.
  const req = { kind: "relayed_call", sheet: { warnings: ["x"] } };
  check("approveWhy: the tick", approveWhy({ request: req, busy: false, chosen: ["Public/A"], risky: true, understood: false }), "Tick the box above to approve.");
  check("approveWhy: nothing when it can be approved", approveWhy({ request: req, busy: false, chosen: ["Public/A"], risky: true, understood: true }), "");
  check("approveWhy: no account", approveWhy({ request: { kind: "connect" }, busy: false, chosen: [] }), "Choose the account above.");
  check("approveWhy: registry check", approveWhy({ request: { kind: "call", checking: true }, chosen: ["a"] }), "Waiting for the Program Registry's answer.");
  check("receiveMakeWhy", [receiveMakeWhy({ account: "A", ticked: false }), receiveMakeWhy({ account: "", ticked: true }), receiveMakeWhy({ account: "A", ticked: true }), receiveMakeWhy({ asking: true })],
    ["Tick “Not private” above to make the code.", "Choose the account above.", "", ""]);
}

{
  // The page itself keeps to STYLE.md: red only in problem lines, no red boxes, no violet text, one
  // back button, an honest Reject.
  const qml = readFileSync(new URL("./qml/Main.qml", import.meta.url), "utf8");
  check("no red borders (tick boxes, buttons)", (qml.match(/border\.color:[^\n]*root\.alarm/g) || []).length, 0);
  check("no red Hint: problems go through StatusLine", qml.split("\n").filter((l) => /\bHint \{[^\n]*root\.alarm/.test(l)).length, 0);
  check("no violet under-way text tone", /root\.underway\b/.test(qml), false);
  check("one back button: no tiny ‹ Wallet / ← Back links", [/"‹ Wallet"/.test(qml), /"← Back"/.test(qml)], [false, false]);
  // "Not now" only where nothing is refused: the settlement's consent (§4a), which leaves it as it was,
  // and a checked bridge code (Settings → Bridge, the owner's words: "Use this bridge / Not now"; nothing is kept).
  check("no button says Not now for a reject", qml.split("\n").filter((l) => /label: "Not now"/.test(l) && !/sbSettleAsking = false/.test(l) && !/bridgeAddOpen = false/.test(l)).length, 0);
  check("the global notice says its kind", (qml.match(/root\.noticeIsError \? root\.alarm/g) || []).length, 0);
  check("every sub-page has the shared header", (qml.match(/PageHeader \{ title:/g) || []).length >= 9, true);
  // "If they ignore it, we ignore it": no timer reads the card's balances; the setup poll runs only on view.
  check("no 30 s card balance timer", /id: bridgeNetPoll/.test(qml), false);
  check("the setup poll waits for its step to be on screen", /"stuck"\) && root\.bridgeSetupOnView/.test(qml), true);
}

{
  // "Not now" rejected for good and the app then said "You said no": one label (Reject), and the
  // after-state never says who said no (the module also rejects when access was taken back).
  const { approvalEndedLine, approvalEnded, claimNext } = await import("./qml/logic.mjs");
  check("ended lines", ["rejected", "expired", "cancelled"].map(approvalEndedLine), ["The approval was rejected, so nothing was signed or sent.", "Nobody answered the approval within 10 minutes, so nothing was signed or sent.", "The approval was cancelled, so nothing was signed or sent."]);
  check("none says who said no", ["rejected", "expired", "cancelled"].map(approvalEndedLine).some((l) => /you said|in your wallet/i.test(l)), false);
  check("they offer Start again", ["rejected", "expired", "cancelled"].map((x) => approvalEnded(approvalEndedLine(x))), [true, true, true]);
  check("a rejected claim: Start again", claimNext("wallet_rejected", true, false), { kind: "problem", label: "Start again", action: "retry" });
}

{
  const { sealInputProblem, sendBackProgress } = await import("./qml/logic.mjs");
  const seal = "0xe71e6620" + "ab".repeat(256);
  check("seal box: other codes said plainly", [sealInputProblem("logoswd1qyqq"), sealInputProblem("LOGOSREL1QQ"), sealInputProblem("https://nameswallet.app/bridge/#logosdep1abc")],
    ["That is the withdraw-to code, not the seal. The seal starts 0xe71e6620.", "That is the release code, not the seal. The seal starts 0xe71e6620.", "That is a deposit code, not the seal. The seal starts 0xe71e6620."]);
  check("seal box: hex checks", [sealInputProblem(seal), sealInputProblem(seal.slice(2)), sealInputProblem("hello"), sealInputProblem("0x1234"), sealInputProblem(seal.slice(0, 100))],
    ["", "", "That isn't a seal: the seal is hex and starts 0xe71e6620.", "That isn't this proof's seal: it starts 0xe71e6620.", "The seal looks cut short: it is 260 bytes (520 hex digits) from 0xe71e6620."]);
  const proved = sendBackProgress({ stage: "proved" }, 0, String);
  check("after the proof: never complete, the next wait", [proved.tone, /complete|done|received/i.test(proved.headline + proved.card), proved.headline.indexOf("Next") >= 0], ["wait", false, true]);
  const stages = ["planned", "approve", "burning", "burned", "recorded", "proving", "proved", "wrapped", "releasable"];
  check("only released is done", stages.every((stage) => sendBackProgress({ stage }, 0, String).tone !== "done" && !/complete|received/i.test(sendBackProgress({ stage }, 0, String).headline)), true);
}

{
  const { faucetReply, grouped } = await import("./qml/logic.mjs");
  check("faucet: got, in a block", faucetReply({ ok: true, amount: "10000000", block: "11222" }, grouped), { line: "Got 10,000,000 LEZ · block 11222.", kind: "done", retry: false });
  // The module asks the faucet on its own thread: until its answer is in it says pending, a working
  // line (never red), and the app asks it again for the answer.
  check("faucet: an ask under way is working, asked again", faucetReply({ ok: false, code: "pending", retry: true, reason: "Asking the faucet…" }), { line: "Asking the faucet…", kind: "working", retry: true, pending: true });
  check("faucet: refusals plainly, waits not red", [faucetReply({ ok: false, code: "already_today", reason: "Once a day per account." }).kind, faucetReply({ ok: false, code: "busy", retry: true }).retry, faucetReply({ ok: false, code: "bad_key" }).kind, faucetReply({ ok: false, code: "faucet_empty" }).line],
    ["waiting", true, "problem", "The faucet is empty for now."]);
}

{
  const { sendBackProgress, sendBackAutoMode, bridgeNextCheck } = await import("./qml/logic.mjs");
  // The bridge's settlement service (rfp-022 NAMES-SEND-BACK.md §4a).
  const offer = { available: true, consent_needed: true, consent_text: SETTLE_CONSENT };
  const rec = sendBackProgress({ stage: "recorded", settle_offer: offer }, 0, () => "12:00");
  check("settle: offered on recorded", [rec.action, rec.more, rec.headline], ["settle", "prove", "Recorded on Ethereum: finish it on Ethereum."]);
  check("settle: not offered: the by-hand path as before", sendBackProgress({ stage: "recorded", settle_offer: { available: false, reason: "The service is paused." } }, 0).action, "prove");
  check("settle: its reason in small text", settleNotOffered({ stage: "recorded", settle_offer: { available: false, reason: "The service is paused." } }), "The service is paused.");
  const run = (stage, extra) => sendBackProgress(Object.assign({ stage: "recorded", settle_id: "s1", settle: Object.assign({ stage }, extra || {}) }), 0, () => "12:34");
  check("settle: stage words", ["queued", "proving", "wrapping", "releasing"].map((x) => run(x).headline),
    ["Waiting its turn on the bridge's machine.", "Making the release proof (about 20 s).", "Wrapping the proof for Ethereum (about 4 min).", "Sending the release on Ethereum."]);
  check("settle: its own text wins", run("queued", { text: "Waiting for memory on the bridge's machine." }).headline, "Waiting for memory on the bridge's machine.");
  check("settle: the vault's delay with a time", run("releasable_at", { releasable_at: 1 }).headline, "Waiting for the vault's delay, until 12:34.");
  check("settle: running has no button and checks every 30 s", [run("proving").action, sendBackAutoMode({ stage: "recorded", settle_id: "s1", settle: { stage: "proving" } }).mode], ["", "settle"]);
  check("settle: card", run("wrapping").card, "Finishing on Ethereum · Wrapping the proof for Ethereum");
  const failed = sendBackProgress({ stage: "recorded", settle_id: "s1", settle: { stage: "failed", error: "The service lost it." }, settle_offer: offer }, 0);
  check("settle: failed: its error, the button again", [failed.action, failed.detail, failed.tone], ["settle", "The service lost it.", "problem"]);
  check("settle: released with its tx", [sendBackProgress({ stage: "released", released_tx: "0xabc" }, 0).headline, sendBackProgress({ stage: "released", released_tx: "0xabc" }, 0).detail], ["Released on Ethereum.", "Sepolia transaction 0xabc."]);
  check("settle: the next check in 30 s on its own screen in front", bridgeNextCheck({ visible: true, frontmost: true, screen: "sendback", now: 1000, items: [{ mode: "settle", focused: true }] }), 31000);
  check("settle: consent sentence verbatim", SETTLE_CONSENT, "This wallet has no proof helper, so the bridge's service will make the proof: it learns which burn pays which Ethereum address.");
}

{
  const { bridgeCodeInput, bridgeRowLine, bridgeActiveLine, bridgeCheckShown, bridgesWith, BRIDGE_CODE_WARNING, codeOlderVault, numbered,
          RECEIVE_DEPOSIT_STEPS, BRIDGE_V4_PROGRAM, BRIDGE_V5_PROGRAM } = await import("./qml/logic.mjs");
  // Settings → Bridge (rfp-022 NAMES-SEND-BACK.md §9): the code box, the list, the check's answer.
  check("bridge code: pasted with spaces, or in the page's link", [bridgeCodeInput(" logosbr1abc\n").code, bridgeCodeInput("https://nameswallet.app/bridge/#logosbr1xyz").code, bridgeCodeInput("LOGOSBR1ABC").code], ["logosbr1abc", "logosbr1xyz", "LOGOSBR1ABC"]);
  check("bridge code: another kind of code, or nothing", [bridgeCodeInput("logoswd1abc").problem, bridgeCodeInput("hello").problem, bridgeCodeInput("").problem, bridgeCodeInput("logosdep1q").code],
    ["That is another kind of code. A bridge code starts logosbr1.", "This is not a bridge code. A bridge code starts logosbr1.", "", ""]);
  check("bridge warning: one plain sentence", BRIDGE_CODE_WARNING, "A bridge code decides where your deposits go: take it only from the bridge's own page.");
  const v5 = { id: "bridge-test-network-v5", label: "bridge v5", kind: "built in", vault_short: "0xAFbD…eD10", current: true };
  const v6 = { id: "added-abc", label: "bridge v6", kind: "added", vault_short: "0x6666…6666", endpoint_host: "lez.example.org" };
  check("bridge rows", [bridgeRowLine(v5), bridgeRowLine(v6), bridgeRowLine(null)], ["bridge v5 · vault 0xAFbD…eD10 · built in", "bridge v6 · vault 0x6666…6666 · added · lez.example.org", ""]);
  check("the chosen bridge in a sentence", [bridgeActiveLine({ active_row: v5 }), bridgeActiveLine(null)], ["New receives and sends to Ethereum use bridge v5 (vault 0xAFbD…eD10, built in).", ""]);
  const checked = { ok: true, id: "added-abc", label: "bridge v6", vault_short: "0x6666…6666", endpoint_host: "lez.example.org", words_text: "a · b · c · d", checks: [{ ok: true, text: "The vault matches." }] };
  check("bridge check: a new bridge, checked", bridgeCheckShown(checked), { kind: "done", line: "Checked on Sepolia and on lez.example.org: bridge v6, vault 0x6666…6666.", words: "a · b · c · d", use: true, checks: ["✓ The vault matches."] });
  check("bridge check: built in, and already chosen", [bridgeCheckShown({ ok: true, builtin: true, label: "bridge v4", words_text: "w" }).line, bridgeCheckShown({ ok: true, builtin: true, current: true, label: "bridge v5" }).use],
    ["This is bridge v4, built into this app.", false]);
  check("bridge check: a mismatch is a problem, a network that can't be read a wait", [
    bridgeCheckShown({ ok: false, error: { code: "mismatch", message: "The deposit desk sends deposits elsewhere. Refused: nothing was added." }, checks: [{ ok: false, text: "desk" }] }),
    bridgeCheckShown({ ok: false, error: { code: "unreachable", message: "Sepolia can't be read now." } }).kind],
    [{ kind: "problem", line: "The deposit desk sends deposits elsewhere. Refused: nothing was added.", words: "", use: false, checks: ["✕ desk"] }, "waiting"]);
  // The wallet reads the chosen bridge: added to its list (at most 6), v2 and v3 make room first.
  const V2 = "JECKBkvoc2e3dC18Wm6zZJEC2BGMwUwLsdZiQzYTnUx7", V3 = "4eaeK3yiqATXhpCEYEFMMtjUn1bS7HLxPwfrH31mXCvr", V6 = "Six6", V7 = "Seven7", V8 = "Eight8", V9 = "Nine9";
  check("bridge list: added with room, unchanged when there", [bridgesWith([V3, BRIDGE_V4_PROGRAM, BRIDGE_V5_PROGRAM], V6, []), bridgesWith([V2, V3, BRIDGE_V4_PROGRAM, BRIDGE_V5_PROGRAM], BRIDGE_V5_PROGRAM, []), bridgesWith([V2, V3, BRIDGE_V4_PROGRAM, BRIDGE_V5_PROGRAM, V6], V7, [])],
    [[V3, BRIDGE_V4_PROGRAM, BRIDGE_V5_PROGRAM, V6], null, [V2, V3, BRIDGE_V4_PROGRAM, BRIDGE_V5_PROGRAM, V6, V7]]);
  check("bridge list: a seventh bridge takes v2's place, then v3's", [bridgesWith([V2, V3, BRIDGE_V4_PROGRAM, BRIDGE_V5_PROGRAM, V6, V7], V8, [BRIDGE_V5_PROGRAM]), bridgesWith([V8, V3, BRIDGE_V4_PROGRAM, BRIDGE_V5_PROGRAM, V6, V7], V9, [V8])],
    [[V8, V3, BRIDGE_V4_PROGRAM, BRIDGE_V5_PROGRAM, V6, V7], [V8, V9, BRIDGE_V4_PROGRAM, BRIDGE_V5_PROGRAM, V6, V7]]);
  check("bridge list: a bridge with deposits on it is kept; full when all are", [bridgesWith([V2, V3, BRIDGE_V4_PROGRAM, BRIDGE_V5_PROGRAM, V6, V7], V8, [V2, V3, BRIDGE_V5_PROGRAM, V6, V7]), bridgesWith(["a", "b", "c", "d", "e", "f"], V8, ["a", "b", "c", "d", "e", "f"])],
    [[V2, V3, V8, BRIDGE_V5_PROGRAM, V6, V7], "full"]);
  // v6 and v7 are added bridges to the LEZ Bridge ("added-" and their cfgId's first 6 bytes): named
  // by their program where a row has it, else by that id; any other added bridge keeps its own name.
  const { bridgeLabelOf, receiveWhere } = await import("./qml/logic.mjs");
  const v6row = { id: "added-7d1a304a12a4", label: "sepolia-v6", kind: "added", vault_short: "0x95cE…427E", bridge_program: "4PxaLAfPZDvpfZaR8SsC2Y7pYMPNwPPGVGMpjEVPQWed", endpoint_host: "lez.nameswallet.app" };
  const v7row = { id: "added-7ef9153b69a3", label: "", kind: "added", vault_short: "0x4D7B…d23E", bridge_program: "CdijfhGnf1NjyCvYTZcGijQZzZ6soNJQ75Piex1ZTM93" };
  check("v6 and v7 named on their rows", [bridgeRowLine(v6row), bridgeRowLine(v7row), bridgeActiveLine({ active_row: v7row })],
    ["bridge v6 · vault 0x95cE…427E · added · lez.nameswallet.app", "bridge v7 · vault 0x4D7B…d23E · added", "New receives and sends to Ethereum use bridge v7 (vault 0x4D7B…d23E, added)."]);
  check("v6 and v7 named by the bridge's id alone (a deployment view)", [bridgeLabelOf({ id: "added-7d1a304a12a4", label: "added-7d1a304a12a4" }), bridgeLabelOf({ id: "added-7ef9153b69a3", label: "added-7ef9153b69a3" }),
    receiveWhere({ deployment: { id: "added-7d1a304a12a4", label: "added-7d1a304a12a4", current: false } }), receiveWhere({ deployment: { id: "added-7ef9153b69a3", label: "added-7ef9153b69a3", current: true } })],
    ["bridge v6", "bridge v7", "bridge v6", ""]);
  check("another added bridge keeps its own name; built-ins by program", [bridgeLabelOf({ id: "added-0123456789ab", label: "my bridge" }), bridgeLabelOf({ id: "added-0123456789ab", label: "" }), bridgeLabelOf({ id: "x", bridge_program: BRIDGE_V4_PROGRAM, label: "y" }), bridgeLabelOf(null)],
    ["my bridge", "added-0123456789ab", "bridge v4", ""]);
  check("bridge check of the v7 code: named bridge v7", bridgeCheckShown({ ok: true, id: "added-7ef9153b69a3", label: "sepolia-v7", bridge_program: "CdijfhGnf1NjyCvYTZcGijQZzZ6soNJQ75Piex1ZTM93", vault_short: "0x4D7B…d23E", endpoint_host: "lez.nameswallet.app" }).line,
    "Checked on Sepolia and on lez.nameswallet.app: bridge v7, vault 0x4D7B…d23E.");
  // A code made on another bridge than the one chosen now is "for an older vault"; the bridge says which is current.
  check("older vault: the bridge's current flag wins", [codeOlderVault({ deployment: { id: "added-abc", current: true } }), codeOlderVault({ deployment: { id: "bridge-test-network-v5", current: false } })], [false, true]);
  // What to do next, numbered (owner, 2026-11-16).
  check("receive: the deposit step in order", numbered(RECEIVE_DEPOSIT_STEPS), [
    "1. Copy the link and open it in the browser with your Ethereum wallet.",
    "2. Check the page shows these four words. If they differ, stop.",
    "3. Deposit there, then come back here and press Check now."]);
}

{
  const { sendBackAddress, sendBackCode, addressHalves, sendBackCodeTitle } = await import("./qml/logic.mjs");
  // Bridge to Ethereum by a pasted address (owner, 2026-11-16): its case kept (the checksum lives in it).
  const A = "0x68B54942A37aE5ceBA34D47eFB8d33a293d3975d";
  check("address: kept as pasted, from a link, with spaces", [sendBackAddress(" " + A + "\n"), sendBackAddress("https://x/#" + A), sendBackAddress("0X" + A.slice(2))], [A, A, A]);
  check("address: not one", [sendBackAddress(A.slice(0, 41)), sendBackAddress("logoswd1abc"), sendBackAddress(""), sendBackCode(A)], ["", "", "", ""]);
  check("address: two halves to read back", addressHalves(A), ["0x68B54942A37aE5ceBA34", "D47eFB8d33a293d3975d"]);
  check("address: refusal title", sendBackCodeTitle("bad_address:no_checksum"), "That address can't be used");
  const { SEND_BACK_PRIVATE_HINT, historyLine } = await import("./qml/logic.mjs");
  // The receiving account's past: nothing sent and nothing held; sent from; holding; not read. Never "new".
  check("history: zero/zero, sent, holds, unknown", [historyLine({ checked: true, sent: 0, balance: "0" }), historyLine({ checked: true, sent: 3, balance: "0" }).text,
    historyLine({ checked: true, sent: 0, balance: "5000000000000000", balance_text: "0.005 ETH" }).text, historyLine({ checked: false }), historyLine(null).text],
    [{ kind: "", text: "Nothing has been sent from this account and it holds nothing." },
     "This account has been used before (3 transactions sent). Anyone can see its history. For privacy, add a new account in your wallet and use that one. Or carry on: it's your choice.",
     "This account has been used before (holds 0.005 ETH). Anyone can see its history. For privacy, add a new account in your wallet and use that one. Or carry on: it's your choice.",
     { kind: "unknown", text: "Couldn't check this account's history." }, ""]);
  check("history: one sent and holding a speck", historyLine({ checked: true, sent: 1, balance: "1" }).text.indexOf("(1 transaction sent / holds less than 0.0001 ETH)") > 0, true);
  const { ethAtMost4 } = await import("./qml/logic.mjs");
  check("history: at most 4 decimals", ["4885795182198553171", "5000000000000000", "1000000000000000000", "1234567000000000000000", "0", "99999999999999"].map(ethAtMost4), ["4.8857", "0.005", "1", "1,234.567", "0", "less than 0.0001"]);
  check("history: never says new or unused", [historyLine({ checked: true, sent: 0, balance: "0" }).text, historyLine({ checked: true, sent: 2, balance: "0" }).text].some((t) => /\b(unused)\b|^New account/i.test(t)), false);
  check("the privacy hint suggests, never requires", [SEND_BACK_PRIVATE_HINT.startsWith("Want it private?"), /must|need to|required/i.test(SEND_BACK_PRIVATE_HINT.split(":")[0])], [true, false]);
}

{
  const { sendBackOffer, sendBackSources, sendBackElsewhere, BRIDGE_V4_PROGRAM, BRIDGE_V5_PROGRAM } = await import("./qml/logic.mjs");
  // Send-backs are planned on the bridge chosen in Settings → Bridge: only its balances are offered
  // (an added bridge's too); the others are named with where to choose them.
  const weth = (bridge, base, label) => ({ symbol: "wETH", bridge, base, amount: "0.01", bridgeLabel: label || "" });
  const v5 = { program: BRIDGE_V5_PROGRAM, release: "proof" }, v6 = { program: "Six6", release: "proof" };
  check("chosen bridge: its balance only", [sendBackOffer(weth(BRIDGE_V5_PROGRAM, "10000000000000000"), v5), sendBackOffer(weth(BRIDGE_V4_PROGRAM, "10000000000000000"), v5), sendBackOffer(weth("Six6", "10000000000000000"), v6), sendBackOffer(weth("Six6", "10000000000000000"), { program: "Six6", release: "never" })], [true, false, true, false]);
  const blocks = [{ account: "Public/A", assets: [weth(BRIDGE_V5_PROGRAM, "10000000000000000", "bridge v5")] }];
  check("chosen bridge: the other bridge's balance is named", [sendBackSources(blocks, { program: BRIDGE_V4_PROGRAM, release: "proof" }).length, sendBackElsewhere(blocks, { program: BRIDGE_V4_PROGRAM, release: "proof" }), sendBackElsewhere(blocks, v5), sendBackElsewhere(blocks, null)],
    [0, "0.01 wETH on bridge v5: choose bridge v5 in Settings → Bridge to send it back.", "", ""]);
}

{
  // The private claim and the private send-back (rfp-022 proposal/PRIVATE-IN-WALLET-SCOPE.md, step 7).
  const L = await import("./qml/logic.mjs");
  const PRIV = "Private/yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z";
  // The adapter: every bridge call the private screens make, in one place.
  check("private adapter: the bridge's calls", L.PRIVATE_BRIDGE, { receive: "make_deposit_request", claimPlan: "claim_plan", claim: "claim", job: "job", jobs: "jobs",
    sendBackPlan: "send_back_plan", sendBackBurn: "send_back_burn", afterUncertain: "after_uncertain",
    // wallet-next (docs/WALLET-NEXT-MODULE-CONTRACT.md): the address step and the card's Remove.
    sendBackCheck: "send_back_check", sendBackRecipients: "send_back_recipients", addressBookList: "address_book_list", addressBookSave: "address_book_save",
    addressBookRemove: "address_book_remove", receiveRemove: "receive_remove", sendBackRemove: "send_back_remove" });
  check("private adapter: features (PRIVATE-IN-WALLET.md)", [L.privateCan(["private_claim_standin"], "claim"), L.privateCan(["private_claim_standin"], "receive"), L.privateCan(["receive"], "receive"), L.privateCan(null, "sendBack"),
    L.privateCan(["private_send_back"], "sendBack"), L.privateCan(["private_send_back"], "nothing")], [true, true, false, false, true, false]);
  check("private send-back as the module plans it", [L.sendBackIsPrivate({ kind: "private", account: "x" }), L.sendBackIsPrivate({ route: "private_call" })], [true, true]);
  check("private send-back: the burn's reply carries the job", L.privateReply({ id: "sb-1", stage: "approve", burn_phase: "approve", wallet_job: "wr-2", job: { id: "wr-2", state: "running", phase: "approve", request: "r2" } }).job.id, "wr-2");
  check("private refusal: the bridge's float is low (the operator tops it up): a wait", L.privateRefusal("float_low").kind, "waiting");
  // Replies: a first-time question (flat, and claim_plan's nested one), a job, a status carrying one, a refusal, an older Names.
  const job = { id: "wr-1", kind: "claim", subject: "dep-p1", request: "r1", state: "running", phase: "approve", wallet_request: { state: "pending", check: "ready" } };
  check("private reply: needs", [L.privateReply({ needs: "secret", request: { request: "r0" } }).kind, L.privateReply({ ok: false, needs: { needs: "connect", request: { request: "r0" } } }).needs.needs], ["needs", "connect"]);
  check("private reply: job", [L.privateReply(job).kind, L.privateReply({ stage: "approve", job: job }).kind, L.privateReply({ stage: "approve", job: job }).job.id], ["job", "status", "wr-1"]);
  check("private reply: refusal, older Names, plain", [L.privateReply({ ok: false, error: { code: "not_final", message: "x" } }).error.code, L.privateReply({ needs: "names_update" }).kind, L.privateReply({ ok: true }).kind, L.privateReply(null).kind],
    ["not_final", "held", "other", "other"]);
  check("private reply: a public send-back is not a job", L.privateReply({ id: "sb-1", stage: "planned" }).kind, "status");
  check("private job found again", [L.privateJobFor({ jobs: [Object.assign({}, job, { state: "done", id: "wr-0" }), job] }, ["claim"], "dep-p1").id, L.privateJobFor({ jobs: [job] }, ["claim"], "dep-2"), L.privateJobFor([job], [], "dep-p1").id],
    ["wr-1", null, "wr-1"]);
  // The job as the screens say it.
  const st = (j) => L.privateJob(Object.assign({}, job, j), "claim");
  check("private job: checking, then the sheet", [st({ wallet_request: { state: "pending", check: "checking" } }).stage, st({}).stage, st({}).tone], ["checking", "approve", "ready"]);
  check("private job: proving (simulated again first), sending", [st({ phase: "simulate", wallet_request: { state: "approved" } }).stage, st({ phase: "simulate", wallet_request: { state: "approved" } }).detail,
    st({ phase: "prove" }).headline, st({ phase: "submit" }).stage, st({ phase: "confirm" }).tone],
    ["proving", "Your wallet checks it once more, then proves it.", "Proving the claim on this computer: about 10 minutes.", "sending", "wait"]);
  check("private job: done", [st({ state: "done", phase: "done", result: { tx: "ab", ok: true } }).headline, st({ state: "done", result: { tx: "ab" } }).tx, L.privateJob(Object.assign({}, job, { state: "done" }), "burn").card],
    ["Claimed privately.", "ab", "Burned privately"]);
  check("private job: the wallet's check stopped it", [st({ state: "failed", error: { code: "wallet_check", message: "The Names Wallet's check stopped this call. Open the Names app to see why." } }).headline, st({ state: "failed", error: { code: "wallet_check" } }).tone],
    ["Your wallet couldn't check this claim, so it can't be approved. Nothing was signed or sent.", "problem"]);
  check("private job: failed while proving, in the wallet's words (no 'open the Names app' inside it)", st({ state: "failed", phase: "prove", error: { code: "wallet_failed", message: "The Names Wallet says: proving stopped: out of memory. Open the Names app for the details." } }).headline,
    "Your wallet couldn't finish the claim: proving stopped: out of memory.");
  check("private job: failed while sending says it may be proved", st({ state: "failed", phase: "submit", error: { code: "wallet_failed", message: "The Names Wallet says: refused." } }).detail, "It may have been proved but not sent.");
  check("private job: ended (never says who said no)", [st({ state: "cancelled", wallet_request: { state: "rejected", check: "ready" } }).headline, st({ state: "cancelled", wallet_request: { state: "expired", check: "ready" } }).headline,
    st({ state: "cancelled", wallet_request: { state: "expired", check: "checking" } }).headline, st({ state: "cancelled", wallet_request: { state: "cancelled" } }).stage],
    ["The approval was rejected, so nothing was signed or sent.", "Nobody answered the approval within 10 minutes, so nothing was signed or sent.",
     "Your wallet's check took more than 10 minutes, so the request lapsed before it could be approved. Nothing was signed or sent.", "ended"]);
  check("private job: a lapse while still checking is not 'nobody answered'", /Nobody answered/.test(st({ state: "cancelled", wallet_request: { state: "expired", check: "checking" } }).headline), false);
  check("private job: only the busy note shows", [L.privateNoteShown("Asking your Names Wallet how this stands: busy. Asking again shortly."), L.privateNoteShown("Your wallet is proving it on this computer (about 11 minutes, about 9.5 GB of memory).")],
    ["Asking your Names Wallet how this stands: busy. Asking again shortly.", ""]);
  check("private job: none", [L.privateJob(null, "claim"), L.privateJob(undefined, "burn")], [null, null]);
  // The wait's clock.
  check("private clock", [L.privateElapsed(1000, 1000 + 252000), L.privateElapsed(0, 5), L.privateElapsed(1000, 1000 + 5000)],
    ["4:12 so far · about 10 minutes in all.", "Proving takes about 10 minutes.", "0:05 so far · about 10 minutes in all."]);
  check("private clock: past 15 min it says so, and that it goes on", L.privateElapsed(1, 1 + 17 * 60000), "17:00 so far · about 10 minutes in all. It is taking longer than usual; it goes on.");
  // The screen's stage and steps.
  check("private claim stage", [L.privateClaimStage({ looking: true }), L.privateClaimStage({}), L.privateClaimStage({ checking: true }), L.privateClaimStage({ asking: true }), L.privateClaimStage({ needs: true }),
    L.privateClaimStage({ uncertain: true }), L.privateClaimStage({ refused: true }), L.privateClaimStage({ job: { stage: "proving" }, checking: true })],
    ["looking", "start", "checking", "asking", "needs", "uncertain", "refused", "proving"]);
  const steps = (s) => L.privateClaimSteps(s).map((x) => (x.state === "done" ? "✓" : x.state === "now" ? ">" : "") + x.short);
  check("private claim steps", [steps({ stage: "start" }), steps({ stage: "needs", needs: "secret" }), steps({ stage: "proving", extras: ["secret"] }), steps({ stage: "sending" }), steps({ stage: "done" }), steps({ stage: "failed", failedAt: "prove" })],
    [["✓Arrived", ">Approve", "Prove", "Claim"], ["✓Arrived", ">Keys", "Approve", "Prove", "Claim"], ["✓Arrived", "✓Keys", "✓Approved", ">Prove", "Claim"], ["✓Arrived", "✓Approved", "✓Proved", ">Claim"],
     ["✓Arrived", "✓Approved", "✓Proved", "✓Claimed"], ["✓Arrived", "✓Approved", ">Prove", "Claim"]]);
  check("private claim steps: the last says claimed only when done", [L.privateClaimSteps({ stage: "done" }).pop().title, L.privateClaimSteps({ stage: "proving" }).pop().title], ["Claimed privately", "Claim"]);
  check("private refusals: one action, waits amber", [L.privateRefusal("not_final"), L.privateRefusal("root_not_attested").kind, L.privateRefusal("already_claimed"), L.privateRefusal("wrong_account").label],
    [{ kind: "waiting", label: "Check now", action: "check" }, "waiting", { kind: "problem", label: "Back to the wallet", action: "back" }, "Back to the wallet"]);
  // Accounts and rows.
  const accts = [{ account: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", kind: "public" }, { account: PRIV, kind: "private" }, { account: PRIV, kind: "private" }, { account: "Private/short", kind: "private" }];
  check("private accounts: private only, once, well formed", L.privateAccounts(accts), [PRIV]);
  check("private receive rows", [L.receiveIsPrivate({ account: PRIV }), L.receiveIsPrivate({ account: "yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z", account_kind: "private" }), L.receiveIsPrivate({ account: "Public/x" }), L.receiveIsPrivate(null),
    L.receiveAccountShown({ account: "yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z", account_kind: "private" })], [true, true, false, false, PRIV]);
  check("card rows keep the account's kind", L.receiveRowsShown([{ id: "d", symbol: "ETH", amount: "0.1", account: PRIV, account_kind: "private" }], { d: { stage: "claimable" } }, {}, 0, false).rows[0].account_kind, "private");
  // Make the deposit code: private needs a bridge that makes private codes, never the "Not private" tick.
  check("make why: private", [L.receiveMakeWhy({ kind: "private", able: true, account: PRIV, ticked: false }), L.receiveMakeWhy({ kind: "private", able: false, account: PRIV }), L.receiveMakeWhy({ kind: "private", able: true, account: "" })],
    ["", "This LEZ Bridge can't make private codes yet. Update it, or choose Public.", "Choose the account above."]);
  check("make why: public unchanged", [L.receiveMakeWhy({ account: "A", ticked: false }), L.receiveMakeWhy({ kind: "public", account: "A", ticked: true })], ["Tick “Not private” above to make the code.", ""]);
  // Send back from a private account: its holding when the wallet lists one, else every amount.
  const held = [{ token_program: "T", holdings: [{ account: PRIV, account_kind: "private", symbol: "wETH", balance: "100000000000000000", decimals: 18 }] }];
  const srcs = L.privateSendBackSources(accts, held);
  check("private sources: the holding", [srcs.length, srcs[0].account, srcs[0].amount, srcs[0].private], [1, PRIV, "0.1", true]);
  check("private sources: unknown balance offers every amount", [L.privateSendBackSources(accts, []).map((s) => s.asset.anyAmount), L.sendBackAmounts(null, L.privateSendBackSources(accts, [])[0].asset).map((d) => d.text)],
    [[true], ["0.01", "0.1", "1"]]);
  check("private sources: a holding caps the amounts", L.sendBackAmounts(null, srcs[0].asset).map((d) => d.text), ["0.01", "0.1"]);
  check("private send-back", [L.sendBackIsPrivate({ account: PRIV }), L.sendBackIsPrivate({ account: "Public/x" })], [true, false]);
  check("private send-back steps", L.sendBackSteps({ stage: "proving_burn", private: true }).map((x) => (x.state === "now" ? ">" : x.state === "done" ? "✓" : "") + x.short), ["✓Account", "✓Address", "✓Approved", ">Prove", "Ethereum"]);
  check("public send-back steps unchanged", L.sendBackSteps({ stage: "approve" }).map((x) => x.key), ["source", "address", "burn", "back"]);
  // No background polling: a private wait is read every 30 s, on its own screen, in front only.
  check("private pace", [L.PRIVATE_CHECK_MS >= 30000, L.sendBackAutoMode({ account: PRIV, stage: "approve" }).mode, L.sendBackAutoMode({ account: "Public/x", stage: "approve" }).mode, L.sendBackAutoMode({ account: PRIV, stage: "burned" }).mode],
    [true, "private", "job", "none"]);
  check("private pace: when", [L.bridgeNextCheck({ visible: true, frontmost: true, screen: "sendback", now: 1000, items: [{ mode: "private", focused: true }] }),
    L.bridgeNextCheck({ visible: true, frontmost: false, screen: "sendback", now: 1000, items: [{ mode: "private", focused: true }] }),
    L.bridgeNextCheck({ visible: true, frontmost: true, screen: "card", now: 1000, items: [{ mode: "private", focused: false }] })], [31000, 0, 0]);
  // Words: never "official", never "lookup"; one time for the proof.
  const words = [L.PRIVATE_RECEIVE_NOTE, L.PRIVATE_CLAIM_NOTE, L.PRIVATE_SEND_BACK_NOTE, L.PRIVATE_LEAVE_LINE]
    .concat(["approve", "simulate", "prove", "submit", "done"].map((p) => st({ phase: p }).headline))
    .concat(["claim", "burn"].map((w) => L.privateJob(Object.assign({}, job, { state: "failed", error: { code: "wallet_check" } }), w).headline + L.privateJob(Object.assign({}, job, { state: "failed", error: { code: "wallet_check" } }), w).detail));
  check("private words: no 'official', no 'lookup'", words.filter((w) => /official|look ?up/i.test(w)), []);
  check("private words: the time said once, as about 10 minutes", [L.PRIVATE_PROOF_TIME, [L.PRIVATE_RECEIVE_NOTE, L.PRIVATE_CLAIM_NOTE, L.PRIVATE_SEND_BACK_NOTE].every((w) => w.indexOf(L.PRIVATE_PROOF_TIME) >= 0)], ["about 10 minutes", true]);
  const qml = readFileSync(new URL("./qml/Main.qml", import.meta.url), "utf8");
  // Every string the page shows: never "official", never "lookup" (say "checking a name").
  const literals = qml.match(/"(?:[^"\\\n]|\\.)*"/g) || [];
  // (WALLET-ROADMAP item 4: the older "look up" strings now say "checking a name" too.)
  check("Main.qml strings: no 'official', no 'look up' in any form", literals.filter((l) => /official|look(s|ed|ing)?[ -]?up/i.test(l)), []);
  // The private wait's timers: one read every 30 s while its screen is open and in front; the 1 s clock reads nothing.
  check("private timer: 30 s, gated on its screen", /interval: Logic\.PRIVATE_CHECK_MS\s*\n\s*repeat: true\s*\n\s*running: root\.privateWaitOnView\s*\n\s*onTriggered: root\.privatePoll\(\)/.test(qml), true);
  check("private clock: moves the time only", /interval: 1000\s*\n\s*repeat: true\s*\n\s*running: root\.privateWaitOnView\s*\n\s*onTriggered: root\.privateNow = Date\.now\(\)\s*\n/.test(qml), true);
  check("private wait: only on its own screen, in front", /readonly property bool privateWaitOnView: section === "wallet" && receiveFrontmost/.test(qml), true);
  check("private screen: the wallet's own sheet in place", [/ApproveBody \{ visible: root\.appInline && root\.walletView === "pclaim"; inline: true \}/.test(qml), /readonly property bool appInline: appPrivateHere \|\|/.test(qml)], [true, true]);
  check("private screen: the bridge's calls only through the adapter", (qml.match(/bridgeCall\("(claim|claim_plan|job|jobs|make_deposit_request)"/g) || []).length, 0);
  check("private screen: every button says what it does at once", [/"Checking…" : pclaimScreen\.stage === "asking" \? "Asking your wallet…"/.test(qml), /root\.receiveAsking \? "Making the code…"/.test(qml)], [true, true]);
}

// ── wallet-next (rfp-022 proposal/WALLET-ROADMAP.md items 1, 4, 5, 7, 8): the address step of
//    Bridge to Ethereum, the saved addresses, Remove on the card, the "checking a name" wording. ──
{
  const L = await import("./qml/logic.mjs");
  const A = "0x68B54942A37aE5ceBA34D47eFB8d33a293d3975d";
  const B = "0xE2C4857976b346F8F9b3a7be714Cd92d0bF68680";
  const CODE = "logoswd1qyqqqqqqqqqzr7dm2wcgqnmxn3uq7c2yupx7qfguwl6zcw0d2ej8zhwpsa8x4a5xg9ws3xcqp";
  const ok = (key) => ({ ok: true, key, recipient: A, words_text: "lens · mushroom · enjoy · orient" });
  const st = (o) => L.addressStep(Object.assign({ canAddress: true }, o));
  // The box: empty at first; nothing pre-filled; the form says what it holds.
  check("address step: empty box", [st({ input: "" }).form, st({ input: "   " }).form, st({ input: "" }).show, st({ input: "" }).canContinue], ["empty", "empty", false, false]);
  check("address step: empty says what to do", st({ input: "" }).why, "Paste a code or an address, or choose a previous address.");
  check("address step: not a code or an address", [st({ input: "hello" }).form, st({ input: A.slice(0, 30) }).form, st({ input: "hello" }).show], ["bad", "bad", false]);
  check("address step: an address on a bridge that takes only codes", [st({ input: A, canAddress: false }).form, /Update it/.test(st({ input: A, canAddress: false }).why), st({ input: CODE, canAddress: false }).form], ["bad", true, "checking"]);
  check("address step: well-formed and not checked yet: check it now", [st({ input: A }).form, st({ input: A }).checkNow, st({ input: A, checking: true }).checkNow, st({ input: " " + CODE + "\n" }).key], ["checking", true, false, CODE]);
  check("address step: the words show only once checked", [st({ input: A, check: ok(A) }).show, st({ input: A, checking: true }).show, st({ input: A, error: "x", errorKey: A }).show], [true, false, false]);
  // The check belongs to the box's value: another value in the box hides it.
  check("address step: a check for another value is not shown", [st({ input: B, check: ok(A) }).show, st({ input: B, check: ok(A) }).form, st({ input: B, check: ok(A) }).checkNow], [false, "checking", true]);
  check("address step: a check without its key is not shown", st({ input: A, check: { ok: true, recipient: A } }).show, false);
  check("address step: refused, for this value only", [st({ input: A, error: "This address is all in small letters.", errorKey: A }).form, st({ input: A, error: "x", errorKey: B }).form], ["refused", "checking"]);
  // The tick is required; the button says why it waits.
  check("address step: continue only with the tick", [st({ input: A, check: ok(A) }).canContinue, st({ input: A, check: ok(A), ticked: true }).canContinue], [false, true]);
  check("address step: the reason under the disabled button", [st({ input: A, check: ok(A) }).why, st({ input: A, check: ok(A), ticked: true }).why], ["Tick “This is my address” to continue.", ""]);
  check("address step: not twice while the plan runs", st({ input: A, check: ok(A), ticked: true, asking: true }).canContinue, false);
  check("address step: the words of the design", [L.ADDRESS_CONTINUE, L.ADDRESS_TICK, L.ADDRESS_LAST_CHECK, L.ADDRESS_CHOOSE_TITLE, L.ADDRESS_SAVE_TITLE],
    ["I've checked the address — continue", "This is my address", "Last check before the burn. Withdrawals to a wrong address can't be recovered.", "Choose a previous address", "Save to my addresses"]);
  check("address step: the words line says what and where (roadmap 7: where to compare)", L.ADDRESS_WORDS_LINE, "These four words are a fingerprint of this address. Compare them on the bridge page: open nameswallet.app/bridge (Bridge to Ethereum) in your browser with that Ethereum wallet connected; it shows the same four words.");
  check("address step: the next step is named for what it does (roadmap 7)", L.ADDRESS_BURN_NEXT.indexOf("Next: approve the burn") === 0, true);
  check("address step: step 3 says the burn is next", /^Next: approve the burn /.test(L.ADDRESS_BURN_NEXT), true);
  check("address step: box key", [L.addressBoxKey(" " + A + " "), L.addressBoxKey("https://nameswallet.app/bridge/#" + CODE), L.addressBoxKey("nope")], [A, CODE, ""]);

  // The previous addresses: the book's names and words; an older bridge's recipients; each once.
  const book = { addresses: [{ address: A, short: "0x68B5…975d", nickname: "main", words_text: "lens · mushroom · enjoy · orient", last_used: 2, saved: true },
                             { address: B, short: "0xE2C4…8680", nickname: "", words_text: "maple · tiger · seven · lamp", last_used: 1, saved: false },
                             { address: A.toLowerCase(), nickname: "dup" }, { address: "0x123", nickname: "bad" }] };
  const rows = L.addressBookRows(book);
  check("previous addresses: each once, valid only, in order", rows.map((r) => r.address), [A, B]);
  check("previous addresses: name, short address and four words", [rows[0].title, rows[0].sub, rows[1].title, rows[1].sub],
    ["main", "0x68B5…975d · lens · mushroom · enjoy · orient", "Sent to before", "0xE2C4…8680 · maple · tiger · seven · lamp"]);
  const old = L.addressBookRows({ recipients: [{ account: "Public/x", address: A, short: "0x68B5…975d", last_used: 1 }] });
  check("previous addresses: an older bridge's list (no name, no words)", [old.length, old[0].title, old[0].sub, old[0].saved], [1, "Sent to before", "0x68B5…975d", false]);
  check("previous addresses: nothing", [L.addressBookRows(null), L.addressBookRows({}), L.addressBookRows({ addresses: "x" })], [[], [], []]);
  check("previous addresses: the one in the box", [L.addressBookChosen(rows, A).nickname, L.addressBookChosen(rows, A.toLowerCase()), L.addressBookChosen(rows, CODE), L.addressBookChosen(rows, " " + B).title], ["main", null, null, "Sent to before"]);

  // Save to my addresses: a short name, offered after the first send to an address without one.
  check("nickname rules", ["main", " ledger ", "", "x".repeat(25), "my wallet.2_b-c", "ünï", "a/b"].map(L.nicknameProblem),
    ["", "", "Give it a short name, such as main or ledger.", "Keep the name to 24 characters or fewer.", "", "Use letters, digits, spaces, dots, dashes or underscores.", "Use letters, digits, spaces, dots, dashes or underscores."]);
  const offer = (o) => L.addressSaveOffered(Object.assign({ can: true, stage: "burned", recipient: B, rows }, o));
  check("save offer: after the burn went out, unnamed address", [offer({}), offer({ stage: "released" }), offer({ stage: "settling" }), offer({ stage: "recorded" })], [true, true, true, true]);
  check("save offer: not before the burn, not a named one, not without the bridge's book", [offer({ stage: "planned" }), offer({ stage: "approve" }), offer({ recipient: A }), offer({ can: false }), offer({ recipient: "" })],
    [false, false, false, false, false]);
  check("save reply: saved", L.addressSaveReply({ ok: true, entry: { address: B, nickname: "ledger" }, addresses: book.addresses }, "ledger"),
    { ok: true, line: "Saved as “ledger”. The name stays in this wallet on this computer, never on chain.", kind: "done", rows });
  check("save reply: refused, in plain words", [L.addressSaveReply({ ok: false, error: { code: "full" } }, "x").line, L.addressSaveReply({ ok: false, error: { code: "bad_nickname" } }, "").line,
    L.addressSaveReply({ ok: false, error: { code: "x", message: "Kept as it was." } }, "a").line, L.addressSaveReply(null, "a").kind,
    L.addressSaveReply({ ok: false, error: { code: "nickname_taken" } }, "main").line, L.addressSaveReply({ ok: false, error: { code: "bad_address:no_checksum" } }, "main").line],
    ["Your list of addresses is full (50). Remove one in Settings → Bridge first.", "Give it a short name, such as main or ledger.", "Kept as it was.", "problem",
     "Another address already has that name. Choose another.", "That address can't be saved."]);

  // Remove on the card: offered only where removing can't lose money.
  const rp = (stage) => L.receiveProgress({ stage });
  check("remove: a receive with no deposit seen, or claimed", ["requested", "claimed"].map((x) => L.removeOffered("receive", rp(x))), [true, true]);
  check("remove: never a deposited, unclaimed receive", ["deposited", "final", "claimable", "unknown", undefined].map((x) => L.removeOffered("receive", rp(x))), [false, false, false, false, false]);
  check("remove: never a claimable receive whose relay moved on", L.removeOffered("receive", L.receiveProgress({ stage: "claimable", relay: false })), false);
  const sp = (stage, more) => L.sendBackProgress(Object.assign({ stage }, more || {}), 0);
  check("remove: a send-back never proposed, or released", [L.removeOffered("send_back", sp("planned"), { stage: "planned" }), L.removeOffered("send_back", sp("released"), { stage: "released" })], [true, true]);
  check("remove: never a send-back with a burn on its way", ["approve", "burning", "burned", "recorded", "proving", "proved", "wrapped", "releasable", "burn_unclear"].map((x) => L.removeOffered("send_back", sp(x), { stage: x })),
    [false, false, false, false, false, false, false, false, false]);
  check("remove: never while settling", L.removeOffered("send_back", sp("recorded", { settle_id: "s1", settle: { stage: "wrapping" } }), { stage: "recorded", settle_id: "s1", settle: { stage: "wrapping" } }), false);
  check("remove: a planned send-back with a burn sent or a wallet job", [L.removeOffered("send_back", sp("planned"), { stage: "planned", burn_tx: "ab" }),
    L.removeOffered("send_back", sp("planned"), { stage: "planned", wallet_job: "wr-1" }),
    L.removeOffered("send_back", sp("planned"), { stage: "planned", wallet_job: "wr-1" }, { id: "wr-1", state: "running" }),
    L.removeOffered("send_back", sp("planned"), { stage: "planned", wallet_job: "wr-1" }, { id: "wr-1", state: "cancelled" })], [false, false, false, true]);
  check("remove reply: the bridge's checks in words", [L.removeReply({ removed: true, code: "no_deposit" }, "receive"), L.removeReply({ removed: false, code: "deposited" }, "receive"),
    L.removeReply({ removed: false, code: "unreachable" }, "receive"), L.removeReply({ removed: false, code: "unreachable" }, "send_back").line],
    [{ removed: true, line: "No deposit was made for this code. Removed.", kind: "done" }, { removed: false, line: "A deposit was made for this code and isn't claimed yet, so it stays.", kind: "" },
     { removed: false, line: "Couldn't read Sepolia just now, so nothing was removed. Try again in a moment.", kind: "waiting" },
     "Couldn't read the bridge test network just now, so nothing was removed. Try again in a moment."]);
  check("remove reply: send-back words; the bridge's own text wins; gone already counts", [L.removeReply({ removed: true, code: "never_burned" }, "send_back").line, L.removeReply({ removed: false, code: "with_wallet" }, "send_back").line,
    L.removeReply({ removed: true, code: "released", text: "Released in tx 0xab. Removed." }, "send_back").line, L.removeReply({ removed: false, code: "not_found" }, "receive").removed, L.removeReply(null, "receive").kind],
    ["Nothing was burned for this send-back. Removed.", "The burn is waiting in your wallet. Reject it there first; this send-back stays.", "Released in tx 0xab. Removed.", true, "problem"]);
  check("remove: features name it", [L.bridgeNextCan(["receive_remove"], "receiveRemove"), L.bridgeNextCan(["send_back_remove"], "sendBackRemove"), L.bridgeNextCan(["address_book"], "addressBook"), L.bridgeNextCan(["send_back"], "addressBook"), L.bridgeNextCan(null, "receiveRemove")],
    [true, true, true, false, false]);

  // The words: "LNS names", never "official", never "look up" (WALLET-ROADMAP item 4).
  const words = [L.ADDRESS_BOX_HINT, L.ADDRESS_GET_CODE, L.ADDRESS_CHOOSE_TITLE, L.ADDRESS_WORDS_LINE, L.ADDRESS_TICK, L.ADDRESS_LAST_CHECK, L.ADDRESS_CONTINUE, L.ADDRESS_BURN_NEXT, L.ADDRESS_SAVE_TITLE, L.ADDRESS_SAVE_NOTE]
    .concat(["no_deposit", "claimed", "deposited", "never_burned", "released", "in_flight", "with_wallet", "unknown", "unreachable"].map((c) => L.removeReply({ code: c }, "receive").line));
  check("wallet-next words: no 'official', no 'look up'", words.filter((w) => /official|look(s|ed|ing)?[ -]?up/i.test(w)), []);

  // Main.qml: the address step and Remove as built.
  const qml = readFileSync(new URL("./qml/Main.qml", import.meta.url), "utf8");
  check("wallet-next: the bridge's new calls only through the adapter", (qml.match(/bridgeCall\("(address_book_list|address_book_save|address_book_remove|receive_remove|send_back_remove|send_back_check|send_back_plan)"/g) || []).length, 0);
  // 44 Timers in Main.qml at 0520887: none added (the box is checked once per change, never repeated).
  // +1 on names-nft: Collectibles' read, only while that page is open and in front, every 30 s (nft.test.mjs).
  check("wallet-next: no new timer", (qml.match(/^\s*Timer \{/gm) || []).length, 45);
  check("address step: the box is always shown, nothing pre-filled", [/Field \{ id: sbCodeField; Layout\.fillWidth: true; size: 15; code: true;/.test(qml), /Send to " \+ root\.sbSaved/.test(qml), /\(used before\)/.test(qml)], [true, false, false]);
  check("address step: the button is the tick's", /Action \{ label: root\.sbAsking \? "Getting it ready…" : Logic\.ADDRESS_CONTINUE; strong: true; enabledNow: sbAddressStep\.a\.canContinue; onPressed: root\.sbContinue\(\) \}/.test(qml), true);
  check("address step: the plan refuses an unticked or other value, and says so", /if \(!sbTicked \|\| sbCheck\.key !== Logic\.addressBoxKey\(sbCodeInput\)\) \{\s*\n\s*sbError = /.test(qml), true);
  check("address step: the box's hint follows what the bridge reads", [L.ADDRESS_BOX_HINT.indexOf("Ethereum address") > 0, L.ADDRESS_BOX_HINT_CODE.indexOf("address") < 0], [true, true]);
  check("remove: says Checking… at once", (qml.match(/text: busyHere \? "Checking…" : "Remove"/g) || []).length, 2);
  check("save: says Saving… at once", /label: root\.sbSaving \? "Saving…" : "Save"/.test(qml), true);
  check("wording: checked on LEZ 0.4", [/prefix: "LNS names and profiles: checked on"/.test(qml), /hint: "Check a name, such as jef"/.test(qml)], [true, true]);
}

{
  // The setup card names Basecamp's button as Basecamp labels it: "Unload" (Module Inspector),
  // never "Unload All" (2026-11-18).
  const L = await import("./qml/logic.mjs");
  const me = { name: "names_ui", installDir: "/x/plugins/names_ui" };
  const newMod = { name: "lez_bridge", hashes: { root: L.BUNDLED_BRIDGE[0].root } };
  const lines = [
    L.bridgeSetupState({ list: [me, newMod], bundle: "ok", answers: true, features: [], install: { phase: "unload" } }).line,
    L.bridgeSetupState({ list: [me, newMod], bundle: "ok", answers: false, stuck: true }).line,
    L.bridgeSetupState({ list: [me, newMod], bundle: "ok", answers: true, features: [] }).line,
  ];
  check("unload step: Basecamp's button is Unload", lines.map((line) => [/LEZ Bridge → Unload\. /.test(line), /Unload All/.test(line)]), [[true, false], [true, false], [true, false]]);
  const qml = readFileSync(new URL("./qml/Main.qml", import.meta.url), "utf8");
  check("unload: no 'Unload All' anywhere the app shows", /Unload All/.test(qml), false);
}

{
  // a. Receive from Ethereum and Bridge to Ethereum start on the SMALLEST amount; the last one is
  // never remembered (2026-11-18).
  const L = await import("./qml/logic.mjs");
  const eth = L.receiveTokens({ tokens: [{ symbol: "ETH", denoms: [{ index: 0, text: "1", value: "1000000000000000000" }, { index: 1, text: "0.1", value: "100000000000000000" }, { index: 2, text: "0.01", value: "10000000000000000" }] }] })[0].denoms;
  check("smallest denomination by value (registry order not sorted)", L.smallestDenom(eth), 2);
  check("smallest denomination by text (no values: the fallback list)", L.smallestDenom([{ index: 0, text: "1,000" }, { index: 1, text: "100" }, { index: 2, text: "10,000" }]), 1);
  check("smallest: empty list", [L.smallestDenom([]), L.smallestDenom(null)], [-1, -1]);
  check("receive: nothing pressed: the smallest; pressed: kept; pressed on another token's list: the smallest",
    [L.denomOrSmallest(eth, -1), L.denomOrSmallest(eth, 0), L.denomOrSmallest([{ index: 0, text: "100" }, { index: 1, text: "1,000" }], 2)], [2, 0, 0]);
  check("Bridge to Ethereum: the smallest amount the account holds", L.smallestDenom(L.sendBackAmounts(null, { symbol: "wETH", base: "1010000000000000000" })),
    L.sendBackAmounts(null, { symbol: "wETH", base: "1010000000000000000" }).sort((a, b) => (BigInt(a.value) < BigInt(b.value) ? -1 : 1))[0].index);
  const qml = readFileSync(new URL("./qml/Main.qml", import.meta.url), "utf8");
  check("receive: the screen starts with no amount pressed (so the smallest)", [/property int receiveDenom: -1/.test(qml), (qml.match(/receiveDenom = -1/g) || []).length >= 3, /String\(receiveDenom\)/.test(qml)], [true, true, false]);
  check("send back: no amount kept from before (sbPick and the screen pick the smallest)", [(qml.match(/sbDenom = Logic\.smallestDenom\(sbAmounts\)/g) || []).length, /sbDenom = sbAmounts\[0\]\.index/.test(qml)], [2, false]);

  // b. A claimed receive's line reads the balance on the row's OWN bridge: a v5 claim said "now
  // holds 0.01 wETH" from v6's row while the account held 1.01 on v5 (2026-11-18).
  const acct = "Public/3J29s1Wy62NYMRuTe6MWXQrpWkqUAkv9t6PqzHWF7CNA";
  const two = [{ account: acct, assets: [
    { symbol: "LEZ", name: "Native", amount: "5" },
    { symbol: "wETH", name: "Wrapped Sepolia ETH · bridge v6", bridgeLabel: "bridge v6", amount: "0.01" },
    { symbol: "wETH", name: "Wrapped Sepolia ETH · bridge v5", bridgeLabel: "bridge v5", amount: "1.01" }] }];
  check("balance line: the v5 row reads v5, not v6", L.receiveBalanceLine(two, acct, "ETH", { id: "bridge-test-network-v5", label: "bridge v5", current: true }), "3J29s1…7CNA now holds 1.01 wETH.");
  check("balance line: the v6 row reads v6", L.receiveBalanceLine(two, acct, "ETH", { id: "added-1", label: "sepolia-v6" }), "3J29s1…7CNA now holds 0.01 wETH.");
  check("balance line: a bridge added from its code (\"added-…\", no version): never another bridge's balance", L.receiveBalanceLine(two, acct, "ETH", { id: "added-95cea20e4a1b", label: "added-95cea20e4a1b" }), "The balance of 3J29s1…7CNA shows on the Wallet card once the network is read again.");
  check("balance line: a bridge the card has no row for: no other bridge's balance", L.receiveBalanceLine(two, acct, "ETH", { id: "bridge-test-network-v4", label: "bridge v4" }), "The balance of 3J29s1…7CNA shows on the Wallet card once the network is read again.");
  check("balance line: one unlabelled row (one bridge): it", L.receiveBalanceLine([{ account: acct, assets: [{ symbol: "wETH", name: "Wrapped Sepolia ETH", amount: "0.2" }] }], acct, "ETH", { id: "bridge-test-network-v5", label: "bridge v5" }), "3J29s1…7CNA now holds 0.2 wETH.");
  check("balance line: a label as before", L.receiveBalanceLine(two, acct, "ETH", "bridge v5"), "3J29s1…7CNA now holds 1.01 wETH.");
  check("bridge key", [L.receiveBridgeKey({ id: "bridge-test-network-v5", label: "bridge v5" }), L.receiveBridgeKey({ id: "bridge-test-network", label: "bridge v2" }), L.receiveBridgeKey(null), L.receiveBridgeKey("bridge v12")], ["v5", "v2", "", "v12"]);
  check("card rows keep their deployment", L.receiveRowsShown([{ id: "r1", symbol: "ETH", amount: "1", account: acct, deployment: { id: "bridge-test-network-v5", label: "bridge v5" } }], {}, {}, 0, true).rows[0].deployment, { id: "bridge-test-network-v5", label: "bridge v5" });
  check("every balance line on screen passes the row's deployment, the read's time and the claim's", (qml.match(/Logic\.receiveBalanceLine\(/g) || []).length, (qml.match(/Logic\.receiveBalanceLine\([^)]*\.deployment, root\.bridgeNetReadAt, root\.receiveClaimedAt\[[^\]]+\.id\]\)/g) || []).length);
  // names-stall-3 review (4): "now holds" only from a read made after the claim.
  const one = [{ account: acct, assets: [{ symbol: "wETH", name: "Wrapped Sepolia ETH", amount: "0" }] }];
  check("balance line: a read from before the claim says nothing of what it holds now", L.receiveBalanceLine(one, acct, "ETH", { id: "bridge-test-network-v5", label: "bridge v5" }, 1000, 1001000 + 1), "The balance of 3J29s1…7CNA shows on the Wallet card once the network is read again.");
  check("balance line: a read after the claim does", L.receiveBalanceLine(one, acct, "ETH", { id: "bridge-test-network-v5", label: "bridge v5" }, 1002, 1001000), "3J29s1…7CNA now holds 0 wETH.");
  check("balance line: no read yet, a claim seen: nothing said", L.receiveBalanceLine(one, acct, "ETH", null, 0, 5000), "The balance of 3J29s1…7CNA shows on the Wallet card once the network is read again.");
  check("bridge read before a recent claim: asked again", [L.bridgeReadBeforeClaims(1000, { r: 1001000 }, 1002000, 120000), L.bridgeReadBeforeClaims(1002, { r: 1001000 }, 1002000, 120000), L.bridgeReadBeforeClaims(1000, { r: 1001000 }, 1001000 + 200000, 120000), L.bridgeReadBeforeClaims(0, {}, 5, 120000)], [true, false, false, false]);
  check("the claim's branch asks the bridge network afresh", /loadBridgeNet\(first\)/.test(qml) && /fresh: true/.test(qml), true);
  // names-stall-3 review (5): Bridge to Ethereum submits the amount it shows.
  // Merge wallet-1119: the registry rows carry eth_token, as the real registry() does; names-erc20 finds a token by address.
  const regA = { tokens: [{ symbol: "ETH", eth_token: "0x0000000000000000000000000000000000000000", denoms: [{ index: 0, value: "10000000000000000" }, { index: 1, value: "100000000000000000" }] }] };
  const regB = { tokens: [{ symbol: "ETH", eth_token: "0x0000000000000000000000000000000000000000", denoms: [{ index: 0, value: "50000000000000000" }, { index: 1, value: "10000000000000000" }] }] };
  const weth = { symbol: "wETH", base: "1000000000000000000" };
  const shownA = L.sendBackAmounts(regA, weth).filter(function(d) { return d.index === L.smallestDenom(L.sendBackAmounts(regA, weth)) })[0];
  check("send back: the index sent is the shown amount's on the bridge as read now", [L.sendBackIndexFor(regA, weth, shownA.value), L.sendBackIndexFor(regB, weth, shownA.value), L.sendBackIndexFor(regB, weth, "70000000000000000"), L.sendBackIndexFor(null, weth, shownA.value)], [0, 1, -1, -1]);
  check("send back: shown == submitted for every amount offered on either bridge", [regA, regB].every(function(reg) { return L.sendBackAmounts(reg, weth).every(function(d) { return L.sendBackAmounts(reg, weth).filter(function(x) { return x.index === L.sendBackIndexFor(reg, weth, d.value) })[0].value === d.value }) }), true);
  // Merge wallet-1119: the fresh check finds an ERC-20 the way its amounts were found (by address), not by a fixed symbol.
  const usdtAddr = "0x" + "11".repeat(20);
  const regU = { tokens: [{ symbol: "USDT", eth_token: usdtAddr, decimals: 6, denoms: [{ index: 0, value: "1000000000" }, { index: 1, value: "100000000" }] }] };
  const usdt = { symbol: "USDT", base: "5000000000", ethToken: usdtAddr, definition: "UsdtDef" };
  const tabU = L.bridgeTokens(regU, null);
  check("send back: an ERC-20's index on the bridge as read now, and shown == submitted for it", [L.sendBackIndexFor(regU, usdt, "100000000", tabU), L.sendBackIndexFor(regU, usdt, "7", tabU),
    L.sendBackAmounts(regU, usdt, tabU).every(function(d) { return L.sendBackIndexFor(regU, usdt, d.value, tabU) === d.index })], [1, -1, true]);
  check("send back: sbPlan sends the fresh registry's index, never the held one", [/sendBackIndexFor\(fresh, sbAsset, shown\.value, bridgeTokenTable\)/.test(qml), /sendBackPlan, \[Logic\.sendBackSymbol\(sbAsset\.symbol\), String\(sbDenom\)/.test(qml)], [true, false]);
  check("send back: a bridge change drops the held amounts; a new send-back reads them again", [/function bridgeChosen\(row\) \{\s*\/\/[^\n]*\n\s*receiveRegistry = null/.test(qml), /if \(sbFocus === "" && receiveRegistry === null\)/.test(qml)], [true, false]);
  // names-stall-3 review (6): Change shows whenever there is anything else to choose.
  check("send back: Change with one amount but a private account too", [L.sendBackChangeShown([{}], [{}], [{}]), L.sendBackChangeShown([{}], [{}], []), L.sendBackChangeShown([{}, {}], [{}], []), L.sendBackChangeShown([], [{}, {}], [])], [true, false, true, true]);
  check("send back: the Change link uses it, and the amount isn't picked again after Change", [/Link \{ visible: Logic\.sendBackChangeShown\(root\.sbAmounts, root\.sbSources, root\.sbPrivateSources\)/.test(qml), /!sbChangePressed\)\s*\n\s*sbDenom = Logic\.smallestDenom\(sbAmounts\)/.test(qml)], [true, true]);

  // c. Bridge to Ethereum step 1: the private-account note only while no public account is chosen.
  const priv = [{ account: "Private/AAA" }];
  check("private note: nothing chosen / a private account chosen / a public account chosen / no private accounts",
    [L.sendBackPrivateNoteShown(priv, "", false), L.sendBackPrivateNoteShown(priv, "Private/AAA", true), L.sendBackPrivateNoteShown(priv, "Public/BBB", false), L.sendBackPrivateNoteShown([], "", false)],
    [true, true, false, false]);
  check("private note: the screen uses the rule", /visible: Logic\.sendBackPrivateNoteShown\(root\.sbPrivateSources, root\.sbAccount, root\.sbFromPrivate\); color: root\.ink; text: Logic\.PRIVATE_SEND_BACK_NOTE/.test(qml), true);
}

// Roadmap 12: moving wrapped tokens between accounts on the bridge test network.
{
  const L = await import("./qml/logic.mjs");
  const qml = readFileSync(new URL("./qml/Main.qml", import.meta.url), "utf8");
  check("move units: 1.2 wETH, 0.01, .5, the smallest unit", [L.unitsText("1.2", 18), L.unitsText("0.01", 18), L.unitsText(".5", 18), L.unitsText("0.000000000000000001", 18)],
    ["1200000000000000000", "10000000000000000", "500000000000000000", "1"]);
  check("move units: not amounts", [L.unitsText("", 18), L.unitsText("0", 18), L.unitsText(".", 18), L.unitsText("1,5", 18), L.unitsText("1e3", 18), L.unitsText("0.0000000000000000001", 18), L.unitsText("-1", 18)],
    [null, null, null, null, null, null, null]);
  check("move units: past what a JS number holds, exactly", L.unitsText("123456789.123456789123456789", 18), "123456789123456789123456789");
  check("move amount problems", [L.moveAmountProblem("", 18, "1", "wETH"), L.moveAmountProblem("0.05", 18, "100000000000000000", "wETH"), L.moveAmountProblem("0.1", 18, "100000000000000000", "wETH"),
    L.moveAmountProblem("0.2", 18, "100000000000000000", "wETH"), L.moveAmountProblem("0", 18, "1", "wETH"), L.moveAmountProblem("-1", 18, "1", "wETH"),
    L.moveAmountProblem("1,5", 18, "1", "wETH"), L.moveAmountProblem("0.0000000000000000001", 18, "1", "wETH")],
    ["", "", "", "That's more than this account holds: 0.1 wETH.", "Enter an amount above 0.", "-1 is negative. Enter an amount above 0.",
     "1,5 isn't an amount. Use digits and one point, such as 0.01.", "0.0000000000000000001 has more than 18 digits after the point, the smallest unit of wETH."]);
  const own = [{ account: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk" }, "Public/7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR", "Private/yGrP", "Public/9vXeAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"];
  const targets = L.moveTargets(own, "Public/9vXeAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", ["Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk"]);
  check("move targets: own public accounts but the one they leave, phrase ones first, imported marked",
    targets.map(function(t) { return [t.account.substr(7, 4), t.imported] }), [["7wx3", false], ["CCVd", true]]);
  check("imported accounts from backup_status", L.importedAccounts({ networks: [{ id: "lez-0.3", accounts: [{ account: "Public/A", imported: true }] }, { id: "lez-0.4", accounts: [{ account: "Public/B", imported: true }, { account: "Public/C", imported: false }] }] }, "lez-0.4"), ["Public/B"]);
  check("move to problems", [L.moveToProblem("", "Public/A"), L.moveToProblem("Private/X", "Public/A"), L.moveToProblem("Public/A", "Public/A"), L.moveToProblem("A", "Public/A"), L.moveToProblem("Public/nope!", "Public/B"), L.moveToProblem("alice", "Public/B")],
    ["", "Moving wrapped tokens to a private account isn't in the wallet yet: choose a public account.", "That's the account the tokens leave from. Choose another one.", "That's the account the tokens leave from. Choose another one.", "That isn't an account ID: it's Public/ and then letters and digits.", ""]);
  const plan = { amount_shown: "0.05", symbol: "wETH", bridge_label: "bridge v5", fee_payer: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", to: "Public/7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR", to_name: null };
  check("move fee line: checking, priced, held", [L.moveFeeLine(plan, { state: "checking" }, "5"), L.moveFeeLine(plan, { state: "ready", fee: { about: "2140", at_most: "3000" } }, "23786858")],
    ["Paid by CCVd76…pKnk in test LEZ. Checking how much…", "About 2,140 test LEZ, at most 3,000, paid by CCVd76…pKnk (it holds 23,786,858). The bridge's relay pays only for claims and burns."]);
  check("move result: done in a block / failed in words", [L.moveResultLine(plan, { state: "done", block: 28512 }), L.moveResultLine(plan, { state: "failed", problem: "Nothing was sent." }), L.moveResultLine(plan, null)],
    [{ kind: "done", text: "Moved 0.05 wETH (bridge v5) to 7wx3U7…LznR, in block 28512." }, { kind: "problem", text: "Nothing was sent." }, { kind: "", text: "" }]);
  // Every press gives feedback at once (the button's own words), and the screen ends in one plain result.
  check("move: Review says Checking the fee… at once", /label: moveScreen\.checking \? "Checking the fee…" : "Review"/.test(qml), true);
  check("move: Approve says Moving… at once", /label: moveScreen\.sending \? "Moving…"/.test(qml), true);
  check("move: the result line is the shared status line", /StatusLine \{ visible: moveScreen\.result\.text !== ""; kind: moveScreen\.result\.kind; text: moveScreen\.result\.text \}/.test(qml), true);
  check("move: offered only with the module's feature", /readonly property bool moveCan: namesNet === "0\.4" && moduleFeatures\.indexOf\("bridge_token_send"\) >= 0/.test(qml), true);
  check("move: the old 'isn't in the wallet yet' line only without the feature", /text: root\.moveCan \? "Move beside a wrapped token/.test(qml), true);
}

// Review 2026-11-19 (1, 2, 3): no second send from a late answer; a name's account pinned at the check; a sent move never said failed plainly.
{
  const L = await import("./qml/logic.mjs");
  const qml = readFileSync(new URL("./qml/Main.qml", import.meta.url), "utf8");
  const busy = "The wallet is busy; still working.", none = "No answer from the wallet yet: it may still be working on it. Look before trying again.";
  check("move start failure: busy is a safe retry, no answer may have been taken, anything else a refusal",
    [L.moveStartFailure(busy, busy, none), L.moveStartFailure(none, busy, none), L.moveStartFailure("That account has never been used", busy, none)], ["busy", "unclear", "refused"]);
  check("move: an unclear start drops the review (no Approve again) and looks for the job", /if \(how === "unclear"\) \{[\s\S]{0,200}moveTurn\+\+\n\s*moveCheck = null[\s\S]{0,300}moveLookForSend\(plan, moveTurn\)/.test(qml), true);
  check("move: the look follows a running bridge_send job, else reloads the balances", /busy\.kind === "bridge_send" && busy\.id[\s\S]{0,300}moveFollowSoon\(\)[\s\S]{0,300}loadBridgeNet\(\)/.test(qml), true);
  const plan = { amount_shown: "0.5", symbol: "wETH", bridge_label: "bridge v5", to: "alice", to_name: "alice" };
  const acct = "Public/7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR";
  const check0 = { args: ["B", "D", "0.5", "Public/F", "alice"], plan: plan, to_account: "", forwarded_to: "" };
  check("move: a name with no resolved account isn't sent", L.moveSendArgs(check0), null);
  check("move: sent to the account the check resolved, never the name", L.moveSendArgs(Object.assign({}, check0, { to_account: acct })), ["B", "D", "0.5", "Public/F", acct]);
  check("move: an account recipient goes as it was", L.moveSendArgs({ args: ["B", "D", "0.5", "Public/F", acct], plan: { to: acct } }), ["B", "D", "0.5", "Public/F", acct]);
  check("move: the sheet shows the account a name pays, and its forward", [L.moveToLine(plan, check0), L.moveToLine(plan, Object.assign({}, check0, { to_account: acct })), L.moveToLine(plan, Object.assign({}, check0, { to_account: acct, forwarded_to: "bob" }))],
    ["alice's public account (checking which…)", "alice's public account 7wx3U7…LznR", "alice's public account 7wx3U7…LznR: alice forwards to bob, so this pays bob"]);
  check("move: the result names the account paid", L.moveResultLine(Object.assign({}, plan, { paid: acct }), { state: "done", block: 9 }).text, "Moved 0.5 wETH (bridge v5) to alice's public account 7wx3U7…LznR, in block 9.");
  check("move: a sent but unconfirmed move is a wait, with its hash", L.moveResultLine(plan, { state: "unclear", problem: "The move was sent but isn't confirmed yet.", tx: "ab12" }), { kind: "waiting", text: "The move was sent but isn't confirmed yet. Its transaction: ab12." });
  check("move: Approve needs the resolved account", /enabledNow: root\.moveCheck !== null && root\.moveCheck\.state === "ready" && !moveScreen\.sending && Logic\.moveSendArgs\(root\.moveCheck\) !== null/.test(qml), true);
}

// Roadmap 9: the signed withdraw-to code's line on step 2.
{
  const L = await import("./qml/logic.mjs");
  const qml = readFileSync(new URL("./qml/Main.qml", import.meta.url), "utf8");
  check("signature line: signed / not signed / an older bridge module / nothing yet",
    [L.signatureLine({ signed: true }), L.signatureLine({ signed: false }), L.signatureLine({ words_text: "a · b · c · d" }), L.signatureLine(null)],
    [{ kind: "done", text: "Signed by this address on the bridge page. Still compare the four words." }, { kind: "", text: "Not signed: compare the four words." }, { kind: "", text: "" }, { kind: "", text: "" }]);
  check("signature line: the module's own text is never shown (fixed words)", L.signatureLine({ signed: true, signature_line: "<b>x</b>" }).text, L.SIGNED_LINE);
  const signed = "logoswd1qgqqqqqqqz4rdfa0hhceys6wnzytkg76umedzhh2vfrw6yrnluqczgjkarl95rrs7jk58am87gggjfsqht535d6mq4tkkf5zs4z4c03fwcyc7gyk9al6rl2784y0kuqyjsjx5e85ddvtkw89vjrfret5skh5wpew2mgaxaqrrdxnurahgfu6sjsm4xcna0";
  check("a signed code (198 characters) is taken whole from the box, a link or chat text", [L.sendBackCode(signed), L.sendBackCode("https://nameswallet.app/bridge/#" + signed.substr(0, 90) + "\n" + signed.substr(90)), L.addressBoxKey(" " + signed.toUpperCase() + " ")], [signed, signed, signed]);
  check("signature line: under the four words, as a status line", /StatusLine \{ readonly property var sig: Logic\.signatureLine\(root\.sbCheck\)/.test(qml), true);
}

// The UI issues of 2026-11-18 (item C).
{
  const L = await import("./qml/logic.mjs");
  const qml = readFileSync(new URL("./qml/Main.qml", import.meta.url), "utf8");
  const notice = "This account was imported. Its bridge secrets come from this wallet's recovery phrase, so the phrase can finish a withdrawal; the account's own key is still needed to move what stays in it.";
  check("imported notice: in the card, plain, once", [L.importedNoticeLine({ account_notice: notice }), L.importedNoticeLine({ account_notice: "a\u202eb\n c" }), L.importedNoticeLine({}), L.importedNoticeLine(null)], [notice, "a b c", "", ""]);
  check("imported notice: shown in the card, not only under Details", /StatusLine \{ visible: text !== ""; kind: ""; text: Logic\.importedNoticeLine\(receiveScreen\.result\) \}/.test(qml) && !/receiveScreen\.result\.account_notice\) : ""/.test(qml), true);
  check("deposit secret: said once and truly", [L.receiveSecretLine({ account_notice: notice }).indexOf("recovery phrase") > 0 && L.receiveSecretLine({ account_notice: notice }).indexOf("own key") < 0, L.receiveSecretLine({}).indexOf("own key") > 0 && L.receiveSecretLine({}).indexOf("recovery phrase") < 0], [true, true]);
  check("the accounts are read again when Bridge to Ethereum or Move opens", /function openSendBack\(account, asset, focusId\) \{\n        walletView = "sendback"\n[^\n]*\n        loadAccounts\(\)/.test(qml) && /loadBackupStatus\(\)\n        loadAccounts\(\)\n        walletView = "bridgemove"/.test(qml), true);
  check("Bridge to Ethereum step 1: another bridge's balance said beside private accounts too", /visible: \(root\.sbSources\.length > 0 \|\| root\.sbPrivateSources\.length > 0\) && root\.sbElsewhere !== ""/.test(qml), true);
}

// E: the LEZ Bridge module's text on the bridge and claim screens is cleaned and drawn as plain text
// (LP0001 job B report: Hints default to AutoText, so markup in a remote message could render).
{
  const L = await import("./qml/logic.mjs");
  const qml = readFileSync(new URL("./qml/Main.qml", import.meta.url), "utf8");
  // names-safety-b (review finding 3): the cleaner drops < > & too; the hints are plain text as well.
  check("bridge text: hidden characters and markup characters gone", L.clean("<b>bad</b>\u202e\u200b ok"), "bbad/b ok");
  const hints = qml.split("\n").filter((l) => /Hint \{/.test(l) && /\.message|\.error\b|\.note\b|\.notice|\.reason|claimLine/.test(l));
  check("bridge text: every hint showing a module's message, error, note, notice or reason is plain text", hints.filter((l) => !/textFormat: Text\.PlainText/.test(l)).length, 0);
  check("bridge text: no raw String(…message) left in a hint", hints.filter((l) => /String\([^)]*\.(message|note|notice|reason) \|\| ""\)/.test(l)).length, 0);
  check("bridge text: the send-back check and plan errors are cleaned", /sbCheckError = e\.message \? sentence\(Logic\.clean\(e\.message\)\)/.test(qml) && /sbError = e\.message \? sentence\(Logic\.clean\(e\.message\)\)/.test(qml), true);
  check("bridge text: the claim's phases are plain and cleaned", /textFormat: Text\.PlainText\n[^\n]*\n[^\n]*\n[^\n]*Logic\.clean\(modelData\.hint\)/.test(qml), true);
}

// F: another app's burn shows a CHECKED Ethereum address when the names module could open its exit.
{
  const L = await import("./qml/logic.mjs");
  const checked = { burn_opening: { v: 1, checked: true, recipient: "0xb0275f9e94b2a437c834bf7dea5699f0b8707ec2", recipient_halves: ["0xb0275f9e94b2a437c834", "bf7dea5699f0b8707ec2"], words_text: "a · b · c · d" } };
  check("burn address: checked", L.burnAddressLine(checked), "Ethereum address: 0xb0275f9e94b2a437c834bf7dea5699f0b8707ec2, checked by this wallet against the burn's exit commitment, with the token and amount burned. Its four words: a · b · c · d. Compare them on the bridge page with that Ethereum wallet connected.");
  check("burn address: the relay fee is shown (review 2026-11-19)", [L.burnAddressLine({ burn_opening: Object.assign({}, checked.burn_opening, { max_relay_fee: "2000000000000000", max_relay_fee_text: "0.002 wETH", at_least_text: "0.998 wETH" }) }).endsWith(" A relay may keep up to 0.002 wETH; the address gets at least 0.998 wETH (less the vault's own fee)."),
    L.burnAddressLine({ burn_opening: Object.assign({}, checked.burn_opening, { max_relay_fee: "0", max_relay_fee_text: "0 wETH", at_least_text: "0.01 wETH" }) }).endsWith(" No relay fee: the address gets the whole 0.01 wETH (less the vault's own fee).")], [true, true]);
  check("burn address: halves only when checked", [L.burnAddressHalves(checked), L.burnAddressHalves({ burn_opening: { checked: false } }), L.burnAddressHalves({})], [["0xb0275f9e94b2a437c834", "bf7dea5699f0b8707ec2"], [], []]);
  check("burn address: unchecked says so, the module's why cleaned", L.burnAddressLine({ burn_opening: { v: 1, checked: false, why: "No opening.\u202e" } }), "Ethereum address: sealed in the burn's exit commitment; the wallet can't check it here. No opening.");
  check("burn address: an older request keeps the sheet's own line", [L.burnAddressLine({}), L.burnAddressLine(null)], ["", ""]);
  // Merge wallet-1119 (docs/wallet/burn-opening-sheet.patch on names-safety-b's burnDestination):
  // the sheet's last line is the checked one when there is one, else safety-b's two sealed lines.
  const burnReq = (extra) => Object.assign({ kind: "relayed_call", app: "other_app", account: "Public/5DNyon1w3jf4Ai2ANzLz1bYnrkeFwuoNRmTL48FTGGb7",
    sheet: { refused: false, warnings: ["w"], burn: { bridge: "B", bridge_label: "bridge v7", amount_text: "0.01", symbol: "wETH", exit: "0xe2ba7765d0fbf247044cb907a4c921c03ca316c6cfcb6323a6b5e2545ff3dfd5" } } }, extra);
  check("burn sheet: a checked opening is the address line, with or without the bridge's record above",
    [L.burnDestination(burnReq(checked))[3], L.burnDestination(burnReq(checked), true)[3]], [L.burnAddressLine(checked), L.burnAddressLine(checked)]);
  check("burn sheet: no opening keeps safety-b's sealed lines",
    [L.burnDestination(burnReq({}))[3].endsWith("so the wallet can't show or check it."), L.burnDestination(burnReq({}), true)[3].endsWith("the LEZ Bridge's record of it, not checked by this wallet.")], [true, true]);
  check("burn sheet: an unchecked opening says the module's why", L.burnDestination(burnReq({ burn_opening: { v: 1, checked: false, why: "No grant." } }))[3], "Ethereum address: sealed in the burn's exit commitment; the wallet can't check it here. No grant.");
}

// D: Import account.
{
  const L = await import("./qml/logic.mjs");
  const qml = readFileSync(new URL("./qml/Main.qml", import.meta.url), "utf8");
  check("import: the result line", [L.importResultLine({ account: "Public/7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR", label: "Shop", notice: "Imported. Keep its key." }),
    L.importResultLine({ account: "Public/7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR", notice: "" }), L.importResultLine(null)],
    ["Imported Shop (7wx3U7…LznR). Imported. Keep its key.", "Imported public account 7wx3U7…LznR. ", "Imported."]);
  check("import: the key field shows dots and is emptied as the import starts", /Field \{ id: importKeyField;[^}]*secret: true/.test(qml) && /try \{ importKeyField\.text = "" \} catch \(e\) \{\}\n        accountChange\("cli_account_import"/.test(qml), true);
  check("import: nothing pasted is logged by the app", /console\.log\([^)]*(importKeyField|asked|textOrFile)/.test(qml), false);
  check("import: the press says Importing… at once", /label: root\.accountsBusy \? "Importing…" : "Import it"/.test(qml), true);
  check("import: the warning and the recovery-phrase line are fixed words", [L.IMPORT_WARNING.indexOf("Anyone who sees it") > 0, L.IMPORT_COVER_LINE.indexOf("recovery phrase won't bring") > 0], [true, true]);
}

{
  const { SEND_BACK_FINISH_YOURSELF } = await import("./qml/logic.mjs");
  // Finishing a send-back needs no service of ours; sending it yourself links your address (rfp-022 release relayer).
  check("finish yourself: no service needed, anyone may send", [/without the bridge's service/.test(SEND_BACK_FINISH_YOURSELF), /anyone can send the release to the vault/.test(SEND_BACK_FINISH_YOURSELF)], [true, true]);
  check("finish yourself: the cost in words", /links that address to this send-back; a relayer avoids that\.$/.test(SEND_BACK_FINISH_YOURSELF), true);
  check("finish yourself: plain words", /official|lookup/i.test(SEND_BACK_FINISH_YOURSELF), false);
  const qml = readFileSync(new URL("./qml/Main.qml", import.meta.url), "utf8");
  check("finish yourself: under the release steps and beside the consent", [/SEND_BACK_RELEASE_STEPS\[1\] }\n\s*Hint {[^\n]*SEND_BACK_FINISH_YOURSELF/.test(qml), /SETTLE_CONSENT }\n\s*Hint {[^\n]*SEND_BACK_FINISH_YOURSELF/.test(qml)], [true, true]);
  // Merge wallet-1119: the relay's label is the bridge module's text, so it goes through Logic.clean (names-safety-b).
  check("relay label: cleaned where it is drawn", [/Logic\.clean\(RelayLine\.relayUsedLine\(/.test(qml), /value: root\.sbFocusStatus !== null \? Logic\.clean\(RelayLine\.relayUsed\(root\.sbFocusStatus\.relay\)\)/.test(qml)], [true, true]);
}


// ERC-20 as a first-class path (rfp-022 docs/ERC20-PATH.md): one token table per bridge, so any
// token the bridge registers (USDC, USDT, DAI, WETH …) is offered on Receive, Bridge to Ethereum and
// the card, not only wETH and dUSDC.
{
  const L = await import("./qml/logic.mjs");
  const ETH = "0x0000000000000000000000000000000000000000", USDT = "0x" + "11".repeat(20), DAI = "0x" + "da".repeat(20), DUSDC = "0xac207b9bfebf3ef7c26297ea6abadeeccc4aa71e";
  const D = (n, k) => String(n) + "0".repeat(k);
  // The bridge code's list (config().deployment.tokens), as a bridge added in Settings carries it.
  const dep = [
    { eth_token: ETH, symbol: "ETH", decimals: 18, definition: "DefEth", denoms: [D(1, 16), D(1, 17), D(1, 18)] },
    { eth_token: USDT, symbol: "USDT", decimals: 6, definition: "DefUsdt", denoms: [D(1, 8), D(1, 9), D(1, 10)] },
    { eth_token: DAI, symbol: "DAI", decimals: 18, definition: "DefDai", denoms: [D(1, 20), D(1, 21), D(1, 22)] },
  ];
  // registry() rows (both chains read): denominations with value and text, the vault's freeze.
  const reg = { tokens: [
    { symbol: "ETH", eth_token: ETH, decimals: 18, definition: "DefEth", denoms: [{ index: 0, value: D(1, 16), text: "0.01" }], freeze: { eth_deposits: false } },
    { symbol: "USDT", eth_token: USDT, decimals: 6, definition: "DefUsdt", denoms: [{ index: 0, value: D(1, 8), text: "100" }, { index: 1, value: D(1, 9), text: "1,000" }], freeze: { eth_deposits: true } },
  ] };
  check("wrapped symbol: ETH arrives as wETH, an ERC-20 keeps its symbol", [L.wrappedSymbol(ETH, "ETH"), L.wrappedSymbol(DUSDC, "dUSDC"), L.wrappedSymbol(USDT, "USDT")], ["wETH", "dUSDC", "USDT"]);
  const fromDep = L.bridgeTokens(null, dep);
  check("token table from the bridge code: symbol, card, decimals, amounts in their own decimals",
    fromDep.map((t) => [t.symbol, t.card, t.decimals, t.denoms.map((d) => d.text).join(" / ")]),
    [["ETH", "wETH", 18, "0.01 / 0.1 / 1"], ["USDT", "USDT", 6, "100 / 1,000 / 10,000"], ["DAI", "DAI", 18, "100 / 1,000 / 10,000"]]);
  const fromReg = L.bridgeTokens(reg, dep);
  check("token table from the registry wins over the code, with its freeze; a token the registry has no row for keeps the code's amounts",
    fromReg.map((t) => [t.symbol, t.frozen, t.denoms.length]), [["ETH", false, 1], ["USDT", true, 2], ["DAI", false, 3]]);
  check("token table without either: the fixed ETH and dUSDC", L.bridgeTokens(null, null).map((t) => [t.symbol, t.card, t.ethToken === ETH || t.ethToken === DUSDC]), [["ETH", "wETH", true], ["dUSDC", "dUSDC", true]]);
  // Receive from Ethereum.
  check("receive tokens: the bridge code's list when the registry can't be read", L.receiveTokens(null, dep).map((t) => t.symbol + ":" + t.denoms.map((d) => d.text).join("/")),
    ["ETH:0.01/0.1/1", "USDT:100/1,000/10,000", "DAI:100/1,000/10,000"]);
  check("receive tokens: the old fixed list without either", L.receiveTokens(null, null), L.RECEIVE_FALLBACK);
  const recv = L.receiveTokens(reg);
  check("receive token: the pressed one, a paused one falls back to ETH", [L.receiveTokenNow(recv, ETH), L.receiveTokenNow(recv, USDT), L.receiveTokenNow(L.receiveTokens(null, dep), DAI)], [ETH, ETH, DAI]);
  check("receive token: a bridge without ETH starts on its first token; none: empty",
    [L.receiveTokenNow(L.receiveTokens(null, dep.slice(1)), ETH), L.receiveTokenNow([], ETH), L.receiveTokenNow([{ symbol: "USDT", ethToken: USDT, frozen: true, denoms: [] }], USDT)], [USDT, "", ""]);
  check("receive: paused tokens are not offered and one line names them", [L.receiveTokensOpen(recv).map((t) => t.symbol), L.receivePausedLine(recv), L.receivePausedLine(L.receiveTokens(null, dep))],
    [["ETH"], "Deposits of USDT are paused on this bridge for now.", ""]);
  check("receive: the smallest amount of an ERC-20 is pre-selected", L.denomOrSmallest(L.receiveTokens(null, dep)[2].denoms, -1), 0);
  check("receive: an ERC-20's deposit step says approve first; ETH's is unchanged",
    [L.receiveDepositSteps("USDT", fromDep)[2], L.receiveDepositSteps("ETH", fromDep), L.receiveDepositSteps("dUSDC", null)[2]],
    ["Approve exactly the amount there, then deposit (your Ethereum wallet asks twice). Then come back here and press Check now.", L.RECEIVE_DEPOSIT_STEPS,
     "Approve exactly the amount there, then deposit (your Ethereum wallet asks twice). Then come back here and press Check now."]);
  // Bridge to Ethereum.
  const chosen = { program: "BrNew", release: "proof", tokens: fromDep };
  const usdtRow = (base) => ({ symbol: "USDT", bridge: "BrNew", base: base, decimals: 6, definition: "DefUsdt", ethToken: USDT });
  check("send back: a USDT balance on the chosen bridge is offered from its smallest amount",
    [L.sendBackOffer(usdtRow(D(1, 8)), chosen), L.sendBackOffer(usdtRow("99999999"), chosen), L.sendBackOffer(usdtRow(D(1, 8)), { program: "Other", release: "proof", tokens: fromDep })], [true, false, false]);
  check("send back: a DAI balance (18 decimals) by its definition", L.sendBackOffer({ symbol: "DAI", bridge: "BrNew", base: D(15, 19), definition: "DefDai" }, chosen), true);
  check("send back: a token the chosen bridge doesn't list is not offered", L.sendBackOffer({ symbol: "XYZ", bridge: "BrNew", base: D(1, 30), definition: "DefX" }, chosen), false);
  check("send back: amounts of USDT in its own decimals, only those held", L.sendBackAmounts(null, usdtRow("1500000000"), fromDep).map((d) => d.text + "@" + d.index), ["100@0", "1,000@1"]);
  check("send back: the registry's amounts for that token, matched by address", L.sendBackAmounts(reg, usdtRow(D(1, 10)), fromDep).map((d) => d.text), ["100", "1,000"]);
  check("send back: wETH and dUSDC unchanged without a table", [L.sendBackOffer({ symbol: "dUSDC", bridge: L.BRIDGE_V4_PROGRAM, base: "100000000" }), L.sendBackAmounts(null, { symbol: "wETH", base: D(1, 17) }).map((d) => d.text)],
    [true, ["0.01", "0.1"]]);
  check("send back: the bridge's symbol for a card symbol", [L.sendBackSymbol("wETH", fromDep), L.sendBackSymbol("USDT", fromDep), L.sendBackSymbol("DAI", fromDep), L.sendBackSymbol("wETH")], ["ETH", "USDT", "DAI", "ETH"]);
  const PRIV2 = "Private/yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z";
  const accts2 = [{ account: PRIV2, kind: "private" }];
  const held2 = [{ token_program: "T", holdings: [
    { account: PRIV2, symbol: "wETH", balance: D(1, 17), decimals: 18, definition: "DefEth" },
    { account: PRIV2, symbol: "USDT", balance: D(25, 7), decimals: 6, definition: "DefUsdt" },
    { account: PRIV2, symbol: "DAI", balance: "0", decimals: 18, definition: "DefDai" }] }];
  check("private send back: a row per token held, in its own decimals", L.privateSendBackSources(accts2, held2, fromDep).map((r) => r.symbol + " " + r.amount), ["wETH 0.1", "USDT 250"]);
  check("private send back: nothing listed: a row per token, every amount", L.privateSendBackSources(accts2, [], fromDep).map((r) => [r.symbol, r.asset.anyAmount]), [["wETH", true], ["USDT", true], ["DAI", true]]);
  check("private send back: every USDT amount when the balance is unknown", L.sendBackAmounts(null, L.privateSendBackSources(accts2, [], fromDep)[1].asset, fromDep).map((d) => d.text), ["100", "1,000", "10,000"]);
  // The card: a token the Names module names by its address gets the table's name.
  const read2 = { accounts: [{ account: "Public/" + "C".repeat(43), found: true, native: "5", tokens: [
    { symbol: USDT, eth_token: USDT, definition: "DefUsdt", amount: "100000000", decimals: 6, bridge: "BrNew" },
    { symbol: "0x" + "ab".repeat(20), eth_token: "0x" + "ab".repeat(20), definition: "DefOther", amount: "7", decimals: 0, bridge: "BrNew" },
    { symbol: "wETH", eth_token: ETH, definition: "DefEth", amount: D(1, 16), decimals: 18, bridge: "BrNew" }] }] };
  check("card: balances per token, an unknown token named from the table or shortened",
    L.bridgeNetRows(read2, null, fromDep)[0].assets.map((a) => a.symbol + " " + a.amount), ["LEZ 5", "USDT 100", "0xabab…abab 7", "wETH 0.01"]);
  check("card: the row keeps the Ethereum address for Send back", L.bridgeNetRows(read2, null, fromDep)[0].assets[1].ethToken, USDT);
  // The screen uses the table (Main.qml can't run here; its bindings are checked as text).
  const qml2 = readFileSync(new URL("./qml/Main.qml", import.meta.url), "utf8");
  check("screen: Receive asks for the token it shows (receiveTokenNow), never a fixed ETH",
    [/var token = Logic\.receiveTokenArg\(receiveTokenList, receiveTokenNow\)\n\s*var args = priv \? \[token, /.test(qml2), /Logic\.receiveTokenNow\(receiveTokenList, receiveToken\)/.test(qml2), /model: Logic\.receiveTokensOpen\(root\.receiveTokenList\)/.test(qml2)], [true, true, true]);
  check("screen: Bridge to Ethereum and the card read the chosen bridge's table",
    [/tokens: bridgeTokenTable \} : null/.test(qml2), /Logic\.sendBackToken\(sbAsset, bridgeTokenTable\), String\((sbDenom|index)\)/.test(qml2), /Logic\.sendBackAmounts\(receiveRegistry, sbAsset, bridgeTokenTable\)/.test(qml2),
     /Logic\.privateSendBackView\(cliAccounts, bridgedHoldings, bridgeTokenTable, /.test(qml2), /Logic\.bridgeNetRows\([^\n]*, bridgeTokenTable\)/.test(qml2)], [true, true, true, true, true]);
  check("screen: the deposit steps come from receiveDepositSteps", (qml2.match(/Logic\.receiveDepositSteps\(/g) || []).length, 2);
  const rows2 = L.privateSendBackSources(accts2, [], fromDep);
  check("private chips name their token, and only one is chosen at a time", [L.privateSourceLabel(rows2[1]), L.privateSourceLabel(L.privateSendBackSources(accts2, held2, fromDep)[0]),
    /label: Logic\.privateSourceLabel\(modelData\)\n\s*chosen: Logic\.sameSource\(root\.sbAccount, root\.sbAsset, modelData\)/.test(qml2)],
    ["yGrPEb…fe4Z · USDT · private", "yGrPEb…fe4Z · 0.1 wETH · private", true]);
  // wallet-1120, the M2 install of 2026-11-19: Bridge to Ethereum offered 0.01 wETH of a private
  // account whose 0.01 wETH an earlier send-back had burned (card: "Recorded on Ethereum · finish it").
  const sbList = [{ id: "sb-9", account: PRIV2, symbol: "ETH", amount: "0.01 ETH", stage: "recorded" }];
  const burns = L.sendBackBurns(sbList, {});
  check("burns: a recorded send-back is a landed burn of its account and token", burns, { [PRIV2 + "|ETH"]: "landed" });
  check("burns: the latest status wins; the symbol from the amount when the row has none",
    [L.sendBackBurns([{ id: "a", account: PRIV2, amount: "0.01 ETH", stage: "planned" }], { a: { stage: "burning" } }), L.sendBackBurns([{ id: "a", account: PRIV2, symbol: "ETH", stage: "planned" }], {}),
     L.sendBackBurns([{ id: "a", account: PRIV2, symbol: "ETH", stage: "released" }, { id: "b", account: PRIV2, symbol: "ETH", stage: "approve" }], {})],
    [{ [PRIV2 + "|ETH"]: "pending" }, {}, { [PRIV2 + "|ETH"]: "pending" }]);
  // 1. As on the M2: no token listing accepted yet, so the wallet can't read the account: its
  // wETH isn't offered at every amount any more (the burn landed); the tokens it never burned are.
  const m2 = L.privateSendBackView(accts2, [], fromDep, burns);
  check("M2: a landed burn and an unreadable balance: that token is held back, said, the others offered",
    [m2.rows.map((r) => r.symbol), m2.held, L.privateHeldUnread(m2.held), L.privateSendBackSources(accts2, [], fromDep, burns).length],
    [["USDT", "DAI"], [{ account: PRIV2, symbol: "wETH", why: "unread" }], true, 2]);
  check("M2: the line names the account and token", L.privateHeldLine(m2.held), "yGrPEb…fe4Z · wETH: a send-back already burned from it, and Names can't read what it holds now. Accept the LEZ Bridge's token listing to see it.");
  // 2. The listing accepted, its first read not made yet (the module's read was dropped when the
  // burn's private call ended): nothing offered yet, and the screen asks again.
  const listing = (read, rows) => [{ token_program: "T", app: "lez_bridge", definitions: ["DefEth", "DefUsdt", "DefDai"], read: read, updating: !read, error: null, holdings: rows }];
  const reading = L.privateSendBackView(accts2, listing(false, []), fromDep, burns);
  check("a read not made yet is not \"holds none\": nothing offered, reading", [reading.rows.length, reading.reading, reading.held.map((h) => h.why)], [0, true, ["reading", "reading", "reading"]]);
  // 3. Read: it holds no wETH (burned) and nothing else: no chip at all, no every-amount fallback.
  const none = L.privateSendBackView(accts2, listing(true, [{ account: PRIV2, definition: "DefEth", symbol: "wETH", balance: "0", decimals: 18 }]), fromDep, burns);
  check("read as holding none: nothing offered", [none.rows.length, none.reading, none.held.length], [0, false, 0]);
  // 4. Read after the burn and it still holds some: offered, with what it holds.
  const more = L.privateSendBackView(accts2, listing(true, [{ account: PRIV2, definition: "DefEth", symbol: "wETH", balance: D(2, 16), decimals: 18 }]), fromDep, burns);
  check("read after a landed burn: what it holds now is offered", more.rows.map((r) => L.privateSourceLabel(r)), ["yGrPEb…fe4Z · 0.02 wETH · private"]);
  check("read after a landed burn: the amounts are capped by it", L.sendBackAmounts(null, more.rows[0].asset, fromDep).map((d) => d.text), ["0.01"]);
  // 5. A burn on its way: not offered even when the read still shows it (it is spent at the block).
  const onWay = L.sendBackBurns([{ id: "sb-10", account: PRIV2, symbol: "ETH", stage: "burning" }], {});
  const pending = L.privateSendBackView(accts2, listing(true, [{ account: PRIV2, definition: "DefEth", symbol: "wETH", balance: D(1, 16), decimals: 18 }]), fromDep, onWay);
  check("a burn on its way holds the token back", [pending.rows.length, pending.held, L.privateHeldLine(pending.held)],
    [0, [{ account: PRIV2, symbol: "wETH", why: "burning" }], "yGrPEb…fe4Z · wETH: a send-back's burn from it is on its way; it's offered again once that lands."]);
  // Without a bridge token table (an older bridge): the same two rules.
  const oldHeld = [{ token_program: "T", holdings: [{ account: PRIV2, account_kind: "private", symbol: "wETH", balance: D(1, 17), decimals: 18 }] }];
  check("no table: a landed burn holds back the every-amount row, a pending one the holding",
    [L.privateSendBackView(accts2, [], undefined, { [PRIV2 + "|ETH"]: "landed" }).held.map((h) => h.why), L.privateSendBackView(accts2, oldHeld, undefined, { [PRIV2 + "|ETH"]: "pending" }).rows.length,
     L.privateSendBackView(accts2, oldHeld, undefined, { [PRIV2 + "|ETH"]: "landed" }).rows.length], [["unread"], 0, 1]);
  // Planning checks the chosen account again, read just now: a burned holding stops before the sheet.
  const chosenEth = L.privateSendBackView(accts2, [], fromDep, {}).rows[0];
  check("plan check: the M2 case once the listing is read (holds none)", L.privateHoldingCheck(none, PRIV2, chosenEth.asset, D(1, 16)),
    { error: "This account holds no wETH now (it was read again just now). Nothing was done." });
  check("plan check: still enough, under way, less, a burn on its way",
    [L.privateHoldingCheck(more, PRIV2, chosenEth.asset, D(1, 16)), L.privateHoldingCheck(reading, PRIV2, chosenEth.asset, D(1, 16)).wait, L.privateHoldingCheck(more, PRIV2, chosenEth.asset, D(3, 16)).error,
     L.privateHoldingCheck(pending, PRIV2, chosenEth.asset, D(1, 16)).error, L.privateHoldingCheck(L.privateSendBackView(accts2, [], fromDep, {}), PRIV2, chosenEth.asset, D(1, 16))],
    [{ ok: true }, true, "This account holds 0.02 wETH now, less than that amount. Choose the amount again. Nothing was done.",
     "A send-back's burn from this account is on its way; send this one once it lands. Nothing was done.", { ok: true }]);
  check("token listing asked: the bridge's answers said",
    [L.listingAskedLine({ token_source: { accepted: true } }), L.listingAskedLine({ needs: "connect", token_source: { accepted: false } }), L.listingAskedLine({ token_source: { accepted: false, request: "r1" } }), L.listingAskedLine(null)],
    ["The token listing is accepted; reading what the account holds…", "Connect the LEZ Bridge to this wallet first (its app asks for that).", "The LEZ Bridge asked to list its tokens in this wallet: approve it in the sheet.", "The LEZ Bridge didn't ask to list its tokens. Nothing changed."]);
  check("screen: the private chips read the send-backs' burns, and the screen asks again while a read is being made",
    [/Logic\.privateSendBackView\(cliAccounts, bridgedHoldings, bridgeTokenTable, Logic\.sendBackBurns\(sendBacks, sbStatuses\)\)/.test(qml2), /sbPrivateView\.reading/.test(qml2), /Logic\.privateHeldLine\(root\.sbPrivateView\.held\)/.test(qml2)], [true, true, true]);
}


// review-branches-3, dimension erc20: tokens are known by their address or definition everywhere
// (never by symbol), "shown == submitted" by construction, a replaced choice is said, and the
// token table is built per token.
{
  const L = await import("./qml/logic.mjs");
  const qml3 = readFileSync(new URL("./qml/Main.qml", import.meta.url), "utf8");
  const ETH = "0x0000000000000000000000000000000000000000", DUSDC = "0xac207b9bfebf3ef7c26297ea6abadeeccc4aa71e", WETH20 = "0x" + "c0".repeat(20), USDT = "0x" + "11".repeat(20);
  const D = (n, k) => String(n) + "0".repeat(k);
  // #1 (a): native ETH and an ERC-20 the bridge lists as "wETH": both arrive as card "wETH".
  const twoWeth = L.bridgeTokens(null, [
    { eth_token: ETH, symbol: "ETH", decimals: 18, definition: "DefNative", denoms: [D(1, 16), D(1, 17), D(1, 18)] },
    { eth_token: WETH20, symbol: "wETH", decimals: 18, definition: "DefWeth20", denoms: [D(1, 17), D(1, 18), D(1, 19)] }]);
  check("#1a both rows have the card wETH", twoWeth.map((t) => t.card), ["wETH", "wETH"]);
  const erc = { symbol: "wETH", bridge: "B", base: D(1, 19), decimals: 18, definition: "DefWeth20", ethToken: WETH20 };
  const native = { symbol: "wETH", bridge: "B", base: D(1, 18), decimals: 18, definition: "DefNative", ethToken: ETH };
  check("#1a the ERC-20 row: its own amounts, and its own address is what is submitted",
    [L.sendBackAmounts(null, erc, twoWeth).map((d) => d.text + "@" + d.index), L.sendBackToken(erc, twoWeth)], [["0.1@0", "1@1", "10@2"], WETH20]);
  check("#1a the native row: its own amounts and the zero address", [L.sendBackAmounts(null, native, twoWeth).map((d) => d.text), L.sendBackToken(native, twoWeth)], [["0.01", "0.1", "1"], ETH]);
  check("#1a the old symbol path sent ETH for both (the bug)", [L.sendBackSymbol("wETH", twoWeth), L.sendBackSymbol("wETH", twoWeth)], ["ETH", "ETH"]);
  check("#1a a card symbol two rows share finds no token for an asset with no ids", L.sendBackKnown({ symbol: "wETH", base: D(1, 19) }, twoWeth), null);
  // #1 (b): the bridge lists the demo USDC's address as "USDC"; the Names card calls it "dUSDC".
  const relabelled = L.bridgeTokens(null, [{ eth_token: DUSDC, symbol: "USDC", decimals: 6, definition: "DefU", denoms: [D(1, 8), D(1, 9)] }]);
  const named = { symbol: "dUSDC", bridge: "B", base: D(2, 8), decimals: 6, definition: "DefU", ethToken: DUSDC };
  check("#1b offered, with the bridge's amounts, and submitted by its address (the old path sent dUSDC, which the module refuses)",
    [L.sendBackOffer(named, { program: "B", release: "proof", tokens: relabelled }), L.sendBackAmounts(null, named, relabelled).map((d) => d.text), L.sendBackToken(named, relabelled), L.sendBackSymbol("dUSDC", relabelled)],
    [true, ["100"], DUSDC, "dUSDC"]);
  check("#1 an asset whose ids the table doesn't know is not offered (never matched by its symbol)",
    [L.sendBackKnown({ symbol: "USDC", definition: "Other", ethToken: USDT }, relabelled), L.sendBackToken({ symbol: "USDC", definition: "Other", ethToken: USDT }, relabelled)], [null, ""]);
  // #1: source rows and the chosen chip by token, not by card symbol.
  const blocks3 = [{ account: "Public/A", assets: [native, erc] }];
  const srcs3 = L.sendBackSources(blocks3, { program: "B", release: "proof", tokens: twoWeth });
  check("#1 two sources with the same card are two keys", srcs3.map((x) => x.key), ["Public/A|def:DefNative", "Public/A|def:DefWeth20"]);
  check("#1 only the pressed one is chosen", [L.sameSource("Public/A", erc, srcs3[1]), L.sameSource("Public/A", erc, srcs3[0]), L.sameSource("Public/B", erc, srcs3[1])], [true, false, false]);
  const PRIV3 = "Private/yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z";
  const held3 = [{ holdings: [{ account: PRIV3, symbol: "wETH", balance: D(1, 18), decimals: 18, definition: "DefNative" },
                               { account: PRIV3, symbol: "wETH", balance: D(1, 19), decimals: 18, definition: "DefWeth20" }] }];
  const priv3 = L.privateSendBackSources([{ account: PRIV3, kind: "private" }], held3, twoWeth);
  check("#1 a private account keeps both tokens of a shared card, each with its address", priv3.map((r) => [r.key, r.asset.ethToken, r.amount]),
    [[PRIV3 + "|def:DefNative", ETH, "1"], [PRIV3 + "|def:DefWeth20", WETH20, "10"]]);
  check("#1 the screen submits the token the amounts came from and chooses chips by token",
    [/bridgeCall\(Logic\.PRIVATE_BRIDGE\.sendBackPlan, \[Logic\.sendBackToken\(sbAsset, bridgeTokenTable\), /.test(qml3), (qml3.match(/chosen: Logic\.sameSource\(root\.sbAccount, root\.sbAsset, modelData\)/g) || []).length,
     /sendBackSymbol\(/.test(qml3)], [true, 2, false]);
  // #1 on Receive too: the deposit request is asked for by address; chips by key.
  const rt = L.receiveTokens(null, [{ eth_token: ETH, symbol: "ETH", decimals: 18, denoms: [D(1, 16)] }, { eth_token: WETH20, symbol: "ETH", decimals: 18, denoms: [D(1, 17)] }]);
  check("#1 Receive: two tokens with one symbol are two keys; the request names the address", [rt.map((t) => t.key), L.receiveTokenArg(rt, WETH20), L.receiveTokenArg(rt, ETH)], [[ETH, WETH20], WETH20, ETH]);
  check("#1 Receive: the screen sends receiveTokenArg and chooses chips by key",
    [/var token = Logic\.receiveTokenArg\(receiveTokenList, receiveTokenNow\)/.test(qml3), /chosen: root\.receiveTokenNow === Logic\.tokenKey\(modelData\)/.test(qml3), /\[receiveTokenNow, String\(receiveDenomNow\)/.test(qml3)], [true, true, false]);
  check("#1 the deposit step reads the code's own token address", [L.receiveDepositSteps({ symbol: "ETH", deposit: { eth_token: WETH20 } }, rt)[2].indexOf("Approve") === 0, L.receiveDepositSteps({ symbol: "ETH", deposit: { eth_token: ETH } }, rt)], [true, L.RECEIVE_DEPOSIT_STEPS]);
  // #3: a pressed token paused or dropped: the choice changes visibly, and the amount starts again.
  const before = L.receiveTokens({ tokens: [{ symbol: "ETH", eth_token: ETH, decimals: 18, denoms: [{ value: D(1, 16) }, { value: D(1, 17) }, { value: D(1, 18) }] },
                                            { symbol: "USDT", eth_token: USDT, decimals: 6, denoms: [{ value: D(1, 8) }, { value: D(1, 9) }, { value: D(1, 10) }] }] });
  const paused = L.receiveTokens({ tokens: [{ symbol: "ETH", eth_token: ETH, decimals: 18, denoms: [{ value: D(1, 16) }, { value: D(1, 17) }, { value: D(1, 18) }] },
                                            { symbol: "USDT", eth_token: USDT, decimals: 6, freeze: { eth_deposits: true }, denoms: [{ value: D(1, 8) }, { value: D(1, 9) }, { value: D(1, 10) }] }] });
  const dropped = L.receiveTokens({ tokens: [{ symbol: "ETH", eth_token: ETH, decimals: 18, denoms: [{ value: D(1, 16) }] }] });
  const delisted = L.receiveTokens({ tokens: [{ symbol: "ETH", eth_token: ETH, decimals: 18, denoms: [{ value: D(1, 16) }] }, { symbol: "USDT", eth_token: USDT, registered: false, decimals: 6, denoms: [{ value: D(1, 8) }] }] });
  check("#3 the pressed USDT, then paused / dropped / delisted: ETH, with the reason said",
    [L.receiveTokenNow(before, USDT), L.receiveTokenNow(paused, USDT), L.receiveSwitchNote(paused, USDT, ETH, true), L.receiveSwitchNote(dropped, USDT, ETH, true), L.receiveSwitchNote(delisted, USDT, ETH, true)],
    [USDT, ETH, "USDT deposits are paused on this bridge for now, so ETH is chosen instead, at its smallest amount. Choose again if you want another.",
     "The token you chose is no longer offered by this bridge, so ETH is chosen instead, at its smallest amount. Choose again if you want another.",
     "USDT is no longer accepted by this bridge's vault, so ETH is chosen instead, at its smallest amount. Choose again if you want another."]);
  check("#3 nothing said for the person's own press or when nothing was pressed", [L.receiveSwitchNote(paused, ETH, ETH, true), L.receiveSwitchNote(paused, USDT, ETH, false)], ["", ""]);
  check("#3 the screen: any change of the token asked for resets the amount and keeps the reason on screen",
    [/onReceiveTokenNowChanged: \{\n\s*receiveSwitched = Logic\.receiveSwitchNote\(receiveTokenList, receiveToken, receiveTokenNow, receivePressed\)\n\s*receiveDenom = -1/.test(qml3),
     /StatusLine \{ visible: text !== ""; kind: ""; text: root\.receiveSwitched \}/.test(qml3)], [true, true]);
  {
    // Writing receiveToken (what receiveTokenNow is made from) inside receiveTokenNow's own change
    // is a binding loop (Qt: "Binding loop detected for property receiveTokenNow", seen in the
    // ERC-20 renders): the replacement is written after the change, through Qt.callLater.
    const handler = (qml3.match(/onReceiveTokenNowChanged: \{[\s\S]*?\n    \}\n/) || [""])[0];
    check("#3 the replacement token is written after its own change (no binding loop)",
      [/Qt\.callLater\(function\(\) \{ if \(receiveTokenNow === now && receiveToken !== now\) receiveToken = now \}\)/.test(handler), /\n\s*receiveToken = receiveTokenNow/.test(handler)], [true, false]);
  }
  check("#3 with the reset, the old index 2 (10,000 USDT) becomes ETH's smallest, not 1 ETH", L.denomOrSmallest(paused.filter((t) => t.key === ETH)[0].denoms, -1), 0);
  // #4: the table per token; Receive is the same table.
  const depList = [{ eth_token: ETH, symbol: "ETH", decimals: 18, definition: "E", denoms: [D(2, 16), D(2, 17)] }, { eth_token: USDT, symbol: "USDT", decimals: 6, definition: "U", denoms: [D(1, 8), D(1, 9)] }];
  const regAll = (registered) => ({ tokens: [{ symbol: "ETH", eth_token: ETH, decimals: 18, registered: registered, denoms: [] }, { symbol: "USDT", eth_token: USDT, decimals: 6, registered: registered, denoms: [] }] });
  const names = (l) => l.map((t) => t.symbol + ":" + t.denoms.map((d) => d.text).join("/"));
  check("#4 every registry row empty, the vault not read: the code's amounts, in the table and in Receive alike",
    [names(L.bridgeTokens(regAll(null), depList)), names(L.receiveTokens(regAll(null), depList))], [["ETH:0.02/0.2", "USDT:100/1,000"], ["ETH:0.02/0.2", "USDT:100/1,000"]]);
  check("#4 every row empty, the vault read (registered false): nothing offered, in both", [names(L.bridgeTokens(regAll(false), depList)), names(L.receiveTokens(regAll(false), depList))], [[], []]);
  const oneEmpty = { tokens: [{ symbol: "ETH", eth_token: ETH, decimals: 18, registered: true, denoms: [{ value: D(2, 16) }] }, { symbol: "USDT", eth_token: USDT, decimals: 6, registered: true, denoms: [] }] };
  check("#4 one row empty with the vault read: that token left out, the other from the registry, in both", [names(L.bridgeTokens(oneEmpty, depList)), names(L.receiveTokens(oneEmpty, depList))], [["ETH:0.02"], ["ETH:0.02"]]);
  check("#4 the fixed list only with no registry and no bridge list", [names(L.bridgeTokens(null, null)), names(L.bridgeTokens(regAll(false), null)), names(L.bridgeTokens(null, []))],
    [["ETH:0.01/0.1/1", "dUSDC:100/1,000/10,000"], ["ETH:0.01/0.1/1", "dUSDC:100/1,000/10,000"], []]);
  check("#4 the screen derives Receive from the same inputs as the table", /readonly property var receiveTokenList: Logic\.receiveTokens\(receiveRegistry, bridgeDeploymentTokens\)/.test(qml3), true);
}

console.log(failures === 0 ? "ALL PASS" : failures + " FAILED");
process.exit(failures ? 1 : 0);

