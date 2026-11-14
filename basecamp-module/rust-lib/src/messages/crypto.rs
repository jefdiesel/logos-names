//! Names Messages, version 1, as the phone app (logos-node-android) and the Names web app speak it: the wire
//! format, the padding the phone adds since its 0.9.22, the inbox topic and the safety code. A port of
//! `gateway/app-src/msg-crypto.js` (sealMessage, openMessage) and of the phone's `chat-core.js` (sealPadded,
//! safetyCode), checked byte for byte against vectors made by those files (testdata/msg-v1-vectors.json, made by
//! tools/msg-vectors.mjs).
//!
//! Wire payload (one Waku message):
//!   0x01 ‖ ephemeral X25519 public key (32) ‖ nonce (24) ‖ XChaCha20-Poly1305 ciphertext
//! Key: HKDF-SHA256(ikm = X25519(ephemeral secret, recipient key), salt = ephemeral public ‖ recipient public,
//!   info = "names/msg/v1"), 32 bytes. Associated data: 0x01 ‖ ephemeral public key.
//! Plaintext: JSON { v: 1, from, fromKey, text, ts, auth } (padded: and p, a string of spaces that makes the
//!   plaintext exactly one of PAD_CLASSES bytes; readers ignore it; it is inside the AEAD, outside `auth`).
//! `auth`: HMAC-SHA256 with key HKDF-SHA256(ikm = X25519(sender secret, recipient public), salt = empty,
//!   info = "names/msg/v1/auth"), over 0x01 ‖ ephemeral public ‖ nonce ‖ utf8(from) ‖ 0x00 ‖ fromKey (32) ‖
//!   utf8(ts) ‖ 0x00 ‖ utf8(text). It proves to the recipient (only) that the sender holds fromKey's secret.
//! Content topic: /names/1/msg-<first 16 hex of SHA-256(recipient public key)>/proto (shard 7 on logos.test).

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256, Sha512};

pub const MSG_VERSION: u8 = 1;
/// The longest text, in UTF-16 code units (JavaScript's `text.length`), as the other apps count it.
pub const MAX_TEXT: usize = 2000;
/// Plaintext sizes in bytes a padded message is made up to (the top one plus the 73 bytes around it stays under
/// 16,384).
pub const PAD_CLASSES: [usize; 4] = [512, 2048, 8192, 15872];
const INFO: &[u8] = b"names/msg/v1";
const AUTH_INFO: &[u8] = b"names/msg/v1/auth";
const HEADER_LEN: usize = 1 + 32 + 24;
const TAG_LEN: usize = 16;

/// A message opened with this device's key.
#[derive(Debug, Clone, PartialEq)]
pub struct Opened {
    /// The ephemeral key (hex): tells repeats apart.
    pub id: String,
    /// The name it says it is from (or `msg-<16 hex>` for a sender with no name).
    pub from: String,
    /// The sender's messaging address (hex).
    pub from_key: String,
    pub text: String,
    pub ts: i64,
    /// The sender holds the secret of `from_key`. Whether `from_key` is `from`'s is for the caller to check.
    pub authentic: bool,
}

/// The randomness of one sealing: the ephemeral secret and the nonce. Fixed only in tests.
pub struct Fresh {
    pub eph_secret: [u8; 32],
    pub nonce: [u8; 24],
}

impl Fresh {
    pub fn random() -> Result<Fresh, String> {
        let mut eph_secret = [0u8; 32];
        let mut nonce = [0u8; 24];
        getrandom::getrandom(&mut eph_secret).map_err(|e| e.to_string())?;
        getrandom::getrandom(&mut nonce).map_err(|e| e.to_string())?;
        Ok(Fresh { eph_secret, nonce })
    }
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn unhex(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 || !text.bytes().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok()).collect()
}

/// The 32-byte key in a name's `msg` entry (trimmed, any case), or None (as msg-crypto.js parseMsgKey).
pub fn parse_msg_key(value: &str) -> Option<[u8; 32]> {
    let v = value.trim().to_ascii_lowercase();
    if v.len() != 64 {
        return None;
    }
    unhex(&v)?.try_into().ok()
}

/// A name as the format allows it in `from`: 1 to 32 of a-z, 0-9 and '-', not starting or ending with '-'.
pub fn is_name(from: &str) -> bool {
    let b = from.as_bytes();
    let ok = |c: &u8| c.is_ascii_lowercase() || c.is_ascii_digit();
    !b.is_empty() && b.len() <= 32 && ok(&b[0]) && ok(&b[b.len() - 1]) && b.iter().all(|c| ok(c) || *c == b'-')
}

/// The `from` of a sender with no name: msg-<first 16 hex of its key>.
pub fn nameless(public: &[u8; 32]) -> String {
    format!("msg-{}", &hex(public)[..16])
}

pub fn public_key(secret: &[u8; 32]) -> [u8; 32] {
    x25519_dalek::x25519(*secret, x25519_dalek::X25519_BASEPOINT_BYTES)
}

/// The Waku content topic of the inbox of `public`.
pub fn inbox_topic(public: &[u8; 32]) -> String {
    format!("/names/1/msg-{}/proto", &hex(&Sha256::digest(public))[..16])
}

/// X25519, refusing the all-zero result of a low-order key.
fn dh(secret: &[u8; 32], public: &[u8; 32]) -> Result<[u8; 32], String> {
    let shared = x25519_dalek::x25519(*secret, *public);
    if shared.iter().all(|b| *b == 0) {
        return Err("bad public key".into());
    }
    Ok(shared)
}

fn hkdf32(ikm: &[u8], salt: &[u8], info: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    Hkdf::<Sha256>::new(Some(salt), ikm).expand(info, &mut out).expect("32 bytes is a valid length");
    out
}

fn auth_tag(static_shared: &[u8; 32], header: &[u8], from: &str, from_key: &[u8; 32], ts: i64, text: &str) -> [u8; 32] {
    let key = hkdf32(static_shared, &[], AUTH_INFO);
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&key).expect("any key length");
    mac.update(header);
    mac.update(from.as_bytes());
    mac.update(&[0]);
    mac.update(from_key);
    mac.update(ts.to_string().as_bytes());
    mac.update(&[0]);
    mac.update(text.as_bytes());
    mac.finalize().into_bytes().into()
}

/// `s` as JavaScript's JSON.stringify writes a string: quotes, backslash and the controls below 0x20 escaped
/// (\b \f \n \r \t, else \u00xx), everything else as it is (U+2028 and U+2029 too).
fn json_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn plaintext(from: &str, from_key: &[u8; 32], text: &str, ts: i64, auth: &[u8; 32], pad: Option<usize>) -> String {
    let mut j = String::from("{\"v\":1,\"from\":");
    json_str(&mut j, from);
    j.push_str(&format!(",\"fromKey\":\"{}\",\"text\":", hex(from_key)));
    json_str(&mut j, text);
    j.push_str(&format!(",\"ts\":{ts},\"auth\":\"{}\"", hex(auth)));
    if let Some(spaces) = pad {
        j.push_str(",\"p\":\"");
        j.push_str(&" ".repeat(spaces));
        j.push('"');
    }
    j.push('}');
    j
}

/// The smallest pad class that holds `n` bytes, or None (then the message goes unpadded, as the phone does).
pub fn pad_class(n: usize) -> Option<usize> {
    PAD_CLASSES.iter().copied().find(|c| *c >= n)
}

/// Seals `text` from the name `from` (whose device key is `from_secret`) to the messaging address `to`: the wire
/// payload. `padded`: as the phone sends since its 0.9.22 (the plaintext made up to a pad class); else as
/// msg-crypto.js sealMessage. `ts`: milliseconds since 1970.
pub fn seal(to: &[u8; 32], from: &str, from_secret: &[u8; 32], text: &str, ts: i64, padded: bool, fresh: Fresh) -> Result<Vec<u8>, String> {
    if !is_name(from) {
        return Err("from must be a name".into());
    }
    let units = text.encode_utf16().count();
    if units == 0 || units > MAX_TEXT {
        return Err("bad text".into());
    }
    if ts.unsigned_abs() > (1u64 << 53) - 1 {
        return Err("bad time".into());
    }
    let from_key = public_key(from_secret);
    let eph_public = public_key(&fresh.eph_secret);
    let mut header = Vec::with_capacity(HEADER_LEN);
    header.push(MSG_VERSION);
    header.extend_from_slice(&eph_public);
    header.extend_from_slice(&fresh.nonce);
    let mut salt = Vec::with_capacity(64);
    salt.extend_from_slice(&eph_public);
    salt.extend_from_slice(to);
    let key = hkdf32(&dh(&fresh.eph_secret, to)?, &salt, INFO);
    let auth = auth_tag(&dh(from_secret, to)?, &header, from, &from_key, ts, text);
    let plain = if padded {
        let bare = plaintext(from, &from_key, text, ts, &auth, Some(0)).len();
        plaintext(from, &from_key, text, ts, &auth, Some(pad_class(bare).map_or(0, |c| c - bare)))
    } else {
        plaintext(from, &from_key, text, ts, &auth, None)
    };
    let mut ad = vec![MSG_VERSION];
    ad.extend_from_slice(&eph_public);
    let sealed = XChaCha20Poly1305::new((&key).into())
        .encrypt(XNonce::from_slice(&fresh.nonce), Payload { msg: plain.as_bytes(), aad: &ad })
        .map_err(|_| "couldn't seal".to_string())?;
    header.extend_from_slice(&sealed);
    Ok(header)
}

/// A JSON number that is a safe integer, as Number.isSafeInteger takes it (1.0 is 1).
fn safe_integer(v: &serde_json::Value) -> Option<i64> {
    const MAX: i64 = (1 << 53) - 1;
    match v {
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                return (i.abs() <= MAX).then_some(i);
            }
            let f = n.as_f64()?;
            (f.fract() == 0.0 && f.abs() <= MAX as f64).then_some(f as i64)
        }
        _ => None,
    }
}

/// Opens a wire payload with this device's secret key: None when it isn't for this key or isn't well formed
/// (as msg-crypto.js openMessage; padded messages open the same way).
pub fn open(secret: &[u8; 32], payload: &[u8]) -> Option<Opened> {
    if payload.len() < HEADER_LEN + TAG_LEN || payload[0] != MSG_VERSION {
        return None;
    }
    let header = &payload[..HEADER_LEN];
    let eph_public: [u8; 32] = payload[1..33].try_into().ok()?;
    let nonce = &payload[33..HEADER_LEN];
    let me = public_key(secret);
    let mut salt = Vec::with_capacity(64);
    salt.extend_from_slice(&eph_public);
    salt.extend_from_slice(&me);
    let key = hkdf32(&dh(secret, &eph_public).ok()?, &salt, INFO);
    let mut ad = vec![MSG_VERSION];
    ad.extend_from_slice(&eph_public);
    let plain = XChaCha20Poly1305::new((&key).into())
        .decrypt(XNonce::from_slice(nonce), Payload { msg: &payload[HEADER_LEN..], aad: &ad })
        .ok()?;
    let msg: serde_json::Value = serde_json::from_slice(&plain).ok()?;
    if msg.get("v").and_then(safe_integer) != Some(1) || !msg.get("v").is_some_and(|v| v.is_number()) {
        return None;
    }
    let from = msg.get("from")?.as_str()?.to_owned();
    let text = msg.get("text")?.as_str()?.to_owned();
    if !is_name(&from) || text.encode_utf16().count() > MAX_TEXT {
        return None;
    }
    let ts = safe_integer(msg.get("ts")?)?;
    let sender = parse_msg_key(msg.get("fromKey")?.as_str()?)?;
    let authentic = (|| {
        let auth = msg.get("auth")?.as_str()?;
        if auth.len() != 64 || !auth.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)) {
            return Some(false);
        }
        let expected = auth_tag(&dh(secret, &sender).ok()?, header, &from, &sender, ts, &text);
        let given = unhex(auth)?;
        Some(given.len() == 32 && given.iter().zip(expected.iter()).fold(0u8, |d, (a, b)| d | (a ^ b)) == 0)
    })()
    .unwrap_or(false);
    Some(Opened { id: hex(&eph_public), from, from_key: hex(&sender), text, ts, authentic })
}

/// Whether an opened message really comes from the name it claims: authentic, and `on_chain_key` (the `msg`
/// entry of `from`, read from the chain) is its `from_key`.
pub fn is_verified(message: &Opened, on_chain_key: &str) -> bool {
    message.authentic && parse_msg_key(on_chain_key).is_some_and(|k| hex(&k) == message.from_key)
}

const SAFETY_DOMAIN: &[u8] = b"names/safety-code/v1";
const SAFETY_ROUNDS: usize = 5200;

/// 30 digits for one messaging address: SHA-512 iterated, 6 groups of 5 (the phone's safetyHalf).
fn safety_half(key: &[u8; 32]) -> String {
    let mut h = Sha512::new().chain_update(SAFETY_DOMAIN).chain_update(key).finalize();
    for _ in 0..SAFETY_ROUNDS {
        h = Sha512::new().chain_update(h).chain_update(key).finalize();
    }
    (0..6)
        .map(|i| {
            let v = (0..5).fold(0u64, |v, j| v * 256 + h[5 * i + j] as u64);
            format!("{:05}", v % 100_000)
        })
        .collect()
}

/// The safety code of two messaging addresses (64 hex each): 60 digits, the same whichever side computes it.
pub fn safety_code(a_hex: &str, b_hex: &str) -> Result<String, String> {
    let key = |h: &str| -> Result<[u8; 32], String> {
        if h.len() == 64 && h.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)) {
            Ok(unhex(h).expect("checked").try_into().expect("32"))
        } else {
            Err("a messaging address is 64 hex".into())
        }
    };
    let mut halves = [safety_half(&key(a_hex)?), safety_half(&key(b_hex)?)];
    halves.sort();
    Ok(halves.concat())
}

/// 12 groups of 5 digits, for people to read.
pub fn safety_groups(code: &str) -> Vec<String> {
    code.as_bytes().chunks(5).filter(|c| c.len() == 5).map(|c| String::from_utf8_lossy(c).into_owned()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn vectors() -> Value {
        serde_json::from_str(include_str!("../../testdata/msg-v1-vectors.json")).expect("vectors")
    }
    fn key32(h: &str) -> [u8; 32] {
        unhex(h).unwrap().try_into().unwrap()
    }
    fn fixed(v: &Value) -> Fresh {
        Fresh { eph_secret: key32(v["eph_secret"].as_str().unwrap()), nonce: unhex(v["nonce"].as_str().unwrap()).unwrap().try_into().unwrap() }
    }

    #[test]
    fn keys_and_topics_match_the_phone() {
        let d = vectors();
        for (_, k) in d["keys"].as_object().unwrap().iter().chain(d["phone"]["keys"].as_object().unwrap().iter()) {
            let p = public_key(&key32(k["secret"].as_str().unwrap()));
            assert_eq!(hex(&p), k["public"].as_str().unwrap());
            assert_eq!(inbox_topic(&p), k["topic"].as_str().unwrap());
        }
    }

    #[test]
    fn seals_byte_for_byte_like_msg_crypto_js_and_the_phone() {
        let d = vectors();
        let keys = &d["keys"];
        for (set, padded) in [("seal", false), ("padded", true)] {
            for v in d[set].as_array().unwrap() {
                let sender = v.get("sender").and_then(Value::as_str).unwrap_or_else(|| v["from"].as_str().unwrap());
                let to = key32(keys[v["to"].as_str().unwrap()]["public"].as_str().unwrap());
                let from_secret = key32(keys[sender]["secret"].as_str().unwrap());
                let got = seal(&to, v["from"].as_str().unwrap(), &from_secret, v["text"].as_str().unwrap(), v["ts"].as_i64().unwrap(), padded, fixed(v)).unwrap();
                assert_eq!(hex(&got), v["payload"].as_str().unwrap(), "{set} {}", v["label"]);
                if padded {
                    assert_eq!(got.len() - HEADER_LEN - TAG_LEN, v["plaintext_len"].as_u64().unwrap() as usize);
                }
                let opened = open(&key32(keys[v["to"].as_str().unwrap()]["secret"].as_str().unwrap()), &got).unwrap();
                assert!(opened.authentic && opened.text == v["text"].as_str().unwrap() && opened.ts == v["ts"].as_i64().unwrap());
                assert_eq!(opened.from_key, hex(&public_key(&from_secret)));
            }
        }
    }

    #[test]
    fn opens_the_names_projects_own_vectors() {
        let d = vectors();
        let p = &d["phone"];
        for m in p["messages"].as_array().unwrap() {
            let to = &p["keys"][m["to"].as_str().unwrap()];
            let from = &p["keys"][m["from"].as_str().unwrap()];
            let o = open(&key32(to["secret"].as_str().unwrap()), &unhex(m["payload"].as_str().unwrap()).unwrap()).unwrap();
            assert_eq!((o.from.as_str(), o.text.as_str(), o.ts), (m["from"].as_str().unwrap(), m["text"].as_str().unwrap(), m["ts"].as_i64().unwrap()));
            assert!(o.authentic && is_verified(&o, from["public"].as_str().unwrap()));
            assert!(!is_verified(&o, to["public"].as_str().unwrap()), "another key isn't the sender's");
            // the wrong key opens nothing; a changed byte opens nothing
            assert!(open(&key32(from["secret"].as_str().unwrap()), &unhex(m["payload"].as_str().unwrap()).unwrap()).is_none());
            let mut bad = unhex(m["payload"].as_str().unwrap()).unwrap();
            let n = bad.len() - 1;
            bad[n] ^= 1;
            assert!(open(&key32(to["secret"].as_str().unwrap()), &bad).is_none());
        }
    }

    #[test]
    fn safety_codes_match_the_phone() {
        for s in vectors()["safety"].as_array().unwrap() {
            let (a, b) = (s["a"].as_str().unwrap(), s["b"].as_str().unwrap());
            assert_eq!(safety_code(a, b).unwrap(), s["code"].as_str().unwrap());
            assert_eq!(safety_code(b, a).unwrap(), s["code"].as_str().unwrap());
        }
        assert_eq!(safety_groups(&"1234567890".repeat(6)).len(), 12);
        assert!(safety_code("zz", "00").is_err());
    }

    #[test]
    fn refuses_what_the_other_apps_refuse() {
        let k = [7u8; 32];
        let to = public_key(&[9u8; 32]);
        let f = || Fresh { eph_secret: [1; 32], nonce: [2; 24] };
        assert!(seal(&to, "Alice", &k, "hi", 1, true, f()).is_err(), "names are lower case");
        assert!(seal(&to, "-a", &k, "hi", 1, true, f()).is_err());
        assert!(seal(&to, "alice", &k, "", 1, true, f()).is_err());
        assert!(seal(&to, "alice", &k, &"x".repeat(2001), 1, true, f()).is_err());
        assert!(seal(&to, "alice", &k, &"😀".repeat(1001), 1, true, f()).is_err(), "2002 UTF-16 units");
        assert!(seal(&to, "alice", &k, &"😀".repeat(1000), 1, true, f()).is_ok());
        assert!(seal(&[0u8; 32], "alice", &k, "hi", 1, true, f()).is_err(), "a low-order key");
        assert!(is_name(&nameless(&to)));
        assert_eq!(pad_class(512), Some(512));
        assert_eq!(pad_class(513), Some(2048));
        assert_eq!(pad_class(15873), None);
    }
}
