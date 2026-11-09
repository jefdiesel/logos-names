//! Reads a Logos Names registry on LEZ 0.4, following SPEC.md. `changes.rs`
//! builds on it to change one.
//!
//! This is a second implementation of what `core/` and `seal/` define, kept
//! separate because a module build sees only its own directory and cannot take
//! the LEZ crates as dependencies. The shared test vectors keep them in step:
//! every layout and derivation here is checked against bytes `core` and `seal`
//! produce.

use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use argon2::{Algorithm, Argon2, Params, Version};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

/// How names are shown: bare, `jef` (SPEC 2).
pub const NAME_SUFFIX: &str = "";
/// The text entry that points a name to another (SPEC.md §7, Forwarding).
pub const FORWARD_KEY: &str = "forward";
/// The old suffix, still accepted and removed when typed or pasted.
pub const LEGACY_SUFFIX: &str = ".lez";
const MAX_NAME_LEN: usize = 32;
const PDA_PREFIX: &[u8] = b"/LEE/v0.2/AccountId/PDA/";
/// One fixed salt for every network, on purpose: a name keeps its identifier
/// and key from testnet to mainnet.
const DERIVATION_SALT: &[u8] = b"logos-names/v5";
/// Argon2id memory in KiB (64 MiB), passes and lanes.
const DERIVATION_MEMORY_KIB: u32 = 65_536;
const DERIVATION_PASSES: u32 = 3;
const DERIVATION_LANES: u32 = 1;
const ID_DOMAIN: &[u8] = b"logos-names/v5/id";
const KEY_DOMAIN: &[u8] = b"logos-names/v5/key";
const NAME_SEED_DOMAIN: &[u8] = b"logos-names/v5/name/";
const PAGE_SEED_DOMAIN: &[u8] = b"logos-names/v5/page/";
const SETTINGS_SEED: &[u8] = b"logos-names/v5/config";
const OWNER_TAG_DOMAIN: &[u8] = b"logos-names/v5/owner";
const COMMITMENT_DOMAIN: &[u8] = b"logos-names/v5/commitment";
const COMMITMENT_SEED_DOMAIN: &[u8] = b"logos-names/v5/commit/";
const DEPOSIT_SEED_DOMAIN: &[u8] = b"logos-names/v5/deposit/";
const CLAIM_LEAF_DOMAIN: &[u8] = b"logos-names/v6/claim-leaf";
/// After a fee-mode name expires, only its holder can renew it for this long.
/// Past that it reads as unregistered and anyone may register it.
pub const GRACE_MS: u64 = 30 * 24 * 60 * 60 * 1000;
/// How long each fee-mode term holds a name, in milliseconds, by term index.
#[allow(dead_code)] // Fee mode is read through `Settings::deposit_mode`; kept for the layout's sake.
pub const TERM_LENGTHS_MS: [u64; 3] = [30 * 24 * 60 * 60 * 1000, 365 * 24 * 60 * 60 * 1000, 5 * 365 * 24 * 60 * 60 * 1000];
/// The registry charges a fee per term, paid to its treasury; names lapse.
#[allow(dead_code)] // Fee mode is read through `Settings::deposit_mode`; kept for the layout's sake.
pub const MODE_FEE: u8 = 0;
/// The registry takes a refundable deposit into the name's deposit account;
/// names never lapse.
pub const MODE_DEPOSIT: u8 = 1;
/// The expiry of a name held by a deposit: it never lapses.
pub const NEVER: u64 = u64::MAX;
/// A commit or reveal is valid only in a block whose timestamp is in
/// `[now, now + TIME_WINDOW_MS)`.
pub const TIME_WINDOW_MS: u64 = 10 * 60 * 1000;
/// How long a commitment must wait before its reveal.
pub const MIN_COMMIT_AGE_MS: u64 = 15 * 60 * 1000;
/// How long a commitment can still be revealed.
pub const MAX_COMMIT_AGE_MS: u64 = 24 * 60 * 60 * 1000;
/// The account the chain keeps its clock on, and the program whose state on
/// it holds the time: `{block_id: u64, timestamp: u64}`, in milliseconds.
const CLOCK_ACCOUNT: &AccountId = b"/LEZ/ClockProgramAccount/0000001";
const BUILTIN_PROGRAM_PREFIX: &[u8] = b"/LEE-BuiltinProgram/v1/AccountId";
const RECORD_SEAL: &[u8] = b"logos-names/v5/record";
const PAGE_SEAL: &[u8] = b"logos-names/v5/page";
const NONCE_LEN: usize = 12;
const TAG_LEN: usize = 16;
/// A sealed profile's plaintext is padded to a multiple of this, so its
/// length says little about what is in it.
const PROFILE_PADDING: usize = 256;
/// The most sealed bytes a record holds; the program refuses more.
pub const MAX_SEALED_RECORD_LEN: usize = 12 * 1024;
/// The most sealed bytes a page holds; the program refuses more.
pub const MAX_SEALED_PAGE_LEN: usize = 80 * 1024;
/// The largest sequencer reply read: one actor's state, a page at most, as JSON.
const MAX_REPLY_LEN: usize = 1024 * 1024;
/// ML-KEM-768 encapsulation key, as used for LEZ private accounts.
pub const VPK_LEN: usize = 1184;

pub type AccountId = [u8; 32];
pub type NameId = [u8; 32];
/// What a record keeps in place of its holder's account: [`owner_tag`].
pub type OwnerTag = [u8; 32];

/// Turns what a person typed, such as `Jef`, `@jef`, `lns:jef` or `jef.lez`,
/// into the bare name.
pub fn normalize_name(input: &str) -> String {
    let lowered = input.trim().to_ascii_lowercase();
    let bare = lowered.strip_prefix("lns:").unwrap_or(&lowered);
    let bare = bare.strip_prefix('@').unwrap_or(bare);
    bare.strip_suffix(LEGACY_SUFFIX).unwrap_or(bare).to_owned()
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
    if is_valid_name(&name) && parse_account(&name).is_err() {
        // A name that is also an account ID would be ambiguous in a "send to" field (SPEC 2).
        Ok(name)
    } else if is_valid_name(&name) {
        Err(format!("{name} is also an account ID, so it can't be a name"))
    } else {
        Err("a name is 1-32 characters of a-z, 0-9 or '-', such as jef".to_owned())
    }
}

pub fn parse_account(text: &str) -> Result<AccountId, String> {
    let text = text.trim();
    let bare = text
        .strip_prefix("Public/")
        .or_else(|| text.strip_prefix("Private/"))
        .unwrap_or(text);
    bs58::decode(bare)
        .into_vec()
        .ok()
        .and_then(|bytes| AccountId::try_from(bytes).ok())
        .ok_or_else(|| format!("not an account ID: {text}"))
}

/// The ID typed after an account's `Public/` or `Private/`: an ID and nothing else, so
/// `Private/Public/<id>` is refused rather than read as `Private/<id>`.
pub fn bare_account(id: &str) -> Result<AccountId, String> {
    let id = id.trim();
    if id.contains('/') || id.contains(char::is_whitespace) {
        return Err("that isn't an account ID: write Public/ or Private/ once, then the ID".to_owned());
    }
    parse_account(id)
}

/// Refuses an ID no one can hold, or one that was most likely mangled on the way: the
/// all-zero account (`1111…`), and an ID whose letters are all one case (base58 IDs mix
/// both, so this is an ID re-typed in lower or upper case, which may still decode).
/// For hand-overs, which can't be undone.
pub fn plausible_holder(id: &str, account: &AccountId) -> Result<(), String> {
    if account.iter().all(|byte| *byte == 0) {
        return Err("that is the empty account (all 1s), which no one holds".to_owned());
    }
    let letters: Vec<char> = id.chars().filter(char::is_ascii_alphabetic).collect();
    if letters.len() >= 8 && (letters.iter().all(char::is_ascii_lowercase) || letters.iter().all(char::is_ascii_uppercase)) {
        return Err("that account ID is all one case, so it was probably changed when it was copied (IDs mix upper and lower case); paste it exactly as it was given".to_owned());
    }
    Ok(())
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
    pub id: NameId,
    pub key: [u8; 32],
}

/// Derives a name's identifier and key (SPEC.md section 3):
/// `master = Argon2id(name, "logos-names/v5", 64 MiB, 3 passes, 1 lane)`, then
/// `id = SHA-256("logos-names/v5/id" ‖ master)` and
/// `key = SHA-256("logos-names/v5/key" ‖ master)`. Slow and memory-hard on
/// purpose, so that scanning for names is expensive; callers that ask
/// repeatedly should keep the result.
pub fn derive(name: &str) -> Derived {
    let master = master(name);
    Derived {
        id: sha256(&[ID_DOMAIN, &master]),
        key: sha256(&[KEY_DOMAIN, &master]),
    }
}

fn master(name: &str) -> [u8; 32] {
    let params = Params::new(DERIVATION_MEMORY_KIB, DERIVATION_PASSES, DERIVATION_LANES, Some(32))
        .expect("the Argon2 parameters are valid");
    let mut out = [0_u8; 32];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(name.as_bytes(), DERIVATION_SALT, &mut out)
        .expect("Argon2 accepts these inputs");
    out
}

/// LEZ's public program-derived address for `seed` under `program`.
pub fn pda(program: &AccountId, seed: &[u8; 32]) -> AccountId {
    let mut prefix = [0_u8; 32];
    prefix[..PDA_PREFIX.len()].copy_from_slice(PDA_PREFIX);
    sha256(&[&prefix, program, seed])
}

/// The seed of a name's record. A transaction that may create a registry account
/// lists its seed as evidence, so each account's seed is here beside its address.
pub fn name_seed(id: &NameId) -> [u8; 32] {
    sha256(&[NAME_SEED_DOMAIN, id])
}

pub fn page_seed(id: &NameId) -> [u8; 32] {
    sha256(&[PAGE_SEED_DOMAIN, id])
}

pub fn deposit_seed(id: &NameId) -> [u8; 32] {
    sha256(&[DEPOSIT_SEED_DOMAIN, id])
}

pub fn commitment_seed(commitment: &[u8; 32]) -> [u8; 32] {
    sha256(&[COMMITMENT_SEED_DOMAIN, commitment])
}

pub fn settings_seed() -> [u8; 32] {
    sha256(&[SETTINGS_SEED])
}

pub fn record_address(program: &AccountId, id: &NameId) -> AccountId {
    pda(program, &name_seed(id))
}

pub fn page_address(program: &AccountId, id: &NameId) -> AccountId {
    pda(program, &page_seed(id))
}

/// The account holding a name's deposit, as its native balance. Only the
/// registry can move it, on release.
pub fn deposit_address(program: &AccountId, id: &NameId) -> AccountId {
    pda(program, &deposit_seed(id))
}

/// The account holding the commitment `commitment`.
pub fn commitment_address(program: &AccountId, commitment: &[u8; 32]) -> AccountId {
    pda(program, &commitment_seed(commitment))
}

/// The account holding the registry's settings.
pub fn settings_address(program: &AccountId) -> AccountId {
    pda(program, &settings_seed())
}

/// The tag that stands for `account` as the holder of the name `id`. A record
/// stores this, never the account, so one account's names stay unlinked.
pub fn owner_tag(id: &NameId, account: &AccountId) -> OwnerTag {
    sha256(&[OWNER_TAG_DOMAIN, id, account])
}

/// What a claim commits to: the name's identifier, the tag of the account
/// that will hold it, and a secret the committer picks at random.
pub fn commitment(id: &NameId, holder: &OwnerTag, secret: &[u8; 32]) -> [u8; 32] {
    sha256(&[COMMITMENT_DOMAIN, id, holder, secret])
}

/// The leaf of a name on the claims list: its record's address on the earlier
/// chain, under the settings' `claims_program`, and its holder's tag.
#[cfg(test)]
pub fn claim_leaf(old_record: &AccountId, holder: &OwnerTag) -> [u8; 32] {
    sha256(&[CLAIM_LEAF_DOMAIN, old_record, holder])
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

/// The registry's settings: who administers it, and how names are paid for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub admin: AccountId,
    /// Where fees go in fee mode. It pays nothing itself.
    pub treasury: AccountId,
    /// The fee for each term, in fee mode.
    pub prices: [u128; 3],
    /// [`MODE_FEE`] or [`MODE_DEPOSIT`].
    pub mode: u8,
    /// The deposit for a normal name and for a three-character name, in deposit mode.
    pub deposits: [u128; 2],
    /// The root of the list of names that can be claimed from the earlier
    /// chain; all zeros when there is none.
    pub claims_root: [u8; 32],
    /// Claims close at this time, in milliseconds. Until then a claim overrides
    /// a registration.
    pub claims_until: u64,
    /// The registry on the earlier chain the list was taken from.
    pub claims_program: AccountId,
}

impl Settings {
    /// Decodes `{version, admin, treasury, prices: [u128; 3], mode: u8, deposits:
    /// [u128; 2], claims_root, claims_until: u64, claims_program}`.
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let mut reader = Reader { bytes };
        let version = reader.u8()?;
        let admin = reader.account()?;
        let treasury = reader.account()?;
        let mut prices = [0_u128; 3];
        for price in &mut prices {
            *price = reader.u128()?;
        }
        let mode = reader.u8()?;
        let mut deposits = [0_u128; 2];
        for deposit in &mut deposits {
            *deposit = reader.u128()?;
        }
        // Version 5 (the registry on the LEZ 0.3 testnet) has no claims list.
        let (claims_root, claims_until, claims_program) =
            if version < 6 { ([0; 32], 0, [0; 32]) } else { (reader.account()?, reader.u64()?, reader.account()?) };
        Ok(Self { admin, treasury, prices, mode, deposits, claims_root, claims_until, claims_program })
    }

    /// Whether names from the earlier chain can still be claimed at `now`.
    pub fn claims_open(&self, now: u64) -> bool {
        self.claims_root != [0; 32] && now < self.claims_until
    }

    pub fn deposit_mode(&self) -> bool {
        self.mode == MODE_DEPOSIT
    }

    pub fn mode_text(settings: Option<&Self>) -> &'static str {
        match settings {
            Some(settings) if settings.deposit_mode() => "deposit",
            Some(_) => "fee",
            None => "unconfigured",
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
        Ok(u64::from_le_bytes(self.take(8)?.try_into().expect("eight bytes")))
    }

    fn u128(&mut self) -> Result<u128, String> {
        Ok(u128::from_le_bytes(self.take(16)?.try_into().expect("16 bytes")))
    }

    fn u32(&mut self) -> Result<usize, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("four bytes")) as usize)
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
    pub fn u8(&mut self, value: u8) {
        self.0.push(value);
    }

    pub fn u32(&mut self, value: u32) {
        self.0.extend(value.to_le_bytes());
    }

    pub fn u64(&mut self, value: u64) {
        self.0.extend(value.to_le_bytes());
    }

    pub fn u128(&mut self, value: u128) {
        self.0.extend(value.to_le_bytes());
    }

    pub fn bool(&mut self, value: bool) {
        self.0.push(u8::from(value));
    }

    pub fn bytes(&mut self, bytes: &[u8]) {
        self.u32(bytes.len() as u32);
        self.0.extend(bytes);
    }

    pub fn string(&mut self, text: &str) {
        self.bytes(text.as_bytes());
    }

    pub fn optional(&mut self, value: Option<&[u8; 32]>) {
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
        writer.u32(self.texts.len() as u32);
        for (key, value) in &self.texts {
            writer.string(key);
            writer.string(value);
        }
        let padded = writer.0.len().div_ceil(PROFILE_PADDING) * PROFILE_PADDING;
        writer.0.resize(padded, 0);
        writer.0
    }

    /// How many bytes the sealed profile takes in its record.
    pub fn sealed_len(&self) -> usize {
        NONCE_LEN + self.encode().len() + TAG_LEN
    }

    /// Seals the profile under the name's key.
    pub fn seal(&self, key: &[u8; 32]) -> Result<Vec<u8>, String> {
        seal(key, &self.encode(), RECORD_SEAL, fresh_nonce()?)
    }

    /// Opens a record's sealed bytes as the profile of `name`. Empty bytes,
    /// bytes this key does not open, and a profile of another name all mean
    /// the same thing: the name has nothing set.
    pub fn open(key: &[u8; 32], sealed: &[u8], name: &str) -> Self {
        // The program never stores more; a reply that has more didn't come from it.
        if sealed.len() > MAX_SEALED_RECORD_LEN {
            return Self::bare(name);
        }
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
    if sealed.len() > MAX_SEALED_PAGE_LEN {
        return None;
    }
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

pub fn fresh_bytes<const N: usize>() -> Result<[u8; N], String> {
    let mut bytes = [0_u8; N];
    getrandom::getrandom(&mut bytes).map_err(|_| "this machine gave no random bytes".to_owned())?;
    Ok(bytes)
}

fn fresh_nonce() -> Result<[u8; NONCE_LEN], String> {
    fresh_bytes()
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
    if sealed.len() < NONCE_LEN + TAG_LEN {
        return None;
    }
    let (nonce, ciphertext) = sealed.split_at(NONCE_LEN);
    Aes256Gcm::new_from_slice(key)
        .expect("a 32-byte key")
        .decrypt(Nonce::from_slice(nonce), Payload { msg: ciphertext, aad: associated })
        .ok()
}

/// A registered name: the tag of who holds it, until when, and its profile opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// The holder's [`owner_tag`] for this name; the chain stores no account.
    pub owner: OwnerTag,
    /// When the name lapses, in milliseconds since the Unix epoch, or
    /// [`NEVER`] for a name held by a deposit.
    pub expires: u64,
    pub profile: Profile,
    /// Carried over from the earlier chain, so it can't be claimed again.
    pub claimed: bool,
}

impl Resolved {
    /// Decodes the on-chain record `{version, owner, expires, sealed, claimed}` and opens it.
    fn decode(bytes: &[u8], key: &[u8; 32], name: &str) -> Result<Self, String> {
        let mut reader = Reader { bytes };
        let version = reader.u8()?;
        let owner = reader.account()?;
        let expires = reader.u64()?;
        let sealed = reader.byte_vec()?;
        // Version 5 records (the LEZ 0.3 testnet) end at `sealed`: nothing was claimed there.
        let claimed = version >= 6 && reader.u8()? != 0;
        Ok(Self { owner, expires, profile: Profile::open(key, sealed, name), claimed })
    }

    /// Whether the name is held by a deposit, and so never lapses.
    pub fn never_expires(&self) -> bool {
        self.expires == NEVER
    }

    /// Whether `account` holds the name `id` this record is for.
    pub fn is_held_by(&self, id: &NameId, account: &AccountId) -> bool {
        self.owner == owner_tag(id, account)
    }

    /// Whether the name has expired and is in its grace period. A name held by
    /// a deposit never is.
    pub fn lapsed(&self, now: u64) -> bool {
        !self.never_expires() && self.expires <= now
    }

    /// Whether the name is past its grace period, and so counts as unregistered.
    /// A name held by a deposit never is.
    pub fn gone(&self, now: u64) -> bool {
        !self.never_expires() && now >= self.expires.saturating_add(GRACE_MS)
    }

    /// The expiry as JSON: null for a name that never expires.
    pub fn expires_json(&self) -> Value {
        if self.never_expires() {
            Value::Null
        } else {
            Value::from(self.expires)
        }
    }

    /// The name in the JSON shape SPEC.md gives.
    /// The name this one forwards to (SPEC.md §7, Forwarding): its `forward` entry, if
    /// that is a valid name other than this one. A client follows it one hop.
    pub fn forward_target(&self) -> Option<String> {
        let (_, value) = self.profile.texts.iter().find(|(key, _)| key == FORWARD_KEY)?;
        parse_name(value).ok().filter(|target| *target != self.profile.name)
    }

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
            "owner_tag": hex(&self.owner),
            "expires": self.expires_json(),
            "never": self.never_expires(),
            "lapsed": self.lapsed(local_now()),
            "claimed": self.claimed,
            "account": profile.account.as_ref().map(account_text),
            "npk": profile.npk.as_ref().map(|npk| hex(npk)),
            "vpk": (!profile.vpk.is_empty()).then(|| hex(&profile.vpk)),
            "texts": texts,
            "forward": self.forward_target(),
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

/// The program whose state on an account is its balance, as 16 little-endian bytes.
pub const NATIVE_TOKEN: AccountId = [0; 32];

/// The state of one actor as a sequencer's `getAccountView` reports it: a list of
/// bytes under the program's ID, or nothing.
fn actor_bytes(account: &Value, program: &AccountId) -> Result<Vec<u8>, String> {
    // A LEZ 0.3 sequencer (the public testnet until it moves to 0.4) answers the same call
    // with every program's shard under `shards`, in the same byte-list form.
    let data = &account["data"];
    let id = account_text(program);
    match data["actor_states"][&id].as_array().or_else(|| data["shards"][&id].as_array()) {
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

/// An account's native balance, from its state; 0 when it has none.
fn native_balance(state: &[u8]) -> u128 {
    <[u8; 16]>::try_from(state).map_or(0, u128::from_le_bytes)
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

/// The Logos Blockchain channel LEZ takes deposits on, as the sequencer names it
/// (`getChannelId`): 64 hex characters.
#[cfg(not(test))]
pub fn channel_id(sequencer: &str) -> Result<String, String> {
    let response = ureq::post(sequencer)
        .timeout(std::time::Duration::from_secs(15))
        .send_json(json!({ "jsonrpc": "2.0", "id": 1, "method": "getChannelId", "params": {} }))
        .map_err(|error| format!("could not reach LEZ to ask where deposits go: {error}"))?;
    let reply: Value = response.into_json().map_err(|_| "LEZ's reply about deposits was unreadable".to_owned())?;
    reply["result"]
        .as_str()
        .filter(|id| id.len() == 64 && id.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| "LEZ didn't say where deposits go. Try again later.".to_owned())
}

/// Whether the sequencer at `sequencer` answers honestly. An older CLI (the October
/// testnet build) prints a public balance of 0 and every name as "no longer held by this
/// wallet" when it can't read the chain, and exits 0, so its 0 and its empty list are
/// believed only when the sequencer itself answers `getLastBlockId` with a block number
/// right now. A 503 from a proxy, a 200 page of HTML, an empty or error reply are no
/// answer (balance-marks-written-from-503-false-arrivals).
#[cfg(not(test))]
pub fn sequencer_reachable(sequencer: &str) -> Result<(), String> {
    probe_sequencer(sequencer)
}

/// Asks `sequencer` for its last block, and is Ok only for a JSON-RPC reply carrying one.
pub fn probe_sequencer(sequencer: &str) -> Result<(), String> {
    let unreachable = |why: String| format!("could not reach the sequencer: {why}");
    let response = ureq::post(sequencer)
        .timeout(std::time::Duration::from_secs(10))
        .send_json(json!({ "jsonrpc": "2.0", "id": 1, "method": "getLastBlockId", "params": [] }))
        .map_err(|error| unreachable(error.to_string()))?;
    let body = response.into_string().map_err(|error| unreachable(error.to_string()))?;
    last_block_answer(&body).map(|_| ()).map_err(unreachable)
}

/// The block number in a `getLastBlockId` reply, or why the reply isn't one.
pub fn last_block_answer(body: &str) -> Result<u64, String> {
    let reply: Value = serde_json::from_str(body.trim())
        .map_err(|_| "it sent a reply that isn't JSON-RPC".to_owned())?;
    if let Some(error) = reply.get("error").filter(|error| !error.is_null()) {
        let message = error["message"].as_str().map_or_else(|| error.to_string(), str::to_owned);
        return Err(format!("it answered with an error: {message}"));
    }
    reply["result"].as_u64().ok_or_else(|| "it sent no block number".to_owned())
}

/// An HTTP server on loopback that answers each of its first `connections` requests with
/// `reply` (status line, headers and body): a stand-in sequencer for tests.
#[cfg(test)]
pub fn stub_http(reply: &'static str, connections: usize) -> String {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("binds");
    let address = listener.local_addr().expect("address");
    std::thread::spawn(move || {
        for stream in listener.incoming().take(connections) {
            let Ok(mut stream) = stream else { continue };
            // The whole request is read first: closing on unread bytes resets the
            // connection, and the client would see that instead of the reply.
            let mut request = Vec::new();
            let mut buffer = [0_u8; 4096];
            loop {
                let Ok(read) = stream.read(&mut buffer) else { break };
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
                let text = String::from_utf8_lossy(&request).to_ascii_lowercase();
                if let Some(end) = text.find("\r\n\r\n") {
                    let length = text.lines().find_map(|line| line.strip_prefix("content-length:"))
                        .and_then(|value| value.trim().parse::<usize>().ok()).unwrap_or(0);
                    if request.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            let _ = stream.write_all(reply.as_bytes());
            let _ = stream.flush();
        }
    });
    format!("http://{address}/")
}

/// A `200 OK` HTTP reply carrying `body` as JSON, for `stub_http`.
#[cfg(test)]
pub fn json_reply(body: &str) -> &'static str {
    Box::leak(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).into_boxed_str())
}

#[cfg(test)]
thread_local! {
    /// The name records a test's registry holds, by name identifier.
    pub static RECORDS: std::cell::RefCell<Vec<(NameId, Resolved)>> = const { std::cell::RefCell::new(Vec::new()) };
}

#[cfg(test)]
thread_local! {
    /// Whether a test's sequencer is down.
    pub static SEQUENCER_DOWN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Tests say whether the sequencer answers: never the network.
#[cfg(test)]
pub fn sequencer_reachable(sequencer: &str) -> Result<(), String> {
    if SEQUENCER_DOWN.with(std::cell::Cell::get) {
        Err(format!("could not reach the sequencer: {sequencer}: Connection Failed"))
    } else {
        Ok(())
    }
}

/// Tests know the channel: a fixed one, never the network.
#[cfg(test)]
pub fn channel_id(_sequencer: &str) -> Result<String, String> {
    Ok("02".repeat(32))
}

/// Whether the sequencer at `sequencer` has ever seen `account`: `getAccountView`
/// answers null for an account no transaction has touched. LEZ can't pay such an
/// account of another wallet by its ID (the send lands, keeps its fee and credits
/// nothing), and a hand-over to one is most likely to a mistyped ID.
#[cfg(not(test))]
pub fn account_seen(sequencer: &str, account: &AccountId) -> Result<bool, String> {
    let response = ureq::post(sequencer)
        .timeout(std::time::Duration::from_secs(15))
        .send_json(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getAccountView",
            "params": [{ "account_id": account_text(account), "program_account_id": account_text(&NATIVE_TOKEN) }],
        }))
        .map_err(|error| format!("could not reach the sequencer: {error}"))?;
    let reply: Value = response.into_json().map_err(|_| "the sequencer sent an unreadable reply".to_owned())?;
    if let Some(error) = reply.get("error") {
        return Err(format!("the sequencer refused the read: {error}"));
    }
    match reply.get("result") {
        Some(Value::Null) => Ok(false),
        Some(_) => Ok(true),
        None => Err("the sequencer sent an unreadable reply".to_owned()),
    }
}

/// What the public account `account` holds, read from the sequencer at `sequencer`:
/// any account, not only the wallet's.
#[cfg(not(test))]
pub fn public_balance(sequencer: &str, account: &AccountId) -> Result<u128, String> {
    Registry { sequencer: sequencer.to_owned(), program: NATIVE_TOKEN }.balance(account)
}

/// Tests say what an account holds: never the network.
#[cfg(test)]
pub fn public_balance(_sequencer: &str, account: &AccountId) -> Result<u128, String> {
    BALANCES.with(|held| held.borrow().iter().find(|(id, _)| id == account).map(|(_, balance)| *balance)).ok_or_else(|| "not reached".to_owned())
}

#[cfg(test)]
thread_local! {
    /// What the accounts of a test's sequencer hold.
    pub static BALANCES: std::cell::RefCell<Vec<(AccountId, u128)>> = const { std::cell::RefCell::new(Vec::new()) };
}

#[cfg(test)]
thread_local! {
    /// The accounts a test's sequencer has never seen.
    pub static UNSEEN: std::cell::RefCell<Vec<AccountId>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Tests say which accounts are unseen: never the network.
#[cfg(test)]
pub fn account_seen(_sequencer: &str, account: &AccountId) -> Result<bool, String> {
    Ok(!UNSEEN.with(|unseen| unseen.borrow().contains(account)))
}

/// A registry on one network: where to read it and which program it is.
#[derive(Debug, Clone)]
pub struct Registry {
    pub sequencer: String,
    pub program: AccountId,
}

impl Registry {
    /// `program`'s state on `account`; empty when it has none.
    fn actor_state(&self, account: &AccountId, program: &AccountId) -> Result<Vec<u8>, String> {
        let response = ureq::post(&self.sequencer)
            .timeout(std::time::Duration::from_secs(15))
            .send_json(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "getAccountView",
                "params": [{ "account_id": account_text(account), "program_account_id": account_text(program) }],
            }))
            .map_err(|error| format!("could not reach the sequencer: {error}"))?;
        // Read at most MAX_REPLY_LEN: a sequencer that sends more is refused, not believed.
        let mut body = Vec::new();
        std::io::Read::read_to_end(&mut std::io::Read::take(response.into_reader(), MAX_REPLY_LEN as u64 + 1), &mut body)
            .map_err(|error| format!("the sequencer's reply broke off: {error}"))?;
        if body.len() > MAX_REPLY_LEN {
            return Err("the sequencer sent a reply larger than any account".to_owned());
        }
        let reply: Value =
            serde_json::from_slice(&body).map_err(|error| format!("the sequencer sent an unreadable reply: {error}"))?;
        if let Some(error) = reply.get("error") {
            return Err(format!("the sequencer refused the read: {error}"));
        }
        actor_bytes(&reply["result"], program)
    }

    /// The registry's state on `account`; empty when the account holds none.
    fn state(&self, account: &AccountId) -> Result<Vec<u8>, String> {
        self.actor_state(account, &self.program)
    }

    /// The native balance of `account`.
    pub fn balance(&self, account: &AccountId) -> Result<u128, String> {
        Ok(native_balance(&self.actor_state(account, &NATIVE_TOKEN)?))
    }

    /// What the name `id`'s deposit account holds: the deposit a release refunds.
    pub fn deposit(&self, id: &NameId) -> Result<u128, String> {
        self.balance(&deposit_address(&self.program, id))
    }

    /// What a wallet shows for one of its accounts: its balance, and the name
    /// it is shown as if it has a confirmed one. The balance is text because it
    /// is a 128-bit number.
    pub fn account_summary(&self, account: &AccountId) -> Result<Value, String> {
        let balance = self.balance(account)?;
        let claimed = self.state(account)?;
        let name = if claimed.is_empty() { None } else { self.confirmed(account, &claimed)? };
        Ok(json!({
            "account": account_text(account),
            "balance": balance.to_string(),
            "funded": balance > 0,
            "name": name,
        }))
    }

    /// Resolves `name`, whose identifier and key the caller has derived. A
    /// fee-mode name past its grace period is not registered, whatever its
    /// record holds; a name held by a deposit is registered until released.
    pub fn resolve(&self, name: &str, derived: &Derived) -> Result<Option<Resolved>, String> {
        Ok(self.record(name, derived)?.filter(|resolved| !resolved.gone(local_now())))
    }

    /// The record of `name` exactly as the chain holds it, lapsed or not.
    pub fn record(&self, name: &str, derived: &Derived) -> Result<Option<Resolved>, String> {
        #[cfg(test)]
        if let Some(found) = RECORDS.with(|records| records.borrow().iter().find(|(id, _)| *id == derived.id).map(|(_, record)| record.clone())) {
            return Ok(Some(found));
        }
        let bytes = self.state(&record_address(&self.program, &derived.id))?;
        if bytes.is_empty() {
            return Ok(None);
        }
        Resolved::decode(&bytes, &derived.key, name).map(Some)
    }

    /// When the commitment `promised` was made, as its account records it
    /// (`{version, made}`), or `None` if there is none: it never landed, or a
    /// reveal used it up.
    pub fn commitment_made(&self, promised: &[u8; 32]) -> Result<Option<u64>, String> {
        let bytes = self.state(&commitment_address(&self.program, promised))?;
        if bytes.is_empty() {
            return Ok(None);
        }
        let mut reader = Reader { bytes: &bytes };
        let _version = reader.u8()?;
        reader.u64().map(Some)
    }

    /// The registry's settings, or `None` while it has none and charges nothing.
    pub fn settings(&self) -> Result<Option<Settings>, String> {
        let bytes = self.state(&settings_address(&self.program))?;
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
        self.actor_state(CLOCK_ACCOUNT, &clock_program())?
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
        let bytes = self.state(&page_address(&self.program, &derived.id))?;
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
        let bytes = self.state(account)?;
        if bytes.is_empty() {
            return Ok(None);
        }
        self.confirmed(account, &bytes)
    }

    /// The name claimed in `reverse_record`, if its own record agrees: the
    /// account holds the name (its owner tag matches) or the name publishes it.
    fn confirmed(&self, account: &AccountId, reverse_record: &[u8]) -> Result<Option<String>, String> {
        let claimed = decode_primary(reverse_record)?;
        if !is_valid_name(&claimed) {
            return Ok(None);
        }
        let derived = derive(&claimed);
        let confirmed = self.resolve(&claimed, &derived)?.is_some_and(|resolved| {
            resolved.is_held_by(&derived.id, account) || resolved.profile.account == Some(*account)
        });
        Ok(confirmed.then_some(claimed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROGRAM: AccountId = [7; 32];

    /// A proxy in front of a sequencer that is down answers 503, or a 200 page of HTML,
    /// or nothing: none of those is the sequencer answering, so a CLI's 0 balances and
    /// empty name list read meanwhile aren't believed (balance-marks-written-from-503-false-arrivals,
    /// mine-balance-accounts-believe-degraded-sequencer-503-and-200-garbage).
    #[test]
    fn only_a_block_number_is_a_sequencer_answer() {
        assert_eq!(probe_sequencer(&stub_http(json_reply(r#"{"jsonrpc":"2.0","id":1,"result":79401}"#), 1)), Ok(()));
        let unavailable = "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        let problem = probe_sequencer(&stub_http(unavailable, 1)).expect_err("a 503 is no answer");
        assert!(problem.starts_with("could not reach the sequencer"), "{problem}");
        let html = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: 37\r\nConnection: close\r\n\r\n<html><body>Bad gateway</body></html>";
        let problem = probe_sequencer(&stub_http(html, 1)).expect_err("HTML is no answer");
        assert!(problem.contains("isn't JSON-RPC"), "{problem}");
        let empty = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        assert!(probe_sequencer(&stub_http(empty, 1)).is_err());
        assert!(last_block_answer(r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found"}}"#).is_err());
        assert!(last_block_answer(r#"{"jsonrpc":"2.0","id":1,"result":null}"#).is_err());
        assert_eq!(last_block_answer(r#"{"jsonrpc":"2.0","id":1,"result":5}"#), Ok(5));
    }

    /// The LEZ 0.3 testnet: its sequencer answers with `shards`, its records are
    /// version 5 (no `claimed`), its settings have no claims list. Read the same way.
    #[test]
    fn a_lez_0_3_registry_reads_as_well() {
        let program = [7_u8; 32];
        let reply = json!({ "nonce": 0, "data": { "shards": { account_text(&program): [5, 6], account_text(&[0; 32]): [1] } } });
        assert_eq!(actor_bytes(&reply, &program).expect("reads"), vec![5, 6]);
        assert_eq!(actor_bytes(&json!({ "data": { "shards": {} } }), &program).expect("reads"), Vec::<u8>::new());
        let v04 = json!({ "data": { "actor_states": { account_text(&program): [9] } } });
        assert_eq!(actor_bytes(&v04, &program).expect("reads"), vec![9]);

        let v5 = unhex(&format!("05{}{}01000000ab", "01".repeat(32), "ff".repeat(8))).expect("hex");
        let record = Resolved::decode(&v5, &[0; 32], "jefdiesel").expect("a v5 record decodes");
        assert_eq!((record.owner, record.expires, record.claimed), ([1; 32], u64::MAX, false));

        let live = unhex(&format!(
            "05{admin}{admin}{}01{}{}",
            "00".repeat(48),
            "00e1f505000000000000000000000000",
            "00943577000000000000000000000000",
            admin = "b6b80d9b12a1f9e36d60476dd4e41b4a2d5412671592f7447827728f83557808",
        ))
        .expect("hex");
        let settings = Settings::decode(&live).expect("the live v5 settings decode");
        assert_eq!(settings.deposits, [100_000_000, 2_000_000_000]);
        assert!(!settings.claims_open(0));
    }

    /// SPEC.md §7, Forwarding: a valid other name, normalised, shown in `resolve`.
    #[test]
    fn a_record_says_where_it_forwards() {
        let with = |value: &str| Resolved {
            owner: [1; 32],
            expires: NEVER,
            profile: Profile { name: "nameswallet".into(), texts: vec![("forward".into(), value.into())], ..Profile::default() },
            claimed: false,
        };
        assert_eq!(with("names").forward_target().as_deref(), Some("names"));
        assert_eq!(with(" @Names ").forward_target().as_deref(), Some("names"));
        assert_eq!(with("nameswallet").forward_target(), None, "not to itself");
        assert_eq!(with("not a name").forward_target(), None);
        assert_eq!(with("names").to_json()["forward"], "names");
        assert_eq!(Resolved { profile: sample_profile(), ..with("x") }.to_json()["forward"], Value::Null);
    }

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
    /// SPEC.md section 10, also asserted by `seal`'s tests.
    const SEALED_PROFILE: &str = "\
        000102030405060708090a0b7a39f7ff67b20894263a28d040c0f2711c04ba247fcf5cb107e27b59\
        05b8a0664b28cd25512addcb74af3efa5a8d6eb3bbb8e89fb8dba0aa2739252efed2be01ab019998\
        a07d416c788d8c9b699891e85bedd1118da7089377220bdb0ab8ae29661bf20e006c5fbffb222237\
        2ea0a2589ed49023186a77e9afee1af338f4a2377afa40da018789ca715318d981b556f71f9640eb\
        d056d8dcbe2b71845b45a578721ccc2c889df37b926c7da5a5202dbf3d50dc04883673134ce0772f\
        bcd7fbc8f31f455bb39b3d867d5e6772c86e59f42a9d04ffaacaa6119b1156d238665c0cf50028a9\
        cbf8b3189fc45a15a724c5775a835a8ccdff6a573d8e046d78e847b7da5668bc8529aedb3bdc70a9\
        9db9b9b4";
    const SEALED_PAGE: &str = "\
        000102030405060708090a0b7339f7ff79b216e10b4846b32bacf2731e06d04f8cf43af535da58e9\
        92f033cb16a471d0";

    /// SPEC.md section 10's derivations, shared with `seal` and the JavaScript resolver.
    #[test]
    fn a_name_derives_the_identifier_and_key_the_specification_gives() {
        for (name, master_hex, id, key) in [
            (
                "jef",
                "60e3df20e1c473ab71c666b913af96619fa14b49593e92598bd7709582e5a74e",
                "7041d9e447f06bdfe327d870402b1524cfe38bd03445b8fc3d0e87bc978eb763",
                "aad91a1a3dd0fb56a213212fbea58fe25d9139db42fc59ce2116c3cfa3440088",
            ),
            (
                "a-b-c",
                "c098ecc6c6af9cfcf449666ce012e3e930e076d870cb135ed8f1f058e7728362",
                "707decf4fde6d9fb61d255c23b34e042c846a0f4b0834151dcb2c1ef4160e849",
                "e3666a8572f7cabe94f6717491bff34c70eeca216436a142185bd8a86b6ccf31",
            ),
        ] {
            assert_eq!(hex(&master(name)), master_hex, "{name}");
            let derived = derive(name);
            assert_eq!(hex(&derived.id), id, "{name}");
            assert_eq!(hex(&derived.key), key, "{name}");
        }
        let jef = hex(&derive("jef").id);
        assert!(jef.starts_with("7041d9e4") && jef.ends_with("b763"));
    }

    /// Addresses as `core` computes them (SPEC.md section 10, and the rest from
    /// `names_core` itself).
    #[test]
    fn a_name_that_is_also_an_account_id_is_refused() {
        for typed in ["Jef.LEZ", "jef", "@jef", "lns:jef", " LNS:@Jef.lez "] {
            assert_eq!(parse_name(typed).unwrap(), "jef", "{typed}");
        }
        assert!(parse_name(&"1".repeat(32)).unwrap_err().contains("account ID"));
        assert!(parse_name(&"1".repeat(31)).is_ok());
    }

    #[test]
    fn addresses_match_core() {
        let jef = derive("jef").id;
        assert_eq!(account_text(&PROGRAM), "US517G5965aydkZ46HS38QLi7UQiSojurfbQfKCELFx");
        assert_eq!(account_text(&record_address(&PROGRAM, &jef)), "5b5xVGob2GYvrq21MSuxm1MfieucDEy9yxhDwdXndbpD");
        assert_eq!(account_text(&page_address(&PROGRAM, &jef)), "HEES9BpNxrAtwxFkyEQTEnjUcSr7usgbefeqiYJsG9mY");
        assert_eq!(account_text(&deposit_address(&PROGRAM, &jef)), "46Qu3gAftVLaXXV2armT1NkWXNQd7KuLLHx5Gqu2FMrc");
        assert_eq!(account_text(&settings_address(&PROGRAM)), "2aXkMrPG771Scj8LWBMyUGjJRou8Ls9kotSbnBfvoVfV");
        let tag = owner_tag(&jef, &[1; 32]);
        assert_eq!(hex(&tag), "f69d7a63559368fd63f7194d5d642d6f8ee3f3d190c784b079e76dfea828fba1");
        let promised = commitment(&jef, &tag, &[5; 32]);
        assert_eq!(hex(&promised), "1d84f6975de22294cad92f3b75056aacdd917c2887205e46132b400f4a24caa3");
        assert_eq!(account_text(&commitment_address(&PROGRAM, &promised)), "8unwVs8Lo7JUNUDraBvPTXKEszrXZKaKcsZjg8C9cg8D");

        let id: [u8; 32] = core::array::from_fn(|index| index as u8);
        assert_eq!(account_text(&record_address(&PROGRAM, &id)), "7bwwvMLtxqqNYiXsd5bYaq8e31E1odYf6jAK5z7tmUdd");
        assert_eq!(account_text(&page_address(&PROGRAM, &id)), "8M4vmdxCXL4eS8QXiHSdbeUUPt5Vbx1UVjLcv2cPk9Y5");
    }

    #[test]
    fn sealing_matches_the_specification_and_opens_again() {
        let key = derive("jef").key;
        assert_eq!(hex(&key), "aad91a1a3dd0fb56a213212fbea58fe25d9139db42fc59ce2116c3cfa3440088");
        let sealed = seal(&key, &sample_profile().encode(), RECORD_SEAL, NONCE).expect("seals");
        assert_eq!(hex(&sealed), SEALED_PROFILE);
        assert_eq!(sealed.len(), sample_profile().sealed_len());
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

    /// A record as `core` encodes `NameRecord { version: 6, owner: [1; 32],
    /// expires: u64::MAX, sealed: [0xab], claimed: false }`.
    #[test]
    fn a_record_held_by_a_deposit_never_lapses() {
        let bytes = unhex(&format!("06{}{}01000000ab00", "01".repeat(32), "ff".repeat(8))).expect("hex");
        let resolved = Resolved::decode(&bytes, &[0; 32], "jef").expect("decodes");
        assert_eq!(resolved.owner, [1; 32]);
        assert_eq!(resolved.expires, NEVER);
        assert!(resolved.never_expires());
        assert!(!resolved.lapsed(u64::MAX) && !resolved.gone(u64::MAX));
        let json = resolved.to_json();
        assert_eq!(json["expires"], Value::Null);
        assert_eq!(json["never"], true);
        assert_eq!(json["lapsed"], false);
        assert_eq!(json["owner_tag"], "01".repeat(32));
    }

    #[test]
    fn a_fee_mode_record_decodes_lapses_and_reports_where_to_pay() {
        let derived = derive("jef");
        let key = derived.key;
        let profile = Profile { npk: Some([3; 32]), vpk: vec![9, 9, 9, 9], ..sample_profile() };
        let mut record = vec![6];
        record.extend(owner_tag(&derived.id, &[1; 32]));
        record.extend(4_102_444_800_000_u64.to_le_bytes()); // the start of 2100
        let sealed = profile.seal(&key).expect("seals");
        record.extend((sealed.len() as u32).to_le_bytes());
        record.extend(&sealed);
        record.push(1);

        let resolved = Resolved::decode(&record, &key, "jef").expect("decodes");
        assert!(resolved.claimed);
        assert!(resolved.is_held_by(&derived.id, &[1; 32]));
        assert!(!resolved.is_held_by(&derived.id, &[2; 32]));
        assert_eq!(resolved.expires, 4_102_444_800_000);
        assert_eq!(resolved.profile, profile);
        assert!(!resolved.lapsed(4_102_444_799_999) && resolved.lapsed(4_102_444_800_000));
        assert!(!resolved.gone(4_102_444_800_000 + GRACE_MS - 1) && resolved.gone(4_102_444_800_000 + GRACE_MS));
        assert_eq!(resolved.to_json()["expires"], 4_102_444_800_000_u64);
        assert_eq!(resolved.to_json()["never"], false);

        let recipient = resolved.recipient_json();
        assert_eq!(recipient["display"], "jef");
        assert_eq!(recipient["public"], account_text(&[2; 32]));
        assert_eq!(recipient["public_hex"], "02".repeat(32));
        assert_eq!(recipient["private"]["nullifier_public_key"], "03".repeat(32));
        assert_eq!(recipient["private"]["viewing_public_key"], "09090909");
        assert_eq!(resolved.to_json()["texts"]["chat"], "hi");

        assert!(Resolved::decode(&record[..20], &key, "jef").is_err(), "truncated");
    }

    /// core's encoding of `Settings { version: 6, admin: [1; 32], treasury: [2; 32],
    /// prices: [1, 2, 3], mode: 1, deposits: [100_000_000, 2_000_000_000], claims_root:
    /// [3; 32], claims_until: 7, claims_program: [4; 32] }`.
    #[test]
    fn settings_decode_as_core_writes_them() {
        let core = unhex(&format!(
            "06{}{}{}{}{}01{}{}{}{}{}",
            "01".repeat(32),
            "02".repeat(32),
            "01000000000000000000000000000000",
            "02000000000000000000000000000000",
            "03000000000000000000000000000000",
            "00e1f505000000000000000000000000",
            "00943577000000000000000000000000",
            "03".repeat(32),
            "0700000000000000",
            "04".repeat(32),
        ))
        .expect("hex");
        let settings = Settings::decode(&core).expect("decodes");
        assert_eq!(
            settings,
            Settings {
                admin: [1; 32],
                treasury: [2; 32],
                prices: [1, 2, 3],
                mode: MODE_DEPOSIT,
                deposits: [100_000_000, 2_000_000_000],
                claims_root: [3; 32],
                claims_until: 7,
                claims_program: [4; 32],
            }
        );
        assert!(settings.deposit_mode());
        assert_eq!(Settings::mode_text(Some(&settings)), "deposit");
        assert_eq!(Settings::mode_text(None), "unconfigured");
        assert!(Settings::decode(&core[..core.len() - 1]).is_err(), "truncated");
        assert!(settings.claims_open(6) && !settings.claims_open(7));
        assert!(!Settings { claims_root: [0; 32], ..settings }.claims_open(6), "no list, no claims");
        // The clock's address as the chain itself reports it.
        assert_eq!(account_text(&clock_program()), "2hwaRW8sMnyKLLRbDRw8k68tiv9bnfq98oxcdtcp157Z");
        assert_eq!(TERM_LENGTHS_MS[1], 31_536_000_000);
    }

    #[test]
    fn typed_names_and_accounts_are_normalised_and_checked() {
        for typed in ["jef", "jef.lez", "Jef.LEZ", "  jef.lez  "] {
            assert_eq!(parse_name(typed).as_deref(), Ok("jef"), "{typed}");
        }
        for bad in ["", ".lez", "jef.eth", "-jef", "jef-", "j f", &"a".repeat(33)] {
            assert!(parse_name(bad).is_err(), "{bad}");
        }
        let text = account_text(&PROGRAM);
        assert_eq!(parse_account(&text), Ok(PROGRAM));
        assert_eq!(parse_account(&format!("Public/{text}")), Ok(PROGRAM));
        assert!(parse_account("not-an-account").is_err());
    }

    /// The shape `getAccountView` answers in; `gateway/app-src/vectors/engine-report.json` has real ones.
    #[test]
    fn actor_state_is_read_by_program_and_hex_reads_back() {
        let mut balance = vec![5];
        balance.resize(16, 0);
        let account = json!({ "data": { "actor_states": {
            account_text(&NATIVE_TOKEN): balance,
            account_text(&PROGRAM): [1, 2, 3],
        } } });
        assert_eq!(account_text(&NATIVE_TOKEN), "1".repeat(32));
        assert_eq!(actor_bytes(&account, &PROGRAM), Ok(vec![1, 2, 3]));
        assert_eq!(actor_bytes(&account, &[9; 32]), Ok(Vec::new()));
        assert_eq!(actor_bytes(&Value::Null, &PROGRAM), Ok(Vec::new()), "an account not on the chain");
        assert_eq!(native_balance(&actor_bytes(&account, &NATIVE_TOKEN).expect("bytes")), 5);
        assert_eq!(native_balance(&[]), 0);

        assert_eq!(unhex("0x0aFF"), Ok(vec![10, 255]));
        assert_eq!(unhex(&hex(&[0, 7, 255])), Ok(vec![0, 7, 255]));
        assert!(unhex("abc").is_err());
        assert!(unhex("zz").is_err());

        let mut primary = vec![5];
        primary.extend(3_u32.to_le_bytes());
        primary.extend(b"jef");
        assert_eq!(decode_primary(&primary).as_deref(), Ok("jef"));
    }
}
