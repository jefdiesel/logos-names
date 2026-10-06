// Run with: node --test resolver/names.test.js
import assert from "node:assert/strict";
import { test } from "node:test";
import {
  base58Decode,
  base58Encode,
  decodeDirectory,
  decodeNameRecord,
  decodePage,
  decodePrimaryName,
  directoryAddress,
  isValidName,
  normalizeName,
  pageAddress,
  recordAddress,
} from "./names.js";

const PROGRAM = base58Encode(new Uint8Array(32).fill(7));

// The same vectors are asserted in core/src/lib.rs, so Rust and JavaScript must agree.
test("addresses match the Rust implementation", async () => {
  assert.equal(await recordAddress(PROGRAM, "jef"), "28mUmKUyH1dJgmwdjVxS22P12iUHVotLcvk2nbrFw29Y");
  assert.equal(await pageAddress(PROGRAM, "jef"), "6ckRpxviqmmhY6QkcJPVPWYSSy3eRZGzy1uUGStf1TnC");
  assert.equal(await directoryAddress(PROGRAM), "9oQ7KANHqw4ok3goEHzgeK7WSxdKqKNXA7bS9HHpq97p");
});

test("base58 round-trips, including leading zero bytes", () => {
  const id = "CbgR6tj5kWx5oziiFptM7jMvrQeYY3Mzaao6ciuhSr2r";
  assert.equal(base58Encode(base58Decode(id)), id);
  assert.equal(base58Encode(new Uint8Array(32)), "1".repeat(32));
  assert.equal(base58Decode("1".repeat(32)).length, 32);
});

test("names are restricted to a safe charset", async () => {
  for (const good of ["a", "jef", "jef-2", "a".repeat(32)]) assert.ok(isValidName(good), good);
  for (const bad of ["", "Jef", "jef.eth", "-jef", "jef-", "j f", "a".repeat(33), 42]) {
    assert.ok(!isValidName(bad), String(bad));
  }
  await assert.rejects(recordAddress(PROGRAM, "Not Valid"), /invalid name/);
});

/** Minimal borsh writer mirroring the layouts in core/src/lib.rs. */
function borsh(...parts) {
  const chunks = parts.map((part) => {
    if (typeof part === "number") return Uint8Array.of(part);
    if (typeof part === "string") {
      const bytes = new TextEncoder().encode(part);
      return Uint8Array.of(bytes.length, 0, 0, 0, ...bytes);
    }
    return part;
  });
  return Uint8Array.from(chunks.flatMap((chunk) => [...chunk]));
}

test("a name record decodes", () => {
  const owner = new Uint8Array(32).fill(1);
  const account = new Uint8Array(32).fill(2);
  const record = decodeNameRecord(
    borsh(
      1, "jef", owner,
      1, account, // Some(account)
      0, // npk: None
      Uint8Array.of(0, 0, 0, 0), // empty vpk
      Uint8Array.of(1, 0, 0, 0), "chat", "hello", // one text entry
      1, "ada", // previous: Some("ada")
    ),
  );
  assert.deepEqual(record, {
    version: 1,
    name: "jef",
    owner: base58Encode(owner),
    account: base58Encode(account),
    npk: null,
    vpk: null,
    texts: { chat: "hello" },
    previous: "ada",
  });
});

test("a page and a primary name decode, and truncated data is refused", () => {
  const page = decodePage(borsh(1, "text/html", Uint8Array.of(2, 0, 0, 0), Uint8Array.of(104, 105)));
  assert.equal(page.contentType, "text/html");
  assert.deepEqual([...page.body], [104, 105]);
  assert.deepEqual(decodePrimaryName(borsh(1, "jef")), { version: 1, name: "jef" });
  assert.throws(() => decodeNameRecord(Uint8Array.of(1, 9, 0, 0, 0)), /truncated/);
});

test("a directory decodes", () => {
  const count = Uint8Array.of(3, 0, 0, 0, 0, 0, 0, 0);
  assert.deepEqual(decodeDirectory(borsh(1, count, 1, "jef")), { version: 1, count: 3, latest: "jef" });
  assert.deepEqual(decodeDirectory(borsh(1, count, 0)), { version: 1, count: 3, latest: null });
});

test("typed names are normalised to the bare name", () => {
  for (const typed of ["jef", "jef.lez", "Jef.LEZ", "  jef.lez  "]) assert.equal(normalizeName(typed), "jef");
  assert.equal(normalizeName("jef.lez.lez"), "jef.lez");
  assert.ok(!isValidName(normalizeName("jef.eth")));
  assert.ok(!isValidName(normalizeName(".lez")));
});
