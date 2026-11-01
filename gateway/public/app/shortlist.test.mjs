// The browser's proof against the real list: run with `node --test`.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const listBytes = readFileSync(new URL("./shortlist.bin", import.meta.url));
const serve = (bytes, status = 200) => {
  globalThis.fetch = async (url) => {
    assert.ok(String(url).endsWith("/shortlist.bin"));
    return new Response(bytes, { status });
  };
};
const fresh = (tag) => import(`./register.js?${tag}`);
const hex = (a) => Buffer.from(a).toString("hex");
const idOf = (first) => new Uint8Array(32).fill(0x5a).map((b, i) => (i === 0 ? first : b));

test("a proof brackets the id with neighbouring leaves", async () => {
  serve(listBytes);
  const { lengthProof } = await fresh("good");
  for (const first of [0x01, 0x5a, 0xab, 0xfe]) {
    const id = idOf(first);
    const proof = await lengthProof(id);
    assert.equal(proof.kind, "NonMembership");
    assert.ok(hex(proof.left.id) < hex(id) && hex(id) < hex(proof.right.id));
    assert.equal(proof.leftPath.length, 16);
    assert.equal(proof.rightPath.length, 16);
  }
});

test("a listed id is refused", async () => {
  serve(listBytes);
  const { lengthProof } = await fresh("listed");
  await assert.rejects(lengthProof(new Uint8Array(listBytes.subarray(33 * 5, 33 * 5 + 32))), /reserved/);
});

test("a changed list is refused before anything is signed", async () => {
  const changed = new Uint8Array(listBytes);
  changed[33 * 7] ^= 1;
  serve(changed);
  const { lengthProof } = await fresh("changed");
  await assert.rejects(lengthProof(idOf(0x5a)), /doesn't match/);
});

test("a missing list says so, and is tried again", async () => {
  serve(null, 404);
  const { lengthProof } = await fresh("missing");
  await assert.rejects(lengthProof(idOf(0x5a)), /HTTP 404/);
  serve(listBytes);
  assert.equal((await lengthProof(idOf(0x5a))).kind, "NonMembership");
});
