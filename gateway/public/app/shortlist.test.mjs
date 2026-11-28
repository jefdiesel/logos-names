// The browser's length proof against the real locked-name list (1,845,228 leaves, depth 21,
// root 9778bed4…990c): the 256 shards are built here from the repo's shortlist/shortlist.bin,
// served through a stubbed fetch, and proved through register.js's own `lengthProof`.
// Run with `node --test`.
import assert from "node:assert/strict";
import { readFileSync, writeFileSync } from "node:fs";
import { test } from "node:test";

import { buildShardsFromLeaves, parseShortlistBin } from "../../app-src/build-shortlist-shards.mjs";
import { SHORTLIST_ROOT, encodeLengthProof, verifyLengthProof } from "./shortlist-shards.js";

const hex = (a) => Buffer.from(a).toString("hex");
const BASE = "http://shards.test/";
const listBytes = readFileSync(new URL("../../../shortlist/shortlist.bin", import.meta.url));
const leaves = parseShortlistBin(new Uint8Array(listBytes));
const built = buildShardsFromLeaves(leaves);
const committed = JSON.parse(readFileSync(new URL("./shortlist-shards/manifest.json", import.meta.url)));

let changedShard = null;
let missing = false;
const served = [];
globalThis.fetch = async (url) => {
  const path = String(url);
  assert.ok(path.startsWith(BASE), path);
  const file = path.slice(BASE.length).replace(/^[a-z]+\//, "");
  served.push(file);
  if (missing) return new Response(null, { status: 404 });
  if (file === "manifest.json") return Response.json(built.manifest);
  const prefix = file.replace(/\.bin$/, "");
  const bytes = built.shards.get(prefix);
  if (!bytes) return new Response(null, { status: 404 });
  if (changedShard === prefix) {
    const copy = Uint8Array.from(bytes);
    copy[copy.length - 1] ^= 1;
    return new Response(copy);
  }
  return new Response(bytes);
};
const fresh = async (tag) => {
  const module = await import(`./register.js?${tag}`);
  module.setShortlistShardsBase(`${BASE}${tag}/`); // a fresh shard cache per test
  return module;
};
const idOf = (first) => new Uint8Array(32).fill(0x5a).map((b, i) => (i === 0 ? first : b));

test("the shards built from the repo's list are the committed manifest, under the compiled root", () => {
  assert.equal(built.manifest.root, "9778bed49005fe7401d25685a27d9cf3e681fa0582b562336cdb8ee9661d990c");
  assert.equal(hex(SHORTLIST_ROOT), built.manifest.root);
  assert.equal(built.manifest.depth, 21);
  assert.equal(built.manifest.totalLeaves, 1_845_228);
  assert.deepEqual(built.manifest, committed, "shortlist-shards/manifest.json is what the builder makes from shortlist.bin");
});

test("a proof brackets the id with neighbouring leaves, from one shard, and verifies against the root", async () => {
  const { lengthProof } = await fresh("good");
  const proofs = {};
  for (const first of [0x00, 0x01, 0x5a, 0xab, 0xff]) {
    const id = first === 0x00 ? idOf(0x00).fill(0x01, 1) : first === 0xff ? idOf(0xff).fill(0xfe, 1) : idOf(first);
    served.length = 0;
    const proof = await lengthProof(id);
    assert.equal(proof.kind, "NonMembership");
    assert.ok(hex(proof.left.id) < hex(id) && hex(id) < hex(proof.right.id));
    assert.equal(proof.leftPath.length, 21);
    assert.equal(proof.rightPath.length, 21);
    verifyLengthProof(SHORTLIST_ROOT, id, proof);
    assert.ok(served.every((file) => file === "manifest.json" || file === `${hex(id.subarray(0, 1))}.bin`), "only its own shard");
    proofs[hex(id)] = hex(encodeLengthProof(proof));
  }
  // For the Rust check (gateway/app-src/vectors `check-proofs`), when asked.
  if (process.env.SHARD_PROOFS_OUT) writeFileSync(process.env.SHARD_PROOFS_OUT, JSON.stringify(proofs, null, 1));
});

test("a locked name on the list is refused", async () => {
  const { lengthProof } = await fresh("listed");
  const locked = leaves.find((leaf) => leaf.tier === 1 && leaf.id[0] === 0x42);
  await assert.rejects(lengthProof(locked.id), /reserved/);
});

test("a changed shard is refused before anything is signed", async () => {
  const { lengthProof } = await fresh("changed");
  changedShard = "5a";
  try {
    await assert.rejects(lengthProof(idOf(0x5a)), /integrity|mismatch|doesn't match/);
  } finally {
    changedShard = null;
  }
});

test("missing shards say so, and are tried again", async () => {
  const { lengthProof } = await fresh("missing");
  missing = true;
  await assert.rejects(lengthProof(idOf(0x77)), /HTTP 404/);
  missing = false;
  assert.equal((await lengthProof(idOf(0x77))).kind, "NonMembership");
});
