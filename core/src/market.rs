//! Secondary marketplace (`List`, `Cancel`, `Buy`, `MarketBucket`) and 21-day
//! falling re-release premium for top-level LNS names (v8).

use borsh::{BorshDeserialize, BorshSerialize};
use lee_core::{
    account::AccountId,
    program::{PdaSeed, Response},
};

use crate::{DAY_MS, NameId, VERSION, sha256, to_bytes};

const MARKET_BUCKET_SEED_DOMAIN: &[u8] = b"logos-names/v8/market/";

/// Number of deterministic public marketplace directory buckets (`0..16`).
pub const MARKET_BUCKETS: u8 = 16;
/// Maximum simultaneous active directory entries per bucket (`16 * 64 = 1,024`).
pub const MAX_BUCKET_ENTRIES: usize = 64;

/// Fixed secondary-sale protocol fee divisor: `2.5% = 250 / 10,000 = 1 / 40`.
pub const SALE_FEE_DIVISOR: u128 = 40;
/// Fixed secondary-sale protocol fee in basis points (`250` = 2.5%).
pub const SALE_FEE_BPS: u16 = 250;
/// Minimum listing price (`40` base units), ensuring `fee >= 1` and `seller >= 39`.
pub const MIN_SALE_PRICE: u128 = SALE_FEE_DIVISOR;

/// Number of days over which the re-release premium decays to 0 after grace ends.
pub const PREMIUM_DAYS: u64 = 21;
/// Duration of the re-release premium window in milliseconds (21 days).
pub const PREMIUM_DURATION_MS: u64 = PREMIUM_DAYS * DAY_MS;
/// Suggested starting day-0 re-release premium (10× the placeholder 5-char
/// permanent price). Admin-adjustable via `Configure` or `SetPremium`; `0` disables.
pub const DEFAULT_PREMIUM_START: u128 = 4_000_000_000;

/// Active secondary-sale listing stored on a top-level [`crate::NameRecord`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Listing {
    /// Fixed sale price in native token base units (`>= MIN_SALE_PRICE`).
    pub price: u128,
    /// Public account receiving 97.5% (`price - fee`) in the atomic `Buy` tx.
    /// A private seller uses a fresh, unlinked admitted public account.
    pub pay_to: AccountId,
    /// Optional listing expiry in ms since Unix epoch (`None` = until name expiry).
    pub expires_at: Option<u64>,
    /// Monotonic nonce copied from `NameRecord.listing_nonce` on `List`;
    /// `Buy` commits to this value to prevent price-change / relist front-running.
    pub nonce: u64,
    /// True iff this listing was published to `MarketBucket` (`display_name.is_some()`).
    pub in_market: bool,
}

/// One public marketplace directory entry inside a [`MarketBucket`].
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MarketEntry {
    /// Top-level name's `slot_id`.
    pub id: NameId,
    pub price: u128,
    pub pay_to: AccountId,
    pub expires_at: Option<u64>,
    /// Mirror of `NameRecord.expires` (`u64::MAX` for permanent/claimed/deposit).
    pub name_expires: u64,
    pub price_tier: u8,
    pub nonce: u64,
    pub nft_token_id: [u8; 32],
    /// Optional plaintext top-level name (`<= 32` bytes). Clients MUST verify
    /// `tiered_id(Argon2id(name), price_tier) == id` locally and drop spoofs.
    pub display_name: Option<String>,
}

/// One of the 16 deterministic public marketplace directory PDAs.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MarketBucket {
    pub version: u8,
    pub entries: Vec<MarketEntry>,
}

/// Splits a secondary-sale `price` into `(seller_amount, treasury_fee)` at the
/// fixed 2.5% (`1 / 40`) treasury fee using ceiling division for the fee.
///
/// Invariants:
/// - `seller_amount + treasury_fee == price` for all `price: u128`
/// - overflow-free for all `price: u128`
/// - for `price >= MIN_SALE_PRICE`, `treasury_fee >= 1` and `seller_amount >= 39`
#[must_use]
pub const fn sale_split(price: u128) -> (u128, u128) {
    let fee = price.div_ceil(SALE_FEE_DIVISOR);
    (price - fee, fee)
}

/// Re-release premium `elapsed_ms` after a lapsed name's 30-day grace period
/// ends (`now - (expires + GRACE_MS)`).
///
/// Halves every 24 hours, linearly interpolated within each day, reaching `0`
/// continuously at `PREMIUM_DURATION_MS` (21 days). Pure integer arithmetic,
/// monotonically non-increasing, and overflow-free for all `u128` inputs.
#[must_use]
pub const fn rerelease_premium(premium_start: u128, elapsed_ms: u64) -> u128 {
    if premium_start == 0 || elapsed_ms >= PREMIUM_DURATION_MS {
        return 0;
    }
    let day = elapsed_ms / DAY_MS; // 0..21
    let rem = (elapsed_ms % DAY_MS) as u128;
    let day_ms = DAY_MS as u128;
    let hi = premium_start >> day;
    let lo = if day + 1 == PREMIUM_DAYS {
        0
    } else {
        premium_start >> (day + 1)
    };
    let span = hi - lo;
    let drop = match span.checked_mul(rem) {
        Some(prod) => prod / day_ms,
        None => (span / day_ms) * rem,
    };
    hi - drop
}

/// Deterministic bucket index (`0..16`) for a top-level name's `slot_id`.
/// Shared by both `MarketBucket` and `SiteBucket`.
#[must_use]
pub const fn bucket_of(id: &NameId) -> u8 {
    id[0] & 0x0f
}

/// The PDA seed behind [`market_bucket_address`].
#[must_use]
pub fn market_bucket_seed(bucket: u8) -> PdaSeed {
    assert!(bucket < MARKET_BUCKETS, "market bucket must be in 0..16");
    let mut bytes = Vec::with_capacity(MARKET_BUCKET_SEED_DOMAIN.len() + 1);
    bytes.extend_from_slice(MARKET_BUCKET_SEED_DOMAIN);
    bytes.push(bucket);
    PdaSeed::new(sha256(&bytes))
}

/// The public PDA holding marketplace directory bucket `bucket` (`0..16`).
#[must_use]
pub fn market_bucket_address(program: &AccountId, bucket: u8) -> AccountId {
    AccountId::for_public_pda(program, &market_bucket_seed(bucket))
}

fn load_market_bucket(pre_data: &[u8]) -> MarketBucket {
    if pre_data.is_empty() {
        MarketBucket {
            version: VERSION,
            entries: Vec::new(),
        }
    } else {
        borsh::from_slice(pre_data).expect("a market bucket actor holds a valid MarketBucket")
    }
}

pub(crate) fn apply_upsert_listing(entry: MarketEntry, now: u64, pre_data: &[u8]) -> Response {
    let mut bucket = load_market_bucket(pre_data);
    // Lazily prune expired listings and lapsed names.
    bucket
        .entries
        .retain(|e| now < e.name_expires && e.expires_at.is_none_or(|exp| now < exp));
    if let Some(existing) = bucket.entries.iter_mut().find(|e| e.id == entry.id) {
        *existing = entry;
    } else {
        assert!(
            bucket.entries.len() < MAX_BUCKET_ENTRIES,
            "market directory bucket is full"
        );
        bucket.entries.push(entry);
    }
    bucket.version = VERSION;
    Response::set_state(to_bytes(&bucket))
}

pub(crate) fn apply_remove_listing(id: NameId, pre_data: &[u8]) -> Response {
    if pre_data.is_empty() {
        return Response::keep_state();
    }
    let mut bucket = load_market_bucket(pre_data);
    let before = bucket.entries.len();
    bucket.entries.retain(|e| e.id != id);
    if bucket.entries.len() == before {
        return Response::keep_state();
    }
    if bucket.entries.is_empty() {
        Response::set_state(Vec::new())
    } else {
        bucket.version = VERSION;
        Response::set_state(to_bytes(&bucket))
    }
}

pub(crate) fn apply_refresh_listing_expiry(
    id: NameId,
    name_expires: u64,
    pre_data: &[u8],
) -> Response {
    if pre_data.is_empty() {
        return Response::keep_state();
    }
    let mut bucket = load_market_bucket(pre_data);
    let Some(entry) = bucket.entries.iter_mut().find(|e| e.id == id) else {
        return Response::keep_state();
    };
    entry.name_expires = name_expires;
    bucket.version = VERSION;
    Response::set_state(to_bytes(&bucket))
}
