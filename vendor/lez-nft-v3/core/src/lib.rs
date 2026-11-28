//! LEZ NFT program v3: non-fungible tokens, collections, LP-0001 shielded
//! private deeds, and authority-linked tokens (for LNS names) on the Logos
//! Execution Zone (v0.4.0 actor model).
//!
//! ## Token ownership and privacy
//!
//! Each token lives in a public PDA derived from the program and its 32-byte
//! `token_id` (`lez-nft/v3/token/`). By default its owner is an unlinkable
//! 32-byte `OwnerTag`:
//! - Unlinked tokens: `SHA-256("lez-nft/v3/owner" || token_id || account)`
//! - Linked tokens:   `SHA-256("logos-names/v5/owner" || linked.target || account)`
//!
//! Using the exact same `OwnerTag` formula on linked tokens keeps `NameRecord.owner`
//! and `NftRecord.owner` byte-for-byte identical so split ownership is impossible
//! and private holders never reveal their `AccountId` across the circuit boundary.
//!
//! Any token holder may optionally shield a token into a private PDA
//! (`for_private_pda(NFT_PROGRAM, SHA-256("lez-nft/v3/deed/" || token_id), npk, vpk)`)
//! holding a `PrivateDeed` readable via `Message::ReadState` (for LP-0001
//! zero-knowledge collection-membership proofs). While shielded, transfers and
//! direct burns are refused until unshielded.
//!
//! ## Linked tokens (LNS lockstep)
//!
//! A token with `linked: Some(LinkedAuthority { program, authority_actor, target, record_actor })`
//! moves in single-transaction atomic lockstep with its linked registry:
//! - **Registry → NFT:** `linked.authority_actor` (under `linked.program`) sends
//!   `Message::AuthoritySync(AuthorityOp::*)` (`Mint`, `Transfer`, `Burn`,
//!   `SetExpiry`, `Shield`, `Unshield`, `RefreshShielded`, `SetFrozen`). No callback
//!   is sent back to the registry.
//! - **NFT → Registry:** the holder sends `Message::Instruction(Instruction::Transfer)`
//!   to `(signer, NFT_PROGRAM)` with `now: Some(now)` and `linked_target: Some(linked.target)`.
//!   The token PDA checks ownership, `!shielded`, `!frozen`, and `now < record.expires`,
//!   bounds the transaction timestamp window to `[now, now + TIME_WINDOW_MS)`, updates
//!   `record.owner`, and sends `RegistryTransferHook` (`names_core::Message::NftTransferHook`,
//!   tag `3`) to `(linked.record_actor, linked.program)`. No callback is sent back.
//! - Direct `Burn`, `Shield`, and `Unshield` instructions are refused on linked tokens;
//!   those lifecycle actions are authority-only via `AuthoritySync`.

use borsh::{BorshDeserialize, BorshSerialize};
use lee_core::{
    NullifierPublicKey,
    account::{AccountId, Actor, ActorState},
    encryption::ViewingPublicKey,
    native_token,
    program::{PdaSeed, ProgramEvent, ReceiveInput, Response, StateReply, SyncMessage},
};

pub const VERSION: u8 = 3;
pub const MAX_CONTENT_LEN: usize = 10 * 1024;
pub const MAX_SEALED_LEN: usize = 8 * 1024;
pub const MAX_SUPPLY: u32 = 1_000_000;
/// Maximum number of linked authority programs registered in [`Config`].
pub const MAX_AUTHORITIES: usize = 32;
/// Bytes in a private account's viewing public key (ML-KEM-768).
pub const VPK_LEN: usize = ViewingPublicKey::LEN;
/// Timestamp validity window width in milliseconds (10 minutes), matching LNS.
pub const TIME_WINDOW_MS: u64 = 10 * 60 * 1000;

/// Discriminant of `names_core::Message::NftTransferHook` (variant index 3).
pub const REGISTRY_NFT_TRANSFER_HOOK_TAG: u8 = 3;

const TOKEN_SEED_DOMAIN: &[u8] = b"lez-nft/v3/token/";
const COLLECTION_SEED_DOMAIN: &[u8] = b"lez-nft/v3/collection/";
const CONFIG_SEED_DOMAIN: &[u8] = b"lez-nft/v3/config";
const DEED_SEED_DOMAIN: &[u8] = b"lez-nft/v3/deed/";
const SERIAL_DOMAIN: &[u8] = b"lez-nft/v3/serial/";
const OWNER_TAG_DOMAIN: &[u8] = b"lez-nft/v3/owner";
const CREATOR_TAG_DOMAIN: &[u8] = b"lez-nft/v3/creator";
const LNS_OWNER_TAG_DOMAIN: &[u8] = b"logos-names/v5/owner";
const LNS_COLLECTION_DOMAIN: &[u8] = b"logos-names/v8/collection/";

pub type TokenId = [u8; 32];
pub type CollectionId = [u8; 32];
pub type OwnerTag = [u8; 32];
pub type NameId = [u8; 32];

// ---------------------------------------------------------------- data types

/// Who a collection's sales pay.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Payee {
    /// 0: A public account receiving native token credits.
    Public(AccountId),
    /// 1: A private account's receiving keys. Each payment may target either
    /// the canonical regular private account or a blinded address under `opening`.
    Private { npk: [u8; 32], vpk: Vec<u8> },
}

impl Payee {
    /// The account a payment with optional blinding `opening` goes to: the
    /// public account, or the regular private account (blinded when `opening`
    /// is `Some`) under the payee's keys.
    #[must_use]
    pub fn account(&self, opening: Option<[u8; 32]>) -> AccountId {
        match self {
            Self::Public(account) => *account,
            Self::Private { npk, vpk } => {
                // Built through its borsh form: `ViewingPublicKey::from_bytes` is host-only.
                let vpk: ViewingPublicKey =
                    borsh::from_slice(&borsh::to_vec(vpk).expect("borsh infallible"))
                        .expect("a viewing key decodes");
                let account =
                    AccountId::for_regular_private_account(&NullifierPublicKey(*npk), &vpk);
                opening.map_or(account, |factor| account.blinded(&factor))
            }
        }
    }

    fn hash(&self) -> [u8; 32] {
        sha256(&to_bytes(self))
    }
}

/// Binding between an [`NftRecord`] and its authoritative registry record.
///
/// Byte-for-byte the registry's `names_core::nft::LinkedAuthority` (top-level
/// LNS names only; subnames have no NFTs).
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct LinkedAuthority {
    /// Authoritative registry program ID (e.g. LNS `names_core`).
    pub program: AccountId,
    /// The only actor of `program` that may send [`Message::AuthoritySync`]
    /// for this token (the registry's `settings_address(&program)`).
    pub authority_actor: AccountId,
    /// The name's `slot_id`: hashed into `linked_owner_tag`, and echoed back as
    /// `id` in [`RegistryTransferHook`].
    pub target: [u8; 32],
    /// Registry record PDA (`record_address(&program, &target)`) that receives
    /// [`RegistryTransferHook`] on an NFT-side transfer.
    pub record_actor: AccountId,
}

/// One authorized external program permitted to mint linked tokens via
/// [`AuthorityOp::Mint`].
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AuthorizedProgram {
    /// Program account ID allowed to send `AuthorityOp::Mint`.
    pub program: AccountId,
    /// If `Some(cid)`, tokens minted by this program MUST carry
    /// `collection == Some((cid, _))`, and ordinary `CreateCollection` cannot
    /// claim `cid` (preventing spoofed LP-0001 collection deeds).
    pub collection_id: Option<CollectionId>,
}

/// Program-level configuration stored at [`config_address`].
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Config {
    pub version: u8,
    pub admin: AccountId,
    pub authorities: Vec<AuthorizedProgram>,
}

/// A minted token stored at [`token_address`].
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NftRecord {
    pub version: u8,
    pub token_id: TokenId,
    /// Unified `OwnerTag`: `linked_owner_tag(&linked.target, account)` when
    /// `linked.is_some()`, else `owner_tag(&token_id, account)`.
    pub owner: OwnerTag,
    /// The collection it was minted from and its serial (0 for authority-minted
    /// deeds, 1..=supply for collection mints); `None` for a one-off.
    pub collection: Option<(CollectionId, u32)>,
    /// The token's own content (JSON metadata by convention, or ciphertext when
    /// `sealed` is non-empty). Empty for ordinary collection tokens.
    pub content: Vec<u8>,
    /// The content key sealed to authorized readers; empty when unsealed.
    pub sealed: Vec<u8>,
    /// Present when this token is coupled to an external authority (e.g. LNS).
    pub linked: Option<LinkedAuthority>,
    /// `true` while custody is mirrored in a private LP-0001 [`PrivateDeed`] PDA.
    pub shielded: bool,
    /// Expiry timestamp in ms (`u64::MAX` for non-expiring / permanent tokens).
    /// When `now >= expires`, linked transfers are refused (frozen during/after grace).
    pub expires: u64,
    /// Explicit authority freeze flag (independent of `expires`).
    pub frozen: bool,
    /// The private LP-0001 deed PDA while `shielded`, so a later unshield or
    /// refresh needs no account from the caller; `None` otherwise.
    pub deed_account: Option<AccountId>,
}

/// Private deed stored inside a private PDA (`deed_address`) for LP-0001
/// zero-knowledge collection membership proofs via [`Message::ReadState`].
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PrivateDeed {
    pub version: u8,
    pub collection_id: CollectionId,
    pub token_id: TokenId,
    pub expires: u64,
}

/// A collection on sale, stored at [`collection_address`].
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CollectionRecord {
    pub version: u8,
    pub collection_id: CollectionId,
    /// The creator's tag, `SHA-256(CREATOR_TAG_DOMAIN || collection_id || account)`.
    pub creator: OwnerTag,
    /// Who payments go to; `None` when the collection is free.
    pub payee: Option<Payee>,
    pub price: u128,
    pub supply: u32,
    pub minted: u32,
    /// Shared by every token in the collection: JSON by convention.
    pub content: Vec<u8>,
}

// ---------------------------------------------------------------- wire messages

/// Top-level message envelope handled by [`handle`].
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Message {
    /// 0: User-initiated instruction, sent as root to `(signer, NFT_PROGRAM)`.
    Instruction(Instruction),
    /// 1: Internal state-transition message, sent only from another actor of
    /// `NFT_PROGRAM`.
    Effect(Effect),
    /// 2: Authority lifecycle call, sent to `(token_pda, NFT_PROGRAM)` by a
    /// linked authority's `authority_actor` (e.g. LNS `names_core` settings).
    AuthoritySync(AuthorityOp),
    /// 3: Read-only state query (e.g. LP-0001 gate reading `PrivateDeed` at
    /// `deed_pda`); replies to `input.from` with [`StateReply`], returning any
    /// lent grant.
    ReadState,
    /// 4: Read-only holder check against `(token_pda, NFT_PROGRAM)`: succeeds
    /// iff the token exists, `record.owner == tag`, `!record.frozen`, and (when
    /// `now` is `Some(t)`) `t < record.expires` with a bounded timestamp window.
    RequireHolder { tag: OwnerTag, now: Option<u64> },
}

/// User-initiated instructions (sent wrapped in [`Message::Instruction`] to
/// `(signer, NFT_PROGRAM)`).
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Instruction {
    /// 0: Opens a collection, signed by its creator.
    /// Public actors to declare: `(collection_pda, NFT)`, `(config_pda, NFT)`.
    CreateCollection {
        collection_id: CollectionId,
        payee: Option<Payee>,
        price: u128,
        supply: u32,
        content: Vec<u8>,
    },
    /// 1: Mints the collection's next token, serial `serial`, to the signer or
    /// `owner`. If `price > 0`, the transaction must also authorize
    /// `(signer, NATIVE_TOKEN_PROGRAM_ID)` so the root can transfer `price` to
    /// `payee.account(payee_opening)`.
    MintFromCollection {
        collection_id: CollectionId,
        serial: u32,
        price: u128,
        payee: Option<Payee>,
        payee_opening: Option<[u8; 32]>,
        owner: Option<OwnerTag>,
    },
    /// 2: Mints a one-off unlinked token to the signer or `owner`.
    Mint {
        token_id: TokenId,
        owner: Option<OwnerTag>,
        content: Vec<u8>,
        sealed: Vec<u8>,
    },
    /// 3: Transfers a token to `new_owner`.
    /// - For unlinked tokens: `linked_target` MUST be `None`. `sealed`, if
    ///   `Some`, replaces the sealed content key.
    /// - For linked tokens: `linked_target` MUST be `Some(linked.target)` (so a
    ///   private signer can compute `linked_owner_tag` inside the proof without
    ///   revealing its `AccountId`), `now` MUST be `Some(t)` with
    ///   `t < record.expires`, and `(token_pda, NFT)` synchronously notifies
    ///   `(linked.record_actor, linked.program)` with [`RegistryTransferHook`].
    /// Refused if `shielded` or `frozen`.
    Transfer {
        token_id: TokenId,
        new_owner: OwnerTag,
        sealed: Option<Vec<u8>>,
        now: Option<u64>,
        linked_target: Option<[u8; 32]>,
    },
    /// 4: Destroys an unlinked, unshielded, unfrozen token.
    /// Refused on linked tokens (must be burned via `AuthorityOp::Burn`).
    Burn { token_id: TokenId },
    /// 5: Shields an unlinked token into the holder's private LP-0001 deed PDA
    /// `deed_account = deed_address(NFT, token_id, npk, vpk)`.
    /// Refused on linked tokens (must go through `AuthorityOp::Shield`).
    Shield {
        token_id: TokenId,
        deed_account: AccountId,
    },
    /// 6: Unshields an unlinked token, clearing `deed_account` and unlocking
    /// transfer/burn.
    /// Refused on linked tokens (must go through `AuthorityOp::Unshield`).
    Unshield {
        token_id: TokenId,
        deed_account: AccountId,
    },
    /// 7: Initializes or updates [`Config`] at `config_address(NFT)`.
    /// On first call, `signer` must equal `admin`. Subsequent calls require
    /// `signer == existing.admin`.
    Configure {
        admin: AccountId,
        authorities: Vec<AuthorizedProgram>,
    },
}

/// Authority-only operations sent wrapped in [`Message::AuthoritySync`] to
/// `(token_pda, NFT_PROGRAM)` by `linked.authority_actor` under `linked.program`.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum AuthorityOp {
    /// 0: Mints a linked token into collection `(lns_collection_id(linked.program), 0)`.
    /// Verified against [`Config`] at `config_address(NFT)` (callers must declare
    /// `(config_address(NFT), NFT)`).
    Mint {
        token_id: TokenId,
        owner: OwnerTag,
        linked: LinkedAuthority,
        content: Vec<u8>,
        expires: u64,
    },
    /// 1: Mirrors a registry-initiated transfer/sale onto `NftRecord`.
    /// Refused if `shielded` or `frozen`. Clears `sealed`. No callback.
    Transfer {
        token_id: TokenId,
        new_owner: OwnerTag,
    },
    /// 2: Burns a linked token (on Release, Claim overwrite, or re-registration
    /// after lapse). Allowed even if `frozen` or `shielded` so lapsed names
    /// always burn cleanly (stale `PrivateDeed` expires naturally at `deed.expires`).
    Burn { token_id: TokenId },
    /// 3: Mirrors a renewal / expiry change from the registry.
    /// Allowed while `shielded` (holder may optionally have the registry send `RefreshShielded` later).
    SetExpiry { token_id: TokenId, expires: u64 },
    /// 4: Mirrors `registry::ShieldDeed`: flips `shielded = true`, syncs `expires`,
    /// and writes [`PrivateDeed`] to private PDA `deed_account` via `private_seed_grant`.
    /// Passing `deed_account` (rather than raw `npk`/`vpk`) avoids publishing the
    /// holder's viewing/nullifier public keys across the public boundary.
    Shield {
        token_id: TokenId,
        deed_account: AccountId,
        expires: u64,
    },
    /// 5: Mirrors `registry::UnshieldDeed`: flips `shielded = false` and clears
    /// `PrivateDeed` in the stored private PDA `record.deed_account`.
    Unshield { token_id: TokenId },
    /// 6: Mirrors `registry::RefreshShieldedDeed`: updates `record.expires` and
    /// `PrivateDeed.expires` in `record.deed_account` while `shielded == true`.
    RefreshShielded { token_id: TokenId, expires: u64 },
    /// 7: Explicitly freezes or unfreezes a linked token without changing `expires`.
    SetFrozen { token_id: TokenId, frozen: bool },
}

/// Internal actor-to-actor effects (sent only by `NFT_PROGRAM` to its own PDAs).
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Effect {
    /// 0: Writes a new collection; refused if the account is occupied.
    CreateCollection(CollectionRecord),
    /// 1: Counts one more token sold; refused unless next serial, not sold out,
    /// and price + payee hash match.
    Sell {
        collection_id: CollectionId,
        serial: u32,
        price: u128,
        payee: Option<[u8; 32]>,
    },
    /// 2: Writes a new unlinked token; refused if occupied.
    Mint(NftRecord),
    /// 3: Transfers a token; verifies owner, `!shielded`, `!frozen`, and if
    /// linked verifies `linked_target`, `now < expires`, bounds timestamp window,
    /// and emits [`RegistryTransferHook`].
    Transfer {
        token_id: TokenId,
        signer: OwnerTag,
        linked_target: Option<[u8; 32]>,
        new_owner: OwnerTag,
        sealed: Option<Vec<u8>>,
        now: Option<u64>,
    },
    /// 4: Empties an unlinked, unshielded, unfrozen token.
    Burn {
        token_id: TokenId,
        signer: OwnerTag,
    },
    /// 5: Shields an unlinked token and writes its private deed PDA.
    Shield {
        token_id: TokenId,
        signer: OwnerTag,
        deed_account: AccountId,
    },
    /// 6: Unshields an unlinked token and clears its private deed PDA.
    Unshield {
        token_id: TokenId,
        signer: OwnerTag,
        deed_account: AccountId,
    },
    /// 7: Writes a fresh [`PrivateDeed`] into a seeded private PDA (`deed_pda`).
    WriteDeed(PrivateDeed),
    /// 8: Updates an existing [`PrivateDeed`] in `deed_pda`.
    UpdateDeed(PrivateDeed),
    /// 9: Clears an existing [`PrivateDeed`] in `deed_pda`.
    ClearDeed { token_id: TokenId },
    /// 10: Initializes or updates [`Config`] at `config_address(NFT)`.
    Configure {
        caller: AccountId,
        admin: AccountId,
        authorities: Vec<AuthorizedProgram>,
    },
    /// 11: Asserts `program` is registered in [`Config`] and `collection_id`
    /// matches any pinned collection for that authority.
    RequireAuthority {
        program: AccountId,
        collection_id: Option<CollectionId>,
    },
    /// 12: Asserts `collection_id` is not reserved by any authority in [`Config`].
    RequireCollectionUnreserved { collection_id: CollectionId },
}

/// Wire payload matching `names_core::Message::NftTransferHook` (enum variant `3`).
///
/// Serializes as:
/// `0x03 || id ([u8; 32]) || token_id ([u8; 32]) || expected_old_owner ([u8; 32]) || new_owner ([u8; 32]) || now (u64 LE)`
/// (137 bytes), where `id` is `linked.target` (the name's `slot_id`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryTransferHook {
    pub id: NameId,
    pub token_id: TokenId,
    pub expected_old_owner: OwnerTag,
    pub new_owner: OwnerTag,
    pub now: u64,
}

impl BorshSerialize for RegistryTransferHook {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        REGISTRY_NFT_TRANSFER_HOOK_TAG.serialize(writer)?;
        self.id.serialize(writer)?;
        self.token_id.serialize(writer)?;
        self.expected_old_owner.serialize(writer)?;
        self.new_owner.serialize(writer)?;
        self.now.serialize(writer)?;
        Ok(())
    }
}

impl BorshDeserialize for RegistryTransferHook {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let tag = u8::deserialize_reader(reader)?;
        if tag != REGISTRY_NFT_TRANSFER_HOOK_TAG {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("expected NftTransferHook tag {REGISTRY_NFT_TRANSFER_HOOK_TAG}, got {tag}"),
            ));
        }
        Ok(Self {
            id: NameId::deserialize_reader(reader)?,
            token_id: TokenId::deserialize_reader(reader)?,
            expected_old_owner: OwnerTag::deserialize_reader(reader)?,
            new_owner: OwnerTag::deserialize_reader(reader)?,
            now: u64::deserialize_reader(reader)?,
        })
    }
}

// ---------------------------------------------------------------- events

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CollectionCreated {
    pub collection_id: CollectionId,
}
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Minted {
    pub token_id: TokenId,
}
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Transferred {
    pub token_id: TokenId,
}
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Burned {
    pub token_id: TokenId,
}
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Shielded {
    pub token_id: TokenId,
}
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Unshielded {
    pub token_id: TokenId,
}
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ExpiryUpdated {
    pub token_id: TokenId,
    pub expires: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FrozenUpdated {
    pub token_id: TokenId,
    pub frozen: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Configured {
    pub admin: AccountId,
}

impl CollectionCreated {
    pub const SELECTOR_NAME: &str = "lez_nft::CollectionCreated";
}
impl Minted {
    pub const SELECTOR_NAME: &str = "lez_nft::Minted";
}
impl Transferred {
    pub const SELECTOR_NAME: &str = "lez_nft::Transferred";
}
impl Burned {
    pub const SELECTOR_NAME: &str = "lez_nft::Burned";
}
impl Shielded {
    pub const SELECTOR_NAME: &str = "lez_nft::Shielded";
}
impl Unshielded {
    pub const SELECTOR_NAME: &str = "lez_nft::Unshielded";
}
impl ExpiryUpdated {
    pub const SELECTOR_NAME: &str = "lez_nft::ExpiryUpdated";
}
impl FrozenUpdated {
    pub const SELECTOR_NAME: &str = "lez_nft::FrozenUpdated";
}
impl Configured {
    pub const SELECTOR_NAME: &str = "lez_nft::Configured";
}

// ---------------------------------------------------------------- hashing & PDAs

fn sha256(bytes: &[u8]) -> [u8; 32] {
    use risc0_zkvm::sha::{Impl, Sha256 as _};
    Impl::hash_bytes(bytes)
        .as_bytes()
        .try_into()
        .expect("SHA-256 output is 32 bytes")
}

fn hash_parts(parts: &[&[u8]]) -> [u8; 32] {
    sha256(&parts.concat())
}

fn to_bytes<T: BorshSerialize>(value: &T) -> Vec<u8> {
    borsh::to_vec(value).expect("borsh serialization is infallible")
}

#[must_use]
pub fn event_selector(selector_name: &str) -> [u8; 8] {
    sha256(selector_name.as_bytes())[..8]
        .try_into()
        .expect("slice of 8")
}

fn event<E: BorshSerialize>(selector_name: &str, data: &E) -> ProgramEvent {
    ProgramEvent {
        selector: event_selector(selector_name),
        data: to_bytes(data),
    }
}

/// The PDA seed for a public token record.
#[must_use]
pub fn token_seed(token_id: &TokenId) -> PdaSeed {
    PdaSeed::new(hash_parts(&[TOKEN_SEED_DOMAIN, token_id]))
}

/// The on-chain account that stores the record for `token_id` under `program`.
#[must_use]
pub fn token_address(program: &AccountId, token_id: &TokenId) -> AccountId {
    AccountId::for_public_pda(program, &token_seed(token_id))
}

/// The PDA seed for a collection record.
#[must_use]
pub fn collection_seed(collection_id: &CollectionId) -> PdaSeed {
    PdaSeed::new(hash_parts(&[COLLECTION_SEED_DOMAIN, collection_id]))
}

/// The on-chain account that stores `collection_id` under `program`.
#[must_use]
pub fn collection_address(program: &AccountId, collection_id: &CollectionId) -> AccountId {
    AccountId::for_public_pda(program, &collection_seed(collection_id))
}

/// The PDA seed for [`Config`].
#[must_use]
pub fn config_seed() -> PdaSeed {
    PdaSeed::new(sha256(CONFIG_SEED_DOMAIN))
}

/// The on-chain account that stores [`Config`] under `program`.
#[must_use]
pub fn config_address(program: &AccountId) -> AccountId {
    AccountId::for_public_pda(program, &config_seed())
}

/// The PDA seed for a shielded LP-0001 [`PrivateDeed`].
#[must_use]
pub fn deed_seed(token_id: &TokenId) -> PdaSeed {
    PdaSeed::new(hash_parts(&[DEED_SEED_DOMAIN, token_id]))
}

/// The private PDA storing [`PrivateDeed`] for `token_id` under `program` and
/// the holder's `(npk, vpk)`.
#[must_use]
pub fn deed_address(
    program: &AccountId,
    token_id: &TokenId,
    npk: &NullifierPublicKey,
    vpk: &ViewingPublicKey,
) -> AccountId {
    AccountId::for_private_pda(program, &deed_seed(token_id), npk, vpk)
}

/// The token id of serial `serial` (from 1) in `collection_id`.
#[must_use]
pub fn serial_token_id(collection_id: &CollectionId, serial: u32) -> TokenId {
    hash_parts(&[SERIAL_DOMAIN, collection_id, &serial.to_le_bytes()])
}

/// Conventional LNS collection id derived from a registry program id.
#[must_use]
pub fn lns_collection_id(registry_program: &AccountId) -> CollectionId {
    hash_parts(&[LNS_COLLECTION_DOMAIN, registry_program.value()])
}

/// Owner tag for an ordinary (unlinked) token:
/// `SHA-256("lez-nft/v3/owner" || token_id || account)`.
#[must_use]
pub fn owner_tag(token_id: &TokenId, account: &AccountId) -> OwnerTag {
    hash_parts(&[OWNER_TAG_DOMAIN, token_id, account.value()])
}

/// Unified owner tag for a linked LNS token:
/// `SHA-256("logos-names/v5/owner" || target || account)`.
#[must_use]
pub fn linked_owner_tag(target: &[u8; 32], account: &AccountId) -> OwnerTag {
    hash_parts(&[LNS_OWNER_TAG_DOMAIN, target, account.value()])
}

/// Owner tag for `record` (uses [`linked_owner_tag`] when linked, else [`owner_tag`]).
#[must_use]
pub fn record_owner_tag(record: &NftRecord, account: &AccountId) -> OwnerTag {
    match &record.linked {
        Some(linked) => linked_owner_tag(&linked.target, account),
        None => owner_tag(&record.token_id, account),
    }
}

/// The tag that binds `account` to the collection it created.
#[must_use]
pub fn creator_tag(collection_id: &CollectionId, account: &AccountId) -> OwnerTag {
    hash_parts(&[CREATOR_TAG_DOMAIN, collection_id, account.value()])
}

// ---------------------------------------------------------------- handler

fn check_content(content: &[u8], sealed: &[u8]) {
    assert!(
        content.len() <= MAX_CONTENT_LEN,
        "content exceeds {MAX_CONTENT_LEN} bytes"
    );
    assert!(
        sealed.len() <= MAX_SEALED_LEN,
        "sealed key exceeds {MAX_SEALED_LEN} bytes"
    );
}

fn bounded_window(response: Response, now: u64) -> Response {
    let end = now
        .checked_add(TIME_WINDOW_MS)
        .expect("timestamp window overflows u64");
    response
        .try_timestamp_window(now..end)
        .expect("non-empty timestamp window")
}

fn effect_msg(account: AccountId, program: AccountId, seed: PdaSeed, effect: &Effect) -> SyncMessage {
    SyncMessage::new(Actor::new(account, program), &Message::Effect(effect.clone()))
        .with_pda_seeds([seed])
}

/// Main LEZ v0.4.0 actor handler.
#[must_use]
pub fn handle(input: &ReceiveInput, message: Message) -> Response {
    match message {
        Message::Instruction(instruction) => handle_instruction(input, instruction),
        Message::Effect(effect) => handle_effect(input, effect),
        Message::AuthoritySync(op) => handle_authority_sync(input, op),
        Message::ReadState => handle_read_state(input),
        Message::RequireHolder { tag, now } => handle_require_holder(input, tag, now),
    }
}

fn handle_instruction(input: &ReceiveInput, instruction: Instruction) -> Response {
    assert!(
        input.from.is_none(),
        "instructions must be top-level transactions"
    );
    assert!(input.is_authorized(), "the account must sign");

    let signer = input.receiver.account_id;
    let program = input.receiver.program_account_id;

    match instruction {
        Instruction::CreateCollection {
            collection_id,
            payee,
            price,
            supply,
            content,
        } => {
            check_content(&content, &[]);
            assert!(
                (1..=MAX_SUPPLY).contains(&supply),
                "supply must be 1 to {MAX_SUPPLY}"
            );
            assert!(
                (price == 0) == payee.is_none(),
                "a priced collection needs a payee, and a free one none"
            );
            if let Some(Payee::Private { vpk, .. }) = &payee {
                assert!(vpk.len() == VPK_LEN, "a viewing key is {VPK_LEN} bytes");
            }
            let record = CollectionRecord {
                version: VERSION,
                collection_id,
                creator: creator_tag(&collection_id, &signer),
                payee,
                price,
                supply,
                minted: 0,
                content,
            };
            Response::keep_state()
                .send(effect_msg(
                    config_address(&program),
                    program,
                    config_seed(),
                    &Effect::RequireCollectionUnreserved { collection_id },
                ))
                .send(effect_msg(
                    collection_address(&program, &collection_id),
                    program,
                    collection_seed(&collection_id),
                    &Effect::CreateCollection(record),
                ))
        }
        Instruction::MintFromCollection {
            collection_id,
            serial,
            price,
            payee,
            payee_opening,
            owner,
        } => {
            let mut resp = Response::keep_state();
            if price > 0 {
                let payee_ref = payee.as_ref().expect("a paid mint names the payee");
                if let Payee::Private { vpk, .. } = payee_ref {
                    assert!(vpk.len() == VPK_LEN, "a viewing key is {VPK_LEN} bytes");
                }
                let to = payee_ref.account(payee_opening);
                let payer_balance = Actor::native_balance(signer);
                assert!(
                    input.grants.contains(&payer_balance),
                    "payer must authorize native balance transfer"
                );
                resp = resp.send(
                    SyncMessage::new(
                        payer_balance,
                        &native_token::Message::Transfer { to, amount: price },
                    )
                    .with_lent_grant(input),
                );
            }
            let token_id = serial_token_id(&collection_id, serial);
            let record = NftRecord {
                version: VERSION,
                token_id,
                owner: owner.unwrap_or_else(|| owner_tag(&token_id, &signer)),
                collection: Some((collection_id, serial)),
                content: Vec::new(),
                sealed: Vec::new(),
                linked: None,
                shielded: false,
                expires: u64::MAX,
                frozen: false,
                deed_account: None,
            };
            resp.send(effect_msg(
                collection_address(&program, &collection_id),
                program,
                collection_seed(&collection_id),
                &Effect::Sell {
                    collection_id,
                    serial,
                    price,
                    payee: payee.as_ref().map(Payee::hash),
                },
            ))
            .send(effect_msg(
                token_address(&program, &token_id),
                program,
                token_seed(&token_id),
                &Effect::Mint(record),
            ))
        }
        Instruction::Mint {
            token_id,
            owner,
            content,
            sealed,
        } => {
            check_content(&content, &sealed);
            let record = NftRecord {
                version: VERSION,
                token_id,
                owner: owner.unwrap_or_else(|| owner_tag(&token_id, &signer)),
                collection: None,
                content,
                sealed,
                linked: None,
                shielded: false,
                expires: u64::MAX,
                frozen: false,
                deed_account: None,
            };
            Response::keep_state().send(effect_msg(
                token_address(&program, &token_id),
                program,
                token_seed(&token_id),
                &Effect::Mint(record),
            ))
        }
        Instruction::Transfer {
            token_id,
            new_owner,
            sealed,
            now,
            linked_target,
        } => {
            if let Some(sealed) = &sealed {
                check_content(&[], sealed);
            }
            let signer_tag = match &linked_target {
                Some(target) => linked_owner_tag(target, &signer),
                None => owner_tag(&token_id, &signer),
            };
            Response::keep_state().send(effect_msg(
                token_address(&program, &token_id),
                program,
                token_seed(&token_id),
                &Effect::Transfer {
                    token_id,
                    signer: signer_tag,
                    linked_target,
                    new_owner,
                    sealed,
                    now,
                },
            ))
        }
        Instruction::Burn { token_id } => Response::keep_state().send(effect_msg(
            token_address(&program, &token_id),
            program,
            token_seed(&token_id),
            &Effect::Burn {
                token_id,
                signer: owner_tag(&token_id, &signer),
            },
        )),
        Instruction::Shield {
            token_id,
            deed_account,
        } => Response::keep_state().send(effect_msg(
            token_address(&program, &token_id),
            program,
            token_seed(&token_id),
            &Effect::Shield {
                token_id,
                signer: owner_tag(&token_id, &signer),
                deed_account,
            },
        )),
        Instruction::Unshield {
            token_id,
            deed_account,
        } => Response::keep_state().send(effect_msg(
            token_address(&program, &token_id),
            program,
            token_seed(&token_id),
            &Effect::Unshield {
                token_id,
                signer: owner_tag(&token_id, &signer),
                deed_account,
            },
        )),
        Instruction::Configure { admin, authorities } => Response::keep_state().send(effect_msg(
            config_address(&program),
            program,
            config_seed(),
            &Effect::Configure {
                caller: signer,
                admin,
                authorities,
            },
        )),
    }
}

fn load_record(pre_data: &[u8]) -> NftRecord {
    assert!(!pre_data.is_empty(), "token does not exist");
    borsh::from_slice(pre_data).expect("a token account holds a valid record")
}

fn load_linked(input: &ReceiveInput, token_id: &TokenId) -> (NftRecord, LinkedAuthority) {
    let program = input.receiver.program_account_id;
    assert_eq!(
        input.receiver.account_id,
        token_address(&program, token_id),
        "the token account does not match its id"
    );
    let record = load_record(&input.pre_state);
    assert_eq!(record.token_id, *token_id, "token id mismatch");
    let linked = record
        .linked
        .clone()
        .expect("token has no linked authority");
    assert_eq!(
        input.from,
        Some(Actor::new(linked.authority_actor, linked.program)),
        "sender is not the token's linked authority"
    );
    (record, linked)
}

fn require_private_deed_target(program: AccountId, token_id: &TokenId, deed_account: AccountId) -> PdaSeed {
    let seed = deed_seed(token_id);
    assert_ne!(
        deed_account,
        AccountId::for_public_pda(&program, &seed),
        "deed must be a private PDA"
    );
    seed
}

fn handle_effect(input: &ReceiveInput, effect: Effect) -> Response {
    assert!(
        input.from_own_program(),
        "effects may only be sent by this program"
    );
    let program = input.receiver.program_account_id;
    let pre_data: &[u8] = &input.pre_state;

    match effect {
        Effect::CreateCollection(record) => {
            assert_eq!(
                input.receiver.account_id,
                collection_address(&program, &record.collection_id),
                "the collection account does not match its id"
            );
            assert!(pre_data.is_empty(), "collection id already taken");
            let collection_id = record.collection_id;
            Response::set_state(to_bytes(&record))
                .event(event(CollectionCreated::SELECTOR_NAME, &CollectionCreated { collection_id }))
        }
        Effect::Sell {
            collection_id,
            serial,
            price,
            payee,
        } => {
            assert_eq!(
                input.receiver.account_id,
                collection_address(&program, &collection_id),
                "the collection account does not match its id"
            );
            assert!(!pre_data.is_empty(), "collection does not exist");
            let mut record: CollectionRecord =
                borsh::from_slice(pre_data).expect("a collection account holds a valid record");
            assert_eq!(record.collection_id, collection_id, "collection id mismatch");
            assert!(record.minted < record.supply, "collection is sold out");
            assert!(
                serial == record.minted + 1,
                "serial {serial} is not the next one ({}); someone minted first, try again",
                record.minted + 1
            );
            assert!(
                price == record.price && payee == record.payee.as_ref().map(Payee::hash),
                "price or payee does not match the collection"
            );
            record.minted = serial;
            Response::set_state(to_bytes(&record))
        }
        Effect::Mint(record) => {
            assert_eq!(
                input.receiver.account_id,
                token_address(&program, &record.token_id),
                "the token account does not match its id"
            );
            assert!(pre_data.is_empty(), "token id already minted");
            let token_id = record.token_id;
            Response::set_state(to_bytes(&record))
                .event(event(Minted::SELECTOR_NAME, &Minted { token_id }))
        }
        Effect::Transfer {
            token_id,
            signer,
            linked_target,
            new_owner,
            sealed,
            now,
        } => {
            assert_eq!(
                input.receiver.account_id,
                token_address(&program, &token_id),
                "the token account does not match its id"
            );
            let mut record = load_record(pre_data);
            assert_eq!(record.token_id, token_id, "token id mismatch");
            assert!(!record.shielded, "token is shielded; unshield before transfer");
            assert!(!record.frozen, "token is frozen");

            if let Some(linked) = record.linked.clone() {
                assert_eq!(
                    linked_target,
                    Some(linked.target),
                    "linked token transfer must use linked target"
                );
                assert_eq!(record.owner, signer, "signer does not own this token");
                let now = now.expect("linked token transfer requires `now` timestamp");
                assert!(
                    now < record.expires,
                    "linked token has expired; renew before transfer"
                );
                let old_owner = record.owner;
                record.owner = new_owner;
                record.sealed = sealed.unwrap_or_default();
                let hook = RegistryTransferHook {
                    id: linked.target,
                    token_id,
                    expected_old_owner: old_owner,
                    new_owner,
                    now,
                };
                let resp = Response::set_state(to_bytes(&record))
                    .event(event(Transferred::SELECTOR_NAME, &Transferred { token_id }))
                    .send_sync(Actor::new(linked.record_actor, linked.program), &hook);
                bounded_window(resp, now)
            } else {
                assert!(
                    linked_target.is_none(),
                    "unlinked token must not pass linked_target"
                );
                assert_eq!(record.owner, signer, "signer does not own this token");
                if record.expires != u64::MAX {
                    let now = now.expect("expiring token transfer requires `now` timestamp");
                    assert!(now < record.expires, "token has expired");
                    record.owner = new_owner;
                    if let Some(sealed) = sealed {
                        record.sealed = sealed;
                    }
                    let resp = Response::set_state(to_bytes(&record))
                        .event(event(Transferred::SELECTOR_NAME, &Transferred { token_id }));
                    bounded_window(resp, now)
                } else {
                    record.owner = new_owner;
                    if let Some(sealed) = sealed {
                        record.sealed = sealed;
                    }
                    Response::set_state(to_bytes(&record))
                        .event(event(Transferred::SELECTOR_NAME, &Transferred { token_id }))
                }
            }
        }
        Effect::Burn { token_id, signer } => {
            assert_eq!(
                input.receiver.account_id,
                token_address(&program, &token_id),
                "the token account does not match its id"
            );
            let record = load_record(pre_data);
            assert_eq!(record.token_id, token_id, "token id mismatch");
            assert!(
                record.linked.is_none(),
                "linked token can only be burned by its linked authority"
            );
            assert_eq!(record.owner, signer, "signer does not own this token");
            assert!(!record.shielded, "token is shielded; unshield before burn");
            assert!(!record.frozen, "token is frozen");
            Response::set_state(ActorState::empty())
                .event(event(Burned::SELECTOR_NAME, &Burned { token_id }))
        }
        Effect::Shield {
            token_id,
            signer,
            deed_account,
        } => {
            assert_eq!(
                input.receiver.account_id,
                token_address(&program, &token_id),
                "the token account does not match its id"
            );
            let mut record = load_record(pre_data);
            assert_eq!(record.token_id, token_id, "token id mismatch");
            assert!(
                record.linked.is_none(),
                "linked token must be shielded through its linked authority"
            );
            assert_eq!(record.owner, signer, "signer does not own this token");
            assert!(!record.shielded, "token is already shielded");
            assert!(!record.frozen, "token is frozen");
            let seed = require_private_deed_target(program, &token_id, deed_account);
            record.shielded = true;
            record.deed_account = Some(deed_account);
            let deed = PrivateDeed {
                version: VERSION,
                collection_id: record.collection.map_or([0u8; 32], |(cid, _)| cid),
                token_id,
                expires: record.expires,
            };
            Response::set_state(to_bytes(&record))
                .event(event(Shielded::SELECTOR_NAME, &Shielded { token_id }))
                .send(effect_msg(
                    deed_account,
                    program,
                    seed,
                    &Effect::WriteDeed(deed),
                ))
        }
        Effect::Unshield {
            token_id,
            signer,
            deed_account,
        } => {
            assert_eq!(
                input.receiver.account_id,
                token_address(&program, &token_id),
                "the token account does not match its id"
            );
            let mut record = load_record(pre_data);
            assert_eq!(record.token_id, token_id, "token id mismatch");
            assert!(
                record.linked.is_none(),
                "linked token must be unshielded through its linked authority"
            );
            assert_eq!(record.owner, signer, "signer does not own this token");
            assert!(record.shielded, "token is not shielded");
            assert_eq!(
                record.deed_account.take(),
                Some(deed_account),
                "not the token's deed account"
            );
            let seed = require_private_deed_target(program, &token_id, deed_account);
            record.shielded = false;
            Response::set_state(to_bytes(&record))
                .event(event(Unshielded::SELECTOR_NAME, &Unshielded { token_id }))
                .send(effect_msg(
                    deed_account,
                    program,
                    seed,
                    &Effect::ClearDeed { token_id },
                ))
        }
        Effect::WriteDeed(deed) => {
            assert_eq!(
                input.from,
                Some(Actor::new(token_address(&program, &deed.token_id), program)),
                "deed writes must come from the token PDA"
            );
            require_private_deed_target(program, &deed.token_id, input.receiver.account_id);
            assert!(
                input.is_authorized(),
                "deed account must be authorized by private_seed_grant"
            );
            assert!(pre_data.is_empty(), "private deed already exists");
            Response::set_state(to_bytes(&deed))
        }
        Effect::UpdateDeed(deed) => {
            assert_eq!(
                input.from,
                Some(Actor::new(token_address(&program, &deed.token_id), program)),
                "deed updates must come from the token PDA"
            );
            require_private_deed_target(program, &deed.token_id, input.receiver.account_id);
            assert!(
                input.is_authorized(),
                "deed account must be authorized by private_seed_grant"
            );
            assert!(!pre_data.is_empty(), "private deed does not exist");
            let existing: PrivateDeed =
                borsh::from_slice(pre_data).expect("deed account holds a valid PrivateDeed");
            assert_eq!(existing.token_id, deed.token_id, "private deed token id mismatch");
            Response::set_state(to_bytes(&deed))
        }
        Effect::ClearDeed { token_id } => {
            assert_eq!(
                input.from,
                Some(Actor::new(token_address(&program, &token_id), program)),
                "deed clears must come from the token PDA"
            );
            require_private_deed_target(program, &token_id, input.receiver.account_id);
            assert!(
                input.is_authorized(),
                "deed account must be authorized by private_seed_grant"
            );
            assert!(!pre_data.is_empty(), "private deed does not exist");
            let existing: PrivateDeed =
                borsh::from_slice(pre_data).expect("deed account holds a valid PrivateDeed");
            assert_eq!(existing.token_id, token_id, "private deed token id mismatch");
            Response::set_state(ActorState::empty())
        }
        Effect::Configure {
            caller,
            admin,
            authorities,
        } => {
            assert_eq!(
                input.receiver.account_id,
                config_address(&program),
                "the config account does not match"
            );
            assert!(
                authorities.len() <= MAX_AUTHORITIES,
                "too many authorities (max {MAX_AUTHORITIES})"
            );
            for i in 0..authorities.len() {
                for j in (i + 1)..authorities.len() {
                    assert_ne!(
                        authorities[i].program, authorities[j].program,
                        "duplicate authority program"
                    );
                    if let (Some(a), Some(b)) =
                        (authorities[i].collection_id, authorities[j].collection_id)
                    {
                        assert_ne!(a, b, "duplicate reserved collection_id");
                    }
                }
            }
            if pre_data.is_empty() {
                assert_eq!(
                    caller, admin,
                    "initial configure must be signed by the chosen admin"
                );
            } else {
                let existing: Config =
                    borsh::from_slice(pre_data).expect("config account holds a valid Config");
                assert_eq!(
                    caller, existing.admin,
                    "only config admin can update config"
                );
            }
            let config = Config {
                version: VERSION,
                admin,
                authorities,
            };
            Response::set_state(to_bytes(&config))
                .event(event(Configured::SELECTOR_NAME, &Configured { admin }))
        }
        Effect::RequireAuthority {
            program: authority_program,
            collection_id,
        } => {
            assert_eq!(
                input.receiver.account_id,
                config_address(&program),
                "the config account does not match"
            );
            assert!(!pre_data.is_empty(), "nft config is not initialized");
            let config: Config =
                borsh::from_slice(pre_data).expect("config account holds a valid Config");
            let entry = config
                .authorities
                .iter()
                .find(|a| a.program == authority_program)
                .expect("sender program is not an authorized linking authority");
            if let Some(pinned_cid) = entry.collection_id {
                assert_eq!(
                    collection_id,
                    Some(pinned_cid),
                    "authority mint collection_id does not match pinned collection"
                );
            } else if let Some(cid) = collection_id {
                assert!(
                    !config
                        .authorities
                        .iter()
                        .any(|a| a.collection_id == Some(cid)),
                    "collection_id is reserved for another authority"
                );
            }
            Response::keep_state()
        }
        Effect::RequireCollectionUnreserved { collection_id } => {
            assert_eq!(
                input.receiver.account_id,
                config_address(&program),
                "the config account does not match"
            );
            if !pre_data.is_empty() {
                let config: Config =
                    borsh::from_slice(pre_data).expect("config account holds a valid Config");
                assert!(
                    !config
                        .authorities
                        .iter()
                        .any(|a| a.collection_id == Some(collection_id)),
                    "collection id is reserved for a linked authority"
                );
            }
            Response::keep_state()
        }
    }
}

fn handle_authority_sync(input: &ReceiveInput, op: AuthorityOp) -> Response {
    let program = input.receiver.program_account_id;

    match op {
        AuthorityOp::Mint {
            token_id,
            owner,
            linked,
            content,
            expires,
        } => {
            assert_eq!(
                input.receiver.account_id,
                token_address(&program, &token_id),
                "the token account does not match its id"
            );
            assert!(input.pre_state.is_empty(), "token id already minted");
            check_content(&content, &[]);
            let from = input.from.expect("authority mint requires a sender");
            assert_eq!(
                from,
                Actor::new(linked.authority_actor, linked.program),
                "sender must match linked authority"
            );
            let authority_program = linked.program;
            // Derived here, never sent: a linked token's collection is its authority's.
            let collection = Some((lns_collection_id(&authority_program), 0));
            let collection_id = collection.map(|(cid, _)| cid);
            let record = NftRecord {
                version: VERSION,
                token_id,
                owner,
                collection,
                content,
                sealed: Vec::new(),
                linked: Some(linked),
                shielded: false,
                expires,
                frozen: false,
                deed_account: None,
            };
            Response::set_state(to_bytes(&record))
                .event(event(Minted::SELECTOR_NAME, &Minted { token_id }))
                .send(effect_msg(
                    config_address(&program),
                    program,
                    config_seed(),
                    &Effect::RequireAuthority {
                        program: authority_program,
                        collection_id,
                    },
                ))
        }
        AuthorityOp::Transfer {
            token_id,
            new_owner,
        } => {
            let (mut record, _) = load_linked(input, &token_id);
            assert!(
                !record.shielded,
                "token is shielded; unshield before transfer"
            );
            assert!(!record.frozen, "token is frozen");
            record.owner = new_owner;
            record.sealed = Vec::new();
            Response::set_state(to_bytes(&record))
                .event(event(Transferred::SELECTOR_NAME, &Transferred { token_id }))
        }
        AuthorityOp::Burn { token_id } => {
            let _ = load_linked(input, &token_id);
            Response::set_state(ActorState::empty())
                .event(event(Burned::SELECTOR_NAME, &Burned { token_id }))
        }
        AuthorityOp::SetExpiry { token_id, expires } => {
            let (mut record, _) = load_linked(input, &token_id);
            record.expires = expires;
            Response::set_state(to_bytes(&record)).event(event(
                ExpiryUpdated::SELECTOR_NAME,
                &ExpiryUpdated { token_id, expires },
            ))
        }
        AuthorityOp::Shield {
            token_id,
            deed_account,
            expires,
        } => {
            let (mut record, _) = load_linked(input, &token_id);
            assert!(!record.shielded, "token is already shielded");
            assert!(!record.frozen, "token is frozen");
            let seed = require_private_deed_target(program, &token_id, deed_account);
            record.shielded = true;
            record.deed_account = Some(deed_account);
            record.expires = expires;
            let deed = PrivateDeed {
                version: VERSION,
                collection_id: record.collection.map_or([0u8; 32], |(cid, _)| cid),
                token_id,
                expires,
            };
            Response::set_state(to_bytes(&record))
                .event(event(Shielded::SELECTOR_NAME, &Shielded { token_id }))
                .send(effect_msg(
                    deed_account,
                    program,
                    seed,
                    &Effect::WriteDeed(deed),
                ))
        }
        AuthorityOp::Unshield { token_id } => {
            let (mut record, _) = load_linked(input, &token_id);
            assert!(record.shielded, "token is not shielded");
            let deed_account = record.deed_account.take().expect("a shielded token keeps its deed account");
            let seed = require_private_deed_target(program, &token_id, deed_account);
            record.shielded = false;
            Response::set_state(to_bytes(&record))
                .event(event(Unshielded::SELECTOR_NAME, &Unshielded { token_id }))
                .send(effect_msg(
                    deed_account,
                    program,
                    seed,
                    &Effect::ClearDeed { token_id },
                ))
        }
        AuthorityOp::RefreshShielded { token_id, expires } => {
            let (mut record, _) = load_linked(input, &token_id);
            assert!(record.shielded, "token is not shielded");
            let deed_account = record.deed_account.expect("a shielded token keeps its deed account");
            let seed = require_private_deed_target(program, &token_id, deed_account);
            record.expires = expires;
            let deed = PrivateDeed {
                version: VERSION,
                collection_id: record.collection.map_or([0u8; 32], |(cid, _)| cid),
                token_id,
                expires,
            };
            Response::set_state(to_bytes(&record))
                .event(event(
                    ExpiryUpdated::SELECTOR_NAME,
                    &ExpiryUpdated { token_id, expires },
                ))
                .send(effect_msg(
                    deed_account,
                    program,
                    seed,
                    &Effect::UpdateDeed(deed),
                ))
        }
        AuthorityOp::SetFrozen { token_id, frozen } => {
            let (mut record, _) = load_linked(input, &token_id);
            record.frozen = frozen;
            Response::set_state(to_bytes(&record)).event(event(
                FrozenUpdated::SELECTOR_NAME,
                &FrozenUpdated { token_id, frozen },
            ))
        }
    }
}

fn handle_read_state(input: &ReceiveInput) -> Response {
    let reader = input
        .from
        .expect("ReadState requires a sender to reply to");
    Response::keep_state()
        .send(SyncMessage::new(reader, &StateReply::from(input)).with_lent_grant(input))
}

fn handle_require_holder(input: &ReceiveInput, tag: OwnerTag, now: Option<u64>) -> Response {
    let record = load_record(&input.pre_state);
    assert_eq!(record.owner, tag, "holder tag does not match token owner");
    assert!(!record.frozen, "token is frozen");
    if let Some(now) = now {
        assert!(now < record.expires, "token has expired");
        bounded_window(Response::keep_state(), now)
    } else {
        Response::keep_state()
    }
}

// ---------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use lee_core::{native_token::NATIVE_TOKEN_PROGRAM_ID, program::ValidityWindows};

    use super::*;

    const PROGRAM: AccountId = AccountId::new([7; 32]);
    const REGISTRY: AccountId = AccountId::new([9; 32]);
    const EVIL_PROGRAM: AccountId = AccountId::new([0x66; 32]);
    const READER_PROGRAM: AccountId = AccountId::new([0x55; 32]);
    const ADMIN: AccountId = AccountId::new([10; 32]);
    const ALICE: AccountId = AccountId::new([1; 32]);
    const BOB: AccountId = AccountId::new([2; 32]);
    const PAYEE: AccountId = AccountId::new([3; 32]);
    const RECORD_PDA: AccountId = AccountId::new([0x41; 32]);
    /// The registry's settings PDA: the only actor that sends `AuthoritySync`.
    const AUTHORITY_PDA: AccountId = AccountId::new([0x40; 32]);
    const TOKEN_A: TokenId = [0xAA; 32];
    const TOKEN_B: TokenId = [0xBB; 32];
    const COLL: CollectionId = [0xCC; 32];
    const SLOT_A: NameId = [0x11; 32];

    fn lns_coll() -> CollectionId {
        lns_collection_id(&REGISTRY)
    }

    fn top_level_link() -> LinkedAuthority {
        LinkedAuthority {
            program: REGISTRY,
            authority_actor: AUTHORITY_PDA,
            target: SLOT_A,
            record_actor: RECORD_PDA,
        }
    }

    fn test_vpk(byte: u8) -> ViewingPublicKey {
        borsh::from_slice(&borsh::to_vec(&vec![byte; VPK_LEN]).unwrap()).unwrap()
    }

    struct Delivery {
        from: Option<Actor>,
        to: Actor,
        grants: BTreeSet<Actor>,
        pda_seeds: BTreeSet<PdaSeed>,
        message: Vec<u8>,
    }

    #[derive(Default)]
    struct Chain {
        states: BTreeMap<Actor, ActorState>,
        /// Private PDAs: `account_id -> (owning_program, seed)`.
        private_pdas: BTreeMap<AccountId, (AccountId, PdaSeed)>,
        /// Native transfers executed: `(from, to, amount)`.
        paid: Vec<(AccountId, AccountId, u128)>,
        /// Registry transfer hooks observed: `(from, to, hook)`.
        registry_hooks: Vec<(Actor, Actor, RegistryTransferHook)>,
        /// State replies observed by `READER_PROGRAM`: `(from, to, grants, reply)`.
        state_replies: Vec<(Actor, Actor, BTreeSet<Actor>, StateReply)>,
        /// Emitted events.
        events: Vec<(Actor, ProgramEvent)>,
        /// Intersected validity window of the last transaction.
        last_validity: ValidityWindows,
    }

    impl Chain {
        fn fund(&mut self, account: AccountId, amount: u128) {
            self.states.insert(
                Actor::native_balance(account),
                native_token::encode_balance(amount),
            );
        }

        fn register_private_deed(
            &mut self,
            token_id: &TokenId,
            npk_byte: u8,
            vpk_byte: u8,
        ) -> AccountId {
            let npk = NullifierPublicKey([npk_byte; 32]);
            let vpk = test_vpk(vpk_byte);
            let addr = deed_address(&PROGRAM, token_id, &npk, &vpk);
            self.private_pdas.insert(addr, (PROGRAM, deed_seed(token_id)));
            addr
        }

        fn execute(
            &mut self,
            from: Option<Actor>,
            to: Actor,
            grants: BTreeSet<Actor>,
            message: &Message,
        ) {
            let mut next_states = self.states.clone();
            let mut validity = ValidityWindows::new_unbounded();
            let mut stack = vec![Delivery {
                from,
                to,
                grants,
                pda_seeds: BTreeSet::new(),
                message: to_bytes(message),
            }];

            while let Some(delivery) = stack.pop() {
                let mut effective_grants = delivery.grants.clone();
                if let (Some(from_actor), Some(&(pda_prog, pda_seed))) = (
                    delivery.from,
                    self.private_pdas.get(&delivery.to.account_id),
                ) {
                    if from_actor.program_account_id == pda_prog
                        && delivery.pda_seeds.contains(&pda_seed)
                    {
                        effective_grants.insert(delivery.to);
                    }
                }
                let pre_state = next_states
                    .get(&delivery.to)
                    .cloned()
                    .unwrap_or_else(ActorState::empty);
                let input = ReceiveInput {
                    receiver: delivery.to,
                    from: delivery.from,
                    grants: effective_grants.clone(),
                    pre_state,
                    message: delivery.message,
                };

                if delivery.to.program_account_id == PROGRAM {
                    let msg: Message = borsh::from_slice(&input.message).expect("decodes Message");
                    let resp = handle(&input, msg);
                    validity = validity
                        .intersect(resp.validity)
                        .expect("non-empty validity window intersection");
                    if let Some(post) = resp.post_state {
                        if post.is_empty() {
                            next_states.remove(&delivery.to);
                        } else {
                            next_states.insert(delivery.to, post);
                        }
                    }
                    for ev in resp.events {
                        self.events.push((delivery.to, ev));
                    }
                    for sync in resp.sync_messages.into_iter().rev() {
                        for g in &sync.grants {
                            assert!(
                                effective_grants.contains(g),
                                "actor attached unheld grant {g:?}"
                            );
                        }
                        stack.push(Delivery {
                            from: Some(delivery.to),
                            to: sync.to,
                            grants: sync.grants,
                            pda_seeds: sync.pda_seeds,
                            message: sync.message,
                        });
                    }
                } else if delivery.to.program_account_id == NATIVE_TOKEN_PROGRAM_ID {
                    let transition =
                        native_token::handle_message(&input).expect("native token succeeds");
                    if let Some(post) = transition.response.post_state {
                        if post.is_empty() {
                            next_states.remove(&delivery.to);
                        } else {
                            next_states.insert(delivery.to, post);
                        }
                    }
                    if let Ok(native_token::Message::Transfer { to, amount }) =
                        borsh::from_slice::<native_token::Message>(&input.message)
                    {
                        self.paid.push((delivery.to.account_id, to, amount));
                    }
                } else if delivery.to.program_account_id == REGISTRY {
                    let hook: RegistryTransferHook =
                        borsh::from_slice(&input.message).expect("decodes RegistryTransferHook");
                    let sender = delivery.from.expect("hook must have sender");
                    self.registry_hooks.push((sender, delivery.to, hook));
                } else if delivery.to.program_account_id == READER_PROGRAM {
                    let reply: StateReply =
                        borsh::from_slice(&input.message).expect("decodes StateReply");
                    let sender = delivery.from.expect("reply must have sender");
                    self.state_replies
                        .push((sender, delivery.to, effective_grants, reply));
                } else {
                    panic!("unexpected target program {:?}", delivery.to.program_account_id);
                }
            }

            self.states = next_states;
            self.last_validity = validity;
        }

        fn run_instruction(
            &mut self,
            signer: AccountId,
            signed: bool,
            extra_grants: impl IntoIterator<Item = Actor>,
            instruction: Instruction,
        ) {
            let root = Actor::new(signer, PROGRAM);
            let mut grants: BTreeSet<Actor> = extra_grants.into_iter().collect();
            if signed {
                grants.insert(root);
            }
            self.execute(None, root, grants, &Message::Instruction(instruction));
        }

        fn run_authority(&mut self, from: Actor, token_id: &TokenId, op: AuthorityOp) {
            let to = Actor::new(token_address(&PROGRAM, token_id), PROGRAM);
            self.execute(Some(from), to, BTreeSet::new(), &Message::AuthoritySync(op));
        }

        fn configure_lns(&mut self) {
            self.run_instruction(
                ADMIN,
                true,
                [],
                Instruction::Configure {
                    admin: ADMIN,
                    authorities: vec![AuthorizedProgram {
                        program: REGISTRY,
                        collection_id: Some(lns_coll()),
                    }],
                },
            );
        }

        fn mint_linked(&mut self, token_id: TokenId, owner: AccountId, link: LinkedAuthority, expires: u64) {
            let from = Actor::new(link.authority_actor, link.program);
            let tag = linked_owner_tag(&link.target, &owner);
            self.run_authority(
                from,
                &token_id,
                AuthorityOp::Mint {
                    token_id,
                    owner: tag,
                    linked: link,
                    content: br#"{"name":"LNS Deed"}"#.to_vec(),
                    expires,
                },
            );
        }

        fn record(&self, token_id: &TokenId) -> Option<NftRecord> {
            let data = self
                .states
                .get(&Actor::new(token_address(&PROGRAM, token_id), PROGRAM))?;
            (!data.is_empty()).then(|| borsh::from_slice(data).expect("valid NftRecord"))
        }

        fn deed(&self, deed_account: AccountId) -> Option<PrivateDeed> {
            let data = self.states.get(&Actor::new(deed_account, PROGRAM))?;
            (!data.is_empty()).then(|| borsh::from_slice(data).expect("valid PrivateDeed"))
        }

        fn collection(&self) -> CollectionRecord {
            let data = &self.states[&Actor::new(collection_address(&PROGRAM, &COLL), PROGRAM)];
            borsh::from_slice(data).expect("valid CollectionRecord")
        }

        fn mint(&mut self, token_id: TokenId, minter: AccountId, content: &[u8]) {
            self.run_instruction(
                minter,
                true,
                [],
                Instruction::Mint {
                    token_id,
                    owner: None,
                    content: content.to_vec(),
                    sealed: vec![],
                },
            );
        }

        fn create(&mut self, price: u128, supply: u32) {
            self.run_instruction(
                ALICE,
                true,
                [],
                Instruction::CreateCollection {
                    collection_id: COLL,
                    payee: (price > 0).then_some(Payee::Public(PAYEE)),
                    price,
                    supply,
                    content: b"{\"name\":\"c\"}".to_vec(),
                },
            );
        }

        fn buy(&mut self, buyer: AccountId, serial: u32, price: u128) {
            let extra = if price > 0 {
                vec![Actor::native_balance(buyer)]
            } else {
                vec![]
            };
            self.run_instruction(
                buyer,
                true,
                extra,
                Instruction::MintFromCollection {
                    collection_id: COLL,
                    serial,
                    price,
                    payee: (price > 0).then_some(Payee::Public(PAYEE)),
                    payee_opening: None,
                    owner: None,
                },
            );
        }
    }

    // ---- Ordinary (unlinked) NFT tests (v2 parity on v0.4.0) ----

    #[test]
    fn mint_creates_a_record_owned_by_the_minter() {
        let mut chain = Chain::default();
        chain.mint(TOKEN_A, ALICE, b"{}");
        assert_eq!(
            chain.record(&TOKEN_A),
            Some(NftRecord {
                version: VERSION,
                token_id: TOKEN_A,
                owner: owner_tag(&TOKEN_A, &ALICE),
                collection: None,
                content: b"{}".to_vec(),
                sealed: vec![],
                linked: None,
                shielded: false,
                expires: u64::MAX,
                frozen: false,
                deed_account: None,
            })
        );
    }

    #[test]
    fn mint_can_go_to_another_owner() {
        let mut chain = Chain::default();
        chain.run_instruction(
            ALICE,
            true,
            [],
            Instruction::Mint {
                token_id: TOKEN_A,
                owner: Some(owner_tag(&TOKEN_A, &BOB)),
                content: vec![],
                sealed: b"key".to_vec(),
            },
        );
        let record = chain.record(&TOKEN_A).expect("exists");
        assert_eq!(record.owner, owner_tag(&TOKEN_A, &BOB));
        assert_eq!(record.sealed, b"key");
    }

    #[test]
    #[should_panic(expected = "token id already minted")]
    fn cannot_mint_the_same_token_twice() {
        let mut chain = Chain::default();
        chain.mint(TOKEN_A, ALICE, b"first");
        chain.mint(TOKEN_A, BOB, b"second");
    }

    #[test]
    #[should_panic(expected = "the account must sign")]
    fn mint_needs_a_signature() {
        let mut chain = Chain::default();
        chain.run_instruction(
            ALICE,
            false,
            [],
            Instruction::Mint {
                token_id: TOKEN_A,
                owner: None,
                content: vec![],
                sealed: vec![],
            },
        );
    }

    #[test]
    fn transfer_changes_the_owner_and_can_reseal() {
        let mut chain = Chain::default();
        chain.mint(TOKEN_A, ALICE, b"data");
        chain.run_instruction(
            ALICE,
            true,
            [],
            Instruction::Transfer {
                token_id: TOKEN_A,
                new_owner: owner_tag(&TOKEN_A, &BOB),
                sealed: Some(b"for bob".to_vec()),
                now: None,
                linked_target: None,
            },
        );
        let record = chain.record(&TOKEN_A).expect("exists");
        assert_eq!(record.owner, owner_tag(&TOKEN_A, &BOB));
        assert_eq!(record.sealed, b"for bob");
        assert_eq!(record.content, b"data");
        assert!(chain.registry_hooks.is_empty());
    }

    #[test]
    #[should_panic(expected = "signer does not own this token")]
    fn non_owner_cannot_transfer() {
        let mut chain = Chain::default();
        chain.mint(TOKEN_A, ALICE, b"data");
        chain.run_instruction(
            BOB,
            true,
            [],
            Instruction::Transfer {
                token_id: TOKEN_A,
                new_owner: owner_tag(&TOKEN_A, &BOB),
                sealed: None,
                now: None,
                linked_target: None,
            },
        );
    }

    #[test]
    fn burn_removes_the_record() {
        let mut chain = Chain::default();
        chain.mint(TOKEN_A, ALICE, b"data");
        chain.run_instruction(ALICE, true, [], Instruction::Burn { token_id: TOKEN_A });
        assert_eq!(chain.record(&TOKEN_A), None);
    }

    #[test]
    #[should_panic(expected = "signer does not own this token")]
    fn non_owner_cannot_burn() {
        let mut chain = Chain::default();
        chain.mint(TOKEN_A, ALICE, b"data");
        chain.run_instruction(BOB, true, [], Instruction::Burn { token_id: TOKEN_A });
    }

    #[test]
    #[should_panic(expected = "token does not exist")]
    fn cannot_burn_unminted_token() {
        let mut chain = Chain::default();
        chain.run_instruction(ALICE, true, [], Instruction::Burn { token_id: TOKEN_A });
    }

    #[test]
    fn two_tokens_are_independent() {
        let mut chain = Chain::default();
        chain.mint(TOKEN_A, ALICE, b"a");
        chain.mint(TOKEN_B, BOB, b"b");
        chain.run_instruction(ALICE, true, [], Instruction::Burn { token_id: TOKEN_A });
        assert_eq!(chain.record(&TOKEN_B).expect("still exists").content, b"b");
    }

    #[test]
    fn owner_tags_differ_across_tokens_for_the_same_account() {
        assert_ne!(owner_tag(&TOKEN_A, &ALICE), owner_tag(&TOKEN_B, &ALICE));
    }

    #[test]
    #[should_panic(expected = "content exceeds")]
    fn oversized_content_is_refused() {
        let mut chain = Chain::default();
        chain.mint(TOKEN_A, ALICE, &vec![0u8; MAX_CONTENT_LEN + 1]);
    }

    #[test]
    fn a_paid_collection_sells_in_order_and_pays_the_payee() {
        let mut chain = Chain::default();
        chain.fund(BOB, 1_000);
        chain.fund(ALICE, 1_000);
        chain.create(500, 2);
        chain.buy(BOB, 1, 500);
        chain.buy(ALICE, 2, 500);
        assert_eq!(chain.collection().minted, 2);
        assert_eq!(chain.paid, vec![(BOB, PAYEE, 500), (ALICE, PAYEE, 500)]);
        let first = chain.record(&serial_token_id(&COLL, 1)).expect("minted");
        assert_eq!(first.owner, owner_tag(&first.token_id, &BOB));
        assert_eq!(first.collection, Some((COLL, 1)));
        assert_eq!(chain.collection().creator, creator_tag(&COLL, &ALICE));
    }

    #[test]
    fn a_free_collection_charges_nothing() {
        let mut chain = Chain::default();
        chain.create(0, 1);
        chain.buy(BOB, 1, 0);
        assert!(chain.paid.is_empty());
        assert_eq!(chain.collection().minted, 1);
    }

    #[test]
    #[should_panic(expected = "payer must authorize native balance transfer")]
    fn paid_mint_requires_native_balance_grant() {
        let mut chain = Chain::default();
        chain.fund(BOB, 1_000);
        chain.create(500, 1);
        chain.run_instruction(
            BOB,
            true,
            [],
            Instruction::MintFromCollection {
                collection_id: COLL,
                serial: 1,
                price: 500,
                payee: Some(Payee::Public(PAYEE)),
                payee_opening: None,
                owner: None,
            },
        );
    }

    #[test]
    #[should_panic(expected = "collection is sold out")]
    fn a_collection_cannot_oversell() {
        let mut chain = Chain::default();
        chain.create(0, 1);
        chain.buy(BOB, 1, 0);
        chain.buy(BOB, 2, 0);
    }

    #[test]
    #[should_panic(expected = "is not the next one")]
    fn serials_cannot_be_skipped() {
        let mut chain = Chain::default();
        chain.create(0, 5);
        chain.buy(BOB, 2, 0);
    }

    #[test]
    #[should_panic(expected = "price or payee does not match")]
    fn underpaying_is_refused() {
        let mut chain = Chain::default();
        chain.fund(BOB, 1_000);
        chain.create(500, 5);
        chain.buy(BOB, 1, 1);
    }

    #[test]
    #[should_panic(expected = "price or payee does not match")]
    fn a_free_mint_of_a_paid_collection_is_refused() {
        let mut chain = Chain::default();
        chain.create(500, 5);
        chain.buy(BOB, 1, 0);
    }

    fn private_payee() -> Payee {
        Payee::Private {
            npk: [9; 32],
            vpk: vec![5; VPK_LEN],
        }
    }

    #[test]
    fn a_private_payee_is_paid_at_the_account_its_keys_and_opening_give() {
        let mut chain = Chain::default();
        chain.fund(BOB, 1_000);
        let payee = private_payee();
        chain.run_instruction(
            ALICE,
            true,
            [],
            Instruction::CreateCollection {
                collection_id: COLL,
                payee: Some(payee.clone()),
                price: 500,
                supply: 3,
                content: vec![],
            },
        );
        let opening = [4; 32];
        let expected = payee.account(Some(opening));
        chain.run_instruction(
            BOB,
            true,
            [Actor::native_balance(BOB)],
            Instruction::MintFromCollection {
                collection_id: COLL,
                serial: 1,
                price: 500,
                payee: Some(payee.clone()),
                payee_opening: Some(opening),
                owner: None,
            },
        );
        assert_eq!(chain.paid, vec![(BOB, expected, 500)]);
        assert_ne!(expected, payee.account(Some([5; 32])));
        assert_ne!(expected, payee.account(None));
    }

    #[test]
    #[should_panic(expected = "price or payee does not match")]
    fn a_private_payee_cannot_be_swapped_for_other_keys() {
        let mut chain = Chain::default();
        chain.fund(BOB, 1_000);
        chain.run_instruction(
            ALICE,
            true,
            [],
            Instruction::CreateCollection {
                collection_id: COLL,
                payee: Some(private_payee()),
                price: 500,
                supply: 3,
                content: vec![],
            },
        );
        let other = Payee::Private {
            npk: [8; 32],
            vpk: vec![5; VPK_LEN],
        };
        chain.run_instruction(
            BOB,
            true,
            [Actor::native_balance(BOB)],
            Instruction::MintFromCollection {
                collection_id: COLL,
                serial: 1,
                price: 500,
                payee: Some(other),
                payee_opening: Some([4; 32]),
                owner: None,
            },
        );
    }

    #[test]
    #[should_panic(expected = "collection id already taken")]
    fn a_collection_id_is_taken_once() {
        let mut chain = Chain::default();
        chain.create(0, 1);
        chain.create(0, 1);
    }

    #[test]
    #[should_panic(expected = "a priced collection needs a payee")]
    fn a_priced_collection_needs_a_payee() {
        let mut chain = Chain::default();
        chain.run_instruction(
            ALICE,
            true,
            [],
            Instruction::CreateCollection {
                collection_id: COLL,
                payee: None,
                price: 5,
                supply: 1,
                content: vec![],
            },
        );
    }

    #[test]
    #[should_panic(expected = "supply must be")]
    fn zero_supply_is_refused() {
        let mut chain = Chain::default();
        chain.create(0, 0);
    }

    // ---- Actor boundary & Config tests ----

    #[test]
    #[should_panic(expected = "effects may only be sent by this program")]
    fn direct_effect_from_root_is_refused() {
        let mut chain = Chain::default();
        let to = Actor::new(token_address(&PROGRAM, &TOKEN_A), PROGRAM);
        chain.execute(
            None,
            to,
            BTreeSet::new(),
            &Message::Effect(Effect::Burn {
                token_id: TOKEN_A,
                signer: owner_tag(&TOKEN_A, &ALICE),
            }),
        );
    }

    #[test]
    #[should_panic(expected = "effects may only be sent by this program")]
    fn direct_effect_from_foreign_program_is_refused() {
        let mut chain = Chain::default();
        let to = Actor::new(token_address(&PROGRAM, &TOKEN_A), PROGRAM);
        chain.execute(
            Some(Actor::new(RECORD_PDA, EVIL_PROGRAM)),
            to,
            BTreeSet::new(),
            &Message::Effect(Effect::Burn {
                token_id: TOKEN_A,
                signer: owner_tag(&TOKEN_A, &ALICE),
            }),
        );
    }

    #[test]
    fn configure_sets_admin_and_authorities_and_allows_admin_update() {
        let mut chain = Chain::default();
        chain.configure_lns();
        let cfg_bytes = &chain.states[&Actor::new(config_address(&PROGRAM), PROGRAM)];
        let cfg: Config = borsh::from_slice(cfg_bytes).unwrap();
        assert_eq!(cfg.version, VERSION);
        assert_eq!(cfg.admin, ADMIN);
        assert_eq!(cfg.authorities.len(), 1);
        assert_eq!(cfg.authorities[0].program, REGISTRY);

        // Admin rotates admin to BOB
        chain.run_instruction(
            ADMIN,
            true,
            [],
            Instruction::Configure {
                admin: BOB,
                authorities: cfg.authorities,
            },
        );
        let cfg_bytes = &chain.states[&Actor::new(config_address(&PROGRAM), PROGRAM)];
        let cfg: Config = borsh::from_slice(cfg_bytes).unwrap();
        assert_eq!(cfg.admin, BOB);
    }

    #[test]
    #[should_panic(expected = "only config admin can update config")]
    fn non_admin_cannot_update_config() {
        let mut chain = Chain::default();
        chain.configure_lns();
        chain.run_instruction(
            ALICE,
            true,
            [],
            Instruction::Configure {
                admin: ALICE,
                authorities: vec![],
            },
        );
    }

    #[test]
    #[should_panic(expected = "duplicate authority program")]
    fn configure_rejects_duplicate_authority() {
        let mut chain = Chain::default();
        chain.run_instruction(
            ADMIN,
            true,
            [],
            Instruction::Configure {
                admin: ADMIN,
                authorities: vec![
                    AuthorizedProgram {
                        program: REGISTRY,
                        collection_id: Some(lns_coll()),
                    },
                    AuthorizedProgram {
                        program: REGISTRY,
                        collection_id: None,
                    },
                ],
            },
        );
    }

    #[test]
    #[should_panic(expected = "collection id is reserved for a linked authority")]
    fn create_collection_refuses_reserved_authority_collection_id() {
        let mut chain = Chain::default();
        chain.configure_lns();
        chain.run_instruction(
            ALICE,
            true,
            [],
            Instruction::CreateCollection {
                collection_id: lns_coll(),
                payee: None,
                price: 0,
                supply: 10,
                content: vec![],
            },
        );
    }

    // ---- Linked token (LNS) lifecycle tests ----

    #[test]
    fn authority_mint_creates_linked_record_in_the_authoritys_collection() {
        let mut chain = Chain::default();
        chain.configure_lns();
        chain.mint_linked(TOKEN_A, ALICE, top_level_link(), 2_000_000);

        let rec_a = chain.record(&TOKEN_A).expect("top-level exists");
        assert_eq!(rec_a.owner, linked_owner_tag(&SLOT_A, &ALICE));
        assert_eq!(record_owner_tag(&rec_a, &ALICE), rec_a.owner);
        assert_eq!(rec_a.linked, Some(top_level_link()));
        assert_eq!(rec_a.collection, Some((lns_coll(), 0)));
        assert_eq!(rec_a.expires, 2_000_000);
        assert!(!rec_a.shielded);
        assert!(!rec_a.frozen);
        assert_eq!(rec_a.deed_account, None);
    }

    #[test]
    #[should_panic(expected = "nft config is not initialized")]
    fn authority_mint_refused_when_config_uninitialized() {
        let mut chain = Chain::default();
        chain.mint_linked(TOKEN_A, ALICE, top_level_link(), u64::MAX);
    }

    #[test]
    #[should_panic(expected = "sender program is not an authorized linking authority")]
    fn authority_mint_refused_for_unauthorized_program() {
        let mut chain = Chain::default();
        chain.configure_lns();
        let link = LinkedAuthority {
            program: EVIL_PROGRAM,
            authority_actor: AUTHORITY_PDA,
            target: SLOT_A,
            record_actor: RECORD_PDA,
        };
        chain.run_authority(
            Actor::new(AUTHORITY_PDA, EVIL_PROGRAM),
            &TOKEN_A,
            AuthorityOp::Mint {
                token_id: TOKEN_A,
                owner: linked_owner_tag(&SLOT_A, &ALICE),
                linked: link,
                content: vec![],
                expires: u64::MAX,
            },
        );
    }

    #[test]
    #[should_panic(expected = "sender must match linked authority")]
    fn authority_mint_refused_when_sent_by_the_record_instead_of_the_authority_actor() {
        let mut chain = Chain::default();
        chain.configure_lns();
        chain.run_authority(
            Actor::new(RECORD_PDA, REGISTRY),
            &TOKEN_A,
            AuthorityOp::Mint {
                token_id: TOKEN_A,
                owner: linked_owner_tag(&SLOT_A, &ALICE),
                linked: top_level_link(),
                content: vec![],
                expires: u64::MAX,
            },
        );
    }

    #[test]
    #[should_panic(expected = "authority mint collection_id does not match pinned collection")]
    fn authority_mint_refused_when_collection_mismatches_pin() {
        let mut chain = Chain::default();
        chain.run_instruction(
            ADMIN,
            true,
            [],
            Instruction::Configure {
                admin: ADMIN,
                authorities: vec![AuthorizedProgram {
                    program: REGISTRY,
                    collection_id: Some(COLL),
                }],
            },
        );
        chain.mint_linked(TOKEN_A, ALICE, top_level_link(), u64::MAX);
    }

    #[test]
    fn authority_transfer_updates_owner_without_callback() {
        let mut chain = Chain::default();
        chain.configure_lns();
        chain.mint_linked(TOKEN_A, ALICE, top_level_link(), 5_000_000);
        let bob_tag = linked_owner_tag(&SLOT_A, &BOB);
        chain.run_authority(
            Actor::new(AUTHORITY_PDA, REGISTRY),
            &TOKEN_A,
            AuthorityOp::Transfer {
                token_id: TOKEN_A,
                new_owner: bob_tag,
            },
        );
        assert_eq!(chain.record(&TOKEN_A).unwrap().owner, bob_tag);
        assert!(chain.registry_hooks.is_empty(), "Path A must not call back");
    }

    #[test]
    #[should_panic(expected = "sender is not the token's linked authority")]
    fn authority_transfer_refused_from_wrong_record_actor() {
        let mut chain = Chain::default();
        chain.configure_lns();
        chain.mint_linked(TOKEN_A, ALICE, top_level_link(), 5_000_000);
        chain.run_authority(
            Actor::new(RECORD_PDA, REGISTRY),
            &TOKEN_A,
            AuthorityOp::Transfer {
                token_id: TOKEN_A,
                new_owner: linked_owner_tag(&SLOT_A, &BOB),
            },
        );
    }

    #[test]
    #[should_panic(expected = "token has no linked authority")]
    fn authority_transfer_refused_on_unlinked_token() {
        let mut chain = Chain::default();
        chain.mint(TOKEN_A, ALICE, b"{}");
        chain.run_authority(
            Actor::new(AUTHORITY_PDA, REGISTRY),
            &TOKEN_A,
            AuthorityOp::Transfer {
                token_id: TOKEN_A,
                new_owner: owner_tag(&TOKEN_A, &BOB),
            },
        );
    }

    #[test]
    fn linked_nft_transfer_moves_owner_notifies_registry_and_bounds_window() {
        let mut chain = Chain::default();
        chain.configure_lns();
        chain.mint_linked(TOKEN_A, ALICE, top_level_link(), 5_000_000);

        let now = 1_000_000;
        let bob_tag = linked_owner_tag(&SLOT_A, &BOB);
        chain.run_instruction(
            ALICE,
            true,
            [],
            Instruction::Transfer {
                token_id: TOKEN_A,
                new_owner: bob_tag,
                sealed: None,
                now: Some(now),
                linked_target: Some(SLOT_A),
            },
        );

        let rec = chain.record(&TOKEN_A).unwrap();
        assert_eq!(rec.owner, bob_tag);
        assert_eq!(chain.last_validity.timestamps.start(), Some(now));
        assert_eq!(
            chain.last_validity.timestamps.end(),
            Some(now + TIME_WINDOW_MS)
        );
        assert_eq!(chain.registry_hooks.len(), 1);
        let (from, to, hook) = &chain.registry_hooks[0];
        assert_eq!(*from, Actor::new(token_address(&PROGRAM, &TOKEN_A), PROGRAM));
        assert_eq!(*to, Actor::new(RECORD_PDA, REGISTRY));
        assert_eq!(
            *hook,
            RegistryTransferHook {
                id: SLOT_A,
                token_id: TOKEN_A,
                expected_old_owner: linked_owner_tag(&SLOT_A, &ALICE),
                new_owner: bob_tag,
                now,
            }
        );
    }

    #[test]
    #[should_panic(expected = "linked token has expired")]
    fn linked_nft_transfer_refused_when_expired_or_in_grace() {
        let mut chain = Chain::default();
        chain.configure_lns();
        chain.mint_linked(TOKEN_A, ALICE, top_level_link(), 1_000_000);
        chain.run_instruction(
            ALICE,
            true,
            [],
            Instruction::Transfer {
                token_id: TOKEN_A,
                new_owner: linked_owner_tag(&SLOT_A, &BOB),
                sealed: None,
                now: Some(1_000_000),
                linked_target: Some(SLOT_A),
            },
        );
    }

    #[test]
    #[should_panic(expected = "linked token transfer requires `now` timestamp")]
    fn linked_nft_transfer_refused_without_now() {
        let mut chain = Chain::default();
        chain.configure_lns();
        chain.mint_linked(TOKEN_A, ALICE, top_level_link(), 5_000_000);
        chain.run_instruction(
            ALICE,
            true,
            [],
            Instruction::Transfer {
                token_id: TOKEN_A,
                new_owner: linked_owner_tag(&SLOT_A, &BOB),
                sealed: None,
                now: None,
                linked_target: Some(SLOT_A),
            },
        );
    }

    #[test]
    #[should_panic(expected = "linked token transfer must use linked target")]
    fn linked_nft_transfer_refused_with_missing_or_wrong_linked_target() {
        let mut chain = Chain::default();
        chain.configure_lns();
        chain.mint_linked(TOKEN_A, ALICE, top_level_link(), 5_000_000);
        chain.run_instruction(
            ALICE,
            true,
            [],
            Instruction::Transfer {
                token_id: TOKEN_A,
                new_owner: linked_owner_tag(&SLOT_A, &BOB),
                sealed: None,
                now: Some(1_000),
                linked_target: None,
            },
        );
    }

    #[test]
    #[should_panic(expected = "signer does not own this token")]
    fn linked_nft_transfer_refused_for_non_owner() {
        let mut chain = Chain::default();
        chain.configure_lns();
        chain.mint_linked(TOKEN_A, ALICE, top_level_link(), 5_000_000);
        chain.run_instruction(
            BOB,
            true,
            [],
            Instruction::Transfer {
                token_id: TOKEN_A,
                new_owner: linked_owner_tag(&SLOT_A, &BOB),
                sealed: None,
                now: Some(1_000),
                linked_target: Some(SLOT_A),
            },
        );
    }

    #[test]
    #[should_panic(expected = "linked token can only be burned by its linked authority")]
    fn direct_burn_on_linked_token_is_refused() {
        let mut chain = Chain::default();
        chain.configure_lns();
        chain.mint_linked(TOKEN_A, ALICE, top_level_link(), 5_000_000);
        chain.run_instruction(ALICE, true, [], Instruction::Burn { token_id: TOKEN_A });
    }

    #[test]
    fn authority_set_expiry_unfreezes_after_renewal() {
        let mut chain = Chain::default();
        chain.configure_lns();
        chain.mint_linked(TOKEN_A, ALICE, top_level_link(), 1_000);
        chain.run_authority(
            Actor::new(AUTHORITY_PDA, REGISTRY),
            &TOKEN_A,
            AuthorityOp::SetExpiry {
                token_id: TOKEN_A,
                expires: 9_000_000,
            },
        );
        assert_eq!(chain.record(&TOKEN_A).unwrap().expires, 9_000_000);
        chain.run_instruction(
            ALICE,
            true,
            [],
            Instruction::Transfer {
                token_id: TOKEN_A,
                new_owner: linked_owner_tag(&SLOT_A, &BOB),
                sealed: None,
                now: Some(2_000),
                linked_target: Some(SLOT_A),
            },
        );
        assert_eq!(chain.record(&TOKEN_A).unwrap().owner, linked_owner_tag(&SLOT_A, &BOB));
    }

    #[test]
    #[should_panic(expected = "token is frozen")]
    fn authority_set_frozen_blocks_direct_transfer() {
        let mut chain = Chain::default();
        chain.configure_lns();
        chain.mint_linked(TOKEN_A, ALICE, top_level_link(), 9_000_000);
        chain.run_authority(
            Actor::new(AUTHORITY_PDA, REGISTRY),
            &TOKEN_A,
            AuthorityOp::SetFrozen {
                token_id: TOKEN_A,
                frozen: true,
            },
        );
        chain.run_instruction(
            ALICE,
            true,
            [],
            Instruction::Transfer {
                token_id: TOKEN_A,
                new_owner: linked_owner_tag(&SLOT_A, &BOB),
                sealed: None,
                now: Some(2_000),
                linked_target: Some(SLOT_A),
            },
        );
    }

    #[test]
    fn authority_unfreeze_restores_transfer() {
        let mut chain = Chain::default();
        chain.configure_lns();
        chain.mint_linked(TOKEN_A, ALICE, top_level_link(), 9_000_000);
        let auth = Actor::new(AUTHORITY_PDA, REGISTRY);
        chain.run_authority(
            auth,
            &TOKEN_A,
            AuthorityOp::SetFrozen {
                token_id: TOKEN_A,
                frozen: true,
            },
        );
        chain.run_authority(
            auth,
            &TOKEN_A,
            AuthorityOp::SetFrozen {
                token_id: TOKEN_A,
                frozen: false,
            },
        );
        chain.run_instruction(
            ALICE,
            true,
            [],
            Instruction::Transfer {
                token_id: TOKEN_A,
                new_owner: linked_owner_tag(&SLOT_A, &BOB),
                sealed: None,
                now: Some(2_000),
                linked_target: Some(SLOT_A),
            },
        );
        assert_eq!(chain.record(&TOKEN_A).unwrap().owner, linked_owner_tag(&SLOT_A, &BOB));
    }

    #[test]
    fn authority_burn_succeeds_even_when_frozen_and_shielded_and_allows_remint() {
        let mut chain = Chain::default();
        chain.configure_lns();
        chain.mint_linked(TOKEN_A, ALICE, top_level_link(), 1_000);
        let deed_pda = chain.register_private_deed(&TOKEN_A, 1, 2);
        let auth = Actor::new(AUTHORITY_PDA, REGISTRY);
        chain.run_authority(
            auth,
            &TOKEN_A,
            AuthorityOp::Shield {
                token_id: TOKEN_A,
                deed_account: deed_pda,
                expires: 1_000,
            },
        );
        chain.run_authority(
            auth,
            &TOKEN_A,
            AuthorityOp::SetFrozen {
                token_id: TOKEN_A,
                frozen: true,
            },
        );
        // Lapsed re-registration burns old T1 and mints fresh T2.
        chain.run_authority(auth, &TOKEN_A, AuthorityOp::Burn { token_id: TOKEN_A });
        assert_eq!(chain.record(&TOKEN_A), None);

        chain.mint_linked(TOKEN_B, BOB, top_level_link(), 9_000_000);
        assert_eq!(chain.record(&TOKEN_B).unwrap().owner, linked_owner_tag(&SLOT_A, &BOB));
    }

    // ---- LP-0001 PrivateDeed Shield / Unshield / Refresh / ReadState / RequireHolder ----

    #[test]
    fn unlinked_token_shield_read_state_and_unshield_round_trip() {
        let mut chain = Chain::default();
        chain.create(0, 10);
        chain.buy(ALICE, 1, 0);
        let tid = serial_token_id(&COLL, 1);
        let deed_pda = chain.register_private_deed(&tid, 3, 4);

        chain.run_instruction(
            ALICE,
            true,
            [],
            Instruction::Shield {
                token_id: tid,
                deed_account: deed_pda,
            },
        );
        assert!(chain.record(&tid).unwrap().shielded);
        assert_eq!(
            chain.deed(deed_pda),
            Some(PrivateDeed {
                version: VERSION,
                collection_id: COLL,
                token_id: tid,
                expires: u64::MAX,
            })
        );

        // Gate reads PrivateDeed via Message::ReadState, lending its own grant.
        let gate = Actor::new(ALICE, READER_PROGRAM);
        let deed_actor = Actor::new(deed_pda, PROGRAM);
        chain.execute(Some(gate), deed_actor, BTreeSet::from([gate]), &Message::ReadState);
        assert_eq!(chain.state_replies.len(), 1);
        let (reply_from, reply_to, returned_grants, reply) = &chain.state_replies[0];
        assert_eq!(*reply_from, deed_actor);
        assert_eq!(*reply_to, gate);
        assert!(returned_grants.contains(&gate));
        let decoded: PrivateDeed = borsh::from_slice(&reply.state).unwrap();
        assert_eq!(decoded.collection_id, COLL);
        assert_eq!(decoded.token_id, tid);

        // Unshield clears deed_pda and re-enables transfer.
        chain.run_instruction(
            ALICE,
            true,
            [],
            Instruction::Unshield {
                token_id: tid,
                deed_account: deed_pda,
            },
        );
        assert!(!chain.record(&tid).unwrap().shielded);
        assert_eq!(chain.deed(deed_pda), None);
    }

    #[test]
    #[should_panic(expected = "token is shielded; unshield before transfer")]
    fn shielded_unlinked_token_cannot_transfer() {
        let mut chain = Chain::default();
        chain.mint(TOKEN_A, ALICE, b"{}");
        let deed_pda = chain.register_private_deed(&TOKEN_A, 3, 4);
        chain.run_instruction(
            ALICE,
            true,
            [],
            Instruction::Shield {
                token_id: TOKEN_A,
                deed_account: deed_pda,
            },
        );
        chain.run_instruction(
            ALICE,
            true,
            [],
            Instruction::Transfer {
                token_id: TOKEN_A,
                new_owner: owner_tag(&TOKEN_A, &BOB),
                sealed: None,
                now: None,
                linked_target: None,
            },
        );
    }

    #[test]
    #[should_panic(expected = "token is shielded; unshield before burn")]
    fn shielded_unlinked_token_cannot_burn() {
        let mut chain = Chain::default();
        chain.mint(TOKEN_A, ALICE, b"{}");
        let deed_pda = chain.register_private_deed(&TOKEN_A, 3, 4);
        chain.run_instruction(
            ALICE,
            true,
            [],
            Instruction::Shield {
                token_id: TOKEN_A,
                deed_account: deed_pda,
            },
        );
        chain.run_instruction(ALICE, true, [], Instruction::Burn { token_id: TOKEN_A });
    }

    #[test]
    fn authority_shield_set_expiry_refresh_and_unshield_linked_token() {
        let mut chain = Chain::default();
        chain.configure_lns();
        chain.mint_linked(TOKEN_A, ALICE, top_level_link(), 1_000_000);
        let deed_pda = chain.register_private_deed(&TOKEN_A, 5, 6);
        let auth = Actor::new(AUTHORITY_PDA, REGISTRY);

        chain.run_authority(
            auth,
            &TOKEN_A,
            AuthorityOp::Shield {
                token_id: TOKEN_A,
                deed_account: deed_pda,
                expires: 1_000_000,
            },
        );
        assert!(chain.record(&TOKEN_A).unwrap().shielded);
        assert_eq!(chain.deed(deed_pda).unwrap().expires, 1_000_000);
        assert_eq!(chain.deed(deed_pda).unwrap().collection_id, lns_coll());

        // Public renewal while shielded updates NftRecord.expires.
        chain.run_authority(
            auth,
            &TOKEN_A,
            AuthorityOp::SetExpiry {
                token_id: TOKEN_A,
                expires: 9_000_000,
            },
        );
        assert_eq!(chain.record(&TOKEN_A).unwrap().expires, 9_000_000);
        assert_eq!(chain.deed(deed_pda).unwrap().expires, 1_000_000);

        assert_eq!(chain.record(&TOKEN_A).unwrap().deed_account, Some(deed_pda));

        // Private RefreshShielded bumps PrivateDeed.expires at the stored deed account.
        chain.run_authority(
            auth,
            &TOKEN_A,
            AuthorityOp::RefreshShielded {
                token_id: TOKEN_A,
                expires: 9_000_000,
            },
        );
        assert_eq!(chain.deed(deed_pda).unwrap().expires, 9_000_000);

        // Unshield clears deed_pda and unlocks transfer.
        chain.run_authority(
            auth,
            &TOKEN_A,
            AuthorityOp::Unshield { token_id: TOKEN_A },
        );
        assert!(!chain.record(&TOKEN_A).unwrap().shielded);
        assert_eq!(chain.record(&TOKEN_A).unwrap().deed_account, None);
        assert_eq!(chain.deed(deed_pda), None);
    }

    #[test]
    #[should_panic(expected = "linked token must be shielded through its linked authority")]
    fn direct_shield_on_linked_token_is_refused() {
        let mut chain = Chain::default();
        chain.configure_lns();
        chain.mint_linked(TOKEN_A, ALICE, top_level_link(), 5_000_000);
        let deed_pda = chain.register_private_deed(&TOKEN_A, 1, 2);
        chain.run_instruction(
            ALICE,
            true,
            [],
            Instruction::Shield {
                token_id: TOKEN_A,
                deed_account: deed_pda,
            },
        );
    }

    #[test]
    #[should_panic(expected = "token is shielded; unshield before transfer")]
    fn authority_transfer_refused_while_shielded() {
        let mut chain = Chain::default();
        chain.configure_lns();
        chain.mint_linked(TOKEN_A, ALICE, top_level_link(), 5_000_000);
        let deed_pda = chain.register_private_deed(&TOKEN_A, 1, 2);
        let auth = Actor::new(AUTHORITY_PDA, REGISTRY);
        chain.run_authority(
            auth,
            &TOKEN_A,
            AuthorityOp::Shield {
                token_id: TOKEN_A,
                deed_account: deed_pda,
                expires: 5_000_000,
            },
        );
        chain.run_authority(
            auth,
            &TOKEN_A,
            AuthorityOp::Transfer {
                token_id: TOKEN_A,
                new_owner: linked_owner_tag(&SLOT_A, &BOB),
            },
        );
    }

    #[test]
    #[should_panic(expected = "deed must be a private PDA")]
    fn shield_refuses_public_pda_as_deed_account() {
        let mut chain = Chain::default();
        chain.mint(TOKEN_A, ALICE, b"{}");
        let public_deed = AccountId::for_public_pda(&PROGRAM, &deed_seed(&TOKEN_A));
        chain.run_instruction(
            ALICE,
            true,
            [],
            Instruction::Shield {
                token_id: TOKEN_A,
                deed_account: public_deed,
            },
        );
    }

    #[test]
    #[should_panic(expected = "ReadState requires a sender to reply to")]
    fn read_state_from_root_is_refused() {
        let mut chain = Chain::default();
        let to = Actor::new(token_address(&PROGRAM, &TOKEN_A), PROGRAM);
        chain.execute(None, to, BTreeSet::new(), &Message::ReadState);
    }

    #[test]
    fn require_holder_checks_owner_and_expiry_window() {
        let mut chain = Chain::default();
        chain.configure_lns();
        chain.mint_linked(TOKEN_A, ALICE, top_level_link(), 5_000_000);
        let to = Actor::new(token_address(&PROGRAM, &TOKEN_A), PROGRAM);
        let now = 1_000_000;
        chain.execute(
            None,
            to,
            BTreeSet::new(),
            &Message::RequireHolder {
                tag: linked_owner_tag(&SLOT_A, &ALICE),
                now: Some(now),
            },
        );
        assert_eq!(chain.last_validity.timestamps.start(), Some(now));
        assert_eq!(
            chain.last_validity.timestamps.end(),
            Some(now + TIME_WINDOW_MS)
        );
    }

    #[test]
    #[should_panic(expected = "token has expired")]
    fn require_holder_rejects_expired_token() {
        let mut chain = Chain::default();
        chain.configure_lns();
        chain.mint_linked(TOKEN_A, ALICE, top_level_link(), 1_000);
        let to = Actor::new(token_address(&PROGRAM, &TOKEN_A), PROGRAM);
        chain.execute(
            None,
            to,
            BTreeSet::new(),
            &Message::RequireHolder {
                tag: linked_owner_tag(&SLOT_A, &ALICE),
                now: Some(1_000),
            },
        );
    }

    // ---- Exact Borsh encoding vectors ----

    fn test_link_bytes() -> (LinkedAuthority, Vec<u8>) {
        let linked = LinkedAuthority {
            program: AccountId::new([0x09; 32]),
            authority_actor: AccountId::new([0x40; 32]),
            target: [0x11; 32],
            record_actor: AccountId::new([0x41; 32]),
        };
        let bytes: Vec<u8> = [&[0x09; 32][..], &[0x40; 32], &[0x11; 32], &[0x41; 32]].concat();
        (linked, bytes)
    }

    #[test]
    fn borsh_vector_linked_authority() {
        let (linked, expected) = test_link_bytes();
        assert_eq!(expected.len(), 128);
        assert_eq!(to_bytes(&linked), expected);
        assert_eq!(borsh::from_slice::<LinkedAuthority>(&expected).unwrap(), linked);
    }

    #[test]
    fn borsh_vector_nft_record_and_private_deed() {
        let (linked, _) = test_link_bytes();
        let expires: u64 = 0x0102_0304_0506_0708;
        let rec = NftRecord {
            version: 3,
            token_id: [0xAA; 32],
            owner: [0xBB; 32],
            collection: Some(([0xCC; 32], 0)),
            content: b"{}".to_vec(),
            sealed: vec![],
            linked: Some(linked.clone()),
            shielded: false,
            expires,
            frozen: false,
            deed_account: None,
        };
        let expected_rec: Vec<u8> = [
            &[0x03][..],                        // version = 3
            &[0xAA; 32],                        // token_id
            &[0xBB; 32],                        // owner
            &[0x01],                            // collection = Some
            &[0xCC; 32],                        // collection_id
            &[0x00, 0x00, 0x00, 0x00],          // serial = 0
            &[0x02, 0x00, 0x00, 0x00],          // content len = 2
            b"{}",                              // content
            &[0x00, 0x00, 0x00, 0x00],          // sealed len = 0
            &[0x01],                            // linked = Some
            &to_bytes(&linked),                 // LinkedAuthority (128 bytes)
            &[0x00],                            // shielded = false
            &[0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01], // expires LE
            &[0x00],                            // frozen = false
            &[0x00],                            // deed_account = None
        ]
        .concat();
        assert_eq!(to_bytes(&rec), expected_rec);
        assert_eq!(borsh::from_slice::<NftRecord>(&expected_rec).unwrap(), rec);

        let deed = PrivateDeed {
            version: 3,
            collection_id: [0xCC; 32],
            token_id: [0xAA; 32],
            expires,
        };
        let expected_deed: Vec<u8> = [
            &[0x03][..],
            &[0xCC; 32],
            &[0xAA; 32],
            &[0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01],
        ]
        .concat();
        assert_eq!(expected_deed.len(), 73);
        assert_eq!(to_bytes(&deed), expected_deed);
        assert_eq!(
            borsh::from_slice::<PrivateDeed>(&expected_deed).unwrap(),
            deed
        );
    }

    #[test]
    fn borsh_vector_registry_transfer_hook() {
        let hook = RegistryTransferHook {
            id: [0x11; 32],
            token_id: [0xAA; 32],
            expected_old_owner: [0x10; 32],
            new_owner: [0x20; 32],
            now: 1_000,
        };
        let expected: Vec<u8> = [
            &[0x03][..],                                       // names_core::Message::NftTransferHook (variant 3)
            &[0x11; 32],                                       // id (slot_id)
            &[0xAA; 32],                                       // token_id
            &[0x10; 32],                                       // expected_old_owner
            &[0x20; 32],                                       // new_owner
            &[0xE8, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00], // now = 1000 LE
        ]
        .concat();
        assert_eq!(expected.len(), 137);
        assert_eq!(to_bytes(&hook), expected);
        assert_eq!(
            borsh::from_slice::<RegistryTransferHook>(&expected).unwrap(),
            hook
        );
    }

    #[test]
    fn borsh_vector_authority_sync_and_read_messages() {
        // Message::AuthoritySync(AuthorityOp::Mint) => 0x02 || 0x00 || ...
        let (link, _) = test_link_bytes();
        let mint = Message::AuthoritySync(AuthorityOp::Mint {
            token_id: [0xAA; 32],
            owner: [0xBB; 32],
            linked: link.clone(),
            content: b"{}".to_vec(),
            expires: 1_000,
        });
        let expected_mint: Vec<u8> = [
            &[0x02, 0x00][..],                                 // Message::AuthoritySync + AuthorityOp::Mint
            &[0xAA; 32],                                       // token_id
            &[0xBB; 32],                                       // owner
            &to_bytes(&link),                                  // LinkedAuthority (128 bytes)
            &[0x02, 0x00, 0x00, 0x00],                         // content len = 2
            b"{}",                                             // content
            &[0xE8, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00], // expires = 1000 LE
        ]
        .concat();
        assert_eq!(to_bytes(&mint), expected_mint);
        assert_eq!(borsh::from_slice::<Message>(&expected_mint).unwrap(), mint);

        // Message::AuthoritySync(AuthorityOp::Transfer) => 0x02 || 0x01 || token_id || new_owner
        let msg = Message::AuthoritySync(AuthorityOp::Transfer {
            token_id: [0xAA; 32],
            new_owner: [0xBB; 32],
        });
        let expected: Vec<u8> = [&[0x02, 0x01][..], &[0xAA; 32], &[0xBB; 32]].concat();
        assert_eq!(to_bytes(&msg), expected);
        assert_eq!(borsh::from_slice::<Message>(&expected).unwrap(), msg);

        // Message::AuthoritySync(AuthorityOp::Burn) => 0x02 || 0x02 || token_id
        let burn = Message::AuthoritySync(AuthorityOp::Burn { token_id: [0xAA; 32] });
        let expected_burn: Vec<u8> = [&[0x02, 0x02][..], &[0xAA; 32]].concat();
        assert_eq!(to_bytes(&burn), expected_burn);

        // Message::AuthoritySync(AuthorityOp::SetExpiry) => 0x02 || 0x03 || token_id || expires LE
        let set_exp = Message::AuthoritySync(AuthorityOp::SetExpiry {
            token_id: [0xAA; 32],
            expires: 1_000,
        });
        let expected_exp: Vec<u8> = [
            &[0x02, 0x03][..],
            &[0xAA; 32],
            &[0xE8, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        ]
        .concat();
        assert_eq!(to_bytes(&set_exp), expected_exp);

        // Message::AuthoritySync(AuthorityOp::Shield) => 0x02 || 0x04 || token_id || deed_account || expires LE
        let shield = Message::AuthoritySync(AuthorityOp::Shield {
            token_id: [0xAA; 32],
            deed_account: AccountId::new([0xDD; 32]),
            expires: 1_000,
        });
        let expected_shield: Vec<u8> = [
            &[0x02, 0x04][..],
            &[0xAA; 32],
            &[0xDD; 32],
            &[0xE8, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        ]
        .concat();
        assert_eq!(to_bytes(&shield), expected_shield);

        // Message::AuthoritySync(AuthorityOp::Unshield) => 0x02 || 0x05 || token_id
        let unshield = Message::AuthoritySync(AuthorityOp::Unshield {
            token_id: [0xAA; 32],
        });
        let expected_unshield: Vec<u8> = [&[0x02, 0x05][..], &[0xAA; 32]].concat();
        assert_eq!(to_bytes(&unshield), expected_unshield);

        // Message::AuthoritySync(AuthorityOp::RefreshShielded) => 0x02 || 0x06 || token_id || expires LE
        let refresh = Message::AuthoritySync(AuthorityOp::RefreshShielded {
            token_id: [0xAA; 32],
            expires: 1_000,
        });
        let expected_refresh: Vec<u8> = [
            &[0x02, 0x06][..],
            &[0xAA; 32],
            &[0xE8, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        ]
        .concat();
        assert_eq!(to_bytes(&refresh), expected_refresh);

        // Message::AuthoritySync(AuthorityOp::SetFrozen) => 0x02 || 0x07 || token_id || frozen
        let freeze = Message::AuthoritySync(AuthorityOp::SetFrozen {
            token_id: [0xAA; 32],
            frozen: true,
        });
        let expected_freeze: Vec<u8> = [&[0x02, 0x07][..], &[0xAA; 32], &[0x01]].concat();
        assert_eq!(to_bytes(&freeze), expected_freeze);

        // Message::ReadState => 0x03
        assert_eq!(to_bytes(&Message::ReadState), vec![0x03]);

        // Message::RequireHolder => 0x04 || tag || Some(1000)
        let req = Message::RequireHolder {
            tag: [0xBB; 32],
            now: Some(1_000),
        };
        let expected_req: Vec<u8> = [
            &[0x04][..],
            &[0xBB; 32],
            &[0x01],
            &[0xE8, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        ]
        .concat();
        assert_eq!(to_bytes(&req), expected_req);
    }

    #[test]
    fn borsh_vector_instruction_transfer_linked() {
        let msg = Message::Instruction(Instruction::Transfer {
            token_id: [0xAA; 32],
            new_owner: [0xBB; 32],
            sealed: None,
            now: Some(1_000),
            linked_target: Some([0x11; 32]),
        });
        let expected: Vec<u8> = [
            &[0x00, 0x03][..],                                 // Message::Instruction + Instruction::Transfer
            &[0xAA; 32],                                       // token_id
            &[0xBB; 32],                                       // new_owner
            &[0x00],                                           // sealed = None
            &[0x01],                                           // now = Some
            &[0xE8, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00], // 1000 LE
            &[0x01],                                           // linked_target = Some
            &[0x11; 32],                                       // slot_id
        ]
        .concat();
        assert_eq!(to_bytes(&msg), expected);
        assert_eq!(borsh::from_slice::<Message>(&expected).unwrap(), msg);
    }
}
