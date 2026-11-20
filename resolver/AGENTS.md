# Resolver notes for agents and apps

An LNS name is a bare lowercase string of 1-32 letters, digits or hyphens. Names of
1-4 characters are locked: they are not registrable now. Call it an "LNS name" —
never ".lez" or "Logos names". `normalizeName` turns what a person typed
(`Jef.lez`, `@jef`, `lns:jef`) into that bare name; `isValidName` checks it.

Latest version: https://github.com/jefdiesel/logos-names (placeholder until the repo is public).

## Resolve

`resolveName(rpcUrl, program, name)` reads the record from the LEZ sequencer. It
returns `{ name, expires, lapsed, account, npk, vpk, texts, deed }`, or null when
the name is not registered or is past its grace period. `expires` is milliseconds
since the Unix epoch, or null for a name held by a deposit (it does not expire).
`lapsed` is true in the grace period, while the name still resolves. `npk` and
`vpk` are private receiving keys for the payer's wallet: never show them to a
person, use `publicView(record)` for that.

`resolveUsing` follows one `forward` entry and sets `forwardedFrom` — a payer must
be shown that field. Use `resolveUsing` to pay or message, `resolveName` to change
a name (it never follows a forward). `resolvePage` reads a name's page,
`reverseLookup` finds the name an account is shown as.

## Read the NFT (deed)

The NFT is optional: a name works fully without one, and only names of 5-32
characters that are not locked or reserved can claim one. `deed` is null for
records of version 1-6. A version 7 record carries a definition and a 32-byte nonce; the
resolver returns `deed: { definition, holding, nonce }`, the definition PDA and the
holder's deed account. The registry alone controls that address, so the holder
cannot move the deed on their own — LNS stays the source of truth.
`deedAccounts(program, id, ownerTag, nonce)` derives the same pair if you already
have the record. `readDeed(rpcUrl, deed)` reads that holding account from the NFT
program (`CjrUpegdA1RjqDN43T8P19kAkwnD1quAkNkER8k3dw9G`) and returns
`{ held, kind, definition }`: held is whether the account really holds the master or an owned copy,
kind is "master" or "copy" only while held (else null), and definition comes from the state, or
from `deed.definition` when the account is empty or unreadable. An unreadable state is never shown
as held. The public NFT image must not show the name; the wallet or an SDK
app draws the holder's own card locally.

## Draw the card

`nameCard(name)` returns a 480x480 SVG string drawn per `design/lns-name-card`.
Fonts are named (Gloock, Fira Code), never embedded. Write it to a `.svg` file or
inline it; escaping of the name is done for you.

## Three examples

```js
import { normalizeName, resolveName, publicView } from "./names.js";
const name = normalizeName("Jef.lez");
const record = await resolveName(RPC, PROGRAM, name);
if (record) console.log(publicView(record)); // npk/vpk are not in this view
```

```js
import { resolveName, readDeed } from "./names.js";
const record = await resolveName(RPC, PROGRAM, "jef");
console.log(record?.deed); // { definition, holding, nonce } or null
if (record?.deed) console.log(await readDeed(RPC, record.deed)); // { held, kind, definition }
```

To derive the accounts from raw record bytes instead, read the record address,
`decodeNameRecord` it, then call `deedAccounts(program, id, rec.ownerTag, rec.deed.nonce)`.

```js
import { nameCard } from "./card.js";
import { writeFileSync } from "node:fs";
writeFileSync("jef.svg", nameCard("jefdiesel")); // a well-formed SVG string
```

`RPC` is the sequencer's JSON-RPC URL and `PROGRAM` the registry's base58 account
id. Both are passed in; the resolver does not fix a network.

## Errors

The resolver throws plain `Error`s with these exact messages (a `${part}` is the
value named):

- `invalid name: ${name}` — the name failed `isValidName`.
- `sequencer returned HTTP ${status}` — the RPC answered with a non-2xx status.
- `sequencer error: ${message}` — the RPC returned a JSON-RPC error.
- `${name} points to ${target}, which isn't registered` — `resolveUsing` one-hop forward.
- `the record's deed is unreadable` — a v7 record's deed tag was neither 0 nor 1.
- `record is truncated`, `not a boolean` — a malformed on-chain layout.
- `invalid base58 character: ${char}`, `account ID must be 32 bytes: ${accountId}`,
  `invalid hex: ${text}`, `name ID must be 32 bytes`, `deed nonce must be 32 bytes`,
  `${what} must be 32 bytes`, `nonce must be 12 bytes`,
  `unknown kind of sealed data: ${kind}` — bad input to a helper.

A sealed value that does not open, a missing record and a page that does not open
return null rather than throwing.
