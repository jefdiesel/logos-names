// Checks the view's own logic (qml/logic.mjs, which qml/Main.qml imports), since the QML
// engine cannot run in CI. Run with: node basecamp/logic.test.mjs
import { atLeast, someAmount, entryShown, amountRead, amountProblem, amountDigits, AMOUNT_MAX, CHAIN_AMOUNT_MAX, jobProblem, tokenToProblem, tokenSendProves, tokenShown, feeEstimate, feePaid, accountTitle, accountsShown, publisherLine, durationWords, keyShown, chainToProblem, stakeLines, rewardsLine, grouped, addDigits, walletAssets, subDigits, walletGroups, stakeTable, activityLine, activityFor, agoShown, readNote, chainKeyTitle, chainKeySub, chainUsable, chainLaterNote } from "./qml/logic.mjs";
import { readFileSync } from "node:fs";

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
  check("activityLine bridge", activityLine(bridge, now), { title: "Bridge to LEZ", sub: "from key 7a6223ab…2a2a into Public 9W9huNeA…WfoW", amount: "1,000,000", sign: "-", state: "In a block · arrives in about 12 h", tone: "warn", ago: "2 h ago" });
  check("activityLine arrived", activityLine({ kind: "arrived", chain: "lez", to: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", amount: "5", state: "arrived", at: now }, now).sign, "+");
  check("activityLine split", activityLine({ kind: "split", chain: "logos", from: KEY, amount: "1000000", fee: "2018", state: "in_block", at: now }, now).title, "Made a note of exactly 1,000,000");
  check("activityLine failed pay", activityLine({ kind: "pay", chain: "lez", name: "ada", amount: "7", state: "failed", at: now }, now).tone, "bad");
  check("activityLine receive_privately", [activityLine({ kind: "receive_privately", chain: "lez", name: "xferecho", fee: { about: "400000", at_most: "900000", paid: "417376" }, state: "done", at: now }, now).title, activityLine({ kind: "receive_privately", chain: "lez", name: "xferecho", fee: { about: "400000", at_most: "900000", paid: "417376" }, state: "done", at: now }, now).sub], ["Private payments to xferecho", "on LEZ, new receiving keys · fee 417,376"]);
  check("activityLine bridge not followed", activityLine(Object.assign({}, bridge, { to_before: null, followed: false }), now).state, "In a block · its arrival on LEZ can't be followed here");
  check("activityLine nothing claimed", activityLine({ kind: "receive", chain: "lez", claimed: false, state: "done", at: now }, now).title, "Nothing claimed");
  // A send whose module restarted mid-way, or whose job record was pruned: not "failed", not "done".
  const unsure = activityLine({ kind: "pay_account", chain: "lez", to: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", amount: "100", state: "unknown", at: now }, now);
  check("activityLine unknown", [unsure.state, unsure.tone], ["Outcome unknown · check the balance", "warn"]);
  // An arrival whose window had a fee the module couldn't know claims no figure, only a least one.
  const least = activityLine({ kind: "arrived", chain: "lez", to: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", amount: null, at_least: "95648", state: "arrived", at: now }, now);
  check("activityLine arrived at least", [least.amount, least.sign, least.sub], ["", "", "in Public CCVd76Tk…pKnk on LEZ · at least 95,648"]);
  check("activityFor", activityFor([bridge, { kind: "pay", name: "ada" }], KEY).length, 1);
  check("agoShown", [agoShown(now - 5000, now), agoShown(now - 3 * 86400000, now)], ["just now", "3 d ago"]);
  check("agoShown in seconds", agoShown(Math.floor(now / 1000) - 7200, now), "2 h ago");
  // activity-public-send-says-proving: a public send in flight said "Proving…".
  const flying = (fields) => activityLine(Object.assign({ chain: "lez", state: "sending", to: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", amount: "1000", at: Math.floor(now / 1000) }, fields), now).state;
  check("activityLine public sends are sending", [flying({ kind: "pay_account", private: false }), flying({ kind: "token_send", from: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk" }), flying({ kind: "pay_account", private: true }), flying({ kind: "shield" }), flying({ kind: "receive" }), flying({ kind: "pay", from: "Private/yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z" }), activityLine({ kind: "send", chain: "logos", state: "sending", at: now }, now).state], ["Sending…", "Sending…", "Proving…", "Proving…", "Proving…", "Proving…", "Sending…"]);
  // activity-bridge-row-wording: 2 h, 3 d and 1090 d old all said "arrives in about 12 h".
  check("activityLine bridge overdue", [activityLine(bridge, now).state, activityLine(Object.assign({}, bridge, { at: Math.floor(now / 1000) - 259200 }), now).state, activityLine(Object.assign({}, bridge, { at: 1700000000 }), now).state], ["In a block · arrives in about 12 h", "In a block · should have arrived by now; check the balance", "In a block · should have arrived by now; check the balance"]);
  // single-bridge-false-arrival-on-foreign-credit-at-least-amount: bridges matched together
  // by one credit are hedged, one matched by its own credit is not.
  check("activityLine bridge arrived hedged", [activityLine(Object.assign({}, bridge, { state: "arrived" }), now).state, activityLine(Object.assign({}, bridge, { state: "arrived", maybe: true }), now).state], ["Arrived on LEZ", "A credit of the same total as your bridges reached LEZ"]);
  // late-or-expired-bridge-arrival-logged-twice / expired-bridge-takes-later-bridges-credit:
  // a bridge not credited within 2 days is overdue (still followed by its receipt), and one
  // credited after that arrives late, as one row.
  const overdue = activityLine(Object.assign({}, bridge, { state: "expired", followed: true }), now);
  check("activityLine bridge expired", [overdue.state, overdue.tone, activityLine(Object.assign({}, bridge, { state: "expired", followed: false }), now).state],
    ["Not credited on LEZ after 2 days · still followed; check the balance", "warn", "Not seen on LEZ after 2 days · check the balance"]);
  check("activityLine bridge arrived late", activityLine(Object.assign({}, bridge, { state: "arrived", late: true }), now).state, "Arrived on LEZ, late");
  // claim-entry-not-on-credited-accounts-page-and-shown-as-credit-on-payer: the module's
  // reply for a live 2-account claim (payer E3, two accounts credited 2,000,000 each).
  const claim = { all: true, amount: "4000000", at: 1794204085, chain: "lez", fee: { about: "5070616", at_most: "8239751", paid: "6972097" }, from: "Private/E3j7HPx3ZhoqRnB9BtMsGjWixbms3n9kamGKXKkjiuy5", id: "1794204085-receive", kind: "receive", received: { "Private/6MJ8W5vH7vTLPpCcnuA87YU3foxrx676CkzvtGHfBJXC": "2000000", "Private/9drrc1ARLeTXhHrgV8r54iNiWDD6Rj3yUMxQJTditnaK": "2000000" }, seq: 10, state: "done" };
  const onPage = (entry, who) => activityFor([entry], who).map((x) => { const l = activityLine(x, 1794205600000, who); return [l.title, l.sign + l.amount, l.sub]; });
  check("claim on a credited page", onPage(claim, "Private/9drrc1ARLeTXhHrgV8r54iNiWDD6Rj3yUMxQJTditnaK"), [["Claimed waiting payments", "+2,000,000", "on LEZ, into your private accounts"]]);
  check("claim on the other credited page", onPage(claim, "Private/6MJ8W5vH7vTLPpCcnuA87YU3foxrx676CkzvtGHfBJXC").length, 1);
  check("claim on the payer's page", onPage(claim, claim.from), [["Paid the fee to claim waiting payments", "-6,972,097", "on LEZ, into your other private accounts"]]);
  check("claim in the wallet's history", [activityLine(claim, 1794205600000).sign, activityLine(claim, 1794205600000).amount], ["+", "4,000,000"]);
  // The live 1-account claim (seq 9): its `to` is the credited account.
  const one = Object.assign({}, claim, { amount: "2000000", fee: { about: "5070616", at_most: "8239751", paid: "5070616" }, received: { "Private/6MJ8W5vH7vTLPpCcnuA87YU3foxrx676CkzvtGHfBJXC": "2000000" }, to: "Private/6MJ8W5vH7vTLPpCcnuA87YU3foxrx676CkzvtGHfBJXC" });
  check("one claim, payer and credited pages", [onPage(one, one.from)[0][1], onPage(one, one.to)[0][1]], ["-5,070,616", "+2,000,000"]);
  // A payer credited too: its own credit, with the fee it paid beside it.
  const both = Object.assign({}, claim, { received: { [claim.from]: "3000000" } });
  check("claim payer also credited", onPage(both, claim.from), [["Claimed waiting payments", "+3,000,000", "on LEZ, into your private accounts · fee 6,972,097"]]);
  // partial-claim-job-and-pages-say-nothing-true: 1 of 2 claims landed (R1), R2's credit waits,
  // the payer paid one claim's fee. Partly done on all three pages, each in its own words.
  const R1 = "Private/6MJ8W5vH7vTLPpCcnuA87YU3foxrx676CkzvtGHfBJXC", R2 = "Private/9drrc1ARLeTXhHrgV8r54iNiWDD6Rj3yUMxQJTditnaK";
  const partial = Object.assign({}, claim, { amount: "1000000", to: R1, received: { [R1]: "1000000" }, left: { [R2]: "1000000" }, fee: { about: "10141232", at_most: "16479502", paid: "5070616" }, state: "partial" });
  check("partial claim on the credited page", onPage(partial, R1), [["Claimed waiting payments", "+1,000,000", "on LEZ, into your private accounts · some still wait"]]);
  check("partial claim on the waiting page", onPage(partial, R2), [["Still waiting to be claimed", "1,000,000", "on LEZ, claim it with Receive once your paying account holds enough"]]);
  check("partial claim on the payer's page", onPage(partial, claim.from), [["Paid the fee to claim waiting payments", "-5,070,616", "on LEZ, into your other private accounts"]]);
  check("partial claim state", [activityLine(partial, 1794205600000).state, activityLine(partial, 1794205600000).tone], ["Partly done · see what's left", "warn"]);
  // incoming-payment-shown-as-outgoing-on-receiving-accounts-page: a payment into A showed
  // "Paid Public A ... -1,000" on A's own page.
  const A = "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", P = "Private/yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z";
  const paid = { kind: "pay_account", chain: "lez", from: P, to: A, amount: "1000", private: true, fee: { about: null, at_most: null, paid: "70000" }, state: "done", at: 1794200000 };
  check("payment on the payee's page", onPage(paid, A), [["Received from Private yGrPEbP3…fe4Z", "+1,000", "on LEZ"]]);
  check("payment on the payer's page", onPage(paid, P), [["Paid Public CCVd76Tk…pKnk", "-1,000", "on LEZ, from your private account · fee 70,000"]]);
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
  check("credited page: a failed claim", onPage(failedClaim, "Private/9drrc1ARLeTXhHrgV8r54iNiWDD6Rj3yUMxQJTditnaK"), [["Claim of waiting payments", "2,000,000", "on LEZ, into your private accounts"]]);
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

console.log(failures === 0 ? "ALL PASS" : failures + " FAILED");
process.exit(failures ? 1 : 0);
