// Checks the one piece of logic the view keeps for itself, since the QML engine
// cannot run in CI. Run with: node basecamp/logic.test.mjs
import { readFileSync } from "node:fs";

const qml = readFileSync(new URL("./Main.qml", import.meta.url), "utf8");
function grab(name) {
  const start = qml.indexOf(`    function ${name}(`);
  if (start < 0) throw new Error("missing function " + name);
  let depth = 0, i = qml.indexOf("{", start);
  for (;; i++) { if (qml[i] === "{") depth++; else if (qml[i] === "}" && --depth === 0) break; }
  return qml.slice(start, i + 1);
}
const { amountHex } = new Function(grab("amountHex") + "\nreturn { amountHex };")();

let failures = 0;
const check = (label, got, want) => {
  const ok = JSON.stringify(got) === JSON.stringify(want);
  if (!ok) failures++;
  console.log((ok ? "ok   " : "FAIL ") + label + (ok ? "" : `\n   got  ${JSON.stringify(got)}\n   want ${JSON.stringify(want)}`));
};
// The wallet takes amounts as 16 bytes, little-endian, in hex.
check("amount 1", amountHex("1"), "01" + "00".repeat(15));
check("amount 4242", amountHex("4242"), "9210" + "00".repeat(14));
check("amount 30000000", amountHex("30000000"), "80c3c901" + "00".repeat(12));
check("amount 999999999999999", amountHex("999999999999999"), "ff7fc6a47e8d03" + "00".repeat(9));
check("rejected amounts", ["0", "1.5", "", "-3", "1e3", " "].map(amountHex), ["", "", "", "", "", ""]);
console.log(failures === 0 ? "ALL PASS" : failures + " FAILED");
process.exit(failures ? 1 : 0);
