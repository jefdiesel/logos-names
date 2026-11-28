// Run: node --test gateway/public/app/claims.test.mjs
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import { claimFor, parseSnapshot } from "./claims.js";
import { base58Encode, fromHex, systemProgramId, toHex } from "./lez-tx.js";
import {
  CLOCK_ACCOUNT,
  CLOCK_PROGRAM,
  MIN_COMMIT_AGE_MS,
  PUBLIC_MIN_COMMIT_AGE_MS,
  PUBLIC_TIME_WINDOW_MS,
  TIME_WINDOW_MS,
  chainNow,
  confirmCommitted,
  confirmRegistered,
  setRpc,
} from "./register.js";
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
  accounts.push(entry(fill(40), new Uint8Array(9).fill(5))); // an earlier-chain commitment
  accounts.push(entry(fill(41), new Uint8Array(10).fill(6))); // a v0.4.0 10-byte commitment
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
function answering(state, onRequest = null) {
  globalThis.fetch = async (_url, request) => {
    const { params } = JSON.parse(request.body);
    onRequest?.(params[0]);
    const result = state === null ? null : { data: { actor_states: { [params[0].program_account_id]: [...state] } } };
    return { ok: true, json: async () => ({ result }) };
  };
}

test("register.js v0.4.0 public commit windows and clock addresses", () => {
  assert.equal(PUBLIC_MIN_COMMIT_AGE_MS, 5 * 60 * 1000);
  assert.equal(MIN_COMMIT_AGE_MS, 15 * 60 * 1000);
  assert.equal(PUBLIC_TIME_WINDOW_MS, 60 * 1000);
  assert.equal(TIME_WINDOW_MS, 10 * 60 * 1000);
  assert.equal(CLOCK_ACCOUNT, base58Encode(new TextEncoder().encode("/LEZ/ClockProgramAccount/0000000")));
  assert.equal(CLOCK_PROGRAM, base58Encode(systemProgramId("clock")));
  // The Rust ids (clock_core on LEZ v0.4.0), as vectors/ and the testimonial app have them.
  assert.equal(CLOCK_ACCOUNT, VECTORS.clock.account);
  assert.equal(CLOCK_PROGRAM, VECTORS.clock.program);
  const testimonial = JSON.parse(readFileSync(new URL("../../../apps/testimonial/test/vectors.json", import.meta.url)));
  assert.deepEqual(testimonial.clock, { account: CLOCK_ACCOUNT, program: CLOCK_PROGRAM });
});

test("the chain's clock is read from the v0.4.0 clock system program and fails closed if unreadable", async () => {
  const clock = new Uint8Array(16);
  new DataView(clock.buffer).setBigUint64(8, 1_700_000_000_000n, true);
  let seen = null;
  answering(clock, (req) => { seen = req; });
  assert.equal(await chainNow("http://x"), 1_700_000_000_000);
  assert.deepEqual(seen, { account_id: CLOCK_ACCOUNT, program_account_id: CLOCK_PROGRAM });

  // Missing, truncated, or zero clock state must reject instead of falling back to Date.now().
  answering(null);
  await assert.rejects(chainNow("http://x"), /clock/i);
  answering(new Uint8Array(8));
  await assert.rejects(chainNow("http://x"), /clock/i);
  answering(new Uint8Array(16));
  await assert.rejects(chainNow("http://x"), /clock/i);
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

  // Old 9-byte commitment state is rejected on v0.4.0.
  answering(Uint8Array.of(6, 1, 0, 0, 0, 0, 0, 0, 0));
  await assert.rejects(confirmCommitted("http://x", program, fill(3), "tx"), { code: "reverted" });

  // 10-byte Commitment: version(1) + made(8) + public(1).
  answering(Uint8Array.of(6, 1, 0, 0, 0, 0, 0, 0, 0, 1));
  const pub = await confirmCommitted("http://x", program, fill(3), "tx", { public: true });
  assert.deepEqual(pub, { version: 6, made: 1, public: true });
  await assert.rejects(confirmCommitted("http://x", program, fill(3), "tx", { public: false }), { code: "reverted" });

  answering(Uint8Array.of(6, 1, 0, 0, 0, 0, 0, 0, 0, 0));
  const priv = await confirmCommitted("http://x", program, fill(3), "tx", { public: false });
  assert.deepEqual(priv, { version: 6, made: 1, public: false });
  await assert.rejects(confirmCommitted("http://x", program, fill(3), "tx", { public: true }), { code: "reverted" });
});

// ---------- engine-report.json: the browser's transactions run on the real v0.4.0 engine ----------

const REPORT = JSON.parse(readFileSync(new URL("../../app-src/vectors/engine-report.json", import.meta.url)));

test("the app's own register.js transactions fit their gas limits on the real engine", () => {
  const steps = Object.fromEntries(REPORT.steps.map((step) => [step.label, step]));
  for (const label of ["register.js commit public", "register.js register"]) {
    const { outcome, cycles, gasLimit } = steps[label];
    assert.equal(outcome, "ok", label);
    assert.ok(cycles > 0 && cycles <= gasLimit, `${label}: ${cycles} cycles under a ${gasLimit} limit`);
  }
});

test("a public commitment the real guest stored confirms as public", async () => {
  const view = REPORT.views.publicCommitment;
  const [[program]] = Object.entries(view.data.actor_states);
  globalThis.fetch = async (_url, { body }) => Response.json({ jsonrpc: "2.0", id: JSON.parse(body).id, result: view });
  setRpc("http://rpc.test");
  const commitment = await confirmCommitted("http://rpc.test", program, fill(0xdd), "tx", { public: true });
  assert.equal(commitment.public, true);
  assert.equal(commitment.version, 6);
  await assert.rejects(confirmCommitted("http://rpc.test", program, fill(0xdd), "tx", { public: false }), /failed/);
});

test("each signer's key on the engine is the next key its last transaction committed to", () => {
  assert.deepEqual(REPORT.signingKeys, REPORT.expectedKeys);
});
