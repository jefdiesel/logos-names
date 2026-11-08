// Checks the view's own logic (qml/logic.mjs, which qml/Main.qml imports), since the QML
// engine cannot run in CI. Run with: node basecamp/logic.test.mjs
import { atLeast, someAmount, entryShown, tokenAmountProblem, tokenToProblem, tokenSendProves, tokenShown, feeEstimate, feePaid, accountTitle, accountsShown, publisherLine, durationWords, keyShown, chainToProblem, stakeLines, rewardsLine, grouped, addDigits, walletAssets, subDigits, walletGroups, stakeTable, activityLine, activityFor, agoShown } from "./qml/logic.mjs";
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
check("chainTo good", [KEY, "0x" + KEY, "alice", "@Alice", "alice.lez"].map((t) => chainToProblem(t)), ["", "", "", "", ""]);
check("chainTo short key", chainToProblem(KEY.substr(0, 60)), "A Logos Blockchain key is 64 hex characters; that one has 60.");
check("chainTo empty", chainToProblem("  "), "Enter a key or a name.");
check("stakeLines one key", stakeLines({ staked: "34002373886", notes: 4538, keys: [KEY], epoch: 7, next_epoch_in_s: 113980 }), [
  "34,002,373,886 staked in 4538 notes on 66ecf4bc…a81b.",
  "Epoch 7; the next starts in about 31.7 h. New tokens join the stake at an epoch boundary, up to two epochs after they arrive.",
]);
check("stakeLines no epoch length", stakeLines({ staked: "0", notes: 0, keys: [], epoch: 7, next_epoch_in_s: null })[1], "Epoch 7. New tokens join the stake at an epoch boundary, up to two epochs after they arrive.");
check("rewardsLine", [rewardsLine({ vouchers: 0 }), rewardsLine({ vouchers: 2, claimable: "26280" }), rewardsLine({ vouchers: 1722, claimable: "65436", reward_per_voucher: "38" })], ["Nothing to claim. Each block your node leads earns a reward to claim here.", "26,280 to claim from 2 blocks your node led.", "65,436 to claim from 1722 blocks your node led. Each claim cashes one block's reward (38) and pays its own fee, which can be more than that."]);

// Tokens: amounts are 128-bit, so they're checked as text.
const ok = (problem) => problem === "";
check("tokenAmount good", ["1", "1,000", "1_000", " 007 ", "340282366920938463463374607431768211455"].map((a) => ok(tokenAmountProblem(a))), [true, true, true, true, true]);
check("tokenAmount bad", ["", "0", "000", "-1", "1.5", "1e3", "340282366920938463463374607431768211456", "9".repeat(40), null].map((a) => ok(tokenAmountProblem(a))), [false, false, false, false, false, false, false, false, false]);
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
  // A send whose module restarted mid-way, or whose job record was pruned: not "failed", not "done".
  const unsure = activityLine({ kind: "pay_account", chain: "lez", to: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", amount: "100", state: "unknown", at: now }, now);
  check("activityLine unknown", [unsure.state, unsure.tone], ["Outcome unknown · check the balance", "warn"]);
  // An arrival whose window had a fee the module couldn't know claims no figure, only a least one.
  const least = activityLine({ kind: "arrived", chain: "lez", to: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", amount: null, at_least: "95648", state: "arrived", at: now }, now);
  check("activityLine arrived at least", [least.amount, least.sign, least.sub], ["", "", "in Public CCVd76Tk…pKnk on LEZ · at least 95,648"]);
  check("activityFor", activityFor([bridge, { kind: "pay", name: "ada" }], KEY).length, 1);
  check("agoShown", [agoShown(now - 5000, now), agoShown(now - 3 * 86400000, now)], ["just now", "3 d ago"]);
  check("agoShown in seconds", agoShown(Math.floor(now / 1000) - 7200, now), "2 h ago");
}

console.log(failures === 0 ? "ALL PASS" : failures + " FAILED");
process.exit(failures ? 1 : 0);
