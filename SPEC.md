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
  name usable. The derivation is slow and memory-hard on purpose (Argon2id,
  64 MiB, about 70 to 140 ms per name on a current laptop). But its salt is the
  same for every name and every registry, and for testnet and mainnet alike so
  that a name carries over between networks, so that cost is paid once per
  candidate name, not per registry: a dictionary of likely names is computed
  once and then checks every registry, now and later, for free. The memory
  requirement is what limits a GPU: one RTX 4090 does roughly 1,700 derivations
  a second at these parameters (an estimate from hashcat's Argon2 mode),
  against roughly 90,000 a second for the PBKDF2 version 2 used, so a
  dictionary costs about 50 times more to run. Five million common names still
  take under an hour on one GPU. Treat any name someone might guess as public.
- **Who holds a name is not stored.** A record keeps an owner tag,
  `SHA-256("logos-names/v5/owner" ‖ id ‖ account)`, in place of its holder: it
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
- **A public holder can be found, at any time.** A holder acting from a public
  account signs each change, so the chain shows that account touching some
  record. Worse, the owner tag is a plain hash: anyone who knows a name can
  test every public account that ever appeared on chain against its record
  and find the holder, long after the fact, without having watched anything.
  Only a private-account holder is hidden. Whoever serves the RPC, holding a
  dictionary of name identifiers, can also tell which names a client looks up.
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
master = Argon2id(password = name, salt = "logos-names/v5",
                  version = 0x13, memory = 65536 KiB, passes = 3,
                  lanes = 1, length = 32 bytes)
id     = SHA-256("logos-names/v5/id"  ‖ master)
key    = SHA-256("logos-names/v5/key" ‖ master)
```

`name` is the normalised name, as UTF-8 bytes (names are ASCII). Argon2id is
the variant of RFC 9106; no secret and no associated data are used. `id` is
public: it is what the chain knows the name by. `key` is never sent anywhere:
it opens and seals what the name resolves to. Neither can be turned back into
the name except by guessing.

The salt is one fixed string, shared by every registry and every network. That
is deliberate: a name keeps the same identifier and key on testnet and on
mainnet, so a name moves between them without being derived again. The price is
the one section 1 states: a dictionary computed for one registry serves all.

The derivation needs 64 MiB of memory and takes about 70 to 140 ms on an Apple
M4 in native code (about 0.5 s in pure JavaScript). Memory is what makes it
costly on a GPU: a dictionary run is about 50 times dearer than with the
100,000-round PBKDF2 of version 2. A client SHOULD derive once per name and
keep the result. A server that derives for each request pays this cost for
every request, including requests for names that do not exist, so a gateway
SHOULD NOT derive on behalf of anonymous callers.

## 4. Where a name lives

A registry is one LEZ program, identified by its account ID (the `program`
below). All of a registry's data is in **that program's shard** on three kinds
of account, and one settings account.

| Data | Account ID |
|---|---|
| Name record | `PDA(program, SHA-256("logos-names/v5/name/" ‖ id))` |
| Page | `PDA(program, SHA-256("logos-names/v5/page/" ‖ id))` |
| Reverse record | the account itself |
| Settings | `PDA(program, SHA-256("logos-names/v5/config"))` |

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
`logos-names/v5/record` for a record and `logos-names/v5/page` for a page. A
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
| `display` | A display name, shown on the name's profile card. |
| `bio` | A short text about the holder, shown on the profile card. |
| `avatar` | A small image as a `data:image/png`, `jpeg`, `gif` or `webp` base64 URL, kept within the record's 12 KiB. A viewer shows only inline images, so showing one fetches nothing. |
| `color` | The card's accent colour, `#rrggbb`. |
| `theme` | The card's theme: `light` (the default), `dark` or `auto` (follows the viewer). |
| `font` | The card's font: `sans` (the default), `serif`, `mono` or `rounded`, from the viewer's own system fonts. |
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
| 0 | `Register` | `id, sealed: Vec<u8>, term: u8, fee: u128, now: u64, secret: [u8; 32]` | record, page, settings, commitment, payer, then treasury if `fee` is not 0 |
| 1 | `Renew` | `id, term: u8, fee: u128, now: u64` | record, settings, payer, then treasury if `fee` is not 0 |
| 2 | `SetRecord` | `id, sealed: Vec<u8>` | record, owner |
| 3 | `SetPage` | `id, sealed: Vec<u8>` | record, page, owner |
| 4 | `Transfer` | `id, new_owner: [u8; 32]` (the new holder's owner tag) | record, page, owner |
| 5 | `Release` | `id` | record, page, owner |
| 6 | `SetPrimary` | `name: String` | owner |
| 7 | `Configure` | `admin: AccountId, treasury: AccountId, prices: [u128; 3]` | settings, signer (the program's own account the first time, then the admin) |
| 8 | `Commit` | `commitment: [u8; 32], now: u64` | commitment, signer |

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
- **Claiming is commit, then reveal** (registry version 4), so nobody watching
  pending transactions can register an identifier first. `Commit` stores, at
  `PDA(program, SHA-256("logos-names/v4/commit/" ‖ c))`, when it was made, where
  `c = SHA-256("logos-names/v4/commitment" ‖ id ‖ tag ‖ secret)`, `tag` is the
  owner tag of the account that will reveal it and `secret` is 32 random bytes
  the client keeps. `Register` names that commitment account and `secret`; the
  program recomputes `c` with the payer's tag, so a reveal seen pending can't
  be replayed for another account. The commitment must have been made at least
  15 minutes and less than 24 hours before the reveal's `now`; the reveal uses it
  up. `Commit` and `Register` are valid only in a block whose timestamp is in
  `[now, now + 10 minutes)` (room for a private holder's proof), so a claimed
  time is at most 10 minutes early and at least 5 minutes really pass between a commitment and its reveal: longer than a
  reveal waits to be included, so a commitment made after seeing one loses.
  A `Commit` refuses to overwrite a commitment that can still be revealed.
- `Register` succeeds if the name is unregistered, or if its record's
  `expires + grace <= now`. The payer becomes the owner (its tag is stored), `expires` becomes
  `now` plus the term, and any page left by a lapsed holder is emptied. Its
  `sealed` is the name's first profile and may be empty.
- `Renew` succeeds while `now < expires + grace`, for any payer. `expires`
  becomes the later of `expires` and `now`, plus the term.
- `Configure` sets the settings. While the settings account is empty only the
  program's own account may sign it: the account the program was deployed at,
  whose key only the deployer holds. Afterwards only the settings' `admin` may.
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

### 8.1 The short-name list (v5)

The chain never sees a name, so the length rules of v5 (1-2 character names
locked, 3-character names at a higher deposit) are enforced with a fixed list of
the ids of all 49,284 names of 1 to 3 characters (36 + 1,296 + 47,952), derived
with the normal name derivation. The program has the list's Merkle root built
in; the list itself (about 1.6 MB, `shortlist.bin`) ships with the CLI and any
other client, which builds a proof from it for each registration.

- A leaf is `(id, tier)`: tier 1 for a 1-2 character name, 3 for a 3-character
  name. Two sentinel leaves, `id = 00..00` and `id = ff..ff`, tier `0xff`, are
  added and are never names. Leaves are sorted by id, strictly ascending.
- `leaf = SHA-256("logos-names/v5/short-leaf" || id || tier)`,
  `node = SHA-256("logos-names/v5/short-node" || left || right)`,
  `empty = SHA-256("logos-names/v5/short-empty")`.
- The leaf count (49,286) is padded with `empty` to the next power of two
  (65,536), so every path has exactly 16 hashes. Level 0 is the leaf hashes;
  each node hashes two neighbours (even index on the left). A path lists the
  sibling at each level from the leaf up; bit `k` of the index says whether the
  node at level `k` is a left (0) or right (1) child.
- A registration carries a borsh `LengthProof`: `Membership { index: u32, tier:
  u8, path }` (the id is a leaf with that tier; 521 bytes) or `NonMembership {
  left: Leaf, left_path, right: Leaf, right_path, left_index: u32 }` (the two
  adjacent leaves at `left_index` and `left_index + 1` with `left.id < id <
  right.id`; 1,098 bytes), each plus one byte for the enum variant. The program
  rejects a wrong path length, an out-of-range index, a membership claim whose
  tier is not 1 or 3 (so a sentinel or a padding slot never passes), and
  non-adjacent or non-surrounding leaves. A non-membership proof means a normal
  name.
- `shortlist generate <dir>` (tools/shortlist) writes `shortlist.bin` (33 bytes
  per leaf, `id || tier`, sentinels included) and `root.hex`;
  `shortlist verify <dir>` rebuilds the root and spot-checks proofs.

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

Argon2id itself, RFC 9106 section 5.3 (version 0x13, 32 KiB, 3 passes, 4 lanes,
with a secret and associated data): password `01` × 32, salt `02` × 16, secret
`03` × 8, associated data `04` × 12, 32 bytes out:
`0d640df58d78766c08c037a34a8b53c9d01ef0452d75b65eb52520e96b01e659`.
A client MUST reproduce this before trusting its Argon2id.

Derivation (section 3), generated by the Rust implementation and reproduced
byte for byte by the JavaScript one:

| Name | Value | Hex |
|---|---|---|
| `jef` | `master` | `60e3df20e1c473ab71c666b913af96619fa14b49593e92598bd7709582e5a74e` |
| | `id` | `7041d9e447f06bdfe327d870402b1524cfe38bd03445b8fc3d0e87bc978eb763` |
| | `key` | `aad91a1a3dd0fb56a213212fbea58fe25d9139db42fc59ce2116c3cfa3440088` |
| `logos` | `master` | `140f118f9f0c7a99f04e3214d8e084590cd69687be6effb7f57dcc1a06a35372` |
| | `id` | `4a8bf61b00dbc63907a935aa15ed534bb32d866978aa1f2128f9b6f37e0c4025` |
| | `key` | `d67dd628fa51fa62bd32ea82c3e9ff1b67f93b01103be956eaebc52f1699df44` |
| `a` | `master` | `c42903b96a43d01c6c912b647c21902bffd17afdf932e5f8c407dea08a32b494` |
| | `id` | `24b73f0b70605c7f27fb91f9952b550e13c4cc38dcf325be45442d1a2cb085b4` |
| | `key` | `cd9ded7c94d52743db4ee67152766480be5bb5199e83b6696e502cf818933150` |
| `abcdefghijklmnopqrstuvwxyz012345` | `master` | `a9296b3bfd39d63908326e19c2d1b20a27cb6b47df1829725c68355bc3585f5b` |
| | `id` | `0044f00be946536689d83e364ac25462c65c615d74059652d1f3e2977bd75403` |
| | `key` | `79c7aa8c06d4983a59b235915ae4f5767918824a9150d5d8b576761b7d3654f6` |
| `a-b-c` | `master` | `c098ecc6c6af9cfcf449666ce012e3e930e076d870cb135ed8f1f058e7728362` |
| | `id` | `707decf4fde6d9fb61d255c23b34e042c846a0f4b0834151dcb2c1ef4160e849` |
| | `key` | `e3666a8572f7cabe94f6717491bff34c70eeca216436a142185bd8a86b6ccf31` |

With `program` = 32 bytes of `0x07` (base58 `US517G5965aydkZ46HS38QLi7UQiSojurfbQfKCELFx`):

| Account | Base58 |
|---|---|
| Record of `jef` | `5b5xVGob2GYvrq21MSuxm1MfieucDEy9yxhDwdXndbpD` |
| Page of `jef` | `HEES9BpNxrAtwxFkyEQTEnjUcSr7usgbefeqiYJsG9mY` |
| Record of the identifier `00 01 02 … 1f` | `7bwwvMLtxqqNYiXsd5bYaq8e31E1odYf6jAK5z7tmUdd` |
| Page of the identifier `00 01 02 … 1f` | `8M4vmdxCXL4eS8QXiHSdbeUUPt5Vbx1UVjLcv2cPk9Y5` |

Sealing, under the key of `jef` above, with the nonce `00 01 02 … 0b` (a real
client uses a random one). The profile is `{ name: "jef", account: 32 bytes of
0x02, npk: none, vpk: empty, texts: [("chat", "hi")] }`, 63 bytes, padded to
256:

```
000102030405060708090a0b7a39f7ff67b20894263a28d040c0f2711c04ba247fcf5cb107e27b59
05b8a0664b28cd25512addcb74af3efa5a8d6eb3bbb8e89fb8dba0aa2739252efed2be01ab019998
a07d416c788d8c9b699891e85bedd1118da7089377220bdb0ab8ae29661bf20e006c5fbffb222237
2ea0a2589ed49023186a77e9afee1af338f4a2377afa40da018789ca715318d981b556f71f9640eb
d056d8dcbe2b71845b45a578721ccc2c889df37b926c7da5a5202dbf3d50dc04883673134ce0772f
bcd7fbc8f31f455bb39b3d867d5e6772c86e59f42a9d04ffaacaa6119b1156d238665c0cf50028a9
cbf8b3189fc45a15a724c5775a835a8ccdff6a573d8e046d78e847b7da5668bc8529aedb3bdc70a9
9db9b9b4
```

The page `{ content_type: "text/plain", body: "hi" }`, same nonce:

```
000102030405060708090a0b7339f7ff79b216e10b4846b32bacf2731e06d04f8cf43af535da58e9
92f033cb16a471d0
```

Instructions, with `id` = `00 01 02 … 1f`. `term` is one byte, `fee` 16 bytes
and `now` 8 bytes, little-endian:

| Instruction | Encoding |
|---|---|
| `Register { id, sealed: [ee, ff], term: 1, fee: 100000, now: 1700000000000, secret: 32 × 0x5a }` | `00` ‖ id ‖ `02000000eeff` ‖ `01` ‖ `a0860100000000000000000000000000` ‖ `0068e5cf8b010000` ‖ `5a` 32 times |
| `Renew { id, term: 2, fee: 0, now: 1700000000000 }` | `01` ‖ id ‖ `02` ‖ `00` 16 times ‖ `0068e5cf8b010000` |
| `SetRecord { id, sealed: [ee] }` | `02` ‖ id ‖ `01000000ee` |
| `SetPage { id, sealed: [] }` | `03` ‖ id ‖ `00000000` |
| `Transfer { id, new_owner: 32 bytes of 0x02 }` | `04` ‖ id ‖ `02` 32 times |
| `Release { id }` | `05` ‖ id |
| `SetPrimary { name: "jef" }` | `06030000006a6566` |
| `Configure { admin: 32 × 0x01, treasury: 32 × 0x02, prices: [1, 2, 3] }` | `07` ‖ `01` 32 times ‖ `02` 32 times ‖ three 16-byte little-endian numbers |
| `Commit { commitment: 32 × 0xc0, now: 1700000000000 }` | `08` ‖ `c0` 32 times ‖ `0068e5cf8b010000` |

## 11. Canonical registries

Anyone can deploy a copy of the program. Clients on a network must agree on one
registry, as they agree on one token contract.

| Network | Registry program ID |
|---|---|
| Logos testnet v0.3 (LEZ) | `38jEfVV1RCyMzu1FGbYcCn5FqxCxKyY58VgzjruAh2gq` (registry v5, deployed 2026-10-16, CI build `322dda10…c4ce`, deposit mode: 100,000,000 normal, 2,000,000,000 three-character; replaces `2PXg…zCkX`, v4.1, whose names use the old PBKDF2 derivation and do not carry over) |

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

### 12.2 Viewing pages

A name's page is read and opened on the viewer's own machine; no server sits
between the viewer and the chain, and none learns which name was opened.

`names open <name>` reads the page from the chain, opens it with the name's key
and serves it over HTTP on the loopback interface only (`127.0.0.1`, a free port
unless `--port` is given), then opens the browser there. A viewer that does this
must:

- listen on loopback only, and answer only requests whose `Host` is its own
  `127.0.0.1:<port>` or `localhost:<port>` (another site's page could otherwise
  reach it through DNS rebinding);
- serve the page with a policy that loads nothing from the network:
  `Content-Security-Policy: sandbox allow-scripts allow-popups; default-src 'none';
  script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:;
  font-src data:; media-src data:; form-action 'none'; base-uri 'none'`, plus
  `Referrer-Policy: no-referrer`. A page is self-contained: anything it shows is
  inline or a `data:` URL, so opening a name tells no third party.

Without a page, a viewer shows a profile card built on the viewer's machine from
the name's entries (`display`, `bio`, `avatar`, how to pay it, and the other
entries), every value HTML-escaped; `names open` serves it at `/` when there is no
page and always at `/profile`.
A `cid` made only of letters and digits is linked to the viewer's own Logos
Storage node (`--storage`, default `http://127.0.0.1:8080`,
`/api/storage/v1/data/<cid>/network/stream`), never to a public gateway.

### 12.3 In a browser (once sequencers allow it)

A web page can read the chain directly only if the sequencer's RPC allows
cross-origin requests (CORS: it answers the `OPTIONS` preflight and sends
`Access-Control-Allow-Origin`). The LEZ testnet does not yet. When a sequencer
does, a static page resolves names entirely in the visitor's browser:

- the name is carried after `#` (`https://<site>/#jef.lez`), which browsers
  never send to the server, so the host of the static files cannot see it;
- the page derives the identifier and key (section 3), reads the record and page
  accounts from the sequencer's JSON-RPC itself, and opens them;
- it shows the page in an `<iframe sandbox="allow-scripts allow-popups">` whose
  document carries the policy of 12.2, so the page has an opaque origin and
  loads nothing from the network.

No relay or proxy may stand between the page and the sequencer: one would see
every visitor's address and the record addresses they read, which a dictionary
of names turns back into names (section 1). A site whose sequencer does not
allow cross-origin requests shows no names and points to `names open`.

### 12.4 Model Context Protocol

`mcp/server.js` exposes `resolve_name`, `get_page` and `reverse_lookup` as
read-only tools.

### 12.5 Logos module

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

The `version` byte in each structure and the `v5` in each derivation string
identify this version. A future version uses new strings, so its identifiers
and accounts never collide with this one's, and a client can support both.
