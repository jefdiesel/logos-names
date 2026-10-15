# Logos Names: specification

Version 2, draft. This document is the contract between the name registry and
everything that uses it: wallets, apps, gateways and agents. A client that
follows it resolves the same names to the same answers as every other client,
without depending on this repository's code.

Version 2 replaces version 1 entirely. Version 1 kept names and a list of them
on the chain in the clear; version 2 keeps neither.

## 1. What is private and what is not

The registry is built so that knowing a name is the only way to find it.

- **The chain never sees a name.** Every instruction, account address and
  event uses a 32-byte identifier derived from the name (section 3).
- **What a name resolves to is sealed**: encrypted under a key that is also
  derived from the name (section 5). The registry program stores the bytes and
  cannot read them.
- **There is no list of names** and nothing links one record to another.

What this does not hide:

- **Anyone who knows or guesses a name can resolve it.** That is what makes a
  name usable. The derivation is slow on purpose, about 100,000 times the cost
  of one hash, which makes scanning for names expensive. It does not protect a
  short or obvious name from a targeted guess.
- **Who holds a name is not stored.** A record keeps an owner tag,
  `SHA-256("logos-names/v3/owner" ‖ id ‖ account)`, in place of its holder: it
  names no account, and one account's tags for two names are unrelated. No
  instruction's effects or events carry an account either. A holder acting
  from a **LEZ private account** sends privacy-preserving transactions: the
  registry runs inside a proof made on the holder's machine, and the chain sees
  only the change to the record, never the account (section 8).
- **A name that points at a public account** shows that account, and so its
  balance and history, to anyone who resolves it. A name that points only at
  private receiving keys shows keys that reveal nothing about funds.
- **When a name expires and what was paid for it** are public: the record
  holds its expiry in the clear, and the fee arrives in the treasury. Neither
  says which name, and from a private holder the fee does not say who paid.
- **A public holder is seen signing.** A holder acting from a public account
  signs each change, so the chain shows that account touching some record. Use
  a private account to avoid that.
- **The connection.** Whoever serves the sequencer's RPC sees which record
  addresses a client reads and from where, though not the names.
- **Clients never tie a name to an account.** A conforming client sets only
  `npk` and `vpk` in a profile, never `account`; pays a name only through its
  receiving keys; never displays an address or the keys behind a name; and
  writes no reverse record (section 6). Those fields remain in the format for
  compatibility only.

## 2. Names

- A name is 1 to 32 bytes of `a-z`, `0-9` and `-`, and does not start or end
  with `-`.
- People write a name with the suffix `.lez`: `jef.lez`. The suffix is not part
  of the name.
- **Normalisation.** Before using input as a name, a client trims surrounding
  whitespace, lowers ASCII case, and removes one trailing `.lez`. It then
  rejects the result unless it is a valid name. `Jef.LEZ` becomes `jef`;
  `jef.eth` is rejected.
- A client displays names with the suffix.

Because the chain cannot check a name it never sees, these rules are enforced
by clients. Two clients that normalise differently would disagree about which
record a name means, so a client MUST apply them exactly.

## 3. From a name to its identifier and key

```
master = PBKDF2-HMAC-SHA256(password = name, salt = "logos-names/v2",
                            iterations = 100000, length = 32 bytes)
id     = SHA-256("logos-names/v2/id"  ‖ master)
key    = SHA-256("logos-names/v2/key" ‖ master)
```

`name` is the normalised name as ASCII. `id` is public: it is what the chain
knows the name by. `key` is never sent anywhere: it opens and seals what the
name resolves to. Neither can be turned back into the name except by guessing.

100,000 iterations is the most a Cloudflare Worker's Web Crypto allows, so a
gateway can run the derivation.

## 4. Where a name lives

A registry is one LEZ program, identified by its account ID (the `program`
below). All of a registry's data is in **that program's shard** on three kinds
of account, and one settings account.

| Data | Account ID |
|---|---|
| Name record | `PDA(program, SHA-256("logos-names/v2/name/" ‖ id))` |
| Page | `PDA(program, SHA-256("logos-names/v2/page/" ‖ id))` |
| Reverse record | the account itself |
| Settings | `PDA(program, SHA-256("logos-names/v2/config"))` |

`PDA(program, seed)` is LEZ's public program-derived address:

```
SHA-256( pad32("/LEE/v0.2/AccountId/PDA/") ‖ program (32 bytes) ‖ seed (32 bytes) )
```

where `pad32` right-pads the ASCII string with zero bytes to 32 bytes.

A shard that is absent or empty means "not set": the name is unregistered, has
no page, or the account has no reverse record.

## 5. Data layouts

All data is [borsh](https://borsh.io)-encoded. `String` is a `u32` length and
UTF-8 bytes; `Vec<u8>` is a `u32` length and bytes; `Option<T>` is one tag byte
(`0` none, `1` some) then `T`; integers are little-endian; `AccountId` is 32
raw bytes.

On the chain:

```rust
struct NameRecord { version: u8, owner: [u8; 32], expires: u64, sealed: Vec<u8> }   // version 3; owner is the owner tag; sealed at most 12,288 bytes
struct Page       { version: u8, sealed: Vec<u8> }                     // sealed at most 81,920 bytes
struct PrimaryName { version: u8, name: String }                       // the reverse record, in the clear
struct Settings   { version: u8, admin: AccountId, treasury: AccountId, prices: [u128; 3] }
```

`expires` is a time in milliseconds since the Unix epoch, as LEZ block
timestamps are. `prices` are in the native token's base units, one per term:

| Term | Index | Length |
|---|---|---|
| 30 days | 0 | 2,592,000,000 ms |
| 1 year | 1 | 31,536,000,000 ms |
| 5 years | 2 | 157,680,000,000 ms |

A registry whose settings account is empty charges nothing.

`sealed` is:

```
nonce (12 random bytes) ‖ AES-256-GCM(key, nonce, plaintext, associated data)
```

with the 16-byte tag appended to the ciphertext, as Web Crypto and most
libraries produce it. The associated data is the ASCII string
`logos-names/v2/record` for a record and `logos-names/v2/page` for a page. A
fresh random nonce is used every time something is sealed.

Inside a record's `sealed`, the plaintext is a profile followed by zero bytes
up to the next multiple of 256 bytes, so that its length says little:

```rust
struct Profile {
    name: String,                // the name itself; lets a reader confirm the record
    account: Option<AccountId>,  // public account the name can be paid at
    npk: Option<[u8; 32]>,       // nullifier public key, for private payments
    vpk: Vec<u8>,                // viewing public key: empty, or 1184 bytes
    texts: Vec<TextRecord>,      // at most 16
}
struct TextRecord { key: String, value: String }   // key 1-32 bytes, value at most 512
```

A text key is 1 to 32 bytes of `a-z`, `0-9`, `-`, `_` and `.`. A reader decodes
the profile from the start of the plaintext and ignores the padding.

Inside a page's `sealed`, the plaintext is, with no padding:

```rust
struct PageContent { content_type: String, body: Vec<u8> }   // content type 1-64 bytes
```

An empty `sealed` in a record means the name is registered with nothing set, as
it is just after a transfer. If a non-empty `sealed` does not open with the
name's key, or opens to a profile whose `name` is not the name, a reader treats
the name as registered with nothing set.

The registry holds only what is true now. A released name's record and page are
emptied, and a transferred name keeps nothing its last holder sealed. The chain
itself still holds every past transaction, as any chain does; what those
transactions contain is identifiers and sealed bytes.

## 6. Resolving

**Forward (name to profile).** Normalise the name, derive `id` and `key`, derive
the record account, read the registry's shard on it, decode `NameRecord`, open
`sealed`. One derivation and one account read.

**Expiry.** A name is held for a term and then lapses. With `now` the reader's
own clock in milliseconds and a grace period of 30 days (2,592,000,000 ms):

- `now < expires`: the name is active.
- `expires <= now < expires + grace`: the name is in grace. It can still be
  renewed, nobody else can register it, and it still resolves. A client
  SHOULD tell a payer that the name has lapsed.
- `now >= expires + grace`: a reader treats the name as unregistered, whatever
  the record still holds. Anyone may register it, which empties what was there.

**Paying a name.**

- Private: send a shielded transfer to the key pair (`npk`, `vpk`) with a fresh
  random identifier. If `npk` is `None`, the name does not accept private
  payments. A client SHOULD prefer this whenever the name offers it.
- Public: send to `account`. If it is `None`, the name does not accept public
  payments.
- A wallet should show the user what it resolved before sending, as it would
  for a pasted address.

What a private payment hides depends on where it is sent from. The recipient is
always hidden: each payment lands on a fresh private account only the holder can
find. Sent from a public account, the sender and the amount are visible. Sent
from a private account, neither is.

**Reverse (account to name).** Read the registry's shard on the account and
decode `PrimaryName`. This is only a claim, set by the account itself. A client
MUST then resolve that name forward and accept it only if the record's `owner`
equals the account's owner tag for that name, or the profile's `account` equals
the account. Otherwise the account has no
name.

**There is no listing.** A client that wants to show a person the names they
hold remembers them itself.

## 7. Text entries

Text entries are free-form. These keys have an agreed meaning; others are
application-defined and should be namespaced, for example `myapp.setting`.

| Key | Value |
|---|---|
| `chat` | The holder's Logos Chat address: 64 lowercase hex characters, as returned by `chat_module.get_address()`. In Chat 0.3.0 it changes each time the app starts, so it can be stale. |
| `inbox` | A Logos Delivery content topic, `/{app}/{version}/{name}/{encoding}`. A topic routes only: anyone can read or publish to it. |
| `agent` | A URL or address where the holder's AI agent can be reached. |
| `cid` | A Logos Storage content ID the name pins. |
| `logos.account` | The holder's Logos account: an [AccountLog](https://github.com/logos-co/logos-lips/blob/main/docs/identity/raw/accountlog.md) address, the account's Ed25519 public key as 64 lowercase hex characters. See below. |

### Linking a name to a Logos account

A Logos account (AccountLog LIP) is an Ed25519 key pair whose public key is its
address; it publishes a signed, append-only log of the keys and records it
endorses. The AccountLog leaves out how a consumer comes to hold an address and
whose it is. An LNS name fills that in, with a link each way:

- **Name to account:** the name's `logos.account` entry holds the address.
- **Account to name:** the account's log has a live `Add(Text)` entry under the
  context `lns.name` whose value is the name with its suffix, such as
  `jef.lez`. Its bytes are `01 <len u16 LE> 08 "lns.name" 02 <name>`.

A client treats a name as belonging to an account only when both hold: it
verifies the log under the address from `logos.account` (strictly, as the
AccountLog LIP requires), derives the live set, and finds the name in it. One
side alone proves nothing: anyone can write any address into a name they hold,
and any account can endorse any name. How the log is fetched (a DHT, Logos
Storage, a file) is out of scope, as it is in the AccountLog LIP.

Linking is public: anyone who resolves the name sees the address, and so the
account's log, which is public and permanent by design.

## 8. Changing a name

Instructions are a borsh enum, in this order (the variant index is the first
byte). `id` is 32 raw bytes.

| # | Instruction | Fields | Accounts, in order |
|---|---|---|---|
| 0 | `Register` | `id, sealed: Vec<u8>, term: u8, fee: u128, now: u64` | record, page, settings, payer, then treasury if `fee` is not 0 |
| 1 | `Renew` | `id, term: u8, fee: u128, now: u64` | record, settings, payer, then treasury if `fee` is not 0 |
| 2 | `SetRecord` | `id, sealed: Vec<u8>` | record, owner |
| 3 | `SetPage` | `id, sealed: Vec<u8>` | record, page, owner |
| 4 | `Transfer` | `id, new_owner: [u8; 32]` (the new holder's owner tag) | record, page, owner |
| 5 | `Release` | `id` | record, page, owner |
| 6 | `SetPrimary` | `name: String` | owner |
| 7 | `Configure` | `admin: AccountId, treasury: AccountId, prices: [u128; 3]` | settings, admin |

Rules:

- Every account is passed with the registry's own shard selected, except
  `payer` and `treasury`, which are passed with the native token's shard
  selected (the all-zero program ID). `owner`, `payer` and `admin` must sign.
- **Private holders.** `owner` and `payer` may be LEZ private accounts. The
  client then sends a privacy-preserving transaction: it runs the registry's
  bytecode in the privacy-preserving circuit, with the private account as an
  authorized input, and submits the proof. The chain sees the registry's
  effects on the public accounts (record, page, settings, treasury) and the
  fee arriving in the treasury, not the private account. Such transactions
  carry no fee of their own on LEZ v0.3.1. The client needs the registry's
  exact bytecode; the sequencer refuses a proof of any other.
- **Time.** `now` is the sender's claim of the current time in milliseconds. The
  program makes the transaction valid only in a block whose timestamp is at
  least `now`, so `now` is never in the future, and uses it as the time. A
  client takes `now` from the chain's clock: the timestamp, in the last 8 of 16
  bytes, of the clock program's shard on the account whose ID is the ASCII
  bytes `/LEZ/ClockProgramAccount/0000001`. It does not use its own clock: a
  transaction whose `now` is ahead of the chain is rejected and is still
  charged its whole gas limit.
- **Fees.** `fee` must equal the settings' price for `term`, and `treasury` must
  be the settings' treasury; the program then moves `fee` from `payer` to
  `treasury` with a native token transfer in the same transaction. When the
  settings account is empty, the price for the term is 0, or `payer` is the
  treasury itself, `fee` is 0 and no treasury account is passed. A client reads
  the settings account first.
- `Register` succeeds if the name is unregistered, or if its record's
  `expires + grace <= now`. The payer becomes the owner (its tag is stored), `expires` becomes
  `now` plus the term, and any page left by a lapsed holder is emptied. Its
  `sealed` is the name's first profile and may be empty.
- `Renew` succeeds while `now < expires + grace`, for any payer. `expires`
  becomes the later of `expires` and `now`, plus the term.
- `Configure` sets the settings. While the settings account is empty anyone may
  send it, so whoever deploys a registry configures it at once and checks that
  the admin is theirs; afterwards only the settings' `admin` may.
- Every other instruction on a name succeeds only if the signer's owner tag
  for the name is the record's `owner`.
- A profile is changed by reading it, opening it, changing it, sealing it again
  and sending the whole of it with `SetRecord`.
- `SetPage` with an empty `sealed` removes the page. `SetPrimary` with an empty
  name removes the reverse record; with a name, it publishes that name.
- `Transfer` sets the new owner and empties the profile and the page, so a
  transferred name never keeps routing payments or messages to its last holder.
  The expiry is kept.
- A holder may still change a lapsed name until someone else registers it;
  readers ignore it once it is past grace.
- `Release` empties the record and the page. The name is then unregistered and
  anyone may register it.
- A reverse record is not touched by `Transfer` or `Release`. It stops counting
  on its own, because the forward check in section 6 no longer passes.
- LEZ includes failed transactions in blocks. A client MUST confirm a change by
  reading the account back, not by the transaction's inclusion.
- A client sends one change per signing account at a time, and waits for it to
  show before sending the next.

## 9. Events

The registry emits LEZ program events. The selector is the first 8 bytes of
`SHA-256("<program>::<EventName>")`; the data is borsh.

| Event | Selector (hex) | Data |
|---|---|---|
| `logos_names::Registered` | `e87848390f741b1a` | `id, expires: u64` |
| `logos_names::Renewed` | `c0a35e0e73dbcc59` | `id, expires: u64` (`now` plus the term: the earliest the new expiry can be) |
| `logos_names::Transferred` | `b78a6f5ae756bf67` | `id` |
| `logos_names::Released` | `3d2e60d8d0ed2a9a` | `id` |

Events are emitted before the registry has checked the record, so an event can
exist for a change that was refused: they are hints to confirm against the
record. They carry identifiers, never names and never accounts.

## 10. Test vectors

For the name `jef`:

| Value | Hex |
|---|---|
| `master` | `83a5b494ae6f1505c3a8abca5389bb60b0f614e9d1f0490858708dd2a2dff800` |
| `id` | `86d7e719e7acabe344ef8a69782b72ed0c21087e73e400e7056ab0daf7e6c618` |
| `key` | `d19aa10c57273a3ff4f569cc3a9c35819b2a9bcac7ba8a48deb131269cb6e2e0` |

With `program` = 32 bytes of `0x07` (base58 `US517G5965aydkZ46HS38QLi7UQiSojurfbQfKCELFx`):

| Account | Base58 |
|---|---|
| Record of `jef` | `6gsCMau9ajq4FcxDCJmqqtUWVnuCiJHtXoYbCzCZPGJJ` |
| Page of `jef` | `338iYF6WmqnFryDjLWyKTYUhAgpT6o7C8qhHAd5pSf32` |
| Record of the identifier `00 01 02 … 1f` | `HqAjYDvn33ACMyTPQ5VELMbfMvQserWSvpPkzCHJPqN9` |
| Page of the identifier `00 01 02 … 1f` | `9UFQXB1QKPHScwFpPzjXUVYGJW3LQCvXHvL7dmSTkaCu` |

Sealing, with the nonce `00 01 02 … 0b` (a real client uses a random one). The
profile is `{ name: "jef", account: 32 bytes of 0x02, npk: none, vpk: empty,
texts: [("chat", "hi")] }`, 63 bytes, padded to 256:

```
000102030405060708090a0b67bf760994a19946abc799235b95e48bfb804c3ca05471c9261f5099
e1b00570e6139ab5bbb5fac423627e0fd18c5299475a5c4ed914f1e506f23b66d4d2377de959bff4
e8d4ab02b63865f9a2453fef24302c211eb70eadebf5b38038d080e2b6f3b1fa17a777b940f17eb9
4a83f4ce719552c92108600800f3d020a7d8fee73e2792c907db62125ba636607ed94ea46b73f046
981a751be920449222629d2561194416c993b4bfdeeec20026656f11b64f1c2ac368d64e57d07054
ac0fcdac4a6e83b60ac363901963488c93c7762d19b34b6991b9abdbc5d293fca06b69cab31e7eaa
769142940f4813c73713dbd34b24386fb53937e2c9915323f35d39bf50e63d4289cb48a7c7aff0e5
404987b0
```

The page `{ content_type: "text/plain", body: "hi" }`, same nonce:

```
000102030405060708090a0b6ebf76098aa1873386b5f74030f9e489f9822657c6ba930c224eb1f2
64767f7cf7b5a511
```

Instructions, with `id` = `00 01 02 … 1f`. `term` is one byte, `fee` 16 bytes
and `now` 8 bytes, little-endian:

| Instruction | Encoding |
|---|---|
| `Register { id, sealed: [ee, ff], term: 1, fee: 100000, now: 1700000000000 }` | `00` ‖ id ‖ `02000000eeff` ‖ `01` ‖ `a0860100000000000000000000000000` ‖ `0068e5cf8b010000` |
| `Renew { id, term: 2, fee: 0, now: 1700000000000 }` | `01` ‖ id ‖ `02` ‖ `00` 16 times ‖ `0068e5cf8b010000` |
| `SetRecord { id, sealed: [ee] }` | `02` ‖ id ‖ `01000000ee` |
| `SetPage { id, sealed: [] }` | `03` ‖ id ‖ `00000000` |
| `Transfer { id, new_owner: 32 bytes of 0x02 }` | `04` ‖ id ‖ `02` 32 times |
| `Release { id }` | `05` ‖ id |
| `SetPrimary { name: "jef" }` | `06030000006a6566` |
| `Configure { admin: 32 × 0x01, treasury: 32 × 0x02, prices: [1, 2, 3] }` | `07` ‖ `01` 32 times ‖ `02` 32 times ‖ three 16-byte little-endian numbers |

## 11. Canonical registries

Anyone can deploy a copy of the program. Clients on a network must agree on one
registry, as they agree on one token contract.

| Network | Registry program ID |
|---|---|
| Logos testnet v0.3 (LEZ) | `66nUvzJtTxA729aUpTcAWDrb7cV9Heb2bDzceHQhLYad` (record version 3, deployed 2026-10-15, CI build `506bfbfd…e9c4`; replaces `DugXYz…m2z`, version 2) |

The deployed program is the reproducible build produced by this repository's
CI; its SHA-256 is recorded with each release. The program is upgradeable by
whoever holds the key of its header account.

## 12. Interfaces

The sections above are enough to implement a client. These interfaces exist so
that most clients do not have to.

### 12.1 Libraries

- Rust: the `names_core` crate holds the program (`plan`, `apply`), the on-chain
  types, `record_address`, `page_address`, `normalize_name` and
  `validate_name`. The `names_seal` crate holds what a client adds: `derive`,
  `Profile`, `seal_profile`, `open_profile`, `seal_page` and `open_page`.
- JavaScript: `resolver/names.js`, with no dependencies, using Web Crypto.
  `normalizeName`, `deriveName`, `resolveName`, `resolvePage`, `reverseLookup`
  and the decoders. The three resolvers take the current time as an optional
  last argument.

### 12.2 HTTP (by the gateway)

| Request | Response |
|---|---|
| `GET https://<name>.<gateway>/` or `GET https://<gateway>/<name>` | The name's page, with its content type |
| `GET …/.well-known/logos-name.json` for a name | The resolved name as JSON, or 404 `{"error": "not registered"}` |

The JSON has `name`, `expires` (milliseconds), `lapsed` (true in the
grace period) and the fields of `Profile`: account IDs in base58, `npk` and
`vpk` in hex or `null`, and `texts` as an object. A name past its grace period
is not registered. A gateway
learns every name asked of it and can alter what it serves; a client that needs
privacy or certainty reads the chain itself.

### 12.3 Model Context Protocol

`mcp/server.js` exposes `resolve_name`, `get_page` and `reverse_lookup` as
read-only tools.

### 12.4 Logos module

A Logos core module named `names`, so that any Basecamp app or module,
including a wallet, resolves and manages names with one call and no knowledge of
this document. It is in `basecamp-module/`. It reads the chain itself, so
reading needs no wallet.

```
module names {
  method recipient(name: tstr) -> tstr
  method resolve(name: tstr) -> tstr
  method page(name: tstr) -> tstr
  method reverse(account: tstr) -> tstr
  method normalize(input: tstr) -> tstr

  method wallet() -> tstr
  method mine() -> tstr
  method keep(name: tstr) -> tstr
  method prices() -> tstr
  method register(name: tstr, owner: tstr, private_account: tstr, term: uint) -> tstr
  method renew(name: tstr, payer: tstr, term: uint) -> tstr
  method receive_privately(name: tstr, private_account: tstr) -> tstr
  method set_account(name: tstr, account: tstr) -> tstr
  method set_private_keys(name: tstr, nullifier_public_key: tstr, viewing_public_key: tstr) -> tstr
  method clear_addresses(name: tstr) -> tstr
  method set_text(name: tstr, key: tstr, value: tstr) -> tstr
  method set_page(name: tstr, content_type: tstr, body: tstr) -> tstr
  method transfer(name: tstr, new_owner: tstr) -> tstr
  method release(name: tstr) -> tstr
  method set_primary(account: tstr, name: tstr) -> tstr

  method get_config() -> tstr
  method set_registry(program_id: tstr) -> bool
  method set_sequencer(url: tstr) -> bool
  method set_wallet_files(config_path: tstr, storage_path: tstr) -> bool
}
```

Every method except `normalize` and the four configuration methods answers
with JSON text in the envelope Logos modules use:

```json
{ "success": true,  "value": <answer>, "error": null }
{ "success": false, "value": null,     "error": "why" }
```

**Reading.**

| Method | `value` |
|---|---|
| `recipient("jef.lez")` | `{"name", "display", "public", "public_hex", "private"}`, or `null` if unregistered. `public` is a base58 account ID or `null`; `private` is `{"nullifier_public_key", "viewing_public_key"}` in hex, or `null`. |
| `resolve("jef.lez")` | `{"name", "display", "owner", "expires", "lapsed", "account", "npk", "vpk", "texts"}`, or `null`. `expires` is in milliseconds; `lapsed` is true in the grace period. A name past grace is `null`. |
| `prices()` | `{"treasury", "terms": [{"term", "label", "days", "fee"}]}`: what registering or renewing costs for each term, `fee` as decimal text. |
| `page("jef.lez")` | `{"content_type", "bytes", "text"}`, or `null`. `text` is the body if it is UTF-8. |
| `reverse(account)` | `{"name", "display"}` for the confirmed name, or `null`. |

A wallet's send field calls `recipient` when the input ends in `.lez`, then
sends to the `private` keys if there are any, otherwise to `public_hex`. Both
are in the form the LEZ wallet module's `transfer_shielded`, `transfer_private`
and `transfer_public` take.

`set_wallet_files` names a wallet's files for the module to hand to `lez_core`
when that module is running with no wallet open, so a person does not have to
open the wallet by hand first.

**Changing.** The module holds no keys. It builds the instruction, checks it
against the chain, and asks the LEZ wallet module (`lez_core`) to sign and send
it with `send_generic_public_transaction`. The wallet must be open and must
hold the account that signs, which also pays the fee: the `owner` passed to
`register`, the `account` passed to `set_primary`, and the name's owner for
everything else.

| Method | `value` |
|---|---|
| `wallet()` | `{"accounts": [{"account", "balance", "funded", "name"}], "private_accounts": [{"account", "balance", "funded"}]}`: the open wallet's public accounts and its private accounts, funded ones first. `balance` is decimal text; `name` is the account's confirmed name or `null`. |
| `mine()` | `{"names": [{"name", "display", "owner", "expires", "lapsed"}]}`: the names this device remembers that the open wallet holds. |
| `keep("jef.lez")` | `true`: remember the name on this device, for `mine`. |
| every change | `{"name", "signer", "tx"}`: the transaction was sent. |

`mine` is this device's memory, not the chain's: the module remembers each name
it registers and each name passed to `keep`, and `mine` returns those the open
wallet still holds. A name registered elsewhere appears once it is looked up
and kept.

`term` is 0, 1 or 2 (30 days, 1 year, 5 years); the module reads the fee from
the settings and the time from the chain. `renew` may be paid by any account in
the wallet. `private_account` is one of the wallet's private accounts in base58, or `auto`
for its first one, which is made if the wallet has none. `register` seals a
first profile holding the name and, unless `private_account` is empty, that
account's receiving keys. `receive_privately` puts those keys in the profile
and removes any public account from it.

A change that the chain would refuse is answered with an error and nothing is
sent: the name is taken, the wallet does not hold the owner, the account has no
funds, a key or value is out of range. A change that is sent can still be
refused, so the caller confirms it by calling `resolve` (or `page`, or
`reverse`) until it shows, which takes one block.

## 13. Versioning

The `version` byte in each structure and the `v2` in each derivation string
identify this version. A future version uses new strings, so its identifiers
and accounts never collide with this one's, and a client can support both.
