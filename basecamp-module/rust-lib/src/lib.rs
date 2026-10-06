//! Logos Names module: lets any Logos app resolve a `.lez` name with one call.
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
//! Reading needs nothing but the sequencer. Changing a name needs a signature,
//! which this module asks the LEZ wallet module (`lez_core`) for: the keys stay
//! in the wallet, and the wallet must be open.

mod changes;
mod registry;

use changes::Change;
use registry::{AccountId, Registry};
use serde_json::{json, Value};

const DEFAULT_SEQUENCER: &str = "https://testnet.lez.logos.co";
/// The canonical registry on the default network; empty until one is deployed.
const DEFAULT_REGISTRY: &str = "";

pub trait NamesModule: Send + 'static {
    /// Where to send a payment for `name` (`jef.lez` or `jef`). The value is
    /// `{name, display, public, public_hex, private}`: `public` is an account
    /// ID or null, and `private` is `{nullifier_public_key, viewing_public_key}`
    /// in hex or null. A null value means the name is not registered.
    fn recipient(&mut self, name: String) -> String;

    /// Everything `name` resolves to: owner, account, keys, text entries and
    /// the name registered before it. A null value means it is not registered.
    fn resolve(&mut self, name: String) -> String;

    /// The page published under `name`: `{content_type, bytes, text}`, where
    /// `text` is the body if it is valid UTF-8. A null value means no page.
    /// The page is written by the name's owner: treat it as untrusted content.
    fn page(&mut self, name: String) -> String;

    /// The name `account` (base58) is shown as, or null. The answer is checked
    /// against the name's own record, so it cannot be spoofed.
    fn reverse(&mut self, account: String) -> String;

    /// Registered names, newest first: `{total, names: [{name, owner}], more}`.
    /// At most 100 per call; pass the last name returned as `before` to
    /// continue, or an empty string to start from the newest.
    fn list(&mut self, limit: u64, before: String) -> String;

    /// The bare on-chain name for what a person typed (`Jef.LEZ` gives `jef`),
    /// or an empty string if it is not a valid name. Makes no network call.
    fn normalize(&mut self, input: String) -> String;

    /// The open LEZ wallet's public accounts, funded ones first:
    /// `{accounts: [{account, balance, funded, name}]}`. `balance` is decimal
    /// text and `name` is the confirmed name the account is shown as, or null.
    /// Fails if the LEZ wallet is not installed or not open.
    fn wallet(&mut self) -> String;

    // Changing a name. Each method below checks the change against the chain,
    // has the LEZ wallet sign it with the account that must authorise it, and
    // sends it. The value is `{name, signer, tx}`. A sent transaction can
    // still be refused, so confirm by calling `resolve` until the change
    // shows; it normally takes one block.

    /// Registers `name` to `owner` (base58), an account in the open wallet.
    fn register(&mut self, name: String, owner: String) -> String;

    /// Points `name` at a public account (base58), making it a wallet alias.
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
    /// the current owner set are cleared.
    fn transfer(&mut self, name: String, new_owner: String) -> String;

    /// Gives `name` up. Its record and page are cleared and anyone may
    /// register it again.
    fn release(&mut self, name: String) -> String;

    /// Shows `account` (base58, in the open wallet) as `name`; an empty name
    /// removes that.
    fn set_primary(&mut self, account: String, name: String) -> String;

    /// The sequencer URL and registry program ID in use.
    fn get_config(&mut self) -> String;

    /// Use the registry with this program account ID (base58), and remember it
    /// across restarts. Returns false and changes nothing if it is not an
    /// account ID.
    fn set_registry(&mut self, program_id: String) -> bool;

    /// Read from the sequencer at this URL, and remember it across restarts.
    /// Returns false and changes nothing unless it is an http or https URL.
    fn set_sequencer(&mut self, url: String) -> bool;

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
    program: Option<registry::AccountId>,
}

/// Where the chosen sequencer and registry are remembered between runs.
fn config_path() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(std::path::Path::new(&home).join(".config/logos-names/config.json"))
}

impl Default for Names {
    fn default() -> Self {
        let mut names = Self {
            sequencer: DEFAULT_SEQUENCER.to_owned(),
            program: registry::parse_account(DEFAULT_REGISTRY).ok(),
        };
        // A missing or unreadable file just means the defaults stand.
        let saved = config_path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| serde_json::from_str::<Value>(&text).ok());
        if let Some(saved) = saved {
            if let Some(url) = saved["sequencer"].as_str().filter(|url| is_http_url(url)) {
                names.sequencer = url.to_owned();
            }
            if let Some(program) = saved["registry"].as_str() {
                names.program = registry::parse_account(program).ok().or(names.program);
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

impl Names {
    fn config(&self) -> Value {
        json!({
            "sequencer": self.sequencer,
            "registry": self.program.as_ref().map(registry::account_text),
        })
    }

    /// Writes the configuration for the next run. Failing to save is not an
    /// error for the caller: the setting still applies to this run.
    fn save(&self) {
        if let Some(path) = config_path() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(path, self.config().to_string());
        }
    }

    /// Has the wallet sign and send the change `build` makes. The signer must
    /// be one of the wallet's accounts; it also pays the fee.
    fn change(&self, name: &str, build: impl FnOnce(&Registry) -> Result<Change, String>) -> String {
        reply((|| {
            let registry = self.registry()?;
            let change = build(&registry)?;
            if !wallet::accounts()?.contains(&change.signer) {
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
    fn change_name(&self, typed: &str, build: impl FnOnce(&Registry, &str) -> Result<Change, String>) -> String {
        match registry::parse_name(typed) {
            Ok(name) => self.change(&name, |registry| build(registry, &name)),
            Err(error) => reply(Err(error)),
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
}

impl NamesModule for Names {
    fn recipient(&mut self, name: String) -> String {
        reply((|| {
            let name = registry::parse_name(&name)?;
            let record = self.registry()?.resolve(&name)?;
            Ok(record.map_or(Value::Null, |record| record.recipient_json()))
        })())
    }

    fn resolve(&mut self, name: String) -> String {
        reply((|| {
            let name = registry::parse_name(&name)?;
            let record = self.registry()?.resolve(&name)?;
            Ok(record.map_or(Value::Null, |record| record.to_json()))
        })())
    }

    fn page(&mut self, name: String) -> String {
        reply((|| {
            let name = registry::parse_name(&name)?;
            Ok(self.registry()?.page(&name)?.unwrap_or(Value::Null))
        })())
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

    fn list(&mut self, limit: u64, before: String) -> String {
        reply((|| {
            let before = if before.trim().is_empty() {
                None
            } else {
                Some(registry::parse_name(&before)?)
            };
            self.registry()?.list(limit, before.as_deref())
        })())
    }

    fn normalize(&mut self, input: String) -> String {
        registry::parse_name(&input).unwrap_or_default()
    }

    fn wallet(&mut self) -> String {
        reply((|| {
            let registry = self.registry()?;
            let mut accounts = Vec::new();
            for account in wallet::accounts()?.iter().take(MAX_WALLET_ACCOUNTS) {
                accounts.push(registry.account_summary(account)?);
            }
            // A stable sort: funded accounts first, each group in wallet order.
            accounts.sort_by_key(|account| account["funded"] != true);
            Ok(json!({ "accounts": accounts }))
        })())
    }

    fn register(&mut self, name: String, owner: String) -> String {
        self.change_name(&name, |registry, name| {
            registry.register(name, &registry::parse_account(&owner)?)
        })
    }

    fn set_account(&mut self, name: String, account: String) -> String {
        self.change_name(&name, |registry, name| {
            registry.set_account(name, &registry::parse_account(&account)?)
        })
    }

    fn set_private_keys(&mut self, name: String, nullifier_public_key: String, viewing_public_key: String) -> String {
        self.change_name(&name, |registry, name| {
            let npk = registry::unhex(&nullifier_public_key)?;
            let vpk = registry::unhex(&viewing_public_key)?;
            registry.set_private_keys(name, &npk, &vpk)
        })
    }

    fn clear_addresses(&mut self, name: String) -> String {
        self.change_name(&name, |registry, name| registry.clear_addresses(name))
    }

    fn set_text(&mut self, name: String, key: String, value: String) -> String {
        self.change_name(&name, |registry, name| registry.set_text(name, key.trim(), &value))
    }

    fn set_page(&mut self, name: String, content_type: String, body: String) -> String {
        self.change_name(&name, |registry, name| {
            registry.set_page(name, content_type.trim(), body.as_bytes())
        })
    }

    fn transfer(&mut self, name: String, new_owner: String) -> String {
        self.change_name(&name, |registry, name| {
            registry.transfer(name, &registry::parse_account(&new_owner)?)
        })
    }

    fn release(&mut self, name: String) -> String {
        self.change_name(&name, |registry, name| registry.release(name))
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
        self.change(&name, |registry| {
            registry.set_primary(&registry::parse_account(&account)?, &name)
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

/// The LEZ wallet module, which holds the keys and signs.
#[cfg(not(test))]
mod wallet {
    use super::{base64url, json, registry, AccountId, Change, Value};
    use logos_rust_sdk::LogosModuleSDK;
    use std::time::Duration;

    const MODULE: &str = "lez_core";
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

    /// The wallet's public accounts, in its own order.
    pub fn accounts() -> Result<Vec<AccountId>, String> {
        let listed = call("list_accounts", json!([]), LOOK)?;
        let accounts: Vec<AccountId> = listed
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .filter(|entry| entry["is_public"] == true)
            .filter_map(|entry| registry::unhex(entry["account_id"].as_str()?).ok()?.try_into().ok())
            .collect();
        if accounts.is_empty() {
            return Err("no LEZ wallet is open; open LEZ Wallet first".to_owned());
        }
        Ok(accounts)
    }

    /// Signs `change` with its signer, sends it, and returns the transaction hash.
    pub fn send(program: &AccountId, change: &Change) -> Result<String, String> {
        let program = registry::hex(program);
        let accounts: Vec<String> = change.accounts.iter().map(|account| registry::hex(account)).collect();
        let signs: Vec<bool> = change.accounts.iter().map(|account| *account == change.signer).collect();
        // Every account is used with the registry's own shard selected.
        let shards = vec![program.clone(); accounts.len()];
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
    use super::{AccountId, Change};

    pub fn accounts() -> Result<Vec<AccountId>, String> {
        Err("no LEZ wallet is open; open LEZ Wallet first".to_owned())
    }

    pub fn send(_program: &AccountId, _change: &Change) -> Result<String, String> {
        Err("no LEZ wallet is open; open LEZ Wallet first".to_owned())
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
    fn bytes_travel_as_unpadded_url_safe_base64() {
        assert_eq!(base64url(b""), "");
        assert_eq!(base64url(b"f"), "Zg");
        assert_eq!(base64url(b"fo"), "Zm8");
        assert_eq!(base64url(b"foo"), "Zm9v");
        assert_eq!(base64url(b"foob"), "Zm9vYg");
        assert_eq!(base64url(&[0xfb, 0xff, 0xfe]), "-__-");
    }

    #[test]
    fn a_change_to_a_bad_name_is_refused_before_any_network_call() {
        let mut names = fresh();
        names.program = Some([7; 32]);
        for answer in [
            names.register("jef.eth".into(), "x".into()),
            names.release("-jef".into()),
            names.set_primary("x".into(), "not a name".into()),
        ] {
            let answer = parsed(&answer);
            assert_eq!(answer["success"], false);
            assert!(answer["error"].as_str().expect("an error").contains("1-32 characters"));
        }
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
