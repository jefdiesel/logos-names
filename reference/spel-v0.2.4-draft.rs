#![no_main]

use nssa_core::account::Data;
use spel_framework::prelude::*;

risc0_zkvm::guest::entry!(main);

/// Record layout version written by this program.
pub const VERSION: u8 = 1;
/// Names are used directly as a 32-byte PDA seed, so 32 bytes is a hard cap.
pub const MAX_NAME_LEN: usize = 32;
pub const MAX_TEXT_RECORDS: usize = 16;
pub const MAX_TEXT_KEY_LEN: usize = 32;
pub const MAX_TEXT_VALUE_LEN: usize = 512;
/// ML-KEM-768 encapsulation key, as used for LEZ private accounts.
pub const VPK_LEN: usize = 1184;
pub const MAX_CONTENT_TYPE_LEN: usize = 64;
/// Account data is capped at 100 KiB by LEZ; leave headroom for the header.
pub const MAX_PAGE_BODY_LEN: usize = 96 * 1024;

pub const ERR_INVALID_NAME: u32 = 100;
pub const ERR_NOT_OWNER: u32 = 101;
pub const ERR_BAD_RECORD: u32 = 102;
pub const ERR_INVALID_TEXT: u32 = 103;
pub const ERR_TOO_MANY_TEXTS: u32 = 104;
pub const ERR_INVALID_VPK: u32 = 105;
pub const ERR_INVALID_PAGE: u32 = 106;
pub const ERR_TOO_BIG: u32 = 107;

#[lez_program]
mod logos_names {
    #[allow(unused_imports)]
    use super::*;

    /// One free-form key/value entry on a name (e.g. `chat`, `inbox`, `agent`).
    #[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    pub struct TextRecord {
        pub key: String,
        pub value: String,
    }

    /// A registered name. Lives at PDA `["name", name]`, so any client can
    /// derive its address from the name alone and read it with one `getAccount`.
    #[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    #[account_type]
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
    }

    /// The page served for a name. Lives at PDA `["page", name]`.
    #[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    #[account_type]
    pub struct Page {
        pub version: u8,
        pub content_type: String,
        pub body: Vec<u8>,
    }

    /// Reverse record: the name an account wants to be shown as. Lives at PDA
    /// `["primary", account]`. It is only a claim; clients must check that the
    /// forward record's `owner` or `account` points back at the same account.
    #[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    #[account_type]
    pub struct PrimaryName {
        pub version: u8,
        pub name: String,
    }

    fn err(code: u32, message: &str) -> SpelError {
        SpelError::custom(code, message)
    }

    fn validate_name(name: &str) -> Result<(), SpelError> {
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
            return Err(err(
                ERR_INVALID_NAME,
                "name must be 1-32 chars of a-z, 0-9 or '-', not starting or ending with '-'",
            ));
        }
        Ok(())
    }

    fn validate_text_key(key: &str) -> Result<(), SpelError> {
        let charset_ok = key.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_' || b == b'.'
        });
        if key.is_empty() || key.len() > MAX_TEXT_KEY_LEN || !charset_ok {
            return Err(err(
                ERR_INVALID_TEXT,
                "text key must be 1-32 chars of a-z, 0-9, '-', '_' or '.'",
            ));
        }
        Ok(())
    }

    fn store<T: BorshSerialize>(target: &mut AccountWithMetadata, value: &T) -> Result<(), SpelError> {
        let bytes = borsh::to_vec(value).map_err(|e| err(ERR_BAD_RECORD, &format!("borsh: {e}")))?;
        target.account.data =
            Data::try_from(bytes).map_err(|_| err(ERR_TOO_BIG, "account data exceeds 100 KiB"))?;
        Ok(())
    }

    /// Decode the name record and check it is the one for `name`.
    fn load_record(record: &AccountWithMetadata, name: &str) -> Result<NameRecord, SpelError> {
        let parsed = NameRecord::try_from_slice(&record.account.data)
            .map_err(|_| err(ERR_BAD_RECORD, "name is not registered"))?;
        if parsed.name != name {
            return Err(err(ERR_BAD_RECORD, "record does not match name"));
        }
        Ok(parsed)
    }

    fn load_owned(
        record: &AccountWithMetadata,
        name: &str,
        owner: &AccountWithMetadata,
    ) -> Result<NameRecord, SpelError> {
        let parsed = load_record(record, name)?;
        if parsed.owner != owner.account_id {
            return Err(err(ERR_NOT_OWNER, "signer does not own this name"));
        }
        Ok(parsed)
    }

    fn build_page(content_type: String, body: Vec<u8>) -> Result<Page, SpelError> {
        if content_type.is_empty() || content_type.len() > MAX_CONTENT_TYPE_LEN {
            return Err(err(ERR_INVALID_PAGE, "content type must be 1-64 bytes"));
        }
        if body.len() > MAX_PAGE_BODY_LEN {
            return Err(err(ERR_INVALID_PAGE, "page body exceeds 96 KiB"));
        }
        Ok(Page {
            version: VERSION,
            content_type,
            body,
        })
    }

    /// Claim an unregistered name for the signer.
    #[instruction]
    pub fn register(
        #[account(init, pda = [literal("name"), arg("name")])]
        mut record: AccountWithMetadata,
        #[account(signer)]
        owner: AccountWithMetadata,
        name: String,
    ) -> SpelResult {
        validate_name(&name)?;
        let new_record = NameRecord {
            version: VERSION,
            name,
            owner: owner.account_id,
            account: None,
            npk: None,
            vpk: Vec::new(),
            texts: Vec::new(),
        };
        store(&mut record, &new_record)?;
        Ok(SpelOutput::execute(vec![record, owner], vec![]))
    }

    /// Point the name at a public account, making it a wallet alias.
    #[instruction]
    pub fn set_account(
        #[account(mut, pda = [literal("name"), arg("name")])]
        mut record: AccountWithMetadata,
        #[account(signer)]
        owner: AccountWithMetadata,
        name: String,
        account: AccountId,
    ) -> SpelResult {
        let mut parsed = load_owned(&record, &name, &owner)?;
        parsed.account = Some(account);
        store(&mut record, &parsed)?;
        Ok(SpelOutput::execute(vec![record, owner], vec![]))
    }

    /// Publish the keys senders need for a private transfer to this name.
    #[instruction]
    pub fn set_private_keys(
        #[account(mut, pda = [literal("name"), arg("name")])]
        mut record: AccountWithMetadata,
        #[account(signer)]
        owner: AccountWithMetadata,
        name: String,
        npk: [u8; 32],
        vpk: Vec<u8>,
    ) -> SpelResult {
        let mut parsed = load_owned(&record, &name, &owner)?;
        if vpk.len() != VPK_LEN {
            return Err(err(ERR_INVALID_VPK, "vpk must be 1184 bytes"));
        }
        parsed.npk = Some(npk);
        parsed.vpk = vpk;
        store(&mut record, &parsed)?;
        Ok(SpelOutput::execute(vec![record, owner], vec![]))
    }

    /// Remove both the public account and the private keys from the name.
    #[instruction]
    pub fn clear_addresses(
        #[account(mut, pda = [literal("name"), arg("name")])]
        mut record: AccountWithMetadata,
        #[account(signer)]
        owner: AccountWithMetadata,
        name: String,
    ) -> SpelResult {
        let mut parsed = load_owned(&record, &name, &owner)?;
        parsed.account = None;
        parsed.npk = None;
        parsed.vpk = Vec::new();
        store(&mut record, &parsed)?;
        Ok(SpelOutput::execute(vec![record, owner], vec![]))
    }

    /// Set a text entry; an empty value removes the key.
    #[instruction]
    pub fn set_text(
        #[account(mut, pda = [literal("name"), arg("name")])]
        mut record: AccountWithMetadata,
        #[account(signer)]
        owner: AccountWithMetadata,
        name: String,
        key: String,
        value: String,
    ) -> SpelResult {
        let mut parsed = load_owned(&record, &name, &owner)?;
        validate_text_key(&key)?;
        if value.len() > MAX_TEXT_VALUE_LEN {
            return Err(err(ERR_INVALID_TEXT, "text value exceeds 512 bytes"));
        }
        parsed.texts.retain(|t| t.key != key);
        if !value.is_empty() {
            if parsed.texts.len() >= MAX_TEXT_RECORDS {
                return Err(err(ERR_TOO_MANY_TEXTS, "a name holds at most 16 text entries"));
            }
            parsed.texts.push(TextRecord { key, value });
        }
        store(&mut record, &parsed)?;
        Ok(SpelOutput::execute(vec![record, owner], vec![]))
    }

    /// Hand the name to a new owner. Addresses are cleared so the name never
    /// keeps routing funds to the previous owner; text entries and the page stay.
    #[instruction]
    pub fn transfer(
        #[account(mut, pda = [literal("name"), arg("name")])]
        mut record: AccountWithMetadata,
        #[account(signer)]
        owner: AccountWithMetadata,
        name: String,
        new_owner: AccountId,
    ) -> SpelResult {
        let mut parsed = load_owned(&record, &name, &owner)?;
        parsed.owner = new_owner;
        parsed.account = None;
        parsed.npk = None;
        parsed.vpk = Vec::new();
        store(&mut record, &parsed)?;
        Ok(SpelOutput::execute(vec![record, owner], vec![]))
    }

    /// Publish the name's page for the first time.
    #[instruction]
    pub fn publish_page(
        #[account(pda = [literal("name"), arg("name")])]
        record: AccountWithMetadata,
        #[account(init, pda = [literal("page"), arg("name")])]
        mut page: AccountWithMetadata,
        #[account(signer)]
        owner: AccountWithMetadata,
        name: String,
        content_type: String,
        body: Vec<u8>,
    ) -> SpelResult {
        load_owned(&record, &name, &owner)?;
        store(&mut page, &build_page(content_type, body)?)?;
        Ok(SpelOutput::execute(vec![record, page, owner], vec![]))
    }

    /// Replace the name's existing page.
    #[instruction]
    pub fn update_page(
        #[account(pda = [literal("name"), arg("name")])]
        record: AccountWithMetadata,
        #[account(mut, pda = [literal("page"), arg("name")])]
        mut page: AccountWithMetadata,
        #[account(signer)]
        owner: AccountWithMetadata,
        name: String,
        content_type: String,
        body: Vec<u8>,
    ) -> SpelResult {
        load_owned(&record, &name, &owner)?;
        store(&mut page, &build_page(content_type, body)?)?;
        Ok(SpelOutput::execute(vec![record, page, owner], vec![]))
    }

    /// Set the signer's reverse record for the first time.
    #[instruction]
    pub fn set_primary(
        #[account(init, pda = [literal("primary"), account("owner")])]
        mut primary: AccountWithMetadata,
        #[account(signer)]
        owner: AccountWithMetadata,
        name: String,
    ) -> SpelResult {
        validate_name(&name)?;
        store(&mut primary, &PrimaryName { version: VERSION, name })?;
        Ok(SpelOutput::execute(vec![primary, owner], vec![]))
    }

    /// Change the signer's existing reverse record.
    #[instruction]
    pub fn change_primary(
        #[account(mut, pda = [literal("primary"), account("owner")])]
        mut primary: AccountWithMetadata,
        #[account(signer)]
        owner: AccountWithMetadata,
        name: String,
    ) -> SpelResult {
        validate_name(&name)?;
        store(&mut primary, &PrimaryName { version: VERSION, name })?;
        Ok(SpelOutput::execute(vec![primary, owner], vec![]))
    }
}
