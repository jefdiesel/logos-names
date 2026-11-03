// Checks the Faucet mini-app's own logic (src/qml/logic.mjs, which src/qml/Main.qml
// imports), since the QML engine cannot run in CI. Run with: node apps/faucet/logic.test.mjs
//
// The answers below are the ones chain/faucet.py (the Names faucet) gives.
import * as L from "./src/qml/logic.mjs";
import { readFileSync } from "node:fs";

let failures = 0;
const check = (label, got, want) => {
  const ok = JSON.stringify(got) === JSON.stringify(want);
  if (!ok) failures++;
  console.log((ok ? "ok   " : "FAIL ") + label + (ok ? "" : `\n   got  ${JSON.stringify(got)}\n   want ${JSON.stringify(want)}`));
};
const NOW = Date.UTC(2026, 10, 3, 12, 0, 0);
const MIN = 60000, DAY = 86400000;
const ID = "CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk";

// ── Which faucet may be contacted ──────────────────────────────────────────────────
check("urls allowed", [L.DEFAULT_FAUCET_URL, "https://faucet.example/x?y=1", "http://127.0.0.1:3041/faucet", "http://localhost:3041/faucet", "http://[::1]:8/f"].map(L.urlProblem), ["", "", "", "", ""]);
check("urls refused", ["http://faucet.example/", "ftp://x/", "https://", "https://user@evil/", "javascript:alert(1)", "", "https://a b/"].map((u) => L.urlProblem(u) !== ""), [true, true, true, true, true, true, true]);
check("hostOf", [L.hostOf(L.DEFAULT_FAUCET_URL), L.hostOf("http://127.0.0.1:3041/faucet")], ["lez.buggers.online", "127.0.0.1"]);

// ── Which accounts it can pay ──────────────────────────────────────────────────────
check("fundable: public only", L.fundable([{ account: "Public/" + ID, account_kind: "public" }, { account: "Private/" + ID, account_kind: "private" }, { account: "Public/bad!" }]), ["Public/" + ID]);
check("bare", L.bare("Public/" + ID), ID);

// ── Answers ────────────────────────────────────────────────────────────────────────
const sent = L.interpret({ status: 200, body: { account: ID, amount: "2000000", tx: "ab".repeat(32) }, retry_after: "" }, NOW, 0);
check("200: sent", [sent.ok, sent.amount, sent.tx, sent.message], [true, "2000000", "ab".repeat(32), "The faucet sent 2,000,000 LEZ. It arrives within a block or two."]);
check("200 without tx is not success", L.interpret({ status: 200, body: {} }, NOW, 0).ok, false);
const today = L.interpret({ status: 429, body: { error: "this account was funded in the last day" } }, NOW, NOW - 3 * 3600000);
check("429 account: retry a day after the last grant this app saw", [today.ok, today.retryAt, today.final, /once a day/.test(today.message)], [false, NOW + 21 * 3600000, false, true]);
check("429 account: retry text names the hour", /^in 21 h/.test(today.retryText), true);
const unknown = L.interpret({ status: 429, body: { error: "this account was funded in the last day" } }, NOW, 0);
check("429 account, grant unknown: says so", [unknown.retryAt, unknown.retryText], [0, "about a day after its last grant"]);
const address = L.interpret({ status: 429, body: { error: "too many requests from your address today" } }, NOW, 0);
check("429 address", [/internet address/.test(address.message), address.retryText], [true, "about a day after that grant"]);
const spent = L.interpret({ status: 429, body: { error: "the faucet's daily limit is spent; try tomorrow" } }, NOW, 0);
check("429 daily limit", [/all it gives in a day/.test(spent.message), spent.retryText], [true, "in a few hours, or tomorrow"]);
const header = L.interpret({ status: 429, body: { error: "slow down" }, retry_after: "120" }, NOW, 0);
check("429 with Retry-After seconds", [header.retryAt, header.retryText], [NOW + 120000, "in 2 min"]);
const dated = L.interpret({ status: 503, body: null, retry_after: new Date(NOW + 30000).toUTCString() }, NOW, 0);
check("503 with Retry-After date", dated.retryAt, NOW + 30000);
const busy = L.interpret({ status: 409, body: { error: "the faucet is busy sending; try again in a minute" } }, NOW, 0);
check("409 busy: a minute", [busy.retryAt, busy.retryText, busy.message], [NOW + MIN, "in 1 min", "The faucet is busy sending; try again in a minute."]);
const empty = L.interpret({ status: 409, body: { error: "the faucet is empty until its next refill; you can bridge tokens from the Logos Blockchain instead" } }, NOW, 0);
check("409 empty: final, suggests bridging", [empty.final, empty.retryAt, /bridge/.test(empty.message)], [true, 0, true]);
check("422: a minute", L.interpret({ status: 422, body: { error: "the transfer could not be sent; try again shortly" } }, NOW, 0).retryAt, NOW + MIN);
check("400: final", L.interpret({ status: 400, body: { error: "that is not a public account ID" } }, NOW, 0).final, true);
check("404: no faucet", L.interpret({ status: 404, body: { error: "not found" } }, NOW, 0).message, "There is no faucet at that address.");
const down = L.interpret({ status: 0, error: "Connection refused" }, NOW, 0);
check("unreachable", [down.ok, down.retryAt, /Connection refused/.test(down.message)], [false, NOW + MIN, true]);
check("garbage reply", L.interpret(null, NOW, 0).ok, false);
check("describe", [L.describe({ status: 200, body: { amount: "2000000", per_account: "once a day" } }), L.describe({ status: 404, body: {} })], [{ amount: "2000000", per_account: "once a day" }, null]);

// ── Waiting, balances ──────────────────────────────────────────────────────────────
check("waitText", [L.waitText(0, NOW), L.waitText(1500, NOW), L.waitText(90000, NOW), /^in 5 h 1 min \(at \d\d:\d\d\)$/.test(L.waitText(5 * 3600000 + 30000, NOW + 5 * 3600000))], ["now", "in 2 s", "in 2 min", true]);
check("above", [L.above("2000001", "2000000"), L.above("2000000", "2000000"), L.above("100000000000000000000", "99999999999999999999"), L.above("9", "10")], [true, false, true, false]);
check("grouped", [L.grouped("2000000"), L.grouped("12"), L.grouped("x")], ["2,000,000", "12", "x"]);
check("balanceOf", [L.balanceOf([{ account: "Public/" + ID, balance: "5" }], "Public/" + ID), L.balanceOf([], "Public/" + ID)], ["5", null]);

// ── QML hygiene (as basecamp/logic.test.mjs) ───────────────────────────────────────
const reserved = /\.(public|private|protected|package|interface|implements|static|let|yield|enum|class|const|export|import|extends|super)\b/;
for (const file of ["./src/qml/Main.qml", "./src/qml/logic.mjs"]) {
  const source = readFileSync(new URL(file, import.meta.url), "utf8").split("\n");
  const dotted = source
    .map((line, i) => [i + 1, line.replace(/"(\\.|[^"\\])*"|'(\\.|[^'\\])*'|\/\/.*$/g, '""')])
    .filter(([, line]) => reserved.test(line))
    .map(([n]) => n);
  check(`no reserved word after a dot in ${file}`, dotted, []);
}
console.log(failures === 0 ? "ALL PASS" : failures + " FAILED");
process.exit(failures ? 1 : 0);
