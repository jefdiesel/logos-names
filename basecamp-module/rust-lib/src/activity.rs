//! What the Wallet did and saw, kept on this computer (`activity.json` in the module's
//! folder): sends, splits and bridges on the Logos Blockchain, payments and moves on LEZ,
//! and money seen arriving. Neither chain keeps a history the module can ask for (no
//! indexer is wired in), so this is the app's own record: what it sent, followed until
//! it settles, and every rise in a balance between two reads.
//!
//! An entry: `{id, at, kind, chain, from?, to?, name?, amount?, hash?, job?, fee?,
//! state}`. `kind` is `send`, `split`, `bridge`, `pay`, `pay_account`, `shield`,
//! `receive`, `token_send` or `arrived`; `chain` is `logos` or `lez`; `state` is
//! `sending`, `in_block`, `arrived`, `done` or `failed`.

use serde_json::{json, Map, Value};
use std::path::Path;

/// Entries kept; past this the oldest go.
pub const MAX_ENTRIES: usize = 500;

fn file(dir: &Path) -> std::path::PathBuf {
    dir.join("activity.json")
}

fn balances_file(dir: &Path) -> std::path::PathBuf {
    dir.join("activity-balances.json")
}

fn load(path: &Path) -> Map<String, Value> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|saved| saved.as_object().cloned())
        .unwrap_or_default()
}

fn save(path: &Path, object: &Map<String, Value>) {
    let _ = crate::cli::write_private(path, Value::Object(object.clone()).to_string().as_bytes());
}

/// Adds `entry` at time `at` and returns its id.
pub fn add(dir: &Path, at: u64, mut entry: Value) -> String {
    let mut all = load(&file(dir));
    let kind = entry["kind"].as_str().unwrap_or("entry").to_owned();
    let mut id = format!("{at}-{kind}");
    while all.contains_key(&id) {
        id.push('1');
    }
    entry["id"] = Value::from(id.clone());
    entry["at"] = Value::from(at);
    if entry["state"].is_null() {
        entry["state"] = Value::from("sending");
    }
    while all.len() >= MAX_ENTRIES {
        let Some(oldest) = all.iter().min_by_key(|(_, e)| e["at"].as_u64().unwrap_or(0)).map(|(k, _)| k.clone()) else { break };
        all.remove(&oldest);
    }
    all.insert(id.clone(), entry);
    save(&file(dir), &all);
    id
}

/// Merges `fields` into entry `id`.
pub fn update(dir: &Path, id: &str, fields: &Value) {
    let mut all = load(&file(dir));
    if let (Some(entry), Some(fields)) = (all.get_mut(id).and_then(Value::as_object_mut), fields.as_object()) {
        for (key, value) in fields {
            entry.insert(key.clone(), value.clone());
        }
        save(&file(dir), &all);
    }
}

/// Every entry, newest first.
pub fn all(dir: &Path) -> Vec<Value> {
    let mut rows: Vec<Value> = load(&file(dir)).into_values().collect();
    rows.sort_by_key(|entry| std::cmp::Reverse(entry["at"].as_u64().unwrap_or(0)));
    rows
}

/// Whether `entry` involves `account` (a LEZ account `Public/…`/`Private/…`, a node
/// key, or a name).
pub fn involves(entry: &Value, account: &str) -> bool {
    let account = account.trim();
    account.is_empty() || ["from", "to", "name"].iter().any(|field| entry[*field].as_str() == Some(account))
}

/// Notes what `chain`'s accounts hold now, `(account, balance)`: every account that holds
/// more than it did at the last read gets an `arrived` entry for the difference. The
/// first read of an account only sets its mark. Returns the entries added.
pub fn note_balances(dir: &Path, at: u64, chain: &str, balances: &[(String, u128)]) -> Vec<String> {
    let mut seen = load(&balances_file(dir));
    let mut added = Vec::new();
    let mut changed = false;
    for (account, balance) in balances {
        let key = format!("{chain}:{account}");
        let before = seen.get(&key).and_then(Value::as_str).and_then(|text| text.parse::<u128>().ok());
        if before != Some(*balance) {
            changed = true;
            seen.insert(key, Value::from(balance.to_string()));
        }
        if let Some(before) = before {
            if *balance > before {
                added.push(add(
                    dir,
                    at,
                    json!({ "kind": "arrived", "chain": chain, "to": account, "amount": (balance - before).to_string(), "state": "arrived" }),
                ));
            }
        }
    }
    if changed {
        save(&balances_file(dir), &seen);
    }
    added
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("names-activity-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn entries_are_kept_newest_first_and_updated_in_place() {
        let d = dir("order");
        let a = add(&d, 10, json!({ "kind": "send", "chain": "logos", "from": "k1", "to": "k2", "amount": "5" }));
        let b = add(&d, 20, json!({ "kind": "bridge", "chain": "logos", "from": "k1", "to": "Public/X", "amount": "7" }));
        update(&d, &a, &json!({ "state": "in_block" }));
        let rows = all(&d);
        assert_eq!(rows.iter().map(|e| e["id"].as_str().unwrap()).collect::<Vec<_>>(), [b.as_str(), a.as_str()]);
        assert_eq!(rows[1]["state"], "in_block");
        assert_eq!(rows[0]["state"], "sending");
        assert!(involves(&rows[0], "Public/X") && !involves(&rows[1], "Public/X") && involves(&rows[1], ""));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn a_rise_in_a_balance_is_an_arrival_and_the_first_read_only_sets_the_mark() {
        let d = dir("arrive");
        assert!(note_balances(&d, 1, "lez", &[("Public/X".into(), 100)]).is_empty());
        assert!(note_balances(&d, 2, "lez", &[("Public/X".into(), 100)]).is_empty());
        assert!(note_balances(&d, 3, "lez", &[("Public/X".into(), 40)]).is_empty(), "a fall is not an arrival");
        let added = note_balances(&d, 4, "lez", &[("Public/X".into(), 1_000_040)]);
        assert_eq!(added.len(), 1);
        let rows = all(&d);
        assert_eq!((rows[0]["kind"].as_str(), rows[0]["amount"].as_str(), rows[0]["to"].as_str()), (Some("arrived"), Some("1000000"), Some("Public/X")));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn only_so_many_entries_are_kept() {
        let d = dir("cap");
        for at in 0..(MAX_ENTRIES as u64 + 5) {
            add(&d, at, json!({ "kind": "send" }));
        }
        let rows = all(&d);
        assert_eq!(rows.len(), MAX_ENTRIES);
        assert_eq!(rows.last().unwrap()["at"], 5, "the oldest went first");
        let _ = std::fs::remove_dir_all(d);
    }
}
