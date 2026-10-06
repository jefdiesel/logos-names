//! Builds the transactions that change a name, following SPEC.md section 6.
//!
//! Each function reads what it needs from the registry, checks what the program
//! will check, and returns a [`Change`] for a wallet to sign. Checking here
//! first turns a refused transaction into an explanation: LEZ includes failed
//! transactions in blocks without saying why they failed.

use crate::registry::{
    account_text, directory_address, is_valid_name, page_address, record_address, AccountId,
    NameRecord, Registry, NAME_SUFFIX,
};

const MAX_TEXT_RECORDS: usize = 16;
const MAX_TEXT_KEY_LEN: usize = 32;
const MAX_TEXT_VALUE_LEN: usize = 512;
const MAX_CONTENT_TYPE_LEN: usize = 64;
const MAX_PAGE_BODY_LEN: usize = 80 * 1024;
const VPK_LEN: usize = 1184;

/// One instruction for the registry, ready to sign and send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// The borsh-encoded instruction.
    pub instruction: Vec<u8>,
    /// The accounts the instruction runs over, in the order the program expects.
    pub accounts: Vec<AccountId>,
    /// The one account that signs and pays. It is always among `accounts`.
    pub signer: AccountId,
}

/// Minimal borsh writer for the registry's instructions.
struct Writer(Vec<u8>);

impl Writer {
    fn instruction(index: u8, name: &str) -> Self {
        let mut writer = Self(vec![index]);
        writer.string(name);
        writer
    }

    fn bytes(&mut self, bytes: &[u8]) {
        self.0.extend((bytes.len() as u32).to_le_bytes());
        self.0.extend(bytes);
    }

    fn string(&mut self, text: &str) {
        self.bytes(text.as_bytes());
    }

    fn account(&mut self, account: &AccountId) {
        self.0.extend(account);
    }

    fn optional_name(&mut self, name: Option<&str>) {
        match name {
            None => self.0.push(0),
            Some(name) => {
                self.0.push(1);
                self.string(name);
            }
        }
    }
}

pub fn encode_register(name: &str, previous: Option<&str>) -> Vec<u8> {
    let mut writer = Writer::instruction(0, name);
    writer.optional_name(previous);
    writer.0
}

pub fn encode_set_account(name: &str, account: &AccountId) -> Vec<u8> {
    let mut writer = Writer::instruction(1, name);
    writer.account(account);
    writer.0
}

pub fn encode_set_private_keys(name: &str, npk: &[u8; 32], vpk: &[u8]) -> Vec<u8> {
    let mut writer = Writer::instruction(2, name);
    writer.account(npk);
    writer.bytes(vpk);
    writer.0
}

pub fn encode_clear_addresses(name: &str) -> Vec<u8> {
    Writer::instruction(3, name).0
}

pub fn encode_set_text(name: &str, key: &str, value: &str) -> Vec<u8> {
    let mut writer = Writer::instruction(4, name);
    writer.string(key);
    writer.string(value);
    writer.0
}

pub fn encode_transfer(name: &str, new_owner: &AccountId) -> Vec<u8> {
    let mut writer = Writer::instruction(5, name);
    writer.account(new_owner);
    writer.0
}

pub fn encode_set_page(name: &str, content_type: &str, body: &[u8]) -> Vec<u8> {
    let mut writer = Writer::instruction(6, name);
    writer.string(content_type);
    writer.bytes(body);
    writer.0
}

pub fn encode_set_primary(name: &str) -> Vec<u8> {
    Writer::instruction(7, name).0
}

pub fn encode_release(name: &str, previous: Option<&str>, next: Option<&str>) -> Vec<u8> {
    let mut writer = Writer::instruction(8, name);
    writer.optional_name(previous);
    writer.optional_name(next);
    writer.0
}

fn is_valid_text_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= MAX_TEXT_KEY_LEN
        && key
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_' | b'.'))
}

fn display(name: &str) -> String {
    format!("{name}{NAME_SUFFIX}")
}

impl Registry {
    /// The record of a registered name, for a change only its owner can make.
    fn owned(&self, name: &str) -> Result<NameRecord, String> {
        self.resolve(name)?
            .ok_or_else(|| format!("{} is not registered", display(name)))
    }

    /// A change over the usual `[record, owner]` accounts.
    fn on_record(&self, name: &str, instruction: Vec<u8>) -> Result<Change, String> {
        let record = self.owned(name)?;
        Ok(Change {
            instruction,
            accounts: vec![record_address(&self.program, name), record.owner],
            signer: record.owner,
        })
    }

    /// Registers `name` to `owner`, linking it after the newest name.
    pub fn register(&self, name: &str, owner: &AccountId) -> Result<Change, String> {
        if self.resolve(name)?.is_some() {
            return Err(format!("{} is already registered", display(name)));
        }
        let (_count, latest) = self.directory()?;
        let mut accounts = vec![
            record_address(&self.program, name),
            directory_address(&self.program),
        ];
        if let Some(previous) = &latest {
            accounts.push(record_address(&self.program, previous));
        }
        accounts.push(*owner);
        Ok(Change {
            instruction: encode_register(name, latest.as_deref()),
            accounts,
            signer: *owner,
        })
    }

    pub fn set_account(&self, name: &str, account: &AccountId) -> Result<Change, String> {
        self.on_record(name, encode_set_account(name, account))
    }

    pub fn set_private_keys(&self, name: &str, npk: &[u8], vpk: &[u8]) -> Result<Change, String> {
        let npk: &[u8; 32] = npk
            .try_into()
            .map_err(|_| "the nullifier public key must be 32 bytes".to_owned())?;
        if vpk.len() != VPK_LEN {
            return Err(format!("the viewing public key must be {VPK_LEN} bytes"));
        }
        self.on_record(name, encode_set_private_keys(name, npk, vpk))
    }

    pub fn clear_addresses(&self, name: &str) -> Result<Change, String> {
        self.on_record(name, encode_clear_addresses(name))
    }

    /// Sets a text entry; an empty value removes the key.
    pub fn set_text(&self, name: &str, key: &str, value: &str) -> Result<Change, String> {
        if !is_valid_text_key(key) {
            return Err("an entry's key is 1-32 characters of a-z, 0-9, '-', '_' or '.'".to_owned());
        }
        if value.len() > MAX_TEXT_VALUE_LEN {
            return Err(format!("an entry's value is at most {MAX_TEXT_VALUE_LEN} bytes"));
        }
        let record = self.owned(name)?;
        let exists = record.texts.iter().any(|(existing, _)| existing == key);
        if value.is_empty() && !exists {
            return Err(format!("{} has no entry called {key}", display(name)));
        }
        if !value.is_empty() && !exists && record.texts.len() >= MAX_TEXT_RECORDS {
            return Err(format!("a name holds at most {MAX_TEXT_RECORDS} entries"));
        }
        Ok(Change {
            instruction: encode_set_text(name, key, value),
            accounts: vec![record_address(&self.program, name), record.owner],
            signer: record.owner,
        })
    }

    /// Writes the name's page; an empty body removes it.
    pub fn set_page(&self, name: &str, content_type: &str, body: &[u8]) -> Result<Change, String> {
        if content_type.is_empty() || content_type.len() > MAX_CONTENT_TYPE_LEN {
            return Err(format!("the content type is 1-{MAX_CONTENT_TYPE_LEN} bytes"));
        }
        if body.len() > MAX_PAGE_BODY_LEN {
            return Err(format!("a page is at most {MAX_PAGE_BODY_LEN} bytes"));
        }
        let record = self.owned(name)?;
        Ok(Change {
            instruction: encode_set_page(name, content_type, body),
            accounts: vec![
                record_address(&self.program, name),
                page_address(&self.program, name),
                record.owner,
            ],
            signer: record.owner,
        })
    }

    /// Hands the name to `new_owner`. The program clears everything the
    /// current owner set, so the page account is passed too.
    pub fn transfer(&self, name: &str, new_owner: &AccountId) -> Result<Change, String> {
        let record = self.owned(name)?;
        if record.owner == *new_owner {
            return Err(format!("{} already belongs to that account", display(name)));
        }
        Ok(Change {
            instruction: encode_transfer(name, new_owner),
            accounts: vec![
                record_address(&self.program, name),
                page_address(&self.program, name),
                record.owner,
            ],
            signer: record.owner,
        })
    }

    /// Gives the name up, unlinking it from its neighbours in the list.
    pub fn release(&self, name: &str) -> Result<Change, String> {
        let record = self.owned(name)?;
        let mut accounts = vec![
            record_address(&self.program, name),
            page_address(&self.program, name),
            directory_address(&self.program),
            record.owner,
        ];
        for neighbour in [&record.previous, &record.next].into_iter().flatten() {
            accounts.push(record_address(&self.program, neighbour));
        }
        Ok(Change {
            instruction: encode_release(name, record.previous.as_deref(), record.next.as_deref()),
            accounts,
            signer: record.owner,
        })
    }

    /// Sets the name `account` is shown as; an empty name removes it. A reverse
    /// record counts only when the name points back, so one that would not is
    /// refused here rather than written and ignored.
    pub fn set_primary(&self, account: &AccountId, name: &str) -> Result<Change, String> {
        if !name.is_empty() {
            if !is_valid_name(name) {
                return Err("not a valid name".to_owned());
            }
            let record = self.owned(name)?;
            if record.owner != *account && record.account != Some(*account) {
                return Err(format!(
                    "{} does not point at {}",
                    display(name),
                    account_text(account)
                ));
            }
        }
        Ok(Change {
            instruction: encode_set_primary(name),
            accounts: vec![*account],
            signer: *account,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::hex;

    /// The vectors in SPEC.md, shared with the Rust core's own encoding test.
    #[test]
    fn instructions_encode_as_the_specification_says() {
        assert_eq!(hex(&encode_register("jef", None)), "00030000006a656600");
        assert_eq!(
            hex(&encode_register("ada", Some("jef"))),
            "000300000061646101030000006a6566"
        );
        assert_eq!(
            hex(&encode_set_text("jef", "chat", "hi")),
            "04030000006a6566040000006368617402000000".to_owned() + "6869"
        );
        assert_eq!(
            hex(&encode_transfer("jef", &[2; 32])),
            "05030000006a6566".to_owned() + &"02".repeat(32)
        );
        assert_eq!(
            hex(&encode_release("ada", Some("jef"), None)),
            "080300000061646101030000006a656600"
        );
    }

    #[test]
    fn the_remaining_instructions_follow_the_same_layout() {
        assert_eq!(
            hex(&encode_set_account("jef", &[2; 32])),
            "01030000006a6566".to_owned() + &"02".repeat(32)
        );
        assert_eq!(
            hex(&encode_set_private_keys("jef", &[3; 32], &[9, 9])),
            "02030000006a6566".to_owned() + &"03".repeat(32) + "020000000909"
        );
        assert_eq!(hex(&encode_clear_addresses("jef")), "03030000006a6566");
        assert_eq!(
            hex(&encode_set_page("jef", "text/html", b"hi")),
            "06030000006a656609000000746578742f68746d6c020000006869"
        );
        assert_eq!(hex(&encode_set_primary("jef")), "07030000006a6566");
        assert_eq!(hex(&encode_set_primary("")), "0700000000");
    }

    #[test]
    fn text_keys_are_checked_before_sending() {
        for key in ["chat", "my-app.setting_1"] {
            assert!(is_valid_text_key(key), "{key}");
        }
        for key in ["", "Chat", "has space", &"k".repeat(33)] {
            assert!(!is_valid_text_key(key), "{key}");
        }
    }
}
