//! A burn's Ethereum address, checked (`propose_relayed_call` with `opening`; LP0001 job B's main gap).
//!
//! A public burn to a bridge carries only the exit commitment
//! `E = SHA256("RFP022/exit" | k' | ethDest | token | u256(denom) | u256(maxRelayFee) | cfgId)`,
//! so the approval sheet could show where the ETH goes only as the asking app said it. With an
//! OPENING (version 1, rfp-022 `modules/NAMES-SEND-BACK.md` §3.3a), the LEZ Bridge sends E's inputs
//! but k': the recipient, the token, the denomination, the max relay fee, the cfgId and the exit key's
//! index, and the chain id, vault and relay-fee choice for the four words. The wallet makes k' itself
//! (the same app secret it gives that app for that account: scope `RFP022/exit`, context
//! `cfgId | token | u256(denom) | ethDest | u256(maxRelayFee)`, tag `RFP022/ek`, that index) and
//! recomputes E:
//! - equal to the burn's memo: the sheet shows the address and its four words as CHECKED;
//! - different: the call is refused, nothing is shown to approve;
//! - no opening (an older LEZ Bridge), or no grant to make k' with: the sheet says the address is
//!   sealed and can't be checked, as before.
//!
//! The four words are computed here (the withdraw-to code's: SHA-256 of `logos-bridge/withdraw-to/v1`
//! and the version-1 payload, then the compare words), never taken from the app. The address is shown
//! in lower case (this module has no Keccak for EIP-55); the words are the check against the bridge page.
//! Golden vector: `testdata/burn-opening-v1.json`, a byte-for-byte copy of rfp-022's (generator
//! `modules/tools/make-burn-opening-vector.py`, stdlib).

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

/// The opening's one version.
pub const VERSION: u64 = 1;
/// The app secret k' is asked with.
pub const SCOPE: &str = "RFP022/exit";
pub const TAG: &str = "RFP022/ek";
/// The BIP-39 English list (MIT, `data/bip39-english.LICENSE`), the same file as rfp-022's.
const LIST: &str = include_str!("../data/bip39-english.txt");
/// The list's SHA-256, as rfp-022 pins it.
pub const LIST_SHA256: &str = "2f5eed53a4727b4bf8880d8f3f199efc90e58503646d9ff8eff3a2ed3b24dbda";

/// An opening, read and checked for shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opening {
    pub recipient: [u8; 20],
    pub token: [u8; 20],
    pub denom: u128,
    pub max_relay_fee: u128,
    pub cfg_id: [u8; 32],
    pub index: u64,
    pub chain_id: u64,
    pub vault: [u8; 20],
    pub fee_choice: u8,
}

fn sha256(parts: &[&[u8]]) -> [u8; 32] {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}

fn hex_bytes<const N: usize>(value: &Value, what: &str) -> Result<[u8; N], String> {
    let text = value.as_str().ok_or_else(|| format!("the opening's {what} is missing"))?;
    let digits = text.trim().strip_prefix("0x").or_else(|| text.trim().strip_prefix("0X")).unwrap_or(text.trim());
    if digits.len() != 2 * N || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("the opening's {what} isn't {N} bytes of hex"));
    }
    let mut out = [0u8; N];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&digits[2 * i..2 * i + 2], 16).map_err(|_| format!("the opening's {what} isn't hex"))?;
    }
    Ok(out)
}

fn amount(value: &Value, what: &str) -> Result<u128, String> {
    let text = value.as_str().ok_or_else(|| format!("the opening's {what} is missing"))?;
    if text.is_empty() || text.len() > 39 || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!("the opening's {what} isn't a whole number"));
    }
    text.parse().map_err(|_| format!("the opening's {what} is too large"))
}

/// Reads an opening; anything else than version 1 with every field well formed is refused.
pub fn parse(value: &Value) -> Result<Opening, String> {
    let fields = value.as_object().ok_or("the opening is a JSON object")?;
    const KNOWN: [&str; 10] = ["v", "recipient", "token", "denom", "max_relay_fee", "cfg_id", "index", "chain_id", "vault", "fee_choice"];
    if let Some(other) = fields.keys().find(|k| !KNOWN.contains(&k.as_str())) {
        return Err(format!("the opening has no field {}", crate::text::clean_text(other, 20)));
    }
    if value["v"].as_u64() != Some(VERSION) {
        return Err("the opening is of a version this wallet doesn't read".to_owned());
    }
    let fee_choice = value["fee_choice"].as_u64().filter(|f| *f <= 4).ok_or("the opening's relay-fee choice isn't 0 to 4")?;
    Ok(Opening {
        recipient: hex_bytes(&value["recipient"], "recipient")?,
        token: hex_bytes(&value["token"], "token")?,
        denom: amount(&value["denom"], "denomination")?,
        max_relay_fee: amount(&value["max_relay_fee"], "max relay fee")?,
        cfg_id: hex_bytes(&value["cfg_id"], "cfgId")?,
        index: value["index"].as_u64().ok_or("the opening's index isn't a number")?,
        chain_id: value["chain_id"].as_u64().filter(|c| *c > 0).ok_or("the opening's chain id isn't a number")?,
        vault: hex_bytes(&value["vault"], "vault")?,
        fee_choice: fee_choice as u8,
    })
}

fn u256(v: u128) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[16..].copy_from_slice(&v.to_be_bytes());
    out
}

/// The context k' is asked with: `cfgId | token | u256(denom) | ethDest | u256(maxRelayFee)`.
pub fn context(o: &Opening) -> Vec<u8> {
    [&o.cfg_id[..], &o.token, &u256(o.denom), &o.recipient, &u256(o.max_relay_fee)].concat()
}

/// E from k' and the opening.
pub fn exit_commitment(k: &[u8; 32], o: &Opening) -> [u8; 32] {
    sha256(&[b"RFP022/exit", k, &o.recipient, &o.token, &u256(o.denom), &u256(o.max_relay_fee), &o.cfg_id])
}

/// The four words of the address on that vault, as the bridge page shows them.
pub fn words_text(o: &Opening) -> String {
    let mut payload = vec![1u8];
    payload.extend(o.chain_id.to_be_bytes());
    payload.extend(o.vault);
    payload.extend(o.recipient);
    payload.push(o.fee_choice);
    let digest = sha256(&[b"logos-bridge/withdraw-to/v1", &payload]);
    let h = sha256(&[b"logos-bridge/compare/v1", &digest]);
    let top = u64::from_be_bytes([0, 0, h[0], h[1], h[2], h[3], h[4], h[5]]) >> 4;
    let words: Vec<&str> = LIST.lines().collect();
    [33u32, 22, 11, 0].iter().map(|shift| words[((top >> shift) & 0x7ff) as usize]).collect::<Vec<_>>().join(" \u{b7} ")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Splits `opening` off a relayed call's JSON (the call itself goes on unchanged; an older wallet
/// refuses a call with a field it doesn't know, so the LEZ Bridge sends it only when `burn_opening`
/// is in this module's features). `(call_json, opening)`; text that isn't a JSON object goes on as it is.
pub fn split(call_json: &str) -> (String, Option<Value>) {
    match serde_json::from_str::<Value>(call_json) {
        Ok(Value::Object(mut fields)) => match fields.remove("opening") {
            Some(opening) => (Value::Object(fields).to_string(), Some(opening)),
            None => (call_json.to_owned(), None),
        },
        _ => (call_json.to_owned(), None),
    }
}

/// What the burn itself says, against which the opening's token and amount are checked: the
/// Ethereum token of the burned definition (from the bridge's Config) and the amount burned, with the
/// token's decimals and symbol for the fee line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Burned {
    pub eth_token: [u8; 20],
    pub amount: u128,
    pub decimals: u8,
    pub symbol: String,
}

/// The most relay fee a vault takes: 2% of the denomination (`BridgeVault.MAX_FEE_BPS`, rfp-022
/// `lez-bridge/client/src/kdf.rs` `max_relay_fee_cap`).
pub fn max_relay_fee_cap(denom: u128) -> u128 {
    (denom / 10_000) * 200 + (denom % 10_000) * 200 / 10_000
}

/// The opening against the burn: the same token and amount, a fee the vault could take and that
/// agrees with the fee choice. Each refusal is the wallet not signing.
pub fn matches_burn(o: &Opening, burned: &Burned) -> Result<(), String> {
    if o.token != burned.eth_token {
        return Err("the burn's opening names another token than the one burned, so the wallet doesn't sign it".to_owned());
    }
    if o.denom != burned.amount {
        return Err("the burn's opening names another amount than the one burned, so the wallet doesn't sign it".to_owned());
    }
    if o.max_relay_fee > max_relay_fee_cap(o.denom) {
        return Err("the burn's opening lets a relay keep more than a vault allows, so the wallet doesn't sign it".to_owned());
    }
    if (o.fee_choice == 0) != (o.max_relay_fee == 0) {
        return Err("the burn's opening has a relay fee that doesn't agree with its fee choice, so the wallet doesn't sign it".to_owned());
    }
    Ok(())
}

/// What the sheet shows: the checked address and its words, and what a relay may keep of the amount,
/// when E matches the burn's memo; an error (the call refused) when it doesn't.
pub fn checked(o: &Opening, k: &[u8; 32], memo: &[u8; 32], burned: &Burned) -> Result<Value, String> {
    if exit_commitment(k, o) != *memo {
        return Err("the burn's Ethereum address doesn't match its exit commitment, so the wallet doesn't sign it".to_owned());
    }
    let address = format!("0x{}", hex(&o.recipient));
    let fee = crate::bridge_send::shown(o.max_relay_fee, burned.decimals);
    let left = crate::bridge_send::shown(o.denom.saturating_sub(o.max_relay_fee), burned.decimals);
    Ok(json!({
        "v": VERSION, "checked": true, "recipient": address, "recipient_halves": [&address[..22], &address[22..]],
        "words_text": words_text(o), "chain_id": o.chain_id, "vault": format!("0x{}", hex(&o.vault)),
        "amount": o.denom.to_string(), "max_relay_fee": o.max_relay_fee.to_string(), "fee_choice": o.fee_choice,
        "max_relay_fee_text": format!("{fee} {}", burned.symbol), "at_least_text": format!("{left} {}", burned.symbol),
    }))
}

/// What the sheet shows when the address can't be checked (no opening, or no grant to make k').
pub fn unchecked(why: &str) -> Value {
    json!({ "v": VERSION, "checked": false, "why": why })
}

/// The sheet's `burn_opening` for a burn whose message holds `memo`: unchecked without an opening;
/// refused when the opening is malformed or doesn't give `memo`; checked when it does. `key` makes k'
/// for the opening (the CLI's app secret), or None when the wallet may not (no grant for that app).
///
/// `bridge`: the burn's own bridge as its Config says (`bridge_net::decode_config_identity`: cfgId,
/// vault, chain id), or None when it can't be read now. E binds the cfgId but not the vault or the
/// chain the four words are made of, so all three must be the bridge's own, or an app could pick a
/// vault that gives another address someone's familiar words.
///
/// `burned`: the burn's own token and amount ([`Burned`]), or None when the Config's token list
/// can't be read now. E binds the token, the amount and the relay fee, but the app chose them: they
/// must be the burn's own, and the fee is shown.
pub fn for_burn(opening: Option<&Value>, memo: &[u8; 32], bridge: Option<([u8; 32], [u8; 20], u64)>, burned: Option<&Burned>, key: impl FnOnce(&Opening) -> Result<Option<[u8; 32]>, String>) -> Result<Value, String> {
    let Some(opening) = opening else {
        return Ok(unchecked("This LEZ Bridge sends no opening: the address is sealed in the burn and can't be checked here."));
    };
    let o = parse(opening).map_err(|why| format!("{why}; the wallet doesn't sign the burn"))?;
    let (Some((cfg, vault, chain)), Some(burned)) = (bridge, burned) else {
        return Ok(unchecked("The address can't be checked now: the bridge's Config couldn't be read."));
    };
    if (o.cfg_id, o.vault, o.chain_id) != (cfg, vault, chain) {
        return Err("the burn's opening names another bridge than the one the burn goes to, so the wallet doesn't sign it".to_owned());
    }
    matches_burn(&o, burned)?;
    match key(&o)? {
        Some(k) => checked(&o, &k, memo, burned),
        None => Ok(unchecked("The address can't be checked: this app has no exit values of this account to make it with.")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VECTOR: &str = include_str!("../testdata/burn-opening-v1.json");

    fn burned_of(o: &Opening) -> Burned {
        Burned { eth_token: o.token, amount: o.denom, decimals: 18, symbol: "wETH".to_owned() }
    }

    fn key(text: &str) -> [u8; 32] {
        hex_bytes(&Value::from(text), "k").unwrap()
    }

    #[test]
    fn the_word_list_is_bip39_english() {
        assert_eq!(hex(&sha256(&[LIST.as_bytes()])), LIST_SHA256);
        assert_eq!(LIST.lines().count(), 2048);
    }

    #[test]
    fn an_opening_gives_the_burn_s_exit_commitment_and_the_page_s_words() {
        let v: Value = serde_json::from_str(VECTOR).unwrap();
        for c in v["cases"].as_array().unwrap() {
            let o = parse(&c["opening"]).unwrap();
            assert_eq!(hex(&context(&o)), c["context"].as_str().unwrap(), "{}", c["name"]);
            let k = key(c["k"].as_str().unwrap());
            let memo = hex_bytes::<32>(&c["exit"], "exit").unwrap();
            assert_eq!(exit_commitment(&k, &o), memo);
            assert_eq!(words_text(&o), c["words_text"].as_str().unwrap());
            let shown = checked(&o, &k, &memo, &burned_of(&o)).unwrap();
            assert_eq!(shown["checked"], true);
            assert_eq!(shown["recipient"], c["opening"]["recipient"].as_str().unwrap().to_lowercase());
            // Another key (another wallet), another address, another fee: refused.
            assert!(checked(&o, &[7; 32], &memo, &burned_of(&o)).unwrap_err().contains("doesn't match"));
            let mut moved = o.clone();
            moved.recipient[0] ^= 1;
            assert!(checked(&moved, &k, &memo, &burned_of(&moved)).is_err());
            let mut fee = o.clone();
            fee.max_relay_fee += 1;
            assert!(checked(&fee, &k, &memo, &burned_of(&fee)).is_err());
        }
    }

    #[test]
    fn a_malformed_opening_is_refused_and_split_leaves_the_call_as_it_was() {
        let v: Value = serde_json::from_str(VECTOR).unwrap();
        let good = v["cases"][0]["opening"].clone();
        for (field, bad, words) in [("v", json!(2), "version"), ("recipient", json!("0x12"), "20 bytes"), ("denom", json!("1.5"), "whole number"),
                                    ("fee_choice", json!(5), "0 to 4"), ("index", json!("x"), "index"), ("chain_id", json!(0), "chain id")] {
            let mut o = good.clone();
            o[field] = bad;
            assert!(parse(&o).unwrap_err().contains(words), "{field}");
        }
        let mut extra = good.clone();
        extra["secret"] = json!("x");
        assert!(parse(&extra).unwrap_err().contains("no field"));
        let call = json!({ "program": "P", "message": "00", "opening": good }).to_string();
        let (rest, opening) = split(&call);
        assert_eq!(serde_json::from_str::<Value>(&rest).unwrap(), json!({ "program": "P", "message": "00" }));
        assert_eq!(opening, Some(good));
        assert_eq!(split("{\"program\":\"P\"}"), ("{\"program\":\"P\"}".to_owned(), None));
        assert_eq!(split("not json").1, None);
        assert_eq!(unchecked("x")["checked"], false);
    }

    #[test]
    fn the_sheet_s_value_for_each_case() {
        let v: Value = serde_json::from_str(VECTOR).unwrap();
        let c = &v["cases"][1];
        let memo = hex_bytes::<32>(&c["exit"], "exit").unwrap();
        let k = key(c["k"].as_str().unwrap());
        // Checked: k' asked with the opening's index and context.
        let o = parse(&c["opening"]).unwrap();
        let bridge = Some((o.cfg_id, o.vault, o.chain_id));
        let burned = burned_of(&o);
        let shown = for_burn(Some(&c["opening"]), &memo, bridge, Some(&burned), |o| {
            assert_eq!((o.index, hex(&context(o))), (7, c["context"].as_str().unwrap().to_owned()));
            Ok(Some(k))
        })
        .unwrap();
        assert_eq!((shown["checked"].as_bool(), shown["words_text"].as_str()), (Some(true), c["words_text"].as_str()));
        // No opening (an older LEZ Bridge), or no grant: sealed, said so.
        assert_eq!(for_burn(None, &memo, bridge, Some(&burned), |_| panic!("no key is made without an opening")).unwrap()["checked"], false);
        assert_eq!(for_burn(Some(&c["opening"]), &memo, bridge, Some(&burned), |_| Ok(None)).unwrap()["checked"], false);
        // The bridge's Config unread: unchecked; another vault, chain or cfgId than the burn's own: refused
        // (the words would be another vault's: an app could grind them to look familiar).
        assert_eq!(for_burn(Some(&c["opening"]), &memo, None, Some(&burned), |_| panic!("not asked")).unwrap()["checked"], false);
        for other in [Some((o.cfg_id, [0x26; 20], o.chain_id)), Some((o.cfg_id, o.vault, 1)), Some(([1; 32], o.vault, o.chain_id))] {
            assert!(for_burn(Some(&c["opening"]), &memo, other, Some(&burned), |_| panic!("not asked")).unwrap_err().contains("another bridge"));
        }
        // Another wallet's k', or a malformed opening: refused.
        assert!(for_burn(Some(&c["opening"]), &memo, bridge, Some(&burned), |_| Ok(Some([9; 32]))).unwrap_err().contains("doesn't match"));
        assert!(for_burn(Some(&json!({ "v": 1 })), &memo, bridge, Some(&burned), |_| Ok(Some(k))).unwrap_err().contains("doesn't sign the burn"));
        // The burn's own token, amount and fee (review 2026-11-19): another token or amount than the one
        // burned, a fee past 2%, or a fee that disagrees with the choice, each with a matching E: refused.
        let mut other_token = burned.clone();
        other_token.eth_token = [0xac; 20];
        assert!(for_burn(Some(&c["opening"]), &memo, bridge, Some(&other_token), |_| panic!("not asked")).unwrap_err().contains("another token"));
        let mut other_amount = burned.clone();
        other_amount.amount += 1;
        assert!(for_burn(Some(&c["opening"]), &memo, bridge, Some(&other_amount), |_| panic!("not asked")).unwrap_err().contains("another amount"));
        let mut greedy = c["opening"].clone();
        greedy["max_relay_fee"] = json!((max_relay_fee_cap(o.denom) + 1).to_string());
        assert!(for_burn(Some(&greedy), &memo, bridge, Some(&burned), |_| panic!("not asked")).unwrap_err().contains("more than a vault allows"));
        let mut mismatched = c["opening"].clone();
        mismatched["fee_choice"] = json!(0);
        assert!(for_burn(Some(&mismatched), &memo, bridge, Some(&burned), |_| panic!("not asked")).unwrap_err().contains("fee choice"));
        // The Config's token list unread: unchecked.
        assert_eq!(for_burn(Some(&c["opening"]), &memo, bridge, None, |_| panic!("not asked")).unwrap()["checked"], false);
        // The fee is shown with what the address gets at least.
        assert_eq!((shown["max_relay_fee_text"].as_str(), shown["at_least_text"].as_str()), (Some("0.002 wETH"), Some("0.998 wETH")));
        // A wallet that can't make k' now (the tool failed): the call isn't shown.
        assert_eq!(for_burn(Some(&c["opening"]), &memo, bridge, Some(&burned), |_| Err("busy".to_owned())), Err("busy".to_owned()));
    }
}
