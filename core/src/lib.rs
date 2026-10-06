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
//! What stays public is what LEZ itself makes public: that some identifier is
//! registered, and which account holds it, since that account signs for it.
//!
//! Each name owns this program's shard on two accounts, both derived from its
//! identifier: the record ([`record_address`]) and the page ([`page_address`]).
//! A reverse record, the one thing a holder may choose to publish in the
//! clear, lives on the holder's own account.
//!
//! The registry keeps no history. Releasing a name empties its record and page;
//! transferring a name empties everything its last holder sealed.
//!
//! `plan` never sees account data, so every ownership check is carried by an
//! effect and enforced in `apply`, where the shard's current data is visible.

use borsh::{BorshDeserialize, BorshSerialize};
use lee_core::{
    account::AccountId,
    program::{AccountMeta, PdaSeed, Plan, PlanInput, ProgramEvent},
};

/// Record layout version written by this program.
pub const VERSION: u8 = 2;
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

/// What the chain knows a name by: 32 bytes a client derives from the name.
/// The derivation is deliberately slow and is not this program's business.
pub type NameId = [u8; 32];

/// A registered name. `sealed` is opaque here: only someone who knows the name
/// can open it.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NameRecord {
    pub version: u8,
    /// The account that may change, transfer or release the name.
    pub owner: AccountId,
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

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Instruction {
    /// Claim an unregistered name for the signer, with its first sealed
    /// profile (which may be empty).
    /// Accounts: `[record, owner]`.
    Register { id: NameId, sealed: Vec<u8> },
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
    Transfer { id: NameId, new_owner: AccountId },
    /// Give the name up. Its record and page are emptied, so nothing of it
    /// remains and anyone may register it again.
    /// Accounts: `[record, page, owner]`.
    Release { id: NameId },
    /// Publish `name` as what the signer's account is shown as; an empty name
    /// removes it. Unlike everything else here, this puts a name on the chain
    /// in the clear.
    /// Accounts: `[owner]`.
    SetPrimary { name: String },
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Effect {
    /// On the record: refused unless the shard is empty.
    Register { owner: AccountId, sealed: Vec<u8> },
    /// On the record: replace the sealed profile, refused unless `signer` owns it.
    Seal { signer: AccountId, sealed: Vec<u8> },
    /// On the record: give it to `new_owner` with an empty profile, refused
    /// unless `signer` owns it.
    Transfer {
        signer: AccountId,
        new_owner: AccountId,
    },
    /// On the record: empty it, refused unless `signer` owns it.
    Remove { signer: AccountId },
    /// On the record: changes nothing, refused unless `signer` owns it.
    RequireOwner { signer: AccountId },
    /// On the page account; an empty value empties it.
    WritePage { sealed: Vec<u8> },
    /// On the signer's own account.
    WritePrimary { name: String },
}

/// Emitted when a name is claimed. Events are emitted while planning, before
/// the program has checked the record, so they are hints: confirm each against
/// the record. They carry the identifier, never the name.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Registered {
    pub id: NameId,
    pub owner: AccountId,
}

/// Emitted when a name is handed to a new owner. A hint, like [`Registered`].
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Transferred {
    pub id: NameId,
    pub from: AccountId,
    pub to: AccountId,
}

/// Emitted when a name is released. A hint, like [`Registered`].
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Released {
    pub id: NameId,
    pub owner: AccountId,
}

impl Registered {
    pub const SELECTOR_NAME: &str = "logos_names::Registered";
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
        "the {what} account does not match the name"
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

#[must_use]
pub fn plan(input: &PlanInput, instruction: Instruction) -> Plan {
    let mut plan = Plan::new(input);

    match instruction {
        Instruction::Register { id, sealed } => {
            assert!(
                sealed.len() <= MAX_SEALED_RECORD_LEN,
                "sealed profile exceeds {MAX_SEALED_RECORD_LEN} bytes"
            );
            let (record, owner) = record_and_owner(input, &id);
            plan.event(event(
                Registered::SELECTOR_NAME,
                &Registered {
                    id,
                    owner: owner.account_id,
                },
            ));
            plan.effect(
                record,
                &Effect::Register {
                    owner: owner.account_id,
                    sealed,
                },
            );
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
                    signer: owner.account_id,
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
                    signer: owner.account_id,
                },
            );
            plan.effect(page, &Effect::WritePage { sealed });
        }
        Instruction::Transfer { id, new_owner } => {
            let (record, page, owner) = record_page_and_owner(input, &id);
            plan.event(event(
                Transferred::SELECTOR_NAME,
                &Transferred {
                    id,
                    from: owner.account_id,
                    to: new_owner,
                },
            ));
            plan.effect(
                record,
                &Effect::Transfer {
                    signer: owner.account_id,
                    new_owner,
                },
            );
            plan.effect(page, &Effect::WritePage { sealed: Vec::new() });
        }
        Instruction::Release { id } => {
            let (record, page, owner) = record_page_and_owner(input, &id);
            plan.event(event(
                Released::SELECTOR_NAME,
                &Released {
                    id,
                    owner: owner.account_id,
                },
            ));
            plan.effect(
                record,
                &Effect::Remove {
                    signer: owner.account_id,
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
    }
    plan
}

fn to_bytes<T: BorshSerialize>(value: &T) -> Vec<u8> {
    borsh::to_vec(value).expect("borsh serialization is infallible")
}

/// Decodes the record in `pre_data`, refusing unless `signer` owns it.
fn load_owned(pre_data: &[u8], signer: &AccountId) -> NameRecord {
    assert!(!pre_data.is_empty(), "name is not registered");
    let record: NameRecord =
        borsh::from_slice(pre_data).expect("a record shard holds a valid record");
    assert!(record.owner == *signer, "signer does not own this name");
    record
}

/// Applies one effect to the shard it targets. `pre_data` is the shard's
/// current data (empty if unset). Returns the new data, or `None` to leave the
/// shard untouched. Panicking fails the whole transaction.
#[must_use]
pub fn apply(effect: Effect, pre_data: &[u8]) -> Option<Vec<u8>> {
    match effect {
        Effect::Register { owner, sealed } => {
            assert!(pre_data.is_empty(), "name is already registered");
            Some(to_bytes(&NameRecord {
                version: VERSION,
                owner,
                sealed,
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

        fn register(&mut self, id: &NameId, owner: AccountId, sealed: &[u8]) {
            self.run(
                on_record(id, owner),
                Instruction::Register {
                    id: *id,
                    sealed: sealed.to_vec(),
                },
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
                owner: ALICE,
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
            on_record(&ADA, ALICE),
            Instruction::Register {
                id: JEF,
                sealed: Vec::new(),
            },
        );
    }

    #[test]
    #[should_panic(expected = "the owner account must sign")]
    fn registering_needs_the_owners_signature() {
        let mut chain = Chain::default();
        chain.run(
            vec![meta(record_address(&PROGRAM, &JEF), false), meta(ALICE, false)],
            Instruction::Register {
                id: JEF,
                sealed: Vec::new(),
            },
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
        assert_eq!(record.owner, ALICE);
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
                new_owner: BOB,
            },
        );
        assert_eq!(
            chain.record(&JEF),
            Some(NameRecord {
                version: VERSION,
                owner: BOB,
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
                new_owner: BOB,
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
                new_owner: BOB,
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
        assert_eq!(chain.record(&JEF).expect("registered").owner, BOB);
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
            hex(&Instruction::Register { id, sealed: vec![0xEE, 0xFF] }),
            format!("00{id_hex}02000000eeff")
        );
        assert_eq!(
            hex(&Instruction::SetRecord { id, sealed: vec![0xEE] }),
            format!("01{id_hex}01000000ee")
        );
        assert_eq!(
            hex(&Instruction::SetPage { id, sealed: Vec::new() }),
            format!("02{id_hex}00000000")
        );
        assert_eq!(
            hex(&Instruction::Transfer { id, new_owner: BOB }),
            format!("03{id_hex}{}", "02".repeat(32))
        );
        assert_eq!(hex(&Instruction::Release { id }), format!("04{id_hex}"));
        assert_eq!(
            hex(&Instruction::SetPrimary { name: "jef".into() }),
            "05030000006a6566"
        );
    }

    const RECORD_VECTOR: &str = "HqAjYDvn33ACMyTPQ5VELMbfMvQserWSvpPkzCHJPqN9";
    const PAGE_VECTOR: &str = "9UFQXB1QKPHScwFpPzjXUVYGJW3LQCvXHvL7dmSTkaCu";

    #[test]
    fn events_carry_the_identifier_and_never_a_name() {
        let events = run_plan_events(
            on_record(&JEF, ALICE),
            Instruction::Register {
                id: JEF,
                sealed: Vec::new(),
            },
        );
        assert_eq!(events.len(), 1);
        let registered = &events[0];
        assert_eq!(registered.selector, event_selector(Registered::SELECTOR_NAME));
        assert_eq!(
            borsh::from_slice::<Registered>(&registered.data).expect("decodes"),
            Registered { id: JEF, owner: ALICE }
        );

        let events = run_plan_events(
            on_record_and_page(&JEF, ALICE),
            Instruction::Transfer {
                id: JEF,
                new_owner: BOB,
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
}
