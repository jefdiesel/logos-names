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

    /// Use the registry with this program account ID (base58). Returns false
    /// and changes nothing if it is not an account ID.
    fn set_registry(&mut self, program_id: String) -> bool;

    /// Read from the sequencer at this URL. Returns false and changes nothing
    /// unless it is an http or https URL.
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

impl Default for Names {
    fn default() -> Self {
        Self {
            sequencer: DEFAULT_SEQUENCER.to_owned(),
            program: registry::parse_account(DEFAULT_REGISTRY).ok(),
        }
    }
}

fn reply(outcome: Result<Value, String>) -> String {
    match outcome {
        Ok(value) => json!({ "success": true, "value": value, "error": null }),
        Err(error) => json!({ "success": false, "value": null, "error": error }),
    }
    .to_string()
}

impl Names {
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
        json!({
            "sequencer": self.sequencer,
            "registry": self.program.as_ref().map(registry::account_text),
        })
        .to_string()
    }

    fn set_registry(&mut self, program_id: String) -> bool {
        match registry::parse_account(&program_id) {
            Ok(program) => {
                self.program = Some(program);
                true
            }
            Err(_) => false,
        }
    }

    fn set_sequencer(&mut self, url: String) -> bool {
        let url = url.trim();
        if url.starts_with("http://") || url.starts_with("https://") {
            self.sequencer = url.to_owned();
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

    #[test]
    fn questions_fail_clearly_until_a_registry_is_configured() {
        let mut names = Names::default();
        let answer = parsed(&names.recipient("jef.lez".into()));
        assert_eq!(answer["success"], false);
        assert!(answer["error"].as_str().expect("an error").contains("set_registry"));
    }

    #[test]
    fn a_bad_name_is_refused_before_any_network_call() {
        let mut names = Names::default();
        assert!(names.set_registry("US517G5965aydkZ46HS38QLi7UQiSojurfbQfKCELFx".into()));
        let answer = parsed(&names.resolve("jef.eth".into()));
        assert_eq!(answer["success"], false);
        assert_eq!(names.normalize("Jef.LEZ".into()), "jef");
        assert_eq!(names.normalize("jef.eth".into()), "");
    }

    #[test]
    fn configuration_rejects_malformed_values() {
        let mut names = Names::default();
        assert!(!names.set_registry("not-an-account".into()));
        assert!(!names.set_sequencer("ftp://example.test".into()));
        assert!(names.set_sequencer(" http://127.0.0.1:3040 ".into()));
        assert_eq!(parsed(&names.get_config())["sequencer"], "http://127.0.0.1:3040");
        assert_eq!(parsed(&names.get_config())["registry"], Value::Null);
    }
}
