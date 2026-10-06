//! Reads a Logos Names registry, following SPEC.md. `changes.rs` builds on it
//! to change one.
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
    pub next: Option<String>,
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
        let next = if reader.is_some()? { Some(reader.string()?) } else { None };
        Ok(Self { name, owner, account, npk, vpk, texts, previous, next })
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
            "next": self.next,
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

/// Decodes a page into the JSON an app needs to show it. `text` is the body when
/// it is valid UTF-8, and null otherwise.
pub fn page_json(bytes: &[u8]) -> Result<Value, String> {
    let mut reader = Reader { bytes };
    let _version = reader.u8()?;
    let content_type = reader.string()?;
    let body = reader.byte_vec()?;
    Ok(json!({
        "content_type": content_type,
        "bytes": body.len(),
        "text": std::str::from_utf8(body).ok(),
    }))
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

/// The program that holds every account's balance, as 16 little-endian bytes.
const SYSTEM_PROGRAM: AccountId = [0; 32];

/// One program's shard out of an account's shards; empty when it has none.
fn shard_bytes(shards: &Map<String, Value>, program: &AccountId) -> Result<Vec<u8>, String> {
    match shards.get(&account_text(program)).and_then(Value::as_array) {
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

/// Reads hex, with or without a `0x` prefix.
pub fn unhex(text: &str) -> Result<Vec<u8>, String> {
    let text = text.trim();
    let digits = text.strip_prefix("0x").unwrap_or(text);
    if digits.len() % 2 != 0 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("not hexadecimal".to_owned());
    }
    (0..digits.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&digits[at..at + 2], 16).map_err(|_| "not hexadecimal".to_owned()))
        .collect()
}

/// A registry on one network: where to read it and which program it is.
#[derive(Debug, Clone)]
pub struct Registry {
    pub sequencer: String,
    pub program: AccountId,
}

impl Registry {
    /// Every program's shard on `account`, keyed by program ID in base58.
    fn shards(&self, account: &AccountId) -> Result<Map<String, Value>, String> {
        let mut reply: Value = ureq::post(&self.sequencer)
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
        match reply["result"]["data"]["shards"].take() {
            Value::Object(shards) => Ok(shards),
            _ => Ok(Map::new()),
        }
    }

    /// The registry's shard on `account`; empty when the account holds none.
    fn shard(&self, account: &AccountId) -> Result<Vec<u8>, String> {
        shard_bytes(&self.shards(account)?, &self.program)
    }

    /// What a wallet shows for one of its accounts: its balance, and the name
    /// it is shown as if it has a confirmed one. The balance is text because it
    /// is a 128-bit number.
    pub fn account_summary(&self, account: &AccountId) -> Result<Value, String> {
        let shards = self.shards(account)?;
        let balance = match <[u8; 16]>::try_from(shard_bytes(&shards, &SYSTEM_PROGRAM)?) {
            Ok(bytes) => u128::from_le_bytes(bytes),
            Err(_) => 0,
        };
        let claimed = shard_bytes(&shards, &self.program)?;
        let name = if claimed.is_empty() { None } else { self.confirmed(account, &claimed)? };
        Ok(json!({
            "account": account_text(account),
            "balance": balance.to_string(),
            "funded": balance > 0,
            "name": name,
        }))
    }

    /// The directory as `(count, latest)`; an unused registry has an empty one.
    pub fn directory(&self) -> Result<(u64, Option<String>), String> {
        let bytes = self.shard(&directory_address(&self.program))?;
        if bytes.is_empty() {
            Ok((0, None))
        } else {
            decode_directory(&bytes)
        }
    }

    pub fn resolve(&self, name: &str) -> Result<Option<NameRecord>, String> {
        let bytes = self.shard(&record_address(&self.program, name))?;
        if bytes.is_empty() {
            return Ok(None);
        }
        NameRecord::decode(&bytes).map(Some)
    }

    /// The page published under `name`, or `None` when it has none.
    pub fn page(&self, name: &str) -> Result<Option<Value>, String> {
        let bytes = self.shard(&page_address(&self.program, name))?;
        if bytes.is_empty() {
            return Ok(None);
        }
        page_json(&bytes).map(Some)
    }

    /// The name `account` is shown as. A reverse record is only a claim, so it
    /// counts only when the forward record points back at the same account.
    pub fn reverse(&self, account: &AccountId) -> Result<Option<String>, String> {
        let bytes = self.shard(account)?;
        if bytes.is_empty() {
            return Ok(None);
        }
        self.confirmed(account, &bytes)
    }

    /// The name claimed in `reverse_record`, if its own record agrees.
    fn confirmed(&self, account: &AccountId, reverse_record: &[u8]) -> Result<Option<String>, String> {
        let claimed = decode_primary(reverse_record)?;
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
        let (total, latest) = self.directory()?;

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
        bytes.extend(string("ada")); // previous
        bytes.push(0); // next
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
    fn a_page_decodes_with_its_text() {
        let mut page = vec![1];
        page.extend(string("text/html"));
        page.extend(2_u32.to_le_bytes());
        page.extend(b"hi");
        let decoded = page_json(&page).expect("decodes");
        assert_eq!(decoded["content_type"], "text/html");
        assert_eq!(decoded["bytes"], 2);
        assert_eq!(decoded["text"], "hi");
    }

    #[test]
    fn shards_are_picked_by_program_and_hex_reads_back() {
        let mut balance = vec![5];
        balance.resize(16, 0);
        let mut shards = Map::new();
        shards.insert(account_text(&SYSTEM_PROGRAM), json!(balance));
        shards.insert(account_text(&PROGRAM), json!([1, 2, 3]));
        assert_eq!(account_text(&SYSTEM_PROGRAM), "1".repeat(32));
        assert_eq!(shard_bytes(&shards, &PROGRAM), Ok(vec![1, 2, 3]));
        assert_eq!(shard_bytes(&shards, &[9; 32]), Ok(Vec::new()));
        assert_eq!(shard_bytes(&shards, &SYSTEM_PROGRAM).map(|bytes| bytes.len()), Ok(16));

        assert_eq!(unhex("0x0aFF"), Ok(vec![10, 255]));
        assert_eq!(unhex(&hex(&[0, 7, 255])), Ok(vec![0, 7, 255]));
        assert!(unhex("abc").is_err());
        assert!(unhex("zz").is_err());
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
