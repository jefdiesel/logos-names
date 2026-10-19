// Names Messages, version 1: end-to-end encrypted messages to a name.
//
// One module for the phone app and for anything else that speaks the format (the
// test bot imports the same bundled file). Build both browser files from here:
//   cd gateway/app-src && npm install
//   npx rolldown msg-crypto.js --format esm --file ../public/app/msg-crypto.js
//   npx rolldown waku.js --platform browser --format esm --file ../public/app/waku.js
// (rolldown is gateway's; dependencies are pinned in app-src/package.json.)
//
// A name turns messages on with the text entry `msg` = its device's X25519 public
// key in hex (64 characters):  names set-text <name> msg <hex>
//
// Wire payload (one Waku message):
//   0x01 ‖ ephemeral X25519 public key (32) ‖ nonce (24) ‖ XChaCha20-Poly1305 ciphertext
// Key: HKDF-SHA256(ikm = X25519(ephemeral secret, recipient key),
//                  salt = ephemeral public ‖ recipient public, info = "names/msg/v1"), 32 bytes.
// Associated data: 0x01 ‖ ephemeral public key.
// Plaintext: JSON { v: 1, from, fromKey, text, ts, auth }.
//   `auth` proves the sender holds the secret key of `fromKey` (otherwise anyone
//   could write anyone's published key there): HMAC-SHA256 with key
//   HKDF-SHA256(ikm = X25519(sender secret, recipient public), salt = empty,
//   info = "names/msg/v1/auth"), over
//   0x01 ‖ ephemeral public ‖ nonce ‖ utf8(from) ‖ 0x00 ‖ fromKey (32) ‖ utf8(ts) ‖ 0x00 ‖ utf8(text).
//   The recipient can compute the same key, so it proves who sent it to the
//   recipient only, not to anyone else (like a signature would).
// Waku content topic: /names/1/msg-<first 16 hex of SHA-256(recipient public key, 32 raw bytes)>/proto

import { x25519 } from "@noble/curves/ed25519.js";
import { xchacha20poly1305 } from "@noble/ciphers/chacha.js";
import { hkdf } from "@noble/hashes/hkdf.js";
import { hmac } from "@noble/hashes/hmac.js";
import { sha256 } from "@noble/hashes/sha2.js";

export const MSG_VERSION = 1;
export const MSG_TEXT_KEY = "msg";
export const MAX_TEXT = 2000;
export const DEFAULT_PEER =
  "/dns4/west-territories-location-trek.trycloudflare.com/tcp/443/wss/p2p/16Uiu2HAmFQGCcQzKpY9QhESgVXWXUv4AHWWvR6pX6vE1qvFUpyoc";
export const CLUSTER = { clusterId: 3, numShardsInCluster: 8 };

const INFO = new TextEncoder().encode("names/msg/v1");
const AUTH_INFO = new TextEncoder().encode("names/msg/v1/auth");
const HEADER_LEN = 1 + 32 + 24;
const TAG_LEN = 16;
const NAME = /^[a-z0-9](?:[a-z0-9-]{0,30}[a-z0-9])?$/;
const utf8 = (text) => new TextEncoder().encode(text);

export function toHex(bytes) {
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

export function fromHex(hex) {
  if (typeof hex !== "string" || !/^(?:[0-9a-fA-F]{2})*$/.test(hex)) throw new Error("not hex");
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(hex.slice(2 * i, 2 * i + 2), 16);
  return out;
}

function concat(...parts) {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let at = 0;
  for (const p of parts) {
    out.set(p, at);
    at += p.length;
  }
  return out;
}

function equal(a, b) {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= a[i] ^ b[i];
  return diff === 0;
}

/** X25519 shared secret, refusing the all-zero result of a low-order key. */
function dh(secretKey, publicKey) {
  const shared = x25519.getSharedSecret(secretKey, publicKey);
  if (shared.every((b) => b === 0)) throw new Error("bad public key");
  return shared;
}

/** A new device messaging key pair: { secretKey, publicKey }, 32 bytes each. */
export function newKeyPair() {
  const secretKey = x25519.utils.randomSecretKey();
  return { secretKey, publicKey: x25519.getPublicKey(secretKey) };
}

export function publicKeyOf(secretKey) {
  return x25519.getPublicKey(secretKey);
}

/** The 32-byte key in a name's `msg` entry, or null when it isn't one. */
export function parseMsgKey(value) {
  const hex = typeof value === "string" ? value.trim().toLowerCase() : "";
  return /^[0-9a-f]{64}$/.test(hex) ? fromHex(hex) : null;
}

/** The Waku content topic of the inbox of `publicKey`. */
export function inboxTopic(publicKey) {
  return `/names/1/msg-${toHex(sha256(publicKey)).slice(0, 16)}/proto`;
}

function authTag(staticShared, header, from, fromKey, ts, text) {
  const key = hkdf(sha256, staticShared, new Uint8Array(0), AUTH_INFO, 32);
  const data = concat(header, utf8(from), [0], fromKey, utf8(String(ts)), [0], utf8(text));
  return hmac(sha256, key, data);
}

/**
 * Seals `text` from the name `from` (whose device key is `fromSecret`) to the
 * device key `to`. Returns the wire payload.
 */
export function sealMessage({ to, from, fromSecret, text, ts = Date.now() }) {
  if (!(to instanceof Uint8Array) || to.length !== 32) throw new Error("recipient key must be 32 bytes");
  if (!NAME.test(from)) throw new Error("from must be a name");
  if (typeof text !== "string" || text.length === 0 || text.length > MAX_TEXT) throw new Error("bad text");
  const fromKey = publicKeyOf(fromSecret);
  const eph = newKeyPair();
  const nonce = crypto.getRandomValues(new Uint8Array(24));
  const header = concat([MSG_VERSION], eph.publicKey, nonce);
  const key = hkdf(sha256, dh(eph.secretKey, to), concat(eph.publicKey, to), INFO, 32);
  const auth = toHex(authTag(dh(fromSecret, to), header, from, fromKey, ts, text));
  const plain = utf8(JSON.stringify({ v: MSG_VERSION, from, fromKey: toHex(fromKey), text, ts, auth }));
  const sealed = xchacha20poly1305(key, nonce, concat([MSG_VERSION], eph.publicKey)).encrypt(plain);
  return concat(header, sealed);
}

/**
 * Opens a wire payload with this device's secret key. Returns null when it isn't
 * for this key or isn't well formed, otherwise
 *   { id, from, fromKey (hex), text, ts, authentic }
 * where `authentic` says the sender holds the secret key of `fromKey`. Whether
 * `fromKey` belongs to the name `from` is for the caller to check on chain.
 * `id` (the ephemeral key, hex) tells repeats apart.
 */
export function openMessage(secretKey, payload) {
  try {
    if (!(payload instanceof Uint8Array) || payload.length < HEADER_LEN + TAG_LEN || payload[0] !== MSG_VERSION) return null;
    const header = payload.subarray(0, HEADER_LEN);
    const ephPub = payload.subarray(1, 33);
    const nonce = payload.subarray(33, HEADER_LEN);
    const me = publicKeyOf(secretKey);
    const key = hkdf(sha256, dh(secretKey, ephPub), concat(ephPub, me), INFO, 32);
    const plain = xchacha20poly1305(key, nonce, concat([MSG_VERSION], ephPub)).decrypt(payload.subarray(HEADER_LEN));
    const msg = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(plain));
    const { v, from, fromKey, text, ts, auth } = msg ?? {};
    if (v !== MSG_VERSION || typeof from !== "string" || !NAME.test(from)) return null;
    if (typeof text !== "string" || text.length > MAX_TEXT || !Number.isSafeInteger(ts)) return null;
    const senderKey = parseMsgKey(fromKey);
    if (senderKey === null) return null;
    let authentic = false;
    try {
      const expected = authTag(dh(secretKey, senderKey), header, from, senderKey, ts, text);
      authentic = typeof auth === "string" && /^[0-9a-f]{64}$/.test(auth) && equal(expected, fromHex(auth));
    } catch {
      authentic = false;
    }
    return { id: toHex(ephPub), from, fromKey: toHex(senderKey), text, ts, authentic };
  } catch {
    return null;
  }
}

/**
 * Whether a message opened by openMessage really comes from the name it claims:
 * authentic, and `onChainKey` (the `msg` entry of `from`, as read from the chain)
 * is its `fromKey`.
 */
export function isVerified(message, onChainKey) {
  const key = parseMsgKey(onChainKey ?? "");
  return Boolean(message?.authentic && key && toHex(key) === message.fromKey);
}
