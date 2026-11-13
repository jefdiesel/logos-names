// The web app's chain settings (public/app/settings.js). Finding C2 of the 2026-11-13
// pressure test: a link with ?rpc= used to switch the sequencer (and, for the test-chain
// preset, the registry) and save that switch for good. A link must change nothing.
//   node --test gateway/app-settings.test.mjs
import assert from "node:assert/strict";
import test from "node:test";
import * as S from "./public/app/settings.js";
import { DEFAULTS, loadSettings, resetSettings, saveSettings, STORE_KEY } from "./public/app/settings.js";

function memoryStorage(initial = {}) {
  const map = new Map(Object.entries(initial));
  return {
    map,
    getItem: (key) => (map.has(key) ? map.get(key) : null),
    setItem: (key, value) => map.set(key, String(value)),
    removeItem: (key) => map.delete(key),
  };
}

const at = (url) => new URL(url);
const EVIL = "https://evil.example/";
const TEST_CHAIN = "https://lez.buggers.online";
const REGISTRY = "38jEfVV1RCyMzu1FGbYcCn5FqxCxKyY58VgzjruAh2gq";
const BOARD = "8PwnCKStKQtoBxJCgopCG6CPPMJCUWba1SDG1SXtXYTf";
const LEGACY_KEY = "lns-app-settings";
const OTHER_PEER = "/dns4/evil.example/tcp/443/wss/p2p/16Uiu2HAkxrwXguVQUCPqMyXrn9VuPNce2xoK9Nr7AW2zcHQeS6dg";

test("a ?rpc= link does not change the sequencer, now or on the next visit", () => {
  const storage = memoryStorage();
  const first = loadSettings(storage, at(`https://app.example/app/?rpc=${EVIL}#alice`));
  assert.equal(first.rpc, DEFAULTS.rpc);
  assert.equal(first.registry, DEFAULTS.registry);
  assert.equal(storage.getItem(STORE_KEY), null, "nothing is saved by a link");
  const second = loadSettings(storage, at("https://app.example/app/"));
  assert.deepEqual(second, DEFAULTS);
});

test("a link naming the test-chain preset does not switch the registry", () => {
  const storage = memoryStorage();
  const settings = loadSettings(storage, at(`https://app.example/app/?rpc=${TEST_CHAIN}#alice`));
  assert.deepEqual(settings, DEFAULTS);
  assert.equal(storage.getItem(STORE_KEY), null);
});

test("a ?peer= link does not change the messaging node or save anything", () => {
  const storage = memoryStorage();
  const settings = loadSettings(storage, at(`https://app.example/app/?peer=${encodeURIComponent(OTHER_PEER)}`));
  assert.equal(settings.peer, DEFAULTS.peer);
  assert.equal(storage.getItem(STORE_KEY), null);
});

test("settings saved before this fix (possibly by a link) are dropped and cleared", () => {
  const linkSaved = { rpc: EVIL, registry: DEFAULTS.registry, board: DEFAULTS.board, peer: DEFAULTS.peer };
  const storage = memoryStorage({ [STORE_KEY]: JSON.stringify(linkSaved) });
  const settings = loadSettings(storage, at("https://app.example/app/"));
  assert.deepEqual(settings, DEFAULTS);
  assert.equal(storage.getItem(STORE_KEY), null, "the old record is removed");
});

test("a choice saved in the Settings drawer is kept, and a link cannot override it", () => {
  const storage = memoryStorage();
  const chosen = { rpc: TEST_CHAIN, registry: "ignored-for-a-preset", board: "ignored", peer: DEFAULTS.peer };
  saveSettings(storage, chosen);
  const settings = loadSettings(storage, at(`https://app.example/app/?rpc=${EVIL}`));
  assert.equal(settings.rpc, TEST_CHAIN);
  assert.equal(settings.registry, "m82S3dUDxdcMBtZtXpHXYcVhU68U7bS4AKqsi6NomJm", "a preset's registry comes from the code");
  const custom = { rpc: "https://my-node.example/", registry: REGISTRY, board: BOARD, peer: OTHER_PEER };
  saveSettings(storage, custom);
  assert.deepEqual(loadSettings(storage, at("https://app.example/app/")), custom);
});

test("reset forgets the saved choice", () => {
  const storage = memoryStorage();
  saveSettings(storage, { ...DEFAULTS, rpc: TEST_CHAIN });
  assert.deepEqual(resetSettings(storage), DEFAULTS);
  assert.equal(storage.getItem(STORE_KEY), null);
});

test("no storage at all (blocked site data) still gives the defaults", () => {
  assert.deepEqual(loadSettings(null, at(`https://app.example/app/?rpc=${EVIL}`)), DEFAULTS);
});

// Review of 7f5c68d (W1): an older build on the same origin still reads ?rpc= and saves
// the whole record, mark included, under the old key. Nothing under that key is trusted.
test("a marked record under the old key (what an older build writes) is not trusted", () => {
  const planted = { rpc: EVIL, registry: REGISTRY, board: BOARD, peer: DEFAULTS.peer, savedBy: "settings-drawer" };
  const storage = memoryStorage({ [LEGACY_KEY]: JSON.stringify(planted) });
  assert.deepEqual(loadSettings(storage, at("https://app.example/app/")), DEFAULTS);
  assert.notEqual(STORE_KEY, LEGACY_KEY, "settings live under a key older builds never touch");
});

// W4: the one-time drop is reported, so the page can tell the person what changed.
test("dropping old settings removes them and reports a non-default chain", () => {
  assert.equal(typeof S.dropLegacySettings, "function");
  const storage = memoryStorage({ [LEGACY_KEY]: JSON.stringify({ rpc: EVIL, registry: REGISTRY, board: BOARD }) });
  assert.deepEqual(S.dropLegacySettings(storage), { rpc: EVIL });
  assert.equal(storage.getItem(LEGACY_KEY), null);
  assert.equal(S.dropLegacySettings(storage), null, "only once");
  const plain = memoryStorage({ [LEGACY_KEY]: JSON.stringify({ ...DEFAULTS }) });
  assert.equal(S.dropLegacySettings(plain), null, "default settings need no notice");
  assert.equal(plain.getItem(LEGACY_KEY), null);
  assert.equal(S.dropLegacySettings(memoryStorage({ [LEGACY_KEY]: "{not json" })), null);
  assert.equal(S.dropLegacySettings(null), null);
});

// W2: whatever is stored is checked like a fresh choice in the drawer.
test("stored values are validated at load and fall back to the defaults", () => {
  const bad = [
    { rpc: "constructor" },
    { rpc: "__proto__" },
    { rpc: "toString" },
    { rpc: "javascript:alert(1)" },
    { rpc: "https://my-node.example/", registry: "R" },
    { rpc: "https://my-node.example/", board: "not base58 0OIl" },
    { rpc: "https://my-node.example/", peer: "/dns4/x/tcp/80/ws/p2p/abc" },
    { rpc: "http://evil.example/" },
    { rpc: "https://user:pass@my-node.example/" },
    { rpc: 42 },
  ];
  for (const fields of bad) {
    const storage = memoryStorage();
    saveSettings(storage, { rpc: DEFAULTS.rpc, registry: REGISTRY, board: BOARD, peer: DEFAULTS.peer, ...fields });
    assert.deepEqual(loadSettings(storage, at("https://app.example/app/")), DEFAULTS, JSON.stringify(fields));
    assert.equal(storage.getItem(STORE_KEY), null, `removed: ${JSON.stringify(fields)}`);
  }
});

// W3: the drawer's check refuses what can't work from this page.
test("the drawer refuses non-local http:// and URLs with credentials", () => {
  assert.equal(typeof S.checkSettings, "function");
  const base = { registry: REGISTRY, board: BOARD, peer: DEFAULTS.peer };
  const refused = (rpc) => S.checkSettings({ ...base, rpc }).error;
  assert.ok(refused("http://evil.example/"));
  assert.ok(refused("https://user:pass@my-node.example/"));
  assert.ok(refused("https://user@my-node.example/"));
  assert.ok(refused("ftp://my-node.example/"));
  assert.ok(refused("not a url"));
  assert.deepEqual(S.checkSettings({ ...base, rpc: "http://127.0.0.1:3040" }).value, { ...base, rpc: "http://127.0.0.1:3040" });
  assert.deepEqual(S.checkSettings({ ...base, rpc: "https://my-node.example/" }).value, { ...base, rpc: "https://my-node.example/" });
  // A preset's programs come from the code, whatever the form says.
  assert.deepEqual(S.checkSettings({ ...base, rpc: TEST_CHAIN, registry: "x", board: "y" }).value.registry, "m82S3dUDxdcMBtZtXpHXYcVhU68U7bS4AKqsi6NomJm");
});
