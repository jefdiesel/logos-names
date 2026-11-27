//! Permissionless on-chain LNS sites directory (`SiteBucket` & `SiteDirHeader`,
//! Option A from `notes/DISCOVERY.md`, v8).
//!
//! Top-level names only for now. Discovery is on by default on clients when
//! publishing a website, with `discover=off` opt-out (`SiteDirOp::Keep` /
//! `SiteDirOp::Remove`). Bucket pages hold at most 32 entries (~3.5 KiB max),
//! paginating automatically via `SiteDirHeader.active_page`.

use borsh::{BorshDeserialize, BorshSerialize};
use lee_core::{
    account::AccountId,
    program::{PdaSeed, Response},
};

use crate::{Effect, GRACE_MS, MAX_NAME_LEN, NameId, VERSION, bucket_of, effect, sha256, to_bytes};

const SITE_DIR_HEADER_SEED_DOMAIN: &[u8] = b"logos-names/v8/sites-meta";
const SITE_BUCKET_SEED_DOMAIN: &[u8] = b"logos-names/v8/sites/";

/// Number of deterministic site directory buckets (`0..16`).
pub const SITE_BUCKETS: u8 = 16;
/// Maximum entries per bucket page (`16 * 32 = 512` sites on page 0; spills to page 1+).
pub const MAX_SITE_BUCKET_ENTRIES: usize = 32;
/// Maximum byte length of a listed site's plaintext name (top-level names only).
pub const MAX_SITE_NAME_BYTES: usize = MAX_NAME_LEN;
/// Maximum byte length of an optional site title.
pub const MAX_SITE_TITLE_BYTES: usize = 64;

/// One entry in a [`SiteBucket`] page.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SiteEntry {
    /// Top-level name's `slot_id`.
    pub id: NameId,
    /// Plaintext top-level name (`<= 32` bytes, passes [`crate::validate_label`]).
    /// Clients MUST verify `tiered_id(Argon2id(name), tier) == id` locally and drop spoofs.
    pub name: String,
    /// Optional short site title (`<= 64` bytes, no ASCII control characters).
    pub title: String,
    /// Last listed/updated timestamp in ms (bounded window).
    pub updated_at: u64,
    /// Mirror of `NameRecord.expires` (`u64::MAX` for permanent/claimed/deposit).
    pub name_expires: u64,
}

/// One paginated bucket PDA in the sites directory (`SiteBucket(bucket, page)`).
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SiteBucket {
    pub version: u8,
    pub entries: Vec<SiteEntry>,
}

/// Single metadata PDA tracking the active write page per bucket and approximate live count.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SiteDirHeader {
    pub version: u8,
    /// Current frontier write page per bucket (`0..16`). Callers may only upsert
    /// into pages `<= active_page[bucket]`, preventing pagination griefing.
    pub active_page: [u16; SITE_BUCKETS as usize],
    /// Approximate count of live entries across all buckets (updated on upsert, prune, remove).
    pub total_listed: u32,
}

/// Directory update operation attached to [`crate::Instruction::SetRecord`] and
/// [`crate::Instruction::SetPage`].
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum SiteDirOp {
    /// `0x00`: Leave the sites directory untouched (zero extra actors or gas).
    Keep,
    /// `0x01`: Add or update the name's entry in `SiteBucket(bucket_of(&id), page)`.
    Upsert {
        name: String,
        title: String,
        page: u16,
        now: u64,
    },
    /// `0x02`: Remove the name's entry from its current `SiteBucket` page (no-op if unlisted).
    Remove,
}

/// Validates an optional site directory title: at most [`MAX_SITE_TITLE_BYTES`]
/// UTF-8 bytes and no ASCII control characters (`0x00..=0x1f`, `0x7f`).
pub fn validate_site_title(title: &str) -> Result<(), &'static str> {
    if title.len() > MAX_SITE_TITLE_BYTES {
        return Err("site title must be at most 64 bytes");
    }
    if title.bytes().any(|b| b <= 0x1f || b == 0x7f) {
        return Err("site title must not contain ASCII control characters");
    }
    Ok(())
}

/// The PDA seed behind [`site_dir_header_address`].
#[must_use]
pub fn site_dir_header_seed() -> PdaSeed {
    PdaSeed::new(sha256(SITE_DIR_HEADER_SEED_DOMAIN))
}

/// The public PDA holding the [`SiteDirHeader`] metadata under `program`.
#[must_use]
pub fn site_dir_header_address(program: &AccountId) -> AccountId {
    AccountId::for_public_pda(program, &site_dir_header_seed())
}

/// The PDA seed behind [`site_bucket_address`].
#[must_use]
pub fn site_bucket_seed(bucket: u8, page: u16) -> PdaSeed {
    assert!(bucket < SITE_BUCKETS, "site bucket must be in 0..16");
    let mut bytes = Vec::with_capacity(SITE_BUCKET_SEED_DOMAIN.len() + 3);
    bytes.extend_from_slice(SITE_BUCKET_SEED_DOMAIN);
    bytes.push(bucket);
    bytes.extend_from_slice(&page.to_le_bytes());
    PdaSeed::new(sha256(&bytes))
}

/// The public PDA holding site directory bucket `bucket` (`0..16`), page `page`.
#[must_use]
pub fn site_bucket_address(program: &AccountId, bucket: u8, page: u16) -> AccountId {
    AccountId::for_public_pda(program, &site_bucket_seed(bucket, page))
}

fn load_site_bucket(pre_data: &[u8]) -> SiteBucket {
    if pre_data.is_empty() {
        SiteBucket {
            version: VERSION,
            entries: Vec::new(),
        }
    } else {
        borsh::from_slice(pre_data).expect("a site bucket actor holds a valid SiteBucket")
    }
}

fn load_site_dir_header(pre_data: &[u8]) -> SiteDirHeader {
    if pre_data.is_empty() {
        SiteDirHeader {
            version: VERSION,
            active_page: [0; SITE_BUCKETS as usize],
            total_listed: 0,
        }
    } else {
        borsh::from_slice(pre_data).expect("the site dir header actor holds a valid SiteDirHeader")
    }
}

pub(crate) fn apply_upsert_site(
    program: &AccountId,
    entry: SiteEntry,
    page: u16,
    now: u64,
    pre_data: &[u8],
) -> Response {
    let mut bucket = load_site_bucket(pre_data);
    // Lazily prune entries whose name has passed its 30-day grace period
    // (keeping active and in-grace names aligned with NameRecord.site_listed).
    let before_len = bucket.entries.len();
    bucket
        .entries
        .retain(|e| now < e.name_expires.saturating_add(GRACE_MS));
    let pruned = (before_len - bucket.entries.len()) as u32;

    let bucket_index = bucket_of(&entry.id);
    let added = if let Some(existing) = bucket.entries.iter_mut().find(|e| e.id == entry.id) {
        *existing = entry;
        0
    } else {
        assert!(
            bucket.entries.len() < MAX_SITE_BUCKET_ENTRIES,
            "site directory bucket page is full; use the next page"
        );
        bucket.entries.push(entry);
        1
    };
    let page_full = bucket.entries.len() >= MAX_SITE_BUCKET_ENTRIES;
    bucket.version = VERSION;
    Response::set_state(to_bytes(&bucket)).send(effect(
        site_dir_header_address(program),
        program,
        Effect::RecordSiteUpsert {
            bucket_index,
            page,
            page_full,
            added,
            pruned,
        },
    ))
}

pub(crate) fn apply_remove_site(program: &AccountId, id: NameId, pre_data: &[u8]) -> Response {
    if pre_data.is_empty() {
        return Response::keep_state();
    }
    let mut bucket = load_site_bucket(pre_data);
    let before = bucket.entries.len();
    bucket.entries.retain(|e| e.id != id);
    if bucket.entries.len() == before {
        return Response::keep_state();
    }
    let state = if bucket.entries.is_empty() {
        Vec::new()
    } else {
        bucket.version = VERSION;
        to_bytes(&bucket)
    };
    Response::set_state(state).send(effect(
        site_dir_header_address(program),
        program,
        Effect::DecrementSiteCount { count: 1 },
    ))
}

pub(crate) fn apply_refresh_site_expiry(
    id: NameId,
    new_expires: u64,
    pre_data: &[u8],
) -> Response {
    if pre_data.is_empty() {
        return Response::keep_state();
    }
    let mut bucket = load_site_bucket(pre_data);
    let Some(entry) = bucket.entries.iter_mut().find(|e| e.id == id) else {
        return Response::keep_state();
    };
    entry.name_expires = new_expires;
    bucket.version = VERSION;
    Response::set_state(to_bytes(&bucket))
}

pub(crate) fn apply_record_site_upsert(
    bucket_index: u8,
    page: u16,
    page_full: bool,
    added: u32,
    pruned: u32,
    pre_data: &[u8],
) -> Response {
    let mut header = load_site_dir_header(pre_data);
    let idx = usize::from(bucket_index);
    assert!(
        page <= header.active_page[idx],
        "site directory page exceeds active page"
    );
    header.total_listed = header
        .total_listed
        .saturating_add(added)
        .saturating_sub(pruned);
    if page == header.active_page[idx] && page_full {
        header.active_page[idx] = page
            .checked_add(1)
            .expect("site directory page overflow");
    }
    header.version = VERSION;
    Response::set_state(to_bytes(&header))
}

pub(crate) fn apply_decrement_site_count(count: u32, pre_data: &[u8]) -> Response {
    if pre_data.is_empty() {
        return Response::keep_state();
    }
    let mut header = load_site_dir_header(pre_data);
    header.total_listed = header.total_listed.saturating_sub(count);
    header.version = VERSION;
    Response::set_state(to_bytes(&header))
}
