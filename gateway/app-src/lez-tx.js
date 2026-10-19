// LEZ public transactions in JavaScript (browser and Node): keys, account IDs,
// the borsh message, its hash, BIP-340 signatures, the witness set and the
// transaction bytes `sendTransaction` takes, byte for byte as Rust makes them.
//
// This is the source; public/app/lez-tx.js is the bundle (crypto included), made with
//   cd gateway/app-src && npx rolldown lez-tx.js --format esm --file ../public/app/lez-tx.js
// (rolldown is gateway's; @noble/curves and @noble/hashes are pinned in app-src/package.json.)
// Tests: node --test gateway/public/app/lez-tx.test.mjs
//
// Sources this follows (LEZ dev 4fba494):
//   lee/state_machine/src/signature/{mod,private_key,public_key}.rs   BIP-340, tweak, account ID
//   lee/state_machine/src/public_transaction/{message,witness_set,transaction}.rs
//   lee/state_machine/core/src/{account.rs,native_token.rs}   Nonce, ProgramShardSelector, Transfer
//   lee/state_machine/src/fees.rs, lez/programs/fee/core/src/assess.rs   FeeDeclaration, reserve
//   lee/key_protocol/src/key_management/key_tree/keys_public.rs   seed -> public account keys
//   lez/common/src/{transaction.rs,borsh_base64.rs}   LeeTransaction on the wire
//   lez/sequencer/service/rpc/src/lib.rs   the JSON-RPC methods
//   lez/wallet/src/lib.rs   send_pub_tx_with_pre_check, max_fee_for

import { schnorr, secp256k1 } from "@noble/curves/secp256k1.js";
import { hmac } from "@noble/hashes/hmac.js";
import { sha256, sha512 } from "@noble/hashes/sha2.js";

const utf8 = new TextEncoder();
const N = 0xfffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141n;
const BASE58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

function pad32(text) {
  const bytes = new Uint8Array(32);
  bytes.set(utf8.encode(text));
  return bytes;
}

/** `b"/LEE/v0.3/Message/Public/\x00…"` (message.rs PREFIX). */
export const MESSAGE_PREFIX = pad32("/LEE/v0.3/Message/Public/");
/** `b"/LEE/v0.3/AccountId/Public/\x00…"` (public_key.rs PUBLIC_ACCOUNT_ID_PREFIX). */
export const ACCOUNT_ID_PREFIX = pad32("/LEE/v0.3/AccountId/Public/");
const MASTER_PUBLIC_KEY = utf8.encode("/LEE-Keys/v1/Master/Public");

/** `NATIVE_TOKEN_PROGRAM_ID`: 32 zero bytes, also the shard holding an account's balance. */
export const NATIVE_TOKEN_PROGRAM_ID = new Uint8Array(32);
/** `LeeTransaction` borsh variant tags. */
export const LEE_TX_PUBLIC = 0;

/** The wallet's assumed worst base fee and data size (lez/wallet/src/lib.rs). */
const ASSUMED_BASE_FEE = 64n;
const ASSUMED_DATA_BYTES = 100_000n;
/** The gas limit the names CLI gives a public native transfer: the native token meters no cycles. */
export const NATIVE_TRANSFER_GAS_LIMIT = 1_000n;

// ---------- bytes ----------

export function concat(...parts) {
  const out = new Uint8Array(parts.reduce((total, part) => total + part.length, 0));
  let offset = 0;
  for (const part of parts) {
    out.set(part, offset);
    offset += part.length;
  }
  return out;
}

export const toHex = (bytes) => Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");

export function fromHex(text) {
  if (!/^([0-9a-f]{2})*$/i.test(text)) throw new Error("invalid hex");
  return Uint8Array.from(text.match(/../g) ?? [], (pair) => parseInt(pair, 16));
}

export const equalBytes = (a, b) => a.length === b.length && a.every((byte, i) => byte === b[i]);

export function base58Encode(bytes) {
  let value = 0n;
  for (const byte of bytes) value = (value << 8n) | BigInt(byte);
  let text = "";
  while (value > 0n) {
    text = BASE58[Number(value % 58n)] + text;
    value /= 58n;
  }
  for (const byte of bytes) {
    if (byte !== 0) break;
    text = "1" + text;
  }
  return text;
}

export function base58Decode(text) {
  let value = 0n;
  for (const char of text) {
    const digit = BASE58.indexOf(char);
    if (digit < 0) throw new Error(`invalid base58 character: ${char}`);
    value = value * 58n + BigInt(digit);
  }
  const bytes = [];
  while (value > 0n) {
    bytes.unshift(Number(value & 0xffn));
    value >>= 8n;
  }
  for (const char of text) {
    if (char !== "1") break;
    bytes.unshift(0);
  }
  return Uint8Array.from(bytes);
}

/** A 32-byte account ID from base58, with or without a `Public/` prefix (`AccountId::from_str`). */
export function parseAccountId(text) {
  const bytes = base58Decode(String(text).trim().replace(/^Public\//, ""));
  if (bytes.length !== 32) throw new Error(`invalid account ID length: expected 32 bytes, got ${bytes.length}`);
  return bytes;
}

/** `AccountId` Display: base58 of the 32 bytes. */
export const formatAccountId = (bytes) => base58Encode(bytes);

export function toBase64(bytes) {
  let text = "";
  for (const byte of bytes) text += String.fromCharCode(byte);
  return btoa(text);
}

export function fromBase64(text) {
  return Uint8Array.from(atob(text), (char) => char.charCodeAt(0));
}

// ---------- keys and account IDs ----------

const bytesToNumber = (bytes) => BigInt(`0x${toHex(bytes) || "0"}`);
const numberToBytes32 = (n) => fromHex(n.toString(16).padStart(64, "0"));

/** `PrivateKey::try_new`: a valid secp256k1 secret key (1..n-1, big-endian). */
export function checkPrivateKey(bytes) {
  if (!(bytes instanceof Uint8Array) || bytes.length !== 32) throw new Error("a private key is 32 bytes");
  const n = bytesToNumber(bytes);
  if (n === 0n || n >= N) throw new Error("invalid private key");
  return bytes;
}

/** `PrivateKey::new_os_random`: a fresh signing key. */
export const randomPrivateKey = () => schnorr.utils.randomSecretKey();

/** `PublicKey::new_from_private_key`: the x-only (BIP-340) public key, 32 bytes. */
export const publicKeyOf = (privateKey) => schnorr.getPublicKey(checkPrivateKey(privateKey));

/** `PublicKey::try_new`: whether 32 bytes are a valid x-only key (on the curve, below p). */
export function isValidPublicKey(bytes) {
  if (!(bytes instanceof Uint8Array) || bytes.length !== 32) return false;
  try {
    schnorr.utils.lift_x(bytesToNumber(bytes));
    return true;
  } catch {
    return false;
  }
}

/** `AccountId::from(&PublicKey)`: sha256(ACCOUNT_ID_PREFIX ‖ x-only key). */
export function accountIdOf(publicKey) {
  if (publicKey.length !== 32) throw new Error("a public key is 32 bytes (x-only)");
  return sha256(concat(ACCOUNT_ID_PREFIX, publicKey));
}

/** The public account a signing key controls. */
export const accountIdOfPrivateKey = (privateKey) => accountIdOf(publicKeyOf(privateKey));

/** Like k256 `Scalar::from_repr`: a big-endian scalar, refused when it isn't below n. */
function scalar(bytes) {
  const n = bytesToNumber(bytes);
  if (n >= N) throw new Error("scalar out of range");
  return n;
}

/**
 * `PrivateKey::tweak`: sk + sha256(compressed public key of sk) mod n. The wallet
 * signs with, and names the account after, this tweaked key (`ssk`).
 */
export function tweak(sk) {
  checkPrivateKey(sk);
  const hashed = sha256(secp256k1.getPublicKey(sk, true));
  return checkPrivateKey(numberToBytes32((bytesToNumber(sk) + scalar(hashed)) % N));
}

function keyNode(sk, cc, cci) {
  const ssk = tweak(sk);
  const pk = publicKeyOf(ssk);
  return { sk, ssk, pk, cc, cci, accountId: accountIdOf(pk) };
}

/** `ChildKeysPublic::root`: HMAC-SHA512(key "/LEE-Keys/v1/Master/Public", seed). */
export function publicRoot(seed) {
  if (seed.length !== 64) throw new Error("seed must be 64 bytes");
  const hash = hmac(sha512, MASTER_PUBLIC_KEY, seed);
  return keyNode(checkPrivateKey(hash.slice(0, 32)), hash.slice(32), null);
}

/** `ChildKeysPublic::nth_child`: HMAC-SHA512(cc, 0 ‖ sk ‖ cci BE), sk' = left + sk mod n. */
export function publicChild(parent, cci) {
  const index = new Uint8Array(4);
  new DataView(index.buffer).setUint32(0, cci, false);
  const hash = hmac(sha512, parent.cc, concat(Uint8Array.of(0), parent.sk, index));
  const sk = checkPrivateKey(numberToBytes32((scalar(hash.subarray(0, 32)) + scalar(parent.sk)) % N));
  return keyNode(sk, hash.slice(32), cci);
}

// ---------- BIP-340 ----------

/**
 * `Signature::new_with_aux_random`: k256 `sign_prehash_with_aux_rand`, i.e. BIP-340
 * over the 32-byte hash as the message. `aux` defaults to 32 random bytes (`Signature::new`).
 */
export function signHash(hash32, privateKey, aux = null) {
  if (hash32.length !== 32) throw new Error("expected a 32-byte prehash");
  const auxRand = aux ?? crypto.getRandomValues(new Uint8Array(32));
  return schnorr.sign(hash32, checkPrivateKey(privateKey), auxRand);
}

/** `Signature::is_valid_for`: false for an invalid key or signature, never throws. */
export function verifyHash(signature, hash32, publicKey) {
  if (!isValidPublicKey(publicKey) || signature.length !== 64) return false;
  try {
    return schnorr.verify(signature, hash32, publicKey);
  } catch {
    return false;
  }
}

// ---------- borsh ----------

export class Writer {
  constructor() {
    this.parts = [];
  }
  bytes(bytes) {
    this.parts.push(Uint8Array.from(bytes));
    return this;
  }
  u8(n) {
    return this.uint(n, 1);
  }
  uint(n, size) {
    let value = BigInt(n);
    if (value < 0n || value >= 1n << BigInt(size * 8)) throw new Error(`number out of range for u${size * 8}`);
    const out = new Uint8Array(size);
    for (let i = 0; i < size; i += 1) {
      out[i] = Number(value & 0xffn);
      value >>= 8n;
    }
    return this.bytes(out);
  }
  u32(n) {
    return this.uint(n, 4);
  }
  u64(n) {
    return this.uint(n, 8);
  }
  u128(n) {
    return this.uint(n, 16);
  }
  fixed(bytes, length) {
    if (bytes.length !== length) throw new Error(`expected ${length} bytes, got ${bytes.length}`);
    return this.bytes(bytes);
  }
  /** `Vec<u8>`: u32 LE length, then the bytes. */
  vec(bytes) {
    return this.u32(bytes.length).bytes(bytes);
  }
  option(value, write) {
    if (value === null || value === undefined) return this.u8(0);
    this.u8(1);
    write(this, value);
    return this;
  }
  done() {
    return concat(...this.parts);
  }
}

// ---------- public transactions ----------

/** `max_fee_for(gas_limit)`: (gas_limit + 100,000) × 64, the cap the wallet declares. */
export const maxFeeFor = (gasLimit) => (BigInt(gasLimit) + ASSUMED_DATA_BYTES) * ASSUMED_BASE_FEE;

/** `FeeDeclaration::new(payer, gas_limit, tip, max_fee)`. */
export const feeDeclaration = (payer, gasLimit, tip, maxFee) => ({
  payer,
  gasLimit: BigInt(gasLimit),
  tip: BigInt(tip),
  maxFee: BigInt(maxFee),
});

/** `ProgramShardSelector::new(account_id, program_account_id)`. */
export const shardSelector = (accountId, programAccountId) => ({ accountId, programAccountId });
/** `ProgramShardSelector::native_balance(account_id)`. */
export const nativeBalance = (accountId) => shardSelector(accountId, NATIVE_TOKEN_PROGRAM_ID);

/**
 * borsh(`public_transaction::Message`): program_account_id [32] ‖ Vec<ProgramShardSelector>
 * (u32 count, then account_id [32] ‖ program_account_id [32]) ‖ Vec<Nonce> (u32 count, u128 LE
 * each) ‖ instruction_data Vec<u8> ‖ Option<FeeDeclaration> (0, or 1 ‖ payer [32] ‖ gas_limit
 * u64 ‖ tip u64 ‖ max_fee u128).
 */
export function encodeMessage({ programAccountId, shardSelectors, nonces, instructionData, fee = null }) {
  const w = new Writer().fixed(programAccountId, 32).u32(shardSelectors.length);
  for (const selector of shardSelectors) w.fixed(selector.accountId, 32).fixed(selector.programAccountId, 32);
  w.u32(nonces.length);
  for (const nonce of nonces) w.u128(nonce);
  w.vec(instructionData);
  w.option(fee, (out, { payer, gasLimit, tip, maxFee }) => out.fixed(payer, 32).u64(gasLimit).u64(tip).u128(maxFee));
  return w.done();
}

/** `Message::hash`: sha256(MESSAGE_PREFIX ‖ borsh(message)). Takes the message or its bytes. */
export function messageHash(message) {
  const bytes = message instanceof Uint8Array ? message : encodeMessage(message);
  return sha256(concat(MESSAGE_PREFIX, bytes));
}

/** `WitnessSet::for_message`: one (signature, public key) per key, in order. */
export function witnessSetFor(message, privateKeys, aux = null) {
  const hash = messageHash(message);
  return privateKeys.map((key) => ({ signature: signHash(hash, key, aux), publicKey: publicKeyOf(key) }));
}

/** borsh(`WitnessSet`): u32 count, then signature [64] ‖ x-only public key [32] each. */
export function encodeWitnessSet(witnesses) {
  const w = new Writer().u32(witnesses.length);
  for (const { signature, publicKey } of witnesses) w.fixed(signature, 64).fixed(publicKey, 32);
  return w.done();
}

/** borsh(`PublicTransaction`): message ‖ witness_set. */
export const encodePublicTransaction = (message, witnesses) =>
  concat(message instanceof Uint8Array ? message : encodeMessage(message), encodeWitnessSet(witnesses));

/** `PublicTransaction::hash`: sha256(borsh(PublicTransaction)), without the LeeTransaction tag. */
export const publicTransactionHash = (txBytes) => sha256(txBytes);

/** borsh(`LeeTransaction::Public(tx)`): tag 0, then the transaction. */
export const encodeLeeTransaction = (txBytes) => concat(Uint8Array.of(LEE_TX_PUBLIC), txBytes);

/**
 * Signs `message` with `privateKeys` (in the order of `message.nonces`) and assembles it.
 * Returns the message hash, the transaction bytes, the tagged wire bytes, their base64
 * (the `sendTransaction` parameter) and the hash the sequencer reports (hex).
 */
export function signPublicTransaction(message, privateKeys, aux = null) {
  const messageBytes = encodeMessage(message);
  const witnesses = witnessSetFor(messageBytes, privateKeys, aux);
  const txBytes = encodePublicTransaction(messageBytes, witnesses);
  const wire = encodeLeeTransaction(txBytes);
  return {
    messageBytes,
    messageHash: messageHash(messageBytes),
    witnesses,
    txBytes,
    wire,
    base64: toBase64(wire),
    hash: toHex(publicTransactionHash(txBytes)),
  };
}

/** borsh(`native_token::Instruction::Transfer { amount }`): variant 0, u128 LE. */
export const transferInstruction = (amount) => new Writer().u8(0).u128(amount).done();

/**
 * The message the wallet's `send_public_transfer` builds: the native token program, both
 * balances selected (sender, recipient), the sender's nonce, the sender paying the fee.
 */
export function transferMessage({ from, to, nonce, amount, gasLimit = NATIVE_TRANSFER_GAS_LIMIT, tip = 0n, maxFee = null }) {
  return {
    programAccountId: NATIVE_TOKEN_PROGRAM_ID,
    shardSelectors: [nativeBalance(from), nativeBalance(to)],
    nonces: [BigInt(nonce)],
    instructionData: transferInstruction(amount),
    fee: feeDeclaration(from, gasLimit, tip, maxFee ?? maxFeeFor(gasLimit)),
  };
}

/** A signed public transfer from the account of `privateKey`. */
export function signTransfer({ privateKey, to, nonce, amount, gasLimit, tip, maxFee, aux = null }) {
  const from = accountIdOfPrivateKey(privateKey);
  return signPublicTransaction(transferMessage({ from, to, nonce, amount, gasLimit, tip, maxFee }), [privateKey], aux);
}

/**
 * The fee reserve the sequencer holds from the payer (`fee_reserve`): gas_limit × base_fee_exec
 * + data_bytes × base_fee_stor + tip, data_bytes being the wire length. With a `getFeeState`
 * quote this prices at the next block's ceiling, as the names CLI does.
 */
export function feeReserve({ gasLimit, dataBytes, tip = 0n, quote }) {
  const exec = BigInt(quote.next_base_fee_exec_ceiling ?? quote.base_fee_exec);
  const stor = BigInt(quote.next_base_fee_stor_ceiling ?? quote.base_fee_stor);
  return BigInt(gasLimit) * exec + BigInt(dataBytes) * stor + BigInt(tip);
}

// ---------- the sequencer's JSON-RPC ----------

export class RpcError extends Error {
  constructor(message, { code = null, data = null } = {}) {
    super(message);
    this.code = code;
    this.data = data;
  }
}

/** Keeps integers above 2^53 exact (u128 balances) where the engine gives JSON source text. */
function reviveBig(_key, value, context) {
  if (typeof value === "number" && Number.isInteger(value) && !Number.isSafeInteger(value) && context?.source) {
    return BigInt(context.source);
  }
  return value;
}

let rpcId = 0;
export async function rpc(url, method, params = []) {
  const response = await fetch(url, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: ++rpcId, method, params }),
    cache: "no-store",
    referrerPolicy: "no-referrer",
  });
  const text = await response.text();
  let body;
  try {
    body = JSON.parse(text, reviveBig);
  } catch {
    throw new RpcError(`${method}: HTTP ${response.status}: ${text.slice(0, 200)}`);
  }
  if (body.error) {
    const { code, message, data } = body.error;
    throw new RpcError(`${method}: ${message}${data ? ` (${typeof data === "string" ? data : JSON.stringify(data)})` : ""}`, { code, data });
  }
  return body.result;
}

/** `getFeeState`: the head fee market (base fees and the next block's floor/ceiling). */
export const getFeeState = (url) => rpc(url, "getFeeState", []);

/** `getAccountsNonces`: the current nonce of each account (what the next transaction carries). */
export async function getAccountsNonces(url, accountIds) {
  const nonces = await rpc(url, "getAccountsNonces", [accountIds.map(formatAccountId)]);
  return nonces.map((nonce) => BigInt(nonce));
}

/** `getAccountBalance`: the account's native balance. */
export const getAccountBalance = async (url, accountId) => BigInt(await rpc(url, "getAccountBalance", [formatAccountId(accountId)]));

/** `sendTransaction`: submits base64(borsh(LeeTransaction)); resolves to the transaction hash (hex). */
export const sendTransaction = (url, signed) =>
  rpc(url, "sendTransaction", [typeof signed === "string" ? signed : signed.base64 ?? toBase64(signed)]);

/** `getTransaction`: null, or [base64 transaction, block ID] once it is in a block. */
export const getTransaction = (url, hashHex) => rpc(url, "getTransaction", [hashHex]);

/**
 * Polls `getTransaction` until `hashHex` is in a block; resolves to the block ID. A block holds
 * failed transactions too, so callers check the effect (balances).
 */
export async function waitForTransaction(url, hashHex, { timeoutMs = 180_000, intervalMs = 2_000, onTick = null } = {}) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    const found = await getTransaction(url, hashHex).catch(() => null);
    if (Array.isArray(found) && found.length === 2) return found[1];
    onTick?.(Date.now() - start);
    await new Promise((resolve) => setTimeout(resolve, intervalMs));
  }
  throw new RpcError(`transaction ${hashHex} was not in a block after ${Math.round(timeoutMs / 1000)} s`);
}
