// Pinned vectors from the LEZ v0.4.0 Rust sources, hand-computed byte-for-byte borsh
// encodings, and the unchanged v0.4.0-compatible sections of vectors.json (made by
// ../../app-src/vectors). Run: node --test gateway/public/app/lez-tx.test.mjs
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import {
  ACCOUNT_ID_PREFIX,
  ACCOUNT_ID_SCHNORR_PREFIX,
  AUTHORITY_NONE,
  AUTHORITY_SIGNER,
  Authority,
  KEYED_MESSAGE_PREFIX,
  KEY_SEED_PREFIX,
  KEY_TYPE_SCHNORR,
  KeyType,
  MESSAGE_PREFIX,
  NATIVE_TOKEN_PROGRAM_ID,
  PDA_PREFIX,
  SCHNORR_KEYGEN_PREFIX,
  SCHNORR_KEY_COMMIT_PREFIX,
  SYSTEM_PROGRAM_SEED_PREFIX,
  SYSTEM_UPGRADER_ACCOUNT_ID,
  accountExists,
  accountIdForInitialKey,
  actor,
  base58Decode,
  base58Encode,
  checkAccountSecret,
  concat,
  encodeLeeTransaction,
  encodeMessage,
  encodePublicTransaction,
  encodeSigningKeyState,
  encodeWitnessSet,
  feeDeclaration,
  feeReserve,
  formatAccountId,
  fromBase64,
  fromHex,
  getAccountView,
  hashWithKeys,
  initialKey,
  initialKeyBytes,
  isValidPublicKey,
  keyEvidence,
  keySeed,
  keyState,
  maxFeeFor,
  messageHash,
  messageWithKeys,
  nativeBalance,
  parseAccountId,
  pdaEvidence,
  publicAccountId,
  publicChild,
  publicKeyOf,
  publicRoot,
  randomAccountSecret,
  schnorrKeyCommit,
  schnorrKeygen,
  signHash,
  signPublicTransaction,
  signTransfer,
  signerActor,
  signingKey,
  signingKeyState,
  systemProgramId,
  toHex,
  transferInstruction,
  transferMessage,
  verifyHash,
  verifyWitnessEntry,
  witnessSetFor,
} from "./lez-tx.js";
import * as lns from "./lns-tx.js";

const VECTORS = JSON.parse(readFileSync(new URL("../../app-src/vectors/vectors.json", import.meta.url)));

// An independent SHA-256 (Node's), not the one bundled in lez-tx.js.
const sha256 = (data) => new Uint8Array(createHash("sha256").update(data).digest());

const bytes = (...parts) => Uint8Array.from(parts.flat());
const fill = (value, length = 32) => new Uint8Array(length).fill(value);
const pad32 = (text) => {
  const out = new Uint8Array(32);
  out.set(new TextEncoder().encode(text));
  return out;
};

// ---------- domain separation prefixes (LEZ v0.4.0) ----------

test("domain separation prefixes match the 32-byte zero-padded Rust constants", () => {
  assert.equal(toHex(MESSAGE_PREFIX), toHex(pad32("/LEE/v0.3/Message/Public/")));
  assert.equal(toHex(KEYED_MESSAGE_PREFIX), toHex(pad32("/LEE/v0.3/KeyedMessage/Public/")));
  assert.equal(toHex(ACCOUNT_ID_PREFIX), toHex(pad32("/LEE/v0.3/AccountId/Public/")));
  assert.equal(toHex(ACCOUNT_ID_SCHNORR_PREFIX), toHex(pad32("/LEE/v0.3/AccountId/Public/Schn/")));
  assert.equal(toHex(KEY_SEED_PREFIX), toHex(pad32("/LEE/v0.3/KeySeed/")));
  assert.equal(toHex(SCHNORR_KEYGEN_PREFIX), toHex(pad32("/LEE/v0.3/KeyGen/Schnorr/")));
  assert.equal(toHex(SCHNORR_KEY_COMMIT_PREFIX), toHex(pad32("/LEE/v0.3/KeyCommitment/Schnorr/")));
  assert.equal(toHex(PDA_PREFIX), toHex(pad32("/LEE/v0.2/AccountId/PDA/")));
  assert.equal(toHex(SYSTEM_PROGRAM_SEED_PREFIX), toHex(pad32("/LEE/SystemUpgrader/v1/Name")));
  assert.equal(toHex(SYSTEM_UPGRADER_ACCOUNT_ID), toHex(fill(0xfd)));
  assert.equal(Authority.None, 0);
  assert.equal(Authority.Signer, 1);
  assert.equal(KeyType.Schnorr, 0);
});

// ---------- message.rs (LEZ v0.4.0) ----------

test("message.rs a_public_message_has_a_pinned_layout_and_hash", () => {
  const to = signerActor(fill(42), fill(0));
  const message = { to, data: bytes(0), actors: [to], nonces: [{ accountId: fill(42), nonce: 1n }], fee: null };
  // Hand-computed from lez-v0.4.0/lee/state_machine/src/public_transaction/message.rs:141-160.
  const expected = concat(
    bytes(1, 0, 0, 0), fill(42), fill(0), bytes(1), // public_context.actors: (actor, Authority::Signer)
    bytes(1, 0, 0, 0), fill(42), // public_context.authorized_accounts
    bytes(0, 0, 0, 0), // public_context.promotions
    bytes(0, 0, 0, 0), // public_context.private_signers
    fill(42), fill(0), // execution.root.to
    bytes(1, 0, 0, 0, 0), // execution.root.message
    bytes(1, 0, 0, 0), fill(42), bytes(1), new Uint8Array(15), // nonces
    bytes(0), // fee: None (after nonces in v0.4.0)
    bytes(0, 0, 0, 0), // admission_evidence
  );
  assert.equal(expected.length, 239);
  assert.equal(toHex(encodeMessage(message)), toHex(expected));
  assert.equal(toHex(messageHash(message)), toHex(sha256(concat(MESSAGE_PREFIX, expected))));
});

test("message.rs message_with_keys_has_known_answers and order sensitivity", () => {
  const to = signerActor(fill(42), fill(0));
  const message = { to, data: bytes(0), actors: [to], nonces: [{ accountId: fill(42), nonce: 1n }], fee: null };
  const key = (b) => signingKeyState(fill(b));
  // Known answers pinned in lez-v0.4.0/lee/state_machine/src/public_transaction/message.rs:168-178.
  assert.equal(
    toHex(messageWithKeys(message, [key(7)])),
    "ae983ec82822a8363e7fc615b34760e312acba3b319b34b6d58e96b4192b6ed7",
  );
  assert.equal(
    toHex(messageWithKeys(message, [key(1), key(2), key(3)])),
    "53a14d2f1445b1e1eb042d95a39d58268edf89efc90600be5eb8fa5aa9da37fc",
  );
  assert.equal(
    toHex(hashWithKeys(messageHash(message), [key(7)])),
    toHex(sha256(concat(KEYED_MESSAGE_PREFIX, messageHash(message), encodeSigningKeyState(key(7))))),
  );
  assert.notEqual(
    toHex(messageWithKeys(message, [key(1), key(2), key(3)])),
    toHex(messageWithKeys(message, [key(2), key(1), key(3)])),
  );
});

test("encodeMessage sorts, merges actor authority (Signer wins), puts nonces before fee, and encodes v0.4.0 evidence", () => {
  const [high, low, program] = [fill(0xf0), fill(0x10), fill(0x77)];
  const message = {
    to: signerActor(high, program),
    data: bytes(1, 2, 3),
    // Duplicate (high, program) with Authority.None must merge to Authority.Signer; duplicate (low, program) collapses.
    actors: [
      actor(high, program),
      nativeBalance(high, AUTHORITY_SIGNER),
      actor(low, program),
      nativeBalance(low),
      actor(low, program),
    ],
    nonces: [{ accountId: high, nonce: 9n }, { accountId: low, nonce: 2n ** 64n }],
    fee: feeDeclaration(high, 250_000n, 3n, 12_345_678_901_234n),
    evidence: [pdaEvidence(program, fill(0x5d)), keyEvidence(signingKeyState(fill(0xab)))],
  };
  // Hand-computed from TransactionMessage + PublicExecutionContext + PublicAccountEvidence borsh layout.
  const expected = concat(
    bytes(4, 0, 0, 0),
    low, fill(0), bytes(AUTHORITY_NONE),
    low, program, bytes(AUTHORITY_NONE),
    high, fill(0), bytes(AUTHORITY_SIGNER),
    high, program, bytes(AUTHORITY_SIGNER),
    bytes(2, 0, 0, 0), low, high, // authorized_accounts
    bytes(0, 0, 0, 0), // promotions
    bytes(0, 0, 0, 0), // private_signers
    high, program, // execution.root.to
    bytes(3, 0, 0, 0, 1, 2, 3), // execution.root.message
    bytes(2, 0, 0, 0), // nonces (sorted by account_id)
    low, new Uint8Array(8), bytes(1), new Uint8Array(7), // 2^64 LE
    high, bytes(9), new Uint8Array(15), // 9 LE
    bytes(1), high, // fee: Some(FeeDeclaration)
    bytes(0x90, 0xd0, 0x03, 0, 0, 0, 0, 0), // gas_limit = 250_000 u64 LE
    bytes(3, 0, 0, 0, 0, 0, 0, 0), // tip = 3 u64 LE
    bytes(0xf2, 0x2f, 0xce, 0x73, 0x3a, 0x0b), new Uint8Array(10), // max_fee = 12_345_678_901_234 u128 LE
    bytes(2, 0, 0, 0), // admission_evidence
    bytes(1), program, fill(0x5d), // PublicAccountEvidence::Pda (65 bytes)
    bytes(0), bytes(KEY_TYPE_SCHNORR), fill(0xab), // PublicAccountEvidence::Key(SigningKeyState) (34 bytes)
  );
  assert.equal(toHex(encodeMessage(message)), toHex(expected));
  assert.equal(toHex(messageHash(message)), toHex(sha256(concat(MESSAGE_PREFIX, expected))));
});

test("transferMessage borsh layout (v0.4.0), byte for byte", () => {
  const [from, to] = [fill(0x11), fill(0x22)];
  const expectedInstruction = concat(bytes(0), to, bytes(0x39, 0x05), new Uint8Array(14)); // 1337 u128 LE
  assert.equal(toHex(transferInstruction(to, 1337n)), toHex(expectedInstruction));
  const message = transferMessage({ from, to, nonce: 3n, amount: 1337n, gasLimit: 1000n });
  const expected = concat(
    bytes(2, 0, 0, 0),
    from, fill(0), bytes(AUTHORITY_SIGNER), // sender balance carries Authority::Signer
    to, fill(0), bytes(AUTHORITY_NONE), // recipient balance carries Authority::None
    bytes(1, 0, 0, 0), from, // authorized_accounts
    bytes(0, 0, 0, 0), // promotions
    bytes(0, 0, 0, 0), // private_signers
    from, fill(0), // execution.root.to
    bytes(49, 0, 0, 0), expectedInstruction, // execution.root.message
    bytes(1, 0, 0, 0), from, bytes(3), new Uint8Array(15), // nonces
    bytes(1), from, // fee: Some
    bytes(0xe8, 0x03, 0, 0, 0, 0, 0, 0), // gas_limit = 1000
    new Uint8Array(8), // tip = 0
    bytes(0x00, 0xa2, 0x62), new Uint8Array(13), // max_fee = 6_464_000
    bytes(0, 0, 0, 0), // admission_evidence
  );
  assert.equal(expected.length, 416);
  assert.equal(toHex(encodeMessage(message)), toHex(expected));
  assert.equal(toHex(messageHash(message)), toHex(sha256(concat(MESSAGE_PREFIX, expected))));
});

test("one account can't carry two nonces, and only its signers can sign", () => {
  const to = nativeBalance(fill(1), AUTHORITY_SIGNER);
  const nonces = [{ accountId: fill(1), nonce: 0n }, { accountId: fill(1), nonce: 1n }];
  assert.throws(() => encodeMessage({ to, data: bytes(), actors: [], nonces }), /one nonce/);
  const message = { to, data: bytes(), actors: [], nonces: [{ accountId: fill(9), nonce: 0n }] };
  assert.throws(() => signPublicTransaction(message, [fill(1)]), /signing keys/);
});

// ---------- account.rs / signing_keys.rs / schnorr.rs / transaction.rs (v0.4.0) ----------

test("account.rs an_initial_key_gives_its_account_id", () => {
  const id = accountIdForInitialKey(signingKeyState(fill(7)));
  assert.equal(formatAccountId(id), "N2k68narJVM21VGmt8V6nM5o9E38ASXxAwvLZzt7XjE");
  // Also accepts 33-byte encoded SigningKeyState directly.
  assert.equal(formatAccountId(accountIdForInitialKey(concat(bytes(0), fill(7)))), "N2k68narJVM21VGmt8V6nM5o9E38ASXxAwvLZzt7XjE");
});

test("signing_keys.rs key_seed_known_answers and signing_key_known_answers", () => {
  const secret = fill(7);
  assert.equal(toHex(keySeed(secret, 0n)), "68256777fe99e612069c47cf884d5d3ca225b6caa113df93d847859c99cf5ab1");
  assert.equal(toHex(keySeed(secret, 1n)), "5994302e299bf9e059ce56b00574b1224032a9aa669243a8e1a8ba100eca0c43");
  assert.equal(toHex(signingKey(secret, 0n).publicKey), "c633dbb2522dd340094b469d8a4b2acb067731140e2e4880978269116aa05d8c");
  assert.equal(toHex(signingKey(secret, 1n).publicKey), "c0ed4b72427094127108dc89df6134b26db2ff3646308e57e064de64d24db8aa");
  assert.equal(initialKeyBytes(secret).length, 33);
  assert.equal(initialKeyBytes(secret)[0], KEY_TYPE_SCHNORR);
  assert.equal(
    toHex(initialKeyBytes(secret).subarray(1)),
    toHex(schnorrKeyCommit(signingKey(secret, 0n).publicKey)),
  );
});

test("schnorr.rs keygen_known_answers", () => {
  const cases = [
    [
      fill(0),
      "554da361d5c4a3f0b746dac9126015694d84a058fb4e12ab4e4eea32810f96fa",
      "40a52dbcf274dacbf01dc90e67c03023c144ac68434acb28d6a87997a50c17f8",
    ],
    [
      fill(1),
      "4741acf27ac4233ba083c7e28dbb9a6738134cd26a4f29dda68df53ce62db1f0",
      "8ca5e88efbcd3a65f175a58ea99d5ac84dce30286b373e6008e0fb3f442dcb85",
    ],
    [
      fill(2),
      "b3c82641c528e05d0f468cc8720dc8e602f9c8cbc692d7c842031ab647cf868a",
      "daa95922468f34c5c692e5b78f1309e56279d22849e7eb3d792578118b03040c",
    ],
  ];
  for (const [seed, expectedSk, expectedPk] of cases) {
    const { secretKey, publicKey } = schnorrKeygen(seed);
    assert.equal(toHex(secretKey), expectedSk);
    assert.equal(toHex(publicKey), expectedPk);
  }
});

test("transaction.rs signer_account_ids: secrets [1;32] and [2;32] (sorted)", () => {
  // Pinned in lez-v0.4.0/lee/state_machine/src/public_transaction/transaction.rs:187-201.
  const expected = [
    bytes(114, 83, 166, 143, 160, 156, 94, 122, 224, 105, 223, 2, 29, 64, 102, 35, 77, 69, 163, 112, 86, 191, 187, 179, 35, 250, 63, 42, 139, 197, 161, 45),
    bytes(181, 34, 253, 23, 228, 197, 158, 103, 215, 145, 57, 36, 172, 61, 169, 218, 159, 133, 254, 66, 40, 25, 75, 112, 207, 251, 110, 125, 246, 232, 196, 243),
  ].map(toHex);
  const actual = [publicAccountId(fill(1)), publicAccountId(fill(2))].map(toHex).sort();
  assert.deepEqual(actual, expected);
});

test("checkAccountSecret accepts any 32 bytes (including all-zero and all-0xff); randomAccountSecret is 32 bytes", () => {
  assert.equal(checkAccountSecret(fill(0)).length, 32);
  assert.equal(checkAccountSecret(fill(0xff)).length, 32);
  assert.equal(randomAccountSecret().length, 32);
  assert.throws(() => checkAccountSecret(new Uint8Array(31)));
});

test("base58 round trip and leading zeros", () => {
  for (const sample of [fill(0), fill(255), fill(1), publicAccountId(fill(1))]) {
    assert.equal(toHex(base58Decode(base58Encode(sample))), toHex(sample));
  }
  assert.equal(base58Encode(NATIVE_TOKEN_PROGRAM_ID), "11111111111111111111111111111111");
});

test("systemProgramId matches AccountId::from_system_program_name", () => {
  const paddedName = pad32("clock");
  const expectedSeed = sha256(concat(SYSTEM_PROGRAM_SEED_PREFIX, paddedName));
  const expectedId = sha256(concat(PDA_PREFIX, SYSTEM_UPGRADER_ACCOUNT_ID, expectedSeed));
  assert.equal(toHex(systemProgramId("clock")), toHex(expectedId));
  assert.throws(() => systemProgramId(""));
  assert.throws(() => systemProgramId("a".repeat(33)));
});

// ---------- keys_public.rs / key_tree/mod.rs: v0.4.0 public key tree ----------

const SEED = bytes(
  88, 189, 37, 237, 199, 125, 151, 226, 69, 153, 165, 113, 191, 69, 188, 221, 9, 34, 173, 134, 61, 109, 34, 103, 121, 39, 237, 14, 107, 194, 24, 194,
  191, 14, 237, 185, 12, 87, 22, 227, 38, 71, 17, 144, 251, 118, 217, 115, 33, 222, 201, 61, 203, 246, 121, 214, 6, 187, 148, 92, 44, 253, 210, 37,
);

test("keys_public.rs master_keys_generation, child_keys_generation (cci 2^31 + 13), and account_id_is_that_of_the_nonce_zero_key", () => {
  const root = publicRoot(SEED);
  assert.deepEqual(
    [...root.secret],
    [142, 140, 44, 81, 255, 159, 131, 163, 210, 67, 198, 176, 43, 243, 163, 35, 242, 200, 232, 99, 69, 240, 63, 16, 33, 104, 8, 152, 243, 153, 180, 169],
  );
  assert.equal(toHex(root.accountId), toHex(accountIdForInitialKey(initialKey(root.secret))));

  const child = publicChild(root, 2 ** 31 + 13);
  assert.deepEqual(
    [...child.secret],
    [112, 124, 240, 49, 175, 95, 86, 169, 176, 184, 184, 250, 10, 9, 128, 16, 199, 85, 135, 32, 151, 67, 135, 229, 2, 233, 137, 79, 222, 205, 137, 248],
  );
  assert.equal(toHex(child.accountId), toHex(publicAccountId(child.secret)));
});

test("key_tree/mod.rs simple_key_tree: [42;64] root account ID", () => {
  const root = publicRoot(fill(42, 64));
  assert.deepEqual(
    [...root.accountId],
    [110, 125, 111, 198, 184, 104, 121, 139, 210, 17, 104, 106, 188, 89, 89, 91, 14, 148, 135, 35, 69, 250, 36, 200, 169, 24, 22, 55, 243, 101, 66, 223],
  );
});

// ---------- signature/bip340_test_vectors.rs ----------

// The 13 vectors bip340_test_vectors.rs keeps (BIP-340 CSV 0–4 and 6–13; 5 and 14 have
// invalid public keys and are tested below, 15–18 are commented out there).
const BIP340 = [
  {"seckey": "0000000000000000000000000000000000000000000000000000000000000003", "pubkey": "F9308A019258C31049344F85F89D5229B531C845836F99B08601F113BCE036F9", "aux": "0000000000000000000000000000000000000000000000000000000000000000", "msg": "0000000000000000000000000000000000000000000000000000000000000000", "sig": "E907831F80848D1069A5371B402410364BDF1C5F8307B0084C55F1CE2DCA821525F66A4A85EA8B71E482A74F382D2CE5EBEEE8FDB2172F477DF4900D310536C0", "ok": true},
  {"seckey": "B7E151628AED2A6ABF7158809CF4F3C762E7160F38B4DA56A784D9045190CFEF", "pubkey": "DFF1D77F2A671C5F36183726DB2341BE58FEAE1DA2DECED843240F7B502BA659", "aux": "0000000000000000000000000000000000000000000000000000000000000001", "msg": "243F6A8885A308D313198A2E03707344A4093822299F31D0082EFA98EC4E6C89", "sig": "6896BD60EEAE296DB48A229FF71DFE071BDE413E6D43F917DC8DCF8C78DE33418906D11AC976ABCCB20B091292BFF4EA897EFCB639EA871CFA95F6DE339E4B0A", "ok": true},
  {"seckey": "C90FDAA22168C234C4C6628B80DC1CD129024E088A67CC74020BBEA63B14E5C9", "pubkey": "DD308AFEC5777E13121FA72B9CC1B7CC0139715309B086C960E18FD969774EB8", "aux": "C87AA53824B4D7AE2EB035A2B5BBBCCC080E76CDC6D1692C4B0B62D798E6D906", "msg": "7E2D58D8B3BCDF1ABADEC7829054F90DDA9805AAB56C77333024B9D0A508B75C", "sig": "5831AAEED7B44BB74E5EAB94BA9D4294C49BCF2A60728D8B4C200F50DD313C1BAB745879A5AD954A72C45A91C3A51D3C7ADEA98D82F8481E0E1E03674A6F3FB7", "ok": true},
  {"seckey": "0B432B2677937381AEF05BB02A66ECD012773062CF3FA2549E44F58ED2401710", "pubkey": "25D1DFF95105F5253C4022F628A996AD3A0D95FBF21D468A1B33F8C160D8F517", "aux": "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF", "msg": "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF", "sig": "7EB0509757E246F19449885651611CB965ECC1A187DD51B64FDA1EDC9637D5EC97582B9CB13DB3933705B32BA982AF5AF25FD78881EBB32771FC5922EFC66EA3", "ok": true},
  {"seckey": null, "pubkey": "D69C3509BB99E412E68B0FE8544E72837DFA30746D8BE2AA65975F29D22DC7B9", "aux": null, "msg": "4DF3C3F68FCC83B27E9D42C90431A72499F17875C81A599B566C9889B9696703", "sig": "00000000000000000000003B78CE563F89A0ED9414F5AA28AD0D96D6795F9C6376AFB1548AF603B3EB45C9F8207DEE1060CB71C04E80F593060B07D28308D7F4", "ok": true},
  {"seckey": null, "pubkey": "DFF1D77F2A671C5F36183726DB2341BE58FEAE1DA2DECED843240F7B502BA659", "aux": null, "msg": "243F6A8885A308D313198A2E03707344A4093822299F31D0082EFA98EC4E6C89", "sig": "FFF97BD5755EEEA420453A14355235D382F6472F8568A18B2F057A14602975563CC27944640AC607CD107AE10923D9EF7A73C643E166BE5EBEAFA34B1AC553E2", "ok": false},
  {"seckey": null, "pubkey": "DFF1D77F2A671C5F36183726DB2341BE58FEAE1DA2DECED843240F7B502BA659", "aux": null, "msg": "243F6A8885A308D313198A2E03707344A4093822299F31D0082EFA98EC4E6C89", "sig": "1FA62E331EDBC21C394792D2AB1100A7B432B013DF3F6FF4F99FCB33E0E1515F28890B3EDB6E7189B630448B515CE4F8622A954CFE545735AAEA5134FCCDB2BD", "ok": false},
  {"seckey": null, "pubkey": "DFF1D77F2A671C5F36183726DB2341BE58FEAE1DA2DECED843240F7B502BA659", "aux": null, "msg": "243F6A8885A308D313198A2E03707344A4093822299F31D0082EFA98EC4E6C89", "sig": "6CFF5C3BA86C69EA4B7376F31A9BCB4F74C1976089B2D9963DA2E5543E177769961764B3AA9B2FFCB6EF947B6887A226E8D7C93E00C5ED0C1834FF0D0C2E6DA6", "ok": false},
  {"seckey": null, "pubkey": "DFF1D77F2A671C5F36183726DB2341BE58FEAE1DA2DECED843240F7B502BA659", "aux": null, "msg": "243F6A8885A308D313198A2E03707344A4093822299F31D0082EFA98EC4E6C89", "sig": "0000000000000000000000000000000000000000000000000000000000000000123DDA8328AF9C23A94C1FEECFD123BA4FB73476F0D594DCB65C6425BD186051", "ok": false},
  {"seckey": null, "pubkey": "DFF1D77F2A671C5F36183726DB2341BE58FEAE1DA2DECED843240F7B502BA659", "aux": null, "msg": "243F6A8885A308D313198A2E03707344A4093822299F31D0082EFA98EC4E6C89", "sig": "00000000000000000000000000000000000000000000000000000000000000017615FBAF5AE28864013C099742DEADB4DBA87F11AC6754F93780D5A1837CF197", "ok": false},
  {"seckey": null, "pubkey": "DFF1D77F2A671C5F36183726DB2341BE58FEAE1DA2DECED843240F7B502BA659", "aux": null, "msg": "243F6A8885A308D313198A2E03707344A4093822299F31D0082EFA98EC4E6C89", "sig": "4A298DACAE57395A15D0795DDBFD1DCB564DA82B0F269BC70A74F8220429BA1D69E89B4C5564D00349106B8497785DD7D1D713A8AE82B32FA79D5F7FC407D39B", "ok": false},
  {"seckey": null, "pubkey": "DFF1D77F2A671C5F36183726DB2341BE58FEAE1DA2DECED843240F7B502BA659", "aux": null, "msg": "243F6A8885A308D313198A2E03707344A4093822299F31D0082EFA98EC4E6C89", "sig": "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC2F69E89B4C5564D00349106B8497785DD7D1D713A8AE82B32FA79D5F7FC407D39B", "ok": false},
  {"seckey": null, "pubkey": "DFF1D77F2A671C5F36183726DB2341BE58FEAE1DA2DECED843240F7B502BA659", "aux": null, "msg": "243F6A8885A308D313198A2E03707344A4093822299F31D0082EFA98EC4E6C89", "sig": "6CFF5C3BA86C69EA4B7376F31A9BCB4F74C1976089B2D9963DA2E5543E177769FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141", "ok": false},
];

test("signature/mod.rs signature_generation_from_bip340_test_vectors", () => {
  let checked = 0;
  BIP340.forEach((v, i) => {
    if (!v.seckey || !v.aux || !v.ok) return;
    const signature = signHash(fromHex(v.msg), fromHex(v.seckey), fromHex(v.aux));
    assert.equal(toHex(signature), v.sig.toLowerCase(), `vector ${i}`);
    checked += 1;
  });
  assert.equal(checked, 4);
});

test("signature/mod.rs signature_verification_from_bip340_test_vectors", () => {
  BIP340.forEach((v, i) => {
    assert.equal(verifyHash(fromHex(v.sig), fromHex(v.msg), fromHex(v.pubkey)), v.ok, `vector ${i}`);
  });
});

test("public_key.rs public_key_generation_from_bip340_test_vectors / try_new_valid_public_keys", () => {
  BIP340.forEach((v, i) => {
    assert.ok(isValidPublicKey(fromHex(v.pubkey)), `vector ${i} valid`);
    if (v.seckey) assert.equal(toHex(publicKeyOf(fromHex(v.seckey))), v.pubkey.toLowerCase(), `vector ${i}`);
  });
});

test("public_key.rs try_new_invalid_public_key_from_bip340_test_vectors_5 / _14", () => {
  const five = bytes(238, 253, 234, 76, 219, 103, 119, 80, 164, 32, 254, 232, 7, 234, 207, 33, 235, 152, 152, 174, 121, 185, 118, 135, 102, 228, 250, 160, 74, 45, 74, 52);
  const fourteen = bytes(Array(27).fill(255), 254, 255, 255, 252, 48);
  assert.equal(isValidPublicKey(five), false);
  assert.equal(isValidPublicKey(fourteen), false);
});

// ---------- witness_set.rs / transaction.rs (v0.4.0) ----------

test("witness_set.rs sign_orders_entries_by_account_and_signs_with_every_next_key", () => {
  const [secret1, secret2] = [fill(1), fill(2)];
  const [addr1, addr2] = [publicAccountId(secret1), publicAccountId(secret2)];
  const message = {
    to: signerActor(addr1, fill(0)),
    data: bytes(4, 0, 0, 0, 1, 2, 3, 4), // borsh(vec![1, 2, 3, 4])
    actors: [nativeBalance(addr1, AUTHORITY_SIGNER), nativeBalance(addr2, AUTHORITY_SIGNER)],
    nonces: [{ accountId: addr1, nonce: 1n }, { accountId: addr2, nonce: 2n }],
    fee: null,
  };
  const expected = [
    { accountId: addr1, current: keyState(secret1, 1n), next: keyState(secret1, 2n), pk: signingKey(secret1, 1n).publicKey },
    { accountId: addr2, current: keyState(secret2, 2n), next: keyState(secret2, 3n), pk: signingKey(secret2, 2n).publicKey },
  ].sort((a, b) => toHex(a.accountId).localeCompare(toHex(b.accountId)));

  // Pass signers in reverse order to verify canonical ascending AccountId ordering.
  const witnesses = witnessSetFor(message, [secret2, secret1], new Uint8Array(32));
  assert.equal(witnesses.length, 2);
  const keyedHash = messageWithKeys(message, expected.map((e) => e.next));

  witnesses.forEach((entry, i) => {
    const exp = expected[i];
    assert.equal(entry.authData.length, 96); // nonce > 0: sig(64) || pk(32)
    assert.equal(toHex(entry.authData.subarray(64, 96)), toHex(exp.pk));
    assert.equal(toHex(encodeSigningKeyState(entry.next)), toHex(encodeSigningKeyState(exp.next)));
    assert.ok(verifyWitnessEntry(exp.current, exp.accountId, keyedHash, entry.authData));
    assert.equal(verifyWitnessEntry(exp.next, exp.accountId, keyedHash, entry.authData), false);
  });

  // Hand-computed borsh(WitnessSet) = u32 count || (u32 len || auth_data || next[33])*
  const encoded = encodeWitnessSet(witnesses);
  const expectedBytes = concat(
    bytes(2, 0, 0, 0),
    bytes(96, 0, 0, 0), witnesses[0].authData, encodeSigningKeyState(expected[0].next),
    bytes(96, 0, 0, 0), witnesses[1].authData, encodeSigningKeyState(expected[1].next),
  );
  assert.equal(encoded.length, 4 + 2 * (4 + 96 + 33));
  assert.equal(toHex(encoded), toHex(expectedBytes));
});

test("a signed transaction at nonce 0 prefixes auth_data with the 33-byte initial SigningKeyState", () => {
  const secret = fill(1);
  const from = publicAccountId(secret);
  const message = transferMessage({ from, to: fill(9), nonce: 0n, amount: 1337n });
  const signed = signPublicTransaction(message, [secret], new Uint8Array(32));
  assert.equal(toHex(signed.txBytes), toHex(encodePublicTransaction(signed.messageBytes, signed.witnesses)));
  // PublicTransaction::hash = sha256(borsh(tx)), without the LeeTransaction tag.
  assert.equal(signed.hash, toHex(sha256(signed.txBytes)));
  // LeeTransaction::Public = tag 0 || tx; on the wire base64 (STANDARD) of that.
  assert.equal(toHex(signed.wire), toHex(encodeLeeTransaction(signed.txBytes)));
  assert.equal(signed.wire[0], 0);
  assert.equal(toHex(fromBase64(signed.base64)), toHex(signed.wire));

  assert.equal(signed.witnesses.length, 1);
  const [entry] = signed.witnesses;
  // Nonce 0: auth_data = initial_key(33) || sig(64) || pk_0(32) = 129 bytes; next = key_state(1) (33 bytes).
  assert.equal(entry.authData.length, 129);
  assert.equal(toHex(entry.authData.subarray(0, 33)), toHex(initialKeyBytes(secret)));
  assert.equal(toHex(encodeSigningKeyState(entry.next)), toHex(encodeSigningKeyState(keyState(secret, 1n))));
  assert.equal(toHex(signed.messageWithKeys), toHex(messageWithKeys(message, [entry.next])));
  assert.ok(verifyWitnessEntry(null, from, signed.messageWithKeys, entry.authData));
  assert.equal(verifyWitnessEntry(null, fill(9), signed.messageWithKeys, entry.authData), false);
  // Wire lengths at nonce 0: 416 (msg) + 4 (count) + 4 (vec len) + 129 (auth) + 33 (next) = 586; +1 tag = 587.
  assert.equal(signed.txBytes.length, 586);
  assert.equal(signed.wire.length, 587);
});

// ---------- the wallet's public transfer (send_public_transfer) ----------

test("transferMessage: the sender pays, and the wallet's fee cap", () => {
  const from = publicAccountId(fill(1));
  const to = fill(9);
  const message = transferMessage({ from, to, nonce: 3n, amount: 1000n, gasLimit: 1000n });
  assert.equal(toHex(message.to.accountId), toHex(from));
  assert.equal(toHex(message.to.programAccountId), toHex(NATIVE_TOKEN_PROGRAM_ID));
  assert.equal(message.to.authority, AUTHORITY_SIGNER);
  assert.deepEqual(message.nonces, [{ accountId: from, nonce: 3n }]);
  assert.equal(toHex(message.fee.payer), toHex(from));
  assert.equal(message.fee.maxFee, 6_464_000n);
  assert.equal(maxFeeFor(2_000_000n), 134_400_000n); // DEFAULT_MAX_FEE
  const signed = signTransfer({ secret: fill(1), to, nonce: 3n, amount: 1000n, gasLimit: 1000n });
  // v0.4.0 at nonce > 0:
  //   msg = (4 + 2*65) + (4 + 32) + 4 + 4 + 64 + (4 + 49) + (4 + 48) + (1 + 32 + 8 + 8 + 16) + 4 = 416
  //   witness_set = 4 + (4 + 96 + 33) = 137 -> txBytes = 553, wire = 554
  assert.equal(signed.txBytes.length, 553);
  assert.equal(signed.wire.length, 554);
  const quote = { base_fee_exec: 8, base_fee_stor: 8, next_base_fee_exec_ceiling: 9, next_base_fee_stor_ceiling: 9 };
  assert.equal(feeReserve({ gasLimit: 1000n, dataBytes: signed.wire.length, quote }), 1000n * 9n + 554n * 9n);
});

// ---------- the sequencer's JSON-RPC ----------

function stubRpc(results) {
  const real = globalThis.fetch;
  globalThis.fetch = async (_url, { body }) => {
    const { id, method, params } = JSON.parse(body);
    return Response.json({ jsonrpc: "2.0", id, result: results(method, params) });
  };
  return () => { globalThis.fetch = real; };
}

test("getAccountView selects an actor; a missing account is null", async () => {
  const seen = [];
  const restore = stubRpc((method, params) => { seen.push([method, params]); return null; });
  const id = fill(9);
  assert.equal(await getAccountView("http://rpc.test", nativeBalance(id)), null);
  assert.equal(await accountExists("http://rpc.test", id), false);
  restore();
  assert.deepEqual(seen[0], ["getAccountView", [{ account_id: base58Encode(id), program_account_id: base58Encode(fill(0)) }]]);
});

// ---------- the registry's instructions and addresses (core/src/lib.rs) ----------

const hexOf = (text) => fromHex(text);
const ID = Uint8Array.from({ length: 32 }, (_, i) => i);
const NOW = 1_700_000_000_000;

test("addresses and seeds match the Rust", () => {
  const { addresses, seeds } = VECTORS;
  const registry = fill(7);
  const alice = parseAccountId(addresses.alice);
  const holder = lns.ownerTag(ID, alice);
  const promised = lns.commitmentHash(ID, holder, fill(0x5a));
  assert.equal(toHex(holder), addresses.ownerTag);
  assert.equal(toHex(promised), addresses.commitmentHash);
  assert.equal(base58Encode(lns.recordAddress(registry, ID)), addresses.record);
  assert.equal(base58Encode(lns.pageAddress(registry, ID)), addresses.page);
  assert.equal(base58Encode(lns.depositAddress(registry, ID)), addresses.deposit);
  assert.equal(base58Encode(lns.settingsAddress(registry)), addresses.settings);
  assert.equal(base58Encode(lns.commitmentAddress(registry, promised)), addresses.commitment);
  assert.equal(toHex(lns.nameSeed(ID)), seeds.name);
  assert.equal(toHex(lns.pageSeed(ID)), seeds.page);
  assert.equal(toHex(lns.depositSeed(ID)), seeds.deposit);
  assert.equal(toHex(lns.settingsSeed()), seeds.settings);
  assert.equal(toHex(lns.commitmentSeed(promised)), seeds.commitment);
});

test("every v6/v8 instruction matches borsh(Message::Instruction(..))", () => {
  const { instructions: expected } = VECTORS;
  const check = (name, encoded, want = expected[name]) => assert.equal(toHex(encoded), want, name);
  const nowHex = "0068e5cf8b010000"; // 1_700_000_000_000 u64 LE

  // Changed in v7/v8 vs v6 vectors.json: Register (+priceTier), SetRecord (+SiteDirOp),
  // SetPage (+SiteDirOp), Transfer (+now), Claim (+priceTier).
  // (v8 suffixes hand-computed from notes/SUBNAMES.md §2.4 & notes/REGISTRY-04-CHANGES.md §2.3 — awaiting compiled v8 vector tool.)
  check("register", lns.registerInstruction({
    id: ID, sealed: bytes(0xee, 0xff), term: 1, fee: 100_000n, treasury: fill(2), now: NOW, secret: fill(0x5a), deposit: false,
    lengthProof: { kind: "Membership", index: 7, tier: 3, path: [fill(9)] }, priceTier: 2,
  }), `${expected.register}02`);
  check("registerDeposit", lns.registerInstruction({
    id: ID, sealed: bytes(0xee), term: 0, fee: 1_000_000n, now: NOW, secret: fill(0x5a), deposit: true,
    lengthProof: {
      kind: "NonMembership",
      left: { id: fill(1), tier: 3 }, leftPath: [fill(3)],
      right: { id: fill(2), tier: 1 }, rightPath: [fill(4), fill(5)],
      leftIndex: 6,
    }, priceTier: 2,
  }), `${expected.registerDeposit}02`);
  check("renew", lns.renewInstruction({ id: ID, term: 2, fee: 5n, treasury: fill(2), now: NOW }));
  check("setRecord (Keep)", lns.setRecordInstruction({ id: ID, sealed: bytes(0xee) }), `${expected.setRecord}00`);
  check("setRecord (Remove)", lns.setRecordInstruction({ id: ID, sealed: bytes(0xee), siteDir: lns.SITE_DIR_REMOVE }), `${expected.setRecord}02`);
  check(
    "setRecord (Upsert)",
    lns.setRecordInstruction({ id: ID, sealed: bytes(0xee), siteDir: lns.siteDirUpsert({ name: "alice", title: "Hi", page: 2, now: NOW }) }),
    `${expected.setRecord}0105000000616c6963650200000048690200${nowHex}`,
  );
  check("setPage (Keep)", lns.setPageInstruction({ id: ID, sealed: bytes() }), `${expected.setPage}00`);
  check("transfer", lns.transferInstruction({ id: ID, newOwner: fill(2), now: NOW }), `${expected.transfer}${nowHex}`);
  check("release", lns.releaseInstruction({ id: ID, refund: 1_000_000n }));
  check("setPrimary", lns.setPrimaryInstruction({ name: "jef" }));
  check("commit", lns.commitInstruction({ commitment: fill(0xc0), now: NOW }));
  check("claim", lns.claimInstruction({ id: ID, sealed: bytes(0xee), now: NOW, proof: [fill(1), fill(2)], priceTier: 1 }), `${expected.claim}01`);

  // CommitPublic (variant 10): exact borsh vector from repo/core/src/lib.rs.
  assert.equal(
    toHex(lns.commitPublicInstruction({ commitment: fill(0xc0), now: NOW })),
    `000a${"c0".repeat(32)}${nowHex}`,
  );
});

test("v7/v8 new instructions (0x07 Configure and 0x0b..0x1a) match exact Borsh vectors", () => {
  // Pinned v7 vectors from notes/SUBNAMES.md §2.4 (0x0b..0x13) and hand-computed v8 vectors
  // from notes/REGISTRY-04-CHANGES.md §2.3 (0x07, 0x14..0x1a — awaiting compiled v8 vector tool).
  const idHex = toHex(ID);
  const subId = fill(0x33);
  const subHex = "33".repeat(32);
  const nowHex = "0068e5cf8b010000";
  const u128OneHundredK = `a08601${"00".repeat(13)}`;

  // 0x07 Configure (v8: 3x4 prices + premiumStart + nftProgram)
  const prices = [[1n, 2n, 3n, 4n], [5n, 6n, 7n, 8n], [9n, 10n, 11n, 12n]];
  const pricesHex = Array.from({ length: 12 }, (_, i) => `${(i + 1).toString(16).padStart(2, "0")}${"00".repeat(15)}`).join("");
  assert.equal(
    toHex(lns.configureInstruction({
      admin: fill(1), treasury: fill(2), prices, mode: 0, deposits: [0n, 0n],
      claimsRoot: fill(0), claimsUntil: 0, claimsProgram: fill(7),
      premiumStart: 4_000_000_000n, nftProgram: fill(8),
    })),
    `0007${"01".repeat(32)}${"02".repeat(32)}${pricesHex}00${"00".repeat(72)}${"07".repeat(32)}00286bee${"00".repeat(12)}${"08".repeat(32)}`,
    "configure v8",
  );

  // 0x0b SetPrice
  assert.equal(
    toHex(lns.setPriceInstruction({ priceTier: 0, term: 3, price: 400_000_000n })),
    "000b00030084d717000000000000000000000000",
    "setPrice",
  );

  // 0x0c SetSubnameSale
  assert.equal(
    toHex(lns.setSubnameSaleInstruction({ id: ID, sale: { price: 100_000n, payout: fill(2) } })),
    `000c${idHex}01${u128OneHundredK}${"02".repeat(32)}`,
    "setSubnameSale on",
  );
  assert.equal(
    toHex(lns.setSubnameSaleInstruction({ id: ID, sale: null })),
    `000c${idHex}00`,
    "setSubnameSale off",
  );

  // 0x0d CreateSubname
  assert.equal(
    toHex(lns.createSubnameInstruction({ parentId: ID, subId, owner: fill(2), revocable: true, sealed: bytes(0xee), now: NOW })),
    `000d${idHex}${subHex}${"02".repeat(32)}0101000000ee${nowHex}`,
    "createSubname",
  );

  // 0x0e RevokeSubname
  assert.equal(
    toHex(lns.revokeSubnameInstruction({ parentId: ID, subId, now: NOW })),
    `000e${idHex}${subHex}${nowHex}`,
    "revokeSubname",
  );

  // 0x0f BuySubname
  assert.equal(
    toHex(lns.buySubnameInstruction({ parentId: ID, subId, sealed: bytes(0xee), price: 100_000n, payout: fill(2), now: NOW, secret: fill(0x5a) })),
    `000f${idHex}${subHex}01000000ee${u128OneHundredK}01${"02".repeat(32)}${nowHex}${"5a".repeat(32)}`,
    "buySubname",
  );

  // 0x10 SetSubnameRecord
  assert.equal(
    toHex(lns.setSubnameRecordInstruction({ parentId: ID, subId, sealed: bytes(0xee), now: null })),
    `0010${idHex}${subHex}01000000ee00`,
    "setSubnameRecord None",
  );
  assert.equal(
    toHex(lns.setSubnameRecordInstruction({ parentId: ID, subId, sealed: bytes(0xee), now: NOW })),
    `0010${idHex}${subHex}01000000ee01${nowHex}`,
    "setSubnameRecord Some",
  );

  // 0x11 SetSubnamePage
  assert.equal(
    toHex(lns.setSubnamePageInstruction({ parentId: ID, subId, sealed: bytes(), now: null })),
    `0011${idHex}${subHex}0000000000`,
    "setSubnamePage",
  );

  // 0x12 TransferSubname
  assert.equal(
    toHex(lns.transferSubnameInstruction({ parentId: ID, subId, newOwner: fill(2), now: NOW })),
    `0012${idHex}${subHex}${"02".repeat(32)}01${nowHex}`,
    "transferSubname",
  );

  // 0x13 ReleaseSubname
  assert.equal(
    toHex(lns.releaseSubnameInstruction({ parentId: ID, subId, now: null })),
    `0013${idHex}${subHex}00`,
    "releaseSubname",
  );

  // 0x14 SetPremium (awaiting compiled v8 vector tool)
  assert.equal(
    toHex(lns.setPremiumInstruction({ premiumStart: 4_000_000_000n })),
    `001400286bee${"00".repeat(12)}`,
    "setPremium",
  );

  // 0x15 List (public MarketBucket + OTC) (awaiting compiled v8 vector tool)
  assert.equal(
    toHex(lns.listInstruction({ id: ID, price: 100_000n, payTo: fill(2), expiresAt: NOW + 1000, now: NOW, displayName: "alice" })),
    `0015${idHex}${u128OneHundredK}${"02".repeat(32)}01e86be5cf8b010000${nowHex}0105000000616c696365`,
    "list public",
  );
  assert.equal(
    toHex(lns.listInstruction({ id: ID, price: 100_000n, payTo: fill(2), expiresAt: null, now: NOW, displayName: null })),
    `0015${idHex}${u128OneHundredK}${"02".repeat(32)}00${nowHex}00`,
    "list otc",
  );

  // 0x16 Cancel (awaiting compiled v8 vector tool)
  assert.equal(toHex(lns.cancelInstruction({ id: ID })), `0016${idHex}`, "cancel");

  // 0x17 Buy (awaiting compiled v8 vector tool)
  assert.equal(
    toHex(lns.buyInstruction({ id: ID, price: 100_000n, payTo: fill(2), treasury: fill(3), expectedNonce: 7, now: NOW })),
    `0017${idHex}${u128OneHundredK}${"02".repeat(32)}${"03".repeat(32)}0700000000000000${nowHex}`,
    "buy",
  );

  // 0x18..0x1a ShieldDeed / UnshieldDeed / RefreshShieldedDeed (awaiting compiled v8 vector tool)
  assert.equal(
    toHex(lns.shieldDeedInstruction({ id: ID, deedAccount: fill(4), now: NOW })),
    `0018${idHex}${"04".repeat(32)}${nowHex}`,
    "shieldDeed",
  );
  assert.equal(toHex(lns.unshieldDeedInstruction({ id: ID })), `0019${idHex}`, "unshieldDeed");
  assert.equal(toHex(lns.refreshShieldedDeedInstruction({ id: ID })), `001a${idHex}`, "refreshShieldedDeed");
});

test("saleSplit (2.5% ceil) and rereleasePremium (21-day daily halving) match exact v8 formulas", () => {
  assert.throws(() => lns.saleSplit(39n), /at least 40/);
  assert.deepEqual(lns.saleSplit(40n), { seller: 39n, fee: 1n });
  assert.deepEqual(lns.saleSplit(41n), { seller: 39n, fee: 2n });
  assert.deepEqual(lns.saleSplit(1_000_000n), { seller: 975_000n, fee: 25_000n });

  const start = 4_000_000_000n;
  assert.equal(lns.rereleasePremium(0n, 0n), 0n);
  assert.equal(lns.rereleasePremium(start, -1n), 0n);
  assert.equal(lns.rereleasePremium(start, 0n), start);
  assert.equal(lns.rereleasePremium(start, lns.DAY_MS), start >> 1n);
  assert.equal(lns.rereleasePremium(start, lns.DAY_MS / 2n), (start + (start >> 1n)) / 2n);
  assert.equal(lns.rereleasePremium(start, 20n * lns.DAY_MS), start >> 20n);
  assert.equal(lns.rereleasePremium(start, 21n * lns.DAY_MS), 0n);
});

test("v8 message builders declare required actors and PDA evidence (buy, list, buySubname)", () => {
  const program = fill(7);
  const nftProgram = fill(8);
  const signer = fill(1);
  const payTo = fill(2);
  const treasury = fill(3);

  const buyMsg = lns.buyMessage({
    program, signer, nonce: 1n, gasLimit: 300_000n,
    id: ID, price: 1_000_000n, payTo, treasury, expectedNonce: 0, now: NOW,
    inMarket: true, nftProgram, nftTokenId: fill(0xaa), sitePage: 0,
  });
  const hasActor = (msg, acc, prog) =>
    msg.actors.some((a) => toHex(a.accountId) === toHex(acc) && toHex(a.programAccountId) === toHex(prog));
  assert.ok(hasActor(buyMsg, payTo, NATIVE_TOKEN_PROGRAM_ID));
  assert.ok(hasActor(buyMsg, treasury, NATIVE_TOKEN_PROGRAM_ID));
  assert.ok(hasActor(buyMsg, lns.marketBucketAddress(program, lns.marketBucketIndex(ID)), program));
  assert.ok(hasActor(buyMsg, lns.nftTokenAddress(nftProgram, fill(0xaa)), nftProgram));
  assert.ok(hasActor(buyMsg, lns.siteDirHeaderAddress(program), program));
  assert.ok(hasActor(buyMsg, lns.siteBucketAddress(program, lns.siteBucketIndex(ID), 0), program));

  const otcList = lns.listMessage({
    program, signer, nonce: 2n, gasLimit: 150_000n, id: ID, price: 1_000_000n, payTo, now: NOW, displayName: null,
  });
  assert.equal(otcList.actors.length, 1); // no market bucket for OTC
  const pubList = lns.listMessage({
    program, signer, nonce: 2n, gasLimit: 150_000n, id: ID, price: 1_000_000n, payTo, now: NOW, displayName: "alice",
  });
  assert.equal(pubList.actors.length, 2); // record + market bucket

  const subBuy = lns.buySubnameMessage({
    program, signer, nonce: 3n, gasLimit: 300_000n,
    parentId: ID, subId: fill(0x33), sealed: bytes(0xee), price: 50_000n, payout: payTo, now: NOW, secret: fill(0x5a),
  });
  assert.ok(hasActor(subBuy, lns.subnameRecordAddress(program, ID, fill(0x33)), program));
  assert.ok(hasActor(subBuy, lns.subnamePageAddress(program, ID, fill(0x33)), program));
  assert.ok(hasActor(subBuy, payTo, NATIVE_TOKEN_PROGRAM_ID));
});

test("lns.decodeCommitment decodes the 10-byte v0.4.0 Commitment state and rejects 9-byte / invalid bool", () => {
  assert.equal(lns.COMMITMENT_BYTES, 10);
  const pub = concat(bytes(6), bytes(0x00, 0x68, 0xe5, 0xcf, 0x8b, 0x01, 0x00, 0x00), bytes(1));
  const priv = concat(bytes(6), bytes(0x00, 0x68, 0xe5, 0xcf, 0x8b, 0x01, 0x00, 0x00), bytes(0));
  assert.deepEqual(lns.decodeCommitment(pub), { version: 6, made: NOW, public: true });
  assert.deepEqual(lns.decodeCommitment(priv), { version: 6, made: NOW, public: false });
  assert.throws(() => lns.decodeCommitment(pub.subarray(0, 9)), /expected 10 bytes/);
  assert.throws(() => lns.decodeCommitment(concat(pub.subarray(0, 9), bytes(2))), /public flag/);
});

test("lns.commitPublicMessage encodes byte for byte with v0.4.0 Authority::Signer and nonces-before-fee", () => {
  const program = fill(7);
  const signer = fill(0); // strictly less than any SHA-256 PDA address
  const commitment = fill(0xc0);
  const commitAddr = lns.commitmentAddress(program, commitment);
  const commitSeed = lns.commitmentSeed(commitment);
  const msg = lns.commitPublicMessage({ program, signer, nonce: 0n, gasLimit: 50_000n, now: NOW, commitment });
  const expectedData = fromHex(`000a${"c0".repeat(32)}0068e5cf8b010000`);
  const expected = concat(
    bytes(2, 0, 0, 0),
    signer, program, bytes(AUTHORITY_SIGNER),
    commitAddr, program, bytes(AUTHORITY_NONE),
    bytes(1, 0, 0, 0), signer, // authorized_accounts
    bytes(0, 0, 0, 0), // promotions
    bytes(0, 0, 0, 0), // private_signers
    signer, program, // execution.root.to
    bytes(42, 0, 0, 0), expectedData, // execution.root.message
    bytes(1, 0, 0, 0), signer, new Uint8Array(16), // nonces (nonce 0)
    bytes(1), signer, // fee: Some
    bytes(0x50, 0xc3, 0, 0, 0, 0, 0, 0), // gas_limit = 50_000
    new Uint8Array(8), // tip = 0
    bytes(0x00, 0x7c, 0x92), new Uint8Array(13), // max_fee = 9_600_000
    bytes(1, 0, 0, 0), bytes(1), program, commitSeed, // admission_evidence: Pda
  );
  assert.equal(toHex(encodeMessage(msg)), toHex(expected));
});

test("lns.registerMessage and renewMessage grant Authority::Signer on signer's native balance when fee > 0", () => {
  const program = fill(7);
  const signer = fill(1);
  const treasury = fill(2);
  const paidReg = lns.registerMessage({
    program, signer, nonce: 1n, gasLimit: 250_000n, id: ID, secret: fill(0x5a),
    sealed: bytes(0xee), term: 0, fee: 1_000_000n, now: NOW, deposit: true,
    lengthProof: { kind: "Membership", index: 7, tier: 3, path: [fill(9)] },
  });
  assert.equal(paidReg.to.authority, AUTHORITY_SIGNER);
  const signerBal = paidReg.actors.find(
    (a) => toHex(a.accountId) === toHex(signer) && toHex(a.programAccountId) === toHex(NATIVE_TOKEN_PROGRAM_ID),
  );
  assert.equal(signerBal?.authority, AUTHORITY_SIGNER);
  const otherSigners = paidReg.actors.filter((a) => a.authority === AUTHORITY_SIGNER);
  assert.equal(otherSigners.length, 1); // only signer's native balance in actors (+ `to` adds (signer, program))

  const freeReg = lns.registerMessage({
    program, signer, nonce: 1n, gasLimit: 250_000n, id: ID, secret: fill(0x5a),
    sealed: bytes(0xee), term: 0, fee: 0n, now: NOW, deposit: false,
    lengthProof: { kind: "Membership", index: 7, tier: 3, path: [fill(9)] },
  });
  assert.equal(freeReg.actors.filter((a) => a.authority === AUTHORITY_SIGNER).length, 0);

  const paidRenew = lns.renewMessage({
    program, signer, nonce: 2n, gasLimit: 100_000n, id: ID, term: 1, fee: 5n, treasury, now: NOW,
  });
  const renewSignerBal = paidRenew.actors.find(
    (a) => toHex(a.accountId) === toHex(signer) && toHex(a.programAccountId) === toHex(NATIVE_TOKEN_PROGRAM_ID),
  );
  assert.equal(renewSignerBal?.authority, AUTHORITY_SIGNER);
});

test("a claim's leaf and proof check the way claim_holds does", () => {
  const { claims } = VECTORS;
  assert.equal(toHex(lns.claimLeaf(fill(7), fill(8))), claims.leafExample);
  const walk = (leafHex, proof) =>
    toHex(lns.claimRootFrom(hexOf(leafHex), proof.map(hexOf)));
  assert.equal(walk(claims.leaves[3], claims.proof3), claims.root);
  assert.equal(walk(claims.leaves[4], claims.proof4), claims.root);
  assert.notEqual(walk(claims.leaves[2], claims.proof3), claims.root);
});

// ---------- vectors.json: bytes the real LEZ v0.4.0 Rust types made (../../app-src/vectors gen) ----------

const ALICE_SECRET = fill(1);
const BOB_SECRET = fill(2);
const VALICE = () => parseAccountId(VECTORS.addresses.alice);
const VBOB = () => parseAccountId(VECTORS.addresses.bob);
const vTransfer = (nonce) => transferMessage({ from: VALICE(), to: VBOB(), nonce, amount: 1337n, gasLimit: 1000n });

/** Splits borsh(PublicTransaction) after `messageLength` bytes into its witness entries. */
function readWitnessSet(txBytes, messageLength) {
  const view = new DataView(txBytes.buffer, txBytes.byteOffset, txBytes.byteLength);
  let at = messageLength;
  const count = view.getUint32(at, true);
  at += 4;
  const entries = [];
  for (let i = 0; i < count; i += 1) {
    const length = view.getUint32(at, true);
    at += 4;
    const authData = txBytes.slice(at, at + length);
    at += length;
    const next = txBytes.slice(at, at + 33);
    at += 33;
    entries.push({ authData, next, signatureAt: at - 33 - length + (length === 129 ? 33 : 0) });
  }
  assert.equal(at, txBytes.length, "no trailing bytes");
  return entries;
}

/** The transaction with each entry's 64 signature bytes zeroed: all that may differ, as signing uses fresh aux randomness. */
function withoutSignatures(txBytes, entries) {
  const out = Uint8Array.from(txBytes);
  for (const { signatureAt } of entries) out.fill(0, signatureAt, signatureAt + 64);
  return out;
}

test("vectors: the pinned message and its keyed hashes", () => {
  const to = signerActor(fill(42), fill(0));
  const message = { to, data: bytes(0), actors: [to], nonces: [{ accountId: fill(42), nonce: 1n }], fee: null };
  assert.equal(toHex(encodeMessage(message)), VECTORS.pinned.bytes);
  assert.equal(toHex(messageHash(message)), VECTORS.pinned.hash);
  const key = (b) => signingKeyState(fill(b));
  assert.equal(toHex(messageWithKeys(message, [key(7)])), VECTORS.pinnedWithKeys.seven);
  assert.equal(toHex(messageWithKeys(message, [key(1), key(2), key(3)])), VECTORS.pinnedWithKeys.oneTwoThree);
});

test("vectors: sorting, merged duplicates, nonces before the fee, and both kinds of evidence", () => {
  const [high, low, program] = [fill(0xf0), fill(0x10), fill(0x77)];
  const message = {
    to: signerActor(high, program),
    data: bytes(1, 2, 3),
    actors: [actor(high, program), nativeBalance(high, AUTHORITY_SIGNER), actor(low, program), nativeBalance(low), actor(low, program)],
    nonces: [{ accountId: high, nonce: 9n }, { accountId: low, nonce: 2n ** 64n }],
    fee: feeDeclaration(high, 250_000n, 3n, 12_345_678_901_234n),
    evidence: [pdaEvidence(program, fill(0x5d)), keyEvidence(initialKey(ALICE_SECRET))],
  };
  assert.equal(toHex(encodeMessage(message)), VECTORS.sorted.bytes);
  assert.equal(toHex(messageHash(message)), VECTORS.sorted.hash);
});

test("vectors: account ids, initial keys, per-nonce key states and public keys", () => {
  for (const [name, byte] of [["one", 1], ["two", 2], ["seven", 7]]) {
    const expected = VECTORS.keys[name];
    const secret = fill(byte);
    assert.equal(formatAccountId(publicAccountId(secret)), expected.accountId, name);
    assert.equal(toHex(initialKeyBytes(secret)), expected.initialKey, name);
    expected.keyStates.forEach((state, nonce) => {
      assert.equal(toHex(encodeSigningKeyState(keyState(secret, BigInt(nonce)))), state, `${name} key state ${nonce}`);
    });
    expected.publicKeys.forEach((key, nonce) => {
      assert.equal(toHex(signingKey(secret, BigInt(nonce)).publicKey), key, `${name} public key ${nonce}`);
    });
  }
  assert.equal(VECTORS.addresses.alice, VECTORS.keys.one.accountId);
  assert.equal(VECTORS.addresses.bob, VECTORS.keys.two.accountId);
});

test("vectors: the wallet's native transfer, at nonce 3 and at nonce 0", () => {
  assert.equal(toHex(transferInstruction(VBOB(), 1337n)), VECTORS.transferInstruction);
  assert.equal(toHex(encodeMessage(vTransfer(3n))), VECTORS.walletTransfer.bytes);
  assert.equal(toHex(messageHash(vTransfer(3n))), VECTORS.walletTransfer.hash);
  assert.equal(toHex(encodeMessage(vTransfer(0n))), VECTORS.walletTransferFirst.bytes);
  assert.equal(toHex(messageHash(vTransfer(0n))), VECTORS.walletTransferFirst.hash);
});

test("vectors: transactions Rust signed verify here, and ours match them but for the signatures", () => {
  const twoSigners = {
    to: nativeBalance(VALICE(), AUTHORITY_SIGNER),
    data: transferInstruction(VBOB(), 1337n),
    actors: [nativeBalance(VALICE(), AUTHORITY_SIGNER), nativeBalance(VBOB(), AUTHORITY_SIGNER)],
    nonces: [{ accountId: VALICE(), nonce: 1n }, { accountId: VBOB(), nonce: 0n }],
    fee: feeDeclaration(VALICE(), 1000n, 0n, 6_464_000n),
  };
  const cases = [
    ["transfer", vTransfer(3n), [[ALICE_SECRET, 3n]]],
    ["transferFirst", vTransfer(0n), [[ALICE_SECRET, 0n]]],
    ["twoSigners", twoSigners, [[BOB_SECRET, 0n], [ALICE_SECRET, 1n]]],
  ];
  for (const [name, message, signers] of cases) {
    const rust = VECTORS.signed[name];
    const rustTx = fromHex(rust.tx);
    const messageBytes = encodeMessage(message);
    assert.equal(toHex(rustTx.subarray(0, messageBytes.length)), toHex(messageBytes), `${name}: message`);
    assert.equal(toHex(sha256(rustTx)), rust.hash, `${name}: hash`);
    const entries = readWitnessSet(rustTx, messageBytes.length);
    assert.deepEqual(entries.map((e) => e.authData.length), rust.authDataLengths, `${name}: auth data lengths`);
    // Entries in account order, each committing to its signer's key at the next nonce.
    const sorted = signers
      .map(([secret, nonce]) => ({ secret, nonce, id: publicAccountId(secret) }))
      .sort((a, b) => (toHex(a.id) < toHex(b.id) ? -1 : 1));
    const next = sorted.map(({ secret, nonce }) => toHex(encodeSigningKeyState(keyState(secret, nonce + 1n))));
    assert.deepEqual(entries.map((e) => toHex(e.next)), next, `${name}: next keys`);
    assert.deepEqual(rust.next, next, `${name}: next keys (Rust)`);
    const keyed = messageWithKeys(message, sorted.map(({ secret, nonce }) => keyState(secret, nonce + 1n)));
    assert.equal(toHex(keyed), rust.messageWithKeys, `${name}: message_with_keys`);
    sorted.forEach(({ secret, nonce, id }, i) => {
      const stored = nonce === 0n ? null : keyState(secret, nonce);
      assert.ok(verifyWitnessEntry(stored, id, keyed, entries[i].authData), `${name}: Rust signature ${i} verifies`);
    });
    // Ours: same bytes everywhere but the signatures.
    const ours = signPublicTransaction(message, signers.map(([secret]) => secret));
    assert.equal(ours.txBytes.length, rustTx.length, `${name}: length`);
    assert.equal(toHex(ours.messageWithKeys), rust.messageWithKeys, `${name}: our keyed hash`);
    const ourEntries = readWitnessSet(ours.txBytes, messageBytes.length);
    assert.equal(toHex(withoutSignatures(ours.txBytes, ourEntries)), toHex(withoutSignatures(rustTx, entries)), `${name}: bytes`);
  }
});

test("vectors: CommitPublic is variant 10, and the Commitment state decodes", () => {
  assert.equal(toHex(lns.commitPublicInstruction({ commitment: fill(0xc0), now: NOW })), VECTORS.instructions.commitPublic);
  const { made } = VECTORS.commitmentState;
  assert.deepEqual(lns.decodeCommitment(fromHex(VECTORS.commitmentState.public)), { version: 6, made, public: true });
  assert.deepEqual(lns.decodeCommitment(fromHex(VECTORS.commitmentState.private)), { version: 6, made, public: false });
});

test("vectors: the clock's ids", () => {
  assert.equal(formatAccountId(systemProgramId("clock")), VECTORS.clock.program);
  assert.equal(formatAccountId(new TextEncoder().encode("/LEZ/ClockProgramAccount/0000000")), VECTORS.clock.account);
  assert.equal(formatAccountId(systemProgramId("signature")), VECTORS.clock.signatureProgram);
});
