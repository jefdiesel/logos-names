// Wallet: a browser-held LEZ wallet for the PUBLIC testnet path — make a key, get
// faucet tokens, see your balance. Public actions only; private/ZK flows (a private
// name, posting as a name) need proving on a computer and stay in Basecamp.
//
// TODO: move the key to the isolated separate-origin signer before mainnet / real
// money. For testnet this holds a secp256k1 key in the app, behind the passkey vault
// (vault.js) exactly like the messaging key, with a "use without saving" session path.

import {
  randomPrivateKey,
  accountIdOfPrivateKey,
  checkPrivateKey,
  formatAccountId,
  fromHex,
  getAccountBalance,
  toHex,
} from "./lez-tx.js";
import * as vault from "./vault.js";

const KEY_STORE = "lns-wallet-device"; // vault record (vault.js): the wallet's secp256k1 key
const FAUCET_URL = "https://lez.buggers.online/faucet";

const $ = (id) => document.getElementById(id);

let ctx = null; // { settings(), explain(error, rpc) } from app.js
let wallet = null; // { secretKey, accountId, accountB58 } once unlocked
let keyKept = true;
let wired = false;
let lockMode = null;
let pollTimer = null;

const rpc = () => ctx.settings().rpc;
const fmt = (n) => n.toLocaleString("en-US");

function readStore(key) {
  try {
    const t = localStorage.getItem(key);
    return t === null ? null : JSON.parse(t);
  } catch {
    return null;
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

// ---------- the vault (wallet key, passkey-encrypted at rest) ----------
//
// Same rule as the messaging key: saving the wallet REQUIRES a passkey (no password);
// otherwise "use without saving" keeps it in memory for the session only. Unlike the
// messaging key, a wallet can hold tokens, so "use without saving" is NOT offered when
// a saved wallet exists — it would discard it. Lost passkey = make a new wallet.

function lockError(t) {
  $("wallet-lock-error").textContent = t;
  $("wallet-lock-error").hidden = false;
}
function lockBusy(on, msg) {
  const b = $("wallet-lock-busy");
  b.hidden = !on;
  if (on && msg) b.textContent = msg;
  for (const id of ["wallet-passkey", "wallet-skip", "wallet-newkey"]) $(id).disabled = on;
}

function showLock() {
  const rec = readStore(KEY_STORE);
  $("wallet-lock-error").hidden = true;
  lockBusy(false);
  $("wallet-main").hidden = true;
  $("wallet-lock").hidden = false;
  const show = (id, on) => ($(id).hidden = !on);

  if (vault.isVaultRecord(rec)) {
    // a passkey-protected wallet — unlock it
    lockMode = "unlock";
    $("wallet-lock-title").textContent = "Unlock your wallet";
    $("wallet-lock-note").textContent = "This device's wallet is protected with a passkey.";
    show("wallet-choose", false);
    show("wallet-passkey", true); $("wallet-passkey").textContent = "Unlock with passkey";
    show("wallet-newkey", true); $("wallet-newkey").textContent = "Lost your passkey? Make a new wallet";
    show("wallet-skip", false);
    return;
  }

  // nothing recoverable saved — ASK: connect an existing key, or make a new one
  lockMode = "new";
  $("wallet-lock-title").textContent = "Set up your wallet";
  $("wallet-lock-note").textContent = "Connect a key you already have, or make a new wallet. Testnet only.";
  show("wallet-choose", true);
  show("wallet-passkey", false);
  show("wallet-newkey", false);
  show("wallet-skip", false);
}

/** Store a key ENCRYPTED behind biometric (Face ID / fingerprint) — never in the clear. */
async function storeBehindBiometric(sk, clearInput) {
  if (!vault.passkeySupported()) {
    lockError("This device can't protect a key with biometrics (no passkey support). Open the app where Face ID or a fingerprint is available.");
    return;
  }
  try {
    lockBusy(true, "Set up Face ID / fingerprint…");
    const record = await vault.enrollPasskey(sk, { rpId: location.hostname, userName: "Names Wallet" });
    writeStore(KEY_STORE, record); // encrypted vault record — the key is kept behind biometric
    if (clearInput) clearInput.value = "";
    finishUnlock(sk, true);
  } catch (e) {
    lockBusy(false);
    lockError(e?.message || "Couldn't set up biometric protection. Try again.");
  }
}

/** Connect an existing key pasted on the lock screen, kept behind biometric. */
async function doConnect() {
  const input = $("wallet-connect-key");
  const hex = (input.value || "").trim().toLowerCase();
  $("wallet-lock-error").hidden = true;
  if (!/^[0-9a-f]{64}$/.test(hex)) { lockError("Paste a 64-character hex key."); return; }
  let sk;
  try { sk = checkPrivateKey(fromHex(hex)); } catch { lockError("That isn't a valid wallet key."); return; }
  await storeBehindBiometric(sk, input);
}

/** Make a fresh wallet, kept behind biometric. */
async function doMake() {
  await storeBehindBiometric(randomPrivateKey(), null);
}

function setWallet(secretKey, kept) {
  if (!(secretKey instanceof Uint8Array) || secretKey.length !== 32) throw new Error("bad wallet key");
  const accountId = accountIdOfPrivateKey(secretKey);
  wallet = { secretKey, accountId, accountB58: formatAccountId(accountId) };
  keyKept = kept;
}

function finishUnlock(secretKey, kept) {
  setWallet(secretKey, kept);
  lockBusy(false);
  proceed();
}

/** Default path: load the plain wallet key, or make one and keep it plainly. No gate.
 * Testnet only; the passkey (vault) is OPTIONAL. TODO: isolated signer before mainnet. */
function loadPlainKey() {
  const rec = readStore(KEY_STORE);
  if (rec && typeof rec.secretKey === "string") {
    try {
      setWallet(checkPrivateKey(fromHex(rec.secretKey)), true);
      return;
    } catch {}
  }
  if (rec != null) {
    // A stored wallet record we couldn't read as a plain key. NEVER overwrite it —
    // that could orphan a funded account. Surface it instead of making a new key.
    throw new Error("unreadable-wallet");
  }
  const sk = randomPrivateKey();
  const kept = writeStore(KEY_STORE, { secretKey: toHex(sk) });
  setWallet(sk, kept);
}

/** Optional: protect the plain wallet key with a passkey. Off by default. */
async function protectWithPasskey() {
  const state = $("wallet-protect-state");
  if (!state) return;
  if (vault.isVaultRecord(readStore(KEY_STORE))) { state.textContent = "Protected with a passkey ✓"; return; }
  if (!vault.passkeySupported()) { state.textContent = "This browser has no passkey support."; return; }
  try {
    state.textContent = "Waiting for your passkey…";
    const record = await vault.enrollPasskey(wallet.secretKey, { rpId: location.hostname, userName: "Names Wallet" });
    writeStore(KEY_STORE, record);
    keyKept = true;
    reflectProtect();
  } catch (error) {
    state.textContent = error?.message ?? "Couldn't protect the wallet.";
  }
}

function reflectProtect() {
  const btn = $("wallet-protect");
  const state = $("wallet-protect-state");
  if (!btn || !state) return;
  if (vault.isVaultRecord(readStore(KEY_STORE))) {
    btn.hidden = true;
    state.textContent = "Protected with a passkey ✓";
  } else {
    btn.hidden = false;
    state.textContent = "";
  }
}

async function doPasskey() {
  $("wallet-lock-error").hidden = true;
  try {
    lockBusy(true, "Waiting for your passkey…");
    if (lockMode === "unlock") {
      finishUnlock(await vault.unlockPasskey(readStore(KEY_STORE)), true);
    } else {
      const secretKey = randomPrivateKey();
      const record = await vault.enrollPasskey(secretKey, { rpId: location.hostname, userName: "Names Wallet" });
      finishUnlock(secretKey, writeStore(KEY_STORE, record));
    }
  } catch (error) {
    lockBusy(false);
    lockError(error?.message ?? String(error));
  }
}

/** A wallet can hold tokens, so NEVER make a throwaway one. Always persist it. */
function doSkip() {
  if (readStore(KEY_STORE) != null) { showLock(); return; } // never clobber a saved wallet
  loadPlainKey(); // makes AND saves a plain key
  proceed();
}

/** Replace the wallet — DESTRUCTIVE, so confirm explicitly and tell them to back up first. */
function doNewKey() {
  const ok = window.confirm(
    "Make a NEW wallet? This replaces the one on this device. If the current wallet holds tokens " +
      "and you haven't backed up its key, they become unreachable. Continue?",
  );
  if (!ok) return;
  try { localStorage.removeItem(KEY_STORE); } catch {}
  wallet = null;
  loadPlainKey();
  proceed();
}

// ---------- account, balance, faucet ----------

function renderBalance(text) {
  $("wallet-balance").textContent = text;
  $("wallet-balance-note").hidden = true;
}

async function refreshBalance() {
  try {
    const bal = await getAccountBalance(rpc(), wallet.accountId);
    renderBalance(fmt(bal));
    return bal;
  } catch (error) {
    const why = (await ctx.explain(error, rpc())).message;
    renderBalance("—");
    $("wallet-balance-note").textContent = `Couldn't read the balance: ${why}`;
    $("wallet-balance-note").hidden = false;
    return null;
  }
}

async function getFaucet() {
  const btn = $("wallet-faucet");
  const status = $("wallet-faucet-status");
  btn.disabled = true;
  status.className = "me-status";
  status.textContent = "Asking the faucet…";
  status.hidden = false;
  let before = 0n;
  try {
    before = (await getAccountBalance(rpc(), wallet.accountId)) ?? 0n;
  } catch {}
  try {
    // The faucet is fronted by the CORS proxy, so the response is readable: a 200 with
    // {tx} on success, or a non-200 with {error} (e.g. the once-a-day limit).
    const res = await fetch(FAUCET_URL, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ account: wallet.accountB58 }),
    });
    const data = await res.json().catch(() => ({}));
    if (!res.ok) {
      status.textContent = data.error
        ? `Faucet: ${data.error}`
        : "The faucet refused — you may have used today's grant (one per account and per network address a day).";
      status.classList.add("bad");
      btn.disabled = false;
      return;
    }
    status.textContent = "Sent. Waiting for the tokens to land (up to a minute or two)…";
    pollBalance(before);
  } catch {
    status.textContent = "Couldn't reach the faucet. Check your connection and try again.";
    status.classList.add("bad");
    btn.disabled = false;
  }
}

function pollBalance(before) {
  clearInterval(pollTimer);
  const started = Date.now();
  pollTimer = setInterval(async () => {
    let bal = null;
    try { bal = await getAccountBalance(rpc(), wallet.accountId); } catch {}
    if (bal !== null) {
      renderBalance(fmt(bal));
      if (bal > before) {
        clearInterval(pollTimer);
        const status = $("wallet-faucet-status");
        status.textContent = `✓ Got ${fmt(bal - before)} tokens.`;
        status.classList.remove("bad");
        status.classList.add("ok");
        $("wallet-faucet").disabled = false;
        return;
      }
    }
    if (Date.now() - started > 150000) {
      clearInterval(pollTimer);
      $("wallet-faucet-status").textContent =
        "No tokens arrived. The faucet gives one grant a day per account and per network address — if you've already used it today, that's why. Otherwise it may be busy; try again shortly.";
      $("wallet-faucet").disabled = false;
    }
  }, 5000);
}

function proceed() {
  $("wallet-lock").hidden = true;
  $("wallet-main").hidden = false;
  $("wallet-account").textContent = `Public/${wallet.accountB58}`;
  $("wallet-key-note").hidden = keyKept;
  $("wallet-faucet").disabled = false;
  reflectProtect();
  renderBalance("…");
  refreshBalance();
  ctx.onReady?.(); // tell app.js a wallet is ready (reveals "Continue →")
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

/** Reveal the raw key so the user can save it outside the browser (the only recovery). */
function doBackup() {
  if (!wallet) return;
  const box = $("wallet-backup-key");
  box.textContent = toHex(wallet.secretKey);
  box.hidden = false;
  $("wallet-backup-copy").hidden = false;
  $("wallet-backup").hidden = true;
}

/** Connect a different key from the main view (a wallet is already loaded) — confirms. */
function doImportMain() {
  const input = $("wallet-import-key");
  const err = $("wallet-import-err");
  const hex = (input.value || "").trim().toLowerCase();
  err.hidden = true;
  if (!/^[0-9a-f]{64}$/.test(hex)) { err.textContent = "Paste a 64-character hex key."; err.hidden = false; return; }
  let sk;
  try { sk = checkPrivateKey(fromHex(hex)); } catch { err.textContent = "That isn't a valid wallet key."; err.hidden = false; return; }
  if (wallet && toHex(wallet.secretKey) !== hex &&
      !window.confirm("Connect this key? It replaces the wallet shown here. If that wallet holds tokens and you haven't backed it up, they become unreachable. Continue?")) return;
  writeStore(KEY_STORE, { secretKey: hex });
  wallet = null;
  input.value = "";
  setWallet(sk, true);
  proceed();
}

function wire() {
  $("wallet-connect-go")?.addEventListener("click", doConnect);
  $("wallet-make")?.addEventListener("click", doMake);
  $("wallet-import-go")?.addEventListener("click", doImportMain);
  $("wallet-passkey").addEventListener("click", doPasskey);
  $("wallet-skip").addEventListener("click", doSkip);
  $("wallet-newkey").addEventListener("click", doNewKey);
  $("wallet-backup")?.addEventListener("click", doBackup);
  $("wallet-backup-copy")?.addEventListener("click", (e) => copy(toHex(wallet.secretKey), e.currentTarget));
  $("wallet-account-copy").addEventListener("click", (e) => copy(`Public/${wallet.accountB58}`, e.currentTarget));
  $("wallet-faucet").addEventListener("click", getFaucet);
  $("wallet-refresh").addEventListener("click", refreshBalance);
  $("wallet-protect")?.addEventListener("click", protectWithPasskey);
}

/** Shows Wallet; app.js calls this when the view opens. Stays on the lock until unlocked. */
export function open(context) {
  ctx = context;
  if (!wired) {
    wire();
    wired = true;
  }
  if (wallet) {
    proceed();
    return;
  }
  const rec = readStore(KEY_STORE);
  if (vault.isVaultRecord(rec)) {
    showLock(); // passkey-protected → unlock
    return;
  }
  if (rec && typeof rec.secretKey === "string") {
    try {
      setWallet(checkPrivateKey(fromHex(rec.secretKey)), true); // saved plain → load it, no asking
      proceed();
      return;
    } catch {}
  }
  if (rec != null) {
    // an unreadable stored record — NEVER overwrite it
    showLock();
    lockError("Found a wallet record I couldn't read. Don't clear this site's data — your key may be recoverable.");
    return;
  }
  showLock(); // nothing saved → ASK: connect an existing key, or make a new one
}

// For the in-browser public-register flow (register.js): the unlocked key stays in
// this page, never sent anywhere. Null while the wallet is locked.
export function walletKey() { return wallet ? wallet.secretKey : null; }
export function walletAccount() { return wallet ? wallet.accountB58 : null; }

/** Silently load a plain saved wallet into memory if one exists (no UI). Returns true if a key is
 *  loaded. A passkey-protected wallet can't load silently — open the Wallet step to unlock it. */
export function ensureLoaded() {
  if (wallet) return true;
  const rec = readStore(KEY_STORE);
  if (rec && typeof rec.secretKey === "string") {
    try { setWallet(checkPrivateKey(fromHex(rec.secretKey)), true); return true; } catch {}
  }
  return false;
}
