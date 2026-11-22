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
//!
//! An **imported** account (its key is in the wallet, but neither key tree of the recovery phrase
//! holds it) takes its values from the phrase instead, so a wallet restored from the phrase alone
//! makes them again (rfp-022 `proposal/IMPORTED-ACCOUNTS-DESIGN.md`):
//!
//! ```text
//! R     = the private key tree root's secret spending key (first 32 bytes of
//!         HMAC-SHA512(key = "/LEE-Keys/v1/Master/Private", msg = BIP-39 seed of the phrase))
//! prk   = HMAC-SHA256(key = "logos-names/app-secret/imported/v1", msg = R)
//! info  = "logos-names/app-secret/imported/v1/info" | the same fields as above
//! m, value as above
//! ```
//!
//! New salt and new `info` prefix: nothing a phrase account gets changes, and the two schemes never
//! meet. Which scheme an account follows is the names CLI's decision (tree membership); its answer
//! names it (`"scheme"`).

use sha2::{Digest, Sha256};

/// HKDF-Extract salt.
pub const SALT: &[u8] = b"logos-names/app-secret/v1";
/// First bytes of every HKDF-Expand `info`.
pub const INFO_TAG: &[u8] = b"logos-names/app-secret/v1/info";
/// HKDF-Extract salt of an imported account's values.
pub const IMPORTED_SALT: &[u8] = b"logos-names/app-secret/imported/v1";
/// First bytes of an imported account's HKDF-Expand `info`.
pub const IMPORTED_INFO_TAG: &[u8] = b"logos-names/app-secret/imported/v1/info";
/// The CLI's word for each scheme (`"scheme"` in its answer).
pub const SCHEME_PHRASE: &str = "phrase";
pub const SCHEME_IMPORTED: &str = "imported";
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
    info_with(INFO_TAG, app, scope, context, account, index)
}

/// `info` after `prefix` (the phrase scheme's or the imported one's).
fn info_with(prefix: &[u8], app: &str, scope: &str, context: &[u8], account: &[u8; 32], index: u64) -> Vec<u8> {
    let mut info = Vec::with_capacity(prefix.len() + 2 + app.len() + scope.len() + 2 + context.len() + 40);
    info.extend_from_slice(prefix);
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

/// The master value `m` of an imported account for `index`, from the wallet's root `R`.
pub fn imported_master(root: &[u8; 32], account: &[u8; 32], ask: &Ask, index: u64) -> [u8; 32] {
    let mut prk = hmac_sha256(IMPORTED_SALT, root);
    let mut message = info_with(IMPORTED_INFO_TAG, &ask.app, &ask.scope, &ask.context, account, index);
    message.push(1);
    let m = hmac_sha256(&prk, &message);
    prk.fill(0);
    m
}

/// The values `ask` gets from the imported account `account` of the wallet whose root is `root`.
pub fn imported_values(root: &[u8; 32], account: &[u8; 32], ask: &Ask) -> Vec<[u8; 32]> {
    (0..ask.count).map(|offset| ask.first + offset).map(|index| tagged(&ask.tag, &imported_master(root, account, ask, index))).collect()
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
    (0..ask.count).map(|offset| ask.first + offset).map(|index| tagged(&ask.tag, &master(account_secret, account, ask, index))).collect()
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

    /// rfp-022 `lez-bridge/client/tests/vectors/gen_names_imported_vectors.py` (Python, stdlib), its
    /// output copied byte for byte to `testdata/app-secret-imported-v1.json`.
    #[test]
    fn imported_vectors() {
        let doc: serde_json::Value = serde_json::from_str(include_str!("../testdata/app-secret-imported-v1.json")).unwrap();
        let unhex32 = |v: &serde_json::Value| -> [u8; 32] {
            let t = v.as_str().unwrap().trim_start_matches("0x");
            (0..32).map(|i| u8::from_str_radix(&t[2 * i..2 * i + 2], 16).unwrap()).collect::<Vec<_>>().try_into().unwrap()
        };
        assert_eq!(doc["salt"], std::str::from_utf8(IMPORTED_SALT).unwrap());
        assert_eq!(doc["info_tag"], std::str::from_utf8(IMPORTED_INFO_TAG).unwrap());
        let c = &doc["names_case"];
        let root = unhex32(&c["root"]);
        let account = [9_u8; 32];
        let ask = Ask::new("lez_bridge", "RFP022/deposit", vec![0xab, 0xcd], "RFP022/ck", 0, 2).unwrap();
        assert_eq!(imported_master(&root, &account, &ask, 0), unhex32(&c["master_0"]));
        let got = imported_values(&root, &account, &ask);
        assert_eq!(got, c["values"].as_array().unwrap().iter().map(unhex32).collect::<Vec<_>>());
        // The phrase scheme with the same 32 bytes as an account secret gives other values.
        assert_eq!(values(&root, &account, &ask), c["phrase_scheme_values"].as_array().unwrap().iter().map(unhex32).collect::<Vec<_>>());
        assert_ne!(values(&root, &account, &ask), got);
        // The bridge's cases: claim and exit secrets.
        for case in doc["cases"].as_array().unwrap() {
            let r = unhex32(&case["root"]);
            let id = unhex32(&case["account"]);
            let t = |k: &str| case[k].as_str().unwrap().trim_start_matches("0x").to_owned();
            let bytes = |h: String| (0..h.len() / 2).map(|i| u8::from_str_radix(&h[2 * i..2 * i + 2], 16).unwrap()).collect::<Vec<u8>>();
            let denom: u128 = case["denom"].as_str().unwrap().parse().unwrap();
            let fee: u128 = case["max_relay_fee"].as_str().unwrap().parse().unwrap();
            let u = |v: u128| { let mut o = [0u8; 32]; o[16..].copy_from_slice(&v.to_be_bytes()); o };
            let dctx = [bytes(t("cfg_id")), bytes(t("token")), u(denom).to_vec()].concat();
            let ectx = [dctx.clone(), bytes(t("eth_dest")), u(fee).to_vec()].concat();
            let index = case["index"].as_u64().unwrap();
            let dep = Ask::new("lez_bridge", "RFP022/deposit", dctx, "RFP022/ck", index, 1).unwrap();
            assert_eq!(imported_master(&r, &id, &dep, index), unhex32(&case["deposit_master"]));
            assert_eq!(imported_values(&r, &id, &dep)[0], unhex32(&case["claim_secret"]));
            let exit = Ask::new("lez_bridge", "RFP022/exit", ectx, "RFP022/ek", index, 1).unwrap();
            assert_eq!(imported_values(&r, &id, &exit)[0], unhex32(&case["exit_secret"]));
            assert_eq!(values(&r, &id, &exit)[0], unhex32(&case["phrase_scheme_exit_secret"]));
        }
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
        // Deep review B5: the last index is made like any other (its range never overflows).
        let last = Ask::new("app", "s", Vec::new(), "t", u64::MAX, 1).unwrap();
        assert_eq!(values(&[1; 32], &[2; 32], &last).len(), 1);
        assert_eq!(imported_values(&[1; 32], &[2; 32], &last).len(), 1);
    }
}
