//! Logos Names: a private name registry for the Logos Execution Zone (LEZ v0.4).
//!
//! Version 8 unifies every finalized 0.4 testnet feature into one registry:
//!
//! - **Privacy-by-default names:** Clients derive a 32-byte identifier and
//!   encryption key via Argon2id; records and pages are sealed with AES-256-GCM;
//!   holders are stored as unlinkable [`OwnerTag`]s; private holders act inside
//!   LEZ's privacy circuit without ever revealing their account.
//! - **Commit-reveal with 5m public / 15m private waits:** [`Instruction::CommitPublic`]
//!   (1-minute window, 5-minute wait) and [`Instruction::Commit`] (10-minute window,
//!   15-minute wait).
//! - **Admin-adjustable LEZ length-tier × term pricing:** 3 length tiers (5 chars
//!   and admin-placed 1–4; 6–7 chars; 8+ chars) × 4 terms (30d, 1y, 5y, permanent
//!   at 20× a year), bound into `slot_id = tiered_id(base_id, price_tier)`. Locked
//!   1–4 char and reserved names remain admin-only via [`SHORTLIST_ROOT`].
//! - **21-day falling re-release premium:** After a lapsed term name's 30-day
//!   grace period ends, re-registration charges an admin-adjustable premium
//!   halving daily over 21 days to 0, paid 100% to `settings.treasury`.
//! - **One-level subnames:** `label.parent` (total ≤ 62 bytes, `--` banned),
//!   `Revocable` (bound to parent `generation`) or `Permanent`, with parent-run
//!   primary sales (`SetSubnameSale` / `BuySubname`).
//! - **Top-level names as tied 1-of-1 NFTs (`lez_nft` v3 lockstep):** Every
//!   top-level name has a tied 1-of-1 NFT in `settings.nft_program` (bound once
//!   at `Configure`, immutable thereafter). Registry is the source of truth;
//!   plain transfers work in both directions (Registry → NFT and NFT → Registry
//!   via [`Message::NftTransferHook`]) with 0 protocol fee. Opt-in LP-0001
//!   shielded deeds (`ShieldDeed` / `UnshieldDeed` / `RefreshShieldedDeed`)
//!   enable hidden membership proofs.
//! - **Atomic on-chain resale:** `List` / `Cancel` / `Buy` for top-level names
//!   with a fixed 2.5% (`1 / 40`) treasury fee, public `pay_to` account, optional
//!   plaintext `display_name` in 16 `MarketBucket` PDAs, and automatic listing
//!   death on transfer, expiry, grace renewal, release, claim, or re-registration.
//! - **Permissionless on-chain sites directory (`SiteBucket`):** Option A from
//!   `notes/DISCOVERY.md` — 16 paginated buckets × 32 entries/page + `SiteDirHeader`,
//!   top-level names only for now, updated atomically in `SetRecord` / `SetPage`
//!   (`SiteDirOp::Keep` = 0 extra gas on routine edits or `discover=off` opt-out),
//!   and cleaned up atomically on transfer, sale, release, claim, or re-registration.

pub mod discovery;
pub mod market;
pub mod nft;
pub mod shortlist;
#[cfg(test)]
mod tests;

pub use discovery::{
    MAX_SITE_BUCKET_ENTRIES, MAX_SITE_NAME_BYTES, MAX_SITE_TITLE_BYTES, SITE_BUCKETS, SiteBucket,
    SiteDirHeader, SiteDirOp, SiteEntry, site_bucket_address, site_bucket_seed,
    site_dir_header_address, site_dir_header_seed, validate_site_title,
};
pub use market::{
    DEFAULT_PREMIUM_START, Listing, MARKET_BUCKETS, MAX_BUCKET_ENTRIES, MIN_SALE_PRICE,
    MarketBucket, MarketEntry, PREMIUM_DAYS, PREMIUM_DURATION_MS, SALE_FEE_BPS, SALE_FEE_DIVISOR,
    bucket_of, market_bucket_address, market_bucket_seed, rerelease_premium, sale_split,
};
pub use nft::{
    DEFAULT_DEED_CONTENT, LinkedAuthority, NFT_MESSAGE_TAG_AUTHORITY_SYNC, NftAuthorityOp,
    NftAuthoritySyncMessage, nft_deed_seed, nft_token_address, nft_token_id, nft_token_seed,
};
pub use shortlist::{LengthProof, TIER_LOCKED, TIER_NORMAL, TIER_THREE};

use borsh::{BorshDeserialize, BorshSerialize};
use lee_core::{
    account::{AccountId, Actor},
    native_token,
    program::{PdaSeed, ProgramEvent, ReceiveInput, Response, SyncMessage},
};

/// Record layout version written by this program.
///
/// - v3–v5: `OwnerTag` ownership, commit-reveal, shortlist, deposit mode.
/// - v6: `claimed` flag and LEZ 0.4 actor model.
/// - v7: length-tier × term pricing (including permanent), `generation`,
///   `subname_sale`, and one-level subnames (`SubnameRecord`).
/// - v8: tied top-level name NFTs (`lez_nft` v3 lockstep), atomic `List`/`Cancel`/`Buy`
///   with fixed 2.5% treasury fee, 21-day falling re-release premium, opt-in
///   LP-0001 shielded deeds, and the paginated `SiteBucket` sites directory.
pub const VERSION: u8 = 8;

/// Maximum byte length of a single name label (top-level name, or either side
/// of a subname `label.parent`).
pub const MAX_NAME_LEN: usize = 32;
/// Maximum byte length of a full subname `label.parent`. Chosen as 62 so that
/// the gateway's single-label encoding `label--parent` is at most 63 bytes,
/// the RFC 1035 DNS label limit.
pub const MAX_SUBNAME_LEN: usize = 62;
/// How names are written for people: bare, `jef` (SPEC 2).
pub const NAME_SUFFIX: &str = "";
/// Legacy suffix still accepted and stripped on input.
pub const LEGACY_SUFFIX: &str = ".lez";
/// Room for a sealed profile with private receiving keys and sixteen entries.
pub const MAX_SEALED_RECORD_LEN: usize = 12 * 1024;
/// LEZ caps actor data at 100 KiB; 80 KiB fits comfortably within one tx gas budget.
pub const MAX_SEALED_PAGE_LEN: usize = 80 * 1024;

const NAME_SEED_DOMAIN: &[u8] = b"logos-names/v5/name/";
const PAGE_SEED_DOMAIN: &[u8] = b"logos-names/v5/page/";
const SETTINGS_SEED_DOMAIN: &[u8] = b"logos-names/v5/config";
const OWNER_TAG_DOMAIN: &[u8] = b"logos-names/v5/owner";
const COMMITMENT_DOMAIN: &[u8] = b"logos-names/v5/commitment";
const COMMITMENT_SEED_DOMAIN: &[u8] = b"logos-names/v5/commit/";
const DEPOSIT_SEED_DOMAIN: &[u8] = b"logos-names/v5/deposit/";
const CLAIM_LEAF_DOMAIN: &[u8] = b"logos-names/v6/claim-leaf";
const CLAIM_NODE_DOMAIN: &[u8] = b"logos-names/v6/claim-node";
const TIER_ID_DOMAIN: &[u8] = b"logos-names/v7/tier/";
const SUBNAME_SEED_DOMAIN: &[u8] = b"logos-names/v7/subname/";
const SUBPAGE_SEED_DOMAIN: &[u8] = b"logos-names/v7/subpage/";
const SUBNAME_COMMITMENT_DOMAIN: &[u8] = b"logos-names/v7/sub-commitment";
const GEN_REGISTER_DOMAIN: &[u8] = b"logos-names/v7/gen/register";
const GEN_CLAIM_DOMAIN: &[u8] = b"logos-names/v7/gen/claim";
const GEN_TRANSFER_DOMAIN: &[u8] = b"logos-names/v7/gen/transfer";

/// The registry charges a fee per term, paid to its treasury; names lapse
/// (unless registered or renewed with [`TERM_PERMANENT`]).
pub const MODE_FEE: u8 = 0;
/// The registry takes a deposit, held in the name's deposit account until the
/// name is released; names never lapse.
pub const MODE_DEPOSIT: u8 = 1;

/// Timestamp validity window for private commits/reveals, claims, and standard
/// time-bounded instructions (`[now, now + TIME_WINDOW_MS)` = 10 minutes).
pub const TIME_WINDOW_MS: u64 = 10 * 60 * 1000;
/// Minimum age of a private commitment ([`Instruction::Commit`]) before reveal (15 minutes).
pub const MIN_COMMIT_AGE_MS: u64 = 15 * 60 * 1000;
/// Timestamp validity window for public commits ([`Instruction::CommitPublic`])
/// and their reveals (`[now, now + PUBLIC_TIME_WINDOW_MS)` = 1 minute).
pub const PUBLIC_TIME_WINDOW_MS: u64 = 60 * 1000;
/// Minimum age of a public commitment ([`Instruction::CommitPublic`]) before reveal (5 minutes).
pub const PUBLIC_MIN_COMMIT_AGE_MS: u64 = 5 * 60 * 1000;
/// Maximum age of any commitment before it expires (24 hours).
pub const MAX_COMMIT_AGE_MS: u64 = 24 * 60 * 60 * 1000;

pub(crate) const DAY_MS: u64 = 24 * 60 * 60 * 1000;
/// Finite term durations in milliseconds: 30 days, 1 year, 5 years.
/// Term index 3 is [`TERM_PERMANENT`] (`expires = u64::MAX`).
pub const TERM_LENGTHS_MS: [u64; 3] = [30 * DAY_MS, 365 * DAY_MS, 5 * 365 * DAY_MS];
/// Number of pricing terms: 30 days, 1 year, 5 years, permanent.
pub const TERMS: usize = 4;
pub const TERM_30D: u8 = 0;
pub const TERM_1Y: u8 = 1;
pub const TERM_5Y: u8 = 2;
pub const TERM_PERMANENT: u8 = 3;
/// Number of length-based pricing tiers: 5 chars (and admin-placed 1–4), 6–7 chars, 8+ chars.
pub const PRICE_TIERS: usize = 3;
pub const PRICE_TIER_5: u8 = 0;
pub const PRICE_TIER_6_7: u8 = 1;
pub const PRICE_TIER_8_PLUS: u8 = 2;
/// Starting placeholder price table `[price_tier][term]` in LEZ native base units:
/// 1y = 20M / 5M / 1M (5 / 6–7 / 8+ chars); 30d = 1/10×, 5y = 4×, permanent = 20×.
/// Admin-adjustable on chain via [`Instruction::Configure`] or [`Instruction::SetPrice`].
pub const DEFAULT_PRICES: [[u128; TERMS]; PRICE_TIERS] = [
    [2_000_000, 20_000_000, 80_000_000, 400_000_000],
    [500_000, 5_000_000, 20_000_000, 100_000_000],
    [100_000, 1_000_000, 4_000_000, 20_000_000],
];
/// Grace period after expiry (30 days) during which only renewal or release is allowed.
pub const GRACE_MS: u64 = 30 * DAY_MS;

/// What the chain knows a name by: 32 bytes derived from the name.
pub type NameId = [u8; 32];

/// Unlinkable owner tag binding a holder's `AccountId` to one name (`slot_id` or `sub_id`).
pub type OwnerTag = [u8; 32];

/// Subname primary sales settings stored on a parent name's record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SubnameSale {
    /// Price in native token base units to buy any free subname of this parent.
    /// `0` allows anyone to claim free permanent subnames (gas only).
    pub price: u128,
    /// Public account receiving the native-token payment in the same transaction.
    /// Ignored when `price == 0`.
    pub payout: AccountId,
}

/// A registered top-level name (v8), stored at [`record_address`].
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NameRecord {
    pub version: u8,
    /// The tag of the account that may change, transfer, list, or release the name.
    pub owner: OwnerTag,
    /// Expiry in milliseconds since Unix epoch (`u64::MAX` for permanent/claimed/deposit).
    pub expires: u64,
    pub sealed: Vec<u8>,
    /// Carried over from an earlier chain by [`Instruction::Claim`].
    pub claimed: bool,
    /// Length-based pricing tier (`0`, `1`, or `2`), bound into `slot_id` via [`tiered_id`].
    pub price_tier: u8,
    /// Unique hold generation token. Advances on `Register`, `Claim`, `Transfer`,
    /// `Buy`, and `NftTransferHook`; revocable subnames bind to this value.
    pub generation: [u8; 32],
    /// Public subname sale settings for this parent; cleared on transfer/sale/release.
    pub subname_sale: Option<SubnameSale>,
    /// Tied 1-of-1 NFT token ID in `settings.nft_program`, derived on `Register`/`Claim`.
    pub nft_token_id: [u8; 32],
    /// True while custody of the tied NFT is shielded inside a private LP-0001 deed PDA.
    pub nft_shielded: bool,
    /// Active secondary resale listing; `None` when not for sale.
    pub listing: Option<Listing>,
    /// Strictly monotonic counter incremented on every `List` to defeat front-running.
    pub listing_nonce: u64,
    /// Sites directory page index if currently listed in `SiteBucket(bucket_of(&slot_id), page)`.
    pub site_listed: Option<u16>,
}

/// Whether a subname is permanent or revocable by its parent's holder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum SubnameKind {
    /// Independent of the parent once created or bought; never expires and cannot be revoked.
    Permanent,
    /// Controlled by the parent's holder and bound to `parent_generation`.
    Revocable { parent_generation: [u8; 32] },
}

/// A registered one-level subname (`label.parent`), stored at [`subname_record_address`].
/// Subnames have no tied NFTs and are not listed in `SiteBucket` in v8.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SubnameRecord {
    pub version: u8,
    pub owner: OwnerTag,
    pub kind: SubnameKind,
    pub sealed: Vec<u8>,
}

/// A commitment to register a top-level name or buy a subname.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Commitment {
    pub version: u8,
    pub made: u64,
    pub public: bool,
}

/// A name's sealed on-chain page.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Page {
    pub version: u8,
    pub sealed: Vec<u8>,
}

/// The opt-in reverse record on an account.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PrimaryName {
    pub version: u8,
    pub name: String,
}

/// The registry's settings (v8), stored at [`settings_address`].
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Settings {
    pub version: u8,
    /// The account that may update settings, prices, and re-release premium.
    pub admin: AccountId,
    /// The account that receives registration/renewal fees, re-release premiums,
    /// and the fixed 2.5% secondary-sale fee.
    pub treasury: AccountId,
    /// Admin-adjustable LEZ price table `[price_tier][term]` in native token base units.
    pub prices: [[u128; TERMS]; PRICE_TIERS],
    /// [`MODE_FEE`] or [`MODE_DEPOSIT`].
    pub mode: u8,
    /// Deposits for normal and three-character names when in [`MODE_DEPOSIT`].
    pub deposits: [u128; 2],
    /// Root of the legacy claims list; all zeros when claims are disabled.
    pub claims_root: [u8; 32],
    /// Claims cutoff timestamp in milliseconds since Unix epoch.
    pub claims_until: u64,
    /// Earlier-chain registry program ID for legacy claims.
    pub claims_program: AccountId,
    /// Day-0 re-release premium after a lapsed name's 30-day grace period ends
    /// (halving daily over 21 days to 0). `0` disables the premium.
    pub premium_start: u128,
    /// Our standalone `lez_nft` v3 program ID, bound once on initial `Configure`
    /// and immutable thereafter.
    pub nft_program: AccountId,
}

/// Top-level message accepted by this program.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Message {
    /// `0x00`: User/admin instruction sent to the signer's own registry actor.
    Instruction(Instruction),
    /// `0x01`: Internal self-effect sent between actors of this program.
    Effect(Effect),
    /// `0x02`: Read-only ownership check on a record actor, callable by any program.
    RequireHolder { tag: OwnerTag },
    /// `0x03`: Cross-program lockstep hook sent by `(nft_token_address, settings.nft_program)`
    /// to `(record_address(program, &id), program)` on a plain NFT-side transfer.
    NftTransferHook {
        id: NameId,
        token_id: [u8; 32],
        expected_old_owner: OwnerTag,
        new_owner: OwnerTag,
        now: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Instruction {
    /// `0x00`: Reveal a commitment and register a top-level name (fresh or past grace).
    /// In fee mode, `fee` covers `base_price + rerelease_premium` (if in the 21-day
    /// re-release window); mints a tied 1-of-1 NFT (and burns any lapsed predecessor's NFT).
    Register {
        id: NameId,
        sealed: Vec<u8>,
        term: u8,
        fee: u128,
        treasury: Option<AccountId>,
        now: u64,
        secret: [u8; 32],
        deposit: bool,
        length: LengthProof,
        price_tier: u8,
    },
    /// `0x01`: Extend a top-level name (`id` = `slot_id`) that is active or in grace.
    /// Clears any dead listing if renewing out of grace, refreshes site/listing expiry,
    /// and mirrors the new expiry to the tied NFT.
    Renew {
        id: NameId,
        term: u8,
        fee: u128,
        treasury: Option<AccountId>,
        now: u64,
    },
    /// `0x02`: Replace a top-level name's sealed profile, optionally updating its
    /// [`SiteBucket`] directory entry (`SiteDirOp::Keep` touches no directory actor).
    SetRecord {
        id: NameId,
        sealed: Vec<u8>,
        site_dir: SiteDirOp,
    },
    /// `0x03`: Write or clear a top-level name's sealed page, optionally updating
    /// its [`SiteBucket`] directory entry.
    SetPage {
        id: NameId,
        sealed: Vec<u8>,
        site_dir: SiteDirOp,
    },
    /// `0x04`: Plain transfer of an active, unshielded top-level name (0 protocol fee).
    /// Empties profile and page, clears subname sales, listing, and site directory entry,
    /// advances `generation`, and syncs ownership to the tied NFT.
    Transfer {
        id: NameId,
        new_owner: OwnerTag,
        now: u64,
    },
    /// `0x05`: Release an unshielded top-level name, emptying its record and page,
    /// removing any listing and site directory entry, burning its tied NFT, and
    /// refunding any deposit.
    Release { id: NameId, refund: u128 },
    /// `0x06`: Publish or clear the signer's reverse record (top-level or `label.parent`).
    SetPrimary { name: String },
    /// `0x07`: Create or update registry settings. Initial call must be signed by the
    /// program's own account and permanently binds `nft_program`; later calls require
    /// `settings.admin` and must pass the same `nft_program`.
    Configure {
        admin: AccountId,
        treasury: AccountId,
        prices: [[u128; TERMS]; PRICE_TIERS],
        mode: u8,
        deposits: [u128; 2],
        claims_root: [u8; 32],
        claims_until: u64,
        claims_program: AccountId,
        premium_start: u128,
        nft_program: AccountId,
    },
    /// `0x08`: Private commitment (10-minute clock window, 15-minute minimum wait).
    Commit { commitment: [u8; 32], now: u64 },
    /// `0x09`: Legacy snapshot claim into `tiered_id(&id, price_tier)` (permanent).
    /// Burns any overwritten unclaimed registration's NFT and clears its listing/site entry.
    Claim {
        id: NameId,
        sealed: Vec<u8>,
        now: u64,
        proof: ClaimProof,
        price_tier: u8,
    },
    /// `0x0a`: Public fast commitment (1-minute clock window, 5-minute minimum wait).
    CommitPublic { commitment: [u8; 32], now: u64 },
    /// `0x0b`: Update one cell of `settings.prices[price_tier][term]` (admin only).
    SetPrice {
        price_tier: u8,
        term: u8,
        price: u128,
    },
    /// `0x0c`: Enable or disable primary subname sales on a parent name.
    SetSubnameSale {
        id: NameId,
        sale: Option<SubnameSale>,
    },
    /// `0x0d`: Create a one-level subname (`label.parent`) by the parent's holder.
    CreateSubname {
        parent_id: NameId,
        sub_id: NameId,
        owner: OwnerTag,
        revocable: bool,
        sealed: Vec<u8>,
        now: u64,
    },
    /// `0x0e`: Revoke a revocable subname by the parent's holder.
    RevokeSubname {
        parent_id: NameId,
        sub_id: NameId,
        now: u64,
    },
    /// `0x0f`: Buy a permanent subname from a parent with subname sales enabled (commit-reveal).
    BuySubname {
        parent_id: NameId,
        sub_id: NameId,
        sealed: Vec<u8>,
        price: u128,
        payout: Option<AccountId>,
        now: u64,
        secret: [u8; 32],
    },
    /// `0x10`: Replace a subname's sealed profile.
    SetSubnameRecord {
        parent_id: NameId,
        sub_id: NameId,
        sealed: Vec<u8>,
        now: Option<u64>,
    },
    /// `0x11`: Write or clear a subname's sealed page.
    SetSubnamePage {
        parent_id: NameId,
        sub_id: NameId,
        sealed: Vec<u8>,
        now: Option<u64>,
    },
    /// `0x12`: Transfer a subname to a new owner.
    TransferSubname {
        parent_id: NameId,
        sub_id: NameId,
        new_owner: OwnerTag,
        now: Option<u64>,
    },
    /// `0x13`: Release a subname.
    ReleaseSubname {
        parent_id: NameId,
        sub_id: NameId,
        now: Option<u64>,
    },
    /// `0x14`: Update `settings.premium_start` (day-0 re-release premium; `0` disables). Admin only.
    SetPremium { premium_start: u128 },
    /// `0x15`: List an active, unshielded top-level name for atomic resale.
    /// `pay_to` must be an admitted public account. If `display_name` is `Some`,
    /// publishes the listing into `MarketBucket(bucket_of(&id))`; if `None`, it
    /// is an unlisted OTC listing touching no market bucket.
    List {
        id: NameId,
        price: u128,
        pay_to: AccountId,
        expires_at: Option<u64>,
        now: u64,
        display_name: Option<String>,
    },
    /// `0x16`: Cancel an active (or in-grace) secondary-sale listing on a top-level name.
    Cancel { id: NameId },
    /// `0x17`: Atomically buy a listed top-level name, splitting `price` into
    /// 97.5% to `pay_to` and 2.5% (`div_ceil(40)`) to `treasury`, transferring
    /// both `NameRecord` and the tied NFT in one transaction.
    Buy {
        id: NameId,
        price: u128,
        pay_to: AccountId,
        treasury: AccountId,
        expected_nonce: u64,
        now: u64,
    },
    /// `0x18`: Opt-in LP-0001 shielding of an active, unlisted top-level name's
    /// tied NFT into the private PDA `deed_account`. Refused while claims are open
    /// for unclaimed names. Blocks transfer/list/buy/release until unshielded.
    ShieldDeed {
        id: NameId,
        deed_account: AccountId,
        now: u64,
    },
    /// `0x19`: Unshield a top-level name's tied NFT from its private LP-0001 deed PDA.
    UnshieldDeed { id: NameId },
    /// `0x1a`: Refresh `PrivateDeed.expires` after renewing a shielded top-level name.
    RefreshShieldedDeed { id: NameId },
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Effect {
    /// On the record: register a fresh or past-grace name, compute re-release
    /// premium elapsed time, clean up any lapsed market/site entries, and forward
    /// price validation + NFT burn/mint to settings.
    Register {
        id: NameId,
        owner: OwnerTag,
        now: u64,
        expires: u64,
        sealed: Vec<u8>,
        price_tier: u8,
        generation: [u8; 32],
        token_id: [u8; 32],
        fee: u128,
        treasury: Option<AccountId>,
        term: u8,
        deposit: bool,
        shortlist_tier: u8,
    },
    /// On the record: extend expiry, clear dead listing if renewing out of grace,
    /// refresh active listing/site expiry, and forward `RequirePrice` + `SetExpiry`.
    Renew {
        id: NameId,
        payer: OwnerTag,
        treasury: Option<AccountId>,
        term: u8,
        fee: u128,
        now: u64,
    },
    /// On settings: verify shortlist lock, mode, treasury, base price + optional
    /// 21-day re-release premium.
    RequirePrice {
        id: NameId,
        payer: OwnerTag,
        treasury: Option<AccountId>,
        term: u8,
        fee: u128,
        deposit: bool,
        shortlist_tier: u8,
        price_tier: u8,
        premium_elapsed: Option<u64>,
    },
    /// On settings: write initial or updated settings (`nft_program` immutable once set).
    WriteSettings {
        signer: AccountId,
        by_program: bool,
        admin: AccountId,
        treasury: AccountId,
        prices: [[u128; TERMS]; PRICE_TIERS],
        mode: u8,
        deposits: [u128; 2],
        claims_root: [u8; 32],
        claims_until: u64,
        claims_program: AccountId,
        premium_start: u128,
        nft_program: AccountId,
    },
    /// On settings: update one cell of `prices` (admin only).
    SetPrice {
        signer: AccountId,
        price_tier: u8,
        term: u8,
        price: u128,
    },
    /// On settings: update `premium_start` (admin only).
    SetPremium {
        signer: AccountId,
        premium_start: u128,
    },
    /// On settings: verify `treasury == settings.treasury` (for `Buy`).
    RequireTreasury { treasury: AccountId },
    /// On settings: verify `from` is `(nft_token_address(settings.nft_program, token_id), settings.nft_program)`.
    RequireNftTokenSender { from: Actor, token_id: [u8; 32] },
    /// On settings: forward `NftAuthorityOp` to `settings.nft_program` (no-op if unconfigured).
    SyncNft { op: NftAuthorityOp },
    /// On settings: verify claims window allows shielding, then forward `NftAuthorityOp::Shield`.
    ShieldNft {
        token_id: [u8; 32],
        deed_account: AccountId,
        expires: u64,
        claimed: bool,
        now: u64,
    },
    /// On settings: verify legacy claim proof and claims window.
    RequireClaim {
        id: NameId,
        holder: OwnerTag,
        now: u64,
        proof: ClaimProof,
    },
    /// On the record: claim a name permanently, burning any overwritten unclaimed NFT
    /// and cleaning up stale market/site entries.
    Claim {
        id: NameId,
        owner: OwnerTag,
        sealed: Vec<u8>,
        price_tier: u8,
        generation: [u8; 32],
        token_id: [u8; 32],
    },
    /// On the record: replace `sealed` and apply `site_dir`.
    Seal {
        id: NameId,
        signer: OwnerTag,
        sealed: Vec<u8>,
        site_dir: SiteDirOp,
    },
    /// On the record: transfer active, unshielded name; clear sealed, subname_sale,
    /// listing, site_listed; bump generation; sync NFT `Transfer`.
    Transfer {
        id: NameId,
        signer: OwnerTag,
        new_owner: OwnerTag,
        now: u64,
    },
    /// On the record: release unshielded name; clean up listing & site_listed; burn NFT.
    Remove { id: NameId, signer: OwnerTag },
    /// On the record: verify owner and optionally apply `site_dir` (for `SetPage`).
    RequireOwner {
        id: NameId,
        signer: OwnerTag,
        site_dir: SiteDirOp,
    },
    /// On the record: create or update a secondary-sale listing.
    List {
        id: NameId,
        signer: OwnerTag,
        price: u128,
        pay_to: AccountId,
        expires_at: Option<u64>,
        now: u64,
        display_name: Option<String>,
    },
    /// On the record: cancel an active or in-grace listing.
    CancelListing { id: NameId, signer: OwnerTag },
    /// On the record: execute an atomic secondary purchase and sync NFT `Transfer`.
    Buy {
        id: NameId,
        buyer: OwnerTag,
        price: u128,
        pay_to: AccountId,
        expected_nonce: u64,
        now: u64,
    },
    /// On the record: mark `nft_shielded = true` and forward `ShieldNft` to settings.
    ShieldDeed {
        signer: OwnerTag,
        deed_account: AccountId,
        now: u64,
    },
    /// On the record: mark `nft_shielded = false` and forward `Unshield` to settings.
    UnshieldDeed { signer: OwnerTag },
    /// On the record: forward `RefreshShielded` with current `record.expires` to settings.
    RefreshShieldedDeed { signer: OwnerTag },
    /// On a `MarketBucket` PDA: insert or update a listing (with lazy expiry pruning).
    UpsertListing { entry: MarketEntry, now: u64 },
    /// On a `MarketBucket` PDA: idempotently remove a listing.
    RemoveListing { id: NameId },
    /// On a `MarketBucket` PDA: refresh `name_expires` on an active listing after `Renew`.
    RefreshListingExpiry { id: NameId, name_expires: u64 },
    /// On a `SiteBucket` PDA: insert or update a site entry (with lazy post-grace pruning).
    UpsertSite {
        entry: SiteEntry,
        page: u16,
        now: u64,
    },
    /// On a `SiteBucket` PDA: idempotently remove a site entry and decrement `SiteDirHeader`.
    RemoveSite { id: NameId },
    /// On a `SiteBucket` PDA: refresh `name_expires` on a listed site after `Renew`.
    RefreshSiteExpiry { id: NameId, new_expires: u64 },
    /// On `SiteDirHeader`: validate `page <= active_page[bucket]`, update `total_listed`,
    /// and advance `active_page[bucket]` when the frontier page hits 32 entries.
    RecordSiteUpsert {
        bucket_index: u8,
        page: u16,
        page_full: bool,
        added: u32,
        pruned: u32,
    },
    /// On `SiteDirHeader`: decrement `total_listed`.
    DecrementSiteCount { count: u32 },
    /// On a page account (top-level or subname); empty empties it.
    WritePage { sealed: Vec<u8> },
    /// On the signer's own account.
    WritePrimary { name: String },
    /// On a commitment account.
    Commit { now: u64, public: bool },
    /// On a commitment account.
    Reveal { now: u64 },
    /// On the parent record: update `subname_sale`.
    SetSubnameSale {
        signer: OwnerTag,
        sale: Option<SubnameSale>,
    },
    /// On the parent record: authorize `CreateSubname`.
    AuthorizeCreateSubname {
        signer: OwnerTag,
        parent_id: NameId,
        sub_id: NameId,
        owner: OwnerTag,
        revocable: bool,
        sealed: Vec<u8>,
        now: u64,
    },
    /// On the parent record: authorize `BuySubname`.
    AuthorizeBuySubname {
        parent_id: NameId,
        sub_id: NameId,
        owner: OwnerTag,
        sealed: Vec<u8>,
        price: u128,
        payout: Option<AccountId>,
        now: u64,
    },
    /// On the parent record: verify owner & parent not past grace (for `RevokeSubname`).
    RequireParentOwner { signer: OwnerTag, now: u64 },
    /// On the parent record: verify parent liveness and forward its `generation` to subname.
    VerifyParentForSubname {
        parent_id: NameId,
        sub_id: NameId,
        now: u64,
    },
    /// On the subname record: write a new subname.
    WriteSubname {
        owner: OwnerTag,
        kind: SubnameKind,
        current_parent_generation: [u8; 32],
        sealed: Vec<u8>,
    },
    /// On the subname record: revoke a revocable subname.
    RevokeSubname,
    /// On the subname record: check revocable subname's `parent_generation`.
    RequireSubnameParentGeneration { parent_generation: [u8; 32] },
    /// On the subname record: replace `sealed`.
    SubnameSeal {
        signer: OwnerTag,
        sealed: Vec<u8>,
        require_permanent: bool,
    },
    /// On the subname record: transfer ownership and clear `sealed`.
    SubnameTransfer {
        signer: OwnerTag,
        new_owner: OwnerTag,
        require_permanent: bool,
    },
    /// On the subname record: empty it.
    SubnameRemove {
        signer: OwnerTag,
        require_permanent: bool,
    },
    /// On the subname record: verify owner (for `SetSubnamePage`).
    SubnameRequireOwner {
        signer: OwnerTag,
        require_permanent: bool,
    },
}

// ── Events ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Registered {
    pub id: NameId,
    pub expires: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Claimed {
    pub id: NameId,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Renewed {
    pub id: NameId,
    pub expires: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Transferred {
    pub id: NameId,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Released {
    pub id: NameId,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SubnameCreated {
    pub sub_id: NameId,
    pub revocable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SubnameBought {
    pub sub_id: NameId,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SubnameRevoked {
    pub sub_id: NameId,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SubnameTransferred {
    pub sub_id: NameId,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SubnameReleased {
    pub sub_id: NameId,
}

/// Emitted when a top-level name is listed (or re-listed) for secondary sale.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Listed {
    pub id: NameId,
    pub price: u128,
    pub expires_at: Option<u64>,
    pub nonce: u64,
}

/// Emitted when a secondary-sale listing is cancelled.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ListingCancelled {
    pub id: NameId,
}

/// Emitted when a top-level name is sold atomically via [`Instruction::Buy`].
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Sold {
    pub id: NameId,
    pub price: u128,
    pub fee: u128,
}

/// Emitted when a top-level site is added or updated in the sites directory.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SiteListed {
    pub id: NameId,
    pub updated_at: u64,
}

/// Emitted when a top-level site is removed from the sites directory.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SiteUnlisted {
    pub id: NameId,
}

impl Registered {
    pub const SELECTOR_NAME: &str = "logos_names::Registered";
}
impl Claimed {
    pub const SELECTOR_NAME: &str = "logos_names::Claimed";
}
impl Renewed {
    pub const SELECTOR_NAME: &str = "logos_names::Renewed";
}
impl Released {
    pub const SELECTOR_NAME: &str = "logos_names::Released";
}
impl Transferred {
    pub const SELECTOR_NAME: &str = "logos_names::Transferred";
}
impl SubnameCreated {
    pub const SELECTOR_NAME: &str = "logos_names::SubnameCreated";
}
impl SubnameBought {
    pub const SELECTOR_NAME: &str = "logos_names::SubnameBought";
}
impl SubnameRevoked {
    pub const SELECTOR_NAME: &str = "logos_names::SubnameRevoked";
}
impl SubnameTransferred {
    pub const SELECTOR_NAME: &str = "logos_names::SubnameTransferred";
}
impl SubnameReleased {
    pub const SELECTOR_NAME: &str = "logos_names::SubnameReleased";
}
impl Listed {
    pub const SELECTOR_NAME: &str = "logos_names::Listed";
}
impl ListingCancelled {
    pub const SELECTOR_NAME: &str = "logos_names::ListingCancelled";
}
impl Sold {
    pub const SELECTOR_NAME: &str = "logos_names::Sold";
}
impl SiteListed {
    pub const SELECTOR_NAME: &str = "logos_names::SiteListed";
}
impl SiteUnlisted {
    pub const SELECTOR_NAME: &str = "logos_names::SiteUnlisted";
}

// ── Hashing, PDAs, and validation ───────────────────────────────────────

pub(crate) fn sha256(bytes: &[u8]) -> [u8; 32] {
    use risc0_zkvm::sha::{Impl, Sha256 as _};

    Impl::hash_bytes(bytes)
        .as_bytes()
        .try_into()
        .expect("SHA-256 output is 32 bytes")
}

/// LEZ's convention for event selectors: first 8 bytes of `sha256("<program>::<EventName>")`.
#[must_use]
pub fn event_selector(selector_name: &str) -> [u8; 8] {
    sha256(selector_name.as_bytes())[..8]
        .try_into()
        .expect("a slice of 8 bytes")
}

fn event<E: BorshSerialize>(selector_name: &str, data: &E) -> ProgramEvent {
    ProgramEvent {
        selector: event_selector(selector_name),
        data: borsh::to_vec(data).expect("borsh serialization is infallible"),
    }
}

fn seed(domain: &[u8], id: &NameId) -> PdaSeed {
    let mut bytes = Vec::with_capacity(domain.len() + id.len());
    bytes.extend_from_slice(domain);
    bytes.extend_from_slice(id);
    PdaSeed::new(sha256(&bytes))
}

fn subname_seed(domain: &[u8], parent_id: &NameId, sub_id: &NameId) -> PdaSeed {
    let mut bytes = Vec::with_capacity(domain.len() + 64);
    bytes.extend_from_slice(domain);
    bytes.extend_from_slice(parent_id);
    bytes.extend_from_slice(sub_id);
    PdaSeed::new(sha256(&bytes))
}

/// The seeds behind a top-level name's record, page, and deposit accounts (`id` = `slot_id`).
#[must_use]
pub fn name_seeds(id: &NameId) -> [PdaSeed; 3] {
    [
        seed(NAME_SEED_DOMAIN, id),
        seed(PAGE_SEED_DOMAIN, id),
        seed(DEPOSIT_SEED_DOMAIN, id),
    ]
}

/// The seeds behind a subname's record and page accounts.
#[must_use]
pub fn subname_seeds(parent_id: &NameId, sub_id: &NameId) -> [PdaSeed; 2] {
    [
        subname_seed(SUBNAME_SEED_DOMAIN, parent_id, sub_id),
        subname_seed(SUBPAGE_SEED_DOMAIN, parent_id, sub_id),
    ]
}

/// The seed behind [`commitment_address`].
#[must_use]
pub fn commitment_seed(commitment: &[u8; 32]) -> PdaSeed {
    seed(COMMITMENT_SEED_DOMAIN, commitment)
}

/// The seed behind [`settings_address`].
#[must_use]
pub fn settings_seed() -> PdaSeed {
    PdaSeed::new(sha256(SETTINGS_SEED_DOMAIN))
}

/// The pricing tier for a top-level name of byte length `len`:
/// `0` (`PRICE_TIER_5`) for 5 chars (and admin-placed 1–4 char names),
/// `1` (`PRICE_TIER_6_7`) for 6–7 chars, `2` (`PRICE_TIER_8_PLUS`) for 8+ chars.
#[must_use]
pub const fn price_tier_for_len(len: usize) -> u8 {
    if len <= 5 {
        PRICE_TIER_5
    } else if len <= 7 {
        PRICE_TIER_6_7
    } else {
        PRICE_TIER_8_PLUS
    }
}

/// Derives the on-chain slot identifier of a top-level name from its base
/// identifier (`derive(name).id`) and length-based `price_tier`.
#[must_use]
pub fn tiered_id(base_id: &NameId, price_tier: u8) -> NameId {
    assert!(
        usize::from(price_tier) < PRICE_TIERS,
        "price tier must be 0 (<=5 chars), 1 (6-7 chars), or 2 (8+ chars)"
    );
    let mut bytes = Vec::with_capacity(TIER_ID_DOMAIN.len() + 1 + 32);
    bytes.extend_from_slice(TIER_ID_DOMAIN);
    bytes.push(price_tier);
    bytes.extend_from_slice(base_id);
    sha256(&bytes)
}

/// Unified owner tag shared by `names_core` and linked tokens in `lez_nft` v3:
/// `SHA-256("logos-names/v5/owner" || id || account)`.
#[must_use]
pub fn owner_tag(id: &NameId, account: &AccountId) -> OwnerTag {
    let mut bytes = Vec::with_capacity(OWNER_TAG_DOMAIN.len() + 64);
    bytes.extend_from_slice(OWNER_TAG_DOMAIN);
    bytes.extend_from_slice(id);
    bytes.extend_from_slice(account.value());
    sha256(&bytes)
}

/// Commitment for a top-level registration (`slot_id`, `owner_tag`, `secret`).
#[must_use]
pub fn commitment(id: &NameId, holder: &OwnerTag, secret: &[u8; 32]) -> [u8; 32] {
    let mut bytes = Vec::with_capacity(COMMITMENT_DOMAIN.len() + 96);
    bytes.extend_from_slice(COMMITMENT_DOMAIN);
    bytes.extend_from_slice(id);
    bytes.extend_from_slice(holder);
    bytes.extend_from_slice(secret);
    sha256(&bytes)
}

/// Commitment for a public subname purchase ([`Instruction::BuySubname`]).
#[must_use]
pub fn subname_commitment(
    parent_id: &NameId,
    sub_id: &NameId,
    holder: &OwnerTag,
    secret: &[u8; 32],
) -> [u8; 32] {
    let mut bytes = Vec::with_capacity(SUBNAME_COMMITMENT_DOMAIN.len() + 128);
    bytes.extend_from_slice(SUBNAME_COMMITMENT_DOMAIN);
    bytes.extend_from_slice(parent_id);
    bytes.extend_from_slice(sub_id);
    bytes.extend_from_slice(holder);
    bytes.extend_from_slice(secret);
    sha256(&bytes)
}

fn register_generation(
    slot_id: &NameId,
    owner: &OwnerTag,
    now: u64,
    secret: &[u8; 32],
) -> [u8; 32] {
    let mut bytes = Vec::with_capacity(GEN_REGISTER_DOMAIN.len() + 104);
    bytes.extend_from_slice(GEN_REGISTER_DOMAIN);
    bytes.extend_from_slice(slot_id);
    bytes.extend_from_slice(owner);
    bytes.extend_from_slice(&now.to_le_bytes());
    bytes.extend_from_slice(secret);
    sha256(&bytes)
}

fn claim_generation(slot_id: &NameId, owner: &OwnerTag, now: u64) -> [u8; 32] {
    let mut bytes = Vec::with_capacity(GEN_CLAIM_DOMAIN.len() + 72);
    bytes.extend_from_slice(GEN_CLAIM_DOMAIN);
    bytes.extend_from_slice(slot_id);
    bytes.extend_from_slice(owner);
    bytes.extend_from_slice(&now.to_le_bytes());
    sha256(&bytes)
}

fn transfer_generation(prev: &[u8; 32], new_owner: &OwnerTag) -> [u8; 32] {
    let mut bytes = Vec::with_capacity(GEN_TRANSFER_DOMAIN.len() + 64);
    bytes.extend_from_slice(GEN_TRANSFER_DOMAIN);
    bytes.extend_from_slice(prev);
    bytes.extend_from_slice(new_owner);
    sha256(&bytes)
}

#[must_use]
pub fn commitment_address(program: &AccountId, commitment: &[u8; 32]) -> AccountId {
    AccountId::for_public_pda(program, &seed(COMMITMENT_SEED_DOMAIN, commitment))
}

const fn commit_window_ms(public: bool) -> u64 {
    if public {
        PUBLIC_TIME_WINDOW_MS
    } else {
        TIME_WINDOW_MS
    }
}

const fn min_commit_age_ms(public: bool) -> u64 {
    if public {
        PUBLIC_MIN_COMMIT_AGE_MS
    } else {
        MIN_COMMIT_AGE_MS
    }
}

/// Valid only in a block whose timestamp is in `[now, now + window_ms)`.
fn bounded_window_of(response: Response, now: u64, window_ms: u64) -> Response {
    let window = lee_core::program::TimestampValidityWindow::try_from((
        Some(now),
        Some(after(now, window_ms)),
    ))
    .expect("a window of positive length");
    response.timestamp_window(window)
}

/// Valid only in a block whose timestamp is in `[now, now + TIME_WINDOW_MS)`.
fn bounded_window(response: Response, now: u64) -> Response {
    bounded_window_of(response, now, TIME_WINDOW_MS)
}

#[must_use]
pub fn claim_leaf(old_record: &AccountId, holder: &OwnerTag) -> [u8; 32] {
    let mut bytes = Vec::with_capacity(CLAIM_LEAF_DOMAIN.len() + 64);
    bytes.extend_from_slice(CLAIM_LEAF_DOMAIN);
    bytes.extend_from_slice(old_record.value());
    bytes.extend_from_slice(holder);
    sha256(&bytes)
}

fn claim_node(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let (low, high) = if left <= right {
        (left, right)
    } else {
        (right, left)
    };
    let mut bytes = Vec::with_capacity(CLAIM_NODE_DOMAIN.len() + 64);
    bytes.extend_from_slice(CLAIM_NODE_DOMAIN);
    bytes.extend_from_slice(low);
    bytes.extend_from_slice(high);
    sha256(&bytes)
}

pub type ClaimProof = Vec<[u8; 32]>;

fn claim_level(level: &[[u8; 32]]) -> Vec<[u8; 32]> {
    level
        .chunks(2)
        .map(|pair| match pair {
            [left, right] => claim_node(left, right),
            [only] => *only,
            _ => unreachable!("chunks of two"),
        })
        .collect()
}

#[must_use]
pub fn claim_root(leaves: &[[u8; 32]]) -> [u8; 32] {
    let Some(first) = leaves.first() else {
        return [0; 32];
    };
    let mut level = leaves.to_vec();
    while level.len() > 1 {
        level = claim_level(&level);
    }
    level.first().copied().unwrap_or(*first)
}

#[must_use]
pub fn claim_proof(leaves: &[[u8; 32]], mut index: usize) -> ClaimProof {
    let mut proof = Vec::new();
    let mut level = leaves.to_vec();
    while level.len() > 1 {
        if let Some(sibling) = level.get(index ^ 1) {
            proof.push(*sibling);
        }
        level = claim_level(&level);
        index /= 2;
    }
    proof
}

fn claim_holds(root: &[u8; 32], leaf: [u8; 32], proof: &[[u8; 32]]) -> bool {
    proof
        .iter()
        .fold(leaf, |node, sibling| claim_node(&node, sibling))
        == *root
}

#[must_use]
pub fn record_address(program: &AccountId, id: &NameId) -> AccountId {
    AccountId::for_public_pda(program, &seed(NAME_SEED_DOMAIN, id))
}

#[must_use]
pub fn page_address(program: &AccountId, id: &NameId) -> AccountId {
    AccountId::for_public_pda(program, &seed(PAGE_SEED_DOMAIN, id))
}

#[must_use]
pub fn deposit_address(program: &AccountId, id: &NameId) -> AccountId {
    AccountId::for_public_pda(program, &seed(DEPOSIT_SEED_DOMAIN, id))
}

#[must_use]
pub fn subname_record_address(
    program: &AccountId,
    parent_id: &NameId,
    sub_id: &NameId,
) -> AccountId {
    AccountId::for_public_pda(
        program,
        &subname_seed(SUBNAME_SEED_DOMAIN, parent_id, sub_id),
    )
}

#[must_use]
pub fn subname_page_address(
    program: &AccountId,
    parent_id: &NameId,
    sub_id: &NameId,
) -> AccountId {
    AccountId::for_public_pda(
        program,
        &subname_seed(SUBPAGE_SEED_DOMAIN, parent_id, sub_id),
    )
}

#[must_use]
pub fn settings_address(program: &AccountId) -> AccountId {
    AccountId::for_public_pda(program, &settings_seed())
}

/// Checks a single label (a top-level name, or either side of `label.parent`):
/// 1–32 ASCII chars of `a-z`, `0-9`, `-`, not starting or ending with `-`, and
/// never containing `--`.
pub fn validate_label(label: &str) -> Result<(), &'static str> {
    let bytes = label.as_bytes();
    let charset_ok = bytes
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-');
    if bytes.is_empty()
        || bytes.len() > MAX_NAME_LEN
        || !charset_ok
        || bytes[0] == b'-'
        || bytes[bytes.len() - 1] == b'-'
        || bytes.windows(2).any(|w| w == b"--")
    {
        return Err(
            "name must be 1-32 chars of a-z, 0-9 or '-', not starting or ending with '-', and without '--'",
        );
    }
    Ok(())
}

/// Splits a valid one-level subname `label.parent` into `(label, parent)`.
#[must_use]
pub fn split_subname(name: &str) -> Option<(&str, &str)> {
    if name.len() > MAX_SUBNAME_LEN {
        return None;
    }
    let (label, parent) = name.split_once('.')?;
    validate_label(label).ok()?;
    validate_label(parent).ok()?;
    Some((label, parent))
}

/// Checks a top-level name (`alice`) or a one-level subname (`blog.alice`).
pub fn validate_name(name: &str) -> Result<(), &'static str> {
    if let Some((label, parent)) = name.split_once('.') {
        if name.len() > MAX_SUBNAME_LEN {
            return Err(
                "subname must be at most 62 chars (so gateway label--parent fits in 63 bytes)",
            );
        }
        validate_label(label)?;
        validate_label(parent)?;
        return Ok(());
    }
    validate_label(name)
}

/// Normalizes user input (`Jef`, `@jef`, `lns:jef`, `lns://blog.alice`, `jef.lez`)
/// into the bare name. The result still has to pass [`validate_name`].
#[must_use]
pub fn normalize_name(input: &str) -> String {
    let lowered = input.trim().to_ascii_lowercase();
    let bare = lowered
        .strip_prefix("lns://")
        .or_else(|| lowered.strip_prefix("lns:"))
        .unwrap_or(&lowered);
    let bare = bare.strip_prefix('@').unwrap_or(bare);
    bare.strip_suffix(LEGACY_SUFFIX).unwrap_or(bare).to_owned()
}

fn checked<T>(result: Result<T, &'static str>) -> T {
    result.unwrap_or_else(|reason| panic!("{reason}"))
}

fn term_length(term: u8) -> u64 {
    *TERM_LENGTHS_MS
        .get(usize::from(term))
        .unwrap_or_else(|| panic!("term must be 0, 1, 2 or 3"))
}

fn check_term(term: u8) {
    assert!(usize::from(term) < TERMS, "term must be 0, 1, 2 or 3");
}

fn after(time: u64, length: u64) -> u64 {
    time.checked_add(length).expect("the time is out of range")
}

/// The root of the locked-name list ([`shortlist`]): 1–4 char names + reserved list.
pub const SHORTLIST_ROOT: [u8; 32] =
    hex_literal(b"9778bed49005fe7401d25685a27d9cf3e681fa0582b562336cdb8ee9661d990c");

const fn hex_literal(hex: &[u8; 64]) -> [u8; 32] {
    const fn nibble(c: u8) -> u8 {
        match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            _ => panic!("not a lowercase hex digit"),
        }
    }
    let mut out = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        out[i] = nibble(hex[2 * i]) << 4 | nibble(hex[2 * i + 1]);
        i += 1;
    }
    out
}

fn length_tier(id: &NameId, proof: &LengthProof) -> u8 {
    #[cfg(not(test))]
    let checked = shortlist::verify(&SHORTLIST_ROOT, id, proof);
    #[cfg(test)]
    let checked = tests::verify_against_test_list(id, proof);
    checked.unwrap_or_else(|reason| panic!("the length proof does not hold: {reason}"))
}

fn self_actor(account_id: AccountId, program: &AccountId) -> Actor {
    Actor::new(account_id, *program)
}

pub(crate) fn effect(to: AccountId, program: &AccountId, effect: Effect) -> SyncMessage {
    SyncMessage::new(self_actor(to, program), &Message::Effect(effect))
}

fn sync_nft_effect(program: &AccountId, op: NftAuthorityOp) -> SyncMessage {
    effect(settings_address(program), program, Effect::SyncNft { op })
}

fn linked_authority(program: &AccountId, slot_id: &NameId) -> LinkedAuthority {
    LinkedAuthority {
        program: *program,
        authority_actor: settings_address(program),
        target: *slot_id,
        record_actor: record_address(program, slot_id),
    }
}

fn validate_site_dir_op(site_dir: &SiteDirOp) -> Option<u64> {
    match site_dir {
        SiteDirOp::Keep | SiteDirOp::Remove => None,
        SiteDirOp::Upsert {
            name, title, now, ..
        } => {
            checked(validate_label(name));
            checked(validate_site_title(title));
            Some(*now)
        }
    }
}

fn apply_site_dir_on_record(
    program: &AccountId,
    id: NameId,
    record: &mut NameRecord,
    site_dir: SiteDirOp,
) -> (bool, Option<SyncMessage>, Option<ProgramEvent>) {
    match site_dir {
        SiteDirOp::Keep => (false, None, None),
        SiteDirOp::Upsert {
            name,
            title,
            page,
            now,
        } => {
            assert!(
                now < record.expires,
                "lapsed name cannot be listed in the sites directory"
            );
            if let Some(old_page) = record.site_listed {
                assert!(
                    old_page == page,
                    "name is already listed on another directory page; update on that page or remove first"
                );
            }
            record.site_listed = Some(page);
            let entry = SiteEntry {
                id,
                name,
                title,
                updated_at: now,
                name_expires: record.expires,
            };
            let msg = effect(
                site_bucket_address(program, bucket_of(&id), page),
                program,
                Effect::UpsertSite { entry, page, now },
            );
            let evt = event(
                SiteListed::SELECTOR_NAME,
                &SiteListed {
                    id,
                    updated_at: now,
                },
            );
            (true, Some(msg), Some(evt))
        }
        SiteDirOp::Remove => match record.site_listed.take() {
            Some(page) => {
                let msg = effect(
                    site_bucket_address(program, bucket_of(&id), page),
                    program,
                    Effect::RemoveSite { id },
                );
                let evt = event(SiteUnlisted::SELECTOR_NAME, &SiteUnlisted { id });
                (true, Some(msg), Some(evt))
            }
            None => (false, None, None),
        },
    }
}

// ── Entry points ────────────────────────────────────────────────────────

/// Handles a message to one of this program's actors. The guest's entry point.
#[must_use]
pub fn handle(input: &ReceiveInput, message: Message) -> Response {
    match message {
        Message::Instruction(instruction) => instruct(input, instruction),
        Message::Effect(effect) => {
            assert!(
                input.from_own_program(),
                "only this program may send an effect"
            );
            let reveal_window = match &effect {
                Effect::Reveal { now } => load_commitment(&input.pre_state)
                    .map(|held| (*now, commit_window_ms(held.public))),
                _ => None,
            };
            let response = apply(&input.receiver.program_account_id, effect, &input.pre_state);
            match reveal_window {
                Some((now, window_ms)) => bounded_window_of(response, now, window_ms),
                None => response,
            }
        }
        Message::RequireHolder { tag } => {
            load_owned(&input.pre_state, &tag);
            Response::keep_state()
        }
        Message::NftTransferHook {
            id,
            token_id,
            expected_old_owner,
            new_owner,
            now,
        } => {
            let from = input.from.expect("nft transfer hook requires a sender");
            let program = &input.receiver.program_account_id;
            assert!(
                input.receiver.account_id == record_address(program, &id),
                "nft transfer hook must target the name record actor"
            );
            let response = apply_nft_transfer_hook(
                program,
                from,
                id,
                token_id,
                expected_old_owner,
                new_owner,
                now,
                &input.pre_state,
            );
            bounded_window(response, now)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_nft_transfer_hook(
    program: &AccountId,
    from: Actor,
    id: NameId,
    token_id: [u8; 32],
    expected_old_owner: OwnerTag,
    new_owner: OwnerTag,
    now: u64,
    pre_data: &[u8],
) -> Response {
    assert!(!pre_data.is_empty(), "name is not registered");
    let mut record: NameRecord =
        borsh::from_slice(pre_data).expect("a record actor holds a valid record");
    assert!(record.nft_token_id == token_id, "token id mismatch");
    assert!(!record.nft_shielded, "unshield private deed before transfer");
    assert!(record.owner == expected_old_owner, "owner mismatch");
    assert!(
        now < record.expires,
        "lapsed name cannot transfer; renew first"
    );

    let remove_market = record.listing.take().is_some_and(|l| l.in_market);
    let remove_site = record.site_listed.take();
    record.owner = new_owner;
    record.sealed = Vec::new();
    record.subname_sale = None;
    record.generation = transfer_generation(&record.generation, &new_owner);

    let mut response = Response::set_state(to_bytes(&record))
        .event(event(Transferred::SELECTOR_NAME, &Transferred { id }))
        .send(effect(
            settings_address(program),
            program,
            Effect::RequireNftTokenSender { from, token_id },
        ))
        .send(effect(
            page_address(program, &id),
            program,
            Effect::WritePage { sealed: Vec::new() },
        ));
    if remove_market {
        response = response.send(effect(
            market_bucket_address(program, bucket_of(&id)),
            program,
            Effect::RemoveListing { id },
        ));
    }
    if let Some(page) = remove_site {
        response = response.send(effect(
            site_bucket_address(program, bucket_of(&id), page),
            program,
            Effect::RemoveSite { id },
        ));
    }
    response
}

/// Turns an instruction from the signer into the effects it has on other actors.
#[must_use]
pub fn instruct(input: &ReceiveInput, instruction: Instruction) -> Response {
    assert!(input.is_authorized(), "the signer's account must sign");
    let program = &input.receiver.program_account_id;
    let signer = &input.receiver.account_id;
    let response = Response::keep_state();

    match instruction {
        Instruction::Register {
            id,
            sealed,
            term,
            fee,
            treasury,
            now,
            secret,
            deposit,
            length,
            price_tier,
        } => {
            assert!(
                sealed.len() <= MAX_SEALED_RECORD_LEN,
                "sealed profile exceeds {MAX_SEALED_RECORD_LEN} bytes"
            );
            let expires = if deposit {
                u64::MAX
            } else {
                check_term(term);
                if term == TERM_PERMANENT {
                    u64::MAX
                } else {
                    after(now, term_length(term))
                }
            };
            let shortlist_tier = length_tier(&id, &length);
            let slot_id = tiered_id(&id, price_tier);
            let owner = owner_tag(&slot_id, signer);
            let promised = commitment(&slot_id, &owner, &secret);
            let generation = register_generation(&slot_id, &owner, now, &secret);
            let token_id = nft_token_id(&slot_id, &generation);
            let to = match (fee, deposit) {
                (0, _) => None,
                (1.., true) => Some(deposit_address(program, &slot_id)),
                (1.., false) => Some(treasury.expect("a fee is paid to the treasury")),
            };
            let response = bounded_window(response, now)
                .send(effect(
                    commitment_address(program, &promised),
                    program,
                    Effect::Reveal { now },
                ))
                .event(event(
                    Registered::SELECTOR_NAME,
                    &Registered {
                        id: slot_id,
                        expires,
                    },
                ))
                .send(effect(
                    record_address(program, &slot_id),
                    program,
                    Effect::Register {
                        id: slot_id,
                        owner,
                        now,
                        expires,
                        sealed,
                        price_tier,
                        generation,
                        token_id,
                        fee,
                        treasury: if deposit { None } else { to },
                        term,
                        deposit,
                        shortlist_tier,
                    },
                ))
                .send(effect(
                    page_address(program, &slot_id),
                    program,
                    Effect::WritePage { sealed: Vec::new() },
                ));
            match to {
                Some(to) => response.send(
                    SyncMessage::new(
                        Actor::native_balance(*signer),
                        &native_token::Message::Transfer { to, amount: fee },
                    )
                    .with_lent_grant(input),
                ),
                None => response,
            }
        }
        Instruction::Renew {
            id,
            term,
            fee,
            treasury,
            now,
        } => {
            check_term(term);
            let to = if fee == 0 {
                None
            } else {
                Some(treasury.expect("a fee is paid to the treasury"))
            };
            let event_expires = if term == TERM_PERMANENT {
                u64::MAX
            } else {
                after(now, term_length(term))
            };
            let response = bounded_window(response, now)
                .event(event(
                    Renewed::SELECTOR_NAME,
                    &Renewed {
                        id,
                        expires: event_expires,
                    },
                ))
                .send(effect(
                    record_address(program, &id),
                    program,
                    Effect::Renew {
                        id,
                        payer: owner_tag(&id, signer),
                        treasury: to,
                        term,
                        fee,
                        now,
                    },
                ));
            match to {
                Some(to) => response.send(
                    SyncMessage::new(
                        Actor::native_balance(*signer),
                        &native_token::Message::Transfer { to, amount: fee },
                    )
                    .with_lent_grant(input),
                ),
                None => response,
            }
        }
        Instruction::SetRecord {
            id,
            sealed,
            site_dir,
        } => {
            assert!(
                sealed.len() <= MAX_SEALED_RECORD_LEN,
                "sealed profile exceeds {MAX_SEALED_RECORD_LEN} bytes"
            );
            let response = match validate_site_dir_op(&site_dir) {
                Some(now) => bounded_window(response, now),
                None => response,
            };
            response.send(effect(
                record_address(program, &id),
                program,
                Effect::Seal {
                    id,
                    signer: owner_tag(&id, signer),
                    sealed,
                    site_dir,
                },
            ))
        }
        Instruction::SetPage {
            id,
            sealed,
            site_dir,
        } => {
            assert!(
                sealed.len() <= MAX_SEALED_PAGE_LEN,
                "sealed page exceeds {MAX_SEALED_PAGE_LEN} bytes"
            );
            let response = match validate_site_dir_op(&site_dir) {
                Some(now) => bounded_window(response, now),
                None => response,
            };
            response
                .send(effect(
                    record_address(program, &id),
                    program,
                    Effect::RequireOwner {
                        id,
                        signer: owner_tag(&id, signer),
                        site_dir,
                    },
                ))
                .send(effect(
                    page_address(program, &id),
                    program,
                    Effect::WritePage { sealed },
                ))
        }
        Instruction::Transfer { id, new_owner, now } => bounded_window(response, now)
            .event(event(Transferred::SELECTOR_NAME, &Transferred { id }))
            .send(effect(
                record_address(program, &id),
                program,
                Effect::Transfer {
                    id,
                    signer: owner_tag(&id, signer),
                    new_owner,
                    now,
                },
            ))
            .send(effect(
                page_address(program, &id),
                program,
                Effect::WritePage { sealed: Vec::new() },
            )),
        Instruction::Release { id, refund } => {
            let response = response
                .event(event(Released::SELECTOR_NAME, &Released { id }))
                .send(effect(
                    record_address(program, &id),
                    program,
                    Effect::Remove {
                        id,
                        signer: owner_tag(&id, signer),
                    },
                ))
                .send(effect(
                    page_address(program, &id),
                    program,
                    Effect::WritePage { sealed: Vec::new() },
                ));
            if refund == 0 {
                return response;
            }
            response.send(native_token::custody_transfer(
                deposit_address(program, &id),
                seed(DEPOSIT_SEED_DOMAIN, &id),
                *signer,
                refund,
            ))
        }
        Instruction::SetPrimary { name } => {
            if !name.is_empty() {
                checked(validate_name(&name));
            }
            apply(program, Effect::WritePrimary { name }, &input.pre_state)
        }
        Instruction::Configure {
            admin,
            treasury,
            prices,
            mode,
            deposits,
            claims_root,
            claims_until,
            claims_program,
            premium_start,
            nft_program,
        } => {
            assert!(
                mode == MODE_FEE || mode == MODE_DEPOSIT,
                "mode must be fee (0) or deposit (1)"
            );
            response.send(effect(
                settings_address(program),
                program,
                Effect::WriteSettings {
                    signer: *signer,
                    by_program: signer == program,
                    admin,
                    treasury,
                    prices,
                    mode,
                    deposits,
                    claims_root,
                    claims_until,
                    claims_program,
                    premium_start,
                    nft_program,
                },
            ))
        }
        Instruction::SetPrice {
            price_tier,
            term,
            price,
        } => {
            assert!(
                usize::from(price_tier) < PRICE_TIERS,
                "price tier must be 0 (<=5 chars), 1 (6-7 chars), or 2 (8+ chars)"
            );
            check_term(term);
            response.send(effect(
                settings_address(program),
                program,
                Effect::SetPrice {
                    signer: *signer,
                    price_tier,
                    term,
                    price,
                },
            ))
        }
        Instruction::SetPremium { premium_start } => response.send(effect(
            settings_address(program),
            program,
            Effect::SetPremium {
                signer: *signer,
                premium_start,
            },
        )),
        Instruction::Commit { commitment, now } => bounded_window(response, now).send(effect(
            commitment_address(program, &commitment),
            program,
            Effect::Commit { now, public: false },
        )),
        Instruction::CommitPublic { commitment, now } => {
            bounded_window_of(response, now, PUBLIC_TIME_WINDOW_MS).send(effect(
                commitment_address(program, &commitment),
                program,
                Effect::Commit { now, public: true },
            ))
        }
        Instruction::Claim {
            id,
            sealed,
            now,
            proof,
            price_tier,
        } => {
            assert!(
                sealed.len() <= MAX_SEALED_RECORD_LEN,
                "sealed profile exceeds {MAX_SEALED_RECORD_LEN} bytes"
            );
            let slot_id = tiered_id(&id, price_tier);
            let old_holder = owner_tag(&id, signer);
            let owner = owner_tag(&slot_id, signer);
            let generation = claim_generation(&slot_id, &owner, now);
            let token_id = nft_token_id(&slot_id, &generation);
            bounded_window(response, now)
                .event(event(Claimed::SELECTOR_NAME, &Claimed { id: slot_id }))
                .send(effect(
                    settings_address(program),
                    program,
                    Effect::RequireClaim {
                        id,
                        holder: old_holder,
                        now,
                        proof,
                    },
                ))
                .send(effect(
                    record_address(program, &slot_id),
                    program,
                    Effect::Claim {
                        id: slot_id,
                        owner,
                        sealed,
                        price_tier,
                        generation,
                        token_id,
                    },
                ))
                .send(effect(
                    page_address(program, &slot_id),
                    program,
                    Effect::WritePage { sealed: Vec::new() },
                ))
        }
        Instruction::List {
            id,
            price,
            pay_to,
            expires_at,
            now,
            display_name,
        } => {
            assert!(
                price >= MIN_SALE_PRICE,
                "sale price must be at least {MIN_SALE_PRICE}"
            );
            if let Some(exp) = expires_at {
                assert!(exp > now, "listing expiry must be in the future");
            }
            if let Some(ref name) = display_name {
                checked(validate_label(name));
            }
            bounded_window(response, now).send(effect(
                record_address(program, &id),
                program,
                Effect::List {
                    id,
                    signer: owner_tag(&id, signer),
                    price,
                    pay_to,
                    expires_at,
                    now,
                    display_name,
                },
            ))
        }
        Instruction::Cancel { id } => response
            .event(event(
                ListingCancelled::SELECTOR_NAME,
                &ListingCancelled { id },
            ))
            .send(effect(
                record_address(program, &id),
                program,
                Effect::CancelListing {
                    id,
                    signer: owner_tag(&id, signer),
                },
            )),
        Instruction::Buy {
            id,
            price,
            pay_to,
            treasury,
            expected_nonce,
            now,
        } => {
            assert!(
                price >= MIN_SALE_PRICE,
                "sale price must be at least {MIN_SALE_PRICE}"
            );
            let (seller_amount, fee) = sale_split(price);
            let buyer = owner_tag(&id, signer);
            let response = bounded_window(response, now)
                .event(event(Sold::SELECTOR_NAME, &Sold { id, price, fee }))
                .send(effect(
                    settings_address(program),
                    program,
                    Effect::RequireTreasury { treasury },
                ))
                .send(effect(
                    record_address(program, &id),
                    program,
                    Effect::Buy {
                        id,
                        buyer,
                        price,
                        pay_to,
                        expected_nonce,
                        now,
                    },
                ))
                .send(effect(
                    page_address(program, &id),
                    program,
                    Effect::WritePage { sealed: Vec::new() },
                ));
            if pay_to == treasury {
                response.send(
                    SyncMessage::new(
                        Actor::native_balance(*signer),
                        &native_token::Message::Transfer {
                            to: treasury,
                            amount: price,
                        },
                    )
                    .with_lent_grant(input),
                )
            } else {
                response
                    .send(
                        SyncMessage::new(
                            Actor::native_balance(*signer),
                            &native_token::Message::Transfer {
                                to: pay_to,
                                amount: seller_amount,
                            },
                        )
                        .with_lent_grant(input),
                    )
                    .send(
                        SyncMessage::new(
                            Actor::native_balance(*signer),
                            &native_token::Message::Transfer {
                                to: treasury,
                                amount: fee,
                            },
                        )
                        .with_lent_grant(input),
                    )
            }
        }
        Instruction::ShieldDeed {
            id,
            deed_account,
            now,
        } => bounded_window(response, now).send(effect(
            record_address(program, &id),
            program,
            Effect::ShieldDeed {
                signer: owner_tag(&id, signer),
                deed_account,
                now,
            },
        )),
        Instruction::UnshieldDeed { id } => response.send(effect(
            record_address(program, &id),
            program,
            Effect::UnshieldDeed {
                signer: owner_tag(&id, signer),
            },
        )),
        Instruction::RefreshShieldedDeed { id } => response.send(effect(
            record_address(program, &id),
            program,
            Effect::RefreshShieldedDeed {
                signer: owner_tag(&id, signer),
            },
        )),
        Instruction::SetSubnameSale { id, sale } => response.send(effect(
            record_address(program, &id),
            program,
            Effect::SetSubnameSale {
                signer: owner_tag(&id, signer),
                sale,
            },
        )),
        Instruction::CreateSubname {
            parent_id,
            sub_id,
            owner,
            revocable,
            sealed,
            now,
        } => {
            assert!(
                sealed.len() <= MAX_SEALED_RECORD_LEN,
                "sealed profile exceeds {MAX_SEALED_RECORD_LEN} bytes"
            );
            bounded_window(response, now)
                .event(event(
                    SubnameCreated::SELECTOR_NAME,
                    &SubnameCreated { sub_id, revocable },
                ))
                .send(effect(
                    record_address(program, &parent_id),
                    program,
                    Effect::AuthorizeCreateSubname {
                        signer: owner_tag(&parent_id, signer),
                        parent_id,
                        sub_id,
                        owner,
                        revocable,
                        sealed,
                        now,
                    },
                ))
        }
        Instruction::RevokeSubname {
            parent_id,
            sub_id,
            now,
        } => bounded_window(response, now)
            .event(event(
                SubnameRevoked::SELECTOR_NAME,
                &SubnameRevoked { sub_id },
            ))
            .send(effect(
                record_address(program, &parent_id),
                program,
                Effect::RequireParentOwner {
                    signer: owner_tag(&parent_id, signer),
                    now,
                },
            ))
            .send(effect(
                subname_record_address(program, &parent_id, &sub_id),
                program,
                Effect::RevokeSubname,
            ))
            .send(effect(
                subname_page_address(program, &parent_id, &sub_id),
                program,
                Effect::WritePage { sealed: Vec::new() },
            )),
        Instruction::BuySubname {
            parent_id,
            sub_id,
            sealed,
            price,
            payout,
            now,
            secret,
        } => {
            assert!(
                sealed.len() <= MAX_SEALED_RECORD_LEN,
                "sealed profile exceeds {MAX_SEALED_RECORD_LEN} bytes"
            );
            let to = if price == 0 {
                None
            } else {
                Some(payout.expect("a subname purchase payment needs the parent's payout account"))
            };
            let owner = owner_tag(&sub_id, signer);
            let promised = subname_commitment(&parent_id, &sub_id, &owner, &secret);
            let response = bounded_window(response, now)
                .send(effect(
                    commitment_address(program, &promised),
                    program,
                    Effect::Reveal { now },
                ))
                .event(event(
                    SubnameBought::SELECTOR_NAME,
                    &SubnameBought { sub_id },
                ))
                .send(effect(
                    record_address(program, &parent_id),
                    program,
                    Effect::AuthorizeBuySubname {
                        parent_id,
                        sub_id,
                        owner,
                        sealed,
                        price,
                        payout: to,
                        now,
                    },
                ));
            match to {
                Some(to) => response.send(
                    SyncMessage::new(
                        Actor::native_balance(*signer),
                        &native_token::Message::Transfer { to, amount: price },
                    )
                    .with_lent_grant(input),
                ),
                None => response,
            }
        }
        Instruction::SetSubnameRecord {
            parent_id,
            sub_id,
            sealed,
            now,
        } => {
            assert!(
                sealed.len() <= MAX_SEALED_RECORD_LEN,
                "sealed profile exceeds {MAX_SEALED_RECORD_LEN} bytes"
            );
            let (response, require_permanent) =
                with_optional_parent_check(response, program, &parent_id, &sub_id, now);
            response.send(effect(
                subname_record_address(program, &parent_id, &sub_id),
                program,
                Effect::SubnameSeal {
                    signer: owner_tag(&sub_id, signer),
                    sealed,
                    require_permanent,
                },
            ))
        }
        Instruction::SetSubnamePage {
            parent_id,
            sub_id,
            sealed,
            now,
        } => {
            assert!(
                sealed.len() <= MAX_SEALED_PAGE_LEN,
                "sealed page exceeds {MAX_SEALED_PAGE_LEN} bytes"
            );
            let (response, require_permanent) =
                with_optional_parent_check(response, program, &parent_id, &sub_id, now);
            response
                .send(effect(
                    subname_record_address(program, &parent_id, &sub_id),
                    program,
                    Effect::SubnameRequireOwner {
                        signer: owner_tag(&sub_id, signer),
                        require_permanent,
                    },
                ))
                .send(effect(
                    subname_page_address(program, &parent_id, &sub_id),
                    program,
                    Effect::WritePage { sealed },
                ))
        }
        Instruction::TransferSubname {
            parent_id,
            sub_id,
            new_owner,
            now,
        } => {
            let (response, require_permanent) =
                with_optional_parent_check(response, program, &parent_id, &sub_id, now);
            response
                .event(event(
                    SubnameTransferred::SELECTOR_NAME,
                    &SubnameTransferred { sub_id },
                ))
                .send(effect(
                    subname_record_address(program, &parent_id, &sub_id),
                    program,
                    Effect::SubnameTransfer {
                        signer: owner_tag(&sub_id, signer),
                        new_owner,
                        require_permanent,
                    },
                ))
                .send(effect(
                    subname_page_address(program, &parent_id, &sub_id),
                    program,
                    Effect::WritePage { sealed: Vec::new() },
                ))
        }
        Instruction::ReleaseSubname {
            parent_id,
            sub_id,
            now,
        } => {
            let (response, require_permanent) =
                with_optional_parent_check(response, program, &parent_id, &sub_id, now);
            response
                .event(event(
                    SubnameReleased::SELECTOR_NAME,
                    &SubnameReleased { sub_id },
                ))
                .send(effect(
                    subname_record_address(program, &parent_id, &sub_id),
                    program,
                    Effect::SubnameRemove {
                        signer: owner_tag(&sub_id, signer),
                        require_permanent,
                    },
                ))
                .send(effect(
                    subname_page_address(program, &parent_id, &sub_id),
                    program,
                    Effect::WritePage { sealed: Vec::new() },
                ))
        }
    }
}

fn with_optional_parent_check(
    response: Response,
    program: &AccountId,
    parent_id: &NameId,
    sub_id: &NameId,
    now: Option<u64>,
) -> (Response, bool) {
    match now {
        Some(now) => (
            bounded_window(response, now).send(effect(
                record_address(program, parent_id),
                program,
                Effect::VerifyParentForSubname {
                    parent_id: *parent_id,
                    sub_id: *sub_id,
                    now,
                },
            )),
            false,
        ),
        None => (response, true),
    }
}

pub(crate) fn to_bytes<T: BorshSerialize>(value: &T) -> Vec<u8> {
    borsh::to_vec(value).expect("borsh serialization is infallible")
}

fn load_owned(pre_data: &[u8], signer: &OwnerTag) -> NameRecord {
    assert!(!pre_data.is_empty(), "name is not registered");
    let record: NameRecord =
        borsh::from_slice(pre_data).expect("a record actor holds a valid record");
    assert!(record.owner == *signer, "signer does not own this name");
    record
}

fn load_owned_subname(
    pre_data: &[u8],
    signer: &OwnerTag,
    require_permanent: bool,
) -> SubnameRecord {
    assert!(!pre_data.is_empty(), "subname is not registered");
    let record: SubnameRecord =
        borsh::from_slice(pre_data).expect("a subname record actor holds a valid subname record");
    if require_permanent {
        assert!(
            matches!(record.kind, SubnameKind::Permanent),
            "revocable subnames must be checked against the parent"
        );
    }
    assert!(record.owner == *signer, "signer does not own this subname");
    record
}

fn load_settings(pre_data: &[u8]) -> Option<Settings> {
    (!pre_data.is_empty())
        .then(|| borsh::from_slice(pre_data).expect("the settings actor holds valid settings"))
}

fn load_commitment(pre_data: &[u8]) -> Option<Commitment> {
    (!pre_data.is_empty())
        .then(|| borsh::from_slice(pre_data).expect("a commitment actor holds a commitment"))
}

/// Applies one effect to the actor it targets. Panicking fails the whole transaction.
#[must_use]
pub fn apply(program: &AccountId, effect: Effect, pre_data: &[u8]) -> Response {
    match effect {
        Effect::Register {
            id,
            owner,
            now,
            expires,
            sealed,
            price_tier,
            generation,
            token_id,
            fee,
            treasury,
            term,
            deposit,
            shortlist_tier,
        } => {
            let (premium_elapsed, old_token_id, remove_market, remove_site, listing_nonce) =
                if !pre_data.is_empty() {
                    let held: NameRecord =
                        borsh::from_slice(pre_data).expect("a record actor holds a valid record");
                    assert!(held.expires != u64::MAX, "name is already registered");
                    let grace_end = held.expires.saturating_add(GRACE_MS);
                    assert!(grace_end <= now, "name is already registered");
                    let elapsed = now - grace_end;
                    let premium_elapsed = (elapsed < PREMIUM_DURATION_MS).then_some(elapsed);
                    (
                        premium_elapsed,
                        Some(held.nft_token_id),
                        held.listing.is_some_and(|l| l.in_market),
                        held.site_listed,
                        held.listing_nonce,
                    )
                } else {
                    (None, None, false, None, 0)
                };

            let mut response = Response::set_state(to_bytes(&NameRecord {
                version: VERSION,
                owner,
                expires,
                sealed,
                claimed: false,
                price_tier,
                generation,
                subname_sale: None,
                nft_token_id: token_id,
                nft_shielded: false,
                listing: None,
                listing_nonce,
                site_listed: None,
            }))
            .send(self::effect(
                settings_address(program),
                program,
                Effect::RequirePrice {
                    id,
                    payer: owner,
                    treasury,
                    term,
                    fee,
                    deposit,
                    shortlist_tier,
                    price_tier,
                    premium_elapsed,
                },
            ));

            if let Some(old_token_id) = old_token_id {
                response = response.send(sync_nft_effect(
                    program,
                    NftAuthorityOp::Burn {
                        token_id: old_token_id,
                    },
                ));
            }
            response = response.send(sync_nft_effect(
                program,
                NftAuthorityOp::Mint {
                    token_id,
                    owner,
                    linked: linked_authority(program, &id),
                    content: DEFAULT_DEED_CONTENT.to_vec(),
                    expires,
                },
            ));
            if remove_market {
                response = response.send(self::effect(
                    market_bucket_address(program, bucket_of(&id)),
                    program,
                    Effect::RemoveListing { id },
                ));
            }
            if let Some(page) = remove_site {
                response = response.send(self::effect(
                    site_bucket_address(program, bucket_of(&id), page),
                    program,
                    Effect::RemoveSite { id },
                ));
            }
            response
        }
        Effect::Renew {
            id,
            payer,
            treasury,
            term,
            fee,
            now,
        } => {
            assert!(!pre_data.is_empty(), "name is not registered");
            let mut record: NameRecord =
                borsh::from_slice(pre_data).expect("a record actor holds a valid record");
            assert!(
                record.expires != u64::MAX,
                "a permanent or deposit-held name does not lapse or renew"
            );
            assert!(
                now < after(record.expires, GRACE_MS),
                "name has lapsed and must be registered again"
            );
            let was_in_grace = now >= record.expires;
            let dead_market_listing = if was_in_grace {
                record.listing.take().is_some_and(|l| l.in_market)
            } else {
                false
            };
            let active_market_listing =
                !was_in_grace && record.listing.is_some_and(|l| l.in_market);
            record.expires = if term == TERM_PERMANENT {
                u64::MAX
            } else {
                after(record.expires.max(now), term_length(term))
            };
            let price_tier = record.price_tier;
            let new_expires = record.expires;
            let token_id = record.nft_token_id;
            let site_page = record.site_listed;

            let mut response = Response::set_state(to_bytes(&record))
                .send(self::effect(
                    settings_address(program),
                    program,
                    Effect::RequirePrice {
                        id,
                        payer,
                        treasury,
                        term,
                        fee,
                        deposit: false,
                        shortlist_tier: TIER_NORMAL,
                        price_tier,
                        premium_elapsed: None,
                    },
                ))
                .send(sync_nft_effect(
                    program,
                    NftAuthorityOp::SetExpiry {
                        token_id,
                        expires: new_expires,
                    },
                ));
            if dead_market_listing {
                response = response.send(self::effect(
                    market_bucket_address(program, bucket_of(&id)),
                    program,
                    Effect::RemoveListing { id },
                ));
            } else if active_market_listing {
                response = response.send(self::effect(
                    market_bucket_address(program, bucket_of(&id)),
                    program,
                    Effect::RefreshListingExpiry {
                        id,
                        name_expires: new_expires,
                    },
                ));
            }
            if let Some(page) = site_page {
                response = response.send(self::effect(
                    site_bucket_address(program, bucket_of(&id), page),
                    program,
                    Effect::RefreshSiteExpiry { id, new_expires },
                ));
            }
            response
        }
        Effect::RequirePrice {
            id,
            payer,
            treasury,
            term,
            fee,
            deposit,
            shortlist_tier,
            price_tier,
            premium_elapsed,
        } => {
            let settings = load_settings(pre_data);
            if shortlist_tier == TIER_LOCKED {
                assert!(
                    settings
                        .as_ref()
                        .is_some_and(|settings| payer == owner_tag(&id, &settings.admin)),
                    "this name is locked (one to four characters, or on the reserved list); only the registry's admin may register it"
                );
            }
            let owed = match settings {
                Some(settings) => {
                    let deposit_mode = settings.mode == MODE_DEPOSIT;
                    assert!(
                        deposit == deposit_mode,
                        "{}",
                        if deposit_mode {
                            "the registry takes deposits: register with a deposit; names don't renew"
                        } else {
                            "the registry charges fees, not deposits"
                        }
                    );
                    if deposit_mode && payer == owner_tag(&id, &settings.admin) {
                        0
                    } else if deposit_mode {
                        settings.deposits[usize::from(shortlist_tier == TIER_THREE)]
                    } else if payer == owner_tag(&id, &settings.treasury) {
                        0
                    } else {
                        assert!(
                            treasury.is_none_or(|treasury| treasury == settings.treasury),
                            "the treasury account is not the registry's treasury"
                        );
                        let base = settings.prices[usize::from(price_tier)][usize::from(term)];
                        let premium = premium_elapsed
                            .map_or(0, |e| rerelease_premium(settings.premium_start, e));
                        base.checked_add(premium).expect("total fee overflow")
                    }
                }
                None => 0,
            };
            if deposit {
                assert!(fee == owed, "the deposit is not this name's deposit");
            } else if owed == 0 {
                assert!(fee == 0, "the fee is not the price of this term");
            } else {
                // `fee >= owed` lets an in-flight private proof succeed if the admin
                // lowers a price or `premium_start` mid-proof, while rejecting underpayment.
                assert!(fee >= owed, "the fee is not the price of this term");
            }
            Response::keep_state()
        }
        Effect::WriteSettings {
            signer,
            by_program,
            admin,
            treasury,
            prices,
            mode,
            deposits,
            claims_root,
            claims_until,
            claims_program,
            premium_start,
            nft_program,
        } => {
            assert!(
                nft_program != native_token::NATIVE_TOKEN_PROGRAM_ID,
                "nft_program cannot be the native token program"
            );
            assert!(
                nft_program != *program,
                "nft_program cannot be the registry itself"
            );
            match load_settings(pre_data) {
                Some(settings) => {
                    assert!(signer == settings.admin, "signer is not the registry's admin");
                    assert!(
                        nft_program == settings.nft_program,
                        "nft_program is immutable once configured"
                    );
                }
                None => assert!(
                    by_program,
                    "the first settings must be signed by the program's own account"
                ),
            }
            Response::set_state(to_bytes(&Settings {
                version: VERSION,
                admin,
                treasury,
                prices,
                mode,
                deposits,
                claims_root,
                claims_until,
                claims_program,
                premium_start,
                nft_program,
            }))
        }
        Effect::SetPrice {
            signer,
            price_tier,
            term,
            price,
        } => {
            let mut settings = load_settings(pre_data).expect("the registry has no settings");
            assert!(signer == settings.admin, "signer is not the registry's admin");
            settings.prices[usize::from(price_tier)][usize::from(term)] = price;
            Response::set_state(to_bytes(&settings))
        }
        Effect::SetPremium {
            signer,
            premium_start,
        } => {
            let mut settings = load_settings(pre_data).expect("the registry has no settings");
            assert!(signer == settings.admin, "signer is not the registry's admin");
            settings.premium_start = premium_start;
            Response::set_state(to_bytes(&settings))
        }
        Effect::RequireTreasury { treasury } => {
            let settings = load_settings(pre_data).expect("the registry has no settings");
            assert!(
                treasury == settings.treasury,
                "the treasury account is not the registry's treasury"
            );
            Response::keep_state()
        }
        Effect::RequireNftTokenSender { from, token_id } => {
            let settings = load_settings(pre_data).expect("the registry has no settings");
            assert!(
                from.program_account_id == settings.nft_program,
                "transfer hook must come from the configured nft program"
            );
            assert!(
                from.account_id == nft_token_address(&settings.nft_program, &token_id),
                "transfer hook must come from the token's pda"
            );
            Response::keep_state()
        }
        Effect::SyncNft { op } => match load_settings(pre_data) {
            Some(settings) => Response::keep_state()
                .send(nft::authority_sync_message(&settings.nft_program, op)),
            None => Response::keep_state(),
        },
        Effect::ShieldNft {
            token_id,
            deed_account,
            expires,
            claimed,
            now,
        } => {
            let settings = load_settings(pre_data).expect("the registry has no settings");
            if !claimed {
                assert!(
                    settings.claims_root == [0; 32] || now >= settings.claims_until,
                    "cannot shield an unclaimed name while claims are open"
                );
            }
            Response::keep_state().send(nft::authority_sync_message(
                &settings.nft_program,
                NftAuthorityOp::Shield {
                    token_id,
                    deed_account,
                    expires,
                },
            ))
        }
        Effect::RequireClaim {
            id,
            holder,
            now,
            proof,
        } => {
            let settings = load_settings(pre_data).expect("the registry has no settings");
            assert!(
                settings.claims_root != [0; 32],
                "this registry takes no claims"
            );
            assert!(now < settings.claims_until, "the claim period has ended");
            let leaf = claim_leaf(&record_address(&settings.claims_program, &id), &holder);
            assert!(
                claim_holds(&settings.claims_root, leaf, &proof),
                "this account did not hold this name"
            );
            Response::keep_state()
        }
        Effect::Claim {
            id,
            owner,
            sealed,
            price_tier,
            generation,
            token_id,
        } => {
            let (old_token_id, remove_market, remove_site, listing_nonce) = if !pre_data.is_empty()
            {
                let held: NameRecord =
                    borsh::from_slice(pre_data).expect("a record state holds a valid record");
                assert!(!held.claimed, "this name was already claimed");
                assert!(
                    !held.nft_shielded,
                    "cannot overwrite a shielded deed during claim"
                );
                (
                    Some(held.nft_token_id),
                    held.listing.is_some_and(|l| l.in_market),
                    held.site_listed,
                    held.listing_nonce,
                )
            } else {
                (None, false, None, 0)
            };
            let mut response = Response::set_state(to_bytes(&NameRecord {
                version: VERSION,
                owner,
                expires: u64::MAX,
                sealed,
                claimed: true,
                price_tier,
                generation,
                subname_sale: None,
                nft_token_id: token_id,
                nft_shielded: false,
                listing: None,
                listing_nonce,
                site_listed: None,
            }));
            if let Some(old_token_id) = old_token_id {
                response = response.send(sync_nft_effect(
                    program,
                    NftAuthorityOp::Burn {
                        token_id: old_token_id,
                    },
                ));
            }
            response = response.send(sync_nft_effect(
                program,
                NftAuthorityOp::Mint {
                    token_id,
                    owner,
                    linked: linked_authority(program, &id),
                    content: DEFAULT_DEED_CONTENT.to_vec(),
                    expires: u64::MAX,
                },
            ));
            if remove_market {
                response = response.send(self::effect(
                    market_bucket_address(program, bucket_of(&id)),
                    program,
                    Effect::RemoveListing { id },
                ));
            }
            if let Some(page) = remove_site {
                response = response.send(self::effect(
                    site_bucket_address(program, bucket_of(&id), page),
                    program,
                    Effect::RemoveSite { id },
                ));
            }
            response
        }
        Effect::Seal {
            id,
            signer,
            sealed,
            site_dir,
        } => {
            let mut record = load_owned(pre_data, &signer);
            record.sealed = sealed;
            let (_, site_msg, site_evt) =
                apply_site_dir_on_record(program, id, &mut record, site_dir);
            let mut response = Response::set_state(to_bytes(&record));
            if let Some(msg) = site_msg {
                response = response.send(msg);
            }
            if let Some(evt) = site_evt {
                response = response.event(evt);
            }
            response
        }
        Effect::Transfer {
            id,
            signer,
            new_owner,
            now,
        } => {
            let mut record = load_owned(pre_data, &signer);
            assert!(!record.nft_shielded, "unshield private deed before transfer");
            assert!(
                now < record.expires,
                "lapsed name cannot transfer; renew first"
            );
            let remove_market = record.listing.take().is_some_and(|l| l.in_market);
            let remove_site = record.site_listed.take();
            let token_id = record.nft_token_id;
            record.owner = new_owner;
            record.sealed = Vec::new();
            record.generation = transfer_generation(&record.generation, &new_owner);
            record.subname_sale = None;

            let mut response = Response::set_state(to_bytes(&record)).send(sync_nft_effect(
                program,
                NftAuthorityOp::Transfer {
                    token_id,
                    new_owner,
                },
            ));
            if remove_market {
                response = response.send(self::effect(
                    market_bucket_address(program, bucket_of(&id)),
                    program,
                    Effect::RemoveListing { id },
                ));
            }
            if let Some(page) = remove_site {
                response = response.send(self::effect(
                    site_bucket_address(program, bucket_of(&id), page),
                    program,
                    Effect::RemoveSite { id },
                ));
            }
            response
        }
        Effect::Remove { id, signer } => {
            let record = load_owned(pre_data, &signer);
            assert!(
                !record.nft_shielded,
                "unshield private deed before releasing name"
            );
            let mut response = Response::set_state(Vec::new()).send(sync_nft_effect(
                program,
                NftAuthorityOp::Burn {
                    token_id: record.nft_token_id,
                },
            ));
            if record.listing.is_some_and(|l| l.in_market) {
                response = response.send(self::effect(
                    market_bucket_address(program, bucket_of(&id)),
                    program,
                    Effect::RemoveListing { id },
                ));
            }
            if let Some(page) = record.site_listed {
                response = response.send(self::effect(
                    site_bucket_address(program, bucket_of(&id), page),
                    program,
                    Effect::RemoveSite { id },
                ));
            }
            response
        }
        Effect::RequireOwner {
            id,
            signer,
            site_dir,
        } => {
            let mut record = load_owned(pre_data, &signer);
            let (changed, site_msg, site_evt) =
                apply_site_dir_on_record(program, id, &mut record, site_dir);
            let mut response = if changed {
                Response::set_state(to_bytes(&record))
            } else {
                Response::keep_state()
            };
            if let Some(msg) = site_msg {
                response = response.send(msg);
            }
            if let Some(evt) = site_evt {
                response = response.event(evt);
            }
            response
        }
        Effect::List {
            id,
            signer,
            price,
            pay_to,
            expires_at,
            now,
            display_name,
        } => {
            let mut record = load_owned(pre_data, &signer);
            assert!(!record.nft_shielded, "unshield private deed before listing");
            assert!(
                now < record.expires,
                "lapsed name cannot be listed; renew first"
            );
            let was_in_market = record.listing.is_some_and(|l| l.in_market);
            let nonce = record
                .listing_nonce
                .checked_add(1)
                .expect("listing nonce overflow");
            record.listing_nonce = nonce;
            let in_market = display_name.is_some();
            record.listing = Some(Listing {
                price,
                pay_to,
                expires_at,
                nonce,
                in_market,
            });
            let name_expires = record.expires;
            let price_tier = record.price_tier;
            let nft_token_id = record.nft_token_id;
            let mut response = Response::set_state(to_bytes(&record)).event(event(
                Listed::SELECTOR_NAME,
                &Listed {
                    id,
                    price,
                    expires_at,
                    nonce,
                },
            ));
            if in_market {
                response = response.send(self::effect(
                    market_bucket_address(program, bucket_of(&id)),
                    program,
                    Effect::UpsertListing {
                        entry: MarketEntry {
                            id,
                            price,
                            pay_to,
                            expires_at,
                            name_expires,
                            price_tier,
                            nonce,
                            nft_token_id,
                            display_name,
                        },
                        now,
                    },
                ));
            } else if was_in_market {
                response = response.send(self::effect(
                    market_bucket_address(program, bucket_of(&id)),
                    program,
                    Effect::RemoveListing { id },
                ));
            }
            response
        }
        Effect::CancelListing { id, signer } => {
            let mut record = load_owned(pre_data, &signer);
            let listing = record.listing.take().expect("name is not listed for sale");
            let mut response = Response::set_state(to_bytes(&record));
            if listing.in_market {
                response = response.send(self::effect(
                    market_bucket_address(program, bucket_of(&id)),
                    program,
                    Effect::RemoveListing { id },
                ));
            }
            response
        }
        Effect::Buy {
            id,
            buyer,
            price,
            pay_to,
            expected_nonce,
            now,
        } => {
            assert!(!pre_data.is_empty(), "name is not registered");
            let mut record: NameRecord =
                borsh::from_slice(pre_data).expect("a record actor holds a valid record");
            assert!(
                !record.nft_shielded,
                "deed is shielded and cannot be bought"
            );
            assert!(buyer != record.owner, "cannot buy your own name");
            assert!(now < record.expires, "lapsed name cannot be bought");
            let listing = record.listing.take().expect("name is not listed for sale");
            assert!(listing.price == price, "sale price does not match listing");
            assert!(
                listing.pay_to == pay_to,
                "payout account does not match listing"
            );
            assert!(listing.nonce == expected_nonce, "listing has changed");
            assert!(
                listing.expires_at.is_none_or(|exp| now < exp),
                "listing has expired"
            );

            let remove_market = listing.in_market;
            let remove_site = record.site_listed.take();
            let token_id = record.nft_token_id;
            record.owner = buyer;
            record.sealed = Vec::new();
            record.subname_sale = None;
            record.generation = transfer_generation(&record.generation, &buyer);

            let mut response = Response::set_state(to_bytes(&record)).send(sync_nft_effect(
                program,
                NftAuthorityOp::Transfer {
                    token_id,
                    new_owner: buyer,
                },
            ));
            if remove_market {
                response = response.send(self::effect(
                    market_bucket_address(program, bucket_of(&id)),
                    program,
                    Effect::RemoveListing { id },
                ));
            }
            if let Some(page) = remove_site {
                response = response.send(self::effect(
                    site_bucket_address(program, bucket_of(&id), page),
                    program,
                    Effect::RemoveSite { id },
                ));
            }
            response
        }
        Effect::ShieldDeed {
            signer,
            deed_account,
            now,
        } => {
            let mut record = load_owned(pre_data, &signer);
            assert!(!record.nft_shielded, "deed is already shielded");
            assert!(
                record.listing.is_none(),
                "cancel listing before shielding deed"
            );
            assert!(
                now < record.expires,
                "lapsed name cannot be shielded; renew first"
            );
            record.nft_shielded = true;
            let token_id = record.nft_token_id;
            let expires = record.expires;
            let claimed = record.claimed;
            Response::set_state(to_bytes(&record)).send(self::effect(
                settings_address(program),
                program,
                Effect::ShieldNft {
                    token_id,
                    deed_account,
                    expires,
                    claimed,
                    now,
                },
            ))
        }
        Effect::UnshieldDeed { signer } => {
            let mut record = load_owned(pre_data, &signer);
            assert!(record.nft_shielded, "deed is not shielded");
            record.nft_shielded = false;
            let token_id = record.nft_token_id;
            Response::set_state(to_bytes(&record)).send(sync_nft_effect(
                program,
                NftAuthorityOp::Unshield { token_id },
            ))
        }
        Effect::RefreshShieldedDeed { signer } => {
            let record = load_owned(pre_data, &signer);
            assert!(record.nft_shielded, "deed is not shielded");
            Response::keep_state().send(sync_nft_effect(
                program,
                NftAuthorityOp::RefreshShielded {
                    token_id: record.nft_token_id,
                    expires: record.expires,
                },
            ))
        }
        Effect::UpsertListing { entry, now } => market::apply_upsert_listing(entry, now, pre_data),
        Effect::RemoveListing { id } => market::apply_remove_listing(id, pre_data),
        Effect::RefreshListingExpiry { id, name_expires } => {
            market::apply_refresh_listing_expiry(id, name_expires, pre_data)
        }
        Effect::UpsertSite { entry, page, now } => {
            discovery::apply_upsert_site(program, entry, page, now, pre_data)
        }
        Effect::RemoveSite { id } => discovery::apply_remove_site(program, id, pre_data),
        Effect::RefreshSiteExpiry { id, new_expires } => {
            discovery::apply_refresh_site_expiry(id, new_expires, pre_data)
        }
        Effect::RecordSiteUpsert {
            bucket_index,
            page,
            page_full,
            added,
            pruned,
        } => discovery::apply_record_site_upsert(
            bucket_index,
            page,
            page_full,
            added,
            pruned,
            pre_data,
        ),
        Effect::DecrementSiteCount { count } => {
            discovery::apply_decrement_site_count(count, pre_data)
        }
        Effect::WritePage { sealed } => Response::set_state(if sealed.is_empty() {
            Vec::new()
        } else {
            to_bytes(&Page {
                version: VERSION,
                sealed,
            })
        }),
        Effect::WritePrimary { name } => Response::set_state(if name.is_empty() {
            Vec::new()
        } else {
            to_bytes(&PrimaryName {
                version: VERSION,
                name,
            })
        }),
        Effect::Commit { now, public } => {
            if let Some(held) = load_commitment(pre_data) {
                assert!(
                    after(held.made, MAX_COMMIT_AGE_MS) <= now,
                    "this commitment was already made"
                );
            }
            Response::set_state(to_bytes(&Commitment {
                version: VERSION,
                made: now,
                public,
            }))
        }
        Effect::Reveal { now } => {
            let held = load_commitment(pre_data).expect("no commitment to reveal");
            assert!(
                after(held.made, min_commit_age_ms(held.public)) <= now,
                "the commitment is too recent; reveal it later"
            );
            assert!(
                now < after(held.made, MAX_COMMIT_AGE_MS),
                "the commitment has expired"
            );
            Response::set_state(Vec::new())
        }
        Effect::SetSubnameSale { signer, sale } => {
            let mut record = load_owned(pre_data, &signer);
            record.subname_sale = sale;
            Response::set_state(to_bytes(&record))
        }
        Effect::AuthorizeCreateSubname {
            signer,
            parent_id,
            sub_id,
            owner,
            revocable,
            sealed,
            now,
        } => {
            let parent = load_owned(pre_data, &signer);
            assert!(now < parent.expires, "parent name has lapsed");
            let kind = if revocable {
                SubnameKind::Revocable {
                    parent_generation: parent.generation,
                }
            } else {
                SubnameKind::Permanent
            };
            Response::keep_state()
                .send(self::effect(
                    subname_record_address(program, &parent_id, &sub_id),
                    program,
                    Effect::WriteSubname {
                        owner,
                        kind,
                        current_parent_generation: parent.generation,
                        sealed,
                    },
                ))
                .send(self::effect(
                    subname_page_address(program, &parent_id, &sub_id),
                    program,
                    Effect::WritePage { sealed: Vec::new() },
                ))
        }
        Effect::AuthorizeBuySubname {
            parent_id,
            sub_id,
            owner,
            sealed,
            price,
            payout,
            now,
        } => {
            assert!(!pre_data.is_empty(), "parent name is not registered");
            let parent: NameRecord =
                borsh::from_slice(pre_data).expect("a record actor holds a valid record");
            assert!(now < parent.expires, "parent name has lapsed");
            let sale = parent
                .subname_sale
                .expect("subname sales are not enabled for this parent");
            assert!(
                price == sale.price,
                "the payment does not match the parent's subname price"
            );
            if sale.price > 0 {
                assert!(
                    payout == Some(sale.payout),
                    "the payout account is not the parent's payout account"
                );
            }
            Response::keep_state()
                .send(self::effect(
                    subname_record_address(program, &parent_id, &sub_id),
                    program,
                    Effect::WriteSubname {
                        owner,
                        kind: SubnameKind::Permanent,
                        current_parent_generation: parent.generation,
                        sealed,
                    },
                ))
                .send(self::effect(
                    subname_page_address(program, &parent_id, &sub_id),
                    program,
                    Effect::WritePage { sealed: Vec::new() },
                ))
        }
        Effect::RequireParentOwner { signer, now } => {
            let parent = load_owned(pre_data, &signer);
            assert!(
                now < parent.expires.saturating_add(GRACE_MS),
                "parent name has lapsed"
            );
            Response::keep_state()
        }
        Effect::VerifyParentForSubname {
            parent_id,
            sub_id,
            now,
        } => {
            assert!(!pre_data.is_empty(), "parent name is not registered");
            let parent: NameRecord =
                borsh::from_slice(pre_data).expect("a record actor holds a valid record");
            assert!(
                now < parent.expires.saturating_add(GRACE_MS),
                "parent name has lapsed"
            );
            Response::keep_state().send(self::effect(
                subname_record_address(program, &parent_id, &sub_id),
                program,
                Effect::RequireSubnameParentGeneration {
                    parent_generation: parent.generation,
                },
            ))
        }
        Effect::WriteSubname {
            owner,
            kind,
            current_parent_generation,
            sealed,
        } => {
            if !pre_data.is_empty() {
                let held: SubnameRecord = borsh::from_slice(pre_data)
                    .expect("a subname record actor holds a valid subname record");
                match held.kind {
                    SubnameKind::Permanent => panic!("subname is already registered"),
                    SubnameKind::Revocable { parent_generation } => {
                        assert!(
                            parent_generation != current_parent_generation,
                            "subname is already registered"
                        );
                    }
                }
            }
            Response::set_state(to_bytes(&SubnameRecord {
                version: VERSION,
                owner,
                kind,
                sealed,
            }))
        }
        Effect::RevokeSubname => {
            assert!(!pre_data.is_empty(), "subname is not registered");
            let held: SubnameRecord = borsh::from_slice(pre_data)
                .expect("a subname record actor holds a valid subname record");
            assert!(
                matches!(held.kind, SubnameKind::Revocable { .. }),
                "permanent subnames cannot be revoked"
            );
            Response::set_state(Vec::new())
        }
        Effect::RequireSubnameParentGeneration { parent_generation } => {
            assert!(!pre_data.is_empty(), "subname is not registered");
            let held: SubnameRecord = borsh::from_slice(pre_data)
                .expect("a subname record actor holds a valid subname record");
            if let SubnameKind::Revocable {
                parent_generation: expected,
            } = held.kind
            {
                assert!(
                    expected == parent_generation,
                    "subname is no longer valid under the parent's current holder"
                );
            }
            Response::keep_state()
        }
        Effect::SubnameSeal {
            signer,
            sealed,
            require_permanent,
        } => {
            let mut record = load_owned_subname(pre_data, &signer, require_permanent);
            record.sealed = sealed;
            Response::set_state(to_bytes(&record))
        }
        Effect::SubnameTransfer {
            signer,
            new_owner,
            require_permanent,
        } => {
            let mut record = load_owned_subname(pre_data, &signer, require_permanent);
            record.owner = new_owner;
            record.sealed = Vec::new();
            Response::set_state(to_bytes(&record))
        }
        Effect::SubnameRemove {
            signer,
            require_permanent,
        } => {
            load_owned_subname(pre_data, &signer, require_permanent);
            Response::set_state(Vec::new())
        }
        Effect::SubnameRequireOwner {
            signer,
            require_permanent,
        } => {
            load_owned_subname(pre_data, &signer, require_permanent);
            Response::keep_state()
        }
    }
}
