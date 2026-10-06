// Logos Names resolver: derives a name's identifier, key and accounts, reads
// them over the LEZ sequencer's JSON-RPC and opens what they hold. No
// dependencies; runs in Node 20+, browsers and Cloudflare Workers (anything
// with fetch and WebCrypto).

const utf8 = new TextEncoder();

const BASE58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
const PDA_PREFIX = pad32("/LEE/v0.2/AccountId/PDA/");
const DERIVE_SALT = "logos-names/v2";
const DERIVE_ITERATIONS = 100000;
const ID_DOMAIN = "logos-names/v2/id";
const KEY_DOMAIN = "logos-names/v2/key";
const NAME_SEED_DOMAIN = "logos-names/v2/name/";
const PAGE_SEED_DOMAIN = "logos-names/v2/page/";
// Sealed data is bound to its kind, so a sealed page never opens as a record.
const ASSOCIATED_DATA = { record: "logos-names/v2/record", page: "logos-names/v2/page" };
const NONCE_LENGTH = 12;
const TAG_LENGTH = 16;
const PROFILE_BLOCK = 256;
const NAME_PATTERN = /^(?!-)[a-z0-9-]{1,32}(?<!-)$/;

/** How long an expired name still resolves, in milliseconds: 30 days. */
export const GRACE_MS = 2_592_000_000;

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

/** How names are written for people: `jef.lez`. The suffix is not part of the name. */
export const NAME_SUFFIX = ".lez";

/**
 * Turns what a person typed, such as `Jef.lez`, into the bare name `jef`. Only
 * ASCII letters are lowered, so no other character can turn into a valid one.
 * The result still has to pass `isValidName`.
 */
export function normalizeName(input) {
  const lowered = String(input)
    .trim()
    .replace(/[A-Z]/g, (letter) => letter.toLowerCase());
  return lowered.endsWith(NAME_SUFFIX) ? lowered.slice(0, -NAME_SUFFIX.length) : lowered;
}

export function isValidName(name) {
  return typeof name === "string" && NAME_PATTERN.test(name);
}

/**
 * The identifier and key of `name`. The chain knows the name only by `id`;
 * `key` opens and seals what the name resolves to and is never sent anywhere.
 * The derivation is slow on purpose, to make scanning for names expensive.
 */
export async function deriveName(name) {
  if (!isValidName(name)) throw new Error(`invalid name: ${name}`);
  const usages = ["deriveBits"];
  const password = await crypto.subtle.importKey("raw", utf8.encode(name), "PBKDF2", false, usages);
  const params = {
    name: "PBKDF2",
    hash: "SHA-256",
    salt: utf8.encode(DERIVE_SALT),
    iterations: DERIVE_ITERATIONS,
  };
  const master = new Uint8Array(await crypto.subtle.deriveBits(params, password, 256));
  return {
    id: await sha256(concat(utf8.encode(ID_DOMAIN), master)),
    key: await sha256(concat(utf8.encode(KEY_DOMAIN), master)),
  };
}

async function derivedAddress(program, domain, id) {
  if (!(id instanceof Uint8Array) || id.length !== 32) throw new Error("name ID must be 32 bytes");
  const seed = await sha256(concat(utf8.encode(domain), id));
  return base58Encode(await sha256(concat(PDA_PREFIX, accountIdBytes(program), seed)));
}

/** Account holding the record of the name `id` under the registry `program`. */
export function recordAddress(program, id) {
  return derivedAddress(program, NAME_SEED_DOMAIN, id);
}

/** Account holding the page of the name `id` under the registry `program`. */
export function pageAddress(program, id) {
  return derivedAddress(program, PAGE_SEED_DOMAIN, id);
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
  u32() {
    const [a, b, c, d] = this.take(4);
    return (a | (b << 8) | (c << 16) | (d << 24)) >>> 0;
  }
  u64() {
    const low = this.u32();
    const high = this.u32();
    return high * 2 ** 32 + low;
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

export function decodeNameRecord(bytes) {
  const reader = new Reader(bytes);
  return {
    version: reader.u8(),
    owner: base58Encode(reader.take(32)),
    expires: reader.u64(),
    sealed: reader.byteVec(),
  };
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

/** Reads the registry's shard on `accountId`; empty when the account holds none. */
export async function readShard(rpcUrl, program, accountId) {
  const response = await fetch(rpcUrl, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "getAccount", params: [accountId] }),
  });
  if (!response.ok) throw new Error(`sequencer returned HTTP ${response.status}`);
  const { result, error } = await response.json();
  if (error) throw new Error(`sequencer error: ${error.message}`);
  return Uint8Array.from(result?.data?.shards?.[program] ?? []);
}

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

/** The record of the name `id`, or null when it is not registered or is past grace at `now`. */
async function readRecord(rpcUrl, program, id, now) {
  const bytes = await readShard(rpcUrl, program, await recordAddress(program, id));
  if (bytes.length === 0) return null;
  const record = decodeNameRecord(bytes);
  return now >= record.expires + GRACE_MS ? null : record;
}

/**
 * What `name` resolves to at the time `now`, or null when it is not registered
 * or is past its grace period. `expires` is in milliseconds since the Unix
 * epoch; `lapsed` is true for a name that has expired and is in its grace
 * period, which still resolves. A record whose sealed profile is empty, does
 * not open with the name's key or names another name is a registered name with
 * nothing set.
 */
export async function resolveName(rpcUrl, program, name, now = Date.now()) {
  const { id, key } = await deriveName(name);
  const record = await readRecord(rpcUrl, program, id, now);
  if (record === null) return null;
  const { owner, expires, sealed } = record;
  const held = { name, owner, expires, lapsed: expires <= now };
  const profile = await openAs(key, sealed, "record", decodeProfile);
  if (profile === null || profile.name !== name) {
    return { ...held, account: null, npk: null, vpk: null, texts: {} };
  }
  const { account, npk, vpk, texts } = profile;
  return { ...held, account, npk, vpk, texts };
}

/**
 * The page for `name` at the time `now`, or null when the name is not
 * registered or is past its grace period, has no page, or the page does not
 * open with the name's key.
 */
export async function resolvePage(rpcUrl, program, name, now = Date.now()) {
  const { id, key } = await deriveName(name);
  if ((await readRecord(rpcUrl, program, id, now)) === null) return null;
  const bytes = await readShard(rpcUrl, program, await pageAddress(program, id));
  if (bytes.length === 0) return null;
  return openAs(key, decodePage(bytes).sealed, "page", decodePageContent);
}

/**
 * The name `accountId` is shown as at the time `now`, or null. A reverse record
 * is only a claim, so it counts only when the forward record points back at the
 * same account.
 */
export async function reverseLookup(rpcUrl, program, accountId, now = Date.now()) {
  const bytes = await readShard(rpcUrl, program, accountId);
  if (bytes.length === 0) return null;
  const { name } = decodePrimaryName(bytes);
  if (!isValidName(name)) return null;
  const record = await resolveName(rpcUrl, program, name, now);
  const confirmed = record && (record.owner === accountId || record.account === accountId);
  return confirmed ? name : null;
}
