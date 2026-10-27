// Messages: end-to-end encrypted messages to and from names (format in msg-crypto.js).
// Loaded by app.js the first time Messages opens; waku.js (about 2 MB) loads after.
//
// This device has one messaging key (X25519), made here and kept in localStorage. A name
// turns messages on by publishing that key as its `msg` text entry. A message is sealed
// to the recipient's key and sent over Logos messaging (Waku light push) to the inbox
// topic of that key; this page listens on its own inbox with a Waku filter subscription.
// Live only: nothing is fetched from the network's store.

import {
  MAX_TEXT,
  MSG_TEXT_KEY,
  CLUSTER,
  fromHex,
  inboxTopic,
  isVerified,
  newKeyPair,
  openMessage,
  parseMsgKey,
  publicKeyOf,
  sealMessage,
  toHex,
} from "./msg-crypto.js";
import { isValidName, normalizeName } from "./names.js";
import * as vault from "./vault.js";

const KEY_STORE = "lns-msg-device"; // vault record (see vault.js); old builds stored { secretKey: hex }
const ME_STORE = "lns-msg-me:"; // + RPC: the name this device answers to on that chain
const CONVO_STORE = "lns-msg-convos:"; // + RPC: { name: [message] }
const KEEP = 200; // messages kept per conversation
const TEST_RPC = "https://lez.buggers.online";

const $ = (id) => document.getElementById(id);
const el = (tag, props = {}, ...children) => {
  const node = Object.assign(document.createElement(tag), props);
  node.append(...children);
  return node;
};

let ctx = null; // { settings(), lookUp(name), explain(error, rpc) } from app.js
let device = null; // { secretKey, publicKey }, only once unlocked
let keyKept = true; // false when this browser won't store the key
let wired = false;
let thread = null; // the name whose thread is open
let lockMode = null; // "unlock-passcode" | "unlock-passkey" | "migrate" | "new"

// ---------- storage ----------

function readStore(key, fallback) {
  try {
    const text = localStorage.getItem(key);
    return text === null ? fallback : JSON.parse(text);
  } catch {
    return fallback;
  }
}

function writeStore(key, value) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
    return true;
  } catch {
    return false;
  }
}

const memory = {}; // what couldn't be stored lasts until the page closes

// ---------- the messaging key ----------
//
// The X25519 device secret is kept PLAINLY in localStorage ({ secretKey: hex }) and
// generated silently the first time chat opens. It can't spend money and is rotatable,
// so plain persistence is the right tradeoff and chat never blocks on a passkey.
//
// A passkey is OPTIONAL: `protectWithPasskey` wraps the same key in a vault.js record
// (WebAuthn PRF, no password). Once protected, opening chat unlocks it with the passkey
// (showLock). Lose that passkey and the saved key can't be recovered — fine, it's
// rotatable: "Make a new key" starts fresh; republish it with set-text msg.

const storedRecord = () => readStore(KEY_STORE, null);

function lockError(text) {
  $("msg-lock-error").textContent = text;
  $("msg-lock-error").hidden = false;
}
function lockBusy(on, msg) {
  const b = $("msg-lock-busy");
  b.hidden = !on;
  if (on && msg) b.textContent = msg;
  for (const id of ["msg-passkey", "msg-skip", "msg-newkey"]) $(id).disabled = on;
}

/** Decide the lock mode from what's stored + passkey support, and render the panel. */
function showLock() {
  const rec = storedRecord();
  $("msg-lock-error").hidden = true;
  lockBusy(false);
  $("msg-main").hidden = true;
  $("msg-lock").hidden = false;
  const passkey = vault.passkeySupported();
  const show = (id, on) => ($(id).hidden = !on);

  if (vault.isVaultRecord(rec)) {
    lockMode = "unlock";
    $("msg-lock-title").textContent = "Unlock messaging";
    $("msg-lock-note").textContent = "Unlock this device's messaging key with your passkey.";
    show("msg-passkey", true); $("msg-passkey").textContent = "Unlock with passkey";
    // Lost the passkey? The saved key can't be recovered; offer a fresh start.
    show("msg-newkey", true); $("msg-newkey").textContent = "Lost your passkey? Make a new key";
    show("msg-skip", true); $("msg-skip").textContent = "Use without saving on this browser";
    return;
  }

  const legacy = vault.isLegacyPlaintext(rec);
  lockMode = legacy ? "migrate" : "new";
  if (passkey) {
    $("msg-lock-title").textContent = legacy ? "Protect your messaging key" : "Protect messaging on this device";
    $("msg-lock-note").textContent = legacy
      ? "This browser is holding your messaging key unprotected. Protect it with a passkey to keep it, or use it just for this session."
      : "Save this device's messaging key behind a passkey, or use it just for this session.";
    show("msg-passkey", true); $("msg-passkey").textContent = "Protect with a passkey";
    show("msg-newkey", false);
    show("msg-skip", true); $("msg-skip").textContent = "Use without saving on this browser";
  } else {
    // No WebAuthn here: never fall back to a password — session-only is the only option.
    $("msg-lock-title").textContent = "Messaging";
    $("msg-lock-note").textContent = legacy
      ? "This browser can't securely save a key (no passkey support), so the key it is holding can't be protected. You can use it for this session; it won't be kept."
      : "This browser can't securely save a key (no passkey support). You can use messaging for this session; the key won't be kept.";
    show("msg-passkey", false);
    show("msg-newkey", false);
    show("msg-skip", true); $("msg-skip").textContent = "Use for this session";
  }
}

function finishUnlock(secretKey, kept) {
  if (!(secretKey instanceof Uint8Array) || secretKey.length !== 32) throw new Error("bad key");
  device = { secretKey, publicKey: publicKeyOf(secretKey) };
  keyKept = kept;
  lockBusy(false);
  proceed();
}

async function doPasskey() {
  $("msg-lock-error").hidden = true;
  try {
    lockBusy(true, "Waiting for your passkey\u2026");
    if (lockMode === "unlock") {
      finishUnlock(await vault.unlockPasskey(storedRecord()), true);
    } else {
      const secretKey = lockMode === "migrate" ? fromHex(storedRecord().secretKey) : newKeyPair().secretKey;
      const record = await vault.enrollPasskey(secretKey, { rpId: location.hostname, userName: "Names Messages" });
      finishUnlock(secretKey, writeStore(KEY_STORE, record)); // replaces any plaintext in place
    }
  } catch (error) {
    lockBusy(false);
    lockError(error?.message ?? String(error));
  }
}

/** "Use without saving": key in memory for this session only; persist nothing. */
function doSkip() {
  let secretKey;
  const rec = storedRecord();
  if (vault.isLegacyPlaintext(rec)) { try { secretKey = fromHex(rec.secretKey); } catch {} }
  if (!secretKey) secretKey = newKeyPair().secretKey;
  try { localStorage.removeItem(KEY_STORE); } catch {} // drop any plaintext; nothing stored this way
  finishUnlock(secretKey, false);
}

/** Lost-passkey recovery: discard the unrecoverable saved key and start fresh (plain). */
function doNewKey() {
  try { localStorage.removeItem(KEY_STORE); } catch {}
  delete memory[KEY_STORE];
  device = null;
  loadPlainKey();
  proceed();
}

/** Optional: protect the (plain) messaging key with a passkey. Off by default; a
 * no-op unless the user asks. After this, opening chat unlocks with the passkey. */
async function protectWithPasskey() {
  const state = $("msg-protect-state");
  if (!state) return;
  if (vault.isVaultRecord(storedRecord())) { state.textContent = "Protected with a passkey ✓"; return; }
  if (!vault.passkeySupported()) { state.textContent = "This browser has no passkey support."; return; }
  try {
    state.textContent = "Waiting for your passkey…";
    const record = await vault.enrollPasskey(device.secretKey, { rpId: location.hostname, userName: "Names Messages" });
    writeStore(KEY_STORE, record);
    keyKept = true;
    reflectProtect();
  } catch (error) {
    state.textContent = error?.message ?? "Couldn't protect the key.";
  }
}

function reflectProtect() {
  const btn = $("msg-protect");
  const state = $("msg-protect-state");
  if (!btn || !state) return;
  if (vault.isVaultRecord(storedRecord())) {
    btn.hidden = true;
    state.textContent = "Protected with a passkey ✓";
  } else {
    btn.hidden = false;
    state.textContent = "";
  }
}

const rpc = () => ctx.settings().rpc;
const myName = () => memory[ME_STORE + rpc()] ?? readStore(ME_STORE + rpc(), "");
function setMyName(name) {
  if (!writeStore(ME_STORE + rpc(), name)) memory[ME_STORE + rpc()] = name;
}
const convos = () => memory[CONVO_STORE + rpc()] ?? readStore(CONVO_STORE + rpc(), {});
function saveConvos(all) {
  if (!writeStore(CONVO_STORE + rpc(), all)) memory[CONVO_STORE + rpc()] = all;
}

function addMessage(name, message) {
  const all = convos();
  const list = all[name] ?? [];
  if (list.some((m) => m.id === message.id)) return false;
  list.push(message);
  all[name] = list.slice(-KEEP);
  saveConvos(all);
  return true;
}

// ---------- the chain ----------

const keyCache = new Map(); // rpc|registry|name -> Promise<{ registered, key }>

/** The `msg` entry of `name` on the selected chain: { registered, key (hex or null) }. */
function chainKey(name, { fresh = false } = {}) {
  const { rpc: url, registry } = ctx.settings();
  const id = `${url}|${registry}|${name}`;
  if (fresh || !keyCache.has(id)) {
    const read = ctx.lookUp(name).then(({ view }) => {
      const key = parseMsgKey(view?.texts?.[MSG_TEXT_KEY] ?? "");
      return { registered: view !== null, key: key ? toHex(key) : null };
    });
    // Only a key found is kept: a name that turns messages on later is seen at once.
    read.then((found) => found.key || keyCache.delete(id), () => keyCache.delete(id));
    keyCache.set(id, read);
  }
  return keyCache.get(id);
}

async function chainError(error) {
  return (await ctx.explain(error, rpc())).message;
}

// ---------- the messaging node ----------

let node = null;
let encoderFor = null;
let connecting = null;
let retryTimer = null;
let retryDelay = 5000;
let connectedPeer = null;
const seen = new Set();

function connState(text, kind = "") {
  $("msg-conn").textContent = text;
  $("msg-conn").className = `conn ${kind}`;
  $("msg-send").disabled = kind !== "ok";
}

function connFailed(detail) {
  connState("Can't reach the messaging node", "bad");
  $("msg-conn-detail").textContent = detail;
  $("msg-conn-error").hidden = false;
}

async function stopNode() {
  const old = node;
  node = null;
  encoderFor = null;
  connectedPeer = null;
  if (old) await old.stop().catch(() => {});
}

function connect() {
  if (connecting) return connecting;
  clearTimeout(retryTimer);
  const peer = ctx.settings().peer;
  connecting = (async () => {
    await stopNode();
    connState("Connecting…");
    $("msg-conn-error").hidden = true;
    const { createLightNode, Protocols } = await import("./waku.js");
    const fresh = await createLightNode({
      defaultBootstrap: false,
      libp2p: { hideWebSocketInfo: true },
      networkConfig: CLUSTER,
    });
    try {
      await fresh.start();
      await fresh.dial(peer);
      await fresh.waitForPeers([Protocols.LightPush, Protocols.Filter], 20000);
      const decoder = fresh.createDecoder({ contentTopic: inboxTopic(device.publicKey) });
      if (!(await fresh.filter.subscribe(decoder, receive))) throw new Error("The node didn't take the inbox subscription.");
    } catch (error) {
      await fresh.stop().catch(() => {});
      throw error;
    }
    node = fresh;
    connectedPeer = peer;
    const encoders = new Map();
    encoderFor = (topic) => {
      if (!encoders.has(topic)) encoders.set(topic, fresh.createEncoder({ contentTopic: topic }));
      return encoders.get(topic);
    };
    fresh.libp2p.addEventListener("peer:disconnect", () => {
      if (node === fresh && fresh.libp2p.getConnections().length === 0) lost();
    });
    retryDelay = 5000;
    connState("Connected", "ok");
  })()
    .catch((error) => {
      console.warn("messaging node:", error);
      connFailed(`${error?.message ?? error}. Trying again in ${retryDelay / 1000} s.`);
      scheduleRetry();
    })
    .finally(() => {
      connecting = null;
    });
  return connecting;
}

function scheduleRetry() {
  clearTimeout(retryTimer);
  retryTimer = setTimeout(connect, retryDelay);
  retryDelay = Math.min(retryDelay * 2, 60000);
}

function lost() {
  stopNode();
  connState("Reconnecting…", "bad");
  scheduleRetry();
}

// ---------- receiving ----------

async function receive(wakuMessage) {
  const message = openMessage(device.secretKey, wakuMessage.payload);
  if (message === null || seen.has(message.id)) return;
  seen.add(message.id);
  let verified = false;
  let note = "";
  if (message.authentic) {
    try {
      verified = isVerified(message, (await chainKey(message.from)).key);
      // A cached key may be an old device's: read again before calling it unverified.
      if (!verified) verified = isVerified(message, (await chainKey(message.from, { fresh: true })).key);
    } catch (error) {
      note = `couldn't check: ${await chainError(error)}`;
    }
  }
  const added = addMessage(message.from, {
    id: message.id,
    dir: "in",
    text: message.text,
    at: Date.now(),
    verified,
    note,
  });
  if (added) render();
}

// ---------- sending ----------

async function send(to, text) {
  const from = myName();
  if (!from) throw new Error("Enter which name is yours under “Set up this device” first.");
  if (!node) throw new Error("Not connected to the messaging node yet.");
  let found;
  try {
    found = await chainKey(to);
  } catch (error) {
    throw new Error(await chainError(error));
  }
  if (!found.registered) throw new Error(`${to} isn't registered on this chain.`);
  if (!found.key) throw new Error(`${to} hasn't turned on messages yet.`);
  const key = fromHex(found.key);
  const payload = sealMessage({ to: key, from, fromSecret: device.secretKey, text });
  const result = await node.lightPush.send(encoderFor(inboxTopic(key)), { payload });
  if (!result.successes?.length) {
    const why = result.failures?.map((f) => f.error).filter(Boolean).join(", ");
    throw new Error(`The messaging node didn't take it${why ? ` (${why})` : ""}. Try again.`);
  }
  addMessage(to, { id: toHex(payload.subarray(1, 33)), dir: "out", text, at: Date.now() });
}

// ---------- showing it ----------

const when = (at) =>
  new Date(at).toLocaleString(undefined, { dateStyle: "short", timeStyle: "short" });

function commandFor(name) {
  const who = name || "<yourname>";
  const key = toHex(device.publicKey);
  return rpc() === TEST_RPC
    ? `~/logos/lez-test/lez-test set-text ${who} msg ${key}`
    : `names set-text ${who} msg ${key}`;
}

function renderSetup() {
  $("msg-key").textContent = toHex(device.publicKey);
  $("msg-key-note").textContent = "This browser won't keep the key, so it lasts only until the page closes.";
  $("msg-key-note").hidden = keyKept;
  $("msg-command").textContent = commandFor(normalizeName($("msg-me").value) || myName());
}

let checkToken = 0;
async function checkMe() {
  const mine = ++checkToken;
  const name = normalizeName($("msg-me").value);
  const box = $("msg-me-status");
  box.className = "me-status";
  if (!name || isValidName(name)) setMyName(name);
  renderSetup();
  if (!name) {
    box.textContent = "";
    $("msg-setup-summary").textContent = "Set up this device";
    $("msg-setup").open = true;
    return;
  }
  if (!isValidName(name)) {
    box.textContent = "That isn't a name.";
    box.classList.add("bad");
    return;
  }
  box.textContent = `Checking ${name} on the chain…`;
  try {
    const { registered, key } = await chainKey(name, { fresh: true });
    if (mine !== checkToken) return;
    const here = toHex(device.publicKey);
    if (!registered) {
      box.textContent = `${name} isn't registered on this chain.`;
      box.classList.add("bad");
    } else if (key === here) {
      box.textContent = `✓ Messages to ${name} come to this device.`;
      box.classList.add("ok");
      $("msg-setup-summary").textContent = `✓ You are ${name}`;
      return;
    } else if (key) {
      box.textContent = `Not yet: ${name}'s messages go to another key (another device). Run the command above to move them here.`;
      box.classList.add("bad");
    } else {
      box.textContent = "Not yet: run the command above. You can send now, but replies can't reach you until it's done.";
      box.classList.add("bad");
    }
  } catch (error) {
    if (mine !== checkToken) return;
    box.textContent = `Couldn't check: ${await chainError(error)}`;
    box.classList.add("bad");
  }
  $("msg-setup-summary").textContent = `Set up this device (${name}: not yet)`;
  $("msg-setup").open = true;
}

function renderList() {
  const all = convos();
  const names = Object.keys(all).filter((n) => all[n].length).sort((a, b) => all[b].at(-1).at - all[a].at(-1).at);
  $("msg-convos").replaceChildren(
    ...names.map((name) => {
      const last = all[name].at(-1);
      const button = el(
        "button",
        { type: "button", className: "convo" },
        el("span", { className: "convo-name", textContent: name }),
        el("span", { className: "convo-last", textContent: `${last.dir === "out" ? "You: " : ""}${last.text}` }),
        el("span", { className: "convo-when", textContent: when(last.at) }),
      );
      button.addEventListener("click", () => openThread(name));
      return el("li", {}, button);
    }),
  );
  $("msg-empty").hidden = names.length > 0;
}

function renderThread() {
  const list = convos()[thread] ?? [];
  $("msg-with").textContent = thread;
  $("msg-thread").replaceChildren(
    ...list.map((m) => {
      let who;
      if (m.dir === "out") who = "You";
      else if (m.verified) who = `✓ ${thread}`;
      else who = `unverified: claims to be ${thread}${m.note ? ` (${m.note})` : ""}`;
      return el(
        "li",
        { className: `${m.dir}${m.dir === "in" && !m.verified ? " unverified" : ""}` },
        el("div", { className: "meta", textContent: `${who} · ${when(m.at)}` }),
        el("p", { className: "text", textContent: m.text }),
      );
    }),
  );
  $("msg-thread-note").textContent = list.length ? "" : `No messages with ${thread} yet.`;
  $("msg-thread-note").hidden = list.length > 0;
}

function render() {
  if (!device) return;
  renderSetup();
  if (thread) renderThread();
  else renderList();
}

function openThread(name) {
  thread = name;
  $("msg-list-view").hidden = true;
  $("msg-thread-view").hidden = false;
  $("msg-send-error").hidden = true;
  renderThread();
  $("msg-thread").lastElementChild?.scrollIntoView({ block: "end" });
}

function closeThread() {
  thread = null;
  $("msg-thread-view").hidden = true;
  $("msg-list-view").hidden = false;
  renderList();
}

async function copy(text, button) {
  const label = button.textContent;
  try {
    await navigator.clipboard.writeText(text);
    button.textContent = "Copied";
  } catch {
    button.textContent = "Couldn't copy; select the text instead";
  }
  setTimeout(() => (button.textContent = label), 1600);
}

// Publish this device's messaging key onto the user's name (set-text msg …), signed
// in the browser by the wallet, so replies can reach them — one tap, no CLI.
async function publishKey() {
  const name = normalizeName($("msg-me").value) || myName();
  const status = $("msg-publish-status");
  status.className = "me-status";
  status.hidden = false;
  if (!name || !isValidName(name)) {
    status.textContent = "Enter which name is yours first.";
    status.classList.add("bad");
    return;
  }
  const walletMod = await import("./wallet.js");
  const walletSecret = walletMod.walletKey();
  if (!walletSecret) {
    status.textContent = "Set up your wallet first (step 1), then publish.";
    status.classList.add("bad");
    return;
  }
  const reg = await import("./register.js");
  reg.setRpc(ctx.settings().rpc);
  const btn = $("msg-publish");
  btn.disabled = true;
  status.textContent = `Publishing your key on ${name}…`;
  try {
    await reg.setText({ privateKey: walletSecret, program: ctx.settings().registry, name, key: MSG_TEXT_KEY, value: toHex(device.publicKey) });
    status.textContent = "✓ Published — replies can now reach you.";
    status.classList.add("ok");
    checkMe(); // refresh the "✓ You are <name>" state
  } catch (error) {
    status.textContent =
      error?.code === "unregistered" ? `Claim ${name} first (the Name step), then publish.`
      : error?.code === "not_owner" ? `This device's wallet doesn't hold ${name}.`
      : `Couldn't publish: ${(error && error.message) || error}`;
    status.classList.add("bad");
    btn.disabled = false;
  }
}

function wire() {
  $("msg-publish")?.addEventListener("click", publishKey);
  $("msg-passkey").addEventListener("click", doPasskey);
  $("msg-skip").addEventListener("click", doSkip);
  $("msg-newkey").addEventListener("click", doNewKey);
  $("msg-key-copy").addEventListener("click", (e) => copy(toHex(device.publicKey), e.currentTarget));
  $("msg-command-copy").addEventListener("click", (e) => copy($("msg-command").textContent, e.currentTarget));
  $("msg-me").addEventListener("input", renderSetup);
  $("msg-me").addEventListener("change", checkMe);
  $("msg-me").addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      checkMe();
    }
  });
  $("msg-me-check").addEventListener("click", checkMe);
  $("msg-retry").addEventListener("click", () => {
    retryDelay = 5000;
    connect();
  });
  $("msg-back").addEventListener("click", closeThread);
  $("msg-new").addEventListener("submit", (e) => {
    e.preventDefault();
    const name = normalizeName($("msg-to").value);
    if (!isValidName(name)) {
      $("msg-to").setCustomValidity("That isn't a name.");
      $("msg-to").reportValidity();
      return;
    }
    $("msg-to").value = "";
    openThread(name);
  });
  $("msg-to").addEventListener("input", () => $("msg-to").setCustomValidity(""));
  $("msg-compose").addEventListener("submit", async (e) => {
    e.preventDefault();
    const text = $("msg-text").value.trim();
    if (!text || !thread) return;
    const error = $("msg-send-error");
    const button = $("msg-send");
    error.hidden = true;
    button.disabled = true;
    button.textContent = "Sending…";
    try {
      await send(thread, text.slice(0, MAX_TEXT));
      $("msg-text").value = "";
      renderThread();
      $("msg-thread").lastElementChild?.scrollIntoView({ block: "end" });
    } catch (err) {
      error.textContent = err.message;
      error.hidden = false;
    } finally {
      button.textContent = "Send";
      button.disabled = !node;
    }
  });
  $("msg-text").addEventListener("keydown", (e) => {
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) $("msg-compose").requestSubmit();
  });
  $("msg-protect")?.addEventListener("click", protectWithPasskey);
}

/** Everything past the lock: app.js's original open() body, run once device is in memory. */
function proceed() {
  $("msg-lock").hidden = true;
  $("msg-main").hidden = false;
  keyCache.clear();
  $("msg-me").value = myName();
  if (thread) openThread(thread);
  else closeThread();
  render();
  if (myName()) checkMe();
  else {
    $("msg-setup-summary").textContent = "Set up this device";
    $("msg-setup").open = true;
  }
  reflectProtect();
  if (!node || connectedPeer !== ctx.settings().peer) connect();
}

/** Default path: load the plain messaging key, or make one and keep it plainly. No gate.
 * The chat key can't spend money and is rotatable, so plain persistence is the right
 * tradeoff; a passkey is an optional extra (see protectWithPasskey). */
function loadPlainKey() {
  const rec = storedRecord() || memory[KEY_STORE] || null;
  if (rec && typeof rec.secretKey === "string") {
    try {
      const sk = fromHex(rec.secretKey);
      if (sk.length === 32) {
        device = { secretKey: sk, publicKey: publicKeyOf(sk) };
        keyKept = true;
        return;
      }
    } catch {}
  }
  const sk = newKeyPair().secretKey;
  const value = { secretKey: toHex(sk) };
  keyKept = writeStore(KEY_STORE, value);
  if (!keyKept) memory[KEY_STORE] = value; // private windows: keep for the session
  device = { secretKey: sk, publicKey: publicKeyOf(sk) };
}

/** Shows Chat; app.js calls this each time the view opens or settings change. The key
 * is frictionless: only a key the user chose to protect (a vault record) shows a lock. */
export function open(context) {
  ctx = context;
  if (!wired) {
    wire();
    wired = true;
  }
  if (device) {
    proceed();
    return;
  }
  if (vault.isVaultRecord(storedRecord())) {
    showLock(); // the user opted into passkey protection: unlock it
  } else {
    loadPlainKey();
    proceed();
  }
}
