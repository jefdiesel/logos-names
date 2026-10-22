# Building on LNS

How to resolve and show LNS names in your wallet, app or service, and keep up as
the registry changes. [SPEC.md](../SPEC.md) is the authority; this page tells you
which parts to read and what goes wrong most often.

## 1. Use the live registry

The one registry every client should use is listed in
[SPEC.md section 11](../SPEC.md#11-canonical-registries). Today, on the public
LEZ testnet (`https://testnet.lez.logos.co`):

```
38jEfVV1RCyMzu1FGbYcCn5FqxCxKyY58VgzjruAh2gq   registry v5, since 2026-10-16
```

Older registries are retired, and their names did not carry over. If your code
has one of these, change it:

| Program ID | Version | Status |
|---|---|---|
| `2PXg3SSmh5hvECTz4xcfV2d5YvxwyF6yVhpnjTokzCkX` | v4.1 | retired 2026-10-16 (old PBKDF2 derivation) |
| `66nUvzJtTxA729aUpTcAWDrb7cV9Heb2bDzceHQhLYad` | v3 | retired |

Make the registry a setting with this ID as the default, so a new registry is a
configuration change, not a release.

## 2. Reuse a library before writing your own

Every part of resolving a name has to match the spec byte for byte, or your client
quietly disagrees with every other client about what a name means. These already
do it:

| You are writing | Use |
|---|---|
| A Basecamp app or module | The `names` core module (SPEC 12.5): `recipient`, `resolve`, `reverse`, `normalize`. No crypto on your side. |
| Rust | `names_core` (addresses, `normalize_name`, on-chain types) and `names_seal` (`derive`, `open_profile`, `open_page`) from this repo. |
| JavaScript or TypeScript | [`resolver/names.js`](../resolver/names.js): one file, one dependency (`@noble/hashes` for Argon2id). Runs in Node 20+, browsers and Workers. |
| An AI agent | [`mcp/server.js`](../mcp/server.js): `resolve_name`, `get_page`, `reverse_lookup`. |

If you port it to another language, port `resolver/names.js`. It is about 400
lines and is the whole read path.

## 3. Rules a port must get right

These are the mistakes we have seen in ports:

- **Lower ASCII only.** Use an ASCII-only lowercase (`to_ascii_lowercase`, a
  `[A-Z]` replace), never full Unicode lowercasing. Unicode folds lookalikes into
  letters: U+212A KELVIN SIGN becomes `k`, so a spoofed string would resolve to a
  real name.
- **Names are bare.** `jef` is the name. Accept and strip one leading `lns:`, then
  one leading `@`, then one trailing legacy `.lez`, then require
  `^[a-z0-9-]{1,32}$` with no hyphen at either end (SPEC 2).
- **Names versus addresses.** In a "send to" field, try the input as an account
  ID first and otherwise as a name. Refuse a name that also decodes as an account
  ID (SPEC 2).
- **Milliseconds.** `expires` and block timestamps are in milliseconds. The grace
  period is 2,592,000,000 ms. A name held by a deposit has `expires = u64::MAX`
  and never lapses (SPEC 6, 8).
- **Associated data.** Sealed records open with AES-256-GCM under the name's key,
  with the associated data `logos-names/v5/record` (pages: `logos-names/v5/page`).
  The first 12 bytes are the nonce (SPEC 5).
- **Check the name inside.** After opening a profile, require
  `profile.name == name`. Otherwise treat the name as registered with nothing set
  (SPEC 6).
- **Resolve right before you pay.** A name can change hands at any time. Don't
  pay to a cached result: resolve again just before signing.

## 4. Test against the vectors

[SPEC.md section 10](../SPEC.md#10-test-vectors) gives the Argon2id output, the
`id`, `key` and record address for `jef`, plus a sealed profile and page with a
fixed nonce. Assert all of them in your tests. A port that only checks that its
output is repeatable proves nothing; these vectors show it matches every other
client. `resolver/names.test.js` and `seal/src/lib.rs` are examples.

For an end-to-end check, resolve `jef` on the testnet: it is registered in v5.

## 5. Keep up with changes

- **Registry IDs change only in SPEC section 11**, together with a line in the
  README's Status section. Watch this repository's releases, or check section 11
  before you ship.
- **A new version changes every derivation string** (`logos-names/v5/...` becomes
  `v6`), so its identifiers and accounts never collide with v5's, and a client can
  support both while names move over (SPEC 13).
- **Mainnet:** testnet names do not carry over automatically. Holders claim them
  within 30 days of launch. The mainnet registry will be added to section 11.

Questions or a port you'd like checked: open an issue on this repository.
