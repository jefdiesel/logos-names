// The registry's transactions on LEZ 0.4 (LNS v8), as data: addresses, instructions and the
// messages that carry them. Nothing here touches the network, so every shape can be checked
// against the Rust encoder and engine (gateway/app-src/vectors). Follows core/src/lib.rs:
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
  AUTHORITY_SIGNER, Writer, actor, base58Decode, concat, feeDeclaration, maxFeeFor, nativeBalance, pdaEvidence, sha256,
  signerActor,
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
const TIER_DOMAIN = utf8("logos-names/v7/tier/");
const SUBNAME_SEED_DOMAIN = utf8("logos-names/v7/subname/");
const SUBPAGE_SEED_DOMAIN = utf8("logos-names/v7/subpage/");
const SUBNAME_COMMITMENT_DOMAIN = utf8("logos-names/v7/sub-commitment");
const GEN_REGISTER_DOMAIN = utf8("logos-names/v7/gen/register");
const GEN_CLAIM_DOMAIN = utf8("logos-names/v7/gen/claim");
const GEN_TRANSFER_DOMAIN = utf8("logos-names/v7/gen/transfer");
const MARKET_BUCKET_DOMAIN = utf8("logos-names/v8/market/");
const SITE_DIR_HEADER_DOMAIN = utf8("logos-names/v8/sites-meta");
const SITE_BUCKET_DOMAIN = utf8("logos-names/v8/sites/");
const NFT_TOKEN_ID_DOMAIN = utf8("logos-names/v8/nft-token/");
const NFT_TOKEN_SEED_DOMAIN = utf8("lez-nft/v3/token/");
const NFT_DEED_SEED_DOMAIN = utf8("lez-nft/v3/deed/");

// ---------- v7 / v8 pricing, marketplace and re-release constants ----------

export const PRICE_TIER_5 = 0;
export const PRICE_TIER_6_7 = 1;
export const PRICE_TIER_8_PLUS = 2;

export const TERM_30D = 0;
export const TERM_1Y = 1;
export const TERM_5Y = 2;
export const TERM_PERMANENT = 3;

export const GRACE_MS = 2_592_000_000;
export const DAY_MS = 86_400_000n;
export const PREMIUM_DAYS = 21n;
export const PREMIUM_DURATION_MS = PREMIUM_DAYS * DAY_MS;
export const MIN_SALE_PRICE = 40n;
export const MARKET_BUCKETS = 16;
export const SITE_BUCKETS = 16;
export const MARKET_BUCKET_MAX = 64;
export const SITE_BUCKET_MAX = 32;

export const DEFAULT_PRICES = [
  [2_000_000n, 20_000_000n, 80_000_000n, 400_000_000n],
  [500_000n, 5_000_000n, 20_000_000n, 100_000_000n],
  [100_000n, 1_000_000n, 4_000_000n, 20_000_000n],
];
export const DEFAULT_PREMIUM_START = 4_000_000_000n;

/** Length-based pricing tier for a top-level name: 0 (<=5), 1 (6-7), 2 (8+). */
export function priceTierForLen(len) {
  if (len <= 5) return PRICE_TIER_5;
  if (len <= 7) return PRICE_TIER_6_7;
  return PRICE_TIER_8_PLUS;
}

/**
 * Secondary marketplace split: fixed 2.5% treasury fee rounded up (`ceil(price / 40)`),
 * seller receives `price - fee`. Overflow-free for all BigInt prices >= 40n.
 */
export function saleSplit(price) {
  const p = BigInt(price);
  if (p < MIN_SALE_PRICE) throw new Error(`sale price must be at least ${MIN_SALE_PRICE}`);
  const fee = (p + 39n) / 40n;
  return { seller: p - fee, fee };
}

/**
 * 21-day falling re-release premium after a name lapses past its 30-day grace period.
 * Halves every 24h with linear intra-day interpolation, reaching 0 continuously at day 21.
 */
export function rereleasePremium(premiumStart, elapsedMs) {
  const start = BigInt(premiumStart);
  const elapsed = BigInt(elapsedMs);
  if (start <= 0n || elapsed < 0n || elapsed >= PREMIUM_DURATION_MS) return 0n;
  const day = elapsed / DAY_MS;
  const rem = elapsed % DAY_MS;
  const hi = start >> day;
  const lo = day + 1n === PREMIUM_DAYS ? 0n : start >> (day + 1n);
  return hi - ((hi - lo) * rem) / DAY_MS;
}

// ---------- seeds and addresses (all bytes) ----------

const u16Le = (n) => Uint8Array.of(n & 0xff, (n >>> 8) & 0xff);
function u64Le(n) {
  const out = new Uint8Array(8);
  new DataView(out.buffer).setBigUint64(0, BigInt(n), true);
  return out;
}

const pda = (program, seed) => sha256(concat(PDA_PREFIX, program, seed));
const seedOf = (domain, id) => sha256(concat(domain, id));

export const tieredId = (baseId, priceTier) => sha256(concat(TIER_DOMAIN, Uint8Array.of(priceTier & 0xff), baseId));

export const nameSeed = (id) => seedOf(NAME_SEED_DOMAIN, id);
export const pageSeed = (id) => seedOf(PAGE_SEED_DOMAIN, id);
export const depositSeed = (id) => seedOf(DEPOSIT_SEED_DOMAIN, id);
export const commitmentSeed = (promised) => seedOf(COMMITMENT_SEED_DOMAIN, promised);
export const settingsSeed = () => sha256(SETTINGS_SEED_DOMAIN);
export const subnameSeed = (parentSlotId, subId) => sha256(concat(SUBNAME_SEED_DOMAIN, parentSlotId, subId));
export const subpageSeed = (parentSlotId, subId) => sha256(concat(SUBPAGE_SEED_DOMAIN, parentSlotId, subId));
export const marketBucketIndex = (slotId) => slotId[0] & 0x0f;
export const marketBucketSeed = (bucket) => sha256(concat(MARKET_BUCKET_DOMAIN, Uint8Array.of(bucket & 0xff)));
export const siteBucketIndex = (slotId) => slotId[0] & 0x0f;
export const siteDirHeaderSeed = () => sha256(SITE_DIR_HEADER_DOMAIN);
export const siteBucketSeed = (bucket, page) => sha256(concat(SITE_BUCKET_DOMAIN, Uint8Array.of(bucket & 0xff), u16Le(page)));
export const nftTokenId = (slotId, mintGen) => sha256(concat(NFT_TOKEN_ID_DOMAIN, slotId, mintGen));
export const nftTokenSeed = (tokenId) => sha256(concat(NFT_TOKEN_SEED_DOMAIN, tokenId));
export const nftDeedSeed = (tokenId) => sha256(concat(NFT_DEED_SEED_DOMAIN, tokenId));

export const recordAddress = (program, id) => pda(program, nameSeed(id));
export const pageAddress = (program, id) => pda(program, pageSeed(id));
export const depositAddress = (program, id) => pda(program, depositSeed(id));
export const commitmentAddress = (program, promised) => pda(program, commitmentSeed(promised));
export const settingsAddress = (program) => pda(program, settingsSeed());
export const subnameRecordAddress = (program, parentSlotId, subId) => pda(program, subnameSeed(parentSlotId, subId));
export const subnamePageAddress = (program, parentSlotId, subId) => pda(program, subpageSeed(parentSlotId, subId));
export const marketBucketAddress = (program, bucket) => pda(program, marketBucketSeed(bucket));
export const siteDirHeaderAddress = (program) => pda(program, siteDirHeaderSeed());
export const siteBucketAddress = (program, bucket, page) => pda(program, siteBucketSeed(bucket, page));
export const nftTokenAddress = (nftProgram, tokenId) => pda(nftProgram, nftTokenSeed(tokenId));

export const ownerTag = (id, account) => sha256(concat(OWNER_TAG_DOMAIN, id, account));
export const commitmentHash = (id, holder, secret) => sha256(concat(COMMITMENT_DOMAIN, id, holder, secret));
export const subnameCommitmentHash = (parentSlotId, subId, holder, secret) =>
  sha256(concat(SUBNAME_COMMITMENT_DOMAIN, parentSlotId, subId, holder, secret));
export const registerGeneration = (slotId, holder, now, secret) =>
  sha256(concat(GEN_REGISTER_DOMAIN, slotId, holder, u64Le(now), secret));
export const claimGeneration = (slotId, holder, now) =>
  sha256(concat(GEN_CLAIM_DOMAIN, slotId, holder, u64Le(now)));
export const transferGeneration = (prevGeneration, newOwner) =>
  sha256(concat(GEN_TRANSFER_DOMAIN, prevGeneration, newOwner));

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

export const SITE_DIR_KEEP = { kind: "Keep" };
export const SITE_DIR_REMOVE = { kind: "Remove" };
export const siteDirUpsert = ({ name, title, page, now }) => ({ kind: "Upsert", name, title, page, now });

function writeSiteDirOp(w, op = SITE_DIR_KEEP) {
  if (!op || op.kind === "Keep") {
    w.u8(0);
    return;
  }
  if (op.kind === "Upsert") {
    w.u8(1).vec(utf8(op.name)).vec(utf8(op.title)).u8(op.page & 0xff).u8((op.page >>> 8) & 0xff).u64(op.now);
    return;
  }
  if (op.kind === "Remove") {
    w.u8(2);
    return;
  }
  throw new Error(`unknown SiteDirOp: ${op.kind}`);
}

const writeAccountOpt = (w, account) => w.option(account, (out, acc) => out.fixed(acc, 32));
const writeU64Opt = (w, value) => w.option(value, (out, v) => out.u64(v));
const writeStringOpt = (w, value) => w.option(value, (out, v) => out.vec(utf8(v)));

/** Variant 0: `Register` (`id` is `base_id`; `priceTier` derives `slot_id` on chain). */
export function registerInstruction({ id, sealed, term, fee, treasury = null, now, secret, deposit, lengthProof, priceTier = PRICE_TIER_8_PLUS }) {
  const w = instruction(0).fixed(id, 32).vec(sealed).u8(term).u128(fee);
  writeAccountOpt(w, treasury);
  w.u64(now).fixed(secret, 32).u8(deposit ? 1 : 0);
  writeLengthProof(w, lengthProof);
  return w.u8(priceTier).done();
}

/** Variant 1: `Renew` (`id` is `slot_id`; `term` accepts 0..3 including permanent). */
export function renewInstruction({ id, term, fee, treasury = null, now }) {
  const w = instruction(1).fixed(id, 32).u8(term).u128(fee);
  writeAccountOpt(w, treasury);
  return w.u64(now).done();
}

/** Variant 2: `SetRecord` (v8 adds `siteDir: SiteDirOp`). */
export function setRecordInstruction({ id, sealed, siteDir = SITE_DIR_KEEP }) {
  const w = instruction(2).fixed(id, 32).vec(sealed);
  writeSiteDirOp(w, siteDir);
  return w.done();
}

/** Variant 3: `SetPage` (v8 adds `siteDir: SiteDirOp`). */
export function setPageInstruction({ id, sealed, siteDir = SITE_DIR_KEEP }) {
  const w = instruction(3).fixed(id, 32).vec(sealed);
  writeSiteDirOp(w, siteDir);
  return w.done();
}

/** Variant 4: `Transfer` (v8 adds `now: u64`). */
export const transferInstruction = ({ id, newOwner, now }) =>
  instruction(4).fixed(id, 32).fixed(newOwner, 32).u64(now).done();

/** Variant 5: `Release`. */
export const releaseInstruction = ({ id, refund }) => instruction(5).fixed(id, 32).u128(refund).done();

/** Variant 6: `SetPrimary`. */
export const setPrimaryInstruction = ({ name }) => instruction(6).vec(utf8(name)).done();

/** Variant 7: `Configure` (v8: 3x4 `prices`, `premiumStart`, `nftProgram`). */
export function configureInstruction({
  admin, treasury, prices, mode, deposits, claimsRoot, claimsUntil, claimsProgram, premiumStart, nftProgram,
}) {
  const w = instruction(7).fixed(admin, 32).fixed(treasury, 32);
  for (let tier = 0; tier < 3; tier += 1) {
    for (let term = 0; term < 4; term += 1) w.u128(prices[tier][term]);
  }
  w.u8(mode).u128(deposits[0]).u128(deposits[1]);
  w.fixed(claimsRoot, 32).u64(claimsUntil).fixed(claimsProgram, 32);
  w.u128(premiumStart).fixed(nftProgram, 32);
  return w.done();
}

/** Variant 8: `Commit`. */
export const commitInstruction = ({ commitment, now }) => instruction(8).fixed(commitment, 32).u64(now).done();

/** Variant 9: `Claim` (v7/v8 appends `priceTier: u8`). */
export function claimInstruction({ id, sealed, now, proof, priceTier = PRICE_TIER_8_PLUS }) {
  const w = instruction(9).fixed(id, 32).vec(sealed).u64(now);
  writePath(w, proof);
  return w.u8(priceTier).done();
}

/** Variant 10: `CommitPublic`. */
export const commitPublicInstruction = ({ commitment, now }) => instruction(10).fixed(commitment, 32).u64(now).done();

/** Variant 11 (`0x0b`): `SetPrice`. */
export const setPriceInstruction = ({ priceTier, term, price }) =>
  instruction(11).u8(priceTier).u8(term).u128(price).done();

/** Variant 12 (`0x0c`): `SetSubnameSale`. */
export function setSubnameSaleInstruction({ id, sale = null }) {
  const w = instruction(12).fixed(id, 32);
  w.option(sale, (out, s) => out.u128(s.price).fixed(s.payout, 32));
  return w.done();
}

/** Variant 13 (`0x0d`): `CreateSubname`. */
export const createSubnameInstruction = ({ parentId, subId, owner, revocable, sealed, now }) =>
  instruction(13).fixed(parentId, 32).fixed(subId, 32).fixed(owner, 32).u8(revocable ? 1 : 0).vec(sealed).u64(now).done();

/** Variant 14 (`0x0e`): `RevokeSubname`. */
export const revokeSubnameInstruction = ({ parentId, subId, now }) =>
  instruction(14).fixed(parentId, 32).fixed(subId, 32).u64(now).done();

/** Variant 15 (`0x0f`): `BuySubname`. */
export function buySubnameInstruction({ parentId, subId, sealed, price, payout = null, now, secret }) {
  const w = instruction(15).fixed(parentId, 32).fixed(subId, 32).vec(sealed).u128(price);
  writeAccountOpt(w, payout);
  return w.u64(now).fixed(secret, 32).done();
}

/** Variant 16 (`0x10`): `SetSubnameRecord`. */
export function setSubnameRecordInstruction({ parentId, subId, sealed, now = null }) {
  const w = instruction(16).fixed(parentId, 32).fixed(subId, 32).vec(sealed);
  writeU64Opt(w, now);
  return w.done();
}

/** Variant 17 (`0x11`): `SetSubnamePage`. */
export function setSubnamePageInstruction({ parentId, subId, sealed, now = null }) {
  const w = instruction(17).fixed(parentId, 32).fixed(subId, 32).vec(sealed);
  writeU64Opt(w, now);
  return w.done();
}

/** Variant 18 (`0x12`): `TransferSubname`. */
export function transferSubnameInstruction({ parentId, subId, newOwner, now = null }) {
  const w = instruction(18).fixed(parentId, 32).fixed(subId, 32).fixed(newOwner, 32);
  writeU64Opt(w, now);
  return w.done();
}

/** Variant 19 (`0x13`): `ReleaseSubname`. */
export function releaseSubnameInstruction({ parentId, subId, now = null }) {
  const w = instruction(19).fixed(parentId, 32).fixed(subId, 32);
  writeU64Opt(w, now);
  return w.done();
}

/** Variant 20 (`0x14`): `SetPremium`. */
export const setPremiumInstruction = ({ premiumStart }) => instruction(20).u128(premiumStart).done();

/** Variant 21 (`0x15`): `List`. */
export function listInstruction({ id, price, payTo, expiresAt = null, now, displayName = null }) {
  const w = instruction(21).fixed(id, 32).u128(price).fixed(payTo, 32);
  writeU64Opt(w, expiresAt);
  w.u64(now);
  writeStringOpt(w, displayName);
  return w.done();
}

/** Variant 22 (`0x16`): `Cancel`. */
export const cancelInstruction = ({ id }) => instruction(22).fixed(id, 32).done();

/** Variant 23 (`0x17`): `Buy`. */
export const buyInstruction = ({ id, price, payTo, treasury, expectedNonce, now }) =>
  instruction(23).fixed(id, 32).u128(price).fixed(payTo, 32).fixed(treasury, 32).u64(expectedNonce).u64(now).done();

/** Variant 24 (`0x18`): `ShieldDeed`. */
export const shieldDeedInstruction = ({ id, deedAccount, now }) =>
  instruction(24).fixed(id, 32).fixed(deedAccount, 32).u64(now).done();

/** Variant 25 (`0x19`): `UnshieldDeed`. */
export const unshieldDeedInstruction = ({ id }) => instruction(25).fixed(id, 32).done();

/** Variant 26 (`0x1a`): `RefreshShieldedDeed`. */
export const refreshShieldedDeedInstruction = ({ id }) => instruction(26).fixed(id, 32).done();

/** Size of a borsh-encoded `Commitment` account state (`core/src/lib.rs`): version(1) + made(8) + public(1). */
export const COMMITMENT_BYTES = 10;

/** Decodes a commitment actor's 10-byte state: `{ version: u8, made: u64 LE, public: bool }`. */
export function decodeCommitment(bytes) {
  if (!(bytes instanceof Uint8Array) || bytes.length !== COMMITMENT_BYTES) {
    throw new Error(`invalid commitment length: expected ${COMMITMENT_BYTES} bytes, got ${bytes?.length ?? 0}`);
  }
  const flag = bytes[9];
  if (flag !== 0 && flag !== 1) throw new Error(`invalid commitment public flag: ${flag}`);
  const made = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getBigUint64(1, true);
  return { version: bytes[0], made: Number(made), public: flag === 1 };
}

// ---------- messages: what lez-tx.js encodes and signs ----------

/** `signer` sends `data` to its own actor of the registry (with `Authority.Signer`), and pays the fee. */
function message({ program, signer, nonce, gasLimit, data, actors = [], evidence = [] }) {
  return {
    to: signerActor(signer, program),
    data,
    actors,
    nonces: [{ accountId: signer, nonce: BigInt(nonce) }],
    fee: feeDeclaration(signer, gasLimit, 0n, maxFeeFor(gasLimit)),
    evidence,
  };
}

const nameActors = (program, id) => [actor(recordAddress(program, id), program), actor(pageAddress(program, id), program)];
const nameEvidence = (program, id) => [pdaEvidence(program, nameSeed(id)), pdaEvidence(program, pageSeed(id))];

function siteCleanup(program, slotId, sitePage) {
  if (sitePage === null || sitePage === undefined) return { actors: [], evidence: [] };
  const bucket = siteBucketIndex(slotId);
  return {
    actors: [actor(siteDirHeaderAddress(program), program), actor(siteBucketAddress(program, bucket, sitePage), program)],
    evidence: [pdaEvidence(program, siteDirHeaderSeed()), pdaEvidence(program, siteBucketSeed(bucket, sitePage))],
  };
}

function marketCleanup(program, slotId, inMarket) {
  if (!inMarket) return { actors: [], evidence: [] };
  const bucket = marketBucketIndex(slotId);
  return {
    actors: [actor(marketBucketAddress(program, bucket), program)],
    evidence: [pdaEvidence(program, marketBucketSeed(bucket))],
  };
}

function siteOpExtras(program, slotId, siteDir, existingSitePage = null) {
  if (!siteDir || siteDir.kind === "Keep") return { actors: [], evidence: [] };
  const bucket = siteBucketIndex(slotId);
  const pages = new Set();
  if (siteDir.kind === "Upsert") pages.add(siteDir.page);
  if (existingSitePage !== null && existingSitePage !== undefined) pages.add(existingSitePage);
  const actors = [actor(siteDirHeaderAddress(program), program)];
  const evidence = [pdaEvidence(program, siteDirHeaderSeed())];
  for (const page of pages) {
    actors.push(actor(siteBucketAddress(program, bucket, page), program));
    evidence.push(pdaEvidence(program, siteBucketSeed(bucket, page)));
  }
  return { actors, evidence };
}

export function commitMessage({ program, signer, nonce, gasLimit, now, commitment }) {
  return message({
    program, signer, nonce, gasLimit,
    data: commitInstruction({ commitment, now }),
    actors: [actor(commitmentAddress(program, commitment), program)],
    evidence: [pdaEvidence(program, commitmentSeed(commitment))],
  });
}

export function commitPublicMessage({ program, signer, nonce, gasLimit, now, commitment }) {
  return message({
    program, signer, nonce, gasLimit,
    data: commitPublicInstruction({ commitment, now }),
    actors: [actor(commitmentAddress(program, commitment), program)],
    evidence: [pdaEvidence(program, commitmentSeed(commitment))],
  });
}

/**
 * Registers a top-level name (`id` is `base_id`; `slotId = tieredId(id, priceTier)`).
 * `deposit` pays `fee` into the name's deposit account; otherwise `treasury` gets it.
 */
export function registerMessage({
  program, signer, nonce, gasLimit, id, priceTier = PRICE_TIER_8_PLUS, secret, sealed, term = 0,
  fee, treasury = null, now, deposit, lengthProof,
  nftProgram = null, prevNftTokenId = null, prevSitePage = null, prevInMarket = false,
}) {
  const slotId = tieredId(id, priceTier);
  const holder = ownerTag(slotId, signer);
  const promised = commitmentHash(slotId, holder, secret);
  const paid = deposit ? depositAddress(program, slotId) : treasury;
  const newTokenId = nftProgram ? nftTokenId(slotId, registerGeneration(slotId, holder, now, secret)) : null;
  const site = siteCleanup(program, slotId, prevSitePage);
  const market = marketCleanup(program, slotId, prevInMarket);
  return message({
    program, signer, nonce, gasLimit,
    data: registerInstruction({ id, sealed, term, fee, treasury, now, secret, deposit, lengthProof, priceTier }),
    actors: [
      actor(recordAddress(program, slotId), program),
      actor(pageAddress(program, slotId), program),
      actor(settingsAddress(program), program),
      actor(commitmentAddress(program, promised), program),
      ...(fee > 0n ? [nativeBalance(signer, AUTHORITY_SIGNER), nativeBalance(paid)] : []),
      ...(nftProgram && newTokenId ? [actor(nftTokenAddress(nftProgram, newTokenId), nftProgram)] : []),
      ...(nftProgram && prevNftTokenId ? [actor(nftTokenAddress(nftProgram, prevNftTokenId), nftProgram)] : []),
      ...site.actors,
      ...market.actors,
    ],
    evidence: [
      pdaEvidence(program, nameSeed(slotId)),
      pdaEvidence(program, pageSeed(slotId)),
      pdaEvidence(program, settingsSeed()),
      pdaEvidence(program, commitmentSeed(promised)),
      ...(deposit && fee > 0n ? [pdaEvidence(program, depositSeed(slotId))] : []),
      ...(nftProgram && newTokenId ? [pdaEvidence(nftProgram, nftTokenSeed(newTokenId))] : []),
      ...site.evidence,
      ...market.evidence,
    ],
  });
}

export function renewMessage({
  program, signer, nonce, gasLimit, id, term, fee, treasury = null, now,
  nftProgram = null, nftTokenId: tokenId = null, sitePage = null, inMarket = false,
}) {
  const site = siteCleanup(program, id, sitePage);
  const market = marketCleanup(program, id, inMarket);
  return message({
    program, signer, nonce, gasLimit,
    data: renewInstruction({ id, term, fee, treasury, now }),
    actors: [
      actor(recordAddress(program, id), program),
      actor(settingsAddress(program), program),
      ...(fee > 0n ? [nativeBalance(signer, AUTHORITY_SIGNER), nativeBalance(treasury)] : []),
      ...(nftProgram && tokenId ? [actor(nftTokenAddress(nftProgram, tokenId), nftProgram)] : []),
      ...site.actors,
      ...market.actors,
    ],
    evidence: [
      pdaEvidence(program, nameSeed(id)),
      pdaEvidence(program, settingsSeed()),
      ...site.evidence,
      ...market.evidence,
    ],
  });
}

export function setRecordMessage({ program, signer, nonce, gasLimit, id, sealed, siteDir = SITE_DIR_KEEP, existingSitePage = null }) {
  const site = siteOpExtras(program, id, siteDir, existingSitePage);
  return message({
    program, signer, nonce, gasLimit,
    data: setRecordInstruction({ id, sealed, siteDir }),
    actors: [actor(recordAddress(program, id), program), ...site.actors],
    evidence: [pdaEvidence(program, nameSeed(id)), ...site.evidence],
  });
}

export function setPageMessage({ program, signer, nonce, gasLimit, id, sealed, siteDir = SITE_DIR_KEEP, existingSitePage = null }) {
  const site = siteOpExtras(program, id, siteDir, existingSitePage);
  return message({
    program, signer, nonce, gasLimit,
    data: setPageInstruction({ id, sealed, siteDir }),
    actors: [...nameActors(program, id), ...site.actors],
    evidence: [...nameEvidence(program, id), ...site.evidence],
  });
}

export function transferMessage({
  program, signer, nonce, gasLimit, id, newOwner, now,
  nftProgram = null, nftTokenId: tokenId = null, sitePage = null, inMarket = false,
}) {
  const site = siteCleanup(program, id, sitePage);
  const market = marketCleanup(program, id, inMarket);
  return message({
    program, signer, nonce, gasLimit,
    data: transferInstruction({ id, newOwner, now }),
    actors: [
      ...nameActors(program, id),
      ...(nftProgram && tokenId ? [actor(settingsAddress(program), program), actor(nftTokenAddress(nftProgram, tokenId), nftProgram)] : []),
      ...site.actors,
      ...market.actors,
    ],
    evidence: [
      ...nameEvidence(program, id),
      ...(nftProgram && tokenId ? [pdaEvidence(program, settingsSeed())] : []),
      ...site.evidence,
      ...market.evidence,
    ],
  });
}

/** `refund` is the deposit account's whole balance, 0 for a name with no deposit. */
export function releaseMessage({
  program, signer, nonce, gasLimit, id, refund,
  nftProgram = null, nftTokenId: tokenId = null, sitePage = null, inMarket = false,
}) {
  const site = siteCleanup(program, id, sitePage);
  const market = marketCleanup(program, id, inMarket);
  return message({
    program, signer, nonce, gasLimit,
    data: releaseInstruction({ id, refund }),
    actors: [
      ...nameActors(program, id),
      ...(refund > 0n ? [nativeBalance(depositAddress(program, id)), nativeBalance(signer)] : []),
      ...(nftProgram && tokenId ? [actor(settingsAddress(program), program), actor(nftTokenAddress(nftProgram, tokenId), nftProgram)] : []),
      ...site.actors,
      ...market.actors,
    ],
    evidence: [
      ...nameEvidence(program, id),
      ...(nftProgram && tokenId ? [pdaEvidence(program, settingsSeed())] : []),
      ...site.evidence,
      ...market.evidence,
    ],
  });
}

export function setPrimaryMessage({ program, signer, nonce, gasLimit, name }) {
  return message({ program, signer, nonce, gasLimit, data: setPrimaryInstruction({ name }) });
}

export function claimMessage({
  program, signer, nonce, gasLimit, id, priceTier = PRICE_TIER_8_PLUS, sealed, now, proof,
  nftProgram = null, prevNftTokenId = null, prevSitePage = null, prevInMarket = false,
}) {
  const slotId = tieredId(id, priceTier);
  const newTokenId = nftProgram ? nftTokenId(slotId, claimGeneration(slotId, ownerTag(slotId, signer), now)) : null;
  const site = siteCleanup(program, slotId, prevSitePage);
  const market = marketCleanup(program, slotId, prevInMarket);
  return message({
    program, signer, nonce, gasLimit,
    data: claimInstruction({ id, sealed, now, proof, priceTier }),
    actors: [
      ...nameActors(program, slotId),
      actor(settingsAddress(program), program),
      ...(nftProgram && newTokenId ? [actor(nftTokenAddress(nftProgram, newTokenId), nftProgram)] : []),
      ...(nftProgram && prevNftTokenId ? [actor(nftTokenAddress(nftProgram, prevNftTokenId), nftProgram)] : []),
      ...site.actors,
      ...market.actors,
    ],
    evidence: [
      ...nameEvidence(program, slotId),
      pdaEvidence(program, settingsSeed()),
      ...(nftProgram && newTokenId ? [pdaEvidence(nftProgram, nftTokenSeed(newTokenId))] : []),
      ...site.evidence,
      ...market.evidence,
    ],
  });
}

export function setPriceMessage({ program, signer, nonce, gasLimit, priceTier, term, price }) {
  return message({
    program, signer, nonce, gasLimit,
    data: setPriceInstruction({ priceTier, term, price }),
    actors: [actor(settingsAddress(program), program)],
    evidence: [pdaEvidence(program, settingsSeed())],
  });
}

export function setPremiumMessage({ program, signer, nonce, gasLimit, premiumStart }) {
  return message({
    program, signer, nonce, gasLimit,
    data: setPremiumInstruction({ premiumStart }),
    actors: [actor(settingsAddress(program), program)],
    evidence: [pdaEvidence(program, settingsSeed())],
  });
}

export function setSubnameSaleMessage({ program, signer, nonce, gasLimit, id, sale = null }) {
  return message({
    program, signer, nonce, gasLimit,
    data: setSubnameSaleInstruction({ id, sale }),
    actors: [actor(recordAddress(program, id), program)],
    evidence: [pdaEvidence(program, nameSeed(id))],
  });
}

export function createSubnameMessage({ program, signer, nonce, gasLimit, parentId, subId, owner, revocable, sealed, now }) {
  return message({
    program, signer, nonce, gasLimit,
    data: createSubnameInstruction({ parentId, subId, owner, revocable, sealed, now }),
    actors: [
      actor(recordAddress(program, parentId), program),
      actor(subnameRecordAddress(program, parentId, subId), program),
      actor(subnamePageAddress(program, parentId, subId), program),
    ],
    evidence: [
      pdaEvidence(program, nameSeed(parentId)),
      pdaEvidence(program, subnameSeed(parentId, subId)),
      pdaEvidence(program, subpageSeed(parentId, subId)),
    ],
  });
}

export function revokeSubnameMessage({ program, signer, nonce, gasLimit, parentId, subId, now }) {
  return message({
    program, signer, nonce, gasLimit,
    data: revokeSubnameInstruction({ parentId, subId, now }),
    actors: [
      actor(recordAddress(program, parentId), program),
      actor(subnameRecordAddress(program, parentId, subId), program),
      actor(subnamePageAddress(program, parentId, subId), program),
    ],
    evidence: [
      pdaEvidence(program, nameSeed(parentId)),
      pdaEvidence(program, subnameSeed(parentId, subId)),
      pdaEvidence(program, subpageSeed(parentId, subId)),
    ],
  });
}

export function buySubnameMessage({ program, signer, nonce, gasLimit, parentId, subId, sealed, price, payout = null, now, secret }) {
  const promised = subnameCommitmentHash(parentId, subId, ownerTag(subId, signer), secret);
  return message({
    program, signer, nonce, gasLimit,
    data: buySubnameInstruction({ parentId, subId, sealed, price, payout, now, secret }),
    actors: [
      actor(recordAddress(program, parentId), program),
      actor(subnameRecordAddress(program, parentId, subId), program),
      actor(subnamePageAddress(program, parentId, subId), program),
      actor(commitmentAddress(program, promised), program),
      ...(price > 0n ? [nativeBalance(signer, AUTHORITY_SIGNER), nativeBalance(payout)] : []),
    ],
    evidence: [
      pdaEvidence(program, nameSeed(parentId)),
      pdaEvidence(program, subnameSeed(parentId, subId)),
      pdaEvidence(program, subpageSeed(parentId, subId)),
      pdaEvidence(program, commitmentSeed(promised)),
    ],
  });
}

export function setSubnameRecordMessage({ program, signer, nonce, gasLimit, parentId, subId, sealed, now = null }) {
  return message({
    program, signer, nonce, gasLimit,
    data: setSubnameRecordInstruction({ parentId, subId, sealed, now }),
    actors: [
      actor(subnameRecordAddress(program, parentId, subId), program),
      ...(now !== null ? [actor(recordAddress(program, parentId), program)] : []),
    ],
    evidence: [
      pdaEvidence(program, subnameSeed(parentId, subId)),
      ...(now !== null ? [pdaEvidence(program, nameSeed(parentId))] : []),
    ],
  });
}

export function setSubnamePageMessage({ program, signer, nonce, gasLimit, parentId, subId, sealed, now = null }) {
  return message({
    program, signer, nonce, gasLimit,
    data: setSubnamePageInstruction({ parentId, subId, sealed, now }),
    actors: [
      actor(subnameRecordAddress(program, parentId, subId), program),
      actor(subnamePageAddress(program, parentId, subId), program),
      ...(now !== null ? [actor(recordAddress(program, parentId), program)] : []),
    ],
    evidence: [
      pdaEvidence(program, subnameSeed(parentId, subId)),
      pdaEvidence(program, subpageSeed(parentId, subId)),
      ...(now !== null ? [pdaEvidence(program, nameSeed(parentId))] : []),
    ],
  });
}

export function transferSubnameMessage({ program, signer, nonce, gasLimit, parentId, subId, newOwner, now = null }) {
  return message({
    program, signer, nonce, gasLimit,
    data: transferSubnameInstruction({ parentId, subId, newOwner, now }),
    actors: [
      actor(subnameRecordAddress(program, parentId, subId), program),
      actor(subnamePageAddress(program, parentId, subId), program),
      ...(now !== null ? [actor(recordAddress(program, parentId), program)] : []),
    ],
    evidence: [
      pdaEvidence(program, subnameSeed(parentId, subId)),
      pdaEvidence(program, subpageSeed(parentId, subId)),
      ...(now !== null ? [pdaEvidence(program, nameSeed(parentId))] : []),
    ],
  });
}

export function releaseSubnameMessage({ program, signer, nonce, gasLimit, parentId, subId, now = null }) {
  return message({
    program, signer, nonce, gasLimit,
    data: releaseSubnameInstruction({ parentId, subId, now }),
    actors: [
      actor(subnameRecordAddress(program, parentId, subId), program),
      actor(subnamePageAddress(program, parentId, subId), program),
      ...(now !== null ? [actor(recordAddress(program, parentId), program)] : []),
    ],
    evidence: [
      pdaEvidence(program, subnameSeed(parentId, subId)),
      pdaEvidence(program, subpageSeed(parentId, subId)),
      ...(now !== null ? [pdaEvidence(program, nameSeed(parentId))] : []),
    ],
  });
}

export function listMessage({
  program, signer, nonce, gasLimit, id, price, payTo, expiresAt = null, now, displayName = null, prevInMarket = false,
}) {
  const needsMarket = displayName !== null || prevInMarket;
  const market = marketCleanup(program, id, needsMarket);
  return message({
    program, signer, nonce, gasLimit,
    data: listInstruction({ id, price, payTo, expiresAt, now, displayName }),
    actors: [actor(recordAddress(program, id), program), ...market.actors],
    evidence: [pdaEvidence(program, nameSeed(id)), ...market.evidence],
  });
}

export function cancelMessage({ program, signer, nonce, gasLimit, id, inMarket = false }) {
  const market = marketCleanup(program, id, inMarket);
  return message({
    program, signer, nonce, gasLimit,
    data: cancelInstruction({ id }),
    actors: [actor(recordAddress(program, id), program), ...market.actors],
    evidence: [pdaEvidence(program, nameSeed(id)), ...market.evidence],
  });
}

export function buyMessage({
  program, signer, nonce, gasLimit, id, price, payTo, treasury, expectedNonce, now,
  inMarket = false, nftProgram = null, nftTokenId: tokenId = null, sitePage = null,
}) {
  const market = marketCleanup(program, id, inMarket);
  const site = siteCleanup(program, id, sitePage);
  return message({
    program, signer, nonce, gasLimit,
    data: buyInstruction({ id, price, payTo, treasury, expectedNonce, now }),
    actors: [
      ...nameActors(program, id),
      actor(settingsAddress(program), program),
      nativeBalance(signer, AUTHORITY_SIGNER),
      nativeBalance(payTo),
      nativeBalance(treasury),
      ...(nftProgram && tokenId ? [actor(nftTokenAddress(nftProgram, tokenId), nftProgram)] : []),
      ...market.actors,
      ...site.actors,
    ],
    evidence: [
      ...nameEvidence(program, id),
      pdaEvidence(program, settingsSeed()),
      ...market.evidence,
      ...site.evidence,
    ],
  });
}

export function shieldDeedMessage({ program, signer, nonce, gasLimit, id, deedAccount, now, nftProgram = null, nftTokenId: tokenId = null }) {
  return message({
    program, signer, nonce, gasLimit,
    data: shieldDeedInstruction({ id, deedAccount, now }),
    actors: [
      actor(recordAddress(program, id), program),
      actor(settingsAddress(program), program),
      ...(nftProgram && tokenId ? [actor(nftTokenAddress(nftProgram, tokenId), nftProgram)] : []),
    ],
    evidence: [pdaEvidence(program, nameSeed(id)), pdaEvidence(program, settingsSeed())],
  });
}

export function unshieldDeedMessage({ program, signer, nonce, gasLimit, id, nftProgram = null, nftTokenId: tokenId = null }) {
  return message({
    program, signer, nonce, gasLimit,
    data: unshieldDeedInstruction({ id }),
    actors: [
      actor(recordAddress(program, id), program),
      actor(settingsAddress(program), program),
      ...(nftProgram && tokenId ? [actor(nftTokenAddress(nftProgram, tokenId), nftProgram)] : []),
    ],
    evidence: [pdaEvidence(program, nameSeed(id)), pdaEvidence(program, settingsSeed())],
  });
}

export function refreshShieldedDeedMessage({ program, signer, nonce, gasLimit, id, nftProgram = null, nftTokenId: tokenId = null }) {
  return message({
    program, signer, nonce, gasLimit,
    data: refreshShieldedDeedInstruction({ id }),
    actors: [
      actor(recordAddress(program, id), program),
      actor(settingsAddress(program), program),
      ...(nftProgram && tokenId ? [actor(nftTokenAddress(nftProgram, tokenId), nftProgram)] : []),
    ],
    evidence: [pdaEvidence(program, nameSeed(id)), pdaEvidence(program, settingsSeed())],
  });
}
