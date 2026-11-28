// LEZ public transactions in JavaScript (browser and Node): keys, account IDs,
// the borsh message, its hash, BIP-340 signatures, the witness set and the
// transaction bytes `sendTransaction` takes, byte for byte as Rust makes them.
//
// This is the source; public/app/lez-tx.js is the bundle (crypto included), made with
//   cd gateway/app-src && npx rolldown lez-tx.js --format esm --file ../public/app/lez-tx.js
// (rolldown is gateway's; @noble/curves and @noble/hashes are pinned in app-src/package.json.)
// Tests: node --test gateway/public/app/lez-tx.test.mjs
//
// Checked against the real Rust types and engine by vectors/ (see vectors/README.md).
//
// Sources this follows (LEZ v0.4.0):
//   lee/state_machine/src/signature/{mod,private_key,public_key,scheme,schnorr,signing_keys}.rs
//   lee/state_machine/core/src/{account.rs,execution_state.rs,native_token.rs,program/mod.rs,signing_key.rs}
//   lee/state_machine/src/public_transaction/{message,witness_set,transaction}.rs
//   lee/state_machine/src/transaction_message.rs   TransactionMessage, PublicAccountEvidence, hash_with_keys
//   lee/state_machine/src/fees.rs, lez/programs/fee/core/src/assess.rs   FeeDeclaration, reserve
//   lee/key_protocol/src/key_management/key_tree/keys_public.rs   seed -> PublicAccountSecret
//   lez/common/src/{transaction.rs,borsh_base64.rs}   LeeTransaction on the wire
//   lez/sequencer/service/rpc/src/lib.rs   the JSON-RPC methods
//   lez/wallet/src/lib.rs   send_public, max_fee_for

import { schnorr } from "@noble/curves/secp256k1.js";
import { hmac } from "@noble/hashes/hmac.js";
import { sha256, sha512 } from "@noble/hashes/sha2.js";

export { sha256 };

const utf8 = new TextEncoder();
const N = 0xfffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141n;
const BASE58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

function pad32(text) {
  const bytes = new Uint8Array(32);
  bytes.set(utf8.encode(text));
  return bytes;
}

/** `b"/LEE/v0.3/Message/Public/\x00…"` (message.rs:16 PREFIX). */
export const MESSAGE_PREFIX = pad32("/LEE/v0.3/Message/Public/");
/** `b"/LEE/v0.3/KeyedMessage/Public/\x00\x00"` (message.rs:17 KEYED_PREFIX). */
export const KEYED_MESSAGE_PREFIX = pad32("/LEE/v0.3/KeyedMessage/Public/");
/** `b"/LEE/v0.3/AccountId/Public/\x00…"` (account.rs:259-260 PUBLIC_ACCOUNT_ID_PREFIX). */
export const ACCOUNT_ID_PREFIX = pad32("/LEE/v0.3/AccountId/Public/");
/** `b"/LEE/v0.3/AccountId/Public/Schn/"` (account.rs:261 PREFIX_SCHNORR). */
export const ACCOUNT_ID_SCHNORR_PREFIX = pad32("/LEE/v0.3/AccountId/Public/Schn/");
/** `b"/LEE/v0.3/KeySeed/\x00…"` (signing_keys.rs:15-16 DS_KEY_SEED). */
export const KEY_SEED_PREFIX = pad32("/LEE/v0.3/KeySeed/");
/** `b"/LEE/v0.3/KeyGen/Schnorr/\x00…"` (schnorr.rs:9 DS_KEYGEN). */
export const SCHNORR_KEYGEN_PREFIX = pad32("/LEE/v0.3/KeyGen/Schnorr/");
/** `b"/LEE/v0.3/KeyCommitment/Schnorr/"` (schnorr.rs:8 DS_KEY). */
export const SCHNORR_KEY_COMMIT_PREFIX = pad32("/LEE/v0.3/KeyCommitment/Schnorr/");
/** `b"/LEE/v0.2/AccountId/PDA/\x00…"` (program/mod.rs:201-202 PROGRAM_DERIVED_ACCOUNT_ID_PREFIX). */
export const PDA_PREFIX = pad32("/LEE/v0.2/AccountId/PDA/");
/** `b"/LEE/SystemUpgrader/v1/Name\x00…"` (program/mod.rs:127-128 SYSTEM_PROGRAM_SEED_PREFIX). */
export const SYSTEM_PROGRAM_SEED_PREFIX = pad32("/LEE/SystemUpgrader/v1/Name");
/** `SYSTEM_UPGRADER_ACCOUNT_ID`: `[0xFD; 32]` (program/mod.rs:32). */
export const SYSTEM_UPGRADER_ACCOUNT_ID = new Uint8Array(32).fill(0xfd);

const MASTER_PUBLIC_KEY = utf8.encode("/LEE-Keys/v1/Master/Public");
const CHILD_PUBLIC_PREFIX = utf8.encode("/LEE-Keys/v1/Child/Public");

/** `NATIVE_TOKEN_PROGRAM_ID`: 32 zero bytes; an account's balance is its actor state under this. */
export const NATIVE_TOKEN_PROGRAM_ID = new Uint8Array(32);
/** `LeeTransaction` borsh variant tags. */
export const LEE_TX_PUBLIC = 0;

/** `KeyType` borsh discriminants (`signing_key.rs:17-19`). */
export const KEY_TYPE_SCHNORR = 0;
export const KeyType = Object.freeze({ Schnorr: KEY_TYPE_SCHNORR });

/** `Authority` borsh discriminants (`execution_state.rs:46-49`). */
export const AUTHORITY_NONE = 0;
export const AUTHORITY_SIGNER = 1;
export const Authority = Object.freeze({ None: AUTHORITY_NONE, Signer: AUTHORITY_SIGNER });

/** The wallet's assumed worst base fee and data size (lez/wallet/src/lib.rs:92-98). */
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

/**
 * `AccountId::from_system_program_name` (`program/mod.rs:48-56, 77-89, 125-140, 199-214`):
 * `PDA([0xFD; 32], sha256("/LEE/SystemUpgrader/v1/Name\0…" ‖ name_padded_32))`.
 */
export function systemProgramId(name) {
  const raw = typeof name === "string" ? utf8.encode(name) : name;
  if (!(raw instanceof Uint8Array) || raw.length === 0 || raw.length > 32) {
    throw new Error("a system program name is 1 to 32 bytes");
  }
  const padded = new Uint8Array(32);
  padded.set(raw);
  const seed = sha256(concat(SYSTEM_PROGRAM_SEED_PREFIX, padded));
  return sha256(concat(PDA_PREFIX, SYSTEM_UPGRADER_ACCOUNT_ID, seed));
}

// ---------- keys and account IDs (v0.4.0 per-nonce rotation) ----------

const bytesToNumber = (bytes) => BigInt(`0x${toHex(bytes) || "0"}`);

/** `PublicAccountSecret::new`: any 32-byte root secret for a public account (`signing_keys.rs:23-62`). */
export function checkAccountSecret(bytes) {
  if (!(bytes instanceof Uint8Array) || bytes.length !== 32) throw new Error("a public account secret is 32 bytes");
  return bytes;
}

/** `PublicAccountSecret::new_os_random`: 32 random bytes (`signing_keys.rs:54-56`). */
export const randomAccountSecret = () => crypto.getRandomValues(new Uint8Array(32));

/** `PrivateKey::try_new`: a valid secp256k1 secret key (1..n-1, big-endian). */
export function checkPrivateKey(bytes) {
  if (!(bytes instanceof Uint8Array) || bytes.length !== 32) throw new Error("a private key is 32 bytes");
  const n = bytesToNumber(bytes);
  if (n === 0n || n >= N) throw new Error("invalid private key");
  return bytes;
}

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

/** `key_seed` (`signing_keys.rs:149-156`): sha256(DS_KEY_SEED ‖ secret ‖ key_type u8 ‖ nonce u128 LE). */
export function keySeed(secret, nonce, keyType = KEY_TYPE_SCHNORR) {
  checkAccountSecret(secret);
  if (keyType !== KEY_TYPE_SCHNORR) throw new Error(`unsupported key type: ${keyType}`);
  return sha256(concat(KEY_SEED_PREFIX, secret, Uint8Array.of(keyType), new Writer().u128(nonce).done()));
}

/**
 * `Schnorr::keygen` (`schnorr.rs:23-33, 83-92`): tries `ctr = 0..=255` with
 * `sha256(DS_KEYGEN ‖ seed ‖ ctr)` until a valid secp256k1 scalar is found.
 */
export function schnorrKeygen(seed) {
  if (!(seed instanceof Uint8Array) || seed.length !== 32) throw new Error("seed must be 32 bytes");
  for (let ctr = 0; ctr <= 255; ctr += 1) {
    const candidate = sha256(concat(SCHNORR_KEYGEN_PREFIX, seed, Uint8Array.of(ctr)));
    const n = bytesToNumber(candidate);
    if (n > 0n && n < N) {
      return { secretKey: candidate, publicKey: schnorr.getPublicKey(candidate) };
    }
  }
  throw new Error("no valid secp256k1 scalar found in 256 candidates");
}

/** `signing_key::<Schnorr>` (`signing_keys.rs:160-168`): the account's per-nonce keypair. */
export function signingKey(secret, nonce, keyType = KEY_TYPE_SCHNORR) {
  if (keyType !== KEY_TYPE_SCHNORR) throw new Error(`unsupported key type: ${keyType}`);
  return schnorrKeygen(keySeed(secret, nonce, keyType));
}

/** `Schnorr::commit` (`schnorr.rs:53-58`): sha256(DS_KEY ‖ x-only pk). */
export function schnorrKeyCommit(publicKey) {
  if (!(publicKey instanceof Uint8Array) || publicKey.length !== 32) throw new Error("a public key is 32 bytes (x-only)");
  return sha256(concat(SCHNORR_KEY_COMMIT_PREFIX, publicKey));
}

/** `SigningKeyState { key_type, key_commitment }` (`signing_key.rs:36-39`). */
export function signingKeyState(keyCommitment, keyType = KEY_TYPE_SCHNORR) {
  if (keyType !== KEY_TYPE_SCHNORR) throw new Error(`unsupported key type: ${keyType}`);
  if (!(keyCommitment instanceof Uint8Array) || keyCommitment.length !== 32) throw new Error("a key commitment is 32 bytes");
  return { keyType, keyCommitment };
}

/** borsh(`SigningKeyState`): 1-byte `KeyType` tag ‖ 32-byte `KeyCommitment` (33 bytes, `signing_key.rs:36-48`). */
export function encodeSigningKeyState(state) {
  if (state instanceof Uint8Array) {
    if (state.length !== 33) throw new Error(`a SigningKeyState is 33 bytes, got ${state.length}`);
    if (state[0] !== KEY_TYPE_SCHNORR) throw new Error(`unsupported key type: ${state[0]}`);
    return state;
  }
  if (!state || typeof state !== "object") {
    throw new Error("expected a SigningKeyState (33 bytes or { keyType, keyCommitment })");
  }
  const { keyType, keyCommitment } = state;
  if (keyType !== KEY_TYPE_SCHNORR) throw new Error(`unsupported key type: ${keyType}`);
  return new Writer().u8(keyType).fixed(keyCommitment, 32).done();
}

/** `key_state(secret, key_type, nonce)` (`signing_keys.rs:184-186`). */
export function keyState(secret, nonce, keyType = KEY_TYPE_SCHNORR) {
  const { publicKey } = signingKey(secret, nonce, keyType);
  return signingKeyState(schnorrKeyCommit(publicKey), keyType);
}

/** `initial_key(secret)` (`signing_keys.rs:172-174`): the account's nonce-0 `SigningKeyState`. */
export const initialKey = (secret) => keyState(secret, 0n, KEY_TYPE_SCHNORR);

/** 33-byte borsh encoding of `initial_key(secret)` (66 hex chars, as `SigningKeyState::Display`). */
export const initialKeyBytes = (secret) => encodeSigningKeyState(initialKey(secret));

/**
 * `AccountId::for_initial_key` (`account.rs:258-278`):
 * `sha256(ACCOUNT_ID_PREFIX ‖ sha256(ACCOUNT_ID_SCHNORR_PREFIX ‖ key_commitment))`.
 */
export function accountIdForInitialKey(state) {
  const encoded = encodeSigningKeyState(state);
  const keyHash = sha256(concat(ACCOUNT_ID_SCHNORR_PREFIX, encoded.subarray(1)));
  return sha256(concat(ACCOUNT_ID_PREFIX, keyHash));
}

/** `public_account_id(secret)` (`signing_keys.rs:178-180`): the public account `secret` controls. */
export const publicAccountId = (secret) => accountIdForInitialKey(initialKey(secret));

/** `ChildKeysPublic::root` (`keys_public.rs:17-25`): HMAC-SHA512(key "/LEE-Keys/v1/Master/Public", seed)[..32]. */
export function publicRoot(seed) {
  if (seed.length !== 64) throw new Error("seed must be 64 bytes");
  const secret = hmac(sha512, MASTER_PUBLIC_KEY, seed).slice(0, 32);
  return { secret, cci: null, accountId: publicAccountId(secret) };
}

/** `ChildKeysPublic::nth_child` (`keys_public.rs:28-39`): HMAC-SHA512(key parent.secret, "/LEE-Keys/v1/Child/Public" ‖ cci BE)[..32]. */
export function publicChild(parent, cci) {
  const index = new Uint8Array(4);
  new DataView(index.buffer).setUint32(0, cci, false);
  const parentSecret = checkAccountSecret(parent instanceof Uint8Array ? parent : parent.secret);
  const secret = hmac(sha512, parentSecret, concat(CHILD_PUBLIC_PREFIX, index)).slice(0, 32);
  return { secret, cci, accountId: publicAccountId(secret) };
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

/** `Actor::new(account_id, program_account_id)` with optional `Authority` (default `Authority.None`). */
export const actor = (accountId, programAccountId, authority = AUTHORITY_NONE) => ({
  accountId,
  programAccountId,
  authority,
});
/** An actor carrying `Authority.Signer` (1). */
export const signerActor = (accountId, programAccountId) => actor(accountId, programAccountId, AUTHORITY_SIGNER);
/** `Actor::native_balance(account_id)` with optional `Authority` (default `Authority.None`). */
export const nativeBalance = (accountId, authority = AUTHORITY_NONE) =>
  actor(accountId, NATIVE_TOKEN_PROGRAM_ID, authority);

/** `PublicAccountEvidence::Key(SigningKeyState)`: the 33-byte initial signing key state behind a plain account. */
export const keyEvidence = (keyStateOrBytes) => ({ keyState: encodeSigningKeyState(keyStateOrBytes) });
/** `PublicAccountEvidence::Pda { program, seed }`: where a program-derived account comes from. */
export const pdaEvidence = (program, seed) => ({ program, seed });

function compareBytes(a, b) {
  for (let i = 0; i < a.length; i += 1) {
    if (a[i] !== b[i]) return a[i] - b[i];
  }
  return 0;
}

const compareActors = (a, b) =>
  compareBytes(a.accountId, b.accountId) || compareBytes(a.programAccountId, b.programAccountId);

/** Sorted and deduplicated as borsh writes `BTreeMap<Actor, Authority>`; `Authority.Signer` wins on merge. */
function mergeActors(items) {
  const sorted = items
    .map((item) => ({
      accountId: item.accountId,
      programAccountId: item.programAccountId,
      authority: item.authority ?? AUTHORITY_NONE,
    }))
    .sort(compareActors);
  const out = [];
  for (const item of sorted) {
    if (item.authority !== AUTHORITY_NONE && item.authority !== AUTHORITY_SIGNER) {
      throw new Error(`invalid authority: ${item.authority}`);
    }
    const prev = out[out.length - 1];
    if (prev && compareActors(prev, item) === 0) {
      if (item.authority === AUTHORITY_SIGNER) prev.authority = AUTHORITY_SIGNER;
    } else {
      out.push(item);
    }
  }
  return out;
}

function writeActor(w, { accountId, programAccountId }) {
  w.fixed(accountId, 32).fixed(programAccountId, 32);
}

function writeEvidence(w, evidence) {
  if (evidence.keyState) w.u8(0).fixed(encodeSigningKeyState(evidence.keyState), 33);
  else w.u8(1).fixed(evidence.program, 32).fixed(evidence.seed, 32);
}

/**
 * borsh(`public_transaction::Message`) in LEZ v0.4.0 (`message.rs:19-45, 141-160`,
 * `transaction_message.rs:15-51`, `execution_state.rs:45-60`), in exact field order:
 *   public_context.actors               u32 len + sorted (account_id [32] ‖ program_account_id [32] ‖ authority u8)
 *   public_context.authorized_accounts  u32 len + sorted account_id [32]: exactly the signers
 *   public_context.promotions           u32 0: a public transaction promotes nothing
 *   public_context.private_signers      u32 0: a public transaction declares no private signers
 *   execution.root.to                   account_id [32] ‖ program_account_id [32]
 *   execution.root.message              Vec<u8>
 *   nonces                              u32 len + sorted (account_id [32] ‖ nonce u128 LE)
 *   fee                                 0, or 1 ‖ payer [32] ‖ gas_limit u64 ‖ tip u64 ‖ max_fee u128
 *   admission_evidence                  u32 len + (0 ‖ key_type u8 ‖ commitment [32]) or (1 ‖ program [32] ‖ seed [32])
 *
 * `to` is the actor the transaction starts at, `data` the message to it. `actors` is everything
 * the transaction reaches; `to` joins it. `nonces` is `[{ accountId, nonce }]`, one per signer.
 */
export function encodeMessage({ to, data, actors = [], nonces, fee = null, evidence = [] }) {
  const declared = mergeActors([to, ...actors]);
  const signers = [...nonces].sort((a, b) => compareBytes(a.accountId, b.accountId));
  if (signers.some(({ accountId }, i) => i > 0 && compareBytes(signers[i - 1].accountId, accountId) === 0)) {
    throw new Error("an account can only have one nonce"); // nonce_map refuses this too
  }
  const w = new Writer().u32(declared.length);
  for (const declaredActor of declared) {
    writeActor(w, declaredActor);
    w.u8(declaredActor.authority);
  }
  w.u32(signers.length);
  for (const { accountId } of signers) w.fixed(accountId, 32);
  w.u32(0); // promotions
  w.u32(0); // private_signers
  writeActor(w, to);
  w.vec(data);
  w.u32(signers.length);
  for (const { accountId, nonce } of signers) w.fixed(accountId, 32).u128(nonce);
  w.option(fee, (out, { payer, gasLimit, tip, maxFee }) => out.fixed(payer, 32).u64(gasLimit).u64(tip).u128(maxFee));
  w.u32(evidence.length);
  for (const item of evidence) writeEvidence(w, item);
  return w.done();
}

/** `Message::hash`: sha256(MESSAGE_PREFIX ‖ borsh(message)). Takes the message or its bytes. */
export function messageHash(message) {
  const bytes = message instanceof Uint8Array ? message : encodeMessage(message);
  return sha256(concat(MESSAGE_PREFIX, bytes));
}

/**
 * `hash_with_keys` (`transaction_message.rs:155-167`):
 * `sha256(KEYED_MESSAGE_PREFIX ‖ message_hash ‖ next_0(33) ‖ ... ‖ next_k(33))`.
 */
export function hashWithKeys(msgHash32, nextKeys) {
  if (!(msgHash32 instanceof Uint8Array) || msgHash32.length !== 32) throw new Error("expected a 32-byte message hash");
  return sha256(concat(KEYED_MESSAGE_PREFIX, msgHash32, ...nextKeys.map(encodeSigningKeyState)));
}

/** `Message::message_with_keys` (`transaction_message.rs:100-102`): the 32-byte value each signer signs. */
export function messageWithKeys(message, nextKeys) {
  return hashWithKeys(messageHash(message), nextKeys);
}

function buildWitnessSet(message, messageBytes, secrets, aux = null) {
  const nonceByAccount = new Map();
  for (const { accountId, nonce } of message.nonces) {
    const hex = toHex(accountId);
    if (nonceByAccount.has(hex)) throw new Error("an account can only have one nonce");
    nonceByAccount.set(hex, BigInt(nonce));
  }
  const signers = secrets
    .map((secret) => {
      checkAccountSecret(secret);
      const accountId = publicAccountId(secret);
      return { secret, accountId, hex: toHex(accountId) };
    })
    .sort((a, b) => compareBytes(a.accountId, b.accountId));
  const expected = [...nonceByAccount.keys()].sort();
  const actual = signers.map((s) => s.hex);
  if (actual.length !== expected.length || actual.some((hex, i) => hex !== expected[i])) {
    throw new Error("the signing keys are not the accounts with nonces");
  }

  const prepared = signers.map(({ secret, accountId, hex }) => {
    const nonce = nonceByAccount.get(hex);
    const { secretKey, publicKey } = signingKey(secret, nonce, KEY_TYPE_SCHNORR);
    const current = signingKeyState(schnorrKeyCommit(publicKey), KEY_TYPE_SCHNORR);
    const next = keyState(secret, nonce + 1n, KEY_TYPE_SCHNORR);
    return { nonce, secretKey, publicKey, current, next };
  });

  const msgHash = messageHash(messageBytes);
  const keyedHash = hashWithKeys(msgHash, prepared.map((p) => p.next));
  const witnesses = prepared.map(({ nonce, secretKey, publicKey, current, next }) => {
    const signature = signHash(keyedHash, secretKey, aux);
    const schnorrAuth = concat(signature, publicKey);
    const authData = nonce === 0n ? concat(encodeSigningKeyState(current), schnorrAuth) : schnorrAuth;
    return { authData, next };
  });
  return { msgHash, keyedHash, witnesses };
}

/**
 * `WitnessSet::sign` (`witness_set.rs:19-58`, `signing_keys.rs:105-145`):
 * one `{ authData, next }` entry per signer in ascending `AccountId` (`nonces`) order.
 * At nonce 0 `authData` is 129 bytes (`initial_key(33) ‖ sig(64) ‖ pk_0(32)`); at nonce > 0
 * it is 96 bytes (`sig(64) ‖ pk_n(32)`). `next` is the 33-byte `SigningKeyState` at `nonce + 1`.
 */
export function witnessSetFor(message, secrets, aux = null) {
  return buildWitnessSet(message, encodeMessage(message), secrets, aux).witnesses;
}

/**
 * Verifies one witness entry against `storedKeyState` (or `null` at nonce 0, where the initial
 * key prefixes `authData` and must derive `accountId`), mirroring `scheme.rs:91-142`.
 */
export function verifyWitnessEntry(storedKeyState, accountId, keyedHash, authData) {
  let current;
  let rest;
  if (storedKeyState !== null && storedKeyState !== undefined) {
    current = encodeSigningKeyState(storedKeyState);
    rest = authData;
  } else {
    if (authData.length < 33) return false;
    const prefix = authData.subarray(0, 33);
    if (prefix[0] !== KEY_TYPE_SCHNORR) return false;
    if (!equalBytes(accountIdForInitialKey(prefix), accountId)) return false;
    current = prefix;
    rest = authData.subarray(33);
  }
  if (rest.length !== 96) return false;
  const signature = rest.subarray(0, 64);
  const publicKey = rest.subarray(64, 96);
  if (!isValidPublicKey(publicKey)) return false;
  if (!equalBytes(schnorrKeyCommit(publicKey), current.subarray(1))) return false;
  return verifyHash(signature, keyedHash, publicKey);
}

/** borsh(`WitnessSet`): u32 count, then `(Vec<u8> auth_data ‖ SigningKeyState next[33])` each (`witness_set.rs:6-15`). */
export function encodeWitnessSet(witnesses) {
  const w = new Writer().u32(witnesses.length);
  for (const { authData, next } of witnesses) {
    w.vec(authData).fixed(encodeSigningKeyState(next), 33);
  }
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
 * Signs `message` with `secrets` (`PublicAccountSecret`s), which must be exactly the accounts in
 * `message.nonces` (`check_signers`), and assembles the transaction.
 * Returns the message bytes, message hash, keyed message hash, witnesses, transaction bytes,
 * tagged wire bytes, their base64 (the `sendTransaction` parameter) and the hex transaction hash.
 */
export function signPublicTransaction(message, secrets, aux = null) {
  const messageBytes = encodeMessage(message);
  const { msgHash, keyedHash, witnesses } = buildWitnessSet(message, messageBytes, secrets, aux);
  const txBytes = encodePublicTransaction(messageBytes, witnesses);
  const wire = encodeLeeTransaction(txBytes);
  return {
    messageBytes,
    messageHash: msgHash,
    messageWithKeys: keyedHash,
    witnesses,
    txBytes,
    wire,
    base64: toBase64(wire),
    hash: toHex(publicTransactionHash(txBytes)),
  };
}

/** borsh(`native_token::Message::Transfer { to, amount }`): variant 0, account [32], u128 LE. */
export const transferInstruction = (to, amount) => new Writer().u8(0).fixed(to, 32).u128(amount).done();

/**
 * The message the wallet's `NativeTokenTransfer::transfer` builds: sent to the sender's balance
 * actor (with `Authority.Signer`), both balances declared, the sender's nonce, the sender paying
 * the fee. An account that isn't on the chain yet is admitted only with `evidence`:
 * `[keyEvidence(initialKey(recipientSecret))]`.
 */
export function transferMessage({ from, to, nonce, amount, gasLimit = NATIVE_TRANSFER_GAS_LIMIT, tip = 0n, maxFee = null, evidence = [] }) {
  return {
    to: nativeBalance(from, AUTHORITY_SIGNER),
    data: transferInstruction(to, amount),
    actors: [nativeBalance(from, AUTHORITY_SIGNER), nativeBalance(to, AUTHORITY_NONE)],
    nonces: [{ accountId: from, nonce: BigInt(nonce) }],
    fee: feeDeclaration(from, gasLimit, tip, maxFee ?? maxFeeFor(gasLimit)),
    evidence,
  };
}

/** A signed public transfer from the account of `secret` (or `privateKey` alias). */
export function signTransfer({ secret, privateKey, to, nonce, amount, gasLimit, tip, maxFee, evidence, aux = null }) {
  const accountSecret = secret ?? privateKey;
  const from = publicAccountId(accountSecret);
  return signPublicTransaction(transferMessage({ from, to, nonce, amount, gasLimit, tip, maxFee, evidence }), [accountSecret], aux);
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

/**
 * `getAccountView`: null when the account isn't on the chain, or `{ nonce, data: { actor_states } }`
 * with only `selected`'s state, which reads as empty when that actor has none.
 */
export const getAccountView = (url, selected) =>
  rpc(url, "getAccountView", [{
    account_id: formatAccountId(selected.accountId),
    program_account_id: formatAccountId(selected.programAccountId),
  }]);

/**
 * Whether the account is on the chain. Only one that is can be paid without evidence; a
 * transaction paying one that isn't fails in its block, and the fee is still charged.
 */
export const accountExists = async (url, accountId) => (await getAccountView(url, nativeBalance(accountId))) !== null;

/** `sendTransaction`: submits base64(borsh(LeeTransaction)); resolves to the transaction hash (hex). */
export const sendTransaction = (url, signed) =>
  rpc(url, "sendTransaction", [typeof signed === "string" ? signed : signed.base64 ?? toBase64(signed)]);

/** `getTransaction`: null, or [base64 transaction, block ID] once it is in a block. */
export const getTransaction = (url, hashHex) => rpc(url, "getTransaction", [hashHex]);

/** `getTransactionReceipt` (added in LEZ v0.4.0): null, or the transaction's receipt. */
export const getTransactionReceipt = (url, hashHex) => rpc(url, "getTransactionReceipt", [hashHex]);

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
