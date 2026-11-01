// The registry's transactions on LEZ 0.4, as data: addresses, instructions and the messages
// that carry them. Nothing here touches the network, so every shape can be checked against
// the Rust encoder and engine (gateway/app-src/vectors). Follows core/src/lib.rs:
//
//  - A transaction is one `Message::Instruction` to the signer's own actor of the registry,
//    so the signer's actor is the root and every instruction starts with the tag 0.
//  - A transaction declares every actor it reaches, the balances of whoever pays and is paid
//    included. The engine refuses a credit to a balance that isn't declared.
//  - A public account that isn't on the chain yet needs evidence of where it comes from. Each
//    of the registry's accounts is listed by its seed; listing one that exists is harmless.
//    Signers need none. A plain account that hasn't been used can't be listed without its
//    public key, which is why a fee-mode treasury has to be on the chain already.

import {
  Writer, actor, base58Decode, concat, feeDeclaration, maxFeeFor, nativeBalance, pdaEvidence, sha256,
} from "./lez-tx.js";

const utf8 = (text) => new TextEncoder().encode(text);

function pad32(text) {
  const bytes = new Uint8Array(32);
  bytes.set(utf8(text));
  return bytes;
}

// "v0.2" here is LEZ's own prefix, and it hasn't changed.
const PDA_PREFIX = pad32("/LEE/v0.2/AccountId/PDA/");
const NAME_SEED_DOMAIN = utf8("logos-names/v5/name/");
const PAGE_SEED_DOMAIN = utf8("logos-names/v5/page/");
const DEPOSIT_SEED_DOMAIN = utf8("logos-names/v5/deposit/");
const COMMITMENT_SEED_DOMAIN = utf8("logos-names/v5/commit/");
const SETTINGS_SEED_DOMAIN = utf8("logos-names/v5/config");
const OWNER_TAG_DOMAIN = utf8("logos-names/v5/owner");
const COMMITMENT_DOMAIN = utf8("logos-names/v5/commitment");
const CLAIM_LEAF_DOMAIN = utf8("logos-names/v6/claim-leaf");
const CLAIM_NODE_DOMAIN = utf8("logos-names/v6/claim-node");

// ---------- seeds and addresses (all bytes) ----------

const pda = (program, seed) => sha256(concat(PDA_PREFIX, program, seed));
const seedOf = (domain, id) => sha256(concat(domain, id));

export const nameSeed = (id) => seedOf(NAME_SEED_DOMAIN, id);
export const pageSeed = (id) => seedOf(PAGE_SEED_DOMAIN, id);
export const depositSeed = (id) => seedOf(DEPOSIT_SEED_DOMAIN, id);
export const commitmentSeed = (promised) => seedOf(COMMITMENT_SEED_DOMAIN, promised);
export const settingsSeed = () => sha256(SETTINGS_SEED_DOMAIN);

export const recordAddress = (program, id) => pda(program, nameSeed(id));
export const pageAddress = (program, id) => pda(program, pageSeed(id));
export const depositAddress = (program, id) => pda(program, depositSeed(id));
export const commitmentAddress = (program, promised) => pda(program, commitmentSeed(promised));
export const settingsAddress = (program) => pda(program, settingsSeed());

export const ownerTag = (id, account) => sha256(concat(OWNER_TAG_DOMAIN, id, account));
export const commitmentHash = (id, holder, secret) => sha256(concat(COMMITMENT_DOMAIN, id, holder, secret));

/** An account ID as bytes, from base58 (`Public/` or not) or from bytes. */
export function accountBytes(account) {
  if (account instanceof Uint8Array) return account;
  return base58Decode(account.replace(/^Public\//, ""));
}

// ---------- claims of names held on an earlier chain ----------

const compareHashes = (a, b) => {
  for (let i = 0; i < 32; i += 1) {
    if (a[i] !== b[i]) return a[i] - b[i];
  }
  return 0;
};

function claimNode(left, right) {
  const [low, high] = compareHashes(left, right) <= 0 ? [left, right] : [right, left];
  return sha256(concat(CLAIM_NODE_DOMAIN, low, high));
}

/** The leaf of a name on the claims list: its record's address on the earlier chain, and its holder's tag. */
export const claimLeaf = (oldRecord, holder) => sha256(concat(CLAIM_LEAF_DOMAIN, oldRecord, holder));

/** The root a claim's `proof` leads to from `leaf`. Pairs hash in sorted order, so it needs no directions. */
export const claimRootFrom = (leaf, proof) => proof.reduce(claimNode, leaf);

/**
 * What the registry's `claim_holds` decides, for checking a proof before paying to send it. A
 * claim is keyed on the old registry's address, so `claimsProgram` comes from the settings.
 */
export function claimHolds({ claimsRoot, claimsProgram, id, account, proof }) {
  const leaf = claimLeaf(recordAddress(claimsProgram, id), ownerTag(id, account));
  return compareHashes(claimRootFrom(leaf, proof), claimsRoot) === 0;
}

const claimLevel = (level) => {
  const next = [];
  for (let at = 0; at < level.length; at += 2) {
    next.push(at + 1 < level.length ? claimNode(level[at], level[at + 1]) : level[at]);
  }
  return next;
};

/** The root of the claims list `leaves`; all zeros for an empty list. An odd last node moves up as it is. */
export function claimRoot(leaves) {
  if (leaves.length === 0) return new Uint8Array(32);
  let level = leaves;
  while (level.length > 1) level = claimLevel(level);
  return level[0];
}

/** The proof for the leaf at `index` of `leaves`. */
export function claimProof(leaves, index) {
  const proof = [];
  let level = leaves;
  let at = index;
  while (level.length > 1) {
    const sibling = level[at ^ 1];
    if (sibling) proof.push(sibling);
    level = claimLevel(level);
    at = Math.floor(at / 2);
  }
  return proof;
}

// ---------- instructions: borsh(Message::Instruction(..)) ----------

const instruction = (variant) => new Writer().u8(0).u8(variant);

function writeLeaf(w, leaf) {
  w.fixed(leaf.id, 32).u8(leaf.tier);
}

function writePath(w, path) {
  w.u32(path.length);
  for (const hash of path) w.fixed(hash, 32);
}

/** borsh(LengthProof). Membership is variant 0, NonMembership 1. */
function writeLengthProof(w, proof) {
  if (proof.kind === "Membership") {
    w.u8(0).u32(proof.index).u8(proof.tier);
    writePath(w, proof.path);
    return;
  }
  w.u8(1);
  writeLeaf(w, proof.left);
  writePath(w, proof.leftPath);
  writeLeaf(w, proof.right);
  writePath(w, proof.rightPath);
  w.u32(proof.leftIndex);
}

const writeTreasury = (w, treasury) => w.option(treasury, (out, account) => out.fixed(account, 32));

export function registerInstruction({ id, sealed, term, fee, treasury = null, now, secret, deposit, lengthProof }) {
  const w = instruction(0).fixed(id, 32).vec(sealed).u8(term).u128(fee);
  writeTreasury(w, treasury);
  w.u64(now).fixed(secret, 32).u8(deposit ? 1 : 0);
  writeLengthProof(w, lengthProof);
  return w.done();
}

export function renewInstruction({ id, term, fee, treasury = null, now }) {
  const w = instruction(1).fixed(id, 32).u8(term).u128(fee);
  writeTreasury(w, treasury);
  return w.u64(now).done();
}

export const setRecordInstruction = ({ id, sealed }) => instruction(2).fixed(id, 32).vec(sealed).done();
export const setPageInstruction = ({ id, sealed }) => instruction(3).fixed(id, 32).vec(sealed).done();
export const transferInstruction = ({ id, newOwner }) => instruction(4).fixed(id, 32).fixed(newOwner, 32).done();
export const releaseInstruction = ({ id, refund }) => instruction(5).fixed(id, 32).u128(refund).done();
export const setPrimaryInstruction = ({ name }) => instruction(6).vec(utf8(name)).done();
export const commitInstruction = ({ commitment, now }) => instruction(8).fixed(commitment, 32).u64(now).done();

export function claimInstruction({ id, sealed, now, proof }) {
  const w = instruction(9).fixed(id, 32).vec(sealed).u64(now);
  writePath(w, proof);
  return w.done();
}

// ---------- messages: what lez-tx.js encodes and signs ----------

/** `signer` sends `data` to its own actor of the registry, and pays the fee. */
function message({ program, signer, nonce, gasLimit, data, actors = [], evidence = [] }) {
  return {
    to: actor(signer, program),
    data,
    actors,
    nonces: [{ accountId: signer, nonce: BigInt(nonce) }],
    fee: feeDeclaration(signer, gasLimit, 0n, maxFeeFor(gasLimit)),
    evidence,
  };
}

export function commitMessage({ program, signer, nonce, gasLimit, now, commitment }) {
  return message({
    program, signer, nonce, gasLimit,
    data: commitInstruction({ commitment, now }),
    actors: [actor(commitmentAddress(program, commitment), program)],
    evidence: [pdaEvidence(program, commitmentSeed(commitment))],
  });
}

/** `deposit` pays `fee` into the name's deposit account; otherwise `treasury` gets it. */
export function registerMessage({ program, signer, nonce, gasLimit, id, secret, sealed, term = 0, fee, treasury = null, now, deposit, lengthProof }) {
  const promised = commitmentHash(id, ownerTag(id, signer), secret);
  const paid = deposit ? depositAddress(program, id) : treasury;
  return message({
    program, signer, nonce, gasLimit,
    data: registerInstruction({ id, sealed, term, fee, treasury, now, secret, deposit, lengthProof }),
    actors: [
      actor(recordAddress(program, id), program),
      actor(pageAddress(program, id), program),
      actor(settingsAddress(program), program),
      actor(commitmentAddress(program, promised), program),
      ...(fee > 0n ? [nativeBalance(signer), nativeBalance(paid)] : []),
    ],
    evidence: [
      pdaEvidence(program, nameSeed(id)),
      pdaEvidence(program, pageSeed(id)),
      pdaEvidence(program, settingsSeed()),
      pdaEvidence(program, commitmentSeed(promised)),
      ...(deposit && fee > 0n ? [pdaEvidence(program, depositSeed(id))] : []),
    ],
  });
}

export function renewMessage({ program, signer, nonce, gasLimit, id, term, fee, treasury = null, now }) {
  return message({
    program, signer, nonce, gasLimit,
    data: renewInstruction({ id, term, fee, treasury, now }),
    actors: [
      actor(recordAddress(program, id), program),
      actor(settingsAddress(program), program),
      ...(fee > 0n ? [nativeBalance(signer), nativeBalance(treasury)] : []),
    ],
    evidence: [pdaEvidence(program, nameSeed(id)), pdaEvidence(program, settingsSeed())],
  });
}

export function setRecordMessage({ program, signer, nonce, gasLimit, id, sealed }) {
  return message({
    program, signer, nonce, gasLimit,
    data: setRecordInstruction({ id, sealed }),
    actors: [actor(recordAddress(program, id), program)],
    evidence: [pdaEvidence(program, nameSeed(id))],
  });
}

const nameActors = (program, id) => [actor(recordAddress(program, id), program), actor(pageAddress(program, id), program)];
const nameEvidence = (program, id) => [pdaEvidence(program, nameSeed(id)), pdaEvidence(program, pageSeed(id))];

export function setPageMessage({ program, signer, nonce, gasLimit, id, sealed }) {
  return message({
    program, signer, nonce, gasLimit,
    data: setPageInstruction({ id, sealed }),
    actors: nameActors(program, id),
    evidence: nameEvidence(program, id),
  });
}

export function transferMessage({ program, signer, nonce, gasLimit, id, newOwner }) {
  return message({
    program, signer, nonce, gasLimit,
    data: transferInstruction({ id, newOwner }),
    actors: nameActors(program, id),
    evidence: nameEvidence(program, id),
  });
}

/** `refund` is the deposit account's whole balance, 0 for a name with no deposit. */
export function releaseMessage({ program, signer, nonce, gasLimit, id, refund }) {
  return message({
    program, signer, nonce, gasLimit,
    data: releaseInstruction({ id, refund }),
    actors: [
      ...nameActors(program, id),
      ...(refund > 0n ? [nativeBalance(depositAddress(program, id)), nativeBalance(signer)] : []),
    ],
    evidence: nameEvidence(program, id),
  });
}

export function setPrimaryMessage({ program, signer, nonce, gasLimit, name }) {
  return message({ program, signer, nonce, gasLimit, data: setPrimaryInstruction({ name }) });
}

export function claimMessage({ program, signer, nonce, gasLimit, id, sealed, now, proof }) {
  return message({
    program, signer, nonce, gasLimit,
    data: claimInstruction({ id, sealed, now, proof }),
    actors: [...nameActors(program, id), actor(settingsAddress(program), program)],
    evidence: [...nameEvidence(program, id), pdaEvidence(program, settingsSeed())],
  });
}
