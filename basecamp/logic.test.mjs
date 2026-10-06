// Pulls the plain-JS helpers out of Main.qml and checks them against known vectors,
// since the QML engine cannot run in CI. Run with: node basecamp/logic.test.mjs
import { readFileSync } from "node:fs";
import { createHash } from "node:crypto";
const qml = readFileSync(new URL("./Main.qml", import.meta.url), "utf8");
function grab(name) {
  const start = qml.indexOf(`    function ${name}(`);
  if (start < 0) throw new Error("missing function " + name);
  let depth = 0, i = qml.indexOf("{", start);
  for (;; i++) { if (qml[i] === "{") depth++; else if (qml[i] === "}" && --depth === 0) break; }
  return qml.slice(start, i + 1);
}
const consts = qml.match(/readonly property var sha256Constants: (\[[\s\S]*?\])/)[1];
const names = ["hexToBytes","bytesToHex","asciiBytes","utf8Decode","sha256","base58Encode","base58Decode","accountHex","normalizeName","isValidName","derivedAddressHex","recordAddressHex","pageAddressHex","directoryAddressHex","reader","decodeRecord","decodePage","decodeDirectory","amountHex"];
const src = `var sha256Constants = ${consts}; var base58Alphabet = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"; var nameSuffix = ".lez"; var programId = "";\n` + names.map(grab).join("\n") + `\nreturn { ${names.join(", ")}, setProgram: function(p){ programId = p } };`;
const q = new Function(src)();
let failures = 0;
const check = (label, got, want) => { const ok = JSON.stringify(got) === JSON.stringify(want); if (!ok) failures++; console.log((ok ? "ok   " : "FAIL ") + label + (ok ? "" : `\n   got  ${JSON.stringify(got)}\n   want ${JSON.stringify(want)}`)); };
for (const m of ["", "abc", "a".repeat(55), "a".repeat(56), "a".repeat(64), "x".repeat(1000)]) {
  check(`sha256(${m.length} bytes)`, q.bytesToHex(q.sha256(q.asciiBytes(m))), createHash("sha256").update(m).digest("hex"));
}
const program = q.base58Encode(new Array(32).fill(7));
check("base58 of [7;32]", program, "US517G5965aydkZ46HS38QLi7UQiSojurfbQfKCELFx");
check("base58 zeros", q.base58Encode(new Array(32).fill(0)), "1".repeat(32));
check("accountHex zeros", q.accountHex("1".repeat(32)), "00".repeat(32));
check("accountHex round trip", q.accountHex("CbgR6tj5kWx5oziiFptM7jMvrQeYY3Mzaao6ciuhSr2r").length, 64);
check("base58 round trip", q.base58Encode(q.hexToBytes(q.accountHex("CbgR6tj5kWx5oziiFptM7jMvrQeYY3Mzaao6ciuhSr2r"))), "CbgR6tj5kWx5oziiFptM7jMvrQeYY3Mzaao6ciuhSr2r");
q.setProgram(program);
check("record address", q.base58Encode(q.hexToBytes(q.recordAddressHex("jef"))), "28mUmKUyH1dJgmwdjVxS22P12iUHVotLcvk2nbrFw29Y");
check("page address", q.base58Encode(q.hexToBytes(q.pageAddressHex("jef"))), "6ckRpxviqmmhY6QkcJPVPWYSSy3eRZGzy1uUGStf1TnC");
check("directory address", q.base58Encode(q.hexToBytes(q.directoryAddressHex())), "9oQ7KANHqw4ok3goEHzgeK7WSxdKqKNXA7bS9HHpq97p");
check("normalize", ["jef.lez", "Jef.LEZ", "  jef  ", "jef.lez.lez"].map(q.normalizeName), ["jef", "jef", "jef", "jef.lez"]);
check("valid", ["jef", "a-b", "-a", "a-", "", "A", "a.b", "a".repeat(33)].map(q.isValidName), [true, true, false, false, false, false, false, false]);
check("amount 1", q.amountHex("1"), "01" + "00".repeat(15));
check("amount 4242", q.amountHex("4242"), "9210" + "00".repeat(14));
check("amount 30000000", q.amountHex("30000000"), "80c3c901" + "00".repeat(12));
check("amount bad", [q.amountHex("0"), q.amountHex("1.5"), q.amountHex(""), q.amountHex("-3")], ["", "", "", ""]);
// A record as the chain stores it.
const str = (s) => { const b = Buffer.from(s, "utf8"); return [b.length, 0, 0, 0, ...b]; };
const rec = [1, ...str("jef"), ...new Array(32).fill(1), 1, ...new Array(32).fill(2), 0, 0, 0, 0, 0, 2, 0, 0, 0, ...str("chat"), ...str("héllo"), ...str("agent"), ...str("x"), 1, ...str("ada")];
const d = q.decodeRecord(rec);
check("decode record", [d.name, d.account === q.base58Encode(new Array(32).fill(2)), d.npk, d.vpk, d.texts, d.previous], ["jef", true, null, "", [{ key: "chat", value: "héllo" }, { key: "agent", value: "x" }], "ada"]);
check("decode directory", q.decodeDirectory([1, 5, 0, 0, 0, 0, 0, 0, 0, 1, ...str("jef")]), { version: 1, count: 5, latest: "jef" });
const pg = q.decodePage([1, ...str("text/html"), 2, 0, 0, 0, 104, 105]);
check("decode page", [pg.contentType, pg.body], ["text/html", [104, 105]]);
console.log(failures === 0 ? "ALL PASS" : failures + " FAILED"); process.exit(failures ? 1 : 0);
