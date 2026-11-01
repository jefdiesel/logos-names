// In-browser PUBLIC name registration for the LNS registry, ported from the Rust CLI
// (cli/src/main.rs) and program (core/src/lib.rs, core/src/shortlist.rs). Public path
// only: the name carries a public account for open payments; private/ZK registration
// stays in Basecamp. The transactions themselves are in lns-tx.js.
//
// Two steps, as the program demands: Commit, wait MIN_COMMIT_AGE_MS (15 min), then
// Register. The commitment hides the name's id so nobody front-runs the claim.
//
// LengthProof: NonMembership over the 65,536-leaf (depth 16) Merkle tree built from
// shortlist.bin, domain-separated SHA-256; root must equal SHORTLIST_ROOT.

import { deriveName, encodeProfile, seal, readActorState, decodeNameRecord, decodeSettings, openSealed, decodeProfile } from "./names.js";
import {
  concat, sha256, formatAccountId, accountIdOfPrivateKey, feeReserve, getFeeState,
  signPublicTransaction, getAccountsNonces, getAccountBalance, sendTransaction, waitForTransaction,
  toHex, fromHex,
} from "./lez-tx.js";
import {
  accountBytes, commitMessage, commitmentAddress, commitmentHash, ownerTag, recordAddress, registerMessage, setRecordMessage,
  settingsAddress,
} from "./lns-tx.js";

// Gas limits, well above what the real guest used on the LEZ 0.4 engine for a one-block
// profile (gateway/app-src/vectors): commit 21k, register 139k, set record 32k.
const REGISTER_GAS = 250000n;
const COMMIT_GAS = 60000n;
const SET_RECORD_GAS = 120000n;
// Transaction sizes, for the fee estimate: measured 2,591 and 569 bytes.
const REGISTER_BYTES = 2700n;
const COMMIT_BYTES = 600n;

// ---- constants (core/src/lib.rs, registry.rs, shortlist.rs) ----
const te = new TextEncoder();
const utf8 = (s) => te.encode(s);
export const MIN_COMMIT_AGE_MS = 15 * 60 * 1000;
export const MAX_COMMIT_AGE_MS = 24 * 60 * 60 * 1000;
export const TIME_WINDOW_MS = 10 * 60 * 1000;
export const DEPOSIT = 1_000_000n; // normal name's deposit (deposit mode)
// shortlist
const LEAF_DOMAIN = utf8("logos-names/v5/short-leaf");
const NODE_DOMAIN = utf8("logos-names/v5/short-node");
const EMPTY_DOMAIN = utf8("logos-names/v5/short-empty");
const SHORTLIST_DEPTH = 16;
// TODO(v5.1): update to shortlist/v51/root.hex after the 1–4 regen finishes; the client-side
// proof check uses this, so a mismatch rejects all proofs. Do not deploy the webapp until updated.
export const SHORTLIST_ROOT = fromHex("66c75f76e42f1e87c80b8783c8a315dd84471041b029066bd5f8911d6c5a5e3c");
const TIER_NORMAL = 0, TIER_LOCKED = 1, TIER_THREE = 3;

// ---- shortlist Merkle non-membership proof (fetched from the names-proof endpoint) ----
// The 1–4 list is ~60 MB, far too big to ship in the page, so the proof is served by
// the M2 (behind the CORS proxy, like /lez). We still verify it here against
// SHORTLIST_ROOT before signing, so a wrong or stale endpoint can't make us burn a deposit.
const MIN_NAME_LENGTH = 5;
const leafHash = (id32, tier) => sha256(concat(LEAF_DOMAIN, id32, Uint8Array.of(tier)));
const nodeHash = (l, r) => sha256(concat(NODE_DOMAIN, l, r));
const cmp = (a, b) => { for (let i = 0; i < 32; i++) { if (a[i] !== b[i]) return a[i] - b[i]; } return 0; };

let _proofEndpoint = "https://lez.buggers.online/names-proof";
export function setProofEndpoint(url) { _proofEndpoint = url; }

function rootFromPath(leaf, index, path) {
  let acc = leaf;
  for (let lvl = 0; lvl < path.length; lvl++) {
    acc = ((index >>> lvl) & 1) === 0 ? nodeHash(acc, path[lvl]) : nodeHash(path[lvl], acc);
  }
  return acc;
}

/** NonMembership LengthProof for a 5+ char name, fetched from the endpoint and verified. */
export async function lengthProof(id32) {
  const res = await fetch(`${_proofEndpoint}?id=${toHex(id32)}`, { cache: "no-store" });
  if (!res.ok) {
    const e = await res.json().catch(() => ({}));
    throw new Error(e.error || `the name-proof service returned HTTP ${res.status}`);
  }
  const p = await res.json();
  const proof = {
    kind: "NonMembership",
    left: { id: fromHex(p.left.id), tier: p.left.tier }, leftIndex: p.leftIndex, leftPath: p.leftPath.map(fromHex),
    right: { id: fromHex(p.right.id), tier: p.right.tier }, rightPath: p.rightPath.map(fromHex),
  };
  // verify locally — a bad/stale endpoint must not make us spend on an invalid proof
  if (!(cmp(proof.left.id, id32) < 0 && cmp(id32, proof.right.id) < 0)) throw new Error("name-proof: id not between the proof leaves");
  const lr = rootFromPath(leafHash(proof.left.id, proof.left.tier), proof.leftIndex, proof.leftPath);
  const rr = rootFromPath(leafHash(proof.right.id, proof.right.tier), proof.leftIndex + 1, proof.rightPath);
  if (toHex(lr) !== toHex(SHORTLIST_ROOT) || toHex(rr) !== toHex(SHORTLIST_ROOT)) {
    throw new Error("name-proof doesn't match the registry's name list (stale endpoint or wrong chain)");
  }
  return proof;
}

// ---- building the signed txs (no network) ----
export async function buildCommit({ privateKey, program, name, secret, now, gasLimit = COMMIT_GAS }) {
  const prog = accountBytes(program);
  const { id } = await deriveName(name);
  const signer = accountIdOfPrivateKey(privateKey);
  const holder = ownerTag(id, signer);
  const commitment = commitmentHash(id, holder, secret);
  const [nonce] = await getAccountsNonces(defaultRpc(), [signer]);
  const message = commitMessage({ program: prog, signer, nonce, gasLimit, now, commitment });
  return { signed: signPublicTransaction(message, [privateKey]), id, holder, promised: commitment };
}

export async function buildRegister({ privateKey, program, name, secret, now, term = 0, deposit = true, gasLimit = REGISTER_GAS }) {
  if (!name || name.length < MIN_NAME_LENGTH) throw Object.assign(new Error(`Names must be at least ${MIN_NAME_LENGTH} characters.`), { code: "too_short" });
  const prog = accountBytes(program);
  const { id, key } = await deriveName(name);
  const signer = accountIdOfPrivateKey(privateKey);
  // public name: publish the payer's account on the name (open payments). No private keys.
  const profile = encodeProfile({ name, account: formatAccountId(signer), npk: null, vpk: null, texts: {} });
  const sealed = await seal(key, profile, "record");
  const proof = await lengthProof(id);
  const [nonce] = await getAccountsNonces(defaultRpc(), [signer]);
  const message = registerMessage({
    program: prog, signer, nonce, gasLimit, id, secret, sealed, term, fee: DEPOSIT, now, deposit, lengthProof: proof,
  });
  return { signed: signPublicTransaction(message, [privateKey]), id };
}

// The RPC the webapp reads/writes through. Defaults to the public CORS proxy.
let _rpc = "https://lez.buggers.online/lez";
export function setRpc(url) { _rpc = url; }
export function defaultRpc() { return _rpc; }

// The account the chain keeps its clock on, and the program whose state there holds it:
// { block_id: u64, timestamp: u64 }, in milliseconds.
const CLOCK_ACCOUNT = formatAccountId(utf8("/LEZ/ClockProgramAccount/0000001"));
const CLOCK_PROGRAM = "2hwaRW8sMnyKLLRbDRw8k68tiv9bnfq98oxcdtcp157Z";

/**
 * The chain's time in milliseconds. The program takes a claimed time only if the block's is
 * at or after it, and a refused transaction is charged, so this device's clock won't do. If
 * the chain's can't be read, this device's set back a minute, so it isn't ahead of the next block.
 */
export async function chainNow(rpc = defaultRpc()) {
  try {
    const state = await readActorState(rpc, CLOCK_PROGRAM, CLOCK_ACCOUNT);
    const timestamp = new DataView(state.buffer, state.byteOffset, state.byteLength).getBigUint64(8, true);
    if (timestamp > 0n) return Number(timestamp);
  } catch {
    // fall through to this device's clock
  }
  return Date.now() - 60000;
}

const reverted = (what, tx) => Object.assign(
  new Error(`${what} was included in a block but failed, so nothing changed. Its fee was charged. (${tx})`),
  { code: "reverted" },
);

/**
 * A block includes failed transactions too, and on LEZ 0.4 they are charged, so success is
 * only what the chain shows afterwards: `signer` holds the record of `id`.
 */
export async function confirmRegistered(rpc, program, id, signer, tx) {
  const state = await readActorState(rpc, program, formatAccountId(recordAddress(accountBytes(program), id)));
  if (state.length === 0 || decodeNameRecord(state).ownerTag !== toHex(ownerTag(id, signer))) throw reverted("The registration", tx);
}

/** The same for a commitment: its account exists. */
export async function confirmCommitted(rpc, program, promised, tx) {
  const state = await readActorState(rpc, program, formatAccountId(commitmentAddress(accountBytes(program), promised)));
  if (state.length === 0) throw reverted("The commitment", tx);
}

// ---- the full public-register flow (commit → wait → register) ----
// onStep(stage, detail) reports progress. secretHex/commitMade let the caller
// resume across the 15-min wait (persist them). Returns {name, id, txHash}.
/** What claiming costs at the current fee market: the 1M deposit + the gas the wallet
 * must hold to send. `resume` = the commit is already paid, so only the register's cost. */
export async function estimateNeed(privateKey, resume = null) {
  const rpc = defaultRpc();
  const quote = await getFeeState(rpc);
  const regReserve = feeReserve({ gasLimit: REGISTER_GAS, dataBytes: REGISTER_BYTES, quote });
  const commitReserve = feeReserve({ gasLimit: COMMIT_GAS, dataBytes: COMMIT_BYTES, quote });
  const need = DEPOSIT + regReserve + (resume ? 0n : commitReserve);
  const have = await getAccountBalance(rpc, accountIdOfPrivateKey(privateKey));
  return { need, have, deposit: DEPOSIT, enough: have >= need };
}

/**
 * When claims of names from the earlier chain close, or null once they have. Until then a
 * holder's claim overwrites a registration, and the deposit goes to the holder when they
 * release the name, so someone registering should know first.
 */
export async function claimsOpenUntil(program, now = null) {
  const account = formatAccountId(settingsAddress(accountBytes(program)));
  const bytes = await readActorState(defaultRpc(), program, account);
  if (bytes.length === 0) return null;
  const { claimsRoot, claimsUntil } = decodeSettings(bytes);
  if (!claimsRoot || /^0+$/.test(claimsRoot)) return null;
  return claimsUntil > (now ?? await chainNow(defaultRpc())) ? claimsUntil : null;
}

export async function registerPublic({ privateKey, program = "38jEfVV1RCyMzu1FGbYcCn5FqxCxKyY58VgzjruAh2gq", name, resume = null, onStep = () => {}, now = null, acceptClaimRisk = async () => true }) {
  const rpc = defaultRpc();
  const clock = async () => now ?? await chainNow(rpc);
  if (!name || name.length < MIN_NAME_LENGTH) throw Object.assign(new Error(`Names must be at least ${MIN_NAME_LENGTH} characters.`), { code: "too_short" });
  // Fail early (before any tx) if the wallet can't cover deposit + fee.
  const { need, have, enough } = await estimateNeed(privateKey, resume);
  if (!enough) {
    const e = new Error(`Not enough tokens to claim: need about ${need} (a 1,000,000 refundable deposit plus the fee), you have ${have}. Get more test tokens first.`);
    e.code = "insufficient"; e.need = need; e.have = have;
    throw e;
  }
  if (!resume) {
    const until = await claimsOpenUntil(program, await clock());
    if (until !== null && !(await acceptClaimRisk(until))) {
      throw Object.assign(new Error("Claiming was cancelled."), { code: "cancelled" });
    }
  }
  let secret, made;
  if (resume) { secret = fromHex(resume.secretHex); made = resume.made; }
  else {
    secret = crypto.getRandomValues(new Uint8Array(32));
    made = await clock();
    onStep("commit", "committing to the name");
    const { signed, promised } = await buildCommit({ privateKey, program, name, secret, now: made });
    const h = await sendTransaction(rpc, signed);
    await waitForTransaction(rpc, signed.hash);
    await confirmCommitted(rpc, program, promised, signed.hash);
    onStep("committed", { secretHex: toHex(secret), made, tx: h });
  }
  const ready = made + MIN_COMMIT_AGE_MS;
  for (let at = await clock(); at < ready; at = await clock()) {
    onStep("waiting", { remainingMs: ready - at });
    await new Promise((r) => setTimeout(r, Math.min(30000, Math.max(5000, ready - at))));
  }
  onStep("register", "claiming the name");
  const { signed, id } = await buildRegister({ privateKey, program, name, secret, now: await clock(), deposit: true });
  const h = await sendTransaction(rpc, signed);
  const block = await waitForTransaction(rpc, signed.hash);
  await confirmRegistered(rpc, program, id, accountIdOfPrivateKey(privateKey), signed.hash);
  onStep("done", { tx: h, block });
  return { name, id: toHex(id), txHash: signed.hash, block };
}

// ---- set-text: publish/update a text entry on a name you hold (SetRecord) ----
// A single signed public tx: read the name's sealed profile, change one text entry,
// re-seal, write it back. No deposit, no commit, no proof.

/** Preflight/test only: build a SetRecord with arbitrary sealed bytes (skips the read). */
export async function buildSetRecordRaw({ privateKey, program, name, sealed, gasLimit = SET_RECORD_GAS }) {
  const { id } = await deriveName(name);
  const signer = accountIdOfPrivateKey(privateKey);
  const [nonce] = await getAccountsNonces(defaultRpc(), [signer]);
  const message = setRecordMessage({ program: accountBytes(program), signer, nonce, gasLimit, id, sealed });
  return { signed: signPublicTransaction(message, [privateKey]), id };
}

/** Set (or clear, with empty value) a text entry on a name the wallet holds. */
export async function setText({ privateKey, program = "38jEfVV1RCyMzu1FGbYcCn5FqxCxKyY58VgzjruAh2gq", name, key, value, gasLimit = SET_RECORD_GAS }) {
  const rpc = defaultRpc();
  const prog = accountBytes(program);
  const { id, key: nameKey } = await deriveName(name);
  const signer = accountIdOfPrivateKey(privateKey);
  const state = await readActorState(rpc, program, formatAccountId(recordAddress(prog, id)));
  if (!state.length) { const e = new Error(`${name} isn't registered yet — claim it first.`); e.code = "unregistered"; throw e; }
  const rec = decodeNameRecord(state);
  if (rec.ownerTag !== toHex(ownerTag(id, signer))) {
    const e = new Error(`This device's wallet doesn't hold ${name}.`); e.code = "not_owner"; throw e;
  }
  const plain = await openSealed(nameKey, rec.sealed, "record");
  const profile = plain ? decodeProfile(plain) : { name, account: null, npk: null, vpk: null, texts: {} };
  if (value) profile.texts[key] = value; else delete profile.texts[key];
  const sealed = await seal(nameKey, encodeProfile(profile), "record");
  const [nonce] = await getAccountsNonces(rpc, [signer]);
  const signed = signPublicTransaction(setRecordMessage({ program: prog, signer, nonce, gasLimit, id, sealed }), [privateKey]);
  const h = await sendTransaction(rpc, signed);
  const block = await waitForTransaction(rpc, signed.hash);
  const after = decodeNameRecord(await readActorState(rpc, program, formatAccountId(recordAddress(prog, id))));
  if (toHex(after.sealed) !== toHex(sealed)) throw reverted("The change", signed.hash);
  return { txHash: signed.hash, block };
}
