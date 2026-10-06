//! Logos Names: a name registry for the Logos Execution Zone (LEZ v0.3).
//!
//! A name is a short human-readable handle that resolves to a wallet (public
//! account and/or private receiving keys), free-form text entries (chat,
//! inbox, agent endpoint, ...) and an optional page.
//!
//! Everything a name owns lives in this program's shard on three kinds of
//! account, all of which a client can address without an indexer:
//!
//! - the name record, on the account derived from the name ([`record_address`]),
//! - the page, on a second derived account ([`page_address`]),
//! - the reverse record, on the user's own account,
//! - the directory, on a fixed derived account ([`directory_address`]).
//!
//! Names form a linked list: the directory holds the newest name and every
//! record holds the name registered before it, so all names can be listed by
//! walking back from the directory with no indexer.
//!
//! `plan` never sees account data, so every ownership check is carried by an
//! effect and enforced in `apply`, where the shard's current data is visible.

use borsh::{BorshDeserialize, BorshSerialize};
use lee_core::{
    account::AccountId,
    program::{AccountMeta, PdaSeed, Plan, PlanInput, ProgramEvent},
};

/// Record layout version written by this program.
pub const VERSION: u8 = 1;
pub const MAX_NAME_LEN: usize = 32;
/// How names are written for people: `jef.lez`. The suffix is a display
/// convention only; the chain stores and derives everything from the bare name.
pub const NAME_SUFFIX: &str = ".lez";
pub const MAX_TEXT_RECORDS: usize = 16;
pub const MAX_TEXT_KEY_LEN: usize = 32;
pub const MAX_TEXT_VALUE_LEN: usize = 512;
/// ML-KEM-768 encapsulation key, as used for LEZ private accounts.
pub const VPK_LEN: usize = 1184;
pub const MAX_CONTENT_TYPE_LEN: usize = 64;
/// LEZ caps shard data at 100 KiB, but a page that large does not fit one
/// transaction under the wallet's default gas limit: 80 KiB applies, 96 KiB fails.
pub const MAX_PAGE_BODY_LEN: usize = 80 * 1024;

const NAME_SEED_DOMAIN: &[u8] = b"logos-names/v1/name/";
const PAGE_SEED_DOMAIN: &[u8] = b"logos-names/v1/page/";
const DIRECTORY_SEED: &[u8] = b"logos-names/v1/directory";

/// One free-form key/value entry on a name (e.g. `chat`, `inbox`, `agent`).
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TextRecord {
    pub key: String,
    pub value: String,
}

/// A registered name.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NameRecord {
    pub version: u8,
    pub name: String,
    /// The account allowed to change or transfer this record.
    pub owner: AccountId,
    /// Public LEZ account this name is an alias for.
    pub account: Option<AccountId>,
    /// Nullifier public key for private transfers to this name.
    pub npk: Option<[u8; 32]>,
    /// Viewing public key for private transfers; empty when unset.
    pub vpk: Vec<u8>,
    pub texts: Vec<TextRecord>,
    /// The name registered just before this one; `None` for the first name.
    pub previous: Option<String>,
}

/// The head of the list of all names.
#[derive(Debug, Clone, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Directory {
    pub version: u8,
    /// How many names have been registered.
    pub count: u64,
    /// The most recently registered name.
    pub latest: Option<String>,
}

/// The page served for a name.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Page {
    pub version: u8,
    pub content_type: String,
    pub body: Vec<u8>,
}

/// Reverse record: the name an account wants to be shown as. It is only a
/// claim; clients must check that the forward record's `owner` or `account`
/// points back at the same account.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PrimaryName {
    pub version: u8,
    pub name: String,
}

/// Every instruction that touches a name takes `[record, owner]` as accounts,
/// with `owner` signing. The exceptions are noted on the variants.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Instruction {
    /// Claim an unregistered name for the signer. `previous` must be the
    /// directory's current latest name, which links the new name into the list;
    /// the registration is refused if another name got in first.
    /// Accounts: `[record, directory, owner]`.
    Register {
        name: String,
        previous: Option<String>,
    },
    /// Point the name at a public account, making it a wallet alias.
    SetAccount { name: String, account: AccountId },
    /// Publish the keys senders need for a private transfer to this name.
    SetPrivateKeys {
        name: String,
        npk: [u8; 32],
        vpk: Vec<u8>,
    },
    /// Remove both the public account and the private keys.
    ClearAddresses { name: String },
    /// Set a text entry; an empty value removes the key.
    SetText {
        name: String,
        key: String,
        value: String,
    },
    /// Hand the name to a new owner. Addresses are cleared so the name never
    /// keeps routing funds to the previous owner; texts and the page stay.
    Transfer { name: String, new_owner: AccountId },
    /// Write the name's page; an empty body removes it.
    /// Accounts: `[record, page, owner]`.
    SetPage {
        name: String,
        content_type: String,
        body: Vec<u8>,
    },
    /// Set the signer's reverse record; an empty name removes it.
    /// Accounts: `[owner]`.
    SetPrimary { name: String },
}

/// A change to an existing record, applied once ownership is confirmed.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Change {
    SetAccount(AccountId),
    SetPrivateKeys { npk: [u8; 32], vpk: Vec<u8> },
    ClearAddresses,
    SetText { key: String, value: String },
    Transfer(AccountId),
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Effect {
    /// On the record: refused unless the shard is empty.
    Register {
        name: String,
        owner: AccountId,
        previous: Option<String>,
    },
    /// On the directory: refused unless its latest name is `expected_latest`.
    Advance {
        expected_latest: Option<String>,
        name: String,
    },
    /// On the record: refused unless `signer` owns `name`.
    Update {
        name: String,
        signer: AccountId,
        change: Change,
    },
    /// On the record: changes nothing, refused unless `signer` owns `name`.
    RequireOwner { name: String, signer: AccountId },
    /// On the page account.
    WritePage(Page),
    /// On the signer's own account.
    WritePrimary { name: String },
}

/// Emitted when a name is claimed. Events are emitted while planning, before the
/// program has checked the record, so a lister must treat them as hints and
/// confirm each name by reading its record.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Registered {
    pub name: String,
    pub owner: AccountId,
}

/// Emitted when a name is handed to a new owner. A hint, like [`Registered`].
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Transferred {
    pub name: String,
    pub from: AccountId,
    pub to: AccountId,
}

impl Registered {
    pub const SELECTOR_NAME: &str = "logos_names::Registered";
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

fn seed(domain: &[u8], name: &str) -> PdaSeed {
    let mut bytes = Vec::with_capacity(domain.len() + name.len());
    bytes.extend_from_slice(domain);
    bytes.extend_from_slice(name.as_bytes());
    PdaSeed::new(sha256(&bytes))
}

/// The account holding the record for `name` under the registry `program`.
#[must_use]
pub fn record_address(program: &AccountId, name: &str) -> AccountId {
    AccountId::for_public_pda(program, &seed(NAME_SEED_DOMAIN, name))
}

/// The account holding the page for `name` under the registry `program`.
#[must_use]
pub fn page_address(program: &AccountId, name: &str) -> AccountId {
    AccountId::for_public_pda(program, &seed(PAGE_SEED_DOMAIN, name))
}

/// The account holding the directory under the registry `program`.
#[must_use]
pub fn directory_address(program: &AccountId) -> AccountId {
    AccountId::for_public_pda(program, &PdaSeed::new(sha256(DIRECTORY_SEED)))
}

/// # Errors
///
/// Returns why `name` is not an acceptable name.
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

/// Turns what a person typed, such as `Jef.lez`, into the bare on-chain name
/// `jef`. The result still has to pass [`validate_name`].
#[must_use]
pub fn normalize_name(input: &str) -> String {
    let lowered = input.trim().to_ascii_lowercase();
    lowered
        .strip_suffix(NAME_SUFFIX)
        .map_or(lowered.clone(), str::to_owned)
}

/// # Errors
///
/// Returns why `key` is not an acceptable text key.
pub fn validate_text_key(key: &str) -> Result<(), &'static str> {
    let charset_ok = key.bytes().all(|b| {
        b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_' || b == b'.'
    });
    if key.is_empty() || key.len() > MAX_TEXT_KEY_LEN || !charset_ok {
        return Err("text key must be 1-32 chars of a-z, 0-9, '-', '_' or '.'");
    }
    Ok(())
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

/// Plans an update to an existing record from the usual `[record, owner]` accounts.
fn plan_update(plan: &mut Plan, input: &PlanInput, name: String, change: Change) {
    let [record, owner] = accounts::<2>(input);
    require_address(
        record,
        record_address(&input.self_account_id, &name),
        "record",
    );
    require_signer(owner);
    plan.effect(
        record,
        &Effect::Update {
            name,
            signer: owner.account_id,
            change,
        },
    );
}

#[must_use]
pub fn plan(input: &PlanInput, instruction: Instruction) -> Plan {
    let mut plan = Plan::new(input);
    let program = input.self_account_id;

    match instruction {
        Instruction::Register { name, previous } => {
            checked(validate_name(&name));
            let [record, directory, owner] = accounts::<3>(input);
            require_address(record, record_address(&program, &name), "record");
            assert!(
                directory.account_id == directory_address(&program),
                "the directory account is not this registry's directory"
            );
            require_signer(owner);
            plan.event(event(
                Registered::SELECTOR_NAME,
                &Registered {
                    name: name.clone(),
                    owner: owner.account_id,
                },
            ));
            plan.effect(
                record,
                &Effect::Register {
                    name: name.clone(),
                    owner: owner.account_id,
                    previous: previous.clone(),
                },
            );
            plan.effect(
                directory,
                &Effect::Advance {
                    expected_latest: previous,
                    name,
                },
            );
        }
        Instruction::SetAccount { name, account } => {
            plan_update(&mut plan, input, name, Change::SetAccount(account));
        }
        Instruction::SetPrivateKeys { name, npk, vpk } => {
            assert!(vpk.len() == VPK_LEN, "vpk must be {VPK_LEN} bytes");
            plan_update(&mut plan, input, name, Change::SetPrivateKeys { npk, vpk });
        }
        Instruction::ClearAddresses { name } => {
            plan_update(&mut plan, input, name, Change::ClearAddresses);
        }
        Instruction::SetText { name, key, value } => {
            checked(validate_text_key(&key));
            assert!(
                value.len() <= MAX_TEXT_VALUE_LEN,
                "text value exceeds {MAX_TEXT_VALUE_LEN} bytes"
            );
            plan_update(&mut plan, input, name, Change::SetText { key, value });
        }
        Instruction::Transfer { name, new_owner } => {
            let [_, owner] = accounts::<2>(input);
            plan.event(event(
                Transferred::SELECTOR_NAME,
                &Transferred {
                    name: name.clone(),
                    from: owner.account_id,
                    to: new_owner,
                },
            ));
            plan_update(&mut plan, input, name, Change::Transfer(new_owner));
        }
        Instruction::SetPage {
            name,
            content_type,
            body,
        } => {
            assert!(
                !content_type.is_empty() && content_type.len() <= MAX_CONTENT_TYPE_LEN,
                "content type must be 1-{MAX_CONTENT_TYPE_LEN} bytes"
            );
            assert!(
                body.len() <= MAX_PAGE_BODY_LEN,
                "page body exceeds {MAX_PAGE_BODY_LEN} bytes"
            );
            let [record, page, owner] = accounts::<3>(input);
            require_address(record, record_address(&program, &name), "record");
            require_address(page, page_address(&program, &name), "page");
            require_signer(owner);
            plan.effect(
                record,
                &Effect::RequireOwner {
                    name,
                    signer: owner.account_id,
                },
            );
            plan.effect(
                page,
                &Effect::WritePage(Page {
                    version: VERSION,
                    content_type,
                    body,
                }),
            );
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

/// Decodes the record in `pre_data`, refusing unless it is `name` and `signer` owns it.
fn load_owned(pre_data: &[u8], name: &str, signer: &AccountId) -> NameRecord {
    assert!(!pre_data.is_empty(), "name is not registered");
    let record: NameRecord =
        borsh::from_slice(pre_data).expect("a record shard holds a valid record");
    assert!(record.name == name, "record does not match the name");
    assert!(record.owner == *signer, "signer does not own this name");
    record
}

#[must_use]
pub fn apply(effect: Effect, pre_data: &[u8]) -> Option<Vec<u8>> {
    match effect {
        Effect::Register {
            name,
            owner,
            previous,
        } => {
            assert!(pre_data.is_empty(), "name is already registered");
            Some(to_bytes(&NameRecord {
                version: VERSION,
                name,
                owner,
                account: None,
                npk: None,
                vpk: Vec::new(),
                texts: Vec::new(),
                previous,
            }))
        }
        Effect::Advance {
            expected_latest,
            name,
        } => {
            let directory: Directory = if pre_data.is_empty() {
                Directory::default()
            } else {
                borsh::from_slice(pre_data).expect("the directory shard holds a valid directory")
            };
            assert!(
                directory.latest == expected_latest,
                "another name was registered first; retry with the current latest name"
            );
            Some(to_bytes(&Directory {
                version: VERSION,
                count: directory.count.checked_add(1).expect("fewer than 2^64 names"),
                latest: Some(name),
            }))
        }
        Effect::Update {
            name,
            signer,
            change,
        } => {
            let mut record = load_owned(pre_data, &name, &signer);
            match change {
                Change::SetAccount(account) => record.account = Some(account),
                Change::SetPrivateKeys { npk, vpk } => {
                    record.npk = Some(npk);
                    record.vpk = vpk;
                }
                Change::ClearAddresses => {
                    record.account = None;
                    record.npk = None;
                    record.vpk = Vec::new();
                }
                Change::SetText { key, value } => {
                    record.texts.retain(|text| text.key != key);
                    if !value.is_empty() {
                        assert!(
                            record.texts.len() < MAX_TEXT_RECORDS,
                            "a name holds at most {MAX_TEXT_RECORDS} text entries"
                        );
                        record.texts.push(TextRecord { key, value });
                    }
                }
                Change::Transfer(new_owner) => {
                    record.owner = new_owner;
                    record.account = None;
                    record.npk = None;
                    record.vpk = Vec::new();
                }
            }
            Some(to_bytes(&record))
        }
        Effect::RequireOwner { name, signer } => {
            load_owned(pre_data, &name, &signer);
            None
        }
        Effect::WritePage(page) => Some(if page.body.is_empty() {
            Vec::new()
        } else {
            to_bytes(&page)
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

    fn meta(account_id: AccountId, is_authorized: bool) -> AccountMeta {
        AccountMeta::new(account_id, is_authorized, PROGRAM)
    }

    fn run_plan(accounts: Vec<AccountMeta>, instruction: Instruction) -> Vec<(AccountId, Effect)> {
        let input = PlanInput {
            self_account_id: PROGRAM,
            caller_account_id: None,
            accounts,
            instruction_data: borsh::to_vec(&instruction).expect("the instruction serializes"),
        };
        plan(&input, instruction)
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
        let input = PlanInput {
            self_account_id: PROGRAM,
            caller_account_id: None,
            accounts,
            instruction_data: borsh::to_vec(&instruction).expect("the instruction serializes"),
        };
        plan(&input, instruction).output().events.clone()
    }

    fn record_accounts(name: &str, signer: AccountId) -> Vec<AccountMeta> {
        vec![
            meta(record_address(&PROGRAM, name), false),
            meta(signer, true),
        ]
    }

    /// Plans `instruction` on `[record, signer]` and applies its single effect to `pre_data`.
    fn execute(name: &str, signer: AccountId, instruction: Instruction, pre_data: &[u8]) -> Vec<u8> {
        let effects = run_plan(record_accounts(name, signer), instruction);
        let [(_, effect)] = <[_; 1]>::try_from(effects).expect("one effect");
        apply(effect, pre_data).expect("the record is rewritten")
    }

    fn register_accounts(name: &str, signer: AccountId) -> Vec<AccountMeta> {
        vec![
            meta(record_address(&PROGRAM, name), false),
            meta(directory_address(&PROGRAM), false),
            meta(signer, true),
        ]
    }

    /// Plans a registration, returning its `(record, directory)` effects.
    fn register_effects(name: &str, owner: AccountId, previous: Option<&str>) -> (Effect, Effect) {
        let effects = run_plan(
            register_accounts(name, owner),
            Instruction::Register {
                name: name.into(),
                previous: previous.map(Into::into),
            },
        );
        let [(record_id, record), (directory_id, directory)] =
            <[_; 2]>::try_from(effects).expect("two effects");
        assert_eq!(record_id, record_address(&PROGRAM, name));
        assert_eq!(directory_id, directory_address(&PROGRAM));
        (record, directory)
    }

    fn registered(name: &str, owner: AccountId) -> Vec<u8> {
        let (record, _) = register_effects(name, owner, None);
        apply(record, &[]).expect("the record is written")
    }

    fn decode(bytes: &[u8]) -> NameRecord {
        borsh::from_slice(bytes).expect("a record decodes")
    }

    #[test]
    fn names_are_restricted_to_a_safe_charset() {
        for good in ["a", "jef", "jef-2", "0x", &"a".repeat(32)] {
            assert!(validate_name(good).is_ok(), "{good} should be valid");
        }
        for bad in ["", "Jef", "jef.eth", "-jef", "jef-", "j f", "jéf", &"a".repeat(33)] {
            assert!(validate_name(bad).is_err(), "{bad} should be invalid");
        }
    }

    #[test]
    fn typed_names_are_normalised_to_the_bare_name() {
        for typed in ["jef", "jef.lez", "Jef.LEZ", "  jef.lez  "] {
            assert_eq!(normalize_name(typed), "jef", "{typed}");
        }
        // Only one suffix is stripped, and other dots are left for validation to refuse.
        assert_eq!(normalize_name("jef.lez.lez"), "jef.lez");
        assert!(validate_name(&normalize_name("jef.eth")).is_err());
        assert!(validate_name(&normalize_name(".lez")).is_err());
    }

    #[test]
    fn record_and_page_addresses_differ_per_name_and_kind() {
        assert_ne!(record_address(&PROGRAM, "jef"), record_address(&PROGRAM, "jeff"));
        assert_ne!(record_address(&PROGRAM, "jef"), page_address(&PROGRAM, "jef"));
        assert_ne!(record_address(&PROGRAM, "jef"), record_address(&ALICE, "jef"));
    }

    /// Vectors produced by `resolver/names.js`; both sides must derive the same accounts.
    #[test]
    fn addresses_match_the_javascript_resolver() {
        assert_eq!(
            record_address(&PROGRAM, "jef").to_string(),
            "28mUmKUyH1dJgmwdjVxS22P12iUHVotLcvk2nbrFw29Y"
        );
        assert_eq!(
            page_address(&PROGRAM, "jef").to_string(),
            "6ckRpxviqmmhY6QkcJPVPWYSSy3eRZGzy1uUGStf1TnC"
        );
        assert_eq!(
            directory_address(&PROGRAM).to_string(),
            "9oQ7KANHqw4ok3goEHzgeK7WSxdKqKNXA7bS9HHpq97p"
        );
    }

    #[test]
    fn registering_writes_a_record_owned_by_the_signer() {
        let record = decode(&registered("jef", ALICE));
        assert_eq!(record.name, "jef");
        assert_eq!(record.owner, ALICE);
        assert_eq!(record.account, None);
        assert!(record.texts.is_empty());
    }

    #[test]
    #[should_panic(expected = "name is already registered")]
    fn a_taken_name_cannot_be_registered_again() {
        let taken = registered("jef", ALICE);
        let (record, _) = register_effects("jef", BOB, None);
        let _ = apply(record, &taken);
    }

    #[test]
    #[should_panic(expected = "the owner account must sign")]
    fn registering_needs_the_owner_signature() {
        run_plan(
            vec![
                meta(record_address(&PROGRAM, "jef"), false),
                meta(directory_address(&PROGRAM), false),
                meta(ALICE, false),
            ],
            Instruction::Register {
                name: "jef".into(),
                previous: None,
            },
        );
    }

    #[test]
    #[should_panic(expected = "the record account does not match the name")]
    fn a_record_cannot_be_written_to_another_names_account() {
        run_plan(
            register_accounts("someone-else", ALICE),
            Instruction::Register {
                name: "jef".into(),
                previous: None,
            },
        );
    }

    #[test]
    #[should_panic(expected = "signer does not own this name")]
    fn only_the_owner_can_change_a_record() {
        let record = registered("jef", ALICE);
        execute(
            "jef",
            BOB,
            Instruction::SetAccount {
                name: "jef".into(),
                account: BOB,
            },
            &record,
        );
    }

    #[test]
    #[should_panic(expected = "name is not registered")]
    fn an_unregistered_name_cannot_be_changed() {
        execute(
            "jef",
            ALICE,
            Instruction::SetAccount {
                name: "jef".into(),
                account: ALICE,
            },
            &[],
        );
    }

    #[test]
    fn a_name_can_alias_a_public_account_and_private_keys() {
        let record = registered("jef", ALICE);
        let record = execute(
            "jef",
            ALICE,
            Instruction::SetAccount {
                name: "jef".into(),
                account: BOB,
            },
            &record,
        );
        let record = execute(
            "jef",
            ALICE,
            Instruction::SetPrivateKeys {
                name: "jef".into(),
                npk: [9; 32],
                vpk: vec![5; VPK_LEN],
            },
            &record,
        );
        let decoded = decode(&record);
        assert_eq!(decoded.account, Some(BOB));
        assert_eq!(decoded.npk, Some([9; 32]));
        assert_eq!(decoded.vpk.len(), VPK_LEN);

        let cleared = decode(&execute(
            "jef",
            ALICE,
            Instruction::ClearAddresses { name: "jef".into() },
            &record,
        ));
        assert_eq!((cleared.account, cleared.npk, cleared.vpk.len()), (None, None, 0));
    }

    #[test]
    #[should_panic(expected = "vpk must be 1184 bytes")]
    fn a_viewing_key_of_the_wrong_size_is_refused() {
        run_plan(
            record_accounts("jef", ALICE),
            Instruction::SetPrivateKeys {
                name: "jef".into(),
                npk: [9; 32],
                vpk: vec![5; 32],
            },
        );
    }

    fn set_text(record: &[u8], key: &str, value: &str) -> Vec<u8> {
        execute(
            "jef",
            ALICE,
            Instruction::SetText {
                name: "jef".into(),
                key: key.into(),
                value: value.into(),
            },
            record,
        )
    }

    #[test]
    fn text_entries_are_set_replaced_and_removed() {
        let record = registered("jef", ALICE);
        let record = set_text(&record, "chat", "one");
        let record = set_text(&record, "agent", "https://example.test/agent");
        let record = set_text(&record, "chat", "two");
        let texts = decode(&record).texts;
        assert_eq!(texts.len(), 2);
        assert!(texts.contains(&TextRecord {
            key: "chat".into(),
            value: "two".into()
        }));

        let record = set_text(&record, "chat", "");
        assert_eq!(decode(&record).texts.len(), 1);
    }

    #[test]
    #[should_panic(expected = "a name holds at most 16 text entries")]
    fn text_entries_are_capped() {
        let mut record = registered("jef", ALICE);
        for index in 0..=MAX_TEXT_RECORDS {
            record = set_text(&record, &format!("key{index}"), "value");
        }
    }

    #[test]
    fn transferring_changes_the_owner_and_clears_addresses() {
        let record = registered("jef", ALICE);
        let record = execute(
            "jef",
            ALICE,
            Instruction::SetAccount {
                name: "jef".into(),
                account: ALICE,
            },
            &record,
        );
        let record = set_text(&record, "chat", "kept");
        let moved = execute(
            "jef",
            ALICE,
            Instruction::Transfer {
                name: "jef".into(),
                new_owner: BOB,
            },
            &record,
        );
        let decoded = decode(&moved);
        assert_eq!(decoded.owner, BOB);
        assert_eq!(decoded.account, None);
        assert_eq!(decoded.texts.len(), 1);

        // The new owner can now act; the previous owner cannot.
        execute("jef", BOB, Instruction::ClearAddresses { name: "jef".into() }, &moved);
    }

    fn decode_directory(bytes: &[u8]) -> Directory {
        borsh::from_slice(bytes).expect("a directory decodes")
    }

    #[test]
    fn registrations_link_names_into_a_list() {
        let (first_record, first_advance) = register_effects("jef", ALICE, None);
        let directory = apply(first_advance, &[]).expect("the directory is written");
        assert_eq!(
            decode_directory(&directory),
            Directory {
                version: VERSION,
                count: 1,
                latest: Some("jef".into())
            }
        );
        assert_eq!(decode(&apply(first_record, &[]).expect("written")).previous, None);

        let (second_record, second_advance) = register_effects("ada", BOB, Some("jef"));
        let directory = apply(second_advance, &directory).expect("the directory is written");
        assert_eq!(decode_directory(&directory).count, 2);
        assert_eq!(decode_directory(&directory).latest.as_deref(), Some("ada"));
        assert_eq!(
            decode(&apply(second_record, &[]).expect("written")).previous.as_deref(),
            Some("jef")
        );
    }

    #[test]
    #[should_panic(expected = "another name was registered first")]
    fn a_registration_built_on_a_stale_list_is_refused() {
        let (_, first_advance) = register_effects("jef", ALICE, None);
        let directory = apply(first_advance, &[]).expect("the directory is written");
        // Bob still believes the list is empty.
        let (_, stale_advance) = register_effects("ada", BOB, None);
        let _ = apply(stale_advance, &directory);
    }

    #[test]
    #[should_panic(expected = "the directory account is not this registry's directory")]
    fn a_registration_must_name_the_real_directory() {
        run_plan(
            vec![
                meta(record_address(&PROGRAM, "jef"), false),
                meta(BOB, false),
                meta(ALICE, true),
            ],
            Instruction::Register {
                name: "jef".into(),
                previous: None,
            },
        );
    }

    #[test]
    fn registering_and_transferring_emit_events() {
        let events = run_plan_events(
            register_accounts("jef", ALICE),
            Instruction::Register {
                name: "jef".into(),
                previous: None,
            },
        );
        assert_eq!(events.len(), 1);
        let registered = &events[0];
        assert_eq!(registered.selector, event_selector("logos_names::Registered"));
        assert_eq!(
            borsh::from_slice::<Registered>(&registered.data).expect("decodes"),
            Registered {
                name: "jef".into(),
                owner: ALICE
            }
        );

        let events = run_plan_events(
            record_accounts("jef", ALICE),
            Instruction::Transfer {
                name: "jef".into(),
                new_owner: BOB,
            },
        );
        assert_eq!(events.len(), 1);
        let transferred = &events[0];
        assert_eq!(transferred.selector, event_selector("logos_names::Transferred"));
        assert_eq!(
            borsh::from_slice::<Transferred>(&transferred.data).expect("decodes"),
            Transferred {
                name: "jef".into(),
                from: ALICE,
                to: BOB
            }
        );
    }

    #[test]
    fn other_instructions_emit_no_events() {
        let events = run_plan_events(
            record_accounts("jef", ALICE),
            Instruction::SetText {
                name: "jef".into(),
                key: "chat".into(),
                value: "x".into(),
            },
        );
        assert!(events.is_empty());
    }

    fn page_effects(signer: AccountId, body: &[u8]) -> Vec<(AccountId, Effect)> {
        run_plan(
            vec![
                meta(record_address(&PROGRAM, "jef"), false),
                meta(page_address(&PROGRAM, "jef"), false),
                meta(signer, true),
            ],
            Instruction::SetPage {
                name: "jef".into(),
                content_type: "text/html".into(),
                body: body.to_vec(),
            },
        )
    }

    #[test]
    fn a_page_is_written_only_alongside_an_ownership_check() {
        let record = registered("jef", ALICE);
        let effects = page_effects(ALICE, b"<h1>hi</h1>");
        let [(record_id, guard), (page_id, write)] = <[_; 2]>::try_from(effects).expect("two effects");
        assert_eq!(record_id, record_address(&PROGRAM, "jef"));
        assert_eq!(page_id, page_address(&PROGRAM, "jef"));

        assert_eq!(apply(guard, &record), None, "the record is left untouched");
        let page: Page =
            borsh::from_slice(&apply(write, &[]).expect("the page is written")).expect("a page decodes");
        assert_eq!(page.content_type, "text/html");
        assert_eq!(page.body, b"<h1>hi</h1>");
    }

    #[test]
    #[should_panic(expected = "signer does not own this name")]
    fn someone_else_cannot_write_a_names_page() {
        let record = registered("jef", ALICE);
        let (_, guard) = page_effects(BOB, b"defaced").swap_remove(0);
        let _ = apply(guard, &record);
    }

    #[test]
    fn an_empty_page_body_removes_the_page() {
        let (_, write) = page_effects(ALICE, b"").swap_remove(1);
        assert_eq!(apply(write, b"old page bytes"), Some(Vec::new()));
    }

    #[test]
    fn the_reverse_record_lives_on_the_signers_own_account() {
        let effects = run_plan(
            vec![meta(ALICE, true)],
            Instruction::SetPrimary { name: "jef".into() },
        );
        let [(account, effect)] = <[_; 1]>::try_from(effects).expect("one effect");
        assert_eq!(account, ALICE);
        let primary: PrimaryName =
            borsh::from_slice(&apply(effect, &[]).expect("written")).expect("decodes");
        assert_eq!(primary.name, "jef");

        let (_, clear) = run_plan(
            vec![meta(ALICE, true)],
            Instruction::SetPrimary { name: String::new() },
        )
        .swap_remove(0);
        assert_eq!(apply(clear, b"anything"), Some(Vec::new()));
    }
}
