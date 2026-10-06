//! Reads a Logos Names registry, following SPEC.md. `changes.rs` builds on it
//! to change one.
//!
//! This is a second implementation of what `core/` and `seal/` define, kept
//! separate because a module build sees only its own directory and cannot take
//! the LEZ crates as dependencies. The shared test vectors keep them in step.

use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

pub const NAME_SUFFIX: &str = ".lez";
const MAX_NAME_LEN: usize = 32;
const PDA_PREFIX: &[u8] = b"/LEE/v0.2/AccountId/PDA/";
const DERIVATION_SALT: &[u8] = b"logos-names/v2";
/// The most a Cloudflare Worker's Web Crypto allows, so a gateway can keep up.
const DERIVATION_ROUNDS: u32 = 100_000;
const ID_DOMAIN: &[u8] = b"logos-names/v2/id";
const KEY_DOMAIN: &[u8] = b"logos-names/v2/key";
const NAME_SEED_DOMAIN: &[u8] = b"logos-names/v2/name/";
const PAGE_SEED_DOMAIN: &[u8] = b"logos-names/v2/page/";
const SETTINGS_SEED: &[u8] = b"logos-names/v2/config";
/// After a name expires, only its holder can renew it for this long. Past
/// that it reads as unregistered and anyone may register it.
pub const GRACE_MS: u64 = 30 * 24 * 60 * 60 * 1000;
/// How long each term holds a name, in milliseconds, with its label.
pub const TERMS: [(u64, &str, u64); 3] = [
    (30 * 24 * 60 * 60 * 1000, "30 days", 30),
    (365 * 24 * 60 * 60 * 1000, "1 year", 365),
    (5 * 365 * 24 * 60 * 60 * 1000, "5 years", 5 * 365),
];
/// The account the chain keeps its clock on, and the program whose shard on
/// it holds the time: `{block_id: u64, timestamp: u64}`, in milliseconds.
const CLOCK_ACCOUNT: &AccountId = b"/LEZ/ClockProgramAccount/0000001";
const BUILTIN_PROGRAM_PREFIX: &[u8] = b"/LEE-BuiltinProgram/v1/AccountId";
const RECORD_SEAL: &[u8] = b"logos-names/v2/record";
const PAGE_SEAL: &[u8] = b"logos-names/v2/page";
const NONCE_LEN: usize = 12;
/// A sealed profile's plaintext is padded to a multiple of this, so its
/// length says little about what is in it.
const PROFILE_PADDING: usize = 256;
/// ML-KEM-768 encapsulation key, as used for LEZ private accounts.
pub const VPK_LEN: usize = 1184;

pub type AccountId = [u8; 32];

/// Turns what a person typed, such as `Jef.lez`, into the bare name.
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
/// The chain never sees a name, so this check is the only one there is.
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

/// What a name turns into: the identifier the chain knows it by, and the key
/// that opens and seals what it resolves to. The key never leaves the client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Derived {
    pub id: [u8; 32],
    pub key: [u8; 32],
}

/// Derives a name's identifier and key. Slow on purpose, so that scanning for
/// names is expensive; callers that ask repeatedly should keep the result.
pub fn derive(name: &str) -> Derived {
    let mut master = [0_u8; 32];
    pbkdf2::pbkdf2_hmac::<Sha256>(name.as_bytes(), DERIVATION_SALT, DERIVATION_ROUNDS, &mut master);
    Derived {
        id: sha256(&[ID_DOMAIN, &master]),
        key: sha256(&[KEY_DOMAIN, &master]),
    }
}

fn derived_address(program: &AccountId, domain: &[u8], id: &[u8; 32]) -> AccountId {
    let mut prefix = [0_u8; 32];
    prefix[..PDA_PREFIX.len()].copy_from_slice(PDA_PREFIX);
    let seed = sha256(&[domain, id]);
    sha256(&[&prefix, program, &seed])
}

pub fn record_address(program: &AccountId, id: &[u8; 32]) -> AccountId {
    derived_address(program, NAME_SEED_DOMAIN, id)
}

pub fn page_address(program: &AccountId, id: &[u8; 32]) -> AccountId {
    derived_address(program, PAGE_SEED_DOMAIN, id)
}

/// The account holding the registry's settings: who is paid, and how much.
pub fn settings_address(program: &AccountId) -> AccountId {
    let mut prefix = [0_u8; 32];
    prefix[..PDA_PREFIX.len()].copy_from_slice(PDA_PREFIX);
    sha256(&[&prefix, program, &sha256(&[SETTINGS_SEED])])
}

/// The ID of the chain's built-in clock program.
fn clock_program() -> AccountId {
    sha256(&[BUILTIN_PROGRAM_PREFIX, b"clock"])
}

/// This machine's clock, in milliseconds since the Unix epoch.
pub fn local_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| u64::try_from(since.as_millis()).unwrap_or(u64::MAX))
}

/// What registering and renewing cost, and who is paid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub admin: AccountId,
    pub treasury: AccountId,
    pub prices: [u128; 3],
}

impl Settings {
    fn decode(bytes: &[u8]) -> Result<Self, String> {
        let mut reader = Reader { bytes };
        let _version = reader.u8()?;
        let admin = reader.account()?;
        let treasury = reader.account()?;
        let mut prices = [0_u128; 3];
        for price in &mut prices {
            *price = u128::from_le_bytes(reader.take(16)?.try_into().expect("16 bytes"));
        }
        Ok(Self { admin, treasury, prices })
    }

    /// What `payer` owes for `term`, and whom it is paid to. Nothing is owed
    /// by the treasury itself, which would only be paying itself.
    pub fn fee(settings: Option<&Self>, payer: &AccountId, term: usize) -> (u128, Option<AccountId>) {
        match settings {
            Some(settings) if settings.treasury != *payer && settings.prices[term] > 0 => {
                (settings.prices[term], Some(settings.treasury))
            }
            _ => (0, None),
        }
    }
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

    fn u64(&mut self) -> Result<u64, String> {
        let bytes: [u8; 8] = self.take(8)?.try_into().expect("eight bytes");
        Ok(u64::from_le_bytes(bytes))
    }

    fn u32(&mut self) -> Result<usize, String> {
        let bytes: [u8; 4] = self.take(4)?.try_into().expect("four bytes");
        Ok(u32::from_le_bytes(bytes) as usize)
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

/// Minimal borsh writer, the reader's counterpart.
#[derive(Default)]
pub struct Writer(pub Vec<u8>);

impl Writer {
    pub fn bytes(&mut self, bytes: &[u8]) {
        self.0.extend((bytes.len() as u32).to_le_bytes());
        self.0.extend(bytes);
    }

    pub fn string(&mut self, text: &str) {
        self.bytes(text.as_bytes());
    }

    fn optional(&mut self, value: Option<&[u8; 32]>) {
        match value {
            None => self.0.push(0),
            Some(value) => {
                self.0.push(1);
                self.0.extend(value);
            }
        }
    }
}

/// What a name resolves to, as its holder sealed it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Profile {
    pub name: String,
    pub account: Option<AccountId>,
    pub npk: Option<[u8; 32]>,
    pub vpk: Vec<u8>,
    pub texts: Vec<(String, String)>,
}

impl Profile {
    /// The profile of a name with nothing set.
    pub fn bare(name: &str) -> Self {
        Self { name: name.to_owned(), ..Self::default() }
    }

    /// Decodes a profile from the start of `bytes`; padding after it is ignored.
    fn decode(bytes: &[u8]) -> Result<Self, String> {
        let mut reader = Reader { bytes };
        let name = reader.string()?;
        let account = if reader.is_some()? { Some(reader.account()?) } else { None };
        let npk = if reader.is_some()? { Some(reader.account()?) } else { None };
        let vpk = reader.byte_vec()?.to_vec();
        let count = reader.u32()?;
        let mut texts = Vec::new();
        for _ in 0..count {
            texts.push((reader.string()?, reader.string()?));
        }
        Ok(Self { name, account, npk, vpk, texts })
    }

    /// The profile's borsh bytes, zero-padded to a multiple of [`PROFILE_PADDING`].
    fn encode(&self) -> Vec<u8> {
        let mut writer = Writer::default();
        writer.string(&self.name);
        writer.optional(self.account.as_ref());
        writer.optional(self.npk.as_ref());
        writer.bytes(&self.vpk);
        writer.0.extend((self.texts.len() as u32).to_le_bytes());
        for (key, value) in &self.texts {
            writer.string(key);
            writer.string(value);
        }
        let padded = writer.0.len().div_ceil(PROFILE_PADDING) * PROFILE_PADDING;
        writer.0.resize(padded, 0);
        writer.0
    }

    /// Seals the profile under the name's key.
    pub fn seal(&self, key: &[u8; 32]) -> Result<Vec<u8>, String> {
        seal(key, &self.encode(), RECORD_SEAL, fresh_nonce()?)
    }

    /// Opens a record's sealed bytes as the profile of `name`. Empty bytes,
    /// bytes this key does not open, and a profile of another name all mean
    /// the same thing: the name has nothing set.
    pub fn open(key: &[u8; 32], sealed: &[u8], name: &str) -> Self {
        open(key, sealed, RECORD_SEAL)
            .and_then(|plaintext| Self::decode(&plaintext).ok())
            .filter(|profile| profile.name == name)
            .unwrap_or_else(|| Self::bare(name))
    }
}

/// Seals a page under the name's key.
pub fn seal_page(key: &[u8; 32], content_type: &str, body: &[u8]) -> Result<Vec<u8>, String> {
    let mut writer = Writer::default();
    writer.string(content_type);
    writer.bytes(body);
    seal(key, &writer.0, PAGE_SEAL, fresh_nonce()?)
}

/// Opens a sealed page into the JSON an app needs to show it. `text` is the
/// body when it is valid UTF-8, and null otherwise.
pub fn open_page(key: &[u8; 32], sealed: &[u8]) -> Option<Value> {
    let plaintext = open(key, sealed, PAGE_SEAL)?;
    let mut reader = Reader { bytes: &plaintext };
    let content_type = reader.string().ok()?;
    let body = reader.byte_vec().ok()?;
    Some(json!({
        "content_type": content_type,
        "bytes": body.len(),
        "text": std::str::from_utf8(body).ok(),
    }))
}

fn fresh_nonce() -> Result<[u8; NONCE_LEN], String> {
    let mut nonce = [0_u8; NONCE_LEN];
    getrandom::getrandom(&mut nonce).map_err(|_| "this machine gave no random bytes".to_owned())?;
    Ok(nonce)
}

/// `nonce ‖ AES-256-GCM(key, nonce, plaintext, associated data)`.
fn seal(key: &[u8; 32], plaintext: &[u8], associated: &[u8], nonce: [u8; NONCE_LEN]) -> Result<Vec<u8>, String> {
    let cipher = Aes256Gcm::new_from_slice(key).expect("a 32-byte key");
    let sealed = cipher
        .encrypt(Nonce::from_slice(&nonce), Payload { msg: plaintext, aad: associated })
        .map_err(|_| "could not seal".to_owned())?;
    Ok([nonce.as_slice(), &sealed].concat())
}

fn open(key: &[u8; 32], sealed: &[u8], associated: &[u8]) -> Option<Vec<u8>> {
    if sealed.len() <= NONCE_LEN {
        return None;
    }
    let (nonce, ciphertext) = sealed.split_at(NONCE_LEN);
    Aes256Gcm::new_from_slice(key)
        .expect("a 32-byte key")
        .decrypt(Nonce::from_slice(nonce), Payload { msg: ciphertext, aad: associated })
        .ok()
}

/// A registered name: who holds it, until when, and its profile opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub owner: AccountId,
    /// When the name lapses, in milliseconds since the Unix epoch.
    pub expires: u64,
    pub profile: Profile,
}

impl Resolved {
    /// Decodes the on-chain record `{version, owner, expires, sealed}` and opens it.
    fn decode(bytes: &[u8], key: &[u8; 32], name: &str) -> Result<Self, String> {
        let mut reader = Reader { bytes };
        let _version = reader.u8()?;
        let owner = reader.account()?;
        let expires = reader.u64()?;
        let sealed = reader.byte_vec()?;
        Ok(Self { owner, expires, profile: Profile::open(key, sealed, name) })
    }

    /// Whether the name has expired and is in its grace period.
    pub fn lapsed(&self, now: u64) -> bool {
        self.expires <= now
    }

    /// Whether the name is past its grace period, and so counts as unregistered.
    pub fn gone(&self, now: u64) -> bool {
        now >= self.expires.saturating_add(GRACE_MS)
    }

    /// The name in the JSON shape SPEC.md gives.
    pub fn to_json(&self) -> Value {
        let profile = &self.profile;
        let texts: Map<String, Value> = profile
            .texts
            .iter()
            .map(|(key, value)| (key.clone(), Value::from(value.clone())))
            .collect();
        json!({
            "name": profile.name,
            "display": format!("{}{NAME_SUFFIX}", profile.name),
            "owner": account_text(&self.owner),
            "expires": self.expires,
            "lapsed": self.lapsed(local_now()),
            "account": profile.account.as_ref().map(account_text),
            "npk": profile.npk.as_ref().map(|npk| hex(npk)),
            "vpk": (!profile.vpk.is_empty()).then(|| hex(&profile.vpk)),
            "texts": texts,
        })
    }

    /// Where to send a payment for this name: privately, publicly, or neither.
    pub fn recipient_json(&self) -> Value {
        let profile = &self.profile;
        let private = profile.npk.as_ref().filter(|_| !profile.vpk.is_empty()).map(|npk| {
            json!({
                "nullifier_public_key": hex(npk),
                "viewing_public_key": hex(&profile.vpk),
            })
        });
        json!({
            "name": profile.name,
            "display": format!("{}{NAME_SUFFIX}", profile.name),
            "public": profile.account.as_ref().map(account_text),
            "public_hex": profile.account.as_ref().map(|account| hex(account)),
            "private": private,
            "lapsed": self.lapsed(local_now()),
        })
    }
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

    /// Resolves `name`, whose identifier and key the caller has derived. A
    /// name past its grace period is not registered, whatever its record holds.
    pub fn resolve(&self, name: &str, derived: &Derived) -> Result<Option<Resolved>, String> {
        Ok(self.record(name, derived)?.filter(|resolved| !resolved.gone(local_now())))
    }

    /// The record of `name` exactly as the chain holds it, lapsed or not.
    pub fn record(&self, name: &str, derived: &Derived) -> Result<Option<Resolved>, String> {
        let bytes = self.shard(&record_address(&self.program, &derived.id))?;
        if bytes.is_empty() {
            return Ok(None);
        }
        Resolved::decode(&bytes, &derived.key, name).map(Some)
    }

    /// The registry's settings, or `None` while it has none and charges nothing.
    pub fn settings(&self) -> Result<Option<Settings>, String> {
        let bytes = self.shard(&settings_address(&self.program))?;
        if bytes.is_empty() {
            return Ok(None);
        }
        Settings::decode(&bytes).map(Some)
    }

    /// The time to claim in a transaction: the chain's own clock, which the
    /// next block can never be behind. This machine's clock is not a
    /// substitute: a transaction that claims a time ahead of the chain's is
    /// rejected and still charged the whole fee ceiling.
    pub fn now(&self) -> Result<u64, String> {
        shard_bytes(&self.shards(CLOCK_ACCOUNT)?, &clock_program())?
            .get(8..16)
            .and_then(|bytes| <[u8; 8]>::try_from(bytes).ok())
            .map(u64::from_le_bytes)
            .filter(|timestamp| *timestamp > 0)
            .ok_or_else(|| "could not read the chain's clock, so nothing was sent".to_owned())
    }

    /// The page published under a name, or `None` when it has none or the
    /// name is no longer registered.
    pub fn page(&self, name: &str, derived: &Derived) -> Result<Option<Value>, String> {
        if self.resolve(name, derived)?.is_none() {
            return Ok(None);
        }
        let bytes = self.shard(&page_address(&self.program, &derived.id))?;
        if bytes.is_empty() {
            return Ok(None);
        }
        let mut reader = Reader { bytes: &bytes };
        let _version = reader.u8()?;
        Ok(open_page(&derived.key, reader.byte_vec()?))
    }

    /// The name `account` is shown as. A reverse record is only a claim, so it
    /// counts only when the name's own record points back at the same account.
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
        let confirmed = self.resolve(&claimed, &derive(&claimed))?.is_some_and(|resolved| {
            resolved.owner == *account || resolved.profile.account == Some(*account)
        });
        Ok(confirmed.then_some(claimed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROGRAM: AccountId = [7; 32];

    /// The profile sealed in SPEC.md's vector.
    fn sample_profile() -> Profile {
        Profile {
            name: "jef".to_owned(),
            account: Some([2; 32]),
            npk: None,
            vpk: Vec::new(),
            texts: vec![("chat".to_owned(), "hi".to_owned())],
        }
    }

    const NONCE: [u8; NONCE_LEN] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
    const SEALED_PROFILE: &str = "000102030405060708090a0b67bf760994a19946abc799235b95e48bfb804c3ca05471c9261f5099\
e1b00570e6139ab5bbb5fac423627e0fd18c5299475a5c4ed914f1e506f23b66d4d2377de959bff4\
e8d4ab02b63865f9a2453fef24302c211eb70eadebf5b38038d080e2b6f3b1fa17a777b940f17eb9\
4a83f4ce719552c92108600800f3d020a7d8fee73e2792c907db62125ba636607ed94ea46b73f046\
981a751be920449222629d2561194416c993b4bfdeeec20026656f11b64f1c2ac368d64e57d07054\
ac0fcdac4a6e83b60ac363901963488c93c7762d19b34b6991b9abdbc5d293fca06b69cab31e7eaa\
769142940f4813c73713dbd34b24386fb53937e2c9915323f35d39bf50e63d4289cb48a7c7aff0e5\
404987b0";
    const SEALED_PAGE: &str =
        "000102030405060708090a0b6ebf76098aa1873386b5f74030f9e489f9822657c6ba930c224eb1f264767f7cf7b5a511";

    /// The vectors in SPEC.md, shared with the Rust client and the JavaScript resolver.
    #[test]
    fn a_name_derives_the_identifier_and_key_the_specification_gives() {
        let derived = derive("jef");
        assert_eq!(hex(&derived.id), "86d7e719e7acabe344ef8a69782b72ed0c21087e73e400e7056ab0daf7e6c618");
        assert_eq!(hex(&derived.key), "d19aa10c57273a3ff4f569cc3a9c35819b2a9bcac7ba8a48deb131269cb6e2e0");
        assert_eq!(account_text(&PROGRAM), "US517G5965aydkZ46HS38QLi7UQiSojurfbQfKCELFx");
        assert_eq!(
            account_text(&record_address(&PROGRAM, &derived.id)),
            "6gsCMau9ajq4FcxDCJmqqtUWVnuCiJHtXoYbCzCZPGJJ"
        );
        assert_eq!(
            account_text(&page_address(&PROGRAM, &derived.id)),
            "338iYF6WmqnFryDjLWyKTYUhAgpT6o7C8qhHAd5pSf32"
        );
    }

    #[test]
    fn sealing_matches_the_specification_and_opens_again() {
        let key = derive("jef").key;
        let sealed = seal(&key, &sample_profile().encode(), RECORD_SEAL, NONCE).expect("seals");
        assert_eq!(hex(&sealed), SEALED_PROFILE);
        assert_eq!(Profile::open(&key, &sealed, "jef"), sample_profile());

        let mut page = Writer::default();
        page.string("text/plain");
        page.bytes(b"hi");
        assert_eq!(hex(&seal(&key, &page.0, PAGE_SEAL, NONCE).expect("seals")), SEALED_PAGE);
        let opened = open_page(&key, &unhex(SEALED_PAGE).expect("hex")).expect("opens");
        assert_eq!(opened["content_type"], "text/plain");
        assert_eq!(opened["text"], "hi");
        assert_eq!(opened["bytes"], 2);
    }

    #[test]
    fn a_profile_is_padded_and_sealed_differently_each_time() {
        let key = [9; 32];
        assert_eq!(sample_profile().encode().len(), 256);
        let with_keys = Profile { npk: Some([3; 32]), vpk: vec![4; VPK_LEN], ..sample_profile() };
        assert_eq!(with_keys.encode().len() % 256, 0);
        let first = with_keys.seal(&key).expect("seals");
        let second = with_keys.seal(&key).expect("seals");
        assert_ne!(first, second, "a fresh nonce every time");
        assert_eq!(Profile::open(&key, &first, "jef"), with_keys);
        assert_eq!(Profile::open(&key, &second, "jef"), with_keys);
    }

    #[test]
    fn what_does_not_open_reads_as_a_name_with_nothing_set() {
        let key = derive("jef").key;
        let sealed = unhex(SEALED_PROFILE).expect("hex");
        let bare = Profile::bare("jef");
        assert_eq!(Profile::open(&key, &[], "jef"), bare, "nothing sealed");
        assert_eq!(Profile::open(&[1; 32], &sealed, "jef"), bare, "another key");
        let mut tampered = sealed.clone();
        tampered[40] ^= 1;
        assert_eq!(Profile::open(&key, &tampered, "jef"), bare, "tampered");
        assert_eq!(Profile::open(&key, &sealed, "ada"), Profile::bare("ada"), "another name's profile");
        // A page is sealed for pages only: it does not open as a profile.
        assert_eq!(Profile::open(&key, &unhex(SEALED_PAGE).expect("hex"), "jef"), bare);
    }

    #[test]
    fn a_record_decodes_and_reports_where_to_pay() {
        let key = derive("jef").key;
        let profile = Profile { npk: Some([3; 32]), vpk: vec![9, 9, 9, 9], ..sample_profile() };
        let mut record = vec![2];
        record.extend([1; 32]); // owner
        record.extend(4_102_444_800_000_u64.to_le_bytes()); // expires at the start of 2100
        let sealed = profile.seal(&key).expect("seals");
        record.extend((sealed.len() as u32).to_le_bytes());
        record.extend(&sealed);

        let resolved = Resolved::decode(&record, &key, "jef").expect("decodes");
        assert_eq!(resolved.owner, [1; 32]);
        assert_eq!(resolved.expires, 4_102_444_800_000);
        assert_eq!(resolved.profile, profile);
        assert!(!resolved.lapsed(4_102_444_799_999) && resolved.lapsed(4_102_444_800_000));
        assert!(!resolved.gone(4_102_444_800_000 + GRACE_MS - 1) && resolved.gone(4_102_444_800_000 + GRACE_MS));
        assert_eq!(resolved.to_json()["expires"], 4_102_444_800_000_u64);
        assert_eq!(resolved.to_json()["lapsed"], false);

        let recipient = resolved.recipient_json();
        assert_eq!(recipient["display"], "jef.lez");
        assert_eq!(recipient["public"], account_text(&[2; 32]));
        assert_eq!(recipient["public_hex"], "02".repeat(32));
        assert_eq!(recipient["private"]["nullifier_public_key"], "03".repeat(32));
        assert_eq!(recipient["private"]["viewing_public_key"], "09090909");
        assert_eq!(resolved.to_json()["texts"]["chat"], "hi");
        assert_eq!(resolved.to_json()["owner"], account_text(&[1; 32]));

        assert!(Resolved::decode(&record[..20], &key, "jef").is_err(), "truncated");
    }

    #[test]
    fn settings_decode_and_say_what_is_owed() {
        let mut bytes = vec![2];
        bytes.extend([1; 32]); // admin
        bytes.extend([2; 32]); // treasury
        for price in [10_000_u128, 100_000, 400_000] {
            bytes.extend(price.to_le_bytes());
        }
        let settings = Settings::decode(&bytes).expect("decodes");
        assert_eq!(settings.admin, [1; 32]);
        assert_eq!(settings.prices, [10_000, 100_000, 400_000]);
        assert_eq!(Settings::fee(Some(&settings), &[9; 32], 1), (100_000, Some([2; 32])));
        assert_eq!(Settings::fee(Some(&settings), &[2; 32], 1), (0, None), "the treasury pays nothing");
        assert_eq!(Settings::fee(None, &[9; 32], 2), (0, None), "an unconfigured registry is free");
        assert_eq!(TERMS[1].0, 31_536_000_000);
        // The clock's address as the chain itself reports it.
        assert_eq!(account_text(&clock_program()), "2hwaRW8sMnyKLLRbDRw8k68tiv9bnfq98oxcdtcp157Z");
        assert_eq!(account_text(CLOCK_ACCOUNT), bs58::encode(b"/LEZ/ClockProgramAccount/0000001").into_string());
        assert_eq!(GRACE_MS, 2_592_000_000);
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

        let mut primary = vec![2];
        primary.extend(3_u32.to_le_bytes());
        primary.extend(b"jef");
        assert_eq!(decode_primary(&primary).as_deref(), Ok("jef"));
    }
}
