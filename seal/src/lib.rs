//! What a Logos Names client adds to the registry program: turning a name into
//! the identifier the chain knows it by, and sealing and opening what the name
//! resolves to. See `SPEC.md`, sections 3 and 5.
//!
//! None of this runs on the chain. The program stores sealed bytes it cannot
//! read, so the checks on what goes inside them are made here.

use aes_gcm::{
    Aes256Gcm, Key, Nonce,
    aead::{Aead as _, KeyInit as _, Payload},
};
use borsh::{BorshDeserialize, BorshSerialize};
use names_core::{MAX_SEALED_PAGE_LEN, MAX_SEALED_RECORD_LEN, NameId};
use sha2::{Digest as _, Sha256};

const MASTER_SALT: &[u8] = b"logos-names/v2";
/// The most a Cloudflare Worker's Web Crypto allows, so a gateway can derive too.
const MASTER_ITERATIONS: u32 = 100_000;
const ID_DOMAIN: &[u8] = b"logos-names/v2/id";
const KEY_DOMAIN: &[u8] = b"logos-names/v2/key";
const RECORD_ASSOCIATED_DATA: &[u8] = b"logos-names/v2/record";
const PAGE_ASSOCIATED_DATA: &[u8] = b"logos-names/v2/page";

pub const NONCE_LEN: usize = 12;
const TAG_LEN: usize = 16;
/// A sealed profile's plaintext is zero-padded to a multiple of this, so that
/// its length says little about what it holds.
pub const PROFILE_PADDING: usize = 256;

pub const MAX_TEXTS: usize = 16;
pub const MAX_TEXT_KEY_LEN: usize = 32;
pub const MAX_TEXT_VALUE_LEN: usize = 512;
/// Length of an ML-KEM-768 viewing public key.
pub const VPK_LEN: usize = 1184;
pub const MAX_CONTENT_TYPE_LEN: usize = 64;

/// What a name derives to: `id` is public, `key` never leaves the client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Derived {
    pub id: NameId,
    pub key: [u8; 32],
}

/// What a name resolves to, sealed inside its record.
#[derive(Debug, Clone, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Profile {
    /// The name itself, so a reader can confirm the record is this name's.
    pub name: String,
    /// Public account the name can be paid at.
    pub account: Option<[u8; 32]>,
    /// Nullifier public key, for private payments.
    pub npk: Option<[u8; 32]>,
    /// Viewing public key: empty, or [`VPK_LEN`] bytes.
    pub vpk: Vec<u8>,
    pub texts: Vec<TextRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TextRecord {
    pub key: String,
    pub value: String,
}

/// A name's page, sealed inside its page account.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PageContent {
    pub content_type: String,
    pub body: Vec<u8>,
}

/// Derives the identifier and key of `name`, which must already be normalised.
/// This is slow on purpose: it is what makes scanning for names expensive.
#[must_use]
pub fn derive(name: &str) -> Derived {
    let master = master(name);
    Derived {
        id: domain_hash(ID_DOMAIN, &master),
        key: domain_hash(KEY_DOMAIN, &master),
    }
}

fn master(name: &str) -> [u8; 32] {
    pbkdf2::pbkdf2_hmac_array::<Sha256, 32>(name.as_bytes(), MASTER_SALT, MASTER_ITERATIONS)
}

fn domain_hash(domain: &[u8], master: &[u8; 32]) -> [u8; 32] {
    Sha256::new()
        .chain_update(domain)
        .chain_update(master)
        .finalize()
        .into()
}

pub fn validate_text_key(key: &str) -> Result<(), &'static str> {
    let charset_ok = key
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_' | b'.'));
    if key.is_empty() || key.len() > MAX_TEXT_KEY_LEN || !charset_ok {
        return Err("text key must be 1-32 chars of a-z, 0-9, '-', '_' or '.'");
    }
    Ok(())
}

/// Checks a profile against the limits every reader expects. The chain cannot
/// make these checks, so a client makes them before sealing.
pub fn validate_profile(profile: &Profile) -> Result<(), &'static str> {
    if !profile.vpk.is_empty() && profile.vpk.len() != VPK_LEN {
        return Err("viewing public key must be 1184 bytes");
    }
    if profile.texts.len() > MAX_TEXTS {
        return Err("a name holds at most 16 text entries");
    }
    for text in &profile.texts {
        validate_text_key(&text.key)?;
        if text.value.len() > MAX_TEXT_VALUE_LEN {
            return Err("text value must be at most 512 bytes");
        }
    }
    if NONCE_LEN + padded_len(to_bytes(profile).len()) + TAG_LEN > MAX_SEALED_RECORD_LEN {
        return Err("the sealed profile is too large for a record");
    }
    Ok(())
}

/// Checks a page against the content type rule and the room a page account has.
pub fn validate_page(page: &PageContent) -> Result<(), &'static str> {
    if page.content_type.is_empty() || page.content_type.len() > MAX_CONTENT_TYPE_LEN {
        return Err("content type must be 1-64 bytes");
    }
    if NONCE_LEN + to_bytes(page).len() + TAG_LEN > MAX_SEALED_PAGE_LEN {
        return Err("the sealed page is too large; a page holds about 80 KiB");
    }
    Ok(())
}

/// Seals `profile` under the name's key, with a fresh random nonce.
#[must_use]
pub fn seal_profile(key: &[u8; 32], profile: &Profile) -> Vec<u8> {
    seal_profile_with_nonce(key, &random_nonce(), profile)
}

/// Opens a record's `sealed`. `None` means nothing is set: the input is empty,
/// does not open with this key, or holds a profile for a name other than
/// `expected_name`.
#[must_use]
pub fn open_profile(key: &[u8; 32], sealed: &[u8], expected_name: &str) -> Option<Profile> {
    let plaintext = open(key, sealed, RECORD_ASSOCIATED_DATA)?;
    // Decoded from the start; the zero padding after the profile is ignored.
    let profile = Profile::deserialize(&mut plaintext.as_slice()).ok()?;
    (profile.name == expected_name).then_some(profile)
}

/// Seals `page` under the name's key, with a fresh random nonce.
#[must_use]
pub fn seal_page(key: &[u8; 32], page: &PageContent) -> Vec<u8> {
    seal_page_with_nonce(key, &random_nonce(), page)
}

/// Opens a page's `sealed`. `None` means there is no page a reader can use.
#[must_use]
pub fn open_page(key: &[u8; 32], sealed: &[u8]) -> Option<PageContent> {
    let plaintext = open(key, sealed, PAGE_ASSOCIATED_DATA)?;
    borsh::from_slice(&plaintext).ok()
}

/// [`seal_profile`] with a chosen nonce, to reproduce the specification's test
/// vectors. Reusing a nonce under one key breaks the seal; clients must not.
#[doc(hidden)]
#[must_use]
pub fn seal_profile_with_nonce(
    key: &[u8; 32],
    nonce: &[u8; NONCE_LEN],
    profile: &Profile,
) -> Vec<u8> {
    let mut plaintext = to_bytes(profile);
    plaintext.resize(padded_len(plaintext.len()), 0);
    seal(key, nonce, &plaintext, RECORD_ASSOCIATED_DATA)
}

/// [`seal_page`] with a chosen nonce, to reproduce the specification's test vectors.
#[doc(hidden)]
#[must_use]
pub fn seal_page_with_nonce(
    key: &[u8; 32],
    nonce: &[u8; NONCE_LEN],
    page: &PageContent,
) -> Vec<u8> {
    seal(key, nonce, &to_bytes(page), PAGE_ASSOCIATED_DATA)
}

fn padded_len(len: usize) -> usize {
    len.div_ceil(PROFILE_PADDING) * PROFILE_PADDING
}

fn random_nonce() -> [u8; NONCE_LEN] {
    let mut nonce = [0; NONCE_LEN];
    getrandom::fill(&mut nonce).expect("the system has a source of randomness");
    nonce
}

/// `nonce ‖ AES-256-GCM(key, nonce, plaintext, associated data)`, tag appended.
fn seal(key: &[u8; 32], nonce: &[u8; NONCE_LEN], plaintext: &[u8], aad: &[u8]) -> Vec<u8> {
    let ciphertext = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key))
        .encrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .expect("the plaintext is far below the AES-GCM length limit");
    [nonce.as_slice(), &ciphertext].concat()
}

fn open(key: &[u8; 32], sealed: &[u8], aad: &[u8]) -> Option<Vec<u8>> {
    if sealed.len() < NONCE_LEN + TAG_LEN {
        return None;
    }
    let (nonce, ciphertext) = sealed.split_at(NONCE_LEN);
    Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key))
        .decrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .ok()
}

fn to_bytes<T: BorshSerialize>(value: &T) -> Vec<u8> {
    borsh::to_vec(value).expect("borsh serialization to a Vec cannot fail")
}

#[cfg(test)]
mod tests {
    use super::*;

    // The vectors of SPEC.md, section 10.
    const MASTER: &str = "83a5b494ae6f1505c3a8abca5389bb60b0f614e9d1f0490858708dd2a2dff800";
    const ID: &str = "86d7e719e7acabe344ef8a69782b72ed0c21087e73e400e7056ab0daf7e6c618";
    const KEY: &str = "d19aa10c57273a3ff4f569cc3a9c35819b2a9bcac7ba8a48deb131269cb6e2e0";
    const NONCE: [u8; NONCE_LEN] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
    const SEALED_PROFILE: &str = "\
        000102030405060708090a0b67bf760994a19946abc799235b95e48bfb804c3ca05471c9261f5099\
        e1b00570e6139ab5bbb5fac423627e0fd18c5299475a5c4ed914f1e506f23b66d4d2377de959bff4\
        e8d4ab02b63865f9a2453fef24302c211eb70eadebf5b38038d080e2b6f3b1fa17a777b940f17eb9\
        4a83f4ce719552c92108600800f3d020a7d8fee73e2792c907db62125ba636607ed94ea46b73f046\
        981a751be920449222629d2561194416c993b4bfdeeec20026656f11b64f1c2ac368d64e57d07054\
        ac0fcdac4a6e83b60ac363901963488c93c7762d19b34b6991b9abdbc5d293fca06b69cab31e7eaa\
        769142940f4813c73713dbd34b24386fb53937e2c9915323f35d39bf50e63d4289cb48a7c7aff0e5\
        404987b0";
    const SEALED_PAGE: &str = "\
        000102030405060708090a0b6ebf76098aa1873386b5f74030f9e489f9822657c6ba930c224eb1f2\
        64767f7cf7b5a511";

    fn key() -> [u8; 32] {
        hex::decode(KEY).unwrap().try_into().unwrap()
    }

    fn profile() -> Profile {
        Profile {
            name: "jef".to_owned(),
            account: Some([2; 32]),
            npk: None,
            vpk: Vec::new(),
            texts: vec![text("chat", "hi")],
        }
    }

    fn text(key: &str, value: &str) -> TextRecord {
        TextRecord {
            key: key.to_owned(),
            value: value.to_owned(),
        }
    }

    fn page() -> PageContent {
        PageContent {
            content_type: "text/plain".to_owned(),
            body: b"hi".to_vec(),
        }
    }

    #[test]
    fn derives_the_spec_vector() {
        assert_eq!(hex::encode(master("jef")), MASTER);
        let derived = derive("jef");
        assert_eq!(hex::encode(derived.id), ID);
        assert_eq!(hex::encode(derived.key), KEY);
    }

    #[test]
    fn seals_the_spec_profile() {
        assert_eq!(to_bytes(&profile()).len(), 63);
        let sealed = seal_profile_with_nonce(&key(), &NONCE, &profile());
        assert_eq!(sealed.len(), NONCE_LEN + 256 + TAG_LEN);
        assert_eq!(hex::encode(sealed), SEALED_PROFILE);
    }

    #[test]
    fn seals_the_spec_page() {
        let sealed = seal_page_with_nonce(&key(), &NONCE, &page());
        assert_eq!(hex::encode(sealed), SEALED_PAGE);
    }

    #[test]
    fn opens_the_spec_vectors() {
        let sealed = hex::decode(SEALED_PROFILE).unwrap();
        assert_eq!(open_profile(&key(), &sealed, "jef"), Some(profile()));
        let sealed = hex::decode(SEALED_PAGE).unwrap();
        assert_eq!(open_page(&key(), &sealed), Some(page()));
    }

    #[test]
    fn a_tampered_byte_does_not_open() {
        let sealed = hex::decode(SEALED_PROFILE).unwrap();
        for index in [0, NONCE_LEN, sealed.len() - 1] {
            let mut tampered = sealed.clone();
            tampered[index] ^= 1;
            assert_eq!(open_profile(&key(), &tampered, "jef"), None, "byte {index}");
        }
        let mut tampered = hex::decode(SEALED_PAGE).unwrap();
        tampered[NONCE_LEN] ^= 1;
        assert_eq!(open_page(&key(), &tampered), None);
    }

    #[test]
    fn nothing_set_opens_as_none() {
        let sealed = hex::decode(SEALED_PROFILE).unwrap();
        assert_eq!(open_profile(&key(), &[], "jef"), None);
        assert_eq!(open_profile(&key(), &sealed[..20], "jef"), None);
        assert_eq!(open_profile(&[9; 32], &sealed, "jef"), None);
        // A profile sealed for one name is not accepted as another's.
        assert_eq!(open_profile(&key(), &sealed, "ada"), None);
        // A record is not a page: the associated data differs.
        assert_eq!(open_page(&key(), &sealed), None);
        assert_eq!(open_page(&key(), &[]), None);
    }

    #[test]
    fn sealing_uses_a_fresh_nonce_and_round_trips() {
        let first = seal_profile(&key(), &profile());
        let second = seal_profile(&key(), &profile());
        assert_ne!(first, second);
        assert_eq!(open_profile(&key(), &first, "jef"), Some(profile()));
        assert_eq!(open_page(&key(), &seal_page(&key(), &page())), Some(page()));
    }

    #[test]
    fn padding_hides_small_differences_in_length() {
        let mut longer = profile();
        longer
            .texts
            .push(text("agent", "https://example.org/agent"));
        assert_eq!(
            seal_profile(&key(), &profile()).len(),
            seal_profile(&key(), &longer).len()
        );
        assert_eq!(padded_len(0), 0);
        assert_eq!(padded_len(256), 256);
        assert_eq!(padded_len(257), 512);
    }

    #[test]
    fn profile_limits_are_checked() {
        assert_eq!(validate_profile(&profile()), Ok(()));

        let mut full = profile();
        full.npk = Some([3; 32]);
        full.vpk = vec![4; VPK_LEN];
        full.texts = (0..MAX_TEXTS)
            .map(|index| text(&format!("{index:0>32}"), &"v".repeat(MAX_TEXT_VALUE_LEN)))
            .collect();
        assert_eq!(validate_profile(&full), Ok(()));
        assert!(seal_profile(&key(), &full).len() <= MAX_SEALED_RECORD_LEN);

        let mut too_many = full.clone();
        too_many.texts.push(text("one-more", ""));
        assert!(validate_profile(&too_many).is_err());

        let mut bad_vpk = profile();
        bad_vpk.vpk = vec![4; 32];
        assert!(validate_profile(&bad_vpk).is_err());

        let mut long_value = profile();
        long_value.texts[0].value = "v".repeat(MAX_TEXT_VALUE_LEN + 1);
        assert!(validate_profile(&long_value).is_err());

        for key in ["chat", "my-app.setting_1", &"k".repeat(32)] {
            assert!(validate_text_key(key).is_ok(), "{key}");
        }
        for key in ["", "Chat", "a b", "caf\u{e9}", &"k".repeat(33)] {
            assert!(validate_text_key(key).is_err(), "{key}");
        }
    }

    #[test]
    fn page_limits_are_checked() {
        assert_eq!(validate_page(&page()), Ok(()));

        let mut page = page();
        page.content_type = String::new();
        assert!(validate_page(&page).is_err());
        page.content_type = "t".repeat(MAX_CONTENT_TYPE_LEN + 1);
        assert!(validate_page(&page).is_err());

        page.content_type = "text/plain".to_owned();
        page.body = vec![0; MAX_SEALED_PAGE_LEN - NONCE_LEN - TAG_LEN - 18];
        assert_eq!(validate_page(&page), Ok(()));
        assert_eq!(seal_page(&key(), &page).len(), MAX_SEALED_PAGE_LEN);
        page.body.push(0);
        assert!(validate_page(&page).is_err());
    }
}
