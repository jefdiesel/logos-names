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
    account_text, commitment, commitment_seed, deposit_seed, is_valid_name, name_seed, owner_tag, page_seed,
    pda, seal_page, settings_seed, AccountId, Derived, NameId, OwnerTag, Profile, Registry, Resolved, Settings,
    Writer, MAX_SEALED_PAGE_LEN, MAX_SEALED_RECORD_LEN, NAME_SUFFIX, NATIVE_TOKEN, VPK_LEN,
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
    /// Whose state on `account` it is: the registry's, or the native balance.
    pub program: AccountId,
    /// The account's seed, when it is one of the registry's.
    pub seed: Option<[u8; 32]>,
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
        let mut mentions = vec![Mention { account: signer, program: *program, seed: None, signs: true }];
        mentions.extend(reached);
        Self { message, mentions, signer, needs: 0, extra: Value::Null }
    }
}

/// The registry's account behind `seed`.
fn registry_actor(program: &AccountId, seed: [u8; 32]) -> Mention {
    Mention { account: pda(program, &seed), program: *program, seed: Some(seed), signs: false }
}

/// The native balance of the registry's account behind `seed`.
fn registry_balance(program: &AccountId, seed: [u8; 32]) -> Mention {
    Mention { account: pda(program, &seed), program: NATIVE_TOKEN, seed: Some(seed), signs: false }
}

/// The native balance of the account that signs.
fn signer_balance(signer: AccountId) -> Mention {
    Mention { account: signer, program: NATIVE_TOKEN, seed: None, signs: true }
}

/// The native balance of a plain account.
fn plain_balance(account: AccountId) -> Mention {
    Mention { account, program: NATIVE_TOKEN, seed: None, signs: false }
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

/// `Register { id, sealed, term, fee, treasury, now, secret, deposit, length }`.
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
    writer.0
}

pub fn encode_set_record(id: &NameId, sealed: &[u8]) -> Vec<u8> {
    let mut writer = on_name(2, id);
    writer.bytes(sealed);
    writer.0
}

pub fn encode_set_page(id: &NameId, sealed: &[u8]) -> Vec<u8> {
    let mut writer = on_name(3, id);
    writer.bytes(sealed);
    writer.0
}

/// `Transfer { id, new_owner }`, where `new_owner` is the new holder's owner tag.
pub fn encode_transfer(id: &NameId, new_owner: &OwnerTag) -> Vec<u8> {
    let mut writer = on_name(4, id);
    writer.0.extend(new_owner);
    writer.0
}

/// `Release { id, refund }`.
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

/// `Commit { commitment, now }`.
pub fn encode_commit(commitment: &[u8; 32], now: u64) -> Vec<u8> {
    let mut writer = instruction(8);
    writer.0.extend(commitment);
    writer.u64(now);
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
    payer: &AccountId,
) -> Result<Charge, String> {
    let admin = settings.map(|settings| settings.admin);
    if tier == TIER_LOCKED && admin != Some(*payer) {
        return Err("one- and two-character names are locked: they are released by the registry's admin".to_owned());
    }
    Ok(match settings {
        Some(settings) if settings.deposit_mode() => {
            let amount = if *payer == settings.admin { 0 } else { settings.deposits[usize::from(tier == TIER_THREE)] };
            Charge { amount, deposit: true, treasury: None }
        }
        Some(settings) if settings.treasury != *payer => {
            let amount = settings.prices[usize::from(REGISTER_TERM)];
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
            .find(|account| record.is_held_by(&derived.id, account))
            .copied()
            .ok_or_else(|| {
                format!(
                    "none of the open wallet's public accounts holds {}; this module signs only with public accounts",
                    display(name)
                )
            })?;
        Ok((record, holder))
    }

    /// Replaces the name's profile with what `edit` makes of the current one.
    fn edit_profile(&self, name: &str, derived: &Derived, held: &[AccountId], edit: Edit<'_>) -> Result<Change, String> {
        let (record, holder) = self.holder(name, derived, held)?;
        let mut profile = record.profile;
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
        Ok(Change::new(
            &self.program,
            encode_set_record(&derived.id, &profile.seal(&derived.key)?),
            holder,
            vec![registry_actor(&self.program, name_seed(&derived.id))],
        ))
    }

    /// A change that reaches the name's record and page.
    fn on_record_and_page(&self, derived: &Derived, holder: AccountId, message: Vec<u8>) -> Change {
        let reached = vec![
            registry_actor(&self.program, name_seed(&derived.id)),
            registry_actor(&self.program, page_seed(&derived.id)),
        ];
        Change::new(&self.program, message, holder, reached)
    }

    /// Refuses a name that is registered (a fee-mode name past its grace
    /// period is not), and works out what registering it costs `owner`.
    fn claimable(&self, name: &str, derived: &Derived, owner: &AccountId) -> Result<(LengthProof, u8, Charge), String> {
        if self.resolve(name, derived)?.is_some() {
            return Err(format!("{} is already registered", display(name)));
        }
        let (length, tier) = length_proof(&derived.id)?;
        let charge = registration_charge(self.settings()?.as_ref(), tier, owner)
            .map_err(|reason| format!("{} is not available: {reason}", display(name)))?;
        Ok((length, tier, charge))
    }

    /// Step 1 of registering: commits `owner` to claiming `name` with `secret`,
    /// without revealing the name, its identifier or `owner`. Returns the
    /// change and the time it claims; the reveal ([`Self::reveal`]) is valid
    /// from [`MIN_COMMIT_AGE_MS`](crate::registry::MIN_COMMIT_AGE_MS) after it.
    pub fn commit(&self, name: &str, derived: &Derived, owner: &AccountId, secret: &[u8; 32]) -> Result<(Change, u64), String> {
        let (_, tier, charge) = self.claimable(name, derived, owner)?;
        let promised = commitment(&derived.id, &owner_tag(&derived.id, owner), secret);
        let now = self.now()?;
        let mut change = Change::new(
            &self.program,
            encode_commit(&promised, now),
            *owner,
            vec![registry_actor(&self.program, commitment_seed(&promised))],
        );
        // The deposit is checked now, so a commitment isn't made that can't be revealed.
        change.needs = charge.amount;
        change.extra = charge.to_json();
        change.extra["tier"] = Value::from(tier_text(tier));
        if let Some(settings) = self.settings()?.filter(|settings| settings.claims_open(now)) {
            change.extra["claims_until"] = Value::from(settings.claims_until);
        }
        Ok((change, now))
    }

    /// Step 2 of registering: reveals the commitment made with `secret` and
    /// claims `name` for `owner`, with a first profile that holds the name and,
    /// when `private_keys` are given, where it can be paid privately. The
    /// deposit or fee is paid from `owner`.
    pub fn reveal(
        &self,
        name: &str,
        derived: &Derived,
        owner: &AccountId,
        secret: &[u8; 32],
        private_keys: Option<([u8; 32], Vec<u8>)>,
    ) -> Result<Change, String> {
        let (length, tier, charge) = self.claimable(name, derived, owner)?;
        let mut profile = Profile::bare(name);
        if let Some((npk, vpk)) = private_keys {
            profile.npk = Some(npk);
            profile.vpk = vpk;
        }
        let promised = commitment(&derived.id, &owner_tag(&derived.id, owner), secret);
        let mut reached = vec![
            registry_actor(&self.program, name_seed(&derived.id)),
            registry_actor(&self.program, page_seed(&derived.id)),
            registry_actor(&self.program, settings_seed()),
            registry_actor(&self.program, commitment_seed(&promised)),
        ];
        reached.extend(charge.paying(&self.program, &derived.id, *owner));
        let message = encode_register(
            &derived.id,
            &profile.seal(&derived.key)?,
            REGISTER_TERM,
            charge.amount,
            charge.treasury.as_ref(),
            self.now()?,
            secret,
            charge.deposit,
            &length,
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
        let (_, holder) = self.holder(name, derived, held)?;
        let sealed = if body.is_empty() { Vec::new() } else { seal_page(&derived.key, content_type, body)? };
        Ok(self.on_record_and_page(derived, holder, encode_set_page(&derived.id, &sealed)))
    }

    /// Hands the name to `new_owner`; only its tag reaches the chain. The
    /// program empties everything the current holder sealed, profile and page.
    /// The deposit stays with the name, for whoever releases it.
    pub fn transfer(&self, name: &str, derived: &Derived, held: &[AccountId], new_owner: &AccountId) -> Result<Change, String> {
        let (record, holder) = self.holder(name, derived, held)?;
        if record.is_held_by(&derived.id, new_owner) {
            return Err(format!("{} already belongs to that account", display(name)));
        }
        let new_tag = owner_tag(&derived.id, new_owner);
        Ok(self.on_record_and_page(derived, holder, encode_transfer(&derived.id, &new_tag)))
    }

    /// Gives the name up, refunding its deposit to the holder: the deposit
    /// account's whole balance, read from the chain.
    pub fn release(&self, name: &str, derived: &Derived, held: &[AccountId]) -> Result<Change, String> {
        let (_, holder) = self.holder(name, derived, held)?;
        let refund = self.deposit(&derived.id)?;
        let mut change = self.on_record_and_page(derived, holder, encode_release(&derived.id, refund));
        if refund != 0 {
            change.mentions.push(registry_balance(&self.program, deposit_seed(&derived.id)));
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
            if !resolved.is_held_by(&derived.id, account) && resolved.profile.account != Some(*account) {
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
    use crate::registry::{derive, hex, unhex, MODE_DEPOSIT, MODE_FEE};
    use crate::shortlist::{Leaf, Membership, NonMembership, TIER_NORMAL};

    fn id() -> NameId {
        core::array::from_fn(|index| index as u8)
    }

    fn sha(bytes: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        hex(&Sha256::digest(bytes))
    }

    /// Bytes from `names_core`'s own borsh encoding of each instruction, from
    /// `gateway/app-src/vectors/vectors.json`, which the Rust types made.
    #[test]
    fn instructions_encode_as_core_does() {
        let vectors: Value = serde_json::from_str(include_str!("../../../gateway/app-src/vectors/vectors.json")).expect("JSON");
        let expected = |name: &str| vectors["instructions"][name].as_str().expect("a vector").to_owned();
        let id = id();
        const NOW: u64 = 1_700_000_000_000;
        let member = LengthProof::Membership(Membership { index: 7, tier: TIER_THREE, path: vec![[9; 32]] });
        assert_eq!(
            hex(&encode_register(&id, &[0xEE, 0xFF], 1, 100_000, Some(&[2; 32]), NOW, &[0x5A; 32], false, &member)),
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
            hex(&encode_register(&id, &[0xEE], 0, 1_000_000, None, NOW, &[0x5A; 32], true, &non_member)),
            expected("registerDeposit")
        );
        assert_eq!(hex(&encode_commit(&[0xC0; 32], NOW)), expected("commit"));
        assert_eq!(hex(&encode_set_record(&id, &[0xEE])), expected("setRecord"));
        assert_eq!(hex(&encode_set_page(&id, &[])), expected("setPage"));
        assert_eq!(hex(&encode_transfer(&id, &[2; 32])), expected("transfer"));
        assert_eq!(hex(&encode_release(&id, 1_000_000)), expected("release"));
        assert_eq!(hex(&encode_set_primary("jef")), expected("setPrimary"));
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
        assert_eq!(paying[0], Mention { account: owner, program: NATIVE_TOKEN, seed: None, signs: true });
        let seed = deposit_seed(&id);
        assert_eq!(paying[1], Mention { account: pda(&program, &seed), program: NATIVE_TOKEN, seed: Some(seed), signs: false });
        let fee = Charge { amount: 100, deposit: false, treasury: Some([2; 32]) };
        assert_eq!(fee.paying(&program, &id, owner)[1], Mention { account: [2; 32], program: NATIVE_TOKEN, seed: None, signs: false });
        assert!(Charge { amount: 0, deposit: true, treasury: None }.paying(&program, &id, owner).is_empty());

        let change = Change::new(&program, vec![0, 6], owner, vec![registry_actor(&program, name_seed(&id))]);
        assert_eq!(change.mentions[0], Mention { account: owner, program, seed: None, signs: true }, "the root is the signer's own actor");
        assert_eq!(change.mentions[1].program, program);
        assert_eq!(change.mentions[1].seed, Some(name_seed(&id)));
        assert_eq!(change.mentions[1].account, crate::registry::record_address(&program, &id));
    }

    /// Whole registrations with the real list's proofs, against the sha256 of
    /// core's encoding of `Register { id, sealed: [ee, ff], term: 1, fee:
    /// 100_000_000, treasury: None, now: 1_700_000_000_000, secret: 32 × 0x5a,
    /// deposit: true, length }`, as `Message::Instruction`.
    #[test]
    fn real_registrations_encode_as_core_does() {
        for (name, length, digest) in [
            ("qkz", 781, "f9c5c7b223dd4152f327cc3e0d3d887b207e24c8d1709e923282cf1bf868d74b"),
            ("walletci", 1522, "c3b72e0e3cd22798dbadbe0eb353db3ff2836959dea725f83cddb70ba6e2fc1d"),
        ] {
            let id = derive(name).id;
            let (proof, _) = length_proof(&id).expect("a proof");
            let bytes = encode_register(&id, &[0xEE, 0xFF], 1, 100_000_000, None, 1_700_000_000_000, &[0x5A; 32], true, &proof);
            assert_eq!(bytes.len(), length, "{name}");
            assert_eq!(sha(&bytes), digest, "{name}");
        }
    }

    fn settings(mode: u8) -> Settings {
        Settings {
            admin: [1; 32],
            treasury: [2; 32],
            prices: [10, 20, 30],
            mode,
            deposits: [100, 2_000],
            claims_root: [0; 32],
            claims_until: 0,
            claims_program: [0; 32],
        }
    }

    #[test]
    fn deposits_follow_the_tier_and_the_admin_pays_nothing() {
        let payer = [9; 32];
        let deposits = settings(MODE_DEPOSIT);
        assert_eq!(
            registration_charge(Some(&deposits), TIER_NORMAL, &payer),
            Ok(Charge { amount: 100, deposit: true, treasury: None })
        );
        assert_eq!(
            registration_charge(Some(&deposits), TIER_THREE, &payer),
            Ok(Charge { amount: 2_000, deposit: true, treasury: None })
        );
        assert!(registration_charge(Some(&deposits), TIER_LOCKED, &payer).is_err());
        // The admin places names, a locked one included, without a deposit.
        for tier in [TIER_NORMAL, TIER_THREE, TIER_LOCKED] {
            assert_eq!(
                registration_charge(Some(&deposits), tier, &[1; 32]),
                Ok(Charge { amount: 0, deposit: true, treasury: None })
            );
        }
        // A deposit of 0 takes no balances.
        let free = Settings { deposits: [0, 0], ..settings(MODE_DEPOSIT) };
        assert_eq!(
            registration_charge(Some(&free), TIER_THREE, &payer),
            Ok(Charge { amount: 0, deposit: true, treasury: None })
        );
    }

    #[test]
    fn fee_mode_charges_a_years_price_to_the_treasury() {
        let payer = [9; 32];
        let fees = settings(MODE_FEE);
        assert_eq!(
            registration_charge(Some(&fees), TIER_THREE, &payer),
            Ok(Charge { amount: 20, deposit: false, treasury: Some([2; 32]) })
        );
        assert_eq!(
            registration_charge(Some(&fees), TIER_NORMAL, &[2; 32]),
            Ok(Charge { amount: 0, deposit: false, treasury: None }),
            "the treasury pays nothing"
        );
        assert!(registration_charge(Some(&fees), TIER_LOCKED, &payer).is_err());
        assert_eq!(
            registration_charge(None, TIER_NORMAL, &payer),
            Ok(Charge { amount: 0, deposit: false, treasury: None }),
            "an unconfigured registry is free"
        );
        assert!(registration_charge(None, TIER_LOCKED, &payer).is_err(), "and locks short names");
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
