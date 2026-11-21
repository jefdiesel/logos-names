// The Names Wallet SDK (qml/NamesWalletSDK.mjs) against a stand-in names module.
// Run with: node --test sdk/NamesWalletSDK.test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import * as SDK from "./qml/NamesWalletSDK.mjs";

const ok = (value) => JSON.stringify({ success: true, value, error: null });
const no = (error) => JSON.stringify({ success: false, value: null, error });
const BUSY = JSON.stringify({ success: false, busy: true, value: null, error: "The wallet is busy; still working." });
const CHAIN = "2d".repeat(32);
const CONFIG = { zone: { id: "home", channel: CHAIN, home: true }, features: ["call_reads_clock", "request_access_with"] };
const GRANTED = { connected: true, accounts: [{ account: "Public/A1", account_kind: "public" }] };

const NAMES_ROW = { name: "names", moduleName: "names", type: "core", category: "identity", displayName: "Names", version: "0.18.0" };

/// A stand-in host: `answers[method]` is a reply text, a function(args, n, module) giving one,
/// or a list used in turn (the last repeats). Basecamp's package list is `options.installed`
/// (default: Names only), or `options.packages` (a raw reply). Timers run when `tick()` is
/// called; the wallet's calls are recorded in `calls` ([method, args]), every call in `all`.
function harness(answers, options) {
  const o = options || {};
  const calls = [], all = [], timers = [], opened = [], changes = [], kept = {};
  const counts = {};
  const host = {
    call(module, method, args, callback) {
      if (o.noBridge) return false;
      all.push([module, method, args]);
      if (module === "package_manager") {
        const reply = o.packages !== undefined ? o.packages : JSON.stringify(o.installed || [NAMES_ROW]);
        queueMicrotask(() => callback(reply));
        return true;
      }
      calls.push([method, args]);
      if (o.byModule) { const r = o.byModule(module, method, args); queueMicrotask(() => callback(r)); return true; }
      counts[method] = (counts[method] || 0) + 1;
      let a = answers[method];
      if (Array.isArray(a)) a = a[Math.min(counts[method] - 1, a.length - 1)];
      if (typeof a === "function") a = a(args, counts[method], module);
      if (a === undefined) a = no("not in this test");
      queueMicrotask(() => callback(a));
      return true;
    },
    later(f, ms) { timers.push({ f, ms }); },
    open(app) { opened.push(app); },
    recall(key) { return kept[key] || ""; },
    remember(key, value) { kept[key] = value; },
    changed(w) { changes.push(w.state); },
  };
  if (o.kept) Object.assign(kept, o.kept);
  const wallet = SDK.create(host);
  const settle = () => new Promise((r) => setTimeout(r, 0));
  async function tick(n = 1) {
    for (let i = 0; i < n; i++) {
      await settle();
      const due = timers.splice(0, timers.length);
      due.forEach((t) => t.f());
      await settle();
    }
  }
  return { wallet, calls, all, timers, opened, changes, kept, tick, settle };
}

test("VERSION is semver text", () => assert.match(SDK.VERSION, /^\d+\.\d+\.\d+$/));

test("reply: value, busy, refusal, missing, unclear", () => {
  assert.deepEqual(SDK.reply(ok({ a: 1 })), { ok: true, value: { a: 1 } });
  assert.deepEqual(SDK.reply(JSON.stringify(ok(3))), { ok: true, value: 3 }, "encoded twice");
  assert.equal(SDK.reply(BUSY).busy, true);
  assert.equal(SDK.reply({ success: false, value: null, error: "ask for access first" }).error, "ask for access first");
  assert.equal(SDK.reply("Module not connected: names").missing, true);
  assert.equal(SDK.reply("").unclear, true);
  assert.equal(SDK.reply(undefined).unclear, true);
  assert.equal(SDK.reply("<html>").unclear, true);
  assert.equal(SDK.reply("<html>").error, SDK.WORDS.restarting);
});

test("every argument goes as text (Basecamp refuses a QML number for an int)", async () => {
  const h = harness({ propose_transfer: ok({ request: "r1", state: "pending" }) });
  h.wallet.proposeTransfer("LEZ", 1000, "alice", {});
  h.wallet.call("anything", [1, true, null, { a: 1 }]);
  await h.settle();
  for (const [, args] of h.calls) assert.ok(args.every((a) => typeof a === "string"), JSON.stringify(args));
  assert.deepEqual(h.calls[0][1], ["LEZ", "1000", "alice"]);
  assert.deepEqual(h.calls[1][1], ["1", "true", "", '{"a":1}']);
});

test("busy is asked again, one timer per call, each wait longer", async () => {
  const h = harness({ granted_accounts: [BUSY, BUSY, ok(GRANTED)], get_config: [BUSY, ok(CONFIG)] });
  let a = null, b = null;
  h.wallet.call("granted_accounts", [], (v) => (a = v));
  h.wallet.call("get_config", [], (v) => (b = v));
  await h.settle();
  assert.equal(h.timers.length, 2, "two calls retrying at once each keep their own timer");
  assert.deepEqual(h.timers.map((t) => t.ms), [SDK.RETRY_MS, SDK.RETRY_MS]);
  await h.tick();
  assert.ok(b && b.zone, "get_config came back");
  assert.equal(h.timers[0].ms, SDK.RETRY_MS * 2, "the second wait is longer");
  await h.tick();
  assert.equal(a.connected, true);
});

test("busy gives up after RETRIES, saying it plainly", async () => {
  const h = harness({ granted_accounts: BUSY });
  let failed = null;
  h.wallet.call("granted_accounts", [], () => {}, (r) => (failed = r));
  await h.tick(SDK.RETRIES + 1);
  assert.equal(h.calls.length, SDK.RETRIES + 1);
  assert.equal(failed.busy, true);
  assert.equal(failed.error, SDK.WORDS.busy);
});

test("an unclear answer is asked again for a read, never for a proposal", async () => {
  const h = harness({ request_status: ["", ok({ request: "r1", state: "done" })], propose_call: "" });
  let status = null, failed = null;
  h.wallet.call("request_status", ["r1"], (v) => (status = v));
  h.wallet.call("propose_call", ["P", "[]", "00", "s"], () => assert.fail("no value"), (r) => (failed = r));
  await h.tick(2);
  assert.equal(status.state, "done");
  assert.equal(h.calls.filter(([m]) => m === "propose_call").length, 1, "a proposal is sent once");
  assert.equal(failed.uncertain, true);
  assert.equal(failed.error, SDK.WORDS.uncertain);
});

test("busy on a proposal is asked again (busy promises nothing was taken)", async () => {
  const h = harness({ propose_call: [BUSY, ok({ request: "r9", state: "pending" })], request_status: ok({ request: "r9", state: "pending" }) });
  let sent = null;
  h.wallet.proposeCall("P", [{ seed: "00" }], "00", "s", { sent: (v) => (sent = v) });
  await h.tick();
  assert.equal(sent.request, "r9");
  assert.equal(h.calls[0][1][1], '[{"seed":"00"}]', "accounts given as a list go as JSON text");
});

test("Names missing: the state says so, in plain words", async () => {
  const h = harness({ get_config: "Module not connected: names" });
  let w = null;
  h.wallet.check({}, (x) => (w = x));
  await h.settle();
  assert.equal(w.state, "missing");
  assert.equal(w.problem, SDK.WORDS.missing);
  const none = harness({}, { noBridge: true });
  let r = null;
  none.wallet.call("get_config", [], () => {}, (x) => (r = x));
  assert.equal(none.wallet.state, "missing");
  assert.equal(r.error, SDK.WORDS.noBridge);
});

test("check: features, zone, accounts", async () => {
  const h = harness({ get_config: ok(CONFIG), granted_accounts: ok(GRANTED) });
  let w = null;
  h.wallet.check({ chain: CHAIN.toUpperCase() }, (x) => (w = x));
  await h.settle(); await h.settle();
  assert.equal(w.state, "connected");
  assert.deepEqual(w.accounts, GRANTED.accounts);
  assert.equal(w.has("call_reads_clock"), true);
  assert.equal(w.has("nothing"), false);
  const other = harness({ get_config: ok({ zone: { channel: "01".repeat(32) }, features: [] }) });
  let o = null;
  other.wallet.check({ chain: CHAIN }, (x) => (o = x));
  await other.settle();
  assert.equal(o.state, "otherZone");
  assert.equal(o.problem, SDK.WORDS.otherZone);
  assert.equal(other.calls.length, 1, "no account read on another zone");
  const none = harness({ get_config: ok(CONFIG), granted_accounts: ok({ connected: false, accounts: [] }) });
  let n = null;
  none.wallet.check({}, (x) => (n = x));
  await none.settle(); await none.settle();
  assert.equal(n.state, "disconnected");
});

test("zoneMatches: unknown on either side is not a problem", () => {
  assert.equal(SDK.zoneMatches({ zone: { channel: "AB" } }, "ab"), true);
  assert.equal(SDK.zoneMatches({ zone: { channel: "01" } }, "ab"), false);
  assert.equal(SDK.zoneMatches({}, "ab"), true);
  assert.equal(SDK.zoneMatches({ zone: { channel: "01" } }, ""), true);
});

test("connect: asks, opens Names, waits, then reads the accounts", async () => {
  const h = harness({
    request_access: ok({ request: "c1", kind: "connect", state: "pending" }),
    request_status: [ok({ request: "c1", state: "pending" }), ok({ request: "c1", state: "approved", accounts: ["Public/A1"] })],
    get_config: ok(CONFIG), granted_accounts: ok(GRANTED),
  });
  const waiting = [];
  let done = null;
  h.wallet.connect("To show your things", {}, { waiting: (v) => waiting.push(v.state), done: (v) => (done = v) });
  await h.settle();
  assert.equal(h.wallet.state, "asking");
  assert.equal(h.opened.length, 1, "Names was brought forward");
  await h.tick(3);
  assert.equal(done.state, "approved");
  assert.equal(h.wallet.state, "connected");
  assert.deepEqual(h.wallet.accounts, GRANTED.accounts);
  assert.deepEqual(waiting, ["pending", "pending"]);
});

test("connect: already connected asks nobody and opens nothing", async () => {
  const h = harness({ request_access: ok({ request: null, state: "approved", accounts: ["Public/A1"] }), get_config: ok(CONFIG), granted_accounts: ok(GRANTED) });
  let done = null;
  h.wallet.connect("why", {}, { done: (v) => (done = v) });
  await h.tick(2);
  assert.equal(done.state, "approved");
  assert.equal(h.opened.length, 0);
  assert.equal(h.wallet.state, "connected");
});

test("connect: an account hint uses request_access_with only when Names has it", async () => {
  const h = harness({ get_config: ok(CONFIG), granted_accounts: ok({ connected: false, accounts: [] }), request_access_with: ok({ request: null, state: "approved" }) });
  h.wallet.check({}, () => {});
  await h.tick();
  h.wallet.connect("why", { accountHint: "Public/A1" }, {});
  await h.settle();
  assert.deepEqual(h.calls[2], ["request_access_with", ["why", '{"account_hint":"Public/A1"}']]);
  const old = harness({ request_access: ok({ request: null, state: "approved" }) });
  old.wallet.connect("why", { accountHint: "Public/A1" }, {});
  await old.settle();
  assert.deepEqual(old.calls[0], ["request_access", ["why"]]);
});

test("connect: declined ends disconnected with the plain line", async () => {
  const h = harness({ request_access: ok({ request: "c1", state: "pending" }), request_status: ok({ request: "c1", state: "rejected" }) });
  let done = null;
  h.wallet.connect("why", {}, { done: (v) => (done = v) });
  await h.tick(2);
  assert.equal(h.wallet.state, "disconnected");
  assert.equal(SDK.requestLine(done, "the connection"), "You declined the connection in Names Wallet. Nothing was sent.");
});

test("propose + watch: sent, updates, done; withdraw cancels", async () => {
  const h = harness({
    propose_call: ok({ request: "p1", kind: "call", state: "pending" }),
    request_status: [ok({ request: "p1", state: "pending" }), ok({ request: "p1", state: "running" }), ok({ request: "p1", state: "done", tx: "ab" })],
    cancel_request: ok({ request: "p1", state: "cancelled" }),
  });
  const seen = [];
  let done = null, watcher = null;
  h.wallet.proposeCall("P", "[]", "00", "s", { sent: (v, w) => { seen.push("sent:" + v.state); watcher = w }, update: (v) => seen.push(v.state), done: (v) => (done = v) });
  await h.tick(4);
  assert.deepEqual(seen, ["sent:pending", "pending", "running"]);
  assert.equal(done.tx, "ab");
  assert.equal(h.opened.length, 1);
  watcher.withdraw();
  await h.settle();
  assert.deepEqual(h.calls[h.calls.length - 1], ["cancel_request", ["p1"]]);
});

test("watch: polls never overlap, and stop() ends it", async () => {
  const h = harness({ request_status: ok({ request: "p1", state: "pending" }) });
  const w = h.wallet.watch("p1", {});
  await h.tick(3);
  assert.equal(h.calls.length, 3);
  assert.equal(h.timers.length, 1, "one wait at a time");
  w.stop();
  await h.tick(2);
  assert.equal(h.calls.length, 3);
});

test("watch: a refusal ends it through failed", async () => {
  const h = harness({ request_status: no("no such request") });
  let failed = null;
  h.wallet.watch("zz", { failed: (r) => (failed = r) });
  await h.tick(2);
  assert.equal(failed.error, "no such request");
});

test("request lines: every end state in plain words", () => {
  const L = (state, error) => SDK.requestLine({ state, error }, "the payment");
  assert.equal(L("pending"), "Waiting for you to approve the payment in Names Wallet.");
  assert.equal(L("running"), "Names Wallet is sending the payment…");
  assert.equal(L("rejected"), "You declined the payment in Names Wallet. Nothing was sent.");
  assert.equal(L("expired"), "Nobody approved the payment within 10 minutes, so Names Wallet dropped it. Nothing was sent.");
  assert.equal(L("cancelled"), "You withdrew the payment. Nothing was sent.");
  assert.equal(L("failed", "out of LEZ"), "Names Wallet couldn't send the payment: out of LEZ.");
  assert.equal(SDK.requestLine({ state: "done" }), "");
  // A reason about the wallet's own setup is said whole; anything else keeps the generic line.
  assert.equal(SDK.requestLine({ state: "failed", error: "Names Wallet needs an update to send this" }, "the call"), "Names Wallet needs an update to send this.");
  assert.equal(SDK.requestLine({ state: "failed", error: "Names Wallet isn't set up to send this yet" }, "the call"), "Names Wallet isn't set up to send this yet.");
  assert.equal(SDK.requestLine({ state: "failed", error: "the wallet couldn't send it" }, "the call"), "Names Wallet couldn't send the call: the wallet couldn't send it.");
  assert.equal(SDK.requestOpen({ state: "running" }), true);
  assert.equal(SDK.requestOpen({ state: "done" }), false);
});

test("disconnect drops access and says disconnected", async () => {
  const h = harness({ get_config: ok(CONFIG), granted_accounts: ok(GRANTED), drop_access: ok({ revoked: true }) });
  h.wallet.check({}, () => {});
  await h.tick();
  h.wallet.disconnect();
  await h.settle();
  assert.equal(h.wallet.state, "disconnected");
  assert.deepEqual(h.wallet.accounts, []);
});

test("balances joined to the accounts", async () => {
  const h = harness({ account_balances: ok([{ account: "Public/A1", balance: "12" }]) });
  let b = null;
  h.wallet.balances((x) => (b = x));
  await h.settle();
  assert.equal(b[0].balance, "12");
  let again = null;
  h.wallet.balances((x) => (again = x));
  await h.settle();
  assert.equal(again[0].balance, "12", "balances can be asked again (its answer is kept apart, in lastBalances)");
  assert.equal(typeof h.wallet.balances, "function");
  assert.deepEqual(SDK.withBalances([{ account: "Public/A1", account_kind: "public" }, { account: "Private/B", account_kind: "private" }], b),
    [{ account: "Public/A1", account_kind: "public", balance: "12" }, { account: "Private/B", account_kind: "private", balance: "0" }]);
});

// ── Which wallet ────────────────────────────────────────────────────────────────────
const X_ROW = { name: "xwallet", type: "core", category: "wallet", displayName: "X Wallet", version: "1.2.0" };
const X_CONFIG = { ...CONFIG, wallet_sdk: { name: "names-wallet", version: "1.0", app: "xwallet_ui" } };

test("only Names installed: it is the wallet, every call goes to it, Names is opened", async () => {
  const h = harness({ get_config: ok(CONFIG), granted_accounts: ok(GRANTED), request_access: ok({ request: "c1", state: "pending" }), request_status: ok({ request: "c1", state: "pending" }) });
  let w = null;
  h.wallet.check({}, (x) => (w = x));
  await h.tick();
  assert.equal(w.current.module, "names");
  assert.equal(w.current.sdk_version, "1.0", "Names 0.18, before the declaration, counts as 1.0");
  assert.ok(h.all.filter(([m]) => m !== "package_manager").every(([m]) => m === "names"));
  h.wallet.connect("why", {}, {});
  await h.settle();
  assert.deepEqual(h.opened, ["names_ui"]);
});

test("no wallet installed: the plain state, and calls say so", async () => {
  const h = harness({}, { installed: [{ name: "vesting", type: "core", category: "finance" }, { name: "names_ui", type: "ui_qml" }] });
  let w = null, failed = null;
  h.wallet.check({}, (x) => (w = x));
  await h.settle();
  assert.equal(w.state, "none");
  assert.equal(w.problem, "No wallet installed: install Names from Basecamp's app list.");
  h.wallet.call("granted_accounts", [], () => {}, (r) => (failed = r));
  await h.settle();
  assert.equal(failed.none, true);
  assert.equal(h.calls.length, 0, "nothing was sent to any module");
});

test("Basecamp's package list unreadable: Names is assumed", async () => {
  for (const packages of ["", "not json", JSON.stringify({ error: "Call refused" })]) {
    const h = harness({ get_config: ok(CONFIG), granted_accounts: ok(GRANTED) }, { packages });
    let w = null;
    h.wallet.check({}, (x) => (w = x));
    await h.tick();
    assert.equal(w.current.module, "names", JSON.stringify(packages));
  }
});

test("several wallets: the person chooses once, it is remembered, and calls go there", async () => {
  const byModule = (module, method) => method === "get_config" ? ok(module === "xwallet" ? X_CONFIG : CONFIG) : ok(GRANTED);
  const h = harness({}, { installed: [X_ROW, NAMES_ROW], byModule });
  let w = null;
  h.wallet.check({}, (x) => (w = x));
  await h.settle();
  assert.equal(h.wallet.state, "choose");
  assert.equal(h.wallet.problem, SDK.WORDS.choose);
  assert.deepEqual(h.wallet.wallets.map((x) => [x.module, x.display_name]), [["names", "Names"], ["xwallet", "X Wallet"]], "Names first");
  assert.equal(w, null, "check waits for the choice");
  assert.equal(h.wallet.chooseWallet("nothing"), false);
  assert.equal(h.wallet.chooseWallet("xwallet"), true);
  await h.tick();
  assert.equal(w.state, "connected");
  assert.equal(h.kept.wallet, "xwallet", "remembered for this app");
  assert.ok(h.all.filter(([m]) => m !== "package_manager").every(([m]) => m === "xwallet"));
  h.wallet.open();
  assert.deepEqual(h.opened, ["xwallet_ui"], "the app its get_config names");
  // Next time: no question.
  const again = harness({}, { installed: [X_ROW, NAMES_ROW], byModule, kept: { wallet: "xwallet" } });
  let w2 = null;
  again.wallet.check({}, (x) => (w2 = x));
  await again.tick();
  assert.equal(w2.current.module, "xwallet");
  assert.equal(again.changes.includes("choose"), false);
  // Forgotten: asked again.
  again.wallet.forgetWallet();
  again.wallet.check({}, () => {});
  await again.settle();
  assert.equal(again.wallet.state, "choose");
});

test("a remembered wallet since removed is not used: one left is used, several are asked again", async () => {
  const h = harness({ get_config: ok(CONFIG), granted_accounts: ok(GRANTED) }, { kept: { wallet: "gone" } });
  let w = null;
  h.wallet.check({}, (x) => (w = x));
  await h.tick();
  assert.equal(w.current.module, "names");
});

test("listWallets: each with its SDK version, null for one that doesn't answer or implement it", async () => {
  const Y_ROW = { name: "ywallet", type: "core", category: "wallet", display_name: "Y" };
  const Z_ROW = { name: "zmod", type: "core", category: "Wallet", displayName: "Z" };
  const byModule = (module) => module === "xwallet" ? ok(X_CONFIG) : module === "names" ? ok(CONFIG) : module === "zmod" ? ok({ zone: null }) : "Module not connected: ywallet";
  const h = harness({}, { installed: [Y_ROW, X_ROW, NAMES_ROW, Z_ROW, { name: "names_ui", type: "ui_qml", category: "wallet" }], byModule });
  let list = null;
  h.wallet.listWallets((x) => (list = x));
  await h.tick(2);
  assert.deepEqual(list.map((w) => [w.module, w.display_name, w.sdk_version]), [["names", "Names", "1.0"], ["ywallet", "Y", null], ["xwallet", "X Wallet", "1.0"], ["zmod", "Z", null]]);
  assert.equal(list.find((w) => w.module === "xwallet").app, "xwallet_ui");
  assert.equal(h.wallet.state, "checking", "a wallet that isn't running doesn't make the app's state 'missing'");
});

test("a wallet that doesn't declare a version this file speaks is not used", async () => {
  const byModule = (module, method) => method === "get_config" ? ok(module === "oldwallet" ? { features: [] } : { ...X_CONFIG, wallet_sdk: { name: "names-wallet", version: "2.0", app: "x_ui" } }) : ok(GRANTED);
  for (const [row, version] of [[{ name: "oldwallet", type: "core", category: "wallet" }, null], [X_ROW, "2.0"]]) {
    const h = harness({}, { installed: [row], byModule });
    let w = null;
    h.wallet.check({}, (x) => (w = x));
    await h.tick();
    assert.equal(w.state, "unsupported", row.name);
    assert.equal(w.problem, SDK.WORDS.unsupported);
    assert.equal(w.current.sdk_version, version);
    assert.ok(!h.calls.some(([m]) => m === "granted_accounts"), "nothing more is asked of it");
  }
});

test("walletRows and speaks", () => {
  assert.deepEqual(SDK.walletRows([{ name: "names_ui", type: "ui_qml" }, { name: "chat", type: "core", category: "social" }, NAMES_ROW, NAMES_ROW]).map((w) => w.module), ["names"]);
  assert.equal(SDK.walletRows([{ name: "w", type: "core", category: "wallet", displayName: "a\u202eb" }])[0].display_name, "ab");
  assert.deepEqual([SDK.speaks("1.0"), SDK.speaks("1.7"), SDK.speaks("2.0"), SDK.speaks(null)], [true, true, false, false]);
  assert.equal(SDK.declared("other", CONFIG), null, "only Names counts without the declaration");
});

test("NFT requests are watched and withdrawn with their own methods", async () => {
  const h = harness({ propose_nft_transfer: ok({ request: "n1", kind: "nft_transfer", state: "pending" }), nft_request_status: ok({ request: "n1", state: "done", tx: "aa" }), cancel_nft_request: ok({ state: "cancelled" }) });
  let done = null, watch = null;
  h.wallet.propose("propose_nft_transfer", ["P", "Public/A", "alice", "s"], { sent: (v, w) => (watch = w), done: (v) => (done = v) }, { status: "nft_request_status", cancel: "cancel_nft_request" });
  await h.tick(2);
  assert.equal(done.tx, "aa");
  assert.ok(h.calls.some(([m]) => m === "nft_request_status"));
  assert.ok(!h.calls.some(([m]) => m === "request_status"));
  watch.withdraw();
  await h.settle();
  assert.equal(h.calls[h.calls.length - 1][0], "cancel_nft_request");
});

// ── LNS ─────────────────────────────────────────────────────────────────────────────
const RECORD = { name: "alice", display: "alice.lez", owner_tag: "aa", account: "FLBC4VrQ12UDpbG2ArnRgcwrZTz5cabKbT9oY1M7BE2Z", npk: null, vpk: null, texts: {}, forward: null, lapsed: false, deposit: "1000000" };

test("resolve: a name → {name, account}, nothing else of the record", async () => {
  const h = harness({ resolve: ok(RECORD) });
  let r = null;
  h.wallet.resolve(" alice ", (x) => (r = x));
  await h.settle();
  assert.deepEqual(h.calls[0], ["resolve", ["alice"]]);
  assert.deepEqual(r, { ok: true, name: "alice", display: "alice.lez", account: RECORD.account, forwardedFrom: null, lapsed: false });
});

test("resolve: not registered, no account, refused, busy then answered", async () => {
  const cases = { nobody: ok(null), empty: ok({ ...RECORD, name: "empty", account: null }), "b@d": no("a name is 1-32 characters of a-z, 0-9 or '-', such as jef") };
  const h = harness({ resolve: (args) => cases[args[0]] });
  const out = {};
  for (const n of Object.keys(cases)) h.wallet.resolve(n, (x) => (out[n] = x));
  await h.settle();
  assert.deepEqual([out.nobody.code, out.empty.code, out["b@d"].code], ["not_registered", "no_account", "refused"]);
  assert.equal(out.nobody.error, "No one holds the LNS name nobody.");
  const slow = harness({ resolve: [BUSY, ok(RECORD)] });
  let r = null;
  slow.wallet.resolve("alice", (x) => (r = x));
  await slow.tick();
  assert.equal(r.ok, true);
});

test("resolve: a name that forwards is followed one hop", async () => {
  const h = harness({ resolve: (args) => args[0] === "old" ? ok({ ...RECORD, name: "old", account: null, forward: "alice" }) : ok({ ...RECORD, forward: "loop" }) });
  let r = null;
  h.wallet.resolve("old", (x) => (r = x));
  await h.settle(); await h.settle();
  assert.equal(h.calls.length, 2, "one hop only");
  assert.deepEqual([r.ok, r.name, r.account, r.forwardedFrom], [true, "alice", RECORD.account, "old"]);
});

test("resolve: Names missing", async () => {
  const h = harness({ resolve: "Module not connected: names" });
  let r = null;
  h.wallet.resolve("alice", (x) => (r = x));
  await h.settle();
  assert.equal(r.code, "missing");
});

test("reverse: an account → its name, or none", async () => {
  const h = harness({ reverse: (args) => args[0] === "Public/A1" ? ok({ name: "alice", display: "alice.lez" }) : ok(null) });
  let a = null, b = null;
  h.wallet.reverse("Public/A1", (x) => (a = x));
  h.wallet.reverse("B2", (x) => (b = x));
  await h.settle();
  assert.deepEqual(a, { ok: true, account: "Public/A1", name: "alice", display: "alice.lez" });
  assert.deepEqual(b, { ok: true, account: "B2", name: null, display: null });
});

// ── The files themselves ───────────────────────────────────────────────────────────
const source = readFileSync(new URL("./qml/NamesWalletSDK.mjs", import.meta.url), "utf8");
const qml = readFileSync(new URL("./qml/NamesWallet.qml", import.meta.url), "utf8");

test("written for Qt's engine too: no optional chaining, ??, spread, let/const in bodies", () => {
  const code = source.replace(/\/\/.*$/gm, "").replace(/"(\\.|[^"\\])*"/g, '""');
  assert.equal(/\?\.|\?\?|\.\.\.|\blet\b|=>/.test(code), false);
  assert.equal(/^\s+const\b/m.test(code), false);
});

test("no reserved word after a dot (some Qt builds refuse x.public)", () => {
  const reserved = /\.(public|private|protected|package|interface|implements|static|let|yield|enum|class|const|export|import|extends|super)\b/;
  for (const text of [source, qml]) {
    const lines = text.split("\n").map((l) => l.replace(/"(\\.|[^"\\])*"|'(\\.|[^'\\])*'|\/\/.*$/g, '""'));
    assert.deepEqual(lines.map((l, i) => (reserved.test(l) ? i + 1 : 0)).filter(Boolean), []);
  }
});

test("about no one app: no app's own words in the SDK", () => {
  assert.equal(/vest|faucet|testimonial|bridge test network|schedule/i.test(source.replace(/^\s*\/\/.*$/gm, "")), false);
  assert.equal(/"names"/.test(qml), false, "the QML side names no wallet");
});

test("no data field shadows a call", () => {
  const w = SDK.create({});
  const fields = [...source.matchAll(/^\s+wallet\.(\w+) = (?!function)/gm)].map((m) => m[1]);
  const shadowed = fields.filter((f) => typeof w[f] === "function");
  assert.deepEqual(shadowed, []);
});

test("the QML side forwards every call the SDK has", () => {
  for (const name of ["listWallets", "getWallet", "chooseWallet", "forgetWallet", "call", "has", "check", "balances", "open", "connect", "disconnect", "watch", "withdraw", "propose", "proposeCall", "proposeTransfer", "resolve", "reverse"])
    assert.match(qml, new RegExp("function " + name + "\\("), name);
  assert.match(qml, /basecamp\.apps\.launch/);
  assert.match(qml, /oneShot\.createObject/);
});
