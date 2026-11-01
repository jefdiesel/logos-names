#!/usr/bin/env node
// Builds transactions with the browser's own code (lez-tx.js, lns-tx.js) and writes them
// to a scenario the Rust side runs through the real LEZ 0.4 engine and guest:
//   node engine-run.mjs scenario.json
//   lns_js_vectors run scenario.json report.json
// Keys are [n; 32], as in src/main.rs; the first Configure is made there.
import { writeFileSync, readFileSync } from "node:fs";

import * as lez from "../../public/app/lez-tx.js";
import * as lns from "../../public/app/lns-tx.js";
import { buildCommit, buildRegister, setRpc } from "../../public/app/register.js";

const VECTORS = JSON.parse(readFileSync(new URL("vectors.json", import.meta.url)));
const T0 = 1_800_000_000_000;
const MIN_COMMIT_AGE_MS = 15 * 60 * 1000;
const DEPOSIT = 1_000_000n;
const GAS = 2_000_000n;

const fill = (value) => new Uint8Array(32).fill(value);
const who = (byte) => {
  const key = fill(byte);
  return { key, id: lez.accountIdOfPrivateKey(key), nonce: 0n };
};
const proofOf = (json) => ({
  kind: "NonMembership",
  left: { id: lez.fromHex(json.left.id), tier: json.left.tier },
  leftIndex: json.leftIndex,
  leftPath: json.leftPath.map(lez.fromHex),
  right: { id: lez.fromHex(json.right.id), tier: json.right.tier },
  rightPath: json.rightPath.map(lez.fromHex),
});

const program = who(9).id;
const [alice, bob] = [who(2), who(3)];
const [jef, held] = [fill(0xaa), fill(0xcc)];
const steps = [];

const details = (signed) => ({ wire: lez.toHex(signed.wire), hash: signed.hash, messageHash: lez.toHex(signed.messageHash) });

function push(label, from, message, now, expect = "ok") {
  steps.push({ label, ...details(lez.signPublicTransaction(message, [from.key])), now, expect });
  if (expect === "ok") from.nonce += 1n;
}
const base = (from) => ({ program, signer: from.id, nonce: from.nonce, gasLimit: GAS });

const secret = fill(5);
const commitment = lns.commitmentHash(jef, lns.ownerTag(jef, alice.id), secret);
push("commit", alice, lns.commitMessage({ ...base(alice), now: T0, commitment }), T0);

const later = T0 + MIN_COMMIT_AGE_MS;
const sealed = new Uint8Array(284).fill(0xee); // the size of a sealed one-block profile
const proof = proofOf(VECTORS.lengthProofs.aa);
push("register with a deposit", alice, lns.registerMessage({
  ...base(alice), id: jef, secret, sealed, fee: DEPOSIT, now: later, deposit: true, lengthProof: proof,
}), later);
push("register again is refused", alice, lns.registerMessage({
  ...base(alice), id: jef, secret, sealed, fee: DEPOSIT, now: later, deposit: true, lengthProof: proof,
}), later, "Failed");
push("set record", alice, lns.setRecordMessage({ ...base(alice), id: jef, sealed }), later);
push("set page", alice, lns.setPageMessage({ ...base(alice), id: jef, sealed }), later);
push("bob can't set alice's record", bob, lns.setRecordMessage({ ...base(bob), id: jef, sealed }), later, "Failed");
push("set primary", alice, lns.setPrimaryMessage({ ...base(alice), name: "alice" }), later);
push("transfer the name to bob", alice, lns.transferMessage({ ...base(alice), id: jef, newOwner: lns.ownerTag(jef, bob.id) }), later);
push("bob releases and collects the deposit", bob, lns.releaseMessage({ ...base(bob), id: jef, refund: DEPOSIT }), later);

const claimProof = [lez.fromHex(VECTORS.scenarioClaimSibling)];
const claim = (from) => lns.claimMessage({ ...base(from), id: held, sealed, now: later, proof: claimProof });
push("bob can't claim alice's name", bob, claim(bob), later, "Failed");
push("alice claims for free", alice, claim(alice), later);
push("a name is claimed once", alice, claim(alice), later, "Failed");

const pay = (to, options = {}) => lez.transferMessage({ from: alice.id, to, nonce: alice.nonce, amount: 1000n, gasLimit: GAS, ...options });
push("pay an account on the chain", alice, pay(bob.id), later);
const fresh = who(11);
push("pay a new account with no evidence", alice, pay(fresh.id), later, "UnadmittedPublicActor");
push("pay a new account, showing its key", alice, pay(fresh.id, { evidence: [lez.keyEvidence(lez.publicKeyOf(fresh.key))] }), later);
const undeclared = pay(who(12).id);
undeclared.actors = [];
push("pay an account the transaction doesn't declare", alice, undeclared, later, "UnboundAsyncDestination");

// The app's own flow, register.js with a real name, and the RPC it would have used stubbed.
const RPC = "http://rpc.test";
setRpc(RPC);
globalThis.fetch = async (url, options) => {
  if (String(url).endsWith("/shortlist.bin")) return new Response(readFileSync(new URL(url)));
  const { id, method } = JSON.parse(options.body);
  if (method !== "getAccountsNonces") throw new Error(`unexpected ${method}`);
  return Response.json({ jsonrpc: "2.0", id, result: [Number(alice.nonce)] });
};
const name = "alice-lns";
const nameSecret = fill(6);
const shared = { privateKey: alice.key, program: lez.base58Encode(program), name, secret: nameSecret };
const committed = await buildCommit({ ...shared, now: later });
steps.push({ label: "register.js commit", ...details(committed.signed), now: later, expect: "ok" });
alice.nonce += 1n;
const revealed = later + MIN_COMMIT_AGE_MS;
const registered = await buildRegister({ ...shared, now: revealed });
steps.push({ label: "register.js register", ...details(registered.signed), now: revealed, expect: "ok" });
alice.nonce += 1n;

const program58 = lez.base58Encode(program);
const record = lez.base58Encode(lns.recordAddress(program, held));
const views = [
  { label: "claimedRecord", account: record, program: program58 },
  { label: "namedRecord", account: lez.base58Encode(lns.recordAddress(program, registered.id)), program: program58 },
  { label: "settings", account: lez.base58Encode(lns.settingsAddress(program)), program: program58 },
  { label: "emptyActor", account: record, program: lez.base58Encode(fill(0)) },
  { label: "reverse", account: lez.base58Encode(alice.id), program: program58 },
  { label: "missing", account: lez.base58Encode(fill(0x44)), program: program58 },
];
const balances = [alice, bob, fresh, who(12)].map(({ id }) => lez.base58Encode(id));
balances.push(lez.base58Encode(lns.depositAddress(program, registered.id)));
balances.push(lez.base58Encode(lns.depositAddress(program, jef)));
writeFileSync(process.argv[2], JSON.stringify({ program: program58, account: lez.base58Encode(alice.id), steps, views, balances }, null, 1));
