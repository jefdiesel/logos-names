//! `names zone`: the LEZ zones this CLI is set up for, and which one its commands use.
//!
//! A zone is one LEZ chain, known by its channel. The rules are the Names app's own
//! (`zones.rs`, included by path from the Basecamp module): `home` is the chain `names`
//! always used, where LNS names are kept; a zone added later has its own sequencer and its
//! own wallet folder, `~/.config/logos-names/zones/<channel>/wallet`. The Names app keeps
//! the same zone's wallet in the same folder, so a zone set up in both is one wallet.
//!
//! The list here is the CLI's own, in `~/.config/logos-names/cli-zones.json`. The Names app
//! keeps its list in its own settings and chooses the zone it uses on its own, so nothing
//! typed here changes the zone its screens are showing.
//!
//! A command uses the zone in use (`names zone use`), or the one `--zone` names for that
//! command alone. `--rpc` (or `LOGOS_NAMES_RPC`) names a sequencer directly and sets the
//! zones aside, as before there were any: the Names app runs `names` that way.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};
use clap::Subcommand;
use serde_json::{json, Value};

use crate::text::is_http_url;
use crate::zones::{self, Zone, Zones};

/// The CLI's zones, in the folder the Names app keeps its settings in.
pub const FILE: &str = "cli-zones.json";
/// The wallet's file in a wallet folder.
const STORAGE_FILE: &str = "storage.json";
/// How long a sequencer is given to say its channel.
const CHANNEL_WAIT: std::time::Duration = std::time::Duration::from_secs(8);

#[derive(Subcommand, Debug, Clone)]
pub enum ZoneCommand {
    /// The zones set up here, their sequencers and wallet folders, and the one in use.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Add the zone a sequencer serves. The sequencer is asked which chain it serves (its
    /// channel); the zone gets its own wallet folder, empty until `names setup` or
    /// `names wallet restore-keys` is run on it.
    Add {
        /// The sequencer's address: https://, or http:// on this computer.
        sequencer: String,
        /// What to call the zone (default: "Zone" and the start of its channel).
        #[arg(long)]
        name: Option<String>,
    },
    /// Use a zone from now on. ZONE is `home`, a zone's name, or its channel (the first
    /// characters are enough). `--zone` does the same for one command.
    Use { zone: String },
    /// Take a zone off the list. Its wallet folder stays where it is, and adding the zone
    /// again finds it. Home, and the zone in use, can't be removed.
    Remove { zone: String },
    /// Give a zone another name.
    Rename { zone: String, name: String },
    /// Have a zone use another sequencer of its own chain: the wallet, its accounts and
    /// what they hold stay as they are. A sequencer of another chain is refused.
    Sequencer {
        zone: String,
        /// The sequencer's address: https://, or http:// on this computer.
        url: String,
        /// Home only: take the sequencer even though it serves another chain than home
        /// did. Home's wallet folder then holds what it read from the old chain.
        #[arg(long)]
        other_chain: bool,
    },
}

/// Where the list is kept: the Names app's folder.
pub fn config_dir() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").ok_or_else(|| anyhow!("HOME isn't set, so there is nowhere to keep the zones"))?;
    Ok(Path::new(&home).join(".config/logos-names"))
}

/// The saved zones. Home's sequencer is the one saved for it, else `default_rpc`.
pub fn load(dir: &Path, default_rpc: &str) -> Zones {
    let saved: Value = std::fs::read_to_string(dir.join(FILE)).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or(Value::Null);
    let sequencer = saved["sequencer"].as_str().filter(|url| is_http_url(url)).unwrap_or(default_rpc).to_owned();
    Zones::load(&saved, Zone { name: String::new(), channel: None, sequencer, wallet: None, token_program: None })
}

/// Saves the zones, readable by their owner alone. Home's sequencer is written only when
/// it was chosen: left to the default, a later `names` with another default follows it.
pub fn save(dir: &Path, zones: &Zones, default_rpc: &str) -> Result<()> {
    let mut saved = json!({ "zone": zones.current_id(), "zones": zones.json() });
    if zones.home().sequencer != default_rpc {
        saved["sequencer"] = json!(zones.home().sequencer);
    }
    private_dir(dir)?;
    let file = dir.join(FILE);
    let draft = dir.join(format!("{FILE}.{}.tmp", std::process::id()));
    write_private(&draft, saved.to_string().as_bytes()).map_err(|error| anyhow!("cannot write {}: {error}", draft.display()))?;
    std::fs::rename(&draft, &file).map_err(|error| {
        let _ = std::fs::remove_file(&draft);
        anyhow!("cannot write {}: {error}", file.display())
    })
}

/// Makes `dir` (and what leads to it) if it isn't there, the last folder its owner's alone.
fn private_dir(dir: &Path) -> Result<()> {
    if !dir.is_dir() {
        std::fs::create_dir_all(dir).map_err(|error| anyhow!("cannot make {}: {error}", dir.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
        }
    }
    Ok(())
}

fn write_private(file: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    options.open(file)?.write_all(bytes)
}

/// The zone `typed` names: `home`, a whole channel, a zone's name (in any case), or the
/// start of one zone's channel (4 characters or more).
pub fn find(zones: &Zones, typed: &str) -> Result<usize, String> {
    let typed = typed.trim();
    if let Some(at) = zones.index(typed) {
        return Ok(at);
    }
    let named = |at: &usize| zones.at(*at).is_some_and(|zone| zone.name.to_lowercase() == typed.to_lowercase());
    if let Some(at) = (0..zones.len()).find(named) {
        return Ok(at);
    }
    let start = typed.to_ascii_lowercase();
    if start.len() >= 4 && start.bytes().all(|b| b.is_ascii_hexdigit()) {
        let begins = |at: &usize| zones.at(*at).and_then(|zone| zone.channel.as_deref()).is_some_and(|channel| channel.starts_with(&start));
        let found: Vec<usize> = (1..zones.len()).filter(begins).collect();
        match found.as_slice() {
            [at] => return Ok(*at),
            [] => {}
            _ => return Err(format!("more than one zone's channel starts with {start}; give more of it, or the zone's name")),
        }
    }
    Err(format!("no zone here is called {typed}. `names zone list` shows the zones set up here."))
}

/// Where zone `at` keeps its wallet: home's is the wallet folder `names` always used
/// (`home_wallet`), an added zone's is the folder chosen for it, else its own under `dir`.
fn wallet_of(dir: &Path, zones: &Zones, at: usize, home_wallet: Option<&Path>) -> Option<PathBuf> {
    let zone = zones.at(at)?;
    if at == 0 {
        return home_wallet.map(Path::to_owned);
    }
    match (&zone.wallet, &zone.channel) {
        (Some(chosen), _) => Some(PathBuf::from(chosen)),
        (None, Some(channel)) => Some(zones::wallet_dir(dir, channel)),
        (None, None) => None,
    }
}

/// The zones as `names zone list --json` gives them (the fields the Names app's
/// `zone_list` gives).
fn view(dir: &Path, zones: &Zones, home_wallet: Option<&Path>) -> Value {
    let list: Vec<Value> = (0..zones.len())
        .filter_map(|at| {
            let zone = zones.at(at)?;
            let wallet = wallet_of(dir, zones, at, home_wallet);
            Some(json!({
                "id": zones.id(at),
                "name": zone.name,
                "channel": zone.channel,
                "sequencer": zone.sequencer,
                "home": at == 0,
                "current": at == zones.current(),
                "wallet_dir": wallet.as_ref().map(|dir| dir.to_string_lossy().into_owned()),
                "has_wallet": wallet.as_ref().is_some_and(|dir| dir.join(STORAGE_FILE).is_file()),
            }))
        })
        .collect();
    json!({ "zones": list, "current": zones.current_id(), "max": zones::MAX_ZONES })
}

/// The list in words: one zone a paragraph, the one in use marked.
fn listed(view: &Value) -> String {
    let mut out = String::new();
    for zone in view["zones"].as_array().map(Vec::as_slice).unwrap_or_default() {
        let text = |key: &str| zone[key].as_str().unwrap_or_default();
        let mark = if zone["current"] == true { "*" } else { " " };
        let called = if zone["home"] == true { "home".to_owned() } else { format!("channel {}", text("channel")) };
        out.push_str(&format!("{mark} {}  ({called})\n", text("name")));
        out.push_str(&format!("    sequencer  {}\n", text("sequencer")));
        if zone["home"] == true && zone["channel"].is_string() {
            out.push_str(&format!("    channel    {}\n", text("channel")));
        }
        let wallet = match (zone["wallet_dir"].as_str(), zone["has_wallet"] == true) {
            (Some(dir), true) => dir.to_owned(),
            (Some(dir), false) => format!("{dir}  (no wallet there yet)"),
            (None, _) => "unknown".to_owned(),
        };
        out.push_str(&format!("    wallet     {wallet}\n"));
    }
    out.push_str("* is the zone in use. LNS names are kept on home.");
    out
}

/// Asks the sequencer at `url` which chain it serves.
fn read_channel(url: &str) -> Result<String, String> {
    let body = json!({ "jsonrpc": "2.0", "id": 1, "method": "getChannelId", "params": {} });
    let text = ureq::post(url)
        .config()
        .timeout_global(Some(CHANNEL_WAIT))
        .build()
        .header("Content-Type", "application/json")
        .send(body.to_string())
        .map_err(|error| format!("it can't be reached: {error}"))?
        .body_mut()
        .read_to_string()
        .map_err(|error| format!("its answer can't be read: {error}"))?;
    let reply: Value = serde_json::from_str(&text).map_err(|_| "it didn't answer as a sequencer does".to_owned())?;
    let named = reply["result"].as_str().ok_or_else(|| "it named no channel".to_owned())?;
    zones::channel(named).ok_or_else(|| "its channel isn't 64 hex characters".to_owned())
}

/// Home's channel: the one known, else what home's sequencer says now (kept, unless an
/// added zone already has it). None while the sequencer doesn't say.
fn home_channel(zones: &mut Zones, read: &dyn Fn(&str) -> Result<String, String>) -> Option<String> {
    if let Some(channel) = &zones.home().channel {
        return Some(channel.clone());
    }
    let channel = read(&zones.home().sequencer).ok()?;
    if zones.holder(&channel, Some(0)).is_some() {
        return None;
    }
    zones.at_mut(0)?.channel = Some(channel.clone());
    Some(channel)
}

const NOT_A_SEQUENCER: &str = "a sequencer's address starts with https://, or with http:// when it is on this computer";

/// Does one `names zone` command on `zones` and says what was done. `read` asks a
/// sequencer for its channel. The caller saves the zones when this returns Ok.
fn change(
    dir: &Path,
    zones: &mut Zones,
    command: &ZoneCommand,
    home_wallet: Option<&Path>,
    read: &dyn Fn(&str) -> Result<String, String>,
) -> Result<String, String> {
    match command {
        ZoneCommand::List { json } => {
            // Home's channel is learnt along the way, when its sequencer answers.
            home_channel(zones, read);
            let view = view(dir, zones, home_wallet);
            Ok(if *json { view.to_string() } else { listed(&view) })
        }
        ZoneCommand::Add { sequencer, name } => {
            let url = sequencer.trim();
            if !is_http_url(url) {
                return Err(NOT_A_SEQUENCER.to_owned());
            }
            let name = name.as_deref().unwrap_or_default();
            if !name.trim().is_empty() {
                zones::name(name)?;
            }
            // Home's channel first: a sequencer that serves home is not another zone.
            if home_channel(zones, read).is_none() {
                return Err(format!(
                    "{}'s own sequencer didn't say which chain it serves, so a new zone can't be told apart from it. Nothing was added; try again when it answers.",
                    zones.home().name
                ));
            }
            let channel = read(url).map_err(|why| format!("that sequencer didn't say which chain it serves, so it can't be added. ({})", why.trim().trim_end_matches('.')))?;
            let at = zones.add(name, url, &channel)?;
            let wallet = wallet_of(dir, zones, at, home_wallet);
            let has_wallet = wallet.as_ref().is_some_and(|dir| dir.join(STORAGE_FILE).is_file());
            let zone = zones.at(at).ok_or("the zone wasn't added")?;
            let mut said = format!("Added {} (channel {}), served by {url}.\n", zone.name, channel);
            if let Some(wallet) = wallet {
                said.push_str(&if has_wallet {
                    format!("Its wallet is the one already in {}.\n", wallet.display())
                } else {
                    format!("Its wallet will be kept in {}: there is none yet.\n", wallet.display())
                });
            }
            let start = &channel[..8];
            said.push_str(&format!("Use it with `names zone use {start}`, or `--zone {start}` on one command."));
            Ok(said)
        }
        ZoneCommand::Use { zone } => {
            let at = find(zones, zone)?;
            zones.use_zone(at);
            let zone = zones.zone();
            let mut said = format!("Using {} from now on (sequencer {}).", zone.name, zone.sequencer);
            if at > 0 {
                let wallet = wallet_of(dir, zones, at, home_wallet);
                if !wallet.as_ref().is_some_and(|dir| dir.join(STORAGE_FILE).is_file()) {
                    said.push_str("\nThere is no wallet on this zone yet: `names setup --shield=0` makes one, or `names wallet restore-keys` brings your recovery phrase's accounts here.");
                }
                said.push_str(&format!("\nLNS names are kept on {}: add `--zone home` to a command about a name.", zones.home().name));
            }
            Ok(said)
        }
        ZoneCommand::Remove { zone } => {
            let at = find(zones, zone)?;
            let wallet = wallet_of(dir, zones, at, home_wallet);
            let removed = zones.remove(at)?;
            Ok(match wallet {
                Some(wallet) if wallet.exists() => format!("Removed {}. Its wallet folder stays in {}: adding the zone again finds it.", removed.name, wallet.display()),
                _ => format!("Removed {}.", removed.name),
            })
        }
        ZoneCommand::Rename { zone, name } => {
            let at = find(zones, zone)?;
            let was = zones.at(at).map(|zone| zone.name.clone()).unwrap_or_default();
            zones.rename(at, name)?;
            Ok(format!("{was} is now called {}.", zones.at(at).map(|zone| zone.name.as_str()).unwrap_or_default()))
        }
        ZoneCommand::Sequencer { zone, url, other_chain } => {
            let at = find(zones, zone)?;
            let url = url.trim();
            if !is_http_url(url) {
                return Err(NOT_A_SEQUENCER.to_owned());
            }
            if *other_chain && at != 0 {
                return Err("--other-chain is for home only: a sequencer of another chain is another zone (`names zone add`)".to_owned());
            }
            if zones.at(at).is_some_and(|zone| zone.sequencer == url) {
                return Ok(format!("{} already uses {url}. Nothing was changed.", zones.at(at).map(|zone| zone.name.as_str()).unwrap_or_default()));
            }
            // A home with no wallet yet has nothing read from the old chain to lose.
            let home_empty = at == 0 && !home_wallet.is_some_and(|dir| dir.join(STORAGE_FILE).is_file());
            let take_other = at == 0 && (*other_chain || home_empty);
            if at == 0 && !take_other {
                // What home is now, so the new address can be told to serve it or not.
                home_channel(zones, read);
            }
            let now = zones.at(at).cloned().ok_or("there is no such zone here")?;
            let answer = read(url);
            let holder = answer.as_ref().ok().and_then(|channel| zones.holder(channel, Some(at))).and_then(|at| zones.at(at)).map(|zone| zone.name.clone());
            let channel = zones::sequencer_change(&now.name, now.channel.as_deref(), answer.as_deref().map_err(String::as_str), holder.as_deref(), take_other).map_err(|refused| {
                if at == 0 && refused.other_chain.is_some() {
                    format!("{} To have home use it all the same, add --other-chain.", refused.why)
                } else {
                    refused.why
                }
            })?;
            let changed = zones.at_mut(at).ok_or("there is no such zone here")?;
            changed.sequencer = url.to_owned();
            changed.channel = channel.clone();
            Ok(match (channel, now.channel) {
                (Some(new), Some(old)) if new != old => format!("{} now uses {url}, which serves another chain (channel {new}).", now.name),
                (Some(_), Some(_)) => format!("{} now uses {url}, the same chain: its wallet and what it holds are as they were.", now.name),
                (Some(new), None) => format!("{} now uses {url} (channel {new}).", now.name),
                (None, _) => format!("{} now uses {url}. It didn't say which chain it serves, so that wasn't checked.", now.name),
            })
        }
    }
}

/// Runs `names zone …`.
pub fn run(command: &ZoneCommand, default_rpc: &str, home_wallet: Option<&Path>) -> Result<()> {
    let dir = config_dir()?;
    let mut zones = load(&dir, default_rpc);
    let before = zones.clone();
    let said = change(&dir, &mut zones, command, home_wallet, &read_channel).map_err(|why| anyhow!(why))?;
    if zones != before {
        save(&dir, &zones, default_rpc)?;
    }
    println!("{said}");
    Ok(())
}

/// What a command other than `names zone` uses, as its zone gives it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    /// The zone's name.
    pub name: String,
    pub home: bool,
    /// The sequencer to use. None on home when none was chosen for it: the command's own
    /// default applies, as before there were zones.
    pub sequencer: Option<String>,
    /// The wallet folder to use (an added zone's). None on home: the wallet folder `names`
    /// always used.
    pub wallet: Option<PathBuf>,
    /// What home is called, for the refusal of a command about names.
    pub home_name: String,
}

/// The zone a command runs on: the one `--zone` names (`asked`), else the one in use.
/// None when `--rpc` names a sequencer: the zones are set aside then. Both is refused.
pub fn applied(dir: &Path, default_rpc: &str, rpc_given: bool, asked: Option<&str>) -> Result<Option<Applied>> {
    if rpc_given {
        if asked.is_some() {
            bail!("give --zone or --rpc (LOGOS_NAMES_RPC), not both: a zone has its own sequencer, and `names zone sequencer` changes it");
        }
        return Ok(None);
    }
    let zones = load(dir, default_rpc);
    let at = match asked {
        Some(asked) => find(&zones, asked).map_err(|why| anyhow!(why))?,
        None => zones.current(),
    };
    let zone = zones.at(at).ok_or_else(|| anyhow!("there is no such zone here"))?;
    let home_name = zones.home().name.clone();
    if at == 0 {
        let chosen = (zone.sequencer != default_rpc).then(|| zone.sequencer.clone());
        return Ok(Some(Applied { name: zone.name.clone(), home: true, sequencer: chosen, wallet: None, home_name }));
    }
    let wallet = wallet_of(dir, &zones, at, None).ok_or_else(|| anyhow!("{} has no wallet folder", zone.name))?;
    Ok(Some(Applied { name: zone.name.clone(), home: false, sequencer: Some(zone.sequencer.clone()), wallet: Some(wallet), home_name }))
}

impl Applied {
    /// Makes an added zone's wallet folder (its owner's alone) and returns it.
    pub fn wallet_ready(&self) -> Result<Option<&Path>> {
        let Some(wallet) = &self.wallet else { return Ok(None) };
        if let Some(parent) = wallet.parent() {
            std::fs::create_dir_all(parent).map_err(|error| anyhow!("cannot make {}: {error}", parent.display()))?;
        }
        private_dir(wallet)?;
        Ok(Some(wallet))
    }

    /// Why a command about names can't run on this zone.
    pub fn names_elsewhere(&self) -> String {
        format!(
            "names are kept on {}, and this command would run on {}. Add `--zone home` to it, or switch with `names zone use home`. Nothing was started.",
            self.home_name, self.name
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RPC: &str = "https://testnet.lez.logos.co";
    const A: &str = "0101010101010101010101010101010101010101010101010101010101010101";
    const B: &str = "b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2";
    const C: &str = "b2c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3";

    fn dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("names-zone-test-{}-{label}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A stand-in for the network: each address serves one chain, or doesn't answer.
    fn chains(url: &str) -> Result<String, String> {
        match url {
            RPC | "https://home.two" => Ok(A.to_owned()),
            "http://127.0.0.1:3931" | "http://127.0.0.1:3932" => Ok(B.to_owned()),
            "http://127.0.0.1:3941" => Ok(C.to_owned()),
            _ => Err("it can't be reached".to_owned()),
        }
    }

    fn does(dir: &Path, zones: &mut Zones, command: ZoneCommand) -> Result<String, String> {
        change(dir, zones, &command, Some(&dir.join("home-wallet")), &chains)
    }

    #[test]
    fn zones_are_added_used_changed_and_removed_and_kept() {
        let dir = dir("whole");
        let mut zones = load(&dir, RPC);
        assert_eq!((zones.len(), zones.current_id().as_str(), zones.home().sequencer.as_str()), (1, "home", RPC));

        // An address that isn't a sequencer's, one that doesn't answer, and home's own chain.
        assert_eq!(does(&dir, &mut zones, ZoneCommand::Add { sequencer: "http://example.com".into(), name: None }).unwrap_err(), NOT_A_SEQUENCER);
        assert!(does(&dir, &mut zones, ZoneCommand::Add { sequencer: "http://127.0.0.1:9".into(), name: None }).unwrap_err().contains("didn't say which chain"));
        assert!(does(&dir, &mut zones, ZoneCommand::Add { sequencer: "https://home.two".into(), name: None }).unwrap_err().contains("a zone already set up here"));
        assert_eq!(zones.len(), 1);

        let said = does(&dir, &mut zones, ZoneCommand::Add { sequencer: "http://127.0.0.1:3931".into(), name: Some("Second".into()) }).unwrap();
        assert!(said.starts_with(&format!("Added Second (channel {B}), served by http://127.0.0.1:3931.")), "{said}");
        assert!(said.contains(&format!("zones/{B}/wallet: there is none yet")), "{said}");
        assert_eq!(zones.home().channel.as_deref(), Some(A), "home's channel was learnt first");
        assert_eq!(zones.current(), 0, "adding a zone doesn't switch to it");

        // Found by name in any case, by its channel, and by the start of it.
        for typed in ["second", "SECOND", B, "b2b2", " b2b2b2b2 "] {
            assert_eq!(find(&zones, typed), Ok(1), "{typed}");
        }
        assert_eq!(find(&zones, "home"), Ok(0));
        assert!(find(&zones, "b2").unwrap_err().starts_with("no zone here is called b2"));
        does(&dir, &mut zones, ZoneCommand::Add { sequencer: "http://127.0.0.1:3941".into(), name: None }).unwrap();
        assert!(find(&zones, "b2b2").is_ok() && find(&zones, "b2c3").is_ok());

        let said = does(&dir, &mut zones, ZoneCommand::Use { zone: "second".into() }).unwrap();
        assert!(said.starts_with("Using Second from now on (sequencer http://127.0.0.1:3931)."), "{said}");
        assert!(said.contains("no wallet on this zone yet") && said.contains("--zone home"), "{said}");
        save(&dir, &zones, RPC).unwrap();

        // Kept: the list, the zone in use; the file is the owner's alone and names no
        // sequencer for home, which was left to the default.
        let kept = load(&dir, RPC);
        assert_eq!(kept, zones);
        assert_eq!(kept.current_id(), B);
        let saved: Value = serde_json::from_str(&std::fs::read_to_string(dir.join(FILE)).unwrap()).unwrap();
        assert!(saved.get("sequencer").is_none(), "{saved}");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(std::fs::metadata(dir.join(FILE)).unwrap().permissions().mode() & 0o777, 0o600);
        }

        // Another sequencer of the same chain keeps the zone; one of another chain is refused.
        let said = does(&dir, &mut zones, ZoneCommand::Sequencer { zone: "second".into(), url: "http://127.0.0.1:3932".into(), other_chain: false }).unwrap();
        assert!(said.contains("the same chain"), "{said}");
        assert_eq!((zones.at(1).unwrap().sequencer.as_str(), zones.at(1).unwrap().channel.as_deref()), ("http://127.0.0.1:3932", Some(B)));
        let refused = does(&dir, &mut zones, ZoneCommand::Sequencer { zone: "second".into(), url: "http://127.0.0.1:3941".into(), other_chain: false }).unwrap_err();
        assert!(refused.contains("a zone already set up here"), "{refused}");
        let refused = does(&dir, &mut zones, ZoneCommand::Sequencer { zone: "second".into(), url: "http://127.0.0.1:9".into(), other_chain: false }).unwrap_err();
        assert!(refused.contains("didn't say which chain it serves") && refused.contains("Nothing was changed"), "{refused}");
        assert!(does(&dir, &mut zones, ZoneCommand::Sequencer { zone: "second".into(), url: "http://127.0.0.1:3932".into(), other_chain: true }).unwrap_err().starts_with("--other-chain is for home only"));
        assert_eq!(zones.at(1).unwrap().sequencer, "http://127.0.0.1:3932");

        // The zone in use and home stay; another goes, and its wallet folder is left.
        assert!(does(&dir, &mut zones, ZoneCommand::Remove { zone: "second".into() }).unwrap_err().contains("is the zone in use"));
        assert!(does(&dir, &mut zones, ZoneCommand::Remove { zone: "home".into() }).unwrap_err().contains("can't be removed"));
        does(&dir, &mut zones, ZoneCommand::Rename { zone: "b2c3".into(), name: "Third".into() }).unwrap();
        assert!(does(&dir, &mut zones, ZoneCommand::Rename { zone: "third".into(), name: "second".into() }).unwrap_err().contains("already called"));
        std::fs::create_dir_all(zones::wallet_dir(&dir, C)).unwrap();
        let said = does(&dir, &mut zones, ZoneCommand::Remove { zone: "Third".into() }).unwrap();
        assert!(said.starts_with("Removed Third. Its wallet folder stays in"), "{said}");
        assert!(zones::wallet_dir(&dir, C).is_dir());
        assert_eq!((zones.len(), zones.current_id().as_str()), (2, B));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn homes_sequencer_is_kept_once_chosen_and_another_chain_needs_saying() {
        let dir = dir("home");
        let mut zones = load(&dir, RPC);
        // Home has a wallet: another chain is refused until the person says so.
        std::fs::create_dir_all(dir.join("home-wallet")).unwrap();
        std::fs::write(dir.join("home-wallet").join(STORAGE_FILE), "{}").unwrap();
        let said = does(&dir, &mut zones, ZoneCommand::Sequencer { zone: "home".into(), url: "https://home.two".into(), other_chain: false }).unwrap();
        assert!(said.contains("the same chain"), "{said}");
        let refused = does(&dir, &mut zones, ZoneCommand::Sequencer { zone: "home".into(), url: "http://127.0.0.1:3931".into(), other_chain: false }).unwrap_err();
        assert!(refused.contains("serves another chain") && refused.ends_with("add --other-chain."), "{refused}");
        assert_eq!(zones.home().sequencer, "https://home.two");
        save(&dir, &zones, RPC).unwrap();
        let kept = load(&dir, RPC);
        assert_eq!((kept.home().sequencer.as_str(), kept.home().channel.as_deref()), ("https://home.two", Some(A)));
        let said = does(&dir, &mut zones, ZoneCommand::Sequencer { zone: "home".into(), url: "http://127.0.0.1:3931".into(), other_chain: true }).unwrap();
        assert!(said.contains(&format!("another chain (channel {B})")), "{said}");
        assert_eq!(zones.home().channel.as_deref(), Some(B));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_command_runs_on_the_zone_in_use_or_the_one_asked_for() {
        let dir = dir("applied");
        // No zones saved: home, with nothing chosen for it.
        let home = applied(&dir, RPC, false, None).unwrap().unwrap();
        assert_eq!((home.home, home.sequencer.clone(), home.wallet.clone()), (true, None, None));
        // --rpc sets the zones aside; with --zone it is refused.
        assert_eq!(applied(&dir, RPC, true, None).unwrap(), None);
        assert!(applied(&dir, RPC, true, Some("home")).unwrap_err().to_string().starts_with("give --zone or --rpc (LOGOS_NAMES_RPC), not both"));

        let mut zones = load(&dir, RPC);
        does(&dir, &mut zones, ZoneCommand::Add { sequencer: "http://127.0.0.1:3931".into(), name: Some("Second".into()) }).unwrap();
        save(&dir, &zones, RPC).unwrap();
        // Added, not in use: commands still run on home, one command can ask for it.
        assert!(applied(&dir, RPC, false, None).unwrap().unwrap().home);
        let asked = applied(&dir, RPC, false, Some("second")).unwrap().unwrap();
        assert_eq!((asked.home, asked.sequencer.as_deref(), asked.wallet.clone()), (false, Some("http://127.0.0.1:3931"), Some(zones::wallet_dir(&dir, B))));
        assert!(asked.names_elsewhere().starts_with("names are kept on Home, and this command would run on Second."));
        let wallet = asked.wallet_ready().unwrap().unwrap().to_owned();
        assert!(wallet.is_dir());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(std::fs::metadata(&wallet).unwrap().permissions().mode() & 0o777, 0o700);
        }
        assert!(applied(&dir, RPC, false, Some("nowhere")).unwrap_err().to_string().starts_with("no zone here is called nowhere"));
        // In use: every command runs there, and --zone home goes back for one.
        does(&dir, &mut zones, ZoneCommand::Use { zone: "Second".into() }).unwrap();
        save(&dir, &zones, RPC).unwrap();
        assert_eq!(applied(&dir, RPC, false, None).unwrap().unwrap().name, "Second");
        assert!(applied(&dir, RPC, false, Some("home")).unwrap().unwrap().home);
        // A sequencer chosen for home is the one its commands use.
        does(&dir, &mut zones, ZoneCommand::Sequencer { zone: "home".into(), url: "https://home.two".into(), other_chain: false }).unwrap();
        save(&dir, &zones, RPC).unwrap();
        assert_eq!(applied(&dir, RPC, false, Some("home")).unwrap().unwrap().sequencer.as_deref(), Some("https://home.two"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_list_says_which_zone_is_in_use_and_where_each_wallet_is() {
        let dir = dir("list");
        let mut zones = load(&dir, RPC);
        does(&dir, &mut zones, ZoneCommand::Add { sequencer: "http://127.0.0.1:3931".into(), name: Some("Second".into()) }).unwrap();
        let listed = does(&dir, &mut zones, ZoneCommand::List { json: false }).unwrap();
        assert!(listed.starts_with("* Home  (home)\n    sequencer  https://testnet.lez.logos.co\n"), "{listed}");
        assert!(listed.contains(&format!("  Second  (channel {B})\n    sequencer  http://127.0.0.1:3931\n")), "{listed}");
        assert!(listed.contains("(no wallet there yet)"), "{listed}");
        let json: Value = serde_json::from_str(&does(&dir, &mut zones, ZoneCommand::List { json: true }).unwrap()).unwrap();
        assert_eq!((json["current"].as_str(), json["zones"][1]["id"].as_str(), json["zones"][1]["has_wallet"].as_bool()), (Some("home"), Some(B), Some(false)));
        let _ = std::fs::remove_dir_all(dir);
    }
}
