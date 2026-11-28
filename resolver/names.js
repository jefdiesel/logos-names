// Logos Names resolver: derives a name's identifier, key and accounts, reads
// them over the LEZ sequencer's JSON-RPC and opens what they hold. Its one
// dependency is @noble/hashes (pure-JS Argon2id); it runs in Node 20+, browsers
// and Cloudflare Workers (anything with fetch and WebCrypto).

import { argon2idAsync } from "@noble/hashes/argon2.js";

const utf8 = new TextEncoder();

const BASE58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
const PDA_PREFIX = pad32("/LEE/v0.2/AccountId/PDA/");
const DERIVE_SALT = "logos-names/v5";
// Argon2id version 0x13: 64 MiB, 3 passes, 1 lane, 32 bytes out. One fixed salt for
// every network, so a name keeps its identifier and key from testnet to mainnet.
const DERIVE_PARAMS = { t: 3, m: 65536, p: 1, dkLen: 32, version: 0x13, asyncTick: 50 };
const ID_DOMAIN = "logos-names/v5/id";
const KEY_DOMAIN = "logos-names/v5/key";
const NAME_SEED_DOMAIN = "logos-names/v5/name/";
const PAGE_SEED_DOMAIN = "logos-names/v5/page/";
const SETTINGS_SEED_DOMAIN = "logos-names/v5/config";
const TIER_DOMAIN = "logos-names/v7/tier/";
const SUBNAME_SEED_DOMAIN = "logos-names/v7/subname/";
const SUBPAGE_SEED_DOMAIN = "logos-names/v7/subpage/";
const MARKET_BUCKET_DOMAIN = "logos-names/v8/market/";
const SITE_DIR_HEADER_DOMAIN = "logos-names/v8/sites-meta";
const SITE_BUCKET_DOMAIN = "logos-names/v8/sites/";
// Sealed data is bound to its kind, so a sealed page never opens as a record.
const ASSOCIATED_DATA = { record: "logos-names/v5/record", page: "logos-names/v5/page" };
const NONCE_LENGTH = 12;
const TAG_LENGTH = 16;
const PROFILE_BLOCK = 256;
const LABEL_PATTERN = /^(?!-)[a-z0-9-]{1,32}(?<!-)$/;

/** Maximum byte length of a `label.parent` subname (so `label--parent` fits 63-byte DNS labels). */
export const MAX_SUBNAME_LEN = 62;
/** How long an expired name still resolves, in milliseconds: 30 days. */
export const GRACE_MS = 2_592_000_000;
export const PRICE_TIER_5 = 0;
export const PRICE_TIER_6_7 = 1;
export const PRICE_TIER_8_PLUS = 2;
export const TERM_30D = 0;
export const TERM_1Y = 1;
export const TERM_5Y = 2;
export const TERM_PERMANENT = 3;
export const MARKET_BUCKETS = 16;
export const SITE_BUCKETS = 16;
export const MARKET_BUCKET_MAX = 64;
export const SITE_BUCKET_MAX = 32;
export const DEFAULT_PRICES = [
  [2_000_000n, 20_000_000n, 80_000_000n, 400_000_000n],
  [500_000n, 5_000_000n, 20_000_000n, 100_000_000n],
  [100_000n, 1_000_000n, 4_000_000n, 20_000_000n],
];

function pad32(text) {
  const bytes = new Uint8Array(32);
  bytes.set(utf8.encode(text));
  return bytes;
}

function concat(...parts) {
  const out = new Uint8Array(parts.reduce((total, part) => total + part.length, 0));
  let offset = 0;
  for (const part of parts) {
    out.set(part, offset);
    offset += part.length;
  }
  return out;
}

async function sha256(bytes) {
  return new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
}

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

function accountIdBytes(accountId) {
  const bytes = base58Decode(accountId);
  if (bytes.length !== 32) throw new Error(`account ID must be 32 bytes: ${accountId}`);
  return bytes;
}

const toHex = (bytes) => Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");

function fromHex(text) {
  if (!/^([0-9a-f]{2})*$/i.test(text)) throw new Error(`invalid hex: ${text}`);
  return Uint8Array.from(text.match(/../g) ?? [], (pair) => parseInt(pair, 16));
}

/** How names are written for people: bare, `jef` (SPEC 2). */
export const NAME_SUFFIX = "";
/** The old suffix, still accepted and removed when typed or pasted. */
export const LEGACY_SUFFIX = ".lez";

/**
 * Turns what a person typed, such as `Jef.lez`, into the bare name `jef`. Only
 * ASCII letters are lowered, so no other character can turn into a valid one.
 * The result still has to pass `isValidName`.
 */
export function normalizeName(input) {
  const lowered = String(input)
    .trim()
    .replace(/[A-Z]/g, (letter) => letter.toLowerCase());
  let bare = lowered.startsWith("lns://")
    ? lowered.slice(6)
    : lowered.startsWith("lns:")
      ? lowered.slice(4)
      : lowered;
  if (bare.startsWith("@")) bare = bare.slice(1);
  return bare.endsWith(LEGACY_SUFFIX) ? bare.slice(0, -LEGACY_SUFFIX.length) : bare;
}

/** A single LNS label: 1–32 chars of `[a-z0-9-]`, no leading/trailing `-`, and no `--`. */
export function isValidLabel(label) {
  return typeof label === "string" && LABEL_PATTERN.test(label) && !label.includes("--");
}

/** Splits a one-level subname `label.parent`, returning `{ label, parent }` or `null`. */
export function splitSubname(name) {
  if (typeof name !== "string" || name.length > MAX_SUBNAME_LEN) return null;
  const dot = name.indexOf(".");
  if (dot < 0 || name.indexOf(".", dot + 1) >= 0) return null;
  const label = name.slice(0, dot);
  const parent = name.slice(dot + 1);
  return isValidLabel(label) && isValidLabel(parent) ? { label, parent } : null;
}

export function isValidName(name) {
  return isValidLabel(name) || splitSubname(name) !== null;
}

/** Length-based pricing tier for a top-level name: 0 (<=5), 1 (6–7), 2 (8+). */
export function priceTierForLen(len) {
  if (len <= 5) return PRICE_TIER_5;
  if (len <= 7) return PRICE_TIER_6_7;
  return PRICE_TIER_8_PLUS;
}

/**
 * The identifier and key of `name`. The chain knows the name only by `id`;
 * `key` opens and seals what the name resolves to and is never sent anywhere.
 * The derivation is slow and memory-hard on purpose, to make scanning for names
 * expensive: about half a second and 64 MiB in pure JavaScript.
 */
export async function deriveName(name) {
  if (!isValidName(name)) throw new Error(`invalid name: ${name}`);
  const master = await argon2idAsync(utf8.encode(name), utf8.encode(DERIVE_SALT), DERIVE_PARAMS);
  return {
    id: await sha256(concat(utf8.encode(ID_DOMAIN), master)),
    key: await sha256(concat(utf8.encode(KEY_DOMAIN), master)),
  };
}

/** On-chain v7/v8 slot identifier for a top-level name, bound to its `priceTier`. */
export async function tieredId(baseId, priceTier) {
  if (!(baseId instanceof Uint8Array) || baseId.length !== 32) throw new Error("name ID must be 32 bytes");
  return sha256(concat(utf8.encode(TIER_DOMAIN), Uint8Array.of(priceTier & 0xff), baseId));
}

async function pdaAddress(program, seed) {
  return base58Encode(await sha256(concat(PDA_PREFIX, accountIdBytes(program), seed)));
}

async function derivedAddress(program, domain, id) {
  if (!(id instanceof Uint8Array) || id.length !== 32) throw new Error("name ID must be 32 bytes");
  const seed = await sha256(concat(utf8.encode(domain), id));
  return pdaAddress(program, seed);
}

/** Account holding the record of the name `id` under the registry `program`. */
export function recordAddress(program, id) {
  return derivedAddress(program, NAME_SEED_DOMAIN, id);
}

/** Account holding the page of the name `id` under the registry `program`. */
export function pageAddress(program, id) {
  return derivedAddress(program, PAGE_SEED_DOMAIN, id);
}

/** Account holding the registry's settings under `program`. */
export async function settingsAddress(program) {
  return pdaAddress(program, await sha256(utf8.encode(SETTINGS_SEED_DOMAIN)));
}

/** Account holding a subname's record under `program`. */
export async function subnameRecordAddress(program, parentSlotId, subId) {
  if (!(parentSlotId instanceof Uint8Array) || parentSlotId.length !== 32) throw new Error("parent ID must be 32 bytes");
  if (!(subId instanceof Uint8Array) || subId.length !== 32) throw new Error("subname ID must be 32 bytes");
  const seed = await sha256(concat(utf8.encode(SUBNAME_SEED_DOMAIN), parentSlotId, subId));
  return pdaAddress(program, seed);
}

/** Account holding a subname's page under `program`. */
export async function subnamePageAddress(program, parentSlotId, subId) {
  if (!(parentSlotId instanceof Uint8Array) || parentSlotId.length !== 32) throw new Error("parent ID must be 32 bytes");
  if (!(subId instanceof Uint8Array) || subId.length !== 32) throw new Error("subname ID must be 32 bytes");
  const seed = await sha256(concat(utf8.encode(SUBPAGE_SEED_DOMAIN), parentSlotId, subId));
  return pdaAddress(program, seed);
}

/** Account holding marketplace bucket `bucket` (0..15). */
export async function marketBucketAddress(program, bucket) {
  const seed = await sha256(concat(utf8.encode(MARKET_BUCKET_DOMAIN), Uint8Array.of(bucket & 0xff)));
  return pdaAddress(program, seed);
}

/** Account holding the site directory header (`SiteDirHeader`). */
export async function siteDirHeaderAddress(program) {
  return pdaAddress(program, await sha256(utf8.encode(SITE_DIR_HEADER_DOMAIN)));
}

/** Account holding site directory bucket `bucket` (0..15) page `page` (u16). */
export async function siteBucketAddress(program, bucket, page) {
  const seed = await sha256(
    concat(utf8.encode(SITE_BUCKET_DOMAIN), Uint8Array.of(bucket & 0xff, page & 0xff, (page >>> 8) & 0xff)),
  );
  return pdaAddress(program, seed);
}

function sealingKey(key, usage) {
  return crypto.subtle.importKey("raw", key, "AES-GCM", false, [usage]);
}

function sealingParams(nonce, kind) {
  const associated = ASSOCIATED_DATA[kind];
  if (!associated) throw new Error(`unknown kind of sealed data: ${kind}`);
  return { name: "AES-GCM", iv: nonce, additionalData: utf8.encode(associated) };
}

/**
 * Seals `plaintext` under a name's `key` as a "record" or a "page": the nonce,
 * then the ciphertext and its tag. The nonce is random; only tests pass one.
 */
export async function seal(key, plaintext, kind, nonce = null) {
  nonce ??= crypto.getRandomValues(new Uint8Array(NONCE_LENGTH));
  if (nonce.length !== NONCE_LENGTH) throw new Error(`nonce must be ${NONCE_LENGTH} bytes`);
  const params = sealingParams(nonce, kind);
  const sealed = await crypto.subtle.encrypt(params, await sealingKey(key, "encrypt"), plaintext);
  return concat(nonce, new Uint8Array(sealed));
}

/** The plaintext of `sealed`, or null when it does not open with `key` as that kind. */
export async function openSealed(key, sealed, kind) {
  const params = sealingParams(sealed.subarray(0, NONCE_LENGTH), kind);
  if (sealed.length < NONCE_LENGTH + TAG_LENGTH) return null;
  try {
    const opening = await sealingKey(key, "decrypt");
    const ciphertext = sealed.subarray(NONCE_LENGTH);
    return new Uint8Array(await crypto.subtle.decrypt(params, opening, ciphertext));
  } catch {
    return null;
  }
}

/** Minimal borsh reader for the registry's layouts. */
class Reader {
  constructor(bytes) {
    this.bytes = bytes;
    this.offset = 0;
  }
  take(length) {
    if (this.offset + length > this.bytes.length) throw new Error("record is truncated");
    const slice = this.bytes.subarray(this.offset, this.offset + length);
    this.offset += length;
    return slice;
  }
  u8() {
    return this.take(1)[0];
  }
  u16() {
    const [a, b] = this.take(2);
    return a | (b << 8);
  }
  u32() {
    const [a, b, c, d] = this.take(4);
    return (a | (b << 8) | (c << 16) | (d << 24)) >>> 0;
  }
  u64() {
    const low = this.u32();
    const high = this.u32();
    return high * 2 ** 32 + low;
  }
  u128() {
    return Array.from(this.take(16)).reduceRight((value, byte) => (value << 8n) | BigInt(byte), 0n);
  }
  bool() {
    const value = this.u8();
    if (value > 1) throw new Error("not a boolean");
    return value === 1;
  }
  byteVec() {
    return this.take(this.u32());
  }
  string() {
    return new TextDecoder("utf-8", { fatal: true }).decode(this.byteVec());
  }
  option(read) {
    return this.u8() === 0 ? null : read();
  }
}

/** Minimal borsh writers, the counterparts of the reader. */
const writeU32 = (value) => Uint8Array.of(value, value >>> 8, value >>> 16, value >>> 24);
const writeByteVec = (bytes) => concat(writeU32(bytes.length), bytes);
const writeString = (text) => writeByteVec(utf8.encode(text));
const writeOption = (bytes) =>
  bytes === null ? Uint8Array.of(0) : concat(Uint8Array.of(1), bytes);

function fixed32(bytes, what) {
  if (bytes.length !== 32) throw new Error(`${what} must be 32 bytes`);
  return bytes;
}

/**
 * The tag that stands for the account `accountId` as the holder of the name
 * `id` (32 bytes). A record keeps this in place of its holder, so it names no
 * account and links no two names.
 */
export async function ownerTag(id, accountId) {
  return toHex(await sha256(concat(utf8.encode("logos-names/v5/owner"), id, base58Decode(accountId))));
}

function decodeSubnameSale(reader) {
  return {
    price: reader.u128(),
    payout: base58Encode(reader.take(32)),
  };
}

function decodeListing(reader) {
  return {
    price: reader.u128(),
    payTo: base58Encode(reader.take(32)),
    expiresAt: reader.option(() => reader.u64()),
    nonce: reader.u64(),
    inMarket: reader.bool(),
  };
}

/**
 * Decodes a top-level `NameRecord` (versions 3–8).
 * - v6 adds `claimed`.
 * - v7 adds `priceTier`, `generation`, `subnameSale`.
 * - v8 adds `nftTokenId`, `nftShielded`, `listing`, `listingNonce`, `siteListed`.
 */
export function decodeNameRecord(bytes) {
  const reader = new Reader(bytes);
  const version = reader.u8();
  const base = {
    version,
    ownerTag: toHex(reader.take(32)),
    expires: reader.u64(),
    sealed: reader.byteVec(),
    claimed: version >= 6 ? reader.bool() : false,
  };
  if (version < 7) return base;
  const v7 = {
    ...base,
    priceTier: reader.u8(),
    generation: toHex(reader.take(32)),
    subnameSale: reader.option(() => decodeSubnameSale(reader)),
  };
  if (version < 8) return v7;
  return {
    ...v7,
    nftTokenId: toHex(reader.take(32)),
    nftShielded: reader.bool(),
    listing: reader.option(() => decodeListing(reader)),
    listingNonce: reader.u64(),
    siteListed: reader.option(() => reader.u16()),
  };
}

/** Decodes a v7/v8 `SubnameRecord`. */
export function decodeSubnameRecord(bytes) {
  const reader = new Reader(bytes);
  const version = reader.u8();
  const ownerTagHex = toHex(reader.take(32));
  const kindTag = reader.u8();
  let kind;
  if (kindTag === 0) {
    kind = { type: "permanent" };
  } else if (kindTag === 1) {
    kind = { type: "revocable", parentGeneration: toHex(reader.take(32)) };
  } else {
    throw new Error(`unknown subname kind: ${kindTag}`);
  }
  return { version, ownerTag: ownerTagHex, kind, sealed: reader.byteVec() };
}

/**
 * Decodes the registry's settings (versions 5–8).
 * - v5–v6: `prices` is `[u128; 3]`.
 * - v7–v8: `prices` is `[[u128; 4]; 3]` (`[priceTier][term]`).
 * - v8 adds `premiumStart` and `nftProgram`.
 */
export function decodeSettings(bytes) {
  const reader = new Reader(bytes);
  const version = reader.u8();
  const admin = base58Encode(reader.take(32));
  const treasury = base58Encode(reader.take(32));
  const prices =
    version >= 7
      ? Array.from({ length: 3 }, () => [reader.u128(), reader.u128(), reader.u128(), reader.u128()])
      : [reader.u128(), reader.u128(), reader.u128()];
  const mode = reader.u8();
  const deposits = [reader.u128(), reader.u128()];
  const settings = { version, admin, treasury, prices, mode, deposits };
  if (version < 6) {
    return { ...settings, claimsRoot: null, claimsUntil: 0, claimsProgram: null };
  }
  const v6 = {
    ...settings,
    claimsRoot: toHex(reader.take(32)),
    claimsUntil: reader.u64(),
    claimsProgram: base58Encode(reader.take(32)),
  };
  if (version < 8) return v6;
  return {
    ...v6,
    premiumStart: reader.u128(),
    nftProgram: base58Encode(reader.take(32)),
  };
}

/** Decodes a v8 `MarketBucket` (`version || Vec<MarketEntry>`). */
export function decodeMarketBucket(bytes) {
  const reader = new Reader(bytes);
  const version = reader.u8();
  const count = reader.u32();
  const entries = [];
  for (let i = 0; i < count; i += 1) {
    entries.push({
      id: toHex(reader.take(32)),
      price: reader.u128(),
      payTo: base58Encode(reader.take(32)),
      expiresAt: reader.option(() => reader.u64()),
      nameExpires: reader.u64(),
      priceTier: reader.u8(),
      nonce: reader.u64(),
      nftTokenId: toHex(reader.take(32)),
      displayName: reader.option(() => reader.string()),
    });
  }
  return { version, entries };
}

/** Decodes a v8 `SiteDirHeader` (`version || [u16; 16] || u32`). */
export function decodeSiteDirHeader(bytes) {
  const reader = new Reader(bytes);
  const version = reader.u8();
  const activePage = Array.from({ length: SITE_BUCKETS }, () => reader.u16());
  const totalListed = reader.u32();
  return { version, activePage, totalListed };
}

/** Decodes a v8 `SiteBucket` (`version || Vec<SiteEntry>`). */
export function decodeSiteBucket(bytes) {
  const reader = new Reader(bytes);
  const version = reader.u8();
  const count = reader.u32();
  const entries = [];
  for (let i = 0; i < count; i += 1) {
    entries.push({
      id: toHex(reader.take(32)),
      name: reader.string(),
      title: reader.string(),
      updatedAt: reader.u64(),
      nameExpires: reader.u64(),
    });
  }
  return { version, entries };
}

export function decodePage(bytes) {
  const reader = new Reader(bytes);
  return { version: reader.u8(), sealed: reader.byteVec() };
}

export function decodePrimaryName(bytes) {
  const reader = new Reader(bytes);
  return { version: reader.u8(), name: reader.string() };
}

/** Decodes the profile at the start of an opened record; the zero padding after it is ignored. */
export function decodeProfile(bytes) {
  const reader = new Reader(bytes);
  const profile = {
    name: reader.string(),
    account: reader.option(() => base58Encode(reader.take(32))),
    npk: reader.option(() => toHex(reader.take(32))),
    vpk: toHex(reader.byteVec()) || null,
    texts: {},
  };
  const count = reader.u32();
  for (let index = 0; index < count; index += 1) {
    const key = reader.string();
    profile.texts[key] = reader.string();
  }
  return profile;
}

/** Encodes a profile, padded with zeros to a whole number of blocks so its length says little. */
export function encodeProfile({ name, account = null, npk = null, vpk = null, texts = {} }) {
  const entries = Object.entries(texts);
  const encoded = concat(
    writeString(name),
    writeOption(account === null ? null : accountIdBytes(account)),
    writeOption(npk === null ? null : fixed32(fromHex(npk), "npk")),
    writeByteVec(vpk === null ? new Uint8Array(0) : fromHex(vpk)),
    writeU32(entries.length),
    ...entries.flatMap(([key, value]) => [writeString(key), writeString(value)]),
  );
  const padded = new Uint8Array(Math.ceil(encoded.length / PROFILE_BLOCK) * PROFILE_BLOCK);
  padded.set(encoded);
  return padded;
}

export function decodePageContent(bytes) {
  const reader = new Reader(bytes);
  return { contentType: reader.string(), body: reader.byteVec() };
}

export function encodePageContent({ contentType, body }) {
  return concat(writeString(contentType), writeByteVec(body));
}

/**
 * Reads what `program` keeps on the account `accountId`: its actor state, empty when
 * there is none. `getAccountView` returns null for an account that isn't on the chain,
 * and otherwise that one actor's state, as a list of bytes keyed by the program. A LEZ
 * 0.3 sequencer answers the same call with the state under `shards`.
 */
export async function readActorState(rpcUrl, program, accountId) {
  const response = await fetch(rpcUrl, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      jsonrpc: "2.0",
      id: 1,
      method: "getAccountView",
      params: [{ account_id: accountId, program_account_id: program }],
    }),
  });
  if (!response.ok) throw new Error(`sequencer returned HTTP ${response.status}`);
  const { result, error } = await response.json();
  if (error) throw new Error(`sequencer error: ${error.message}`);
  const data = result?.data;
  return Uint8Array.from(data?.actor_states?.[program] ?? data?.shards?.[program] ?? []);
}

/** The old name of `readActorState`, kept for existing callers. */
export const readShard = readActorState;

/** Opens and decodes `sealed`, or null when it is empty, does not open or does not decode. */
async function openAs(key, sealed, kind, decode) {
  if (sealed.length === 0) return null;
  const plaintext = await openSealed(key, sealed, kind);
  if (plaintext === null) return null;
  try {
    return decode(plaintext);
  } catch {
    return null;
  }
}

/** The record of the name slot `slotId`, or null when it is not registered or is past grace at `now`. */
async function readRecord(rpcUrl, program, slotId, now) {
  const bytes = await readActorState(rpcUrl, program, await recordAddress(program, slotId));
  if (bytes.length === 0) return null;
  const record = decodeNameRecord(bytes);
  return now >= record.expires + GRACE_MS ? null : record;
}

/**
 * Reads a top-level name's raw on-chain `NameRecord` (including past-grace records so callers
 * can inspect re-release premium, listing state, NFT token ID, and site directory page).
 */
export async function readRawNameRecord(rpcUrl, program, name) {
  if (!isValidLabel(name)) throw new Error(`not a top-level name: ${name}`);
  const { id: baseId, key } = await deriveName(name);
  const priceTier = priceTierForLen(name.length);
  const slotId = await tieredId(baseId, priceTier);
  const bytes = await readActorState(rpcUrl, program, await recordAddress(program, slotId));
  const record = bytes.length === 0 ? null : decodeNameRecord(bytes);
  return { baseId, slotId, key, priceTier, record };
}

/** Reads an active subname record, checking parent liveness + generation for revocable subnames. */
async function readActiveSubname(rpcUrl, program, name, split, now) {
  const { id: parentBaseId } = await deriveName(split.parent);
  const parentSlotId = await tieredId(parentBaseId, priceTierForLen(split.parent.length));
  const { id: subId, key } = await deriveName(name);
  const bytes = await readActorState(rpcUrl, program, await subnameRecordAddress(program, parentSlotId, subId));
  if (bytes.length === 0) return null;
  const sub = decodeSubnameRecord(bytes);
  let expires = null;
  if (sub.kind.type === "revocable") {
    const parentRecord = await readRecord(rpcUrl, program, parentSlotId, now);
    if (parentRecord === null || parentRecord.generation !== sub.kind.parentGeneration) return null;
    expires = parentRecord.expires >= Number.MAX_SAFE_INTEGER ? null : parentRecord.expires;
  }
  return { parentSlotId, subId, key, sub, expires };
}

/**
 * What `name` resolves to at the time `now`, or null when it is not registered
 * or is past its grace period. Supports both top-level names (using v7/v8 `tieredId`)
 * and one-level subnames (`label.parent`). `expires` is in milliseconds since the Unix
 * epoch (or null for permanent/deposit names); `lapsed` is true for a name that has
 * expired and is in its grace period, which still resolves.
 */
export async function resolveName(rpcUrl, program, name, now = Date.now()) {
  if (!isValidName(name)) throw new Error(`invalid name: ${name}`);
  const split = splitSubname(name);
  let key;
  let sealed;
  let expires;
  if (split === null) {
    const { id: baseId, key: k } = await deriveName(name);
    const slotId = await tieredId(baseId, priceTierForLen(name.length));
    const record = await readRecord(rpcUrl, program, slotId, now);
    if (record === null) return null;
    key = k;
    sealed = record.sealed;
    expires = record.expires >= Number.MAX_SAFE_INTEGER ? null : record.expires;
  } else {
    const active = await readActiveSubname(rpcUrl, program, name, split, now);
    if (active === null) return null;
    key = active.key;
    sealed = active.sub.sealed;
    expires = active.expires;
  }
  const held = { name, expires, lapsed: expires !== null && expires <= now };
  const profile = await openAs(key, sealed, "record", decodeProfile);
  if (profile === null || profile.name !== name) {
    return { ...held, account: null, npk: null, vpk: null, texts: {} };
  }
  const { account, npk, vpk, texts } = profile;
  return { ...held, account, npk, vpk, texts };
}

/** The text entry that points a name to another (SPEC.md §7, Forwarding). */
export const FORWARD_KEY = "forward";

/**
 * Where a resolved record forwards to: its `forward` entry, normalised, if that
 * is a valid name other than the record's own; otherwise null.
 */
export function forwardOf(record) {
  const value = record && record.texts ? record.texts[FORWARD_KEY] : undefined;
  if (typeof value !== "string") return null;
  const target = normalizeName(value);
  return isValidName(target) && target !== record.name ? target : null;
}

/**
 * Resolves `name` for using it (paying, messaging, showing it): when it forwards,
 * the record of the name it points to, one hop, with `forwardedFrom` set to the
 * name asked for, which a payer must be shown. Null when `name` isn't
 * registered; throws when it points to a name that isn't. Changing a name never
 * follows a forward: use `resolveName` for that.
 */
export async function resolveUsing(rpcUrl, program, name, now = Date.now()) {
  const record = await resolveName(rpcUrl, program, name, now);
  const target = forwardOf(record);
  if (target === null) return record;
  const forwarded = await resolveName(rpcUrl, program, target, now);
  if (forwarded === null) throw new Error(`${name} points to ${target}, which isn't registered`);
  return { ...forwarded, forwardedFrom: name };
}

/**
 * What a name shows people: whether it can be paid privately, and the public
 * account only if its holder published one. The private receiving keys are
 * never shown; a payer's wallet uses them from `resolveName`.
 */
export function publicView(record) {
  const { name, expires, lapsed, account, npk, vpk, texts } = record;
  return {
    name,
    expires,
    lapsed,
    payablePrivately: Boolean(npk && vpk),
    // Only if the holder chose to publish one.
    publicAccount: account,
    texts,
  };
}

/**
 * The page for `name` at the time `now`, or null when the name is not
 * registered or is past its grace period, has no page, or the page does not
 * open with the name's key.
 */
export async function resolvePage(rpcUrl, program, name, now = Date.now()) {
  if (!isValidName(name)) throw new Error(`invalid name: ${name}`);
  const split = splitSubname(name);
  if (split === null) {
    const { id: baseId, key } = await deriveName(name);
    const slotId = await tieredId(baseId, priceTierForLen(name.length));
    if ((await readRecord(rpcUrl, program, slotId, now)) === null) return null;
    const bytes = await readActorState(rpcUrl, program, await pageAddress(program, slotId));
    if (bytes.length === 0) return null;
    return openAs(key, decodePage(bytes).sealed, "page", decodePageContent);
  }
  const active = await readActiveSubname(rpcUrl, program, name, split, now);
  if (active === null) return null;
  const bytes = await readActorState(rpcUrl, program, await subnamePageAddress(program, active.parentSlotId, active.subId));
  if (bytes.length === 0) return null;
  return openAs(active.key, decodePage(bytes).sealed, "page", decodePageContent);
}

/**
 * The name `accountId` is shown as at the time `now`, or null. A reverse record
 * is only a claim, so it counts only when the forward record points back at the
 * same account.
 */
export async function reverseLookup(rpcUrl, program, accountId, now = Date.now()) {
  const bytes = await readActorState(rpcUrl, program, accountId);
  if (bytes.length === 0) return null;
  const { name } = decodePrimaryName(bytes);
  if (!isValidName(name)) return null;
  const split = splitSubname(name);
  if (split === null) {
    const { id: baseId, key } = await deriveName(name);
    const slotId = await tieredId(baseId, priceTierForLen(name.length));
    const record = await readRecord(rpcUrl, program, slotId, now);
    if (record === null) return null;
    if (record.ownerTag === (await ownerTag(slotId, accountId))) return name;
    const profile = await openAs(key, record.sealed, "record", decodeProfile);
    return profile !== null && profile.name === name && profile.account === accountId ? name : null;
  }
  const active = await readActiveSubname(rpcUrl, program, name, split, now);
  if (active === null) return null;
  if (active.sub.ownerTag === (await ownerTag(active.subId, accountId))) return name;
  const profile = await openAs(active.key, active.sub.sealed, "record", decodeProfile);
  return profile !== null && profile.name === name && profile.account === accountId ? name : null;
}
