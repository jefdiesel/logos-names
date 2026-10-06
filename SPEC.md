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
    previous: Option<String>,    // the next older name in the list
    next: Option<String>,        // the next newer name in the list
}
struct TextRecord { key: String, value: String }   // key 1-32 bytes, value at most 512

struct Page { version: u8, content_type: String, body: Vec<u8> }   // body at most 81,920 bytes
struct Directory { version: u8, count: u64, latest: Option<String> }   // count of names now registered
struct PrimaryName { version: u8, name: String }    // the reverse record
```

A text key is 1 to 32 bytes of `a-z`, `0-9`, `-`, `_` and `.`.

The registry holds only what is true now. Registered names form a two-way list
through `previous` and `next`, newest first from the directory's `latest`. A
name that is released is cut out of the list and its record and page are
emptied, so the registry's accounts keep no trace of it or of who held it. The
chain itself still holds every past transaction and event, as any chain does.

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
number now registered. Each record's `previous` is the next older name. Walk
until `previous` is `None`. One account read per name.

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
| 0 | `Register` | `name: String, previous: Option<String>` | record, directory, previous name's record if `previous` is given, owner |
| 1 | `SetAccount` | `name: String, account: AccountId` | record, owner |
| 2 | `SetPrivateKeys` | `name: String, npk: [u8; 32], vpk: Vec<u8>` | record, owner |
| 3 | `ClearAddresses` | `name: String` | record, owner |
| 4 | `SetText` | `name: String, key: String, value: String` | record, owner |
| 5 | `Transfer` | `name: String, new_owner: AccountId` | record, page, owner |
| 6 | `SetPage` | `name: String, content_type: String, body: Vec<u8>` | record, page, owner |
| 7 | `SetPrimary` | `name: String` | owner |
| 8 | `Release` | `name: String, previous: Option<String>, next: Option<String>` | record, page, directory, owner, previous name's record if any, next name's record if any |

Rules:

- Every account is passed with the registry's own shard selected, and `owner`
  must sign.
- `Register` succeeds only if the name is unregistered and `previous` equals the
  directory's current `latest`. A client reads the directory immediately before
  registering, and retries with the new `latest` if it is refused. It sets the
  new record's `previous` and the previous record's `next`.
- Every other instruction on a name succeeds only if the signer is
  `record.owner`.
- `SetText` with an empty value removes the key. `SetPage` with an empty body
  removes the page. `SetPrimary` with an empty name removes the reverse record.
- `Transfer` sets the new owner and clears everything the last owner set:
  `account`, `npk`, `vpk`, the text entries and the page. A transferred name
  never keeps routing payments or messages to its previous owner.
- `Release` gives the name up. `previous` and `next` must equal the record's
  own. The record and page are emptied, the neighbours are linked to each
  other, the directory's `count` drops by one, and `latest` becomes `previous`
  if the released name was the newest. The name is then unregistered and anyone
  may register it.
- A reverse record is not touched by `Transfer` or `Release`. It stops counting
  on its own, because the forward check in section 4 no longer passes.
- A client sends one change per signing account at a time, and waits for it to
  show before sending the next.
- LEZ includes failed transactions in blocks. A client MUST confirm a change by
  reading the account back, not by the transaction's inclusion.

### Test vectors

Instructions, in hex:

| Instruction | Encoding |
|---|---|
| `Register { name: "jef", previous: None }` | `00030000006a656600` |
| `Register { name: "ada", previous: Some("jef") }` | `000300000061646101030000006a6566` |
| `SetText { name: "jef", key: "chat", value: "hi" }` | `04030000006a65660400000063686174020000006869` |
| `Transfer { name: "jef", new_owner: 32 bytes of 0x02 }` | `05030000006a6566` then `02` 32 times |
| `Release { name: "ada", previous: Some("jef"), next: None }` | `080300000061646101030000006a656600` |

## 7. Events

The registry emits LEZ program events. The selector is the first 8 bytes of
`SHA-256("<program>::<EventName>")`; the data is borsh.

| Event | Selector (hex) | Data |
|---|---|---|
| `logos_names::Registered` | `e87848390f741b1a` | `name: String, owner: AccountId` |
| `logos_names::Transferred` | `b78a6f5ae756bf67` | `name: String, from: AccountId, to: AccountId` |
| `logos_names::Released` | `3d2e60d8d0ed2a9a` | `name: String, owner: AccountId` |

Events are emitted before the registry has checked the record, so an event can
exist for a registration, transfer or release that was refused. An indexer MUST treat
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

### 9.4 Logos module (implemented)

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
  method list(limit: uint, before: tstr) -> tstr
  method normalize(input: tstr) -> tstr

  method wallet() -> tstr
  method register(name: tstr, owner: tstr) -> tstr
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
}
```

Every method except `normalize` and the three configuration methods answers
with JSON text in the envelope Logos modules use:

```json
{ "success": true,  "value": <answer>, "error": null }
{ "success": false, "value": null,     "error": "why" }
```

**Reading.**

| Method | `value` |
|---|---|
| `recipient("jef.lez")` | `{"name", "display", "public", "public_hex", "private"}`, or `null` if unregistered. `public` is a base58 account ID or `null`; `private` is `{"nullifier_public_key", "viewing_public_key"}` in hex, or `null`. |
| `resolve("jef.lez")` | The record as in 9.2, plus `display`; or `null`. |
| `page("jef.lez")` | `{"content_type", "bytes", "text"}`, or `null`. `text` is the body if it is UTF-8. |
| `reverse(account)` | `{"name", "display"}` for the confirmed name, or `null`. |
| `list(limit, before)` | As the HTTP list. At most 100 per call; `before` is `""` to start from the newest. |

A wallet's send field calls `recipient` when the input ends in `.lez`, then
sends to `public_hex` or to the `private` keys, which are in the form the LEZ
wallet module's `transfer_public` and `transfer_shielded` take.

**Changing.** The module holds no keys. It builds the instruction, checks it
against the chain, and asks the LEZ wallet module (`lez_core`) to sign and send
it with `send_generic_public_transaction`. The wallet must be open and must
hold the account that signs, which also pays the fee: the `owner` passed to
`register`, the `account` passed to `set_primary`, and the name's owner for
everything else.

| Method | `value` |
|---|---|
| `wallet()` | `{"accounts": [{"account", "balance", "funded", "name"}]}`: the open wallet's public accounts, funded ones first. `balance` is decimal text; `name` is the account's confirmed name or `null`. |
| every change | `{"name", "signer", "tx"}`: the transaction was sent. |

A change that the chain would refuse is answered with an error and nothing is
sent: the name is taken, the wallet does not hold the owner, the account has no
funds, a key or value is out of range. A change that is sent can still be
refused, so the caller confirms it by calling `resolve` (or `page`, or
`reverse`) until it shows, which takes one block.

## 10. Versioning

The `version` byte in each structure and the `v1` in each seed identify this
version. A future version uses new seeds, so its accounts never collide with
version 1's, and a client can support both.
