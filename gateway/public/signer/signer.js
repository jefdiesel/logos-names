// Names money-key signer. Served on its OWN origin (a separate subdomain in prod)
// and embedded by the app as an iframe. It OWNS the wallet's secp256k1 key.
//
// THREAT MODEL
// ------------
// A fully compromised parent page may REQUEST an account or a signature, and may
// show/hide this iframe. It must NOT be able to:
//   - read the private key (the key never crosses postMessage, never appears in a
//     reply/result, a log, or an error — only { accountId, publicKey } and a tx hash
//     ever leave here), or
//   - obtain a signature without the user approving THIS iframe's own confirmation UI
//     and re-asserting their passkey.
// Defences: (1) the key is generated here, vaulted with a passkey (WebAuthn PRF →
// AES-GCM, vault.js) in THIS origin's localStorage, and only ever decrypted into a
// local variable for the duration of one signature, then dropped; (2) a strict parent-
// origin allowlist on every inbound message, and an explicit targetOrigin on every
// reply; (3) the "Send …?" confirmation is rendered BY THIS PAGE, so a hostile parent
// can't spoof it; every passkey prompt happens inside an in-iframe button click.
// If WebAuthn/PRF is unavailable the signer is read-only: it can show a stored account
// but cannot create or persist a key. There is no password fallback.

import { NS, isAllowedOrigin, parseRequest, resMsg, evtMsg, validateSignAndSend } from "./protocol.js";
import * as vault from "./vault.js";
import {
  randomPrivateKey, accountIdOfPrivateKey, publicKeyOf, parseAccountId, formatAccountId, toHex,
  signTransfer, maxFeeFor, getAccountsNonces, getAccountBalance, sendTransaction, waitForTransaction,
  NATIVE_TRANSFER_GAS_LIMIT,
} from "./lez-tx.js";

// ---------- parent-origin allowlist ----------
// In production, pin this to the app's exact origin. Replace the placeholder.
const PROD_PARENT_ORIGINS = ["https://names.logos.co"]; // TODO: set the real app origin
const isLocalDev = /^(localhost|127\.0\.0\.1|\[::1\])$/.test(location.hostname);
// Dev only (this page served from localhost): allow a ?parent=<origin> for two-origin tests.
const devParent = isLocalDev ? new URLSearchParams(location.search).get("parent") : null;
const ALLOWED_PARENTS = devParent ? [devParent] : PROD_PARENT_ORIGINS;

const STORE = "lns-wallet-signer"; // vault record + { account:{ accountId, publicKey } }

const $ = (id) => document.getElementById(id);

// ---------- storage (try/catch: private windows / blocked storage) ----------
function readRecord() {
  try { const t = localStorage.getItem(STORE); return t === null ? null : JSON.parse(t); } catch { return null; }
}
function writeRecord(rec) {
  try { localStorage.setItem(STORE, JSON.stringify(rec)); return true; } catch { return false; }
}

// ---------- talking to the parent ----------
function toParent(msg) {
  // Reply only to the allowed parent, with an explicit targetOrigin (never "*").
  for (const origin of ALLOWED_PARENTS) {
    try { parent.postMessage(msg, origin); } catch {}
  }
}
const reply = (id, ok, payload) => toParent(resMsg(id, ok, payload));
const emit = (event, data) => toParent(evtMsg(event, data));

// ---------- the confirmation overlay (rendered here, not by the parent) ----------
let pendingUi = null; // { resolve } for the current Approve/Cancel

function showUi({ title, lines, approveLabel }) {
  emit("show"); // ask the parent to make this iframe visible
  $("title").textContent = title;
  const body = $("lines");
  body.replaceChildren();
  for (const [k, v] of lines) {
    const row = document.createElement("div");
    row.className = "row";
    row.append(Object.assign(document.createElement("span"), { className: "k", textContent: k }));
    row.append(Object.assign(document.createElement("span"), { className: "v", textContent: v }));
    body.append(row);
  }
  $("approve").textContent = approveLabel;
  $("error").hidden = true;
  $("panel").hidden = false;
  $("busy").hidden = true;
  setButtons(true);
  return new Promise((resolve) => { pendingUi = { resolve }; });
}
function setButtons(on) { $("approve").disabled = !on; $("cancel").disabled = !on; }
function uiBusy(text) { $("busy").textContent = text; $("busy").hidden = false; setButtons(false); }
function uiError(text) { $("error").textContent = text; $("error").hidden = false; $("busy").hidden = true; setButtons(true); }
function closeUi() { $("panel").hidden = true; pendingUi = null; emit("hide"); }

$("approve").addEventListener("click", () => pendingUi?.resolve(true));
$("cancel").addEventListener("click", () => { const p = pendingUi; closeUi(); p?.resolve(false); });

// ---------- methods ----------

async function mGetAccount() {
  const rec = readRecord();
  return { account: vault.isVaultRecord(rec) && rec.account ? rec.account : null, canPersist: vault.passkeySupported() };
}

// Create the wallet key. Needs a passkey (user clicks Approve inside this iframe).
async function mCreateAccount() {
  if (!vault.passkeySupported()) throw { code: "no_passkey", message: "this browser can't securely save a key (no passkey support)" };
  const existing = readRecord();
  if (vault.isVaultRecord(existing) && existing.account) return existing.account; // idempotent
  const approved = await showUi({
    title: "Create your wallet key",
    lines: [["Protected by", "a passkey on this device"], ["Stored", "only here, encrypted"]],
    approveLabel: "Create with passkey",
  });
  if (!approved) { closeUi(); throw { code: "cancelled", message: "cancelled" }; }
  let pk;
  try {
    uiBusy("Waiting for your passkey…");
    pk = randomPrivateKey();
    const account = { accountId: formatAccountId(accountIdOfPrivateKey(pk)), publicKey: toHex(publicKeyOf(pk)) };
    const record = await vault.enrollPasskey(pk, { rpId: location.hostname, userName: "Names wallet" });
    record.account = account;
    if (!writeRecord(record)) throw { code: "store_failed", message: "couldn't save the key on this browser" };
    closeUi();
    return account;
  } catch (e) {
    uiError(e?.message ? String(e.message) : "couldn't create the key");
    throw normalizeErr(e);
  } finally {
    if (pk) pk.fill(0); // wipe
  }
}

async function mSignAndSend(rawParams) {
  const rec = readRecord();
  if (!vault.isVaultRecord(rec) || !rec.account) throw { code: "no_account", message: "no wallet key on this device yet" };
  const { to, amount, gasLimit, rpc } = validateSignAndSend(rawParams); // throws bad_params
  let toBytes;
  try { toBytes = parseAccountId(to); } catch { throw { code: "bad_params", message: "that recipient isn't a valid account id" }; }
  const gas = gasLimit !== null ? BigInt(gasLimit) : NATIVE_TRANSFER_GAS_LIMIT;
  const amt = BigInt(amount);
  const maxFee = maxFeeFor(gas);

  const approved = await showUi({
    title: "Approve this payment",
    lines: [["Send", amount + " base units"], ["To", to], ["Max fee", maxFee.toString() + " base units"], ["From", rec.account.accountId]],
    approveLabel: "Approve & sign",
  });
  if (!approved) { closeUi(); throw { code: "cancelled", message: "you cancelled the payment" }; }

  let pk;
  try {
    uiBusy("Confirm with your passkey…");
    pk = await vault.unlockPasskey(rec); // re-prompt the passkey for THIS payment
    uiBusy("Preparing…");
    const fromBytes = accountIdOfPrivateKey(pk);
    const [nonce] = await getAccountsNonces(rpc, [fromBytes]);
    let balance;
    try { balance = await getAccountBalance(rpc, fromBytes); } catch { balance = null; }
    if (balance !== null && balance < amt + maxFee) {
      throw { code: "insufficient_funds", message: `not enough: need ${(amt + maxFee).toString()}, have ${balance.toString()}` };
    }
    uiBusy("Signing & sending…");
    const { wire, hash } = signTransfer({ privateKey: pk, to: toBytes, nonce, amount: amt, gasLimit: gas, tip: 0n, maxFee });
    const sentHash = await sendTransaction(rpc, wire);
    let block = null;
    try { block = await waitForTransaction(rpc, sentHash, { timeoutMs: 45000 }); } catch {} // best-effort; hash already returned
    closeUi();
    return { hash: sentHash || hash, block };
  } catch (e) {
    uiError(e?.message ? String(e.message) : "the payment failed");
    throw normalizeErr(e);
  } finally {
    if (pk) pk.fill(0); // wipe the key; nothing persists in memory between payments
  }
}

async function mLock() {
  // Nothing is held between operations (unlock-per-payment), so this is a clean no-op.
  if (!$("panel").hidden) closeUi();
  return { locked: true };
}

// Never leak key material through an error; map to a typed, safe shape.
function normalizeErr(e) {
  if (e && typeof e === "object" && typeof e.code === "string") return { code: e.code, message: String(e.message ?? e.code) };
  return { code: "error", message: "operation failed" };
}

// ---------- message router ----------
window.addEventListener("message", async (e) => {
  if (!isAllowedOrigin(e.origin, ALLOWED_PARENTS)) return; // strict: ignore everyone else
  if (e.source !== parent) return; // only our embedder
  const request = parseRequest(e.data);
  if (!request) return;
  const { id, method, params } = request;
  try {
    let result;
    if (method === "getAccount") result = await mGetAccount();
    else if (method === "createAccount") result = await mCreateAccount();
    else if (method === "signAndSend") result = await mSignAndSend(params);
    else if (method === "lock") result = await mLock();
    else throw { code: "unknown_method", message: `no method ${method}` };
    reply(id, true, result);
  } catch (err) {
    reply(id, false, normalizeErr(err));
  }
});

// Tell the parent which origin we trust and that we're ready.
emit("ready", { ns: NS, allows: ALLOWED_PARENTS });
