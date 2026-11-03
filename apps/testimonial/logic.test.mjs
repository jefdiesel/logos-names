// Checks the Testimonial mini-app's own logic (src/qml/logic.mjs, which src/qml/Main.qml
// imports) against the program's Rust encoding, since the QML engine cannot run in CI.
// Run with: node apps/testimonial/logic.test.mjs
//
// test/vectors.json is written by testimonial/core/tests/js_vectors.rs (and checked there),
// so both languages are held to the same bytes.
import * as L from "./src/qml/logic.mjs";
import { readFileSync } from "node:fs";
import { createHash, randomBytes } from "node:crypto";

let failures = 0;
const check = (label, got, want) => {
  const ok = JSON.stringify(got) === JSON.stringify(want);
  if (!ok) failures++;
  console.log((ok ? "ok   " : "FAIL ") + label + (ok ? "" : `\n   got  ${JSON.stringify(got)}\n   want ${JSON.stringify(want)}`));
};
const throws = (f) => { try { f(); return false; } catch (e) { return true; } };

// ── Primitives against Node's own ───────────────────────────────────────────────────
for (const n of [0, 1, 55, 56, 63, 64, 65, 119, 120, 1000]) {
  const bytes = [...randomBytes(n)];
  check(`sha256 of ${n} random bytes`, L.hex(L.sha256(bytes)), createHash("sha256").update(Buffer.from(bytes)).digest("hex"));
}
const words = "Ünïcode ✓ ⚓ 𝄞 emoji 😀 and plain";
check("utf8 matches Buffer", L.utf8(words), [...Buffer.from(words, "utf8")]);
check("utf8 round trip", L.fromUtf8(L.utf8(words)), words);
check("utf8 refuses a bad byte", throws(() => L.fromUtf8([0xff])), true);
check("utf8 refuses a cut sequence", throws(() => L.fromUtf8([0xe2, 0x9c])), true);
check("base58 of zeros", L.base58(new Array(32).fill(0)), "1".repeat(32));
check("base58 round trip with leading zeros", L.unbase58(L.base58([0, 0, 5, 255, 1])), [0, 0, 5, 255, 1]);
for (let i = 0; i < 20; i++) {
  const bytes = [...randomBytes(32)];
  if (i < 3) bytes[0] = 0;
  const text = L.base58(bytes);
  if (JSON.stringify(L.unbase58(text)) !== JSON.stringify(bytes)) { failures++; console.log("FAIL base58 round trip " + text); }
}
check("base58 refuses 0OIl", [throws(() => L.unbase58("0")), throws(() => L.unbase58("O")), throws(() => L.unbase58("l"))], [true, true, true]);
check("accountBytes takes Public/ and refuses Private/", [L.accountBytes("Public/" + L.base58(new Array(32).fill(9))).length, throws(() => L.accountBytes("Private/" + L.base58(new Array(32).fill(9))))], [32, true]);
check("accountBytes refuses a short ID", throws(() => L.accountBytes("abc")), true);

// ── The program's encoding (vectors from Rust) ──────────────────────────────────────
const vectors = JSON.parse(readFileSync(new URL("./test/vectors.json", import.meta.url), "utf8"));
check("clock program ID", L.CLOCK_PROGRAM, vectors.clock.program);
check("clock account ID", L.CLOCK_ACCOUNT, vectors.clock.account);
check("clock IDs derive as in Rust", [
  L.base58(L.sha256(L.utf8("/LEE-BuiltinProgram/v1/AccountId").concat(L.utf8("clock")))),
  L.base58(L.utf8("/LEZ/ClockProgramAccount/0000001")),
], [vectors.clock.program, vectors.clock.account]);
for (const [i, v] of vectors.cases.entries()) {
  const call = L.postCall(v.program, v.submission, v.username, v.text, v.now, "Public/" + v.author);
  check(`case ${i}: message`, call.instruction_hex, v.message_hex);
  check(`case ${i}: accounts_json`, call.accounts_json, v.accounts_json);
  check(`case ${i}: seeds`, [L.hex(L.headSeed(v.submission)), L.hex(L.nodeSeed(v.submission, L.accountBytes(v.author)))], [v.head_seed_hex, v.node_seed_hex]);
  check(`case ${i}: addresses`, [call.head, call.node, L.headAddress(v.program, v.submission), L.nodeAddress(v.program, v.submission, v.author)], [v.head, v.node, v.head, v.node]);
  check(`case ${i}: program written back`, call.program, v.program);
  const head = L.decodeHead(L.unhex(v.head_record_hex));
  check(`case ${i}: head decodes`, head, { schema: 1, submission: v.submission, count: 3, newest: v.node, first_at: Math.max(0, v.now - 86400000), last_at: v.now });
  const t = L.decodeTestimonial(L.unhex(v.testimonial_record_hex));
  check(`case ${i}: testimonial decodes`, t, { schema: 1, submission: v.submission, author: v.author, seq: 2, at: v.now, username: v.username, text: v.text, prev: v.head });
  check(`case ${i}: trailing bytes refused`, throws(() => L.decodeHead(L.unhex(v.head_record_hex + "00"))), true);
}
// An empty username is "none", like the client's missing --username.
const v0 = vectors.cases[0];
check("empty username is None", L.hex(L.postMessage(v0.submission, "", "x", 5)), L.hex(L.postMessage(v0.submission, null, "x", 5)));
check("now past 2^53 refused", throws(() => L.postMessage("a", null, "x", 2 ** 53)), true);

// ── The program's limits ───────────────────────────────────────────────────────────
check("submission ok", ["lp-0021:names-wallet", "lp-0021/names-wallet", "a".repeat(64)].map(L.submissionProblem), ["", "", ""]);
check("submission refused", ["", "LP-0021", "-lp", "lp 0021", "lp#1", "a".repeat(65)].map((s) => L.submissionProblem(s) !== ""), [true, true, true, true, true, true]);
check("username ok", ["", "alice", "Ünïcode name", "x".repeat(32)].map(L.usernameProblem), ["", "", "", ""]);
check("username refused", [" alice", "alice ", "a\u0007b", "x".repeat(33), "é".repeat(17), "a\u00a0"].map((s) => L.usernameProblem(s) !== ""), [true, true, true, true, true, true]);
check("text ok", ["line one\nline two\ttabbed", "x".repeat(1024), "  padded  "].map(L.textProblem), ["", "", ""]);
check("text refused", ["", "   \n", "bell\u0007", "cr\r", "x".repeat(1025), "\u0085"].map((s) => L.textProblem(s) !== ""), [true, true, true, true, true, true]);

// ── Reading a list back ─────────────────────────────────────────────────────────────
// Test-only encoders, checked against the Rust records first.
const u32 = (n) => [n & 255, (n >>> 8) & 255, (n >>> 16) & 255, (n >>> 24) & 255];
const u64 = (n) => u32(n % 2 ** 32).concat(u32(Math.floor(n / 2 ** 32)));
const s = (t) => u32(L.utf8(t).length).concat(L.utf8(t));
const opt = (x, f) => (x === null ? [0] : [1].concat(f(x)));
const acct = (b58) => L.unbase58(b58);
const encHead = (h) => [1].concat(s(h.submission), u64(h.count), opt(h.newest, acct), u64(h.first_at), u64(h.last_at));
const encT = (t) => [1].concat(s(t.submission), acct(t.author), u64(t.seq), u64(t.at), opt(t.username, s), s(t.text), opt(t.prev, acct));
check("test encoder matches Rust (head)", L.hex(encHead(L.decodeHead(L.unhex(v0.head_record_hex)))), v0.head_record_hex);
check("test encoder matches Rust (testimonial)", L.hex(encT(L.decodeTestimonial(L.unhex(v0.testimonial_record_hex)))), v0.testimonial_record_hex);

const PROGRAM = v0.program, SUB = "lp-0021:names-wallet";
const DEC = Date.UTC(2026, 11, 11), JAN = Date.UTC(2027, 0, 15);
function chain(authors, times) {
  const states = {};
  let newest = null;
  authors.forEach((author, seq) => {
    const at = L.nodeAddress(PROGRAM, SUB, author);
    states[at] = encT({ submission: SUB, author, seq, at: times[seq], username: seq % 2 ? null : "u" + seq, text: "vouch " + seq, prev: newest });
    newest = at;
  });
  states[L.headAddress(PROGRAM, SUB)] = encHead({ submission: SUB, count: authors.length, newest, first_at: times[0], last_at: times[times.length - 1] });
  return states;
}
const reader = (states) => (account, cb) => cb(null, states[account] || []);
const walked = (states, sub = SUB) => { let out; L.walk(PROGRAM, sub, reader(states), null, (e, l) => { out = { e, l }; }); return out; };
const people = [1, 2, 3].map((b) => L.base58(new Array(32).fill(b)));
const three = chain(people, [DEC, DEC + 1, JAN]);
const good = walked(three);
check("walk: newest first", [good.e, good.l.items.map((t) => [t.seq, t.author])], [null, [[2, people[2]], [1, people[1]], [0, people[0]]]]);
check("walk: addresses kept", good.l.items[0].address, L.nodeAddress(PROGRAM, SUB, people[2]));
check("walk: empty submission", [walked({}).e, walked({}).l.items.length, walked({}).l.head], [null, 0, null]);
const sum = L.summarize(good.l.items);
check("summary", [sum.total, sum.distinct_authors, sum.months, sum.meets_adoption],
  [3, 3, [{ month: "2026-12", count: 2, qualifies: false }, { month: "2027-01", count: 1, qualifies: false }], false]);
// Tampering is caught, as in testimonial_core::walk.
const moved = { ...three, [L.nodeAddress(PROGRAM, SUB, people[2])]: three[L.nodeAddress(PROGRAM, SUB, people[1])] };
check("walk: a record at another author's address", /not the testimonial account|number/.test(walked(moved).e || ""), true);
const over = { ...three, [L.headAddress(PROGRAM, SUB)]: encHead({ submission: SUB, count: 4, newest: L.nodeAddress(PROGRAM, SUB, people[2]), first_at: DEC, last_at: JAN }) };
check("walk: a head that overcounts", walked(over).e !== null, true);
check("walk: a bad submission id", walked(three, "BAD").e !== null, true);
let failedRead;
L.walk(PROGRAM, SUB, (a, cb) => cb("offline"), null, (e) => { failedRead = e; });
check("walk: a failed read is reported", /offline/.test(failedRead), true);
// The adoption bar over 150 posts in two months.
const many = Array.from({ length: 150 }, (_, i) => L.base58(new Array(32).fill(0).map((_, j) => (j === 0 ? 1 : (i * 7 + j) & 255))));
const big = walked(chain(many, many.map((_, i) => (i < 75 ? DEC + i : JAN + i))));
check("walk + summarize: 150 over two months meets the bar", [big.e, L.summarize(big.l.items).meets_adoption, L.summarize(big.l.items.slice(1)).meets_adoption], [null, true, false]);

// ── Reads and time ─────────────────────────────────────────────────────────────────
check("actorState", L.actorState({ nonce: 1, data: { actor_states: { [PROGRAM]: [1, 2, 3] } } }, PROGRAM), [1, 2, 3]);
check("actorState none", [L.actorState(null, PROGRAM), L.actorState({ data: { actor_states: {} } }, PROGRAM)], [[], []]);
check("actorState refuses junk", throws(() => L.actorState({ data: { actor_states: { [PROGRAM]: [1, 300] } } }, PROGRAM)), true);
check("clockTime", L.clockTime(u64(42).concat(u64(1800000000000))), 1800000000000);
check("postTime prefers the chain", [L.postTime(1800000000000, 1900000000000), L.postTime(0, 1900000000000)], [1800000000000, 1899999940000]);
check("accountViewRequest", JSON.parse(L.accountViewRequest("A", "B")).params, [{ account_id: "A", program_account_id: "B" }]);
check("months", [L.monthOf(0), L.monthOf(1798761599999), L.monthOf(1798761600000), L.formatTime(1800000000000)], ["1970-01", "2026-12", "2027-01", "2027-01-15 08:00 UTC"]);
check("postingAccounts keeps public ones", L.postingAccounts([{ account: "Public/abc", account_kind: "public" }, { account: "Private/def", account_kind: "private" }]), ["Public/abc"]);
check("postSummary fits 280", L.postSummary(SUB, "Public/" + people[0], "x".repeat(2000)).length <= 280, true);

// ── QML hygiene ────────────────────────────────────────────────────────────────────
// Qt's QML parser on Linux refuses `x.public` ("Expected token 'identifier'") and the whole
// app then fails to load, so reserved words are read as x["public"]. Strings are blanked
// first, so a word inside text doesn't count. (Same check as basecamp/logic.test.mjs.)
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
