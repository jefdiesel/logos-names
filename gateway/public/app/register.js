// In-browser PUBLIC name registration for the LNS registry (v5), ported from the
// Rust CLI (cli/src/main.rs) and program (core/src/lib.rs, core/src/shortlist.rs,
// basecamp-module/.../registry.rs). Public path only: the name carries a public
// account for open payments; private/ZK registration stays in Basecamp.
//
// Two steps, as the program demands: Commit, wait MIN_COMMIT_AGE_MS (15 min), then
// Register. The commitment hides the name's id so nobody front-runs the claim.
//
// Correctness notes (matched byte-for-byte to the Rust):
//  - Instruction borsh enum indices: Register = 0, Commit = 8.
//  - Register accounts [record,page,settings,commitment] select the REGISTRY shard;
//    payer + deposit select the native-token shard (money). Single signer = payer.
//  - LengthProof: NonMembership over the 65,536-leaf (depth 16) Merkle tree built
//    from shortlist.bin, domain-separated SHA-256; root must equal SHORTLIST_ROOT.

import { deriveName, encodeProfile, seal, readShard, decodeNameRecord, openSealed, decodeProfile } from "./names.js";
import {
  Writer, concat, sha256, base58Decode, formatAccountId, accountIdOfPrivateKey,
  shardSelector, nativeBalance, feeDeclaration, maxFeeFor, feeReserve, getFeeState,
  signPublicTransaction, getAccountsNonces, getAccountBalance, sendTransaction, waitForTransaction,
  toHex, fromHex,
} from "./lez-tx.js";

const REGISTER_GAS = 250000n; // ~1.5x the registry's ~165k-cycle register run
const COMMIT_GAS = 60000n;

// ---- constants (core/src/lib.rs, registry.rs, shortlist.rs) ----
const te = new TextEncoder();
const utf8 = (s) => te.encode(s);
export const MIN_COMMIT_AGE_MS = 15 * 60 * 1000;
export const MAX_COMMIT_AGE_MS = 24 * 60 * 60 * 1000;
export const TIME_WINDOW_MS = 10 * 60 * 1000;
export const DEPOSIT = 1_000_000n; // normal name's deposit (deposit mode)
const PDA_PREFIX = pad32Bytes("/LEE/v0.2/AccountId/PDA/");
const NAME_SEED_DOMAIN = utf8("logos-names/v5/name/");
const PAGE_SEED_DOMAIN = utf8("logos-names/v5/page/");
const DEPOSIT_SEED_DOMAIN = utf8("logos-names/v5/deposit/");
const COMMITMENT_SEED_DOMAIN = utf8("logos-names/v5/commit/");
const SETTINGS_SEED = utf8("logos-names/v5/config");
const OWNER_TAG_DOMAIN = utf8("logos-names/v5/owner");
const COMMITMENT_DOMAIN = utf8("logos-names/v5/commitment");
// shortlist
const LEAF_DOMAIN = utf8("logos-names/v5/short-leaf");
const NODE_DOMAIN = utf8("logos-names/v5/short-node");
const EMPTY_DOMAIN = utf8("logos-names/v5/short-empty");
const SHORTLIST_DEPTH = 16;
// TODO(v5.1): update to shortlist/v51/root.hex after the 1–4 regen finishes; the client-side
// proof check uses this, so a mismatch rejects all proofs. Do not deploy the webapp until updated.
export const SHORTLIST_ROOT = fromHex("66c75f76e42f1e87c80b8783c8a315dd84471041b029066bd5f8911d6c5a5e3c");
const TIER_NORMAL = 0, TIER_LOCKED = 1, TIER_THREE = 3;

function pad32Bytes(text) { const b = new Uint8Array(32); b.set(utf8(text)); return b; }
const b58 = (bytes) => base58Decode(bytes); // not used; kept symmetric
const acctBytes = (b58OrBytes) => (b58OrBytes instanceof Uint8Array ? b58OrBytes : base58Decode(stripPublic(b58OrBytes)));
const stripPublic = (s) => s.replace(/^Public\//, "").replace(/^Private\//, "");

// ---- address derivations (bytes), all SHA-256 ----
const pda = (programBytes, seed32) => sha256(concat(PDA_PREFIX, programBytes, seed32));
const derivedAddr = (programBytes, domain, id32) => pda(programBytes, sha256(concat(domain, id32)));
const recordAddr = (prog, id) => derivedAddr(prog, NAME_SEED_DOMAIN, id);
const pageAddr = (prog, id) => derivedAddr(prog, PAGE_SEED_DOMAIN, id);
const depositAddr = (prog, id) => derivedAddr(prog, DEPOSIT_SEED_DOMAIN, id);
const commitmentAddr = (prog, c32) => derivedAddr(prog, COMMITMENT_SEED_DOMAIN, c32);
const settingsAddr = (prog) => pda(prog, sha256(SETTINGS_SEED));
const ownerTag = (id32, account32) => sha256(concat(OWNER_TAG_DOMAIN, id32, account32));
const commitmentHash = (id32, holder32, secret32) => sha256(concat(COMMITMENT_DOMAIN, id32, holder32, secret32));

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

// borsh(LengthProof): enum NonMembership = 1; { left:Leaf, left_path:Vec<[32]>, right:Leaf, right_path:Vec<[32]>, left_index:u32 }
function encodeLeaf(w, leaf) { w.fixed(leaf.id, 32).u8(leaf.tier); }
function encodePath(w, path) { w.u32(path.length); for (const h of path) w.fixed(h, 32); }
function encodeLengthProof(proof) {
  const w = new Writer().u8(1); // NonMembership
  encodeLeaf(w, proof.left); encodePath(w, proof.leftPath);
  encodeLeaf(w, proof.right); encodePath(w, proof.rightPath);
  w.u32(proof.leftIndex);
  return w.done();
}

// ---- instruction encoders ----
// borsh(Instruction::Commit { commitment:[u8;32], now:u64 }) — enum idx 8
function commitInstruction(commitment32, now) {
  return new Writer().u8(8).fixed(commitment32, 32).u64(now).done();
}
// borsh(Instruction::Register { id, sealed, term, fee, now, secret, deposit, length }) — enum idx 0
function registerInstruction({ id, sealed, term, fee, now, secret, deposit, lengthProof }) {
  const w = new Writer().u8(0)
    .fixed(id, 32)
    .vec(sealed)
    .u8(term)
    .u128(fee)
    .u64(now)
    .fixed(secret, 32)
    .u8(deposit ? 1 : 0);
  w.bytes(encodeLengthProof(lengthProof));
  return w.done();
}

// ---- message builders ----
function commitMessage({ programBytes, commitAddr, payerBytes, nonce, now, commitment32, gasLimit }) {
  return {
    programAccountId: programBytes,
    shardSelectors: [shardSelector(commitAddr, programBytes), shardSelector(payerBytes, programBytes)],
    nonces: [BigInt(nonce)],
    instructionData: commitInstruction(commitment32, now),
    fee: feeDeclaration(payerBytes, gasLimit, 0n, maxFeeFor(gasLimit)),
  };
}
function registerMessage({ programBytes, recAddr, pgAddr, setAddr, commitAddr, payerBytes, depAddr, nonce, instr, gasLimit }) {
  return {
    programAccountId: programBytes,
    shardSelectors: [
      shardSelector(recAddr, programBytes),
      shardSelector(pgAddr, programBytes),
      shardSelector(setAddr, programBytes),
      shardSelector(commitAddr, programBytes),
      nativeBalance(payerBytes),   // payer, money
      nativeBalance(depAddr),      // deposit account, money
    ],
    nonces: [BigInt(nonce)],
    instructionData: instr,
    fee: feeDeclaration(payerBytes, gasLimit, 0n, maxFeeFor(gasLimit)),
  };
}

// ---- building the signed txs (no network) ----
export async function buildCommit({ privateKey, program, name, secret, now, gasLimit = COMMIT_GAS }) {
  const prog = acctBytes(program);
  const { id } = await deriveName(name);
  const payer = accountIdOfPrivateKey(privateKey);
  const holder = ownerTag(id, payer);
  const promised = commitmentHash(id, holder, secret);
  const commitAddr = commitmentAddr(prog, promised);
  const [nonce] = await getAccountsNonces(defaultRpc(program), [payer]);
  const msg = commitMessage({ programBytes: prog, commitAddr, payerBytes: payer, nonce, now, commitment32: promised, gasLimit });
  return { signed: signPublicTransaction(msg, [privateKey]), id, holder, promised };
}

export async function buildRegister({ privateKey, program, name, secret, now, term = 0, deposit = true, gasLimit = REGISTER_GAS }) {
  if (!name || name.length < MIN_NAME_LENGTH) throw Object.assign(new Error(`Names must be at least ${MIN_NAME_LENGTH} characters.`), { code: "too_short" });
  const prog = acctBytes(program);
  const { id, key } = await deriveName(name);
  const payer = accountIdOfPrivateKey(privateKey);
  const holder = ownerTag(id, payer);
  const promised = commitmentHash(id, holder, secret);
  // public name: publish the payer's account on the name (open payments). No private keys.
  const profile = encodeProfile({ name, account: formatAccountId(payer), npk: null, vpk: null, texts: {} });
  const sealed = await seal(key, profile, "record");
  const proof = await lengthProof(id);
  const fee = DEPOSIT;
  const instr = registerInstruction({ id, sealed, term, fee, now, secret, deposit, lengthProof: proof });
  const [nonce] = await getAccountsNonces(defaultRpc(program), [payer]);
  const msg = registerMessage({
    programBytes: prog,
    recAddr: recordAddr(prog, id), pgAddr: pageAddr(prog, id), setAddr: settingsAddr(prog),
    commitAddr: commitmentAddr(prog, promised), payerBytes: payer, depAddr: depositAddr(prog, id),
    nonce, instr, gasLimit,
  });
  return { signed: signPublicTransaction(msg, [privateKey]), id };
}

// The RPC the webapp reads/writes through. Defaults to the public CORS proxy.
let _rpc = "https://lez.buggers.online/lez";
export function setRpc(url) { _rpc = url; }
function defaultRpc() { return _rpc; }

// ---- the full public-register flow (commit → wait → register) ----
// onStep(stage, detail) reports progress. secretHex/commitMade let the caller
// resume across the 15-min wait (persist them). Returns {name, id, txHash}.
/** What claiming costs at the current fee market: the 1M deposit + the gas the wallet
 * must hold to send. `resume` = the commit is already paid, so only the register's cost. */
export async function estimateNeed(privateKey, resume = null) {
  const rpc = defaultRpc();
  const quote = await getFeeState(rpc);
  const regReserve = feeReserve({ gasLimit: REGISTER_GAS, dataBytes: 2400n, quote });
  const commitReserve = feeReserve({ gasLimit: COMMIT_GAS, dataBytes: 480n, quote });
  const need = DEPOSIT + regReserve + (resume ? 0n : commitReserve);
  const have = await getAccountBalance(rpc, accountIdOfPrivateKey(privateKey));
  return { need, have, deposit: DEPOSIT, enough: have >= need };
}

export async function registerPublic({ privateKey, program = "38jEfVV1RCyMzu1FGbYcCn5FqxCxKyY58VgzjruAh2gq", name, resume = null, onStep = () => {}, now = null }) {
  const rpc = defaultRpc();
  const clock = () => now ?? Date.now();
  if (!name || name.length < MIN_NAME_LENGTH) throw Object.assign(new Error(`Names must be at least ${MIN_NAME_LENGTH} characters.`), { code: "too_short" });
  // Fail early (before any tx) if the wallet can't cover deposit + fee.
  const { need, have, enough } = await estimateNeed(privateKey, resume);
  if (!enough) {
    const e = new Error(`Not enough tokens to claim: need about ${need} (a 1,000,000 refundable deposit plus the fee), you have ${have}. Get more test tokens first.`);
    e.code = "insufficient"; e.need = need; e.have = have;
    throw e;
  }
  let secret, made;
  if (resume) { secret = fromHex(resume.secretHex); made = resume.made; }
  else {
    secret = crypto.getRandomValues(new Uint8Array(32));
    made = clock();
    onStep("commit", "committing to the name");
    const { signed } = await buildCommit({ privateKey, program, name, secret, now: made });
    const h = await sendTransaction(rpc, signed);
    await waitForTransaction(rpc, signed.hash);
    onStep("committed", { secretHex: toHex(secret), made, tx: h });
  }
  const ready = made + MIN_COMMIT_AGE_MS;
  while (clock() < ready) {
    onStep("waiting", { remainingMs: ready - clock() });
    await new Promise((r) => setTimeout(r, Math.min(30000, Math.max(5000, ready - clock()))));
  }
  onStep("register", "claiming the name");
  const { signed, id } = await buildRegister({ privateKey, program, name, secret, now: clock(), deposit: true });
  const h = await sendTransaction(rpc, signed);
  const block = await waitForTransaction(rpc, signed.hash);
  onStep("done", { tx: h, block });
  return { name, id: toHex(id), txHash: signed.hash, block };
}

// ---- set-text: publish/update a text entry on a name you hold (SetRecord, idx 2) ----
// A single signed public tx: read the name's sealed profile, change one text entry,
// re-seal, write it back. No deposit, no commit, no proof. Accounts [record, owner].

function setRecordInstruction(id, sealed) {
  return new Writer().u8(2).fixed(id, 32).vec(sealed).done();
}
function setRecordMessage({ programBytes, recBytes, payerBytes, nonce, instr, gasLimit }) {
  return {
    programAccountId: programBytes,
    shardSelectors: [shardSelector(recBytes, programBytes), shardSelector(payerBytes, programBytes)],
    nonces: [BigInt(nonce)],
    instructionData: instr,
    fee: feeDeclaration(payerBytes, gasLimit, 0n, maxFeeFor(gasLimit)),
  };
}

/** Preflight/test only: build a SetRecord with arbitrary sealed bytes (skips the read). */
export async function buildSetRecordRaw({ privateKey, program, name, sealed, gasLimit = 120000n }) {
  const prog = acctBytes(program);
  const { id } = await deriveName(name);
  const payer = accountIdOfPrivateKey(privateKey);
  const [nonce] = await getAccountsNonces(defaultRpc(), [payer]);
  const msg = setRecordMessage({ programBytes: prog, recBytes: recordAddr(prog, id), payerBytes: payer, nonce, instr: setRecordInstruction(id, sealed), gasLimit });
  return { signed: signPublicTransaction(msg, [privateKey]), id };
}

/** Set (or clear, with empty value) a text entry on a name the wallet holds. */
export async function setText({ privateKey, program = "38jEfVV1RCyMzu1FGbYcCn5FqxCxKyY58VgzjruAh2gq", name, key, value, gasLimit = 120000n }) {
  const rpc = defaultRpc();
  const prog = acctBytes(program);
  const { id, key: nameKey } = await deriveName(name);
  const payer = accountIdOfPrivateKey(privateKey);
  const recBytes = recordAddr(prog, id);
  const shard = await readShard(rpc, program, formatAccountId(recBytes));
  if (!shard.length) { const e = new Error(`${name} isn't registered yet — claim it first.`); e.code = "unregistered"; throw e; }
  const rec = decodeNameRecord(shard);
  if (rec.ownerTag !== toHex(ownerTag(id, payer))) {
    const e = new Error(`This device's wallet doesn't hold ${name}.`); e.code = "not_owner"; throw e;
  }
  const plain = await openSealed(nameKey, rec.sealed, "record");
  const profile = plain ? decodeProfile(plain) : { name, account: null, npk: null, vpk: null, texts: {} };
  if (value) profile.texts[key] = value; else delete profile.texts[key];
  const sealed = await seal(nameKey, encodeProfile(profile), "record");
  const [nonce] = await getAccountsNonces(rpc, [payer]);
  const msg = setRecordMessage({ programBytes: prog, recBytes, payerBytes: payer, nonce, instr: setRecordInstruction(id, sealed), gasLimit });
  const signed = signPublicTransaction(msg, [privateKey]);
  const h = await sendTransaction(rpc, signed);
  const block = await waitForTransaction(rpc, signed.hash);
  return { txHash: signed.hash, block };
}
