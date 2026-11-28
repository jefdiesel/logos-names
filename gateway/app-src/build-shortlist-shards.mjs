#!/usr/bin/env node
// Builds 256 static shortlist Merkle proof shards (`00.bin`..`ff.bin`) and a
// `manifest.json` from a sorted `shortlist.bin` (33 bytes/leaf: id[32] || tier[1],
// sentinels included), matching `core/src/shortlist.rs` byte for byte.
//
// Each shard `b` (0x00..0xff) contains:
//   - a 92-byte header (magic, version, prefix, depth, totalLeaves, startIndex,
//     leafCount, leftSibCount, rightSibCount, full-list root, shardRoot)
//   - the contiguous slice of leaves covering prefix `b` plus its immediate
//     predecessor/successor boundary leaves (so any hash starting with `b` lies
//     strictly between the first and last leaf of the shard)
//   - the minimal boundary sibling hashes at each tree level needed to rebuild
//     every leaf's full-depth Merkle path AND verify directly against the
//     full-list on-chain root (at most 2 * depth * 32 bytes ≈ 1.3 KB for depth 21)
//
// Usage:
//   node build-shortlist-shards.mjs <shortlist.bin> <out-dir>

import { createHash } from "node:crypto";
import { realpathSync } from "node:fs";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const utf8 = new TextEncoder();

export const SHARD_MAGIC = utf8.encode("LNSSHARD");
export const SHARD_VERSION = 1;
export const SHARD_HEADER_BYTES = 92;
export const LEAF_SIZE = 33;

export const TIER_NORMAL = 0;
export const TIER_LOCKED = 1;
export const TIER_THREE = 3;
export const TIER_SENTINEL = 0xff;

const LEAF_DOMAIN = utf8.encode("logos-names/v5/short-leaf");
const NODE_DOMAIN = utf8.encode("logos-names/v5/short-node");
const EMPTY_DOMAIN = utf8.encode("logos-names/v5/short-empty");
const SHARD_DOMAIN = utf8.encode("logos-names/v5/short-shard");

export const SENTINEL_LOW = new Uint8Array(32).fill(0x00);
export const SENTINEL_HIGH = new Uint8Array(32).fill(0xff);

function sha256Parts(...parts) {
  const h = createHash("sha256");
  for (const p of parts) h.update(p);
  const d = h.digest();
  return new Uint8Array(d.buffer, d.byteOffset, d.byteLength);
}

export function sha256Hex(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

const toHex = (bytes) => Buffer.from(bytes.buffer, bytes.byteOffset, bytes.byteLength).toString("hex");

function compareIds(a, b) {
  for (let i = 0; i < 32; i += 1) {
    if (a[i] !== b[i]) return a[i] - b[i];
  }
  return 0;
}

function equalBytes(a, b) {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i += 1) {
    if (a[i] !== b[i]) return false;
  }
  return true;
}

export function leafHash(id32, tier) {
  return sha256Parts(LEAF_DOMAIN, id32, Uint8Array.of(tier));
}

export function nodeHash(left32, right32) {
  return sha256Parts(NODE_DOMAIN, left32, right32);
}

export function emptyHash() {
  return sha256Parts(EMPTY_DOMAIN);
}

export function shardContentRoot(prefix, startIndex, leafCount, leavesBytes) {
  const meta = new Uint8Array(9);
  meta[0] = prefix;
  const view = new DataView(meta.buffer);
  view.setUint32(1, startIndex, true);
  view.setUint32(5, leafCount, true);
  return sha256Parts(SHARD_DOMAIN, meta, leavesBytes);
}

function nextPowerOfTwo(n) {
  let width = 1;
  while (width < n) width *= 2;
  return width;
}

/**
 * Parses `shortlist.bin` (33 bytes per leaf, sentinels included) into `{ id, tier }[]`.
 */
export function parseShortlistBin(bytes) {
  const buf = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
  if (buf.length === 0 || buf.length % LEAF_SIZE !== 0) {
    throw new Error(`shortlist.bin must be a non-empty multiple of ${LEAF_SIZE} bytes`);
  }
  const count = buf.length / LEAF_SIZE;
  const leaves = new Array(count);
  for (let i = 0; i < count; i += 1) {
    const off = i * LEAF_SIZE;
    leaves[i] = {
      id: buf.slice(off, off + 32),
      tier: buf[off + 32],
    };
  }
  return leaves;
}

/**
 * Builds sorted leaves (with sentinels) from real `{ id, tier }` entries (for fixtures/tests).
 */
export function makeSortedLeaves(entries) {
  const sorted = entries.map(({ id, tier }) => {
    if (!(id instanceof Uint8Array) || id.length !== 32) {
      throw new Error("leaf id must be 32 bytes");
    }
    if (tier !== TIER_LOCKED && tier !== TIER_THREE) {
      throw new Error("entry tier must be TIER_LOCKED (1) or TIER_THREE (3)");
    }
    if (equalBytes(id, SENTINEL_LOW) || equalBytes(id, SENTINEL_HIGH)) {
      throw new Error("real entry id must not equal a sentinel");
    }
    return { id: Uint8Array.from(id), tier };
  });
  sorted.sort((a, b) => compareIds(a.id, b.id));
  for (let i = 1; i < sorted.length; i += 1) {
    if (compareIds(sorted[i - 1].id, sorted[i].id) === 0) {
      throw new Error("duplicate id in shortlist entries");
    }
  }
  return [
    { id: Uint8Array.from(SENTINEL_LOW), tier: TIER_SENTINEL },
    ...sorted,
    { id: Uint8Array.from(SENTINEL_HIGH), tier: TIER_SENTINEL },
  ];
}

/**
 * Builds the full shortlist Merkle tree, matching `core/src/shortlist.rs::ShortList::from_sorted_leaves`.
 */
export function buildShortlistTree(leaves) {
  if (!Array.isArray(leaves) || leaves.length < 2) {
    throw new Error("list must start and end with the sentinels");
  }
  if (
    !equalBytes(leaves[0].id, SENTINEL_LOW) ||
    leaves[0].tier !== TIER_SENTINEL ||
    !equalBytes(leaves[leaves.length - 1].id, SENTINEL_HIGH) ||
    leaves[leaves.length - 1].tier !== TIER_SENTINEL
  ) {
    throw new Error("list must start and end with the sentinels");
  }
  for (let i = 1; i < leaves.length; i += 1) {
    if (compareIds(leaves[i - 1].id, leaves[i].id) >= 0) {
      throw new Error(`leaves are not strictly ascending at index ${i}`);
    }
  }
  for (let i = 1; i < leaves.length - 1; i += 1) {
    if (leaves[i].tier !== TIER_LOCKED && leaves[i].tier !== TIER_THREE) {
      throw new Error(`entry tier must be locked or three at index ${i}`);
    }
  }

  const width = nextPowerOfTwo(leaves.length);
  const empty = emptyHash();
  const level0 = new Uint8Array(width * 32);
  for (let i = 0; i < leaves.length; i += 1) {
    level0.set(leafHash(leaves[i].id, leaves[i].tier), i * 32);
  }
  for (let i = leaves.length; i < width; i += 1) {
    level0.set(empty, i * 32);
  }

  const levels = [level0];
  let curLen = width;
  while (curLen > 1) {
    const prev = levels[levels.length - 1];
    const nextLen = curLen >>> 1;
    const next = new Uint8Array(nextLen * 32);
    for (let j = 0; j < nextLen; j += 1) {
      const l = prev.subarray(j * 64, j * 64 + 32);
      const r = prev.subarray(j * 64 + 32, j * 64 + 64);
      next.set(nodeHash(l, r), j * 32);
    }
    levels.push(next);
    curLen = nextLen;
  }

  const depth = levels.length - 1;
  const root = levels[depth].slice(0, 32);

  function nodeAt(level, idx) {
    const arr = levels[level];
    return arr.slice(idx * 32, idx * 32 + 32);
  }

  function pathOf(index) {
    const out = new Array(depth);
    for (let k = 0; k < depth; k += 1) {
      out[k] = nodeAt(k, (index >>> k) ^ 1);
    }
    return out;
  }

  function prove(id32) {
    if (equalBytes(id32, SENTINEL_LOW) || equalBytes(id32, SENTINEL_HIGH)) {
      throw new Error("sentinel ids are not names");
    }
    let low = 0;
    let high = leaves.length;
    while (low < high) {
      const mid = (low + high) >>> 1;
      if (compareIds(leaves[mid].id, id32) < 0) low = mid + 1;
      else high = mid;
    }
    if (low < leaves.length && compareIds(leaves[low].id, id32) === 0) {
      return {
        kind: "Membership",
        index: low,
        tier: leaves[low].tier,
        path: pathOf(low),
      };
    }
    const leftIndex = low - 1;
    return {
      kind: "NonMembership",
      left: leaves[leftIndex],
      leftIndex,
      leftPath: pathOf(leftIndex),
      right: leaves[leftIndex + 1],
      rightPath: pathOf(leftIndex + 1),
    };
  }

  return { leaves, levels, depth, root, nodeAt, pathOf, prove };
}

/**
 * Splits a built shortlist tree (or sorted leaves) into 256 binary shards (one per
 * first byte `00`..`ff`) and a manifest listing every shard's SHA-256 and the full root.
 */
export function buildShardsFromLeaves(leaves) {
  const tree = buildShortlistTree(leaves);
  const { depth, root, nodeAt } = tree;
  const totalLeaves = leaves.length;

  // Locate the first index for each prefix byte 0..255 in a single linear pass.
  const firstAtOrAbove = new Uint32Array(257);
  let cursor = 0;
  for (let b = 0; b <= 255; b += 1) {
    while (cursor < totalLeaves && leaves[cursor].id[0] < b) cursor += 1;
    firstAtOrAbove[b] = cursor;
  }
  firstAtOrAbove[256] = totalLeaves;

  const shards = new Map();
  const manifestShards = {};

  for (let prefix = 0; prefix <= 255; prefix += 1) {
    const startIndex = prefix === 0 ? 0 : firstAtOrAbove[prefix] - 1;
    const endIndex = prefix === 255 ? totalLeaves - 1 : firstAtOrAbove[prefix + 1];
    const leafCount = endIndex - startIndex + 1;

    const leavesBytes = new Uint8Array(leafCount * LEAF_SIZE);
    for (let i = 0; i < leafCount; i += 1) {
      const leaf = leaves[startIndex + i];
      const off = i * LEAF_SIZE;
      leavesBytes.set(leaf.id, off);
      leavesBytes[off + 32] = leaf.tier;
    }

    const leftSiblings = [];
    const rightSiblings = [];
    for (let k = 0; k < depth; k += 1) {
      const lo = startIndex >>> k;
      const hi = endIndex >>> k;
      if ((lo & 1) === 1) leftSiblings.push(nodeAt(k, lo - 1));
      if ((hi & 1) === 0) rightSiblings.push(nodeAt(k, hi + 1));
    }

    const shardRoot = shardContentRoot(prefix, startIndex, leafCount, leavesBytes);
    const totalBytes =
      SHARD_HEADER_BYTES +
      leavesBytes.length +
      leftSiblings.length * 32 +
      rightSiblings.length * 32;
    const out = new Uint8Array(totalBytes);
    const view = new DataView(out.buffer);

    out.set(SHARD_MAGIC, 0);
    out[8] = SHARD_VERSION;
    out[9] = prefix;
    out[10] = depth;
    out[11] = 0; // reserved flags
    view.setUint32(12, totalLeaves, true);
    view.setUint32(16, startIndex, true);
    view.setUint32(20, leafCount, true);
    view.setUint16(24, leftSiblings.length, true);
    view.setUint16(26, rightSiblings.length, true);
    out.set(root, 28);
    out.set(shardRoot, 60);

    let off = SHARD_HEADER_BYTES;
    out.set(leavesBytes, off);
    off += leavesBytes.length;
    for (const sib of leftSiblings) {
      out.set(sib, off);
      off += 32;
    }
    for (const sib of rightSiblings) {
      out.set(sib, off);
      off += 32;
    }

    const prefixHex = prefix.toString(16).padStart(2, "0");
    const file = `${prefixHex}.bin`;
    shards.set(prefixHex, out);
    manifestShards[prefixHex] = {
      file,
      sha256: sha256Hex(out),
      bytes: out.length,
      startIndex,
      leafCount,
      minId: toHex(leaves[startIndex].id),
      maxId: toHex(leaves[endIndex].id),
    };
  }

  const manifest = {
    version: SHARD_VERSION,
    root: toHex(root),
    depth,
    totalLeaves,
    shardCount: 256,
    shards: manifestShards,
  };

  return { tree, manifest, shards };
}

/**
 * Writes `manifest.json` and `00.bin`..`ff.bin` into `outDir`.
 */
export async function writeShardsToDir(leaves, outDir) {
  const built = buildShardsFromLeaves(leaves);
  await mkdir(outDir, { recursive: true });
  for (const [prefixHex, bytes] of built.shards.entries()) {
    await writeFile(join(outDir, `${prefixHex}.bin`), bytes);
  }
  const manifestJson = JSON.stringify(built.manifest, null, 2) + "\n";
  await writeFile(join(outDir, "manifest.json"), manifestJson, "utf-8");
  return built;
}

/**
 * Reads `shortlist.bin` from `shortlistPath` and writes all 256 shards + `manifest.json` to `outDir`.
 */
export async function buildShardsFromFile(shortlistPath, outDir) {
  const raw = await readFile(shortlistPath);
  const leaves = parseShortlistBin(raw);
  return writeShardsToDir(leaves, outDir);
}

function isExecutedDirectly() {
  if (!process.argv[1]) return false;
  const selfPath = fileURLToPath(import.meta.url);
  try {
    return realpathSync(selfPath) === realpathSync(resolve(process.argv[1]));
  } catch {
    return selfPath === resolve(process.argv[1]);
  }
}

if (isExecutedDirectly()) {
  const [shortlistPath, outDir] = process.argv.slice(2);
  if (!shortlistPath || !outDir) {
    console.error("Usage: node build-shortlist-shards.mjs <shortlist.bin> <out-dir>");
    process.exit(1);
  }
  const { manifest } = await buildShardsFromFile(shortlistPath, outDir);
  const totalBytes = Object.values(manifest.shards).reduce((sum, s) => sum + s.bytes, 0);
  console.log(
    `Wrote ${manifest.shardCount} shards (${totalBytes} bytes total) for ${manifest.totalLeaves} leaves, ` +
      `depth ${manifest.depth}, root ${manifest.root} -> ${outDir}`,
  );
}
