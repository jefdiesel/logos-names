// Run with: node --test resolver/names.test.js
import assert from "node:assert/strict";
import { afterEach, test } from "node:test";
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
const JEF_ID = "86d7e719e7acabe344ef8a69782b72ed0c21087e73e400e7056ab0daf7e6c618";
const JEF_KEY = "d19aa10c57273a3ff4f569cc3a9c35819b2a9bcac7ba8a48deb131269cb6e2e0";
const PROFILE = { name: "jef", account: ACCOUNT, npk: null, vpk: null, texts: { chat: "hi" } };
const SEALED_PROFILE =
  "000102030405060708090a0b67bf760994a19946abc799235b95e48bfb804c3ca05471c9261f5099" +
  "e1b00570e6139ab5bbb5fac423627e0fd18c5299475a5c4ed914f1e506f23b66d4d2377de959bff4" +
  "e8d4ab02b63865f9a2453fef24302c211eb70eadebf5b38038d080e2b6f3b1fa17a777b940f17eb9" +
  "4a83f4ce719552c92108600800f3d020a7d8fee73e2792c907db62125ba636607ed94ea46b73f046" +
  "981a751be920449222629d2561194416c993b4bfdeeec20026656f11b64f1c2ac368d64e57d07054" +
  "ac0fcdac4a6e83b60ac363901963488c93c7762d19b34b6991b9abdbc5d293fca06b69cab31e7eaa" +
  "769142940f4813c73713dbd34b24386fb53937e2c9915323f35d39bf50e63d4289cb48a7c7aff0e5" +
  "404987b0";
const PAGE = { contentType: "text/plain", body: new TextEncoder().encode("hi") };
const SEALED_PAGE =
  "000102030405060708090a0b6ebf76098aa1873386b5f74030f9e489f9822657c6ba930c224eb1f2" +
  "64767f7cf7b5a511";

const jef = await deriveName("jef");

test("a name derives its identifier and key", async () => {
  assert.equal(toHex(jef.id), JEF_ID);
  assert.equal(toHex(jef.key), JEF_KEY);
  await assert.rejects(deriveName("Not Valid"), /invalid name/);
  await assert.rejects(deriveName("jef.lez"), /invalid name/);
});

test("addresses match the Rust implementation", async () => {
  assert.equal(base58Encode(new Uint8Array(32).fill(7)), "US517G5965aydkZ46HS38QLi7UQiSojurfbQfKCELFx");
  assert.equal(await recordAddress(PROGRAM, jef.id), "6gsCMau9ajq4FcxDCJmqqtUWVnuCiJHtXoYbCzCZPGJJ");
  assert.equal(await pageAddress(PROGRAM, jef.id), "338iYF6WmqnFryDjLWyKTYUhAgpT6o7C8qhHAd5pSf32");
  assert.equal(await recordAddress(PROGRAM, COUNTING_ID), "HqAjYDvn33ACMyTPQ5VELMbfMvQserWSvpPkzCHJPqN9");
  assert.equal(await pageAddress(PROGRAM, COUNTING_ID), "9UFQXB1QKPHScwFpPzjXUVYGJW3LQCvXHvL7dmSTkaCu");
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

/** A `NameRecord` shard: the owner goes in raw, then the expiry, then the sealed bytes length-prefixed. */
function recordShard(owner, sealed, expires = EXPIRES) {
  const expiry = new Uint8Array(8);
  new DataView(expiry.buffer).setBigUint64(0, BigInt(expires), true);
  return Uint8Array.from([2, ...base58Decode(owner), ...expiry, ...borsh(sealed)]);
}

test("the on-chain layouts decode, and truncated data is refused", () => {
  const sealed = Uint8Array.of(0xee, 0xff);
  assert.deepEqual(decodeNameRecord(recordShard(OWNER, sealed)), {
    version: 2,
    owner: OWNER,
    expires: EXPIRES,
    sealed,
  });
  assert.equal(decodeNameRecord(recordShard(OWNER, sealed, 0x0102030405)).expires, 0x0102030405);
  assert.deepEqual(decodePage(borsh(2, sealed)), { version: 2, sealed });
  assert.deepEqual(decodePrimaryName(borsh(2, "jef")), { version: 2, name: "jef" });
  assert.throws(() => decodeNameRecord(Uint8Array.of(2, 9, 0, 0, 0)), /truncated/);
  assert.throws(() => decodeNameRecord(recordShard(OWNER, sealed).subarray(0, 40)), /truncated/);
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
  owner: OWNER,
  expires: EXPIRES,
  lapsed: false,
  account: ACCOUNT,
  npk: null,
  vpk: null,
  texts: { chat: "hi" },
};
const NOTHING_SET = { ...RESOLVED, account: null, texts: {} };

test("a name resolves with one read of its record account", async () => {
  const asked = stubSequencer({ [JEF_RECORD]: recordShard(OWNER, Buffer.from(SEALED_PROFILE, "hex")) });
  assert.deepEqual(await resolveName(RPC_URL, PROGRAM, "jef"), RESOLVED);
  assert.deepEqual(asked, [JEF_RECORD]);
});

test("an unregistered name resolves to null", async () => {
  stubSequencer({});
  assert.equal(await resolveName(RPC_URL, PROGRAM, "jef"), null);
  await assert.rejects(resolveName(RPC_URL, PROGRAM, "Jef.lez"), /invalid name/);
});

test("a record with nothing readable in it is a registered name with nothing set", async () => {
  stubSequencer({ [JEF_RECORD]: recordShard(OWNER, new Uint8Array(0)) });
  assert.deepEqual(await resolveName(RPC_URL, PROGRAM, "jef"), NOTHING_SET);

  // Sealed under another name's key.
  const { key: otherKey } = await deriveName("ada");
  stubSequencer({ [JEF_RECORD]: recordShard(OWNER, await seal(otherKey, encodeProfile(PROFILE), "record")) });
  assert.deepEqual(await resolveName(RPC_URL, PROGRAM, "jef"), NOTHING_SET);

  // Sealed under the right key, but the profile names another name.
  const foreign = encodeProfile({ ...PROFILE, name: "ada" });
  stubSequencer({ [JEF_RECORD]: recordShard(OWNER, await seal(jef.key, foreign, "record")) });
  assert.deepEqual(await resolveName(RPC_URL, PROGRAM, "jef"), NOTHING_SET);
});

test("a page resolves, and a missing or unreadable one is null", async () => {
  const record = recordShard(OWNER, Buffer.from(SEALED_PROFILE, "hex"));
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
    [JEF_RECORD]: recordShard(OWNER, Buffer.from(SEALED_PROFILE, "hex")),
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
  stubSequencer({ [JEF_RECORD]: recordShard(OWNER, new Uint8Array(0)) });
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
  stubSequencer({ ...accounts, [JEF_RECORD]: recordShard(OWNER, sealed, inGrace) });
  const resolved = await resolveName(RPC_URL, PROGRAM, "jef");
  assert.deepEqual(resolved, { ...RESOLVED, expires: inGrace, lapsed: true });
  assert.deepEqual(await resolvePage(RPC_URL, PROGRAM, "jef"), PAGE);
  assert.equal(await reverseLookup(RPC_URL, PROGRAM, OWNER), "jef");

  stubSequencer({ ...accounts, [JEF_RECORD]: recordShard(OWNER, sealed, inGrace - GRACE_MS) });
  assert.equal(await resolveName(RPC_URL, PROGRAM, "jef"), null);
  assert.equal(await resolvePage(RPC_URL, PROGRAM, "jef"), null);
  assert.equal(await reverseLookup(RPC_URL, PROGRAM, OWNER), null);
});

test("a reverse record counts only when the name points back", async () => {
  const sealed = Buffer.from(SEALED_PROFILE, "hex");
  const claim = borsh(2, "jef");
  const accounts = {
    [JEF_RECORD]: recordShard(OWNER, sealed),
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
  stubSequencer({ ...accounts, [JEF_RECORD]: recordShard(OWNER, new Uint8Array(0)) });
  assert.equal(await reverseLookup(RPC_URL, PROGRAM, OWNER), "jef");
  assert.equal(await reverseLookup(RPC_URL, PROGRAM, ACCOUNT), null);
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
