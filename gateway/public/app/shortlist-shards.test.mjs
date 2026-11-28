// Tests for the 256-shard locked-name shortlist builder and browser proof builder.
// Run: node --test gateway/public/app/shortlist-shards.test.mjs

import assert from "node:assert/strict";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import {
  buildShardsFromLeaves,
  buildShortlistTree,
  makeSortedLeaves,
  writeShardsToDir,
} from "../../app-src/build-shortlist-shards.mjs";
import { fromHex, toHex } from "./lez-tx.js";
import {
  SENTINEL_HIGH,
  SENTINEL_LOW,
  SHARD_HEADER_BYTES,
  TIER_LOCKED,
  TIER_NORMAL,
  TIER_SENTINEL,
  TIER_THREE,
  createShardProver,
  encodeLengthProof,
  parseShard,
  proveFromShard,
  verifyLengthProof,
} from "./shortlist-shards.js";

/** Deterministic 32-byte ID with chosen first byte and distinct tail. */
function makeId(firstByte, seed) {
  const out = new Uint8Array(32);
  out[0] = firstByte & 0xff;
  let x = (BigInt(seed) * 0x9e3779b97f4a7c15n) | 1n;
  for (let i = 1; i < 32; i += 1) {
    x ^= (x << 13n) & 0xffffffffffffffffn;
    x ^= x >> 7n;
    x ^= (x << 17n) & 0xffffffffffffffffn;
    out[i] = Number(x & 0xffn);
  }
  return out;
}

/** Small fixture list: ~50 locked names spanning scattered prefixes, leaving many prefixes empty. */
function buildFixtureLeaves() {
  const entries = [];
  // Multiple leaves in prefix 0x00, 0x01, 0x42, 0x80, 0xfe, 0xff, plus singles in a few others.
  const prefixes = [
    [0x00, 3],
    [0x01, 4],
    [0x05, 1],
    [0x20, 5],
    [0x42, 7],
    [0x7f, 2],
    [0x80, 6],
    [0xab, 4],
    [0xfe, 3],
    [0xff, 3],
  ];
  let seq = 1;
  for (const [p, count] of prefixes) {
    for (let j = 0; j < count; j += 1) {
      entries.push({ id: makeId(p, seq++), tier: seq % 5 === 0 ? TIER_THREE : TIER_LOCKED });
    }
  }
  return makeSortedLeaves(entries);
}

test("Borsh LengthProof encoding matches core/src/shortlist.rs & basecamp-module vectors byte for byte", () => {
  // Vector pinned in basecamp-module/rust-lib/src/shortlist.rs:463-482.
  const nonMember = {
    kind: "NonMembership",
    left: { id: SENTINEL_LOW, tier: TIER_SENTINEL },
    leftPath: [new Uint8Array(32).fill(0xaa)],
    right: { id: SENTINEL_HIGH, tier: TIER_SENTINEL },
    rightPath: [new Uint8Array(32).fill(0xbb)],
    leftIndex: 9,
  };
  assert.equal(
    toHex(encodeLengthProof(nonMember)),
    `01${"00".repeat(32)}ff01000000${"aa".repeat(32)}${"ff".repeat(32)}ff01000000${"bb".repeat(32)}09000000`,
  );

  const member = { kind: "Membership", index: 7, tier: TIER_THREE, path: [] };
  assert.equal(toHex(encodeLengthProof(member)), "00070000000300000000");
});

test("shard builder splits a fixture list into 256 shards and matches full-tree non-membership proofs byte for byte", () => {
  const leaves = buildFixtureLeaves();
  const { tree, manifest, shards } = buildShardsFromLeaves(leaves);

  assert.equal(manifest.version, 1);
  assert.equal(manifest.shardCount, 256);
  assert.equal(manifest.totalLeaves, leaves.length);
  assert.equal(manifest.depth, tree.depth);
  assert.equal(manifest.root, toHex(tree.root));
  assert.equal(shards.size, 256);

  // Test allowed names across populated prefixes, empty prefixes (e.g. 0x02, 0x7e, 0x99),
  // prefix 0x00 (near low sentinel), and prefix 0xff (near high sentinel).
  const allowedIds = [
    Uint8Array.from([0x00, ...new Array(30).fill(0x00), 0x01]),
    makeId(0x00, 999),
    makeId(0x01, 998),
    makeId(0x02, 997), // empty bucket!
    makeId(0x42, 996),
    makeId(0x7e, 995), // empty bucket right before 0x7f
    makeId(0x99, 994), // empty bucket in the middle
    makeId(0xfe, 993),
    Uint8Array.from([0xff, ...new Array(30).fill(0xff), 0xfe]),
  ];

  for (const id of allowedIds) {
    const prefixHex = id[0].toString(16).padStart(2, "0");
    const parsed = parseShard(shards.get(prefixHex), {
      expectedPrefix: id[0],
      expectedRoot: tree.root,
      expectedDepth: tree.depth,
    });
    const shardProof = proveFromShard(parsed, id);
    const fullProof = tree.prove(id);

    assert.equal(fullProof.kind, "NonMembership");
    assert.equal(toHex(encodeLengthProof(shardProof)), toHex(encodeLengthProof(fullProof)));
    assert.equal(verifyLengthProof(tree.root, id, shardProof, tree.depth), TIER_NORMAL);
  }
});

test("locked names and sentinels are refused from shards", () => {
  const leaves = buildFixtureLeaves();
  const { tree, shards } = buildShardsFromLeaves(leaves);

  // Every real locked/reserved leaf must be refused with code="reserved".
  for (let i = 1; i < leaves.length - 1; i += 1) {
    const leaf = leaves[i];
    const prefixHex = leaf.id[0].toString(16).padStart(2, "0");
    const parsed = parseShard(shards.get(prefixHex), {
      expectedPrefix: leaf.id[0],
      expectedRoot: tree.root,
      expectedDepth: tree.depth,
    });
    assert.throws(
      () => proveFromShard(parsed, leaf.id),
      (err) => err?.code === "reserved",
      `expected locked leaf ${i} (prefix ${prefixHex}) to be refused`,
    );
  }

  const shard00 = parseShard(shards.get("00"), { expectedPrefix: 0, expectedRoot: tree.root, expectedDepth: tree.depth });
  const shardFF = parseShard(shards.get("ff"), { expectedPrefix: 0xff, expectedRoot: tree.root, expectedDepth: tree.depth });
  assert.throws(() => proveFromShard(shard00, SENTINEL_LOW), /isn't a name/);
  assert.throws(() => proveFromShard(shardFF, SENTINEL_HIGH), /isn't a name/);
});

test("createShardProver fetches only the needed shard, caches it, and refuses tampered shards or manifests", async () => {
  const dir = await mkdtemp(join(tmpdir(), "lns-shards-"));
  try {
    const leaves = buildFixtureLeaves();
    const { tree } = await writeShardsToDir(leaves, dir);

    const fetched = [];
    const diskFetch = async (url) => {
      fetched.push(String(url));
      const filename = String(url).split("/").pop();
      const buf = await readFile(join(dir, filename));
      return new Response(buf, { status: 200 });
    };

    const prover = createShardProver({
      baseUrl: "https://static.test/shortlist-shards/",
      expectedRoot: tree.root,
      expectedDepth: tree.depth,
      fetchImpl: diskFetch,
    });

    const allowedA = makeId(0x42, 7001);
    const allowedB = makeId(0x42, 7002);
    const proofA = await prover.prove(allowedA);
    assert.equal(verifyLengthProof(tree.root, allowedA, proofA, tree.depth), TIER_NORMAL);
    assert.deepEqual(fetched, [
      "https://static.test/shortlist-shards/manifest.json",
      "https://static.test/shortlist-shards/42.bin",
    ]);

    // Second lookup in the same prefix hits the cache (zero extra fetches).
    const proofB = await prover.prove(allowedB);
    assert.equal(verifyLengthProof(tree.root, allowedB, proofB, tree.depth), TIER_NORMAL);
    assert.equal(fetched.length, 2);

    // Locked name in prefix 0x42 is refused using the cached shard.
    const locked42 = leaves.find((l) => l.id[0] === 0x42);
    await assert.rejects(() => prover.prove(locked42.id), (err) => err?.code === "reserved");
    assert.equal(fetched.length, 2);

    // Tampered shard bytes (even 1 bit flipped in a leaf) -> refused by SHA-256 check.
    const real42 = new Uint8Array(await readFile(join(dir, "42.bin")));
    const tampered42 = Uint8Array.from(real42);
    tampered42[SHARD_HEADER_BYTES + 10] ^= 0x01;
    const tamperedProver = createShardProver({
      baseUrl: "https://static.test/shortlist-shards/",
      expectedRoot: tree.root,
      expectedDepth: tree.depth,
      fetchImpl: async (url) => {
        const filename = String(url).split("/").pop();
        if (filename === "42.bin") return new Response(tampered42, { status: 200 });
        return new Response(await readFile(join(dir, filename)), { status: 200 });
      },
    });
    await assert.rejects(() => tamperedProver.prove(allowedA), /SHA-256 mismatch/);

    // Tampered shard + forged manifest (matching tampered SHA-256!) -> still refused by Merkle root check!
    const forgedManifest = JSON.parse(await readFile(join(dir, "manifest.json"), "utf-8"));
    const forgedLeafBytes = Uint8Array.from(real42);
    forgedLeafBytes[forgedLeafBytes.length - 1] ^= 0x80; // flip last sibling hash byte
    const { sha256Hex } = await import("../../app-src/build-shortlist-shards.mjs");
    forgedManifest.shards["42"].sha256 = sha256Hex(forgedLeafBytes);
    const forgedProver = createShardProver({
      baseUrl: "https://static.test/shortlist-shards/",
      expectedRoot: tree.root,
      expectedDepth: tree.depth,
      fetchImpl: async (url) => {
        const filename = String(url).split("/").pop();
        if (filename === "manifest.json") return new Response(JSON.stringify(forgedManifest), { status: 200 });
        if (filename === "42.bin") return new Response(forgedLeafBytes, { status: 200 });
        return new Response("not found", { status: 404 });
      },
    });
    await assert.rejects(() => forgedProver.prove(allowedA), /does not match/);

    // Wrong on-chain root -> refused before any shard is even fetched.
    const wrongRoot = Uint8Array.from(tree.root);
    wrongRoot[0] ^= 0xff;
    const wrongRootProver = createShardProver({
      baseUrl: "https://static.test/shortlist-shards/",
      expectedRoot: wrongRoot,
      expectedDepth: tree.depth,
      fetchImpl: diskFetch,
    });
    await assert.rejects(() => wrongRootProver.prove(allowedA), /doesn't match the on-chain root/);

    // Wrong prefix shard served for prefix 0x42 -> refused by parseShard directly.
    const real05 = new Uint8Array(await readFile(join(dir, "05.bin")));
    assert.throws(
      () => parseShard(real05, { expectedPrefix: 0x42, expectedRoot: tree.root, expectedDepth: tree.depth }),
      /prefix mismatch/,
    );
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("minimal 2-sentinel list (0 locked names) shards and proves cleanly", () => {
  const leaves = makeSortedLeaves([]);
  const { tree, shards } = buildShardsFromLeaves(leaves);
  assert.equal(tree.depth, 1);
  for (const b of [0x00, 0x01, 0x7f, 0xff]) {
    const parsed = parseShard(shards.get(b.toString(16).padStart(2, "0")), {
      expectedPrefix: b,
      expectedRoot: tree.root,
      expectedDepth: 1,
    });
    const probe = makeId(b, 42);
    const proof = proveFromShard(parsed, probe);
    assert.equal(verifyLengthProof(tree.root, probe, proof, 1), TIER_NORMAL);
  }
});
