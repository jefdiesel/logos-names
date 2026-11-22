// Checks qml/tidy.mjs (the wallet card kept tidy: older bridge tokens folded, the bridge panels
// foldable, withdrawals hidden or said to be dead). Run with: node basecamp/tidy.test.mjs
import { asList, BRIDGE_ORDER, currentBridges, tokensFolded, olderLine, foldedNote, arrivingReady, bridgingReady, deadReason, deadSaid, hiddenRead, hiddenWith, visibleRows, hiddenLine, settingOn, HIDDEN_MAX } from "./qml/tidy.mjs";
import { bridgeNetRows, BRIDGE_LABELS, sendBackProgress } from "./qml/logic.mjs";
import { readFileSync } from "node:fs";

let failures = 0;
const check = (label, got, want) => {
  const ok = JSON.stringify(got) === JSON.stringify(want);
  if (!ok) failures++;
  console.log((ok ? "ok   " : "FAIL ") + label + (ok ? "" : `\n   got  ${JSON.stringify(got)}\n   want ${JSON.stringify(want)}`));
};

const [V2, V3, V4, V5, V6, V7] = BRIDGE_ORDER;
check("BRIDGE_ORDER is logic.mjs BRIDGE_LABELS' order", BRIDGE_ORDER, BRIDGE_LABELS.map((e) => e[0]));

// ── Which bridges stay shown ──
const read = [V2, V3, V4, V5, V6, V7].map((program) => ({ program }));
check("current: the chosen bridge and the newest read", currentBridges(V5, read), [V5, V7]);
check("current: chosen is the newest", currentBridges(V7, read), [V7]);
check("current: no chosen (an older LEZ Bridge): the newest read", currentBridges("", read), [V7]);
check("current: the newest known one whatever the list's order", currentBridges("", [{ program: V7 }, { program: V3 }]), [V7]);
check("current: an unknown bridge only: the last listed", currentBridges("", [{ program: "Xx1" }, { program: "Yy2" }]), ["Yy2"]);
check("current: an added bridge chosen, known ones read", currentBridges("Added9", read), ["Added9", V7]);
check("current: nothing known", [currentBridges("", null), currentBridges(null, [])], [[], []]);

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
  const split = tokensFolded(jefAssets, currentBridges(V5, jefRead.bridges));
  check("Jef's card, LEZ Bridge on v5: shown", split.shown.map(label), ["LEZ native", "wETH bridge v3", "dUSDC bridge v5", "wETH bridge v5", "dUSDC bridge v7", "wETH bridge v7"]);
  check("Jef's card, LEZ Bridge on v5: folded", split.folded.map(label), ["dUSDC bridge v2", "wETH bridge v2", "dUSDC bridge v3", "dUSDC bridge v4", "wETH bridge v4", "dUSDC bridge v6", "wETH bridge v6"]);
  check("the folded line", olderLine(split.folded.length), "Older bridge tokens · 7");
}
{
  const split = tokensFolded(jefAssets, currentBridges(V7, jefRead.bridges));
  check("on v7: v3's 0.1 wETH stays with its Move", split.shown.map(label), ["LEZ native", "wETH bridge v3", "dUSDC bridge v7", "wETH bridge v7"]);
  check("on v7: 9 fold", olderLine(split.folded.length), "Older bridge tokens · 9");
}
check("a token whose bridge isn't said stays", tokensFolded([{ symbol: "X", base: "0", bridge: "" }], [V7]).shown.length, 1);
// QML hands a Repeater's nested array over as a sequence Array.isArray refuses (the first render
// showed no tokens): taken as a list all the same.
const seq = { length: 2, 0: { symbol: "LEZ", base: "1" }, 1: { symbol: "wETH", base: "0", bridge: V2 } };
check("asList: a sequence, an array, nothing", [asList(seq).length, asList([1]).length, asList(null).length, asList("ab").length], [2, 1, 0, 0]);
check("a sequence folds like an array", tokensFolded(seq, { length: 1, 0: V7 }).folded.length, 1);
check("nothing current known: nothing folds", tokensFolded(jefAssets, []).folded.length, 0);
check("a held amount with leading zeros is still nothing", tokensFolded([{ symbol: "X", base: "000", bridge: V2 }], [V7]).folded.length, 1);
check("no folded tokens: no line", olderLine(0), "");

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
check("settingOn", [settingOn("yes"), settingOn(""), settingOn(undefined), settingOn("no")], [true, false, false, false]);

// ── QML rules (Basecamp's engine) and where Main.qml uses it ──
const code = readFileSync(new URL("./qml/tidy.mjs", import.meta.url), "utf8");
check("tidy.mjs: no BigInt literals or calls", /\b\d+n\b|\bBigInt\s*\(/.test(code.replace(/^\s*\/\/.*$/gm, "")), false);
check("tidy.mjs: no variables named like Java's reserved words", /\b(?:const|let|var|function)\s+(?:short|int|long|byte|char|float|double|boolean|native|package|private|protected|public|static|synchronized|throws|transient|volatile|goto|abstract|final|implements|interface)\b/.test(code), false);
const qml = readFileSync(new URL("./qml/Main.qml", import.meta.url), "utf8");
check("Main.qml imports tidy.mjs", qml.includes('import "tidy.mjs" as Tidy'), true);
check("Main.qml: the card's tokens fold by Tidy.tokensFolded", qml.includes("Tidy.tokensFolded(netAccount.modelData.assets, root.bridgeCurrent)"), true);
check("Main.qml: both panels have a PanelHead", (qml.match(/PanelHead \{/g) || []).length, 2);
check("Main.qml: a folded Arriving still counts its Claims", qml.includes('Tidy.arrivingReady(arrivingPanel.view.rows), "ready to claim"'), true);
check("Main.qml: hidden ids kept with app_setting", qml.includes('keepSetting("wallet_hidden_send_backs"'), true);
check("Main.qml: a hidden withdrawal isn't polled on the card", qml.includes("sbShowHidden || sbHidden.indexOf(x.id) < 0"), true);
check("Main.qml: module call args are text (app_setting)", /askLater\("app_setting", \[key, value\]/.test(qml), true);

console.log(failures === 0 ? "ALL PASS" : failures + " FAILED");
process.exit(failures ? 1 : 0);
