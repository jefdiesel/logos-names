// Logos Names resolver: derives a name's accounts and reads them over the LEZ
// sequencer's JSON-RPC. No dependencies; runs in Node 20+, browsers and
// Cloudflare Workers (anything with fetch and WebCrypto).

const utf8 = new TextEncoder();

const BASE58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
const PDA_PREFIX = pad32("/LEE/v0.2/AccountId/PDA/");
const NAME_SEED_DOMAIN = "logos-names/v1/name/";
const PAGE_SEED_DOMAIN = "logos-names/v1/page/";
const DIRECTORY_SEED = "logos-names/v1/directory";
const NAME_PATTERN = /^(?!-)[a-z0-9-]{1,32}(?<!-)$/;

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

export function isValidName(name) {
  return typeof name === "string" && NAME_PATTERN.test(name);
}

async function derivedAddress(program, domain, name) {
  if (!isValidName(name)) throw new Error(`invalid name: ${name}`);
  const seed = await sha256(utf8.encode(domain + name));
  return base58Encode(await sha256(concat(PDA_PREFIX, accountIdBytes(program), seed)));
}

/** Account holding the record for `name` under the registry `program`. */
export function recordAddress(program, name) {
  return derivedAddress(program, NAME_SEED_DOMAIN, name);
}

/** Account holding the page for `name` under the registry `program`. */
export function pageAddress(program, name) {
  return derivedAddress(program, PAGE_SEED_DOMAIN, name);
}

/** Account holding the directory: the head of the list of all names. */
export async function directoryAddress(program) {
  const seed = await sha256(utf8.encode(DIRECTORY_SEED));
  return base58Encode(await sha256(concat(PDA_PREFIX, accountIdBytes(program), seed)));
}

/** Minimal borsh reader for the registry's record layouts. */
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
    return Number((BigInt(this.u32()) << 32n) | BigInt(low));
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

const toHex = (bytes) => Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");

export function decodeNameRecord(bytes) {
  const reader = new Reader(bytes);
  const record = {
    version: reader.u8(),
    name: reader.string(),
    owner: base58Encode(reader.take(32)),
    account: reader.option(() => base58Encode(reader.take(32))),
    npk: reader.option(() => toHex(reader.take(32))),
    vpk: toHex(reader.byteVec()) || null,
    texts: {},
  };
  const count = reader.u32();
  for (let index = 0; index < count; index += 1) {
    const key = reader.string();
    record.texts[key] = reader.string();
  }
  record.previous = reader.option(() => reader.string());
  return record;
}

export function decodeDirectory(bytes) {
  const reader = new Reader(bytes);
  return {
    version: reader.u8(),
    count: reader.u64(),
    latest: reader.option(() => reader.string()),
  };
}

export function decodePage(bytes) {
  const reader = new Reader(bytes);
  return { version: reader.u8(), contentType: reader.string(), body: reader.byteVec() };
}

export function decodePrimaryName(bytes) {
  const reader = new Reader(bytes);
  return { version: reader.u8(), name: reader.string() };
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

/** The record for `name`, or null when it is not registered. */
export async function resolveName(rpcUrl, program, name) {
  const bytes = await readShard(rpcUrl, program, await recordAddress(program, name));
  return bytes.length === 0 ? null : decodeNameRecord(bytes);
}

/** The page for `name`, or null when it has none. */
export async function resolvePage(rpcUrl, program, name) {
  const bytes = await readShard(rpcUrl, program, await pageAddress(program, name));
  return bytes.length === 0 ? null : decodePage(bytes);
}

/**
 * The name `accountId` is shown as, or null. A reverse record is only a claim,
 * so it counts only when the forward record points back at the same account.
 */
export async function reverseLookup(rpcUrl, program, accountId) {
  const bytes = await readShard(rpcUrl, program, accountId);
  if (bytes.length === 0) return null;
  const { name } = decodePrimaryName(bytes);
  if (!isValidName(name)) return null;
  const record = await resolveName(rpcUrl, program, name);
  const confirmed = record && (record.owner === accountId || record.account === accountId);
  return confirmed ? name : null;
}

/**
 * Lists registered names, newest first, by walking the on-chain list back from
 * the directory. Costs one read per name, so `limit` bounds the work; `before`
 * continues an earlier listing from the name it ended on.
 */
export async function listNames(rpcUrl, program, { limit = 50, before = null } = {}) {
  const bytes = await readShard(rpcUrl, program, await directoryAddress(program));
  const directory = bytes.length === 0 ? { count: 0, latest: null } : decodeDirectory(bytes);

  let next = directory.latest;
  if (before !== null) {
    const record = await resolveName(rpcUrl, program, before);
    if (!record) throw new Error(`${before} is not registered`);
    next = record.previous;
  }

  const names = [];
  while (next !== null && names.length < limit) {
    const record = await resolveName(rpcUrl, program, next);
    if (!record) throw new Error(`the name list is broken at ${next}`);
    names.push({ name: record.name, owner: record.owner });
    next = record.previous;
  }
  return { total: directory.count, names, more: next !== null };
}
