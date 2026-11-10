//! The Wallet Provider SDK's request queue (LP-0021): what other apps ask the
//! person's wallet to do, kept until the person decides in the Names app.
//!
//! Any app may ask; only the Names app (and the host) may decide. An app is
//! identified by the name Basecamp gives its credential (`current_caller()`),
//! never by anything the app passes. A request is checked when it is made, so
//! an app learns at once that it asked for something impossible, and again
//! when it is approved.
//!
//! Requests and grants are kept in one JSON file, 0600 and written atomically
//! (`cli::write_private`), so a grant survives a restart and a half-written
//! file is never read. Everything is bounded: requests per app and in all,
//! requests per app per hour, text lengths, instruction size, accounts per
//! call, apps with access and accounts per app.
//!
//! Accounts are written as the LEZ wallet writes them: `Public/<base58>` or
//! `Private/<base58>`.

use crate::registry::{self, AccountId};
use serde_json::{json, Map, Value};
use std::path::PathBuf;

/// The native asset a transfer moves: LEZ. A transfer can also move a token
/// of the wallet's token program, named by its definition's account ID
/// (`asset_kind` `token`).
pub const ASSET: &str = "LEZ";
/// What a transfer's asset is.
pub const NATIVE: &str = "native";
pub const TOKEN: &str = "token";
/// Requests one app may have waiting at once, and all apps together.
pub const MAX_PENDING_PER_APP: usize = 4;
pub const MAX_PENDING: usize = 32;
/// Requests one app may make in an hour, decided or not, so an app can't
/// bury the person in prompts by asking again after each refusal.
pub const MAX_PER_HOUR: usize = 30;
/// Requests remembered in all; the oldest decided ones go first.
pub const MAX_KEPT: usize = 64;
/// How long a request waits for the person before it expires.
pub const PENDING_FOR_MS: u64 = 10 * 60 * 1000;
/// How long a decided request stays readable by the app that made it.
pub const KEEP_DECIDED_MS: u64 = 24 * 60 * 60 * 1000;
/// Apps with access, and accounts one app may be given.
pub const MAX_APPS: usize = 64;
pub const MAX_GRANTED: usize = 8;
/// A program call's accounts and instruction.
pub const MAX_CALL_ACCOUNTS: usize = 16;
pub const MAX_INSTRUCTION: usize = 2048;
/// A reason or summary an app writes, in characters.
pub const MAX_TEXT: usize = 280;
/// An app's name as Basecamp gives it.
pub const MAX_APP_NAME: usize = 64;
/// The placeholder for the account the person chooses, in a call's accounts.
pub const SIGNER: &str = "signer";

/// What a request can be.
pub const CONNECT: &str = "connect";
pub const TRANSFER: &str = "transfer";
pub const CALL: &str = "call";
/// A testimonial inscribed on the Logos Blockchain (LP-0022, provider_inscribe.rs).
pub const INSCRIBE: &str = "inscribe";

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| u64::try_from(since.as_millis()).unwrap_or(u64::MAX))
}

/// An app name fit to show and to key a grant by: what Basecamp names a
/// module, letters, digits, `_`, `-` and `.`.
pub fn app_name(name: &str) -> Result<String, String> {
    if name.is_empty()
        || name.len() > MAX_APP_NAME
        || !name.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
    {
        return Err("this app's name can't be shown safely, so it can't ask the wallet for anything".to_owned());
    }
    Ok(name.to_owned())
}

pub use crate::text::clean_text;

/// An account written `Public/<id>` or `Private/<id>`: whether it is private,
/// and its ID. A bare ID is refused, so public and private are never confused.
pub fn account_ref(text: &str) -> Result<(bool, AccountId), String> {
    let text = text.trim();
    let (private, id) = if let Some(id) = text.strip_prefix("Public/") {
        (false, id)
    } else if let Some(id) = text.strip_prefix("Private/") {
        (true, id)
    } else {
        return Err(format!("an account is written Public/<id> or Private/<id>, not {}", clean_text(text, 60)));
    };
    if id.contains('/') {
        return Err("not an account ID".to_owned());
    }
    Ok((private, registry::parse_account(id)?))
}

/// The canonical spelling of an account reference.
pub fn account_written(private: bool, id: &AccountId) -> String {
    format!("{}/{}", if private { "Private" } else { "Public" }, registry::account_text(id))
}

/// The asset a transfer names: LEZ, however the app wrote it, or a token of
/// the wallet's token program, by its definition's account ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asset {
    Native,
    Token(AccountId),
}

pub fn asset(text: &str) -> Result<Asset, String> {
    let trimmed = text.trim();
    match trimmed.to_ascii_lowercase().as_str() {
        "" | "lez" | "native" => return Ok(Asset::Native),
        _ => {}
    }
    match registry::parse_account(trimmed) {
        Ok(id) if id == registry::NATIVE_TOKEN => Ok(Asset::Native),
        Ok(id) => Ok(Asset::Token(id)),
        Err(_) => Err(format!(
            "the asset is LEZ (the native token) or a token's definition ID (base58); {} is neither",
            clean_text(text, 40)
        )),
    }
}

/// A new request's common fields.
fn base(app: &str, kind: &str, now: u64) -> Result<Map<String, Value>, String> {
    let id = registry::hex(&registry::fresh_bytes::<16>()?);
    let mut request = Map::new();
    request.insert("id".into(), json!(id));
    request.insert("app".into(), json!(app));
    request.insert("kind".into(), json!(kind));
    request.insert("state".into(), json!("pending"));
    request.insert("created".into(), json!(now));
    request.insert("expires".into(), json!(now + PENDING_FOR_MS));
    Ok(request)
}

/// A request to see some of the person's accounts.
pub fn connect_request(app: &str, reason: &str, now: u64) -> Result<Value, String> {
    let mut request = base(app, CONNECT, now)?;
    request.insert("reason".into(), json!(clean_text(reason, MAX_TEXT)));
    Ok(Value::Object(request))
}

/// A request to send `amount` of an asset (LEZ, or a token by its
/// definition's ID) to `destination`, a name or `Public/<id>`. Checked the
/// way the payment itself will check it. A token's name and program are
/// added by [`add_token`] once the wallet has read them.
pub fn transfer_request(app: &str, asset_text: &str, amount: &str, destination: &str, now: u64) -> Result<Value, String> {
    let asset = asset(asset_text)?;
    // The one way every amount is read (cli::amount), for LEZ and tokens alike.
    let amount = crate::cli::amount_text(amount)?;
    let (to, to_kind) = match crate::cli::pay_to(destination)? {
        crate::cli::PayTo::Name(name) => (name, "name"),
        crate::cli::PayTo::Account(id) => (format!("Public/{id}"), "account"),
    };
    let mut request = base(app, TRANSFER, now)?;
    match asset {
        Asset::Native => {
            request.insert("asset".into(), json!(ASSET));
            request.insert("asset_kind".into(), json!(NATIVE));
        }
        Asset::Token(id) => {
            request.insert("asset".into(), json!(registry::account_text(&id)));
            request.insert("asset_kind".into(), json!(TOKEN));
        }
    }
    request.insert("amount".into(), json!(amount));
    request.insert("destination".into(), json!(to));
    request.insert("destination_kind".into(), json!(to_kind));
    Ok(Value::Object(request))
}

/// The token a transfer request moves, if it moves one.
pub fn token_of(request: &Value) -> Option<AccountId> {
    if request["asset_kind"] != TOKEN {
        return None;
    }
    registry::parse_account(request["asset"].as_str()?).ok()
}

/// Adds to a token transfer what its prompt shows besides the amount and
/// destination: the token's name as its definition gives it (`token_name`),
/// the token program that moves it and what the Program Registry says about
/// that program (`token_program`, `program_label`), and the warnings that
/// follow. The name is the token maker's own text, so it is cleaned, and the
/// definition's ID is always shown with it.
pub fn add_token(request: &mut Value, name: Option<&str>, program: &AccountId, label: Option<&Value>) {
    let label = label_fields(label);
    let mut warnings = label_warnings(&label, "token program");
    if name.is_none_or(|name| name.trim().is_empty()) {
        warnings.push("The token's definition gives it no name; check its ID.".to_owned());
    }
    request["token_name"] = name.map_or(Value::Null, |name| json!(clean_text(name, 64)));
    request["token_program"] = json!(registry::account_text(program));
    request["program_label"] = label;
    request["warnings"] = json!(warnings);
}

/// What a prompt warns about a program, from what the Program Registry said
/// (`label_fields`). `what` is how the prompt names the program.
fn label_warnings(label: &Value, what: &str) -> Vec<String> {
    let mut warnings = Vec::new();
    if label["registry"] != true {
        warnings.push(format!("The Program Registry isn't installed, so who wrote this {what} and whether its source is verified is unknown."));
    } else if label["stale"] == true {
        warnings.push(format!("The {what} now runs different code than its registered source."));
    } else if label["verified"] != true {
        warnings.push(if label["kind"] == "unregistered" {
            format!("This {what} isn't in the Program Registry: its source is unknown.")
        } else {
            format!("This {what}'s source hasn't been verified on this computer.")
        });
    }
    if label["upgradeable"] == true {
        warnings.push(format!("This {what} can be upgraded, so its code may change later."));
    }
    warnings
}

/// What the `program_registry` module said about a program, kept to the
/// fields a prompt shows. None when the module isn't installed or failed.
pub fn label_fields(label: Option<&Value>) -> Value {
    let Some(label) = label.filter(|label| label.is_object()) else {
        return json!({ "registry": false, "text": null, "kind": null, "verified": null, "stale": null, "upgradeable": null });
    };
    let flag = |key: &str| label[key].as_bool();
    json!({
        "registry": true,
        "text": label["text"].as_str().map(|text| clean_text(text, 120)),
        "kind": label["kind"].as_str().map(|kind| clean_text(kind, 20)),
        "verified": flag("verified").unwrap_or(false),
        "stale": flag("stale").unwrap_or(false),
        "upgradeable": flag("upgradeable"),
    })
}

/// One account a call reaches besides the signer's own actor: who it is (an
/// ID, or `signer` for the account the person chooses), the seed when it is
/// one of the program's own accounts, and whether it is used for its LEZ
/// balance rather than for the program's data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reached {
    pub account: String,
    pub seed: Option<[u8; 32]>,
    pub balance: bool,
}

/// One entry of a call's `accounts_json`: an ID (the program's data on that
/// account), or `{"account", "seed", "balance"}`. The account the person
/// chooses is written `signer`, and only for its balance: its data for the
/// program is always the call's first actor.
fn reached(entry: &Value, program: &AccountId) -> Result<Reached, String> {
    let (account, seed, balance) = match entry {
        Value::String(text) => (Some(text.as_str()), None, false),
        Value::Object(fields) => (
            fields.get("account").and_then(Value::as_str),
            fields.get("seed").and_then(Value::as_str),
            fields.get("balance").and_then(Value::as_bool).unwrap_or(false),
        ),
        _ => return Err("each account is an ID or {\"account\", \"seed\", \"balance\"}".to_owned()),
    };
    let seed = match seed {
        Some(hex) => Some(
            registry::unhex(hex).ok().and_then(|bytes| <[u8; 32]>::try_from(bytes).ok()).ok_or("a seed is 32 bytes of hex")?,
        ),
        None => None,
    };
    let account = match (account.map(str::trim), seed) {
        (Some(SIGNER), None) if balance => SIGNER.to_owned(),
        (Some(SIGNER), _) => {
            return Err("\"signer\" is listed only for its LEZ balance ({\"account\": \"signer\", \"balance\": true}); its data for the program is always reached".to_owned())
        }
        (Some(text), seed) => {
            let bare = text.strip_prefix("Public/").unwrap_or(text);
            if bare.starts_with("Private/") {
                return Err("a program call reaches public accounts only".to_owned());
            }
            let id = registry::parse_account(bare)?;
            if seed.is_some_and(|seed| registry::pda(program, &seed) != id) {
                return Err(format!("{bare} is not the program's account for that seed"));
            }
            registry::account_text(&id)
        }
        (None, Some(seed)) => registry::account_text(&registry::pda(program, &seed)),
        (None, None) => return Err("each account object needs \"account\" or \"seed\"".to_owned()),
    };
    Ok(Reached { account, seed, balance })
}

/// A request to run `program` with the message `instruction_hex` (the bytes
/// the program's transaction carries: on LEZ 0.4, `borsh(Message::Instruction(..))`),
/// signed by the account the person chooses. Its own actor of the program
/// comes first, then the accounts in `accounts_json`. `label` is what the
/// program registry said (None without it) and `names_registry` the Names
/// registry, so a call to it can be called out. The effects are written here
/// from what the call reaches, not from what the app says it does:
/// `app_summary` is kept apart and shown as the app's own words.
#[allow(clippy::too_many_arguments)]
pub fn call_request(
    app: &str,
    program_id: &str,
    accounts_json: &str,
    instruction_hex: &str,
    summary: &str,
    label: Option<&Value>,
    names_registry: Option<&AccountId>,
    now: u64,
) -> Result<Value, String> {
    let program = registry::parse_account(program_id.trim().strip_prefix("Public/").unwrap_or(program_id.trim()))?;
    if program == registry::NATIVE_TOKEN {
        return Err("that is the native token; send LEZ with propose_transfer".to_owned());
    }
    if accounts_json.len() > MAX_CALL_ACCOUNTS * 160 {
        return Err("the accounts list is too long".to_owned());
    }
    let text = if accounts_json.trim().is_empty() { "[]" } else { accounts_json.trim() };
    let listed: Value = serde_json::from_str(text).map_err(|error| format!("the accounts are not JSON: {error}"))?;
    let listed = listed.as_array().ok_or("the accounts are a JSON array")?;
    if listed.len() > MAX_CALL_ACCOUNTS {
        return Err(format!("a call reaches at most {MAX_CALL_ACCOUNTS} accounts besides the signer's own"));
    }
    let accounts: Vec<Reached> = listed.iter().map(|entry| reached(entry, &program)).collect::<Result<_, _>>()?;
    if instruction_hex.len() > MAX_INSTRUCTION * 2 + 2 {
        return Err(format!("the message is at most {MAX_INSTRUCTION} bytes"));
    }
    let instruction = registry::unhex(instruction_hex).map_err(|_| "the message is hexadecimal bytes".to_owned())?;
    if instruction.is_empty() {
        return Err("the message is empty".to_owned());
    }
    if instruction.len() > MAX_INSTRUCTION {
        return Err(format!("the message is at most {MAX_INSTRUCTION} bytes"));
    }

    let label = label_fields(label);
    let program_text = registry::account_text(&program);
    let named = label["text"].as_str().map_or_else(|| short(&program_text), str::to_owned);
    let mut effects = vec![format!("The account you choose signs. {named} may change the data it keeps for {named} on that account.")];
    effects.push(if accounts.iter().any(|reached| reached.account == SIGNER) {
        format!("{named} may move LEZ out of the account you choose, up to all it holds.")
    } else {
        "It can't move the LEZ in the account you choose; only the fee is paid from it.".to_owned()
    });
    for reached in accounts.iter().filter(|reached| reached.account != SIGNER) {
        effects.push(match (reached.seed.is_some(), reached.balance) {
            (true, true) => format!("Uses the LEZ balance of {named}'s own account Public/{}.", reached.account),
            (true, false) => format!("Reads or writes {named}'s own account Public/{} (made if it is new).", reached.account),
            (false, true) => format!("Uses the LEZ balance of Public/{}.", reached.account),
            (false, false) => format!("Reads or writes {named}'s data on Public/{}.", reached.account),
        });
    }
    let mut warnings = Vec::new();
    if names_registry == Some(&program) {
        warnings.push("This is the Names registry: a call can change, give away or release the names this account holds.".to_owned());
    }
    warnings.extend(label_warnings(&label, "program"));

    let mut request = base(app, CALL, now)?;
    request.insert("program".into(), json!(program_text));
    request.insert("program_label".into(), label);
    request.insert(
        "accounts".into(),
        Value::from(
            accounts
                .iter()
                .map(|reached| json!({ "account": reached.account, "seed": reached.seed.map(|seed| registry::hex(&seed)), "balance": reached.balance }))
                .collect::<Vec<_>>(),
        ),
    );
    request.insert("instruction".into(), json!(registry::hex(&instruction)));
    request.insert("instruction_len".into(), json!(instruction.len()));
    request.insert("app_summary".into(), json!(clean_text(summary, MAX_TEXT)));
    request.insert("effects".into(), json!(effects));
    request.insert("warnings".into(), json!(warnings));
    // The wallet tries the call and sizes its gas only as it sends it.
    request.insert("gas".into(), Value::Null);
    Ok(Value::Object(request))
}

/// The accounts a stored call reaches, read back.
pub fn stored_reached(request: &Value) -> Result<Vec<Reached>, String> {
    request["accounts"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|entry| {
            let seed = match entry["seed"].as_str() {
                Some(hex) => Some(
                    registry::unhex(hex).ok().and_then(|bytes| <[u8; 32]>::try_from(bytes).ok()).ok_or("a stored seed is unreadable")?,
                ),
                None => None,
            };
            Ok(Reached { account: entry["account"].as_str().unwrap_or_default().to_owned(), seed, balance: entry["balance"] == true })
        })
        .collect()
}

/// `abcd…wxyz`.
pub fn short(text: &str) -> String {
    if text.len() <= 12 {
        return text.to_owned();
    }
    format!("{}…{}", &text[..6], &text[text.len() - 4..])
}

/// Whether a request has been decided: anything but waiting or running.
pub fn decided(request: &Value) -> bool {
    !matches!(request["state"].as_str(), Some("pending" | "running"))
}

/// The requests and the access each app has been given.
pub struct Store {
    path: Option<PathBuf>,
    pub requests: Vec<Value>,
    /// app → `{accounts: [account], since}`.
    pub grants: Map<String, Value>,
}

impl Store {
    /// Reads the store at `path`; anything unreadable or out of shape is dropped.
    pub fn load(path: Option<PathBuf>) -> Self {
        let saved: Value = path
            .as_ref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or(Value::Null);
        let ok_text = |value: &Value, max: usize| value.as_str().is_some_and(|text| !text.is_empty() && text.len() <= max);
        let mut requests: Vec<Value> = saved["requests"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .filter(|request| {
                ok_text(&request["id"], 64)
                    && request["app"].as_str().is_some_and(|app| app_name(app).is_ok())
                    && matches!(request["kind"].as_str(), Some(CONNECT | TRANSFER | CALL | INSCRIBE))
                    && ok_text(&request["state"], 16)
                    && request["created"].is_u64()
                    && request["expires"].is_u64()
            })
            .cloned()
            .collect();
        requests.truncate(MAX_KEPT);
        let mut grants = Map::new();
        for (app, grant) in saved["grants"].as_object().into_iter().flatten() {
            if app_name(app).is_err() || grants.len() >= MAX_APPS {
                continue;
            }
            let accounts: Vec<String> = grant["accounts"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default()
                .iter()
                .filter_map(|account| account_ref(account.as_str()?).ok())
                .map(|(private, id)| account_written(private, &id))
                .take(MAX_GRANTED)
                .collect();
            if !accounts.is_empty() {
                grants.insert(app.clone(), json!({ "accounts": accounts, "since": grant["since"].as_u64().unwrap_or(0) }));
            }
        }
        Self { path, requests, grants }
    }

    /// Writes the store, 0600, in one step. Failing to save is not an error
    /// for the caller: the store still serves this run.
    pub fn save(&self) {
        if let Some(path) = &self.path {
            let saved = json!({ "requests": self.requests, "grants": self.grants });
            let _ = crate::cli::write_private(path, saved.to_string().as_bytes());
        }
    }

    /// Expires requests nobody decided in time and forgets old decided ones.
    /// Returns whether anything changed.
    pub fn sweep(&mut self, now: u64) -> bool {
        let mut changed = false;
        for request in &mut self.requests {
            if request["state"] == "pending" && request["expires"].as_u64().is_some_and(|at| at <= now) {
                request["state"] = json!("expired");
                request["decided"] = json!(now);
                changed = true;
            }
        }
        let before = self.requests.len();
        self.requests.retain(|request| {
            !decided(request) || request["decided"].as_u64().unwrap_or(0).saturating_add(KEEP_DECIDED_MS) > now
        });
        while self.requests.len() > MAX_KEPT {
            match self.requests.iter().position(decided) {
                Some(oldest) => {
                    self.requests.remove(oldest);
                }
                None => break,
            }
        }
        changed || self.requests.len() != before
    }

    /// Adds a new request, unless the app or everyone has too many waiting.
    pub fn add(&mut self, request: Value, now: u64) -> Result<(), String> {
        let app = request["app"].as_str().unwrap_or_default().to_owned();
        self.room_for(&app, now)?;
        if self.requests.len() >= MAX_KEPT {
            if let Some(oldest) = self.requests.iter().position(decided) {
                self.requests.remove(oldest);
            }
        }
        self.requests.push(request);
        Ok(())
    }

    /// Whether `app` may make another request now: checked before anything
    /// costly is done for one, such as reading a token from the chain.
    pub fn room_for(&self, app: &str, now: u64) -> Result<(), String> {
        let waiting = |of: Option<&str>| {
            self.requests.iter().filter(|r| r["state"] == "pending" && of.is_none_or(|app| r["app"] == app)).count()
        };
        if waiting(Some(app)) >= MAX_PENDING_PER_APP {
            return Err(format!("this app already has {MAX_PENDING_PER_APP} requests waiting; wait for the person to answer them"));
        }
        if waiting(None) >= MAX_PENDING {
            return Err("the wallet has too many requests waiting; try again later".to_owned());
        }
        let recent = self
            .requests
            .iter()
            .filter(|r| r["app"] == app && r["created"].as_u64().unwrap_or(0).saturating_add(60 * 60 * 1000) > now)
            .count();
        if recent >= MAX_PER_HOUR {
            return Err(format!("this app has made {MAX_PER_HOUR} requests in the last hour; try again later"));
        }
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&Value> {
        self.requests.iter().find(|request| request["id"] == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Value> {
        self.requests.iter_mut().find(|request| request["id"] == id)
    }

    /// The accounts `app` has been given.
    pub fn granted(&self, app: &str) -> Vec<String> {
        self.grants
            .get(app)
            .and_then(|grant| grant["accounts"].as_array())
            .map(|accounts| accounts.iter().filter_map(|account| account.as_str().map(str::to_owned)).collect())
            .unwrap_or_default()
    }

    /// Gives `app` these accounts too.
    pub fn grant(&mut self, app: &str, accounts: &[String], now: u64) -> Result<Vec<String>, String> {
        if !self.grants.contains_key(app) && self.grants.len() >= MAX_APPS {
            return Err(format!("{MAX_APPS} apps already have access; take it back from one in Settings first"));
        }
        let mut all = self.granted(app);
        for account in accounts {
            if !all.contains(account) {
                all.push(account.clone());
            }
        }
        if all.len() > MAX_GRANTED {
            return Err(format!("an app can be given at most {MAX_GRANTED} accounts"));
        }
        let since = self.grants.get(app).and_then(|grant| grant["since"].as_u64()).unwrap_or(now);
        self.grants.insert(app.to_owned(), json!({ "accounts": all, "since": since }));
        Ok(all)
    }

    /// Takes back everything `app` was given. Returns whether it had anything.
    pub fn revoke(&mut self, app: &str) -> bool {
        self.grants.remove(app).is_some()
    }
}

/// A request as the app that made it sees it: what it asked, and how it
/// went. Never the job's log, which may show the person's accounts.
pub fn app_view(request: &Value) -> Value {
    let mut view = json!({
        "request": request["id"],
        "kind": request["kind"],
        "state": request["state"],
        "created": request["created"],
        "expires": request["expires"],
        "decided": request["decided"],
        "account": request["account"],
        "tx": request["tx"],
        "error": request["error"],
    });
    match request["kind"].as_str() {
        Some(CONNECT) => view["accounts"] = request["granted"].clone(),
        Some(TRANSFER) => {
            for key in ["asset", "asset_kind", "token_name", "amount", "destination"] {
                view[key] = request[key].clone();
            }
            zone_view(&mut view, request);
        }
        Some(CALL) => {
            view["program"] = request["program"].clone();
            zone_view(&mut view, request);
        }
        Some(INSCRIBE) => {
            for key in ["chain", "says", "submission", "text", "fee", "channel", "msg_id"] {
                view[key] = request[key].clone();
            }
        }
        _ => {}
    }
    view
}

/// Adds the zone a transfer or a call was asked for to the app's view of it: `zone`
/// (`home`, or the zone's channel) and `zone_channel` when it is known. Never what the
/// person calls the zone.
fn zone_view(view: &mut Value, request: &Value) {
    view["zone"] = json!(request["zone"].as_str().unwrap_or(crate::zones::HOME));
    view["zone_channel"] = request["zone_channel"].clone();
}

#[cfg(test)]
mod tests {
    use super::*;

    const ONE: &str = "CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk";
    const TWO: &str = "7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR";

    #[test]
    fn app_text_is_cleaned_before_anyone_sees_it() {
        assert_eq!(clean_text("  pay\n\tme  now ", 100), "pay me now");
        // Right-to-left override and zero-width space can't hide or reorder text.
        assert_eq!(clean_text("send 1\u{202E}000\u{200B}1", 100), "send 1 000 1");
        assert_eq!(clean_text(&"x".repeat(400), 10).chars().count(), 10);
        assert!(app_name("testimonial_ui").is_ok());
        for bad in ["", "a b", "evil\nNames", "naïve", &"a".repeat(65)] {
            assert!(app_name(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn accounts_are_written_with_their_kind() {
        assert_eq!(account_ref(&format!("Public/{ONE}")).unwrap().0, false);
        assert_eq!(account_ref(&format!("Private/{ONE}")).unwrap().0, true);
        for bad in [ONE.to_owned(), format!("Public/{ONE}/x"), "Public/nope".to_owned(), format!("public/{ONE}")] {
            assert!(account_ref(&bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_transfer_is_checked_when_it_is_asked_for() {
        let made = transfer_request("app", "lez", "1,000", "Jef.lez", 5).unwrap();
        assert_eq!((made["asset"].as_str(), made["amount"].as_str(), made["destination"].as_str()), (Some("LEZ"), Some("1000"), Some("jef")));
        assert_eq!(made["destination_kind"], "name");
        assert_eq!(made["expires"], 5 + PENDING_FOR_MS);
        assert_eq!(made["id"].as_str().unwrap().len(), 32);
        let to_account = transfer_request("app", "", "7", &format!("Public/{ONE}"), 5).unwrap();
        assert_eq!(to_account["destination"], format!("Public/{ONE}"));
        assert_eq!(made["asset_kind"], NATIVE);
        assert!(transfer_request("app", "USDC", "7", "jef", 5).unwrap_err().contains("neither"));
        for (amount, to) in [("0", "jef"), ("-5", "jef"), ("1.5", "jef"), ("7", ONE), ("7", &format!("Private/{ONE}") as &str), ("7", "$(id)")] {
            assert!(transfer_request("app", "LEZ", amount, to, 5).is_err(), "{amount} {to}");
            assert!(transfer_request("app", TWO, amount, to, 5).is_err(), "token {amount} {to}");
        }
        // The native token's own ID is LEZ, not a token.
        assert_eq!(transfer_request("app", &registry::account_text(&registry::NATIVE_TOKEN), "7", "jef", 5).unwrap()["asset"], ASSET);
    }

    #[test]
    fn a_token_transfer_names_its_token_and_program() {
        // A token is named by its definition's ID; its amounts reach 2^128 - 1.
        let max = u128::MAX.to_string();
        let mut made = transfer_request("app", &format!(" {TWO} "), &max, "jef", 5).unwrap();
        assert_eq!((made["asset"].as_str(), made["asset_kind"].as_str(), made["amount"].as_str()), (Some(TWO), Some(TOKEN), Some(max.as_str())));
        assert_eq!(token_of(&made), Some(registry::parse_account(TWO).unwrap()));
        assert!(transfer_request("app", TWO, "340282366920938463463374607431768211456", "jef", 5).is_err());
        assert_eq!(token_of(&transfer_request("app", "LEZ", "7", "jef", 5).unwrap()), None);
        // The name is the maker's text: cleaned, and a missing one is warned about.
        let program = registry::parse_account(ONE).unwrap();
        add_token(&mut made, Some("Demo\u{202E}DSU"), &program, None);
        assert_eq!(made["token_name"], "Demo DSU");
        assert_eq!(made["token_program"], ONE);
        assert_eq!(made["program_label"]["registry"], false);
        assert!(made["warnings"].to_string().contains("this token program"), "{}", made["warnings"]);
        let label = json!({ "text": "Tokens 0.1 ✓ verified", "kind": "deployer", "verified": true, "stale": false, "upgradeable": false });
        add_token(&mut made, Some("Demo"), &program, Some(&label));
        assert_eq!(made["warnings"], json!([]));
        assert_eq!(made["program_label"]["text"], "Tokens 0.1 ✓ verified");
        add_token(&mut made, None, &program, Some(&label));
        assert!(made["warnings"].to_string().contains("no name"));
        // The app sees what it asked, with the token's name.
        let view = app_view(&made);
        assert_eq!((view["asset"].as_str(), view["asset_kind"].as_str()), (Some(TWO), Some(TOKEN)));
        assert!(view.get("warnings").is_none() && view.get("program_label").is_none());
    }

    #[test]
    fn a_call_says_what_it_can_do_not_what_the_app_claims() {
        let program = registry::parse_account(ONE).unwrap();
        let seed = [9_u8; 32];
        let own = registry::account_text(&registry::pda(&program, &seed));
        let accounts = format!("[\"{TWO}\", {{\"seed\": \"{}\"}}, {{\"account\": \"signer\", \"balance\": true}}]", registry::hex(&seed));
        let made = call_request("app", ONE, &accounts, "0x000a0b", "Just a harmless \u{202E}tip", None, Some(&program), 9).unwrap();
        assert_eq!(made["instruction"], "000a0b");
        assert_eq!(made["instruction_len"], 3);
        assert_eq!(made["app_summary"], "Just a harmless tip");
        assert_eq!(made["program_label"]["registry"], false);
        assert_eq!(made["accounts"][1], json!({ "account": own, "seed": registry::hex(&seed), "balance": false }));
        let effects = made["effects"].to_string();
        assert!(effects.contains("may move LEZ out of the account you choose"), "{effects}");
        assert!(effects.contains(&format!("data on Public/{TWO}")), "{effects}");
        assert!(effects.contains(&format!("own account Public/{own} (made if it is new)")), "{effects}");
        let warnings = made["warnings"].to_string();
        assert!(warnings.contains("Names registry"), "{warnings}");
        assert!(warnings.contains("isn't installed"), "{warnings}");
        // Without the signer's balance, the call can't move its LEZ, and says so.
        let label = json!({ "text": "Tips 1.0 ✓ verified", "kind": "deployer", "verified": true, "stale": false, "upgradeable": false, "extra": "x" });
        let made = call_request("app", TWO, "[]", "01", "", Some(&label), Some(&program), 9).unwrap();
        assert_eq!(made["program_label"]["text"], "Tips 1.0 ✓ verified");
        assert_eq!(made["warnings"], json!([]));
        assert!(made["effects"][0].as_str().unwrap().contains("Tips 1.0"));
        assert!(made["effects"][1].as_str().unwrap().contains("can't move the LEZ"));
        // Unverified, stale and unregistered are each said.
        for (label, says) in [
            (json!({ "text": "x", "kind": "deployer", "verified": false }), "hasn't been verified"),
            (json!({ "text": "x", "kind": "deployer", "verified": false, "stale": true }), "different code"),
            (json!({ "text": "x", "kind": "unregistered", "verified": false }), "isn't in the Program Registry"),
        ] {
            let made = call_request("app", TWO, "", "01", "", Some(&label), None, 9).unwrap();
            assert!(made["warnings"].to_string().contains(says), "{says}");
        }
        // A stored call reads back as it was made.
        let made = call_request("app", ONE, &accounts, "01", "", None, None, 9).unwrap();
        let back = stored_reached(&made).unwrap();
        assert_eq!(back[1].seed, Some(seed));
        assert_eq!((back[2].account.as_str(), back[2].balance), (SIGNER, true));
    }

    #[test]
    fn a_bad_call_is_refused_when_it_is_asked_for() {
        let big = "00".repeat(MAX_INSTRUCTION + 1);
        let many = format!("[{}]", (0..=MAX_CALL_ACCOUNTS).map(|_| format!("\"{TWO}\"")).collect::<Vec<_>>().join(","));
        let private_one = format!("[\"Private/{TWO}\"]");
        let wrong_seed = format!("[{{\"account\": \"{TWO}\", \"seed\": \"{}\"}}]", "01".repeat(32));
        let native = registry::account_text(&registry::NATIVE_TOKEN);
        let cases: Vec<(&str, &str, &str)> = vec![
            ("nope", "[]", "01"),
            (&native, "[]", "01"),
            (ONE, &many, "01"),
            (ONE, "[\"signer\"]", "01"),
            (ONE, "[{\"account\": \"signer\"}]", "01"),
            (ONE, &private_one, "01"),
            (ONE, &wrong_seed, "01"),
            (ONE, "[{\"seed\": \"0102\"}]", "01"),
            (ONE, "[{\"balance\": true}]", "01"),
            (ONE, "{\"signer\": 1}", "01"),
            (ONE, "[]", ""),
            (ONE, "[]", "xyz"),
            (ONE, "[]", &big),
        ];
        for (program, accounts, instruction) in cases {
            assert!(call_request("app", program, accounts, instruction, "", None, None, 1).is_err(), "{program} {accounts} {}", instruction.len());
        }
    }

    #[test]
    fn requests_are_bounded_expire_and_are_forgotten() {
        let mut store = Store::load(None);
        for _ in 0..MAX_PENDING_PER_APP {
            store.add(connect_request("one", "", 0).unwrap(), 0).unwrap();
        }
        assert!(store.add(connect_request("one", "", 0).unwrap(), 0).unwrap_err().contains("waiting"));
        store.add(connect_request("two", "", 0).unwrap(), 0).unwrap();
        // Ten minutes on, they expire; a day later they're gone.
        assert!(store.sweep(PENDING_FOR_MS));
        assert!(store.requests.iter().all(|request| request["state"] == "expired"));
        store.sweep(PENDING_FOR_MS + KEEP_DECIDED_MS + 1);
        assert!(store.requests.is_empty());
        // An app that keeps asking is stopped for the hour.
        let mut store = Store::load(None);
        for at in 0..MAX_PER_HOUR as u64 {
            store.add(connect_request("pest", "", at).unwrap(), at).unwrap();
            store.requests.last_mut().unwrap()["state"] = json!("rejected");
        }
        assert!(store.add(connect_request("pest", "", 99).unwrap(), 99).unwrap_err().contains("last hour"));
        assert!(store.add(connect_request("pest", "", 2 * 60 * 60 * 1000).unwrap(), 2 * 60 * 60 * 1000).is_ok());
    }

    #[test]
    fn grants_are_bounded_and_survive_a_reload() {
        let dir = std::env::temp_dir().join(format!("logos-names-provider-{}-{}", std::process::id(), now_ms()));
        let path = dir.join("provider.json");
        let mut store = Store::load(Some(path.clone()));
        let one = format!("Public/{ONE}");
        let two = format!("Private/{TWO}");
        assert_eq!(store.grant("app", &[one.clone()], 5).unwrap(), vec![one.clone()]);
        assert_eq!(store.grant("app", &[one.clone(), two.clone()], 6).unwrap(), vec![one.clone(), two.clone()]);
        store.add(connect_request("app", "hi", 5).unwrap(), 5).unwrap();
        store.save();
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        let again = Store::load(Some(path.clone()));
        assert_eq!(again.granted("app"), vec![one.clone(), two]);
        assert_eq!(again.grants["app"]["since"], 5);
        assert_eq!(again.requests.len(), 1);
        // A hand-edited file can't smuggle in a bad app name or account.
        std::fs::write(&path, format!("{{\"grants\": {{\"evil app\": {{\"accounts\": [\"Public/{ONE}\"]}}, \"ok\": {{\"accounts\": [\"{ONE}\", \"Public/{ONE}\"]}}}}, \"requests\": [{{\"id\": \"x\"}}]}}")).unwrap();
        let edited = Store::load(Some(path));
        assert!(edited.grants.get("evil app").is_none());
        assert_eq!(edited.granted("ok"), vec![one]);
        assert!(edited.requests.is_empty());
        assert_eq!(edited.grants.len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }
}
