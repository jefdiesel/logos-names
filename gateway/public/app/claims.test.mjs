// Run: node --test gateway/public/app/claims.test.mjs
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import { claimFor, parseSnapshot } from "./claims.js";
import { base58Encode, fromHex, toHex } from "./lez-tx.js";
import { chainNow, confirmCommitted, confirmRegistered, setRpc } from "./register.js";
import { claimHolds, claimLeaf, claimProof, claimRoot, ownerTag, pageAddress, recordAddress } from "./lns-tx.js";

const VECTORS = JSON.parse(readFileSync(new URL("../../app-src/vectors/vectors.json", import.meta.url)));
const fill = (byte) => new Uint8Array(32).fill(byte);
const le32 = (n) => { const b = new Uint8Array(4); new DataView(b.buffer).setUint32(0, n, true); return b; };
const join = (...parts) => Uint8Array.from(parts.flatMap((p) => [...p]));

test("root and proofs match the Rust list", () => {
  const leaves = VECTORS.claims.leaves.map(fromHex);
  assert.equal(toHex(claimRoot(leaves)), VECTORS.claims.root);
  assert.deepEqual(claimProof(leaves, 3).map(toHex), VECTORS.claims.proof3);
  assert.deepEqual(claimProof(leaves, 4).map(toHex), VECTORS.claims.proof4);
  assert.equal(toHex(claimRoot([])), "00".repeat(32));
});

const PROGRAM = fill(7);
const ALICE = fill(1);
const record = (id, holder, sealed) => join([5], ownerTag(id, holder), new Uint8Array(8).fill(255), le32(sealed.length), sealed);
const entry = (account, data) => ({ account: base58Encode(account), data: toHex(data) });

test("a snapshot gives the proof the registry checks, and other accounts are not leaves", () => {
  const ids = [fill(11), fill(12), fill(13)];
  const accounts = ids.map((id) => entry(recordAddress(PROGRAM, id), record(id, ALICE, Uint8Array.of(1, 2, 3))));
  accounts.push(entry(pageAddress(PROGRAM, ids[0]), join([5], le32(4), Uint8Array.of(9, 9, 9, 9))));
  accounts.push(entry(fill(40), new Uint8Array(9).fill(5))); // a commitment
  const snapshot = parseSnapshot({ program: base58Encode(PROGRAM), height: 1, accounts });
  assert.equal(snapshot.leaves.length, 3);
  for (const id of ids) {
    const { proof, record: found } = claimFor(snapshot, id);
    assert.deepEqual([...found.sealed], [1, 2, 3]);
    assert.ok(claimHolds({ claimsRoot: snapshot.root, claimsProgram: PROGRAM, id, account: ALICE, proof }));
    assert.ok(!claimHolds({ claimsRoot: snapshot.root, claimsProgram: PROGRAM, id, account: fill(2), proof }));
  }
  assert.deepEqual([...claimFor(snapshot, ids[0]).page], [9, 9, 9, 9]);
  assert.equal(claimFor(snapshot, ids[1]).page, null);
  assert.equal(claimFor(snapshot, fill(99)), null);
  // The order of the file does not matter, as on the Rust side.
  const reversed = parseSnapshot({ program: base58Encode(PROGRAM), height: 1, accounts: [...accounts].reverse() });
  assert.equal(toHex(reversed.root), toHex(snapshot.root));
  assert.equal(toHex(claimLeaf(snapshot.records[0].account, snapshot.records[0].owner)), toHex(snapshot.leaves[0]));
});

/** A sequencer whose every `getAccountView` answers with `state` under any program. */
function answering(state) {
  globalThis.fetch = async (_url, request) => {
    const { params } = JSON.parse(request.body);
    const result = state === null ? null : { data: { actor_states: { [params[0].program_account_id]: [...state] } } };
    return { ok: true, json: async () => ({ result }) };
  };
}

test("the chain's clock is read from its account, and falls back behind this device's", async () => {
  const clock = new Uint8Array(16);
  new DataView(clock.buffer).setBigUint64(8, 1_700_000_000_000n, true);
  answering(clock);
  assert.equal(await chainNow("http://x"), 1_700_000_000_000);
  answering(null);
  const fallback = await chainNow("http://x");
  assert.ok(fallback < Date.now() - 50000 && fallback > Date.now() - 70000);
});

test("success is only what the chain shows afterwards", async () => {
  setRpc("http://x");
  const program = base58Encode(PROGRAM);
  const id = fill(11);
  const held = join([6], ownerTag(id, ALICE), new Uint8Array(8).fill(255), le32(0), [0]);
  answering(held);
  await confirmRegistered("http://x", program, id, ALICE, "tx");
  await assert.rejects(confirmRegistered("http://x", program, id, fill(2), "tx"), /failed.*fee was charged/);
  answering(null);
  await assert.rejects(confirmRegistered("http://x", program, id, ALICE, "tx"), { code: "reverted" });
  await assert.rejects(confirmCommitted("http://x", program, fill(3), "tx"), /commitment/i);
  answering(Uint8Array.of(6, 1, 0, 0, 0, 0, 0, 0, 0));
  await confirmCommitted("http://x", program, fill(3), "tx");
});
