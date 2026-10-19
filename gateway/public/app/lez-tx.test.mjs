// Pinned vectors from the LEZ Rust sources (dev 4fba494), reproduced byte for byte by
// lez-tx.js (the bundle the browser loads). Run: node --test gateway/public/app/lez-tx.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";

import {
  ACCOUNT_ID_PREFIX,
  MESSAGE_PREFIX,
  NATIVE_TOKEN_PROGRAM_ID,
  accountIdOf,
  base58Decode,
  base58Encode,
  concat,
  encodeLeeTransaction,
  encodeMessage,
  encodePublicTransaction,
  encodeWitnessSet,
  feeDeclaration,
  feeReserve,
  fromBase64,
  fromHex,
  isValidPublicKey,
  maxFeeFor,
  messageHash,
  nativeBalance,
  publicChild,
  publicKeyOf,
  publicRoot,
  shardSelector,
  signHash,
  signPublicTransaction,
  signTransfer,
  toHex,
  transferInstruction,
  transferMessage,
  tweak,
  verifyHash,
  witnessSetFor,
} from "./lez-tx.js";
import { createHash } from "node:crypto";

// An independent SHA-256 (Node's), not the one bundled in lez-tx.js.
const sha256 = (data) => new Uint8Array(createHash("sha256").update(data).digest());

const bytes = (...parts) => Uint8Array.from(parts.flat());
const fill = (value, length = 32) => new Uint8Array(length).fill(value);

// ---------- message.rs: hash_public_pinned_* ----------

const PROGRAM_ACCOUNT_ID_BYTES = bytes(1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0);
const POSITIONS_BYTES = bytes([1, 0, 0, 0], Array(32).fill(42), Array(32).fill(43));
const BALANCE_POSITIONS_BYTES = bytes([1, 0, 0, 0], Array(32).fill(42), Array(32).fill(0));
const NONCES_BYTES = bytes(1, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0);

const pinnedMessage = (shardSelectors, instructionData, fee) => ({
  programAccountId: PROGRAM_ACCOUNT_ID_BYTES,
  shardSelectors,
  nonces: [5n],
  instructionData: Uint8Array.from(instructionData),
  fee,
});
const namedShardSelector = () => shardSelector(fill(42), fill(43));

function assertHashPinned(message, shardSelectorsBytes, instructionBytes, feeBytes) {
  const expectedBorsh = concat(PROGRAM_ACCOUNT_ID_BYTES, shardSelectorsBytes, NONCES_BYTES, bytes(instructionBytes), bytes(feeBytes));
  assert.equal(toHex(encodeMessage(message)), toHex(expectedBorsh), "borsh order");
  const expectedHash = sha256(concat(MESSAGE_PREFIX, expectedBorsh));
  assert.equal(toHex(messageHash(message)), toHex(expectedHash), "prefixed hash");
  return toHex(expectedHash);
}

test("message PREFIX is b\"/LEE/v0.3/Message/Public/\" + 7 zero bytes", () => {
  assert.equal(toHex(MESSAGE_PREFIX), toHex(concat(new TextEncoder().encode("/LEE/v0.3/Message/Public/"), new Uint8Array(7))));
  assert.equal(MESSAGE_PREFIX.length, 32);
});

test("message.rs hash_public_pinned_exempt", () => {
  assertHashPinned(pinnedMessage([namedShardSelector()], [], null), POSITIONS_BYTES, [0, 0, 0, 0], [0]);
});

test("message.rs hash_public_pinned_balance_shard_selector", () => {
  assertHashPinned(pinnedMessage([nativeBalance(fill(42))], [], null), BALANCE_POSITIONS_BYTES, [0, 0, 0, 0], [0]);
});

test("message.rs hash_public_pinned_nonempty_instruction", () => {
  assertHashPinned(pinnedMessage([namedShardSelector()], [7, 8, 9], null), POSITIONS_BYTES, [3, 0, 0, 0, 7, 8, 9], [0]);
});

test("message.rs hash_public_pinned_charged", () => {
  const feeBytes = [1, Array(32).fill(7), [9, 0, 0, 0, 0, 0, 0, 0], [3, 0, 0, 0, 0, 0, 0, 0], [100, ...Array(15).fill(0)]].flat();
  assertHashPinned(
    pinnedMessage([namedShardSelector()], [], feeDeclaration(fill(7), 9, 3, 100)),
    POSITIONS_BYTES,
    [0, 0, 0, 0],
    feeBytes,
  );
});

// ---------- public_key.rs / transaction.rs: account IDs ----------

test("account ID PREFIX is b\"/LEE/v0.3/AccountId/Public/\" + 5 zero bytes", () => {
  assert.equal(toHex(ACCOUNT_ID_PREFIX), toHex(concat(new TextEncoder().encode("/LEE/v0.3/AccountId/Public/"), new Uint8Array(5))));
});

test("transaction.rs signer_account_ids: keys [1;32] and [2;32]", () => {
  const expected1 = bytes(148, 179, 206, 253, 199, 51, 82, 86, 232, 2, 152, 122, 80, 243, 54, 207, 237, 112, 83, 153, 44, 59, 204, 49, 128, 84, 160, 227, 216, 149, 97, 102);
  const expected2 = bytes(30, 145, 107, 3, 207, 73, 192, 230, 160, 63, 238, 207, 18, 69, 54, 216, 103, 244, 92, 94, 124, 248, 42, 16, 141, 19, 119, 18, 14, 226, 140, 204);
  assert.equal(toHex(accountIdOf(publicKeyOf(fill(1)))), toHex(expected1));
  assert.equal(toHex(accountIdOf(publicKeyOf(fill(2)))), toHex(expected2));
});

test("base58 round trip and leading zeros", () => {
  for (const sample of [fill(0), fill(255), fill(1), accountIdOf(publicKeyOf(fill(1)))]) {
    assert.equal(toHex(base58Decode(base58Encode(sample))), toHex(sample));
  }
  assert.equal(base58Encode(NATIVE_TOKEN_PROGRAM_ID), "11111111111111111111111111111111");
});

// ---------- private_key.rs: tweak ----------

test("private_key.rs tweak_deterministic", () => {
  const expected = bytes(242, 210, 33, 19, 65, 108, 136, 176, 179, 128, 110, 210, 107, 193, 168, 112, 206, 171, 86, 238, 131, 10, 39, 36, 44, 39, 246, 20, 46, 193, 204, 66);
  assert.equal(toHex(tweak(fill(1))), toHex(expected));
});

test("private_key.rs tweak_rejects_zero_key / tweak_rejects_out_of_range_key", () => {
  assert.throws(() => tweak(fill(0)));
  assert.throws(() => tweak(fill(0xff)));
});

// ---------- keys_public.rs: the wallet's public key tree ----------

const SEED = bytes(
  88, 189, 37, 237, 199, 125, 151, 226, 69, 153, 165, 113, 191, 69, 188, 221, 9, 34, 173, 134, 61, 109, 34, 103, 121, 39, 237, 14, 107, 194, 24, 194,
  191, 14, 237, 185, 12, 87, 22, 227, 38, 71, 17, 144, 251, 118, 217, 115, 33, 222, 201, 61, 203, 246, 121, 214, 6, 187, 148, 92, 44, 253, 210, 37,
);

test("keys_public.rs master_keys_generation", () => {
  const keys = publicRoot(SEED);
  assert.deepEqual([...keys.cc], [184, 94, 197, 114, 84, 79, 170, 62, 107, 107, 141, 196, 11, 255, 15, 165, 7, 40, 93, 211, 244, 153, 12, 70, 10, 174, 141, 69, 117, 167, 165, 81]);
  assert.deepEqual([...keys.sk], [142, 140, 44, 81, 255, 159, 131, 163, 210, 67, 198, 176, 43, 243, 163, 35, 242, 200, 232, 99, 69, 240, 63, 16, 33, 104, 8, 152, 243, 153, 180, 169]);
  assert.deepEqual([...keys.ssk], [241, 47, 167, 208, 182, 77, 106, 158, 182, 41, 17, 3, 91, 229, 165, 35, 90, 33, 145, 202, 246, 65, 127, 65, 124, 240, 165, 152, 127, 50, 60, 198]);
  assert.deepEqual([...keys.pk], [43, 138, 92, 79, 223, 49, 90, 162, 205, 76, 143, 151, 96, 77, 10, 85, 179, 208, 244, 71, 251, 191, 237, 226, 120, 247, 194, 57, 117, 180, 96, 65]);
});

test("keys_public.rs child_keys_generation (cci 2^31 + 13)", () => {
  const keys = publicChild(publicRoot(SEED), 2 ** 31 + 13);
  assert.deepEqual([...keys.cc], [184, 162, 65, 125, 129, 202, 96, 126, 157, 15, 189, 122, 22, 152, 31, 107, 244, 188, 215, 30, 70, 205, 164, 142, 6, 152, 106, 147, 160, 1, 168, 168]);
  assert.deepEqual([...keys.sk], [222, 78, 224, 138, 167, 32, 235, 208, 192, 129, 121, 150, 204, 149, 151, 33, 82, 109, 238, 245, 20, 106, 70, 126, 120, 66, 165, 169, 241, 242, 224, 10]);
  assert.deepEqual([...keys.ssk], [103, 101, 18, 63, 86, 198, 110, 120, 163, 160, 181, 249, 184, 163, 7, 38, 132, 223, 72, 208, 74, 223, 16, 110, 60, 227, 167, 192, 89, 28, 14, 222]);
  assert.deepEqual([...keys.pk], [107, 153, 105, 58, 6, 157, 131, 253, 141, 130, 168, 182, 82, 2, 99, 26, 211, 22, 55, 203, 23, 34, 236, 147, 86, 156, 194, 114, 89, 77, 219, 173]);
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

// ---------- witness_set.rs / transaction.rs ----------

test("witness_set.rs for_message_constructor", () => {
  const [key1, key2] = [fill(1), fill(2)];
  const [pubkey1, pubkey2] = [publicKeyOf(key1), publicKeyOf(key2)];
  // Message::try_new(from_builtin_program([0; 8]), …, instruction: vec![1, 2, 3, 4]): the
  // instruction is borsh(Vec<u8>), so instruction_data = [4, 0, 0, 0, 1, 2, 3, 4].
  const message = {
    programAccountId: new Uint8Array(32),
    shardSelectors: [nativeBalance(accountIdOf(pubkey1)), nativeBalance(accountIdOf(pubkey2))],
    nonces: [1n, 2n],
    instructionData: bytes(4, 0, 0, 0, 1, 2, 3, 4),
    fee: null,
  };
  const witnesses = witnessSetFor(message, [key1, key2]);
  assert.equal(witnesses.length, 2);
  const hash = messageHash(message);
  witnesses.forEach(({ signature, publicKey }, i) => {
    assert.equal(toHex(publicKey), toHex([pubkey1, pubkey2][i]));
    assert.ok(verifyHash(signature, hash, publicKey));
  });
  // borsh(WitnessSet) = u32 count ‖ (sig[64] ‖ pk[32])*
  const encoded = encodeWitnessSet(witnesses);
  assert.equal(encoded.length, 4 + 2 * 96);
  assert.deepEqual([...encoded.subarray(0, 4)], [2, 0, 0, 0]);
  assert.equal(toHex(encoded.subarray(4 + 64, 4 + 96)), toHex(pubkey1));
});

test("transaction.rs transaction_for_tests: Transfer { amount: 1337 } layout, hash and wire", () => {
  const [key1, key2] = [fill(1), fill(2)];
  const [addr1, addr2] = [accountIdOf(publicKeyOf(key1)), accountIdOf(publicKeyOf(key2))];
  // native_token::Instruction::Transfer { amount } = variant 0 ‖ u128 LE
  assert.deepEqual([...transferInstruction(1337n)], [0, 57, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
  const message = {
    programAccountId: NATIVE_TOKEN_PROGRAM_ID,
    shardSelectors: [nativeBalance(addr1), nativeBalance(addr2)],
    nonces: [0n, 0n],
    instructionData: transferInstruction(1337n),
    fee: null,
  };
  const aux = new Uint8Array(32);
  const signed = signPublicTransaction(message, [key1, key2], aux);
  const expectedMessage = concat(
    new Uint8Array(32),
    bytes(2, 0, 0, 0), addr1, new Uint8Array(32), addr2, new Uint8Array(32),
    bytes(2, 0, 0, 0), new Uint8Array(16), new Uint8Array(16),
    bytes(17, 0, 0, 0), transferInstruction(1337n),
    bytes(0),
  );
  assert.equal(toHex(signed.messageBytes), toHex(expectedMessage));
  assert.equal(toHex(signed.txBytes), toHex(encodePublicTransaction(expectedMessage, signed.witnesses)));
  // PublicTransaction::hash = sha256(borsh(tx)), without the LeeTransaction tag.
  assert.equal(signed.hash, toHex(sha256(signed.txBytes)));
  // LeeTransaction::Public = tag 0 ‖ tx; on the wire base64 (STANDARD) of that.
  assert.equal(toHex(signed.wire), toHex(encodeLeeTransaction(signed.txBytes)));
  assert.equal(signed.wire[0], 0);
  assert.equal(toHex(fromBase64(signed.base64)), toHex(signed.wire));
  for (const { signature, publicKey } of signed.witnesses) assert.ok(verifyHash(signature, signed.messageHash, publicKey));
});

// ---------- the wallet's public transfer (send_public_transfer) ----------

test("transferMessage matches send_pub_tx_with_pre_check for a native transfer", () => {
  const from = accountIdOf(publicKeyOf(fill(1)));
  const to = fill(9);
  const message = transferMessage({ from, to, nonce: 3n, amount: 1000n, gasLimit: 1000n });
  assert.equal(toHex(message.programAccountId), toHex(NATIVE_TOKEN_PROGRAM_ID));
  assert.deepEqual(message.nonces, [3n]);
  assert.equal(toHex(message.fee.payer), toHex(from));
  assert.equal(message.fee.maxFee, 6_464_000n);
  assert.equal(maxFeeFor(2_000_000n), 134_400_000n); // DEFAULT_MAX_FEE
  const signed = signTransfer({ privateKey: fill(1), to, nonce: 3n, amount: 1000n, gasLimit: 1000n });
  // 32 + (4 + 2·64) + (4 + 16) + (4 + 17) + (1 + 32 + 8 + 8 + 16) + (4 + 96) = 370, plus the tag
  assert.equal(signed.txBytes.length, 370);
  assert.equal(signed.wire.length, 371);
  const quote = { base_fee_exec: 8, base_fee_stor: 8, next_base_fee_exec_ceiling: 9, next_base_fee_stor_ceiling: 9 };
  assert.equal(feeReserve({ gasLimit: 1000n, dataBytes: signed.wire.length, quote }), 1000n * 9n + 371n * 9n);
});
