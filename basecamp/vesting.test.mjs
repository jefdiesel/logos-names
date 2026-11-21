// Checks the Vesting page's rules (qml/vesting.mjs) and how Main.qml and metadata.json use them.
// Run with: node basecamp/vesting.test.mjs
import { accountIds, reply, groupOf, unlockMs, groups, badgeCount, badgeText, seenWith, behind, scanLine, someAmount, rowLines, pageState, stateText } from "./qml/vesting.mjs";
import { readFileSync } from "node:fs";

let failures = 0;
const check = (label, got, want) => {
  const ok = JSON.stringify(got) === JSON.stringify(want);
  if (!ok) failures++;
  console.log((ok ? "ok   " : "FAIL ") + label + (ok ? "" : `\n   got  ${JSON.stringify(got)}\n   want ${JSON.stringify(want)}`));
};

const ALICE = "2RHZhw9h534Zr3eq2RGhQete2Hh667foECzXPmSkGni2";
const BOB = "CbgR6tj5kWx5oziiFptM7jMvrQeYY3Mzaao6ciuhSr2r";
const S1 = "9iHYqf77noB5oyGrxhL5GNVLSKcKQZeGBzp294fMi3Mv";
const S2 = "HLEwXNBDYx6GS9H5CsawDrMTptD6R4XsJywFEMr6eim2";
const S3 = "7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR";
const S4 = "CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk";
const row = (address, status, extra) => Object.assign({ address, creator: BOB, beneficiary: ALICE, total_display: "1,000 LEZ", claimed_display: "0", claimable_display: "0", next_unlock_ms: 0, status }, extra || {});

// The accounts incoming takes: bare base58 IDs, each once, nothing else.
check("accountIds: Public/ stripped, once each", accountIds(["Public/" + ALICE, "Public/" + ALICE, BOB, "Private/" + S1, "Public/xyz", "", null]), [ALICE, BOB]);
check("accountIds: objects and non-lists", [accountIds([{ account: "Public/" + ALICE }]), accountIds(null), accountIds("x")], [[ALICE], [], []]);

// Replies: the module's {ok} text, encoded once or twice more by Basecamp, and the host's words.
const answer = { ok: true, to_block: 10, tip: 20, schedules: [] };
check("reply: ok, as sent", reply(JSON.stringify(answer)), { kind: "ok", value: answer, error: "" });
check("reply: ok, encoded twice", reply(JSON.stringify(JSON.stringify(answer))).kind, "ok");
check("reply: ok, in a host envelope", reply(JSON.stringify({ success: true, value: JSON.stringify(answer) })).value, answer);
check("reply: the module's refusal", reply(JSON.stringify({ ok: false, error: "the chain didn't answer" })), { kind: "error", value: null, error: "the chain didn't answer" });
check("reply: refusal object with a message", reply(JSON.stringify({ ok: false, error: { message: "bad accounts" } })).error, "bad accounts");
check("reply: no Vest module", [reply("Module not connected").kind, reply(JSON.stringify({ error: "Module not reachable yet" })).kind, reply("vesting is not installed").kind], ["missing", "missing", "missing"]);
check("reply: an older Vest without the call", [reply(JSON.stringify({ error: "Method not found" })).kind, reply("unknown method incoming").kind, reply(JSON.stringify({ ok: false, error: "unknown_method: incoming" })).kind], ["nomethod", "nomethod", "nomethod"]);
check("reply: no answer yet", [reply(undefined).kind, reply(null).kind, reply("").kind, reply("Timed out waiting for the module").kind, reply(JSON.stringify({ error: "timeout" })).kind, reply("{}").kind], ["busy", "busy", "busy", "busy", "busy", "busy"]);
check("reply: the host refused the call", reply(JSON.stringify({ error: "Call refused", message: "expected string at arg0" })), { kind: "error", value: null, error: "Basecamp refused the call: expected string at arg0" });

// Groups: Claimable now, Unvested (soonest unlock first, no date last), Finished; each once.
const list = [row(S1, "unvested", { next_unlock_ms: 3000 }), row(S2, "claimable", { claimable_display: "250 LEZ" }), row(S3, "closed"), row(S4, "unvested", { next_unlock_ms: 1000 }),
  row("Dup", "unvested", { next_unlock_ms: 0 }), row("Dup", "claimable"), row("Odd", "paused")];
const g = groups(list);
check("groups: claimable", g.claimable.map((s) => s.address), [S2]);
check("groups: unvested by next unlock, none last, unknown status there", g.unvested.map((s) => s.address), [S4, S1, "Dup", "Odd"]);
check("groups: finished", g.finished.map((s) => s.address), [S3]);
check("groups: nothing", groups(null), { claimable: [], unvested: [], finished: [] });
check("groupOf", [groupOf({ status: "claimable" }), groupOf({ status: "closed" }), groupOf({ status: "unvested" }), groupOf(null)], ["claimable", "finished", "unvested", "unvested"]);
check("unlockMs: a real, 0 for none", [unlockMs({ next_unlock_ms: 1792000000123 }), unlockMs({ next_unlock_ms: "1792000000123" }), unlockMs({}), unlockMs({ next_unlock_ms: -5 })], [1792000000123, 1792000000123, 0, 0]);

// The menu's count: claimable ones, and unvested ones not seen on the page yet.
check("badgeCount: claimable + new unvested", badgeCount(list, []), 1 + 4);
check("badgeCount: seen unvested don't count", badgeCount(list, [S1, S4, "Dup", "Odd"]), 1);
check("badgeCount: claimable always count", badgeCount(list, [S2]), 5);
check("badgeCount: finished never count", badgeCount([row(S3, "closed")], []), 0);
check("seenWith: the page marks unvested and claimable seen", seenWith([], list).sort(), [S1, S2, S4, "Dup", "Odd"].sort());
check("seenWith: after it, only claimable count", badgeCount(list, seenWith([], list)), 1);
check("badgeText", [badgeText(0), badgeText(3), badgeText(120), badgeText(undefined)], ["", "3", "99+", ""]);

// The scan.
check("behind", [behind({ to_block: 10, tip: 20 }), behind({ to_block: 20, tip: 20 }), behind({ to_block: 0, tip: 0 }), behind(null)], [true, false, false, false]);
check("scanLine", [scanLine({ to_block: 41200, tip: 52310 }), scanLine({ to_block: 52310, tip: 52310 })], ["Looking for schedules… block 41,200 of 52,310", ""]);

// A row's words; display amounts shown exactly as the module gives them.
const date = (ms) => "D" + ms;
check("rowLines: claimable", rowLines(row(S2, "claimable", { claimable_display: "250 LEZ" }), date), { amount: "1,000 LEZ", line: "Claimable now: 250 LEZ" });
check("rowLines: claimable, some claimed, next unlock", rowLines(row(S2, "claimable", { claimable_display: "250 LEZ", claimed_display: "100 LEZ", next_unlock_ms: 5 }), date).line, "Claimable now: 250 LEZ · claimed so far: 100 LEZ · next unlock D5");
check("rowLines: unvested with a date", rowLines(row(S1, "unvested", { next_unlock_ms: 3000 }), date), { amount: "1,000 LEZ", line: "Nothing to claim until D3000" });
check("rowLines: unvested without a date", rowLines(row(S1, "unvested"), date).line, "Nothing to claim yet");
check("rowLines: finished", [rowLines(row(S3, "closed", { claimed_display: "1,000 LEZ" }), date).line, rowLines(row(S3, "closed"), date).line], ["Finished · claimed 1,000 LEZ", "Finished · nothing left to claim"]);
check("rowLines: a huge amount is never a Number", rowLines(row(S1, "unvested", { total_display: "123456789012345678901234567890" }), date).amount, "123456789012345678901234567890");
check("someAmount", [someAmount("0"), someAmount("0 LEZ"), someAmount("0.000"), someAmount("0.5"), someAmount(""), someAmount("12 LEZ")], [false, false, false, true, false, true]);

// What the page shows.
const at = (over) => pageState(Object.assign({ net: "0.4", installed: true, kind: "ok", answered: true, accounts: 2, accountsKnown: true, count: 3 }, over));
check("pageState: list", at({}), "list");
check("pageState: empty", at({ count: 0 }), "empty");
check("pageState: another network", at({ net: "0.3" }), "network");
check("pageState: no Vest installed", [at({ installed: false, kind: "" }), at({ installed: null, kind: "missing" })], ["missing", "missing"]);
check("pageState: installed, not running", at({ kind: "missing" }), "idle");
check("pageState: an older Vest", at({ kind: "nomethod" }), "older");
check("pageState: no public account", [at({ accounts: 0 }), at({ accounts: 0, accountsKnown: false })], ["noaccounts", "looking"]);
check("pageState: first read on its way", at({ kind: "", answered: false }), "looking");
check("pageState: no answer yet, or kept", [at({ kind: "busy", answered: false }), at({ kind: "busy", answered: true })], ["busy", "list"]);
check("pageState: the module refused", at({ kind: "error" }), "problem");
check("stateText: no Vest", stateText("missing"), "Vest finds schedules that pay you. Install Vest from the app catalog to see them here.");
check("stateText: every state but the list has words", ["network", "missing", "idle", "older", "noaccounts", "looking", "busy", "empty"].every((s) => stateText(s).length > 20), true);
check("stateText: list and problem have none (the list, or the module's words)", [stateText("list"), stateText("problem")], ["", ""]);

// The optional dependency, as lez_bridge's: Names works with no Vest installed.
const meta = JSON.parse(readFileSync(new URL("./metadata.json", import.meta.url), "utf8"));
check("metadata: vesting is optional, never required", [meta.optional_dependencies.includes("vesting"), meta.dependencies.some((d) => d.name === "vesting")], [true, false]);
check("metadata: vesting's lidl", meta.dependency_overrides.vesting, { file: "lidl/vesting.lidl" });
check("metadata: lez_bridge kept", [meta.optional_dependencies.includes("lez_bridge"), meta.dependency_overrides.lez_bridge.file], [true, "lidl/lez_bridge.lidl"]);
const lidl = readFileSync(new URL("./lidl/vesting.lidl", import.meta.url), "utf8");
check("lidl: module vesting", /^module vesting \{/.test(lidl), true);
check("lidl: incoming(accounts_json: tstr) -> tstr", /\n  method incoming\(accounts_json: tstr\) -> tstr description "/.test(lidl), true);
check("lidl: discover(max_blocks: tstr) -> tstr", /\n  method discover\(max_blocks: tstr\) -> tstr description "/.test(lidl), true);
check("lidl: just those two calls", (lidl.match(/\n  method /g) || []).length, 2);

// Main.qml: the menu item, the page, and no reads while the page is closed.
const main = readFileSync(new URL("./qml/Main.qml", import.meta.url), "utf8");
check("Main.qml imports vesting.mjs", /^import "vesting\.mjs" as Vesting$/m.test(main), true);
const tab = main.split("\n").find((l) => /label: "Vesting"/.test(l)) || "";
check("menu: a Vesting tab with the count, shown on every network", [/badge: Vesting\.badgeText\(root\.vestCount\)/.test(tab), /visible:/.test(tab)], [true, false]);
const block = main.slice(main.indexOf("// ── Vesting ──"), main.indexOf("// ── end Vesting\n")) + main.slice(main.indexOf("// ── Vesting page"), main.indexOf("// ── end Vesting page"));
check("Vesting blocks present", block.length > 3000, true);
check("Vesting: no 'lookup' / 'official'", /\blook ?up\b|\bofficial\b/i.test(block.replace(/^\s*\/\/.*$/gm, "")), false);
const texts = [];
for (const m of block.matchAll(/\bText \{/g)) {
  let depth = 0, end = m.index + m[0].length - 1;
  for (; end < block.length; end++) {
    if (block[end] === "{") depth++;
    else if (block[end] === "}" && --depth === 0) break;
  }
  texts.push(block.slice(m.index, end));
}
check("Vesting Text elements found", texts.length >= 1, true);
check("every Vesting Text sets textFormat", texts.filter((body) => !/textFormat:/.test(body)).length, 0);
const timers = [...block.matchAll(/Timer \{[\s\S]*?running: ([^\n]*)/g)].map((m) => m[1]);
check("Vesting timers: the 60 s poll runs only while the page is in view", timers[0], "root.vestOnView");
check("Vesting timers: the count's refresh only while Names is in front", /Qt\.application\.state === Qt\.ApplicationActive/.test(timers[1] || ""), true);
check("vestOnView: the page, LEZ 0.4, Names in front", /readonly property bool vestOnView: section === "vesting" && namesNet === "0\.4" && Qt\.application\.state === Qt\.ApplicationActive/.test(main), true);
check("Open in Vest: Basecamp's launch intent, app vesting_ui", /logos\.request\("basecamp\.apps\.launch", \{ app: "vesting_ui" \}/.test(block), true);
check("every vesting call's argument is text", [...block.matchAll(/vestCall\("(\w+)", \[([^\]]*)\]/g)].map((m) => [m[1], m[2]]), [["incoming", "JSON.stringify(vestAccounts)"], ["discover", '""']]);
check("next unlock kept as a real", /property real vestReadAt/.test(block) && !/property int vest\w*(At|Ms)\b/.test(block), true);

// QML rules (Basecamp's engine): no BigInt, no Java reserved words as names.
const code = readFileSync(new URL("./qml/vesting.mjs", import.meta.url), "utf8");
for (const [label, text] of [["vesting.mjs", code], ["Main.qml Vesting", block]]) {
  check(label + ": no BigInt literals or calls", /\b\d+n\b|\bBigInt\s*\(/.test(text.replace(/^\s*\/\/.*$/gm, "")), false);
  check(label + ": no variables named like Java's reserved words", /\b(?:const|let|var|function|property \w+)\s+(?:short|int|long|byte|char|float|double|boolean|native|package|private|protected|public|static|synchronized|throws|transient|volatile|goto|abstract|final|implements|interface)\b/.test(text), false);
}
check("vesting.mjs: no 'lookup' / 'official'", /\blook ?up\b|\bofficial\b/i.test(code.split("\n").filter((l) => !l.trim().startsWith("//")).join("\n")), false);

console.log(failures === 0 ? "ALL PASS" : failures + " FAILED");
process.exit(failures === 0 ? 0 : 1);
