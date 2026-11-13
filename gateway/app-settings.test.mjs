// The web app's chain settings (public/app/settings.js). Finding C2 of the 2026-11-13
// pressure test: a link with ?rpc= used to switch the sequencer (and, for the test-chain
// preset, the registry) and save that switch for good. A link must change nothing.
//   node --test gateway/app-settings.test.mjs
import assert from "node:assert/strict";
import test from "node:test";
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
  const custom = { rpc: "https://my-node.example/", registry: "R", board: "B", peer: OTHER_PEER };
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
