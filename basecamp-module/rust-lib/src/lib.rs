//! Logos Names module: lets any Logos app resolve and manage `.lez` names
//! with one call, against registry v5 (SPEC.md).
//!
//! The trait below is the module's contract; the builder derives the `.lidl`
//! from it. Every method that answers a question returns JSON text in the
//! shape Logos modules use elsewhere:
//!
//! ```json
//! { "success": true,  "value": <answer>, "error": null }
//! { "success": false, "value": null,     "error": "why" }
//! ```
//!
//! A wallet's send field needs only `recipient`.
//!
//! Names are private: the chain holds an identifier derived from each name
//! (Argon2id, then SHA-256) and a sealed profile, never the name, and there is
//! no list of names. Whoever knows a name can resolve it; nobody can browse.
//! A record names no holder either, only a tag of the holder's account bound
//! to that one name.
//!
//! Registry v5 holds a name by a refundable deposit: it never lapses, and the
//! deposit comes back to whoever releases it. One- and two-character names are
//! locked, and three-character names take a higher deposit; the module proves a
//! name's length tier from a built-in copy of the short-name list. Claiming a
//! name is commit, then reveal at least 15 minutes later (see `register`).
//!
//! Reading needs nothing but the sequencer. Changing a name needs a signature,
//! which this module asks the LEZ wallet module (`lez_core`) for: the keys stay
//! in the wallet, and the wallet must be open. The module signs only with the
//! wallet's public accounts.
//!
//! A name held privately by the person's own `names` CLI is changed through
//! that CLI instead, run in the background as a job (`cli_*` and `job`; see
//! `cli.rs`): its changes are proven on this machine with the CLI's own keys.

mod avatar;
mod changes;
mod cli;
mod page_view;
mod spell;
mod registry;
mod shortlist;

use changes::Change;
use registry::{AccountId, Derived, Registry, Settings};
use serde_json::{json, Map, Value};
use std::collections::HashMap;

const DEFAULT_SEQUENCER: &str = "https://testnet.lez.logos.co";
/// The canonical registry on the default network: the one SPEC.md section 11
/// lists, registry v5 in deposit mode.
const DEFAULT_REGISTRY: &str = "38jEfVV1RCyMzu1FGbYcCn5FqxCxKyY58VgzjruAh2gq";
/// Registries a saved configuration may still name whose names use an older
/// derivation and can't be read by this version (v2, v3, v4.0 and v4.1 on the
/// public testnet). They are ignored.
const RETIRED_REGISTRIES: [&str; 4] = [
    "DugXYzVqYsXJumqsjaGtoZeJ742WmdRHGpByi8pNJm2z",
    "66nUvzJtTxA729aUpTcAWDrb7cV9Heb2bDzceHQhLYad",
    "E6oXebf6sicLtYBpEMijFdB8mqbXg3HRsWfYoJBgjvdj",
    "2PXg3SSmh5hvECTz4xcfV2d5YvxwyF6yVhpnjTokzCkX",
];

pub trait NamesModule: Send + 'static {
    /// Where to send a payment for `name` (`jef.lez` or `jef`). The value is
    /// `{name, display, public, public_hex, private}`: `private` is
    /// `{nullifier_public_key, viewing_public_key}` in hex or null, and
    /// `public` is an account ID or null. Prefer `private` when it is there.
    /// A null value means the name is not registered.
    fn recipient(&mut self, name: String) -> String;

    /// Everything `name` resolves to: `{name, display, owner_tag, expires,
    /// never, lapsed, deposit, account, npk, vpk, texts}`. A name held by a
    /// deposit never expires: `expires` is null, `never` true and `lapsed`
    /// false; `deposit` is what its deposit account holds (decimal text), which
    /// a release refunds. In fee mode `expires` is in milliseconds and `lapsed`
    /// is true once it has passed. `owner_tag` is the holder's tag (hex): the
    /// chain never names the holder. A null value means it is not registered,
    /// which includes a fee-mode name that lapsed more than 30 days ago.
    fn resolve(&mut self, name: String) -> String;

    /// The page published under `name`: `{content_type, bytes, text}`, where
    /// `text` is the body if it is valid UTF-8. A null value means no page.
    /// The page is written by the name's owner: treat it as untrusted content.
    fn page(&mut self, name: String) -> String;

    /// The page published under `name`, made safe to show inside an app:
    /// `{content_type, bytes, kind, title, html, text, truncated}`. `kind` is
    /// `html`, `text` or `file`. For an HTML page, `html` is the body rewritten
    /// to an allowlist Qt's rich text can draw (headings, paragraphs, lists,
    /// tables, emphasis, code, and links to `http:`/`https:` URLs only; no
    /// scripts, styles, images, attributes or comments), and `title` is its
    /// `<title>` as plain text; both are null otherwise. `text` is the page's
    /// own source, as in `page`, to be shown only as plain text. At most 64 KB
    /// of a page is read; `truncated` says it was cut. A null value means no page.
    fn page_view(&mut self, name: String) -> String;

    /// `body`, a page not yet published, as `page_view` would show it: an
    /// HTML body (one that opens with a tag) sanitized the same way, anything
    /// else as plain text. For previewing a page while it is written.
    fn page_preview(&mut self, body: String) -> String;

    /// The picture in `name`'s `avatar` entry, decoded, for an app that can't
    /// load a `data:` URL: `{width, height, pixels}`, where `pixels` is base64
    /// of RGBA8 bytes, row by row from the top left. At most 128 pixels a side
    /// (a larger picture is scaled down, nearest neighbour). Only PNG is
    /// decoded. A null value means the name has no avatar this can decode.
    fn avatar(&mut self, name: String) -> String;

    /// The name `account` (base58) has published for itself, or null. The
    /// answer is checked against the name's own record, so it cannot be spoofed.
    fn reverse(&mut self, account: String) -> String;

    /// The bare name for what a person typed (`Jef.LEZ` gives `jef`), or an
    /// empty string if it is not a valid name. Makes no network call.
    fn normalize(&mut self, input: String) -> String;

    /// The open LEZ wallet: `{accounts: [{account, balance, funded, name}],
    /// private_accounts: [account]}`. `accounts` are its public accounts,
    /// funded ones first; `balance` is decimal text and `name` is the name the
    /// account has published, or null. Fails if the LEZ wallet is not
    /// installed or not open.
    fn wallet(&mut self) -> String;

    /// The names this device remembers that the open wallet holds, and the
    /// registrations it has under way: `{names: [{name, display, owner,
    /// expires, never, lapsed}], pending: [{name, display, owner, stage,
    /// ready_at, tx}]}`. `owner` is the wallet's account that holds the name.
    /// A pending `stage` is `committed` (call `register` again from
    /// `ready_at`, in milliseconds) or `revealed` (sent; it shows in `names`
    /// once it lands). The chain keeps no list of names, so this is the
    /// device's own memory: every name registered from here, and every name
    /// passed to `keep`.
    fn mine(&mut self) -> String;

    /// Remembers `name` on this device, if the open wallet holds it, so `mine`
    /// keeps returning it. Fails for a name the wallet does not hold. The value is the bare name.
    fn keep(&mut self, name: String) -> String;

    /// What registering `name` takes: `{name, display, tier, deposit, mode}`.
    /// `tier` is `normal`, `three` (a three-character name, at the higher
    /// deposit) or `locked` (one or two characters: only the registry's admin
    /// may register it, and `deposit` is null). `deposit` is decimal text in
    /// the native token's base units, held while the name is and refunded on
    /// release. `mode` is `deposit`, `fee` or `unconfigured`; in fee mode
    /// `deposit` is 0 and `fee` gives the price of a one-year term instead.
    /// Makes one network read, besides deriving the name.
    fn deposit(&mut self, name: String) -> String;

    /// The registry's settings: `{mode, deposits: {normal, three}, admin,
    /// treasury, prices: {30d, 1y, 5y}}`, amounts as decimal text. A null value
    /// means the registry has no settings yet (it charges nothing, and locks
    /// one- and two-character names).
    fn settings(&mut self) -> String;

    // Changing a name. Each method below checks the change against the chain,
    // has the LEZ wallet sign it with the account that must authorise it, and
    // sends it. The value is `{name, signer, tx}`, plus what the method lists.
    // A sent transaction can still be refused, so confirm by calling `resolve`
    // until the change shows; it normally takes one block. Send one change per
    // account at a time.

    /// Registers `name` to `owner` (base58), a public account in the open
    /// wallet, which pays the deposit (see `deposit`). Registering is two
    /// steps, so that nobody watching pending transactions can take the name
    /// first: the first call sends a commitment, which hides the name, and
    /// answers `stage: "committed"` with `ready_at` (milliseconds, chain time,
    /// about 15 minutes on). Calling `register` again from then on sends the
    /// reveal that claims the name (`stage: "revealed"`), and once that has
    /// landed, answers `stage: "registered"` without sending anything. A call
    /// before `ready_at` sends nothing. The commitment is kept on this device
    /// and stays usable for 24 hours. The value also has `deposit` (or `fee`)
    /// and `tier`; `tx` is null when nothing was sent.
    /// `private_account` makes the name payable privately from the start: one
    /// of the wallet's private accounts (base58), `auto` for its first one
    /// (made if it has none), or empty to set nothing. It is used by the
    /// reveal; given at commit time it is remembered for it.
    fn register(&mut self, name: String, owner: String, private_account: String) -> String;

    /// Makes `name` payable privately and only privately: publishes the
    /// receiving keys of `private_account` (as for `register`, `auto` allowed)
    /// and removes any public account from the name.
    fn receive_privately(&mut self, name: String, private_account: String) -> String;

    /// Points `name` at a public account (base58). Anyone who resolves the
    /// name then sees that account, its balance and its history.
    fn set_account(&mut self, name: String, account: String) -> String;

    /// Publishes the keys for private payments to `name`, both in hex: a
    /// 32-byte nullifier public key and a 1184-byte viewing public key.
    fn set_private_keys(&mut self, name: String, nullifier_public_key: String, viewing_public_key: String) -> String;

    /// Removes the public account and the private keys from `name`.
    fn clear_addresses(&mut self, name: String) -> String;

    /// Sets the text entry `key` on `name`; an empty value removes it.
    fn set_text(&mut self, name: String, key: String, value: String) -> String;

    /// Writes many text entries on `name` in one transaction, as the CLI's
    /// `names profile` does. `entries_json` is a JSON object of key to value
    /// (text); an empty value (or null) removes that key. The card keys are
    /// `display`, `bio`, `avatar` (a `data:image/...` URL), `theme` (`light`,
    /// `dark` or `auto`), `color` (`#rrggbb`), `font` (`sans`, `serif`, `mono`
    /// or `rounded`) and `cid` (a Logos Storage content ID); any other key of
    /// 1-32 characters of a-z, 0-9, `-`, `_` or `.` works too. With `replace`,
    /// every existing entry is dropped first, so the object becomes the whole
    /// set; the name's account and payment keys stay either way. A name holds
    /// at most 16 entries and its sealed profile at most 12 KiB.
    fn set_profile(&mut self, name: String, entries_json: String, replace: bool) -> String;

    /// Publishes `body` as the page of `name`; an empty body removes the page.
    fn set_page(&mut self, name: String, content_type: String, body: String) -> String;

    /// Hands `name` to `new_owner` (base58); only a tag of it reaches the
    /// chain. The addresses, entries and page the current owner set are
    /// emptied. The deposit stays with the name, for whoever releases it.
    fn transfer(&mut self, name: String, new_owner: String) -> String;

    /// Gives `name` up. Its record and page are emptied, anyone may register
    /// it again, and its deposit comes back to the holder: the value has
    /// `refund`, the deposit account's whole balance (decimal text).
    fn release(&mut self, name: String) -> String;

    /// Publishes `name` as what `account` (base58, in the open wallet) is
    /// shown as; an empty name withdraws it. This is the one change that puts
    /// a name on the chain in the clear.
    fn set_primary(&mut self, account: String, name: String) -> String;

    /// The sequencer URL, registry program ID and wallet files in use.
    fn get_config(&mut self) -> String;

    /// Use the registry with this program account ID (base58), and remember it
    /// across restarts. Returns false and changes nothing if it is not an
    /// account ID.
    fn set_registry(&mut self, program_id: String) -> bool;

    /// Read from the sequencer at this URL, and remember it across restarts.
    /// Returns false and changes nothing unless it is an http or https URL.
    fn set_sequencer(&mut self, url: String) -> bool;

    /// Names the wallet to open when the LEZ wallet module is running with no
    /// wallet open: the paths of its configuration and storage files, as the
    /// LEZ Wallet app writes them. Remembered across restarts; two empty paths
    /// forget it. Returns false and changes nothing if a file does not exist.
    fn set_wallet_files(&mut self, config_path: String, storage_path: String) -> bool;

    // Names held through the person's own `names` CLI. A name held by a
    // private account is changed with a proof made on this machine, which the
    // CLI does with the keys in its own wallet directory. Each change below
    // runs the CLI in the background, as a job, and answers at once with
    // `{id, name, kind, state, started}`; `job` says how it is going. Proving
    // takes a few minutes. At most one job runs per name. The CLI is given
    // this module's registry and sequencer, and never run through a shell.

    /// The names the CLI's wallet holds, as `names mine` lists them: `[{name,
    /// display, signer, state, lapsed}]`, `signer` being `cli`. A wallet
    /// directory with no wallet in it holds none (and the CLI is not run, so
    /// it makes none). Waits for the CLI, at most 15 seconds.
    fn cli_mine(&mut self) -> String;

    /// Starts `names profile` for `name`: `entries_json` is a JSON object of
    /// key to text, as for `set_profile`. `display`, `bio`, `theme`, `color`,
    /// `font` and `cid` go to their own options, any other key to `--set`, and
    /// an empty value (or null) removes the key. `avatar` is passed only when
    /// it is the full path of a PNG file on this machine; any other avatar
    /// value is left as it is (with `replace`, a `data:image/` URL is kept),
    /// and the answer's `skipped` lists it.
    fn cli_profile(&mut self, name: String, entries_json: String, replace: bool) -> String;

    /// Starts `names set-page` with `body` (written to a file in the jobs
    /// directory, removed once the job ends); an empty body starts `names
    /// clear-page`.
    fn cli_set_page(&mut self, name: String, content_type: String, body: String) -> String;

    /// Starts `names set-text`; an empty value removes the entry.
    fn cli_set_text(&mut self, name: String, key: String, value: String) -> String;

    /// Registers `name` through the names CLI, from its private account, so
    /// the holder never appears on chain. Starts a job like `cli_set_text`;
    /// it ends once the commitment is made (a proof of a few minutes), and
    /// the CLI finishes the registration in the background about 15 minutes
    /// later. `cli_mine` lists it with `registering` true until then. With
    /// `public`, the CLI then moves it to a new public account funded privately
    /// (`names make-public`), so it posts in one block and nothing links it to you.
    fn cli_register(&mut self, name: String, public: bool) -> String;

    // ── The LNS board (see board/ and docs/BOARD-PLAN.md) ──────────────

    /// The board's recent posts, newest first, read through the names CLI:
    /// `{posts, updating, error}`, where `posts` is `[{post, name, n, parent,
    /// time, text, cids, likes, hidden, block}]` (`post` and `parent` written
    /// `<name>/<n>`). Reading takes the CLI many seconds, so this answers at once
    /// with the last result and fetches a newer one on its own thread; while
    /// `updating` is true, ask again shortly. With `following` true, only posts
    /// by the names this device follows.
    fn board_feed(&mut self, following: bool) -> String;

    /// A post (`<name>/<n>`) and its replies, oldest first, answered as `board_feed` is.
    fn board_thread(&mut self, post: String) -> String;

    /// Posts `text` as `name`, which the CLI's wallet holds privately, with up to
    /// four Logos Storage CIDs (`cids_json`, a JSON list) and, if `reply` is a
    /// post, as a reply to it. A CLI job, as `cli_set_text`; about 4 minutes.
    fn board_post(&mut self, name: String, text: String, cids_json: String, reply: String) -> String;

    /// Likes `post` as `name` (or takes the like back with `undo`). A CLI job.
    fn board_like(&mut self, name: String, post: String, undo: bool) -> String;

    /// Takes down `post`, one of the CLI wallet's names' posts. A CLI job.
    fn board_hide(&mut self, post: String) -> String;

    /// The names this device follows, a JSON list.
    fn board_follows(&mut self) -> String;

    /// Follows `name` (or stops, with `undo`); kept on this device only.
    fn board_follow(&mut self, name: String, undo: bool) -> String;

    /// The name this device posts, likes and pays as, or null if none is set.
    fn main_name(&mut self) -> String;

    /// Sets the name this device posts, likes and pays as.
    fn set_main_name(&mut self, name: String) -> String;

    /// The names CLI wallet's anonymous name (`names anon`), or null if it has none yet.
    fn anon_name(&mut self) -> String;

    /// Makes the wallet's anonymous name, a CLI job (`names anon`): a private
    /// transfer of a few minutes, then a registration of about 20 that the CLI
    /// finishes in the background.
    fn make_anon(&mut self, public: bool) -> String;

    /// The CLI jobs still running, `[{id, name, kind, state, started}]`, so an app
    /// opened mid-change can show it and wait for it.
    fn running_jobs(&mut self) -> String;

    /// Moves `name` from the CLI wallet's private account to a new public account
    /// in it (`names make-public`), so its posts take one block and pay gas. A CLI
    /// job of about 9 minutes; the name keeps its profile, page and receiving keys.
    fn make_public(&mut self, name: String) -> String;

    /// The posts this device's names have liked, `<name>/<n>`.
    fn board_likes(&mut self) -> String;

    /// Which name each chat conversation is with: `{convo_id: name}`.
    fn dm_names(&mut self) -> String;

    /// Remembers that chat conversation `convo_id` is with `name`; an empty
    /// name forgets it.
    fn set_dm_name(&mut self, convo_id: String, name: String) -> String;

    /// The words in `text` that look misspelled, `[{word, suggestions}]`, checked
    /// on this machine against the system's word list. Names, links and CIDs are skipped.
    fn spell_check(&mut self, text: String) -> String;

    /// Likes `post` as `name`, or takes the like back if this device liked it.
    /// A CLI job; the like is remembered here once it is started.
    fn board_toggle_like(&mut self, name: String, post: String) -> String;

    /// Pays `amount` (base units, decimal text) to `name` from the CLI wallet's
    /// private account, privately (`names pay`). A CLI job; about 4 minutes.
    fn cli_pay(&mut self, name: String, amount: String) -> String;

    /// The names CLI's default private account and its balance:
    /// `{account, balance}` (decimal text), or null if it has none. Runs
    /// `names balance`, which first catches up with the chain (seconds to a
    /// minute), so ask with a long timeout.
    fn cli_balance(&mut self) -> String;

    /// How job `id` is going: `{id, name, kind, state, started, finished,
    /// exit, log_tail}`. `state` is `running`, `done` (the CLI exited 0: the
    /// change was sent and included; `resolve` shows it) or `failed`;
    /// `log_tail` is the end of the CLI's output, whose last line says why it
    /// failed. Times are in milliseconds.
    fn job(&mut self, id: String) -> String;

    /// Runs the `names` CLI at this path, and remembers it across restarts;
    /// an empty path goes back to the default (`~/.local/bin/names`, then the
    /// first `names` on PATH). Returns false and changes nothing if it is not
    /// an executable file.
    fn set_cli(&mut self, path: String) -> bool;

    /// Has the CLI use the wallet in this directory (`LEE_WALLET_HOME_DIR`),
    /// and remembers it across restarts; an empty path goes back to the
    /// default, `~/.lee/wallet`. Returns false and changes nothing if it is
    /// not a directory.
    fn set_cli_wallet(&mut self, dir: String) -> bool;

    /// Called once the module is wired to the host.
    fn on_context_ready(&mut self, _ctx: &RustModuleContext) {}
}

// The builder injects the generated scaffold here: the `install` entry point,
// `RustModuleContext`, `context()` and `modules()`. Unit tests build without it.
#[cfg(not(test))]
include!(concat!(env!("CARGO_MANIFEST_DIR"), "/generated/provider_gen.rs"));

#[cfg(test)]
pub struct RustModuleContext;

struct Names {
    sequencer: String,
    program: Option<AccountId>,
    /// Deriving a name's identifier and key is slow on purpose, so each name
    /// asked about is derived once.
    derived: HashMap<String, Derived>,
    /// The wallet's configuration and storage files, if one was named.
    wallet_files: Option<(String, String)>,
    /// The `names` CLI and its wallet directory, if set (see `set_cli`).
    cli: Option<String>,
    cli_wallet: Option<String>,
    /// Where CLI jobs keep their logs; the config directory's `jobs` if unset.
    jobs_dir: Option<std::path::PathBuf>,
    /// The names followed on the board, kept on this device only.
    follows: Vec<String>,
    /// The name this device posts, likes and pays as (see `set_main_name`).
    main_name: Option<String>,
    /// The posts this device's names have liked, `<name>/<n>`, so a like toggles.
    likes: Vec<String>,
    /// Which name each chat conversation is with, by conversation id, as this
    /// device started or labelled it. The chat module itself keeps no names.
    dm_names: std::collections::BTreeMap<String, String>,
    /// Board reads, by what was asked: they take the CLI many seconds, so they
    /// run on their own thread and every call answers at once with the last result.
    board_reads: std::sync::Arc<std::sync::Mutex<HashMap<String, BoardRead>>>,
}

/// A board read: the last result, and whether a newer one is being fetched.
#[derive(Default)]
struct BoardRead {
    value: Option<Value>,
    error: Option<String>,
    updating: bool,
}

/// Where this module keeps what it remembers between runs.
fn config_dir() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(std::path::Path::new(&home).join(".config/logos-names"))
}

impl Default for Names {
    fn default() -> Self {
        let mut names = Self {
            sequencer: DEFAULT_SEQUENCER.to_owned(),
            program: registry::parse_account(DEFAULT_REGISTRY).ok(),
            derived: HashMap::new(),
            wallet_files: None,
            cli: None,
            cli_wallet: None,
            jobs_dir: None,
            follows: Vec::new(),
            main_name: None,
            likes: Vec::new(),
            dm_names: Default::default(),
            board_reads: Default::default(),
        };
        // A missing or unreadable file just means the defaults stand.
        let saved = config_dir()
            .and_then(|dir| std::fs::read_to_string(dir.join("config.json")).ok())
            .and_then(|text| serde_json::from_str::<Value>(&text).ok());
        if let Some(saved) = saved {
            names.apply_saved(&saved);
        }
        names
    }
}

fn is_http_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

fn reply(outcome: Result<Value, String>) -> String {
    match outcome {
        Ok(value) => json!({ "success": true, "value": value, "error": null }),
        Err(error) => json!({ "success": false, "value": null, "error": error }),
    }
    .to_string()
}

/// Adds the fields of `extra`, when it is an object, to `value`.
fn merged(mut value: Value, extra: &Value) -> Value {
    if let (Some(value), Some(extra)) = (value.as_object_mut(), extra.as_object()) {
        for (key, field) in extra {
            value.insert(key.clone(), field.clone());
        }
    }
    value
}

/// Text entries from a JSON object of key to text; null counts as empty.
fn parse_entries(entries_json: &str) -> Result<Vec<(String, String)>, String> {
    let parsed: Value = serde_json::from_str(entries_json.trim())
        .map_err(|error| format!("the entries are not JSON: {error}"))?;
    let object = parsed
        .as_object()
        .ok_or("the entries must be a JSON object of key to text")?;
    object
        .iter()
        .map(|(key, value)| match value {
            Value::String(text) => Ok((key.trim().to_owned(), text.clone())),
            Value::Null => Ok((key.trim().to_owned(), String::new())),
            _ => Err(format!("the value of {key} must be text")),
        })
        .collect()
}

/// The names this device remembers, per registry. The chain keeps no list, so
/// without this a person could not be shown the names they hold.
struct Memory {
    path: std::path::PathBuf,
    registries: Map<String, Value>,
}

/// A remembered name that is not registered is dropped after this long. Until
/// then it may be a registration that has not reached a block.
const FORGET_UNREGISTERED_AFTER: u64 = 24 * 60 * 60;

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

fn load_object(path: &std::path::Path) -> Map<String, Value> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|saved| saved.as_object().cloned())
        .unwrap_or_default()
}

/// Failing to save is not an error for the caller: the memory still serves
/// this run.
fn save_object(path: &std::path::Path, object: &Map<String, Value>) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, Value::Object(object.clone()).to_string());
}

impl Memory {
    fn load(path: std::path::PathBuf) -> Self {
        let registries = load_object(&path);
        Self { path, registries }
    }

    /// The names remembered for `program`, each with when it was added.
    fn names(&self, program: &AccountId) -> Vec<(String, u64)> {
        self.registries
            .get(&registry::account_text(program))
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .filter_map(|entry| Some((entry["name"].as_str()?.to_owned(), entry["added"].as_u64()?)))
            .collect()
    }

    fn set(&mut self, program: &AccountId, names: &[(String, u64)]) {
        let entries: Vec<Value> = names
            .iter()
            .map(|(name, added)| json!({ "name": name, "added": added }))
            .collect();
        self.registries.insert(registry::account_text(program), Value::from(entries));
    }

    /// Adds `name` for `program` unless it is already there.
    fn remember(&mut self, program: &AccountId, name: &str) {
        let mut names = self.names(program);
        if !names.iter().any(|(held, _)| held == name) {
            names.push((name.to_owned(), now()));
            self.set(program, &names);
        }
    }

    fn save(&self) {
        save_object(&self.path, &self.registries);
    }
}

fn memory() -> Result<Memory, String> {
    config_dir()
        .map(|dir| Memory::load(dir.join("names.json")))
        .ok_or_else(|| "this device has nowhere to remember names".to_owned())
}

/// A commitment made and not yet revealed: what the reveal needs. The secret
/// is what binds the reveal to the commitment, so it stays on this device.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Commitment {
    owner: AccountId,
    secret: [u8; 32],
    /// The chain time the commitment claimed.
    made: u64,
    /// The `private_account` choice for the reveal.
    private_account: String,
    /// The reveal's transaction, once sent.
    revealed: Option<String>,
    /// The chain time the reveal claimed. Until it can no longer land, the
    /// reveal is not sent again.
    revealed_at: Option<u64>,
}

impl Commitment {
    fn to_json(&self) -> Value {
        json!({
            "owner": registry::account_text(&self.owner),
            "secret": registry::hex(&self.secret),
            "made": self.made,
            "private_account": self.private_account,
            "revealed": self.revealed,
            "revealed_at": self.revealed_at,
        })
    }

    fn from_json(value: &Value) -> Option<Self> {
        Some(Self {
            owner: registry::parse_account(value["owner"].as_str()?).ok()?,
            secret: registry::unhex(value["secret"].as_str()?).ok()?.try_into().ok()?,
            made: value["made"].as_u64()?,
            private_account: value["private_account"].as_str().unwrap_or_default().to_owned(),
            revealed: value["revealed"].as_str().map(str::to_owned),
            revealed_at: value["revealed_at"].as_u64(),
        })
    }

    /// When the reveal may be sent, in chain time.
    fn ready_at(&self) -> u64 {
        self.made.saturating_add(registry::MIN_COMMIT_AGE_MS)
    }

    /// Whether a reveal claiming `chain_now` can still land before the
    /// commitment expires.
    fn usable_at(&self, chain_now: u64) -> bool {
        chain_now.saturating_add(registry::TIME_WINDOW_MS) < self.made.saturating_add(registry::MAX_COMMIT_AGE_MS)
    }
}

/// The commitments this device has under way, per registry and name.
struct Pending {
    path: std::path::PathBuf,
    registries: Map<String, Value>,
}

impl Pending {
    fn load(path: std::path::PathBuf) -> Self {
        let registries = load_object(&path);
        Self { path, registries }
    }

    fn all(&self, program: &AccountId) -> Vec<(String, Commitment)> {
        self.registries
            .get(&registry::account_text(program))
            .and_then(Value::as_object)
            .map(|names| {
                names
                    .iter()
                    .filter_map(|(name, entry)| Some((name.clone(), Commitment::from_json(entry)?)))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn get(&self, program: &AccountId, name: &str) -> Option<Commitment> {
        Commitment::from_json(self.registries.get(&registry::account_text(program))?.get(name)?)
    }

    fn put(&mut self, program: &AccountId, name: &str, pending: &Commitment) {
        let names = self
            .registries
            .entry(registry::account_text(program))
            .or_insert_with(|| Value::Object(Map::new()));
        if let Some(names) = names.as_object_mut() {
            names.insert(name.to_owned(), pending.to_json());
        }
    }

    fn remove(&mut self, program: &AccountId, name: &str) {
        if let Some(names) = self.registries.get_mut(&registry::account_text(program)).and_then(Value::as_object_mut) {
            names.remove(name);
        }
    }

    fn save(&self) {
        save_object(&self.path, &self.registries);
    }
}

fn pending() -> Result<Pending, String> {
    config_dir()
        .map(|dir| Pending::load(dir.join("pending.json")))
        .ok_or_else(|| "this device has nowhere to keep a commitment".to_owned())
}

impl Names {
    /// Applies a saved configuration. A registry that this version can't read
    /// is ignored, so the default stands.
    fn apply_saved(&mut self, saved: &Value) {
        if let Some(url) = saved["sequencer"].as_str().filter(|url| is_http_url(url)) {
            self.sequencer = url.to_owned();
        }
        if let Some(program) = saved["registry"].as_str().filter(|program| !RETIRED_REGISTRIES.contains(program)) {
            self.program = registry::parse_account(program).ok().or(self.program);
        }
        if let (Some(config), Some(storage)) = (saved["wallet_config"].as_str(), saved["wallet_storage"].as_str()) {
            self.wallet_files = Some((config.to_owned(), storage.to_owned()));
        }
        self.cli = saved["cli"].as_str().filter(|path| !path.is_empty()).map(str::to_owned);
        self.cli_wallet = saved["cli_wallet"].as_str().filter(|dir| !dir.is_empty()).map(str::to_owned);
        self.follows = saved["follows"]
            .as_array()
            .map(|list| list.iter().filter_map(|name| registry::parse_name(name.as_str()?).ok()).collect())
            .unwrap_or_default();
        self.main_name = saved["main_name"].as_str().and_then(|name| registry::parse_name(name).ok());
        self.dm_names = saved["dm_names"]
            .as_object()
            .map(|map| {
                map.iter()
                    .filter_map(|(convo, name)| Some((convo.clone(), registry::parse_name(name.as_str()?).ok()?)))
                    .collect()
            })
            .unwrap_or_default();
        self.likes = saved["likes"]
            .as_array()
            .map(|list| list.iter().filter_map(|post| post.as_str().map(str::to_owned)).collect())
            .unwrap_or_default();
    }

    fn config(&self) -> Value {
        json!({
            "sequencer": self.sequencer,
            "registry": self.program.as_ref().map(registry::account_text),
            "wallet_config": self.wallet_files.as_ref().map(|(config, _)| config),
            "wallet_storage": self.wallet_files.as_ref().map(|(_, storage)| storage),
            "cli": self.cli,
            "cli_wallet": self.cli_wallet,
            "follows": self.follows,
            "main_name": self.main_name,
            "likes": self.likes,
            "dm_names": self.dm_names,
        })
    }

    /// The configuration as `get_config` shows it: the CLI and its wallet
    /// directory as they will be used, defaults filled in (`cli` is null if
    /// no CLI is found).
    fn shown_config(&self) -> Value {
        let mut config = self.config();
        config["cli"] = json!(cli::find_program(self.cli.as_deref()).map(|path| path.to_string_lossy().into_owned()));
        config["cli_wallet"] = json!(cli::find_wallet(self.cli_wallet.as_deref()).map(|dir| dir.to_string_lossy().into_owned()));
        config
    }

    /// The CLI to run for this module's registry and sequencer.
    /// Answers a board read at once with what it last returned, `{posts,
    /// updating, error}`, and starts fetching a newer one on its own thread
    /// unless one is already on the way.
    fn board_read_now(&self, key: &str) -> Value {
        let reads = self.board_reads.lock().expect("the board reads lock");
        let read = reads.get(key);
        json!({
            "posts": read.and_then(|read| read.value.clone()).unwrap_or_else(|| json!([])),
            "updating": read.is_some_and(|read| read.updating),
            "error": read.and_then(|read| read.error.clone()),
        })
    }

    fn board_read(&self, key: String, fetch: impl FnOnce() -> Result<Value, String> + Send + 'static) -> Value {
        {
            let mut reads = self.board_reads.lock().expect("the board reads lock");
            let read = reads.entry(key.clone()).or_default();
            if !read.updating {
                read.updating = true;
                let reads = std::sync::Arc::clone(&self.board_reads);
                let key = key.clone();
                std::thread::spawn(move || {
                    let fetched = fetch();
                    let mut reads = reads.lock().expect("the board reads lock");
                    let read = reads.entry(key).or_default();
                    read.updating = false;
                    match fetched {
                        Ok(value) => {
                            read.value = Some(value);
                            read.error = None;
                        }
                        Err(error) => read.error = Some(error),
                    }
                });
            }
        }
        self.board_read_now(&key)
    }

    fn cli_setup(&self) -> Result<cli::Setup, String> {
        let program = cli::find_program(self.cli.as_deref()).ok_or(
            "no names CLI found at ~/.local/bin/names or on PATH; install it, or give its path in App settings",
        )?;
        let wallet = cli::find_wallet(self.cli_wallet.as_deref()).ok_or("no wallet directory for the names CLI")?;
        Ok(cli::Setup {
            program,
            wallet,
            registry: self.program.as_ref().map(registry::account_text),
            sequencer: self.sequencer.clone(),
        })
    }

    fn jobs(&self) -> Result<cli::Jobs, String> {
        self.jobs_dir
            .clone()
            .or_else(|| config_dir().map(|dir| dir.join("jobs")))
            .map(|dir| cli::Jobs { dir })
            .ok_or_else(|| "this device has nowhere to keep a job's log".to_owned())
    }

    /// Starts a CLI job for the name a person typed. `build` makes the
    /// command from the bare name and the job's id.
    fn cli_job(
        &self,
        typed: &str,
        kind: &str,
        build: impl FnOnce(&cli::Jobs, &str, &str) -> Result<(Vec<String>, Option<std::path::PathBuf>, Value), String>,
    ) -> String {
        reply((|| {
            let name = registry::parse_name(typed)?;
            let setup = self.cli_setup()?;
            setup.wallet_ready()?;
            let jobs = self.jobs()?;
            let id = jobs.prepare(&name)?;
            let (command, scratch, extra) = build(&jobs, &name, &id)?;
            let started = jobs.start(&setup, &id, &name, kind, command, scratch)?;
            Ok(merged(started, &extra))
        })())
    }

    /// The open wallet's accounts. If the wallet module is running with no
    /// wallet open and one has been named, it is asked to open that one first.
    fn held(&self) -> Result<Held, String> {
        match (wallet::accounts(), &self.wallet_files) {
            (Err(problem), Some((config, storage))) if problem == wallet::CLOSED => {
                wallet::open(config, storage)?;
                wallet::accounts()
            }
            (outcome, _) => outcome,
        }
    }

    /// Writes the configuration for the next run. Failing to save is not an
    /// error for the caller: the setting still applies to this run.
    fn save(&self) {
        if let Some(dir) = config_dir() {
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::write(dir.join("config.json"), self.config().to_string());
        }
    }

    fn registry(&self) -> Result<Registry, String> {
        let program = self
            .program
            .ok_or("no registry is configured; call set_registry with its program ID")?;
        Ok(Registry {
            sequencer: self.sequencer.clone(),
            program,
        })
    }

    /// The identifier and key of `name`, derived the first time it is asked for.
    fn derived(&mut self, name: &str) -> Derived {
        self.derived
            .entry(name.to_owned())
            .or_insert_with(|| registry::derive(name))
            .clone()
    }

    /// Answers a question about the name a person typed.
    fn about(&mut self, typed: &str, answer: impl FnOnce(&Registry, &str, &Derived) -> Result<Value, String>) -> String {
        reply((|| {
            let name = registry::parse_name(typed)?;
            let registry = self.registry()?;
            let derived = self.derived(&name);
            answer(&registry, &name, &derived)
        })())
    }

    /// Checks that the open wallet can sign and pay for `change`, has it sign
    /// and send it, and returns `{name, signer, tx}` with the change's extras.
    fn send(&self, registry: &Registry, held: &Held, name: &str, change: &Change) -> Result<Value, String> {
        if !held.public.contains(&change.signer) {
            return Err(format!(
                "the open wallet does not hold {} as a public account, which must sign this",
                registry::account_text(&change.signer)
            ));
        }
        let balance = registry.balance(&change.signer)?;
        if balance <= change.needs {
            return Err(if change.needs == 0 {
                format!("{} has no funds to pay the fee", registry::account_text(&change.signer))
            } else {
                format!(
                    "{} holds {balance}, not enough for the {} it pays plus the fee",
                    registry::account_text(&change.signer),
                    change.needs
                )
            });
        }
        let tx = wallet::send(&registry.program, change)?;
        Ok(merged(
            json!({
                "name": name,
                "signer": registry::account_text(&change.signer),
                "tx": tx,
            }),
            &change.extra,
        ))
    }

    /// Has the wallet sign and send the change `build` makes from the open
    /// wallet's public accounts. The signer must be one of them; it also pays.
    fn change(&self, name: &str, build: impl FnOnce(&Registry, &[AccountId]) -> Result<Change, String>) -> String {
        reply((|| {
            let registry = self.registry()?;
            let held = self.held()?;
            let change = build(&registry, &held.public)?;
            self.send(&registry, &held, name, &change)
        })())
    }

    /// The same, for a change to a name a person typed.
    fn change_name(
        &mut self,
        typed: &str,
        build: impl FnOnce(&Registry, &str, &Derived, &[AccountId]) -> Result<Change, String>,
    ) -> String {
        match registry::parse_name(typed) {
            Ok(name) => {
                let derived = self.derived(&name);
                self.change(&name, |registry, held| build(registry, &name, &derived, held))
            }
            Err(error) => reply(Err(error)),
        }
    }

    /// One step of registering `name` for `owner`: commit, wait, reveal, or
    /// report that it is done. See [`NamesModule::register`].
    fn register_step(&mut self, typed: &str, owner: &str, private_account: &str) -> Result<Value, String> {
        let name = registry::parse_name(typed)?;
        let registry = self.registry()?;
        let held = self.held()?;
        let owner = registry::parse_account(owner)?;
        if !held.public.contains(&owner) {
            return Err(format!(
                "the open wallet does not hold {} as a public account; this module registers names only to public accounts",
                registry::account_text(&owner)
            ));
        }
        let derived = self.derived(&name);
        let mut commitments = pending()?;
        let saved = commitments.get(&registry.program, &name).filter(|saved| saved.owner == owner);
        let status = |stage: &str, tx: Option<&str>, ready_at: Option<u64>| {
            json!({
                "name": name,
                "signer": registry::account_text(&owner),
                "tx": tx,
                "stage": stage,
                "ready_at": ready_at,
            })
        };

        if let Some(record) = registry.resolve(&name, &derived)? {
            if !record.is_held_by(&derived.id, &owner) {
                return Err(format!("{name}{} is already registered", registry::NAME_SUFFIX));
            }
            // An earlier reveal has landed.
            if saved.is_some() {
                commitments.remove(&registry.program, &name);
                commitments.save();
            }
            self.remember(&registry.program, &name);
            return Ok(status("registered", None, None));
        }

        let chain_now = registry.now()?;
        if let Some(mut saved) = saved.filter(|saved| saved.usable_at(chain_now)) {
            let promised = registry::commitment(&derived.id, &registry::owner_tag(&derived.id, &owner), &saved.secret);
            let landed = registry.commitment_made(&promised)?.is_some();
            if !landed && chain_now < saved.made.saturating_add(registry::TIME_WINDOW_MS) {
                // Sent, and still able to land.
                return Ok(status("committed", None, Some(saved.ready_at())));
            }
            if landed {
                if chain_now < saved.ready_at() {
                    return Ok(status("committed", None, Some(saved.ready_at())));
                }
                if saved.revealed_at.is_some_and(|at| chain_now < at.saturating_add(registry::TIME_WINDOW_MS)) {
                    // A reveal is on its way and may still land.
                    return Ok(status("revealed", saved.revealed.as_deref(), Some(saved.ready_at())));
                }
                let choice = if private_account.trim().is_empty() { saved.private_account.clone() } else { private_account.to_owned() };
                let keys = if choice.trim().is_empty() { None } else { Some(private_keys(&choice)?) };
                let change = registry.reveal(&name, &derived, &owner, &saved.secret, keys)?;
                let sent = self.send(&registry, &held, &name, &change)?;
                saved.revealed = sent["tx"].as_str().map(str::to_owned);
                saved.revealed_at = Some(chain_now);
                commitments.put(&registry.program, &name, &saved);
                commitments.save();
                self.remember(&registry.program, &name);
                return Ok(merged(sent, &json!({ "stage": "revealed", "ready_at": saved.ready_at() })));
            }
            // The commitment never landed: make a new one.
        }

        let secret: [u8; 32] = registry::fresh_bytes()?;
        let (change, made) = registry.commit(&name, &derived, &owner, &secret)?;
        let sent = self.send(&registry, &held, &name, &change)?;
        let saved = Commitment {
            owner,
            secret,
            made,
            private_account: private_account.trim().to_owned(),
            revealed: None,
            revealed_at: None,
        };
        commitments.put(&registry.program, &name, &saved);
        commitments.save();
        self.remember(&registry.program, &name);
        Ok(merged(sent, &json!({ "stage": "committed", "ready_at": saved.ready_at() })))
    }

    /// Remembers `name` for `program`, for `mine`.
    fn remember(&self, program: &AccountId, name: &str) {
        if let Ok(mut memory) = memory() {
            memory.remember(program, name);
            memory.save();
        }
    }
}

/// The receiving keys of the wallet's private account `choice`: an account ID,
/// or `auto` for its first private account, which is made if it has none.
fn private_keys(choice: &str) -> Result<([u8; 32], Vec<u8>), String> {
    let held = wallet::accounts()?.private;
    let account = if choice.trim() == "auto" {
        match held.first() {
            Some(account) => *account,
            None => wallet::new_private_account()?,
        }
    } else {
        let account = registry::parse_account(choice)?;
        if !held.contains(&account) {
            return Err(format!(
                "{} is not one of the open wallet's private accounts",
                registry::account_text(&account)
            ));
        }
        account
    };
    wallet::private_keys(&account)
}

/// What registering a name of `tier` takes under `settings`, for `deposit`.
fn deposit_json(name: &str, tier: u8, settings: Option<&Settings>) -> Value {
    let mut value = json!({
        "name": name,
        "display": format!("{name}{}", registry::NAME_SUFFIX),
        "tier": changes::tier_text(tier),
        "mode": Settings::mode_text(settings),
    });
    let deposit = match settings {
        _ if tier == shortlist::TIER_LOCKED => Value::Null,
        Some(settings) if settings.deposit_mode() => {
            Value::from(settings.deposits[usize::from(tier == shortlist::TIER_THREE)].to_string())
        }
        _ => Value::from("0"),
    };
    value["deposit"] = deposit;
    if let Some(settings) = settings.filter(|settings| !settings.deposit_mode()) {
        value["fee"] = Value::from(settings.prices[usize::from(changes::REGISTER_TERM)].to_string());
    }
    value
}

fn settings_json(settings: &Settings) -> Value {
    json!({
        "mode": Settings::mode_text(Some(settings)),
        "deposits": {
            "normal": settings.deposits[0].to_string(),
            "three": settings.deposits[1].to_string(),
        },
        "admin": registry::account_text(&settings.admin),
        "treasury": registry::account_text(&settings.treasury),
        "prices": {
            "30d": settings.prices[0].to_string(),
            "1y": settings.prices[1].to_string(),
            "5y": settings.prices[2].to_string(),
        },
    })
}

impl NamesModule for Names {
    fn recipient(&mut self, name: String) -> String {
        self.about(&name, |registry, name, derived| {
            let resolved = registry.resolve(name, derived)?;
            Ok(resolved.map_or(Value::Null, |resolved| resolved.recipient_json()))
        })
    }

    fn resolve(&mut self, name: String) -> String {
        self.about(&name, |registry, name, derived| {
            let Some(resolved) = registry.resolve(name, derived)? else {
                return Ok(Value::Null);
            };
            let mut value = resolved.to_json();
            value["deposit"] = Value::from(registry.deposit(&derived.id)?.to_string());
            Ok(value)
        })
    }

    fn page(&mut self, name: String) -> String {
        self.about(&name, |registry, name, derived| {
            Ok(registry.page(name, derived)?.unwrap_or(Value::Null))
        })
    }

    fn page_view(&mut self, name: String) -> String {
        self.about(&name, |registry, name, derived| {
            Ok(registry.page(name, derived)?.map_or(Value::Null, |page| page_view::view(&page)))
        })
    }

    fn page_preview(&mut self, body: String) -> String {
        let content_type = if body.trim_start().starts_with('<') { "text/html; charset=utf-8" } else { "text/plain; charset=utf-8" };
        reply(Ok(page_view::view(&json!({ "content_type": content_type, "bytes": body.len(), "text": body }))))
    }

    fn avatar(&mut self, name: String) -> String {
        self.about(&name, |registry, name, derived| {
            let resolved = registry.resolve(name, derived)?;
            let entry = resolved.as_ref().and_then(|resolved| {
                resolved.profile.texts.iter().find(|(key, _)| key == "avatar").map(|(_, value)| value.as_str())
            });
            Ok(entry.and_then(avatar::pixels).unwrap_or(Value::Null))
        })
    }

    fn reverse(&mut self, account: String) -> String {
        reply((|| {
            let account = registry::parse_account(&account)?;
            let name = self.registry()?.reverse(&account)?;
            Ok(name.map_or(Value::Null, |name| {
                json!({ "name": name, "display": format!("{name}{}", registry::NAME_SUFFIX) })
            }))
        })())
    }

    fn normalize(&mut self, input: String) -> String {
        registry::parse_name(&input).unwrap_or_default()
    }

    fn wallet(&mut self) -> String {
        reply((|| {
            let registry = self.registry()?;
            let held = self.held()?;
            // Each summary is a read over the network; they are read together.
            let mut accounts = std::thread::scope(|scope| {
                let readers: Vec<_> = held
                    .public
                    .iter()
                    .take(MAX_WALLET_ACCOUNTS)
                    .map(|account| scope.spawn(|| registry.account_summary(account)))
                    .collect();
                readers
                    .into_iter()
                    .map(|reader| reader.join().unwrap_or_else(|_| Err("a read failed".to_owned())))
                    .collect::<Result<Vec<Value>, String>>()
            })?;
            // A stable sort: funded accounts first, each group in wallet order.
            accounts.sort_by_key(|account| account["funded"] != true);
            // A private account's balance is known only to the wallet.
            let mut private = Vec::new();
            for account in held.private.iter().take(MAX_WALLET_ACCOUNTS) {
                let balance = wallet::private_balance(account).unwrap_or(0);
                private.push(json!({
                    "account": registry::account_text(account),
                    "balance": balance.to_string(),
                    "funded": balance > 0,
                }));
            }
            private.sort_by_key(|account| account["funded"] != true);
            Ok(json!({ "accounts": accounts, "private_accounts": private }))
        })())
    }

    fn mine(&mut self) -> String {
        reply((|| {
            let registry = self.registry()?;
            let held = self.held()?.public;
            let mut memory = memory()?;
            let remembered = memory.names(&registry.program);
            // Deriving is done one name at a time (it is CPU work and cached);
            // the reads that follow go out together.
            let derived: Vec<Derived> = remembered.iter().map(|(name, _)| self.derived(name)).collect();
            let reader = &registry;
            let resolved = std::thread::scope(|scope| {
                let readers: Vec<_> = remembered
                    .iter()
                    .zip(&derived)
                    .map(|((name, _), derived)| scope.spawn(move || reader.resolve(name, derived)))
                    .collect();
                readers
                    .into_iter()
                    .map(|reader| reader.join().unwrap_or_else(|_| Err("a read failed".to_owned())))
                    .collect::<Result<Vec<_>, String>>()
            })?;
            let mut commitments = pending()?;
            let mut names = Vec::new();
            let mut kept = Vec::new();
            let mut landed = Vec::new();
            for (((name, added), found), derived) in remembered.iter().cloned().zip(resolved).zip(&derived) {
                match found {
                    Some(resolved) => {
                        let holder = held.iter().find(|account| resolved.is_held_by(&derived.id, account));
                        if let Some(holder) = holder {
                            names.push(json!({
                                "name": name,
                                "display": format!("{name}{}", registry::NAME_SUFFIX),
                                "owner": registry::account_text(holder),
                                "expires": resolved.expires_json(),
                                "never": resolved.never_expires(),
                                "lapsed": resolved.lapsed(registry::local_now()),
                            }));
                            landed.push(name.clone());
                        }
                        // A name another wallet holds is kept: it may be this
                        // person's other wallet.
                        kept.push((name, added));
                    }
                    None if now().saturating_sub(added) < FORGET_UNREGISTERED_AFTER => kept.push((name, added)),
                    // A registration still under way is kept too.
                    None if commitments.get(&registry.program, &name).is_some() => kept.push((name, added)),
                    None => {}
                }
            }
            if kept.len() != remembered.len() {
                memory.set(&registry.program, &kept);
                memory.save();
            }
            // Registrations under way: a landed one is done, and an expired
            // commitment is dropped.
            let local = registry::local_now();
            let mut under_way = Vec::new();
            let mut changed = false;
            for (name, saved) in commitments.all(&registry.program) {
                if landed.contains(&name) || !saved.usable_at(local) {
                    commitments.remove(&registry.program, &name);
                    changed = true;
                    continue;
                }
                under_way.push(json!({
                    "name": name,
                    "display": format!("{name}{}", registry::NAME_SUFFIX),
                    "owner": registry::account_text(&saved.owner),
                    "stage": if saved.revealed.is_some() { "revealed" } else { "committed" },
                    "ready_at": saved.ready_at(),
                    "tx": saved.revealed,
                }));
            }
            if changed {
                commitments.save();
            }
            Ok(json!({ "names": names, "pending": under_way }))
        })())
    }

    fn keep(&mut self, name: String) -> String {
        reply((|| {
            let name = registry::parse_name(&name)?;
            let registry = self.registry()?;
            // Only names the open wallet holds are remembered, so the device
            // keeps no list of names merely looked up.
            let derived = self.derived(&name);
            let held = self.held()?.public;
            let found = registry.resolve(&name, &derived)?;
            if !found.is_some_and(|resolved| held.iter().any(|account| resolved.is_held_by(&derived.id, account))) {
                return Err(format!("{name}{} is not held by the open wallet", registry::NAME_SUFFIX));
            }
            let mut memory = memory()?;
            memory.remember(&registry.program, &name);
            memory.save();
            Ok(Value::from(name))
        })())
    }

    fn deposit(&mut self, name: String) -> String {
        self.about(&name, |registry, name, derived| {
            let tier = changes::tier_of(&derived.id)?;
            Ok(deposit_json(name, tier, registry.settings()?.as_ref()))
        })
    }

    fn settings(&mut self) -> String {
        reply((|| Ok(self.registry()?.settings()?.as_ref().map_or(Value::Null, settings_json)))())
    }

    fn register(&mut self, name: String, owner: String, private_account: String) -> String {
        reply(self.register_step(&name, &owner, &private_account))
    }

    fn receive_privately(&mut self, name: String, private_account: String) -> String {
        self.change_name(&name, |registry, name, derived, held| {
            let (npk, vpk) = private_keys(&private_account)?;
            registry.set_private_keys(name, derived, held, &npk, &vpk, true)
        })
    }

    fn set_account(&mut self, name: String, account: String) -> String {
        self.change_name(&name, |registry, name, derived, held| {
            registry.set_account(name, derived, held, &registry::parse_account(&account)?)
        })
    }

    fn set_private_keys(&mut self, name: String, nullifier_public_key: String, viewing_public_key: String) -> String {
        self.change_name(&name, |registry, name, derived, held| {
            let npk = registry::unhex(&nullifier_public_key)?;
            let vpk = registry::unhex(&viewing_public_key)?;
            registry.set_private_keys(name, derived, held, &npk, &vpk, false)
        })
    }

    fn clear_addresses(&mut self, name: String) -> String {
        self.change_name(&name, |registry, name, derived, held| registry.clear_addresses(name, derived, held))
    }

    fn set_text(&mut self, name: String, key: String, value: String) -> String {
        self.change_name(&name, |registry, name, derived, held| {
            registry.set_text(name, derived, held, key.trim(), &value)
        })
    }

    fn set_profile(&mut self, name: String, entries_json: String, replace: bool) -> String {
        // The entries are checked before anything is read from the chain.
        let entries = match parse_entries(&entries_json) {
            Ok(entries) => entries,
            Err(error) => return reply(Err(error)),
        };
        self.change_name(&name, |registry, name, derived, held| {
            registry.set_profile(name, derived, held, entries, replace)
        })
    }

    fn set_page(&mut self, name: String, content_type: String, body: String) -> String {
        self.change_name(&name, |registry, name, derived, held| {
            registry.set_page(name, derived, held, content_type.trim(), body.as_bytes())
        })
    }

    fn transfer(&mut self, name: String, new_owner: String) -> String {
        self.change_name(&name, |registry, name, derived, held| {
            registry.transfer(name, derived, held, &registry::parse_account(&new_owner)?)
        })
    }

    fn release(&mut self, name: String) -> String {
        self.change_name(&name, |registry, name, derived, held| registry.release(name, derived, held))
    }

    fn set_primary(&mut self, account: String, name: String) -> String {
        let name = if name.trim().is_empty() {
            String::new()
        } else {
            match registry::parse_name(&name) {
                Ok(name) => name,
                Err(error) => return reply(Err(error)),
            }
        };
        let derived = (!name.is_empty()).then(|| self.derived(&name));
        self.change(&name, |registry, _| {
            registry.set_primary(&registry::parse_account(&account)?, &name, derived.as_ref())
        })
    }

    fn get_config(&mut self) -> String {
        self.shown_config().to_string()
    }

    fn cli_mine(&mut self) -> String {
        reply((|| Ok(Value::from(cli::run_mine(&self.cli_setup()?)?)))())
    }

    fn cli_profile(&mut self, name: String, entries_json: String, replace: bool) -> String {
        let entries = match parse_entries(&entries_json) {
            Ok(entries) => entries,
            Err(error) => return reply(Err(error)),
        };
        // Checked before anything runs, so a bad entry fails at once.
        let made = match cli::profile_args(&name, &entries, replace) {
            Ok(made) => made,
            Err(error) => return reply(Err(error)),
        };
        self.cli_job(&name, "profile", |_, _, _| Ok((made.args, None, json!({ "skipped": made.skipped }))))
    }

    fn cli_set_page(&mut self, name: String, content_type: String, body: String) -> String {
        if let Err(error) = cli::set_page_args(&name, &content_type, Some(std::path::Path::new("check"))) {
            return reply(Err(error));
        }
        self.cli_job(&name, "set_page", |jobs, name, id| {
            if body.is_empty() {
                return Ok((cli::set_page_args(name, &content_type, None)?, None, Value::Null));
            }
            let file = jobs.scratch(id, "page");
            std::fs::write(&file, body.as_bytes()).map_err(|error| format!("cannot write the page for the CLI: {error}"))?;
            match cli::set_page_args(name, &content_type, Some(&file)) {
                Ok(command) => Ok((command, Some(file), Value::Null)),
                Err(error) => {
                    let _ = std::fs::remove_file(&file);
                    Err(error)
                }
            }
        })
    }

    fn cli_set_text(&mut self, name: String, key: String, value: String) -> String {
        let command = match cli::set_text_args(&name, &key, &value) {
            Ok(command) => command,
            Err(error) => return reply(Err(error)),
        };
        self.cli_job(&name, "set_text", |_, _, _| Ok((command, None, Value::Null)))
    }

    fn cli_balance(&mut self) -> String {
        reply((|| cli::run_balance(&self.cli_setup()?))())
    }

    fn cli_register(&mut self, name: String, public: bool) -> String {
        let command = match cli::register_args(&name, public) {
            Ok(command) => command,
            Err(error) => return reply(Err(error)),
        };
        self.cli_job(&name, "register", |_, _, _| Ok((command, None, Value::Null)))
    }

    fn board_feed(&mut self, following: bool) -> String {
        reply((|| {
            let setup = self.cli_setup()?;
            if following && self.follows.is_empty() {
                return Ok(json!({ "posts": [], "updating": false, "error": null }));
            }
            let follows = if following { self.follows.clone() } else { Vec::new() };
            let key = format!("feed:{}", follows.join(","));
            Ok(self.board_read(key, move || cli::run_feed(&setup, &follows)))
        })())
    }

    fn board_thread(&mut self, post: String) -> String {
        reply((|| {
            let setup = self.cli_setup()?;
            Ok(self.board_read(format!("thread:{post}"), move || cli::run_thread(&setup, &post)))
        })())
    }

    fn board_post(&mut self, name: String, text: String, cids_json: String, reply_to: String) -> String {
        let cids: Vec<String> = match serde_json::from_str::<Vec<String>>(if cids_json.trim().is_empty() { "[]" } else { &cids_json }) {
            Ok(cids) => cids,
            Err(_) => return reply(Err("cids must be a JSON list of CIDs".to_owned())),
        };
        let command = match cli::post_args(&name, &text, &cids, &reply_to) {
            Ok(command) => command,
            Err(error) => return reply(Err(error)),
        };
        self.cli_job(&name, "post", |_, _, _| Ok((command, None, Value::Null)))
    }

    fn board_like(&mut self, name: String, post: String, undo: bool) -> String {
        let command = match cli::like_args(&name, &post, undo) {
            Ok(command) => command,
            Err(error) => return reply(Err(error)),
        };
        self.cli_job(&name, "like", |_, _, _| Ok((command, None, Value::Null)))
    }

    fn board_hide(&mut self, post: String) -> String {
        let (name, command) = match cli::hide_args(&post) {
            Ok(made) => made,
            Err(error) => return reply(Err(error)),
        };
        self.cli_job(&name, "hide", |_, _, _| Ok((command, None, Value::Null)))
    }

    fn main_name(&mut self) -> String {
        reply(Ok(json!(self.main_name)))
    }

    fn set_main_name(&mut self, name: String) -> String {
        reply((|| {
            let name = registry::parse_name(&name)?;
            self.main_name = Some(name.clone());
            self.save();
            Ok(json!(name))
        })())
    }

    fn anon_name(&mut self) -> String {
        reply((|| {
            let setup = self.cli_setup()?;
            let saved: Value = std::fs::read_to_string(setup.wallet.join("names.json"))
                .ok()
                .and_then(|text| serde_json::from_str(&text).ok())
                .unwrap_or(Value::Null);
            Ok(saved["anon"].as_str().and_then(|name| registry::parse_name(name).ok()).map_or(Value::Null, Value::from))
        })())
    }

    fn running_jobs(&mut self) -> String {
        reply((|| Ok(Value::from(self.jobs()?.running())))())
    }

    fn make_anon(&mut self, public: bool) -> String {
        let mut command = vec!["anon".to_owned()];
        if public {
            command.push("--public".to_owned());
        }
        self.cli_job("anon", "anon", |_, _, _| Ok((command, None, Value::Null)))
    }

    fn spell_check(&mut self, text: String) -> String {
        reply(Ok(spell::check(&text)))
    }

    fn make_public(&mut self, name: String) -> String {
        let name = match registry::parse_name(&name) {
            Ok(name) => name,
            Err(error) => return reply(Err(error)),
        };
        let command = vec!["make-public".to_owned(), "--".to_owned(), name.clone()];
        self.cli_job(&name, "make_public", |_, _, _| Ok((command, None, Value::Null)))
    }

    fn dm_names(&mut self) -> String {
        reply(Ok(json!(self.dm_names)))
    }

    fn set_dm_name(&mut self, convo_id: String, name: String) -> String {
        reply((|| {
            let convo = convo_id.trim();
            if convo.is_empty() || convo.len() > 200 || !convo.bytes().all(|b| b.is_ascii_graphic()) {
                return Err("not a conversation id".to_owned());
            }
            if name.trim().is_empty() {
                self.dm_names.remove(convo);
            } else {
                self.dm_names.insert(convo.to_owned(), registry::parse_name(&name)?);
            }
            self.save();
            Ok(json!(self.dm_names))
        })())
    }

    fn board_likes(&mut self) -> String {
        reply(Ok(json!(self.likes)))
    }

    fn board_toggle_like(&mut self, name: String, post: String) -> String {
        let post = match cli::post_ref(&post) {
            Ok(post) => post,
            Err(error) => return reply(Err(error)),
        };
        let undo = self.likes.contains(&post);
        let command = match cli::like_args(&name, &post, undo) {
            Ok(command) => command,
            Err(error) => return reply(Err(error)),
        };
        let started = self.cli_job(&name, "like", |_, _, _| Ok((command, None, Value::Null)));
        if serde_json::from_str::<Value>(&started).is_ok_and(|reply| reply["success"] == true) {
            self.likes.retain(|liked| *liked != post);
            if !undo {
                self.likes.push(post);
            }
            self.save();
        }
        started
    }

    fn cli_pay(&mut self, name: String, amount: String) -> String {
        let command = match cli::pay_args(&name, &amount) {
            Ok(command) => command,
            Err(error) => return reply(Err(error)),
        };
        self.cli_job(&name, "pay", |_, _, _| Ok((command, None, Value::Null)))
    }

    fn board_follows(&mut self) -> String {
        reply(Ok(json!(self.follows)))
    }

    fn board_follow(&mut self, name: String, undo: bool) -> String {
        reply((|| {
            let name = registry::parse_name(&name)?;
            self.follows.retain(|followed| *followed != name);
            if !undo {
                self.follows.push(name);
            }
            self.save();
            Ok(json!(self.follows))
        })())
    }

    fn job(&mut self, id: String) -> String {
        reply((|| self.jobs()?.state(id.trim()))())
    }

    fn set_cli(&mut self, path: String) -> bool {
        let path = path.trim();
        if path.is_empty() {
            self.cli = None;
        } else if std::path::Path::new(path).is_absolute() && cli::is_executable(std::path::Path::new(path)) {
            self.cli = Some(path.to_owned());
        } else {
            return false;
        }
        self.save();
        true
    }

    fn set_cli_wallet(&mut self, dir: String) -> bool {
        let dir = dir.trim();
        if dir.is_empty() {
            self.cli_wallet = None;
        } else if std::path::Path::new(dir).is_absolute() && std::path::Path::new(dir).is_dir() {
            self.cli_wallet = Some(dir.to_owned());
        } else {
            return false;
        }
        self.save();
        true
    }

    fn set_registry(&mut self, program_id: String) -> bool {
        match registry::parse_account(&program_id) {
            Ok(program) => {
                self.program = Some(program);
                self.save();
                true
            }
            Err(_) => false,
        }
    }

    fn set_wallet_files(&mut self, config_path: String, storage_path: String) -> bool {
        let (config, storage) = (config_path.trim(), storage_path.trim());
        if config.is_empty() && storage.is_empty() {
            self.wallet_files = None;
        } else if std::path::Path::new(config).is_file() && std::path::Path::new(storage).is_file() {
            self.wallet_files = Some((config.to_owned(), storage.to_owned()));
        } else {
            return false;
        }
        self.save();
        true
    }

    fn set_sequencer(&mut self, url: String) -> bool {
        let url = url.trim();
        if is_http_url(url) {
            self.sequencer = url.to_owned();
            self.save();
            true
        } else {
            false
        }
    }
}

/// A wallet can hold many accounts and each costs a read, so `wallet` stops here.
const MAX_WALLET_ACCOUNTS: usize = 64;

/// The accounts in the open wallet, each kind in the wallet's own order.
#[derive(Debug, Default)]
struct Held {
    public: Vec<AccountId>,
    private: Vec<AccountId>,
}

/// The LEZ wallet module, which holds the keys and signs.
#[cfg(not(test))]
mod wallet {
    use super::{base64url, json, registry, AccountId, Change, Held, Value};
    use logos_rust_sdk::LogosModuleSDK;
    use std::time::Duration;

    const MODULE: &str = "lez_core";
    /// The native token program's ID, which is all zeros.
    const NATIVE_TOKEN: &str = "0000000000000000000000000000000000000000000000000000000000000000";
    /// What `accounts` answers when the wallet module runs with no wallet open.
    pub const CLOSED: &str = "no LEZ wallet is open; open LEZ Wallet first";
    /// Asking for the accounts is also how a missing wallet is noticed, and
    /// the caller waits for the answer, so it gives up quickly.
    const LOOK: Duration = Duration::from_secs(3);
    const SEND: Duration = Duration::from_secs(15);

    /// One call to the wallet. It is never repeated here: a send that timed
    /// out may still have gone through.
    fn call(method: &str, arguments: Value, patience: Duration) -> Result<Value, String> {
        let answer = LogosModuleSDK::new()
            .plugin(MODULE)
            .call_json_with_timeout(method, &arguments, patience)
            .map_err(|_| "the LEZ wallet is not running; install LEZ Wallet and open it".to_owned())?;
        // Some of the wallet's methods answer with JSON as text.
        Ok(match answer.as_str().map(serde_json::from_str::<Value>) {
            Some(Ok(parsed)) => parsed,
            _ => answer,
        })
    }

    /// The wallet's accounts.
    pub fn accounts() -> Result<Held, String> {
        let listed = call("list_accounts", json!([]), LOOK)?;
        let mut held = Held::default();
        for entry in listed.as_array().map(Vec::as_slice).unwrap_or_default() {
            let account = entry["account_id"]
                .as_str()
                .and_then(|hex| registry::unhex(hex).ok())
                .and_then(|bytes| AccountId::try_from(bytes).ok());
            match (account, entry["is_public"] == true) {
                (Some(account), true) => held.public.push(account),
                (Some(account), false) => held.private.push(account),
                (None, _) => {}
            }
        }
        if held.public.is_empty() && held.private.is_empty() {
            return Err(CLOSED.to_owned());
        }
        Ok(held)
    }

    /// Asks the wallet module to open the wallet kept in these files. Its
    /// statistics file sits beside its storage file.
    pub fn open(config: &str, storage: &str) -> Result<(), String> {
        let statistics = std::path::Path::new(storage).with_file_name("statistics.json");
        let opened = call("open", json!([config, storage, statistics.to_string_lossy()]), SEND)?;
        // 0 is success and 99 says a wallet is already open.
        match opened.as_i64() {
            Some(0 | 99) => Ok(()),
            _ => Err(format!("the LEZ wallet could not open {storage}")),
        }
    }

    /// The keys others use to pay the wallet's private account `account`:
    /// its nullifier public key and its viewing public key.
    pub fn private_keys(account: &AccountId) -> Result<([u8; 32], Vec<u8>), String> {
        let keys = call("get_private_account_keys", json!([registry::hex(account)]), LOOK)?;
        let read = |field: &str| keys[field].as_str().and_then(|hex| registry::unhex(hex).ok());
        let npk = read("nullifier_public_key").and_then(|bytes| <[u8; 32]>::try_from(bytes).ok());
        match (npk, read("viewing_public_key")) {
            (Some(npk), Some(vpk)) => Ok((npk, vpk)),
            _ => Err("the wallet did not give that account's receiving keys".to_owned()),
        }
    }

    /// What the wallet holds on one of its private accounts.
    pub fn private_balance(account: &AccountId) -> Result<u128, String> {
        let answer = call("get_balance", json!([registry::hex(account), false]), LOOK)?;
        answer
            .as_str()
            .and_then(|text| text.parse().ok())
            .or_else(|| answer.as_u64().map(u128::from))
            .ok_or_else(|| "the wallet did not give that account's balance".to_owned())
    }

    /// Makes a private account in the wallet, saves the wallet, and returns it.
    pub fn new_private_account() -> Result<AccountId, String> {
        let before = accounts().map(|held| held.private).unwrap_or_default();
        call("create_account_private", json!([]), SEND)?;
        let made = accounts()?
            .private
            .into_iter()
            .find(|account| !before.contains(account))
            .ok_or("the wallet did not make a private account")?;
        // Unsaved, the account and whatever is paid to it would be lost when
        // the wallet closes.
        call("save", json!([]), SEND)?;
        Ok(made)
    }

    /// Signs `change` with its signer, sends it, and returns the transaction hash.
    pub fn send(program: &AccountId, change: &Change) -> Result<String, String> {
        let program = registry::hex(program);
        let accounts: Vec<String> = change.accounts.iter().map(|account| registry::hex(account)).collect();
        let signs: Vec<bool> = change.accounts.iter().map(|account| *account == change.signer).collect();
        // Accounts are used with the registry's own shard selected, except the
        // last `balances`, which are used for their native token balance.
        let registry_rows = accounts.len() - change.balances;
        let shards: Vec<String> = (0..accounts.len())
            .map(|row| if row < registry_rows { program.clone() } else { NATIVE_TOKEN.to_owned() })
            .collect();
        let sent = call(
            "send_generic_public_transaction",
            json!([
                accounts,
                signs,
                { "_bytes": base64url(&change.instruction) },
                program,
                registry::hex(&change.signer),
                shards,
            ]),
            SEND,
        )?;
        match sent["tx_hash"].as_str() {
            Some(tx) if sent["success"] == true => Ok(tx.to_owned()),
            _ => Err(match sent["error"].as_str() {
                Some(error) if !error.is_empty() => format!("the wallet could not send it: {error}"),
                _ => "the wallet could not send it".to_owned(),
            }),
        }
    }
}

/// Stands in for the wallet in unit tests, where no module host is running.
#[cfg(test)]
mod wallet {
    use super::{AccountId, Change, Held};

    pub const CLOSED: &str = "no LEZ wallet is open; open LEZ Wallet first";

    pub fn accounts() -> Result<Held, String> {
        Err(CLOSED.to_owned())
    }

    pub fn open(_config: &str, _storage: &str) -> Result<(), String> {
        Err("the LEZ wallet is not running; install LEZ Wallet and open it".to_owned())
    }

    pub fn private_keys(_account: &AccountId) -> Result<([u8; 32], Vec<u8>), String> {
        Err(CLOSED.to_owned())
    }

    pub fn new_private_account() -> Result<AccountId, String> {
        Err(CLOSED.to_owned())
    }

    pub fn private_balance(_account: &AccountId) -> Result<u128, String> {
        Err(CLOSED.to_owned())
    }

    pub fn send(_program: &AccountId, _change: &Change) -> Result<String, String> {
        Err(CLOSED.to_owned())
    }
}

/// Unpadded URL-safe base64, the form Logos module calls carry bytes in.
fn base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut text = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let group = chunk.iter().fold(0_u32, |group, byte| (group << 8) | u32::from(*byte))
            << (8 * (3 - chunk.len()));
        for index in 0..=chunk.len() {
            text.push(ALPHABET[(group >> (18 - 6 * index)) as usize & 63] as char);
        }
    }
    text
}

#[cfg(not(test))]
#[no_mangle]
pub extern "Rust" fn logos_module_install() {
    install::<Names>();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(text: &str) -> Value {
        serde_json::from_str(text).expect("the module answers in JSON")
    }

    /// A module with the built-in defaults, ignoring any saved configuration.
    fn fresh() -> Names {
        Names {
            sequencer: DEFAULT_SEQUENCER.to_owned(),
            program: None,
            derived: HashMap::new(),
            wallet_files: None,
            cli: None,
            cli_wallet: None,
            jobs_dir: None,
            follows: Vec::new(),
            main_name: None,
            likes: Vec::new(),
            dm_names: Default::default(),
            board_reads: Default::default(),
        }
    }

    #[test]
    fn questions_fail_clearly_until_a_registry_is_configured() {
        let mut names = fresh();
        for answer in [names.recipient("jef.lez".into()), names.deposit("jef".into()), names.settings()] {
            let answer = parsed(&answer);
            assert_eq!(answer["success"], false);
            assert!(answer["error"].as_str().expect("an error").contains("set_registry"));
        }
    }

    #[test]
    fn the_default_registry_is_v5_and_a_retired_one_is_not_kept() {
        let v5 = registry::parse_account(DEFAULT_REGISTRY).expect("an account");
        let mut names = fresh();
        names.program = Some(v5);
        names.apply_saved(&json!({ "registry": RETIRED_REGISTRIES[0], "sequencer": "http://127.0.0.1:3040" }));
        assert_eq!(names.program, Some(v5));
        assert_eq!(names.sequencer, "http://127.0.0.1:3040");
        let other = registry::account_text(&[7; 32]);
        names.apply_saved(&json!({ "registry": other }));
        assert_eq!(names.program, Some([7; 32]));
    }

    #[test]
    fn a_bad_name_is_refused_before_any_network_call() {
        let mut names = fresh();
        names.program = Some([7; 32]);
        let answer = parsed(&names.resolve("jef.eth".into()));
        assert_eq!(answer["success"], false);
        assert_eq!(names.normalize("Jef.LEZ".into()), "jef");
        assert_eq!(names.normalize("jef.eth".into()), "");
    }

    #[test]
    fn a_change_to_a_bad_name_is_refused_before_any_network_call() {
        let mut names = fresh();
        names.program = Some([7; 32]);
        for answer in [
            names.register("jef.eth".into(), "x".into(), String::new()),
            names.release("-jef".into()),
            names.set_primary("x".into(), "not a name".into()),
            names.receive_privately("j f".into(), "auto".into()),
            names.set_profile("j f".into(), "{}".into(), true),
        ] {
            let answer = parsed(&answer);
            assert_eq!(answer["success"], false);
            assert!(answer["error"].as_str().expect("an error").contains("1-32 characters"));
        }
    }

    #[test]
    fn profile_entries_are_read_from_a_json_object() {
        assert_eq!(
            parse_entries(r#"{"display": "Jef", "bio": "", "cid": null}"#),
            Ok(vec![
                ("bio".to_owned(), String::new()),
                ("cid".to_owned(), String::new()),
                ("display".to_owned(), "Jef".to_owned()),
            ])
        );
        assert!(parse_entries("[1]").is_err());
        assert!(parse_entries(r#"{"bio": 3}"#).is_err());
        assert!(parse_entries("not json").is_err());
        let mut names = fresh();
        names.program = Some([7; 32]);
        let answer = parsed(&names.set_profile("jef".into(), "[]".into(), false));
        assert!(answer["error"].as_str().expect("an error").contains("JSON object"));
    }

    #[test]
    fn deposits_are_reported_by_tier_and_mode() {
        let deposits = Settings { admin: [1; 32], treasury: [2; 32], prices: [10, 20, 30], mode: registry::MODE_DEPOSIT, deposits: [100, 2_000] };
        let normal = deposit_json("logos", shortlist::TIER_NORMAL, Some(&deposits));
        assert_eq!(normal["tier"], "normal");
        assert_eq!(normal["deposit"], "100");
        assert_eq!(normal["mode"], "deposit");
        assert_eq!(normal["display"], "logos");
        assert_eq!(deposit_json("jef", shortlist::TIER_THREE, Some(&deposits))["deposit"], "2000");
        let locked = deposit_json("ab", shortlist::TIER_LOCKED, Some(&deposits));
        assert_eq!((locked["tier"].clone(), locked["deposit"].clone()), (json!("locked"), Value::Null));
        let fees = Settings { mode: registry::MODE_FEE, ..deposits.clone() };
        let fee = deposit_json("logos", shortlist::TIER_NORMAL, Some(&fees));
        assert_eq!((fee["mode"].clone(), fee["deposit"].clone(), fee["fee"].clone()), (json!("fee"), json!("0"), json!("20")));
        assert_eq!(deposit_json("logos", shortlist::TIER_NORMAL, None)["mode"], "unconfigured");

        let settings = settings_json(&deposits);
        assert_eq!(settings["mode"], "deposit");
        assert_eq!(settings["deposits"]["normal"], "100");
        assert_eq!(settings["deposits"]["three"], "2000");
        assert_eq!(settings["admin"], registry::account_text(&[1; 32]));
    }

    #[test]
    fn a_commitment_is_kept_and_its_timing_follows_the_program() {
        let saved = Commitment {
            owner: [1; 32],
            secret: [5; 32],
            made: 1_000_000,
            private_account: "auto".into(),
            revealed: Some("ab".into()),
            revealed_at: Some(2_000_000),
        };
        assert_eq!(Commitment::from_json(&saved.to_json()), Some(saved.clone()));
        assert_eq!(saved.ready_at(), 1_000_000 + 15 * 60 * 1000);
        assert!(saved.usable_at(1_000_000 + registry::MAX_COMMIT_AGE_MS - registry::TIME_WINDOW_MS - 1));
        assert!(!saved.usable_at(1_000_000 + registry::MAX_COMMIT_AGE_MS - registry::TIME_WINDOW_MS));

        let path = std::env::temp_dir().join(format!("logos-names-pending-{}-{}.json", std::process::id(), now()));
        let mut pending = Pending::load(path.clone());
        pending.put(&[7; 32], "jef", &saved);
        pending.save();
        let reloaded = Pending::load(path.clone());
        assert_eq!(reloaded.get(&[7; 32], "jef"), Some(saved));
        assert_eq!(reloaded.get(&[8; 32], "jef"), None);
        assert_eq!(reloaded.all(&[7; 32]).len(), 1);
        let mut reloaded = reloaded;
        reloaded.remove(&[7; 32], "jef");
        assert!(reloaded.all(&[7; 32]).is_empty());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn extras_are_merged_into_the_answer() {
        let value = merged(json!({ "name": "jef", "tx": "ab" }), &json!({ "refund": "100" }));
        assert_eq!(value, json!({ "name": "jef", "tx": "ab", "refund": "100" }));
        assert_eq!(merged(json!({ "name": "jef" }), &Value::Null), json!({ "name": "jef" }));
    }

    #[test]
    fn a_name_is_derived_once_and_then_remembered() {
        let mut names = fresh();
        let first = names.derived("jef");
        assert_eq!(first, registry::derive("jef"));
        assert_eq!(names.derived.len(), 1);
        assert_eq!(names.derived("jef"), first);
        assert_eq!(names.derived.len(), 1);
    }

    #[test]
    fn the_device_remembers_names_per_registry() {
        let path = std::env::temp_dir().join(format!("logos-names-test-{}-{}.json", std::process::id(), now()));
        let mut memory = Memory::load(path.clone());
        assert!(memory.names(&[7; 32]).is_empty());
        memory.remember(&[7; 32], "jef");
        memory.remember(&[7; 32], "ada");
        memory.remember(&[7; 32], "jef");
        memory.remember(&[8; 32], "linus");
        memory.save();

        let reloaded = Memory::load(path.clone());
        let names: Vec<String> = reloaded.names(&[7; 32]).into_iter().map(|(name, _)| name).collect();
        assert_eq!(names, ["jef", "ada"]);
        assert_eq!(reloaded.names(&[8; 32]).len(), 1);
        assert!(reloaded.names(&[9; 32]).is_empty());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_named_wallet_is_opened_when_none_is_open() {
        let mut names = fresh();
        names.program = Some([7; 32]);
        // With no wallet named, the answer is that none is open.
        assert_eq!(names.held().unwrap_err(), wallet::CLOSED);
        // With one named, the module tries to open it; here no wallet module runs.
        names.wallet_files = Some(("config.json".into(), "storage.json".into()));
        assert!(names.held().unwrap_err().contains("not running"));
        assert_eq!(parsed(&names.get_config())["wallet_storage"], "storage.json");
    }

    #[test]
    fn bytes_travel_as_unpadded_url_safe_base64() {
        assert_eq!(base64url(b""), "");
        assert_eq!(base64url(b"f"), "Zg");
        assert_eq!(base64url(b"fo"), "Zm8");
        assert_eq!(base64url(b"foo"), "Zm9v");
        assert_eq!(base64url(b"foob"), "Zm9vYg");
        assert_eq!(base64url(&[0xfb, 0xff, 0xfe]), "-__-");
    }

    /// A module whose CLI is a script that prints its arguments, one `[arg]`
    /// per line, then runs `tail`; with a wallet and jobs directory of its own.
    fn with_fake_cli(label: &str, tail: &str) -> (Names, std::path::PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("logos-names-module-{label}-{}-{}", std::process::id(), now()));
        let wallet = dir.join("wallet");
        std::fs::create_dir_all(&wallet).expect("a temp dir");
        std::fs::write(wallet.join(cli::STORAGE_FILE), "{}").expect("a wallet");
        let script = dir.join("names");
        std::fs::write(&script, format!("#!/bin/sh\nfor arg in \"$@\"; do printf '[%s]\\n' \"$arg\"; done\n{tail}\n")).expect("a script");
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("executable");
        let mut names = fresh();
        names.program = Some([7; 32]);
        names.cli = Some(script.to_string_lossy().into_owned());
        names.cli_wallet = Some(wallet.to_string_lossy().into_owned());
        names.jobs_dir = Some(dir.join("jobs"));
        (names, dir)
    }

    fn job_end(names: &mut Names, id: &str) -> Value {
        for _ in 0..200 {
            let state = parsed(&names.job(id.to_owned()));
            assert_eq!(state["success"], true, "{state}");
            if state["value"]["state"] != "running" {
                return state["value"].clone();
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        panic!("job {id} did not finish");
    }

    #[test]
    fn cli_changes_run_as_jobs_with_the_module_registry() {
        let (mut names, dir) = with_fake_cli("jobs", "exit 0");
        let started = parsed(&names.cli_set_text("Jef.lez".into(), "chat".into(), "say \"hi\"\nand $(id)".into()));
        assert_eq!(started["success"], true, "{started}");
        assert_eq!(started["value"]["state"], "running");
        let id = started["value"]["id"].as_str().expect("an id").to_owned();
        assert!(id.starts_with("jef-"));
        let done = job_end(&mut names, &id);
        assert_eq!(done["state"], "done");
        let program = registry::account_text(&[7; 32]);
        assert_eq!(
            done["log_tail"],
            format!("[--program={program}]\n[--rpc={DEFAULT_SEQUENCER}]\n[set-text]\n[--]\n[jef]\n[chat]\n[say \"hi\"\nand $(id)]")
        );

        let page = parsed(&names.cli_set_page("jef".into(), "text/plain; charset=utf-8".into(), "line 1\n-- line 2".into()));
        let done = job_end(&mut names, page["value"]["id"].as_str().expect("an id"));
        let tail = done["log_tail"].as_str().expect("a log");
        assert!(tail.contains("[set-page]\n[--content-type=text/plain; charset=utf-8]\n[--]\n[jef]\n["), "{tail}");
        let cleared = parsed(&names.cli_set_page("jef".into(), "text/html".into(), String::new()));
        let done = job_end(&mut names, cleared["value"]["id"].as_str().expect("an id"));
        assert!(done["log_tail"].as_str().expect("a log").ends_with("[clear-page]\n[--]\n[jef]"));

        let profile = parsed(&names.cli_profile("jef".into(), r#"{"display": "Jef", "bio": "", "avatar": "data:image/png;base64,AA", "chat": "x=y"}"#.into(), false));
        assert_eq!(profile["value"]["skipped"], json!(["avatar"]));
        let done = job_end(&mut names, profile["value"]["id"].as_str().expect("an id"));
        assert!(done["log_tail"].as_str().expect("a log").ends_with("[profile]\n[--clear=bio]\n[--set=chat=x=y]\n[--display=Jef]\n[--]\n[jef]"), "{}", done["log_tail"]);

        let config = parsed(&names.get_config());
        assert_eq!(config["cli"], names.cli.clone().expect("set"));
        assert_eq!(config["cli_wallet"], names.cli_wallet.clone().expect("set"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_cli_refusal_ends_the_job_failed_with_its_reason() {
        let (mut names, dir) = with_fake_cli("refusal", "echo 'Error: this wallet does not hold jef.lez' >&2; exit 1");
        let started = parsed(&names.cli_set_text("jef".into(), "chat".into(), "x".into()));
        let id = started["value"]["id"].as_str().expect("an id").to_owned();
        let failed = job_end(&mut names, &id);
        assert_eq!(failed["state"], "failed");
        assert_eq!(cli::last_line(failed["log_tail"].as_str().expect("a log")), "Error: this wallet does not hold jef.lez");
        assert!(parsed(&names.cli_mine())["error"].as_str().expect("an error").contains("does not hold"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn cli_changes_check_their_input_before_running_anything() {
        let (mut names, dir) = with_fake_cli("input", "exit 0");
        for answer in [
            names.cli_set_text("jef; rm -rf ~".into(), "chat".into(), "x".into()),
            names.cli_set_text("$(id)".into(), "chat".into(), "x".into()),
            names.cli_set_page("a|b".into(), "text/plain".into(), "x".into()),
            names.cli_profile("jef`id`".into(), r#"{"bio": "x"}"#.into(), false),
            names.cli_register("--owner=x".into(), false),
        ] {
            assert!(parsed(&answer)["error"].as_str().expect("an error").contains("1-32 characters"));
        }
        assert_eq!(parsed(&names.cli_set_text("jef".into(), "Bad=Key".into(), "x".into()))["success"], false);
        assert_eq!(parsed(&names.cli_profile("jef".into(), r#"{"theme": "neon"}"#.into(), false))["success"], false);
        assert_eq!(parsed(&names.cli_set_page("jef".into(), "text/html\n--x".into(), "x".into()))["success"], false);
        assert!(parsed(&names.job("../../etc/passwd".into()))["error"].as_str().expect("an error").contains("not a job id"));
        // Nothing was started.
        assert!(std::fs::read_dir(dir.join("jobs")).map_or(true, |entries| entries.count() == 0));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_empty_cli_wallet_holds_nothing_and_takes_no_changes() {
        let (mut names, dir) = with_fake_cli("empty", "echo 'jef.lez  held by a deposit (no expiry)'");
        assert_eq!(parsed(&names.cli_mine())["value"], json!([{ "name": "jef", "display": "jef", "signer": "cli", "state": "held by a deposit (no expiry)", "lapsed": false, "registering": false, "failed": false, "private": null }]));
        let empty = dir.join("empty-wallet");
        std::fs::create_dir_all(&empty).expect("a dir");
        names.cli_wallet = Some(empty.to_string_lossy().into_owned());
        assert_eq!(parsed(&names.cli_mine())["value"], json!([]));
        let refused = parsed(&names.cli_set_text("jef".into(), "chat".into(), "x".into()));
        assert!(refused["error"].as_str().expect("an error").contains("names setup"));
        // The CLI was not run, so it made no wallet there.
        assert_eq!(std::fs::read_dir(&empty).expect("the dir").count(), 0);
        assert!(!names.set_cli("relative/names".into()));
        assert!(!names.set_cli(dir.join("wallet").join(cli::STORAGE_FILE).to_string_lossy().into_owned()));
        assert!(!names.set_cli_wallet("/no/such/dir".into()));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn configuration_rejects_malformed_values() {
        let mut names = fresh();
        assert!(!names.set_registry("not-an-account".into()));
        assert!(!names.set_sequencer("ftp://example.test".into()));
        assert_eq!(parsed(&names.get_config())["sequencer"], DEFAULT_SEQUENCER);
        assert_eq!(parsed(&names.get_config())["registry"], Value::Null);
        assert!(is_http_url("http://127.0.0.1:3040"));
        assert!(!is_http_url("ftp://example.test"));
    }
}
