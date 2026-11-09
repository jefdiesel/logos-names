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

/// What a file of ours holds. One that is there but can't be read is never written over:
/// it is set aside as `<file>.corrupt-<time>` and an empty one started; `damaged` tells.
/// The module's record of Logos Blockchain sends (`chain-sent.json`) is read this way too.
pub fn load(path: &Path) -> Map<String, Value> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Map::new(),
        Err(_) => String::new(),
    };
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(saved)) => saved,
        _ => {
            let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
            let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
            let mut aside = path.with_file_name(format!("{name}{CORRUPT}{at}"));
            while aside.exists() {
                aside = aside.with_extension(format!("{}1", aside.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default()));
            }
            let _ = std::fs::rename(path, &aside);
            Map::new()
        }
    }
}

/// What a file set aside is called after its own name.
const CORRUPT: &str = ".corrupt-";

/// The history files found damaged since this was last asked (each told once): why,
/// in words, or `None`.
pub fn damaged(dir: &Path) -> Option<String> {
    damage(dir).map(|(_, why)| why)
}

/// What `damaged` tells, and whether the history itself (`activity.json`) was lost: only
/// then is the list a new one. A damaged file of balance marks costs nothing listed
/// (corrupt-balances-file-fails-whole-activity-call).
pub fn damage(dir: &Path) -> Option<(bool, String)> {
    let mut found: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if (name.starts_with("activity") || name.starts_with("chain-sent")) && name.contains(CORRUPT) && !name.ends_with(".told") {
            let _ = std::fs::rename(entry.path(), dir.join(format!("{name}.told")));
            found.push(format!("{name}.told"));
        }
    }
    found.sort();
    let of = |prefix: &str| found.iter().filter(|name| name.starts_with(prefix)).cloned().collect::<Vec<_>>();
    let (history, marks, sends) = (of("activity.json"), of("activity-balances.json"), of("chain-sent.json"));
    let mut why = Vec::new();
    if !history.is_empty() {
        why.push(format!(
            "The Wallet's history file couldn't be read, so it was kept aside ({}, in {}) and a new history started.",
            history.join(", "),
            dir.display()
        ));
    }
    if !marks.is_empty() {
        why.push(format!(
            "The file of what each account held at the last read couldn't be read, so it was kept aside ({}, in {}); the history is intact, and money that arrived before this read may not be listed.",
            marks.join(", "),
            dir.display()
        ));
    }
    if !sends.is_empty() {
        why.push(format!(
            "The record of the Logos Blockchain sends made here couldn't be read, so it was kept aside ({}, in {}); a send still on its way is followed through the node.",
            sends.join(", "),
            dir.display()
        ));
    }
    (!why.is_empty()).then(|| (!history.is_empty(), why.join(" ")))
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
    // Entries made in the same second keep the order they were made in.
    entry["seq"] = Value::from(next_seq(&all));
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

/// One more than any entry's `seq`.
fn next_seq(all: &Map<String, Value>) -> u64 {
    all.values().filter_map(|entry| entry["seq"].as_u64()).max().map_or(0, |seq| seq + 1)
}

/// Every entry, newest first (by time, then by the order made).
pub fn all(dir: &Path) -> Vec<Value> {
    let mut rows: Vec<Value> = load(&file(dir)).into_values().collect();
    rows.sort_by_key(|entry| std::cmp::Reverse((entry["at"].as_u64().unwrap_or(0), entry["seq"].as_u64().unwrap_or(0))));
    rows
}

/// Whether `entry` involves `account` (a LEZ account `Public/…`/`Private/…`, a node
/// key, or a name).
pub fn involves(entry: &Value, account: &str) -> bool {
    let account = account.trim();
    account.is_empty() || ["from", "to", "name"].iter().any(|field| entry[*field].as_str() == Some(account)) || received(entry, account).is_some()
}

/// What a claim (`receive`) credited `account` with, from its `received` map.
fn received(entry: &Value, account: &str) -> Option<u128> {
    number(&entry["received"][account])
}

/// How long a bridge in a block holds its LEZ account's mark while it waits to arrive
/// (it takes about 12 hours); past this it is taken as never arriving.
pub const BRIDGE_WAIT_S: u64 = 2 * 24 * 3600;

/// How long a send on the Logos Blockchain may stay `sending` (not yet seen in a block)
/// before its outcome is taken as unknown: they land in a minute or two.
pub const SENDING_WAIT_S: u64 = 30 * 60;

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
    let age = now.saturating_sub(entry["at"].as_u64().unwrap_or(0));
    match entry["state"].as_str() {
        // A Logos Blockchain send lands in minutes; one still "sending" past the bound
        // (nothing could settle it) no longer holds the marks of its keys.
        Some("sending") => entry["chain"] != "logos" || age < SENDING_WAIT_S,
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
    // Whether it landed isn't known (its job record was pruned): it moves no expected
    // balance, and what it did shows only as a balance not being certain. Counted as
    // landed, a failed send of 99,999,999,999,999,999 became an arrival of at least
    // that much (failed-send-becomes-done-after-job-prune-regressed).
    if state == "unknown" {
        return (0, false);
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
    // A claim credits what waited to each account it names, and its payer pays only the fee.
    if kind == "receive" {
        let credited = received(entry, account);
        let mut delta = i128::try_from(credited.unwrap_or(0)).unwrap_or(i128::MAX);
        let mut known = true;
        if from {
            match fee_paid(entry) {
                Some(fee) => delta -= i128::try_from(fee).unwrap_or(i128::MAX),
                None => known = false,
            }
        }
        return (delta, known);
    }
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

/// The bridges in a block among `entries` that have reached their LEZ account, by what
/// the accounts hold now (`lez_now`). Per account they arrive in the order they were
/// sent: each once the account holds what it did before the first of them was sent
/// (`to_before`), plus what the logged entries settled since then moved in or out, plus
/// its amount and those of the bridges before it. One credit never marks several
/// bridges arrived, nor one bridge a credit smaller than it.
pub fn bridges_arrived(entries: &[Value], lez_now: &std::collections::HashMap<String, u128>) -> Vec<String> {
    bridge_updates(entries, lez_now).into_iter().filter(|(_, fields)| fields["state"] == "arrived").map(|(id, _)| id).collect()
}

/// What settling the bridges in a block among `entries` changes, `(id, fields)`. LEZ credits
/// a deposit whole, so a bridge arrives only on a credit of exactly its amount, seen one of
/// two ways:
///
/// - the account holds exactly what it did before the first followed bridge was sent
///   (`to_before`), plus what the logged entries settled since moved in or out, plus the
///   amounts of the bridges up to this one (the bridges arrive in the order they were sent);
/// - between two quiet reads (nothing logged in flight) it rose, beyond what the logged
///   entries explain, by exactly this bridge's amount, or the amounts of the first few
///   waiting: a spend the history doesn't log no longer holds it back
///   (bridge-stuck-in-block-after-unlogged-spend-from-destination). The reads are kept on
///   the waiting bridges (`seen`, `seen_moved`).
///
/// A credit larger than the amounts is never taken for them: a 700,000 payment from
/// elsewhere marked a 500,001 bridge arrived (single-bridge-false-arrival-on-foreign-credit-at-least-amount).
/// This reverses the whole-sum rule aa94d3b kept ("all of them once the account holds at
/// least what they bring"): a bridge whose credit is hidden by other unlogged money now
/// waits rather than claim arrival. Several bridges matched by one rise can't be told from
/// one payment of their total, so they arrive with `maybe` (the History hedges them).
/// Once some arrive, the next one waiting is followed from what the account holds now
/// less what the logged entries moved since it was sent (`to_before`), if that is more
/// than its own mark (false-arrival-second-bridge-regressed).
pub fn bridge_updates(entries: &[Value], lez_now: &std::collections::HashMap<String, u128>) -> Vec<(String, Value)> {
    let mut pending: std::collections::BTreeMap<&str, Vec<&Value>> = std::collections::BTreeMap::new();
    for entry in entries {
        if entry["kind"] == "bridge" && entry["state"] == "in_block" && number(&entry["amount"]).is_some() {
            if let Some(to) = entry["to"].as_str() {
                pending.entry(to).or_default().push(entry);
            }
        }
    }
    let amount = |bridge: &Value| i128::try_from(number(&bridge["amount"]).unwrap_or(0)).unwrap_or(i128::MAX);
    let mut updates = Vec::new();
    for (to, mut list) in pending {
        let Some(now) = lez_now.get(to).and_then(|now| i128::try_from(*now).ok()) else { continue };
        list.sort_by_key(|entry| (entry["at"].as_u64().unwrap_or(0), entry["id"].as_str().unwrap_or_default().to_owned()));
        // What the logged entries settled from `since` (and before `until`) moved in or out of `to`.
        let moved_between = |since: u64, until: u64| -> i128 {
            entries
                .iter()
                .filter(|entry| entry["kind"] != "bridge" && (since..until).contains(&entry["at"].as_u64().unwrap_or(0)))
                .filter(|entry| !matches!(entry["state"].as_str(), Some("sending")))
                .map(|entry| effect(entry, to).0)
                .sum()
        };
        let mut arrived: Vec<usize> = Vec::new();
        let mut maybe = false;
        // By the mark: the first few whose credits the account holds exactly.
        if let Some(start) = list.iter().position(|entry| number(&entry["to_before"]).is_some()) {
            let since = list[start]["at"].as_u64().unwrap_or(0);
            let base = i128::try_from(number(&list[start]["to_before"]).unwrap_or(0)).unwrap_or(i128::MAX);
            let mut needed = base.saturating_add(moved_between(since, u64::MAX));
            let thresholds: Vec<i128> = list[start..]
                .iter()
                .map(|bridge| {
                    needed = needed.saturating_add(amount(bridge));
                    needed
                })
                .collect();
            if let Some(at) = thresholds.iter().rposition(|needed| now == *needed) {
                arrived.extend(start..=start + at);
                maybe |= at > 0;
            }
        }
        // By the rise since the last quiet read: what the account holds less all that the
        // logged entries moved, against the same at that read.
        let quiet = !entries.iter().any(|entry| entry["state"] == "sending" && (entry["from"].as_str() == Some(to) || entry["to"].as_str() == Some(to)));
        let moved_now = moved_between(0, u64::MAX);
        let last_read = list.iter().find_map(|bridge| {
            let seen = number(&bridge["seen"]).and_then(|seen| i128::try_from(seen).ok())?;
            Some((seen, bridge["seen_moved"].as_str()?.parse::<i128>().ok()?))
        });
        if quiet && arrived.is_empty() {
            if let Some((seen, seen_moved)) = last_read {
                let rise = (now - moved_now) - (seen - seen_moved);
                if rise > 0 {
                    if let Some(one) = list.iter().position(|bridge| amount(bridge) == rise) {
                        arrived.push(one);
                    } else {
                        let mut total = 0_i128;
                        if let Some(count) = list.iter().position(|bridge| {
                            total = total.saturating_add(amount(bridge));
                            total == rise
                        }) {
                            arrived.extend(0..=count);
                            maybe = true;
                        }
                    }
                }
            }
        }
        for at in &arrived {
            if let Some(id) = list[*at]["id"].as_str() {
                let fields = if maybe { json!({ "state": "arrived", "maybe": true }) } else { json!({ "state": "arrived" }) };
                updates.push((id.to_owned(), fields));
            }
        }
        let waiting: Vec<&Value> = list.iter().enumerate().filter(|(at, _)| !arrived.contains(at)).map(|(_, bridge)| *bridge).collect();
        // The next one waiting, after some arrived: followed from what the account holds now.
        let mut next_mark = None;
        if let (false, Some(next)) = (arrived.is_empty(), waiting.first()) {
            let reached = now - moved_between(next["at"].as_u64().unwrap_or(0), u64::MAX);
            let own = number(&next["to_before"]).and_then(|own| i128::try_from(own).ok());
            if own.is_none_or(|own| own < reached) {
                next_mark = Some(reached.max(0).to_string());
            }
        }
        // The read is kept on those still waiting, when nothing logged was in flight.
        for (index, bridge) in waiting.iter().enumerate() {
            let Some(id) = bridge["id"].as_str() else { continue };
            let mut fields = serde_json::Map::new();
            if let (0, Some(mark)) = (index, &next_mark) {
                fields.insert("to_before".to_owned(), json!(mark));
                fields.insert("followed".to_owned(), json!(true));
            }
            if quiet && (number(&bridge["seen"]) != u128::try_from(now).ok() || bridge["seen_moved"].as_str() != Some(moved_now.to_string().as_str())) {
                fields.insert("seen".to_owned(), json!(now.max(0).to_string()));
                fields.insert("seen_moved".to_owned(), json!(moved_now.to_string()));
            }
            if !fields.is_empty() {
                updates.push((id.to_owned(), Value::Object(fields)));
            }
        }
    }
    updates
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
            .filter(|(_, entry)| entry["from"].as_str() == Some(account) || entry["to"].as_str() == Some(account) || received(entry, account).is_some())
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
                let mut entry = json!({ "id": id, "at": at, "seq": next_seq(&all), "kind": "arrived", "chain": chain, "to": account, "state": "arrived", "counted": [account] });
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

    /// false-arrival-second-bridge: bridges of 2,000,000, 700,000 and 3,000,000 to one
    /// account all flipped to arrived on one credit of 3,000,000 (each tested alone
    /// against `to_before + amount`).
    #[test]
    fn one_credit_marks_bridges_arrived_in_order_and_only_as_far_as_it_goes() {
        let bridge = |at: u64, amount: &str| json!({ "id": format!("{at}-bridge"), "at": at, "kind": "bridge", "chain": "logos", "to": "Public/T", "amount": amount, "to_before": "0", "state": "in_block" });
        let mut entries = vec![bridge(1, "3000000"), bridge(2, "2000000"), bridge(3, "700000")];
        let now = |held: u128| std::collections::HashMap::from([("Public/T".to_owned(), held)]);
        assert!(bridges_arrived(&entries, &now(0)).is_empty());
        assert!(bridges_arrived(&entries, &now(1_000_000)).is_empty(), "less than the first");
        assert!(bridges_arrived(&entries, &now(3_500_000)).is_empty(), "more than the first, but not its credit alone");
        assert_eq!(bridges_arrived(&entries, &now(3_000_000)), ["1-bridge"]);
        assert_eq!(bridges_arrived(&entries, &now(5_000_000)), ["1-bridge", "2-bridge"]);
        assert_eq!(bridges_arrived(&entries, &now(5_700_000)).len(), 3);
        // More than they all bring is not their credits (single-bridge-false-arrival-on-foreign-credit-at-least-amount;
        // this reverses aa94d3b's "all of them once it holds at least what they bring").
        assert!(bridges_arrived(&entries, &now(9_000_000)).is_empty(), "more than all of them bring");
        // br/f2: bridges of 700,000, 3,000,000 and 2,000,000 and one credit of 3,000,000 from elsewhere: none.
        let f2 = vec![bridge(1, "700000"), bridge(2, "3000000"), bridge(3, "2000000")];
        assert!(bridges_arrived(&f2, &now(3_000_000)).is_empty());
        assert_eq!(bridges_arrived(&f2, &now(700_000)), ["1-bridge"]);
        // A logged payment out of the account since then is allowed for.
        entries.push(json!({ "id": "4-pay_account", "at": 4, "kind": "pay_account", "chain": "lez", "from": "Public/T", "to": "Public/Q", "amount": "500000", "fee": { "paid": "4000" }, "state": "done" }));
        assert_eq!(bridges_arrived(&entries, &now(2_496_000)), ["1-bridge"]);
        // Once the first has arrived, the next is followed from where it was sent.
        entries[0]["state"] = json!("arrived");
        entries[1]["to_before"] = json!("3000000");
        assert!(bridges_arrived(&entries, &now(3_000_000)).is_empty(), "{:?}", bridges_arrived(&entries, &now(3_000_000)));
        assert_eq!(bridges_arrived(&entries, &now(4_496_000)), ["2-bridge"]);
    }

    /// receive-entry-has-no-account-amount: a claim's entry named no account, so the payer
    /// BF (36,929,384 -> 23,619,017) and the credited 9Dk3 (954,121 -> 4,123,256) showed nothing.
    #[test]
    fn a_claim_is_on_its_payers_and_its_credited_accounts_pages_and_explains_their_balances() {
        let d = dir("claim");
        note(&d, 1, &[("Private/BF", 36_929_384), ("Private/9Dk3", 954_121)]);
        add(&d, 2, json!({ "kind": "receive", "chain": "lez", "all": true, "from": "Private/BF", "amount": "3169135",
            "received": { "Private/9Dk3": "3169135" }, "fee": { "about": "5070616", "paid": "10141232" }, "state": "done" }));
        let entry = all(&d).remove(0);
        assert!(involves(&entry, "Private/BF") && involves(&entry, "Private/9Dk3") && !involves(&entry, "Private/X"));
        // The credit is the claim's, not money that arrived; the payer's fall is no arrival either.
        assert!(note(&d, 3, &[("Private/BF", 23_619_017), ("Private/9Dk3", 4_123_256)]).is_empty(), "{:?}", arrivals(&d));
        let _ = std::fs::remove_dir_all(d);
    }

    /// failed-send-becomes-done-after-job-prune-regressed (G2): a send of
    /// 99,999,999,999,999,999 that failed, its job record then pruned, was settled
    /// `unknown`, still counted as spent, and the next read logged an arrival of at least
    /// the whole amount.
    #[test]
    fn a_send_whose_outcome_is_unknown_moves_no_expected_balance() {
        let d = dir("unknown");
        note(&d, 1, &[("Public/A", 1_000_000)]);
        add(&d, 2, json!({ "kind": "pay_account", "chain": "lez", "from": "Public/A", "to": "Public/B", "amount": "99999999999999999", "fee": null, "state": "unknown" }));
        assert!(note(&d, 3, &[("Public/A", 1_000_000)]).is_empty(), "{:?}", arrivals(&d));
        assert!(note(&d, 4, &[("Public/A", 1_000_000)]).is_empty(), "{:?}", arrivals(&d));
        // Money that does come in later is told, from the mark the reads reset.
        assert_eq!(note(&d, 5, &[("Public/A", 1_000_500)]).len(), 1);
        assert_eq!(arrivals(&d)[0]["amount"], "500");
        let _ = std::fs::remove_dir_all(d);
    }

    /// false-arrival-second-bridge-regressed (f2b): bridges of 3,000,000 then 2,000,000 to
    /// one account, both sent before either was credited (each `to_before` 0). LEZ credited
    /// 3,000,000: the first poll marked the 3,000,000 arrived, the next poll marked the
    /// 2,000,000 arrived too (judged alone against its own mark, 0 + 2,000,000 <= 3,000,000).
    #[test]
    fn a_second_bridge_is_not_taken_as_arrived_on_the_first_ones_credit() {
        let d = dir("second-bridge");
        let first = add(&d, 10, json!({ "kind": "bridge", "chain": "logos", "to": "Public/T", "amount": "3000000", "to_before": "0", "state": "in_block" }));
        let second = add(&d, 11, json!({ "kind": "bridge", "chain": "logos", "to": "Public/T", "amount": "2000000", "to_before": "0", "state": "in_block" }));
        let held = std::collections::HashMap::from([("Public/T".to_owned(), 3_000_000_u128)]);
        let poll = |d: &Path| {
            for (id, fields) in bridge_updates(&all(d), &held) {
                update(d, &id, &fields);
            }
            let rows = all(d);
            let state = |id: &str| rows.iter().find(|e| e["id"] == id).map(|e| e["state"].as_str().unwrap_or_default().to_owned()).unwrap();
            (state(&first), state(&second))
        };
        assert_eq!(poll(&d), ("arrived".to_owned(), "in_block".to_owned()));
        for _ in 0..3 {
            assert_eq!(poll(&d), ("arrived".to_owned(), "in_block".to_owned()), "a later poll on the same credit");
        }
        // Read back from the file (a reload): the second now waits for its own 2,000,000.
        assert_eq!(all(&d).iter().find(|e| e["id"] == second).unwrap()["to_before"], "3000000");
        let held = std::collections::HashMap::from([("Public/T".to_owned(), 5_000_000_u128)]);
        assert_eq!(bridges_arrived(&all(&d), &held), [second.clone()]);
        // A mark already past the first one's credit (it landed before the second was sent) is kept.
        let entries = vec![
            json!({ "id": "1-bridge", "at": 1, "kind": "bridge", "to": "Public/U", "amount": "3000000", "to_before": "0", "state": "in_block" }),
            json!({ "id": "2-bridge", "at": 2, "kind": "bridge", "to": "Public/U", "amount": "2000000", "to_before": "3000000", "state": "in_block" }),
        ];
        let held = std::collections::HashMap::from([("Public/U".to_owned(), 3_000_000_u128)]);
        let updates = bridge_updates(&entries, &held);
        assert_eq!(updates[0], ("1-bridge".to_owned(), json!({ "state": "arrived" })));
        assert!(updates[1..].iter().all(|(id, fields)| id == "2-bridge" && fields.get("to_before").is_none() && fields.get("state").is_none()), "{updates:?}");
        let _ = std::fs::remove_dir_all(d);
    }

    /// Settles `entries` the way the module does on each balance read: every update applied.
    fn poll(entries: &mut [Value], to: &str, held: u128) {
        let now = std::collections::HashMap::from([(to.to_owned(), held)]);
        for (id, fields) in bridge_updates(entries, &now) {
            let entry = entries.iter_mut().find(|entry| entry["id"] == id.as_str()).expect("an entry");
            for (key, value) in fields.as_object().unwrap() {
                entry[key] = value.clone();
            }
        }
    }

    fn states(entries: &[Value]) -> Vec<(String, bool)> {
        entries.iter().filter(|e| e["kind"] == "bridge").map(|e| (e["state"].as_str().unwrap().to_owned(), e["maybe"] == true)).collect()
    }

    fn bridge_to(at: u64, to: &str, amount: &str, to_before: Option<&str>) -> Value {
        json!({ "id": format!("{at}-bridge"), "at": at, "kind": "bridge", "chain": "logos", "to": to, "amount": amount, "to_before": to_before, "state": "in_block" })
    }

    /// single-bridge-false-arrival-on-foreign-credit-at-least-amount (S8, S7, S4): one payment
    /// of 700,000 from elsewhere marked a 500,001 bridge arrived; one of 1,200,006 marked three.
    #[test]
    fn a_credit_from_elsewhere_is_not_a_bridges_arrival() {
        let arrived = |held: &[u128], bridges: Vec<Value>| {
            let mut entries = bridges;
            for held in held {
                poll(&mut entries, "Public/T", *held);
            }
            states(&entries)
        };
        // S8: 0, then a payment of 700,000 from elsewhere.
        assert_eq!(arrived(&[0, 700_000], vec![bridge_to(1, "Public/T", "500001", Some("0"))]), [("in_block".to_owned(), false)]);
        // S4: 1,000 from elsewhere first, then the bridge's own credit.
        assert_eq!(arrived(&[0, 1_000, 501_001], vec![bridge_to(1, "Public/T", "500001", Some("0"))]), [("arrived".to_owned(), false)]);
        // S7: three bridges and one payment of exactly their total: it can't be told from
        // their credits, so they arrive hedged.
        let three = vec![bridge_to(1, "Public/T", "300003", Some("0")), bridge_to(2, "Public/T", "400002", Some("0")), bridge_to(3, "Public/T", "500001", Some("0"))];
        assert_eq!(arrived(&[0, 1_200_006], three.clone()), vec![("arrived".to_owned(), true); 3]);
        assert_eq!(arrived(&[0, 1_300_006], three), vec![("in_block".to_owned(), false); 3], "more than their total");
    }

    /// bridge-stuck-in-block-after-unlogged-spend-from-destination (S5): T held 20,000, spent
    /// 4,788 outside the History, then the bridge's 500,001 came: it stayed in a block.
    #[test]
    fn a_bridge_arrives_on_its_own_credit_after_an_unlogged_spend() {
        let mut entries = vec![bridge_to(1, "Public/T", "500001", Some("20000"))];
        for held in [20_000, 15_212] {
            poll(&mut entries, "Public/T", held);
        }
        assert_eq!(states(&entries), [("in_block".to_owned(), false)]);
        poll(&mut entries, "Public/T", 515_213);
        assert_eq!(states(&entries), [("arrived".to_owned(), false)]);
        // A logged send between the reads is allowed for.
        let mut entries = vec![bridge_to(1, "Public/V", "500001", Some("20000"))];
        poll(&mut entries, "Public/V", 20_000);
        entries.push(json!({ "id": "2-pay", "at": 2, "kind": "pay_account", "chain": "lez", "from": "Public/V", "to": "Public/Q", "amount": "5000", "fee": { "paid": "100" }, "state": "done" }));
        poll(&mut entries, "Public/V", 514_901);
        assert_eq!(states(&entries), [("arrived".to_owned(), false)]);
    }

    /// bridge-with-unread-to_before-never-arrives (S6b): a bridge sent while its account
    /// couldn't be read (to_before null), then a second one: the first's credit marked the
    /// second arrived.
    #[test]
    fn an_unfollowed_bridges_credit_is_not_taken_for_a_later_ones() {
        let mut entries = vec![bridge_to(1, "Public/T", "500001", None), bridge_to(2, "Public/T", "400002", Some("0"))];
        poll(&mut entries, "Public/T", 0);
        poll(&mut entries, "Public/T", 500_001);
        assert_eq!(states(&entries), [("arrived".to_owned(), false), ("in_block".to_owned(), false)]);
        poll(&mut entries, "Public/T", 900_003);
        assert_eq!(states(&entries), [("arrived".to_owned(), false), ("arrived".to_owned(), false)]);
    }

    /// corrupt-activity-file-silently-erases-history: a truncated activity.json read as
    /// empty, and the next send wrote a history of one entry over the other four.
    #[test]
    fn a_history_that_cant_be_read_is_kept_aside_and_told_never_overwritten() {
        let d = dir("corrupt");
        for at in 1..=4 {
            add(&d, at, json!({ "kind": "pay_account", "amount": "11" }));
        }
        let whole = std::fs::read_to_string(file(&d)).unwrap();
        std::fs::write(file(&d), &whole[..whole.len() - 1]).unwrap();
        std::fs::write(balances_file(&d), "{\"lez@x:Public/F\":\"5").unwrap();
        add(&d, 5, json!({ "kind": "pay_account", "amount": "12" }));
        note(&d, 6, &[("Public/F", 9)]);
        assert_eq!(all(&d).len(), 1, "a new history");
        let why = damaged(&d).expect("told");
        assert!(why.contains("activity.json.corrupt-") && why.contains("activity-balances.json.corrupt-"), "{why}");
        assert!(damaged(&d).is_none(), "told once");
        let kept: Vec<String> = std::fs::read_dir(&d).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.starts_with("activity.json.corrupt-")).collect();
        assert_eq!(std::fs::read_to_string(d.join(&kept[0])).unwrap(), whole[..whole.len() - 1], "the four entries are kept as they were");
        let _ = std::fs::remove_dir_all(d);
    }

    /// corrupt-balances-file-fails-whole-activity-call (G4): with only
    /// activity-balances.json damaged, activity() failed saying the history was lost and a
    /// new one started, though activity.json was intact.
    #[test]
    fn a_damaged_file_of_marks_is_a_warning_not_a_lost_history() {
        let d = dir("marks");
        add(&d, 1, json!({ "kind": "pay_account", "amount": "11" }));
        std::fs::write(balances_file(&d), "{\"lez@x:Public/F\":\"5").unwrap();
        note(&d, 2, &[("Public/F", 9)]);
        let (history_lost, why) = damage(&d).expect("told");
        assert!(!history_lost, "{why}");
        assert!(why.contains("activity-balances.json.corrupt-") && why.contains("the history is intact") && !why.contains("new history"), "{why}");
        assert_eq!(all(&d).len(), 1, "the history is all there");
        assert!(damage(&d).is_none(), "told once");
        let _ = std::fs::remove_dir_all(d);
    }

    /// activity-same-second-order: three pays 0.25 s apart (1, 2, 3) were listed 2, 3, 1.
    #[test]
    fn entries_made_in_the_same_second_are_newest_first() {
        let d = dir("second");
        add(&d, 1794168079, json!({ "kind": "pay_account", "amount": "1" }));
        add(&d, 1794168080, json!({ "kind": "pay_account", "amount": "2" }));
        add(&d, 1794168080, json!({ "kind": "pay_account", "amount": "3" }));
        add(&d, 1794168080, json!({ "kind": "bridge", "amount": "4" }));
        let order: Vec<String> = all(&d).iter().map(|e| e["amount"].as_str().unwrap().to_owned()).collect();
        assert_eq!(order, ["4", "3", "2", "1"]);
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
