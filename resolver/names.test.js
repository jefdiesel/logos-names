// Run with: node --test resolver/names.test.js
import assert from "node:assert/strict";
import { afterEach, test } from "node:test";
import { argon2id } from "@noble/hashes/argon2.js";
import {
  GRACE_MS,
  base58Decode,
  base58Encode,
  decodeNameRecord,
  decodePage,
  decodePageContent,
  decodePrimaryName,
  decodeProfile,
  deriveName,
  encodePageContent,
  encodeProfile,
  isValidName,
  normalizeName,
  ownerTag,
  openSealed,
  pageAddress,
  recordAddress,
  resolveName,
  resolvePage,
  reverseLookup,
  seal,
} from "./names.js";

const RPC_URL = "http://sequencer.test";
const PROGRAM = base58Encode(new Uint8Array(32).fill(7));
const OWNER = base58Encode(new Uint8Array(32).fill(1));
const ACCOUNT = base58Encode(new Uint8Array(32).fill(2));
const STRANGER = base58Encode(new Uint8Array(32).fill(3));
const NONCE = Uint8Array.from({ length: 12 }, (_, index) => index);
const COUNTING_ID = Uint8Array.from({ length: 32 }, (_, index) => index);
// When the fixtures' names expire: far ahead of the real clock, for the tests that do not pass `now`.
const EXPIRES = Date.UTC(2100, 0, 1);

const toHex = (bytes) => Buffer.from(bytes).toString("hex");

// The vectors of SPEC.md section 10. They are asserted in seal/src/lib.rs too,
// so Rust and JavaScript must agree.
const JEF_ID = "7041d9e447f06bdfe327d870402b1524cfe38bd03445b8fc3d0e87bc978eb763";
const JEF_KEY = "aad91a1a3dd0fb56a213212fbea58fe25d9139db42fc59ce2116c3cfa3440088";
// [name, id, key]; the masters are in the Rust test.
const DERIVATIONS = [
  ["jef", JEF_ID, JEF_KEY],
  ["logos", "4a8bf61b00dbc63907a935aa15ed534bb32d866978aa1f2128f9b6f37e0c4025", "d67dd628fa51fa62bd32ea82c3e9ff1b67f93b01103be956eaebc52f1699df44"],
  ["a", "24b73f0b70605c7f27fb91f9952b550e13c4cc38dcf325be45442d1a2cb085b4", "cd9ded7c94d52743db4ee67152766480be5bb5199e83b6696e502cf818933150"],
  ["abcdefghijklmnopqrstuvwxyz012345", "0044f00be946536689d83e364ac25462c65c615d74059652d1f3e2977bd75403", "79c7aa8c06d4983a59b235915ae4f5767918824a9150d5d8b576761b7d3654f6"],
  ["a-b-c", "707decf4fde6d9fb61d255c23b34e042c846a0f4b0834151dcb2c1ef4160e849", "e3666a8572f7cabe94f6717491bff34c70eeca216436a142185bd8a86b6ccf31"],
];
const PROFILE = { name: "jef", account: ACCOUNT, npk: null, vpk: null, texts: { chat: "hi" } };
const SEALED_PROFILE =
  "000102030405060708090a0b7a39f7ff67b20894263a28d040c0f2711c04ba247fcf5cb107e27b59" +
  "05b8a0664b28cd25512addcb74af3efa5a8d6eb3bbb8e89fb8dba0aa2739252efed2be01ab019998" +
  "a07d416c788d8c9b699891e85bedd1118da7089377220bdb0ab8ae29661bf20e006c5fbffb222237" +
  "2ea0a2589ed49023186a77e9afee1af338f4a2377afa40da018789ca715318d981b556f71f9640eb" +
  "d056d8dcbe2b71845b45a578721ccc2c889df37b926c7da5a5202dbf3d50dc04883673134ce0772f" +
  "bcd7fbc8f31f455bb39b3d867d5e6772c86e59f42a9d04ffaacaa6119b1156d238665c0cf50028a9" +
  "cbf8b3189fc45a15a724c5775a835a8ccdff6a573d8e046d78e847b7da5668bc8529aedb3bdc70a9" +
  "9db9b9b4";
const PAGE = { contentType: "text/plain", body: new TextEncoder().encode("hi") };
const SEALED_PAGE =
  "000102030405060708090a0b7339f7ff79b216e10b4846b32bacf2731e06d04f8cf43af535da58e9" +
  "92f033cb16a471d0";

const jef = await deriveName("jef");
/** What jef's record keeps for its holder `OWNER`. */
const OWNER_TAG = await ownerTag(jef.id, OWNER);

test("a name derives its identifier and key", async () => {
  assert.equal(toHex(jef.id), JEF_ID);
  assert.equal(toHex(jef.key), JEF_KEY);
  for (const [name, id, key] of DERIVATIONS) {
    const derived = await deriveName(name);
    assert.equal(toHex(derived.id), id, name);
    assert.equal(toHex(derived.key), key, name);
  }
  await assert.rejects(deriveName("Not Valid"), /invalid name/);
  await assert.rejects(deriveName("jef.lez"), /invalid name/);
});

// RFC 9106 section 5.3 (Argon2id, version 0x13, with a secret and associated
// data): checks the Argon2id implementation itself, as the Rust test does.
test("Argon2id matches RFC 9106", () => {
  const tag = argon2id(new Uint8Array(32).fill(1), new Uint8Array(16).fill(2), {
    t: 3,
    m: 32,
    p: 4,
    dkLen: 32,
    version: 0x13,
    key: new Uint8Array(8).fill(3),
    personalization: new Uint8Array(12).fill(4),
  });
  assert.equal(toHex(tag), "0d640df58d78766c08c037a34a8b53c9d01ef0452d75b65eb52520e96b01e659");
});

test("addresses match the Rust implementation", async () => {
  assert.equal(base58Encode(new Uint8Array(32).fill(7)), "US517G5965aydkZ46HS38QLi7UQiSojurfbQfKCELFx");
  assert.equal(await recordAddress(PROGRAM, jef.id), "5b5xVGob2GYvrq21MSuxm1MfieucDEy9yxhDwdXndbpD");
  assert.equal(await pageAddress(PROGRAM, jef.id), "HEES9BpNxrAtwxFkyEQTEnjUcSr7usgbefeqiYJsG9mY");
  assert.equal(await recordAddress(PROGRAM, COUNTING_ID), "7bwwvMLtxqqNYiXsd5bYaq8e31E1odYf6jAK5z7tmUdd");
  assert.equal(await pageAddress(PROGRAM, COUNTING_ID), "8M4vmdxCXL4eS8QXiHSdbeUUPt5Vbx1UVjLcv2cPk9Y5");
  await assert.rejects(recordAddress(PROGRAM, "jef"), /32 bytes/);
});

test("base58 round-trips, including leading zero bytes", () => {
  const id = "CbgR6tj5kWx5oziiFptM7jMvrQeYY3Mzaao6ciuhSr2r";
  assert.equal(base58Encode(base58Decode(id)), id);
  assert.equal(base58Encode(new Uint8Array(32)), "1".repeat(32));
  assert.equal(base58Decode("1".repeat(32)).length, 32);
});

test("names are restricted to a safe charset", () => {
  for (const good of ["a", "jef", "jef-2", "a".repeat(32)]) assert.ok(isValidName(good), good);
  for (const bad of ["", "Jef", "jef.eth", "-jef", "jef-", "j f", "a".repeat(33), 42]) {
    assert.ok(!isValidName(bad), String(bad));
  }
});

test("typed names are normalised to the bare name", () => {
  for (const typed of ["jef", "jef.lez", "Jef.LEZ", "  jef.lez  "]) assert.equal(normalizeName(typed), "jef");
  assert.equal(normalizeName("jef.lez.lez"), "jef.lez");
  assert.ok(!isValidName(normalizeName("jef.eth")));
  assert.ok(!isValidName(normalizeName(".lez")));
  // Only ASCII is lowered: the Kelvin sign lowers to `k` in Unicode, and must not here.
  assert.ok(!isValidName(normalizeName("Kate")));
});

test("a profile encodes padded, seals to the vector and opens again", async () => {
  const plaintext = encodeProfile(PROFILE);
  assert.equal(plaintext.length, 256);
  assert.ok(plaintext.subarray(63).every((byte) => byte === 0));
  assert.notEqual(plaintext[62], 0);

  const sealed = await seal(jef.key, plaintext, "record", NONCE);
  assert.equal(toHex(sealed), SEALED_PROFILE);
  assert.deepEqual(decodeProfile(await openSealed(jef.key, sealed, "record")), PROFILE);
});

test("a page seals to the vector and opens again", async () => {
  const sealed = await seal(jef.key, encodePageContent(PAGE), "page", NONCE);
  assert.equal(toHex(sealed), SEALED_PAGE);
  assert.deepEqual(decodePageContent(await openSealed(jef.key, sealed, "page")), PAGE);
});

test("a profile round-trips with every field set and pads to the next block", () => {
  const full = {
    name: "ada",
    account: ACCOUNT,
    npk: "ab".repeat(32),
    vpk: "cd".repeat(1184),
    texts: { chat: "é".repeat(200), "myapp.setting": "" },
  };
  const plaintext = encodeProfile(full);
  assert.equal(plaintext.length, 1792);
  assert.deepEqual(decodeProfile(plaintext), full);
  assert.equal(encodeProfile({ name: "ada" }).length, 256);
  assert.throws(() => decodeProfile(Uint8Array.of(9, 0, 0, 0)), /truncated/);
});

test("sealed data opens only with its key, untouched, as its own kind", async () => {
  const sealed = Buffer.from(SEALED_PROFILE, "hex");
  const { key: otherKey } = await deriveName("ada");
  assert.equal(await openSealed(otherKey, sealed, "record"), null);
  assert.equal(await openSealed(jef.key, sealed, "page"), null);
  for (const position of [0, 12, sealed.length - 1]) {
    const tampered = Uint8Array.from(sealed);
    tampered[position] ^= 1;
    assert.equal(await openSealed(jef.key, tampered, "record"), null, `byte ${position}`);
  }
  assert.equal(await openSealed(jef.key, sealed.subarray(0, 27), "record"), null);
  assert.equal(await openSealed(jef.key, new Uint8Array(0), "record"), null);
  await assert.rejects(openSealed(jef.key, sealed, "profile"), /unknown kind/);
});

test("sealing uses a fresh nonce each time", async () => {
  const first = await seal(jef.key, encodePageContent(PAGE), "page");
  const second = await seal(jef.key, encodePageContent(PAGE), "page");
  assert.notEqual(toHex(first.subarray(0, 12)), toHex(second.subarray(0, 12)));
  assert.deepEqual(decodePageContent(await openSealed(jef.key, first, "page")), PAGE);
});

/** Minimal borsh writer mirroring the on-chain layouts in core/src/lib.rs. */
function borsh(...parts) {
  const chunks = parts.map((part) => {
    if (typeof part === "number") return Uint8Array.of(part);
    if (typeof part === "string") part = new TextEncoder().encode(part);
    return [...Uint8Array.of(part.length, part.length >> 8, 0, 0), ...part];
  });
  return Uint8Array.from(chunks.flatMap((chunk) => [...chunk]));
}

/** A `NameRecord` shard: the owner tag goes in raw, then the expiry, then the sealed bytes length-prefixed. */
function recordShard(tag, sealed, expires = EXPIRES) {
  const expiry = new Uint8Array(8);
  new DataView(expiry.buffer).setBigUint64(0, BigInt(expires), true);
  const tagBytes = tag.match(/../g).map((pair) => parseInt(pair, 16));
  return Uint8Array.from([3, ...tagBytes, ...expiry, ...borsh(sealed)]);
}

test("the on-chain layouts decode, and truncated data is refused", () => {
  const sealed = Uint8Array.of(0xee, 0xff);
  assert.deepEqual(decodeNameRecord(recordShard(OWNER_TAG, sealed)), {
    version: 3,
    ownerTag: OWNER_TAG,
    expires: EXPIRES,
    sealed,
  });
  assert.equal(decodeNameRecord(recordShard(OWNER_TAG, sealed, 0x0102030405)).expires, 0x0102030405);
  assert.deepEqual(decodePage(borsh(2, sealed)), { version: 2, sealed });
  assert.deepEqual(decodePrimaryName(borsh(2, "jef")), { version: 2, name: "jef" });
  assert.throws(() => decodeNameRecord(Uint8Array.of(2, 9, 0, 0, 0)), /truncated/);
  assert.throws(() => decodeNameRecord(recordShard(OWNER_TAG, sealed).subarray(0, 40)), /truncated/);
});

/**
 * Stands in for the sequencer: `accounts` maps an account ID to the bytes of
 * the registry's shard on it. Returns the account IDs that were asked for.
 */
function stubSequencer(accounts) {
  const asked = [];
  globalThis.fetch = async (url, { body }) => {
    assert.equal(url, RPC_URL);
    const { id, method, params } = JSON.parse(body);
    assert.equal(method, "getAccount");
    asked.push(params[0]);
    const shard = accounts[params[0]];
    const data = shard ? { shards: { [PROGRAM]: [...shard] } } : { shards: {} };
    return Response.json({ jsonrpc: "2.0", id, result: { data } });
  };
  return asked;
}

const realFetch = globalThis.fetch;
afterEach(() => {
  globalThis.fetch = realFetch;
});

const JEF_RECORD = await recordAddress(PROGRAM, jef.id);
const JEF_PAGE = await pageAddress(PROGRAM, jef.id);
const RESOLVED = {
  name: "jef",
  expires: EXPIRES,
  lapsed: false,
  account: ACCOUNT,
  npk: null,
  vpk: null,
  texts: { chat: "hi" },
};
const NOTHING_SET = { ...RESOLVED, account: null, texts: {} };

test("a name resolves with one read of its record account", async () => {
  const asked = stubSequencer({ [JEF_RECORD]: recordShard(OWNER_TAG, Buffer.from(SEALED_PROFILE, "hex")) });
  assert.deepEqual(await resolveName(RPC_URL, PROGRAM, "jef"), RESOLVED);
  assert.deepEqual(asked, [JEF_RECORD]);
});

test("an unregistered name resolves to null", async () => {
  stubSequencer({});
  assert.equal(await resolveName(RPC_URL, PROGRAM, "jef"), null);
  await assert.rejects(resolveName(RPC_URL, PROGRAM, "Jef.lez"), /invalid name/);
});

test("a record with nothing readable in it is a registered name with nothing set", async () => {
  stubSequencer({ [JEF_RECORD]: recordShard(OWNER_TAG, new Uint8Array(0)) });
  assert.deepEqual(await resolveName(RPC_URL, PROGRAM, "jef"), NOTHING_SET);

  // Sealed under another name's key.
  const { key: otherKey } = await deriveName("ada");
  stubSequencer({ [JEF_RECORD]: recordShard(OWNER_TAG, await seal(otherKey, encodeProfile(PROFILE), "record")) });
  assert.deepEqual(await resolveName(RPC_URL, PROGRAM, "jef"), NOTHING_SET);

  // Sealed under the right key, but the profile names another name.
  const foreign = encodeProfile({ ...PROFILE, name: "ada" });
  stubSequencer({ [JEF_RECORD]: recordShard(OWNER_TAG, await seal(jef.key, foreign, "record")) });
  assert.deepEqual(await resolveName(RPC_URL, PROGRAM, "jef"), NOTHING_SET);
});

test("a page resolves, and a missing or unreadable one is null", async () => {
  const record = recordShard(OWNER_TAG, Buffer.from(SEALED_PROFILE, "hex"));
  const page = borsh(2, Buffer.from(SEALED_PAGE, "hex"));
  const asked = stubSequencer({ [JEF_RECORD]: record, [JEF_PAGE]: page });
  assert.deepEqual(await resolvePage(RPC_URL, PROGRAM, "jef"), PAGE);
  assert.deepEqual(asked, [JEF_RECORD, JEF_PAGE]);

  stubSequencer({ [JEF_RECORD]: record });
  assert.equal(await resolvePage(RPC_URL, PROGRAM, "jef"), null);
  stubSequencer({ [JEF_RECORD]: record, [JEF_PAGE]: borsh(2, new Uint8Array(0)) });
  assert.equal(await resolvePage(RPC_URL, PROGRAM, "jef"), null);
  stubSequencer({ [JEF_RECORD]: record, [JEF_PAGE]: borsh(2, Buffer.from(SEALED_PROFILE, "hex")) });
  assert.equal(await resolvePage(RPC_URL, PROGRAM, "jef"), null);

  // A page is only served for a registered name, and is not read for any other.
  const askedUnregistered = stubSequencer({ [JEF_PAGE]: page });
  assert.equal(await resolvePage(RPC_URL, PROGRAM, "jef"), null);
  assert.deepEqual(askedUnregistered, [JEF_RECORD]);
});

/** A sequencer holding `jef` with a profile, a page and reverse records, expiring at `EXPIRES`. */
function stubHeldName() {
  const claim = borsh(2, "jef");
  return stubSequencer({
    [JEF_RECORD]: recordShard(OWNER_TAG, Buffer.from(SEALED_PROFILE, "hex")),
    [JEF_PAGE]: borsh(2, Buffer.from(SEALED_PAGE, "hex")),
    [OWNER]: claim,
    [ACCOUNT]: claim,
  });
}

test("a name is active until the moment it expires", async () => {
  stubHeldName();
  for (const now of [0, EXPIRES - 1]) {
    assert.deepEqual(await resolveName(RPC_URL, PROGRAM, "jef", now), RESOLVED, `now ${now}`);
    assert.deepEqual(await resolvePage(RPC_URL, PROGRAM, "jef", now), PAGE);
    assert.equal(await reverseLookup(RPC_URL, PROGRAM, ACCOUNT, now), "jef");
  }
});

test("a name in its grace period still resolves, flagged as lapsed", async () => {
  stubHeldName();
  for (const now of [EXPIRES, EXPIRES + GRACE_MS - 1]) {
    const resolved = await resolveName(RPC_URL, PROGRAM, "jef", now);
    assert.deepEqual(resolved, { ...RESOLVED, lapsed: true }, `now ${now}`);
    assert.deepEqual(await resolvePage(RPC_URL, PROGRAM, "jef", now), PAGE);
    assert.equal(await reverseLookup(RPC_URL, PROGRAM, OWNER, now), "jef");
  }

  // Nothing set is flagged the same way.
  stubSequencer({ [JEF_RECORD]: recordShard(OWNER_TAG, new Uint8Array(0)) });
  assert.deepEqual(await resolveName(RPC_URL, PROGRAM, "jef", EXPIRES), { ...NOTHING_SET, lapsed: true });
});

test("a name past its grace period is unregistered, whatever its record still holds", async () => {
  assert.equal(GRACE_MS, 30 * 24 * 60 * 60 * 1000);
  for (const now of [EXPIRES + GRACE_MS, EXPIRES + GRACE_MS + 1, Number.MAX_SAFE_INTEGER]) {
    const asked = stubHeldName();
    assert.equal(await resolveName(RPC_URL, PROGRAM, "jef", now), null, `now ${now}`);
    assert.equal(await resolvePage(RPC_URL, PROGRAM, "jef", now), null);
    assert.equal(await reverseLookup(RPC_URL, PROGRAM, OWNER, now), null);
    assert.equal(await reverseLookup(RPC_URL, PROGRAM, ACCOUNT, now), null);
    // The page of such a name is not even read.
    assert.ok(!asked.includes(JEF_PAGE));
  }
});

test("without a time given, the reader's own clock decides", async () => {
  const sealed = Buffer.from(SEALED_PROFILE, "hex");
  const accounts = { [JEF_PAGE]: borsh(2, Buffer.from(SEALED_PAGE, "hex")), [OWNER]: borsh(2, "jef") };

  const inGrace = Date.now() - 60_000;
  stubSequencer({ ...accounts, [JEF_RECORD]: recordShard(OWNER_TAG, sealed, inGrace) });
  const resolved = await resolveName(RPC_URL, PROGRAM, "jef");
  assert.deepEqual(resolved, { ...RESOLVED, expires: inGrace, lapsed: true });
  assert.deepEqual(await resolvePage(RPC_URL, PROGRAM, "jef"), PAGE);
  assert.equal(await reverseLookup(RPC_URL, PROGRAM, OWNER), "jef");

  stubSequencer({ ...accounts, [JEF_RECORD]: recordShard(OWNER_TAG, sealed, inGrace - GRACE_MS) });
  assert.equal(await resolveName(RPC_URL, PROGRAM, "jef"), null);
  assert.equal(await resolvePage(RPC_URL, PROGRAM, "jef"), null);
  assert.equal(await reverseLookup(RPC_URL, PROGRAM, OWNER), null);
});

test("a reverse record counts only when the name points back", async () => {
  const sealed = Buffer.from(SEALED_PROFILE, "hex");
  const claim = borsh(2, "jef");
  const accounts = {
    [JEF_RECORD]: recordShard(OWNER_TAG, sealed),
    [OWNER]: claim,
    [ACCOUNT]: claim,
    [STRANGER]: claim,
  };

  const asked = stubSequencer(accounts);
  assert.equal(await reverseLookup(RPC_URL, PROGRAM, OWNER), "jef");
  assert.deepEqual(asked, [OWNER, JEF_RECORD]);
  assert.equal(await reverseLookup(RPC_URL, PROGRAM, ACCOUNT), "jef");
  assert.equal(await reverseLookup(RPC_URL, PROGRAM, STRANGER), null);

  // The owner still counts when the profile is empty; the account it used to name does not.
  stubSequencer({ ...accounts, [JEF_RECORD]: recordShard(OWNER_TAG, new Uint8Array(0)) });
  assert.equal(await reverseLookup(RPC_URL, PROGRAM, OWNER), "jef");
  assert.equal(await reverseLookup(RPC_URL, PROGRAM, ACCOUNT), null);
});

test("the owner tag binds an account to one name, and a lookup never shows it", async () => {
  const ada = await deriveName("ada");
  assert.match(OWNER_TAG, /^[0-9a-f]{64}$/);
  assert.notEqual(await ownerTag(ada.id, OWNER), OWNER_TAG);
  stubSequencer({ [JEF_RECORD]: recordShard(OWNER_TAG, new Uint8Array(0)) });
  const resolved = await resolveName(RPC_URL, PROGRAM, "jef", EXPIRES - 1);
  assert.equal(JSON.stringify(resolved).includes(OWNER), false);
  assert.equal(JSON.stringify(resolved).includes(OWNER_TAG), false);
});

test("an account with no reverse record, or one that claims nothing usable, has no name", async () => {
  stubSequencer({});
  assert.equal(await reverseLookup(RPC_URL, PROGRAM, OWNER), null);
  // The name was released: the claim is left behind, the record is gone.
  stubSequencer({ [OWNER]: borsh(2, "jef") });
  assert.equal(await reverseLookup(RPC_URL, PROGRAM, OWNER), null);
  const asked = stubSequencer({ [OWNER]: borsh(2, "Jef.lez") });
  assert.equal(await reverseLookup(RPC_URL, PROGRAM, OWNER), null);
  assert.deepEqual(asked, [OWNER]);
});
