//! A LEZ 0.4 network entered by hand (Settings, Network, Custom network): an RPC URL and a chain id
//! the person types, checked against what the RPC says it serves, plus the network file that names
//! the rest (the LNS registry, the bridges, the NFT programs, the faucet). The file's format is
//! docs/wallet/NETWORKS.md; the one for our own LEZ 0.4 is docs/wallet/networks/<chain-id>.json.
//!
//! The file is read only when the person asks (Check, or Switch), never on a timer. It is
//! untrusted: every id in it is checked as the setters check them, its text is cleaned, its own
//! `rpc` is shown but never replaces the URL the person typed, and its `chain_id` must be the one
//! typed. Nothing here changes a setting; `Names::network_custom` does, after the checks pass.

use crate::registry::{self, AccountId, Deadline};
use serde_json::{json, Value};
use std::io::Read;
use std::time::Duration;

/// What a network file says it is (its `format`).
pub const FORMAT: &str = "names-network/1";
/// Where a chain's network file is published unless another address is given.
pub const FILE_BASE: &str = "https://nameswallet.app/networks/";
/// The most a network file may be (it lists a few ids).
pub const MAX_FILE: usize = 64 * 1024;
/// The whole check, every read together: under the host's 20 s for one call.
pub const BUDGET: Duration = Duration::from_secs(15);
/// The longest the file's own request waits, within the budget.
const FILE_WAIT: Duration = Duration::from_secs(8);
/// The longest a network's name is shown.
pub const MAX_NAME: usize = 40;

/// The parts of a network file the wallet uses, every one checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkFile {
    pub name: String,
    pub chain_id: String,
    pub rpc: Option<String>,
    pub registry: AccountId,
    pub bridge_programs: Vec<AccountId>,
    pub nft_programs: Vec<(AccountId, String)>,
    pub faucet: Option<String>,
}

impl NetworkFile {
    /// As the app shows it.
    pub fn shown(&self) -> Value {
        json!({
            "name": self.name,
            "chain_id": self.chain_id,
            "rpc": self.rpc,
            "registry": registry::account_text(&self.registry),
            "bridge_programs": self.bridge_programs.iter().map(registry::account_text).collect::<Vec<_>>(),
            "nft_programs": self.nft_programs.iter().map(|(program, label)| json!({ "program": registry::account_text(program), "label": label })).collect::<Vec<_>>(),
            "faucet": self.faucet,
        })
    }
}

/// Why the file couldn't be used. `missing` is a plain 404: for our own LEZ 0.4 the built-in ids
/// then stand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileProblem {
    pub why: String,
    pub missing: bool,
}

/// A chain id as typed: 64 hex characters (a `0x` in front allowed), in lower case.
pub fn chain_id(text: &str) -> Result<String, String> {
    let text = text.trim();
    let text = text.strip_prefix("0x").unwrap_or(text);
    if text.len() == 64 && text.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(text.to_ascii_lowercase())
    } else {
        Err("a chain id is 64 hex characters (0-9 and a-f); that one isn't. Nothing was changed.".to_owned())
    }
}

/// A chain id shortened for a sentence: `2da7f7df…5374`.
pub fn short(chain_id: &str) -> String {
    if chain_id.len() > 12 {
        format!("{}…{}", &chain_id[..8], &chain_id[chain_id.len() - 4..])
    } else {
        chain_id.to_owned()
    }
}

/// The published file of `chain_id`, when no other address is given.
pub fn default_file_url(chain_id: &str) -> String {
    format!("{FILE_BASE}{chain_id}.json")
}

/// The chain's block height now (`getLastBlockId`), within `deadline`.
pub fn height(rpc: &str, deadline: Deadline) -> Result<u64, String> {
    registry::rpc(rpc, "getLastBlockId", json!([]), deadline)
        .map_err(|why| format!("that RPC didn't give its block height, so it can't be checked. Nothing was changed. ({})", why.trim().trim_end_matches('.')))?
        .as_u64()
        .ok_or_else(|| "that RPC gave no block height, so it can't be checked. Nothing was changed.".to_owned())
}

/// Reads the network file at `url`: https (http on this computer only), no redirect followed, at
/// most `MAX_FILE`, within `deadline`.
pub fn fetch(url: &str, deadline: Deadline) -> Result<String, FileProblem> {
    let problem = |why: String| FileProblem { why, missing: false };
    if !crate::text::is_http_url(url) {
        return Err(problem("the network file's address isn't one the wallet reads: https://…, or http:// on this computer only".to_owned()));
    }
    crate::served::blocking("network::fetch");
    let wait = deadline.left().min(FILE_WAIT);
    if wait.is_zero() {
        return Err(problem("there was no time left to read the network file; try again".to_owned()));
    }
    let agent = ureq::AgentBuilder::new().redirects(0).timeout(wait).build();
    let response = match agent.get(url).call() {
        Ok(response) if response.status() == 200 => response,
        Ok(response) => return Err(problem(format!("the network file's address answered HTTP {} (moved?), not the file", response.status()))),
        Err(ureq::Error::Status(404, _)) => return Err(FileProblem { why: format!("there is no network file at {url} (HTTP 404)"), missing: true }),
        Err(ureq::Error::Status(status, _)) => return Err(problem(format!("the network file's address answered HTTP {status}"))),
        Err(error) => return Err(problem(format!("the network file couldn't be read: {}", crate::text::clean_remote(&error.to_string(), 160)))),
    };
    let mut body = Vec::new();
    response
        .into_reader()
        .take(MAX_FILE as u64 + 1)
        .read_to_end(&mut body)
        .map_err(|error| problem(format!("the network file broke off: {error}")))?;
    if body.len() > MAX_FILE {
        return Err(problem("the network file is larger than any network file".to_owned()));
    }
    String::from_utf8(body).map_err(|_| problem("the network file isn't text".to_owned()))
}

/// The network file in `text`, checked, for the chain `chain_id` (already checked with `chain_id`).
pub fn parse(text: &str, chain_id: &str) -> Result<NetworkFile, String> {
    let file: Value = serde_json::from_str(text).map_err(|_| "the network file isn't JSON".to_owned())?;
    if !file.is_object() {
        return Err("the network file isn't a JSON object".to_owned());
    }
    if file["format"] != FORMAT {
        return Err(format!("that file isn't a Names network file (its \"format\" isn't {FORMAT})"));
    }
    if file["lez"] != "0.4" {
        return Err("that network file isn't for LEZ 0.4; only LEZ 0.4 networks can be added here".to_owned());
    }
    let said = file["chain_id"].as_str().map(self::chain_id).and_then(Result::ok);
    if said.as_deref() != Some(chain_id) {
        return Err(format!("that network file is for another chain ({}), not {}", file["chain_id"].as_str().map_or_else(|| "none given".to_owned(), |id| short(&crate::text::clean_text(id, 80))), short(chain_id)));
    }
    let url = |key: &str| -> Result<Option<String>, String> {
        match &file[key] {
            Value::Null => Ok(None),
            Value::String(url) if crate::text::is_http_url(url.trim()) => Ok(Some(url.trim().to_owned())),
            _ => Err(format!("the network file's \"{key}\" isn't an https address")),
        }
    };
    let registry = file["registry"]
        .as_str()
        .and_then(|id| registry::parse_account(id.trim()).ok())
        .filter(|id| *id != registry::NATIVE_TOKEN)
        .ok_or("the network file's \"registry\" isn't a program ID")?;
    let mut bridge_programs = Vec::new();
    for entry in file["bridge_programs"].as_array().ok_or("the network file has no \"bridge_programs\" list")? {
        let program = entry.as_str().and_then(crate::bridge_id).ok_or("the network file lists a bridge that isn't a program ID")?;
        if bridge_programs.contains(&program) {
            return Err("the network file lists one bridge twice".to_owned());
        }
        bridge_programs.push(program);
    }
    if bridge_programs.len() > crate::bridge_net::MAX_BRIDGES {
        return Err(format!("the network file lists more than {} bridges", crate::bridge_net::MAX_BRIDGES));
    }
    let mut nft_programs: Vec<(AccountId, String)> = Vec::new();
    for entry in file["nft_programs"].as_array().map(Vec::as_slice).unwrap_or_default() {
        let program = entry["program"].as_str().and_then(|id| registry::parse_account(id.trim()).ok()).ok_or("the network file lists an NFT program that isn't a program ID")?;
        if nft_programs.iter().any(|(known, _)| *known == program) {
            return Err("the network file lists one NFT program twice".to_owned());
        }
        nft_programs.push((program, crate::provider::clean_text(entry["label"].as_str().unwrap_or_default(), crate::nft::MAX_LABEL)));
    }
    if nft_programs.len() > crate::nft::MAX_PROGRAMS {
        return Err(format!("the network file lists more than {} NFT programs", crate::nft::MAX_PROGRAMS));
    }
    let name = crate::text::clean_text(file["name"].as_str().unwrap_or_default(), MAX_NAME);
    Ok(NetworkFile {
        name: if name.trim().is_empty() { "Custom network".to_owned() } else { name.trim().to_owned() },
        chain_id: chain_id.to_owned(),
        rpc: url("rpc")?,
        registry,
        bridge_programs,
        nft_programs,
        faucet: url("faucet")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHAIN: &str = "2da7f7dfb6e51a28c942ea80391aa518253ec515e98d28cb4d83cc9e16ff5374";

    /// Our LEZ 0.4's file as the built-in constants make it.
    fn ours() -> String {
        json!({
            "format": FORMAT,
            "name": "Bridge test network",
            "lez": "0.4",
            "chain_id": crate::LEZ04_CHANNEL,
            "rpc": crate::LEZ04_SEQUENCER,
            "registry": crate::LEZ04_REGISTRY,
            "bridge_programs": crate::bridge_net::DEFAULT_BRIDGE_PROGRAMS,
            "nft_programs": [],
            "faucet": crate::LEZ04_SEQUENCER,
        })
        .to_string()
    }

    #[test]
    fn our_network_file_is_the_built_in_network() {
        let file = parse(&ours(), CHAIN).expect("our file checks");
        assert_eq!(file.chain_id, crate::LEZ04_CHANNEL);
        assert_eq!(file.rpc.as_deref(), Some(crate::bridge_net::RPC));
        // The published copy (docs/wallet/networks/), only in the repository: a build from
        // basecamp-module alone can't see it.
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/wallet/networks").join(format!("{CHAIN}.json"));
        if let Ok(text) = std::fs::read_to_string(&path) {
            assert_eq!(parse(&text, CHAIN).expect("the published file checks"), file, "docs/wallet/networks/{CHAIN}.json no longer names the built-in network");
            assert!(file.nft_programs.is_empty());
        }
    }

    #[test]
    fn a_chain_id_is_64_hex_characters() {
        assert_eq!(chain_id(&format!("  0x{}  ", CHAIN.to_ascii_uppercase())).unwrap(), CHAIN);
        for bad in ["", "2da7", &CHAIN[..63], &format!("{CHAIN}0"), &format!("{}g", &CHAIN[..63])] {
            assert!(chain_id(bad).is_err(), "{bad}");
        }
        assert_eq!(short(CHAIN), "2da7f7df…5374");
        assert_eq!(default_file_url(CHAIN), format!("https://nameswallet.app/networks/{CHAIN}.json"));
    }

    #[test]
    fn a_network_file_is_checked_before_anything_in_it_is_used() {
        let good: Value = serde_json::from_str(&ours()).unwrap();
        let with = |key: &str, value: Value| {
            let mut file = good.clone();
            file[key] = value;
            parse(&file.to_string(), CHAIN)
        };
        assert!(parse("not json", CHAIN).is_err());
        assert!(parse("[]", CHAIN).is_err());
        assert!(with("format", json!("names-network/2")).unwrap_err().contains("isn't a Names network file"));
        assert!(with("lez", json!("0.3")).unwrap_err().contains("isn't for LEZ 0.4"));
        assert!(with("chain_id", json!("11".repeat(32))).unwrap_err().contains("another chain (11111111…1111)"));
        assert!(with("chain_id", Value::Null).unwrap_err().contains("none given"));
        assert!(with("registry", json!("not an id")).is_err());
        assert!(with("registry", json!(registry::account_text(&registry::NATIVE_TOKEN))).is_err());
        assert!(with("bridge_programs", Value::Null).is_err());
        assert!(with("bridge_programs", json!(["JECKBkvoc2e3dC18Wm6zZJEC2BGMwUwLsdZiQzYTnUx7", "JECKBkvoc2e3dC18Wm6zZJEC2BGMwUwLsdZiQzYTnUx7"])).unwrap_err().contains("twice"));
        let seven: Vec<String> = (1..=7u8).map(|n| registry::account_text(&[n; 32])).collect();
        assert!(with("bridge_programs", json!(seven)).unwrap_err().contains("more than 6"));
        assert!(with("faucet", json!("http://evil.example/")).unwrap_err().contains("\"faucet\""));
        assert!(with("rpc", json!("ftp://x")).is_err());
        assert!(with("nft_programs", json!([{ "program": "x" }])).is_err());
        // An empty bridge list is allowed (the wallet then reads no bridge), as is no NFT list.
        assert!(with("bridge_programs", json!([])).unwrap().bridge_programs.is_empty());
        assert!(with("nft_programs", Value::Null).unwrap().nft_programs.is_empty());
        // Its text is cleaned and cut; no name is "Custom network".
        assert_eq!(with("name", json!("A\u{202e}b\nc")).unwrap().name, "A b c");
        assert_eq!(with("name", json!("x".repeat(100))).unwrap().name.chars().count(), MAX_NAME);
        assert_eq!(with("name", Value::Null).unwrap().name, "Custom network");
        let labelled = with("nft_programs", json!([{ "program": "ErycMYqXUUtducaYmpHZjRgdbLXchrMiV4NZRnmRDNmu", "label": "Name cards\u{200b}" }])).unwrap();
        assert_eq!(labelled.nft_programs.len(), 1);
        assert!(!labelled.nft_programs[0].1.contains('\u{200b}'));
    }

    #[test]
    fn the_file_is_read_plainly_and_a_404_is_told_apart() {
        let file = ours();
        let served = file.clone();
        let url = registry::stub_rpc(move |request| if request.is_null() { (200, served.clone()) } else { (400, String::new()) });
        assert_eq!(fetch(&url, Deadline::after(BUDGET)).unwrap(), file);
        let missing = registry::stub_rpc(|_| (404, String::new()));
        let problem = fetch(&missing, Deadline::after(BUDGET)).unwrap_err();
        assert!(problem.missing && problem.why.contains("HTTP 404"), "{problem:?}");
        let moved = registry::stub_rpc(|_| (500, String::new()));
        assert!(!fetch(&moved, Deadline::after(BUDGET)).unwrap_err().missing);
        let huge = registry::stub_rpc(|_| (200, "x".repeat(MAX_FILE + 10)));
        assert!(fetch(&huge, Deadline::after(BUDGET)).unwrap_err().why.contains("larger"));
        assert!(fetch("http://example.org/net.json", Deadline::after(BUDGET)).unwrap_err().why.contains("isn't one the wallet reads"));
    }
}
