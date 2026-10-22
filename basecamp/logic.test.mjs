// Checks the pieces of logic the view keeps for itself, since the QML engine
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
const { atLeast, someAmount, entryShown } = new Function(
  ["atLeast", "someAmount", "entryShown"].map(grab).join("\n") + "\nreturn { atLeast, someAmount, entryShown };",
)();

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
console.log(failures === 0 ? "ALL PASS" : failures + " FAILED");
process.exit(failures ? 1 : 0);
