//! Read-only client for a Logos Names registry, following SPEC.md.
//!
//! This is a second implementation of the layouts in `core/src/lib.rs`, kept
//! separate because a module build sees only its own directory and cannot take
//! the LEZ crates as dependencies. The shared test vectors keep the two in step.

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

pub const NAME_SUFFIX: &str = ".lez";
const MAX_NAME_LEN: usize = 32;
const PDA_PREFIX: &[u8] = b"/LEE/v0.2/AccountId/PDA/";
const NAME_SEED_DOMAIN: &str = "logos-names/v1/name/";
const PAGE_SEED_DOMAIN: &str = "logos-names/v1/page/";
const DIRECTORY_SEED: &str = "logos-names/v1/directory";
/// Listing costs one read per name, so a single call is capped.
pub const MAX_LIST: u64 = 100;

pub type AccountId = [u8; 32];

/// Turns what a person typed, such as `Jef.lez`, into the bare on-chain name.
pub fn normalize_name(input: &str) -> String {
    let lowered = input.trim().to_ascii_lowercase();
    match lowered.strip_suffix(NAME_SUFFIX) {
        Some(bare) => bare.to_owned(),
        None => lowered,
    }
}

pub fn is_valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= MAX_NAME_LEN
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
        && bytes[0] != b'-'
        && bytes[bytes.len() - 1] != b'-'
}

/// Normalises typed input and returns the bare name, or why it is not a name.
pub fn parse_name(input: &str) -> Result<String, String> {
    let name = normalize_name(input);
    if is_valid_name(&name) {
        Ok(name)
    } else {
        Err("a name is 1-32 characters of a-z, 0-9 or '-', such as jef.lez".to_owned())
    }
}

pub fn parse_account(text: &str) -> Result<AccountId, String> {
    bs58::decode(text.trim())
        .into_vec()
        .ok()
        .and_then(|bytes| AccountId::try_from(bytes).ok())
        .ok_or_else(|| format!("not an account ID: {text}"))
}

pub fn account_text(account: &AccountId) -> String {
    bs58::encode(account).into_string()
}

fn sha256(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize().into()
}

fn derived(program: &AccountId, seed_text: &str) -> AccountId {
    let mut prefix = [0_u8; 32];
    prefix[..PDA_PREFIX.len()].copy_from_slice(PDA_PREFIX);
    let seed = sha256(&[seed_text.as_bytes()]);
    sha256(&[&prefix, program, &seed])
}

pub fn record_address(program: &AccountId, name: &str) -> AccountId {
    derived(program, &format!("{NAME_SEED_DOMAIN}{name}"))
}

pub fn page_address(program: &AccountId, name: &str) -> AccountId {
    derived(program, &format!("{PAGE_SEED_DOMAIN}{name}"))
}

pub fn directory_address(program: &AccountId) -> AccountId {
    derived(program, DIRECTORY_SEED)
}

/// Minimal borsh reader for the registry's layouts.
struct Reader<'bytes> {
    bytes: &'bytes [u8],
}

impl<'bytes> Reader<'bytes> {
    fn take(&mut self, length: usize) -> Result<&'bytes [u8], String> {
        if length > self.bytes.len() {
            return Err("the record is truncated".to_owned());
        }
        let (head, rest) = self.bytes.split_at(length);
        self.bytes = rest;
        Ok(head)
    }

    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<usize, String> {
        let bytes: [u8; 4] = self.take(4)?.try_into().expect("four bytes");
        Ok(u32::from_le_bytes(bytes) as usize)
    }

    fn u64(&mut self) -> Result<u64, String> {
        let bytes: [u8; 8] = self.take(8)?.try_into().expect("eight bytes");
        Ok(u64::from_le_bytes(bytes))
    }

    fn account(&mut self) -> Result<AccountId, String> {
        Ok(self.take(32)?.try_into().expect("32 bytes"))
    }

    fn byte_vec(&mut self) -> Result<&'bytes [u8], String> {
        let length = self.u32()?;
        self.take(length)
    }

    fn string(&mut self) -> Result<String, String> {
        String::from_utf8(self.byte_vec()?.to_vec()).map_err(|_| "the record holds invalid text".to_owned())
    }

    fn is_some(&mut self) -> Result<bool, String> {
        Ok(self.u8()? != 0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameRecord {
    pub name: String,
    pub owner: AccountId,
    pub account: Option<AccountId>,
    pub npk: Option<[u8; 32]>,
    pub vpk: Vec<u8>,
    pub texts: Vec<(String, String)>,
    pub previous: Option<String>,
}

impl NameRecord {
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let mut reader = Reader { bytes };
        let _version = reader.u8()?;
        let name = reader.string()?;
        let owner = reader.account()?;
        let account = if reader.is_some()? { Some(reader.account()?) } else { None };
        let npk = if reader.is_some()? { Some(reader.account()?) } else { None };
        let vpk = reader.byte_vec()?.to_vec();
        let count = reader.u32()?;
        let mut texts = Vec::new();
        for _ in 0..count {
            texts.push((reader.string()?, reader.string()?));
        }
        let previous = if reader.is_some()? { Some(reader.string()?) } else { None };
        Ok(Self { name, owner, account, npk, vpk, texts, previous })
    }

    /// The record in the JSON shape SPEC.md gives for HTTP.
    pub fn to_json(&self) -> Value {
        let texts: Map<String, Value> = self
            .texts
            .iter()
            .map(|(key, value)| (key.clone(), Value::from(value.clone())))
            .collect();
        json!({
            "name": self.name,
            "display": format!("{}{NAME_SUFFIX}", self.name),
            "owner": account_text(&self.owner),
            "account": self.account.as_ref().map(account_text),
            "npk": self.npk.as_ref().map(|npk| hex(npk)),
            "vpk": (!self.vpk.is_empty()).then(|| hex(&self.vpk)),
            "texts": texts,
            "previous": self.previous,
        })
    }

    /// Where to send a payment for this name: publicly, privately, or neither.
    pub fn recipient_json(&self) -> Value {
        let private = self.npk.as_ref().filter(|_| !self.vpk.is_empty()).map(|npk| {
            json!({
                "nullifier_public_key": hex(npk),
                "viewing_public_key": hex(&self.vpk),
            })
        });
        json!({
            "name": self.name,
            "display": format!("{}{NAME_SUFFIX}", self.name),
            "public": self.account.as_ref().map(account_text),
            "public_hex": self.account.as_ref().map(|account| hex(account)),
            "private": private,
        })
    }
}

/// Decodes the directory, returning `(count, latest)`.
pub fn decode_directory(bytes: &[u8]) -> Result<(u64, Option<String>), String> {
    let mut reader = Reader { bytes };
    let _version = reader.u8()?;
    let count = reader.u64()?;
    let latest = if reader.is_some()? { Some(reader.string()?) } else { None };
    Ok((count, latest))
}

/// Decodes a reverse record, returning the claimed name.
pub fn decode_primary(bytes: &[u8]) -> Result<String, String> {
    let mut reader = Reader { bytes };
    let _version = reader.u8()?;
    reader.string()
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// A registry on one network: where to read it and which program it is.
#[derive(Debug, Clone)]
pub struct Registry {
    pub sequencer: String,
    pub program: AccountId,
}

impl Registry {
    /// The registry's shard on `account`; empty when the account holds none.
    fn shard(&self, account: &AccountId) -> Result<Vec<u8>, String> {
        let reply: Value = ureq::post(&self.sequencer)
            .timeout(std::time::Duration::from_secs(15))
            .send_json(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "getAccount",
                "params": [account_text(account)],
            }))
            .map_err(|error| format!("could not reach the sequencer: {error}"))?
            .into_json()
            .map_err(|error| format!("the sequencer sent an unreadable reply: {error}"))?;
        if let Some(error) = reply.get("error") {
            return Err(format!("the sequencer refused the read: {error}"));
        }
        let shard = &reply["result"]["data"]["shards"][account_text(&self.program)];
        match shard.as_array() {
            None => Ok(Vec::new()),
            Some(values) => values
                .iter()
                .map(|value| {
                    value
                        .as_u64()
                        .and_then(|byte| u8::try_from(byte).ok())
                        .ok_or_else(|| "the sequencer sent malformed account data".to_owned())
                })
                .collect(),
        }
    }

    pub fn resolve(&self, name: &str) -> Result<Option<NameRecord>, String> {
        let bytes = self.shard(&record_address(&self.program, name))?;
        if bytes.is_empty() {
            return Ok(None);
        }
        NameRecord::decode(&bytes).map(Some)
    }

    /// The name `account` is shown as. A reverse record is only a claim, so it
    /// counts only when the forward record points back at the same account.
    pub fn reverse(&self, account: &AccountId) -> Result<Option<String>, String> {
        let bytes = self.shard(account)?;
        if bytes.is_empty() {
            return Ok(None);
        }
        let claimed = decode_primary(&bytes)?;
        if !is_valid_name(&claimed) {
            return Ok(None);
        }
        let confirmed = self
            .resolve(&claimed)?
            .is_some_and(|record| record.owner == *account || record.account == Some(*account));
        Ok(confirmed.then_some(claimed))
    }

    /// Names newest first, as SPEC.md's list response.
    pub fn list(&self, limit: u64, before: Option<&str>) -> Result<Value, String> {
        let bytes = self.shard(&directory_address(&self.program))?;
        let (total, latest) = if bytes.is_empty() { (0, None) } else { decode_directory(&bytes)? };

        let mut next = match before {
            None => latest,
            Some(name) => self
                .resolve(name)?
                .ok_or_else(|| format!("{name} is not registered"))?
                .previous,
        };
        let mut names = Vec::new();
        while let Some(name) = next.take() {
            if names.len() as u64 >= limit.clamp(1, MAX_LIST) {
                next = Some(name);
                break;
            }
            let record = self
                .resolve(&name)?
                .ok_or_else(|| format!("the name list is broken at {name}"))?;
            names.push(json!({ "name": record.name, "owner": account_text(&record.owner) }));
            next = record.previous;
        }
        Ok(json!({ "total": total, "names": names, "more": next.is_some() }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROGRAM: AccountId = [7; 32];

    /// The vectors in SPEC.md, shared with the Rust core and the JavaScript resolver.
    #[test]
    fn addresses_match_the_specification() {
        assert_eq!(account_text(&PROGRAM), "US517G5965aydkZ46HS38QLi7UQiSojurfbQfKCELFx");
        assert_eq!(
            account_text(&record_address(&PROGRAM, "jef")),
            "28mUmKUyH1dJgmwdjVxS22P12iUHVotLcvk2nbrFw29Y"
        );
        assert_eq!(
            account_text(&page_address(&PROGRAM, "jef")),
            "6ckRpxviqmmhY6QkcJPVPWYSSy3eRZGzy1uUGStf1TnC"
        );
        assert_eq!(
            account_text(&directory_address(&PROGRAM)),
            "9oQ7KANHqw4ok3goEHzgeK7WSxdKqKNXA7bS9HHpq97p"
        );
    }

    #[test]
    fn typed_names_are_normalised_and_checked() {
        for typed in ["jef", "jef.lez", "Jef.LEZ", "  jef.lez  "] {
            assert_eq!(parse_name(typed).as_deref(), Ok("jef"), "{typed}");
        }
        for bad in ["", ".lez", "jef.eth", "-jef", "jef-", "j f", &"a".repeat(33)] {
            assert!(parse_name(bad).is_err(), "{bad}");
        }
    }

    fn string(text: &str) -> Vec<u8> {
        let mut bytes = (text.len() as u32).to_le_bytes().to_vec();
        bytes.extend_from_slice(text.as_bytes());
        bytes
    }

    fn sample_record() -> Vec<u8> {
        let mut bytes = vec![1];
        bytes.extend(string("jef"));
        bytes.extend([1; 32]); // owner
        bytes.push(1);
        bytes.extend([2; 32]); // account
        bytes.push(1);
        bytes.extend([3; 32]); // npk
        bytes.extend(4_u32.to_le_bytes());
        bytes.extend([9, 9, 9, 9]); // vpk
        bytes.extend(1_u32.to_le_bytes());
        bytes.extend(string("chat"));
        bytes.extend(string("hello"));
        bytes.push(1);
        bytes.extend(string("ada"));
        bytes
    }

    #[test]
    fn a_record_decodes_and_reports_where_to_pay() {
        let record = NameRecord::decode(&sample_record()).expect("decodes");
        assert_eq!(record.name, "jef");
        assert_eq!(record.account, Some([2; 32]));
        assert_eq!(record.texts, vec![("chat".to_owned(), "hello".to_owned())]);
        assert_eq!(record.previous.as_deref(), Some("ada"));

        let recipient = record.recipient_json();
        assert_eq!(recipient["display"], "jef.lez");
        assert_eq!(recipient["public"], account_text(&[2; 32]));
        assert_eq!(recipient["public_hex"], "02".repeat(32));
        assert_eq!(recipient["private"]["nullifier_public_key"], "03".repeat(32));
        assert_eq!(recipient["private"]["viewing_public_key"], "09090909");
        assert_eq!(record.to_json()["texts"]["chat"], "hello");
    }

    #[test]
    fn truncated_data_is_refused_not_misread() {
        let bytes = sample_record();
        assert!(NameRecord::decode(&bytes[..bytes.len() - 2]).is_err());
        assert!(NameRecord::decode(&[1, 9, 0, 0, 0]).is_err());
    }

    #[test]
    fn a_directory_and_a_reverse_record_decode() {
        let mut directory = vec![1];
        directory.extend(3_u64.to_le_bytes());
        directory.push(1);
        directory.extend(string("jef"));
        assert_eq!(decode_directory(&directory), Ok((3, Some("jef".to_owned()))));

        let mut primary = vec![1];
        primary.extend(string("jef"));
        assert_eq!(decode_primary(&primary).as_deref(), Ok("jef"));
    }
}
