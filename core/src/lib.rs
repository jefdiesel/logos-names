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
//! A name is claimed in two steps, commit then reveal ([`Instruction::Commit`],
//! [`Instruction::Register`]), so that watching pending transactions doesn't let
//! anyone register an identifier before the person who knows its name: the
//! commitment hides the identifier, and the reveal is bound to the committed
//! holder and only valid a while after the commitment.
//!
//! `plan` never sees account data, so every ownership check is carried by an
//! effect and enforced in `apply`, where the shard's current data is visible.
//! `apply` sees no clock either: the time is the sender's claim, which `plan`
//! binds to the block's timestamp and passes on inside the effects.

pub mod shortlist;
use borsh::{BorshDeserialize, BorshSerialize};
use lee_core::{
    account::{AccountId, ProgramShardSelector},
    native_token::{self, NATIVE_TOKEN_PROGRAM_ID},
    program::{AccountMeta, ChainedCall, PdaSeed, Plan, PlanInput, ProgramEvent},
};

/// Record layout version written by this program. Version 3 holds an owner
/// tag where version 2 held the owner's account; version 4 records are laid out
/// as version 3's, and are only written after a commitment ([`Instruction::Commit`]).
/// Version 5 records are laid out as version 3's; a name registered in deposit
/// mode never lapses (`expires` is `u64::MAX`).
pub const VERSION: u8 = 5;
pub const MAX_NAME_LEN: usize = 32;
/// How names are written for people: bare, `jef` (SPEC 2). Kept as a constant
/// so every message that shows a name shows it the same way.
pub const NAME_SUFFIX: &str = "";
/// The suffix names used to be written with. Typed or pasted, it is still
/// accepted and removed, as are a leading `@` and an `lns:` prefix.
pub const LEGACY_SUFFIX: &str = ".lez";
/// Room for a sealed profile with private receiving keys and sixteen entries.
pub const MAX_SEALED_RECORD_LEN: usize = 12 * 1024;
/// LEZ caps shard data at 100 KiB, but a page that large does not fit one
/// transaction under the wallet's default gas limit: 80 KiB applies, 96 KiB fails.
pub const MAX_SEALED_PAGE_LEN: usize = 80 * 1024;

const NAME_SEED_DOMAIN: &[u8] = b"logos-names/v5/name/";
const PAGE_SEED_DOMAIN: &[u8] = b"logos-names/v5/page/";
const SETTINGS_SEED_DOMAIN: &[u8] = b"logos-names/v5/config";
const OWNER_TAG_DOMAIN: &[u8] = b"logos-names/v5/owner";
const COMMITMENT_DOMAIN: &[u8] = b"logos-names/v5/commitment";
const COMMITMENT_SEED_DOMAIN: &[u8] = b"logos-names/v5/commit/";
const DEPOSIT_SEED_DOMAIN: &[u8] = b"logos-names/v5/deposit/";

/// The registry charges a fee per term, paid to its treasury; names lapse.
pub const MODE_FEE: u8 = 0;
/// The registry takes a deposit, held in the name's deposit account until the
/// name is released; names never lapse.
pub const MODE_DEPOSIT: u8 = 1;

/// How far a commit or a reveal may be from the time it claims: it is valid only
/// in a block whose timestamp is in `[now, now + TIME_WINDOW_MS)`. So a claimed
/// time is at most this much earlier than the real one. Long enough for a
/// private holder's proof (minutes on a laptop) to be made and included.
pub const TIME_WINDOW_MS: u64 = 10 * 60 * 1000;
/// How long a commitment must wait before its reveal. With a claimed time off
/// by at most [`TIME_WINDOW_MS`], at least `MIN_COMMIT_AGE_MS - TIME_WINDOW_MS`
/// (five minutes) really passes between a commitment and its reveal, so a
/// reveal seen pending can't be beaten by a commitment made after seeing it.
pub const MIN_COMMIT_AGE_MS: u64 = 15 * 60 * 1000;
/// How long a commitment can still be revealed.
pub const MAX_COMMIT_AGE_MS: u64 = 24 * 60 * 60 * 1000;

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

/// A commitment to claim some name: when it was made. The name, its
/// identifier and the holder are hidden in the commitment's address.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Commitment {
    pub version: u8,
    /// The committer's claimed time, at most [`TIME_WINDOW_MS`] early.
    pub made: u64,
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
    /// Used in [`MODE_FEE`].
    pub prices: [u128; 3],
    /// [`MODE_FEE`] or [`MODE_DEPOSIT`].
    pub mode: u8,
    /// The deposit for a normal name and for a three-character name, in
    /// [`MODE_DEPOSIT`]. A change applies to new registrations only: each name's
    /// deposit stays in its own account.
    pub deposits: [u128; 2],
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Instruction {
    /// Reveal a commitment and claim a name that is unregistered, or lapsed
    /// past its grace period, for the payer, with its first sealed profile
    /// (which may be empty). The commitment must be
    /// [`commitment(id, owner_tag(id, payer), secret)`](commitment), made at
    /// least [`MIN_COMMIT_AGE_MS`] and less than [`MAX_COMMIT_AGE_MS`] before
    /// `now`; revealing uses it up. `now` is the sender's claim of the time; the
    /// transaction is only valid in a block whose timestamp is in
    /// `[now, now + TIME_WINDOW_MS)`. With `deposit` false (fee mode), `fee`
    /// is the settings' price for `term`, paid to the treasury; with `deposit`
    /// true (deposit mode), `fee` is the settings' deposit for the name's
    /// length, paid into the name's deposit account, and `term` is ignored.
    /// Accounts: `[record, page, settings, commitment, payer]`, then, if `fee`
    /// is not 0, `treasury` (fee mode) or the name's [`deposit_address`]
    /// (deposit mode), all money accounts selecting the native token's shard.
    Register {
        id: NameId,
        sealed: Vec<u8>,
        term: u8,
        fee: u128,
        now: u64,
        secret: [u8; 32],
        deposit: bool,
        /// Where the name's identifier sits relative to the short-name list,
        /// which decides its tier ([`shortlist`]).
        length: LengthProof,
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
    /// remains and anyone may register it again, and `refund` moves from the
    /// name's deposit account to the owner: a client passes the account's whole
    /// balance (0 for a name with no deposit).
    /// Accounts: `[record, page, owner]`, then, if `refund` is not 0, the
    /// name's [`deposit_address`]; `owner` and the deposit account select the
    /// native token's shard.
    Release { id: NameId, refund: u128 },
    /// Publish `name` as what the signer's account is shown as; an empty name
    /// removes it. Unlike everything else here, this puts a name on the chain
    /// in the clear.
    /// Accounts: `[owner]`.
    SetPrimary { name: String },
    /// Set the registry's settings. The first settings must be signed by the
    /// program's own account, whose key only its deployer holds; afterwards
    /// only by their admin.
    /// Accounts: `[settings, signer]`.
    Configure {
        admin: AccountId,
        treasury: AccountId,
        prices: [u128; 3],
        mode: u8,
        deposits: [u128; 2],
    },
    /// Commit to claiming a name: the first step of [`Instruction::Register`].
    /// `commitment` is [`commitment`]`(id, owner_tag(id, payer), secret)` for
    /// the account that will reveal it; nothing here names the name, its
    /// identifier or that account. Valid only in a block whose timestamp is in
    /// `[now, now + TIME_WINDOW_MS)`. Anyone may commit; whoever signs pays.
    /// Accounts: `[commitment, signer]`.
    Commit { commitment: [u8; 32], now: u64 },
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
        /// Paid as a deposit (deposit mode) rather than a fee (fee mode).
        deposit: bool,
        /// The name's length tier, as proven against the short-name list.
        tier: u8,
    },
    /// On the settings: replace them, refused unless `signer` is their admin,
    /// or, while there are none, unless the program's own account signed.
    WriteSettings {
        signer: AccountId,
        by_program: bool,
        admin: AccountId,
        treasury: AccountId,
        prices: [u128; 3],
        mode: u8,
        deposits: [u128; 2],
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
    /// On a commitment account: record that it was made at `now`, refused if
    /// one is there that can still be revealed.
    Commit { now: u64 },
    /// On a commitment account: use it up, refused unless it was made at least
    /// [`MIN_COMMIT_AGE_MS`] and less than [`MAX_COMMIT_AGE_MS`] before `now`.
    Reveal { now: u64 },
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

/// What a claim commits to: the name's identifier, the tag of the account that
/// will hold it, and a secret the committer picks at random.
#[must_use]
pub fn commitment(id: &NameId, holder: &OwnerTag, secret: &[u8; 32]) -> [u8; 32] {
    let mut bytes = Vec::with_capacity(COMMITMENT_DOMAIN.len() + 96);
    bytes.extend_from_slice(COMMITMENT_DOMAIN);
    bytes.extend_from_slice(id);
    bytes.extend_from_slice(holder);
    bytes.extend_from_slice(secret);
    sha256(&bytes)
}

/// The account holding the commitment `commitment` under the registry `program`.
#[must_use]
pub fn commitment_address(program: &AccountId, commitment: &[u8; 32]) -> AccountId {
    AccountId::for_public_pda(program, &seed(COMMITMENT_SEED_DOMAIN, commitment))
}

/// Valid only in a block whose timestamp is in `[now, now + TIME_WINDOW_MS)`.
fn bounded_window(plan: &mut Plan, now: u64) {
    let window = lee_core::program::TimestampValidityWindow::try_from((
        Some(now),
        Some(after(now, TIME_WINDOW_MS)),
    ))
    .expect("a window of positive length");
    plan.timestamp_window(window);
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

/// The account holding the deposit of the name `id` under the registry
/// `program`, as its native balance. Only the registry can move it, on release.
#[must_use]
pub fn deposit_address(program: &AccountId, id: &NameId) -> AccountId {
    AccountId::for_public_pda(program, &seed(DEPOSIT_SEED_DOMAIN, id))
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

/// Turns what a person typed, such as `Jef`, `@jef`, `lns:jef` or `jef.lez`,
/// into the bare name. The result still has to pass [`validate_name`].
#[must_use]
pub fn normalize_name(input: &str) -> String {
    let lowered = input.trim().to_ascii_lowercase();
    let bare = lowered.strip_prefix("lns:").unwrap_or(&lowered);
    let bare = bare.strip_prefix('@').unwrap_or(bare);
    bare.strip_suffix(LEGACY_SUFFIX).unwrap_or(bare).to_owned()
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

pub use shortlist::{LengthProof, TIER_LOCKED, TIER_NORMAL, TIER_THREE};

/// The root of the short-name list ([`shortlist`]): the identifiers of every name
/// of one to three characters, with their tiers. Set from `tools/shortlist`'s
/// output (`shortlist/root.hex`) for the v5 derivation: 49,284 names and two
/// sentinels, depth 16. Regenerate it if the derivation ever changes.
pub const SHORTLIST_ROOT: [u8; 32] = hex_literal(b"66c75f76e42f1e87c80b8783c8a315dd84471041b029066bd5f8911d6c5a5e3c");

/// 64 hex characters as 32 bytes, at compile time.
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

/// The tier `proof` shows for the name `id`; refuses a proof that does not hold.
fn length_tier(id: &NameId, proof: &LengthProof) -> u8 {
    #[cfg(not(test))]
    let checked = shortlist::verify(&SHORTLIST_ROOT, id, proof);
    #[cfg(test)]
    let checked = tests::verify_against_test_list(id, proof);
    checked.unwrap_or_else(|reason| panic!("the length proof does not hold: {reason}"))
}

/// Checks and charges a registration or renewal. `paying` is the `payer`
/// account and, when something is paid, the account it is paid to (the
/// treasury in fee mode, the name's deposit account in deposit mode), all with
/// the native token's shard selected. The settings decide in `apply` whether
/// the amount is right; the money moves by a native token transfer chained to
/// this transaction, which the payer's signature authorizes. Returns the payer.
#[allow(clippy::too_many_arguments)]
fn charge(
    plan: &mut Plan,
    program: &AccountId,
    id: &NameId,
    settings: &AccountMeta,
    paying: &[AccountMeta],
    term: u8,
    fee: u128,
    now: u64,
    deposit: bool,
    tier: u8,
) -> AccountId {
    let (payer, to) = match (paying, fee) {
        ([payer], 0) => (payer, None),
        ([payer, to], 1..) => (payer, Some(to)),
        _ => panic!("expected a payer account, then the account paid only if something is paid"),
    };
    assert!(payer.is_authorized, "the payer account must sign");
    assert!(
        paying
            .iter()
            .all(|account| account.program_account_id == NATIVE_TOKEN_PROGRAM_ID),
        "the payer and paid accounts must select the native token's shard"
    );
    if deposit {
        if let Some(to) = to {
            assert!(
                to.account_id == deposit_address(program, id),
                "the deposit account does not match the name"
            );
        }
    }
    plan.effect(
        settings,
        &Effect::RequirePrice {
            id: *id,
            payer: owner_tag(id, &payer.account_id),
            treasury: if deposit { None } else { to.map(|to| to.account_id) },
            term,
            fee,
            deposit,
            tier,
        },
    );
    // Valid only in a block at least as late as `now`, so `now` is never in
    // the future and `apply` can use it as the time.
    plan.timestamp_window(now..);
    if let Some(to) = to {
        plan.call(ChainedCall::new(
            NATIVE_TOKEN_PROGRAM_ID,
            vec![
                ProgramShardSelector::native_balance(payer.account_id),
                ProgramShardSelector::native_balance(to.account_id),
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
            secret,
            deposit,
            length,
        } => {
            assert!(
                sealed.len() <= MAX_SEALED_RECORD_LEN,
                "sealed profile exceeds {MAX_SEALED_RECORD_LEN} bytes"
            );
            let [record, page, settings, committed, paying @ ..] = input.accounts.as_slice() else {
                panic!("expected at least 5 input accounts, got {}", input.accounts.len());
            };
            let program = &input.self_account_id;
            require_address(record, record_address(program, &id), "record");
            require_address(page, page_address(program, &id), "page");
            require_address(settings, settings_address(program), "settings");
            // A deposit holds the name until it is released.
            let expires = if deposit { u64::MAX } else { after(now, term_length(term)) };
            let tier = length_tier(&id, &length);
            let owner = charge(&mut plan, program, &id, settings, paying, term, fee, now, deposit, tier);
            // Bound to the holder: a reveal seen pending can't be replayed for
            // another account.
            let promised = commitment(&id, &owner_tag(&id, &owner), &secret);
            assert!(
                committed.account_id == commitment_address(program, &promised),
                "no commitment to this name for this account and secret"
            );
            plan.effect(committed, &Effect::Reveal { now });
            bounded_window(&mut plan, now);
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
            charge(&mut plan, program, &id, settings, paying, term, fee, now, false, TIER_NORMAL);
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
        Instruction::Release { id, refund } => {
            let program = &input.self_account_id;
            let [record, page, owner, held @ ..] = input.accounts.as_slice() else {
                panic!("expected at least 3 input accounts, got {}", input.accounts.len());
            };
            require_address(record, record_address(program, &id), "record");
            require_address(page, page_address(program, &id), "page");
            require_signer(owner);
            plan.event(event(Released::SELECTOR_NAME, &Released { id }));
            plan.effect(
                record,
                &Effect::Remove {
                    signer: owner_tag(&id, &owner.account_id),
                },
            );
            plan.effect(page, &Effect::WritePage { sealed: Vec::new() });
            match (held, refund) {
                ([], 0) => {}
                ([held], 1..) => {
                    // Whoever holds the name gets its deposit: only the owner
                    // passes `Remove`, and only this program can sign for the
                    // deposit account.
                    let deposit_seed = seed(DEPOSIT_SEED_DOMAIN, &id);
                    assert!(
                        held.account_id == AccountId::for_public_pda(program, &deposit_seed),
                        "the deposit account does not match the name"
                    );
                    assert!(
                        owner.program_account_id == NATIVE_TOKEN_PROGRAM_ID
                            && held.program_account_id == NATIVE_TOKEN_PROGRAM_ID,
                        "the owner and deposit accounts must select the native token's shard"
                    );
                    plan.call(
                        ChainedCall::new(
                            NATIVE_TOKEN_PROGRAM_ID,
                            vec![
                                ProgramShardSelector::native_balance(held.account_id),
                                ProgramShardSelector::native_balance(owner.account_id),
                            ],
                            &native_token::Instruction::Transfer { amount: refund },
                        )
                        .with_pda_seeds(vec![deposit_seed]),
                    );
                }
                _ => panic!("expected the deposit account only if something is refunded"),
            }
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
            mode,
            deposits,
        } => {
            assert!(mode == MODE_FEE || mode == MODE_DEPOSIT, "mode must be fee (0) or deposit (1)");
            let [settings, signer] = accounts::<2>(input);
            require_address(settings, settings_address(&input.self_account_id), "settings");
            assert!(signer.is_authorized, "the admin account must sign");
            plan.effect(
                settings,
                &Effect::WriteSettings {
                    signer: signer.account_id,
                    by_program: signer.account_id == input.self_account_id,
                    admin,
                    treasury,
                    prices,
                    mode,
                    deposits,
                },
            );
        }
        Instruction::Commit { commitment, now } => {
            let [committed, signer] = accounts::<2>(input);
            assert!(
                committed.account_id == commitment_address(&input.self_account_id, &commitment),
                "the commitment account does not match the commitment"
            );
            assert!(signer.is_authorized, "the committing account must sign");
            plan.effect(committed, &Effect::Commit { now });
            bounded_window(&mut plan, now);
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
                    held.expires.saturating_add(GRACE_MS) <= now,
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
            assert!(record.expires != u64::MAX, "a name held by a deposit does not lapse or renew");
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
            deposit,
            tier,
        } => {
            let settings = load_settings(pre_data);
            if tier == TIER_LOCKED {
                // Locked names are released by the admin registering them, then
                // transferring them to whoever they are for.
                assert!(
                    settings.as_ref().is_some_and(|settings| payer == owner_tag(&id, &settings.admin)),
                    "one- and two-character names are locked; only the registry's admin may register one"
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
                        // The admin places names (a locked one it releases, a
                        // built-in one) without a deposit; there is nothing to
                        // refund when they are released.
                        0
                    } else if deposit_mode {
                        settings.deposits[usize::from(tier == TIER_THREE)]
                    } else if payer == owner_tag(&id, &settings.treasury) {
                        // A treasury paying itself would move nothing.
                        0
                    } else {
                        assert!(
                            treasury.is_none_or(|treasury| treasury == settings.treasury),
                            "the treasury account is not the registry's treasury"
                        );
                        settings.prices[usize::from(term)]
                    }
                }
                // An unconfigured registry charges nothing.
                None => 0,
            };
            assert!(
                fee == owed,
                "{}",
                if deposit { "the deposit is not this name's deposit" } else { "the fee is not the price of this term" }
            );
            None
        }
        Effect::WriteSettings {
            signer,
            by_program,
            admin,
            treasury,
            prices,
            mode,
            deposits,
        } => {
            match load_settings(pre_data) {
                Some(settings) => {
                    assert!(signer == settings.admin, "signer is not the registry's admin");
                }
                // Nobody but the deployer can configure a fresh registry.
                None => assert!(by_program, "the first settings must be signed by the program's own account"),
            }
            Some(to_bytes(&Settings {
                version: VERSION,
                admin,
                treasury,
                prices,
                mode,
                deposits,
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
        Effect::Commit { now } => {
            if let Some(held) = load_commitment(pre_data) {
                assert!(
                    after(held.made, MAX_COMMIT_AGE_MS) <= now,
                    "this commitment was already made"
                );
            }
            Some(to_bytes(&Commitment { version: VERSION, made: now }))
        }
        Effect::Reveal { now } => {
            let made = load_commitment(pre_data).expect("no commitment to reveal").made;
            assert!(
                after(made, MIN_COMMIT_AGE_MS) <= now,
                "the commitment is too recent; reveal it later"
            );
            assert!(now < after(made, MAX_COMMIT_AGE_MS), "the commitment has expired");
            Some(Vec::new())
        }
    }
}

fn load_commitment(pre_data: &[u8]) -> Option<Commitment> {
    (!pre_data.is_empty())
        .then(|| borsh::from_slice(pre_data).expect("a commitment shard holds a commitment"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tests' stand-in for the short-name list: two locked names and two
    /// three-character names, among ids the other tests don't use.
    pub(super) const LOCKED_NAME: NameId = [0x11; 32];
    pub(super) const LOCKED_NAME_2: NameId = [0x12; 32];
    pub(super) const THREE_NAME: NameId = [0x33; 32];
    pub(super) const THREE_NAME_2: NameId = [0x34; 32];

    fn test_list() -> &'static shortlist::ShortList {
        static LIST: std::sync::OnceLock<shortlist::ShortList> = std::sync::OnceLock::new();
        LIST.get_or_init(|| {
            shortlist::ShortList::new(vec![
                (LOCKED_NAME, TIER_LOCKED),
                (LOCKED_NAME_2, TIER_LOCKED),
                (THREE_NAME, TIER_THREE),
                (THREE_NAME_2, TIER_THREE),
            ])
            .expect("a valid test list")
        })
    }

    pub(super) fn verify_against_test_list(id: &NameId, proof: &LengthProof) -> Result<u8, &'static str> {
        let list = test_list();
        shortlist::verify_depth(&list.root(), id, proof, list.depth())
    }

    /// The true length proof for `id` under the test list.
    fn length_of(id: &NameId) -> LengthProof {
        test_list().prove(id).expect("a proof")
    }

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
    /// The secret the test registrations commit with.
    const SECRET: [u8; 32] = [5; 32];

    fn committed(id: &NameId, holder: AccountId) -> AccountId {
        commitment_address(&PROGRAM, &commitment(id, &owner_tag(id, &holder), &SECRET))
    }

    fn on_registration(id: &NameId, payer: AccountId) -> Vec<AccountMeta> {
        vec![
            meta(record_address(&PROGRAM, id), false),
            meta(page_address(&PROGRAM, id), false),
            meta(settings_address(&PROGRAM), false),
            meta(committed(id, payer), false),
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
            secret: SECRET,
            deposit: false,
            length: length_of(&id),
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
            // Tests that are not about commitments register as if one had been
            // made long enough before.
            if let (Instruction::Register { now, .. }, Some(slot)) = (&instruction, accounts.get(3)) {
                self.shards.entry(slot.account_id).or_insert_with(|| {
                    to_bytes(&Commitment { version: VERSION, made: now - MIN_COMMIT_AGE_MS })
                });
            }
            self.run_exactly(accounts, instruction);
        }

        fn run_exactly(&mut self, accounts: Vec<AccountMeta>, instruction: Instruction) {
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

        fn commit(&mut self, id: &NameId, holder: AccountId, now: u64) {
            let promised = commitment(id, &owner_tag(id, &holder), &SECRET);
            self.run_exactly(
                vec![meta(commitment_address(&PROGRAM, &promised), false), meta(CAROL, true)],
                Instruction::Commit { commitment: promised, now },
            );
        }

        fn reveal(&mut self, id: &NameId, holder: AccountId, now: u64) {
            self.run_exactly(on_registration(id, holder), register_at(*id, Vec::new(), 1, 0, now));
        }

        /// The first settings are signed by the program's own account, as its
        /// deployer does; later ones by `signer`.
        fn configure(&mut self, signer: AccountId, admin: AccountId, prices: [u128; 3]) {
            let signer = if self.settings().is_none() { PROGRAM } else { signer };
            self.run(
                vec![meta(settings_address(&PROGRAM), false), meta(signer, true)],
                Instruction::Configure {
                    admin,
                    treasury: BOB,
                    prices,
                    mode: MODE_FEE,
                    deposits: [0, 0],
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
        chain.run(on_record_and_page(&JEF, ALICE), Instruction::Release { id: JEF, refund: 0 });
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
        chain.run(on_record_and_page(&JEF, BOB), Instruction::Release { id: JEF, refund: 0 });
    }

    #[test]
    #[should_panic(expected = "name is not registered")]
    fn an_unregistered_name_cannot_be_released() {
        let mut chain = Chain::default();
        chain.run(on_record_and_page(&JEF, ALICE), Instruction::Release { id: JEF, refund: 0 });
    }

    #[test]
    fn one_names_changes_never_touch_another() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"jef");
        chain.register(&ADA, BOB, b"ada");
        chain.run(on_record_and_page(&JEF, ALICE), Instruction::Release { id: JEF, refund: 0 });
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
                secret: [0x5A; 32],
                deposit: false,
                length: LengthProof::Membership(shortlist::Membership { index: 7, tier: TIER_THREE, path: Vec::new() }),
            }),
            format!(
                "00{id_hex}02000000eeff01a08601000000000000000000000000000068e5cf8b010000{}00000700000003{}",
                "5a".repeat(32),
                "00".repeat(4)
            )
        );
        assert_eq!(
            hex(&Instruction::Commit {
                commitment: [0xC0; 32],
                now: 1_700_000_000_000,
            }),
            format!("08{}0068e5cf8b010000", "c0".repeat(32))
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
                mode: MODE_FEE,
                deposits: [0, 0],
            }),
            format!(
                "07{}{}01{z}02{z}03{z}00{}",
                "01".repeat(32),
                "02".repeat(32),
                "00".repeat(32),
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
        assert_eq!(hex(&Instruction::Release { id, refund: 0 }), format!("05{id_hex}{}", "00".repeat(16)));
        assert_eq!(
            hex(&Instruction::SetPrimary { name: "jef".into() }),
            "06030000006a6566"
        );
    }

    const RECORD_VECTOR: &str = "7bwwvMLtxqqNYiXsd5bYaq8e31E1odYf6jAK5z7tmUdd";
    const PAGE_VECTOR: &str = "8M4vmdxCXL4eS8QXiHSdbeUUPt5Vbx1UVjLcv2cPk9Y5";

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

        let events = run_plan_events(on_record_and_page(&JEF, ALICE), Instruction::Release { id: JEF, refund: 0 });
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
            (on_record_and_page(&JEF, ALICE), Instruction::Release { id: JEF, refund: 0 }),
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
    #[should_panic(expected = "the first settings must be signed by the program's own account")]
    fn nobody_but_the_deployer_configures_a_fresh_registry() {
        Chain::default().run(
            vec![meta(settings_address(&PROGRAM), false), meta(CAROL, true)],
            Instruction::Configure {
                admin: CAROL,
                treasury: CAROL,
                prices: PRICES,
                mode: MODE_FEE,
                deposits: [0, 0],
            },
        );
    }

    #[test]
    fn a_name_is_claimed_by_commit_then_reveal() {
        let mut chain = Chain::default();
        chain.commit(&JEF, ALICE, NOW - MIN_COMMIT_AGE_MS);
        chain.reveal(&JEF, ALICE, NOW);
        assert_eq!(chain.record(&JEF).map(|r| r.owner), Some(owner_tag(&JEF, &ALICE)));
        assert!(chain.data(&committed(&JEF, ALICE)).is_empty(), "revealing uses the commitment up");
    }

    #[test]
    #[should_panic(expected = "the commitment is too recent")]
    fn a_reveal_waits_for_its_commitment_to_age() {
        let mut chain = Chain::default();
        chain.commit(&JEF, ALICE, NOW);
        chain.reveal(&JEF, ALICE, NOW + MIN_COMMIT_AGE_MS - 1);
    }

    #[test]
    #[should_panic(expected = "the commitment has expired")]
    fn an_old_commitment_cannot_be_revealed() {
        let mut chain = Chain::default();
        chain.commit(&JEF, ALICE, NOW);
        chain.reveal(&JEF, ALICE, NOW + MAX_COMMIT_AGE_MS);
    }

    #[test]
    #[should_panic(expected = "no commitment to reveal")]
    fn a_reveal_without_a_commitment_is_refused() {
        Chain::default().reveal(&JEF, ALICE, NOW);
    }

    /// A front-runner who sees Alice's reveal pending knows `id` and `secret`, but
    /// the commitment is bound to Alice, so it can't claim the name with it.
    #[test]
    #[should_panic(expected = "no commitment to this name for this account and secret")]
    fn a_seen_reveal_cannot_be_replayed_for_another_account() {
        let mut chain = Chain::default();
        chain.commit(&JEF, ALICE, NOW - MIN_COMMIT_AGE_MS);
        let mut accounts = on_registration(&JEF, BOB);
        accounts[3] = meta(committed(&JEF, ALICE), false);
        chain.run_exactly(accounts, register_at(JEF, Vec::new(), 1, 0, NOW));
    }

    #[test]
    #[should_panic(expected = "this commitment was already made")]
    fn a_live_commitment_cannot_be_reset() {
        let mut chain = Chain::default();
        chain.commit(&JEF, ALICE, NOW - MIN_COMMIT_AGE_MS);
        chain.commit(&JEF, ALICE, NOW);
    }

    #[test]
    fn commit_and_reveal_are_valid_only_near_their_claimed_time() {
        let promised = commitment(&JEF, &owner_tag(&JEF, &ALICE), &SECRET);
        for instruction in [
            Instruction::Commit { commitment: promised, now: NOW },
            register_at(JEF, Vec::new(), 1, 0, NOW),
        ] {
            let accounts = match &instruction {
                Instruction::Commit { .. } => {
                    vec![meta(commitment_address(&PROGRAM, &promised), false), meta(CAROL, true)]
                }
                _ => on_registration(&JEF, ALICE),
            };
            let window = plan(&input(accounts, &instruction), instruction).output().timestamp_validity_window;
            assert_eq!((window.start(), window.end()), (Some(NOW), Some(NOW + TIME_WINDOW_MS)));
        }
    }

    #[test]
    fn the_deployer_configures_a_fresh_registry_and_then_only_its_admin() {
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
                mode: MODE_FEE,
                deposits: [0, 0],
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
                mode: MODE_FEE,
                deposits: [0, 0],
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

    const DEPOSITS: [u128; 2] = [100_000_000, 2_000_000_000];

    impl Chain {
        /// A registry in deposit mode, administered by `admin`.
        fn configure_deposits(&mut self, admin: AccountId) {
            self.run(
                vec![meta(settings_address(&PROGRAM), false), meta(PROGRAM, true)],
                Instruction::Configure {
                    admin,
                    treasury: BOB,
                    prices: PRICES,
                    mode: MODE_DEPOSIT,
                    deposits: DEPOSITS,
                },
            );
        }

        /// A deposit-mode registration of `id` for `holder`, paying `deposit`.
        fn register_with_deposit(&mut self, id: &NameId, holder: AccountId, deposit: u128, now: u64) {
            let mut accounts = on_registration(id, holder);
            if deposit > 0 {
                accounts.push(AccountMeta::native_balance(deposit_address(&PROGRAM, id), false));
            }
            self.run(accounts, deposit_registration(*id, deposit, now));
        }
    }

    fn deposit_registration(id: NameId, deposit: u128, now: u64) -> Instruction {
        Instruction::Register {
            id,
            sealed: Vec::new(),
            term: 0,
            fee: deposit,
            now,
            secret: SECRET,
            deposit: true,
            length: length_of(&id),
        }
    }

    /// The `[record, page, owner]` accounts of a release, with the owner's native
    /// shard, and the deposit account when something is refunded.
    fn on_release(id: &NameId, owner: AccountId, refund: u128) -> Vec<AccountMeta> {
        let mut accounts = vec![
            meta(record_address(&PROGRAM, id), false),
            meta(page_address(&PROGRAM, id), false),
            AccountMeta::native_balance(owner, true),
        ];
        if refund > 0 {
            accounts.push(AccountMeta::native_balance(deposit_address(&PROGRAM, id), false));
        }
        accounts
    }

    fn only_call(accounts: Vec<AccountMeta>, instruction: Instruction) -> ChainedCall {
        let planned = plan(&input(accounts, &instruction), instruction);
        let [call] = planned.output().chained_calls.as_slice() else {
            panic!("expected one chained call");
        };
        call.clone()
    }

    #[test]
    fn a_deposit_moves_from_the_payer_into_the_names_deposit_account() {
        let mut accounts = on_registration(&JEF, ALICE);
        accounts.push(AccountMeta::native_balance(deposit_address(&PROGRAM, &JEF), false));
        let call = only_call(accounts, deposit_registration(JEF, DEPOSITS[0], NOW));
        assert!(call.program_account_id == NATIVE_TOKEN_PROGRAM_ID);
        assert!(
            call.shard_selectors
                == vec![
                    ProgramShardSelector::native_balance(ALICE),
                    ProgramShardSelector::native_balance(deposit_address(&PROGRAM, &JEF)),
                ]
        );
        assert_eq!(
            call.instruction_data,
            borsh::to_vec(&native_token::Instruction::Transfer { amount: DEPOSITS[0] }).expect("serializes")
        );
        assert!(call.pda_seeds.is_empty(), "the payer's own signature authorizes a deposit");
    }

    #[test]
    #[should_panic(expected = "the deposit account does not match the name")]
    fn a_deposit_into_another_names_account_is_refused() {
        let mut accounts = on_registration(&JEF, ALICE);
        accounts.push(AccountMeta::native_balance(deposit_address(&PROGRAM, &ADA), false));
        let instruction = deposit_registration(JEF, DEPOSITS[0], NOW);
        let _ = plan(&input(accounts, &instruction), instruction);
    }

    #[test]
    #[should_panic(expected = "the deposit account does not match the name")]
    fn a_deposit_to_the_treasury_is_refused() {
        let mut accounts = on_registration(&JEF, ALICE);
        accounts.push(AccountMeta::native_balance(BOB, false));
        let instruction = deposit_registration(JEF, DEPOSITS[0], NOW);
        let _ = plan(&input(accounts, &instruction), instruction);
    }

    #[test]
    fn a_name_held_by_a_deposit_never_lapses() {
        let mut chain = Chain::default();
        chain.configure_deposits(CAROL);
        chain.register_with_deposit(&JEF, ALICE, DEPOSITS[0], NOW);
        let record = chain.record(&JEF).expect("registered");
        assert_eq!(record.expires, u64::MAX);
        assert_eq!(record.owner, owner_tag(&JEF, &ALICE));
        assert_eq!(record.version, 5);
    }

    #[test]
    #[should_panic(expected = "name is already registered")]
    fn nobody_takes_a_deposit_held_name_even_decades_later() {
        let mut chain = Chain::default();
        chain.configure_deposits(CAROL);
        chain.register_with_deposit(&JEF, ALICE, DEPOSITS[0], NOW);
        chain.register_with_deposit(&JEF, BOB, DEPOSITS[0], NOW + 50 * YEAR);
    }

    #[test]
    #[should_panic(expected = "the deposit is not this name's deposit")]
    fn a_short_deposit_is_refused() {
        let mut chain = Chain::default();
        chain.configure_deposits(CAROL);
        chain.register_with_deposit(&JEF, ALICE, DEPOSITS[0] - 1, NOW);
    }

    #[test]
    #[should_panic(expected = "the deposit is not this name's deposit")]
    fn no_deposit_is_refused_in_deposit_mode() {
        let mut chain = Chain::default();
        chain.configure_deposits(CAROL);
        chain.register_with_deposit(&JEF, ALICE, 0, NOW);
    }

    #[test]
    #[should_panic(expected = "the registry takes deposits")]
    fn a_fee_registration_is_refused_in_deposit_mode() {
        let mut chain = Chain::default();
        chain.configure_deposits(CAROL);
        chain.run(paying(on_registration(&JEF, ALICE), BOB), register_at(JEF, Vec::new(), 1, PRICES[1], NOW));
    }

    #[test]
    #[should_panic(expected = "the registry charges fees, not deposits")]
    fn a_deposit_registration_is_refused_in_fee_mode() {
        let mut chain = Chain::default();
        chain.configure(PROGRAM, ALICE, PRICES);
        chain.register_with_deposit(&JEF, ALICE, DEPOSITS[0], NOW);
    }

    #[test]
    #[should_panic(expected = "the registry takes deposits")]
    fn names_do_not_renew_in_deposit_mode() {
        let mut chain = Chain::default();
        chain.configure_deposits(CAROL);
        chain.register_with_deposit(&JEF, ALICE, DEPOSITS[0], NOW);
        chain.run(
            paying(on_renewal(&JEF, ALICE), BOB),
            Instruction::Renew { id: JEF, term: 1, fee: PRICES[1], now: NOW },
        );
    }

    #[test]
    #[should_panic(expected = "a name held by a deposit does not lapse or renew")]
    fn a_deposit_name_cannot_be_renewed_even_after_a_switch_to_fees() {
        let mut chain = Chain::default();
        chain.configure_deposits(CAROL);
        chain.register_with_deposit(&JEF, ALICE, DEPOSITS[0], NOW);
        chain.configure(CAROL, CAROL, PRICES);
        chain.run(
            paying(on_renewal(&JEF, ALICE), BOB),
            Instruction::Renew { id: JEF, term: 1, fee: PRICES[1], now: NOW },
        );
    }

    #[test]
    fn a_release_refunds_the_deposit_to_the_holder_from_the_names_account() {
        let call = only_call(
            on_release(&JEF, ALICE, DEPOSITS[0]),
            Instruction::Release { id: JEF, refund: DEPOSITS[0] },
        );
        assert!(call.program_account_id == NATIVE_TOKEN_PROGRAM_ID);
        assert!(
            call.shard_selectors
                == vec![
                    ProgramShardSelector::native_balance(deposit_address(&PROGRAM, &JEF)),
                    ProgramShardSelector::native_balance(ALICE),
                ]
        );
        assert_eq!(
            call.instruction_data,
            borsh::to_vec(&native_token::Instruction::Transfer { amount: DEPOSITS[0] }).expect("serializes")
        );
        // Only the registry can sign for the deposit account.
        assert!(call.pda_seeds == vec![seed(DEPOSIT_SEED_DOMAIN, &JEF)]);
    }

    #[test]
    #[should_panic(expected = "signer does not own this name")]
    fn only_the_holder_collects_the_deposit() {
        let mut chain = Chain::default();
        chain.configure_deposits(CAROL);
        chain.register_with_deposit(&JEF, ALICE, DEPOSITS[0], NOW);
        chain.run(on_release(&JEF, BOB, DEPOSITS[0]), Instruction::Release { id: JEF, refund: DEPOSITS[0] });
    }

    #[test]
    #[should_panic(expected = "the deposit account does not match the name")]
    fn a_refund_from_another_names_deposit_is_refused() {
        let mut accounts = on_release(&JEF, ALICE, 0);
        accounts.push(AccountMeta::native_balance(deposit_address(&PROGRAM, &ADA), false));
        let instruction = Instruction::Release { id: JEF, refund: DEPOSITS[0] };
        let _ = plan(&input(accounts, &instruction), instruction);
    }

    #[test]
    #[should_panic(expected = "expected the deposit account only if something is refunded")]
    fn a_refund_without_the_deposit_account_is_refused() {
        let instruction = Instruction::Release { id: JEF, refund: DEPOSITS[0] };
        let _ = plan(&input(on_release(&JEF, ALICE, 0), &instruction), instruction);
    }

    #[test]
    fn a_transferred_name_keeps_its_deposit_for_the_new_holder() {
        let mut chain = Chain::default();
        chain.configure_deposits(CAROL);
        chain.register_with_deposit(&JEF, ALICE, DEPOSITS[0], NOW);
        chain.run(
            on_record_and_page(&JEF, ALICE),
            Instruction::Transfer { id: JEF, new_owner: owner_tag(&JEF, &BOB) },
        );
        // The new holder releases it and is refunded; the old one no longer can.
        chain.run(on_release(&JEF, BOB, DEPOSITS[0]), Instruction::Release { id: JEF, refund: DEPOSITS[0] });
        assert!(chain.record(&JEF).is_none());
    }

    #[test]
    fn a_released_deposit_name_can_be_registered_again() {
        let mut chain = Chain::default();
        chain.configure_deposits(CAROL);
        chain.register_with_deposit(&JEF, ALICE, DEPOSITS[0], NOW);
        chain.run(on_release(&JEF, ALICE, DEPOSITS[0]), Instruction::Release { id: JEF, refund: DEPOSITS[0] });
        chain.register_with_deposit(&JEF, BOB, DEPOSITS[0], NOW + MONTH);
        assert_eq!(chain.record(&JEF).expect("registered").owner, owner_tag(&JEF, &BOB));
    }

    #[test]
    fn the_admin_switches_modes_and_changes_deposits() {
        let mut chain = Chain::default();
        chain.configure_deposits(CAROL);
        chain.run(
            vec![meta(settings_address(&PROGRAM), false), meta(CAROL, true)],
            Instruction::Configure {
                admin: CAROL,
                treasury: BOB,
                prices: PRICES,
                mode: MODE_DEPOSIT,
                deposits: [5, 50],
            },
        );
        assert_eq!(chain.settings().expect("configured").deposits, [5, 50]);
        chain.register_with_deposit(&JEF, ALICE, 5, NOW);
    }

    #[test]
    #[should_panic(expected = "mode must be fee (0) or deposit (1)")]
    fn an_unknown_mode_is_refused() {
        let mut chain = Chain::default();
        chain.run(
            vec![meta(settings_address(&PROGRAM), false), meta(PROGRAM, true)],
            Instruction::Configure { admin: CAROL, treasury: BOB, prices: PRICES, mode: 2, deposits: DEPOSITS },
        );
    }

    fn deposit_registration_with(id: NameId, deposit: u128, length: LengthProof) -> Instruction {
        Instruction::Register {
            id,
            sealed: Vec::new(),
            term: 0,
            fee: deposit,
            now: NOW,
            secret: SECRET,
            deposit: true,
            length,
        }
    }

    fn with_deposit_account(id: &NameId, holder: AccountId) -> Vec<AccountMeta> {
        let mut accounts = on_registration(id, holder);
        accounts.push(AccountMeta::native_balance(deposit_address(&PROGRAM, id), false));
        accounts
    }

    #[test]
    fn a_three_character_name_takes_the_higher_deposit() {
        let mut chain = Chain::default();
        chain.configure_deposits(CAROL);
        chain.register_with_deposit(&THREE_NAME, ALICE, DEPOSITS[1], NOW);
        assert!(chain.record(&THREE_NAME).is_some());
    }

    #[test]
    #[should_panic(expected = "the deposit is not this name's deposit")]
    fn a_three_character_name_is_refused_at_the_normal_deposit() {
        let mut chain = Chain::default();
        chain.configure_deposits(CAROL);
        chain.register_with_deposit(&THREE_NAME, ALICE, DEPOSITS[0], NOW);
    }

    #[test]
    #[should_panic(expected = "the length proof does not hold")]
    fn a_short_name_cannot_pass_as_a_normal_one() {
        // A non-membership proof built around the short name's neighbours.
        let around_ada = length_of(&ADA);
        let mut chain = Chain::default();
        chain.configure_deposits(CAROL);
        chain.run(with_deposit_account(&THREE_NAME, ALICE), deposit_registration_with(THREE_NAME, DEPOSITS[0], around_ada));
    }

    #[test]
    #[should_panic(expected = "the length proof does not hold")]
    fn a_short_name_cannot_borrow_another_short_names_proof() {
        let mut chain = Chain::default();
        chain.configure_deposits(CAROL);
        chain.run(
            with_deposit_account(&LOCKED_NAME, ALICE),
            deposit_registration_with(LOCKED_NAME, DEPOSITS[1], length_of(&THREE_NAME)),
        );
    }

    #[test]
    #[should_panic(expected = "only the registry's admin may register one")]
    fn a_locked_name_is_refused_to_everyone_but_the_admin() {
        let mut chain = Chain::default();
        chain.configure_deposits(CAROL);
        chain.register_with_deposit(&LOCKED_NAME, ALICE, DEPOSITS[0], NOW);
    }

    #[test]
    fn the_admin_releases_a_locked_name_by_registering_and_transferring_it() {
        let mut chain = Chain::default();
        chain.configure_deposits(CAROL);
        chain.register_with_deposit(&LOCKED_NAME, CAROL, 0, NOW);
        chain.run(
            on_record_and_page(&LOCKED_NAME, CAROL),
            Instruction::Transfer { id: LOCKED_NAME, new_owner: owner_tag(&LOCKED_NAME, &ALICE) },
        );
        assert_eq!(chain.record(&LOCKED_NAME).expect("held").owner, owner_tag(&LOCKED_NAME, &ALICE));
    }

    #[test]
    #[should_panic(expected = "only the registry's admin may register one")]
    fn locked_names_are_locked_in_fee_mode_too() {
        let mut chain = Chain::default();
        chain.configure(PROGRAM, CAROL, PRICES);
        chain.run(on_registration(&LOCKED_NAME_2, ALICE), register_at(LOCKED_NAME_2, Vec::new(), 1, 0, NOW));
    }

    #[test]
    #[should_panic(expected = "only the registry's admin may register one")]
    fn an_unconfigured_registry_locks_short_names() {
        let mut chain = Chain::default();
        chain.run(on_registration(&LOCKED_NAME, ALICE), register_at(LOCKED_NAME, Vec::new(), 1, 0, NOW));
    }

    #[test]
    fn the_admin_places_a_name_without_a_deposit() {
        let mut chain = Chain::default();
        chain.configure_deposits(CAROL);
        chain.register_with_deposit(&THREE_NAME, CAROL, 0, NOW);
        chain.run(
            on_record_and_page(&THREE_NAME, CAROL),
            Instruction::Transfer { id: THREE_NAME, new_owner: owner_tag(&THREE_NAME, &ALICE) },
        );
        assert_eq!(chain.record(&THREE_NAME).expect("held").owner, owner_tag(&THREE_NAME, &ALICE));
    }

    #[test]
    #[should_panic(expected = "the deposit is not this name's deposit")]
    fn only_the_admin_skips_the_deposit() {
        let mut chain = Chain::default();
        chain.configure_deposits(CAROL);
        chain.register_with_deposit(&THREE_NAME, ALICE, 0, NOW);
    }
}
