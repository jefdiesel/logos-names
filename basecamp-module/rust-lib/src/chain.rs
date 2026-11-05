//! The Logos Blockchain in the Names app (LP-0022), through Basecamp's own node: the
//! `blockchain_module`, asked over Logos Core like `program_registry`. The node holds
//! the keys and proves each transfer; this module only reads its replies and asks it
//! to act, so nothing here talks to any other service.
//!
//! Staking on the Logos Blockchain (Cryptarchia) has no stake operation: every note on
//! a key the node holds plays the leader lottery once it is in an epoch's stake
//! snapshot, and leader wins pay vouchers that a claim turns into notes.
//!
//! The replies are read as the node module gives them (0.3.4: amounts as decimal text,
//! a note's `id`) and as the node's HTTP API does (numbers, `note_id`), since the module
//! passes some of them through.

use serde_json::{json, Value};

/// The node's module in Basecamp.
#[cfg_attr(test, allow(dead_code))]
pub const MODULE: &str = "blockchain_module";

/// The text entry by which a name publishes its Logos Blockchain public key (SPEC.md §7).
pub const BLOCKCHAIN_KEY: &str = "blockchain";

/// What a `blockchain_module` call answered: its value, parsed when it is JSON text,
/// or its error in words a person can act on.
pub fn unwrap(method: &str, answer: Value) -> Result<Value, String> {
    let answer = match answer.as_str().map(serde_json::from_str::<Value>) {
        Some(Ok(parsed)) if parsed.is_object() => parsed,
        _ => answer,
    };
    let Some(success) = answer.get("success").and_then(Value::as_bool) else {
        return Ok(answer);
    };
    if !success {
        let error = match &answer["error"] {
            Value::String(text) => text.clone(),
            Value::Null => "no reason given".to_owned(),
            other => other.to_string(),
        };
        return Err(problem(method, &error));
    }
    Ok(match &answer["value"] {
        Value::String(text) => serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.clone())),
        other => other.clone(),
    })
}

/// A node error in plain words.
pub fn problem(method: &str, error: &str) -> String {
    let lower = error.to_ascii_lowercase();
    if lower.contains("node is not running") || lower.contains("not initialized") {
        return "Your Logos Blockchain node isn't running. Start it in Basecamp's Blockchain app.".to_owned();
    }
    if lower.contains("insufficient") || lower.contains("not enough") {
        return "That key doesn't hold enough for this and its fee.".to_owned();
    }
    format!("the Logos Blockchain node refused {method}: {error}")
}

/// A JSON number or decimal text, as the node writes values.
pub fn amount(value: &Value) -> u128 {
    match value {
        Value::Number(n) => n.as_u64().map(u128::from).unwrap_or(0),
        Value::String(s) => s.trim().parse().unwrap_or(0),
        _ => 0,
    }
}

/// A Logos Blockchain public key, lowercase, or `None`.
pub fn parse_key(input: &str) -> Option<String> {
    let key = input.trim().trim_start_matches("0x").to_lowercase();
    (key.len() == 64 && key.bytes().all(|b| b.is_ascii_hexdigit())).then_some(key)
}

/// A balance reply (decimal text, or `{balance}` from the HTTP shape).
pub fn balance_of(reply: &Value) -> u128 {
    if reply.is_object() {
        amount(&reply["balance"])
    } else {
        amount(reply)
    }
}

/// The node's state for the app, from `get_cryptarchia_info` and `get_time_info`.
pub fn status(info: &Value, time: &Value) -> Value {
    let chain = if info["cryptarchia_info"].is_object() { &info["cryptarchia_info"] } else { info };
    json!({
        "height": chain["height"],
        "slot": chain["slot"],
        "lib_slot": chain["lib_slot"],
        "mode": chain["mode"].as_str().or(chain["state"].as_str()),
        "epoch": time["current_epoch"],
    })
}

/// The stake picture, from `wallet_get_leader_aged_notes`, `wallet_get_claimable_vouchers`
/// and `get_time_info`: `{staked, notes, keys, epoch, next_epoch_in_s, claimable,
/// vouchers, reward_per_voucher}`, amounts as decimal text. `next_epoch_in_s` is null
/// when the node doesn't say how long an epoch is.
pub fn stake(aged: &Value, vouchers: &Value, time: &Value) -> Value {
    let notes = aged["notes"].as_array().cloned().unwrap_or_default();
    let mut keys: Vec<String> = notes.iter().filter_map(|n| n["public_key"].as_str().map(str::to_owned)).collect();
    keys.sort();
    keys.dedup();
    let summed: u128 = notes.iter().map(|n| amount(&n["value"])).sum();
    let staked = match amount(&aged["total_value"]) {
        0 => summed,
        total => total,
    };
    let slot = time["current_slot"].as_u64();
    let per_epoch = time["slots_per_epoch"].as_u64().filter(|n| *n > 0);
    let slot_ms = time["slot_duration_ms"].as_u64().unwrap_or(1000);
    let next = slot.zip(per_epoch).map(|(slot, per)| (per - slot % per) * slot_ms / 1000);
    json!({
        "staked": staked.to_string(),
        "notes": notes.len(),
        "keys": keys,
        "epoch": time["current_epoch"],
        "next_epoch_in_s": next,
        "claimable": amount(&vouchers["total_claimable"]).to_string(),
        "vouchers": vouchers["vouchers"].as_array().map_or(0, Vec::len),
        "reward_per_voucher": amount(&vouchers["reward_amount"]).to_string(),
    })
}

/// Checks a send before anything is asked of the node: `(from, to, amount)`.
pub fn send_input(from_key: &str, to_key: &str, amount: &str) -> Result<(String, String, u64), String> {
    let from = parse_key(from_key).ok_or("choose the key to send from")?;
    let to = parse_key(to_key).ok_or("a Logos Blockchain key is 64 hex characters")?;
    if from == to {
        return Err("that sends a key to itself".to_owned());
    }
    let amount = amount.trim().replace(',', "");
    if amount.is_empty() || !amount.bytes().all(|b| b.is_ascii_digit()) {
        return Err("enter a whole amount, such as 1000".to_owned());
    }
    let amount: u64 = amount.parse().map_err(|_| "that amount is too large".to_owned())?;
    if amount == 0 {
        return Err("enter an amount above zero".to_owned());
    }
    Ok((from, to, amount))
}

/// A transaction hash as the node writes it (64 hex), from a reply that is the hash
/// itself or `{hash}` / `{tx_hash}`.
pub fn hash_of(reply: &Value) -> Option<String> {
    let text = reply.as_str().or(reply["hash"].as_str()).or(reply["tx_hash"].as_str())?;
    parse_key(text)
}

/// The fee a stored transaction paid, if its JSON shows one.
pub fn fee_of(transaction: &Value) -> Option<u128> {
    match transaction {
        Value::Object(map) => {
            for key in ["tx_fee", "fee", "total_fee"] {
                if let Some(found) = map.get(key).map(amount).filter(|a| *a > 0) {
                    return Some(found);
                }
            }
            map.values().find_map(fee_of)
        }
        Value::Array(items) => items.iter().find_map(fee_of),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "66ecf4bccaa13e4e03c3d5c0e3ebecb945c51bbf815e92727a9a4efa6b39a81b";

    /// The replies as the M2 node's module (0.3.4) gave them over logosctl.
    #[test]
    fn module_replies_unwrap() {
        let balance = json!({ "error": null, "success": true, "value": "1191220464" });
        assert_eq!(balance_of(&unwrap("wallet_get_balance", balance).unwrap()), 1_191_220_464);
        let keys = json!({ "error": null, "success": true, "value": [KEY] });
        assert_eq!(unwrap("wallet_get_known_addresses", keys).unwrap(), json!([KEY]));
        let info = json!({ "error": null, "success": true, "value": "{\"height\":66467,\"lib_slot\":2247925,\"mode\":\"Online\",\"slot\":2286493}" });
        let time = json!({ "success": true, "value": "{\"current_epoch\":7,\"current_slot\":2286536}" });
        let shown = status(&unwrap("get_cryptarchia_info", info).unwrap(), &unwrap("get_time_info", time).unwrap());
        assert_eq!(shown, json!({ "height": 66467, "slot": 2_286_493, "lib_slot": 2_247_925, "mode": "Online", "epoch": 7 }));
        // A reply that arrives as JSON text is read the same.
        let text = json!(r#"{"success":true,"value":"5"}"#);
        assert_eq!(balance_of(&unwrap("wallet_get_balance", text).unwrap()), 5);
    }

    #[test]
    fn module_errors_read_as_advice() {
        let down = json!({ "success": false, "value": null, "error": "The node is not running." });
        assert!(unwrap("wallet_get_balance", down).unwrap_err().contains("isn't running"));
        let other = json!({ "success": false, "error": "Keystore file exists." });
        assert_eq!(unwrap("generate_key", other).unwrap_err(), "the Logos Blockchain node refused generate_key: Keystore file exists.");
    }

    #[test]
    fn stake_from_the_modules_replies() {
        let aged = json!({ "notes": [
            { "id": "2d", "public_key": KEY, "value": "1919959" },
            { "id": "89", "public_key": KEY, "value": "1897565" },
        ] });
        let vouchers = json!({ "reward_amount": "13140", "total_claimable": "0", "vouchers": [] });
        let time = json!({ "current_epoch": 7, "current_slot": 2_286_536 });
        let shown = stake(&aged, &vouchers, &time);
        assert_eq!(shown["staked"], "3817524");
        assert_eq!(shown["keys"], json!([KEY]));
        assert_eq!(shown["next_epoch_in_s"], Value::Null, "the module's time info has no epoch length");
        assert_eq!(shown["reward_per_voucher"], "13140");
    }

    #[test]
    fn stake_from_the_http_replies() {
        let aged = json!({ "notes": [{ "note_id": "2d", "value": 5, "public_key": KEY }], "count": 1, "total_value": 5 });
        let vouchers = json!({ "vouchers": [{ "commitment": "c", "nullifier": "n" }], "reward_amount": 13140, "total_claimable": 13140 });
        let time = json!({ "slot_duration_ms": 1000, "current_slot": 2_286_020, "current_epoch": 7, "slots_per_epoch": 300_000 });
        let shown = stake(&aged, &vouchers, &time);
        assert_eq!(shown["staked"], "5");
        assert_eq!(shown["vouchers"], 1);
        assert_eq!(shown["claimable"], "13140");
        assert_eq!(shown["next_epoch_in_s"], 113_980);
    }

    #[test]
    fn sends_are_checked_first() {
        let other = "ab".repeat(32);
        assert_eq!(send_input(KEY, &other, "1,000"), Ok((KEY.to_owned(), other.clone(), 1000)));
        assert!(send_input(KEY, KEY, "1").is_err());
        assert!(send_input(KEY, &other, "0").is_err());
        assert!(send_input(KEY, &other, "1.5").is_err());
        assert!(send_input("", &other, "1").is_err());
        assert!(send_input(KEY, "alice", "1").is_err(), "names are resolved before this");
    }

    #[test]
    fn hashes_and_fees_read() {
        assert_eq!(hash_of(&json!(KEY)), Some(KEY.to_owned()));
        assert_eq!(hash_of(&json!({ "tx_hash": KEY })), Some(KEY.to_owned()));
        assert_eq!(hash_of(&json!("nope")), None);
        assert_eq!(fee_of(&json!({ "mantle_tx": { "tx_fee": "1234" } })), Some(1234));
        assert_eq!(fee_of(&json!({ "ops": [] })), None);
    }
}
