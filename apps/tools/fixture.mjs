// Writes a fake chain (address -> program state bytes) for rendering the Testimonial app.
// Usage: node fixture.mjs <path to apps/testimonial/src/qml/logic.mjs> <out.json>
const L = await import(process.argv[2]);
import { writeFileSync } from "node:fs";
const u32 = (n) => [n & 255, (n >>> 8) & 255, (n >>> 16) & 255, (n >>> 24) & 255];
const u64 = (n) => u32(n % 2 ** 32).concat(u32(Math.floor(n / 2 ** 32)));
const s = (t) => u32(L.utf8(t).length).concat(L.utf8(t));
const opt = (x, f) => (x === null ? [0] : [1].concat(f(x)));
const acct = (b) => L.unbase58(b);
const encHead = (h) => [1].concat(s(h.submission), u64(h.count), opt(h.newest, acct), u64(h.first_at), u64(h.last_at));
const encT = (t) => [1].concat(s(t.submission), acct(t.author), u64(t.seq), u64(t.at), opt(t.username, s), s(t.text), opt(t.prev, acct));
const program = L.base58(L.sha256(L.utf8("testimonial program")));
const SUB = "lp-0021:names-wallet";
const texts = [
  ["ada", "Connected in one click, approved in Names, posted. No seed phrase pasted anywhere."],
  [null, "The approval sheet told me exactly what the call could touch. That is how wallets should work."],
  ["hopper", "Got test tokens from the faucet app and posted this from a public account in under a minute."],
];
const people = [3, 4, 5].map((b) => L.base58(new Array(32).fill(b)));
const states = {};
let newest = null;
const base = Date.UTC(2026, 9, 28, 14, 0);
texts.forEach(([username, text], seq) => {
  const at = L.nodeAddress(program, SUB, people[seq]);
  states[at] = encT({ submission: SUB, author: people[seq], seq, at: base + seq * 5 * 86400000, username, text, prev: newest });
  newest = at;
});
states[L.headAddress(program, SUB)] = encHead({ submission: SUB, count: 3, newest, first_at: base, last_at: base + 10 * 86400000 });
states[L.CLOCK_ACCOUNT] = u64(1000).concat(u64(Date.UTC(2026, 10, 3, 12, 0)));
writeFileSync(process.argv[3], JSON.stringify({ program, submission: SUB, states, author: "Public/" + people[0] }));
console.log(program);
