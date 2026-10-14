//! Logos Names: a private name registry for the Logos Execution Zone (LEZ v0.3).
//!
//! A name such as `jef.lez` resolves to where it can be paid, some free-form
//! entries and an optional page. The registry is built so that knowing a name
//! is the only way to find it:
//!
//! - The chain never sees a name. Clients derive a 32-byte identifier from the
//!   name and every instruction, account address and event uses only that.
//! - What a name resolves to is stored sealed: encrypted by the client under a
//!   key that is also derived from the name. This program stores the bytes and
//!   cannot read them.
//! - There is no list of names. Nothing links one record to another.
//!
//! - No holder is named. A record keeps a tag of its holder's account bound to
//!   that one name ([`owner_tag`]), and no effect or event carries an account.
//!   A holder acting from a LEZ private account, through a privacy-preserving
//!   transaction, never appears on chain; one acting from a public account is
//!   seen signing, but its names stay unlinked to each other.
//!
//! What stays public is that some identifier is registered, until when, and
//! the fee paid into the treasury.
//!
//! Each name owns this program's shard on two accounts, both derived from its
//! identifier: the record ([`record_address`]) and the page ([`page_address`]).
//! A reverse record, the one thing a holder may choose to publish in the
//! clear, lives on the holder's own account.
//!
//! A name is held for a term, paid for in the native token to the registry's
//! treasury, and lapses unless renewed. The prices and the treasury are the
//! registry's settings ([`settings_address`]).
//!
//! The registry keeps no history. Releasing a name empties its record and page;
//! transferring a name empties everything its last holder sealed.
//!
//! `plan` never sees account data, so every ownership check is carried by an
//! effect and enforced in `apply`, where the shard's current data is visible.
//! `apply` sees no clock either: the time is the sender's claim, which `plan`
//! binds to the block's timestamp and passes on inside the effects.

use borsh::{BorshDeserialize, BorshSerialize};
use lee_core::{
    account::{AccountId, ProgramShardSelector},
    native_token::{self, NATIVE_TOKEN_PROGRAM_ID},
    program::{AccountMeta, ChainedCall, PdaSeed, Plan, PlanInput, ProgramEvent},
};

/// Record layout version written by this program. Version 3 holds an owner
/// tag where version 2 held the owner's account.
pub const VERSION: u8 = 3;
pub const MAX_NAME_LEN: usize = 32;
/// How names are written for people: `jef.lez`. The suffix is a display
/// convention only; it is removed before a name is turned into an identifier.
pub const NAME_SUFFIX: &str = ".lez";
/// Room for a sealed profile with private receiving keys and sixteen entries.
pub const MAX_SEALED_RECORD_LEN: usize = 12 * 1024;
/// LEZ caps shard data at 100 KiB, but a page that large does not fit one
/// transaction under the wallet's default gas limit: 80 KiB applies, 96 KiB fails.
pub const MAX_SEALED_PAGE_LEN: usize = 80 * 1024;

const NAME_SEED_DOMAIN: &[u8] = b"logos-names/v2/name/";
const PAGE_SEED_DOMAIN: &[u8] = b"logos-names/v2/page/";
const SETTINGS_SEED_DOMAIN: &[u8] = b"logos-names/v2/config";
const OWNER_TAG_DOMAIN: &[u8] = b"logos-names/v3/owner";

const DAY_MS: u64 = 24 * 60 * 60 * 1000;
/// How long each term holds a name, in milliseconds as LEZ block timestamps
/// are: 30 days, one year, five years. An instruction names a term by its index.
pub const TERM_LENGTHS_MS: [u64; 3] = [30 * DAY_MS, 365 * DAY_MS, 5 * 365 * DAY_MS];
/// How long after its expiry a name can still be renewed, and by nobody else
/// registered.
pub const GRACE_MS: u64 = 30 * DAY_MS;

/// What the chain knows a name by: 32 bytes a client derives from the name.
/// The derivation is deliberately slow and is not this program's business.
pub type NameId = [u8; 32];

/// What a record keeps in place of its holder's account: a tag binding the
/// account to this one name ([`owner_tag`]). Holding several names, or
/// changing one, publishes nothing that links them, and a holder acting from
/// a private account never appears on chain at all.
pub type OwnerTag = [u8; 32];

/// A registered name. `sealed` is opaque here: only someone who knows the name
/// can open it.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NameRecord {
    pub version: u8,
    /// The tag of the account that may change, transfer or release the name.
    pub owner: OwnerTag,
    /// When the name lapses, in milliseconds since the Unix epoch.
    pub expires: u64,
    pub sealed: Vec<u8>,
}

/// A name's page, sealed like its record.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Page {
    pub version: u8,
    pub sealed: Vec<u8>,
}

/// The reverse record on an account: the name it chooses to be shown as. This
/// is the one place a name appears in the clear, and only if its holder asks.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PrimaryName {
    pub version: u8,
    pub name: String,
}

/// The registry's settings. While the settings account is empty the registry
/// charges nothing.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Settings {
    pub version: u8,
    /// The account that may change the settings.
    pub admin: AccountId,
    /// The account fees are paid to. It pays nothing itself.
    pub treasury: AccountId,
    /// The price of each term in the native token's base units, by term index.
    pub prices: [u128; 3],
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Instruction {
    /// Claim a name that is unregistered, or lapsed past its grace period, for
    /// the payer, with its first sealed profile (which may be empty). `now` is
    /// the sender's claim of the time; the transaction is only valid in a
    /// block at least that late. `fee` is the settings' price for `term`.
    /// Accounts: `[record, page, settings, payer]`, then `treasury` if `fee`
    /// is not 0. `payer` and `treasury` select the native token's shard.
    Register {
        id: NameId,
        sealed: Vec<u8>,
        term: u8,
        fee: u128,
        now: u64,
    },
    /// Extend a name that is active or in grace by `term`; anyone may pay.
    /// Accounts: `[record, settings, payer]`, then `treasury` if `fee` is not
    /// 0, as for `Register`.
    Renew {
        id: NameId,
        term: u8,
        fee: u128,
        now: u64,
    },
    /// Replace the name's sealed profile.
    /// Accounts: `[record, owner]`.
    SetRecord { id: NameId, sealed: Vec<u8> },
    /// Write the name's sealed page; empty removes it.
    /// Accounts: `[record, page, owner]`.
    SetPage { id: NameId, sealed: Vec<u8> },
    /// Hand the name to a new owner. Everything the last owner sealed is
    /// emptied, profile and page, so the name never keeps routing payments or
    /// messages to them.
    /// Accounts: `[record, page, owner]`.
    Transfer { id: NameId, new_owner: OwnerTag },
    /// Give the name up. Its record and page are emptied, so nothing of it
    /// remains and anyone may register it again.
    /// Accounts: `[record, page, owner]`.
    Release { id: NameId },
    /// Publish `name` as what the signer's account is shown as; an empty name
    /// removes it. Unlike everything else here, this puts a name on the chain
    /// in the clear.
    /// Accounts: `[owner]`.
    SetPrimary { name: String },
    /// Set the registry's settings. Anyone may while there are none; afterwards
    /// only their admin.
    /// Accounts: `[settings, admin]`.
    Configure {
        admin: AccountId,
        treasury: AccountId,
        prices: [u128; 3],
    },
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Effect {
    /// On the record: refused unless the shard is empty or holds a record
    /// whose grace period ended by `now`.
    Register {
        owner: OwnerTag,
        now: u64,
        expires: u64,
        sealed: Vec<u8>,
    },
    /// On the record: add `term_ms` to the later of its expiry and `now`,
    /// refused once its grace period has ended.
    Renew { now: u64, term_ms: u64 },
    /// On the settings: changes nothing, refused unless `fee` is what the payer
    /// owes for `term` and `treasury`, when a fee is paid, is where it is owed.
    /// `payer` is the payer's tag for the name `id`, so the treasury can be
    /// told apart without the payer being named.
    RequirePrice {
        id: NameId,
        payer: OwnerTag,
        treasury: Option<AccountId>,
        term: u8,
        fee: u128,
    },
    /// On the settings: replace them, refused unless there are none or `signer`
    /// is their admin.
    WriteSettings {
        signer: AccountId,
        admin: AccountId,
        treasury: AccountId,
        prices: [u128; 3],
    },
    /// On the record: replace the sealed profile, refused unless `signer` owns it.
    Seal { signer: OwnerTag, sealed: Vec<u8> },
    /// On the record: give it to `new_owner` with an empty profile, refused
    /// unless `signer` owns it.
    Transfer {
        signer: OwnerTag,
        new_owner: OwnerTag,
    },
    /// On the record: empty it, refused unless `signer` owns it.
    Remove { signer: OwnerTag },
    /// On the record: changes nothing, refused unless `signer` owns it.
    RequireOwner { signer: OwnerTag },
    /// On the page account; an empty value empties it.
    WritePage { sealed: Vec<u8> },
    /// On the signer's own account.
    WritePrimary { name: String },
}

/// Emitted when a name is claimed. Events are emitted while planning, before
/// the program has checked the record, so they are hints: confirm each against
/// the record. They carry the identifier, never the name, and never a holder.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Registered {
    pub id: NameId,
    pub expires: u64,
}

/// Emitted when a name's term is extended. A hint, like [`Registered`].
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Renewed {
    pub id: NameId,
    pub expires: u64,
}

/// Emitted when a name is handed to a new owner. A hint, like [`Registered`].
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Transferred {
    pub id: NameId,
}

/// Emitted when a name is released. A hint, like [`Registered`].
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Released {
    pub id: NameId,
}

impl Registered {
    pub const SELECTOR_NAME: &str = "logos_names::Registered";
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

fn sha256(bytes: &[u8]) -> [u8; 32] {
    use risc0_zkvm::sha::{Impl, Sha256 as _};

    Impl::hash_bytes(bytes)
        .as_bytes()
        .try_into()
        .expect("SHA-256 output is 32 bytes")
}

/// LEZ's convention for event selectors: the first 8 bytes of
/// `sha256("<program>::<EventName>")`.
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

/// The tag that stands for `account` as the holder of the name `id`. Anyone who
/// knows both can compute it; from the tag alone neither can be recovered, and
/// the same account holding two names gets two unrelated tags.
#[must_use]
pub fn owner_tag(id: &NameId, account: &AccountId) -> OwnerTag {
    let mut bytes = Vec::with_capacity(OWNER_TAG_DOMAIN.len() + 64);
    bytes.extend_from_slice(OWNER_TAG_DOMAIN);
    bytes.extend_from_slice(id);
    bytes.extend_from_slice(account.value());
    sha256(&bytes)
}

/// The account holding the record of the name `id` under the registry `program`.
#[must_use]
pub fn record_address(program: &AccountId, id: &NameId) -> AccountId {
    AccountId::for_public_pda(program, &seed(NAME_SEED_DOMAIN, id))
}

/// The account holding the page of the name `id` under the registry `program`.
#[must_use]
pub fn page_address(program: &AccountId, id: &NameId) -> AccountId {
    AccountId::for_public_pda(program, &seed(PAGE_SEED_DOMAIN, id))
}

/// The account holding the settings of the registry `program`.
#[must_use]
pub fn settings_address(program: &AccountId) -> AccountId {
    AccountId::for_public_pda(program, &PdaSeed::new(sha256(SETTINGS_SEED_DOMAIN)))
}

pub fn validate_name(name: &str) -> Result<(), &'static str> {
    let bytes = name.as_bytes();
    let charset_ok = bytes
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-');
    if bytes.is_empty()
        || bytes.len() > MAX_NAME_LEN
        || !charset_ok
        || bytes[0] == b'-'
        || bytes[bytes.len() - 1] == b'-'
    {
        return Err("name must be 1-32 chars of a-z, 0-9 or '-', not starting or ending with '-'");
    }
    Ok(())
}

/// Turns what a person typed, such as `Jef.lez`, into the bare name. The
/// result still has to pass [`validate_name`].
#[must_use]
pub fn normalize_name(input: &str) -> String {
    let lowered = input.trim().to_ascii_lowercase();
    lowered
        .strip_suffix(NAME_SUFFIX)
        .map_or(lowered.clone(), str::to_owned)
}

fn checked<T>(result: Result<T, &'static str>) -> T {
    result.unwrap_or_else(|reason| panic!("{reason}"))
}

fn accounts<const N: usize>(input: &PlanInput) -> &[AccountMeta; N] {
    <&[AccountMeta; N]>::try_from(input.accounts.as_slice())
        .unwrap_or_else(|_| panic!("expected {N} input accounts, got {}", input.accounts.len()))
}

fn require_signer(owner: &AccountMeta) {
    assert!(owner.is_authorized, "the owner account must sign");
}

fn require_address(account: &AccountMeta, expected: AccountId, what: &str) {
    assert!(
        account.account_id == expected,
        "the {what} account does not match the {}",
        if what == "settings" { "registry" } else { "name" }
    );
}

/// The `[record, owner]` accounts of an instruction on the name `id`.
fn record_and_owner<'input>(
    input: &'input PlanInput,
    id: &NameId,
) -> (&'input AccountMeta, &'input AccountMeta) {
    let [record, owner] = accounts::<2>(input);
    require_address(record, record_address(&input.self_account_id, id), "record");
    require_signer(owner);
    (record, owner)
}

/// The `[record, page, owner]` accounts of an instruction on the name `id`.
fn record_page_and_owner<'input>(
    input: &'input PlanInput,
    id: &NameId,
) -> (&'input AccountMeta, &'input AccountMeta, &'input AccountMeta) {
    let [record, page, owner] = accounts::<3>(input);
    require_address(record, record_address(&input.self_account_id, id), "record");
    require_address(page, page_address(&input.self_account_id, id), "page");
    require_signer(owner);
    (record, page, owner)
}

fn term_length(term: u8) -> u64 {
    *TERM_LENGTHS_MS
        .get(usize::from(term))
        .unwrap_or_else(|| panic!("term must be 0, 1 or 2"))
}

fn after(time: u64, length: u64) -> u64 {
    time.checked_add(length).expect("the time is out of range")
}

/// Checks and charges the price of `term`. `paying` is the `payer` account
/// and, when a fee is paid, the `treasury` account, both with the native
/// token's shard selected. The settings decide in `apply` whether `fee` is
/// right; the fee itself moves by a native token transfer chained to this
/// transaction, which the payer's signature authorizes. Returns the payer.
fn charge(
    plan: &mut Plan,
    id: &NameId,
    settings: &AccountMeta,
    paying: &[AccountMeta],
    term: u8,
    fee: u128,
    now: u64,
) -> AccountId {
    let (payer, treasury) = match (paying, fee) {
        ([payer], 0) => (payer, None),
        ([payer, treasury], 1..) => (payer, Some(treasury)),
        _ => panic!("expected a payer account, then a treasury account only if a fee is paid"),
    };
    assert!(payer.is_authorized, "the payer account must sign");
    assert!(
        paying
            .iter()
            .all(|account| account.program_account_id == NATIVE_TOKEN_PROGRAM_ID),
        "the payer and treasury accounts must select the native token's shard"
    );
    plan.effect(
        settings,
        &Effect::RequirePrice {
            id: *id,
            payer: owner_tag(id, &payer.account_id),
            treasury: treasury.map(|treasury| treasury.account_id),
            term,
            fee,
        },
    );
    // Valid only in a block at least as late as `now`, so `now` is never in
    // the future and `apply` can use it as the time.
    plan.timestamp_window(now..);
    if let Some(treasury) = treasury {
        plan.call(ChainedCall::new(
            NATIVE_TOKEN_PROGRAM_ID,
            vec![
                ProgramShardSelector::native_balance(payer.account_id),
                ProgramShardSelector::native_balance(treasury.account_id),
            ],
            &native_token::Instruction::Transfer { amount: fee },
        ));
    }
    payer.account_id
}

#[must_use]
pub fn plan(input: &PlanInput, instruction: Instruction) -> Plan {
    let mut plan = Plan::new(input);

    match instruction {
        Instruction::Register {
            id,
            sealed,
            term,
            fee,
            now,
        } => {
            assert!(
                sealed.len() <= MAX_SEALED_RECORD_LEN,
                "sealed profile exceeds {MAX_SEALED_RECORD_LEN} bytes"
            );
            let [record, page, settings, paying @ ..] = input.accounts.as_slice() else {
                panic!("expected at least 4 input accounts, got {}", input.accounts.len());
            };
            let program = &input.self_account_id;
            require_address(record, record_address(program, &id), "record");
            require_address(page, page_address(program, &id), "page");
            require_address(settings, settings_address(program), "settings");
            let expires = after(now, term_length(term));
            let owner = charge(&mut plan, &id, settings, paying, term, fee, now);
            plan.event(event(Registered::SELECTOR_NAME, &Registered { id, expires }));
            plan.effect(
                record,
                &Effect::Register {
                    owner: owner_tag(&id, &owner),
                    now,
                    expires,
                    sealed,
                },
            );
            // A lapsed holder's page must not outlive their hold on the name.
            plan.effect(page, &Effect::WritePage { sealed: Vec::new() });
        }
        Instruction::Renew { id, term, fee, now } => {
            let [record, settings, paying @ ..] = input.accounts.as_slice() else {
                panic!("expected at least 3 input accounts, got {}", input.accounts.len());
            };
            let program = &input.self_account_id;
            require_address(record, record_address(program, &id), "record");
            require_address(settings, settings_address(program), "settings");
            let term_ms = term_length(term);
            charge(&mut plan, &id, settings, paying, term, fee, now);
            // The new expiry depends on the record, which only `apply` sees;
            // the event carries the least it can be.
            plan.event(event(
                Renewed::SELECTOR_NAME,
                &Renewed {
                    id,
                    expires: after(now, term_ms),
                },
            ));
            plan.effect(record, &Effect::Renew { now, term_ms });
        }
        Instruction::SetRecord { id, sealed } => {
            assert!(
                sealed.len() <= MAX_SEALED_RECORD_LEN,
                "sealed profile exceeds {MAX_SEALED_RECORD_LEN} bytes"
            );
            let (record, owner) = record_and_owner(input, &id);
            plan.effect(
                record,
                &Effect::Seal {
                    signer: owner_tag(&id, &owner.account_id),
                    sealed,
                },
            );
        }
        Instruction::SetPage { id, sealed } => {
            assert!(
                sealed.len() <= MAX_SEALED_PAGE_LEN,
                "sealed page exceeds {MAX_SEALED_PAGE_LEN} bytes"
            );
            let (record, page, owner) = record_page_and_owner(input, &id);
            plan.effect(
                record,
                &Effect::RequireOwner {
                    signer: owner_tag(&id, &owner.account_id),
                },
            );
            plan.effect(page, &Effect::WritePage { sealed });
        }
        Instruction::Transfer { id, new_owner } => {
            let (record, page, owner) = record_page_and_owner(input, &id);
            plan.event(event(Transferred::SELECTOR_NAME, &Transferred { id }));
            plan.effect(
                record,
                &Effect::Transfer {
                    signer: owner_tag(&id, &owner.account_id),
                    new_owner,
                },
            );
            plan.effect(page, &Effect::WritePage { sealed: Vec::new() });
        }
        Instruction::Release { id } => {
            let (record, page, owner) = record_page_and_owner(input, &id);
            plan.event(event(Released::SELECTOR_NAME, &Released { id }));
            plan.effect(
                record,
                &Effect::Remove {
                    signer: owner_tag(&id, &owner.account_id),
                },
            );
            plan.effect(page, &Effect::WritePage { sealed: Vec::new() });
        }
        Instruction::SetPrimary { name } => {
            if !name.is_empty() {
                checked(validate_name(&name));
            }
            let [owner] = accounts::<1>(input);
            require_signer(owner);
            plan.effect(owner, &Effect::WritePrimary { name });
        }
        Instruction::Configure {
            admin,
            treasury,
            prices,
        } => {
            let [settings, signer] = accounts::<2>(input);
            require_address(settings, settings_address(&input.self_account_id), "settings");
            assert!(signer.is_authorized, "the admin account must sign");
            plan.effect(
                settings,
                &Effect::WriteSettings {
                    signer: signer.account_id,
                    admin,
                    treasury,
                    prices,
                },
            );
        }
    }
    plan
}

fn to_bytes<T: BorshSerialize>(value: &T) -> Vec<u8> {
    borsh::to_vec(value).expect("borsh serialization is infallible")
}

/// Decodes the record in `pre_data`, refusing unless `signer` is its owner's tag.
fn load_owned(pre_data: &[u8], signer: &OwnerTag) -> NameRecord {
    assert!(!pre_data.is_empty(), "name is not registered");
    let record: NameRecord =
        borsh::from_slice(pre_data).expect("a record shard holds a valid record");
    assert!(record.owner == *signer, "signer does not own this name");
    record
}

fn load_settings(pre_data: &[u8]) -> Option<Settings> {
    (!pre_data.is_empty())
        .then(|| borsh::from_slice(pre_data).expect("the settings shard holds valid settings"))
}

/// Applies one effect to the shard it targets. `pre_data` is the shard's
/// current data (empty if unset). Returns the new data, or `None` to leave the
/// shard untouched. Panicking fails the whole transaction.
#[must_use]
pub fn apply(effect: Effect, pre_data: &[u8]) -> Option<Vec<u8>> {
    match effect {
        Effect::Register {
            owner,
            now,
            expires,
            sealed,
        } => {
            if !pre_data.is_empty() {
                let held: NameRecord =
                    borsh::from_slice(pre_data).expect("a record shard holds a valid record");
                assert!(
                    after(held.expires, GRACE_MS) <= now,
                    "name is already registered"
                );
            }
            Some(to_bytes(&NameRecord {
                version: VERSION,
                owner,
                expires,
                sealed,
            }))
        }
        Effect::Renew { now, term_ms } => {
            assert!(!pre_data.is_empty(), "name is not registered");
            let mut record: NameRecord =
                borsh::from_slice(pre_data).expect("a record shard holds a valid record");
            assert!(
                now < after(record.expires, GRACE_MS),
                "name has lapsed and must be registered again"
            );
            record.expires = after(record.expires.max(now), term_ms);
            Some(to_bytes(&record))
        }
        Effect::RequirePrice {
            id,
            payer,
            treasury,
            term,
            fee,
        } => {
            let price = match load_settings(pre_data) {
                Some(settings) if payer != owner_tag(&id, &settings.treasury) => {
                    assert!(
                        treasury.is_none_or(|treasury| treasury == settings.treasury),
                        "the treasury account is not the registry's treasury"
                    );
                    settings.prices[usize::from(term)]
                }
                // An unconfigured registry charges nothing, and a treasury
                // paying itself would move nothing.
                _ => 0,
            };
            assert!(fee == price, "the fee is not the price of this term");
            None
        }
        Effect::WriteSettings {
            signer,
            admin,
            treasury,
            prices,
        } => {
            if let Some(settings) = load_settings(pre_data) {
                assert!(signer == settings.admin, "signer is not the registry's admin");
            }
            Some(to_bytes(&Settings {
                version: VERSION,
                admin,
                treasury,
                prices,
            }))
        }
        Effect::Seal { signer, sealed } => {
            let mut record = load_owned(pre_data, &signer);
            record.sealed = sealed;
            Some(to_bytes(&record))
        }
        Effect::Transfer { signer, new_owner } => {
            let mut record = load_owned(pre_data, &signer);
            record.owner = new_owner;
            record.sealed = Vec::new();
            Some(to_bytes(&record))
        }
        Effect::Remove { signer } => {
            load_owned(pre_data, &signer);
            Some(Vec::new())
        }
        Effect::RequireOwner { signer } => {
            load_owned(pre_data, &signer);
            None
        }
        Effect::WritePage { sealed } => Some(if sealed.is_empty() {
            Vec::new()
        } else {
            to_bytes(&Page {
                version: VERSION,
                sealed,
            })
        }),
        Effect::WritePrimary { name } => Some(if name.is_empty() {
            Vec::new()
        } else {
            to_bytes(&PrimaryName {
                version: VERSION,
                name,
            })
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROGRAM: AccountId = AccountId::new([7; 32]);
    const ALICE: AccountId = AccountId::new([1; 32]);
    const BOB: AccountId = AccountId::new([2; 32]);
    const JEF: NameId = [0xAA; 32];
    const ADA: NameId = [0xBB; 32];
    const CAROL: AccountId = AccountId::new([3; 32]);
    const NOW: u64 = 1_700_000_000_000;
    const YEAR: u64 = TERM_LENGTHS_MS[1];
    const MONTH: u64 = TERM_LENGTHS_MS[0];
    const PRICES: [u128; 3] = [10_000, 100_000, 400_000];

    fn meta(account_id: AccountId, is_authorized: bool) -> AccountMeta {
        AccountMeta::new(account_id, is_authorized, PROGRAM)
    }

    fn input(accounts: Vec<AccountMeta>, instruction: &Instruction) -> PlanInput {
        PlanInput {
            self_account_id: PROGRAM,
            caller_account_id: None,
            accounts,
            instruction_data: borsh::to_vec(instruction).expect("the instruction serializes"),
        }
    }

    fn run_plan(accounts: Vec<AccountMeta>, instruction: Instruction) -> Vec<(AccountId, Effect)> {
        plan(&input(accounts, &instruction), instruction)
            .output()
            .effects
            .iter()
            .map(|effect| {
                (
                    effect.selector.account_id,
                    borsh::from_slice(&effect.data).expect("the effect decodes"),
                )
            })
            .collect()
    }

    fn run_plan_events(accounts: Vec<AccountMeta>, instruction: Instruction) -> Vec<ProgramEvent> {
        plan(&input(accounts, &instruction), instruction)
            .output()
            .events
            .clone()
    }

    fn native(account_id: AccountId) -> AccountMeta {
        AccountMeta::native_balance(account_id, true)
    }

    /// The accounts of a `Register` that pays no fee.
    fn on_registration(id: &NameId, payer: AccountId) -> Vec<AccountMeta> {
        vec![
            meta(record_address(&PROGRAM, id), false),
            meta(page_address(&PROGRAM, id), false),
            meta(settings_address(&PROGRAM), false),
            native(payer),
        ]
    }

    /// The accounts of a `Renew` that pays no fee.
    fn on_renewal(id: &NameId, payer: AccountId) -> Vec<AccountMeta> {
        vec![
            meta(record_address(&PROGRAM, id), false),
            meta(settings_address(&PROGRAM), false),
            native(payer),
        ]
    }

    /// The accounts with the treasury a fee is paid to added.
    fn paying(mut accounts: Vec<AccountMeta>, treasury: AccountId) -> Vec<AccountMeta> {
        accounts.push(AccountMeta::native_balance(treasury, false));
        accounts
    }

    /// A free one-year registration at `NOW`.
    fn register(id: NameId, sealed: Vec<u8>) -> Instruction {
        register_at(id, sealed, 1, 0, NOW)
    }

    fn register_at(id: NameId, sealed: Vec<u8>, term: u8, fee: u128, now: u64) -> Instruction {
        Instruction::Register {
            id,
            sealed,
            term,
            fee,
            now,
        }
    }

    fn on_record(id: &NameId, signer: AccountId) -> Vec<AccountMeta> {
        vec![meta(record_address(&PROGRAM, id), false), meta(signer, true)]
    }

    fn on_record_and_page(id: &NameId, signer: AccountId) -> Vec<AccountMeta> {
        vec![
            meta(record_address(&PROGRAM, id), false),
            meta(page_address(&PROGRAM, id), false),
            meta(signer, true),
        ]
    }

    /// Runs an instruction against a tiny in-memory chain: plans it, then
    /// applies every effect to the shard it targets, all or nothing.
    #[derive(Default)]
    struct Chain {
        shards: std::collections::BTreeMap<AccountId, Vec<u8>>,
    }

    impl Chain {
        fn run(&mut self, accounts: Vec<AccountMeta>, instruction: Instruction) {
            let mut next = self.shards.clone();
            for (account, effect) in run_plan(accounts, instruction) {
                let before = next.get(&account).cloned().unwrap_or_default();
                if let Some(after) = apply(effect, &before) {
                    next.insert(account, after);
                }
            }
            self.shards = next;
        }

        fn data(&self, account: &AccountId) -> &[u8] {
            self.shards.get(account).map_or(&[], Vec::as_slice)
        }

        fn record(&self, id: &NameId) -> Option<NameRecord> {
            let data = self.data(&record_address(&PROGRAM, id));
            (!data.is_empty()).then(|| borsh::from_slice(data).expect("a record"))
        }

        fn page(&self, id: &NameId) -> Option<Page> {
            let data = self.data(&page_address(&PROGRAM, id));
            (!data.is_empty()).then(|| borsh::from_slice(data).expect("a page"))
        }

        fn settings(&self) -> Option<Settings> {
            let data = self.data(&settings_address(&PROGRAM));
            (!data.is_empty()).then(|| borsh::from_slice(data).expect("settings decode"))
        }

        fn configure(&mut self, signer: AccountId, admin: AccountId, prices: [u128; 3]) {
            self.run(
                vec![meta(settings_address(&PROGRAM), false), meta(signer, true)],
                Instruction::Configure {
                    admin,
                    treasury: BOB,
                    prices,
                },
            );
        }

        fn renew(&mut self, id: &NameId, payer: AccountId, now: u64) {
            self.run(
                on_renewal(id, payer),
                Instruction::Renew {
                    id: *id,
                    term: 0,
                    fee: 0,
                    now,
                },
            );
        }

        fn register(&mut self, id: &NameId, owner: AccountId, sealed: &[u8]) {
            self.run(
                on_registration(id, owner),
                register(*id, sealed.to_vec()),
            );
        }
    }

    #[test]
    fn a_name_is_registered_with_its_first_profile() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"sealed profile");
        assert_eq!(
            chain.record(&JEF),
            Some(NameRecord {
                version: VERSION,
                owner: owner_tag(&JEF, &ALICE),
                expires: NOW + TERM_LENGTHS_MS[1],
                sealed: b"sealed profile".to_vec(),
            })
        );
    }

    #[test]
    #[should_panic(expected = "name is already registered")]
    fn a_registered_name_cannot_be_registered_again() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"");
        chain.register(&JEF, BOB, b"");
    }

    #[test]
    #[should_panic(expected = "the record account does not match the name")]
    fn the_record_account_must_be_the_one_derived_from_the_name() {
        let mut chain = Chain::default();
        chain.run(
            on_registration(&ADA, ALICE),
            register(JEF, Vec::new()),
        );
    }

    #[test]
    #[should_panic(expected = "the payer account must sign")]
    fn registering_needs_the_owners_signature() {
        let mut chain = Chain::default();
        chain.run(
            on_registration(&JEF, ALICE)
                .into_iter()
                .map(|account| AccountMeta { is_authorized: false, ..account })
                .collect(),
            register(JEF, Vec::new()),
        );
    }

    #[test]
    fn the_owner_replaces_the_profile() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"first");
        chain.run(
            on_record(&JEF, ALICE),
            Instruction::SetRecord {
                id: JEF,
                sealed: b"second".to_vec(),
            },
        );
        let record = chain.record(&JEF).expect("registered");
        assert_eq!(record.sealed, b"second");
        assert_eq!(record.owner, owner_tag(&JEF, &ALICE));
    }

    #[test]
    #[should_panic(expected = "signer does not own this name")]
    fn someone_else_cannot_replace_the_profile() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"first");
        chain.run(
            on_record(&JEF, BOB),
            Instruction::SetRecord {
                id: JEF,
                sealed: b"hijacked".to_vec(),
            },
        );
    }

    #[test]
    #[should_panic(expected = "name is not registered")]
    fn an_unregistered_name_has_no_profile_to_replace() {
        let mut chain = Chain::default();
        chain.run(
            on_record(&JEF, ALICE),
            Instruction::SetRecord {
                id: JEF,
                sealed: b"x".to_vec(),
            },
        );
    }

    #[test]
    #[should_panic(expected = "sealed profile exceeds")]
    fn an_oversized_profile_is_refused() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, &vec![0; MAX_SEALED_RECORD_LEN + 1]);
    }

    #[test]
    fn the_owner_writes_and_removes_the_page() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"");
        chain.run(
            on_record_and_page(&JEF, ALICE),
            Instruction::SetPage {
                id: JEF,
                sealed: b"sealed page".to_vec(),
            },
        );
        assert_eq!(
            chain.page(&JEF),
            Some(Page {
                version: VERSION,
                sealed: b"sealed page".to_vec(),
            })
        );
        chain.run(
            on_record_and_page(&JEF, ALICE),
            Instruction::SetPage {
                id: JEF,
                sealed: Vec::new(),
            },
        );
        assert_eq!(chain.page(&JEF), None);
    }

    #[test]
    #[should_panic(expected = "signer does not own this name")]
    fn someone_else_cannot_write_the_page() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"");
        chain.run(
            on_record_and_page(&JEF, BOB),
            Instruction::SetPage {
                id: JEF,
                sealed: b"defaced".to_vec(),
            },
        );
    }

    #[test]
    #[should_panic(expected = "the page account does not match the name")]
    fn a_page_cannot_be_written_under_another_name() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"");
        chain.run(
            vec![
                meta(record_address(&PROGRAM, &JEF), false),
                meta(page_address(&PROGRAM, &ADA), false),
                meta(ALICE, true),
            ],
            Instruction::SetPage {
                id: JEF,
                sealed: b"x".to_vec(),
            },
        );
    }

    #[test]
    #[should_panic(expected = "sealed page exceeds")]
    fn an_oversized_page_is_refused() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"");
        chain.run(
            on_record_and_page(&JEF, ALICE),
            Instruction::SetPage {
                id: JEF,
                sealed: vec![0; MAX_SEALED_PAGE_LEN + 1],
            },
        );
    }

    #[test]
    fn a_transfer_hands_over_the_name_and_empties_what_was_sealed() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"alice's profile");
        chain.run(
            on_record_and_page(&JEF, ALICE),
            Instruction::SetPage {
                id: JEF,
                sealed: b"alice's page".to_vec(),
            },
        );
        chain.run(
            on_record_and_page(&JEF, ALICE),
            Instruction::Transfer {
                id: JEF,
                new_owner: owner_tag(&JEF, &BOB),
            },
        );
        assert_eq!(
            chain.record(&JEF),
            Some(NameRecord {
                version: VERSION,
                owner: owner_tag(&JEF, &BOB),
                expires: NOW + TERM_LENGTHS_MS[1],
                sealed: Vec::new(),
            })
        );
        assert_eq!(chain.page(&JEF), None);

        // The new owner can use it; the old one no longer can.
        chain.run(
            on_record(&JEF, BOB),
            Instruction::SetRecord {
                id: JEF,
                sealed: b"bob's profile".to_vec(),
            },
        );
        assert_eq!(chain.record(&JEF).expect("registered").sealed, b"bob's profile");
    }

    #[test]
    #[should_panic(expected = "signer does not own this name")]
    fn someone_else_cannot_transfer_the_name() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"");
        chain.run(
            on_record_and_page(&JEF, BOB),
            Instruction::Transfer {
                id: JEF,
                new_owner: owner_tag(&JEF, &BOB),
            },
        );
    }

    #[test]
    #[should_panic(expected = "signer does not own this name")]
    fn the_last_owner_cannot_change_a_transferred_name() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"");
        chain.run(
            on_record_and_page(&JEF, ALICE),
            Instruction::Transfer {
                id: JEF,
                new_owner: owner_tag(&JEF, &BOB),
            },
        );
        chain.run(
            on_record(&JEF, ALICE),
            Instruction::SetRecord {
                id: JEF,
                sealed: b"still mine".to_vec(),
            },
        );
    }

    #[test]
    fn a_released_name_leaves_nothing_and_can_be_registered_again() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"alice's profile");
        chain.run(
            on_record_and_page(&JEF, ALICE),
            Instruction::SetPage {
                id: JEF,
                sealed: b"alice's page".to_vec(),
            },
        );
        chain.run(on_record_and_page(&JEF, ALICE), Instruction::Release { id: JEF });
        assert_eq!(chain.record(&JEF), None);
        assert_eq!(chain.page(&JEF), None);
        assert!(chain.data(&record_address(&PROGRAM, &JEF)).is_empty());
        assert!(chain.data(&page_address(&PROGRAM, &JEF)).is_empty());

        chain.register(&JEF, BOB, b"bob's profile");
        assert_eq!(chain.record(&JEF).expect("registered").owner, owner_tag(&JEF, &BOB));
    }

    #[test]
    #[should_panic(expected = "signer does not own this name")]
    fn someone_else_cannot_release_the_name() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"");
        chain.run(on_record_and_page(&JEF, BOB), Instruction::Release { id: JEF });
    }

    #[test]
    #[should_panic(expected = "name is not registered")]
    fn an_unregistered_name_cannot_be_released() {
        let mut chain = Chain::default();
        chain.run(on_record_and_page(&JEF, ALICE), Instruction::Release { id: JEF });
    }

    #[test]
    fn one_names_changes_never_touch_another() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"jef");
        chain.register(&ADA, BOB, b"ada");
        chain.run(on_record_and_page(&JEF, ALICE), Instruction::Release { id: JEF });
        assert_eq!(chain.record(&ADA).expect("registered").sealed, b"ada");
        assert_eq!(chain.shards.values().filter(|data| !data.is_empty()).count(), 1);
    }

    #[test]
    fn an_account_publishes_and_withdraws_the_name_it_is_shown_as() {
        let mut chain = Chain::default();
        chain.run(
            vec![meta(ALICE, true)],
            Instruction::SetPrimary { name: "jef".into() },
        );
        let primary: PrimaryName = borsh::from_slice(chain.data(&ALICE)).expect("a reverse record");
        assert_eq!(primary.name, "jef");

        chain.run(
            vec![meta(ALICE, true)],
            Instruction::SetPrimary { name: String::new() },
        );
        assert!(chain.data(&ALICE).is_empty());
    }

    #[test]
    #[should_panic(expected = "name must be 1-32 chars")]
    fn a_published_name_must_be_a_valid_name() {
        let mut chain = Chain::default();
        chain.run(
            vec![meta(ALICE, true)],
            Instruction::SetPrimary { name: "Not A Name".into() },
        );
    }

    #[test]
    #[should_panic(expected = "the owner account must sign")]
    fn nobody_can_publish_a_name_for_someone_elses_account() {
        let mut chain = Chain::default();
        chain.run(
            vec![meta(ALICE, false)],
            Instruction::SetPrimary { name: "jef".into() },
        );
    }

    #[test]
    fn names_are_normalised_and_checked() {
        for typed in ["jef", "jef.lez", "Jef.LEZ", "  jef.lez  "] {
            assert_eq!(normalize_name(typed), "jef", "{typed}");
        }
        for name in ["a", "jef", "my-name-2", &"a".repeat(32)] {
            assert!(validate_name(name).is_ok(), "{name}");
        }
        for name in ["", "-jef", "jef-", "Jef", "j f", "jef.eth", &"a".repeat(33)] {
            assert!(validate_name(name).is_err(), "{name}");
        }
    }

    #[test]
    fn a_names_two_accounts_differ_from_each_other_and_from_other_names() {
        let all = [
            record_address(&PROGRAM, &JEF),
            page_address(&PROGRAM, &JEF),
            record_address(&PROGRAM, &ADA),
            page_address(&PROGRAM, &ADA),
        ];
        for (index, account) in all.iter().enumerate() {
            assert!(!all[index + 1..].contains(account));
        }
        assert_ne!(record_address(&PROGRAM, &JEF), record_address(&ALICE, &JEF));
    }

    /// The vectors in SPEC.md: other implementations must produce the same bytes.
    #[test]
    fn addresses_and_instructions_match_the_specification() {
        let id: NameId = core::array::from_fn(|index| index as u8);
        assert_eq!(record_address(&PROGRAM, &id).to_string(), RECORD_VECTOR);
        assert_eq!(page_address(&PROGRAM, &id).to_string(), PAGE_VECTOR);

        let hex = |instruction: &Instruction| -> String {
            borsh::to_vec(instruction)
                .expect("the instruction serializes")
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect()
        };
        let id_hex: String = id.iter().map(|byte| format!("{byte:02x}")).collect();
        assert_eq!(
            hex(&Instruction::Register {
                id,
                sealed: vec![0xEE, 0xFF],
                term: 1,
                fee: 100_000,
                now: 1_700_000_000_000,
            }),
            format!("00{id_hex}02000000eeff01a08601000000000000000000000000000068e5cf8b010000")
        );
        assert_eq!(
            hex(&Instruction::Renew {
                id,
                term: 2,
                fee: 0,
                now: 1_700_000_000_000,
            }),
            format!("01{id_hex}02{}0068e5cf8b010000", "00".repeat(16))
        );
        assert_eq!(
            hex(&Instruction::Configure {
                admin: ALICE,
                treasury: BOB,
                prices: [1, 2, 3],
            }),
            format!(
                "07{}{}01{z}02{z}03{z}",
                "01".repeat(32),
                "02".repeat(32),
                z = "00".repeat(15)
            )
        );
        assert_eq!(
            hex(&Instruction::SetRecord { id, sealed: vec![0xEE] }),
            format!("02{id_hex}01000000ee")
        );
        assert_eq!(
            hex(&Instruction::SetPage { id, sealed: Vec::new() }),
            format!("03{id_hex}00000000")
        );
        assert_eq!(
            hex(&Instruction::Transfer { id, new_owner: [0x02; 32] }),
            format!("04{id_hex}{}", "02".repeat(32))
        );
        assert_eq!(hex(&Instruction::Release { id }), format!("05{id_hex}"));
        assert_eq!(
            hex(&Instruction::SetPrimary { name: "jef".into() }),
            "06030000006a6566"
        );
    }

    const RECORD_VECTOR: &str = "HqAjYDvn33ACMyTPQ5VELMbfMvQserWSvpPkzCHJPqN9";
    const PAGE_VECTOR: &str = "9UFQXB1QKPHScwFpPzjXUVYGJW3LQCvXHvL7dmSTkaCu";

    #[test]
    fn events_carry_the_identifier_and_never_a_name() {
        let events = run_plan_events(
            on_registration(&JEF, ALICE),
            register(JEF, Vec::new()),
        );
        assert_eq!(events.len(), 1);
        let registered = &events[0];
        assert_eq!(registered.selector, event_selector(Registered::SELECTOR_NAME));
        assert_eq!(
            borsh::from_slice::<Registered>(&registered.data).expect("decodes"),
            Registered {
                id: JEF,
                expires: NOW + TERM_LENGTHS_MS[1],
            }
        );

        let events = run_plan_events(
            on_record_and_page(&JEF, ALICE),
            Instruction::Transfer {
                id: JEF,
                new_owner: owner_tag(&JEF, &BOB),
            },
        );
        assert_eq!(events[0].selector, event_selector(Transferred::SELECTOR_NAME));

        let events = run_plan_events(on_record_and_page(&JEF, ALICE), Instruction::Release { id: JEF });
        assert_eq!(events[0].selector, event_selector(Released::SELECTOR_NAME));

        // Changing a profile or a page says nothing at all.
        assert!(run_plan_events(
            on_record(&JEF, ALICE),
            Instruction::SetRecord { id: JEF, sealed: vec![1] },
        )
        .is_empty());
    }

    #[test]
    fn no_effect_or_event_names_a_holder() {
        // What a private transaction publishes is its effects on public
        // accounts and its events: neither may carry the holder's account.
        let holder = ALICE.into_value();
        let carries_holder = |bytes: &[u8]| bytes.windows(32).any(|window| window == holder);
        let instructions = [
            (on_registration(&JEF, ALICE), register(JEF, vec![9])),
            (on_record(&JEF, ALICE), Instruction::SetRecord { id: JEF, sealed: vec![9] }),
            (on_record_and_page(&JEF, ALICE), Instruction::SetPage { id: JEF, sealed: vec![9] }),
            (
                on_record_and_page(&JEF, ALICE),
                Instruction::Transfer { id: JEF, new_owner: owner_tag(&JEF, &BOB) },
            ),
            (on_record_and_page(&JEF, ALICE), Instruction::Release { id: JEF }),
        ];
        for (accounts, instruction) in instructions {
            let output = plan(&input(accounts, &instruction), instruction);
            let output = output.output();
            for effect in &output.effects {
                assert!(!carries_holder(&effect.data), "an effect carries the holder");
            }
            for event in &output.events {
                assert!(!carries_holder(&event.data), "an event carries the holder");
            }
        }
        // The same holder gets an unrelated tag for each name.
        assert_ne!(owner_tag(&JEF, &ALICE), owner_tag(&ADA, &ALICE));
    }

    #[test]
    fn a_name_past_its_grace_period_can_be_registered_by_anyone() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"alice's profile");
        chain.run(
            on_record_and_page(&JEF, ALICE),
            Instruction::SetPage {
                id: JEF,
                sealed: b"alice's page".to_vec(),
            },
        );
        let later = NOW + YEAR + GRACE_MS;
        chain.run(
            on_registration(&JEF, BOB),
            register_at(JEF, b"bob's profile".to_vec(), 0, 0, later),
        );
        assert_eq!(
            chain.record(&JEF),
            Some(NameRecord {
                version: VERSION,
                owner: owner_tag(&JEF, &BOB),
                expires: later + MONTH,
                sealed: b"bob's profile".to_vec(),
            })
        );
        assert_eq!(chain.page(&JEF), None);
    }

    #[test]
    #[should_panic(expected = "name is already registered")]
    fn a_name_in_its_grace_period_cannot_be_registered() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"");
        chain.run(
            on_registration(&JEF, BOB),
            register_at(JEF, Vec::new(), 1, 0, NOW + YEAR + GRACE_MS - 1),
        );
    }

    #[test]
    fn a_renewal_extends_from_the_later_of_the_expiry_and_now() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"profile");
        chain.renew(&JEF, ALICE, NOW + 1);
        let record = chain.record(&JEF).expect("registered");
        assert_eq!(record.expires, NOW + YEAR + MONTH);
        assert_eq!(record.sealed, b"profile");

        // In grace, and paid for by someone else: the name stays its owner's.
        let late = record.expires + GRACE_MS - 1;
        chain.renew(&JEF, BOB, late);
        let record = chain.record(&JEF).expect("registered");
        assert_eq!((record.owner, record.expires), (owner_tag(&JEF, &ALICE), late + MONTH));
    }

    #[test]
    #[should_panic(expected = "name has lapsed")]
    fn a_name_past_its_grace_period_cannot_be_renewed() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"");
        chain.renew(&JEF, ALICE, NOW + YEAR + GRACE_MS);
    }

    #[test]
    #[should_panic(expected = "name is not registered")]
    fn an_unregistered_name_cannot_be_renewed() {
        Chain::default().renew(&JEF, ALICE, NOW);
    }

    #[test]
    #[should_panic(expected = "term must be 0, 1 or 2")]
    fn an_unknown_term_is_refused() {
        Chain::default().run(
            on_registration(&JEF, ALICE),
            register_at(JEF, Vec::new(), 3, 0, NOW),
        );
    }

    #[test]
    #[should_panic(expected = "the fee is not the price of this term")]
    fn an_unconfigured_registry_takes_no_fee() {
        Chain::default().run(
            paying(on_registration(&JEF, ALICE), BOB),
            register_at(JEF, Vec::new(), 1, 5, NOW),
        );
    }

    #[test]
    fn a_configured_registry_charges_the_price_of_the_term() {
        let mut chain = Chain::default();
        chain.configure(ALICE, ALICE, PRICES);
        chain.run(
            paying(on_registration(&JEF, ALICE), BOB),
            register_at(JEF, Vec::new(), 1, PRICES[1], NOW),
        );
        chain.run(
            paying(on_renewal(&JEF, CAROL), BOB),
            Instruction::Renew {
                id: JEF,
                term: 2,
                fee: PRICES[2],
                now: NOW,
            },
        );
        let record = chain.record(&JEF).expect("registered");
        assert_eq!((record.owner, record.expires), (owner_tag(&JEF, &ALICE), NOW + YEAR + 5 * YEAR));
    }

    #[test]
    #[should_panic(expected = "the fee is not the price of this term")]
    fn a_wrong_fee_is_refused() {
        let mut chain = Chain::default();
        chain.configure(ALICE, ALICE, PRICES);
        chain.run(
            paying(on_registration(&JEF, ALICE), BOB),
            register_at(JEF, Vec::new(), 1, PRICES[0], NOW),
        );
    }

    #[test]
    #[should_panic(expected = "the fee is not the price of this term")]
    fn a_configured_registry_is_not_free() {
        let mut chain = Chain::default();
        chain.configure(ALICE, ALICE, PRICES);
        chain.register(&JEF, ALICE, b"");
    }

    #[test]
    #[should_panic(expected = "the treasury account is not the registry's treasury")]
    fn a_fee_paid_to_another_account_is_refused() {
        let mut chain = Chain::default();
        chain.configure(ALICE, ALICE, PRICES);
        chain.run(
            paying(on_registration(&JEF, ALICE), CAROL),
            register_at(JEF, Vec::new(), 1, PRICES[1], NOW),
        );
    }

    #[test]
    fn the_treasury_pays_nothing() {
        let mut chain = Chain::default();
        chain.configure(ALICE, ALICE, PRICES);
        chain.register(&JEF, BOB, b"");
        assert_eq!(chain.record(&JEF).expect("registered").owner, owner_tag(&JEF, &BOB));
    }

    #[test]
    #[should_panic(expected = "the fee is not the price of this term")]
    fn the_treasury_cannot_pay_itself_a_fee() {
        let mut chain = Chain::default();
        chain.configure(ALICE, ALICE, PRICES);
        chain.run(
            paying(on_registration(&JEF, BOB), BOB),
            register_at(JEF, Vec::new(), 1, PRICES[1], NOW),
        );
    }

    #[test]
    fn anyone_configures_an_unconfigured_registry_and_then_only_its_admin() {
        let mut chain = Chain::default();
        assert_eq!(chain.settings(), None);
        chain.configure(CAROL, ALICE, PRICES);
        chain.configure(ALICE, ALICE, [1, 2, 3]);
        assert_eq!(
            chain.settings(),
            Some(Settings {
                version: VERSION,
                admin: ALICE,
                treasury: BOB,
                prices: [1, 2, 3],
            })
        );
    }

    #[test]
    #[should_panic(expected = "signer is not the registry's admin")]
    fn someone_else_cannot_configure_a_configured_registry() {
        let mut chain = Chain::default();
        chain.configure(CAROL, ALICE, PRICES);
        chain.configure(CAROL, CAROL, PRICES);
    }

    #[test]
    #[should_panic(expected = "the admin account must sign")]
    fn configuring_needs_a_signature() {
        Chain::default().run(
            vec![meta(settings_address(&PROGRAM), false), meta(ALICE, false)],
            Instruction::Configure {
                admin: ALICE,
                treasury: BOB,
                prices: PRICES,
            },
        );
    }

    #[test]
    fn a_fee_is_one_native_transfer_from_the_payer_to_the_treasury() {
        let instruction = register_at(JEF, Vec::new(), 1, PRICES[1], NOW);
        let planned = plan(
            &input(paying(on_registration(&JEF, ALICE), BOB), &instruction),
            instruction,
        );
        let output = planned.output();
        let [call] = output.chained_calls.as_slice() else {
            panic!("expected one chained call");
        };
        assert!(call.program_account_id == NATIVE_TOKEN_PROGRAM_ID);
        assert!(
            call.shard_selectors
                == vec![
                    ProgramShardSelector::native_balance(ALICE),
                    ProgramShardSelector::native_balance(BOB),
                ]
        );
        assert_eq!(
            call.instruction_data,
            borsh::to_vec(&native_token::Instruction::Transfer { amount: PRICES[1] })
                .expect("the transfer serializes")
        );
        let window = output.timestamp_validity_window;
        assert!(window.is_valid_for(NOW) && !window.is_valid_for(NOW - 1));

        let instruction = register(JEF, Vec::new());
        let planned = plan(&input(on_registration(&JEF, ALICE), &instruction), instruction);
        assert!(planned.output().chained_calls.is_empty());
    }
}
