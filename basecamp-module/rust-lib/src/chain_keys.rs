//! Accounts on the Logos Blockchain (LP-0022): more than one key on Basecamp's node.
//!
//! The node (blockchain_module 0.3.4) keeps its keys in a keystore file beside its
//! config, and its wallet spends from every zk key in it. What a solo node showed over
//! Logos Core (the lane spike, 2026-11-09):
//! - `generate_key(keystore, "zk", title)` adds a key and answers its public key; the
//!   secret stays in the keystore, which keeps the file's mode.
//! - The same title again REPLACES that key: the first one's secret is gone. So a title
//!   is only ever one the keystore doesn't hold yet, and the keystore is read again after.
//! - The running wallet doesn't follow a key made after it started: the node lists it
//!   in `wallet_get_known_addresses` from its next start, and only then can it spend
//!   from it or see what it receives. What was sent to it in a block already final
//!   at that start stays unseen after it: the wallet starts from its last final block,
//!   not genesis (a send whose block wasn't final yet was seen).
//! - A key with no notes answers `wallet_get_balance` "Unknown wallet address."; its
//!   balance is then 0.
//! - `remove_key(keystore, title)` deletes the key, secret and all.
//! - The node can't say what the chain holds or will hold for a key: a payment on its
//!   way, or one that reached the key before the node used it, isn't in its balance. So
//!   the module never removes a key: `remove` hides the account and the secret stays.
//!
//! The module finds the keystore beside its own data: Basecamp and logosctl give each
//! module `<data>/<module>/<instance>` (`--instance-persistence-path` on the host
//! process), so the node's is `<data>/blockchain_module/<instance>/`. It writes only to
//! a keystore whose keys include every key the running node lists, so never to another
//! node's. Labels are the module's own (`chain-accounts.json`), never in the keystore.

use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

use crate::chain;

/// What the module asks of the node: a method and its arguments, its reply unwrapped.
pub type Ask<'a> = dyn FnMut(&str, Value) -> Result<Value, String> + 'a;

/// The keystore titles the module gives the keys it makes: `names-1`, `names-2`, …
pub const TITLE_PREFIX: &str = "names-";

/// The longest label a person can give an account.
pub const MAX_LABEL: usize = 40;

/// The node's files: its config and the keystore beside it.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeFiles {
    pub config: PathBuf,
    pub keystore: PathBuf,
}

/// A key in the keystore, from `read_accounts`: the zk keys (`accounts`) with their
/// roles, and every title (`keystore_keys`, the Ed25519 ones too).
#[derive(Debug, Clone, PartialEq)]
pub struct Keystore {
    /// `(title, public key, roles)` of each zk key.
    pub accounts: Vec<(String, String, Vec<String>)>,
    /// `(title, key id)` of every key.
    pub titles: Vec<(String, String)>,
}

/// Reads `read_accounts`' reply.
pub fn keystore(reply: &Value) -> Keystore {
    let accounts = reply["accounts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| {
            let roles = a["roles"].as_array().into_iter().flatten().filter_map(|r| r.as_str().map(str::to_owned)).collect();
            Some((a["title"].as_str()?.to_owned(), chain::parse_key(a["public_key"].as_str()?)?, roles))
        })
        .collect();
    let titles = reply["keystore_keys"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|k| Some((k["title"].as_str()?.to_owned(), k["key_id"].as_str()?.to_lowercase())))
        .collect();
    Keystore { accounts, titles }
}

impl Keystore {
    /// The public key titled `title`, if a zk key has it.
    pub fn key_of(&self, title: &str) -> Option<&str> {
        self.accounts.iter().find(|(t, _, _)| t == title).map(|(_, k, _)| k.as_str())
    }

    /// The zk key `key`: its title and roles.
    pub fn find(&self, key: &str) -> Option<(&str, &[String])> {
        self.accounts.iter().find(|(_, k, _)| k == key).map(|(t, _, r)| (t.as_str(), r.as_slice()))
    }

    /// Whether this keystore holds every key the running node lists: the node's own.
    pub fn is_the_nodes(&self, known: &[String]) -> bool {
        !known.is_empty() && known.iter().all(|key| self.accounts.iter().any(|(_, k, _)| k == key))
    }
}

/// The first `names-N` title the keystore doesn't hold (any key type).
pub fn next_title(keystore: &Keystore) -> String {
    (1..)
        .map(|n| format!("{TITLE_PREFIX}{n}"))
        .find(|title| !keystore.titles.iter().any(|(t, _)| t == title) && keystore.key_of(title).is_none())
        .expect("a free title")
}

/// A label as a person writes it: trimmed, at most `MAX_LABEL` characters, no control
/// characters. Empty means no label.
pub fn label_input(text: &str) -> Result<String, String> {
    let label = text.trim();
    if label.chars().count() > MAX_LABEL {
        return Err(format!("a label is at most {MAX_LABEL} characters"));
    }
    if label.chars().any(char::is_control) {
        return Err("a label is one line of text".to_owned());
    }
    Ok(label.to_owned())
}

/// The node's folders to try, from this process's arguments: beside
/// `--instance-persistence-path <data>/<module>/<instance>`, each
/// `<data>/blockchain_module/<instance>/` holding a `user_config.yaml` and a
/// `keystore.yaml`, in name order.
pub fn node_files(args: &[String]) -> Vec<NodeFiles> {
    let Some(own) = args.iter().position(|a| a == "--instance-persistence-path").and_then(|i| args.get(i + 1)) else {
        return Vec::new();
    };
    let Some(data) = Path::new(own).parent().and_then(Path::parent) else { return Vec::new() };
    let mut found: Vec<NodeFiles> = std::fs::read_dir(data.join(chain::MODULE))
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .map(|dir| NodeFiles { config: dir.join("user_config.yaml"), keystore: dir.join("keystore.yaml") })
        .filter(|files| files.config.is_file() && files.keystore.is_file())
        .collect();
    found.sort_by(|a, b| a.config.cmp(&b.config));
    found
}

/// The keys the running node lists (`wallet_get_known_addresses`); fails as the node
/// does when it isn't running.
pub fn known(ask: &mut Ask) -> Result<Vec<String>, String> {
    let keys = ask("wallet_get_known_addresses", json!([]))?;
    Ok(keys.as_array().into_iter().flatten().filter_map(|k| k.as_str().and_then(chain::parse_key)).collect())
}

/// The keystore of the running node, among `candidates`: the first whose keys include
/// every key the node lists. Refuses when none does, never guessing.
pub fn the_nodes(ask: &mut Ask, candidates: &[NodeFiles], known: &[String]) -> Result<(NodeFiles, Keystore), String> {
    for files in candidates {
        let Ok(reply) = ask("read_accounts", json!([files.config.to_string_lossy()])) else { continue };
        let read = keystore(&reply);
        if read.is_the_nodes(known) {
            return Ok((files.clone(), read));
        }
    }
    Err(NO_KEYSTORE.to_owned())
}

/// Why no account can be made or removed: the keystore isn't where the module looks.
pub const NO_KEYSTORE: &str = "The Names app couldn't find your node's keystore beside its own data, so it can't add or remove keys. \
     Accounts are made in the folder Basecamp gives the Blockchain app; a node started from a config elsewhere keeps its keys there.";

/// Makes the keystore readable by this user only, if it is readable by others: it holds
/// every secret key of the node.
pub fn tighten(keystore: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = std::fs::metadata(keystore) {
        if meta.permissions().mode() & 0o077 != 0 {
            let _ = std::fs::set_permissions(keystore, std::fs::Permissions::from_mode(0o600));
        }
    }
}

/// Whether every key of `before` is still in `after` under the same title.
fn kept(before: &Keystore, after: &Keystore) -> bool {
    before.titles.iter().all(|entry| after.titles.contains(entry))
}

/// What a new account needs the person to know: the node takes it at its next start.
/// A key sent to before the node used it, in a block final before the restart, stays
/// unseen after it (spike: 2,000 sent to 8bbf9fbd… while the node didn't list it; after a
/// restart the node listed the key and answered "Unknown wallet address."), so the note
/// says not to receive before then.
pub const RESTART: &str = "Your node uses this account from its next start: stop and start it in Basecamp's Blockchain app. Until then it can't send, and don't have anything sent to it: what reaches it before your node uses it isn't seen.";

/// Makes a new account (a zk key in the node's keystore) labelled `label`:
/// `{key, title, label, ready, note}`. `ready` is whether the running node already
/// lists it (it doesn't until it restarts). `records` gets `{key: {title, label, at}}`.
pub fn make(ask: &mut Ask, candidates: &[NodeFiles], records: &mut Map<String, Value>, label: &str, at: u64) -> Result<Value, String> {
    let label = label_input(label)?;
    let known = known(ask)?;
    let (files, before) = the_nodes(ask, candidates, &known)?;
    let title = next_title(&before);
    tighten(&files.keystore);
    let made = ask("generate_key", json!([files.keystore.to_string_lossy(), "zk", title]))?;
    let key = made.as_str().and_then(chain::parse_key).ok_or_else(|| format!("the node made a key but answered {made}"))?;
    // Read back: the new key under its title, and every other key as it was.
    let after = keystore(&ask("read_accounts", json!([files.config.to_string_lossy()]))?);
    if after.key_of(&title) != Some(key.as_str()) || !kept(&before, &after) {
        return Err(format!("the node answered key {key} but its keystore doesn't show it as {title} beside the keys it had: check the keystore"));
    }
    tighten(&files.keystore);
    records.insert(key.clone(), json!({ "title": title, "label": label, "at": at }));
    let ready = known.contains(&key);
    Ok(json!({ "key": key, "title": title, "label": label, "ready": ready, "note": if ready { Value::Null } else { RESTART.into() } }))
}

/// Labels the account `key` (one the node lists or the module made): `{key, label}`.
/// An empty label takes it off.
pub fn relabel(ask: &mut Ask, records: &mut Map<String, Value>, key: &str, label: &str) -> Result<Value, String> {
    let key = chain::parse_key(key).ok_or("choose the account to label")?;
    let label = label_input(label)?;
    if !records.contains_key(&key) && !known(ask)?.contains(&key) {
        return Err("Your node doesn't hold that key.".to_owned());
    }
    let record = records.entry(key.clone()).or_insert_with(|| json!({}));
    record["label"] = Value::from(label.clone());
    if label.is_empty() && record.get("title").is_none() {
        records.remove(&key);
    }
    Ok(json!({ "key": key, "label": label }))
}

/// What a hidden account's reply says.
pub const HIDDEN: &str = "Hidden from your accounts. Its key stays in your node's keystore, so nothing sent to it is lost: if it ever holds anything, it shows again.";

/// Hides an account the module made, once the running node shows it holds nothing:
/// `{key, title, hidden, note}`. The key stays in the node's keystore: the node can't
/// say what the chain holds or will hold for a key (a payment on its way, or one that
/// reached it before the node used it), so no secret is ever deleted here. A hidden
/// account that comes to hold anything is listed again. Refused for a key the Names app
/// didn't make, one the node doesn't list yet, and one holding anything.
pub fn remove(ask: &mut Ask, records: &mut Map<String, Value>, key: &str) -> Result<Value, String> {
    let key = chain::parse_key(key).ok_or("choose the account to remove")?;
    let Some(title) = records.get(&key).and_then(|r| r["title"].as_str()).map(str::to_owned) else {
        return Err("Only accounts made in the Names app can be removed here.".to_owned());
    };
    let known = known(ask)?;
    if !known.contains(&key) {
        return Err("Your node doesn't use this account yet, so it can't say it holds nothing: restart your node, then remove it.".to_owned());
    }
    let held = match ask("wallet_get_balance", json!([key])) {
        Ok(balance) => chain::balance_of(&balance),
        Err(error) if chain::not_held(&error) => 0,
        Err(error) => return Err(error),
    };
    if held > 0 {
        return Err(format!("That account holds {held}. Send it all to another account first."));
    }
    if let Some(record) = records.get_mut(&key) {
        record["hidden"] = Value::Bool(true);
    }
    Ok(json!({ "key": key, "title": title, "hidden": true, "note": HIDDEN }))
}

/// Whether the module's record of `key` hides it.
fn hidden(records: &Map<String, Value>, key: &str) -> bool {
    records.get(key).is_some_and(|r| r["hidden"] == Value::Bool(true))
}

/// `chain_accounts`' rows with what the module knows of each: `label`, `title` and
/// `ready` on the keys it has a record for, and the accounts it made that the rows lack,
/// as `{key, balance, title, label, ready}`: balance "0" for one the node lists (a key
/// with no notes), null with `ready: false` for one it takes at its next start. A hidden
/// account is left out while it holds nothing, and shown again once it holds anything.
pub fn annotate(rows: Value, known: &[String], records: &Map<String, Value>) -> Value {
    let mut rows = rows.as_array().cloned().unwrap_or_default();
    rows.retain(|row| !(row["key"].as_str().is_some_and(|key| hidden(records, key)) && row["balance"].as_str() == Some("0")));
    for row in rows.iter_mut() {
        if let Some(record) = row["key"].as_str().and_then(|key| records.get(key)) {
            row["label"] = record["label"].clone();
            if let Some(title) = record.get("title") {
                row["title"] = title.clone();
            }
            row["ready"] = Value::Bool(true);
        }
    }
    let mut made: Vec<(&String, &Value)> = records.iter().filter(|(key, r)| r.get("title").is_some() && !hidden(records, key)).collect();
    made.sort_by_key(|(_, r)| r["at"].as_u64().unwrap_or(0));
    for (key, record) in made {
        if rows.iter().any(|row| row["key"].as_str() == Some(key)) {
            continue;
        }
        let ready = known.contains(key);
        rows.push(json!({ "key": key, "balance": if ready { Value::from("0") } else { Value::Null },
            "title": record["title"], "label": record["label"], "ready": ready }));
    }
    Value::from(rows)
}

/// The module's records of accounts: `chain-accounts.json` in `dir`.
pub fn records_path(dir: &Path) -> PathBuf {
    dir.join("chain-accounts.json")
}

/// The module's records of accounts (`{key: {title?, label, at?}}`); empty if none.
pub fn records() -> Map<String, Value> {
    crate::config_dir().map(|dir| crate::activity::load(&records_path(&dir))).unwrap_or_default()
}

/// Runs `edit` against Basecamp's node (its keystore found from this process's
/// arguments) with the module's records, and saves them (0600) when it changed them.
pub fn with_node(edit: impl FnOnce(&mut Ask, &[NodeFiles], &mut Map<String, Value>) -> Result<Value, String>) -> Result<Value, String> {
    let dir = crate::config_dir().ok_or("there is no folder to keep accounts in")?;
    let path = records_path(&dir);
    let mut records = crate::activity::load(&path);
    let before = records.clone();
    let args: Vec<String> = std::env::args().collect();
    let mut ask = |method: &str, args: Value| crate::node::call(method, args, crate::node::QUICK);
    let outcome = edit(&mut ask, &node_files(&args), &mut records);
    if records != before {
        crate::save_object(&path, &records);
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const A: &str = "e3635f207984ae779cf76b5f20714b514373f61ff96260879fe0a6d71f2dce07";
    const B: &str = "39e16b432574571a6bcd8ee36e370589641bd9f35367f6f97a972453c46c3225";
    const LF: &str = "e3cc96c180b5f93468f73d383d06f78de475bd0945da407a3547ce1145d44920";
    const NEW: &str = "e613268c7dd432c972e872d03fff5f79d6a3cbcc8103f34afc2b33fef998bf1d";

    /// read_accounts as the solo node answered it, cut down.
    fn read(extra: &[(&str, &str)]) -> Value {
        let mut accounts = vec![
            json!({ "public_key": A, "roles": [], "title": "GenesisStake" }),
            json!({ "public_key": B, "roles": [], "title": "GenesisSmall" }),
            json!({ "public_key": LF, "roles": ["leader_funding"], "title": "LeaderFunding" }),
        ];
        let mut titles = vec![
            json!({ "key_id": A, "title": "GenesisStake" }),
            json!({ "key_id": B, "title": "GenesisSmall" }),
            json!({ "key_id": LF, "title": "LeaderFunding" }),
            json!({ "key_id": "d986dba2a4d48349720eedfa5a5373f2437a16d58dd52d38768cce7e1f578997", "title": "BlendSigning" }),
        ];
        for (title, key) in extra {
            accounts.push(json!({ "public_key": key, "roles": [], "title": title }));
            titles.push(json!({ "key_id": key, "title": title }));
        }
        json!({ "accounts": accounts, "keystore_keys": titles })
    }

    /// A node stand-in: answers per method from a queue (the last answer repeats), and
    /// keeps what was asked.
    struct Node {
        answers: HashMap<String, Vec<Result<Value, String>>>,
        asked: Vec<(String, Value)>,
    }

    impl Node {
        fn new(answers: &[(&str, Result<Value, String>)]) -> Self {
            let mut map: HashMap<String, Vec<Result<Value, String>>> = HashMap::new();
            for (method, answer) in answers {
                map.entry((*method).to_owned()).or_default().push(answer.clone());
            }
            Self { answers: map, asked: Vec::new() }
        }

        fn ask(&mut self, method: &str, args: Value) -> Result<Value, String> {
            self.asked.push((method.to_owned(), args));
            let queue = self.answers.get_mut(method).ok_or_else(|| format!("not answered: {method}"))?;
            if queue.len() > 1 {
                queue.remove(0)
            } else {
                queue[0].clone()
            }
        }

        fn methods(&self) -> Vec<&str> {
            self.asked.iter().map(|(m, _)| m.as_str()).collect()
        }
    }

    fn files() -> Vec<NodeFiles> {
        vec![NodeFiles { config: "/data/blockchain_module/1/user_config.yaml".into(), keystore: "/data/blockchain_module/1/keystore.yaml".into() }]
    }

    fn known_ab() -> Result<Value, String> {
        Ok(json!([A, B, LF]))
    }

    #[test]
    fn read_accounts_is_read_as_the_node_gives_it() {
        let ks = keystore(&read(&[]));
        assert_eq!(ks.accounts.len(), 3);
        assert_eq!(ks.titles.len(), 4, "the Ed25519 key's title counts too");
        assert_eq!(ks.key_of("GenesisStake"), Some(A));
        assert_eq!(ks.find(LF).map(|(_, r)| r.to_vec()), Some(vec!["leader_funding".to_owned()]));
        assert!(ks.is_the_nodes(&[A.into(), B.into()]));
        assert!(!ks.is_the_nodes(&[A.into(), NEW.into()]), "a key the node lists that this keystore lacks: not its keystore");
        assert!(!ks.is_the_nodes(&[]), "nothing listed proves nothing");
    }

    /// generate_key with a title the keystore holds replaced that key (spike: Acct1
    /// e6b32479 was gone after a second generate_key Acct1), so titles are always new.
    #[test]
    fn a_new_title_is_never_one_the_keystore_holds() {
        assert_eq!(next_title(&keystore(&read(&[]))), "names-1");
        assert_eq!(next_title(&keystore(&read(&[("names-1", NEW), ("names-3", B)]))), "names-2");
        let mut only_title = keystore(&read(&[]));
        only_title.titles.push(("names-1".into(), "ed".into()));
        assert_eq!(next_title(&only_title), "names-2", "an Ed25519 key's title is taken too");
    }

    #[test]
    fn labels_are_one_short_line() {
        assert_eq!(label_input("  Savings "), Ok("Savings".to_owned()));
        assert_eq!(label_input(""), Ok(String::new()));
        assert!(label_input(&"x".repeat(41)).is_err());
        assert!(label_input("a\nb").is_err());
        assert_eq!(label_input(&"é".repeat(40)).map(|l| l.chars().count()), Ok(40));
    }

    #[test]
    fn the_nodes_folder_is_found_beside_the_modules_own() {
        let root = std::env::temp_dir().join(format!("chain-keys-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let own = root.join("data/names/abc");
        std::fs::create_dir_all(&own).unwrap();
        for (dir, both) in [("b", true), ("a", true), ("c", false)] {
            let node = root.join("data/blockchain_module").join(dir);
            std::fs::create_dir_all(&node).unwrap();
            std::fs::write(node.join("user_config.yaml"), "").unwrap();
            if both {
                std::fs::write(node.join("keystore.yaml"), "").unwrap();
            }
        }
        let args: Vec<String> = ["logos_host", "--name", "names", "--instance-persistence-path", own.to_str().unwrap()].iter().map(|s| s.to_string()).collect();
        let found = node_files(&args);
        assert_eq!(found.iter().map(|f| f.config.parent().unwrap().file_name().unwrap().to_str().unwrap()).collect::<Vec<_>>(), ["a", "b"]);
        assert_eq!(found[0].keystore, root.join("data/blockchain_module/a/keystore.yaml"));
        assert!(node_files(&["logos_host".to_owned()]).is_empty(), "no data folder named: none");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_account_is_made_under_a_new_title_and_read_back() {
        let mut node = Node::new(&[
            ("wallet_get_known_addresses", known_ab()),
            ("read_accounts", Ok(read(&[]))),
            ("read_accounts", Ok(read(&[("names-1", NEW)]))),
            ("generate_key", Ok(json!(NEW))),
        ]);
        let mut records = Map::new();
        let made = make(&mut |m, a| node.ask(m, a), &files(), &mut records, " Savings ", 7).unwrap();
        assert_eq!(made, json!({ "key": NEW, "title": "names-1", "label": "Savings", "ready": false, "note": RESTART }));
        assert_eq!(node.methods(), ["wallet_get_known_addresses", "read_accounts", "generate_key", "read_accounts"]);
        assert_eq!(node.asked[2].1, json!(["/data/blockchain_module/1/keystore.yaml", "zk", "names-1"]));
        assert_eq!(records[NEW], json!({ "title": "names-1", "label": "Savings", "at": 7 }));
        // What reaches a key before the node uses it stayed unseen after a restart (spike).
        assert!(RESTART.contains("don't have anything sent to it"), "{RESTART}");
    }

    #[test]
    fn no_key_is_made_without_the_running_nodes_keystore() {
        // The node is down: nothing else is asked.
        let mut node = Node::new(&[("wallet_get_known_addresses", Err(chain::problem("wallet_get_known_addresses", "The node is not running.")))]);
        let mut records = Map::new();
        let down = make(&mut |m, a| node.ask(m, a), &files(), &mut records, "", 1).unwrap_err();
        assert!(down.contains("isn't running"), "{down}");
        assert_eq!(node.methods(), ["wallet_get_known_addresses"]);
        // A keystore that lacks a key the node lists is another node's: refused, nothing generated.
        let mut node = Node::new(&[("wallet_get_known_addresses", Ok(json!([A, NEW]))), ("read_accounts", Ok(read(&[])))]);
        assert_eq!(make(&mut |m, a| node.ask(m, a), &files(), &mut records, "", 1).unwrap_err(), NO_KEYSTORE);
        assert!(!node.methods().contains(&"generate_key"));
        // No folder found at all.
        let mut node = Node::new(&[("wallet_get_known_addresses", known_ab())]);
        assert_eq!(make(&mut |m, a| node.ask(m, a), &[], &mut records, "", 1).unwrap_err(), NO_KEYSTORE);
        // A label refused before the node is asked.
        let mut node = Node::new(&[]);
        assert!(make(&mut |m, a| node.ask(m, a), &files(), &mut records, &"x".repeat(50), 1).is_err());
        assert!(node.asked.is_empty());
        assert!(records.is_empty());
    }

    #[test]
    fn a_key_the_keystore_does_not_show_after_is_not_recorded() {
        // The node answered a key, but the keystore lost another (as a reused title did).
        let mut lost = read(&[("names-1", NEW)]);
        lost["keystore_keys"].as_array_mut().unwrap().remove(1);
        let mut node = Node::new(&[
            ("wallet_get_known_addresses", known_ab()),
            ("read_accounts", Ok(read(&[]))),
            ("read_accounts", Ok(lost)),
            ("generate_key", Ok(json!(NEW))),
        ]);
        let mut records = Map::new();
        let refused = make(&mut |m, a| node.ask(m, a), &files(), &mut records, "", 1).unwrap_err();
        assert!(refused.contains("check the keystore"), "{refused}");
        assert!(records.is_empty());
    }

    /// adv V8 / adv2 D2: the node showed nothing for a key the chain had paid (a payment
    /// before the node used the key, or one still on its way), and remove_key deleted its
    /// only secret. Now "remove" hides the account: the node is never asked to remove it.
    #[test]
    fn removing_an_empty_account_hides_it_and_keeps_its_key() {
        let mut records = Map::new();
        records.insert(NEW.into(), json!({ "title": "names-1", "label": "x", "at": 1 }));
        // Not made here.
        let mut node = Node::new(&[]);
        assert!(remove(&mut |m, a| node.ask(m, a), &mut records, A).unwrap_err().contains("made in the Names app"));
        // Not yet used by the node.
        let mut node = Node::new(&[("wallet_get_known_addresses", known_ab())]);
        assert!(remove(&mut |m, a| node.ask(m, a), &mut records, NEW).unwrap_err().contains("restart your node"));
        // Holding something.
        let mut node = Node::new(&[("wallet_get_known_addresses", Ok(json!([A, B, LF, NEW]))), ("wallet_get_balance", Ok(json!("5000")))]);
        let held = remove(&mut |m, a| node.ask(m, a), &mut records, NEW).unwrap_err();
        assert!(held.contains("holds 5000"), "{held}");
        assert!(records[NEW].get("hidden").is_none());
        // Empty as the node sees it ("Unknown wallet address."): hidden, and its key is not removed.
        let mut node = Node::new(&[
            ("wallet_get_known_addresses", Ok(json!([A, B, LF, NEW]))),
            ("wallet_get_balance", Err("Unknown wallet address.".to_owned())),
            ("read_accounts", Ok(read(&[("names-1", NEW)]))),
            ("remove_key", Ok(Value::Null)),
        ]);
        let gone = remove(&mut |m, a| node.ask(m, a), &mut records, NEW).unwrap();
        assert_eq!(gone, json!({ "key": NEW, "title": "names-1", "hidden": true, "note": HIDDEN }));
        assert!(!node.methods().contains(&"remove_key"), "the key's secret is never deleted: {:?}", node.methods());
        assert_eq!(records[NEW], json!({ "title": "names-1", "label": "x", "at": 1, "hidden": true }));
        // chain_accounts leaves it out while it holds nothing (no row, or a "0" row)...
        assert_eq!(annotate(json!([{ "key": A, "balance": "9" }]), &[A.into(), NEW.into()], &records), json!([{ "key": A, "balance": "9" }]));
        assert_eq!(annotate(json!([{ "key": NEW, "balance": "0" }]), &[NEW.into()], &records), json!([]));
        // ...and shows it again once the payment that was on its way lands (adv2 D2: 1,000).
        assert_eq!(annotate(json!([{ "key": NEW, "balance": "1000" }]), &[NEW.into()], &records),
            json!([{ "key": NEW, "balance": "1000", "title": "names-1", "label": "x", "ready": true }]));
    }

    /// adv2 D3: after a removal the keystore no longer held a key the running node lists,
    /// so every later new account and removal was refused until the node restarted.
    /// Hiding changes nothing in the keystore: the next account is made at once.
    #[test]
    fn an_account_is_made_right_after_one_is_hidden() {
        let mut records = Map::new();
        records.insert(NEW.into(), json!({ "title": "names-1", "label": "", "at": 1 }));
        let mut node = Node::new(&[("wallet_get_known_addresses", Ok(json!([A, B, LF, NEW]))), ("wallet_get_balance", Err("Unknown wallet address.".to_owned()))]);
        remove(&mut |m, a| node.ask(m, a), &mut records, NEW).unwrap();
        let next = "cd".repeat(32);
        let mut node = Node::new(&[
            ("wallet_get_known_addresses", Ok(json!([A, B, LF, NEW]))),
            ("read_accounts", Ok(read(&[("names-1", NEW)]))),
            ("read_accounts", Ok(read(&[("names-1", NEW), ("names-2", next.as_str())]))),
            ("generate_key", Ok(json!(next.as_str()))),
        ]);
        let made = make(&mut |m, a| node.ask(m, a), &files(), &mut records, "After", 2).unwrap();
        assert_eq!(made["title"], "names-2", "the hidden account's title stays taken: {made}");
    }

    #[test]
    fn rows_show_labels_and_the_accounts_made_here() {
        let rows = json!([{ "key": A, "balance": "100000" }, { "key": B, "balance": "100" }]);
        let mut records = Map::new();
        records.insert(A.into(), json!({ "label": "Main" }));
        records.insert(NEW.into(), json!({ "title": "names-1", "label": "Savings", "at": 2 }));
        let other = "ab".repeat(32);
        records.insert(other.clone(), json!({ "title": "names-2", "label": "", "at": 3 }));
        // NEW is listed by the node (no notes yet), the other isn't until the node restarts.
        let shown = annotate(rows.clone(), &[A.into(), B.into(), NEW.into()], &records);
        assert_eq!(shown, json!([
            { "key": A, "balance": "100000", "label": "Main", "ready": true },
            { "key": B, "balance": "100" },
            { "key": NEW, "balance": "0", "title": "names-1", "label": "Savings", "ready": true },
            { "key": other, "balance": null, "title": "names-2", "label": "", "ready": false },
        ]));
        assert_eq!(annotate(rows.clone(), &[], &Map::new()), rows, "no records: the rows as they were");
    }

    #[test]
    fn a_label_goes_on_a_key_the_node_lists_or_one_made_here() {
        let mut records = Map::new();
        let mut node = Node::new(&[("wallet_get_known_addresses", known_ab())]);
        assert_eq!(relabel(&mut |m, a| node.ask(m, a), &mut records, A, " Main ").unwrap(), json!({ "key": A, "label": "Main" }));
        assert_eq!(records[A], json!({ "label": "Main" }));
        assert!(relabel(&mut |m, a| node.ask(m, a), &mut records, NEW, "x").unwrap_err().contains("doesn't hold"));
        relabel(&mut |m, a| node.ask(m, a), &mut records, A, "").unwrap();
        assert!(records.is_empty(), "an empty label on a key not made here leaves no record");
    }
}
