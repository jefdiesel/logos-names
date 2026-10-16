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
use argon2::{Algorithm, Argon2, Params, Version};
use borsh::{BorshDeserialize, BorshSerialize};
use names_core::{MAX_SEALED_PAGE_LEN, MAX_SEALED_RECORD_LEN, NameId};
use sha2::{Digest as _, Sha256};

/// One fixed salt for every network, on purpose: a name keeps its identifier and
/// key from testnet to mainnet.
const MASTER_SALT: &[u8] = b"logos-names/v5";
/// Argon2id memory in KiB (64 MiB), passes and lanes.
const MASTER_MEMORY_KIB: u32 = 65_536;
const MASTER_PASSES: u32 = 3;
const MASTER_LANES: u32 = 1;
const ID_DOMAIN: &[u8] = b"logos-names/v5/id";
const KEY_DOMAIN: &[u8] = b"logos-names/v5/key";
const RECORD_ASSOCIATED_DATA: &[u8] = b"logos-names/v5/record";
const PAGE_ASSOCIATED_DATA: &[u8] = b"logos-names/v5/page";

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
    let params = Params::new(MASTER_MEMORY_KIB, MASTER_PASSES, MASTER_LANES, Some(32))
        .expect("the Argon2 parameters are valid");
    argon2id(
        &Argon2::new(Algorithm::Argon2id, Version::V0x13, params),
        name.as_bytes(),
        MASTER_SALT,
    )
}

fn argon2id(argon2: &Argon2<'_>, password: &[u8], salt: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    argon2
        .hash_password_into(password, salt, &mut out)
        .expect("Argon2 accepts these inputs");
    out
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
    const KEY: &str = "aad91a1a3dd0fb56a213212fbea58fe25d9139db42fc59ce2116c3cfa3440088";
    /// (name, master, id, key), asserted in resolver/names.test.js too.
    const DERIVATIONS: [(&str, &str, &str, &str); 5] = [
        (
            "jef",
            "60e3df20e1c473ab71c666b913af96619fa14b49593e92598bd7709582e5a74e",
            "7041d9e447f06bdfe327d870402b1524cfe38bd03445b8fc3d0e87bc978eb763",
            "aad91a1a3dd0fb56a213212fbea58fe25d9139db42fc59ce2116c3cfa3440088",
        ),
        (
            "logos",
            "140f118f9f0c7a99f04e3214d8e084590cd69687be6effb7f57dcc1a06a35372",
            "4a8bf61b00dbc63907a935aa15ed534bb32d866978aa1f2128f9b6f37e0c4025",
            "d67dd628fa51fa62bd32ea82c3e9ff1b67f93b01103be956eaebc52f1699df44",
        ),
        (
            "a",
            "c42903b96a43d01c6c912b647c21902bffd17afdf932e5f8c407dea08a32b494",
            "24b73f0b70605c7f27fb91f9952b550e13c4cc38dcf325be45442d1a2cb085b4",
            "cd9ded7c94d52743db4ee67152766480be5bb5199e83b6696e502cf818933150",
        ),
        (
            "abcdefghijklmnopqrstuvwxyz012345",
            "a9296b3bfd39d63908326e19c2d1b20a27cb6b47df1829725c68355bc3585f5b",
            "0044f00be946536689d83e364ac25462c65c615d74059652d1f3e2977bd75403",
            "79c7aa8c06d4983a59b235915ae4f5767918824a9150d5d8b576761b7d3654f6",
        ),
        (
            "a-b-c",
            "c098ecc6c6af9cfcf449666ce012e3e930e076d870cb135ed8f1f058e7728362",
            "707decf4fde6d9fb61d255c23b34e042c846a0f4b0834151dcb2c1ef4160e849",
            "e3666a8572f7cabe94f6717491bff34c70eeca216436a142185bd8a86b6ccf31",
        ),
    ];
    const NONCE: [u8; NONCE_LEN] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
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
    fn derives_the_spec_vectors() {
        for (name, master_hex, id, key) in DERIVATIONS {
            assert_eq!(hex::encode(master(name)), master_hex, "{name}");
            let derived = derive(name);
            assert_eq!(hex::encode(derived.id), id, "{name}");
            assert_eq!(hex::encode(derived.key), key, "{name}");
        }
    }

    /// RFC 9106 section 5.3 (Argon2id, version 0x13), which uses a secret and
    /// associated data. It checks the Argon2id implementation itself, so a
    /// mix-up of variant, version or parameters cannot pass silently.
    #[test]
    fn argon2id_matches_rfc_9106() {
        let params = argon2::ParamsBuilder::new()
            .m_cost(32)
            .t_cost(3)
            .p_cost(4)
            .data(argon2::AssociatedData::new(&[4; 12]).unwrap())
            .output_len(32)
            .build()
            .unwrap();
        let argon2 =
            Argon2::new_with_secret(&[3; 8], Algorithm::Argon2id, Version::V0x13, params).unwrap();
        assert_eq!(
            hex::encode(argon2id(&argon2, &[1; 32], &[2; 16])),
            "0d640df58d78766c08c037a34a8b53c9d01ef0452d75b65eb52520e96b01e659"
        );
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
