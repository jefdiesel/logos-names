//! Builds the transactions that change a name, following SPEC.md sections 8
//! and 8.3 (commit then reveal, deposits, the short-name list).
//!
//! Each function reads what it needs from the registry, checks what it can,
//! and returns a [`Change`] for a wallet to sign. The chain cannot check what
//! is inside a sealed profile, so the limits on entries, keys and pages are
//! enforced here. Checking first also turns a refused transaction into an
//! explanation.
//!
//! A change is one `Message::Instruction` to the signer's own actor of the
//! registry. It lists every actor the program reaches, and for each registry
//! account its seed, the evidence without which a transaction that creates the
//! account is refused and still charged. The lists are the ones the CLI
//! (`cli/src/main.rs`) makes. This module signs only with public accounts.

use crate::registry::{
    account_text, bucket_of, commitment, commitment_seed, deposit_seed, is_valid_name,
    market_bucket_seed, name_seed, nft_config_seed, nft_token_id, nft_token_seed, owner_tag, page_address, page_seed, pda,
    price_tier_for_len, register_generation, rerelease_premium, seal_page,
    discover_is_off, settings_seed, site_bucket_seed, site_dir_header_seed, site_title_for, slot_id, split_subname,
    AccountId, Derived, NameId,
    OwnerTag, Profile, Registry, Resolved, Settings, SiteDirOp, SubnameSale, Writer,
    MAX_SEALED_PAGE_LEN, MAX_SEALED_RECORD_LEN, NAME_SUFFIX, NATIVE_TOKEN, VPK_LEN,
};
use crate::shortlist::{length_proof, LengthProof, TIER_LOCKED, TIER_THREE};
use serde_json::{json, Value};

const MAX_TEXT_RECORDS: usize = 16;
const MAX_TEXT_KEY_LEN: usize = 32;
const MAX_TEXT_VALUE_LEN: usize = 512;
const MAX_CONTENT_TYPE_LEN: usize = 64;
/// Sealing adds a nonce, a tag and the page's own framing to the body.
const PAGE_OVERHEAD: usize = 12 + 16 + 4 + MAX_CONTENT_TYPE_LEN + 4;
/// The term a registration claims in fee mode: one year, the CLI's default.
/// In deposit mode the program ignores it.
pub const REGISTER_TERM: u8 = 1;

/// One actor a change reaches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mention {
    pub account: AccountId,
    /// Whose state on `account` it is: the registry's, the NFT program's, or the native balance.
    pub program: AccountId,
    /// The account's seed, when it is a PDA.
    pub seed: Option<[u8; 32]>,
    /// Program whose PDA `seed` belongs to when it differs from the registry (e.g. `nft_program`).
    pub pda_program: Option<AccountId>,
    /// Whether the signer's key signs for it.
    pub signs: bool,
}

/// One instruction for the registry, ready to sign and send.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    /// `borsh(Message::Instruction(..))`.
    pub message: Vec<u8>,
    /// The signer's own actor, then every other actor the message reaches.
    pub mentions: Vec<Mention>,
    /// The one account that signs and pays.
    pub signer: AccountId,
    /// What the signer must hold beyond the transaction's own gas: a deposit or fee.
    pub needs: u128,
    /// More to report with the change, merged into the answer (an object or null).
    pub extra: Value,
}

impl Change {
    /// A change from `signer`'s own actor of `program` to the actors in `reached`.
    fn new(program: &AccountId, message: Vec<u8>, signer: AccountId, reached: Vec<Mention>) -> Self {
        let mut mentions = vec![Mention { account: signer, program: *program, seed: None, pda_program: None, signs: true }];
        mentions.extend(reached);
        Self { message, mentions, signer, needs: 0, extra: Value::Null }
    }
}

/// The registry's account behind `seed`.
fn registry_actor(program: &AccountId, seed: [u8; 32]) -> Mention {
    Mention { account: pda(program, &seed), program: *program, seed: Some(seed), pda_program: None, signs: false }
}

/// An account behind `seed` under `other_program` (e.g. `lez_nft` v3 token PDA).
fn foreign_pda_actor(other_program: &AccountId, seed: [u8; 32]) -> Mention {
    Mention {
        account: pda(other_program, &seed),
        program: *other_program,
        seed: Some(seed),
        pda_program: Some(*other_program),
        signs: false,
    }
}

/// The native balance of the registry's account behind `seed`.
fn registry_balance(program: &AccountId, seed: [u8; 32]) -> Mention {
    Mention { account: pda(program, &seed), program: NATIVE_TOKEN, seed: Some(seed), pda_program: None, signs: false }
}

/// The native balance of the account that signs.
fn signer_balance(signer: AccountId) -> Mention {
    Mention { account: signer, program: NATIVE_TOKEN, seed: None, pda_program: None, signs: true }
}

/// The native balance of a plain account.
fn plain_balance(account: AccountId) -> Mention {
    Mention { account, program: NATIVE_TOKEN, seed: None, pda_program: None, signs: false }
}

/// An instruction, opening with `Message::Instruction`'s tag and its own.
fn instruction(variant: u8) -> Writer {
    Writer(vec![0, variant])
}

/// An instruction that opens with the name's identifier.
fn on_name(variant: u8, id: &NameId) -> Writer {
    let mut writer = instruction(variant);
    writer.0.extend(id);
    writer
}

/// v8 `Register { id, sealed, term, fee, treasury, now, secret, deposit, length, price_tier }`.
#[allow(clippy::too_many_arguments)]
pub fn encode_register(
    id: &NameId,
    sealed: &[u8],
    term: u8,
    fee: u128,
    treasury: Option<&AccountId>,
    now: u64,
    secret: &[u8; 32],
    deposit: bool,
    length: &LengthProof,
    price_tier: u8,
) -> Vec<u8> {
    let mut writer = on_name(0, id);
    writer.bytes(sealed);
    writer.u8(term);
    writer.u128(fee);
    writer.optional(treasury);
    writer.u64(now);
    writer.0.extend(secret);
    writer.bool(deposit);
    length.encode(&mut writer);
    writer.u8(price_tier);
    writer.0
}

/// v8 `Renew { id, term, fee, treasury, now }` (`0x01`).
pub fn encode_renew(id: &NameId, term: u8, fee: u128, treasury: Option<&AccountId>, now: u64) -> Vec<u8> {
    let mut writer = on_name(1, id);
    writer.u8(term);
    writer.u128(fee);
    writer.optional(treasury);
    writer.u64(now);
    writer.0
}

/// Whether the profile points the name at a Logos Storage site (a non-empty `cid` entry).
pub fn has_cid_site(profile: &Profile) -> bool {
    profile.texts.iter().any(|(k, v)| k == "cid" && !v.trim().is_empty())
}

/// The sites-directory step for a top-level name: `Some(true)` lists it, `Some(false)` unlists
/// it, `None` leaves the directory alone. Listed while it has a site and no `discover=off`.
fn site_dir_step(listed: bool, has_site: bool, discover_off: bool) -> Option<bool> {
    let want = has_site && !discover_off;
    (want != listed).then_some(want)
}

/// v8 `SetRecord { id, sealed, site_dir }` (`0x02`).
pub fn encode_set_record(id: &NameId, sealed: &[u8], site_dir: &SiteDirOp) -> Vec<u8> {
    let mut writer = on_name(2, id);
    writer.bytes(sealed);
    site_dir.encode(&mut writer);
    writer.0
}

/// v8 `SetPage { id, sealed, site_dir }` (`0x03`).
pub fn encode_set_page(id: &NameId, sealed: &[u8], site_dir: &SiteDirOp) -> Vec<u8> {
    let mut writer = on_name(3, id);
    writer.bytes(sealed);
    site_dir.encode(&mut writer);
    writer.0
}

/// v8 `Transfer { id, new_owner, now }` (`0x04`), where `new_owner` is the new holder's owner tag.
pub fn encode_transfer(id: &NameId, new_owner: &OwnerTag, now: u64) -> Vec<u8> {
    let mut writer = on_name(4, id);
    writer.0.extend(new_owner);
    writer.u64(now);
    writer.0
}

/// `Release { id, refund }` (`0x05`).
pub fn encode_release(id: &NameId, refund: u128) -> Vec<u8> {
    let mut writer = on_name(5, id);
    writer.u128(refund);
    writer.0
}

pub fn encode_set_primary(name: &str) -> Vec<u8> {
    let mut writer = instruction(6);
    writer.string(name);
    writer.0
}

/// `Commit { commitment, now }` (`0x08`, private, 15-minute wait).
pub fn encode_commit(commitment: &[u8; 32], now: u64) -> Vec<u8> {
    let mut writer = instruction(8);
    writer.0.extend(commitment);
    writer.u64(now);
    writer.0
}

/// `CommitPublic { commitment, now }` (`0x0a`, public, 5-minute wait, 1-minute validity window).
pub fn encode_commit_public(commitment: &[u8; 32], now: u64) -> Vec<u8> {
    let mut writer = instruction(10);
    writer.0.extend(commitment);
    writer.u64(now);
    writer.0
}

/// v8 `SetSubnameSale { id, sale }` (`0x0c`).
pub fn encode_set_subname_sale(id: &NameId, sale: Option<SubnameSale>) -> Vec<u8> {
    let mut writer = on_name(12, id);
    match sale {
        None => writer.u8(0),
        Some(s) => {
            writer.u8(1);
            writer.u128(s.price);
            writer.0.extend(s.payout);
        }
    }
    writer.0
}

/// v8 `CreateSubname { parent_id, sub_id, owner, revocable, sealed, now }` (`0x0d`).
pub fn encode_create_subname(
    parent_id: &NameId,
    sub_id: &NameId,
    owner: &OwnerTag,
    revocable: bool,
    sealed: &[u8],
    now: u64,
) -> Vec<u8> {
    let mut writer = instruction(13);
    writer.0.extend(parent_id);
    writer.0.extend(sub_id);
    writer.0.extend(owner);
    writer.bool(revocable);
    writer.bytes(sealed);
    writer.u64(now);
    writer.0
}

/// v8 `RevokeSubname { parent_id, sub_id, now }` (`0x0e`).
pub fn encode_revoke_subname(parent_id: &NameId, sub_id: &NameId, now: u64) -> Vec<u8> {
    let mut writer = instruction(14);
    writer.0.extend(parent_id);
    writer.0.extend(sub_id);
    writer.u64(now);
    writer.0
}

/// v8 `BuySubname { parent_id, sub_id, sealed, price, payout, now, secret }` (`0x0f`).
#[allow(clippy::too_many_arguments)]
pub fn encode_buy_subname(
    parent_id: &NameId,
    sub_id: &NameId,
    sealed: &[u8],
    price: u128,
    payout: Option<&AccountId>,
    now: u64,
    secret: &[u8; 32],
) -> Vec<u8> {
    let mut writer = instruction(15);
    writer.0.extend(parent_id);
    writer.0.extend(sub_id);
    writer.bytes(sealed);
    writer.u128(price);
    writer.optional(payout);
    writer.u64(now);
    writer.0.extend(secret);
    writer.0
}

/// v8 `List { id, price, pay_to, expires_at, now, display_name }` (`0x15`).
pub fn encode_list(
    id: &NameId,
    price: u128,
    pay_to: &AccountId,
    expires_at: Option<u64>,
    now: u64,
    display_name: Option<&str>,
) -> Vec<u8> {
    let mut writer = on_name(21, id);
    writer.u128(price);
    writer.0.extend(pay_to);
    writer.optional_u64(expires_at);
    writer.u64(now);
    writer.optional_string(display_name);
    writer.0
}

/// v8 `Cancel { id }` (`0x16`).
pub fn encode_cancel(id: &NameId) -> Vec<u8> {
    on_name(22, id).0
}

/// v8 `Buy { id, price, pay_to, treasury, expected_nonce, now }` (`0x17`).
pub fn encode_buy(
    id: &NameId,
    price: u128,
    pay_to: &AccountId,
    treasury: &AccountId,
    expected_nonce: u64,
    now: u64,
) -> Vec<u8> {
    let mut writer = on_name(23, id);
    writer.u128(price);
    writer.0.extend(pay_to);
    writer.0.extend(treasury);
    writer.u64(expected_nonce);
    writer.u64(now);
    writer.0
}

/// `lez_nft` v3 `Message::Instruction(Instruction::Transfer { token_id, new_owner,
/// sealed: None, now: Some(now), linked_target: Some(slot) })` (lez-nft v3-lez04:
/// `Message::Instruction` = 0, `Instruction::Transfer` = 3). Pinned to lez_nft's
/// Borsh output in the tests.
#[allow(dead_code)]
pub fn encode_lez_nft_transfer(token_id: &[u8; 32], new_owner: &OwnerTag, now: u64, slot: &NameId) -> Vec<u8> {
    let mut writer = Writer(vec![0, 3]);
    writer.0.extend(token_id);
    writer.0.extend(new_owner);
    writer.0.push(0); // sealed: None
    writer.0.push(1); // now: Some
    writer.u64(now);
    writer.0.push(1); // linked_target: Some(slot)
    writer.0.extend(slot);
    writer.0
}

fn is_valid_text_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= MAX_TEXT_KEY_LEN
        && key
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_' | b'.'))
}

pub(crate) fn check_text(key: &str, value: &str) -> Result<(), String> {
    if !is_valid_text_key(key) {
        return Err(format!("an entry's key is 1-32 characters of a-z, 0-9, '-', '_' or '.', not {key:?}"));
    }
    if value.len() > MAX_TEXT_VALUE_LEN {
        return Err(format!("the value of {key} is over {MAX_TEXT_VALUE_LEN} bytes"));
    }
    Ok(())
}

fn display(name: &str) -> String {
    format!("{name}{NAME_SUFFIX}")
}

pub fn tier_text(tier: u8) -> &'static str {
    match tier {
        TIER_LOCKED => "locked",
        TIER_THREE => "three",
        _ => "normal",
    }
}

/// What registering a name costs `payer`, and where it goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Charge {
    /// The deposit (deposit mode) or fee (fee mode); 0 when nothing is owed.
    pub amount: u128,
    /// Whether it is paid as a deposit.
    pub deposit: bool,
    /// The treasury a fee goes to, when there is a fee.
    pub treasury: Option<AccountId>,
}

/// What `payer` owes to register the name `id` of length tier `tier`, as the
/// CLI's `registration_cost` and the program's `RequirePrice` work it out:
///
/// - one- and two-character names are locked in every mode: only the
///   registry's admin may register one (an unconfigured registry has none);
/// - in deposit mode, the admin owes nothing; anyone else owes `deposits[0]`
///   for a normal name and `deposits[1]` for a three-character one, paid into
///   the name's deposit account;
/// - in fee mode, the price of [`REGISTER_TERM`], paid to the treasury, which
///   itself owes nothing; an unconfigured registry charges nothing.
pub fn registration_charge(
    settings: Option<&Settings>,
    tier: u8,
    price_tier: u8,
    term: u8,
    premium: u128,
    payer: &AccountId,
) -> Result<Charge, String> {
    let admin = settings.map(|settings| settings.admin);
    if tier == TIER_LOCKED && admin != Some(*payer) {
        return Err("one- to four-character and reserved names are locked: they are released by the registry's admin".to_owned());
    }
    Ok(match settings {
        Some(settings) if settings.deposit_mode() => {
            let amount = if *payer == settings.admin { 0 } else { settings.deposits[usize::from(tier == TIER_THREE)] };
            Charge { amount, deposit: true, treasury: None }
        }
        Some(settings) if settings.treasury != *payer => {
            let base = settings.price_for(price_tier, term);
            let amount = base.saturating_add(premium);
            Charge { amount, deposit: false, treasury: (amount != 0).then_some(settings.treasury) }
        }
        _ => Charge { amount: 0, deposit: false, treasury: None },
    })
}

impl Charge {
    /// The balances a payment of this charge touches: the payer's, and that of the
    /// account paid, which must be declared to be credited in the same transaction.
    fn paying(&self, program: &AccountId, id: &NameId, payer: AccountId) -> Vec<Mention> {
        if self.amount == 0 {
            return Vec::new();
        }
        let paid = match self.treasury {
            Some(treasury) => plain_balance(treasury),
            None => registry_balance(program, deposit_seed(id)),
        };
        vec![signer_balance(payer), paid]
    }

    fn to_json(&self) -> Value {
        if self.deposit {
            json!({ "deposit": self.amount.to_string() })
        } else {
            json!({ "fee": self.amount.to_string() })
        }
    }
}

/// A profile change, applied to the profile as the record holds it.
type Edit<'a> = Box<dyn FnOnce(&mut Profile) -> Result<(), String> + 'a>;

impl Registry {
    /// The record of a name, lapsed or not: its holder may still change it.
    fn held_record(&self, name: &str, derived: &Derived) -> Result<Resolved, String> {
        self.record(name, derived)?
            .ok_or_else(|| format!("{} is not registered", display(name)))
    }

    /// The record of a name and which of `held` (the open wallet's public
    /// accounts) holds it. The chain stores only the holder's tag, so each
    /// account is tried, as the CLI's `holder` does.
    fn holder(&self, name: &str, derived: &Derived, held: &[AccountId]) -> Result<(Resolved, AccountId), String> {
        let record = self.held_record(name, derived)?;
        let holder = held
            .iter()
            .find(|account| record.is_held_by_name(name, derived, account))
            .copied()
            .ok_or_else(|| {
                format!(
                    "none of the open wallet's public accounts holds {}; this module signs only with public accounts",
                    display(name)
                )
            })?;
        Ok((record, holder))
    }

    fn push_cleanup_mentions(&self, record: &Resolved, slot: &NameId, settings: Option<&Settings>, reached: &mut Vec<Mention>) {
        if record.listing.is_some_and(|l| l.in_market) {
            reached.push(registry_actor(&self.program, market_bucket_seed(bucket_of(slot))));
        }
        if let Some(page) = record.site_listed {
            reached.push(registry_actor(&self.program, site_bucket_seed(bucket_of(slot), page)));
            reached.push(registry_actor(&self.program, site_dir_header_seed()));
        }
        if let Some(s) = settings.filter(|s| s.nft_program != [0; 32]) {
            if record.nft_token_id != [0; 32] {
                reached.push(foreign_pda_actor(&s.nft_program, nft_token_seed(&record.nft_token_id)));
            }
        }
    }

    /// Replaces the name's profile with what `edit` makes of the current one.
    fn edit_profile(&self, name: &str, derived: &Derived, held: &[AccountId], edit: Edit<'_>) -> Result<Change, String> {
        let (record, holder) = self.holder(name, derived, held)?;
        let slot = slot_id(name, &derived.id);
        let mut profile = record.profile.clone();
        edit(&mut profile)?;
        if profile.texts.len() > MAX_TEXT_RECORDS {
            return Err(format!("a name holds at most {MAX_TEXT_RECORDS} entries"));
        }
        if profile.sealed_len() > MAX_SEALED_RECORD_LEN {
            return Err(format!(
                "the profile would seal to {} bytes, over the record's {MAX_SEALED_RECORD_LEN}; shorten the entries",
                profile.sealed_len()
            ));
        }
        let has_page = !self.state(&page_address(&self.program, &slot))?.is_empty();
        let (site_dir, site_mentions) = self.site_dir_for(name, &slot, &record, &profile, has_page)?;
        let mut mentions = vec![registry_actor(&self.program, name_seed(&slot))];
        mentions.extend(site_mentions);
        Ok(Change::new(
            &self.program,
            encode_set_record(&slot, &profile.seal(&derived.key)?, &site_dir),
            holder,
            mentions,
        ))
    }

    /// The sites-directory step for a top-level name after a profile or page change, and the
    /// directory actors it reaches. A name is listed while it has a site (an on-chain page or a
    /// `cid` site, as `lns publish` writes) unless its profile sets `discover=off`; it leaves the
    /// directory when its last site goes or `discover=off` is set. Same rule as the CLI.
    fn site_dir_for(
        &self,
        name: &str,
        slot: &NameId,
        record: &Resolved,
        profile: &Profile,
        has_page: bool,
    ) -> Result<(SiteDirOp, Vec<Mention>), String> {
        if split_subname(name).is_some() {
            return Ok((SiteDirOp::Keep, Vec::new()));
        }
        let actors = |page: u16| {
            vec![
                registry_actor(&self.program, site_bucket_seed(bucket_of(slot), page)),
                registry_actor(&self.program, site_dir_header_seed()),
            ]
        };
        let has_site = has_page || has_cid_site(profile);
        match (site_dir_step(record.site_listed.is_some(), has_site, discover_is_off(profile)), record.site_listed) {
            (None, _) => return Ok((SiteDirOp::Keep, Vec::new())),
            (Some(false), Some(page)) => return Ok((SiteDirOp::Remove, actors(page))),
            (Some(false), None) => return Ok((SiteDirOp::Keep, Vec::new())),
            (Some(true), _) => {}
        }
        let now = self.now()?;
        if now >= record.expires {
            // The registry refuses to list a lapsed name (in grace): it stays unlisted until renewed.
            return Ok((SiteDirOp::Keep, Vec::new()));
        }
        let page = self.site_dir_header()?.active_page[usize::from(bucket_of(slot))];
        let title = match site_title_for(profile) {
            t if t.is_empty() => name.to_owned(),
            t => t,
        };
        let op = SiteDirOp::Upsert { name: name.to_owned(), title, page, now };
        Ok((op, actors(page)))
    }

    /// A change that reaches the name's record and page.
    fn on_record_and_page(&self, slot: &NameId, holder: AccountId, message: Vec<u8>) -> Change {
        let reached = vec![
            registry_actor(&self.program, name_seed(slot)),
            registry_actor(&self.program, page_seed(slot)),
        ];
        Change::new(&self.program, message, holder, reached)
    }

    /// Refuses a name that is registered (a fee-mode name past its grace
    /// period is not), and works out what registering it costs `owner` for `term`.
    fn claimable(
        &self,
        name: &str,
        derived: &Derived,
        owner: &AccountId,
        term: u8,
        now: u64,
    ) -> Result<(LengthProof, u8, Charge, Option<Resolved>, Option<Settings>), String> {
        let existing = self.record(name, derived)?;
        if existing.as_ref().is_some_and(|r| !r.gone(now)) {
            return Err(format!("{} is already registered", display(name)));
        }
        let settings = self.settings()?;
        let (length, tier) = length_proof(&derived.id)?;
        let p_tier = price_tier_for_len(name.len());
        let premium = match (&existing, &settings) {
            (Some(old), Some(s)) if !s.deposit_mode() => {
                old.rerelease_elapsed(now).map_or(0, |el| rerelease_premium(s.premium_start, el))
            }
            _ => 0,
        };
        let charge = registration_charge(settings.as_ref(), tier, p_tier, term, premium, owner)
            .map_err(|reason| format!("{} is not available: {reason}", display(name)))?;
        Ok((length, tier, charge, existing, settings))
    }

    /// Step 1 of registering: commits `owner` to claiming `name` with `secret`,
    /// without revealing the name, its identifier or `owner`. Returns the
    /// change and the time it claims; the reveal ([`Self::reveal`]) is valid
    /// from [`PUBLIC_MIN_COMMIT_AGE_MS`](crate::registry::PUBLIC_MIN_COMMIT_AGE_MS) after it.
    pub fn commit(&self, name: &str, derived: &Derived, owner: &AccountId, secret: &[u8; 32], term: u8) -> Result<(Change, u64), String> {
        let now = self.now()?;
        let (_, tier, charge, _, settings) = self.claimable(name, derived, owner, term, now)?;
        let slot = slot_id(name, &derived.id);
        let promised = commitment(&slot, &owner_tag(&slot, owner), secret);
        let mut change = Change::new(
            &self.program,
            encode_commit_public(&promised, now),
            *owner,
            vec![registry_actor(&self.program, commitment_seed(&promised))],
        );
        // The deposit/fee is checked now, so a commitment isn't made that can't be revealed.
        change.needs = charge.amount;
        change.extra = charge.to_json();
        change.extra["tier"] = Value::from(tier_text(tier));
        if let Some(settings) = settings.filter(|settings| settings.claims_open(now)) {
            change.extra["claims_until"] = Value::from(settings.claims_until);
        }
        Ok((change, now))
    }

    /// Step 2 of registering: reveals the commitment made with `secret` and
    /// claims `name` for `owner` for `term`, with a first profile that holds the name and,
    /// when `private_keys` are given, where it can be paid privately.
    pub fn reveal(
        &self,
        name: &str,
        derived: &Derived,
        owner: &AccountId,
        secret: &[u8; 32],
        term: u8,
        private_keys: Option<([u8; 32], Vec<u8>)>,
    ) -> Result<Change, String> {
        let now = self.now()?;
        let (length, tier, charge, existing, settings) = self.claimable(name, derived, owner, term, now)?;
        let slot = slot_id(name, &derived.id);
        let p_tier = price_tier_for_len(name.len());
        let mut profile = Profile::bare(name);
        if let Some((npk, vpk)) = private_keys {
            profile.npk = Some(npk);
            profile.vpk = vpk;
        }
        let owner_t = owner_tag(&slot, owner);
        let promised = commitment(&slot, &owner_t, secret);
        let mut reached = vec![
            registry_actor(&self.program, name_seed(&slot)),
            registry_actor(&self.program, page_seed(&slot)),
            registry_actor(&self.program, settings_seed()),
            registry_actor(&self.program, commitment_seed(&promised)),
        ];
        if let Some(s) = settings.as_ref().filter(|s| s.nft_program != [0; 32]) {
            let gen = register_generation(&slot, &owner_t, now, secret);
            let token_id = nft_token_id(&slot, &gen);
            reached.push(foreign_pda_actor(&s.nft_program, nft_token_seed(&token_id)));
            // lez_nft checks on every linked Mint that the registry is authorised in its Config.
            reached.push(foreign_pda_actor(&s.nft_program, nft_config_seed()));
        }
        if let Some(old) = existing.as_ref() {
            self.push_cleanup_mentions(old, &slot, settings.as_ref(), &mut reached);
        }
        reached.extend(charge.paying(&self.program, &slot, *owner));
        let message = encode_register(
            &derived.id,
            &profile.seal(&derived.key)?,
            term,
            charge.amount,
            charge.treasury.as_ref(),
            now,
            secret,
            charge.deposit,
            &length,
            p_tier,
        );
        let mut change = Change::new(&self.program, message, *owner, reached);
        change.needs = charge.amount;
        change.extra = charge.to_json();
        change.extra["tier"] = Value::from(tier_text(tier));
        Ok(change)
    }

    pub fn set_account(&self, name: &str, derived: &Derived, held: &[AccountId], account: &AccountId) -> Result<Change, String> {
        let account = *account;
        self.edit_profile(name, derived, held, Box::new(move |profile| {
            profile.account = Some(account);
            Ok(())
        }))
    }

    /// Publishes the keys for private payments. With `only`, the public
    /// account is removed too, so the name can be paid no other way.
    pub fn set_private_keys(
        &self,
        name: &str,
        derived: &Derived,
        held: &[AccountId],
        npk: &[u8],
        vpk: &[u8],
        only: bool,
    ) -> Result<Change, String> {
        let npk: [u8; 32] = npk
            .try_into()
            .map_err(|_| "the nullifier public key must be 32 bytes".to_owned())?;
        if vpk.len() != VPK_LEN {
            return Err(format!("the viewing public key must be {VPK_LEN} bytes"));
        }
        let vpk = vpk.to_vec();
        self.edit_profile(name, derived, held, Box::new(move |profile| {
            profile.npk = Some(npk);
            profile.vpk = vpk;
            if only {
                profile.account = None;
            }
            Ok(())
        }))
    }

    pub fn clear_addresses(&self, name: &str, derived: &Derived, held: &[AccountId]) -> Result<Change, String> {
        self.edit_profile(name, derived, held, Box::new(|profile| {
            profile.account = None;
            profile.npk = None;
            profile.vpk = Vec::new();
            Ok(())
        }))
    }

    /// Sets a text entry; an empty value removes the key.
    pub fn set_text(&self, name: &str, derived: &Derived, held: &[AccountId], key: &str, value: &str) -> Result<Change, String> {
        check_text(key, value)?;
        self.edit_profile(name, derived, held, Box::new(move |profile| {
            let existing = profile.texts.iter().position(|(held, _)| held == key);
            match (existing, value.is_empty()) {
                (None, true) => return Err(format!("{} has no entry called {key}", display(name))),
                (Some(at), true) => {
                    profile.texts.remove(at);
                }
                (Some(at), false) => profile.texts[at].1 = value.to_owned(),
                (None, false) => profile.texts.push((key.to_owned(), value.to_owned())),
            }
            Ok(())
        }))
    }

    /// Writes many text entries in one `SetRecord`, as the CLI's `profile`
    /// command does: with `replace`, every existing entry is dropped first;
    /// then each entry is set, and an empty value removes its key. The name's
    /// public account and payment keys are kept either way.
    pub fn set_profile(
        &self,
        name: &str,
        derived: &Derived,
        held: &[AccountId],
        entries: Vec<(String, String)>,
        replace: bool,
    ) -> Result<Change, String> {
        if entries.is_empty() && !replace {
            return Err("nothing to change: give at least one entry, or replace".to_owned());
        }
        for (key, value) in &entries {
            check_text(key, value)?;
        }
        self.edit_profile(name, derived, held, Box::new(move |profile| {
            apply_entries(profile, entries, replace);
            Ok(())
        }))
    }

    /// Writes the name's page; an empty body removes it.
    pub fn set_page(
        &self,
        name: &str,
        derived: &Derived,
        held: &[AccountId],
        content_type: &str,
        body: &[u8],
    ) -> Result<Change, String> {
        if content_type.is_empty() || content_type.len() > MAX_CONTENT_TYPE_LEN {
            return Err(format!("the content type is 1-{MAX_CONTENT_TYPE_LEN} bytes"));
        }
        if body.len() > MAX_SEALED_PAGE_LEN - PAGE_OVERHEAD {
            return Err(format!("a page is at most {} bytes", MAX_SEALED_PAGE_LEN - PAGE_OVERHEAD));
        }
        let (record, holder) = self.holder(name, derived, held)?;
        let slot = slot_id(name, &derived.id);
        let sealed = if body.is_empty() { Vec::new() } else { seal_page(&derived.key, content_type, body)? };
        let (site_dir, site_mentions) = self.site_dir_for(name, &slot, &record, &record.profile, !body.is_empty())?;
        let mut change = self.on_record_and_page(&slot, holder, encode_set_page(&slot, &sealed, &site_dir));
        change.mentions.extend(site_mentions);
        Ok(change)
    }

    /// Hands the name to `new_owner`; only its tag reaches the chain. The
    /// program empties everything the current holder sealed, profile and page,
    /// clears listings/site entries, and mirrors ownership to the tied NFT.
    pub fn transfer(&self, name: &str, derived: &Derived, held: &[AccountId], new_owner: &AccountId) -> Result<Change, String> {
        let (record, holder) = self.holder(name, derived, held)?;
        let slot = slot_id(name, &derived.id);
        if record.is_held_by_name(name, derived, new_owner) {
            return Err(format!("{} already belongs to that account", display(name)));
        }
        let now = self.now()?;
        let new_tag = owner_tag(&slot, new_owner);
        let settings = self.settings()?;
        let mut change = self.on_record_and_page(&slot, holder, encode_transfer(&slot, &new_tag, now));
        change.mentions.push(registry_actor(&self.program, settings_seed()));
        self.push_cleanup_mentions(&record, &slot, settings.as_ref(), &mut change.mentions);
        Ok(change)
    }

    /// Gives the name up, refunding its deposit to the holder: the deposit
    /// account's whole balance, read from the chain.
    pub fn release(&self, name: &str, derived: &Derived, held: &[AccountId]) -> Result<Change, String> {
        let (record, holder) = self.holder(name, derived, held)?;
        let slot = slot_id(name, &derived.id);
        let refund = self.deposit(&slot)?;
        let settings = self.settings()?;
        let mut change = self.on_record_and_page(&slot, holder, encode_release(&slot, refund));
        change.mentions.push(registry_actor(&self.program, settings_seed()));
        self.push_cleanup_mentions(&record, &slot, settings.as_ref(), &mut change.mentions);
        if refund != 0 {
            change.mentions.push(registry_balance(&self.program, deposit_seed(&slot)));
            change.mentions.push(signer_balance(holder));
        }
        change.extra = json!({ "refund": refund.to_string() });
        Ok(change)
    }

    /// Publishes the name `account` is shown as; an empty name withdraws it.
    /// A reverse record counts only when the name points back, so one that
    /// would not is refused here rather than written and ignored.
    pub fn set_primary(&self, account: &AccountId, name: &str, derived: Option<&Derived>) -> Result<Change, String> {
        if let Some(derived) = derived {
            if !is_valid_name(name) {
                return Err("not a valid name".to_owned());
            }
            let resolved = self
                .resolve(name, derived)?
                .ok_or_else(|| format!("{} is not registered", display(name)))?;
            if !resolved.is_held_by_name(name, derived, account) && resolved.profile.account != Some(*account) {
                return Err(format!("{} does not point at {}", display(name), account_text(account)));
            }
        }
        Ok(Change::new(&self.program, encode_set_primary(name), *account, Vec::new()))
    }
}

/// The profile edit [`Registry::set_profile`] makes.
fn apply_entries(profile: &mut Profile, entries: Vec<(String, String)>, replace: bool) {
    if replace {
        profile.texts.clear();
    }
    for (key, value) in entries {
        if value.is_empty() {
            profile.texts.retain(|(held, _)| *held != key);
            continue;
        }
        match profile.texts.iter_mut().find(|(held, _)| *held == key) {
            Some(text) => text.1 = value,
            None => profile.texts.push((key, value)),
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{derive, hex, sale_split, unhex, MIN_SALE_PRICE, MODE_DEPOSIT, MODE_FEE};
    use crate::shortlist::{Leaf, Membership, NonMembership, TIER_NORMAL};

    fn id() -> NameId {
        core::array::from_fn(|index| index as u8)
    }

    fn sha(bytes: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        hex(&Sha256::digest(bytes))
    }

    /// Bytes from the compiled v8 borsh vectors in `vectors.json` (written by
    /// gateway/app-src/vectors from names_core; same inputs as lez-tx.test.mjs).
    #[test]
    fn cid_sites_are_listed_like_pages() {
        let mut profile = Profile::default();
        assert!(!has_cid_site(&profile));
        profile.texts.push(("cid".into(), "  ".into()));
        assert!(!has_cid_site(&profile), "an empty cid is no site");
        profile.texts[0].1 = "zDvZRwzm1234".into();
        assert!(has_cid_site(&profile));
        // (listed, has_site, discover_off) -> step
        assert_eq!(site_dir_step(false, true, false), Some(true), "a new site (page or cid) is listed");
        assert_eq!(site_dir_step(true, true, false), None, "a listed site is left alone");
        assert_eq!(site_dir_step(true, true, true), Some(false), "discover=off unlists");
        assert_eq!(site_dir_step(false, true, true), None);
        assert_eq!(site_dir_step(true, false, false), Some(false), "the last site removed unlists");
        assert_eq!(site_dir_step(false, false, false), None);
    }

    #[test]
    fn instructions_encode_as_core_does() {
        let vectors: Value = serde_json::from_str(include_str!("../../../gateway/app-src/vectors/vectors.json")).expect("JSON");
        let expected = |name: &str| vectors["instructions"][name].as_str().expect("a vector").to_owned();
        let id = id();
        const NOW: u64 = 1_700_000_000_000;
        let member = LengthProof::Membership(Membership { index: 7, tier: TIER_THREE, path: vec![[9; 32]] });
        assert_eq!(
            hex(&encode_register(&id, &[0xEE, 0xFF], 1, 100_000, Some(&[2; 32]), NOW, &[0x5A; 32], false, &member, 2)),
            expected("register")
        );
        let non_member = LengthProof::NonMembership(NonMembership {
            left: Leaf { id: [1; 32], tier: 3 },
            left_path: vec![[3; 32]],
            right: Leaf { id: [2; 32], tier: 1 },
            right_path: vec![[4; 32], [5; 32]],
            left_index: 6,
        });
        assert_eq!(
            hex(&encode_register(&id, &[0xEE], 0, 1_000_000, None, NOW, &[0x5A; 32], true, &non_member, 2)),
            expected("registerDeposit")
        );
        assert_eq!(hex(&encode_commit(&[0xC0; 32], NOW)), expected("commit"));
        // `CommitPublic` is variant 10 (0x0a) with the same `{ commitment, now }` payload as `Commit` (0x08).
        assert_eq!(hex(&encode_commit_public(&[0xC0; 32], NOW)), expected("commit").replacen("0008", "000a", 1));
        assert_eq!(hex(&encode_set_record(&id, &[0xEE], &SiteDirOp::Keep)), expected("setRecord"));
        assert_eq!(hex(&encode_set_page(&id, &[], &SiteDirOp::Keep)), expected("setPage"));
        assert_eq!(hex(&encode_transfer(&id, &[2; 32], NOW)), expected("transfer"));
        assert_eq!(hex(&encode_release(&id, 1_000_000)), expected("release"));
        assert_eq!(hex(&encode_set_primary("jef")), expected("setPrimary"));

        // New v8 instructions: verify variant tags and exact payloads.
        let renew = encode_renew(&id, 3, 20_000_000, Some(&[2; 32]), NOW);
        assert_eq!(&renew[..2], &[0, 1]);
        assert_eq!(renew[34], 3);
        let list = encode_list(&id, 1_000, &[4; 32], Some(NOW + 1000), NOW, Some("alice"));
        assert_eq!(&list[..2], &[0, 21]);
        let cancel = encode_cancel(&id);
        assert_eq!(&cancel[..2], &[0, 22]);
        assert_eq!(cancel.len(), 34);
        let buy = encode_buy(&id, 1_000, &[4; 32], &[2; 32], 7, NOW);
        assert_eq!(&buy[..2], &[0, 23]);
        let sale = encode_set_subname_sale(&id, Some(SubnameSale { price: 500, payout: [5; 32] }));
        assert_eq!(&sale[..2], &[0, 12]);
        let create_sub = encode_create_subname(&id, &[6; 32], &[7; 32], true, &[1, 2], NOW);
        assert_eq!(&create_sub[..2], &[0, 13]);
        let revoke_sub = encode_revoke_subname(&id, &[6; 32], NOW);
        assert_eq!(&revoke_sub[..2], &[0, 14]);
        let buy_sub = encode_buy_subname(&id, &[6; 32], &[], 500, Some(&[5; 32]), NOW, &[9; 32]);
        assert_eq!(&buy_sub[..2], &[0, 15]);
        let upsert_page = encode_set_page(&id, &[1], &SiteDirOp::Upsert { name: "alice".into(), title: "Hi".into(), page: 0, now: NOW });
        assert_eq!(&upsert_page[..2], &[0, 3]);
        assert_eq!(sale_split(MIN_SALE_PRICE), (39, 1));
    }

    /// Everything here against `testdata/v8-core-vectors.json`, which is written by
    /// compiled `names_core` (b63ec93 + the deed constant) and `lez_nft_core`
    /// (3562907): instructions, account layouts, derivations, the sale split and
    /// the premium curve. Regenerate it with testdata/core-vectors (cargo run), never by hand.
    #[test]
    fn v8_core_vectors() {
        use crate::registry::{
            bucket_of, market_bucket_address, nft_config_seed, nft_token_address, nft_token_id, pda,
            register_generation, rerelease_premium, site_bucket_address, site_dir_header_address,
            subname_commitment, subname_page_address, subname_record_address, tiered_id, Listing,
            MarketBucket, Resolved, SiteBucket, SiteDirHeader, SubnameKind, SubnameRecord, SubnameSale,
        };
        let v: Value = serde_json::from_str(include_str!("../testdata/v8-core-vectors.json")).expect("JSON");
        let x = |k: &str| v[k].as_str().unwrap_or_else(|| panic!("vector {k}")).to_owned();
        let (id, sub, tag, secret, gen, tok) = ([0x31u8; 32], [0x32u8; 32], [0x41u8; 32], [0x51u8; 32], [0x61u8; 32], [0x71u8; 32]);
        let length = LengthProof::Membership(Membership { index: 7, tier: TIER_NORMAL, path: vec![[0x81; 32], [0x82; 32]] });
        assert_eq!(hex(&encode_register(&id, &[1, 2, 3], 3, 20_000_000, Some(&[0x91; 32]), 1_700_000_000_000, &secret, false, &length, 1)), x("register"));
        assert_eq!(hex(&encode_renew(&id, 1, 5_000_000, Some(&[0x91; 32]), 1_700_000_000_001)), x("renew"));
        assert_eq!(hex(&encode_set_record(&id, &[9], &SiteDirOp::Keep)), x("set_record_keep"));
        assert_eq!(hex(&encode_set_page(&id, &[8, 7], &SiteDirOp::Upsert { name: "alice".into(), title: "Alice's site".into(), page: 2, now: 5 })), x("set_page_upsert"));
        assert_eq!(hex(&encode_set_record(&id, &[], &SiteDirOp::Remove)), x("set_record_remove"));
        assert_eq!(hex(&encode_transfer(&id, &tag, 77)), x("transfer"));
        assert_eq!(hex(&encode_release(&id, 1_000_000)), x("release"));
        assert_eq!(hex(&encode_commit(&[0xC1; 32], 11)), x("commit"));
        assert_eq!(hex(&encode_commit_public(&[0xC2; 32], 12)), x("commit_public"));
        assert_eq!(hex(&encode_set_subname_sale(&id, Some(SubnameSale { price: 500, payout: [0x92; 32] }))), x("set_subname_sale_on"));
        assert_eq!(hex(&encode_set_subname_sale(&id, None)), x("set_subname_sale_off"));
        assert_eq!(hex(&encode_create_subname(&id, &sub, &tag, true, &[4], 13)), x("create_subname"));
        assert_eq!(hex(&encode_revoke_subname(&id, &sub, 14)), x("revoke_subname"));
        assert_eq!(hex(&encode_buy_subname(&id, &sub, &[5, 6], 500, Some(&[0x92; 32]), 15, &secret)), x("buy_subname"));
        assert_eq!(hex(&encode_list(&id, 1000, &[0x93; 32], Some(99), 16, Some("alice"))), x("list"));
        assert_eq!(hex(&encode_list(&id, 40, &[0x93; 32], None, 17, None)), x("list_unlisted"));
        assert_eq!(hex(&encode_cancel(&id)), x("cancel"));
        assert_eq!(hex(&encode_buy(&id, 1000, &[0x93; 32], &[0x91; 32], 3, 18)), x("buy"));
        assert_eq!(hex(&encode_lez_nft_transfer(&tok, &tag, 42, &id)), x("nft_transfer"));

        // Accounts decode as core encodes them.
        let record = unhex(&x("name_record")).unwrap();
        let r = Resolved::decode(&record, &[0; 32], "alice").expect("a v8 record");
        assert_eq!((r.version, r.owner, r.expires, r.claimed, r.price_tier, r.generation), (8, tag, 123, false, 1, gen));
        assert_eq!(r.subname_sale, Some(SubnameSale { price: 500, payout: [0x92; 32] }));
        assert_eq!((r.nft_token_id, r.nft_shielded, r.listing_nonce, r.site_listed), (tok, true, 3, Some(2)));
        assert_eq!(r.listing, Some(Listing { price: 1000, pay_to: [0x93; 32], expires_at: Some(99), nonce: 3, in_market: true }));
        let plain = Resolved::decode(&unhex(&x("name_record_plain")).unwrap(), &[0; 32], "alice").expect("a plain record");
        assert_eq!((plain.expires, plain.claimed, plain.listing, plain.site_listed, plain.subname_sale), (u64::MAX, true, None, None, None));
        let s = Settings::decode(&unhex(&x("settings")).unwrap()).expect("v8 settings");
        assert_eq!((s.admin, s.treasury, s.mode, s.deposits, s.claims_root, s.claims_until, s.claims_program), ([0xA1; 32], [0x91; 32], MODE_FEE, [7, 8], [0xB1; 32], 19, [0xB2; 32]));
        assert_eq!((s.premium_start, s.nft_program), (4_000_000_000, [0x11; 32]));
        assert_eq!(s.prices[0][3], 400_000_000);
        assert_eq!(s.prices[2][0], 100_000);
        let sr = SubnameRecord::decode(&unhex(&x("subname_record")).unwrap()).expect("a subname record");
        assert_eq!((sr.owner, sr.kind, sr.sealed), (tag, SubnameKind::Revocable { parent_generation: gen }, vec![3]));
        let mb = MarketBucket::decode(&unhex(&x("market_bucket")).unwrap()).expect("a market bucket");
        assert_eq!((mb.entries.len(), mb.entries[0].price, mb.entries[0].display_name.as_deref(), mb.entries[0].nonce), (1, 1000, Some("alice"), 3));
        let sb = SiteBucket::decode(&unhex(&x("site_bucket")).unwrap()).expect("a site bucket");
        assert_eq!((sb.entries[0].name.as_str(), sb.entries[0].title.as_str(), sb.entries[0].name_expires), ("alice", "Alice", 123));
        let h = SiteDirHeader::decode(&unhex(&x("site_dir_header")).unwrap()).expect("a header");
        assert_eq!((h.active_page[3], h.total_listed), (2, 9));

        // Derivations.
        let (reg, nft) = ([0x22u8; 32], [0x11u8; 32]);
        assert_eq!(x("reg"), hex(&reg));
        assert_eq!(hex(&register_generation(&id, &tag, 1_700_000_000_000, &secret)), x("register_generation"));
        assert_eq!(hex(&nft_token_id(&id, &gen)), x("nft_token_id"));
        assert_eq!(hex(&nft_token_address(&nft, &tok)), x("nft_token_address"));
        assert_eq!(hex(&pda(&nft, &nft_config_seed())), x("nft_config_address"));
        assert_eq!(hex(&tiered_id(&id, 1)), x("tiered_id_1"));
        assert_eq!(hex(&market_bucket_address(&reg, 3)), x("market_bucket_address_3"));
        assert_eq!(hex(&site_dir_header_address(&reg)), x("site_dir_header_address"));
        assert_eq!(hex(&site_bucket_address(&reg, 3, 7)), x("site_bucket_address_3_7"));
        assert_eq!(hex(&subname_record_address(&reg, &id, &sub)), x("subname_record_address"));
        assert_eq!(hex(&subname_page_address(&reg, &id, &sub)), x("subname_page_address"));
        assert_eq!(hex(&subname_commitment(&id, &sub, &tag, &secret)), x("subname_commitment"));
        assert_eq!(bucket_of(&[0xA7; 32]).to_string(), x("bucket_of"));
        for item in x("sale_split").split(',') {
            let parts: Vec<u128> = item.split(':').map(|n| n.parse().unwrap()).collect();
            assert_eq!(sale_split(parts[0]), (parts[1], parts[2]), "sale_split({})", parts[0]);
        }
        for item in x("premium").split(',') {
            let (elapsed, want) = item.split_once(':').unwrap();
            assert_eq!(rerelease_premium(4_000_000_000, elapsed.parse().unwrap()).to_string(), want, "premium at {elapsed}");
        }
    }

    /// The seeds a change lists as evidence, and the addresses they give, as core
    /// derives them.
    #[test]
    fn seeds_match_core() {
        let vectors: Value = serde_json::from_str(include_str!("../../../gateway/app-src/vectors/vectors.json")).expect("JSON");
        let id = id();
        let hexed = |key: &str| vectors["seeds"][key].as_str().expect("a seed").to_owned();
        assert_eq!(hex(&name_seed(&id)), hexed("name"));
        assert_eq!(hex(&page_seed(&id)), hexed("page"));
        assert_eq!(hex(&deposit_seed(&id)), hexed("deposit"));
        assert_eq!(hex(&settings_seed()), hexed("settings"));
    }

    /// Every actor a change reaches, in the shape the CLI's public path sends. Each
    /// registry account carries its seed: without one a transaction that creates the
    /// account is refused and still charged.
    #[test]
    fn a_change_declares_the_actors_it_reaches() {
        let program = [7; 32];
        let id = id();
        let owner = [9; 32];
        let charge = Charge { amount: 100, deposit: true, treasury: None };
        let paying = charge.paying(&program, &id, owner);
        assert_eq!(paying[0], Mention { account: owner, program: NATIVE_TOKEN, seed: None, pda_program: None, signs: true });
        let seed = deposit_seed(&id);
        assert_eq!(paying[1], Mention { account: pda(&program, &seed), program: NATIVE_TOKEN, seed: Some(seed), pda_program: None, signs: false });
        let fee = Charge { amount: 100, deposit: false, treasury: Some([2; 32]) };
        assert_eq!(fee.paying(&program, &id, owner)[1], Mention { account: [2; 32], program: NATIVE_TOKEN, seed: None, pda_program: None, signs: false });
        assert!(Charge { amount: 0, deposit: true, treasury: None }.paying(&program, &id, owner).is_empty());

        let change = Change::new(&program, vec![0, 6], owner, vec![registry_actor(&program, name_seed(&id))]);
        assert_eq!(change.mentions[0], Mention { account: owner, program, seed: None, pda_program: None, signs: true }, "the root is the signer's own actor");
        assert_eq!(change.mentions[1].program, program);
        assert_eq!(change.mentions[1].seed, Some(name_seed(&id)));
        assert_eq!(change.mentions[1].account, crate::registry::record_address(&program, &id));
    }

    /// Whole registrations with the real list's proofs: v8 appends `price_tier: u8`
    /// to the v5/v6 `Register` bytes (so prefix matches the v5/v6 sha256 digests).
    #[test]
    fn real_registrations_encode_as_core_does() {
        for (name, length, digest) in [
            ("qkz", 781, "f9c5c7b223dd4152f327cc3e0d3d887b207e24c8d1709e923282cf1bf868d74b"),
            ("walletci", 1522, "c3b72e0e3cd22798dbadbe0eb353db3ff2836959dea725f83cddb70ba6e2fc1d"),
        ] {
            let id = derive(name).id;
            let (proof, _) = length_proof(&id).expect("a proof");
            let p_tier = price_tier_for_len(name.len());
            let bytes = encode_register(&id, &[0xEE, 0xFF], 1, 100_000_000, None, 1_700_000_000_000, &[0x5A; 32], true, &proof, p_tier);
            assert_eq!(bytes.len(), length + 1, "{name}");
            assert_eq!(sha(&bytes[..length]), digest, "{name}");
            assert_eq!(bytes[length], p_tier, "{name}");
        }
    }

    fn settings(mode: u8) -> Settings {
        Settings {
            admin: [1; 32],
            treasury: [2; 32],
            prices: [
                [100, 200, 300, 4_000],
                [50, 100, 150, 2_000],
                [10, 20, 30, 400],
            ],
            mode,
            deposits: [100, 2_000],
            claims_root: [0; 32],
            claims_until: 0,
            claims_program: [0; 32],
            premium_start: 0,
            nft_program: [0; 32],
        }
    }

    #[test]
    fn deposits_follow_the_tier_and_the_admin_pays_nothing() {
        let payer = [9; 32];
        let deposits = settings(MODE_DEPOSIT);
        assert_eq!(
            registration_charge(Some(&deposits), TIER_NORMAL, 2, 1, 0, &payer),
            Ok(Charge { amount: 100, deposit: true, treasury: None })
        );
        assert_eq!(
            registration_charge(Some(&deposits), TIER_THREE, 0, 1, 0, &payer),
            Ok(Charge { amount: 2_000, deposit: true, treasury: None })
        );
        assert!(registration_charge(Some(&deposits), TIER_LOCKED, 0, 1, 0, &payer).is_err());
        // The admin places names, a locked one included, without a deposit.
        for tier in [TIER_NORMAL, TIER_THREE, TIER_LOCKED] {
            assert_eq!(
                registration_charge(Some(&deposits), tier, 0, 1, 0, &[1; 32]),
                Ok(Charge { amount: 0, deposit: true, treasury: None })
            );
        }
        // A deposit of 0 takes no balances.
        let free = Settings { deposits: [0, 0], ..settings(MODE_DEPOSIT) };
        assert_eq!(
            registration_charge(Some(&free), TIER_THREE, 0, 1, 0, &payer),
            Ok(Charge { amount: 0, deposit: true, treasury: None })
        );
    }

    #[test]
    fn fee_mode_charges_tier_and_term_price_plus_premium_to_treasury() {
        let payer = [9; 32];
        let fees = settings(MODE_FEE);
        assert_eq!(
            registration_charge(Some(&fees), TIER_NORMAL, 2, 1, 0, &payer),
            Ok(Charge { amount: 20, deposit: false, treasury: Some([2; 32]) })
        );
        assert_eq!(
            registration_charge(Some(&fees), TIER_NORMAL, 0, 3, 500, &payer),
            Ok(Charge { amount: 4_500, deposit: false, treasury: Some([2; 32]) })
        );
        assert_eq!(
            registration_charge(Some(&fees), TIER_NORMAL, 2, 1, 0, &[2; 32]),
            Ok(Charge { amount: 0, deposit: false, treasury: None }),
            "the treasury pays nothing"
        );
        assert!(registration_charge(Some(&fees), TIER_LOCKED, 0, 1, 0, &payer).is_err());
        assert_eq!(
            registration_charge(None, TIER_NORMAL, 2, 1, 0, &payer),
            Ok(Charge { amount: 0, deposit: false, treasury: None }),
            "an unconfigured registry is free"
        );
        assert!(registration_charge(None, TIER_LOCKED, 0, 1, 0, &payer).is_err(), "and locks short names");
    }

    #[test]
    fn profile_entries_are_set_removed_and_replaced() {
        let mut profile = Profile {
            name: "jef".into(),
            account: Some([2; 32]),
            texts: vec![("bio".into(), "old".into()), ("chat".into(), "hi".into())],
            ..Profile::default()
        };
        apply_entries(
            &mut profile,
            vec![("bio".into(), "new".into()), ("chat".into(), String::new()), ("display".into(), "Jef".into())],
            false,
        );
        assert_eq!(profile.texts, vec![("bio".to_owned(), "new".to_owned()), ("display".to_owned(), "Jef".to_owned())]);
        apply_entries(&mut profile, vec![("theme".into(), "dark".into())], true);
        assert_eq!(profile.texts, vec![("theme".to_owned(), "dark".to_owned())]);
        assert_eq!(profile.account, Some([2; 32]), "replace keeps the payment details");
    }

    #[test]
    fn text_keys_and_tiers_are_checked_before_sending() {
        for key in ["chat", "my-app.setting_1", "logos.account"] {
            assert!(is_valid_text_key(key), "{key}");
        }
        for key in ["", "Chat", "has space", &"k".repeat(33)] {
            assert!(!is_valid_text_key(key), "{key}");
        }
        assert!(check_text("bio", &"v".repeat(513)).is_err());
        assert_eq!(tier_text(TIER_LOCKED), "locked");
        assert_eq!(tier_text(TIER_THREE), "three");
        assert_eq!(tier_text(TIER_NORMAL), "normal");
        assert_eq!(unhex("00").map(|bytes| bytes.len()), Ok(1));
    }
}
