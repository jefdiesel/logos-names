# `lez_nft` v3 Protocol Specification (LEZ v0.4.0)

`VERSION = 3` · Target runtime: **LEZ v0.4.0** (actor model) · **RISC Zero 3.1.0** · Encoding: **Borsh 1.5.7**.

> Encodings changed on branch `v3-lez04` to match the LNS registry; see `ENCODING-CHANGES.md`.

---

## 1. Overview & Actor Model

In LEZ v0.4.0, programs execute as single-actor message handlers (`lee_core::actor_entrypoint!(lez_nft_core::handle)`). `lez_nft` v3 stores all mutable state in Program-Derived Accounts (PDAs):

| Account | Kind | PDA Seed (`SHA-256(...)`) | Stored Type |
|---|---|---|---|
| Program config | Public PDA | `"lez-nft/v3/config"` | `Config` |
| Collection record | Public PDA | `"lez-nft/v3/collection/" \|\| collection_id` | `CollectionRecord` |
| Token record | Public PDA | `"lez-nft/v3/token/" \|\| token_id` | `NftRecord` |
| Shielded LP-0001 deed | Private PDA | `for_private_pda(NFT, SHA-256("lez-nft/v3/deed/" \|\| token_id), npk, vpk)` | `PrivateDeed` |

Additional hash domains:

| Helper | Formula (`SHA-256(...)`) |
|---|---|
| `serial_token_id(collection_id, serial)` | `"lez-nft/v3/serial/" \|\| collection_id \|\| serial.to_le_bytes()` |
| `owner_tag(token_id, account)` | `"lez-nft/v3/owner" \|\| token_id \|\| account` (unlinked tokens) |
| `linked_owner_tag(target, account)` | `"logos-names/v5/owner" \|\| target \|\| account` (linked LNS tokens) |
| `creator_tag(collection_id, account)` | `"lez-nft/v3/creator" \|\| collection_id \|\| account` |
| `lns_collection_id(registry_program)` | `"logos-names/v8/collection/" \|\| registry_program` (convention) |

Limits & constants:
- `VERSION: u8 = 3`
- `MAX_CONTENT_LEN: usize = 10_240` (10 KiB)
- `MAX_SEALED_LEN: usize = 8_192` (8 KiB)
- `MAX_SUPPLY: u32 = 1_000_000`
- `MAX_AUTHORITIES: usize = 32`
- `VPK_LEN: usize = 1184` (ML-KEM-768 encapsulation key)
- `TIME_WINDOW_MS: u64 = 600_000` (10 minutes; bounded validity window `[now, now + TIME_WINDOW_MS)`)
- `REGISTRY_NFT_TRANSFER_HOOK_TAG: u8 = 3` (variant index of `names_core::Message::NftTransferHook`)

---

## 2. Cryptographic Ownership Alignment

To prevent split ownership between `names_core` and `lez_nft` without ever exposing a private recipient's raw `AccountId`, every linked token (`record.linked.is_some()`) computes its `OwnerTag` with the **exact same formula** as LNS:

```text
linked_owner_tag(target, account) = SHA-256("logos-names/v5/owner" || target || account)
```

where `target = slot_id` for top-level names and `target = sub_id` for permanent subnames. Both `NameRecord.owner` (or `SubnameRecord.owner`) and `NftRecord.owner` hold the exact same 32-byte tag.

When a private holder initiates `Instruction::Transfer` on a linked token, the root actor `(signer, NFT)` runs inside the privacy circuit, computes `signer_tag = linked_owner_tag(&target, &signer)` from the caller-supplied `linked_target: Some(target)`, and crosses to public `(token_pda, NFT)` carrying only the 32-byte `OwnerTag` (never the raw `AccountId`). `(token_pda, NFT)` asserts `linked_target == Some(linked.target)` and `record.owner == signer_tag`.

---

## 3. Account Data Layouts (Borsh)

```rust
pub struct AuthorizedProgram {
    pub program: AccountId,                 // 32 bytes
    pub collection_id: Option<CollectionId>,// 1 + [32] bytes
}

pub struct Config {
    pub version: u8,                        // 3
    pub admin: AccountId,                   // 32 bytes
    pub authorities: Vec<AuthorizedProgram>,// u32 len + entries
}

pub enum Payee {
    Public(AccountId),                      // 0x00 || AccountId (32)
    Private { npk: [u8; 32], vpk: Vec<u8> },// 0x01 || npk (32) || u32 len || vpk (1184)
}

pub struct LinkedAuthority {                // 128 bytes, = names_core::nft::LinkedAuthority
    pub program: AccountId,                 // 32 bytes (registry program ID)
    pub authority_actor: AccountId,         // 32 bytes (registry settings PDA: the only AuthoritySync sender)
    pub target: [u8; 32],                   // 32 bytes (the name's slot_id; top-level names only)
    pub record_actor: AccountId,            // 32 bytes (registry record PDA: receives the transfer hook)
}

pub struct NftRecord {
    pub version: u8,                        // 3
    pub token_id: [u8; 32],                 // 32 bytes
    pub owner: [u8; 32],                    // 32 bytes
    pub collection: Option<([u8; 32], u32)>,// 1 + [36] bytes
    pub content: Vec<u8>,                   // u32 len + bytes (<= 10 KiB)
    pub sealed: Vec<u8>,                    // u32 len + bytes (<= 8 KiB)
    pub linked: Option<LinkedAuthority>,    // 1 + [128] bytes
    pub shielded: bool,                     // 1 byte
    pub expires: u64,                       // 8 bytes LE (u64::MAX for non-expiring)
    pub frozen: bool,                       // 1 byte (authority freeze flag)
    pub deed_account: Option<AccountId>,    // 1 + [32] bytes (private deed PDA while shielded)
}

pub struct PrivateDeed {
    pub version: u8,                        // 3
    pub collection_id: [u8; 32],            // 32 bytes
    pub token_id: [u8; 32],                 // 32 bytes
    pub expires: u64,                       // 8 bytes LE
} // fixed 73 bytes

pub struct CollectionRecord {
    pub version: u8,                        // 3
    pub collection_id: [u8; 32],            // 32 bytes
    pub creator: [u8; 32],                  // 32 bytes
    pub payee: Option<Payee>,
    pub price: u128,                        // 16 bytes LE
    pub supply: u32,                        // 4 bytes LE
    pub minted: u32,                        // 4 bytes LE
    pub content: Vec<u8>,
}
```

---

## 4. Wire Messages (`Message`)

| Tag | Variant | Target Actor | Caller Requirement |
|---:|---|---|---|
| `0` | `Message::Instruction(Instruction)` | `(signer, NFT)` | Root call (`input.from.is_none()`), `input.is_authorized()` |
| `1` | `Message::Effect(Effect)` | `(pda, NFT)` | Internal only (`input.from_own_program()`) |
| `2` | `Message::AuthoritySync(AuthorityOp)` | `(token_pda, NFT)` | `input.from == Some(Actor::new(linked.authority_actor, linked.program))` (and `Config` check on `Mint`) |
| `3` | `Message::ReadState` | Any `(actor, NFT)` | Chained call (`input.from.is_some()`); replies with `StateReply` + lent grant |
| `4` | `Message::RequireHolder { tag, now }` | `(token_pda, NFT)` | Any caller; asserts owner == `tag`, `!frozen`, and if `now == Some(t)`, `t < expires` + `[t, t + TIME_WINDOW_MS)` |

### 4.1 `Instruction` Variants (`0x00 || tag || fields`)

| Tag | Variant | Fields | Notes |
|---:|---|---|---|
| `0` | `CreateCollection` | `collection_id, payee, price, supply, content` | Refuses `collection_id` reserved by any authority in `Config`. |
| `1` | `MintFromCollection` | `collection_id, serial, price, payee, payee_opening, owner` | If `price > 0`, requires native balance grant on `signer` and emits `native_token::Message::Transfer`. |
| `2` | `Mint` | `token_id, owner, content, sealed` | Mints an unlinked one-off (`expires = u64::MAX`, `frozen = false`, `shielded = false`). |
| `3` | `Transfer` | `token_id, new_owner, sealed, now, linked_target` | Unlinked: `linked_target = None`. Linked: `linked_target = Some(linked.target)`, `now = Some(t)`, checks `t < expires`, bounds `[t, t + TIME_WINDOW_MS)`, emits `RegistryTransferHook`. Refuses if `shielded` or `frozen`. |
| `4` | `Burn` | `token_id` | Unlinked, unshielded, unfrozen tokens only. Refused on linked tokens. |
| `5` | `Shield` | `token_id, deed_account` | Unlinked, unshielded, unfrozen tokens only. Writes `PrivateDeed` to private PDA `deed_account` via `private_seed_grant`. |
| `6` | `Unshield` | `token_id, deed_account` | Unlinked, shielded tokens only. Clears `PrivateDeed` in `deed_account`. |
| `7` | `Configure` | `admin, authorities` | First call requires `signer == admin`; updates require `signer == existing.admin`. Rejects duplicate programs or duplicate reserved collection IDs. |

### 4.2 `AuthorityOp` Variants (`0x02 || tag || fields`)

Sent synchronously from `(linked.authority_actor, linked.program)` (the registry's settings PDA) to `(token_pda, NFT_PROGRAM)`.

| Tag | Variant | Fields | Auth & Rules |
|---:|---|---|---|
| `0` | `Mint` | `token_id, owner, linked, content, expires` | Asserts `pre_state.is_empty()` and `from == (linked.authority_actor, linked.program)`; sets `collection = Some((lns_collection_id(linked.program), 0))`. Emits `Effect::RequireAuthority` to `(config_address(NFT), NFT)` (caller must declare `config_pda`). |
| `1` | `Transfer` | `token_id, new_owner` | Existing linked token only; asserts `!shielded` and `!frozen`. Updates `owner`, clears `sealed`. No callback. |
| `2` | `Burn` | `token_id` | Existing linked token only. Empties `token_pda`. Permitted even if `frozen` or `shielded` so lapsed re-registration / legacy `Claim` overwrites always burn cleanly (stale `PrivateDeed` expires at `deed.expires`). |
| `3` | `SetExpiry` | `token_id, expires` | Existing linked token only. Updates `record.expires` (extending past `now` unfreezes out of grace). Allowed while `shielded`. |
| `4` | `Shield` | `token_id, deed_account, expires` | Existing linked token only; asserts `!shielded`, `!frozen`, and `deed_account != public_pda(deed_seed)`. Sets `shielded = true`, `deed_account`, `expires`, and sends seeded `Effect::WriteDeed` to private PDA `deed_account`. |
| `5` | `Unshield` | `token_id` | Existing linked token only; asserts `shielded`. Sets `shielded = false`, takes the stored `deed_account`, and sends seeded `Effect::ClearDeed` to it. |
| `6` | `RefreshShielded` | `token_id, expires` | Existing linked token only; asserts `shielded`. Updates `record.expires` and sends seeded `Effect::UpdateDeed` to the stored `deed_account`. |
| `7` | `SetFrozen` | `token_id, frozen` | Existing linked token only. Sets explicit `record.frozen` flag. |

> **Privacy note on `deed_account`:** `AuthorityOp::Shield` takes `deed_account: AccountId` (stored for `Unshield` and `RefreshShielded`) rather than raw `(npk, vpk)`. Passing raw `(npk, vpk)` across the public boundary would expose the holder's 1,184-byte viewing key and link all shielded deeds together. Instead, `(token_pda, NFT)` rejects the public PDA address for `deed_seed(token_id)` and sends `.with_pda_seeds([deed_seed(token_id)])`, so LEZ v0.4.0's `private_seed_grant` guarantees inside the proof that `deed_account` is a private PDA genuinely derived from `(NFT, deed_seed(token_id))` and owned by the prover.

### 4.3 Outbound Registry Hook (`RegistryTransferHook`)

When a holder transfers a linked token directly through `lez_nft` (Path B), `(token_pda, NFT)` emits a single synchronous message to `(linked.record_actor, linked.program)` with wire layout matching `names_core::Message::NftTransferHook` (variant `3`):

```text
0x03                             // names_core::Message::NftTransferHook discriminant
|| id                            // [u8; 32] = linked.target (the name's slot_id); 137 bytes in all
|| token_id                      // [u8; 32]
|| expected_old_owner            // [u8; 32]
|| new_owner                     // [u8; 32]
|| now                           // u64 LE (8 bytes)
```

`names_core` MUST authenticate `input.from == Some(Actor::new(token_address(settings.nft_program, token_id), settings.nft_program))` and emit **no** callback to `lez_nft`.

---

## 5. Actor Message Flows

### Path A: Registry-initiated (`Register`, `Renew`, `Transfer`, `Buy`, `Release`, `Claim`, `ShieldDeed`, `UnshieldDeed`, `RefreshShieldedDeed`)

```text
(signer, REGISTRY)
  ──Sync──▶ (record_pda, REGISTRY)
               ├──Sync──▶ (token_pda, NFT) [Message::AuthoritySync(AuthorityOp::*)]
               │             ├── (Mint only) ──Sync──▶ (config_pda, NFT) [RequireAuthority]
               │             └── (Shield/Unshield/RefreshShielded) ──Sync+pda_seed──▶ (deed_pda, NFT)
               └── ... (page / bucket / settings)
```

### Path B: NFT-initiated transfer of a linked token (`lez_nft::Instruction::Transfer`)

```text
(signer, NFT)
  ──Sync+pda_seed──▶ (token_pda, NFT) [Effect::Transfer; bounds [now, now + 10m)]
                        └──Sync──▶ (record_pda, REGISTRY) [names_core::Message::NftTransferHook]
                                      ├──Sync──▶ (settings_pda, REGISTRY)
                                      ├──Sync──▶ (page_pda, REGISTRY)
                                      └──Sync──▶ (bucket_pda, REGISTRY) [if listed]
```

### LP-0001 Private Membership Gate (`Message::ReadState`)

```text
(holder, GATE_PROGRAM)  [inside privacy circuit; touches zero public token accounts]
  ──Sync+lent_grant──▶ (deed_pda, NFT) [Message::ReadState]
                          └──Sync+lent_grant──▶ (holder, GATE_PROGRAM) [StateReply(PrivateDeed)]
```

---

## 6. Events

All events are emitted on the public PDA actor (so both public and private transactions emit them consistently). Event selector = first 8 bytes of `SHA-256(SELECTOR_NAME)`.

| Selector Name | Payload |
|---|---|
| `lez_nft::CollectionCreated` | `{ collection_id: [u8; 32] }` |
| `lez_nft::Minted` | `{ token_id: [u8; 32] }` |
| `lez_nft::Transferred` | `{ token_id: [u8; 32] }` |
| `lez_nft::Burned` | `{ token_id: [u8; 32] }` |
| `lez_nft::Shielded` | `{ token_id: [u8; 32] }` |
| `lez_nft::Unshielded` | `{ token_id: [u8; 32] }` |
| `lez_nft::ExpiryUpdated` | `{ token_id: [u8; 32], expires: u64 }` |
| `lez_nft::FrozenUpdated` | `{ token_id: [u8; 32], frozen: bool }` |
| `lez_nft::Configured` | `{ admin: AccountId }` |

---

## 7. Borsh Encoding Vectors

All vectors below are verified by unit tests in `core/src/lib.rs`:

1. **`PrivateDeed`** (`version = 3`, `collection_id = [0xCC; 32]`, `token_id = [0xAA; 32]`, `expires = 0x0102_0304_0506_0708`, 73 bytes):
   `03 || CC*32 || AA*32 || 08 07 06 05 04 03 02 01`
2. **`RegistryTransferHook`** (`id = [0x11; 32]`, `token_id = [0xAA; 32]`, `old = [0x10; 32]`, `new = [0x20; 32]`, `now = 1000`, 137 bytes):
   `03 || 11*32 || AA*32 || 10*32 || 20*32 || E8 03 00 00 00 00 00 00`
3. **`Message::AuthoritySync(AuthorityOp::Transfer)`** (`token_id = [0xAA; 32]`, `new_owner = [0xBB; 32]`, 66 bytes):
   `02 01 || AA*32 || BB*32`
4. **`Message::AuthoritySync(AuthorityOp::Burn)`** (`token_id = [0xAA; 32]`, 34 bytes):
   `02 02 || AA*32`
5. **`Message::AuthoritySync(AuthorityOp::SetExpiry)`** (`token_id = [0xAA; 32]`, `expires = 1000`, 42 bytes):
   `02 03 || AA*32 || E8 03 00 00 00 00 00 00`
6. **`Message::AuthoritySync(AuthorityOp::Shield)`** (`token_id = [0xAA; 32]`, `deed_account = [0xDD; 32]`, `expires = 1000`, 74 bytes):
   `02 04 || AA*32 || DD*32 || E8 03 00 00 00 00 00 00`
7. **`Message::AuthoritySync(AuthorityOp::Unshield)`** (`token_id = [0xAA; 32]`, 34 bytes):
   `02 05 || AA*32`
8. **`Message::AuthoritySync(AuthorityOp::RefreshShielded)`** (`token_id = [0xAA; 32]`, `expires = 1000`, 42 bytes):
   `02 06 || AA*32 || E8 03 00 00 00 00 00 00`
9. **`Message::AuthoritySync(AuthorityOp::SetFrozen)`** (`token_id = [0xAA; 32]`, `frozen = true`, 35 bytes):
   `02 07 || AA*32 || 01`
10. **`Message::ReadState`** (1 byte):
    `03`
11. **`Message::RequireHolder`** (`tag = [0xBB; 32]`, `now = Some(1000)`, 42 bytes):
    `04 || BB*32 || 01 || E8 03 00 00 00 00 00 00`
12. **`Message::Instruction(Instruction::Transfer)`** (linked, `token_id = [0xAA; 32]`, `new_owner = [0xBB; 32]`, `sealed = None`, `now = Some(1000)`, `linked_target = Some([0x11; 32])`, 109 bytes):
    `00 03 || AA*32 || BB*32 || 00 || 01 || E8 03 00 00 00 00 00 00 || 01 || 11*32`
