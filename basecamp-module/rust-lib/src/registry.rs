//! Reads a Logos Names registry on LEZ 0.4, following SPEC.md. `changes.rs`
//! builds on it to change one.
//!
//! This is a second implementation of what `core/` and `seal/` define, kept
//! separate because a module build sees only its own directory and cannot take
//! the LEZ crates as dependencies. The shared test vectors keep them in step:
//! every layout and derivation here is checked against bytes `core` and `seal`
//! produce.

use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use argon2::{Algorithm, Argon2, Params, Version};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};

/// How names are shown: bare, `jef` (SPEC 2).
pub const NAME_SUFFIX: &str = "";
/// The text entry that points a name to another (SPEC.md §7, Forwarding).
pub const FORWARD_KEY: &str = "forward";
/// The text entry that opts a site out of the public sites directory (`discover=off`).
pub const DISCOVER_KEY: &str = "discover";
/// The old suffix, still accepted and removed when typed or pasted.
pub const LEGACY_SUFFIX: &str = ".lez";
/// Current registry layout version (LEZ 0.4, v8).
pub const VERSION: u8 = 8;
const MAX_NAME_LEN: usize = 32;
pub const MAX_SUBNAME_LEN: usize = 62;
pub const MAX_SITE_TITLE_BYTES: usize = 64;
const PDA_PREFIX: &[u8] = b"/LEE/v0.2/AccountId/PDA/";
/// One fixed salt for every network, on purpose: a name keeps its identifier
/// and key from testnet to mainnet.
const DERIVATION_SALT: &[u8] = b"logos-names/v5";
/// Argon2id memory in KiB (64 MiB), passes and lanes.
const DERIVATION_MEMORY_KIB: u32 = 65_536;
const DERIVATION_PASSES: u32 = 3;
const DERIVATION_LANES: u32 = 1;
const ID_DOMAIN: &[u8] = b"logos-names/v5/id";
const KEY_DOMAIN: &[u8] = b"logos-names/v5/key";
const NAME_SEED_DOMAIN: &[u8] = b"logos-names/v5/name/";
const PAGE_SEED_DOMAIN: &[u8] = b"logos-names/v5/page/";
const SETTINGS_SEED: &[u8] = b"logos-names/v5/config";
const OWNER_TAG_DOMAIN: &[u8] = b"logos-names/v5/owner";
const COMMITMENT_DOMAIN: &[u8] = b"logos-names/v5/commitment";
const COMMITMENT_SEED_DOMAIN: &[u8] = b"logos-names/v5/commit/";
const DEPOSIT_SEED_DOMAIN: &[u8] = b"logos-names/v5/deposit/";
const CLAIM_LEAF_DOMAIN: &[u8] = b"logos-names/v6/claim-leaf";
const TIER_ID_DOMAIN: &[u8] = b"logos-names/v7/tier/";
const SUBNAME_SEED_DOMAIN: &[u8] = b"logos-names/v7/subname/";
const SUBPAGE_SEED_DOMAIN: &[u8] = b"logos-names/v7/subpage/";
const SUBNAME_COMMITMENT_DOMAIN: &[u8] = b"logos-names/v7/sub-commitment";
const GEN_REGISTER_DOMAIN: &[u8] = b"logos-names/v7/gen/register";
const NFT_TOKEN_ID_DOMAIN: &[u8] = b"logos-names/v8/nft-token/";
const NFT_TOKEN_SEED_DOMAIN: &[u8] = b"lez-nft/v3/token/";
const NFT_CONFIG_SEED_DOMAIN: &[u8] = b"lez-nft/v3/config";
const MARKET_BUCKET_SEED_DOMAIN: &[u8] = b"logos-names/v8/market/";
const SITE_DIR_HEADER_SEED_DOMAIN: &[u8] = b"logos-names/v8/sites-meta";
const SITE_BUCKET_SEED_DOMAIN: &[u8] = b"logos-names/v8/sites/";
pub const DAY_MS: u64 = 24 * 60 * 60 * 1000;
/// After a fee-mode name expires, only its holder can renew it for this long.
/// Past that it reads as unregistered and anyone may register it.
pub const GRACE_MS: u64 = 30 * DAY_MS;
/// How long each finite fee-mode term holds a name, in milliseconds, by term index.
pub const TERM_LENGTHS_MS: [u64; 3] = [30 * DAY_MS, 365 * DAY_MS, 5 * 365 * DAY_MS];
pub const TERMS: usize = 4;
pub const TERM_30D: u8 = 0;
pub const TERM_1Y: u8 = 1;
pub const TERM_5Y: u8 = 2;
pub const TERM_PERMANENT: u8 = 3;
pub const PRICE_TIERS: usize = 3;
pub const PRICE_TIER_5: u8 = 0;
pub const PRICE_TIER_6_7: u8 = 1;
pub const PRICE_TIER_8_PLUS: u8 = 2;
/// 21-day falling re-release premium window after grace ends.
pub const PREMIUM_DAYS: u64 = 21;
pub const PREMIUM_DURATION_MS: u64 = PREMIUM_DAYS * DAY_MS;
/// Fixed secondary-sale protocol fee: 2.5% = 1/40 (`price.div_ceil(40)`).
pub const SALE_FEE_DIVISOR: u128 = 40;
pub const SALE_FEE_BPS: u16 = 250;
pub const MIN_SALE_PRICE: u128 = SALE_FEE_DIVISOR;
pub const MARKET_BUCKETS: u8 = 16;
pub const SITE_BUCKETS: u8 = 16;
/// The registry charges a fee per term, paid to its treasury; names lapse
/// (unless registered or renewed with [`TERM_PERMANENT`]).
pub const MODE_FEE: u8 = 0;
/// The registry takes a refundable deposit into the name's deposit account;
/// names never lapse.
pub const MODE_DEPOSIT: u8 = 1;
/// The expiry of a name held by a deposit: it never lapses.
pub const NEVER: u64 = u64::MAX;
/// A private commit or reveal is valid only in a block whose timestamp is in
/// `[now, now + TIME_WINDOW_MS)`.
pub const TIME_WINDOW_MS: u64 = 10 * 60 * 1000;
/// A public commit (`CommitPublic`) or public reveal is valid only in a block whose
/// timestamp is in `[now, now + PUBLIC_TIME_WINDOW_MS)`.
pub const PUBLIC_TIME_WINDOW_MS: u64 = 60 * 1000;
/// How long a private commitment must wait before its reveal.
pub const MIN_COMMIT_AGE_MS: u64 = 15 * 60 * 1000;
/// How long a public commitment (`CommitPublic`) must wait before its reveal.
pub const PUBLIC_MIN_COMMIT_AGE_MS: u64 = 5 * 60 * 1000;
/// How long a commitment can still be revealed.
pub const MAX_COMMIT_AGE_MS: u64 = 24 * 60 * 60 * 1000;
/// The account the chain keeps its clock on, and the program whose state on
/// it holds the time: `{block_id: u64, timestamp: u64}`, in milliseconds.
const CLOCK_ACCOUNT: &AccountId = b"/LEZ/ClockProgramAccount/0000000";
const RECORD_SEAL: &[u8] = b"logos-names/v5/record";
const PAGE_SEAL: &[u8] = b"logos-names/v5/page";
const NONCE_LEN: usize = 12;
const TAG_LEN: usize = 16;
/// A sealed profile's plaintext is padded to a multiple of this, so its
/// length says little about what is in it.
const PROFILE_PADDING: usize = 256;
/// The most sealed bytes a record holds; the program refuses more.
pub const MAX_SEALED_RECORD_LEN: usize = 12 * 1024;
/// The most sealed bytes a page holds; the program refuses more.
pub const MAX_SEALED_PAGE_LEN: usize = 80 * 1024;
/// The largest sequencer reply read: one actor's state, a page at most, as JSON.
const MAX_REPLY_LEN: usize = 1024 * 1024;
/// ML-KEM-768 encapsulation key, as used for LEZ private accounts.
pub const VPK_LEN: usize = 1184;

pub type AccountId = [u8; 32];
pub type NameId = [u8; 32];
/// What a record keeps in place of its holder's account: [`owner_tag`].
pub type OwnerTag = [u8; 32];

/// Turns what a person typed, such as `Jef`, `@jef`, `lns:jef`, `lns://blog.jef` or `jef.lez`,
/// into the bare name.
pub fn normalize_name(input: &str) -> String {
    let lowered = input.trim().to_ascii_lowercase();
    let bare = lowered
        .strip_prefix("lns://")
        .or_else(|| lowered.strip_prefix("lns:"))
        .unwrap_or(&lowered);
    let bare = bare.strip_prefix('@').unwrap_or(bare);
    bare.strip_suffix(LEGACY_SUFFIX).unwrap_or(bare).to_owned()
}

pub fn is_valid_label(label: &str) -> bool {
    let bytes = label.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= MAX_NAME_LEN
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
        && bytes[0] != b'-'
        && bytes[bytes.len() - 1] != b'-'
        && !bytes.windows(2).any(|w| w == b"--")
}

/// Splits a valid one-level subname `label.parent` into `(label, parent)`.
pub fn split_subname(name: &str) -> Option<(&str, &str)> {
    if name.len() > MAX_SUBNAME_LEN {
        return None;
    }
    let (label, parent) = name.split_once('.')?;
    if !is_valid_label(label) || !is_valid_label(parent) {
        return None;
    }
    Some((label, parent))
}

pub fn is_valid_name(name: &str) -> bool {
    if name.contains('.') {
        split_subname(name).is_some()
    } else {
        is_valid_label(name)
    }
}

/// Normalises typed input and returns the bare name (top-level or one-level subname),
/// or why it is not a name. The chain never sees a name, so this check is the only one there is.
pub fn parse_name(input: &str) -> Result<String, String> {
    let name = normalize_name(input);
    if is_valid_name(&name) && parse_account(&name).is_err() {
        // A name that is also an account ID would be ambiguous in a "send to" field (SPEC 2).
        Ok(name)
    } else if is_valid_name(&name) {
        Err(format!("{name} is also an account ID, so it can't be a name"))
    } else {
        Err("a name is 1-32 characters of a-z, 0-9 or '-' (without '--'), or a subname like blog.jef".to_owned())
    }
}

/// Normalises typed input and requires a top-level name (not a subname).
pub fn parse_top_level_name(input: &str) -> Result<String, String> {
    let name = parse_name(input)?;
    if name.contains('.') {
        return Err("a top-level name has no dot (1-32 characters of a-z, 0-9 or '-', such as jef)".to_owned());
    }
    Ok(name)
}

pub fn parse_account(text: &str) -> Result<AccountId, String> {
    let text = text.trim();
    let bare = text
        .strip_prefix("Public/")
        .or_else(|| text.strip_prefix("Private/"))
        .unwrap_or(text);
    bs58::decode(bare)
        .into_vec()
        .ok()
        .and_then(|bytes| AccountId::try_from(bytes).ok())
        .ok_or_else(|| format!("not an account ID: {text}"))
}

/// The ID typed after an account's `Public/` or `Private/`: an ID and nothing else, so
/// `Private/Public/<id>` is refused rather than read as `Private/<id>`.
pub fn bare_account(id: &str) -> Result<AccountId, String> {
    let id = id.trim();
    if id.contains('/') || id.contains(char::is_whitespace) {
        return Err("that isn't an account ID: write Public/ or Private/ once, then the ID".to_owned());
    }
    parse_account(id)
}

/// Refuses an ID no one can hold, or one that was most likely mangled on the way: the
/// all-zero account (`1111…`), and an ID whose letters are all one case (base58 IDs mix
/// both, so this is an ID re-typed in lower or upper case, which may still decode).
/// For hand-overs, which can't be undone.
pub fn plausible_holder(id: &str, account: &AccountId) -> Result<(), String> {
    if account.iter().all(|byte| *byte == 0) {
        return Err("that is the empty account (all 1s), which no one holds".to_owned());
    }
    let letters: Vec<char> = id.chars().filter(char::is_ascii_alphabetic).collect();
    if letters.len() >= 8 && (letters.iter().all(char::is_ascii_lowercase) || letters.iter().all(char::is_ascii_uppercase)) {
        return Err("that account ID is all one case, so it was probably changed when it was copied (IDs mix upper and lower case); paste it exactly as it was given".to_owned());
    }
    Ok(())
}

pub fn account_text(account: &AccountId) -> String {
    bs58::encode(account).into_string()
}

fn sha256(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize().into()
}

/// What a name turns into: the identifier the chain knows it by, and the key
/// that opens and seals what it resolves to. The key never leaves the client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Derived {
    pub id: NameId,
    pub key: [u8; 32],
}

/// Derives a name's identifier and key (SPEC.md section 3):
/// `master = Argon2id(name, "logos-names/v5", 64 MiB, 3 passes, 1 lane)`, then
/// `id = SHA-256("logos-names/v5/id" ‖ master)` and
/// `key = SHA-256("logos-names/v5/key" ‖ master)`. Slow and memory-hard on
/// purpose, so that scanning for names is expensive; callers that ask
/// repeatedly should keep the result.
pub fn derive(name: &str) -> Derived {
    let master = master(name);
    Derived {
        id: sha256(&[ID_DOMAIN, &master]),
        key: sha256(&[KEY_DOMAIN, &master]),
    }
}

fn master(name: &str) -> [u8; 32] {
    let params = Params::new(DERIVATION_MEMORY_KIB, DERIVATION_PASSES, DERIVATION_LANES, Some(32))
        .expect("the Argon2 parameters are valid");
    let mut out = [0_u8; 32];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(name.as_bytes(), DERIVATION_SALT, &mut out)
        .expect("Argon2 accepts these inputs");
    out
}

/// LEZ's public program-derived address for `seed` under `program`.
pub fn pda(program: &AccountId, seed: &[u8; 32]) -> AccountId {
    let mut prefix = [0_u8; 32];
    prefix[..PDA_PREFIX.len()].copy_from_slice(PDA_PREFIX);
    sha256(&[&prefix, program, seed])
}

/// The seed of a name's record. A transaction that may create a registry account
/// lists its seed as evidence, so each account's seed is here beside its address.
pub fn name_seed(id: &NameId) -> [u8; 32] {
    sha256(&[NAME_SEED_DOMAIN, id])
}

pub fn page_seed(id: &NameId) -> [u8; 32] {
    sha256(&[PAGE_SEED_DOMAIN, id])
}

pub fn deposit_seed(id: &NameId) -> [u8; 32] {
    sha256(&[DEPOSIT_SEED_DOMAIN, id])
}

pub fn commitment_seed(commitment: &[u8; 32]) -> [u8; 32] {
    sha256(&[COMMITMENT_SEED_DOMAIN, commitment])
}

pub fn settings_seed() -> [u8; 32] {
    sha256(&[SETTINGS_SEED])
}

pub fn record_address(program: &AccountId, id: &NameId) -> AccountId {
    pda(program, &name_seed(id))
}

pub fn page_address(program: &AccountId, id: &NameId) -> AccountId {
    pda(program, &page_seed(id))
}

/// The account holding a name's deposit, as its native balance. Only the
/// registry can move it, on release.
pub fn deposit_address(program: &AccountId, id: &NameId) -> AccountId {
    pda(program, &deposit_seed(id))
}

/// The account holding the commitment `commitment`.
pub fn commitment_address(program: &AccountId, commitment: &[u8; 32]) -> AccountId {
    pda(program, &commitment_seed(commitment))
}

/// The account holding the registry's settings.
pub fn settings_address(program: &AccountId) -> AccountId {
    pda(program, &settings_seed())
}

/// The tag that stands for `account` as the holder of the name `id`. A record
/// stores this, never the account, so one account's names stay unlinked.
pub fn owner_tag(id: &NameId, account: &AccountId) -> OwnerTag {
    sha256(&[OWNER_TAG_DOMAIN, id, account])
}

/// What a claim commits to: the name's identifier, the tag of the account
/// that will hold it, and a secret the committer picks at random.
pub fn commitment(id: &NameId, holder: &OwnerTag, secret: &[u8; 32]) -> [u8; 32] {
    sha256(&[COMMITMENT_DOMAIN, id, holder, secret])
}

/// The leaf of a name on the claims list: its record's address on the earlier
/// chain, under the settings' `claims_program`, and its holder's tag.
#[cfg(test)]
pub fn claim_leaf(old_record: &AccountId, holder: &OwnerTag) -> [u8; 32] {
    sha256(&[CLAIM_LEAF_DOMAIN, old_record, holder])
}

/// Pricing tier for a top-level name of length `len`:
/// `0` for <=5 chars, `1` for 6-7 chars, `2` for 8+ chars.
pub const fn price_tier_for_len(len: usize) -> u8 {
    if len <= 5 {
        PRICE_TIER_5
    } else if len <= 7 {
        PRICE_TIER_6_7
    } else {
        PRICE_TIER_8_PLUS
    }
}

/// On-chain v7/v8 slot identifier for a top-level name: `SHA-256("logos-names/v7/tier/" || tier || base_id)`.
pub fn tiered_id(base_id: &NameId, price_tier: u8) -> NameId {
    assert!(usize::from(price_tier) < PRICE_TIERS, "invalid price tier");
    sha256(&[TIER_ID_DOMAIN, &[price_tier], base_id])
}

/// The on-chain identifier used for records/pages/ownership/commitments:
/// `tiered_id(base_id, price_tier_for_len(name.len()))` for top-level names,
/// and `base_id` unchanged for subnames (`label.parent`).
pub fn slot_id(name: &str, base_id: &NameId) -> NameId {
    if split_subname(name).is_some() {
        *base_id
    } else {
        tiered_id(base_id, price_tier_for_len(name.len()))
    }
}

pub fn subname_record_seed(parent_id: &NameId, sub_id: &NameId) -> [u8; 32] {
    sha256(&[SUBNAME_SEED_DOMAIN, parent_id, sub_id])
}

pub fn subname_page_seed(parent_id: &NameId, sub_id: &NameId) -> [u8; 32] {
    sha256(&[SUBPAGE_SEED_DOMAIN, parent_id, sub_id])
}

pub fn subname_record_address(program: &AccountId, parent_id: &NameId, sub_id: &NameId) -> AccountId {
    pda(program, &subname_record_seed(parent_id, sub_id))
}

pub fn subname_page_address(program: &AccountId, parent_id: &NameId, sub_id: &NameId) -> AccountId {
    pda(program, &subname_page_seed(parent_id, sub_id))
}

pub fn subname_commitment(parent_id: &NameId, sub_id: &NameId, holder: &OwnerTag, secret: &[u8; 32]) -> [u8; 32] {
    sha256(&[SUBNAME_COMMITMENT_DOMAIN, parent_id, sub_id, holder, secret])
}

pub fn register_generation(slot: &NameId, owner: &OwnerTag, now: u64, secret: &[u8; 32]) -> [u8; 32] {
    sha256(&[GEN_REGISTER_DOMAIN, slot, owner, &now.to_le_bytes(), secret])
}

pub fn nft_token_id(slot: &NameId, mint_generation: &[u8; 32]) -> [u8; 32] {
    sha256(&[NFT_TOKEN_ID_DOMAIN, slot, mint_generation])
}

pub fn nft_token_seed(token_id: &[u8; 32]) -> [u8; 32] {
    sha256(&[NFT_TOKEN_SEED_DOMAIN, token_id])
}

/// The seed of `lez_nft::config_address(nft)`, declared by every Register that mints.
pub fn nft_config_seed() -> [u8; 32] {
    sha256(&[NFT_CONFIG_SEED_DOMAIN])
}

pub fn nft_token_address(nft_program: &AccountId, token_id: &[u8; 32]) -> AccountId {
    pda(nft_program, &nft_token_seed(token_id))
}

pub const fn bucket_of(id: &NameId) -> u8 {
    id[0] & 0x0f
}

pub fn market_bucket_seed(bucket: u8) -> [u8; 32] {
    assert!(bucket < MARKET_BUCKETS, "market bucket out of range");
    sha256(&[MARKET_BUCKET_SEED_DOMAIN, &[bucket]])
}

pub fn market_bucket_address(program: &AccountId, bucket: u8) -> AccountId {
    pda(program, &market_bucket_seed(bucket))
}

pub fn site_dir_header_seed() -> [u8; 32] {
    sha256(&[SITE_DIR_HEADER_SEED_DOMAIN])
}

pub fn site_dir_header_address(program: &AccountId) -> AccountId {
    pda(program, &site_dir_header_seed())
}

pub fn site_bucket_seed(bucket: u8, page: u16) -> [u8; 32] {
    assert!(bucket < SITE_BUCKETS, "site bucket out of range");
    sha256(&[SITE_BUCKET_SEED_DOMAIN, &[bucket], &page.to_le_bytes()])
}

pub fn site_bucket_address(program: &AccountId, bucket: u8, page: u16) -> AccountId {
    pda(program, &site_bucket_seed(bucket, page))
}

/// Splits a secondary-sale `price` into `(seller_amount, treasury_fee)` at the fixed
/// 2.5% (`1 / 40`) treasury fee using ceiling division for the fee.
pub const fn sale_split(price: u128) -> (u128, u128) {
    let fee = price.div_ceil(SALE_FEE_DIVISOR);
    (price - fee, fee)
}

/// 21-day falling re-release premium `elapsed_ms` after a lapsed name's 30-day grace
/// period ends. Halves every 24 hours with linear interpolation within each day.
pub const fn rerelease_premium(premium_start: u128, elapsed_ms: u64) -> u128 {
    if premium_start == 0 || elapsed_ms >= PREMIUM_DURATION_MS {
        return 0;
    }
    let day = elapsed_ms / DAY_MS;
    let rem = (elapsed_ms % DAY_MS) as u128;
    let day_ms = DAY_MS as u128;
    let hi = premium_start >> day;
    let lo = if day + 1 == PREMIUM_DAYS { 0 } else { premium_start >> (day + 1) };
    let span = hi - lo;
    let drop = match span.checked_mul(rem) {
        Some(prod) => prod / day_ms,
        None => (span / day_ms) * rem,
    };
    hi - drop
}

pub fn parse_term(text: &str) -> Result<u8, String> {
    match text.trim().to_ascii_lowercase().as_str() {
        "30d" | "30" | "month" => Ok(TERM_30D),
        "1y" | "1yr" | "year" | "" => Ok(TERM_1Y),
        "5y" | "5yr" | "5years" => Ok(TERM_5Y),
        "permanent" | "perm" | "forever" => Ok(TERM_PERMANENT),
        other => Err(format!("term must be 30d, 1y, 5y, or permanent, not {other:?}")),
    }
}

pub fn term_label(term: u8) -> &'static str {
    match term {
        TERM_30D => "30d",
        TERM_1Y => "1y",
        TERM_5Y => "5y",
        TERM_PERMANENT => "permanent",
        _ => "1y",
    }
}

pub fn validate_site_title(title: &str) -> Result<(), String> {
    if title.len() > MAX_SITE_TITLE_BYTES {
        return Err(format!("site title must be at most {MAX_SITE_TITLE_BYTES} bytes"));
    }
    if title.bytes().any(|b| b <= 0x1f || b == 0x7f) {
        return Err("site title must not contain control characters".to_owned());
    }
    Ok(())
}

pub fn discover_is_off(profile: &Profile) -> bool {
    profile
        .texts
        .iter()
        .find(|(k, _)| k == DISCOVER_KEY)
        .is_some_and(|(_, v)| v.trim().eq_ignore_ascii_case("off"))
}

pub fn site_title_for(profile: &Profile) -> String {
    let raw = profile
        .texts
        .iter()
        .find(|(k, _)| k == "display")
        .map(|(_, v)| v.trim())
        .unwrap_or("");
    let cleaned: String = raw.chars().filter(|c| !c.is_control()).collect();
    let mut out = String::new();
    for ch in cleaned.chars() {
        if out.len() + ch.len_utf8() > MAX_SITE_TITLE_BYTES {
            break;
        }
        out.push(ch);
    }
    out
}

/// The ID of the chain's clock system program (`AccountId::from_system_program_name("clock")`).
fn clock_program() -> AccountId {
    crate::deposit::system_program(b"clock")
}

/// This machine's clock, in milliseconds since the Unix epoch.
pub fn local_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| u64::try_from(since.as_millis()).unwrap_or(u64::MAX))
}

/// The registry's settings: who administers it, and how names are paid for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub admin: AccountId,
    /// Where fees go in fee mode. It pays nothing itself.
    pub treasury: AccountId,
    /// The fee for each `[price_tier][term]` in fee mode (3 tiers × 4 terms: 30d, 1y, 5y, permanent).
    pub prices: [[u128; TERMS]; PRICE_TIERS],
    /// [`MODE_FEE`] or [`MODE_DEPOSIT`].
    pub mode: u8,
    /// The deposit for a normal name and for a three-character name, in deposit mode.
    pub deposits: [u128; 2],
    /// The root of the list of names that can be claimed from the earlier
    /// chain; all zeros when there is none.
    pub claims_root: [u8; 32],
    /// Claims close at this time, in milliseconds. Until then a claim overrides
    /// a registration.
    pub claims_until: u64,
    /// The registry on the earlier chain the list was taken from.
    pub claims_program: AccountId,
    /// Day-0 re-release premium after a lapsed name's 30-day grace ends (0 disables).
    pub premium_start: u128,
    /// Standalone `lez_nft` v3 program ID bound on initial `Configure` (all zeros if unbound).
    pub nft_program: AccountId,
}

impl Settings {
    /// Decodes v5, v6, v7, or v8 on-chain settings bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let mut reader = Reader { bytes };
        let version = reader.u8()?;
        let admin = reader.account()?;
        let treasury = reader.account()?;
        let mut prices = [[0_u128; TERMS]; PRICE_TIERS];
        if version < 7 {
            let mut flat = [0_u128; 3];
            for price in &mut flat {
                *price = reader.u128()?;
            }
            for row in &mut prices {
                row[0] = flat[0];
                row[1] = flat[1];
                row[2] = flat[2];
                row[3] = 0;
            }
        } else {
            for row in &mut prices {
                for price in row {
                    *price = reader.u128()?;
                }
            }
        }
        let mode = reader.u8()?;
        let mut deposits = [0_u128; 2];
        for deposit in &mut deposits {
            *deposit = reader.u128()?;
        }
        // Version 5 (the registry on the LEZ 0.3 testnet) has no claims list.
        let (claims_root, claims_until, claims_program) =
            if version < 6 { ([0; 32], 0, [0; 32]) } else { (reader.account()?, reader.u64()?, reader.account()?) };
        let (premium_start, nft_program) = if version < 8 {
            (0, [0; 32])
        } else {
            (reader.u128()?, reader.account()?)
        };
        Ok(Self {
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
        })
    }

    pub fn price_for(&self, price_tier: u8, term: u8) -> u128 {
        self.prices
            .get(usize::from(price_tier))
            .and_then(|row| row.get(usize::from(term)))
            .copied()
            .unwrap_or(0)
    }

    /// Whether names from the earlier chain can still be claimed at `now`.
    pub fn claims_open(&self, now: u64) -> bool {
        self.claims_root != [0; 32] && now < self.claims_until
    }

    pub fn deposit_mode(&self) -> bool {
        self.mode == MODE_DEPOSIT
    }

    pub fn mode_text(settings: Option<&Self>) -> &'static str {
        match settings {
            Some(settings) if settings.deposit_mode() => "deposit",
            Some(_) => "fee",
            None => "unconfigured",
        }
    }
}

/// Subname primary sale settings stored on a parent name's record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubnameSale {
    pub price: u128,
    pub payout: AccountId,
}

/// Active secondary-sale listing stored on a top-level name record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Listing {
    pub price: u128,
    pub pay_to: AccountId,
    pub expires_at: Option<u64>,
    pub nonce: u64,
    pub in_market: bool,
}

/// Whether a subname is permanent or revocable by its parent's holder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubnameKind {
    Permanent,
    Revocable { parent_generation: [u8; 32] },
}

/// A registered one-level subname (`label.parent`), stored at `subname_record_address`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubnameRecord {
    pub version: u8,
    pub owner: OwnerTag,
    pub kind: SubnameKind,
    pub sealed: Vec<u8>,
}

impl SubnameRecord {
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let mut reader = Reader { bytes };
        let version = reader.u8()?;
        let owner = reader.account()?;
        let kind = match reader.u8()? {
            0 => SubnameKind::Permanent,
            1 => SubnameKind::Revocable { parent_generation: reader.account()? },
            _ => return Err("unknown subname kind".to_owned()),
        };
        let sealed = reader.byte_vec()?.to_vec();
        Ok(Self { version, owner, kind, sealed })
    }
}

/// Public marketplace directory entry inside a `MarketBucket`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketEntry {
    pub id: NameId,
    pub price: u128,
    pub pay_to: AccountId,
    pub expires_at: Option<u64>,
    pub name_expires: u64,
    pub price_tier: u8,
    pub nonce: u64,
    pub nft_token_id: [u8; 32],
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketBucket {
    pub version: u8,
    pub entries: Vec<MarketEntry>,
}

impl MarketBucket {
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let mut reader = Reader { bytes };
        let version = reader.u8()?;
        let count = reader.u32()?;
        let mut entries = Vec::with_capacity(count.min(128));
        for _ in 0..count {
            let id = reader.account()?;
            let price = reader.u128()?;
            let pay_to = reader.account()?;
            let expires_at = reader.optional_u64()?;
            let name_expires = reader.u64()?;
            let price_tier = reader.u8()?;
            let nonce = reader.u64()?;
            let nft_token_id = reader.account()?;
            let display_name = reader.optional_string()?;
            entries.push(MarketEntry {
                id,
                price,
                pay_to,
                expires_at,
                name_expires,
                price_tier,
                nonce,
                nft_token_id,
                display_name,
            });
        }
        Ok(Self { version, entries })
    }
}

/// One entry in a `SiteBucket` page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteEntry {
    pub id: NameId,
    pub name: String,
    pub title: String,
    pub updated_at: u64,
    pub name_expires: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteBucket {
    pub version: u8,
    pub entries: Vec<SiteEntry>,
}

impl SiteBucket {
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let mut reader = Reader { bytes };
        let version = reader.u8()?;
        let count = reader.u32()?;
        let mut entries = Vec::with_capacity(count.min(64));
        for _ in 0..count {
            let id = reader.account()?;
            let name = reader.string()?;
            let title = reader.string()?;
            let updated_at = reader.u64()?;
            let name_expires = reader.u64()?;
            entries.push(SiteEntry { id, name, title, updated_at, name_expires });
        }
        Ok(Self { version, entries })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteDirHeader {
    pub version: u8,
    pub active_page: [u16; SITE_BUCKETS as usize],
    pub total_listed: u32,
}

impl SiteDirHeader {
    pub fn empty() -> Self {
        Self { version: VERSION, active_page: [0; SITE_BUCKETS as usize], total_listed: 0 }
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let mut reader = Reader { bytes };
        let version = reader.u8()?;
        let mut active_page = [0_u16; SITE_BUCKETS as usize];
        for page in &mut active_page {
            *page = reader.u16()?;
        }
        let total_listed = reader.u32_val()?;
        Ok(Self { version, active_page, total_listed })
    }
}

/// Directory update operation attached to v8 `SetRecord` and `SetPage`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SiteDirOp {
    Keep,
    Upsert { name: String, title: String, page: u16, now: u64 },
    Remove,
}

impl SiteDirOp {
    pub fn encode(&self, writer: &mut Writer) {
        match self {
            Self::Keep => writer.u8(0),
            Self::Upsert { name, title, page, now } => {
                writer.u8(1);
                writer.string(name);
                writer.string(title);
                writer.u16(*page);
                writer.u64(*now);
            }
            Self::Remove => writer.u8(2),
        }
    }
}

/// Minimal borsh reader for the registry's layouts.
struct Reader<'bytes> {
    bytes: &'bytes [u8],
}

impl<'bytes> Reader<'bytes> {
    fn take(&mut self, length: usize) -> Result<&'bytes [u8], String> {
        if length > self.bytes.len() {
            return Err("the record is truncated".to_owned());
        }
        let (head, rest) = self.bytes.split_at(length);
        self.bytes = rest;
        Ok(head)
    }

    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().expect("two bytes")))
    }

    fn u32_val(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("four bytes")))
    }

    fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().expect("eight bytes")))
    }

    fn u128(&mut self) -> Result<u128, String> {
        Ok(u128::from_le_bytes(self.take(16)?.try_into().expect("16 bytes")))
    }

    fn u32(&mut self) -> Result<usize, String> {
        Ok(self.u32_val()? as usize)
    }

    fn account(&mut self) -> Result<AccountId, String> {
        Ok(self.take(32)?.try_into().expect("32 bytes"))
    }

    fn byte_vec(&mut self) -> Result<&'bytes [u8], String> {
        let length = self.u32()?;
        self.take(length)
    }

    fn string(&mut self) -> Result<String, String> {
        String::from_utf8(self.byte_vec()?.to_vec()).map_err(|_| "the record holds invalid text".to_owned())
    }

    fn is_some(&mut self) -> Result<bool, String> {
        Ok(self.u8()? != 0)
    }

    fn optional_u16(&mut self) -> Result<Option<u16>, String> {
        if self.is_some()? { Ok(Some(self.u16()?)) } else { Ok(None) }
    }

    fn optional_u64(&mut self) -> Result<Option<u64>, String> {
        if self.is_some()? { Ok(Some(self.u64()?)) } else { Ok(None) }
    }

    fn optional_string(&mut self) -> Result<Option<String>, String> {
        if self.is_some()? { Ok(Some(self.string()?)) } else { Ok(None) }
    }
}

/// Minimal borsh writer, the reader's counterpart.
#[derive(Default)]
pub struct Writer(pub Vec<u8>);

impl Writer {
    pub fn u8(&mut self, value: u8) {
        self.0.push(value);
    }

    pub fn u16(&mut self, value: u16) {
        self.0.extend(value.to_le_bytes());
    }

    pub fn u32(&mut self, value: u32) {
        self.0.extend(value.to_le_bytes());
    }

    pub fn u64(&mut self, value: u64) {
        self.0.extend(value.to_le_bytes());
    }

    pub fn u128(&mut self, value: u128) {
        self.0.extend(value.to_le_bytes());
    }

    pub fn bool(&mut self, value: bool) {
        self.0.push(u8::from(value));
    }

    pub fn bytes(&mut self, bytes: &[u8]) {
        self.u32(bytes.len() as u32);
        self.0.extend(bytes);
    }

    pub fn string(&mut self, text: &str) {
        self.bytes(text.as_bytes());
    }

    pub fn optional(&mut self, value: Option<&[u8; 32]>) {
        match value {
            None => self.0.push(0),
            Some(value) => {
                self.0.push(1);
                self.0.extend(value);
            }
        }
    }

    pub fn optional_u64(&mut self, value: Option<u64>) {
        match value {
            None => self.0.push(0),
            Some(value) => {
                self.0.push(1);
                self.u64(value);
            }
        }
    }

    pub fn optional_string(&mut self, value: Option<&str>) {
        match value {
            None => self.0.push(0),
            Some(value) => {
                self.0.push(1);
                self.string(value);
            }
        }
    }
}

/// What a name resolves to, as its holder sealed it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Profile {
    pub name: String,
    pub account: Option<AccountId>,
    pub npk: Option<[u8; 32]>,
    pub vpk: Vec<u8>,
    pub texts: Vec<(String, String)>,
}

impl Profile {
    /// The profile of a name with nothing set.
    pub fn bare(name: &str) -> Self {
        Self { name: name.to_owned(), ..Self::default() }
    }

    /// Decodes a profile from the start of `bytes`; padding after it is ignored.
    fn decode(bytes: &[u8]) -> Result<Self, String> {
        let mut reader = Reader { bytes };
        let name = reader.string()?;
        let account = if reader.is_some()? { Some(reader.account()?) } else { None };
        let npk = if reader.is_some()? { Some(reader.account()?) } else { None };
        let vpk = reader.byte_vec()?.to_vec();
        let count = reader.u32()?;
        let mut texts = Vec::new();
        for _ in 0..count {
            texts.push((reader.string()?, reader.string()?));
        }
        Ok(Self { name, account, npk, vpk, texts })
    }

    /// The profile's borsh bytes, zero-padded to a multiple of [`PROFILE_PADDING`].
    fn encode(&self) -> Vec<u8> {
        let mut writer = Writer::default();
        writer.string(&self.name);
        writer.optional(self.account.as_ref());
        writer.optional(self.npk.as_ref());
        writer.bytes(&self.vpk);
        writer.u32(self.texts.len() as u32);
        for (key, value) in &self.texts {
            writer.string(key);
            writer.string(value);
        }
        let padded = writer.0.len().div_ceil(PROFILE_PADDING) * PROFILE_PADDING;
        writer.0.resize(padded, 0);
        writer.0
    }

    /// How many bytes the sealed profile takes in its record.
    pub fn sealed_len(&self) -> usize {
        NONCE_LEN + self.encode().len() + TAG_LEN
    }

    /// Seals the profile under the name's key.
    pub fn seal(&self, key: &[u8; 32]) -> Result<Vec<u8>, String> {
        seal(key, &self.encode(), RECORD_SEAL, fresh_nonce()?)
    }

    /// Opens a record's sealed bytes as the profile of `name`. Empty bytes,
    /// bytes this key does not open, and a profile of another name all mean
    /// the same thing: the name has nothing set.
    pub fn open(key: &[u8; 32], sealed: &[u8], name: &str) -> Self {
        // The program never stores more; a reply that has more didn't come from it.
        if sealed.len() > MAX_SEALED_RECORD_LEN {
            return Self::bare(name);
        }
        open(key, sealed, RECORD_SEAL)
            .and_then(|plaintext| Self::decode(&plaintext).ok())
            .filter(|profile| profile.name == name)
            .unwrap_or_else(|| Self::bare(name))
    }
}

/// Seals a page under the name's key.
pub fn seal_page(key: &[u8; 32], content_type: &str, body: &[u8]) -> Result<Vec<u8>, String> {
    let mut writer = Writer::default();
    writer.string(content_type);
    writer.bytes(body);
    seal(key, &writer.0, PAGE_SEAL, fresh_nonce()?)
}

/// Opens a sealed page into the JSON an app needs to show it. `text` is the
/// body when it is valid UTF-8, and null otherwise.
pub fn open_page(key: &[u8; 32], sealed: &[u8]) -> Option<Value> {
    if sealed.len() > MAX_SEALED_PAGE_LEN {
        return None;
    }
    let plaintext = open(key, sealed, PAGE_SEAL)?;
    let mut reader = Reader { bytes: &plaintext };
    let content_type = reader.string().ok()?;
    let body = reader.byte_vec().ok()?;
    Some(json!({
        "content_type": content_type,
        "bytes": body.len(),
        "text": std::str::from_utf8(body).ok(),
    }))
}

pub fn fresh_bytes<const N: usize>() -> Result<[u8; N], String> {
    let mut bytes = [0_u8; N];
    getrandom::getrandom(&mut bytes).map_err(|_| "this machine gave no random bytes".to_owned())?;
    Ok(bytes)
}

fn fresh_nonce() -> Result<[u8; NONCE_LEN], String> {
    fresh_bytes()
}

/// `nonce ‖ AES-256-GCM(key, nonce, plaintext, associated data)`.
fn seal(key: &[u8; 32], plaintext: &[u8], associated: &[u8], nonce: [u8; NONCE_LEN]) -> Result<Vec<u8>, String> {
    let cipher = Aes256Gcm::new_from_slice(key).expect("a 32-byte key");
    let sealed = cipher
        .encrypt(Nonce::from_slice(&nonce), Payload { msg: plaintext, aad: associated })
        .map_err(|_| "could not seal".to_owned())?;
    Ok([nonce.as_slice(), &sealed].concat())
}

fn open(key: &[u8; 32], sealed: &[u8], associated: &[u8]) -> Option<Vec<u8>> {
    if sealed.len() < NONCE_LEN + TAG_LEN {
        return None;
    }
    let (nonce, ciphertext) = sealed.split_at(NONCE_LEN);
    Aes256Gcm::new_from_slice(key)
        .expect("a 32-byte key")
        .decrypt(Nonce::from_slice(nonce), Payload { msg: ciphertext, aad: associated })
        .ok()
}

/// A registered name (top-level or subname): the tag of who holds it, until when,
/// its v8 metadata (pricing tier, generation, NFT, listing, subname sales, site listing),
/// and its profile opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub version: u8,
    /// The holder's [`owner_tag`] for this name; the chain stores no account.
    pub owner: OwnerTag,
    /// When the name lapses, in milliseconds since the Unix epoch, or
    /// [`NEVER`] for a permanent/deposit/claimed name.
    pub expires: u64,
    pub profile: Profile,
    /// Carried over from the earlier chain, so it can't be claimed again.
    pub claimed: bool,
    pub price_tier: u8,
    pub generation: [u8; 32],
    pub subname_sale: Option<SubnameSale>,
    pub nft_token_id: [u8; 32],
    pub nft_shielded: bool,
    pub listing: Option<Listing>,
    pub listing_nonce: u64,
    pub site_listed: Option<u16>,
    /// `"permanent"` or `"revocable"` for a subname, `None` for a top-level name.
    pub subname_kind: Option<String>,
}

impl Resolved {
    /// Constructs a minimal `Resolved` (used by unit tests and v5/v6 fixtures).
    #[cfg(test)]
    pub fn simple(owner: OwnerTag, expires: u64, profile: Profile, claimed: bool) -> Self {
        let price_tier = price_tier_for_len(profile.name.len().max(1));
        Self {
            version: 6,
            owner,
            expires,
            profile,
            claimed,
            price_tier,
            generation: [0; 32],
            subname_sale: None,
            nft_token_id: [0; 32],
            nft_shielded: false,
            listing: None,
            listing_nonce: 0,
            site_listed: None,
            subname_kind: None,
        }
    }

    /// Decodes an on-chain top-level record (v5, v6, v7, or v8) and opens its sealed profile.
    pub fn decode(bytes: &[u8], key: &[u8; 32], name: &str) -> Result<Self, String> {
        let mut reader = Reader { bytes };
        let version = reader.u8()?;
        let owner = reader.account()?;
        let expires = reader.u64()?;
        let sealed = reader.byte_vec()?;
        // Version 5 records (the LEZ 0.3 testnet) end at `sealed`: nothing was claimed there.
        let claimed = version >= 6 && reader.u8()? != 0;
        let (price_tier, generation, subname_sale) = if version >= 7 {
            let price_tier = reader.u8()?;
            let generation = reader.account()?;
            let subname_sale = if reader.is_some()? {
                Some(SubnameSale { price: reader.u128()?, payout: reader.account()? })
            } else {
                None
            };
            (price_tier, generation, subname_sale)
        } else {
            (price_tier_for_len(name.len()), [0; 32], None)
        };
        let (nft_token_id, nft_shielded, listing, listing_nonce, site_listed) = if version >= 8 {
            let nft_token_id = reader.account()?;
            let nft_shielded = reader.u8()? != 0;
            let listing = if reader.is_some()? {
                Some(Listing {
                    price: reader.u128()?,
                    pay_to: reader.account()?,
                    expires_at: reader.optional_u64()?,
                    nonce: reader.u64()?,
                    in_market: reader.u8()? != 0,
                })
            } else {
                None
            };
            let listing_nonce = reader.u64()?;
            let site_listed = reader.optional_u16()?;
            (nft_token_id, nft_shielded, listing, listing_nonce, site_listed)
        } else {
            ([0; 32], false, None, 0, None)
        };
        Ok(Self {
            version,
            owner,
            expires,
            profile: Profile::open(key, sealed, name),
            claimed,
            price_tier,
            generation,
            subname_sale,
            nft_token_id,
            nft_shielded,
            listing,
            listing_nonce,
            site_listed,
            subname_kind: None,
        })
    }

    /// Whether the name is permanent or held by a deposit, and so never lapses.
    pub fn never_expires(&self) -> bool {
        self.expires == NEVER
    }

    /// Whether `account` holds the name with identifier `id`.
    pub fn is_held_by(&self, id: &NameId, account: &AccountId) -> bool {
        self.owner == owner_tag(id, account)
    }

    /// Whether `account` holds `name` (using `slot_id` on v7/v8, or `derived.id` on v5/v6).
    pub fn is_held_by_name(&self, name: &str, derived: &Derived, account: &AccountId) -> bool {
        let slot = slot_id(name, &derived.id);
        self.owner == owner_tag(&slot, account) || (self.version < 7 && self.owner == owner_tag(&derived.id, account))
    }

    /// Whether the name has expired and is in (or past) its grace period.
    pub fn lapsed(&self, now: u64) -> bool {
        !self.never_expires() && self.expires <= now
    }

    /// Whether the name is currently inside its 30-day grace period.
    pub fn in_grace(&self, now: u64) -> bool {
        self.lapsed(now) && !self.gone(now)
    }

    /// When the grace period ends (for finite-term names).
    pub fn grace_ends(&self) -> Option<u64> {
        (!self.never_expires()).then(|| self.expires.saturating_add(GRACE_MS))
    }

    /// Milliseconds elapsed since grace ended, if the name is past grace.
    pub fn rerelease_elapsed(&self, now: u64) -> Option<u64> {
        let end = self.grace_ends()?;
        (now >= end).then(|| now - end)
    }

    /// Whether the name is past its grace period, and so counts as unregistered.
    pub fn gone(&self, now: u64) -> bool {
        !self.never_expires() && now >= self.expires.saturating_add(GRACE_MS)
    }

    /// The expiry as JSON: null for a name that never expires.
    pub fn expires_json(&self) -> Value {
        if self.never_expires() {
            Value::Null
        } else {
            Value::from(self.expires)
        }
    }

    /// The name this one forwards to (SPEC.md §7, Forwarding): its `forward` entry, if
    /// that is a valid name other than this one. A client follows it one hop.
    pub fn forward_target(&self) -> Option<String> {
        let (_, value) = self.profile.texts.iter().find(|(key, _)| key == FORWARD_KEY)?;
        parse_name(value).ok().filter(|target| *target != self.profile.name)
    }

    pub fn to_json(&self) -> Value {
        self.to_json_with(None, local_now())
    }

    pub fn to_json_with(&self, settings: Option<&Settings>, now: u64) -> Value {
        let profile = &self.profile;
        let texts: Map<String, Value> = profile
            .texts
            .iter()
            .map(|(key, value)| (key.clone(), Value::from(value.clone())))
            .collect();
        let has_nft = self.nft_token_id != [0; 32];
        let nft_program = settings.map(|s| s.nft_program).filter(|p| *p != [0; 32]);
        let nft_json = if has_nft {
            json!({
                "token_id": hex(&self.nft_token_id),
                "token_account": nft_program.as_ref().map(|p| account_text(&nft_token_address(p, &self.nft_token_id))),
                "program": nft_program.as_ref().map(account_text),
                "shielded": self.nft_shielded,
            })
        } else {
            Value::Null
        };
        let listing_json = self.listing.as_ref().map(|listing| {
            let (seller_receives, fee) = sale_split(listing.price);
            json!({
                "price": listing.price.to_string(),
                "fee": fee.to_string(),
                "seller_receives": seller_receives.to_string(),
                "pay_to": account_text(&listing.pay_to),
                "expires_at": listing.expires_at,
                "nonce": listing.nonce,
                "in_market": listing.in_market,
            })
        });
        let subname_sale_json = self.subname_sale.as_ref().map(|sale| {
            json!({
                "price": sale.price.to_string(),
                "payout": account_text(&sale.payout),
            })
        });
        let subname_parent = split_subname(&profile.name).map(|(_, parent)| parent.to_owned());
        json!({
            "name": profile.name,
            "display": format!("{}{NAME_SUFFIX}", profile.name),
            "owner_tag": hex(&self.owner),
            "expires": self.expires_json(),
            "never": self.never_expires(),
            "permanent": self.never_expires(),
            "lapsed": self.lapsed(now),
            "in_grace": self.in_grace(now),
            "grace_ends": self.grace_ends(),
            "claimed": self.claimed,
            "price_tier": self.price_tier,
            "account": profile.account.as_ref().map(account_text),
            "npk": profile.npk.as_ref().map(|npk| hex(npk)),
            "vpk": (!profile.vpk.is_empty()).then(|| hex(&profile.vpk)),
            "texts": texts,
            "forward": self.forward_target(),
            "discover_off": discover_is_off(profile),
            "site_listed": self.site_listed,
            "nft": nft_json,
            "listing": listing_json,
            "listing_nonce": self.listing_nonce,
            "subname_sale": subname_sale_json,
            "subname": self.subname_kind.is_some(),
            "subname_kind": self.subname_kind,
            "parent": subname_parent,
        })
    }

    /// Where to send a payment for this name: privately, publicly, or neither.
    pub fn recipient_json(&self) -> Value {
        let profile = &self.profile;
        let private = profile.npk.as_ref().filter(|_| !profile.vpk.is_empty()).map(|npk| {
            json!({
                "nullifier_public_key": hex(npk),
                "viewing_public_key": hex(&profile.vpk),
            })
        });
        json!({
            "name": profile.name,
            "display": format!("{}{NAME_SUFFIX}", profile.name),
            "public": profile.account.as_ref().map(account_text),
            "public_hex": profile.account.as_ref().map(|account| hex(account)),
            "private": private,
            "lapsed": self.lapsed(local_now()),
        })
    }
}

/// Decodes a reverse record, returning the claimed name.
pub fn decode_primary(bytes: &[u8]) -> Result<String, String> {
    let mut reader = Reader { bytes };
    let _version = reader.u8()?;
    reader.string()
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The program whose state on an account is its balance, as 16 little-endian bytes.
pub const NATIVE_TOKEN: AccountId = [0; 32];

/// The state of one actor as a sequencer's `getAccountView` reports it: a list of
/// bytes under the program's ID, or nothing.
fn actor_bytes(account: &Value, program: &AccountId) -> Result<Vec<u8>, String> {
    // A LEZ 0.3 sequencer (the public testnet until it moves to 0.4) answers the same call
    // with every program's shard under `shards`, in the same byte-list form.
    let data = &account["data"];
    let id = account_text(program);
    match data["actor_states"][&id].as_array().or_else(|| data["shards"][&id].as_array()) {
        None => Ok(Vec::new()),
        Some(values) => values
            .iter()
            .map(|value| {
                value
                    .as_u64()
                    .and_then(|byte| u8::try_from(byte).ok())
                    .ok_or_else(|| "the sequencer sent malformed account data".to_owned())
            })
            .collect(),
    }
}

/// An account's native balance, from its state; 0 when it has none.
fn native_balance(state: &[u8]) -> u128 {
    <[u8; 16]>::try_from(state).map_or(0, u128::from_le_bytes)
}

/// Reads hex, with or without a `0x` prefix.
pub fn unhex(text: &str) -> Result<Vec<u8>, String> {
    let text = text.trim();
    let digits = text.strip_prefix("0x").unwrap_or(text);
    if digits.len() % 2 != 0 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("not hexadecimal".to_owned());
    }
    (0..digits.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&digits[at..at + 2], 16).map_err(|_| "not hexadecimal".to_owned()))
        .collect()
}

/// The Logos Blockchain channel LEZ takes deposits on, as the sequencer names it
/// (`getChannelId`): 64 hex characters.
pub fn channel_id(sequencer: &str) -> Result<String, String> {
    channel_id_within(sequencer, Duration::from_secs(15))
}

/// `channel_id`, waiting at most `wait` for the answer. The channel is also what tells
/// one LEZ zone from another (`zones`).
#[cfg(not(test))]
pub fn channel_id_within(sequencer: &str, wait: Duration) -> Result<String, String> {
    let response = ureq::post(sequencer)
        .timeout(wait)
        .send_json(json!({ "jsonrpc": "2.0", "id": 1, "method": "getChannelId", "params": {} }))
        .map_err(|error| format!("could not reach LEZ to ask where deposits go: {error}"))?;
    let reply: Value = response.into_json().map_err(|_| "LEZ's reply about deposits was unreadable".to_owned())?;
    reply["result"]
        .as_str()
        .filter(|id| id.len() == 64 && id.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| "LEZ didn't say where deposits go. Try again later.".to_owned())
}

/// How long one module call may spend reading the sequencer, all its requests together:
/// the host gives a call 20 s and drops the answer after that, so a read past it is no
/// answer at all (resolve-slow-sequencer-exceeds-host-timeout-no-result).
#[cfg(not(test))]
pub const READ_BUDGET: Duration = Duration::from_secs(15);
#[cfg(test)]
pub const READ_BUDGET: Duration = Duration::from_secs(4);
/// The longest one request waits, within the call's budget.
const REQUEST_CAP: Duration = Duration::from_secs(10);
/// How long a health check (`getLastBlockId`) waits: a sequencer that takes longer to say
/// its last block is not answering.
pub const PROBE_WAIT: Duration = Duration::from_secs(5);

/// When a call that reads the sequencer started and when it must have its answer: every read
/// it makes shares this one deadline, and a read that runs out says how long the call waited
/// in all, not what was left for its last request (resolve-under-slow-says-6-s-after-15).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Deadline {
    pub start: Instant,
    pub at: Instant,
}

impl Deadline {
    /// `budget` from now.
    pub fn after(budget: Duration) -> Self {
        let start = Instant::now();
        Deadline { start, at: start + budget }
    }

    /// The time left.
    pub fn left(&self) -> Duration {
        self.at.saturating_duration_since(Instant::now())
    }

    /// Whether the time is up.
    pub fn passed(&self) -> bool {
        Instant::now() >= self.at
    }

    /// How long the call has waited so far.
    pub fn spent(&self) -> Duration {
        self.start.elapsed()
    }

    /// What a read says when the time ran out: how long the call waited.
    pub fn too_slow(&self) -> String {
        too_slow(self.spent())
    }
}

/// What a call says when its time for the sequencer ran out after `waited`.
pub fn too_slow(waited: Duration) -> String {
    // To the nearest second: a wait of 4.98 s of the probe's 5 is said as 5, and a call
    // that gave up a moment after its 15 s as 15.
    format!("could not reach the sequencer: it didn't answer within {} s. Try again shortly", ((waited.as_millis() + 500) / 1000).max(1))
}

/// How long the next request may wait before `deadline`, or why there is no time left.
pub fn request_wait(deadline: Deadline) -> Result<Duration, String> {
    let left = deadline.left();
    if left < Duration::from_millis(300) {
        return Err(deadline.too_slow());
    }
    Ok(left.min(REQUEST_CAP))
}

/// A request to the sequencer that failed, in words: no answer in time, no connection, or
/// an HTTP error status (a proxy's 503) instead of a JSON-RPC reply.
fn request_problem(error: ureq::Error, deadline: Deadline) -> String {
    match error {
        ureq::Error::Status(code, _) => format!("could not reach the sequencer: it answered HTTP {code} instead of a reading"),
        ureq::Error::Transport(transport) => {
            let text = transport.to_string();
            if text.contains("timed out") || text.contains("deadline") || text.contains("WouldBlock") {
                deadline.too_slow()
            } else {
                format!("could not reach the sequencer: {text}")
            }
        }
    }
}

/// The `result` of a JSON-RPC reply, or why the reply isn't one: an error reply, a body
/// that isn't JSON-RPC (a page of HTML, garbage, nothing) or one without a result. A
/// `null` result is a result (an account no transaction has touched).
pub fn rpc_result(body: &[u8]) -> Result<Value, String> {
    let reply: Value = serde_json::from_slice(body).map_err(|_| "could not reach the sequencer: it sent a reply that isn't JSON-RPC".to_owned())?;
    if let Some(error) = reply.get("error").filter(|error| !error.is_null()) {
        let message = error["message"].as_str().map_or_else(|| error.to_string(), str::to_owned);
        return Err(format!("the sequencer refused the read: {message}"));
    }
    reply.get("result").cloned().ok_or_else(|| "could not reach the sequencer: its reply carries no result".to_owned())
}

/// Sends `method` to `sequencer` and returns the reply's `result`, waiting until
/// `deadline` at most (see `request_wait`); every failure is said in words, never read as
/// an empty answer.
pub fn rpc(sequencer: &str, method: &str, params: Value, deadline: Deadline) -> Result<Value, String> {
    let wait = request_wait(deadline)?;
    let response = ureq::post(sequencer)
        .timeout(wait)
        .send_json(json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }))
        .map_err(|error| request_problem(error, deadline))?;
    // Read at most MAX_REPLY_LEN: a sequencer that sends more is refused, not believed.
    let mut body = Vec::new();
    std::io::Read::read_to_end(&mut std::io::Read::take(response.into_reader(), MAX_REPLY_LEN as u64 + 1), &mut body)
        .map_err(|error| if error.to_string().contains("timed out") { deadline.too_slow() } else { format!("could not reach the sequencer: its reply broke off: {error}") })?;
    if body.len() > MAX_REPLY_LEN {
        return Err("the sequencer sent a reply larger than any account".to_owned());
    }
    rpc_result(&body)
}

/// Whether the sequencer at `sequencer` answers honestly: `getLastBlockId` with a block
/// number, within `PROBE_WAIT`. A 503 from a proxy, a 200 page of HTML, an empty or error
/// reply are no answer (balance-marks-written-from-503-false-arrivals).
#[cfg(not(test))]
pub fn sequencer_reachable(sequencer: &str) -> Result<(), String> {
    probe_sequencer(sequencer)
}

/// Asks `sequencer` for its last block, and is Ok only for a JSON-RPC reply carrying one.
/// Its own wait is said as the health check's: a call that read for longer before it gave up
/// never says the sequencer had only 5 s (job-start-says-5-s-after-9).
pub fn probe_sequencer(sequencer: &str) -> Result<(), String> {
    let result = rpc(sequencer, "getLastBlockId", json!([]), Deadline::after(PROBE_WAIT)).map_err(|why| {
        if why.starts_with("could not reach the sequencer: it didn't answer within") {
            format!("could not reach the sequencer: it didn't answer a health check within {} s. Try again shortly", PROBE_WAIT.as_secs())
        } else {
            why
        }
    })?;
    result.as_u64().map(|_| ()).ok_or_else(|| "could not reach the sequencer: it sent no block number".to_owned())
}

/// Whether the sequencer reads the registry's record of `name` (`getAccountView`) by
/// `deadline`, as every job about a name does first: a sequencer that says its last block
/// but fails account reads can't run the job either.
#[cfg(not(test))]
pub fn name_readable(sequencer: &str, program: &AccountId, name: &str, deadline: Deadline) -> Result<(), String> {
    Registry::by(sequencer, *program, deadline).record(name, &derive(name)).map(|_| ())
}

/// Tests say whether the sequencer answers: never the network.
#[cfg(test)]
pub fn name_readable(sequencer: &str, _program: &AccountId, _name: &str, deadline: Deadline) -> Result<(), String> {
    stub_wait(deadline)?;
    sequencer_reachable(sequencer)
}

/// Whether the sequencer reads accounts (`getAccountView` of `account`) by `deadline`: a
/// sequencer can say its last block and fail every account read, and no wallet job runs
/// then (cli-receive-all-starts-with-only-the-probe-answering).
#[cfg(not(test))]
pub fn account_readable(sequencer: &str, account: &AccountId, deadline: Deadline) -> Result<(), String> {
    view_readable(sequencer, account, deadline)
}

/// `account_readable`, from the network: the account's view, any reading (null too).
pub fn view_readable(sequencer: &str, account: &AccountId, deadline: Deadline) -> Result<(), String> {
    let params = json!([{ "account_id": account_text(account), "program_account_id": account_text(&NATIVE_TOKEN) }]);
    rpc(sequencer, "getAccountView", params, deadline).map(|_| ())
}

/// Tests say whether account reads answer (`ACCOUNTS_UNREADABLE`): never the network.
#[cfg(test)]
pub fn account_readable(_sequencer: &str, _account: &AccountId, deadline: Deadline) -> Result<(), String> {
    stub_wait(deadline)?;
    if ACCOUNTS_UNREADABLE.with(std::cell::Cell::get) {
        return Err("could not reach the sequencer: it answered HTTP 503 instead of a reading".to_owned());
    }
    Ok(())
}

#[cfg(test)]
thread_local! {
    /// Whether a test's sequencer fails every account read (and still says its last block).
    pub static ACCOUNTS_UNREADABLE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// How long each of a test's stand-in reads takes: a slow sequencer.
    pub static STUB_DELAY: std::cell::Cell<Duration> = const { std::cell::Cell::new(Duration::ZERO) };
}

/// A test's stand-in read: it takes `STUB_DELAY`, and fails as a real read does when
/// `deadline` comes first.
#[cfg(test)]
pub fn stub_wait(deadline: Deadline) -> Result<(), String> {
    let delay = STUB_DELAY.with(std::cell::Cell::get);
    if delay >= deadline.left() {
        std::thread::sleep(deadline.left());
        return Err(deadline.too_slow());
    }
    std::thread::sleep(delay);
    Ok(())
}

/// The block number in a `getLastBlockId` reply, or why the reply isn't one.
pub fn last_block_answer(body: &str) -> Result<u64, String> {
    let reply: Value = serde_json::from_str(body.trim())
        .map_err(|_| "it sent a reply that isn't JSON-RPC".to_owned())?;
    if let Some(error) = reply.get("error").filter(|error| !error.is_null()) {
        let message = error["message"].as_str().map_or_else(|| error.to_string(), str::to_owned);
        return Err(format!("it answered with an error: {message}"));
    }
    reply["result"].as_u64().ok_or_else(|| "it sent no block number".to_owned())
}

/// An HTTP server on loopback that answers each of its first `connections` requests with
/// `reply` (status line, headers and body): a stand-in sequencer for tests.
#[cfg(test)]
pub fn stub_http(reply: &'static str, connections: usize) -> String {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("binds");
    let address = listener.local_addr().expect("address");
    std::thread::spawn(move || {
        for stream in listener.incoming().take(connections) {
            let Ok(mut stream) = stream else { continue };
            // The whole request is read first: closing on unread bytes resets the
            // connection, and the client would see that instead of the reply.
            let mut request = Vec::new();
            let mut buffer = [0_u8; 4096];
            loop {
                let Ok(read) = stream.read(&mut buffer) else { break };
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
                let text = String::from_utf8_lossy(&request).to_ascii_lowercase();
                if let Some(end) = text.find("\r\n\r\n") {
                    let length = text.lines().find_map(|line| line.strip_prefix("content-length:"))
                        .and_then(|value| value.trim().parse::<usize>().ok()).unwrap_or(0);
                    if request.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            let _ = stream.write_all(reply.as_bytes());
            let _ = stream.flush();
        }
    });
    format!("http://{address}/")
}

/// A stand-in sequencer on loopback that answers every request (until the test ends) with
/// what `answer` makes of its JSON-RPC body: an HTTP status and a body. It may sleep first,
/// to be slow. A degraded sequencer, as a proxy in front of one shows it, for tests.
#[cfg(test)]
pub fn stub_rpc(answer: impl Fn(&Value) -> (u16, String) + Send + Sync + 'static) -> String {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("binds");
    let address = listener.local_addr().expect("address");
    let answer = std::sync::Arc::new(answer);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let answer = answer.clone();
            std::thread::spawn(move || {
                let mut request = Vec::new();
                let mut buffer = [0_u8; 4096];
                let body = loop {
                    let Ok(read) = stream.read(&mut buffer) else { return };
                    if read == 0 {
                        return;
                    }
                    request.extend_from_slice(&buffer[..read]);
                    let text = String::from_utf8_lossy(&request).to_ascii_lowercase();
                    if let Some(end) = text.find("\r\n\r\n") {
                        let length = text.lines().find_map(|line| line.strip_prefix("content-length:")).and_then(|value| value.trim().parse::<usize>().ok()).unwrap_or(0);
                        if request.len() >= end + 4 + length {
                            break request[end + 4..end + 4 + length].to_vec();
                        }
                    }
                };
                let (status, reply) = answer(&serde_json::from_slice(&body).unwrap_or(Value::Null));
                let head = format!("HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", reply.len());
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(reply.as_bytes());
                let _ = stream.flush();
            });
        }
    });
    format!("http://{address}/")
}

/// A JSON-RPC reply carrying `result`, for `stub_rpc`.
#[cfg(test)]
pub fn rpc_ok(result: Value) -> (u16, String) {
    (200, json!({ "jsonrpc": "2.0", "id": 1, "result": result }).to_string())
}

/// What `getAccountView` answers for an account holding `balance` (and nothing else).
#[cfg(test)]
pub fn view_holding(balance: u128) -> Value {
    json!({ "nonce": 0, "data": { "actor_states": { account_text(&NATIVE_TOKEN): balance.to_le_bytes().to_vec() } } })
}

/// The account a `getAccountView` request asks about.
#[cfg(test)]
pub fn asked_account(request: &Value) -> String {
    request["params"][0]["account_id"].as_str().unwrap_or_default().to_owned()
}

/// A `200 OK` HTTP reply carrying `body` as JSON, for `stub_http`.
#[cfg(test)]
pub fn json_reply(body: &str) -> &'static str {
    Box::leak(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).into_boxed_str())
}

#[cfg(test)]
thread_local! {
    /// The name records a test's registry holds, by name identifier.
    pub static RECORDS: std::cell::RefCell<Vec<(NameId, Resolved)>> = const { std::cell::RefCell::new(Vec::new()) };
}

#[cfg(test)]
thread_local! {
    /// Whether a test's sequencer is down.
    pub static SEQUENCER_DOWN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Tests say whether the sequencer answers: never the network.
#[cfg(test)]
pub fn sequencer_reachable(sequencer: &str) -> Result<(), String> {
    if SEQUENCER_DOWN.with(std::cell::Cell::get) {
        Err(format!("could not reach the sequencer: {sequencer}: Connection Failed"))
    } else {
        Ok(())
    }
}

/// Tests know the channel, never the network: the one a test gave that sequencer
/// (`CHANNELS`), or a fixed one.
#[cfg(test)]
pub fn channel_id_within(sequencer: &str, _wait: Duration) -> Result<String, String> {
    CHANNELS_ASKED.with(|asked| asked.set(asked.get() + 1));
    CHANNELS
        .with(|known| known.borrow().iter().find(|(url, _)| url == sequencer).map(|(_, channel)| channel.clone()))
        .unwrap_or_else(|| Ok("02".repeat(32)))
}

#[cfg(test)]
thread_local! {
    /// What a test's sequencers say their channel is, by address: a channel, or why not.
    pub static CHANNELS: std::cell::RefCell<Vec<(String, Result<String, String>)>> = const { std::cell::RefCell::new(Vec::new()) };
    /// How many times a channel was asked for.
    pub static CHANNELS_ASKED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Which LEZ the sequencer at `sequencer` runs (`deposit::lez_of`), so a deposit carries
/// metadata it decodes. An answer that tells neither is refused (`deposit::UNKNOWN_LEZ`):
/// a deposit can't be undone, so it is never made on a guess.
#[cfg(not(test))]
pub fn lez_version(sequencer: &str) -> Result<crate::deposit::Lez, String> {
    let request = json!({ "jsonrpc": "2.0", "id": 1, "method": crate::deposit::PROBE_METHOD, "params": [crate::deposit::PROBE_ACCOUNT] });
    let deadline = Deadline::after(REQUEST_CAP);
    let response = match ureq::post(sequencer).timeout(REQUEST_CAP).send_json(request) {
        Ok(response) | Err(ureq::Error::Status(_, response)) => response,
        Err(error) => return Err(request_problem(error, deadline)),
    };
    let reply: Value = response.into_json().unwrap_or(Value::Null);
    crate::deposit::lez_of(&reply).ok_or_else(|| crate::deposit::UNKNOWN_LEZ.to_owned())
}

/// Tests say which LEZ answers (`LEZ`; `None` tells neither).
#[cfg(test)]
pub fn lez_version(_sequencer: &str) -> Result<crate::deposit::Lez, String> {
    LEZ.with(std::cell::Cell::get).ok_or_else(|| crate::deposit::UNKNOWN_LEZ.to_owned())
}

#[cfg(test)]
thread_local! {
    /// 0.3 unless a test sets it: the bridge tests about notes, fees and history were written
    /// against 0.3's metadata (the account's bytes, needing no key); the 0.4 ones set it.
    pub static LEZ: std::cell::Cell<Option<crate::deposit::Lez>> = const { std::cell::Cell::new(Some(crate::deposit::Lez::V03)) };
}

/// Whether the sequencer at `sequencer` has ever seen `account`: `getAccountView`
/// answers null for an account no transaction has touched. LEZ can't pay such an
/// account of another wallet by its ID (the send lands, keeps its fee and credits
/// nothing), and a hand-over to one is most likely to a mistyped ID.
/// Read by `deadline` (the asking call's).
#[cfg(not(test))]
pub fn account_seen(sequencer: &str, account: &AccountId, deadline: Deadline) -> Result<bool, String> {
    let params = json!([{ "account_id": account_text(account), "program_account_id": account_text(&NATIVE_TOKEN) }]);
    Ok(!rpc(sequencer, "getAccountView", params, deadline)?.is_null())
}

/// LEZ's bridge program (`deposit::bridge_program`).
pub fn bridge_program() -> AccountId {
    crate::deposit::bridge_program()
}

/// The account in which LEZ's bridge marks the Logos Blockchain deposit `op_id` as minted
/// (its receipt, `deposit::receipt`): written in the same LEZ transaction that credits the
/// deposit, exactly once per deposit, and never by anything else.
pub fn deposit_receipt(op_id: &[u8; 32]) -> AccountId {
    crate::deposit::receipt(op_id)
}

/// Whether LEZ has minted the deposit whose receipt is `receipt`: the bridge's state on it
/// is written. `null` for an account nothing wrote is a real "not yet"; a reply that says
/// neither (an error, a page of HTML, no `result`) is no answer. Read as every read is
/// (`rpc`), by `deadline`: the call that asks shares one deadline among all its reads.
#[cfg(not(test))]
pub fn deposit_minted(sequencer: &str, receipt: &AccountId, deadline: Deadline) -> Result<bool, String> {
    let params = json!([{ "account_id": account_text(receipt), "program_account_id": account_text(&bridge_program()) }]);
    let result = rpc(sequencer, "getAccountView", params, deadline)?;
    minted_in(&json!({ "result": result }))
}

/// What a `getAccountView` reply for a receipt says (`deposit_minted`).
pub fn minted_in(reply: &Value) -> Result<bool, String> {
    if let Some(error) = reply.get("error") {
        return Err(format!("the sequencer refused the read: {error}"));
    }
    match reply.get("result") {
        Some(Value::Null) => Ok(false),
        Some(account @ Value::Object(_)) => Ok(!actor_bytes(account, &bridge_program())?.is_empty()),
        _ => Err("the sequencer sent an unreadable reply".to_owned()),
    }
}

/// Tests say which deposits LEZ minted (`MINTED`), and that it didn't answer (`MINTED_UNREAD`).
#[cfg(test)]
pub fn deposit_minted(_sequencer: &str, receipt: &AccountId, _deadline: Deadline) -> Result<bool, String> {
    if MINTED_UNREAD.with(std::cell::Cell::get) {
        return Err("could not reach the sequencer".to_owned());
    }
    let asked = MINTED_ASKED.with(|asked| asked.replace(asked.get() + 1));
    Ok(MINTED.with(|minted| minted.borrow().iter().any(|(minted, from)| minted == receipt && asked >= *from)))
}

#[cfg(test)]
thread_local! {
    /// The deposit receipts a test's sequencer holds, each from the how-many-th ask on
    /// (`MINTED_ASKED` counts them): 0, from the start.
    pub static MINTED: std::cell::RefCell<Vec<(AccountId, usize)>> = const { std::cell::RefCell::new(Vec::new()) };
    pub static MINTED_ASKED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Whether a test's sequencer doesn't answer receipt reads.
    pub static MINTED_UNREAD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// What the public account `account` holds, read from the sequencer at `sequencer` by
/// `deadline` (the asking call's): any account, not only the wallet's.
#[cfg(not(test))]
pub fn public_balance(sequencer: &str, account: &AccountId, deadline: Deadline) -> Result<u128, String> {
    Registry::by(sequencer, NATIVE_TOKEN, deadline).balance(account)
}

/// Tests say what an account holds: never the network.
#[cfg(test)]
pub fn public_balance(_sequencer: &str, account: &AccountId, _deadline: Deadline) -> Result<u128, String> {
    BALANCES.with(|held| held.borrow().iter().find(|(id, _)| id == account).map(|(_, balance)| *balance)).ok_or_else(|| "not reached".to_owned())
}

#[cfg(test)]
thread_local! {
    /// What the accounts of a test's sequencer hold.
    pub static BALANCES: std::cell::RefCell<Vec<(AccountId, u128)>> = const { std::cell::RefCell::new(Vec::new()) };
}

#[cfg(test)]
thread_local! {
    /// The accounts a test's sequencer has never seen.
    pub static UNSEEN: std::cell::RefCell<Vec<AccountId>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Tests say which accounts are unseen: never the network.
#[cfg(test)]
pub fn account_seen(_sequencer: &str, account: &AccountId, deadline: Deadline) -> Result<bool, String> {
    stub_wait(deadline)?;
    if ACCOUNTS_UNREADABLE.with(std::cell::Cell::get) {
        return Err("could not reach the sequencer: it answered HTTP 503 instead of a reading".to_owned());
    }
    Ok(!UNSEEN.with(|unseen| unseen.borrow().contains(account)))
}

/// A registry on one network: where to read it and which program it is, and until when
/// the call that made it may read: every read it makes shares that one deadline.
#[derive(Debug, Clone)]
pub struct Registry {
    pub sequencer: String,
    pub program: AccountId,
    pub deadline: Deadline,
}

impl Registry {
    /// `program` on `sequencer`, its reads sharing `READ_BUDGET` from now.
    pub fn new(sequencer: &str, program: AccountId) -> Self {
        Registry::by(sequencer, program, Deadline::after(READ_BUDGET))
    }

    /// `program` on `sequencer`, its reads sharing the asking call's `deadline`.
    pub fn by(sequencer: &str, program: AccountId, deadline: Deadline) -> Self {
        Registry { sequencer: sequencer.to_owned(), program, deadline }
    }

    /// `program`'s state on `account`; empty when it has none. A reply that isn't a
    /// reading (an HTTP error, HTML, an error reply, no result) fails, never reads as empty.
    fn actor_state(&self, account: &AccountId, program: &AccountId) -> Result<Vec<u8>, String> {
        let params = json!([{ "account_id": account_text(account), "program_account_id": account_text(program) }]);
        let result = rpc(&self.sequencer, "getAccountView", params, self.deadline)?;
        actor_bytes(&result, program)
    }

    /// The registry's state on `account`; empty when the account holds none.
    pub(crate) fn state(&self, account: &AccountId) -> Result<Vec<u8>, String> {
        self.actor_state(account, &self.program)
    }

    /// The native balance of `account`.
    pub fn balance(&self, account: &AccountId) -> Result<u128, String> {
        Ok(native_balance(&self.actor_state(account, &NATIVE_TOKEN)?))
    }

    /// What the name `id`'s deposit account holds: the deposit a release refunds.
    pub fn deposit(&self, id: &NameId) -> Result<u128, String> {
        self.balance(&deposit_address(&self.program, id))
    }

    /// What a wallet shows for one of its accounts: its balance, and the name
    /// it is shown as if it has a confirmed one. The balance is text because it
    /// is a 128-bit number.
    pub fn account_summary(&self, account: &AccountId) -> Result<Value, String> {
        let balance = self.balance(account)?;
        let claimed = self.state(account)?;
        let name = if claimed.is_empty() { None } else { self.confirmed(account, &claimed)? };
        Ok(json!({
            "account": account_text(account),
            "balance": balance.to_string(),
            "funded": balance > 0,
            "name": name,
        }))
    }

    /// Resolves `name`, whose identifier and key the caller has derived. A
    /// fee-mode name past its grace period is not registered, whatever its
    /// record holds; a name held by a deposit is registered until released.
    pub fn resolve(&self, name: &str, derived: &Derived) -> Result<Option<Resolved>, String> {
        Ok(self.record(name, derived)?.filter(|resolved| !resolved.gone(local_now())))
    }

    /// The record of `name` (top-level or `label.parent`) as the chain holds it, lapsed or not.
    pub fn record(&self, name: &str, derived: &Derived) -> Result<Option<Resolved>, String> {
        let slot = slot_id(name, &derived.id);
        #[cfg(test)]
        if let Some(found) = RECORDS.with(|records| {
            records
                .borrow()
                .iter()
                .find(|(id, _)| *id == slot || *id == derived.id)
                .map(|(_, record)| record.clone())
        }) {
            stub_wait(self.deadline)?;
            return Ok(Some(found));
        }
        if let Some((_, parent)) = split_subname(name) {
            let parent_derived = derive(parent);
            let parent_slot = slot_id(parent, &parent_derived.id);
            let bytes = self.state(&subname_record_address(&self.program, &parent_slot, &derived.id))?;
            if bytes.is_empty() {
                return Ok(None);
            }
            let sub = SubnameRecord::decode(&bytes)?;
            let kind_label = match sub.kind {
                SubnameKind::Permanent => "permanent".to_owned(),
                SubnameKind::Revocable { parent_generation } => {
                    let Some(parent_rec) = self.record(parent, &parent_derived)? else {
                        return Ok(None);
                    };
                    if parent_rec.gone(local_now()) || parent_rec.generation != parent_generation {
                        return Ok(None);
                    }
                    "revocable".to_owned()
                }
            };
            return Ok(Some(Resolved {
                version: sub.version,
                owner: sub.owner,
                expires: NEVER,
                profile: Profile::open(&derived.key, &sub.sealed, name),
                claimed: false,
                price_tier: PRICE_TIER_8_PLUS,
                generation: [0; 32],
                subname_sale: None,
                nft_token_id: [0; 32],
                nft_shielded: false,
                listing: None,
                listing_nonce: 0,
                site_listed: None,
                subname_kind: Some(kind_label),
            }));
        }
        let bytes = self.state(&record_address(&self.program, &slot))?;
        if bytes.is_empty() {
            return Ok(None);
        }
        Resolved::decode(&bytes, &derived.key, name).map(Some)
    }

    /// Reads the `SiteDirHeader` PDA (empty header if not yet initialized).
    pub fn site_dir_header(&self) -> Result<SiteDirHeader, String> {
        let bytes = self.state(&site_dir_header_address(&self.program))?;
        if bytes.is_empty() {
            return Ok(SiteDirHeader::empty());
        }
        SiteDirHeader::decode(&bytes)
    }

    /// Reads the on-chain sites directory (`SiteDirHeader` + `SiteBucket(0..16, page)`),
    /// verifies every entry locally via Argon2id + `slot_id` (dropping spoofs), and filters
    /// entries whose name is past its grace period.
    pub fn site_directory(&self, page_filter: Option<u16>) -> Result<Value, String> {
        let header = self.site_dir_header()?;
        let now = local_now();
        let mut sites = Vec::new();
        for bucket_idx in 0..SITE_BUCKETS {
            let max_page = header.active_page[usize::from(bucket_idx)];
            let (start_p, end_p) = match page_filter {
                Some(p) if p <= max_page => (p, p),
                Some(_) => continue,
                None => (0, max_page),
            };
            for p in start_p..=end_p {
                if self.deadline.passed() {
                    break;
                }
                let bytes = self.state(&site_bucket_address(&self.program, bucket_idx, p))?;
                if bytes.is_empty() {
                    continue;
                }
                let Ok(bucket) = SiteBucket::decode(&bytes) else { continue };
                for entry in bucket.entries {
                    if !is_valid_label(&entry.name) {
                        continue;
                    }
                    if entry.name_expires != NEVER && now >= entry.name_expires.saturating_add(GRACE_MS) {
                        continue;
                    }
                    let expected = slot_id(&entry.name, &derive(&entry.name).id);
                    if expected != entry.id {
                        continue;
                    }
                    sites.push(json!({
                        "name": entry.name,
                        "title": entry.title,
                        "updated_at": entry.updated_at,
                        "name_expires": if entry.name_expires == NEVER { Value::Null } else { Value::from(entry.name_expires) },
                    }));
                }
            }
        }
        sites.sort_by(|a, b| b["updated_at"].as_u64().cmp(&a["updated_at"].as_u64()));
        Ok(json!({
            "total_listed": header.total_listed,
            "sites": sites,
        }))
    }

    /// Reads all 16 `MarketBucket` PDAs, filters expired listings, and verifies any
    /// optional plaintext `display_name` locally via Argon2id + `tiered_id`.
    pub fn market_listings(&self) -> Result<Vec<Value>, String> {
        let now = local_now();
        let mut listings = Vec::new();
        for bucket_idx in 0..MARKET_BUCKETS {
            if self.deadline.passed() {
                break;
            }
            let bytes = self.state(&market_bucket_address(&self.program, bucket_idx))?;
            if bytes.is_empty() {
                continue;
            }
            let Ok(bucket) = MarketBucket::decode(&bytes) else { continue };
            for entry in bucket.entries {
                if entry.name_expires != NEVER && now >= entry.name_expires {
                    continue;
                }
                if entry.expires_at.is_some_and(|exp| now >= exp) {
                    continue;
                }
                let verified_name = entry.display_name.as_deref().and_then(|name| {
                    if !is_valid_label(name) {
                        return None;
                    }
                    let tier = price_tier_for_len(name.len());
                    if tier != entry.price_tier {
                        return None;
                    }
                    let expected = tiered_id(&derive(name).id, tier);
                    (expected == entry.id).then(|| name.to_owned())
                });
                let (seller_receives, fee) = sale_split(entry.price);
                listings.push(json!({
                    "id": hex(&entry.id),
                    "name": verified_name,
                    "price": entry.price.to_string(),
                    "fee": fee.to_string(),
                    "seller_receives": seller_receives.to_string(),
                    "pay_to": account_text(&entry.pay_to),
                    "expires_at": entry.expires_at,
                    "name_expires": if entry.name_expires == NEVER { Value::Null } else { Value::from(entry.name_expires) },
                    "price_tier": entry.price_tier,
                    "nonce": entry.nonce,
                    "nft_token_id": hex(&entry.nft_token_id),
                }));
            }
        }
        Ok(listings)
    }

    /// When the commitment `promised` was made, as its account records it
    /// (`{version, made}`), or `None` if there is none: it never landed, or a
    /// reveal used it up.
    pub fn commitment_made(&self, promised: &[u8; 32]) -> Result<Option<u64>, String> {
        let bytes = self.state(&commitment_address(&self.program, promised))?;
        if bytes.is_empty() {
            return Ok(None);
        }
        let mut reader = Reader { bytes: &bytes };
        let _version = reader.u8()?;
        reader.u64().map(Some)
    }

    /// The registry's settings, or `None` while it has none and charges nothing.
    pub fn settings(&self) -> Result<Option<Settings>, String> {
        let bytes = self.state(&settings_address(&self.program))?;
        if bytes.is_empty() {
            return Ok(None);
        }
        Settings::decode(&bytes).map(Some)
    }

    /// The time to claim in a transaction: the chain's own clock, which the
    /// next block can never be behind. This machine's clock is not a
    /// substitute: a transaction that claims a time ahead of the chain's is
    /// rejected and still charged the whole fee ceiling.
    pub fn now(&self) -> Result<u64, String> {
        self.actor_state(CLOCK_ACCOUNT, &clock_program())?
            .get(8..16)
            .and_then(|bytes| <[u8; 8]>::try_from(bytes).ok())
            .map(u64::from_le_bytes)
            .filter(|timestamp| *timestamp > 0)
            .ok_or_else(|| "could not read the chain's clock, so nothing was sent".to_owned())
    }

    /// The page published under a name (top-level or subname), or `None` when it has none or the
    /// name is no longer registered.
    pub fn page(&self, name: &str, derived: &Derived) -> Result<Option<Value>, String> {
        if self.resolve(name, derived)?.is_none() {
            return Ok(None);
        }
        let addr = if let Some((_, parent)) = split_subname(name) {
            let parent_slot = slot_id(parent, &derive(parent).id);
            subname_page_address(&self.program, &parent_slot, &derived.id)
        } else {
            page_address(&self.program, &slot_id(name, &derived.id))
        };
        let bytes = self.state(&addr)?;
        if bytes.is_empty() {
            return Ok(None);
        }
        let mut reader = Reader { bytes: &bytes };
        let _version = reader.u8()?;
        Ok(open_page(&derived.key, reader.byte_vec()?))
    }

    /// The name `account` is shown as. A reverse record is only a claim, so it
    /// counts only when the name's own record points back at the same account.
    pub fn reverse(&self, account: &AccountId) -> Result<Option<String>, String> {
        let bytes = self.state(account)?;
        if bytes.is_empty() {
            return Ok(None);
        }
        self.confirmed(account, &bytes)
    }

    /// The name claimed in `reverse_record`, if its own record agrees: the
    /// account holds the name (its owner tag matches) or the name publishes it.
    fn confirmed(&self, account: &AccountId, reverse_record: &[u8]) -> Result<Option<String>, String> {
        let claimed = decode_primary(reverse_record)?;
        if !is_valid_name(&claimed) {
            return Ok(None);
        }
        let derived = derive(&claimed);
        let confirmed = self.resolve(&claimed, &derived)?.is_some_and(|resolved| {
            resolved.is_held_by_name(&claimed, &derived, account) || resolved.profile.account == Some(*account)
        });
        Ok(confirmed.then_some(claimed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROGRAM: AccountId = [7; 32];

    /// A LEZ v0.4.0 sequencer derives the bridge system program via `system_upgrader` PDA
    /// and marks each minted deposit at its receipt PDA under `bridge_program()`.
    #[test]
    fn a_deposits_receipt_is_where_lez_marks_it_minted() {
        assert_eq!(bridge_program(), crate::deposit::system_program(b"bridge"));
        let mut genesis = [0_u8; 32];
        genesis[..24].copy_from_slice(b"/LEZ/v0.3/GenesisDeposit");
        let r0 = deposit_receipt(&genesis);
        genesis[24] = 1;
        let r1 = deposit_receipt(&genesis);
        assert_ne!(r0, r1);
        // LEZ v0.4.0 public account derivation vector (`lee-core/src/account.rs`):
        // key_type = Schnorr (0), key_commitment = [7; 32].
        let mut key7 = [7_u8; 33];
        key7[0] = 0;
        assert_eq!(account_text(&crate::deposit::key_account(&key7)), "N2k68narJVM21VGmt8V6nM5o9E38ASXxAwvLZzt7XjE");
        let program = account_text(&bridge_program());
        assert_eq!(minted_in(&json!({ "result": { "nonce": 0, "data": { "actor_states": { program.clone(): [1] } } } })), Ok(true));
        assert_eq!(minted_in(&json!({ "result": null })), Ok(false), "nothing wrote it: not minted yet");
        assert_eq!(minted_in(&json!({ "result": { "nonce": 0, "data": { "actor_states": {} } } })), Ok(false));
        assert!(minted_in(&json!({ "error": { "code": -32603 } })).is_err());
        assert!(minted_in(&json!({ "jsonrpc": "2.0" })).is_err(), "no result is no answer");
        assert!(minted_in(&json!({ "result": "<html>" })).is_err());
    }

    /// A proxy in front of a sequencer that is down answers 503, or a 200 page of HTML,
    /// or nothing: none of those is the sequencer answering, so a CLI's 0 balances and
    /// empty name list read meanwhile aren't believed (balance-marks-written-from-503-false-arrivals,
    /// mine-balance-accounts-believe-degraded-sequencer-503-and-200-garbage).
    #[test]
    fn only_a_block_number_is_a_sequencer_answer() {
        assert_eq!(probe_sequencer(&stub_http(json_reply(r#"{"jsonrpc":"2.0","id":1,"result":79401}"#), 1)), Ok(()));
        let unavailable = "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        let problem = probe_sequencer(&stub_http(unavailable, 1)).expect_err("a 503 is no answer");
        assert!(problem.starts_with("could not reach the sequencer"), "{problem}");
        let html = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: 37\r\nConnection: close\r\n\r\n<html><body>Bad gateway</body></html>";
        let problem = probe_sequencer(&stub_http(html, 1)).expect_err("HTML is no answer");
        assert!(problem.contains("isn't JSON-RPC"), "{problem}");
        let empty = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        assert!(probe_sequencer(&stub_http(empty, 1)).is_err());
        assert!(last_block_answer(r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found"}}"#).is_err());
        assert!(last_block_answer(r#"{"jsonrpc":"2.0","id":1,"result":null}"#).is_err());
        assert_eq!(last_block_answer(r#"{"jsonrpc":"2.0","id":1,"result":5}"#), Ok(5));
    }

    /// Every way a degraded sequencer (or a proxy in front of one) answers is a failed
    /// read in words, never an empty account: a 503, a page of HTML, garbage, an empty
    /// body, an error reply, a reply without a result. A null result is an account no
    /// transaction has touched.
    #[test]
    fn a_reply_that_isnt_a_reading_is_never_an_empty_account() {
        let program = [7_u8; 32];
        let read = |status: u16, body: &'static str| {
            let sequencer = stub_rpc(move |_| (status, body.to_owned()));
            Registry::new(&sequencer, program).balance(&[1; 32])
        };
        assert!(read(503, "upstream 503").expect_err("503").contains("answered HTTP 503"));
        assert!(read(200, "<html><body>Bad gateway</body></html>").expect_err("html").contains("isn't JSON-RPC"));
        assert!(read(200, "\u{0}not json{{").expect_err("garbage").contains("isn't JSON-RPC"));
        assert!(read(200, "").expect_err("empty").contains("isn't JSON-RPC"));
        assert!(read(200, r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32603,"message":"internal error"}}"#).expect_err("error").contains("internal error"));
        assert!(read(200, r#"{"jsonrpc":"2.0","id":1}"#).expect_err("no result").contains("no result"));
        assert_eq!(read(200, r#"{"jsonrpc":"2.0","id":1,"result":null}"#), Ok(0), "an untouched account holds nothing");
        let held = stub_rpc(|_| rpc_ok(view_holding(44_317_823)));
        assert_eq!(Registry::new(&held, program).balance(&[1; 32]), Ok(44_317_823));
        // No connection at all.
        assert!(Registry::new("http://127.0.0.1:9/", program).balance(&[1; 32]).expect_err("down").starts_with("could not reach the sequencer"));
    }

    /// resolve-slow-sequencer-exceeds-host-timeout-no-result: each read waited its own 15 s,
    /// so two of them on a slow sequencer ran past the host's 20 s and the call came back
    /// with no result. A registry's reads now share one deadline (READ_BUDGET, 4 s in tests).
    #[test]
    fn the_reads_of_one_call_share_one_deadline() {
        let slow = stub_rpc(|_| {
            std::thread::sleep(Duration::from_millis(1500));
            rpc_ok(view_holding(5))
        });
        let registry = Registry::new(&slow, [7; 32]);
        let started = Instant::now();
        assert_eq!(registry.balance(&[1; 32]), Ok(5));
        assert_eq!(registry.balance(&[2; 32]), Ok(5));
        let third = registry.balance(&[3; 32]).expect_err("past the deadline");
        assert!(started.elapsed() < READ_BUDGET + Duration::from_millis(700), "answered within the budget: {:?}", started.elapsed());
        // resolve-under-slow-says-6-s-after-15: the third read had 1 s left, and the call
        // waited 4 s in all; it says the 4 s.
        assert!(third.contains(&format!("didn't answer within {} s", READ_BUDGET.as_secs())), "{third}");
        // A hung sequencer is said so within the budget, not after the host gave up.
        let hung = stub_rpc(|_| {
            std::thread::sleep(Duration::from_secs(30));
            rpc_ok(Value::Null)
        });
        let started = Instant::now();
        let problem = Registry::new(&hung, [7; 32]).balance(&[1; 32]).expect_err("hung");
        assert!(started.elapsed() < READ_BUDGET + Duration::from_millis(700), "{:?}", started.elapsed());
        assert!(problem.starts_with("could not reach the sequencer: it didn't answer within"), "{problem}");
    }

    /// cli-receive-all-starts-with-only-the-probe-answering: a sequencer that says its last
    /// block and refuses account reads doesn't read accounts; a null view is a reading.
    #[test]
    fn an_account_read_tells_a_sequencer_that_only_says_its_last_block() {
        let probe_only = stub_rpc(|request| if request["method"] == "getLastBlockId" { rpc_ok(json!(7)) } else { (503, "upstream 503".into()) });
        assert_eq!(probe_sequencer(&probe_only), Ok(()));
        // A health check that times out says it was the health check, and its own 5 s.
        let hung = stub_rpc(|_| {
            std::thread::sleep(Duration::from_secs(30));
            rpc_ok(json!(7))
        });
        assert_eq!(probe_sequencer(&hung), Err("could not reach the sequencer: it didn't answer a health check within 5 s. Try again shortly".to_owned()));
        let refused = view_readable(&probe_only, &[1; 32], Deadline::after(READ_BUDGET)).expect_err("503");
        assert_eq!(refused, "could not reach the sequencer: it answered HTTP 503 instead of a reading");
        let answers = stub_rpc(|_| rpc_ok(Value::Null));
        assert_eq!(view_readable(&answers, &[1; 32], Deadline::after(READ_BUDGET)), Ok(()));
        // A deadline already spent asks nothing, and says how long the call waited.
        let spent = Deadline { start: Instant::now() - Duration::from_secs(12), at: Instant::now() };
        assert_eq!(view_readable(&answers, &[1; 32], spent), Err("could not reach the sequencer: it didn't answer within 12 s. Try again shortly".to_owned()));
    }

    /// The LEZ 0.3 testnet: its sequencer answers with `shards`, its records are
    /// version 5 (no `claimed`), its settings have no claims list. Read the same way.
    #[test]
    fn a_lez_0_3_registry_reads_as_well() {
        let program = [7_u8; 32];
        let reply = json!({ "nonce": 0, "data": { "shards": { account_text(&program): [5, 6], account_text(&[0; 32]): [1] } } });
        assert_eq!(actor_bytes(&reply, &program).expect("reads"), vec![5, 6]);
        assert_eq!(actor_bytes(&json!({ "data": { "shards": {} } }), &program).expect("reads"), Vec::<u8>::new());
        let v04 = json!({ "data": { "actor_states": { account_text(&program): [9] } } });
        assert_eq!(actor_bytes(&v04, &program).expect("reads"), vec![9]);

        let v5 = unhex(&format!("05{}{}01000000ab", "01".repeat(32), "ff".repeat(8))).expect("hex");
        let record = Resolved::decode(&v5, &[0; 32], "jefdiesel").expect("a v5 record decodes");
        assert_eq!((record.owner, record.expires, record.claimed), ([1; 32], u64::MAX, false));

        let live = unhex(&format!(
            "05{admin}{admin}{}01{}{}",
            "00".repeat(48),
            "00e1f505000000000000000000000000",
            "00943577000000000000000000000000",
            admin = "b6b80d9b12a1f9e36d60476dd4e41b4a2d5412671592f7447827728f83557808",
        ))
        .expect("hex");
        let settings = Settings::decode(&live).expect("the live v5 settings decode");
        assert_eq!(settings.deposits, [100_000_000, 2_000_000_000]);
        assert!(!settings.claims_open(0));
    }

    /// SPEC.md §7, Forwarding: a valid other name, normalised, shown in `resolve`.
    #[test]
    fn a_record_says_where_it_forwards() {
        let with = |value: &str| Resolved::simple(
            [1; 32],
            NEVER,
            Profile { name: "nameswallet".into(), texts: vec![("forward".into(), value.into())], ..Profile::default() },
            false,
        );
        assert_eq!(with("names").forward_target().as_deref(), Some("names"));
        assert_eq!(with(" @Names ").forward_target().as_deref(), Some("names"));
        assert_eq!(with("nameswallet").forward_target(), None, "not to itself");
        assert_eq!(with("not a name").forward_target(), None);
        assert_eq!(with("names").to_json()["forward"], "names");
        assert_eq!(Resolved { profile: sample_profile(), ..with("x") }.to_json()["forward"], Value::Null);
    }

    /// The profile sealed in SPEC.md's vector.
    fn sample_profile() -> Profile {
        Profile {
            name: "jef".to_owned(),
            account: Some([2; 32]),
            npk: None,
            vpk: Vec::new(),
            texts: vec![("chat".to_owned(), "hi".to_owned())],
        }
    }

    const NONCE: [u8; NONCE_LEN] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
    /// SPEC.md section 10, also asserted by `seal`'s tests.
    const SEALED_PROFILE: &str = "\
        000102030405060708090a0b7a39f7ff67b20894263a28d040c0f2711c04ba247fcf5cb107e27b59\
        05b8a0664b28cd25512addcb74af3efa5a8d6eb3bbb8e89fb8dba0aa2739252efed2be01ab019998\
        a07d416c788d8c9b699891e85bedd1118da7089377220bdb0ab8ae29661bf20e006c5fbffb222237\
        2ea0a2589ed49023186a77e9afee1af338f4a2377afa40da018789ca715318d981b556f71f9640eb\
        d056d8dcbe2b71845b45a578721ccc2c889df37b926c7da5a5202dbf3d50dc04883673134ce0772f\
        bcd7fbc8f31f455bb39b3d867d5e6772c86e59f42a9d04ffaacaa6119b1156d238665c0cf50028a9\
        cbf8b3189fc45a15a724c5775a835a8ccdff6a573d8e046d78e847b7da5668bc8529aedb3bdc70a9\
        9db9b9b4";
    const SEALED_PAGE: &str = "\
        000102030405060708090a0b7339f7ff79b216e10b4846b32bacf2731e06d04f8cf43af535da58e9\
        92f033cb16a471d0";

    /// SPEC.md section 10's derivations, shared with `seal` and the JavaScript resolver.
    #[test]
    fn a_name_derives_the_identifier_and_key_the_specification_gives() {
        for (name, master_hex, id, key) in [
            (
                "jef",
                "60e3df20e1c473ab71c666b913af96619fa14b49593e92598bd7709582e5a74e",
                "7041d9e447f06bdfe327d870402b1524cfe38bd03445b8fc3d0e87bc978eb763",
                "aad91a1a3dd0fb56a213212fbea58fe25d9139db42fc59ce2116c3cfa3440088",
            ),
            (
                "a-b-c",
                "c098ecc6c6af9cfcf449666ce012e3e930e076d870cb135ed8f1f058e7728362",
                "707decf4fde6d9fb61d255c23b34e042c846a0f4b0834151dcb2c1ef4160e849",
                "e3666a8572f7cabe94f6717491bff34c70eeca216436a142185bd8a86b6ccf31",
            ),
        ] {
            assert_eq!(hex(&master(name)), master_hex, "{name}");
            let derived = derive(name);
            assert_eq!(hex(&derived.id), id, "{name}");
            assert_eq!(hex(&derived.key), key, "{name}");
        }
        let jef = hex(&derive("jef").id);
        assert!(jef.starts_with("7041d9e4") && jef.ends_with("b763"));
    }

    /// Addresses as `core` computes them (SPEC.md section 10, and the rest from
    /// `names_core` itself).
    #[test]
    fn a_name_that_is_also_an_account_id_is_refused() {
        for typed in ["Jef.LEZ", "jef", "@jef", "lns:jef", " LNS:@Jef.lez "] {
            assert_eq!(parse_name(typed).unwrap(), "jef", "{typed}");
        }
        assert!(parse_name(&"1".repeat(32)).unwrap_err().contains("account ID"));
        assert!(parse_name(&"1".repeat(31)).is_ok());
    }

    #[test]
    fn addresses_match_core() {
        let jef = derive("jef").id;
        assert_eq!(account_text(&PROGRAM), "US517G5965aydkZ46HS38QLi7UQiSojurfbQfKCELFx");
        assert_eq!(account_text(&record_address(&PROGRAM, &jef)), "5b5xVGob2GYvrq21MSuxm1MfieucDEy9yxhDwdXndbpD");
        assert_eq!(account_text(&page_address(&PROGRAM, &jef)), "HEES9BpNxrAtwxFkyEQTEnjUcSr7usgbefeqiYJsG9mY");
        assert_eq!(account_text(&deposit_address(&PROGRAM, &jef)), "46Qu3gAftVLaXXV2armT1NkWXNQd7KuLLHx5Gqu2FMrc");
        assert_eq!(account_text(&settings_address(&PROGRAM)), "2aXkMrPG771Scj8LWBMyUGjJRou8Ls9kotSbnBfvoVfV");
        let tag = owner_tag(&jef, &[1; 32]);
        assert_eq!(hex(&tag), "f69d7a63559368fd63f7194d5d642d6f8ee3f3d190c784b079e76dfea828fba1");
        let promised = commitment(&jef, &tag, &[5; 32]);
        assert_eq!(hex(&promised), "1d84f6975de22294cad92f3b75056aacdd917c2887205e46132b400f4a24caa3");
        assert_eq!(account_text(&commitment_address(&PROGRAM, &promised)), "8unwVs8Lo7JUNUDraBvPTXKEszrXZKaKcsZjg8C9cg8D");

        let id: [u8; 32] = core::array::from_fn(|index| index as u8);
        assert_eq!(account_text(&record_address(&PROGRAM, &id)), "7bwwvMLtxqqNYiXsd5bYaq8e31E1odYf6jAK5z7tmUdd");
        assert_eq!(account_text(&page_address(&PROGRAM, &id)), "8M4vmdxCXL4eS8QXiHSdbeUUPt5Vbx1UVjLcv2cPk9Y5");
    }

    #[test]
    fn sealing_matches_the_specification_and_opens_again() {
        let key = derive("jef").key;
        assert_eq!(hex(&key), "aad91a1a3dd0fb56a213212fbea58fe25d9139db42fc59ce2116c3cfa3440088");
        let sealed = seal(&key, &sample_profile().encode(), RECORD_SEAL, NONCE).expect("seals");
        assert_eq!(hex(&sealed), SEALED_PROFILE);
        assert_eq!(sealed.len(), sample_profile().sealed_len());
        assert_eq!(Profile::open(&key, &sealed, "jef"), sample_profile());

        let mut page = Writer::default();
        page.string("text/plain");
        page.bytes(b"hi");
        assert_eq!(hex(&seal(&key, &page.0, PAGE_SEAL, NONCE).expect("seals")), SEALED_PAGE);
        let opened = open_page(&key, &unhex(SEALED_PAGE).expect("hex")).expect("opens");
        assert_eq!(opened["content_type"], "text/plain");
        assert_eq!(opened["text"], "hi");
        assert_eq!(opened["bytes"], 2);
    }

    #[test]
    fn a_profile_is_padded_and_sealed_differently_each_time() {
        let key = [9; 32];
        assert_eq!(sample_profile().encode().len(), 256);
        let with_keys = Profile { npk: Some([3; 32]), vpk: vec![4; VPK_LEN], ..sample_profile() };
        assert_eq!(with_keys.encode().len() % 256, 0);
        let first = with_keys.seal(&key).expect("seals");
        let second = with_keys.seal(&key).expect("seals");
        assert_ne!(first, second, "a fresh nonce every time");
        assert_eq!(Profile::open(&key, &first, "jef"), with_keys);
        assert_eq!(Profile::open(&key, &second, "jef"), with_keys);
    }

    #[test]
    fn what_does_not_open_reads_as_a_name_with_nothing_set() {
        let key = derive("jef").key;
        let sealed = unhex(SEALED_PROFILE).expect("hex");
        let bare = Profile::bare("jef");
        assert_eq!(Profile::open(&key, &[], "jef"), bare, "nothing sealed");
        assert_eq!(Profile::open(&[1; 32], &sealed, "jef"), bare, "another key");
        let mut tampered = sealed.clone();
        tampered[40] ^= 1;
        assert_eq!(Profile::open(&key, &tampered, "jef"), bare, "tampered");
        assert_eq!(Profile::open(&key, &sealed, "ada"), Profile::bare("ada"), "another name's profile");
        // A page is sealed for pages only: it does not open as a profile.
        assert_eq!(Profile::open(&key, &unhex(SEALED_PAGE).expect("hex"), "jef"), bare);
    }

    /// A record as `core` encodes `NameRecord { version: 6, owner: [1; 32],
    /// expires: u64::MAX, sealed: [0xab], claimed: false }`.
    #[test]
    fn a_record_held_by_a_deposit_never_lapses() {
        let bytes = unhex(&format!("06{}{}01000000ab00", "01".repeat(32), "ff".repeat(8))).expect("hex");
        let resolved = Resolved::decode(&bytes, &[0; 32], "jef").expect("decodes");
        assert_eq!(resolved.owner, [1; 32]);
        assert_eq!(resolved.expires, NEVER);
        assert!(resolved.never_expires());
        assert!(!resolved.lapsed(u64::MAX) && !resolved.gone(u64::MAX));
        let json = resolved.to_json();
        assert_eq!(json["expires"], Value::Null);
        assert_eq!(json["never"], true);
        assert_eq!(json["lapsed"], false);
        assert_eq!(json["owner_tag"], "01".repeat(32));
    }

    #[test]
    fn a_fee_mode_record_decodes_lapses_and_reports_where_to_pay() {
        let derived = derive("jef");
        let key = derived.key;
        let profile = Profile { npk: Some([3; 32]), vpk: vec![9, 9, 9, 9], ..sample_profile() };
        let mut record = vec![6];
        record.extend(owner_tag(&derived.id, &[1; 32]));
        record.extend(4_102_444_800_000_u64.to_le_bytes()); // the start of 2100
        let sealed = profile.seal(&key).expect("seals");
        record.extend((sealed.len() as u32).to_le_bytes());
        record.extend(&sealed);
        record.push(1);

        let resolved = Resolved::decode(&record, &key, "jef").expect("decodes");
        assert!(resolved.claimed);
        assert!(resolved.is_held_by(&derived.id, &[1; 32]));
        assert!(!resolved.is_held_by(&derived.id, &[2; 32]));
        assert_eq!(resolved.expires, 4_102_444_800_000);
        assert_eq!(resolved.profile, profile);
        assert!(!resolved.lapsed(4_102_444_799_999) && resolved.lapsed(4_102_444_800_000));
        assert!(!resolved.gone(4_102_444_800_000 + GRACE_MS - 1) && resolved.gone(4_102_444_800_000 + GRACE_MS));
        assert_eq!(resolved.to_json()["expires"], 4_102_444_800_000_u64);
        assert_eq!(resolved.to_json()["never"], false);

        let recipient = resolved.recipient_json();
        assert_eq!(recipient["display"], "jef");
        assert_eq!(recipient["public"], account_text(&[2; 32]));
        assert_eq!(recipient["public_hex"], "02".repeat(32));
        assert_eq!(recipient["private"]["nullifier_public_key"], "03".repeat(32));
        assert_eq!(recipient["private"]["viewing_public_key"], "09090909");
        assert_eq!(resolved.to_json()["texts"]["chat"], "hi");

        assert!(Resolved::decode(&record[..20], &key, "jef").is_err(), "truncated");
    }

    /// core's encoding of `Settings { version: 6, admin: [1; 32], treasury: [2; 32],
    /// prices: [1, 2, 3], mode: 1, deposits: [100_000_000, 2_000_000_000], claims_root:
    /// [3; 32], claims_until: 7, claims_program: [4; 32] }`.
    #[test]
    fn settings_decode_as_core_writes_them() {
        let core = unhex(&format!(
            "06{}{}{}{}{}01{}{}{}{}{}",
            "01".repeat(32),
            "02".repeat(32),
            "01000000000000000000000000000000",
            "02000000000000000000000000000000",
            "03000000000000000000000000000000",
            "00e1f505000000000000000000000000",
            "00943577000000000000000000000000",
            "03".repeat(32),
            "0700000000000000",
            "04".repeat(32),
        ))
        .expect("hex");
        let settings = Settings::decode(&core).expect("decodes");
        assert_eq!(
            settings,
            Settings {
                admin: [1; 32],
                treasury: [2; 32],
                prices: [[1, 2, 3, 0]; PRICE_TIERS],
                mode: MODE_DEPOSIT,
                deposits: [100_000_000, 2_000_000_000],
                claims_root: [3; 32],
                claims_until: 7,
                claims_program: [4; 32],
                premium_start: 0,
                nft_program: [0; 32],
            }
        );
        assert!(settings.deposit_mode());
        assert_eq!(Settings::mode_text(Some(&settings)), "deposit");
        assert_eq!(Settings::mode_text(None), "unconfigured");
        assert!(Settings::decode(&core[..core.len() - 1]).is_err(), "truncated");
        assert!(settings.claims_open(6) && !settings.claims_open(7));
        assert!(!Settings { claims_root: [0; 32], ..settings }.claims_open(6), "no list, no claims");
        // The clock's address as LEZ v0.4.0 derives it (`system_upgrader` PDA for "clock").
        assert_eq!(clock_program(), crate::deposit::system_program(b"clock"));
        assert_eq!(TERM_LENGTHS_MS[1], 31_536_000_000);
    }

    #[test]
    fn typed_names_and_accounts_are_normalised_and_checked() {
        for typed in ["jef", "jef.lez", "Jef.LEZ", "  jef.lez  ", "lns://jef"] {
            assert_eq!(parse_name(typed).as_deref(), Ok("jef"), "{typed}");
        }
        assert_eq!(parse_name("Blog.Jef.LEZ").as_deref(), Ok("blog.jef"));
        assert_eq!(split_subname("blog.jef"), Some(("blog", "jef")));
        assert!(parse_top_level_name("blog.jef").is_err());
        for bad in ["", ".lez", "a.b.c", "a--b", "-jef", "jef-", "j f", &"a".repeat(33)] {
            assert!(parse_name(bad).is_err(), "{bad}");
        }
        let text = account_text(&PROGRAM);
        assert_eq!(parse_account(&text), Ok(PROGRAM));
        assert_eq!(parse_account(&format!("Public/{text}")), Ok(PROGRAM));
        assert!(parse_account("not-an-account").is_err());
    }

    #[test]
    fn v8_slot_id_sale_split_and_rerelease_premium() {
        assert_eq!(price_tier_for_len(5), PRICE_TIER_5);
        assert_eq!(price_tier_for_len(6), PRICE_TIER_6_7);
        assert_eq!(price_tier_for_len(7), PRICE_TIER_6_7);
        assert_eq!(price_tier_for_len(8), PRICE_TIER_8_PLUS);

        let base = [0x11; 32];
        assert_ne!(slot_id("alice", &base), base);
        assert_eq!(slot_id("blog.alice", &base), base);
        assert_eq!(sale_split(40), (39, 1));
        assert_eq!(sale_split(41), (39, 2));
        assert_eq!(sale_split(1000), (975, 25));
        assert_eq!(rerelease_premium(1024, 0), 1024);
        assert_eq!(rerelease_premium(1024, DAY_MS), 512);
        assert_eq!(rerelease_premium(1024, DAY_MS / 2), 768);
        assert_eq!(rerelease_premium(1024, PREMIUM_DURATION_MS), 0);
        assert_eq!(parse_term("permanent"), Ok(TERM_PERMANENT));
        assert_eq!(term_label(TERM_PERMANENT), "permanent");
        assert!(validate_site_title("My Site").is_ok());
        assert!(validate_site_title(&"a".repeat(65)).is_err());
    }

    #[test]
    fn v8_settings_record_and_buckets_decode() {
        let mut w = Writer::default();
        w.u8(8);
        w.0.extend([1; 32]); // admin
        w.0.extend([2; 32]); // treasury
        for tier in 0..PRICE_TIERS {
            for term in 0..TERMS {
                w.u128(((tier + 1) * 100 + term) as u128);
            }
        }
        w.u8(MODE_FEE);
        w.u128(10);
        w.u128(20);
        w.0.extend([3; 32]); // claims_root
        w.u64(0);
        w.0.extend([4; 32]); // claims_program
        w.u128(4_000_000_000); // premium_start
        w.0.extend([9; 32]); // nft_program
        let s = Settings::decode(&w.0).expect("v8 settings decode");
        assert_eq!(s.price_for(0, 3), 103);
        assert_eq!(s.price_for(2, 1), 301);
        assert_eq!(s.premium_start, 4_000_000_000);
        assert_eq!(s.nft_program, [9; 32]);

        // v8 NameRecord with listing, subname_sale, nft, site_listed.
        let mut r = Writer::default();
        r.u8(8);
        r.0.extend([1; 32]); // owner
        r.u64(NEVER);
        r.bytes(&[]); // sealed
        r.bool(false); // claimed
        r.u8(PRICE_TIER_5);
        r.0.extend([0xaa; 32]); // generation
        r.u8(1); // Some(SubnameSale)
        r.u128(500);
        r.0.extend([0xbb; 32]);
        r.0.extend([0xcc; 32]); // nft_token_id
        r.bool(false); // nft_shielded
        r.u8(1); // Some(Listing)
        r.u128(1000);
        r.0.extend([0xdd; 32]);
        r.optional_u64(Some(9999));
        r.u64(7); // nonce
        r.bool(true); // in_market
        r.u64(7); // listing_nonce
        r.u8(1); // Some(site_listed)
        r.u16(2);
        let rec = Resolved::decode(&r.0, &[0; 32], "alice").expect("v8 record decodes");
        assert_eq!(rec.price_tier, PRICE_TIER_5);
        assert_eq!(rec.nft_token_id, [0xcc; 32]);
        assert_eq!(rec.site_listed, Some(2));
        assert_eq!(rec.listing.unwrap().price, 1000);
        assert_eq!(rec.subname_sale.unwrap().price, 500);
        let j = rec.to_json_with(Some(&s), 100);
        assert_eq!(j["listing"]["fee"], "25");
        assert_eq!(j["listing"]["seller_receives"], "975");
        assert_eq!(j["nft"]["token_id"], "cc".repeat(32));
        assert_eq!(j["site_listed"], 2);

        // SiteDirHeader + SiteBucket + MarketBucket decode
        let mut sh = Writer::default();
        sh.u8(8);
        for i in 0..SITE_BUCKETS {
            sh.u16(u16::from(i));
        }
        sh.u32(42);
        let hdr = SiteDirHeader::decode(&sh.0).expect("header");
        assert_eq!(hdr.total_listed, 42);
        assert_eq!(hdr.active_page[3], 3);
    }

    /// The shape `getAccountView` answers in; `gateway/app-src/vectors/engine-report.json` has real ones.
    #[test]
    fn actor_state_is_read_by_program_and_hex_reads_back() {
        let mut balance = vec![5];
        balance.resize(16, 0);
        let account = json!({ "data": { "actor_states": {
            account_text(&NATIVE_TOKEN): balance,
            account_text(&PROGRAM): [1, 2, 3],
        } } });
        assert_eq!(account_text(&NATIVE_TOKEN), "1".repeat(32));
        assert_eq!(actor_bytes(&account, &PROGRAM), Ok(vec![1, 2, 3]));
        assert_eq!(actor_bytes(&account, &[9; 32]), Ok(Vec::new()));
        assert_eq!(actor_bytes(&Value::Null, &PROGRAM), Ok(Vec::new()), "an account not on the chain");
        assert_eq!(native_balance(&actor_bytes(&account, &NATIVE_TOKEN).expect("bytes")), 5);
        assert_eq!(native_balance(&[]), 0);

        assert_eq!(unhex("0x0aFF"), Ok(vec![10, 255]));
        assert_eq!(unhex(&hex(&[0, 7, 255])), Ok(vec![0, 7, 255]));
        assert!(unhex("abc").is_err());
        assert!(unhex("zz").is_err());

        let mut primary = vec![5];
        primary.extend(3_u32.to_le_bytes());
        primary.extend(b"jef");
        assert_eq!(decode_primary(&primary).as_deref(), Ok("jef"));
    }
}
