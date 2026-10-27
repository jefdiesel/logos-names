// Names webapp: one linear flow — set up a wallet, get tokens, choose a name,
// then chat. Returning devices (already onboarded) land straight in chat.
//
// Reads a name straight from the chain's JSON-RPC in this browser to resolve who
// you message. names.js is resolver/names.js bundled with @noble/hashes (Argon2id);
// regenerate it with:
//   cd gateway && npx rolldown ../resolver/names.js --format esm --file public/app/names.js

import {
  GRACE_MS,
  base58Decode,
  decodeNameRecord,
  decodeProfile,
  deriveName,
  isValidName,
  normalizeName,
  openSealed,
  publicView,
  readShard,
  recordAddress,
} from "./names.js";

// Each chain the app knows, with the registry and board deployed on it.
const CHAINS = {
  "https://lez.buggers.online/lez": {
    registry: "38jEfVV1RCyMzu1FGbYcCn5FqxCxKyY58VgzjruAh2gq",
    board: "8PwnCKStKQtoBxJCgopCG6CPPMJCUWba1SDG1SXtXYTf",
  },
  "https://lez.buggers.online": {
    registry: "m82S3dUDxdcMBtZtXpHXYcVhU68U7bS4AKqsi6NomJm",
    board: "CKqLLZrTUNiw5KLtQCZ9oSDXWWEtCHLpKjp7pQAaP5q4",
  },
};
// The Logos messaging (Waku) node chat connects to: the default "door" on a stable
// named tunnel. Overridable in settings, or via ?peer= in the URL.
const DEFAULT_PEER =
  "/dns4/msg.buggers.online/tcp/443/wss/p2p/16Uiu2HAkxrwXguVQUCPqMyXrn9VuPNce2xoK9Nr7AW2zcHQeS6dg";
const DEFAULTS = { rpc: "https://lez.buggers.online/lez", ...CHAINS["https://lez.buggers.online/lez"], peer: DEFAULT_PEER };
const PRESETS = Object.keys(CHAINS);
const STORE_KEY = "lns-app-settings";
const ONBOARD_KEY = "lns-onboarded"; // { name } once this device has been through setup
const WALLET_REC = "lns-wallet-device"; // wallet.js vault record (presence = a wallet exists)

const $ = (id) => document.getElementById(id);

// ---------- settings ----------

const PEER_RE = /^\/(dns4|dns6|dns|ip4|ip6)\/[^/]+\/tcp\/\d+\/(wss|tls\/ws)\/p2p\/\w+$/;

function urlSettings() {
  const out = {};
  try {
    const q = new URLSearchParams(location.search);
    const peer = q.get("peer");
    if (peer && PEER_RE.test(peer)) out.peer = peer;
    const rpc = q.get("rpc");
    if (rpc) {
      const u = new URL(rpc);
      if (u.protocol === "https:" || u.protocol === "http:") out.rpc = rpc;
    }
  } catch {
    // bad URL params: ignore
  }
  return out;
}

function loadSettings() {
  try {
    const saved = { ...DEFAULTS, ...JSON.parse(localStorage.getItem(STORE_KEY) || "{}"), ...urlSettings() };
    return CHAINS[saved.rpc] ? { rpc: saved.rpc, ...CHAINS[saved.rpc], peer: saved.peer } : saved;
  } catch {
    return { ...DEFAULTS, ...urlSettings() };
  }
}

function saveSettings(next) {
  try {
    localStorage.setItem(STORE_KEY, JSON.stringify(next));
  } catch {
    // private windows: settings last until the page closes
  }
}

const isProgramId = (text) => {
  try {
    return base58Decode(text).length === 32;
  } catch {
    return false;
  }
};

let settings = loadSettings();
if (Object.keys(urlSettings()).length) saveSettings(settings);

function fillSettings() {
  const preset = PRESETS.includes(settings.rpc) ? settings.rpc : "custom";
  for (const radio of document.querySelectorAll('input[name="rpc-choice"]')) radio.checked = radio.value === preset;
  $("rpc-custom").value = preset === "custom" ? settings.rpc : "";
  $("rpc-custom").hidden = preset !== "custom";
  $("registry").value = settings.registry;
  $("board").value = settings.board;
  $("peer").value = settings.peer;
  $("programs").hidden = preset !== "custom";
  $("settings-error").hidden = true;
}

function readSettingsForm() {
  const choice = document.querySelector('input[name="rpc-choice"]:checked')?.value ?? DEFAULTS.rpc;
  const rpc = (choice === "custom" ? $("rpc-custom").value : choice).trim();
  const registry = $("registry").value.trim();
  const board = $("board").value.trim();
  const peer = $("peer").value.trim() || DEFAULT_PEER;
  let url;
  try {
    url = new URL(rpc);
  } catch {
    return { error: "The RPC URL isn't a valid web address." };
  }
  if (url.protocol !== "https:" && url.protocol !== "http:") return { error: "The RPC URL must start with https://." };
  if (!isProgramId(registry)) return { error: "The registry program isn't a valid program ID." };
  if (!isProgramId(board)) return { error: "The board program isn't a valid program ID." };
  if (!PEER_RE.test(peer)) {
    return { error: "The messaging node must be a WebSocket address like /dns4/…/tcp/443/wss/p2p/…" };
  }
  return { value: { rpc, registry, board, peer } };
}

// ---------- reading the chain (for resolving who you message) ----------

class ChainError extends Error {}

async function explainFailure(error, rpc) {
  if (error instanceof ChainError) return error;
  if (!(error instanceof TypeError)) {
    return new ChainError("The chain answered with an error.", { cause: error });
  }
  const url = new URL(rpc);
  const local = ["localhost", "127.0.0.1", "[::1]"].includes(url.hostname);
  if (location.protocol === "https:" && url.protocol === "http:" && !local) {
    return new ChainError("This RPC uses http://, which a page on https:// can't reach. Use an https:// RPC.");
  }
  try {
    await fetch(rpc, { method: "POST", mode: "no-cors", body: "{}", cache: "no-store", referrerPolicy: "no-referrer" });
    return new ChainError(
      "This RPC doesn't allow browsers (CORS). If it normally does, it may be down. " +
        "Pick another in Settings, or use the names command on your computer.",
    );
  } catch {
    return new ChainError("Can't reach the chain. Check your connection or the RPC in Settings.");
  }
}

async function shard(rpc, program, account) {
  try {
    return await readShard(rpc, program, account);
  } catch (error) {
    throw await explainFailure(error, rpc);
  }
}

async function openAs(key, sealed, kind, decode) {
  if (sealed.length === 0) return null;
  const plain = await openSealed(key, sealed, kind);
  if (plain === null) return null;
  try {
    return decode(plain);
  } catch {
    return null;
  }
}

/** What `name` resolves to (its public view, incl. the published `msg` key). */
async function lookUp({ rpc, registry }, name, now = Date.now()) {
  const { id, key } = await deriveName(name);
  const bytes = await shard(rpc, registry, await recordAddress(registry, id));
  if (bytes.length === 0) return { id, key, view: null };
  const record = decodeNameRecord(bytes);
  if (now >= record.expires + GRACE_MS) return { id, key, view: null };
  const expires = record.expires >= Number.MAX_SAFE_INTEGER ? null : record.expires;
  const held = { name, expires, lapsed: expires !== null && expires <= now };
  const profile = await openAs(key, record.sealed, "record", decodeProfile);
  const empty = { account: null, npk: null, vpk: null, texts: {} };
  const view = publicView(profile === null || profile.name !== name ? { ...held, ...empty } : { ...held, ...profile });
  return { id, key, view };
}

// ---------- onboarding state ----------

function read(key) {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}
const hasWallet = () => read(WALLET_REC) != null;
const onboarded = () => read(ONBOARD_KEY) != null;
function setOnboarded(name) {
  try {
    localStorage.setItem(ONBOARD_KEY, JSON.stringify({ name: name || null }));
  } catch {}
}
function onboardName() {
  try {
    return JSON.parse(read(ONBOARD_KEY) || "{}").name || "";
  } catch {
    return "";
  }
}

// ---------- the flow ----------

let messages = null; // messages.js, loaded when chat opens
let walletMod = null; // wallet.js, loaded when the wallet step opens
let step = null;

function setProgress(name) {
  for (const li of document.querySelectorAll("#progress li")) {
    li.classList.toggle("on", li.dataset.step === name);
  }
}

function show(next) {
  step = next;
  $("onboard").hidden = next === "chat";
  $("chat").hidden = next !== "chat";
  $("step-wallet").hidden = next !== "wallet";
  $("step-name").hidden = next !== "name";
  setProgress(next);
  if (next === "wallet") openWallet();
  else if (next === "name") openName();
  else if (next === "chat") openChat();
}

async function openWallet() {
  $("wallet-continue").hidden = true;
  try {
    walletMod ??= await import("./wallet.js");
    walletMod.open({
      settings: () => settings,
      explain: explainFailure,
      onReady: () => {
        $("wallet-continue").hidden = false;
      },
    });
  } catch (error) {
    const box = $("wallet-lock-error");
    box.textContent = "Couldn't load the wallet. Reload and try again.";
    box.hidden = false;
    console.error(error);
  }
}

function openName() {
  const input = $("pick-name");
  const pend = pendingRegistration();
  if (pend && isValidName(pend.name)) {
    // A claim was committed earlier and is mid-wait — resume it.
    input.value = pend.name;
    $("pick-chosen").textContent = pend.name;
    $("pick-command").textContent = `names register ${pend.name} --public`;
    $("pick-result").textContent = "";
    $("pick-claim").hidden = false;
    claimName(pend.name);
    return;
  }
  if (onboardName() && !input.value) input.value = onboardName();
  setTimeout(() => input.focus(), 50);
}

async function openChat() {
  // Prefill who you are from the chosen name, and who to message from a #name link.
  const me = onboardName();
  if (me) {
    const field = $("msg-me");
    if (field && !field.value) field.value = me;
  }
  const to = normalizeName(decodeURIComponent(location.hash.slice(1)));
  if (to) {
    const field = $("msg-to");
    if (field) field.value = to;
  }
  try {
    messages ??= await import("./messages.js");
    messages.open({ settings: () => settings, lookUp: (name) => lookUp(settings, name), explain: explainFailure });
  } catch (error) {
    $("msg-conn").textContent = "Couldn't load chat";
    console.error(error);
  }
}

// ---------- step wiring ----------

$("wallet-continue").addEventListener("click", () => show("name"));
$("wallet-skip-all").addEventListener("click", () => {
  setOnboarded(null);
  show("chat");
});

// The step tabs (Wallet / Name / Chat) are clickable navigation.
for (const li of document.querySelectorAll("#progress li")) {
  li.style.cursor = "pointer";
  li.setAttribute("role", "button");
  li.setAttribute("tabindex", "0");
  li.addEventListener("click", () => show(li.dataset.step));
  li.addEventListener("keydown", (e) => {
    if (e.key === "Enter" || e.key === " ") { e.preventDefault(); show(li.dataset.step); }
  });
}

$("pick-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  const name = normalizeName($("pick-name").value);
  const result = $("pick-result");
  const claim = $("pick-claim");
  claim.hidden = true;
  result.className = "me-status";
  if (!name || !isValidName(name)) {
    result.textContent = "Names are lowercase letters, digits or dashes, not starting or ending with a dash.";
    return;
  }
  if (name.length < 5) {
    result.textContent = "Names must be at least 5 characters.";
    result.classList.add("bad");
    return;
  }
  result.textContent = `Checking ${name}…`;
  try {
    const { view } = await lookUp(settings, name);
    if (view === null) {
      result.textContent = `✓ ${name} is free.`;
      result.classList.add("ok");
      $("pick-chosen").textContent = name;
      $("pick-command").textContent = `names register ${name} --public`;
      claim.hidden = false;
      setOnboarded(name); // remember the intended name so chat can prefill it
      $("msg-me") && ($("msg-me").value = name);
    } else {
      result.textContent = `${name} is already taken.`;
      result.classList.add("bad");
    }
  } catch (error) {
    const why = (await explainFailure(error, settings.rpc)).message;
    result.textContent = `Couldn't check: ${why}`;
    result.classList.add("bad");
  }
});

$("pick-command-copy").addEventListener("click", (event) => copy($("pick-command").textContent, event.currentTarget));

// ---------- claiming a public name (register.js, signs with the browser wallet) ----------

let registerMod = null;
let registering = false;
const REGISTER_PENDING = "lns-register-pending"; // { name, owner, secretHex, made }

function pendingRegistration() {
  try { return JSON.parse(localStorage.getItem(REGISTER_PENDING) || "null"); } catch { return null; }
}
function savePending(p) { try { localStorage.setItem(REGISTER_PENDING, JSON.stringify(p)); } catch {} }
function clearPending() { try { localStorage.removeItem(REGISTER_PENDING); } catch {} }

function claimProgress(text, cls) {
  const el = $("pick-progress");
  el.hidden = false;
  el.textContent = text;
  el.className = "me-status" + (cls ? " " + cls : "");
}

async function claimName(name) {
  if (registering) return;
  registering = true;
  $("pick-register").disabled = true;
  try {
    const walletMod = await import("./wallet.js");
    walletMod.ensureLoaded?.(); // pull a plain saved wallet into memory if it isn't already
    const key = walletMod.walletKey();
    const account = walletMod.walletAccount();
    if (!key) {
      claimProgress("Open step 1 (Wallet) to unlock your wallet, then come back — your claim is saved.", "bad");
      $("pick-register").disabled = false;
      return;
    }
    registerMod ??= await import("./register.js");
    registerMod.setRpc(settings.rpc);
    const pend = pendingRegistration();
    const resume = pend && pend.name === name && pend.owner === account ? { secretHex: pend.secretHex, made: pend.made } : null;
    await registerMod.registerPublic({
      privateKey: key, program: settings.registry, name, resume,
      onStep: (stage, detail) => {
        if (stage === "commit") claimProgress("Committing to the name…");
        else if (stage === "committed") {
          savePending({ name, owner: account, secretHex: detail.secretHex, made: detail.made });
          claimProgress("Committed. Now the ~15-minute wait so nobody can take it — you can close this; it resumes when you return.");
        } else if (stage === "waiting") {
          const m = Math.max(1, Math.ceil(detail.remainingMs / 60000));
          claimProgress(`Waiting ~${m} min before claiming… (safe to leave; it resumes)`);
        } else if (stage === "register") claimProgress("Claiming the name on the chain…");
        else if (stage === "done") claimProgress("Registered ✓");
      },
    });
    clearPending();
    setOnboarded(name);
    claimProgress(`✓ ${name} is yours. Opening chat…`, "ok");
    setTimeout(() => show("chat"), 1400);
  } catch (error) {
    const msg = error?.code === "insufficient" ? error.message : `Couldn't claim: ${(error && error.message) || error}`;
    claimProgress(msg, "bad");
    $("pick-register").disabled = false;
  } finally {
    registering = false;
  }
}

$("pick-register")?.addEventListener("click", () => {
  const name = normalizeName($("pick-chosen").textContent || $("pick-name").value);
  if (name) claimName(name);
});

$("name-back").addEventListener("click", () => show("wallet"));
$("name-continue").addEventListener("click", () => {
  if (!onboarded()) setOnboarded($("pick-name").value ? normalizeName($("pick-name").value) : null);
  show("chat");
});

$("redo-setup").addEventListener("click", () => show("wallet"));

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

// ---------- settings drawer ----------

function openDrawer(open) {
  $("privacy").hidden = !open;
  if (open) fillSettings();
}
$("privacy-open").addEventListener("click", () => openDrawer(true));
$("privacy-close").addEventListener("click", () => openDrawer(false));

for (const radio of document.querySelectorAll('input[name="rpc-choice"]')) {
  radio.addEventListener("change", () => {
    const custom = radio.value === "custom" && radio.checked;
    $("rpc-custom").hidden = !custom;
    $("programs").hidden = !custom;
    if (CHAINS[radio.value] && radio.checked) {
      $("registry").value = CHAINS[radio.value].registry;
      $("board").value = CHAINS[radio.value].board;
    }
    if (custom) $("rpc-custom").focus();
  });
}
$("settings-save").addEventListener("click", () => {
  const { value, error } = readSettingsForm();
  if (error) {
    $("settings-error").textContent = error;
    $("settings-error").hidden = false;
    return;
  }
  settings = value;
  saveSettings(settings);
  openDrawer(false);
  if (step === "chat") openChat();
});
$("settings-reset").addEventListener("click", () => {
  settings = { ...DEFAULTS };
  try {
    localStorage.removeItem(STORE_KEY);
  } catch {}
  fillSettings();
});

// ---------- start ----------
// Returning device (already onboarded) → straight to chat. A #name link also goes
// straight to chat (to message that name). Otherwise, start the funnel.

// Your wallet is home — it opens there so your account + balance are the first thing,
// not a link at the bottom. A #name link opens chat to message that name.
const deepLink = normalizeName(decodeURIComponent(location.hash.slice(1)));
if (pendingRegistration()) show("name"); // a claim is mid-wait — resume it
else if (deepLink) show("chat");
else show("wallet");
