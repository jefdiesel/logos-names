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

/// How long a bridge in a block holds its LEZ account's mark while it waits to arrive
/// (it takes about 12 hours); past this it is taken as never arriving.
pub const BRIDGE_WAIT_S: u64 = 2 * 24 * 3600;

/// A number an entry carries as decimal text.
fn number(value: &Value) -> Option<u128> {
    value.as_str().and_then(|text| text.parse().ok())
}

/// The fee an entry paid: decimal text (Logos Blockchain sends) or `{paid}` (`cli::parse_fee`).
fn fee_paid(entry: &Value) -> Option<u128> {
    number(&entry["fee"]).or_else(|| number(&entry["fee"]["paid"]))
}

/// Whether `entry`'s effect on `account` may or may not be in a balance read yet: it is
/// still being sent, or it is a bridge in a block that hasn't reached its LEZ account.
fn in_flight(entry: &Value, account: &str, now: u64) -> bool {
    match entry["state"].as_str() {
        Some("sending") => true,
        Some("in_block") => {
            entry["kind"] == "bridge" && entry["to"].as_str() == Some(account) && now.saturating_sub(entry["at"].as_u64().unwrap_or(0)) < BRIDGE_WAIT_S
        }
        _ => false,
    }
}

/// What a settled `entry` did to `account`'s balance, and whether that is known: what
/// it moved in or out of it (in the native token) and the fee it paid from it.
fn effect(entry: &Value, account: &str) -> (i128, bool) {
    let state = entry["state"].as_str().unwrap_or_default();
    let to = entry["to"].as_str() == Some(account);
    let from = entry["from"].as_str() == Some(account);
    if state == "failed" {
        return (0, true);
    }
    // A bridge that never arrived moved nothing into its LEZ account.
    if to && !from && entry["kind"] == "bridge" && state != "arrived" {
        return (0, true);
    }
    let kind = entry["kind"].as_str().unwrap_or_default();
    // A split pays itself; a token send moves a token, not the native one; a transfer moves a name.
    let native = !matches!(kind, "split" | "token_send" | "transfer" | "receive_privately");
    let amount = number(&entry["amount"]);
    let mut certain = state != "unknown" && (amount.is_some() || !native || !(to || from));
    let mut delta: i128 = 0;
    if native {
        let amount = i128::try_from(amount.unwrap_or(0)).unwrap_or(i128::MAX);
        if to {
            delta += amount;
        }
        if from {
            delta -= amount;
        }
    }
    // A shield's fee is paid inside its proof, from the private account it moves to.
    let payer = if kind == "shield" { to } else { from };
    if payer && kind != "arrived" {
        match fee_paid(entry) {
            Some(fee) => delta -= i128::try_from(fee).unwrap_or(i128::MAX),
            None => certain = false,
        }
    }
    (delta, certain)
}

/// Notes what `chain`'s accounts hold now, `(account, balance)`, as read from `scope`
/// (the sequencer, for LEZ): an account that holds more than its last mark plus what the
/// entries settled since then moved in, less what they moved out and paid, gets an
/// `arrived` entry for the rest. Its `amount` is null (and `at_least` says how much)
/// when a fee or an outcome among those entries isn't known. An account that some entry
/// is still moving money for keeps its mark until that settles; the first read of an
/// account only sets its mark; a public LEZ account read as 0 that its entries don't
/// explain is not believed (an older CLI printed 0 when the sequencer didn't answer).
/// Returns the entries added.
pub fn note_balances(dir: &Path, at: u64, chain: &str, scope: &str, balances: &[(String, u128)]) -> Vec<String> {
    let mut seen = load(&balances_file(dir));
    let mut all = load(&file(dir));
    let mut added = Vec::new();
    let mut marks_changed = false;
    let mut entries_changed = false;
    for (account, balance) in balances {
        let key = if scope.is_empty() { format!("{chain}:{account}") } else { format!("{chain}@{scope}:{account}") };
        let touching: Vec<String> = all
            .iter()
            .filter(|(_, entry)| entry["from"].as_str() == Some(account) || entry["to"].as_str() == Some(account))
            .filter(|(_, entry)| !entry["counted"].as_array().is_some_and(|counted| counted.iter().any(|c| c.as_str() == Some(account))))
            .map(|(id, _)| id.clone())
            .collect();
        if touching.iter().any(|id| in_flight(&all[id], account, at)) {
            continue;
        }
        let before = seen.get(&key).and_then(Value::as_str).and_then(|text| text.parse::<u128>().ok());
        if let Some(before) = before {
            let mut expected = i128::try_from(before).unwrap_or(i128::MAX);
            let mut certain = true;
            for id in &touching {
                let (delta, known) = effect(&all[id], account);
                expected = expected.saturating_add(delta);
                certain &= known;
            }
            let now = i128::try_from(*balance).unwrap_or(i128::MAX);
            if *balance == 0 && expected > 0 && account.starts_with("Public/") && chain == "lez" {
                continue;
            }
            if now > expected {
                let rise = (now - expected).to_string();
                let mut id = format!("{at}-arrived");
                while all.contains_key(&id) {
                    id.push('1');
                }
                let mut entry = json!({ "id": id, "at": at, "kind": "arrived", "chain": chain, "to": account, "state": "arrived", "counted": [account] });
                if certain {
                    entry["amount"] = Value::from(rise);
                } else {
                    entry["amount"] = Value::Null;
                    entry["at_least"] = Value::from(rise);
                }
                all.insert(id.clone(), entry);
                added.push(id);
                entries_changed = true;
            }
        }
        for id in &touching {
            if let Some(counted) = all.get_mut(id).and_then(|entry| entry.as_object_mut()) {
                let list = counted.entry("counted").or_insert_with(|| Value::Array(Vec::new()));
                if let Some(list) = list.as_array_mut() {
                    list.push(Value::from(account.as_str()));
                    entries_changed = true;
                }
            }
        }
        if before != Some(*balance) {
            marks_changed = true;
            seen.insert(key, Value::from(balance.to_string()));
        }
    }
    if entries_changed {
        while all.len() > MAX_ENTRIES {
            let Some(oldest) = all.iter().min_by_key(|(_, e)| e["at"].as_u64().unwrap_or(0)).map(|(k, _)| k.clone()) else { break };
            all.remove(&oldest);
        }
        save(&file(dir), &all);
    }
    if marks_changed {
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

    const SEQ: &str = "http://127.0.0.1:3800";

    fn note(d: &Path, at: u64, rows: &[(&str, u128)]) -> Vec<String> {
        let rows: Vec<(String, u128)> = rows.iter().map(|(a, b)| ((*a).to_owned(), *b)).collect();
        note_balances(d, at, "lez", SEQ, &rows)
    }

    fn arrivals(d: &Path) -> Vec<Value> {
        all(d).into_iter().filter(|e| e["kind"] == "arrived").collect()
    }

    #[test]
    fn a_rise_in_a_balance_is_an_arrival_and_the_first_read_only_sets_the_mark() {
        let d = dir("arrive");
        assert!(note(&d, 1, &[("Public/X", 100)]).is_empty());
        assert!(note(&d, 2, &[("Public/X", 100)]).is_empty());
        assert!(note(&d, 3, &[("Public/X", 40)]).is_empty(), "a fall is not an arrival");
        let added = note(&d, 4, &[("Public/X", 1_000_040)]);
        assert_eq!(added.len(), 1);
        let rows = all(&d);
        assert_eq!((rows[0]["kind"].as_str(), rows[0]["amount"].as_str(), rows[0]["to"].as_str()), (Some("arrived"), Some("1000000"), Some("Public/X")));
        assert!(note(&d, 5, &[("Public/X", 1_000_040)]).is_empty(), "an arrival is counted once");
        let _ = std::fs::remove_dir_all(d);
    }

    /// act-repro F4: one read with the sequencer down (an older CLI printed 0 for every
    /// public account) set every mark to 0, and the next read logged each whole balance.
    #[test]
    fn a_public_balance_read_as_zero_that_nothing_explains_leaves_the_mark() {
        let d = dir("zero");
        note(&d, 1, &[("Public/F", 9_999_998_718_094), ("Public/A", 1_005_123), ("Private/P", 0)]);
        assert!(note(&d, 2, &[("Public/F", 0), ("Public/A", 0), ("Private/P", 0)]).is_empty());
        assert!(note(&d, 3, &[("Public/F", 9_999_998_718_094), ("Public/A", 1_005_123), ("Private/P", 0)]).is_empty(), "{:?}", arrivals(&d));
        // A send of everything it held does explain a 0.
        add(&d, 4, json!({ "kind": "pay_account", "chain": "lez", "from": "Public/A", "to": "Public/Z", "amount": "1000000", "fee": { "paid": "5123" }, "state": "done" }));
        note(&d, 5, &[("Public/A", 0)]);
        add(&d, 6, json!({ "kind": "arrived", "chain": "lez", "to": "Public/A", "amount": "7", "state": "arrived", "counted": ["Public/A"] }));
        assert_eq!(note(&d, 7, &[("Public/A", 70)]).len(), 1, "the mark moved to 0 with the send");
        assert_eq!(arrivals(&d)[0]["amount"], "70");
        let _ = std::fs::remove_dir_all(d);
    }

    /// act-repro F1: a send between the wallet's own accounts was logged as a pay_account
    /// and also as money that arrived.
    #[test]
    fn a_send_between_own_accounts_is_not_an_arrival() {
        let d = dir("own");
        note(&d, 1, &[("Public/F", 9_999_998_995_648), ("Public/A2", 1_000_000)]);
        let pay = add(&d, 2, json!({ "kind": "pay_account", "chain": "lez", "from": "Public/F", "to": "Public/A2", "amount": "5000" }));
        // Landed, but the job isn't settled yet: neither mark moves.
        assert!(note(&d, 3, &[("Public/F", 9_999_998_986_560), ("Public/A2", 1_005_000)]).is_empty());
        update(&d, &pay, &json!({ "state": "done", "fee": { "about": "4000", "at_most": "6464000", "paid": "4088" } }));
        assert!(note(&d, 4, &[("Public/F", 9_999_998_986_560), ("Public/A2", 1_005_000)]).is_empty(), "{:?}", arrivals(&d));
        assert!(note(&d, 5, &[("Public/F", 9_999_998_986_560), ("Public/A2", 1_005_000)]).is_empty());
        let _ = std::fs::remove_dir_all(d);
    }

    /// act-repro F3: the arrival carried the net change (75648) instead of what came in
    /// (100000), the account having paid 20000 and a fee in the same window.
    #[test]
    fn an_arrival_is_what_came_in_not_the_net_change() {
        let d = dir("net");
        note(&d, 1, &[("Public/F", 1_000_000), ("Public/A3", 0)]);
        add(&d, 2, json!({ "kind": "pay_account", "chain": "lez", "from": "Public/F", "to": "Public/A3", "amount": "20000", "fee": { "paid": "4352" }, "state": "done" }));
        note(&d, 3, &[("Public/F", 1_000_000 - 24_352 + 100_000), ("Public/A3", 20_000)]);
        let came = arrivals(&d);
        assert_eq!((came.len(), came[0]["amount"].as_str(), came[0]["to"].as_str()), (1, Some("100000"), Some("Public/F")), "{came:?}");
        // Not knowing the fee, it claims no figure, only a least one.
        add(&d, 4, json!({ "kind": "pay_account", "chain": "lez", "from": "Public/F", "to": "Public/Q", "amount": "20000", "fee": null, "state": "done" }));
        note(&d, 5, &[("Public/F", 1_075_648 - 24_352 + 100_000)]);
        let came = arrivals(&d);
        assert_eq!((came.len(), came[0]["amount"].clone(), came[0]["at_least"].as_str()), (2, Value::Null, Some("95648")), "{came:?}");
        let _ = std::fs::remove_dir_all(d);
    }

    /// act-repro F7: marks were keyed `lez:<account>`, so the same wallet pointed at
    /// another sequencer logged the difference as an arrival.
    #[test]
    fn marks_are_per_sequencer() {
        let d = dir("seq");
        note(&d, 1, &[("Public/F", 9_999_998_718_094)]);
        assert!(note_balances(&d, 2, "lez", "http://127.0.0.1:3802", &[("Public/F".into(), 10_000_000_000_000)]).is_empty());
        assert!(note(&d, 3, &[("Public/F", 9_999_998_718_094)]).is_empty(), "back on the first: its own mark");
        let _ = std::fs::remove_dir_all(d);
    }

    /// act-repro F8: a bridge that reached its LEZ account was logged as the bridge
    /// arriving and also as an arrival of the same amount.
    #[test]
    fn a_bridge_that_arrives_is_not_also_an_arrival() {
        let d = dir("bridge");
        note(&d, 1, &[("Public/G", 20_010)]);
        let bridge = add(&d, 2, json!({ "kind": "bridge", "chain": "logos", "from": "k".repeat(64), "to": "Public/G", "amount": "3000", "to_before": "20010", "state": "in_block" }));
        assert!(note(&d, 3, &[("Public/G", 23_010)]).is_empty(), "the bridge in a block holds the mark");
        update(&d, &bridge, &json!({ "state": "arrived" }));
        assert!(note(&d, 4, &[("Public/G", 23_010)]).is_empty(), "{:?}", arrivals(&d));
        assert_eq!(note(&d, 5, &[("Public/G", 24_010)]).len(), 1, "later money is still an arrival");
        assert_eq!(arrivals(&d)[0]["amount"], "1000");
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
