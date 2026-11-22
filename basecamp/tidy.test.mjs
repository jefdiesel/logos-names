// Checks qml/tidy.mjs (the wallet card kept tidy: older bridge tokens folded, the bridge panels
// foldable, withdrawals hidden or said to be dead). Run with: node basecamp/tidy.test.mjs
import { asList, BRIDGE_ORDER, NEVER_RELEASE, currentBridges, tokensFolded, isOlder, arrivingSplit, bridgingSplit, olderLine, foldedNote, arrivingReady, bridgingReady, deadReason, deadSaid, hiddenRead, hiddenWith, visibleRows, hiddenLine, settingOn, HIDDEN_MAX, followedWhenHidden } from "./qml/tidy.mjs";
import { bridgeNetRows, BRIDGE_LABELS, BURN_BRIDGE_PROGRAMS, sendBackProgress, sendBacksFrom, sendBackRowsShown, receiveRowsShown } from "./qml/logic.mjs";
import { readFileSync } from "node:fs";

const qml0 = readFileSync(new URL("./qml/Main.qml", import.meta.url), "utf8");
let failures = 0;
const check = (label, got, want) => {
  const ok = JSON.stringify(got) === JSON.stringify(want);
  if (!ok) failures++;
  console.log((ok ? "ok   " : "FAIL ") + label + (ok ? "" : `\n   got  ${JSON.stringify(got)}\n   want ${JSON.stringify(want)}`));
};

const [V2, V3, V4, V5, V6, V7] = BRIDGE_ORDER;
check("BRIDGE_ORDER is logic.mjs BRIDGE_LABELS' order", BRIDGE_ORDER, BRIDGE_LABELS.map((e) => e[0]));

// ── Which bridges stay shown (22 Nov: only the current one, the LEZ Bridge's own) ──
const read = [V2, V3, V4, V5, V6, V7].map((program) => ({ program }));
check("current: the LEZ Bridge's chosen bridge only", currentBridges(V5), [V5]);
check("current: an added bridge chosen", currentBridges("Added9"), ["Added9"]);
check("current: not known yet (config not read): none, so nothing is older", [currentBridges(""), currentBridges(null), currentBridges(undefined)], [[], [], []]);
check("never release: v2 and v3", NEVER_RELEASE, [V2, V3]);
check("never release and bridge back are apart, and cover every known bridge", [NEVER_RELEASE.filter((p) => BURN_BRIDGE_PROGRAMS.includes(p)).length, NEVER_RELEASE.length + BURN_BRIDGE_PROGRAMS.length], [0, BRIDGE_ORDER.length]);

// Jef's account on 21 Nov (wallet-tokens.jpg): LEZ, wETH and dUSDC on v2 to v7, 0.1 wETH on v3.
const tok = (bridge, label, symbol, amount) => ({ definition: symbol + label, eth_token: symbol === "wETH" ? "0x0000000000000000000000000000000000000000" : "0xac207b9bfebf3ef7c26297ea6abadeeccc4aa71e",
  symbol, decimals: symbol === "wETH" ? 18 : 6, name: (symbol === "wETH" ? "Wrapped Sepolia ETH" : "Wrapped dUSDC") + " (" + label + ")", bridge, bridge_label: "bridge " + label, amount });
const jefTokens = [[V2, "v2"], [V3, "v3"], [V4, "v4"], [V5, "v5"], [V6, "v6"], [V7, "v7"]].flatMap(([b, l]) =>
  [tok(b, l, "dUSDC", "0"), tok(b, l, "wETH", l === "v3" ? "100000000000000000" : "0")]);
const jefRead = { bridges: read.map((r) => ({ ...r, bridge: true })), accounts: [{ account: "Public/BawxwquWtPEVcrwSFftD6jS3XFxZNv4qYrV8pkyo9mu7", found: true, native: "10000104865784", tokens: jefTokens, other_tokens: 0 }] };
const jefAssets = bridgeNetRows(jefRead, ["Public/BawxwquWtPEVcrwSFftD6jS3XFxZNv4qYrV8pkyo9mu7"], null)[0].assets;
check("Jef's card has 13 rows today", jefAssets.length, 13);
const label = (a) => a.symbol + " " + (a.bridgeLabel || "native");
{
  const split = tokensFolded(jefAssets, currentBridges(V5));
  check("Jef's card, LEZ Bridge on v5: only LEZ and v5's tokens", split.shown.map(label), ["LEZ native", "dUSDC bridge v5", "wETH bridge v5"]);
  check("Jef's card, LEZ Bridge on v5: v3's 0.1 wETH can never go back, the empty ones are nothing: no line", [split.folded.length, olderLine(split.folded.length)], [0, ""]);
}
{
  const split = tokensFolded(jefAssets, currentBridges(V7));
  check("on v7: LEZ and v7's tokens", split.shown.map(label), ["LEZ native", "dUSDC bridge v7", "wETH bridge v7"]);
  check("on v7: nothing older to bring back", split.folded.length, 0);
}
// The Mini's second account, read live on 22 Nov: 0.1 wETH on v2, 0.2 on v3, 1.2 on v5.
const ccv = [tok(V2, "v2", "wETH", "100000000000000000"), tok(V3, "v3", "wETH", "200000000000000000"), tok(V5, "v5", "wETH", "1200000000000000000"), tok(V5, "v5", "dUSDC", "0")];
const ccvAssets = bridgeNetRows({ bridges: jefRead.bridges, accounts: [{ account: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", found: true, native: "1471996", tokens: ccv, other_tokens: 0 }] },
  ["Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk"], null)[0].assets;
{
  const split = tokensFolded(ccvAssets, currentBridges(V5));
  check("the second account on v5: its v5 wETH stays, v2 and v3 are left out", [split.shown.map(label), split.folded.length], [["LEZ native", "wETH bridge v5", "dUSDC bridge v5"], 0]);
  // Had the LEZ Bridge moved to v7, the v5 1.2 wETH can still go back: behind the line.
  const on7 = tokensFolded(ccvAssets, currentBridges(V7));
  check("on v7: v5's 1.2 wETH waits behind the line", [on7.shown.map(label), on7.folded.map(label), olderLine(on7.folded.length)], [["LEZ native"], ["wETH bridge v5"], "Older bridges · 1"]);
}
check("a token whose bridge isn't said stays", tokensFolded([{ symbol: "X", base: "0", bridge: "" }], [V7]).shown.length, 1);
check("an unknown older bridge's held token waits behind the line (it may still go back)", tokensFolded([{ symbol: "X", base: "5", bridge: "Zz9" }], [V5]).folded.length, 1);
// QML hands a Repeater's nested array over as a sequence Array.isArray refuses (the first render
// showed no tokens): taken as a list all the same.
const seq = { length: 2, 0: { symbol: "LEZ", base: "1" }, 1: { symbol: "wETH", base: "7", bridge: V4 } };
check("asList: a sequence, an array, nothing", [asList(seq).length, asList([1]).length, asList(null).length, asList("ab").length], [2, 1, 0, 0]);
check("a sequence splits like an array", tokensFolded(seq, { length: 1, 0: V7 }).folded.length, 1);
check("nothing current known: everything shown", [tokensFolded(jefAssets, []).shown.length, tokensFolded(jefAssets, []).folded.length], [13, 0]);
check("a held amount with leading zeros is still nothing", tokensFolded([{ symbol: "X", base: "000", bridge: V4 }], [V7]), { shown: [], folded: [] });
check("the line", [olderLine(0), olderLine(2)], ["", "Older bridges · 2"]);

// ── Receives and withdrawals of older bridges ──
const dep = (current) => ({ id: "x", label: current ? "bridge v5" : "bridge v4", current });
check("isOlder: only when the LEZ Bridge says not current", [isOlder({ deployment: dep(false) }), isOlder({ deployment: dep(true) }), isOlder({ deployment: null }), isOlder({}), isOlder(null)], [true, false, false, false, false]);
{
  const recv = [{ id: "r1", symbol: "ETH", amount: "0.1", account: "Public/A", deployment: dep(true) }, { id: "r2", deployment: dep(false) }, { id: "r3", deployment: dep(false) },
    { id: "r4", deployment: dep(false) }, { id: "r5" }];
  const st = { r1: { stage: "claimed", claimed_block: 9 }, r2: { stage: "claimable" }, r3: { stage: "claimed", claimed_block: 9 }, r4: { stage: "requested" }, r5: { stage: "requested" } };
  const view = receiveRowsShown(recv, st, {}, 0, true);
  const split = arrivingSplit(view.rows);
  check("arriving: current and unsaid stay; an older one arrived waits; older claimed or unused codes are left out", [split.rows.map((x) => x.id), split.older.map((x) => x.id)], [["r5"], ["r2"]]);
  check("arriving: an older one on its way waits too", ["deposited", "final", "claimable", "noclaim", "claimed", "waiting", "lost", "checking"].map((stage) => arrivingSplit([{ deployment: dep(false), progress: { stage } }]).older.length), [1, 1, 1, 1, 0, 0, 0, 0]);
}
{
  const list = sendBacksFrom({ send_backs: [{ id: "sb-a", stage: "recorded", deployment: dep(true) }, { id: "sb-b", stage: "recorded", deployment: dep(false) }, { id: "sb-c", stage: "recorded", deployment: dep(false) },
    { id: "sb-d", stage: "released", deployment: dep(false) }, { id: "sb-e", stage: "burned" }] });
  check("send_backs rows keep their bridge", list.map((x) => x.deployment && x.deployment.current), [true, false, false, false, null]);
  // sb-a moved to another bridge's status read: the status's deployment wins.
  const rows = sendBackRowsShown(list, { "sb-a": { stage: "recorded", deployment: dep(false) }, "sb-d": { stage: "released" } }, { "sb-d": 1000 }, 2000);
  const split = bridgingSplit(rows, {}, { "sb-c": "the Names Wallet gives another exit secret than this send-back was made with" });
  check("bridging: current and unsaid stay; an older one that can be finished waits; dead or received ones are left out", [split.rows.map((x) => x.id), split.older.map((x) => x.id)], [["sb-e"], ["sb-a", "sb-b"]]);
}

// ── A folded panel's header ──
check("open: nothing beside the title", foldedNote(false, 3, 1, "ready to claim"), "");
check("folded: how many", foldedNote(true, 2, 0, "ready to finish"), "2");
check("folded: the claim ready still said", foldedNote(true, 3, 1, "ready to claim"), "3 · 1 ready to claim");
check("folded: all of them ready", foldedNote(true, 1, 1, "ready to claim"), "1 ready to claim");
const arriving = [{ id: "a", progress: { claim: true } }, { id: "b", progress: { claim: false } }, { id: "c", progress: {} }];
check("arrivingReady counts the Claims", [arrivingReady(arriving), arrivingReady(null)], [1, 0]);
// Jef's two withdrawals as the card shows them (wallet-top.jpg): recorded, the service offered.
const recorded = { stage: "recorded", settle_offer: { available: true, prove_first: true }, root: "0xf3be", releasable_at: 1795047180 };
const row = (id, status) => ({ id, progress: sendBackProgress(status, 1795400000) });
check("Jef's rows read 'finish it' today", row("sb-6163f4e0007b40e3", recorded).progress.card, "Recorded on Ethereum · finish it");
const twoRows = [row("sb-6163f4e0007b40e3", recorded), row("sb-8c065e4e54235fe6", recorded), row("sb-1", { stage: "burned" })];
check("bridgingReady: the ones waiting on the person", bridgingReady(twoRows, {}, {}), 2);
check("bridgingReady: a dead one isn't ready", bridgingReady(twoRows, {}, { "sb-6163f4e0007b40e3": "the Names Wallet gives another exit secret than this send-back was made with (another account or wallet?)" }), 1);
check("Bridging folded", foldedNote(true, twoRows.length, bridgingReady(twoRows, {}, {}), "ready to finish"), "3 · 2 ready to finish");

// ── Dead withdrawals: only the bridge's own words decide ──
check("a recorded one that can be finished isn't dead (Jef's two, read 21 Nov: root recorded, nullifier unspent, vault funded)", deadReason(recorded, ""), "");
check("the wallet's exit secret changed", deadReason(recorded, "the Names Wallet gives another exit secret than this send-back was made with (another account or wallet?)"),
  "Can't be finished: your wallet no longer makes the secret this withdrawal was made with.");
check("made on a bridge no longer listed (send_back_status's note)", deadReason({ stage: "recorded", note: "This send-back was made on \"bridge-test-network-v4\", a bridge this app doesn't list any more (Settings → Bridge), so it can't be read. Nothing was read; add that bridge again from its code." }, ""),
  "Can't be followed: it was made on a bridge the LEZ Bridge no longer lists (Settings → Bridge).");
check("a gate update that will never pass", deadReason({ stage: "burned", update: { dead: true }, text: "Burned. x" }, "").indexOf("Can't be finished") === 0, true);
check("a busy read or the local-prover refusal is not dead",
  [deadReason({ stage: "recorded", note: "busy", busy: true }, ""), deadReason(recorded, "This computer has the proof helper: make the proof here first (\"Make the proof\"), then finish on Ethereum."),
   deadReason(recorded, "the exit leaves read now don't give the recorded root")], ["", "", ""]);
check("released is never dead", deadReason({ stage: "released", note: "a bridge this app doesn't list any more" }, ""), "");
check("deadSaid", [deadSaid("the Names Wallet gives another exit secret than this send-back was made with"), deadSaid("timed out"), deadSaid("")], [true, false, false]);

// ── Hidden withdrawals ──
check("hiddenRead: kept text", hiddenRead('["sb-6163f4e0007b40e3","sb-8c065e4e54235fe6"]'), ["sb-6163f4e0007b40e3", "sb-8c065e4e54235fe6"]);
check("hiddenRead: nothing or junk", [hiddenRead(""), hiddenRead(undefined), hiddenRead("{"), hiddenRead('{"a":1}'), hiddenRead('["x", "sb-zz", "sb-ab", "sb-ab"]')], [[], [], [], [], ["sb-ab"]]);
check("hiddenWith: hide, again, show", [hiddenWith([], "sb-1", true), hiddenWith(["sb-1"], "sb-1", true), hiddenWith(["sb-1", "sb-2"], "sb-1", false)], [["sb-1"], ["sb-1"], ["sb-2"]]);
check("hiddenWith keeps the newest " + HIDDEN_MAX, (() => { let l = []; for (let i = 0; i < HIDDEN_MAX + 5; i++) l = hiddenWith(l, "sb-" + i.toString(16), true); return [l.length, l[0], l[l.length - 1]]; })(), [HIDDEN_MAX, "sb-5", "sb-" + (HIDDEN_MAX + 4).toString(16)]);
check("the kept text reads back", hiddenRead(JSON.stringify(hiddenWith(["sb-1"], "sb-2", true))), ["sb-1", "sb-2"]);
const shown = visibleRows(twoRows, ["sb-6163f4e0007b40e3", "sb-8c065e4e54235fe6"], false);
check("visibleRows: hidden ones off the card", [shown.rows.map((x) => x.id), shown.hidden], [["sb-1"], 2]);
check("visibleRows: Show them", visibleRows(twoRows, ["sb-6163f4e0007b40e3"], true).rows.length, 3);
check("visibleRows: nothing hidden", visibleRows(twoRows, [], false).hidden, 0);
check("hiddenLine", [hiddenLine(0), hiddenLine(1), hiddenLine(2)], ["", "1 hidden withdrawal", "2 hidden withdrawals"]);
check("a hidden one in flight is still followed (its check hands the signed burn to the relay)",
  ["planned", "approve", "burning", "burned", "recorded", "released"].map(followedWhenHidden), [false, true, true, false, false, false]);
check("Main.qml: the card's polls follow a hidden one in flight", (qml0.match(/Tidy\.followedWhenHidden\(/g) || []).length, 2);
check("settingOn", [settingOn("yes"), settingOn(""), settingOn(undefined), settingOn("no")], [true, false, false, false]);

// ── QML rules (Basecamp's engine) and where Main.qml uses it ──
const code = readFileSync(new URL("./qml/tidy.mjs", import.meta.url), "utf8");
check("tidy.mjs: no BigInt literals or calls", /\b\d+n\b|\bBigInt\s*\(/.test(code.replace(/^\s*\/\/.*$/gm, "")), false);
check("tidy.mjs: no variables named like Java's reserved words", /\b(?:const|let|var|function)\s+(?:short|int|long|byte|char|float|double|boolean|native|package|private|protected|public|static|synchronized|throws|transient|volatile|goto|abstract|final|implements|interface)\b/.test(code), false);
const qml = readFileSync(new URL("./qml/Main.qml", import.meta.url), "utf8");
check("Main.qml imports tidy.mjs", qml.includes('import "tidy.mjs" as Tidy'), true);
check("Main.qml: the card's tokens fold by Tidy.tokensFolded", qml.includes("Tidy.tokensFolded(netAccount.modelData.assets, root.bridgeCurrent)"), true);
check("Main.qml: current is the LEZ Bridge's chosen bridge only", /Tidy\.currentBridges\(bridgeConfig !== null[^\n]*bridge_program\) : ""\)\n/.test(qml), true);
check("Main.qml: an opened fold shows only what tokensFolded kept", qml.includes("root.olderOpen ? netAccount.split.shown.concat(netAccount.split.folded) : netAccount.split.shown"), true);
check("Main.qml: receives and withdrawals split by bridge", [qml.includes("Tidy.arrivingSplit(full.rows)"), qml.includes("Tidy.bridgingSplit(sbRowsNow, sbStatuses, sbDeadSaid)")], [true, true]);
check("Main.qml: three Older bridges lines, one switch", [(qml.match(/OlderLine \{ /g) || []).length, (qml.match(/property bool olderOpen/g) || []).length], [3, 1]);
check("Main.qml: an older withdrawal is still followed (the polls read sendBacks, not the card)", [qml.includes("sendBacks.filter(function(x) { return sbShowHidden || sbHidden.indexOf(x.id) < 0") || qml.includes("sbShowHidden || sbHidden.indexOf(x.id) < 0"), /sbScheduleNext[\s\S]{0,400}sbCard/.test(qml)], [true, false]);
check("Main.qml: both panels have a PanelHead", (qml.match(/PanelHead \{/g) || []).length, 2);
check("Main.qml: a folded Arriving still counts its Claims", qml.includes('Tidy.arrivingReady(arrivingPanel.view.rows), "ready to claim"'), true);
check("Main.qml: hidden ids kept with app_setting", qml.includes('keepSetting("wallet_hidden_send_backs"'), true);
check("Main.qml: a hidden withdrawal isn't polled on the card", qml.includes("sbShowHidden || sbHidden.indexOf(x.id) < 0"), true);
check("Main.qml: module call args are text (app_setting)", /askLater\("app_setting", \[key, value\]/.test(qml), true);

console.log(failures === 0 ? "ALL PASS" : failures + " FAILED");
process.exit(failures ? 1 : 0);
