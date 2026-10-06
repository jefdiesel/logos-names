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

mod registry;

use registry::Registry;
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
