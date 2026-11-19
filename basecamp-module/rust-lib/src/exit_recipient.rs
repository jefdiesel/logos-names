//! Where an app's exit secrets send tokens: the Ethereum address in an `RFP022/exit` context.
//!
//! The LEZ Bridge asks the wallet for exit secrets (`app_secret`, scope `RFP022/exit`) with a
//! context of a fixed layout (rfp-022 `modules/lez_bridge/rust-lib/src/wallet.rs` `exit_context`):
//!
//! ```text
//! cfgId(32) | token(20) | u256(denom) | ethDest(20) | u256(maxRelayFee)     136 bytes
//! ```
//!
//! An exit secret for a context is what lets a burn be released to `ethDest`. So the wallet reads
//! the address out of the context, and gives an app values for an address only once the person
//! has approved that address for that app and account: on a sheet that shows it in full, with
//! the four words this module computes itself (the bridge page's `logos-bridge/withdraw-to/v1`
//! words) and the bridge it belongs to, or by reading it back and ticking it at step 2 of the
//! Names app's own Bridge to Ethereum. A context that doesn't decode is refused, never granted.
//!
//! The approved addresses are kept in the network's wallet folder, beside the scheme pins
//! (`app-secret-schemes.json`), in a file of their own (`RECIPIENTS_FILE`): not in
//! `provider.json`, whose grant rows go when an app is disconnected. It holds no secret.

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

/// The scope whose context names an Ethereum recipient.
pub const EXIT_SCOPE: &str = "RFP022/exit";
/// The context's length.
pub const CONTEXT_LEN: usize = 136;
/// The file, in the wallet folder, that keeps the approved recipients.
pub const RECIPIENTS_FILE: &str = "app-secret-recipients.json";
/// The most recipients kept per (app, account).
pub const MAX_RECIPIENTS: usize = 256;

/// The bridges whose exits the wallet knows by their Config: label, `cfg_id`, vault, Ethereum
/// chain id (read from each Config on the bridge test network, testdata/bridge-net/config-v4..v7,
/// 2026-11-15 to 18; a test checks them against the fixtures).
pub const EXIT_BRIDGES: [(&str, &str, &str, u64); 4] = [
    ("bridge v4", "0c0f00c30fbe03c00e566a332fe73aa2f11d9e0fd6b8684e509a8be33cbd8be5", "c2c6a059eab928966b24fa5549d441a3b17b54ed", 11_155_111),
    ("bridge v5", "c513c0f6a949cd81f7e834d0bcf771fd1e89f7430fa1b713c5aad71e1eb4f7d1", "afbdf192434e9888bb23dae6f2d15eea6246ed10", 11_155_111),
    ("bridge v6", "7d1a304a12a486c730d94321244bd6d15741208c1ef9d00e352acda8589e6f60", "95cea20e75d3b352b49d13cb0cddc268a14c427e", 11_155_111),
    ("bridge v7", "7ef9153b69a348ff39e3daf7b893f1b775513489c71c7331d88b4a5cf30652d5", "4d7b360cf385d3477f1a67c1aedeb6c17c26d23e", 11_155_111),
];

/// The BIP-39 English word list (MIT, Pavol Rusnak; `data/bip39-english.LICENSE`), as the bridge
/// module embeds it (a test checks its SHA-256).
const WORDS: &str = include_str!("../data/bip39-english.txt");
const COMPARE_TAG: &[u8] = b"logos-bridge/compare/v1";
const WITHDRAW_TAG: &[u8] = b"logos-bridge/withdraw-to/v1";

/// An `RFP022/exit` context, read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExitContext {
    pub cfg_id: [u8; 32],
    pub token: [u8; 20],
    pub denom: u128,
    pub recipient: [u8; 20],
    pub max_relay_fee: u128,
}

fn u256(bytes: &[u8]) -> Result<u128, String> {
    if bytes[..16].iter().any(|b| *b != 0) {
        return Err("an amount in the context is larger than any the bridge uses".to_owned());
    }
    Ok(u128::from_be_bytes(bytes[16..32].try_into().expect("16 bytes")))
}

/// Reads an exit context; anything else is refused with the reason.
pub fn decode(context: &[u8]) -> Result<ExitContext, String> {
    if context.len() != CONTEXT_LEN {
        return Err(format!("an exit's context is {CONTEXT_LEN} bytes (bridge, token, amount, Ethereum address, relay fee); this one is {}", context.len()));
    }
    let recipient: [u8; 20] = context[84..104].try_into().expect("20 bytes");
    if recipient == [0; 20] {
        return Err("an exit's context names no Ethereum address".to_owned());
    }
    let denom = u256(&context[52..84])?;
    if denom == 0 {
        return Err("an exit's context has no amount".to_owned());
    }
    Ok(ExitContext {
        cfg_id: context[..32].try_into().expect("32 bytes"),
        token: context[32..52].try_into().expect("20 bytes"),
        denom,
        recipient,
        max_relay_fee: u256(&context[104..136])?,
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// An Ethereum address as the wallet keeps and compares it: `0x` and 40 lowercase hex digits.
pub fn address_text(address: &[u8; 20]) -> String {
    format!("0x{}", hex(address))
}

/// `0x` and 40 hex digits, any case, as `address_text` writes it; else None.
pub fn normalized(text: &str) -> Option<String> {
    let text = text.trim();
    let digits = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X"))?;
    (digits.len() == 40 && digits.bytes().all(|b| b.is_ascii_hexdigit()) && digits.bytes().any(|b| b != b'0')).then(|| format!("0x{}", digits.to_ascii_lowercase()))
}

/// The bridge an exit belongs to, by its Config id: (label, vault, chain id).
pub fn bridge_of(cfg_id: &[u8; 32]) -> Option<(&'static str, [u8; 20], u64)> {
    let id = hex(cfg_id);
    EXIT_BRIDGES.iter().find(|(_, cfg, _, _)| *cfg == id).map(|(label, _, vault, chain)| {
        let mut out = [0u8; 20];
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&vault[2 * i..2 * i + 2], 16).expect("hex");
        }
        (*label, out, *chain)
    })
}

/// The four words the bridge page shows for a withdraw-to code: BIP-39 English words of the
/// first 44 bits of `SHA-256("logos-bridge/compare/v1" | SHA-256("logos-bridge/withdraw-to/v1" |
/// payload))`, payload `1 | chainId(8, BE) | vault(20) | recipient(20) | feeChoice(1)`.
pub fn words(chain_id: u64, vault: &[u8; 20], recipient: &[u8; 20], fee_choice: u8) -> String {
    let mut payload = vec![1u8];
    payload.extend(chain_id.to_be_bytes());
    payload.extend(vault);
    payload.extend(recipient);
    payload.push(fee_choice);
    let digest: [u8; 32] = Sha256::new().chain_update(WITHDRAW_TAG).chain_update(&payload).finalize().into();
    let h = Sha256::new().chain_update(COMPARE_TAG).chain_update(digest).finalize();
    let top = u64::from_be_bytes([0, 0, h[0], h[1], h[2], h[3], h[4], h[5]]) >> 4;
    [33u32, 22, 11, 0]
        .map(|shift| WORDS.lines().nth(((top >> shift) & 0x7ff) as usize).expect("2048 words"))
        .join(" \u{b7} ")
}

/// What the sheet shows of an exit context: `{recipient, bridge_label, vault, chain_id, token,
/// denom, max_relay_fee, words_text, words_why}`. The words are made only when the relay fee is
/// 0 (fee choice 0): for another fee the code's choice is an entry of the vault's fee menu, which
/// the context doesn't carry, so `words_text` is null and `words_why` says so.
pub fn sheet(exit: &ExitContext) -> Value {
    let bridge = bridge_of(&exit.cfg_id);
    let (words_text, words_why) = match (bridge, exit.max_relay_fee) {
        (Some((_, vault, chain)), 0) => (Value::from(words(chain, &vault, &exit.recipient, 0)), Value::Null),
        (Some(_), fee) => (Value::Null, Value::from(format!("This send-back pays a relay fee ({fee} base units), and which entry of the vault's fee menu that is isn't in the request, so the wallet can't make the four words: compare the address itself."))),
        (None, _) => (Value::Null, Value::from("This exit is for a bridge this wallet doesn't know by its configuration, so it can't make the four words: compare the address itself.")),
    };
    let symbol = crate::bridge_net::SYMBOLS.iter().find(|(eth, _)| *eth == format!("0x{}", hex(&exit.token))).map(|(_, s)| *s);
    json!({
        "recipient": address_text(&exit.recipient),
        "bridge_label": bridge.map(|(label, _, _)| label),
        "vault": bridge.map(|(_, vault, _)| address_text(&vault)),
        "chain_id": bridge.map(|(_, _, chain)| chain),
        "cfg_id": format!("0x{}", hex(&exit.cfg_id)),
        "token": address_text(&exit.token),
        "symbol": symbol,
        "denom": exit.denom.to_string(),
        "max_relay_fee": exit.max_relay_fee.to_string(),
        "words_text": words_text,
        "words_why": words_why,
    })
}

fn load(dir: &std::path::Path) -> Map<String, Value> {
    std::fs::read_to_string(dir.join(RECIPIENTS_FILE)).ok().and_then(|t| serde_json::from_str::<Map<String, Value>>(&t).ok()).unwrap_or_default()
}

/// The recipients approved for `app`, by account: `{account: ["0x…"]}`.
pub fn approved_of(dir: &std::path::Path, app: &str) -> Value {
    load(dir).remove(app).filter(Value::is_object).unwrap_or_else(|| json!({}))
}

/// Whether `recipient` was approved for (`app`, `account`).
pub fn is_approved(dir: &std::path::Path, app: &str, account: &str, recipient: &str) -> bool {
    let Some(recipient) = normalized(recipient) else { return false };
    approved_of(dir, app)[account].as_array().is_some_and(|list| list.iter().any(|r| r.as_str() == Some(recipient.as_str())))
}

/// Keeps `recipient` as approved for (`app`, `account`) (mode 600; the file replaced whole).
pub fn approve(dir: &std::path::Path, app: &str, account: &str, recipient: &str) -> Result<(), String> {
    let recipient = normalized(recipient).ok_or("an Ethereum address is 0x and 40 hex digits, not all zero")?;
    let mut all = load(dir);
    let by_account = all.entry(app.to_owned()).or_insert_with(|| json!({}));
    let by_account = by_account.as_object_mut().ok_or("the approved addresses are unreadable")?;
    let list = by_account.entry(account.to_owned()).or_insert_with(|| json!([]));
    let list = list.as_array_mut().ok_or("the approved addresses are unreadable")?;
    if list.iter().any(|r| r.as_str() == Some(recipient.as_str())) {
        return Ok(());
    }
    if list.len() >= MAX_RECIPIENTS {
        return Err(format!("at most {MAX_RECIPIENTS} Ethereum addresses are kept for one account and app"));
    }
    list.push(Value::from(recipient));
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot keep the approved address: {e}"))?;
    crate::cli::write_private(&dir.join(RECIPIENTS_FILE), Value::Object(all).to_string().as_bytes()).map_err(|e| format!("cannot keep the approved address: {e}"))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// An exit context as the LEZ Bridge builds it (`wallet::exit_context`).
    pub(crate) fn context(cfg_hex: &str, token: [u8; 20], denom: u128, recipient: [u8; 20], fee: u128) -> Vec<u8> {
        let cfg: Vec<u8> = (0..32).map(|i| u8::from_str_radix(&cfg_hex[2 * i..2 * i + 2], 16).unwrap()).collect();
        let u = |v: u128| [[0u8; 16], v.to_be_bytes()].concat();
        [cfg.as_slice(), &token, &u(denom), &recipient, &u(fee)].concat()
    }

    pub(crate) const V4_CFG: &str = "0c0f00c30fbe03c00e566a332fe73aa2f11d9e0fd6b8684e509a8be33cbd8be5";

    #[test]
    fn the_word_list_is_the_bip39_english_one() {
        assert_eq!(hex(&Sha256::digest(WORDS.as_bytes())), "2f5eed53a4727b4bf8880d8f3f199efc90e58503646d9ff8eff3a2ed3b24dbda");
        assert_eq!(WORDS.lines().count(), 2048);
    }

    /// The bridge's own vectors (rfp-022 web/deposit/test/vectors/withdraw-v1.json).
    #[test]
    fn the_four_words_are_the_bridge_page_s() {
        let vectors: Value = serde_json::from_str(include_str!("../testdata/withdraw-to-words-v1.json")).unwrap();
        for v in vectors["valid"].as_array().unwrap() {
            let f = &v["fields"];
            let parse20 = |s: &str| -> [u8; 20] { (0..20).map(|i| u8::from_str_radix(&s[2 + 2 * i..4 + 2 * i], 16).unwrap()).collect::<Vec<_>>().try_into().unwrap() };
            let got = words(f["chainId"].as_str().unwrap().parse().unwrap(), &parse20(f["vault"].as_str().unwrap()), &parse20(f["recipient"].as_str().unwrap()), f["feeChoice"].as_u64().unwrap() as u8);
            assert_eq!(got, v["words_text"].as_str().unwrap(), "{}", v["name"]);
        }
    }

    /// The table is each bridge's own Config (cfg_id, vault, chain id at its head).
    #[test]
    fn the_exit_bridges_are_their_configs() {
        for ((label, cfg, vault, chain), (file, program)) in EXIT_BRIDGES.iter().zip([
            ("config-v4.json", crate::bridge_net::BRIDGE_V4_PROGRAM),
            ("config-v5.json", crate::bridge_net::BRIDGE_V5_PROGRAM),
            ("config-v6.json", crate::bridge_net::BRIDGE_V6_PROGRAM),
            ("config-v7.json", crate::bridge_net::BRIDGE_V7_PROGRAM),
        ]) {
            let reply: Value = serde_json::from_str(&std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/bridge-net").join(file)).unwrap()).unwrap();
            let bytes = crate::bridge_net::config_bytes_in(&reply["result"], &crate::registry::parse_account(program).unwrap()).unwrap();
            assert_eq!((hex(&bytes[..32]), hex(&bytes[32..52]), u64::from_le_bytes(bytes[52..60].try_into().unwrap())), ((*cfg).to_owned(), (*vault).to_owned(), *chain), "{label}");
            assert_eq!(crate::bridge_net::bridge_label(program), Some(*label));
        }
    }

    #[test]
    fn an_exit_context_is_read_or_refused() {
        let to = [0x3c; 20];
        let ctx = context(V4_CFG, [0; 20], 10_000_000_000_000_000, to, 0);
        let exit = decode(&ctx).unwrap();
        assert_eq!((exit.recipient, exit.denom, exit.max_relay_fee), (to, 10_000_000_000_000_000, 0));
        assert!(decode(&ctx[..135]).unwrap_err().contains("136 bytes"));
        assert!(decode(&[ctx.clone(), vec![0]].concat()).is_err());
        assert!(decode(&[0xab, 0xcd]).is_err(), "what the old tests sent");
        assert!(decode(&context(V4_CFG, [0; 20], 1, [0; 20], 0)).unwrap_err().contains("no Ethereum address"));
        assert!(decode(&context(V4_CFG, [0; 20], 0, to, 0)).unwrap_err().contains("no amount"));
        let mut big = ctx.clone();
        big[52] = 1;
        assert!(decode(&big).unwrap_err().contains("larger"));
    }

    #[test]
    fn the_sheet_has_the_address_the_bridge_and_the_words() {
        let recipient = [0x3c, 0x2f, 0x2c, 0x4b, 0xae, 0xc7, 0x08, 0xae, 0xe2, 0x32, 0x37, 0x9e, 0xb0, 0x0e, 0x9e, 0xcf, 0xa1, 0xb3, 0x51, 0xf8];
        let shown = sheet(&decode(&context(V4_CFG, [0; 20], 10_000_000_000_000_000, recipient, 0)).unwrap());
        assert_eq!(shown["recipient"], "0x3c2f2c4baec708aee232379eb00e9ecfa1b351f8");
        assert_eq!((shown["bridge_label"].as_str(), shown["vault"].as_str(), shown["symbol"].as_str()), (Some("bridge v4"), Some("0xc2c6a059eab928966b24fa5549d441a3b17b54ed"), Some("wETH")));
        // The vector "v4 vault, recipient A, no relay fee".
        assert_eq!(shown["words_text"], "myth \u{b7} nerve \u{b7} session \u{b7} until");
        let with_fee = sheet(&decode(&context(V4_CFG, [0; 20], 1, recipient, 5)).unwrap());
        assert!(with_fee["words_text"].is_null() && with_fee["words_why"].as_str().unwrap().contains("relay fee"));
        let unknown = sheet(&decode(&context(&"11".repeat(32), [0; 20], 1, recipient, 0)).unwrap());
        assert!(unknown["bridge_label"].is_null() && unknown["words_why"].as_str().unwrap().contains("doesn't know"));
    }

    #[test]
    fn approved_recipients_are_kept_per_app_and_account() {
        let dir = std::env::temp_dir().join(format!("names-exit-recipients-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let a = "0x3C2F2C4BAEC708AEE232379EB00E9ECFA1B351F8";
        assert!(!is_approved(&dir, "lez_bridge", "Public/A", a));
        approve(&dir, "lez_bridge", "Public/A", a).unwrap();
        assert!(is_approved(&dir, "lez_bridge", "Public/A", &a.to_ascii_lowercase()), "any case");
        assert!(!is_approved(&dir, "lez_bridge", "Public/B", a), "another account");
        assert!(!is_approved(&dir, "other_app", "Public/A", a), "another app");
        assert!(!is_approved(&dir, "lez_bridge", "Public/A", "0x0000000000000000000000000000000000000001"));
        approve(&dir, "lez_bridge", "Public/A", a).unwrap();
        assert_eq!(approved_of(&dir, "lez_bridge"), json!({ "Public/A": ["0x3c2f2c4baec708aee232379eb00e9ecfa1b351f8"] }), "kept once");
        assert!(approve(&dir, "lez_bridge", "Public/A", "0x00").is_err());
        assert!(approve(&dir, "lez_bridge", "Public/A", &format!("0x{}", "0".repeat(40))).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
