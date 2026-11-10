//! What the Wallet did and saw, kept on this computer (`activity.json` in the module's
//! folder): sends, splits and bridges on the Logos Blockchain, payments and moves on LEZ,
//! and money seen arriving. Neither chain keeps a history the module can ask for (no
//! indexer is wired in), so this is the app's own record: what it sent, followed until
//! it settles, and every rise in a balance between two reads.
//!
//! An entry: `{id, at, kind, chain, from?, to?, name?, amount?, hash?, job?, fee?,
//! state}`. `kind` is `send`, `split`, `bridge`, `pay`, `pay_account`, `shield`,
//! `receive`, `token_send` or `arrived`; `chain` is `logos` or `lez`; `state` is
//! `sending`, `in_block`, `arrived`, `done`, `failed`, `unknown`, or `expired` (a bridge
//! not credited on LEZ within `BRIDGE_WAIT_S`). A bridge also carries the LEZ `receipt`
//! that tells its arrival (`bridge_receipt`).
//!
//! What an entry did on LEZ it did on one zone (`zones`): an entry made while an added
//! zone was in use carries its `zone` (the zone's channel); one without is the home
//! zone's. Balance marks are kept per zone too (`note_zone_balances`).

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

/// The zone `entry`'s LEZ side is on: its `zone`, or home.
pub fn zone_of(entry: &Value) -> &str {
    entry["zone"].as_str().unwrap_or(crate::zones::HOME)
}

/// Whether `entry` did something on LEZ: a LEZ entry, or a bridge (sent on the Logos
/// Blockchain, credited on a zone).
pub fn on_lez(entry: &Value) -> bool {
    entry["chain"] == "lez" || entry["kind"] == "bridge"
}

/// `entry`, said to be on `zone`. Home's entries carry no mark, as before zones.
pub fn in_zone(mut entry: Value, zone: &str) -> Value {
    if zone != crate::zones::HOME {
        entry["zone"] = Value::from(zone);
    }
    entry
}

/// Whether `entry` involves `account` (a LEZ account `Public/…`/`Private/…`, a node
/// key, or a name).
pub fn involves(entry: &Value, account: &str) -> bool {
    let account = account.trim();
    account.is_empty()
        || ["from", "to", "name"].iter().any(|field| entry[*field].as_str() == Some(account))
        || received(entry, account).is_some()
        // A claim that left a credit to `account` waiting shows on its page too; it moved nothing.
        || number(&entry["left"][account]).is_some()
}

/// What a claim (`receive`) credited `account` with, from its `received` map.
fn received(entry: &Value, account: &str) -> Option<u128> {
    number(&entry["received"][account])
}

/// How long a bridge in a block may wait to arrive on LEZ (it takes about 12 hours); past
/// this it is `expired`, shown as overdue.
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
/// still being sent. A bridge never holds its LEZ account: whether it arrived is known
/// exactly (its receipt, `bridge_updates`), and until it does it moved nothing into it, so
/// money from elsewhere meanwhile is told as it comes (foreign-credit-while-bridge-waits-never-in-history).
fn in_flight(entry: &Value, account: &str, now: u64) -> bool {
    let age = now.saturating_sub(entry["at"].as_u64().unwrap_or(0));
    if bridge_waits_for(entry, account) {
        return false;
    }
    match entry["state"].as_str() {
        // A Logos Blockchain send lands in minutes; one still "sending" past the bound
        // (nothing could settle it) no longer holds the marks of its keys.
        Some("sending") => entry["chain"] != "logos" || age < SENDING_WAIT_S,
        _ => false,
    }
}

/// Whether `entry` is a bridge into `account` that hasn't arrived and still may: it moves
/// nothing into it yet, and is counted for it only once it arrives (or fails).
fn bridge_waits_for(entry: &Value, account: &str) -> bool {
    entry["kind"] == "bridge" && entry["to"].as_str() == Some(account) && on_its_way(entry)
}

/// Whether a bridge may still arrive: anything but `arrived` and `failed`. One whose send
/// was taken as `unknown` (not seen in a block in time) still arrives by its receipt.
fn on_its_way(entry: &Value) -> bool {
    !matches!(entry["state"].as_str(), Some("arrived" | "failed"))
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
    // A bridge that hasn't arrived moved nothing into its LEZ account, whatever is known of
    // its send: it arrives only by its receipt.
    if to && !from && entry["kind"] == "bridge" && state != "arrived" {
        return (0, true);
    }
    // Whether it landed isn't known (its job record was pruned): it moves no expected
    // balance, and what it did shows only as a balance not being certain. Counted as
    // landed, a failed send of 99,999,999,999,999,999 became an arrival of at least
    // that much (failed-send-becomes-done-after-job-prune-regressed).
    if state == "unknown" {
        return (0, false);
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

/// The LEZ deposit receipt a bridge is followed by (`registry::deposit_receipt`): the one it
/// recorded, or the one its channel, note and destination give (a bridge made by an earlier
/// version, or recorded before a call that was cut off). `None` for a bridge that names
/// none of them: its arrival can't be told from other money.
pub fn bridge_receipt(entry: &Value) -> Option<crate::registry::AccountId> {
    if let Some(receipt) = entry["receipt"].as_str() {
        return crate::registry::parse_account(receipt).ok();
    }
    Some(crate::registry::deposit_receipt(&bridge_op_id(entry)?))
}

/// A bridge's deposit op id (`chain::deposit_op_id`): the one it recorded, or the one its
/// channel, note and metadata give. The op id hashes the metadata sent, so it is the one
/// recorded with the bridge (`metadata`); a bridge recorded without it was made before the
/// metadata depended on LEZ's version, and sent the destination's 32 bytes.
pub fn bridge_op_id(entry: &Value) -> Option<[u8; 32]> {
    if let Some(op) = entry["op_id"].as_str() {
        return <[u8; 32]>::try_from(crate::registry::unhex(op).ok()?).ok();
    }
    let channel = <[u8; 32]>::try_from(crate::registry::unhex(entry["channel"].as_str()?).ok()?).ok()?;
    let metadata = match entry["metadata"].as_str() {
        Some(metadata) => crate::registry::unhex(metadata).ok()?,
        None => crate::registry::parse_account(entry["to"].as_str()?.strip_prefix("Public/")?).ok()?.to_vec(),
    };
    crate::chain::deposit_op_id(&channel, &[entry["note"].as_str()?.to_owned()], &metadata)
}

/// How long a bridge's receipt is still asked after it was sent: an overdue one may still
/// arrive, and is then told as arrived, late.
pub const BRIDGE_FOLLOW_S: u64 = 30 * 24 * 3600;

/// The bridges whose arrival on LEZ is still followed: not arrived, not failed, sent within
/// `BRIDGE_FOLLOW_S`, with a receipt. `(id, to, receipt)`.
pub fn bridges_followed(entries: &[Value], now: u64) -> Vec<(String, String, crate::registry::AccountId)> {
    entries
        .iter()
        .filter(|entry| entry["kind"] == "bridge" && on_its_way(entry))
        .filter(|entry| now.saturating_sub(entry["at"].as_u64().unwrap_or(0)) < BRIDGE_FOLLOW_S)
        .filter_map(|entry| Some((entry["id"].as_str()?.to_owned(), entry["to"].as_str()?.to_owned(), bridge_receipt(entry)?)))
        .collect()
}

/// What settling the bridges among `entries` changes, `(id, fields)`, given what LEZ says of
/// each one's receipt (`minted`: `Some(true)` minted, `Some(false)` not yet, `None` no answer).
///
/// A bridge arrives when, and only when, LEZ has minted its own deposit: the receipt is
/// written in the same LEZ transaction that credits it, once per deposit. No balance, amount
/// or order is matched, so a payment from elsewhere is never taken for a bridge, nor one
/// bridge's credit for another's, and an overdue bridge can't claim a later one's. A bridge
/// still in a block after `BRIDGE_WAIT_S` is `expired` (shown as overdue); its receipt is
/// still asked until `BRIDGE_FOLLOW_S`, and if it is minted then, the bridge arrives (`late`),
/// one row as ever. One whose receipt isn't known (`bridge_receipt` is `None`) is never
/// followed: its credit shows as money that arrived, and it expires the same way.
///
/// An arrival also takes the bridge's account off its `counted` list: a bridge that hasn't
/// arrived moves nothing into it, so a read that counted it (an older version did, past the
/// wait) counted 0, and its credit is counted now.
pub fn bridge_updates(entries: &[Value], now: u64, minted: &dyn Fn(&crate::registry::AccountId) -> Option<bool>) -> Vec<(String, Value)> {
    let mut updates = Vec::new();
    for entry in entries.iter().filter(|entry| entry["kind"] == "bridge") {
        let Some(id) = entry["id"].as_str() else { continue };
        let state = entry["state"].as_str().unwrap_or_default();
        if !on_its_way(entry) {
            continue;
        }
        let age = now.saturating_sub(entry["at"].as_u64().unwrap_or(0));
        let receipt = bridge_receipt(entry);
        let mut fields = serde_json::Map::new();
        if entry["followed"].as_bool() != Some(receipt.is_some()) {
            fields.insert("followed".to_owned(), json!(receipt.is_some()));
        }
        if let (Some(receipt), true) = (receipt, entry["receipt"].is_null()) {
            fields.insert("receipt".to_owned(), json!(crate::registry::account_text(&receipt)));
        }
        let arrived = receipt.filter(|_| age < BRIDGE_FOLLOW_S).and_then(|receipt| minted(&receipt)) == Some(true);
        if arrived {
            fields.insert("state".to_owned(), json!("arrived"));
            if state == "expired" || age >= BRIDGE_WAIT_S {
                fields.insert("late".to_owned(), json!(true));
            }
            if let (Some(to), Some(counted)) = (entry["to"].as_str(), entry["counted"].as_array()) {
                let kept: Vec<Value> = counted.iter().filter(|account| account.as_str() != Some(to)).cloned().collect();
                if kept.len() != counted.len() {
                    fields.insert("counted".to_owned(), Value::Array(kept));
                }
            }
        } else if state == "in_block" && age >= BRIDGE_WAIT_S {
            fields.insert("state".to_owned(), json!("expired"));
        }
        if !fields.is_empty() {
            updates.push((id.to_owned(), Value::Object(fields)));
        }
    }
    updates
}

/// The key an account's balance mark is kept under.
fn mark_key(chain: &str, scope: &str, account: &str) -> String {
    if scope.is_empty() { format!("{chain}:{account}") } else { format!("{chain}@{scope}:{account}") }
}

/// The entries that move money for `account` and aren't counted in its mark yet: those of
/// `zone` only, when one is given (the same account on another zone is other money).
fn touching(all: &Map<String, Value>, account: &str, zone: Option<&str>) -> Vec<String> {
    all.iter()
        .filter(|(_, entry)| entry["from"].as_str() == Some(account) || entry["to"].as_str() == Some(account) || received(entry, account).is_some())
        .filter(|(_, entry)| zone.is_none_or(|zone| zone_of(entry) == zone))
        .filter(|(_, entry)| !entry["counted"].as_array().is_some_and(|counted| counted.iter().any(|c| c.as_str() == Some(account))))
        .map(|(id, _)| id.clone())
        .collect()
}

/// What noting `balance` for `account` makes of it (`note_balances`): `None` when it isn't
/// noted at all (an entry still moves money for it, or a 0 nothing explains), else its last
/// mark and, when that is known and `balance` is above what the entries since explain, the
/// rise and whether it is exact.
#[allow(clippy::too_many_arguments)]
fn rise(
    all: &Map<String, Value>,
    seen: &Map<String, Value>,
    touching: &[String],
    at: u64,
    chain: &str,
    key: &str,
    account: &str,
    balance: u128,
) -> Option<(Option<u128>, Option<(i128, bool)>)> {
    if touching.iter().any(|id| in_flight(&all[id], account, at)) {
        return None;
    }
    let before = seen.get(key).and_then(Value::as_str).and_then(|text| text.parse::<u128>().ok());
    let Some(mark) = before else { return Some((None, None)) };
    let mut expected = i128::try_from(mark).unwrap_or(i128::MAX);
    let mut certain = true;
    for id in touching {
        let (delta, known) = effect(&all[id], account);
        expected = expected.saturating_add(delta);
        certain &= known;
    }
    let now = i128::try_from(balance).unwrap_or(i128::MAX);
    if balance == 0 && expected > 0 && account.starts_with("Public/") && chain == "lez" {
        return None;
    }
    Some((before, (now > expected).then_some((now - expected, certain))))
}

/// Whether noting `balance` for `account` now would tell money that arrived
/// (`note_balances`): a rise its entries don't explain. When it wouldn't, no credit can be
/// in that balance unseen, so a bridge to the account needn't be asked about this time.
/// `scope` and `zone` are the ones the balance is noted with (`note_zone_balances`).
pub fn would_arrive(dir: &Path, at: u64, chain: &str, scope: &str, zone: Option<&str>, account: &str, balance: u128) -> bool {
    let (seen, all) = (load(&balances_file(dir)), load(&file(dir)));
    let touching = touching(&all, account, zone);
    matches!(rise(&all, &seen, &touching, at, chain, &mark_key(chain, scope, account), account, balance), Some((_, Some(_))))
}

/// How often a followed bridge's receipt is asked (seconds) when its account's balance
/// gives no reason to (`would_arrive`): each ask is a sequencer read on a balance call, for
/// as long as a bridge is followed (`BRIDGE_FOLLOW_S`, 30 days).
pub const RECEIPT_EVERY_S: u64 = 60;
/// The same, for a bridge overdue (past `BRIDGE_WAIT_S`): one that may never arrive.
pub const OVERDUE_RECEIPT_EVERY_S: u64 = 3600;

/// Whether a bridge sent `age` seconds ago, whose receipt LEZ last answered about at
/// `asked` (never: `None`), is due to be asked again at `now`.
pub fn receipt_due(age: u64, asked: Option<u64>, now: u64) -> bool {
    let every = if age < BRIDGE_WAIT_S { RECEIPT_EVERY_S } else { OVERDUE_RECEIPT_EVERY_S };
    asked.map_or(true, |asked| now.saturating_sub(asked) >= every)
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
    note(dir, at, chain, scope, None, balances)
}

/// `note_balances` for the LEZ accounts of `zone`, read from the sequencer of that zone
/// (`scope`: its channel, or its address while the channel isn't known). Only that zone's
/// entries explain a change: the same account on another zone is other money, and what
/// arrives is told as arriving on this one.
pub fn note_zone_balances(dir: &Path, at: u64, scope: &str, zone: &str, balances: &[(String, u128)]) -> Vec<String> {
    note(dir, at, "lez", scope, Some(zone), balances)
}

/// Moves `chain`'s balance marks kept under `from` to `to`, when a zone's marks change
/// what they are kept under (its channel, once known, instead of its sequencer's
/// address), so the next read isn't a first one. A moved mark takes the place of one
/// already under `to`: the moved ones are those the wallet read last, and the history's
/// entries are counted against them.
pub fn move_marks(dir: &Path, chain: &str, from: &str, to: &str) {
    if from == to || from.is_empty() || to.is_empty() {
        return;
    }
    let mut seen = load(&balances_file(dir));
    let (old, new) = (format!("{chain}@{from}:"), format!("{chain}@{to}:"));
    let moved: Vec<String> = seen.keys().filter(|key| key.starts_with(&old)).cloned().collect();
    if moved.is_empty() {
        return;
    }
    for key in moved {
        if let Some(mark) = seen.remove(&key) {
            seen.insert(format!("{new}{}", &key[old.len()..]), mark);
        }
    }
    save(&balances_file(dir), &seen);
}

fn note(dir: &Path, at: u64, chain: &str, scope: &str, zone: Option<&str>, balances: &[(String, u128)]) -> Vec<String> {
    let mut seen = load(&balances_file(dir));
    let mut all = load(&file(dir));
    let mut added = Vec::new();
    let mut marks_changed = false;
    let mut entries_changed = false;
    for (account, balance) in balances {
        let key = mark_key(chain, scope, account);
        let touching = touching(&all, account, zone);
        let Some((before, rise)) = rise(&all, &seen, &touching, at, chain, &key, account, *balance) else {
            continue;
        };
        if let Some((rise, certain)) = rise {
            let rise = rise.to_string();
            let mut id = format!("{at}-arrived");
            while all.contains_key(&id) {
                id.push('1');
            }
            let mut entry = json!({ "id": id, "at": at, "seq": next_seq(&all), "kind": "arrived", "chain": chain, "to": account, "state": "arrived", "counted": [account] });
            entry = in_zone(entry, zone.unwrap_or(crate::zones::HOME));
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
        for id in &touching {
            // A bridge still on its way is counted once it arrives: counted now (as 0), its
            // credit would be logged again as money that arrived (late-or-expired-bridge-arrival-logged-twice).
            if bridge_waits_for(&all[id], account) {
                continue;
            }
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

    /// A LEZ account of the tests' bridges, and a bridge to it as `chain_bridge` records one:
    /// the channel, its one note and the account give its receipt.
    fn t() -> String {
        format!("Public/{}", crate::registry::account_text(&[5; 32]))
    }

    fn bridge(d: &Path, at: u64, amount: u128, note: u8) -> String {
        add(d, at, json!({ "kind": "bridge", "chain": "logos", "from": "k".repeat(64), "to": t(), "amount": amount.to_string(),
            "note": crate::registry::hex(&[note; 32]), "channel": "00".repeat(32), "fee": "1614", "state": "in_block" }))
    }

    fn receipt_of(d: &Path, id: &str) -> crate::registry::AccountId {
        bridge_receipt(all(d).iter().find(|e| e["id"] == id).unwrap()).expect("a receipt")
    }

    /// One read as the module makes it: the bridges settled by what LEZ says of their
    /// receipts (`minted`), then the balance noted.
    fn read(d: &Path, at: u64, minted: &[crate::registry::AccountId], held: u128) -> Vec<String> {
        for (id, fields) in bridge_updates(&all(d), at, &|receipt| Some(minted.contains(receipt))) {
            update(d, &id, &fields);
        }
        note(d, at, &[(&t(), held)])
    }

    /// The rows on T's page: (kind, amount, state), oldest first.
    fn rows(d: &Path) -> Vec<(String, String, String)> {
        let mut rows = all(d);
        rows.reverse();
        rows.iter()
            .filter(|e| involves(e, &t()))
            .map(|e| (e["kind"].as_str().unwrap().to_owned(), e["amount"].as_str().unwrap_or("?").to_owned(), e["state"].as_str().unwrap().to_owned()))
            .collect()
    }

    fn row(kind: &str, amount: &str, state: &str) -> (String, String, String) {
        (kind.to_owned(), amount.to_owned(), state.to_owned())
    }

    /// act-repro F8: a bridge that reached its LEZ account was logged as the bridge
    /// arriving and also as an arrival of the same amount.
    #[test]
    fn a_bridge_that_arrives_is_not_also_an_arrival() {
        let d = dir("bridge");
        read(&d, 1, &[], 20_010);
        let b = bridge(&d, 2, 3000, 1);
        // Its credit and its receipt land in one LEZ transaction.
        let minted = [receipt_of(&d, &b)];
        assert!(read(&d, 3, &minted, 23_010).is_empty(), "{:?}", rows(&d));
        assert_eq!(rows(&d), [row("bridge", "3000", "arrived")]);
        assert!(read(&d, 4, &minted, 23_010).is_empty());
        assert_eq!(read(&d, 5, &minted, 24_010).len(), 1, "later money is still an arrival");
        assert_eq!(arrivals(&d)[0]["amount"], "1000");
        let _ = std::fs::remove_dir_all(d);
    }

    /// foreign-credit-while-bridge-waits-never-in-history (X1, X2, X3): while a bridge of
    /// 500,001 waited, payments from elsewhere were never shown, before or after it arrived.
    /// single-bridge-false-arrival-on-foreign-credit-at-least-amount (S8): a payment of
    /// 700,000 marked it arrived. A payment of exactly its amount doesn't either.
    #[test]
    fn money_from_elsewhere_while_a_bridge_waits_is_its_own_row() {
        let d = dir("foreign");
        read(&d, 1, &[], 0);
        let b = bridge(&d, 2, 500_001, 1);
        read(&d, 3, &[], 0);
        read(&d, 4, &[], 700_000);
        assert_eq!(rows(&d), [row("bridge", "500001", "in_block"), row("arrived", "700000", "arrived")]);
        read(&d, 5, &[], 1_200_001);
        assert_eq!(rows(&d).len(), 3, "a payment of exactly its amount is not the bridge: {:?}", rows(&d));
        assert_eq!(rows(&d)[0], row("bridge", "500001", "in_block"));
        // Its own credit, then: the bridge arrives, and the rows add up to what T holds.
        let minted = [receipt_of(&d, &b)];
        assert!(read(&d, 6, &minted, 1_700_002).is_empty());
        assert_eq!(rows(&d), [row("bridge", "500001", "arrived"), row("arrived", "700000", "arrived"), row("arrived", "500001", "arrived")]);
        // X3: one that never arrives; each payment is told as it comes.
        let _ = std::fs::remove_dir_all(d);
        let d = dir("foreign-x3");
        read(&d, 1, &[], 0);
        bridge(&d, 2, 500_001, 1);
        for (at, held) in [(3, 1000), (4, 3000), (5, 6000)] {
            read(&d, at, &[], held);
        }
        assert_eq!(rows(&d), [row("bridge", "500001", "in_block"), row("arrived", "1000", "arrived"), row("arrived", "2000", "arrived"), row("arrived", "3000", "arrived")]);
        let _ = std::fs::remove_dir_all(d);
    }

    /// late-or-expired-bridge-arrival-logged-twice (W4, W1): a bridge credited after the
    /// two-day wait was logged as the bridge arriving and again as a bare arrival.
    #[test]
    fn a_bridge_that_arrives_late_is_one_row() {
        let d = dir("late");
        read(&d, 1, &[], 20_000);
        let b = bridge(&d, 2, 500_001, 1);
        read(&d, 2 + BRIDGE_WAIT_S - 600, &[], 20_000);
        read(&d, 2 + BRIDGE_WAIT_S + 600, &[], 20_000);
        assert_eq!(rows(&d), [row("bridge", "500001", "expired")], "overdue, and said so");
        let minted = [receipt_of(&d, &b)];
        assert!(read(&d, 2 + BRIDGE_WAIT_S + 1200, &minted, 520_001).is_empty(), "{:?}", rows(&d));
        assert_eq!(rows(&d), [row("bridge", "500001", "arrived")]);
        assert_eq!(all(&d)[0]["late"], true);
        // W1: one an older version counted (as 0) once it was past the wait.
        let _ = std::fs::remove_dir_all(d);
        let d = dir("late-counted");
        read(&d, 1, &[], 20_000);
        let b = bridge(&d, 2, 3000, 1);
        update(&d, &b, &json!({ "counted": [t()] }));
        let minted = [receipt_of(&d, &b)];
        assert!(read(&d, 2 + BRIDGE_WAIT_S + 3600, &minted, 23_000).is_empty(), "{:?}", rows(&d));
        assert_eq!(rows(&d), [row("bridge", "3000", "arrived")]);
        let _ = std::fs::remove_dir_all(d);
    }

    /// expired-bridge-takes-later-bridges-credit (W3, W2): a 49-hour-old bridge of 500,001
    /// that never arrived took the credit of a fresh one of the same amount.
    #[test]
    fn an_overdue_bridge_never_takes_a_later_ones_credit() {
        let d = dir("expired");
        let day = 24 * 3600;
        read(&d, 1, &[], 20_000);
        let old = bridge(&d, 2, 500_001, 1);
        let fresh = bridge(&d, 2 + 2 * day + 3600, 500_001, 2);
        let now = 2 + 2 * day + 7200;
        read(&d, now, &[], 20_000);
        let minted = [receipt_of(&d, &fresh)];
        read(&d, now + 60, &minted, 520_001);
        let state = |id: &str| all(&d).into_iter().find(|e| e["id"] == id).unwrap()["state"].as_str().unwrap().to_owned();
        assert_eq!((state(&old), state(&fresh)), ("expired".to_owned(), "arrived".to_owned()));
        assert!(arrivals(&d).is_empty(), "{:?}", rows(&d));
        let _ = std::fs::remove_dir_all(d);
    }

    /// out-of-order-first-credit-before-any-read-strands-bridge (R2), and
    /// false-arrival-second-bridge (f2b): three bridges, credited last first with no read in
    /// between; each arrives on its own deposit, whatever the order or the amounts.
    #[test]
    fn bridges_arrive_each_on_its_own_deposit_in_any_order() {
        let d = dir("any-order");
        read(&d, 1, &[], 0);
        let (a, b, c) = (bridge(&d, 2, 500_001, 1), bridge(&d, 3, 400_002, 2), bridge(&d, 4, 300_003, 3));
        let (ra, rb, rc) = (receipt_of(&d, &a), receipt_of(&d, &b), receipt_of(&d, &c));
        let state = |d: &Path| all(d).into_iter().rev().filter(|e| e["kind"] == "bridge").map(|e| e["state"].as_str().unwrap().to_owned()).collect::<Vec<_>>();
        read(&d, 5, &[rc], 300_003);
        assert_eq!(state(&d), ["in_block", "in_block", "arrived"]);
        read(&d, 6, &[rc, rb], 700_005);
        assert_eq!(state(&d), ["in_block", "arrived", "arrived"]);
        read(&d, 7, &[rc, rb, ra], 1_200_006);
        assert_eq!(state(&d), ["arrived", "arrived", "arrived"]);
        assert!(arrivals(&d).is_empty(), "{:?}", rows(&d));
        let _ = std::fs::remove_dir_all(d);
    }

    /// A bridge whose send was taken as `unknown` (not seen in a block in time) still
    /// arrives by its receipt, and its credit is not money that arrived besides.
    #[test]
    fn a_bridge_whose_send_was_unknown_still_arrives_by_its_receipt() {
        let d = dir("unknown-bridge");
        read(&d, 1, &[], 0);
        let b = bridge(&d, 2, 3000, 1);
        update(&d, &b, &json!({ "state": "unknown" }));
        read(&d, 3, &[], 0);
        let minted = [receipt_of(&d, &b)];
        assert!(read(&d, 4, &minted, 3000).is_empty(), "{:?}", rows(&d));
        assert_eq!(rows(&d), [row("bridge", "3000", "arrived")]);
        let _ = std::fs::remove_dir_all(d);
    }

    /// A bridge with no channel or note recorded can't be followed: it never arrives, its
    /// credit is told as money that arrived, and it is overdue past the wait.
    #[test]
    fn a_bridge_without_a_receipt_is_not_followed() {
        let d = dir("unfollowed");
        read(&d, 1, &[], 0);
        let b = add(&d, 2, json!({ "kind": "bridge", "chain": "logos", "to": t(), "amount": "3000", "state": "in_block" }));
        read(&d, 3, &[], 3000);
        let entry = all(&d).into_iter().find(|e| e["id"] == b).unwrap();
        assert_eq!((entry["state"].as_str(), entry["followed"].as_bool()), (Some("in_block"), Some(false)));
        assert_eq!(rows(&d), [row("bridge", "3000", "in_block"), row("arrived", "3000", "arrived")]);
        read(&d, 2 + BRIDGE_WAIT_S, &[], 3000);
        assert_eq!(rows(&d)[0], row("bridge", "3000", "expired"));
        // No answer from LEZ about a receipt is not an arrival.
        let _ = std::fs::remove_dir_all(d);
        let d = dir("unread");
        let b = bridge(&d, 2, 3000, 1);
        assert!(bridge_updates(&all(&d), 3, &|_| None).iter().all(|(_, fields)| fields.get("state").is_none()));
        assert_eq!(bridges_followed(&all(&d), 3).len(), 1);
        assert!(bridges_followed(&all(&d), 2 + BRIDGE_FOLLOW_S).is_empty(), "followed for so long");
        let _ = b;
        let _ = std::fs::remove_dir_all(d);
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

    /// partial-claim-job-and-pages-say-nothing-true: one of two claims landed (R1 credited
    /// 1,000,000, payer P paid 5,070,616) and R2's credit still waits. The entry is partly
    /// done, on all three pages, and explains P's and R1's balances.
    #[test]
    fn a_claim_that_stopped_partway_is_on_every_page_and_counts_what_landed() {
        let d = dir("partial");
        note(&d, 1, &[("Private/P", 8_929_384), ("Private/R1", 0), ("Private/R2", 0)]);
        add(&d, 2, json!({ "kind": "receive", "chain": "lez", "from": "Private/P", "amount": "1000000", "to": "Private/R1",
            "received": { "Private/R1": "1000000" }, "left": { "Private/R2": "1000000" }, "fee": { "about": "10141232", "paid": "5070616" }, "state": "partial" }));
        let entry = all(&d).remove(0);
        assert!(["Private/P", "Private/R1", "Private/R2"].iter().all(|page| involves(&entry, page)), "{entry}");
        // P lost the fee (its refund waits as a credit), R1 got its claim, R2 is unchanged.
        assert!(note(&d, 3, &[("Private/P", 8_929_384 - 5_070_616), ("Private/R1", 1_000_000), ("Private/R2", 0)]).is_empty(), "{:?}", arrivals(&d));
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

    const ZONE: &str = "abababababababababababababababababababababababababababababababab";

    fn note_on(d: &Path, at: u64, zone: &str, rows: &[(&str, u128)]) -> Vec<String> {
        let rows: Vec<(String, u128)> = rows.iter().map(|(a, b)| ((*a).to_owned(), *b)).collect();
        let scope = if zone == crate::zones::HOME { "0101" } else { zone };
        note_zone_balances(d, at, scope, zone, &rows)
    }

    /// The same account is other money on another zone: each zone has its own marks, and
    /// only its own entries explain a change there.
    #[test]
    fn each_zone_has_its_own_marks_and_entries() {
        let d = dir("zones");
        assert!(note_on(&d, 1, crate::zones::HOME, &[("Public/X", 100)]).is_empty());
        // The first read on the other zone only sets its mark, whatever home holds.
        assert!(note_on(&d, 2, ZONE, &[("Public/X", 7_000)]).is_empty());
        // A payment made on the other zone explains nothing on home, and the other way round.
        let pay = add(&d, 3, in_zone(json!({ "kind": "pay_account", "chain": "lez", "from": "Public/X", "to": "Public/Y", "amount": "1000" }), ZONE));
        update(&d, &pay, &json!({ "state": "done", "fee": { "about": "10", "at_most": "10", "paid": "10" } }));
        assert_eq!(zone_of(&all(&d)[0]), ZONE);
        assert!(note_on(&d, 4, crate::zones::HOME, &[("Public/X", 100)]).is_empty(), "{:?}", arrivals(&d));
        assert!(note_on(&d, 5, ZONE, &[("Public/X", 5_990)]).is_empty(), "{:?}", arrivals(&d));
        // Money that arrives on the other zone is told as arriving there.
        let told = note_on(&d, 6, ZONE, &[("Public/X", 6_990)]);
        assert_eq!(told.len(), 1);
        let arrived = arrivals(&d);
        assert_eq!((arrived[0]["amount"].as_str(), zone_of(&arrived[0])), (Some("1000"), ZONE));
        // And on home as home's, with no mark of a zone, as before zones.
        assert_eq!(note_on(&d, 7, crate::zones::HOME, &[("Public/X", 150)]).len(), 1);
        let arrived = arrivals(&d);
        assert_eq!(arrived[0]["amount"], "50");
        assert!(arrived[0]["zone"].is_null() && zone_of(&arrived[0]) == crate::zones::HOME);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn an_entry_says_its_zone_only_off_home() {
        let entry = json!({ "kind": "pay_account", "chain": "lez" });
        assert!(in_zone(entry.clone(), crate::zones::HOME)["zone"].is_null());
        assert_eq!(in_zone(entry.clone(), ZONE)["zone"], ZONE);
        assert!(on_lez(&entry) && on_lez(&json!({ "kind": "bridge", "chain": "logos" })));
        assert!(!on_lez(&json!({ "kind": "send", "chain": "logos" })));
    }

    /// A zone's marks are kept under its channel once that is known: they move there from
    /// its sequencer's address, so the next read isn't taken for a first one.
    #[test]
    fn marks_move_with_what_a_zone_is_kept_under() {
        let d = dir("move");
        assert!(note(&d, 1, &[("Public/X", 100), ("Private/P", 5)]).is_empty());
        move_marks(&d, "lez", SEQ, ZONE);
        // Nothing is kept under the address any more: a read there is a first one again.
        assert!(note(&d, 2, &[("Public/X", 900)]).is_empty());
        // Under the channel, the read carries on from the mark.
        let rows = [("Public/X".to_owned(), 130_u128), ("Private/P".to_owned(), 5)];
        assert_eq!(note_balances(&d, 3, "lez", ZONE, &rows).len(), 1);
        assert_eq!(arrivals(&d)[0]["amount"], "30");
        // The marks last read win over ones left under the new place, and moving nothing
        // (the same place, or none) changes nothing.
        move_marks(&d, "lez", SEQ, ZONE);
        move_marks(&d, "lez", ZONE, ZONE);
        move_marks(&d, "lez", "", ZONE);
        let rows = [("Public/X".to_owned(), 900_u128)];
        assert!(note_balances(&d, 4, "lez", ZONE, &rows).is_empty(), "{:?}", arrivals(&d));
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
