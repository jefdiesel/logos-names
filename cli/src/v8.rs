//! LEZ 0.4 / Registry v8 client helpers and commands for `names`:
//! - tiered `slot_id` and generation derivation (matching `names_core`)
//! - multi-program `ProgramCatalog` loading for the tied `lez_nft` program
//! - subnames (`create`, `revoke`, `sale`, `buy`, plus edit/page/transfer/release/resolve)
//! - marketplace (`list`, `cancel`, `buy`, `market`) and tied NFT (`transfer --via-nft`, deed shielding)
//! - sites directory (`sites` with Argon2id spoof verification, `discover=off` opt-out `SiteDirOp`)
//! - admin commands (`set-price`, `set-premium`, `set-treasury`)
//! - backwards-compatible v5/v6 `NameRecord` and `Settings` decoding for legacy testnets.

use anyhow::{Context as _, Result, anyhow, bail};
use clap::Subcommand;
use lee::program::Program;
use lee_core::account::AccountId;
use names_core::{
    DEFAULT_PRICES, GRACE_MS, Instruction, MAX_SITE_TITLE_BYTES, MIN_SALE_PRICE, MODE_FEE,
    MarketBucket, MarketEntry, NAME_SUFFIX, NameId, NameRecord, Page, PREMIUM_DURATION_MS,
    PRICE_TIER_5, PRICE_TIER_6_7, PRICE_TIER_8_PLUS, SiteBucket, SiteDirHeader, SiteDirOp,
    SiteEntry, SubnameKind, SubnameRecord, SubnameSale, TERM_PERMANENT, VERSION,
    bucket_of, commitment_seed, market_bucket_address, market_bucket_seed, name_seeds,
    nft_token_seed, owner_tag, price_tier_for_len,
    record_address, rerelease_premium, sale_split, settings_seed, site_bucket_address,
    site_bucket_seed, site_dir_header_address, site_dir_header_seed, split_subname,
    subname_commitment, subname_page_address, subname_record_address, subname_seeds, tiered_id,
    validate_name, validate_site_title,
};
use names_seal::{
    Derived, PageContent, Profile, derive, open_page, open_profile, seal_page, seal_profile,
    validate_page, validate_profile,
};
use sha2::{Digest as _, Sha256};
use wallet::{AccountIdentity, AccountMention};

use crate::{
    Client, commit_wait_ms, default_private, load_defaults, local_now, parse_account, parse_name,
    parse_new_owner, readable, save_defaults,
};

/// Text entry key that opts a top-level website out of the on-chain sites directory (`discover=off`).
pub const DISCOVER_KEY: &str = "discover";

/// Computes a top-level name's v8 on-chain `slot_id = tiered_id(base_id, price_tier_for_len(name.len()))`.
pub fn slot_id(name: &str, base_id: &NameId) -> NameId {
    tiered_id(base_id, price_tier_for_len(name.len()))
}

pub use names_core::{claim_generation, register_generation};

/// `lez_nft` v3's config PDA seed domain (`lez_nft::config_seed`).
const NFT_CONFIG_SEED_DOMAIN: &[u8] = b"lez-nft/v3/config";

/// `lez_nft::config_address(nft_program)`: the NFT program's Config, which every
/// linked Mint (Register, Claim) must declare as `(config, nft_program)`.
pub fn nft_config_address(nft_program: &AccountId) -> AccountId {
    AccountId::for_public_pda(nft_program, &nft_config_seed())
}

fn nft_config_seed() -> lee_core::program::PdaSeed {
    lee_core::program::PdaSeed::new(Sha256::digest(NFT_CONFIG_SEED_DOMAIN).into())
}

/// Whether the profile sets `discover=off` (trimmed, ASCII case-insensitive; mirrors `gateway=off`).
pub fn discover_is_off(profile: &Profile) -> bool {
    profile
        .texts
        .iter()
        .find(|t| t.key == DISCOVER_KEY)
        .is_some_and(|t| t.value.trim().eq_ignore_ascii_case("off"))
}

/// Chooses a valid site directory title for `name`: explicit override, else `display` text, else `name`.
pub fn site_title_for(name: &str, profile: &Profile, override_title: Option<&str>) -> String {
    let raw = override_title
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            profile
                .texts
                .iter()
                .find(|t| t.key == "display")
                .map(|t| t.value.trim())
                .filter(|s| !s.is_empty())
        })
        .unwrap_or(name);
    let mut cleaned: String = raw
        .chars()
        .filter(|c| !c.is_control())
        .collect();
    // Truncate cleanly at a UTF-8 char boundary within MAX_SITE_TITLE_BYTES bytes.
    while cleaned.len() > MAX_SITE_TITLE_BYTES {
        cleaned.pop();
    }
    let cleaned = cleaned.trim().to_owned();
    if validate_site_title(&cleaned).is_ok() {
        cleaned
    } else {
        name.to_owned()
    }
}

/// Parses a length tier for `names set-price`: `5`/`0` (tier 0), `6-7`/`1` (tier 1), `8+`/`2` (tier 2).
pub fn parse_price_tier(input: &str) -> Result<u8, String> {
    match input.trim().to_ascii_lowercase().as_str() {
        "0" | "5" | "short" | "tier0" => Ok(PRICE_TIER_5),
        "1" | "6-7" | "6" | "7" | "medium" | "tier1" => Ok(PRICE_TIER_6_7),
        "2" | "8+" | "8" | "long" | "tier2" => Ok(PRICE_TIER_8_PLUS),
        _ => Err("tier must be 5 (0), 6-7 (1) or 8+ (2)".to_owned()),
    }
}

/// Human-readable label for a pricing tier.
pub fn tier_label(tier: u8) -> &'static str {
    match tier {
        PRICE_TIER_5 => "5 chars",
        PRICE_TIER_6_7 => "6-7 chars",
        _ => "8+ chars",
    }
}

/// Human-readable label for a registration/renewal term index.
pub fn term_label(term: u8) -> &'static str {
    match term {
        0 => "30d",
        1 => "1y",
        2 => "5y",
        TERM_PERMANENT => "permanent",
        _ => "unknown",
    }
}

/// Parses `--prices` into the 3x4 v8 price table (`[tier][term]`):
/// - 12 values: row-major 3 tiers x 4 terms (`5:30d,1y,5y,perm,6-7:...,8+:...`)
/// - 4 values: one row (`30d,1y,5y,permanent`) applied to all 3 tiers
/// - 3 values: legacy (`30d,1y,5y`, with `permanent = 0`) applied to all 3 tiers
pub fn parse_prices_matrix(values: &[u128]) -> Result<[[u128; 4]; 3], String> {
    match values.len() {
        12 => Ok([
            [values[0], values[1], values[2], values[3]],
            [values[4], values[5], values[6], values[7]],
            [values[8], values[9], values[10], values[11]],
        ]),
        4 => {
            let row = [values[0], values[1], values[2], values[3]];
            Ok([row, row, row])
        }
        3 => {
            let row = [values[0], values[1], values[2], 0];
            Ok([row, row, row])
        }
        _ => Err(
            "--prices takes 12 prices (3 tiers x 4 terms: 30d,1y,5y,permanent), 4 prices (30d,1y,5y,permanent), or 3 legacy prices (30d,1y,5y)"
                .to_owned(),
        ),
    }
}

/// Parses a listing expiry: relative duration (`30m`, `24h`, `7d`) added to `now`, or an absolute Unix ms timestamp.
pub fn parse_expires(input: &str, now: u64) -> Result<u64, String> {
    let text = input.trim();
    if text.is_empty() {
        return Err("expiry cannot be empty".to_owned());
    }
    if let Some(days) = text.strip_suffix('d') {
        let n: u64 = days.parse().map_err(|_| format!("invalid duration: {input}"))?;
        if n == 0 {
            return Err("expiry duration must be above 0".to_owned());
        }
        return Ok(now.saturating_add(n.saturating_mul(86_400_000)));
    }
    if let Some(hours) = text.strip_suffix('h') {
        let n: u64 = hours.parse().map_err(|_| format!("invalid duration: {input}"))?;
        if n == 0 {
            return Err("expiry duration must be above 0".to_owned());
        }
        return Ok(now.saturating_add(n.saturating_mul(3_600_000)));
    }
    if let Some(mins) = text.strip_suffix('m') {
        let n: u64 = mins.parse().map_err(|_| format!("invalid duration: {input}"))?;
        if n == 0 {
            return Err("expiry duration must be above 0".to_owned());
        }
        return Ok(now.saturating_add(n.saturating_mul(60_000)));
    }
    let ts: u64 = text
        .parse()
        .map_err(|_| format!("expiry must be a duration (e.g. 7d, 24h, 30m) or Unix ms timestamp: {input}"))?;
    if ts <= now {
        return Err("expiry timestamp must be in the future".to_owned());
    }
    Ok(ts)
}

/// Verifies a `SiteEntry` against Argon2id + `tiered_id` to filter spoofed directory entries.
pub fn verify_site_entry(entry: &SiteEntry) -> bool {
    if validate_name(&entry.name).is_err() || split_subname(&entry.name).is_some() {
        return false;
    }
    let derived = derive(&entry.name);
    slot_id(&entry.name, &derived.id) == entry.id
}

/// Verifies an optional `MarketEntry.display_name` against Argon2id + `tiered_id`.
/// Returns `Some(name)` only when the name is valid and matches `entry.id`.
pub fn verify_market_entry(entry: &MarketEntry) -> Option<String> {
    let name = entry.display_name.as_deref()?;
    if validate_name(name).is_err() || split_subname(name).is_some() {
        return None;
    }
    let derived = derive(name);
    (slot_id(name, &derived.id) == entry.id).then(|| name.to_owned())
}

/// Upgrades raw v5/v6 `NameRecord` bytes to v8 so `borsh::from_slice::<NameRecord>` succeeds
/// on both legacy LEZ 0.3 testnets and v8 chains.
pub fn upgrade_record_bytes(mut bytes: Vec<u8>) -> Vec<u8> {
    let Some(&version) = bytes.first() else {
        return bytes;
    };
    if version >= VERSION {
        return bytes;
    }
    if version < 6 {
        // v5 lacked `claimed: bool`.
        bytes.push(0);
    }
    if version < 7 {
        // v6 lacked v7/v8 fields:
        // price_tier(1), generation(32), subname_sale(Option=0), nft_token_id(32),
        // nft_shielded(0), listing(Option=0), listing_nonce(u64=0), site_listed(Option=0).
        bytes.push(PRICE_TIER_8_PLUS);
        bytes.extend_from_slice(&[0u8; 32]); // generation
        bytes.push(0); // subname_sale: None
        bytes.extend_from_slice(&[0u8; 32]); // nft_token_id
        bytes.push(0); // nft_shielded: false
        bytes.push(0); // listing: None
        bytes.extend_from_slice(&0u64.to_le_bytes()); // listing_nonce
        bytes.push(0); // site_listed: None
    } else if version == 7 {
        // v7 already had price_tier, generation, subname_sale; lacked v8 fields.
        bytes.extend_from_slice(&[0u8; 32]); // nft_token_id
        bytes.push(0); // nft_shielded: false
        bytes.push(0); // listing: None
        bytes.extend_from_slice(&0u64.to_le_bytes()); // listing_nonce
        bytes.push(0); // site_listed: None
    }
    bytes
}

/// Upgrades raw v5/v6 `Settings` bytes to v8 so `borsh::from_slice::<Settings>` succeeds
/// on both legacy testnets and v8 chains.
pub fn upgrade_settings_bytes(bytes: Vec<u8>) -> Vec<u8> {
    let Some(&version) = bytes.first() else {
        return bytes;
    };
    if version >= VERSION {
        return bytes;
    }
    // v5 layout: version(1) + admin(32) + treasury(32) + prices([u128; 3], 48) + mode(1) + deposits([u128; 2], 32) = 146 bytes
    // v6 layout: v5 + claims_root(32) + claims_until(8) + claims_program(32) = 218 bytes
    if version < 7 && bytes.len() >= 146 {
        let mut out = Vec::with_capacity(1 + 32 + 32 + 192 + 1 + 32 + 32 + 8 + 32 + 16 + 32);
        out.push(version);
        out.extend_from_slice(&bytes[1..65]); // admin + treasury
        // Expand 3 prices ([u128; 3]) to 3x4 matrix (30d, 1y, 5y, permanent=0) for all 3 tiers.
        let p0 = &bytes[65..81];
        let p1 = &bytes[81..97];
        let p2 = &bytes[97..113];
        let zero16 = [0u8; 16];
        for _ in 0..3 {
            out.extend_from_slice(p0);
            out.extend_from_slice(p1);
            out.extend_from_slice(p2);
            out.extend_from_slice(&zero16);
        }
        out.push(bytes[113]); // mode
        out.extend_from_slice(&bytes[114..146]); // deposits
        if version >= 6 && bytes.len() >= 218 {
            out.extend_from_slice(&bytes[146..218]); // claims_root + claims_until + claims_program
        } else {
            out.extend_from_slice(&[0u8; 32 + 8 + 32]);
        }
        out.extend_from_slice(&0u128.to_le_bytes()); // premium_start
        out.extend_from_slice(&[0u8; 32]); // nft_program
        return out;
    }
    if version == 7 && bytes.len() == 1 + 32 + 32 + 192 + 1 + 32 + 32 + 8 + 32 {
        let mut out = bytes;
        out.extend_from_slice(&0u128.to_le_bytes()); // premium_start
        out.extend_from_slice(&[0u8; 32]); // nft_program
        return out;
    }
    bytes
}

/// `lez_nft` v3 `Message::Instruction(Instruction::Transfer { token_id, new_owner,
/// sealed: None, now: Some(now), linked_target: Some(slot_id) })`, from
/// lez-nft v3-lez04 `core/src/lib.rs`: `Message::Instruction` is variant 0 and
/// `Instruction::Transfer` variant 3. For a linked token `new_owner` is the
/// registry's `owner_tag(slot_id, account)` and `linked_target` is the name's
/// `slot_id` (`linked.target`).
pub const NFT_INSTRUCTION_TRANSFER: u8 = 3;

pub fn encode_lez_nft_transfer(
    token_id: &[u8; 32],
    new_owner_tag: &[u8; 32],
    now: u64,
    slot_id: &NameId,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + 32 + 32 + 1 + 9 + 33);
    out.push(0x00); // Message::Instruction
    out.push(NFT_INSTRUCTION_TRANSFER);
    out.extend_from_slice(token_id);
    out.extend_from_slice(new_owner_tag);
    out.push(0); // sealed: None
    out.push(1); // now: Some
    out.extend_from_slice(&now.to_le_bytes());
    out.push(1); // linked_target: Some(slot_id)
    out.extend_from_slice(slot_id);
    out
}

/// Subname commands (`names subname ...`).
#[derive(Subcommand)]
pub enum SubnameCommand {
    /// Create a subname under a parent name you hold (`label.parent`).
    Create {
        /// The full subname to create, e.g. `blog.alice`.
        #[arg(value_parser = parse_name)]
        name: String,
        /// Account that will own the subname; defaults to the parent's holder.
        #[arg(long, value_parser = parse_new_owner)]
        owner: Option<AccountId>,
        /// Make the subname permanent (cannot be revoked by the parent; survives parent transfer).
        #[arg(long, conflicts_with = "revocable")]
        permanent: bool,
        /// Make the subname revocable (default; parent can revoke, and detaches if parent transfers).
        #[arg(long, conflicts_with = "permanent")]
        revocable: bool,
    },
    /// Revoke a revocable subname under a parent name you hold.
    Revoke {
        /// The full subname to revoke, e.g. `blog.alice`.
        #[arg(value_parser = parse_name)]
        name: String,
    },
    /// Turn public subname sales on or off for a parent name you hold.
    Sale {
        /// The parent name, e.g. `alice`.
        #[arg(value_parser = parse_name)]
        parent: String,
        /// Price per permanent subname in base units (required unless `--off`).
        #[arg(long, value_parser = crate::tokens::parse_amount, allow_hyphen_values = true, conflicts_with = "off")]
        price: Option<u128>,
        /// Public account that receives subname sale payments (defaults to the parent's holder if public).
        #[arg(long, value_parser = parse_account, conflicts_with = "off")]
        payout: Option<AccountId>,
        /// Turn subname sales off.
        #[arg(long, conflicts_with_all = ["price", "payout"])]
        off: bool,
    },
    /// Buy a permanent subname from a parent that has subname sales enabled (commit, wait, reveal).
    Buy {
        /// The full subname to buy, e.g. `blog.alice`.
        #[arg(value_parser = parse_name)]
        name: String,
        /// Account that will own and pay for the subname; defaults to the setup's private account.
        #[arg(long, value_parser = parse_account)]
        owner: Option<AccountId>,
        /// Stay until the commit wait finishes and reveal completes (default: true).
        #[arg(long, default_value_t = true)]
        wait: bool,
    },
}

/// Checks whether a name is a subname and splits it into `(label, parent)`, returning an error if not.
pub fn require_subname(name: &str) -> Result<(&str, &str)> {
    split_subname(name)
        .ok_or_else(|| anyhow!("{name} is not a subname (write it as <label>.<parent>, e.g. blog.alice)"))
}

/// Whether the profile points the name at a Logos Storage site (a non-empty `cid` entry, as
/// `lns publish` writes and the gateway serves).
pub fn has_cid_site(profile: &Profile) -> bool {
    profile
        .texts
        .iter()
        .any(|t| t.key == "cid" && !t.value.trim().is_empty())
}

/// Computes what `SiteDirOp` a profile edit should send for a top-level name.
///
/// A name has a site when it has an on-chain page OR a `cid` site in its profile; every such
/// name is listed in the sites directory unless the profile sets `discover=off`. A name whose
/// last site was removed (no page, `cid` cleared) leaves the directory.
pub fn site_dir_op_for_edit(
    name: &str,
    record: &NameRecord,
    profile: &Profile,
    has_page: bool,
    now: u64,
    active_page: u16,
) -> SiteDirOp {
    let has_site = has_page || has_cid_site(profile);
    if discover_is_off(profile) || !has_site {
        if record.site_listed.is_some() {
            SiteDirOp::Remove
        } else {
            SiteDirOp::Keep
        }
    } else if record.site_listed.is_none() && now < record.expires {
        // The registry refuses to list a lapsed name (in grace): it stays unlisted until renewed.
        let title = site_title_for(name, profile, None);
        SiteDirOp::Upsert {
            name: name.to_owned(),
            title,
            page: active_page,
            now,
        }
    } else {
        SiteDirOp::Keep
    }
}

/// Computes what `SiteDirOp` a page publish/clear should send for a top-level name.
pub fn site_dir_op_for_page(
    name: &str,
    record: &NameRecord,
    profile: &Profile,
    publishing: bool,
    no_discover: bool,
    force_discover: bool,
    title_override: Option<&str>,
    now: u64,
    active_page: u16,
) -> SiteDirOp {
    if !publishing {
        // Removing the page leaves the listing alone while a `cid` site remains.
        return if record.site_listed.is_some() && !(has_cid_site(profile) && !discover_is_off(profile)) {
            SiteDirOp::Remove
        } else {
            SiteDirOp::Keep
        };
    }
    let off = no_discover || (!force_discover && discover_is_off(profile));
    if off {
        if record.site_listed.is_some() {
            SiteDirOp::Remove
        } else {
            SiteDirOp::Keep
        }
    } else if now >= record.expires {
        // The registry refuses an Upsert on a lapsed name (in grace).
        SiteDirOp::Keep
    } else {
        let title = site_title_for(name, profile, title_override);
        let page = record.site_listed.unwrap_or(active_page);
        SiteDirOp::Upsert {
            name: name.to_owned(),
            title,
            page,
            now,
        }
    }
}

impl Client {
    /// Returns the `ProgramCatalog` entries required for `mentions`: always `self.program`,
    /// plus `settings.nft_program` if any mention targets that program.
    pub async fn programs_for(&self, mentions: &[AccountMention]) -> Result<Vec<(AccountId, Program)>> {
        let mut programs = vec![(self.program, self.program().await?)];
        let native = AccountId::new([0u8; 32]);
        let mut extra: std::collections::BTreeSet<AccountId> = std::collections::BTreeSet::new();
        for mention in mentions {
            let pid = mention.actor().program_account_id;
            if pid != self.program && pid != native {
                extra.insert(pid);
            }
        }
        for pid in extra {
            let prog = self
                .program_at(pid, "LOGOS_NFT_PROGRAM_ELF", "lez-nft", "the NFT program")
                .await?;
            programs.push((pid, prog));
        }
        Ok(programs)
    }

    /// The tied NFT token actor under `nft_program`.
    pub fn nft_actor(&self, nft_program: AccountId, token_id: &[u8; 32]) -> AccountMention {
        AccountIdentity::PublicPda {
            program: nft_program,
            seed: nft_token_seed(token_id),
        }
        .select_program_actor_state(nft_program)
    }

    /// The marketplace bucket actor for `slot_id`.
    pub fn market_bucket_actor(&self, slot_id: &NameId) -> AccountMention {
        self.registry_actor(market_bucket_seed(bucket_of(slot_id)))
    }

    /// The sites directory bucket and header actors for `(slot_id, page)`.
    pub fn site_actors(&self, slot_id: &NameId, page: u16) -> [AccountMention; 2] {
        [
            self.registry_actor(site_bucket_seed(bucket_of(slot_id), page)),
            self.registry_actor(site_dir_header_seed()),
        ]
    }

    /// Appends the tied NFT token actor if `settings` has a non-zero `nft_program` and `token_id` is non-zero.
    pub fn push_nft_mention(
        &self,
        mentions: &mut Vec<AccountMention>,
        settings: Option<&names_core::Settings>,
        token_id: &[u8; 32],
    ) {
        if let Some(s) = settings {
            let zero = AccountId::new([0u8; 32]);
            if s.nft_program != zero && *token_id != [0u8; 32] {
                mentions.push(self.nft_actor(s.nft_program, token_id));
            }
        }
    }

    /// Appends `(lez_nft::config_address(nft), nft)`, which a Register or Claim that mints
    /// the tied NFT must declare (lez_nft checks the registry is authorised in it).
    pub fn push_nft_config_mention(&self, mentions: &mut Vec<AccountMention>, settings: Option<&names_core::Settings>) {
        if let Some(s) = settings {
            if s.nft_program != AccountId::new([0u8; 32]) {
                mentions.push(
                    AccountIdentity::PublicPda { program: s.nft_program, seed: nft_config_seed() }
                        .select_program_actor_state(s.nft_program),
                );
            }
        }
    }

    /// Appends market/site cleanup mentions for an existing `record` that is being transferred,
    /// released, bought, or overwritten after lapsing.
    pub fn push_cleanup_mentions(
        &self,
        mentions: &mut Vec<AccountMention>,
        slot: &NameId,
        record: &NameRecord,
    ) {
        if record.listing.as_ref().is_some_and(|l| l.in_market) {
            mentions.push(self.market_bucket_actor(slot));
        }
        if let Some(page) = record.site_listed {
            mentions.extend(self.site_actors(slot, page));
        }
    }

    /// Reads the sites directory header (or returns an empty default header if not created yet).
    pub async fn site_dir_header(&self) -> Result<SiteDirHeader> {
        let bytes = self.actor_state(site_dir_header_address(&self.program)).await?;
        if bytes.is_empty() {
            return Ok(SiteDirHeader {
                version: VERSION,
                active_page: [0u16; 16],
                total_listed: 0,
            });
        }
        borsh::from_slice(&bytes).context("the sites directory header did not decode")
    }

    /// Reads a subname record at `(parent_slot_id, sub_id)`.
    pub async fn subname_record(
        &self,
        parent_slot: &NameId,
        sub_id: &NameId,
    ) -> Result<Option<SubnameRecord>> {
        let bytes = self
            .actor_state(subname_record_address(&self.program, parent_slot, sub_id))
            .await?;
        if bytes.is_empty() {
            return Ok(None);
        }
        Ok(Some(
            borsh::from_slice(&bytes).context("the subname record did not decode")?,
        ))
    }

    /// Finds the account in this wallet that holds a subname `sub_id`.
    pub fn subname_holder(&self, name: &str, sub_id: &NameId, record: &SubnameRecord) -> Result<AccountId> {
        let key_chain = self.wallet.storage().key_chain();
        key_chain
            .public_account_ids()
            .map(|(account, _)| account)
            .chain(key_chain.private_account_ids().map(|(account, _)| account))
            .find(|account| owner_tag(sub_id, account) == record.owner)
            .ok_or_else(|| anyhow!("this wallet does not hold {name}{NAME_SUFFIX}"))
    }

    /// Looks up a subname `label.parent`, verifying parent liveness + generation if revocable.
    pub async fn lookup_subname(
        &self,
        name: &str,
        enforce_parent_live: bool,
    ) -> Result<(NameId, Derived, SubnameRecord, Option<NameRecord>, Profile)> {
        let (_, parent) = require_subname(name)?;
        let parent_base = derive(parent);
        let parent_slot = slot_id(parent, &parent_base.id);
        let sub_derived = derive(name);
        let sub = self
            .subname_record(&parent_slot, &sub_derived.id)
            .await?
            .ok_or_else(|| anyhow!("{name} is not registered"))?;
        let parent_record = self.record(&parent_slot).await?;
        if let SubnameKind::Revocable { parent_generation } = sub.kind {
            if enforce_parent_live {
                let Some(p) = &parent_record else {
                    bail!("{name} is no longer valid (its parent {parent} is not registered)");
                };
                if local_now() >= p.expires.saturating_add(GRACE_MS) {
                    bail!("{name} is no longer valid (its parent {parent} has lapsed)");
                }
                if p.generation != parent_generation {
                    bail!("{name} is no longer valid (its parent {parent} changed hands)");
                }
            }
        }
        let profile = open_profile(&sub_derived.key, &sub.sealed, name).unwrap_or_else(|| Profile {
            name: name.to_owned(),
            ..Profile::default()
        });
        Ok((parent_slot, sub_derived, sub, parent_record, profile))
    }

    /// Reads a subname's page.
    pub async fn page_subname(&self, name: &str) -> Result<Option<PageContent>> {
        let (parent_slot, sub_derived, _, _, _) = self.lookup_subname(name, true).await?;
        let bytes = self
            .actor_state(subname_page_address(&self.program, &parent_slot, &sub_derived.id))
            .await?;
        if bytes.is_empty() {
            return Ok(None);
        }
        let page: Page =
            borsh::from_slice(&bytes).context("the subname page account does not hold a page")?;
        Ok(open_page(&sub_derived.key, &page.sealed))
    }

    /// Resolves a subname to JSON.
    pub async fn resolve_subname(&self, name: &str) -> Result<serde_json::Value> {
        let (_, parent) = require_subname(name)?;
        let (parent_slot, sub_derived, sub, parent_record, profile) =
            self.lookup_subname(name, true).await?;
        let page_bytes = self
            .actor_state(subname_page_address(&self.program, &parent_slot, &sub_derived.id))
            .await?;
        let page = if page_bytes.is_empty() {
            None
        } else {
            let p: Page =
                borsh::from_slice(&page_bytes).context("the subname page account does not hold a page")?;
            open_page(&sub_derived.key, &p.sealed)
        };
        let page_json = page.map_or(serde_json::Value::Null, |p| {
            serde_json::json!({ "content_type": p.content_type, "bytes": p.body.len() })
        });
        let texts: serde_json::Map<String, serde_json::Value> = profile
            .texts
            .into_iter()
            .map(|text| (text.key, text.value.into()))
            .collect();
        let (kind_str, expires, expires_date, lapsed) = match sub.kind {
            SubnameKind::Permanent => ("permanent", None, "never".to_owned(), false),
            SubnameKind::Revocable { .. } => {
                let exp = parent_record.as_ref().map_or(0, |p| p.expires);
                let exp_opt = (exp != u64::MAX).then_some(exp);
                let exp_str = if exp == u64::MAX { "never".to_owned() } else { readable(exp) };
                ("revocable", exp_opt, exp_str, local_now() >= exp)
            }
        };
        Ok(serde_json::json!({
            "name": name,
            "parent": parent,
            "subname": true,
            "subname_kind": kind_str,
            "held_by": self.subname_holder(name, &sub_derived.id, &sub).ok().map(|a| a.to_string()),
            "expires": expires,
            "expires_date": expires_date,
            "lapsed": lapsed,
            "payable_privately": profile.npk.is_some() && !profile.vpk.is_empty(),
            "public_account": profile.account.map(|a| AccountId::new(a).to_string()),
            "texts": texts,
            "page": page_json,
        }))
    }

    /// Edits a subname's profile (`SetSubnameRecord`).
    pub async fn edit_subname(
        &mut self,
        name: &str,
        change: impl FnOnce(&mut Profile) -> Result<()>,
    ) -> Result<()> {
        let (parent_slot, sub_derived, sub, _, mut profile) =
            self.lookup_subname(name, false).await?;
        let holder = self.subname_holder(name, &sub_derived.id, &sub)?;
        change(&mut profile)?;
        validate_profile(&profile).map_err(|reason| anyhow!(reason))?;
        let sealed = seal_profile(&sub_derived.key, &profile);
        let [sub_rec, _] = subname_seeds(&parent_slot, &sub_derived.id);
        let mut mentions = vec![self.own_actor(holder), self.registry_actor(sub_rec)];
        let now = match sub.kind {
            SubnameKind::Permanent => None,
            SubnameKind::Revocable { .. } => {
                mentions.push(self.registry_actor(name_seeds(&parent_slot)[0]));
                Some(self.now().await)
            }
        };
        self.send(
            mentions,
            &Instruction::SetSubnameRecord {
                parent_id: parent_slot,
                sub_id: sub_derived.id,
                sealed,
                now,
            },
            0,
        )
        .await
    }

    /// Sets or clears a subname's page (`SetSubnamePage`).
    pub async fn set_page_subname(&mut self, name: &str, page: Option<PageContent>) -> Result<()> {
        let (parent_slot, sub_derived, sub, _, _) = self.lookup_subname(name, false).await?;
        let holder = self.subname_holder(name, &sub_derived.id, &sub)?;
        let sealed = match page {
            Some(p) => {
                validate_page(&p).map_err(|reason| anyhow!(reason))?;
                seal_page(&sub_derived.key, &p)
            }
            None => Vec::new(),
        };
        let [sub_rec, sub_page] = subname_seeds(&parent_slot, &sub_derived.id);
        let mut mentions = vec![
            self.own_actor(holder),
            self.registry_actor(sub_rec),
            self.registry_actor(sub_page),
        ];
        let now = match sub.kind {
            SubnameKind::Permanent => None,
            SubnameKind::Revocable { .. } => {
                mentions.push(self.registry_actor(name_seeds(&parent_slot)[0]));
                Some(self.now().await)
            }
        };
        self.send(
            mentions,
            &Instruction::SetSubnamePage {
                parent_id: parent_slot,
                sub_id: sub_derived.id,
                sealed,
                now,
            },
            0,
        )
        .await
    }

    /// Transfers a subname (`TransferSubname`).
    pub async fn transfer_subname(&mut self, name: &str, new_owner: AccountId) -> Result<()> {
        if new_owner == self.program {
            bail!("that is the registry program's account, which can't hold a name");
        }
        let (parent_slot, sub_derived, sub, _, _) = self.lookup_subname(name, false).await?;
        if owner_tag(&sub_derived.id, &new_owner) == sub.owner {
            bail!("{new_owner} already holds {name}, so nothing was sent");
        }
        let holder = self.subname_holder(name, &sub_derived.id, &sub)?;
        let [sub_rec, sub_page] = subname_seeds(&parent_slot, &sub_derived.id);
        let mut mentions = vec![
            self.own_actor(holder),
            self.registry_actor(sub_rec),
            self.registry_actor(sub_page),
        ];
        let now = match sub.kind {
            SubnameKind::Permanent => None,
            SubnameKind::Revocable { .. } => {
                mentions.push(self.registry_actor(name_seeds(&parent_slot)[0]));
                Some(self.now().await)
            }
        };
        self.send(
            mentions,
            &Instruction::TransferSubname {
                parent_id: parent_slot,
                sub_id: sub_derived.id,
                new_owner: owner_tag(&sub_derived.id, &new_owner),
                now,
            },
            0,
        )
        .await
    }

    /// Releases a subname (`ReleaseSubname`).
    pub async fn release_subname(&mut self, name: &str) -> Result<()> {
        let (parent_slot, sub_derived, sub, _, _) = self.lookup_subname(name, false).await?;
        let holder = self.subname_holder(name, &sub_derived.id, &sub)?;
        let [sub_rec, sub_page] = subname_seeds(&parent_slot, &sub_derived.id);
        let mut mentions = vec![
            self.own_actor(holder),
            self.registry_actor(sub_rec),
            self.registry_actor(sub_page),
        ];
        let now = match sub.kind {
            SubnameKind::Permanent => None,
            SubnameKind::Revocable { .. } => {
                mentions.push(self.registry_actor(name_seeds(&parent_slot)[0]));
                Some(self.now().await)
            }
        };
        self.send(
            mentions,
            &Instruction::ReleaseSubname {
                parent_id: parent_slot,
                sub_id: sub_derived.id,
                now,
            },
            0,
        )
        .await
    }

    // ── Subname management commands ────────────────────────────────────

    pub async fn subname(&mut self, command: SubnameCommand) -> Result<()> {
        match command {
            SubnameCommand::Create {
                name,
                owner,
                permanent,
                revocable: _,
            } => self.subname_create(&name, owner, !permanent).await,
            SubnameCommand::Revoke { name } => self.subname_revoke(&name).await,
            SubnameCommand::Sale {
                parent,
                price,
                payout,
                off,
            } => self.subname_sale(&parent, price, payout, off).await,
            SubnameCommand::Buy { name, owner, wait } => {
                let owner = match owner {
                    Some(o) => o,
                    None => default_private()?,
                };
                self.subname_buy(&name, owner, wait).await
            }
        }
    }

    /// Creates a subname under a parent held by this wallet (`CreateSubname`).
    async fn subname_create(
        &mut self,
        name: &str,
        owner: Option<AccountId>,
        revocable: bool,
    ) -> Result<()> {
        let (_, parent) = require_subname(name)?;
        let (parent_derived, parent_record, _) = self.lookup_held(parent).await?;
        let parent_slot = parent_derived.id;
        let parent_holder = self.holder(parent, &parent_slot, &parent_record)?;
        let sub_owner = owner.unwrap_or(parent_holder);
        let sub_derived = derive(name);
        let now = self.now().await;
        if now >= parent_record.expires {
            bail!("parent {parent} has lapsed; renew it before creating subnames");
        }
        let kind_word = if revocable { "revocable" } else { "permanent" };
        println!("creating {kind_word} subname {name} for {sub_owner}");
        let [parent_rec, ..] = name_seeds(&parent_slot);
        let [sub_rec, sub_page] = subname_seeds(&parent_slot, &sub_derived.id);
        let mentions = vec![
            self.own_actor(parent_holder),
            self.registry_actor(parent_rec),
            self.registry_actor(sub_rec),
            self.registry_actor(sub_page),
        ];
        self.send(
            mentions,
            &Instruction::CreateSubname {
                parent_id: parent_slot,
                sub_id: sub_derived.id,
                owner: owner_tag(&sub_derived.id, &sub_owner),
                revocable,
                sealed: Vec::new(),
                now,
            },
            0,
        )
        .await?;
        if sub_owner == parent_holder {
            let mut defaults = load_defaults();
            if !defaults.names.contains(&name.to_owned()) {
                defaults.names.push(name.to_owned());
                save_defaults(&defaults)?;
            }
        }
        println!("{name} created ({kind_word})");
        Ok(())
    }

    /// Revokes a revocable subname under a parent held by this wallet (`RevokeSubname`).
    async fn subname_revoke(&mut self, name: &str) -> Result<()> {
        let (_, parent) = require_subname(name)?;
        let (parent_derived, parent_record, _) = self.lookup_held(parent).await?;
        let parent_slot = parent_derived.id;
        let parent_holder = self.holder(parent, &parent_slot, &parent_record)?;
        let sub_derived = derive(name);
        let sub = self
            .subname_record(&parent_slot, &sub_derived.id)
            .await?
            .ok_or_else(|| anyhow!("{name} is not registered"))?;
        if matches!(sub.kind, SubnameKind::Permanent) {
            bail!("{name} is a permanent subname and cannot be revoked");
        }
        let now = self.now().await;
        let [parent_rec, ..] = name_seeds(&parent_slot);
        let [sub_rec, sub_page] = subname_seeds(&parent_slot, &sub_derived.id);
        let mentions = vec![
            self.own_actor(parent_holder),
            self.registry_actor(parent_rec),
            self.registry_actor(sub_rec),
            self.registry_actor(sub_page),
        ];
        self.send(
            mentions,
            &Instruction::RevokeSubname {
                parent_id: parent_slot,
                sub_id: sub_derived.id,
                now,
            },
            0,
        )
        .await?;
        println!("{name} revoked");
        Ok(())
    }

    /// Turns subname sales on or off on a parent name (`SetSubnameSale`).
    async fn subname_sale(
        &mut self,
        parent: &str,
        price: Option<u128>,
        payout: Option<AccountId>,
        off: bool,
    ) -> Result<()> {
        if split_subname(parent).is_some() {
            bail!("subnames cannot sell subnames; give a top-level parent name");
        }
        let (parent_derived, parent_record, _) = self.lookup_held(parent).await?;
        let parent_slot = parent_derived.id;
        let parent_holder = self.holder(parent, &parent_slot, &parent_record)?;
        let sale = if off {
            println!("turning subname sales off for {parent}");
            None
        } else {
            let price = price.ok_or_else(|| anyhow!("give --price <amount> or --off"))?;
            let payout = match payout {
                Some(p) => p,
                None if !self.is_private(parent_holder) => parent_holder,
                None => bail!("--payout <Public/account> is required when the parent is held privately"),
            };
            println!("enabling permanent subname sales on {parent}: price {price}, payout Public/{payout}");
            Some(SubnameSale { price, payout })
        };
        let [parent_rec, ..] = name_seeds(&parent_slot);
        self.send(
            vec![self.own_actor(parent_holder), self.registry_actor(parent_rec)],
            &Instruction::SetSubnameSale { id: parent_slot, sale },
            0,
        )
        .await?;
        if off {
            println!("subname sales disabled for {parent}");
        } else {
            println!("subname sales enabled for {parent}");
        }
        Ok(())
    }

    /// Buys a permanent subname (`CommitPublic`/`Commit` -> wait -> `BuySubname`).
    async fn subname_buy(&mut self, name: &str, owner: AccountId, wait: bool) -> Result<()> {
        let (_, parent) = require_subname(name)?;
        let (parent_derived, parent_record, _) = self.lookup(parent).await?;
        let parent_slot = parent_derived.id;
        let sale = parent_record
            .subname_sale
            .ok_or_else(|| anyhow!("{parent} does not have subname sales enabled"))?;
        let sub_derived = derive(name);
        if let Some(existing) = self.subname_record(&parent_slot, &sub_derived.id).await? {
            let taken = match existing.kind {
                SubnameKind::Permanent => true,
                SubnameKind::Revocable { parent_generation } => {
                    parent_generation == parent_record.generation
                }
            };
            if taken {
                bail!("{name} is already registered");
            }
        }
        println!(
            "buying permanent subname {name} from {parent} for {} (paid to Public/{})",
            sale.price, sale.payout
        );
        let is_public_owner = !self.is_private(owner);
        if is_public_owner && sale.price > 0 {
            let have = self.spendable(owner).await?;
            if have < sale.price {
                bail!(
                    "{owner} holds {have}, less than the subname price of {}; add funds first",
                    sale.price
                );
            }
        }
        let mut secret = [0u8; 32];
        getrandom::fill(&mut secret).map_err(|e| anyhow!("no randomness: {e}"))?;
        let buyer_tag = owner_tag(&sub_derived.id, &owner);
        let promised = subname_commitment(&parent_slot, &sub_derived.id, &buyer_tag, &secret);
        let made = self.claim_time(owner).await?;
        let commit_ix = if is_public_owner {
            Instruction::CommitPublic { commitment: promised, now: made }
        } else {
            Instruction::Commit { commitment: promised, now: made }
        };
        println!("step 1 of 2: committing to {name}");
        self.send(
            vec![self.own_actor(owner), self.registry_actor(commitment_seed(&promised))],
            &commit_ix,
            0,
        )
        .await?;
        let wait_ms = commit_wait_ms(is_public_owner);
        let ready = made.saturating_add(wait_ms);
        if !wait {
            println!(
                "committed to {name}; reveal after {} (wait {} minutes)",
                readable(ready),
                wait_ms / 60_000
            );
            return Ok(());
        }
        loop {
            let chain_now = if is_public_owner {
                self.chain_now().await?
            } else {
                self.now().await
            };
            if chain_now >= ready {
                break;
            }
            let secs = (ready - chain_now).div_ceil(1000);
            println!(
                "waiting {secs}s before revealing subname purchase (commitment must be {} minutes old)",
                wait_ms / 60_000
            );
            tokio::time::sleep(std::time::Duration::from_secs(secs.clamp(5, 30))).await;
        }
        let (receiving, npk, vpk) = self.new_receiving_keys()?;
        let profile = Profile {
            name: name.to_owned(),
            npk: Some(npk),
            vpk,
            ..Profile::default()
        };
        let sealed = seal_profile(&sub_derived.key, &profile);
        println!("payments to {name} will arrive privately at Private/{receiving}");
        let claimed = self.claim_time(owner).await?;
        let [parent_rec, ..] = name_seeds(&parent_slot);
        let [sub_rec, sub_page] = subname_seeds(&parent_slot, &sub_derived.id);
        let mut mentions = vec![
            self.own_actor(owner),
            self.registry_actor(parent_rec),
            self.registry_actor(sub_rec),
            self.registry_actor(sub_page),
            self.registry_actor(commitment_seed(&promised)),
        ];
        let payout = if sale.price > 0 {
            mentions.push(self.signer(owner).balance());
            mentions.push(AccountIdentity::PublicNoSign(sale.payout).balance());
            Some(sale.payout)
        } else {
            None
        };
        println!("step 2 of 2: buying {name} for {}", sale.price);
        self.send(
            mentions,
            &Instruction::BuySubname {
                parent_id: parent_slot,
                sub_id: sub_derived.id,
                sealed,
                price: sale.price,
                payout,
                now: claimed,
                secret,
            },
            sale.price,
        )
        .await?;
        let mut defaults = load_defaults();
        if !defaults.names.contains(&name.to_owned()) {
            defaults.names.push(name.to_owned());
            save_defaults(&defaults)?;
        }
        println!("{name} is yours (permanent subname)");
        Ok(())
    }

    // ── Marketplace & NFT commands ─────────────────────────────────────

    /// Lists a top-level name for sale (`Instruction::List`).
    pub async fn list_name(
        &mut self,
        name: &str,
        price: u128,
        pay_to: Option<AccountId>,
        expires: Option<&str>,
        public_name: bool,
    ) -> Result<()> {
        if split_subname(name).is_some() {
            bail!("only top-level names can be listed for sale on the marketplace");
        }
        if price < MIN_SALE_PRICE {
            bail!("listing price must be at least {MIN_SALE_PRICE} (so the 2.5% fee is non-zero)");
        }
        let (derived, record, _) = self.lookup_held(name).await?;
        let slot = derived.id;
        let holder = self.holder(name, &slot, &record)?;
        if record.nft_shielded {
            bail!("{name} has a shielded NFT deed; unshield it first (`names unshield-deed {name}`)");
        }
        let pay_to = match pay_to {
            Some(a) => a,
            None if !self.is_private(holder) => holder,
            None => bail!("--pay-to <Public/account> is required when listing from a private account"),
        };
        let now = self.now().await;
        let expires_at = match expires {
            Some(spec) => Some(parse_expires(spec, now).map_err(|e| anyhow!(e))?),
            None => None,
        };
        let (seller, fee) = sale_split(price);
        println!(
            "listing {name} for {price} (2.5% protocol fee: {fee}, seller receives: {seller} to Public/{pay_to})"
        );
        if public_name {
            println!("  public marketplace listing: {name} will be published in plaintext");
        } else {
            println!("  private listing: name stays hidden off the public marketplace list");
        }
        let display_name = public_name.then(|| name.to_owned());
        let [rec_seed, ..] = name_seeds(&slot);
        let mut mentions = vec![self.own_actor(holder), self.registry_actor(rec_seed)];
        if public_name || record.listing.as_ref().is_some_and(|l| l.in_market) {
            mentions.push(self.market_bucket_actor(&slot));
        }
        self.send(
            mentions,
            &Instruction::List {
                id: slot,
                price,
                pay_to,
                expires_at,
                now,
                display_name,
            },
            0,
        )
        .await?;
        println!("{name} is now listed for sale at {price}");
        Ok(())
    }

    /// Cancels an active listing (`Instruction::Cancel`).
    pub async fn cancel_listing(&mut self, name: &str) -> Result<()> {
        if split_subname(name).is_some() {
            bail!("subnames cannot be listed on the marketplace");
        }
        let (derived, record, _) = self.lookup_held(name).await?;
        let slot = derived.id;
        let holder = self.holder(name, &slot, &record)?;
        let Some(listing) = &record.listing else {
            bail!("{name} is not listed for sale");
        };
        let [rec_seed, ..] = name_seeds(&slot);
        let mut mentions = vec![self.own_actor(holder), self.registry_actor(rec_seed)];
        if listing.in_market {
            mentions.push(self.market_bucket_actor(&slot));
        }
        self.send(mentions, &Instruction::Cancel { id: slot }, 0).await?;
        println!("cancelled listing for {name}");
        Ok(())
    }

    /// Buys a listed top-level name (`Instruction::Buy`).
    pub async fn buy_name(&mut self, name: &str, buyer: AccountId) -> Result<()> {
        if split_subname(name).is_some() {
            bail!("subnames are bought with `names subname buy {name}`");
        }
        let settings = self
            .settings()
            .await?
            .ok_or_else(|| anyhow!("this registry has no settings configured"))?;
        let (derived, record, _) = self.lookup(name).await?;
        let slot = derived.id;
        let listing = record
            .listing
            .as_ref()
            .ok_or_else(|| anyhow!("{name} is not listed for sale"))?;
        let now = self.now().await;
        if let Some(exp) = listing.expires_at {
            if now >= exp {
                bail!("the listing for {name} expired at {}", readable(exp));
            }
        }
        if owner_tag(&slot, &buyer) == record.owner {
            bail!("you already hold {name}");
        }
        let (seller, fee) = sale_split(listing.price);
        println!(
            "buying {name} for {} (seller Public/{} receives {seller}; 2.5% treasury fee Public/{}: {fee})",
            listing.price, listing.pay_to, settings.treasury
        );
        if !self.is_private(buyer) {
            let have = self.spendable(buyer).await?;
            if have < listing.price {
                bail!(
                    "{buyer} holds {have}, less than the listing price of {}; add funds first",
                    listing.price
                );
            }
        }
        let mut mentions = self.on_record_and_page(&slot, buyer);
        mentions.push(self.registry_actor(settings_seed()));
        self.push_nft_mention(&mut mentions, Some(&settings), &record.nft_token_id);
        self.push_cleanup_mentions(&mut mentions, &slot, &record);
        mentions.push(self.signer(buyer).balance());
        mentions.push(AccountIdentity::PublicNoSign(listing.pay_to).balance());
        if settings.treasury != listing.pay_to {
            mentions.push(AccountIdentity::PublicNoSign(settings.treasury).balance());
        }
        self.send(
            mentions,
            &Instruction::Buy {
                id: slot,
                price: listing.price,
                pay_to: listing.pay_to,
                treasury: settings.treasury,
                expected_nonce: listing.nonce,
                now,
            },
            listing.price,
        )
        .await?;
        let mut defaults = load_defaults();
        if !defaults.names.contains(&name.to_owned()) {
            defaults.names.push(name.to_owned());
            save_defaults(&defaults)?;
        }
        println!("{name} is yours! Run `names receive-privately {name}` to set up private payments.");
        Ok(())
    }

    /// Reads the 16 `MarketBucket` PDAs, verifies any plaintext `display_name` via Argon2id,
    /// and prints active listings.
    pub async fn market(&self, json_out: bool) -> Result<()> {
        let now = local_now();
        let mut rows = Vec::new();
        for bucket in 0u8..16 {
            let bytes = self
                .actor_state(market_bucket_address(&self.program, bucket))
                .await?;
            if bytes.is_empty() {
                continue;
            }
            let parsed: MarketBucket =
                borsh::from_slice(&bytes).context("market bucket did not decode")?;
            for entry in parsed.entries {
                if let Some(exp) = entry.expires_at {
                    if now >= exp {
                        continue;
                    }
                }
                if now >= entry.name_expires.saturating_add(GRACE_MS) {
                    continue;
                }
                let verified_name = verify_market_entry(&entry);
                let (seller, fee) = sale_split(entry.price);
                rows.push(serde_json::json!({
                    "id": hex::encode(entry.id),
                    "name": verified_name,
                    "price": entry.price.to_string(),
                    "fee": fee.to_string(),
                    "seller_receives": seller.to_string(),
                    "pay_to": entry.pay_to.to_string(),
                    "expires_at": entry.expires_at,
                    "name_expires": (entry.name_expires != u64::MAX).then_some(entry.name_expires),
                    "price_tier": entry.price_tier,
                    "nonce": entry.nonce,
                    "nft_token_id": hex::encode(entry.nft_token_id),
                }));
            }
        }
        if json_out {
            println!("{}", serde_json::to_string_pretty(&rows)?);
            return Ok(());
        }
        if rows.is_empty() {
            println!("no names are currently listed in the public marketplace");
            return Ok(());
        }
        println!("{} name(s) listed for sale:", rows.len());
        for row in &rows {
            let label = row["name"]
                .as_str()
                .map(|n| format!("{n}{NAME_SUFFIX}"))
                .unwrap_or_else(|| format!("(unverified {})", &row["id"].as_str().unwrap_or("")[..12]));
            println!(
                "  {label:<24}  price {} (fee {}, seller {})",
                row["price"].as_str().unwrap_or("0"),
                row["fee"].as_str().unwrap_or("0"),
                row["seller_receives"].as_str().unwrap_or("0"),
            );
        }
        Ok(())
    }

    /// Transfers a top-level name via its linked NFT (`lez_nft::Instruction::Transfer`),
    /// which calls back into the registry's `Message::NftTransferHook`.
    ///
    /// NOTE: Depends on the unconfirmed `lez_nft` v3 transfer wire encoding — see
    /// [`encode_lez_nft_transfer`] and `neon-job/CHANGES.md`.
    pub async fn transfer_via_nft(&mut self, name: &str, new_owner: AccountId) -> Result<()> {
        if split_subname(name).is_some() {
            bail!("subnames do not have linked NFTs; use `names transfer {name} <new_owner>`");
        }
        let settings = self
            .settings()
            .await?
            .ok_or_else(|| anyhow!("this registry has no settings configured"))?;
        if settings.nft_program == AccountId::new([0u8; 32]) {
            bail!("this registry does not have an NFT program configured");
        }
        let (derived, record, _) = self.lookup_held(name).await?;
        let slot = derived.id;
        if record.nft_token_id == [0u8; 32] {
            bail!("{name} has no linked NFT token ID");
        }
        if record.nft_shielded {
            bail!("{name}'s NFT deed is shielded; unshield it first (`names unshield-deed {name}`)");
        }
        let holder = self.holder(name, &slot, &record)?;
        if owner_tag(&slot, &new_owner) == record.owner {
            bail!("{new_owner} already holds {name}");
        }
        let now = self.now().await;
        let new_tag = owner_tag(&slot, &new_owner);
        let message = encode_lez_nft_transfer(&record.nft_token_id, &new_tag, now, &slot);
        let [rec_seed, page_seed, _] = name_seeds(&slot);
        let mut mentions = vec![
            self.signer(holder).select_program_actor_state(settings.nft_program),
            self.nft_actor(settings.nft_program, &record.nft_token_id),
            self.registry_actor(rec_seed),
            self.registry_actor(page_seed),
            self.registry_actor(settings_seed()),
        ];
        self.push_cleanup_mentions(&mut mentions, &slot, &record);
        let programs = self.programs_for(&mentions).await?;
        println!(
            "transferring {name} via its linked NFT ({}) on {}",
            hex::encode(&record.nft_token_id[..8]),
            settings.nft_program
        );
        self.transact(mentions, message, programs, 0, crate::CHANGE_CYCLES)
            .await
    }

    /// Shields a name's NFT deed (`Instruction::ShieldDeed`).
    pub async fn shield_deed(&mut self, name: &str, deed_account: AccountId) -> Result<()> {
        if split_subname(name).is_some() {
            bail!("subnames do not have linked NFT deeds");
        }
        let settings = self.settings().await?;
        let (derived, record, _) = self.lookup_held(name).await?;
        let slot = derived.id;
        let holder = self.holder(name, &slot, &record)?;
        let now = self.now().await;
        let [rec_seed, ..] = name_seeds(&slot);
        let mut mentions = vec![
            self.own_actor(holder),
            self.registry_actor(rec_seed),
            self.registry_actor(settings_seed()),
        ];
        self.push_nft_mention(&mut mentions, settings.as_ref(), &record.nft_token_id);
        self.send(
            mentions,
            &Instruction::ShieldDeed {
                id: slot,
                deed_account,
                now,
            },
            0,
        )
        .await
    }

    /// Unshields a name's NFT deed (`Instruction::UnshieldDeed`).
    pub async fn unshield_deed(&mut self, name: &str) -> Result<()> {
        if split_subname(name).is_some() {
            bail!("subnames do not have linked NFT deeds");
        }
        let settings = self.settings().await?;
        let (derived, record, _) = self.lookup_held(name).await?;
        let slot = derived.id;
        let holder = self.holder(name, &slot, &record)?;
        let [rec_seed, ..] = name_seeds(&slot);
        let mut mentions = vec![
            self.own_actor(holder),
            self.registry_actor(rec_seed),
            self.registry_actor(settings_seed()),
        ];
        self.push_nft_mention(&mut mentions, settings.as_ref(), &record.nft_token_id);
        self.send(mentions, &Instruction::UnshieldDeed { id: slot }, 0)
            .await
    }

    /// Refreshes a shielded NFT deed (`Instruction::RefreshShieldedDeed`).
    pub async fn refresh_deed(&mut self, name: &str) -> Result<()> {
        if split_subname(name).is_some() {
            bail!("subnames do not have linked NFT deeds");
        }
        let settings = self.settings().await?;
        let (derived, record, _) = self.lookup_held(name).await?;
        let slot = derived.id;
        let holder = self.holder(name, &slot, &record)?;
        let [rec_seed, ..] = name_seeds(&slot);
        let mut mentions = vec![
            self.own_actor(holder),
            self.registry_actor(rec_seed),
            self.registry_actor(settings_seed()),
        ];
        self.push_nft_mention(&mut mentions, settings.as_ref(), &record.nft_token_id);
        self.send(mentions, &Instruction::RefreshShieldedDeed { id: slot }, 0)
            .await
    }

    // ── Sites directory ────────────────────────────────────────────────

    /// Reads `SiteDirHeader` + all `SiteBucket(0..16, 0..=active_page[b])`, verifies every entry
    /// with Argon2id + `tiered_id`, filters expired names, and prints the list.
    pub async fn sites(&self, json_out: bool) -> Result<()> {
        let header = self.site_dir_header().await?;
        let now = local_now();
        let mut sites: Vec<SiteEntry> = Vec::new();
        for bucket in 0u8..16 {
            let max_page = header.active_page[usize::from(bucket)];
            for page in 0..=max_page {
                let bytes = self
                    .actor_state(site_bucket_address(&self.program, bucket, page))
                    .await?;
                if bytes.is_empty() {
                    continue;
                }
                let parsed: SiteBucket =
                    borsh::from_slice(&bytes).context("site directory bucket did not decode")?;
                for entry in parsed.entries {
                    if now >= entry.name_expires.saturating_add(GRACE_MS) {
                        continue;
                    }
                    if !verify_site_entry(&entry) {
                        continue;
                    }
                    sites.push(entry);
                }
            }
        }
        sites.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then_with(|| a.name.cmp(&b.name)));
        if json_out {
            let out: Vec<serde_json::Value> = sites
                .iter()
                .map(|s| {
                    serde_json::json!({
                        "name": s.name,
                        "title": s.title,
                        "updated_at": s.updated_at,
                        "name_expires": (s.name_expires != u64::MAX).then_some(s.name_expires),
                    })
                })
                .collect();
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "total_listed": header.total_listed,
                    "sites": out,
                }))?
            );
            return Ok(());
        }
        if sites.is_empty() {
            println!("no LNS sites are currently listed in the directory");
            return Ok(());
        }
        println!("{} LNS site(s):", sites.len());
        for site in &sites {
            println!("  {:<24}  {}", site.name, site.title);
        }
        Ok(())
    }

    // ── Admin commands ─────────────────────────────────────────────────

    /// Updates one cell in the 3x4 price table (`Instruction::SetPrice`).
    pub async fn set_price(
        &mut self,
        price_tier: u8,
        term: u8,
        price: u128,
        signer: Option<AccountId>,
    ) -> Result<()> {
        let settings = self
            .settings()
            .await?
            .ok_or_else(|| anyhow!("this registry is not configured yet; run `names configure` first"))?;
        let signer = signer.unwrap_or(settings.admin);
        println!(
            "setting price for {} / {} to {price}",
            tier_label(price_tier),
            term_label(term)
        );
        self.send(
            vec![self.own_actor(signer), self.registry_actor(settings_seed())],
            &Instruction::SetPrice {
                price_tier,
                term,
                price,
            },
            0,
        )
        .await
    }

    /// Updates the 21-day re-release falling premium starting price (`Instruction::SetPremium`).
    pub async fn set_premium(&mut self, premium_start: u128, signer: Option<AccountId>) -> Result<()> {
        let settings = self
            .settings()
            .await?
            .ok_or_else(|| anyhow!("this registry is not configured yet; run `names configure` first"))?;
        let signer = signer.unwrap_or(settings.admin);
        println!("setting re-release premium start to {premium_start}");
        self.send(
            vec![self.own_actor(signer), self.registry_actor(settings_seed())],
            &Instruction::SetPremium { premium_start },
            0,
        )
        .await
    }

    /// Updates the treasury account while preserving all other `Settings` fields (`Instruction::Configure`).
    pub async fn set_treasury(
        &mut self,
        treasury: AccountId,
        signer: Option<AccountId>,
    ) -> Result<()> {
        let settings = self
            .settings()
            .await?
            .ok_or_else(|| anyhow!("this registry is not configured yet; run `names configure` first"))?;
        let signer = signer.unwrap_or(settings.admin);
        println!("setting registry treasury to Public/{treasury}");
        self.send(
            vec![self.own_actor(signer), self.registry_actor(settings_seed())],
            &Instruction::Configure {
                admin: settings.admin,
                treasury,
                prices: settings.prices,
                mode: settings.mode,
                deposits: settings.deposits,
                claims_root: settings.claims_root,
                claims_until: settings.claims_until,
                claims_program: settings.claims_program,
                premium_start: settings.premium_start,
                nft_program: settings.nft_program,
            },
            0,
        )
        .await
    }
}

/// Computes the exact re-release premium (if any) when registering over a lapsed name at `now`.
pub fn lapsed_premium(
    settings: Option<&names_core::Settings>,
    existing: Option<&NameRecord>,
    now: u64,
) -> (u128, Option<u64>) {
    let Some(s) = settings else {
        return (0, None);
    };
    if s.mode != MODE_FEE || s.premium_start == 0 {
        return (0, None);
    }
    let Some(rec) = existing else {
        return (0, None);
    };
    if rec.expires == u64::MAX {
        return (0, None);
    }
    let grace_end = rec.expires.saturating_add(GRACE_MS);
    if now < grace_end {
        return (0, None);
    }
    let elapsed = now.saturating_sub(grace_end);
    let premium = rerelease_premium(s.premium_start, elapsed);
    let premium_ends = grace_end.saturating_add(PREMIUM_DURATION_MS);
    (premium, (premium > 0).then_some(premium_ends))
}

/// Formats `Settings.prices` as a readable JSON object for `names settings`.
pub fn prices_json(prices: &[[u128; 4]; 3]) -> serde_json::Value {
    let row = |r: &[u128; 4]| {
        serde_json::json!({
            "30d": r[0].to_string(),
            "1y": r[1].to_string(),
            "5y": r[2].to_string(),
            "permanent": r[3].to_string(),
        })
    };
    serde_json::json!({
        "tier_5": row(&prices[0]),
        "tier_6_7": row(&prices[1]),
        "tier_8_plus": row(&prices[2]),
    })
}

/// Helper to read `DEFAULT_PRICES` for documentation/tests.
pub fn default_prices_flat() -> Vec<u128> {
    DEFAULT_PRICES.iter().flatten().copied().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser as _;

    #[test]
    fn slot_id_and_price_tiers_match_core() {
        let base = [0x42u8; 32];
        assert_eq!(price_tier_for_len(5), PRICE_TIER_5);
        assert_eq!(price_tier_for_len(6), PRICE_TIER_6_7);
        assert_eq!(price_tier_for_len(7), PRICE_TIER_6_7);
        assert_eq!(price_tier_for_len(8), PRICE_TIER_8_PLUS);
        assert_eq!(slot_id("alice", &base), tiered_id(&base, PRICE_TIER_5));
        assert_eq!(slot_id("daniel", &base), tiered_id(&base, PRICE_TIER_6_7));
        assert_eq!(slot_id("longname", &base), tiered_id(&base, PRICE_TIER_8_PLUS));
        assert_ne!(slot_id("alice", &base), base);
    }

    #[test]
    fn parse_price_tier_and_prices_matrix() {
        assert_eq!(parse_price_tier("5"), Ok(0));
        assert_eq!(parse_price_tier("6-7"), Ok(1));
        assert_eq!(parse_price_tier("8+"), Ok(2));
        assert!(parse_price_tier("9").is_err());

        let flat = default_prices_flat();
        assert_eq!(parse_prices_matrix(&flat), Ok(DEFAULT_PRICES));
        assert_eq!(
            parse_prices_matrix(&[10, 20, 30, 40]),
            Ok([[10, 20, 30, 40]; 3])
        );
        assert_eq!(
            parse_prices_matrix(&[10, 20, 30]),
            Ok([[10, 20, 30, 0]; 3])
        );
        assert!(parse_prices_matrix(&[1, 2]).is_err());
    }

    #[test]
    fn parse_expires_accepts_durations_and_timestamps() {
        let now = 1_700_000_000_000;
        assert_eq!(parse_expires("7d", now), Ok(now + 7 * 86_400_000));
        assert_eq!(parse_expires("24h", now), Ok(now + 24 * 3_600_000));
        assert_eq!(parse_expires("30m", now), Ok(now + 30 * 60_000));
        assert_eq!(parse_expires("1800000000000", now), Ok(1_800_000_000_000));
        assert!(parse_expires("0d", now).is_err());
        assert!(parse_expires("100", now).is_err());
    }

    #[test]
    fn discover_off_and_site_dir_ops() {
        let mut profile = Profile {
            name: "alice".to_owned(),
            ..Profile::default()
        };
        assert!(!discover_is_off(&profile));
        profile.texts.push(names_seal::TextRecord {
            key: "discover".to_owned(),
            value: " OFF ".to_owned(),
        });
        assert!(discover_is_off(&profile));

        let mut rec = NameRecord {
            version: VERSION,
            owner: [1; 32],
            expires: u64::MAX,
            sealed: Vec::new(),
            claimed: false,
            price_tier: 0,
            generation: [2; 32],
            subname_sale: None,
            nft_token_id: [3; 32],
            nft_shielded: false,
            listing: None,
            listing_nonce: 0,
            site_listed: Some(0),
        };
        // Setting discover=off on a listed site removes it.
        assert_eq!(
            site_dir_op_for_edit("alice", &rec, &profile, true, 1000, 0),
            SiteDirOp::Remove
        );
        // Publishing a page with discover=off removes an existing listing.
        assert_eq!(
            site_dir_op_for_page("alice", &rec, &profile, true, false, false, None, 1000, 0),
            SiteDirOp::Remove
        );
        // Removing a page removes an existing listing.
        profile.texts.clear();
        assert_eq!(
            site_dir_op_for_page("alice", &rec, &profile, false, false, false, None, 1000, 0),
            SiteDirOp::Remove
        );
        // Publishing a page with discover enabled upserts into active_page.
        rec.site_listed = None;
        assert_eq!(
            site_dir_op_for_page("alice", &rec, &profile, true, false, false, Some("Alice Site"), 1000, 3),
            SiteDirOp::Upsert {
                name: "alice".to_owned(),
                title: "Alice Site".to_owned(),
                page: 3,
                now: 1000,
            }
        );
    }

    #[test]
    fn cid_site_is_listed_like_an_onchain_page() {
        let cid = |v: &str| names_seal::TextRecord { key: "cid".to_owned(), value: v.to_owned() };
        let mut profile = Profile::default();
        profile.texts.push(cid("zDvZRwzm1234"));
        profile.texts.push(names_seal::TextRecord { key: "display".to_owned(), value: "Alice".to_owned() });
        let mut rec = NameRecord {
            version: VERSION,
            owner: [1; 32],
            expires: u64::MAX,
            sealed: Vec::new(),
            claimed: false,
            price_tier: 0,
            generation: [2; 32],
            subname_sale: None,
            nft_token_id: [3; 32],
            nft_shielded: false,
            listing: None,
            listing_nonce: 0,
            site_listed: None,
        };
        // `lns publish` = `names profile <name> --cid <root>`: no page, but a cid site -> listed.
        assert_eq!(
            site_dir_op_for_edit("alice", &rec, &profile, false, 1000, 2),
            SiteDirOp::Upsert { name: "alice".to_owned(), title: "Alice".to_owned(), page: 2, now: 1000 }
        );
        // Already listed: a routine edit touches no directory actor.
        rec.site_listed = Some(2);
        assert_eq!(site_dir_op_for_edit("alice", &rec, &profile, false, 1000, 2), SiteDirOp::Keep);
        // Removing the on-chain page keeps the listing while the cid site remains.
        assert_eq!(
            site_dir_op_for_page("alice", &rec, &profile, false, false, false, None, 1000, 2),
            SiteDirOp::Keep
        );
        // discover=off unlists a cid site.
        profile.texts.push(names_seal::TextRecord { key: DISCOVER_KEY.to_owned(), value: "off".to_owned() });
        assert_eq!(site_dir_op_for_edit("alice", &rec, &profile, false, 1000, 2), SiteDirOp::Remove);
        rec.site_listed = None;
        assert_eq!(site_dir_op_for_edit("alice", &rec, &profile, false, 1000, 2), SiteDirOp::Keep);
        // Clearing the cid with no page left unlists the name.
        profile.texts.retain(|t| t.key != DISCOVER_KEY && t.key != "cid");
        rec.site_listed = Some(2);
        assert_eq!(site_dir_op_for_edit("alice", &rec, &profile, false, 1000, 2), SiteDirOp::Remove);
        // An empty cid value is no site.
        profile.texts.push(cid("  "));
        assert!(!has_cid_site(&profile));
        rec.site_listed = None;
        assert_eq!(site_dir_op_for_edit("alice", &rec, &profile, false, 1000, 2), SiteDirOp::Keep);
        // A page alone still lists, as before.
        assert!(matches!(site_dir_op_for_edit("alice", &rec, &profile, true, 1000, 2), SiteDirOp::Upsert { .. }));
        // A lapsed name (in grace) is not listed: the registry would refuse the Upsert.
        rec.expires = 500;
        assert_eq!(site_dir_op_for_edit("alice", &rec, &profile, true, 1000, 2), SiteDirOp::Keep);
        assert_eq!(site_dir_op_for_page("alice", &rec, &profile, true, false, false, None, 1000, 2), SiteDirOp::Keep);
    }

    #[test]
    fn lapsed_premium_quotes_falling_window() {
        let settings = names_core::Settings {
            version: VERSION,
            admin: AccountId::new([1; 32]),
            treasury: AccountId::new([2; 32]),
            prices: DEFAULT_PRICES,
            mode: MODE_FEE,
            deposits: [0; 2],
            claims_root: [0; 32],
            claims_until: 0,
            claims_program: AccountId::new([0; 32]),
            premium_start: 100_000_000,
            nft_program: AccountId::new([9; 32]),
        };
        let rec = NameRecord {
            version: VERSION,
            owner: [1; 32],
            expires: 1_000_000,
            sealed: Vec::new(),
            claimed: false,
            price_tier: 2,
            generation: [0; 32],
            subname_sale: None,
            nft_token_id: [0; 32],
            nft_shielded: false,
            listing: None,
            listing_nonce: 0,
            site_listed: None,
        };
        let grace_end = rec.expires + GRACE_MS;
        // Still in grace: no premium.
        assert_eq!(lapsed_premium(Some(&settings), Some(&rec), grace_end - 1), (0, None));
        // Right at grace_end: full premium_start.
        let (p0, ends) = lapsed_premium(Some(&settings), Some(&rec), grace_end);
        assert_eq!(p0, 100_000_000);
        assert_eq!(ends, Some(grace_end + PREMIUM_DURATION_MS));
        // Halfway through 21 days: decays to roughly 1/(2^10.5) < 100_000_000 and > 0.
        let (mid, _) = lapsed_premium(Some(&settings), Some(&rec), grace_end + 86_400_000);
        assert_eq!(mid, 50_000_000);
        // Past 21 days: 0.
        assert_eq!(
            lapsed_premium(Some(&settings), Some(&rec), grace_end + PREMIUM_DURATION_MS),
            (0, None)
        );
    }

    #[test]
    fn encode_lez_nft_transfer_matches_lez_nft_borsh() {
        // borsh::to_vec(&lez_nft_core::Message::Instruction(Instruction::Transfer {
        //     token_id: [0xaa; 32], new_owner: [0xbb; 32], sealed: None, now: Some(42),
        //     linked_target: Some([0xcc; 32]) })) from lez-nft v3-lez04 (3562907).
        let expected = concat!(
            "0003",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "00012a0000000000000001",
            "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
        );
        assert_eq!(hex::encode(encode_lez_nft_transfer(&[0xAA; 32], &[0xBB; 32], 42, &[0xCC; 32])), expected);
    }

    #[test]
    fn nft_config_address_matches_lez_nft() {
        // lez_nft_core::config_address(&AccountId::new([0x11; 32])) from lez-nft v3-lez04.
        assert_eq!(
            hex::encode(nft_config_address(&AccountId::new([0x11; 32])).value()),
            "dccc936f962a2f004d6300848470c962dc8ce8a6619d1651fab9ea1eb1ad453a"
        );
    }


    #[test]
    fn v8_cli_commands_parse_cleanly() {
        assert!(crate::Cli::try_parse_from(["names", "register", "alice", "--term", "permanent"]).is_ok());
        assert!(crate::Cli::try_parse_from(["names", "renew", "alice", "--term", "permanent"]).is_ok());
        assert!(crate::Cli::try_parse_from(["names", "subname", "create", "blog.alice", "--permanent"]).is_ok());
        assert!(crate::Cli::try_parse_from(["names", "subname", "revoke", "blog.alice"]).is_ok());
        assert!(crate::Cli::try_parse_from(["names", "subname", "sale", "alice", "--price", "1,000", "--payout", "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk"]).is_ok());
        assert!(crate::Cli::try_parse_from(["names", "subname", "sale", "alice", "--off"]).is_ok());
        assert!(crate::Cli::try_parse_from(["names", "subname", "buy", "blog.alice"]).is_ok());
        assert!(crate::Cli::try_parse_from(["names", "list", "alice", "50,000", "--public-name"]).is_ok());
        assert!(crate::Cli::try_parse_from(["names", "cancel", "alice"]).is_ok());
        assert!(crate::Cli::try_parse_from(["names", "buy", "alice"]).is_ok());
        assert!(crate::Cli::try_parse_from(["names", "market", "--json"]).is_ok());
        assert!(crate::Cli::try_parse_from(["names", "sites", "--json"]).is_ok());
        assert!(crate::Cli::try_parse_from(["names", "transfer", "alice", "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", "--via-nft"]).is_ok());
        assert!(crate::Cli::try_parse_from(["names", "set-price", "--tier", "6-7", "--term", "permanent", "100,000,000"]).is_ok());
        assert!(crate::Cli::try_parse_from(["names", "set-premium", "1,000,000,000"]).is_ok());
        assert!(crate::Cli::try_parse_from(["names", "set-treasury", "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk"]).is_ok());
    }
}
