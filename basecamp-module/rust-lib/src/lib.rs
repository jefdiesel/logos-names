//! Logos Names module: lets any Logos app resolve and manage `.lez` names
//! with one call.
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
//! Names are private: the chain holds an identifier derived from each name and
//! a sealed profile, never the name, and there is no list of names. Whoever
//! knows a name can resolve it; nobody can browse.
//!
//! Reading needs nothing but the sequencer. Changing a name needs a signature,
//! which this module asks the LEZ wallet module (`lez_core`) for: the keys stay
//! in the wallet, and the wallet must be open.

mod changes;
mod registry;

use changes::Change;
use registry::{AccountId, Derived, Registry};
use serde_json::{json, Value};
use std::collections::HashMap;

const DEFAULT_SEQUENCER: &str = "https://testnet.lez.logos.co";
/// The canonical registry on the default network; empty until one is deployed.
const DEFAULT_REGISTRY: &str = "";

pub trait NamesModule: Send + 'static {
    /// Where to send a payment for `name` (`jef.lez` or `jef`). The value is
    /// `{name, display, public, public_hex, private}`: `private` is
    /// `{nullifier_public_key, viewing_public_key}` in hex or null, and
    /// `public` is an account ID or null. Prefer `private` when it is there.
    /// A null value means the name is not registered.
    fn recipient(&mut self, name: String) -> String;

    /// Everything `name` resolves to: owner, expiry, account, keys and text
    /// entries. `expires` is in milliseconds and `lapsed` is true once it has
    /// passed. A null value means it is not registered, which includes a name
    /// that lapsed more than 30 days ago.
    fn resolve(&mut self, name: String) -> String;

    /// The page published under `name`: `{content_type, bytes, text}`, where
    /// `text` is the body if it is valid UTF-8. A null value means no page.
    /// The page is written by the name's owner: treat it as untrusted content.
    fn page(&mut self, name: String) -> String;

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

    /// The names this device remembers that the open wallet holds:
    /// `{names: [{name, display, owner, expires, lapsed}]}`. The chain keeps no list of names,
    /// so this is the device's own memory: every name registered from here,
    /// and every name passed to `keep`.
    fn mine(&mut self) -> String;

    /// Remembers `name` on this device, so `mine` returns it while the open
    /// wallet holds it. The value is the bare name.
    fn keep(&mut self, name: String) -> String;

    // Changing a name. Each method below checks the change against the chain,
    // has the LEZ wallet sign it with the account that must authorise it, and
    // sends it. The value is `{name, signer, tx}`. A sent transaction can
    // still be refused, so confirm by calling `resolve` until the change
    // shows; it normally takes one block. Send one change per account at a
    // time.

    /// What registering or renewing a name costs: `{treasury, terms: [{term,
    /// label, days, fee}]}`. `term` is the number to pass to `register` and
    /// `renew`; `fee` is decimal text in the native token's base units, and 0
    /// while the registry has no settings.
    fn prices(&mut self) -> String;

    /// Registers `name` to `owner` (base58), an account in the open wallet,
    /// for `term`: 0 for 30 days, 1 for a year, 2 for five years. The fee for
    /// the term is paid from `owner` to the registry's treasury.
    /// `private_account` makes the name payable privately from the start: one
    /// of the wallet's private accounts (base58), `auto` for its first one
    /// (made if it has none), or empty to set nothing.
    fn register(&mut self, name: String, owner: String, private_account: String, term: u64) -> String;

    /// Extends `name` by `term`, paid by `payer` (base58), any account in the
    /// open wallet. A lapsed name can be renewed for 30 days after it expires.
    fn renew(&mut self, name: String, payer: String, term: u64) -> String;

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

    /// Publishes `body` as the page of `name`; an empty body removes the page.
    fn set_page(&mut self, name: String, content_type: String, body: String) -> String;

    /// Hands `name` to `new_owner` (base58). The addresses, entries and page
    /// the current owner set are emptied.
    fn transfer(&mut self, name: String, new_owner: String) -> String;

    /// Gives `name` up. Its record and page are emptied and anyone may
    /// register it again.
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
        };
        // A missing or unreadable file just means the defaults stand.
        let saved = config_dir()
            .and_then(|dir| std::fs::read_to_string(dir.join("config.json")).ok())
            .and_then(|text| serde_json::from_str::<Value>(&text).ok());
        if let Some(saved) = saved {
            if let Some(url) = saved["sequencer"].as_str().filter(|url| is_http_url(url)) {
                names.sequencer = url.to_owned();
            }
            if let Some(program) = saved["registry"].as_str() {
                names.program = registry::parse_account(program).ok().or(names.program);
            }
            if let (Some(config), Some(storage)) = (saved["wallet_config"].as_str(), saved["wallet_storage"].as_str()) {
                names.wallet_files = Some((config.to_owned(), storage.to_owned()));
            }
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

/// The names this device remembers, per registry. The chain keeps no list, so
/// without this a person could not be shown the names they hold.
struct Memory {
    path: std::path::PathBuf,
    registries: serde_json::Map<String, Value>,
}

/// A remembered name that is not registered is dropped after this long. Until
/// then it may be a registration that has not reached a block.
const FORGET_UNREGISTERED_AFTER: u64 = 24 * 60 * 60;

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

impl Memory {
    fn load(path: std::path::PathBuf) -> Self {
        let registries = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .and_then(|saved| saved.as_object().cloned())
            .unwrap_or_default();
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

    /// Failing to save is not an error for the caller: the memory still
    /// serves this run.
    fn save(&self) {
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&self.path, Value::Object(self.registries.clone()).to_string());
    }
}

fn memory() -> Result<Memory, String> {
    config_dir()
        .map(|dir| Memory::load(dir.join("names.json")))
        .ok_or_else(|| "this device has nowhere to remember names".to_owned())
}

impl Names {
    fn config(&self) -> Value {
        json!({
            "sequencer": self.sequencer,
            "registry": self.program.as_ref().map(registry::account_text),
            "wallet_config": self.wallet_files.as_ref().map(|(config, _)| config),
            "wallet_storage": self.wallet_files.as_ref().map(|(_, storage)| storage),
        })
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

    /// Has the wallet sign and send the change `build` makes. The signer must
    /// be one of the wallet's accounts; it also pays the fee.
    fn change(&self, name: &str, build: impl FnOnce(&Registry) -> Result<Change, String>) -> String {
        reply((|| {
            let registry = self.registry()?;
            let held = self.held()?;
            let change = build(&registry)?;
            if !held.public.contains(&change.signer) {
                return Err(format!(
                    "the open wallet does not hold {}, which must sign this",
                    registry::account_text(&change.signer)
                ));
            }
            if registry.account_summary(&change.signer)?["funded"] != true {
                return Err(format!(
                    "{} has no funds to pay the fee",
                    registry::account_text(&change.signer)
                ));
            }
            let tx = wallet::send(&registry.program, &change)?;
            Ok(json!({
                "name": name,
                "signer": registry::account_text(&change.signer),
                "tx": tx,
            }))
        })())
    }

    /// The same, for a change to a name a person typed.
    fn change_name(
        &mut self,
        typed: &str,
        build: impl FnOnce(&Registry, &str, &Derived) -> Result<Change, String>,
    ) -> String {
        match registry::parse_name(typed) {
            Ok(name) => {
                let derived = self.derived(&name);
                self.change(&name, |registry| build(registry, &name, &derived))
            }
            Err(error) => reply(Err(error)),
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

impl NamesModule for Names {
    fn recipient(&mut self, name: String) -> String {
        self.about(&name, |registry, name, derived| {
            let resolved = registry.resolve(name, derived)?;
            Ok(resolved.map_or(Value::Null, |resolved| resolved.recipient_json()))
        })
    }

    fn resolve(&mut self, name: String) -> String {
        self.about(&name, |registry, name, derived| {
            let resolved = registry.resolve(name, derived)?;
            Ok(resolved.map_or(Value::Null, |resolved| resolved.to_json()))
        })
    }

    fn page(&mut self, name: String) -> String {
        self.about(&name, |registry, name, derived| {
            Ok(registry.page(name, derived)?.unwrap_or(Value::Null))
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
            let mut accounts = Vec::new();
            for account in held.public.iter().take(MAX_WALLET_ACCOUNTS) {
                accounts.push(registry.account_summary(account)?);
            }
            // A stable sort: funded accounts first, each group in wallet order.
            accounts.sort_by_key(|account| account["funded"] != true);
            let private: Vec<String> = held.private.iter().map(registry::account_text).collect();
            Ok(json!({ "accounts": accounts, "private_accounts": private }))
        })())
    }

    fn mine(&mut self) -> String {
        reply((|| {
            let registry = self.registry()?;
            let held = self.held()?.public;
            let mut memory = memory()?;
            let remembered = memory.names(&registry.program);
            let mut names = Vec::new();
            let mut kept = Vec::new();
            for (name, added) in remembered.iter().cloned() {
                let derived = self.derived(&name);
                match registry.resolve(&name, &derived)? {
                    Some(resolved) => {
                        if held.contains(&resolved.owner) {
                            names.push(json!({
                                "name": name,
                                "display": format!("{name}{}", registry::NAME_SUFFIX),
                                "owner": registry::account_text(&resolved.owner),
                                "expires": resolved.expires,
                                "lapsed": resolved.lapsed(registry::local_now()),
                            }));
                        }
                        // A name another wallet holds is kept: it may be this
                        // person's other wallet.
                        kept.push((name, added));
                    }
                    None if now().saturating_sub(added) < FORGET_UNREGISTERED_AFTER => kept.push((name, added)),
                    None => {}
                }
            }
            if kept.len() != remembered.len() {
                memory.set(&registry.program, &kept);
                memory.save();
            }
            Ok(json!({ "names": names }))
        })())
    }

    fn keep(&mut self, name: String) -> String {
        reply((|| {
            let name = registry::parse_name(&name)?;
            let registry = self.registry()?;
            let mut memory = memory()?;
            memory.remember(&registry.program, &name);
            memory.save();
            Ok(Value::from(name))
        })())
    }

    fn prices(&mut self) -> String {
        reply((|| {
            let settings = self.registry()?.settings()?;
            let terms: Vec<Value> = registry::TERMS
                .iter()
                .enumerate()
                .map(|(term, (_, label, days))| {
                    let fee = settings.as_ref().map_or(0, |settings| settings.prices[term]);
                    json!({ "term": term, "label": label, "days": days, "fee": fee.to_string() })
                })
                .collect();
            Ok(json!({
                "treasury": settings.as_ref().map(|settings| registry::account_text(&settings.treasury)),
                "terms": terms,
            }))
        })())
    }

    fn register(&mut self, name: String, owner: String, private_account: String, term: u64) -> String {
        let sent = self.change_name(&name, |registry, name, derived| {
            let term = changes::parse_term(term)?;
            let keys = if private_account.trim().is_empty() {
                None
            } else {
                Some(private_keys(&private_account)?)
            };
            registry.register(name, derived, &registry::parse_account(&owner)?, term, keys)
        });
        // Remembered as soon as it is sent: `mine` shows it once it is on the
        // chain, and forgets it if it never gets there.
        if sent.contains("\"success\":true") {
            let _ = self.keep(name);
        }
        sent
    }

    fn renew(&mut self, name: String, payer: String, term: u64) -> String {
        self.change_name(&name, |registry, name, derived| {
            registry.renew(name, derived, &registry::parse_account(&payer)?, changes::parse_term(term)?)
        })
    }

    fn receive_privately(&mut self, name: String, private_account: String) -> String {
        self.change_name(&name, |registry, name, derived| {
            let (npk, vpk) = private_keys(&private_account)?;
            registry.set_private_keys(name, derived, &npk, &vpk, true)
        })
    }

    fn set_account(&mut self, name: String, account: String) -> String {
        self.change_name(&name, |registry, name, derived| {
            registry.set_account(name, derived, &registry::parse_account(&account)?)
        })
    }

    fn set_private_keys(&mut self, name: String, nullifier_public_key: String, viewing_public_key: String) -> String {
        self.change_name(&name, |registry, name, derived| {
            let npk = registry::unhex(&nullifier_public_key)?;
            let vpk = registry::unhex(&viewing_public_key)?;
            registry.set_private_keys(name, derived, &npk, &vpk, false)
        })
    }

    fn clear_addresses(&mut self, name: String) -> String {
        self.change_name(&name, |registry, name, derived| registry.clear_addresses(name, derived))
    }

    fn set_text(&mut self, name: String, key: String, value: String) -> String {
        self.change_name(&name, |registry, name, derived| {
            registry.set_text(name, derived, key.trim(), &value)
        })
    }

    fn set_page(&mut self, name: String, content_type: String, body: String) -> String {
        self.change_name(&name, |registry, name, derived| {
            registry.set_page(name, derived, content_type.trim(), body.as_bytes())
        })
    }

    fn transfer(&mut self, name: String, new_owner: String) -> String {
        self.change_name(&name, |registry, name, derived| {
            registry.transfer(name, derived, &registry::parse_account(&new_owner)?)
        })
    }

    fn release(&mut self, name: String) -> String {
        self.change_name(&name, |registry, name, derived| registry.release(name, derived))
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
        self.change(&name, |registry| {
            registry.set_primary(&registry::parse_account(&account)?, &name, derived.as_ref())
        })
    }

    fn get_config(&mut self) -> String {
        self.config().to_string()
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
        }
    }

    #[test]
    fn questions_fail_clearly_until_a_registry_is_configured() {
        let mut names = fresh();
        let answer = parsed(&names.recipient("jef.lez".into()));
        assert_eq!(answer["success"], false);
        assert!(answer["error"].as_str().expect("an error").contains("set_registry"));
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
            names.register("jef.eth".into(), "x".into(), String::new(), 1),
            names.renew("jef.eth".into(), "x".into(), 1),
            names.release("-jef".into()),
            names.set_primary("x".into(), "not a name".into()),
            names.receive_privately("j f".into(), "auto".into()),
        ] {
            let answer = parsed(&answer);
            assert_eq!(answer["success"], false);
            assert!(answer["error"].as_str().expect("an error").contains("1-32 characters"));
        }
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
