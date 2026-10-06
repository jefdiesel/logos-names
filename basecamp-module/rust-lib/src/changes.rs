//! Builds the transactions that change a name, following SPEC.md section 8.
//!
//! Each function reads what it needs from the registry, checks what it can,
//! and returns a [`Change`] for a wallet to sign. The chain cannot check what
//! is inside a sealed profile, so the limits on entries, keys and pages are
//! enforced here. Checking first also turns a refused transaction into an
//! explanation: LEZ includes failed transactions in blocks without saying why.

use crate::registry::{
    account_text, is_valid_name, page_address, record_address, seal_page, AccountId, Derived,
    Profile, Registry, Resolved, Writer, NAME_SUFFIX, VPK_LEN,
};

const MAX_TEXT_RECORDS: usize = 16;
const MAX_TEXT_KEY_LEN: usize = 32;
const MAX_TEXT_VALUE_LEN: usize = 512;
const MAX_CONTENT_TYPE_LEN: usize = 64;
const MAX_SEALED_PAGE_LEN: usize = 80 * 1024;
/// Sealing adds a nonce, a tag and the page's own framing to the body.
const PAGE_OVERHEAD: usize = 12 + 16 + 4 + MAX_CONTENT_TYPE_LEN + 4;

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

/// An instruction that opens with its index and the name's identifier.
fn on_name(index: u8, id: &[u8; 32]) -> Writer {
    let mut writer = Writer(vec![index]);
    writer.0.extend(id);
    writer
}

pub fn encode_register(id: &[u8; 32], sealed: &[u8]) -> Vec<u8> {
    let mut writer = on_name(0, id);
    writer.bytes(sealed);
    writer.0
}

pub fn encode_set_record(id: &[u8; 32], sealed: &[u8]) -> Vec<u8> {
    let mut writer = on_name(1, id);
    writer.bytes(sealed);
    writer.0
}

pub fn encode_set_page(id: &[u8; 32], sealed: &[u8]) -> Vec<u8> {
    let mut writer = on_name(2, id);
    writer.bytes(sealed);
    writer.0
}

pub fn encode_transfer(id: &[u8; 32], new_owner: &AccountId) -> Vec<u8> {
    let mut writer = on_name(3, id);
    writer.0.extend(new_owner);
    writer.0
}

pub fn encode_release(id: &[u8; 32]) -> Vec<u8> {
    on_name(4, id).0
}

pub fn encode_set_primary(name: &str) -> Vec<u8> {
    let mut writer = Writer(vec![5]);
    writer.string(name);
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
    /// A registered name, for a change only its owner can make.
    fn owned(&self, name: &str, derived: &Derived) -> Result<Resolved, String> {
        self.resolve(name, derived)?
            .ok_or_else(|| format!("{} is not registered", display(name)))
    }

    /// Replaces the name's profile with what `edit` makes of the current one.
    fn edit_profile(
        &self,
        name: &str,
        derived: &Derived,
        edit: impl FnOnce(&mut Profile) -> Result<(), String>,
    ) -> Result<Change, String> {
        let resolved = self.owned(name, derived)?;
        let mut profile = resolved.profile;
        edit(&mut profile)?;
        Ok(Change {
            instruction: encode_set_record(&derived.id, &profile.seal(&derived.key)?),
            accounts: vec![record_address(&self.program, &derived.id), resolved.owner],
            signer: resolved.owner,
        })
    }

    /// A change over the `[record, page, owner]` accounts.
    fn on_record_and_page(&self, name: &str, derived: &Derived, instruction: Vec<u8>) -> Result<Change, String> {
        let owner = self.owned(name, derived)?.owner;
        Ok(Change {
            instruction,
            accounts: vec![
                record_address(&self.program, &derived.id),
                page_address(&self.program, &derived.id),
                owner,
            ],
            signer: owner,
        })
    }

    /// Registers `name` to `owner`. Its first profile holds the name and, when
    /// `private_keys` are given, where it can be paid privately.
    pub fn register(
        &self,
        name: &str,
        derived: &Derived,
        owner: &AccountId,
        private_keys: Option<([u8; 32], Vec<u8>)>,
    ) -> Result<Change, String> {
        if self.resolve(name, derived)?.is_some() {
            return Err(format!("{} is already registered", display(name)));
        }
        let mut profile = Profile::bare(name);
        if let Some((npk, vpk)) = private_keys {
            profile.npk = Some(npk);
            profile.vpk = vpk;
        }
        Ok(Change {
            instruction: encode_register(&derived.id, &profile.seal(&derived.key)?),
            accounts: vec![record_address(&self.program, &derived.id), *owner],
            signer: *owner,
        })
    }

    pub fn set_account(&self, name: &str, derived: &Derived, account: &AccountId) -> Result<Change, String> {
        self.edit_profile(name, derived, |profile| {
            profile.account = Some(*account);
            Ok(())
        })
    }

    /// Publishes the keys for private payments. With `only`, the public
    /// account is removed too, so the name can be paid no other way.
    pub fn set_private_keys(
        &self,
        name: &str,
        derived: &Derived,
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
        self.edit_profile(name, derived, |profile| {
            profile.npk = Some(npk);
            profile.vpk = vpk.to_vec();
            if only {
                profile.account = None;
            }
            Ok(())
        })
    }

    pub fn clear_addresses(&self, name: &str, derived: &Derived) -> Result<Change, String> {
        self.edit_profile(name, derived, |profile| {
            profile.account = None;
            profile.npk = None;
            profile.vpk = Vec::new();
            Ok(())
        })
    }

    /// Sets a text entry; an empty value removes the key.
    pub fn set_text(&self, name: &str, derived: &Derived, key: &str, value: &str) -> Result<Change, String> {
        if !is_valid_text_key(key) {
            return Err("an entry's key is 1-32 characters of a-z, 0-9, '-', '_' or '.'".to_owned());
        }
        if value.len() > MAX_TEXT_VALUE_LEN {
            return Err(format!("an entry's value is at most {MAX_TEXT_VALUE_LEN} bytes"));
        }
        self.edit_profile(name, derived, |profile| {
            let existing = profile.texts.iter().position(|(held, _)| held == key);
            match (existing, value.is_empty()) {
                (None, true) => return Err(format!("{} has no entry called {key}", display(name))),
                (Some(at), true) => {
                    profile.texts.remove(at);
                }
                (Some(at), false) => profile.texts[at].1 = value.to_owned(),
                (None, false) => {
                    if profile.texts.len() >= MAX_TEXT_RECORDS {
                        return Err(format!("a name holds at most {MAX_TEXT_RECORDS} entries"));
                    }
                    profile.texts.push((key.to_owned(), value.to_owned()));
                }
            }
            Ok(())
        })
    }

    /// Writes the name's page; an empty body removes it.
    pub fn set_page(&self, name: &str, derived: &Derived, content_type: &str, body: &[u8]) -> Result<Change, String> {
        if content_type.is_empty() || content_type.len() > MAX_CONTENT_TYPE_LEN {
            return Err(format!("the content type is 1-{MAX_CONTENT_TYPE_LEN} bytes"));
        }
        if body.len() > MAX_SEALED_PAGE_LEN - PAGE_OVERHEAD {
            return Err(format!("a page is at most {} bytes", MAX_SEALED_PAGE_LEN - PAGE_OVERHEAD));
        }
        let sealed = if body.is_empty() {
            Vec::new()
        } else {
            seal_page(&derived.key, content_type, body)?
        };
        self.on_record_and_page(name, derived, encode_set_page(&derived.id, &sealed))
    }

    /// Hands the name to `new_owner`. The program empties everything the
    /// current owner sealed, profile and page.
    pub fn transfer(&self, name: &str, derived: &Derived, new_owner: &AccountId) -> Result<Change, String> {
        if self.owned(name, derived)?.owner == *new_owner {
            return Err(format!("{} already belongs to that account", display(name)));
        }
        self.on_record_and_page(name, derived, encode_transfer(&derived.id, new_owner))
    }

    /// Gives the name up.
    pub fn release(&self, name: &str, derived: &Derived) -> Result<Change, String> {
        self.on_record_and_page(name, derived, encode_release(&derived.id))
    }

    /// Publishes the name `account` is shown as; an empty name withdraws it.
    /// A reverse record counts only when the name points back, so one that
    /// would not is refused here rather than written and ignored.
    pub fn set_primary(&self, account: &AccountId, name: &str, derived: Option<&Derived>) -> Result<Change, String> {
        if let Some(derived) = derived {
            if !is_valid_name(name) {
                return Err("not a valid name".to_owned());
            }
            let resolved = self.owned(name, derived)?;
            if resolved.owner != *account && resolved.profile.account != Some(*account) {
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
        let id: [u8; 32] = core::array::from_fn(|index| index as u8);
        let id_hex = hex(&id);
        assert_eq!(hex(&encode_register(&id, &[0xEE, 0xFF])), format!("00{id_hex}02000000eeff"));
        assert_eq!(hex(&encode_set_record(&id, &[0xEE])), format!("01{id_hex}01000000ee"));
        assert_eq!(hex(&encode_set_page(&id, &[])), format!("02{id_hex}00000000"));
        assert_eq!(
            hex(&encode_transfer(&id, &[2; 32])),
            format!("03{id_hex}{}", "02".repeat(32))
        );
        assert_eq!(hex(&encode_release(&id)), format!("04{id_hex}"));
        assert_eq!(hex(&encode_set_primary("jef")), "05030000006a6566");
        assert_eq!(hex(&encode_set_primary("")), "0500000000");
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
