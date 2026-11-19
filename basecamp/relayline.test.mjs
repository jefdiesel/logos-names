// SPDX-License-Identifier: MIT OR Apache-2.0
// Checks qml/relayline.mjs (which relay sent a claim or a burn). Run with: node basecamp/relayline.test.mjs
import { relayUsed, relayUsedLine } from "./qml/relayline.mjs";

let failures = 0;
const check = (label, got, want) => {
  const ok = JSON.stringify(got) === JSON.stringify(want);
  if (!ok) failures++;
  console.log((ok ? "ok   " : "FAIL ") + label + (ok ? "" : `\n   got  ${JSON.stringify(got)}\n   want ${JSON.stringify(want)}`));
};
const listed = { url: "https://relay-b.example/rpc", label: "Relay B", own: false, payer: "Public/1HHhGGKfxtpAL5Uno5QqowGy748EUkYS6QGLFgQMgNT" };
check("a listed relay is named", relayUsed(listed), "Relay B");
check("its line", relayUsedLine(listed), " Sent by Relay B.");
check("the bridge's own relay: nothing new", [relayUsed({ ...listed, own: true }), relayUsedLine({ ...listed, own: true })], ["", ""]);
check("an older module (no relay): nothing", [relayUsed(undefined), relayUsed(null), relayUsedLine({})], ["", "", ""]);
check("own missing is not a listed relay", relayUsed({ label: "x" }), "");
check("a long or spaced label is kept to one short line", relayUsed({ own: false, label: "  A\nrelay  " + "x".repeat(80) }).length, 60);
check("no label: nothing", relayUsedLine({ own: false, label: "" }), "");
if (failures) {
  console.log(failures + " failed");
  process.exit(1);
}
console.log("all passed");
