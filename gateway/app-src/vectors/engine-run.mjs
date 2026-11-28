#!/usr/bin/env node
// Builds transactions with the browser's own code (lez-tx.js, lns-tx.js) and writes them
// to a scenario the Rust side runs through the real LEZ 0.4 engine and guest:
//   node engine-run.mjs scenario.json
//   lns_js_vectors run scenario.json report.json
// Account secrets are [n; 32], as in src/main.rs; the first Configure is made there.
// A refused step is not applied (as engine-test does), so only an "ok" step uses up a nonce.
import { writeFileSync, readFileSync } from "node:fs";

import * as lez from "../../public/app/lez-tx.js";
import * as lns from "../../public/app/lns-tx.js";
import { buildCommit, buildRegister, setRpc } from "../../public/app/register.js";

const VECTORS = JSON.parse(readFileSync(new URL("vectors.json", import.meta.url)));
const T0 = 1_800_000_000_000;
const MIN_COMMIT_AGE_MS = 15 * 60 * 1000;
const PUBLIC_MIN_COMMIT_AGE_MS = 5 * 60 * 1000;
const PUBLIC_TIME_WINDOW_MS = 60 * 1000;
const GAS = 2_000_000n;
// What src/main.rs `run` configures: fee mode, the default price table, premium 64M, and the
// NFT program at [0x4E; 32] whose admin reserved the LNS collection for the registry.
const PRICES = lns.DEFAULT_PRICES;
const PREMIUM_START = 64_000_000n;
const SALE_PRICE = 3_000_017n;

const fill = (value) => new Uint8Array(32).fill(value);
const who = (byte) => {
  const key = fill(byte);
  return { key, id: lez.publicAccountId(key), nonce: 0n };
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
const nftProgram = fill(0x4e);
const [alice, bob] = [who(2), who(3)];
const treasury = who(7).id;
const payout = who(8).id;
const [jef, held] = [fill(0xaa), fill(0xcc)];
const steps = [];

const details = (signed) => ({ wire: lez.toHex(signed.wire), hash: signed.hash, messageHash: lez.toHex(signed.messageHash) });

function push(label, from, message, now, expect = "ok") {
  steps.push({ label, ...details(lez.signPublicTransaction(message, [from.key])), now, expect });
  if (expect === "ok") from.nonce += 1n;
}
const base = (from) => ({ program, signer: from.id, nonce: from.nonce, gasLimit: GAS });

// A private commitment, then a paid one-year registration that mints the tied NFT.
const slot = lns.tieredId(jef, lns.PRICE_TIER_8_PLUS);
const secret = fill(5);
const commitment = lns.commitmentHash(slot, lns.ownerTag(slot, alice.id), secret);
push("commit", alice, lns.commitMessage({ ...base(alice), now: T0, commitment }), T0);

const later = T0 + MIN_COMMIT_AGE_MS;
const sealed = new Uint8Array(284).fill(0xee); // the size of a sealed one-block profile
const proof = proofOf(VECTORS.lengthProofs.aa);
const yearPrice = PRICES[lns.PRICE_TIER_8_PLUS][lns.TERM_1Y];
const register = (fee) => lns.registerMessage({
  ...base(alice), id: jef, priceTier: lns.PRICE_TIER_8_PLUS, secret, sealed, term: lns.TERM_1Y, fee, treasury,
  now: later, deposit: false, lengthProof: proof, nftProgram,
});
push("register underpaying is refused", alice, register(yearPrice - 1n), later, "Failed");
push("register for a year (fee to the treasury, NFT minted)", alice, register(yearPrice), later);
push("register again is refused", alice, register(yearPrice), later, "Failed");
const token = lns.nftTokenId(slot, lns.registerGeneration(slot, lns.ownerTag(slot, alice.id), later, secret));

push("set record (directory untouched)", alice, lns.setRecordMessage({ ...base(alice), id: slot, sealed }), later);
push("set page and list the site", alice, lns.setPageMessage({
  ...base(alice), id: slot, sealed, siteDir: lns.siteDirUpsert({ name: "jefdiesel", title: "Alice's site", page: 0, now: later }),
}), later);
push("bob can't set alice's record", bob, lns.setRecordMessage({ ...base(bob), id: slot, sealed }), later, "Failed");
push("set primary", alice, lns.setPrimaryMessage({ ...base(alice), name: "alice" }), later);

// List in the market, then bob buys: 97.5% to the payout account, 2.5% (rounded up) to the treasury.
push("list in the market", alice, lns.listMessage({
  ...base(alice), id: slot, price: SALE_PRICE, payTo: payout, now: later, displayName: "jefdiesel",
}), later);
const buy = (expectedNonce) => lns.buyMessage({
  ...base(bob), id: slot, price: SALE_PRICE, payTo: payout, treasury, expectedNonce, now: later,
  inMarket: true, nftProgram, nftTokenId: token, sitePage: 0,
});
push("a buy against a stale listing nonce is refused", bob, buy(2), later, "Failed");
push("bob buys the name (NFT moves, site leaves the directory)", bob, buy(1), later);
push("bob gives the name back through the registry", bob, lns.transferMessage({
  ...base(bob), id: slot, newOwner: lns.ownerTag(slot, alice.id), now: later, nftProgram, nftTokenId: token,
}), later);
push("alice releases it (the NFT burns)", alice, lns.releaseMessage({
  ...base(alice), id: slot, refund: 0n, nftProgram, nftTokenId: token,
}), later);

// CommitPublic (variant 10): only within a minute of its claimed time.
const quickSlot = lns.tieredId(fill(0xdd), lns.PRICE_TIER_8_PLUS);
const quick = lns.commitmentHash(quickSlot, lns.ownerTag(quickSlot, bob.id), fill(7));
const commitPublic = () => lns.commitPublicMessage({ ...base(bob), now: later, commitment: quick });
push("a public commit outside its one-minute window is refused", bob, commitPublic(), later + PUBLIC_TIME_WINDOW_MS, "OutOfValidityWindow");
push("bob commits in public", bob, commitPublic(), later + PUBLIC_TIME_WINDOW_MS - 1);

// A claim from the earlier chain: free, permanent, and it mints the NFT too.
const claimProof = [lez.fromHex(VECTORS.scenarioClaimSibling)];
const claim = (from) => lns.claimMessage({ ...base(from), id: held, sealed, now: later, proof: claimProof, nftProgram });
push("bob can't claim alice's name", bob, claim(bob), later, "Failed");
push("alice claims for free", alice, claim(alice), later);
push("a name is claimed once", alice, claim(alice), later, "Failed");

const pay = (to, options = {}) => lez.transferMessage({ from: alice.id, to, nonce: alice.nonce, amount: 1000n, gasLimit: GAS, ...options });
push("pay an account on the chain", alice, pay(bob.id), later);
const fresh = who(11);
push("pay a new account with no evidence", alice, pay(fresh.id), later, "UnadmittedPublicActor");
push("pay a new account, showing its initial key", alice, pay(fresh.id, { evidence: [lez.keyEvidence(lez.initialKey(fresh.key))] }), later);
const undeclared = pay(who(12).id);
undeclared.actors = [];
push("pay an account the transaction doesn't declare", alice, undeclared, later, "UnboundAsyncDestination");

// The app's own flow, register.js with a real name (CommitPublic + 5-min wait), the RPC
// stubbed with the settings `run` configured (Settings = version 8 || Configure's fields).
const RPC = "http://rpc.test";
setRpc(RPC);
const program58 = lez.base58Encode(program);
const settingsAccount = lez.base58Encode(lns.settingsAddress(program));
const claimRootHex = VECTORS.scenarioClaimRoot;
const settingsBytes = lez.concat(Uint8Array.of(8), lns.configureInstruction({
  admin: program, treasury, prices: PRICES, mode: 0, deposits: [0n, 0n], claimsRoot: lez.fromHex(claimRootHex),
  claimsUntil: T0 + 10 * MIN_COMMIT_AGE_MS, claimsProgram: fill(0x38), premiumStart: PREMIUM_START, nftProgram,
}).subarray(2));
globalThis.fetch = async (url, options) => {
  const { id, method, params } = JSON.parse(options.body);
  if (method === "getAccountsNonces") return Response.json({ jsonrpc: "2.0", id, result: [Number(alice.nonce)] });
  if (method === "getAccountView") {
    const { account_id: account, program_account_id: selected } = params[0];
    const state = account === settingsAccount && selected === program58 ? Array.from(settingsBytes) : null;
    return Response.json({ jsonrpc: "2.0", id, result: state ? { nonce: 0, data: { actor_states: { [program58]: state } } } : null });
  }
  throw new Error(`unexpected ${method}`);
};
const name = "alice-lns";
const nameSecret = fill(6);
const shared = { privateKey: alice.key, program: program58, name, secret: nameSecret };
const committed = await buildCommit({ ...shared, now: later });
steps.push({ label: "register.js commit public", ...details(committed.signed), now: later, expect: "ok" });
alice.nonce += 1n;
const revealed = later + PUBLIC_MIN_COMMIT_AGE_MS;
// The registry checks names against the full locked-name list (depth 21, root 9778bed4);
// the proof here is the Rust one from that list (the shards give the same proof).
const registered = await buildRegister({ ...shared, now: revealed, proof: proofOf(VECTORS.lengthProofs.named) });
steps.push({ label: "register.js register", ...details(registered.signed), now: revealed, expect: "ok" });
alice.nonce += 1n;
const namedToken = lns.nftTokenId(registered.slotId, lns.registerGeneration(registered.slotId, lns.ownerTag(registered.slotId, alice.id), revealed, nameSecret));

// A 30-day name lapses: bob registers [0xBB; 32] for 30 days, and after its 30-day grace,
// two days into the re-release premium, alice re-registers it. The old NFT burns and a new
// one is minted (both declared).
const DAY_MS = 86_400_000;
const thirtyDays = PRICES[lns.PRICE_TIER_8_PLUS][lns.TERM_30D];
const short = fill(0xbb);
const shortSlot = lns.tieredId(short, lns.PRICE_TIER_8_PLUS);
const bobSecret = fill(8);
const shortStart = revealed + 1;
push("bob commits to a 30-day name", bob, lns.commitMessage({
  ...base(bob), now: shortStart, commitment: lns.commitmentHash(shortSlot, lns.ownerTag(shortSlot, bob.id), bobSecret),
}), shortStart);
const shortAt = shortStart + MIN_COMMIT_AGE_MS;
push("bob registers it for 30 days", bob, lns.registerMessage({
  ...base(bob), id: short, priceTier: lns.PRICE_TIER_8_PLUS, secret: bobSecret, sealed, term: lns.TERM_30D, fee: thirtyDays, treasury,
  now: shortAt, deposit: false, lengthProof: proofOf(VECTORS.lengthProofs.bb), nftProgram,
}), shortAt);
const shortToken = lns.nftTokenId(shortSlot, lns.registerGeneration(shortSlot, lns.ownerTag(shortSlot, bob.id), shortAt, bobSecret));
const graceEnds = shortAt + 30 * DAY_MS + 30 * DAY_MS;
const lapsedAt = graceEnds + 2 * DAY_MS;
const aliceSecret = fill(9);
const aliceHolder = lns.ownerTag(shortSlot, alice.id);
push("alice commits to the lapsed name", alice, lns.commitMessage({
  ...base(alice), now: lapsedAt, commitment: lns.commitmentHash(shortSlot, aliceHolder, aliceSecret),
}), lapsedAt);
const reRegisterAt = lapsedAt + MIN_COMMIT_AGE_MS;
const premium = lns.rereleasePremium(PREMIUM_START, BigInt(reRegisterAt - graceEnds));
const reRegister = (fee) => lns.registerMessage({
  ...base(alice), id: short, priceTier: lns.PRICE_TIER_8_PLUS, secret: aliceSecret, sealed, term: lns.TERM_30D, fee, treasury,
  now: reRegisterAt, deposit: false, lengthProof: proofOf(VECTORS.lengthProofs.bb), nftProgram, prevNftTokenId: shortToken,
});
push("re-registering without the premium is refused", alice, reRegister(thirtyDays), reRegisterAt, "Failed");
push("alice re-registers the lapsed name with the premium (old NFT burned, new one minted)", alice, reRegister(thirtyDays + premium), reRegisterAt);
const reToken = lns.nftTokenId(shortSlot, lns.registerGeneration(shortSlot, aliceHolder, reRegisterAt, aliceSecret));

const nft58 = lez.base58Encode(nftProgram);
const heldSlot = lns.tieredId(held, lns.PRICE_TIER_8_PLUS);
const record = lez.base58Encode(lns.recordAddress(program, heldSlot));
const views = [
  { label: "claimedRecord", account: record, program: program58 },
  { label: "namedRecord", account: lez.base58Encode(lns.recordAddress(program, registered.slotId)), program: program58 },
  { label: "namedToken", account: lez.base58Encode(lns.nftTokenAddress(nftProgram, namedToken)), program: nft58 },
  { label: "lapsedToken", account: lez.base58Encode(lns.nftTokenAddress(nftProgram, shortToken)), program: nft58 },
  { label: "reRegisteredToken", account: lez.base58Encode(lns.nftTokenAddress(nftProgram, reToken)), program: nft58 },
  { label: "reRegisteredRecord", account: lez.base58Encode(lns.recordAddress(program, shortSlot)), program: program58 },
  { label: "releasedRecord", account: lez.base58Encode(lns.recordAddress(program, slot)), program: program58 },
  { label: "releasedToken", account: lez.base58Encode(lns.nftTokenAddress(nftProgram, token)), program: nft58 },
  { label: "marketBucket", account: lez.base58Encode(lns.marketBucketAddress(program, lns.marketBucketIndex(slot))), program: program58 },
  { label: "siteHeader", account: lez.base58Encode(lns.siteDirHeaderAddress(program)), program: program58 },
  { label: "settings", account: settingsAccount, program: program58 },
  { label: "emptyActor", account: record, program: lez.base58Encode(fill(0)) },
  { label: "reverse", account: lez.base58Encode(alice.id), program: program58 },
  { label: "missing", account: lez.base58Encode(fill(0x44)), program: program58 },
  { label: "publicCommitment", account: lez.base58Encode(lns.commitmentAddress(program, quick)), program: program58 },
];
const balances = [alice, bob, fresh, who(12)].map(({ id }) => lez.base58Encode(id));
balances.push(lez.base58Encode(treasury), lez.base58Encode(payout));
// Each signer's nonce and current key afterwards: the next key its last transaction committed to.
const signers = [alice, bob].map(({ id }) => lez.base58Encode(id));
const expectedKeys = Object.fromEntries([alice, bob].map(({ key, id, nonce }) => [
  lez.base58Encode(id), { nonce: String(nonce), key: lez.toHex(lez.encodeSigningKeyState(lez.keyState(key, nonce))) },
]));
writeFileSync(process.argv[2], JSON.stringify({
  program: program58, nftProgram: nft58, account: lez.base58Encode(alice.id), salePrice: String(SALE_PRICE), yearPrice: String(yearPrice),
  premium: String(premium), thirtyDays: String(thirtyDays),
  steps, views, balances, signers, expectedKeys,
}, null, 1));
