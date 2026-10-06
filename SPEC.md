# Logos Names: specification

Version 1, draft. This document is the contract between the name registry and
everything that uses it: wallets, apps, indexers, gateways and agents. A client
that follows it resolves the same names to the same answers as every other
client, without depending on this repository's code.

Status of each part is marked **implemented** (in this repository and tested)
or **proposed** (designed, not built).

## 1. Names

- A name is 1 to 32 bytes of `a-z`, `0-9` and `-`, and does not start or end
  with `-`.
- People write a name with the suffix `.lez`: `jef.lez`. The suffix is not
  stored anywhere.
- **Normalisation.** Before using input as a name, a client trims surrounding
  whitespace, lowers ASCII case, and removes one trailing `.lez`. It then
  rejects the result unless it is a valid name. `Jef.LEZ` becomes `jef`;
  `jef.eth` is rejected.
- A client displays names with the suffix.

## 2. Where a name lives

A registry is one LEZ program, identified by its account ID (the `program`
below). All of a registry's data is in **that program's shard** on four kinds
of account.

| Data | Account ID |
|---|---|
| Name record | `PDA(program, SHA-256("logos-names/v1/name/" ‖ name))` |
| Page | `PDA(program, SHA-256("logos-names/v1/page/" ‖ name))` |
| Directory | `PDA(program, SHA-256("logos-names/v1/directory"))` |
| Reverse record | the account itself |

`PDA(program, seed)` is LEZ's public program-derived address:

```
SHA-256( pad32("/LEE/v0.2/AccountId/PDA/") ‖ program (32 bytes) ‖ seed (32 bytes) )
```

where `pad32` right-pads the ASCII string with zero bytes to 32 bytes, and
`name` is the normalised name as ASCII.

A shard that is absent or empty means "not set": the name is unregistered, has
no page, or the account has no reverse record.

### Test vectors

With `program` = 32 bytes of `0x07` (base58 `US517G5965aydkZ46HS38QLi7UQiSojurfbQfKCELFx`):

| Account | Base58 |
|---|---|
| Record of `jef` | `28mUmKUyH1dJgmwdjVxS22P12iUHVotLcvk2nbrFw29Y` |
| Page of `jef` | `6ckRpxviqmmhY6QkcJPVPWYSSy3eRZGzy1uUGStf1TnC` |
| Directory | `9oQ7KANHqw4ok3goEHzgeK7WSxdKqKNXA7bS9HHpq97p` |

## 3. Data layouts

All data is [borsh](https://borsh.io)-encoded. `String` is a `u32` length and
UTF-8 bytes; `Vec<u8>` is a `u32` length and bytes; `Option<T>` is one tag byte
(`0` none, `1` some) then `T`; integers are little-endian; `AccountId` is 32
raw bytes.

```rust
struct NameRecord {
    version: u8,                 // 1
    name: String,
    owner: AccountId,            // may change or transfer the record
    account: Option<AccountId>,  // public account the name is an alias for
    npk: Option<[u8; 32]>,       // nullifier public key, for private payments
    vpk: Vec<u8>,                // viewing public key: empty, or 1184 bytes
    texts: Vec<TextRecord>,      // at most 16
    previous: Option<String>,    // the name registered before this one
}
struct TextRecord { key: String, value: String }   // key 1-32 bytes, value at most 512

struct Page { version: u8, content_type: String, body: Vec<u8> }   // body at most 81,920 bytes
struct Directory { version: u8, count: u64, latest: Option<String> }
struct PrimaryName { version: u8, name: String }    // the reverse record
```

A text key is 1 to 32 bytes of `a-z`, `0-9`, `-`, `_` and `.`.

## 4. Resolving

**Forward (name to record).** Normalise the name, derive the record account,
read the registry's shard on it, decode `NameRecord`. One account read.

**Paying a name.**

- Public: send to `record.account`. If it is `None`, the name does not accept
  public payments.
- Private: send a shielded transfer to the key pair (`record.npk`,
  `record.vpk`) with a fresh random identifier. If `npk` is `None`, the name
  does not accept private payments.
- A wallet should show the user the resolved account before sending, as it
  would for a pasted address.

**Reverse (account to name).** Read the registry's shard on the account and
decode `PrimaryName`. This is only a claim, set by the account itself. A client
MUST then resolve that name forward and accept it only if `record.owner` or
`record.account` equals the account. Otherwise the account has no name.

**Listing.** Read the directory; `latest` is the newest name and `count` the
total. Each record's `previous` is the next older name. Walk until `previous`
is `None`. One account read per name.

## 5. Text entries

Text entries are free-form. These keys have an agreed meaning; others are
application-defined and should be namespaced, for example `myapp.setting`.

| Key | Value |
|---|---|
| `chat` | The owner's Logos Chat address: 64 lowercase hex characters, as returned by `chat_module.get_address()`. In Chat 0.3.0 it changes each time the app starts, so it can be stale. |
| `inbox` | A Logos Delivery content topic, `/{app}/{version}/{name}/{encoding}`. A topic routes only: anyone can read or publish to it. |
| `agent` | A URL or address where the owner's AI agent can be reached. |
| `cid` | A Logos Storage content ID the name pins. |

## 6. Changing a name

Instructions are a borsh enum, in this order (the variant index is the first byte):

| # | Instruction | Fields | Accounts, in order |
|---|---|---|---|
| 0 | `Register` | `name: String, previous: Option<String>` | record, directory, owner |
| 1 | `SetAccount` | `name: String, account: AccountId` | record, owner |
| 2 | `SetPrivateKeys` | `name: String, npk: [u8; 32], vpk: Vec<u8>` | record, owner |
| 3 | `ClearAddresses` | `name: String` | record, owner |
| 4 | `SetText` | `name: String, key: String, value: String` | record, owner |
| 5 | `Transfer` | `name: String, new_owner: AccountId` | record, owner |
| 6 | `SetPage` | `name: String, content_type: String, body: Vec<u8>` | record, page, owner |
| 7 | `SetPrimary` | `name: String` | owner |

Rules:

- Every account is passed with the registry's own shard selected, and `owner`
  must sign.
- `Register` succeeds only if the name is unregistered and `previous` equals the
  directory's current `latest`. A client reads the directory immediately before
  registering, and retries with the new `latest` if it is refused.
- Every other instruction on a name succeeds only if the signer is
  `record.owner`.
- `SetText` with an empty value removes the key. `SetPage` with an empty body
  removes the page. `SetPrimary` with an empty name removes the reverse record.
- `Transfer` sets the new owner and clears `account`, `npk` and `vpk`, so a
  transferred name never keeps routing payments to its previous owner. Text
  entries and the page are kept.
- LEZ includes failed transactions in blocks. A client MUST confirm a change by
  reading the account back, not by the transaction's inclusion.

## 7. Events

The registry emits LEZ program events. The selector is the first 8 bytes of
`SHA-256("<program>::<EventName>")`; the data is borsh.

| Event | Selector (hex) | Data |
|---|---|---|
| `logos_names::Registered` | `e87848390f741b1a` | `name: String, owner: AccountId` |
| `logos_names::Transferred` | `b78a6f5ae756bf67` | `name: String, from: AccountId, to: AccountId` |

Events are emitted before the registry has checked the record, so an event can
exist for a registration or transfer that was refused. An indexer MUST treat
events as hints and confirm each against the record. With that check, events
give an indexer what the on-chain list cannot do cheaply: names by owner,
search, and history.

## 8. Canonical registries

Anyone can deploy a copy of the program. Clients on a network must agree on one
registry, as they agree on one token contract.

| Network | Registry program ID |
|---|---|
| Logos testnet v0.3 (LEZ) | not deployed yet |

The deployed program is the reproducible build produced by this repository's
CI; its SHA-256 is recorded with each release. The program is upgradeable by
whoever holds the key of its header account.

## 9. Interfaces

The sections above are enough to implement a client. These interfaces exist so
that most clients do not have to.

### 9.1 Libraries (implemented)

- Rust: the `names_core` crate. `normalize_name`, `validate_name`,
  `record_address`, `page_address`, `directory_address`, the record types, and
  the program's own `plan` and `apply`, which a client can run locally to check
  a transaction before sending it.
- JavaScript: `resolver/names.js`, with no dependencies. `normalizeName`,
  `resolveName`, `resolvePage`, `reverseLookup`, `listNames` and the decoders.

### 9.2 HTTP (implemented, by the gateway)

| Request | Response |
|---|---|
| `GET https://<name>.<gateway>/` or `GET https://<gateway>/<name>` | The name's page, with its content type |
| `GET …/.well-known/logos-name.json` for a name | The record as JSON, or 404 `{"error": "not registered"}` |
| `GET https://<gateway>/.well-known/logos-names.json?limit=&before=` | `{"total", "names": [{"name", "owner"}], "more"}`, newest first |

The record JSON has the fields of `NameRecord`: account IDs in base58, `npk`
and `vpk` in hex or `null`, and `texts` as an object. A gateway can alter what
it serves; a client that needs certainty reads the chain itself.

### 9.3 Model Context Protocol (implemented)

`mcp/server.js` exposes `resolve_name`, `get_page`, `list_names` and
`reverse_lookup` as read-only tools.

### 9.4 Basecamp module (proposed)

A Basecamp core module named `names`, so that any Basecamp app, including a
wallet, resolves a name with one call and no knowledge of this document:

| Method | Returns |
|---|---|
| `resolve(name)` | The record as JSON, or `null` |
| `recipient(name)` | `{"public": <account or null>, "private": {"npk", "vpk"} or null}` |
| `reverse(account)` | The confirmed name, or `null` |
| `list(limit, before)` | As the HTTP list |
| `register(name, owner)`, `set_text(name, key, value)`, and the other changes | The transaction result, after reading the change back |

A wallet's send field would call `recipient` when the input ends in `.lez`.

## 10. Versioning

The `version` byte in each structure and the `v1` in each seed identify this
version. A future version uses new seeds, so its accounts never collide with
version 1's, and a client can support both.
