//! App secrets (Wallet Provider SDK, `app_secret`): key material the wallet derives for one
//! mini-app from one of the person's own accounts, so the app can keep secrets that come back
//! when the wallet is restored from its recovery phrase, without ever seeing a key.
//!
//! One file shared by the Basecamp module (which checks requests and keeps the grants) and the
//! names CLI (which holds the keys and runs the derivation), so both follow the same rules.
//!
//! ```text
//! ikm   = the account's own 32-byte secret: a public account's signing key, a private
//!         account's authorization secret key (both re-derived from the recovery phrase)
//! prk   = HMAC-SHA256(key = "logos-names/app-secret/v1", msg = ikm)              (HKDF-Extract)
//! info  = "logos-names/app-secret/v1/info" | u8 len(app) | app | u8 len(scope) | scope
//!         | u16be len(context) | context | account id (32) | u64be index
//! m     = HMAC-SHA256(key = prk, msg = info | 0x01)                              (HKDF-Expand, L = 32)
//! value = SHA256(tag | m)
//! ```
//!
//! `app` is the calling module's name as Basecamp gives it (`current_caller()`), never an
//! argument, so one app can't ask for another's values. `m` never leaves the wallet: an app
//! gets only `SHA256(tag | m)` for a non-empty tag it names. That is exactly the bridge's
//! two-level convention (`s` = `m` stays in the wallet; `k = SHA256("RFP022/ck" | s)` is what
//! the app gets and what its transactions carry), and it holds for any app: whatever an app
//! learns is a one-way image of a master value it can't recover.

use sha2::{Digest, Sha256};

/// HKDF-Extract salt.
pub const SALT: &[u8] = b"logos-names/app-secret/v1";
/// First bytes of every HKDF-Expand `info`.
pub const INFO_TAG: &[u8] = b"logos-names/app-secret/v1/info";
/// A scope's length in bytes.
pub const MAX_SCOPE: usize = 64;
/// A tag's length in bytes.
pub const MAX_TAG: usize = 32;
/// A context's length in bytes.
pub const MAX_CONTEXT: usize = 256;
/// Values one call returns.
pub const MAX_COUNT: u64 = 64;
/// An app's name, as the provider accepts it.
pub const MAX_APP: usize = 64;

/// HMAC-SHA256 (RFC 2104).
pub fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut block = [0_u8; 64];
    if key.len() > 64 {
        block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let mut inner = Sha256::new();
    inner.update(block.map(|byte| byte ^ 0x36));
    inner.update(message);
    let inner = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(block.map(|byte| byte ^ 0x5c));
    outer.update(inner);
    block.fill(0);
    outer.finalize().into()
}

/// A scope or a tag: printable ASCII without spaces, `1..=max` bytes.
fn label(text: &str, max: usize, what: &str) -> Result<(), String> {
    if text.is_empty() || text.len() > max {
        return Err(format!("{what} is 1 to {max} characters"));
    }
    if !text.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err(format!("{what} is printable ASCII without spaces"));
    }
    Ok(())
}

/// What one `app_secret` call asks for, checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ask {
    pub app: String,
    pub scope: String,
    pub context: Vec<u8>,
    pub tag: String,
    pub first: u64,
    pub count: u64,
}

impl Ask {
    pub fn new(app: &str, scope: &str, context: Vec<u8>, tag: &str, first: u64, count: u64) -> Result<Self, String> {
        if app.is_empty() || app.len() > MAX_APP || !app.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.')) {
            return Err("this app's name can't key a secret".to_owned());
        }
        label(scope, MAX_SCOPE, "a scope")?;
        label(tag, MAX_TAG, "a tag")?;
        if context.len() > MAX_CONTEXT {
            return Err(format!("a context is at most {MAX_CONTEXT} bytes"));
        }
        if count == 0 || count > MAX_COUNT {
            return Err(format!("ask for 1 to {MAX_COUNT} values at a time"));
        }
        if first.checked_add(count - 1).is_none() {
            return Err("the indexes run past 2^64 - 1".to_owned());
        }
        Ok(Self { app: app.to_owned(), scope: scope.to_owned(), context, tag: tag.to_owned(), first, count })
    }
}

/// HKDF-Expand `info` for `index` (see the module docs).
pub fn info(app: &str, scope: &str, context: &[u8], account: &[u8; 32], index: u64) -> Vec<u8> {
    let mut info = Vec::with_capacity(INFO_TAG.len() + 2 + app.len() + scope.len() + 2 + context.len() + 40);
    info.extend_from_slice(INFO_TAG);
    info.push(u8::try_from(app.len()).unwrap_or(u8::MAX));
    info.extend_from_slice(app.as_bytes());
    info.push(u8::try_from(scope.len()).unwrap_or(u8::MAX));
    info.extend_from_slice(scope.as_bytes());
    info.extend_from_slice(&u16::try_from(context.len()).unwrap_or(u16::MAX).to_be_bytes());
    info.extend_from_slice(context);
    info.extend_from_slice(account);
    info.extend_from_slice(&index.to_be_bytes());
    info
}

/// The master value `m` for `index`. Never returned to an app: only [`value`] is.
pub fn master(account_secret: &[u8; 32], account: &[u8; 32], ask: &Ask, index: u64) -> [u8; 32] {
    let prk = hmac_sha256(SALT, account_secret);
    let mut message = info(&ask.app, &ask.scope, &ask.context, account, index);
    message.push(1);
    hmac_sha256(&prk, &message)
}

/// `SHA256(tag | m)`: what the app gets.
pub fn tagged(tag: &str, master: &[u8; 32]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(tag.as_bytes());
    hash.update(master);
    hash.finalize().into()
}

/// The values `ask` gets from the account whose secret is `account_secret`, indexes
/// `first..first + count`.
pub fn values(account_secret: &[u8; 32], account: &[u8; 32], ask: &Ask) -> Vec<[u8; 32]> {
    (ask.first..ask.first + ask.count).map(|index| tagged(&ask.tag, &master(account_secret, account, ask, index))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn hmac_matches_rfc_4231() {
        // Test case 2.
        assert_eq!(
            hex(&hmac_sha256(b"Jefe", b"what do ya want for nothing?")),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
        // Test case 6: a key longer than a block is hashed first.
        assert_eq!(
            hex(&hmac_sha256(&[0xaa; 131], b"Test Using Larger Than Block-Size Key - Hash Key First")),
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
        );
    }

    /// Vectors computed independently (Python `hmac`/`hashlib`) from the formula in the docs.
    #[test]
    fn vectors() {
        let secret = [7_u8; 32];
        let account = [9_u8; 32];
        let ask = Ask::new("lez_bridge", "RFP022/deposit", vec![0xab, 0xcd], "RFP022/ck", 0, 2).unwrap();
        let got: Vec<String> = values(&secret, &account, &ask).iter().map(|v| hex(v)).collect();
        assert_eq!(got, VECTOR_BRIDGE.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>());
        assert_eq!(hex(&master(&secret, &account, &ask, 0)), VECTOR_MASTER_0);
    }

    // From Python's `hmac` and `hashlib` with the formula above (docs/wallet/BRIDGE-SDK.md,
    // "App secrets", states the formula; the script itself is not kept in the repository).
    const VECTOR_MASTER_0: &str = "d3d73d74e8003e7236d01e9b4bfcd9db81d6ade99c2304c4527ef46f5ef4b716";
    const VECTOR_BRIDGE: [&str; 2] = [
        "8bdd25262f56570aa34cd58d46bd80fc6de18deedc75ac8fdc8994c500f19e59",
        "566f88b5c0b2ad570680236cdfb51278ba3b74e772be69443d4560036e2acdfb",
    ];

    #[test]
    fn every_input_separates_the_values() {
        let secret = [7_u8; 32];
        let account = [9_u8; 32];
        let base = Ask::new("lez_bridge", "RFP022/deposit", vec![1, 2], "RFP022/ck", 5, 1).unwrap();
        let one = |ask: &Ask, secret: &[u8; 32], account: &[u8; 32]| values(secret, account, ask)[0];
        let reference = one(&base, &secret, &account);
        let variants = [
            Ask { app: "evil_app".into(), ..base.clone() },
            Ask { scope: "RFP022/exit".into(), ..base.clone() },
            Ask { context: vec![1, 3], ..base.clone() },
            Ask { context: vec![], ..base.clone() },
            Ask { tag: "RFP022/ek".into(), ..base.clone() },
            Ask { first: 6, ..base.clone() },
        ];
        for variant in &variants {
            assert_ne!(one(variant, &secret, &account), reference, "{variant:?}");
        }
        assert_ne!(one(&base, &[8; 32], &account), reference, "another account secret");
        assert_ne!(one(&base, &secret, &[10; 32]), reference, "another account id");
        // Length prefixes keep a moved boundary from giving the same info.
        assert_ne!(info("ab", "c", &[], &account, 0), info("a", "bc", &[], &account, 0));
        assert_ne!(info("a", "b", &[b'c'], &account, 0), info("a", "bc", &[], &account, 0));
        // The same ask gives the same values: nothing random, so a restored wallet gets them back.
        assert_eq!(one(&base, &secret, &account), reference);
        // A batch is the same as asking one index at a time.
        let batch = values(&secret, &account, &Ask { first: 3, count: 4, ..base.clone() });
        for (offset, value) in batch.iter().enumerate() {
            assert_eq!(*value, values(&secret, &account, &Ask { first: 3 + offset as u64, count: 1, ..base.clone() })[0]);
        }
        // What the app gets is never the master value itself.
        assert_ne!(reference, master(&secret, &account, &base, 5));
    }

    #[test]
    fn bad_asks_are_refused() {
        for (app, scope, context, tag, first, count) in [
            ("", "s", 0, "t", 0, 1),
            ("evil app", "s", 0, "t", 0, 1),
            ("app", "", 0, "t", 0, 1),
            ("app", "has space", 0, "t", 0, 1),
            ("app", &"s".repeat(MAX_SCOPE + 1) as &str, 0, "t", 0, 1),
            ("app", "s", 0, "", 0, 1),
            ("app", "s", 0, "t\n", 0, 1),
            ("app", "s", MAX_CONTEXT + 1, "t", 0, 1),
            ("app", "s", 0, "t", 0, 0),
            ("app", "s", 0, "t", 0, MAX_COUNT + 1),
            ("app", "s", 0, "t", u64::MAX, 2),
        ] {
            assert!(Ask::new(app, scope, vec![0; context], tag, first, count).is_err(), "{app:?} {scope:?} {tag:?} {count}");
        }
        assert!(Ask::new("app", "s", vec![0; MAX_CONTEXT], "t", u64::MAX, 1).is_ok());
    }
}
