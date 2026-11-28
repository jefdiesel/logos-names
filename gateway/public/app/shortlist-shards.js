// Sharded locked-name shortlist verifier and non-membership proof builder for the web app.
//
// The full locked-name list (all 1-4 char names + ~24,000 reserved names) is ~61 MB,
// too big to fetch in a browser page. Instead, `build-shortlist-shards.mjs` splits it
// into 256 static shard files (`00.bin`..`ff.bin`) by the first byte of the name's
// 32-byte identifier, plus a `manifest.json` listing each shard's SHA-256 and root.
//
// When registering a name, this module fetches ONLY the one shard for `id[0]`, verifies
// its SHA-256 against the manifest, rebuilds the Merkle root directly from the shard's
// leaves + boundary siblings, asserts it equals the on-chain `SHORTLIST_ROOT`, and builds
// the exact Borsh-compatible `LengthProof::NonMembership` (`core/src/shortlist.rs`).

import { Writer, concat, equalBytes, fromHex, sha256, toHex } from "./lez-tx.js";

const utf8 = (text) => new TextEncoder().encode(text);

export const SHARD_MAGIC = utf8("LNSSHARD");
export const SHARD_VERSION = 1;
export const SHARD_HEADER_BYTES = 92;
export const LEAF_SIZE = 33;

export const TIER_NORMAL = 0;
export const TIER_LOCKED = 1;
export const TIER_THREE = 3;
export const TIER_SENTINEL = 0xff;

/** Depth of the shipped 1-4 char + reserved list (1,845,228 leaves padded to 2^21). */
export const SHORTLIST_DEPTH = 21;
/** Built-in Merkle root of the shipped locked-name list (`core/src/lib.rs::SHORTLIST_ROOT`). */
export const SHORTLIST_ROOT = fromHex("9778bed49005fe7401d25685a27d9cf3e681fa0582b562336cdb8ee9661d990c");

const LEAF_DOMAIN = utf8("logos-names/v5/short-leaf");
const NODE_DOMAIN = utf8("logos-names/v5/short-node");
const EMPTY_DOMAIN = utf8("logos-names/v5/short-empty");
const SHARD_DOMAIN = utf8("logos-names/v5/short-shard");

export const SENTINEL_LOW = new Uint8Array(32).fill(0x00);
export const SENTINEL_HIGH = new Uint8Array(32).fill(0xff);

export const DEFAULT_SHARDS_BASE_URL = new URL("./shortlist-shards/", import.meta.url).href;

export const leafHash = (id32, tier) => sha256(concat(LEAF_DOMAIN, id32, Uint8Array.of(tier)));
export const nodeHash = (left32, right32) => sha256(concat(NODE_DOMAIN, left32, right32));
export const emptyHash = () => sha256(EMPTY_DOMAIN);

function compareIds(a, b) {
  for (let i = 0; i < 32; i += 1) {
    if (a[i] !== b[i]) return a[i] - b[i];
  }
  return 0;
}

function shardContentRoot(prefix, startIndex, leafCount, leavesBytes) {
  const meta = new Uint8Array(9);
  meta[0] = prefix;
  const view = new DataView(meta.buffer);
  view.setUint32(1, startIndex, true);
  view.setUint32(5, leafCount, true);
  return sha256(concat(SHARD_DOMAIN, meta, leavesBytes));
}

function rootFromPath(leafDigest, index, path) {
  let acc = leafDigest;
  for (let level = 0; level < path.length; level += 1) {
    const sibling = path[level];
    acc = ((index >>> level) & 1) === 0 ? nodeHash(acc, sibling) : nodeHash(sibling, acc);
  }
  return acc;
}

function checkLeafPath(root, leaf, index, path, depth) {
  if (!Array.isArray(path) || path.length !== depth) {
    throw new Error("wrong proof path length");
  }
  if (!Number.isInteger(index) || index < 0 || (depth < 31 && index >= (1 << depth))) {
    throw new Error("leaf index out of range");
  }
  const computed = rootFromPath(leafHash(leaf.id, leaf.tier), index, path);
  if (!equalBytes(computed, root)) {
    throw new Error("proof does not match the list root");
  }
}

/**
 * Verifies a `LengthProof` against `root` at `depth`, matching `core/src/shortlist.rs::verify_depth`.
 * Returns `TIER_NORMAL`, `TIER_LOCKED`, or `TIER_THREE`, or throws on any mismatch.
 */
export function verifyLengthProof(root, id32, proof, depth = SHORTLIST_DEPTH) {
  if (!(id32 instanceof Uint8Array) || id32.length !== 32) {
    throw new Error("name id must be 32 bytes");
  }
  if (equalBytes(id32, SENTINEL_LOW) || equalBytes(id32, SENTINEL_HIGH)) {
    throw new Error("sentinel ids are not names");
  }
  if (!proof || typeof proof !== "object") {
    throw new Error("missing length proof");
  }
  if (proof.kind === "Membership") {
    if (proof.tier !== TIER_LOCKED && proof.tier !== TIER_THREE) {
      throw new Error("membership tier must be locked or three");
    }
    checkLeafPath(root, { id: id32, tier: proof.tier }, proof.index, proof.path, depth);
    return proof.tier;
  }
  if (proof.kind === "NonMembership") {
    if (!(compareIds(proof.left.id, id32) < 0 && compareIds(id32, proof.right.id) < 0)) {
      throw new Error("id is not strictly between the proof leaves");
    }
    const rightIndex = proof.leftIndex + 1;
    if (!Number.isSafeInteger(rightIndex) || rightIndex > 0xffffffff) {
      throw new Error("leaf index out of range");
    }
    checkLeafPath(root, proof.left, proof.leftIndex, proof.leftPath, depth);
    checkLeafPath(root, proof.right, rightIndex, proof.rightPath, depth);
    return TIER_NORMAL;
  }
  throw new Error(`unknown length proof kind: ${proof.kind}`);
}

/**
 * Borsh-encodes a `LengthProof` byte for byte as `core/src/shortlist.rs` does.
 */
export function encodeLengthProof(proof) {
  const w = new Writer();
  const writeLeaf = (leaf) => w.fixed(leaf.id, 32).u8(leaf.tier);
  const writePath = (path) => {
    w.u32(path.length);
    for (const h of path) w.fixed(h, 32);
  };
  if (proof.kind === "Membership") {
    w.u8(0).u32(proof.index).u8(proof.tier);
    writePath(proof.path);
  } else if (proof.kind === "NonMembership") {
    w.u8(1);
    writeLeaf(proof.left);
    writePath(proof.leftPath);
    writeLeaf(proof.right);
    writePath(proof.rightPath);
    w.u32(proof.leftIndex);
  } else {
    throw new Error(`unknown length proof kind: ${proof?.kind}`);
  }
  return w.done();
}

/**
 * Parses and cryptographically verifies one binary shard against `expectedPrefix` and
 * `expectedRoot` (and optional `expectedDepth`). Rebuilds every level over the shard's
 * leaf slice and asserts the resulting root equals `expectedRoot`.
 */
export function parseShard(bytes, { expectedPrefix = null, expectedRoot = SHORTLIST_ROOT, expectedDepth = null } = {}) {
  const buf = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
  if (buf.length < SHARD_HEADER_BYTES) {
    throw new Error("shortlist shard is truncated");
  }
  if (!equalBytes(buf.subarray(0, 8), SHARD_MAGIC)) {
    throw new Error("shortlist shard has invalid header");
  }
  const version = buf[8];
  if (version !== SHARD_VERSION) {
    throw new Error(`unsupported shortlist shard version: ${version}`);
  }
  const prefix = buf[9];
  if (expectedPrefix !== null && prefix !== expectedPrefix) {
    throw new Error(`shortlist shard prefix mismatch: expected ${expectedPrefix}, got ${prefix}`);
  }
  const depth = buf[10];
  if (depth < 1 || depth > 30) {
    throw new Error(`invalid shortlist shard depth: ${depth}`);
  }
  if (expectedDepth !== null && depth !== expectedDepth) {
    throw new Error(`shortlist shard depth mismatch: expected ${expectedDepth}, got ${depth}`);
  }

  const view = new DataView(buf.buffer, buf.byteOffset, buf.byteLength);
  const totalLeaves = view.getUint32(12, true);
  const startIndex = view.getUint32(16, true);
  const leafCount = view.getUint32(20, true);
  const leftSibCount = view.getUint16(24, true);
  const rightSibCount = view.getUint16(26, true);
  const headerRoot = buf.slice(28, 60);
  const headerShardRoot = buf.slice(60, 92);

  if (leafCount < 2 || totalLeaves < 2 || startIndex + leafCount > totalLeaves) {
    throw new Error("shortlist shard has invalid leaf range");
  }
  const endIndex = startIndex + leafCount - 1;
  if (totalLeaves > (1 << depth)) {
    throw new Error("shortlist shard leaf count exceeds tree depth");
  }

  let needLeft = 0;
  let needRight = 0;
  for (let k = 0; k < depth; k += 1) {
    if (((startIndex >>> k) & 1) === 1) needLeft += 1;
    if (((endIndex >>> k) & 1) === 0) needRight += 1;
  }
  if (leftSibCount !== needLeft || rightSibCount !== needRight) {
    throw new Error("shortlist shard sibling counts do not match its leaf range");
  }

  const leavesLen = leafCount * LEAF_SIZE;
  const expectedLen = SHARD_HEADER_BYTES + leavesLen + (leftSibCount + rightSibCount) * 32;
  if (buf.length !== expectedLen) {
    throw new Error("shortlist shard length does not match its header");
  }

  if (expectedRoot && !equalBytes(headerRoot, expectedRoot)) {
    throw new Error("shortlist shard root does not match the on-chain root");
  }

  const leavesBytes = buf.subarray(SHARD_HEADER_BYTES, SHARD_HEADER_BYTES + leavesLen);
  if (!equalBytes(shardContentRoot(prefix, startIndex, leafCount, leavesBytes), headerShardRoot)) {
    throw new Error("shortlist shard integrity check failed");
  }

  const leaves = new Array(leafCount);
  for (let i = 0; i < leafCount; i += 1) {
    const off = i * LEAF_SIZE;
    const id = leavesBytes.slice(off, off + 32);
    const tier = leavesBytes[off + 32];
    if (i > 0 && compareIds(leaves[i - 1].id, id) >= 0) {
      throw new Error("shortlist shard leaves are not strictly ascending");
    }
    const globalIdx = startIndex + i;
    if (globalIdx === 0) {
      if (!equalBytes(id, SENTINEL_LOW) || tier !== TIER_SENTINEL) {
        throw new Error("shortlist shard has invalid low sentinel");
      }
    } else if (globalIdx === totalLeaves - 1) {
      if (!equalBytes(id, SENTINEL_HIGH) || tier !== TIER_SENTINEL) {
        throw new Error("shortlist shard has invalid high sentinel");
      }
    } else if (tier !== TIER_LOCKED && tier !== TIER_THREE) {
      throw new Error("shortlist shard has invalid leaf tier");
    }
    leaves[i] = { id, tier };
  }

  // Verify the shard's leaf range actually bounds every possible id starting with `prefix`.
  if (prefix === 0) {
    if (startIndex !== 0 || !equalBytes(leaves[0].id, SENTINEL_LOW)) {
      throw new Error("shortlist shard 00 must begin with the low sentinel");
    }
  } else if (leaves[0].id[0] >= prefix) {
    throw new Error("shortlist shard is missing its lower boundary leaf");
  }
  if (prefix === 255) {
    if (endIndex !== totalLeaves - 1 || !equalBytes(leaves[leafCount - 1].id, SENTINEL_HIGH)) {
      throw new Error("shortlist shard ff must end with the high sentinel");
    }
  } else if (leaves[leafCount - 1].id[0] <= prefix) {
    throw new Error("shortlist shard is missing its upper boundary leaf");
  }

  // Unpack level-indexed left/right boundary siblings.
  let sibOff = SHARD_HEADER_BYTES + leavesLen;
  const leftBoundary = new Array(depth).fill(null);
  for (let k = 0; k < depth; k += 1) {
    if (((startIndex >>> k) & 1) === 1) {
      leftBoundary[k] = buf.slice(sibOff, sibOff + 32);
      sibOff += 32;
    }
  }
  const rightBoundary = new Array(depth).fill(null);
  for (let k = 0; k < depth; k += 1) {
    if (((endIndex >>> k) & 1) === 0) {
      rightBoundary[k] = buf.slice(sibOff, sibOff + 32);
      sibOff += 32;
    }
  }

  // Rebuild every level over the shard's slice all the way up to the full-list root.
  const levelSlices = new Array(depth);
  const levelLos = new Uint32Array(depth);
  let lo = startIndex;
  let hi = endIndex;
  let cur = new Array(leafCount);
  for (let i = 0; i < leafCount; i += 1) {
    cur[i] = leafHash(leaves[i].id, leaves[i].tier);
  }

  for (let k = 0; k < depth; k += 1) {
    levelSlices[k] = cur;
    levelLos[k] = lo;
    const nextLo = lo >>> 1;
    const nextHi = hi >>> 1;
    const nextLen = nextHi - nextLo + 1;
    const next = new Array(nextLen);
    for (let j = 0; j < nextLen; j += 1) {
      const parentIdx = nextLo + j;
      const leftChild = parentIdx * 2;
      const rightChild = leftChild + 1;
      const lHash = leftChild < lo ? leftBoundary[k] : cur[leftChild - lo];
      const rHash = rightChild > hi ? rightBoundary[k] : cur[rightChild - lo];
      next[j] = nodeHash(lHash, rHash);
    }
    cur = next;
    lo = nextLo;
    hi = nextHi;
  }

  const computedRoot = cur[0];
  if (!equalBytes(computedRoot, headerRoot)) {
    throw new Error("shortlist shard Merkle proof does not match its header root");
  }
  if (expectedRoot && !equalBytes(computedRoot, expectedRoot)) {
    throw new Error("shortlist shard Merkle proof does not match the on-chain root");
  }

  return {
    version,
    prefix,
    depth,
    totalLeaves,
    startIndex,
    endIndex,
    leafCount,
    root: computedRoot,
    leaves,
    levelSlices,
    levelLos,
    leftBoundary,
    rightBoundary,
  };
}

function pathFromParsedShard(shard, globalIndex) {
  const path = new Array(shard.depth);
  for (let k = 0; k < shard.depth; k += 1) {
    const nodeIdx = globalIndex >>> k;
    const sibIdx = nodeIdx ^ 1;
    const lo = shard.levelLos[k];
    const slice = shard.levelSlices[k];
    if (sibIdx < lo) {
      path[k] = shard.leftBoundary[k];
    } else if (sibIdx >= lo + slice.length) {
      path[k] = shard.rightBoundary[k];
    } else {
      path[k] = slice[sibIdx - lo];
    }
  }
  return path;
}

/**
 * Builds a verified `LengthProof::NonMembership` for `id32` from an already-parsed shard.
 * Refuses (throws with `code = "reserved"`) if `id32` is present in the locked/reserved list.
 */
export function proveFromShard(shard, id32) {
  if (!(id32 instanceof Uint8Array) || id32.length !== 32) {
    throw new Error("name id must be 32 bytes");
  }
  if (equalBytes(id32, SENTINEL_LOW) || equalBytes(id32, SENTINEL_HIGH)) {
    throw Object.assign(new Error("that id isn't a name"), { code: "invalid_id" });
  }
  if (id32[0] !== shard.prefix) {
    throw new Error(`wrong shard for id: expected prefix ${id32[0]}, got ${shard.prefix}`);
  }

  const { leaves, startIndex } = shard;
  let low = 0;
  let high = leaves.length;
  while (low < high) {
    const mid = (low + high) >>> 1;
    if (compareIds(leaves[mid].id, id32) < 0) low = mid + 1;
    else high = mid;
  }
  if (low === 0 || low >= leaves.length) {
    throw new Error("shortlist shard did not bound the name id");
  }
  if (compareIds(leaves[low].id, id32) === 0) {
    throw Object.assign(new Error("This name is reserved and can't be registered."), {
      code: "reserved",
      tier: leaves[low].tier,
    });
  }

  const leftLocal = low - 1;
  const rightLocal = low;
  const leftIndex = startIndex + leftLocal;
  const rightIndex = startIndex + rightLocal;
  const proof = {
    kind: "NonMembership",
    left: leaves[leftLocal],
    leftIndex,
    leftPath: pathFromParsedShard(shard, leftIndex),
    right: leaves[rightLocal],
    rightPath: pathFromParsedShard(shard, rightIndex),
  };

  // Self-verify against the shard's verified root before returning.
  verifyLengthProof(shard.root, id32, proof, shard.depth);
  return proof;
}

/**
 * Creates a cached shard prover that fetches `manifest.json` once and fetches each
 * `XX.bin` shard on demand, verifying every shard against both the manifest SHA-256
 * and the expected on-chain Merkle root.
 */
export function createShardProver({
  baseUrl = DEFAULT_SHARDS_BASE_URL,
  expectedRoot = SHORTLIST_ROOT,
  expectedDepth = SHORTLIST_DEPTH,
  fetchImpl = (...args) => globalThis.fetch(...args),
} = {}) {
  const rootBytes = typeof expectedRoot === "string" ? fromHex(expectedRoot) : expectedRoot;
  const base = baseUrl.endsWith("/") ? baseUrl : `${baseUrl}/`;
  let manifestPromise = null;
  const shardCache = new Map();

  async function loadManifest() {
    manifestPromise ??= (async () => {
      const res = await fetchImpl(`${base}manifest.json`);
      if (!res.ok) throw new Error(`Couldn't load the locked-name list manifest (HTTP ${res.status}).`);
      const manifest = await res.json();
      if (!manifest || typeof manifest !== "object" || manifest.version !== SHARD_VERSION) {
        throw new Error("The locked-name list manifest is invalid.");
      }
      if (typeof manifest.root !== "string" || manifest.root.toLowerCase() !== toHex(rootBytes)) {
        throw new Error("The locked-name list manifest doesn't match the on-chain root.");
      }
      if (expectedDepth !== null && manifest.depth !== expectedDepth) {
        throw new Error(`Unexpected locked-name list depth: ${manifest.depth}`);
      }
      if (!manifest.shards || typeof manifest.shards !== "object") {
        throw new Error("The locked-name list manifest is missing its shard index.");
      }
      return manifest;
    })().catch((err) => {
      manifestPromise = null;
      throw err;
    });
    return manifestPromise;
  }

  async function loadShard(prefix) {
    const prefixHex = prefix.toString(16).padStart(2, "0");
    if (shardCache.has(prefixHex)) return shardCache.get(prefixHex);
    const pending = (async () => {
      const manifest = await loadManifest();
      const entry = manifest.shards[prefixHex];
      if (!entry || typeof entry.file !== "string" || typeof entry.sha256 !== "string") {
        throw new Error(`Locked-name list manifest is missing shard ${prefixHex}.`);
      }
      const res = await fetchImpl(`${base}${entry.file}`);
      if (!res.ok) throw new Error(`Couldn't load locked-name shard ${prefixHex} (HTTP ${res.status}).`);
      const bytes = new Uint8Array(await res.arrayBuffer());
      if (typeof entry.bytes === "number" && bytes.length !== entry.bytes) {
        throw new Error(`Locked-name shard ${prefixHex} size mismatch.`);
      }
      const digest = toHex(sha256(bytes));
      if (digest !== entry.sha256.toLowerCase()) {
        throw new Error(`Locked-name shard ${prefixHex} failed integrity verification (SHA-256 mismatch).`);
      }
      const parsed = parseShard(bytes, {
        expectedPrefix: prefix,
        expectedRoot: rootBytes,
        expectedDepth,
      });
      if (
        (typeof entry.startIndex === "number" && parsed.startIndex !== entry.startIndex) ||
        (typeof entry.leafCount === "number" && parsed.leafCount !== entry.leafCount)
      ) {
        throw new Error(`Locked-name shard ${prefixHex} range does not match manifest.`);
      }
      return parsed;
    })().catch((err) => {
      shardCache.delete(prefixHex);
      throw err;
    });
    shardCache.set(prefixHex, pending);
    return pending;
  }

  async function prove(id32) {
    if (!(id32 instanceof Uint8Array) || id32.length !== 32) {
      throw new Error("name id must be 32 bytes");
    }
    if (equalBytes(id32, SENTINEL_LOW) || equalBytes(id32, SENTINEL_HIGH)) {
      throw Object.assign(new Error("that id isn't a name"), { code: "invalid_id" });
    }
    const shard = await loadShard(id32[0]);
    return proveFromShard(shard, id32);
  }

  function clearCache() {
    manifestPromise = null;
    shardCache.clear();
  }

  return { loadManifest, loadShard, prove, clearCache };
}

const defaultProver = createShardProver();

/** Builds a verified NonMembership `LengthProof` for `id32` by fetching only its one shard. */
export function lengthProofFromShards(id32) {
  return defaultProver.prove(id32);
}
