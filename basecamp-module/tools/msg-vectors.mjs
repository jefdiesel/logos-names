// Makes basecamp-module/rust-lib/testdata/msg-v1-vectors.json: Names Messages version 1 as the phone app makes it,
// with its randomness fixed, so the module's Rust port (rust-lib/src/messages/) can be checked byte for byte.
//
//   node basecamp-module/tools/msg-vectors.mjs <logos-node-android>/app/messages > basecamp-module/rust-lib/testdata/msg-v1-vectors.json
//
// It runs the phone's own files (msg-crypto.js sealMessage, chat-core.js sealPadded and safetyCode, the vendored
// noble libraries) with the ephemeral secret and the nonce of each message taken from a fixed list, and records
// both, so a reader can seal the same message again and compare every byte. The phone's own vectors
// (tests/msg_vectors.json, made by the names project) are copied in as `phone` for opening.
import { readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { createHash } from "node:crypto";

const dir = resolve(process.argv[2] || "");
const imp = (p) => import(pathToFileURL(join(dir, p)));
const { x25519 } = await imp("../noble-curves/ed25519.js");
const MC = await imp("msg-crypto.js");
const CORE = await imp("chat-core.js");
const hex = (b) => Buffer.from(b).toString("hex");
const unhex = (h) => new Uint8Array(Buffer.from(h, "hex"));
const det = (label) => new Uint8Array(createHash("sha256").update(label).digest());   // fixed "random" bytes

// the randomness the sealers ask for, fixed: first the ephemeral secret (x25519.utils.randomSecretKey), then the
// nonce (crypto.getRandomValues)
// (both come from crypto.getRandomValues: 32 bytes for the secret, 24 for the nonce; each vector checks its
// ephemeral public key is the one of the secret it records)
let nextEph = null, nextNonce = null;
const realGet = crypto.getRandomValues.bind(crypto);
crypto.getRandomValues = (a) => {
  if (a.length === 32 && nextEph) { a.set(nextEph); nextEph = null; return a; }
  if (a.length === 24 && nextNonce) { a.set(nextNonce); nextNonce = null; return a; }
  return realGet(a);
};
const ephOk = (payload, eph) => hex(payload.slice(1, 33)) === hex(x25519.getPublicKey(eph));

const keys = {};
for (const who of ["alice", "bob", "carol"]) {
  const secret = det("names-msg-vectors/key/" + who);
  const pub = x25519.getPublicKey(secret);
  keys[who] = { secret: hex(secret), public: hex(pub), topic: MC.inboxTopic(pub) };
}

const texts = [
  ["ascii", "hello bob, from the phone"],
  ["unicode", "Grüße zurück — 你好 ✓"],
  ["escapes", "quote \" backslash \\ slash / newline \n tab \t cr \r nul \u0000 bell \u0007 del \u007f ls  "],
  ["emoji", "😀👍🏽 family 👨‍👩‍👧"],
  ["max-ascii", "x".repeat(2000)],
  ["mid-2byte", "é".repeat(800)],                         // 1,600 bytes: the 2048 class
  ["max-3byte", "語".repeat(2000)],                       // 6,000 bytes: the 8192 class
  ["max-escaped", "\u0001".repeat(2000)],                  // \u0001 is 6 bytes in JSON: 12,000 bytes, the 15872 class
  ["one", "a"],
];
const seal = [], padded = [];
let n = 0;
for (const [label, text] of texts) {
  for (const [from, to] of [["alice", "bob"], ["bob", "alice"]]) {
    const ts = 1794489600000 + 61000 * n;
    const eph = det(`names-msg-vectors/eph/${n}`), nonce = det(`names-msg-vectors/nonce/${n}`).slice(0, 24);
    const args = { to: unhex(keys[to].public), from, fromSecret: unhex(keys[from].secret), text, ts };
    nextEph = eph; nextNonce = nonce;
    const plain = MC.sealMessage(args);
    nextEph = eph; nextNonce = nonce;
    const pad = CORE.sealPadded(args);
    if (nextEph || nextNonce || !ephOk(plain, eph) || !ephOk(pad, eph) || hex(pad.slice(33, 57)) !== hex(nonce)) throw new Error("randomness not as fixed: " + label);
    const opened = MC.openMessage(unhex(keys[to].secret), pad);
    if (!opened || !opened.authentic || opened.text !== text) throw new Error("padded vector doesn't open: " + label);
    const v = { label, from, to, text, ts, eph_secret: hex(eph), nonce: hex(nonce) };
    seal.push({ ...v, payload: hex(plain) });
    padded.push({ ...v, payload: hex(pad), plaintext_len: pad.length - 57 - 16 });
    n++;
  }
}
// a sender with no name: `from` is msg-<first 16 hex of its key>
{
  const ts = 1794489600000 + 61000 * n;
  const eph = det(`names-msg-vectors/eph/${n}`), nonce = det(`names-msg-vectors/nonce/${n}`).slice(0, 24);
  const from = CORE.nameless(keys.carol.public);
  nextEph = eph; nextNonce = nonce;
  const pad = CORE.sealPadded({ to: unhex(keys.alice.public), from, fromSecret: unhex(keys.carol.secret), text: "no name yet", ts });
  if (!ephOk(pad, eph)) throw new Error("randomness not as fixed: nameless");
  padded.push({ label: "nameless", from, to: "alice", sender: "carol", text: "no name yet", ts, eph_secret: hex(eph), nonce: hex(nonce), payload: hex(pad), plaintext_len: pad.length - 73 });
}
crypto.getRandomValues = realGet;

const safety = [];
const pairs = [["alice", "bob"], ["bob", "alice"], ["alice", "carol"], ["bob", "carol"]];
for (const [a, b] of pairs) safety.push({ a: keys[a].public, b: keys[b].public, code: CORE.safetyCode(keys[a].public, keys[b].public) });

const phone = JSON.parse(readFileSync(join(dir, "..", "..", "tests", "msg_vectors.json"), "utf8"));
process.stdout.write(JSON.stringify({
  made_by: "basecamp-module/tools/msg-vectors.mjs, from the phone app's app/messages (logos-node-android)",
  pad_classes: CORE.PAD_CLASSES, keys, seal, padded, safety, phone,
}, null, 1) + "\n");
