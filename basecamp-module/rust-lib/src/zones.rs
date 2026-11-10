//! The LEZ zones the Wallet is set up for, and which one it uses now.
//!
//! A zone is one LEZ chain. It is known by its channel on the Logos Blockchain
//! (`getChannelId`, 64 hex characters), never by its sequencer's address: two addresses
//! that name the same channel serve the same zone, so changing one for the other keeps
//! everything, and an address that names another channel is another zone.
//!
//! The first zone is `home`: the one the configuration always had, where LNS names are
//! kept. It can't be removed. Each zone added later is called by its channel and keeps
//! its own wallet folder (`wallet_dir`), because a LEZ wallet's file holds what it last
//! read from one chain (its last block, its private accounts' state). The keys can be
//! the same on every zone: restore the wallet there from the recovery phrase.
//!
//! Saved in `config.json` as `zones` (a list, home first) and `zone` (the one in use).
//! Home's settings stay where they always were, the flat `sequencer`, `registry`,
//! `cli_wallet` and `token_program`: an older version reads those and finds the home
//! zone, and what it changes there is read back as home's.
//!
//! The `names` CLI includes this file by path for `names zone` (with `text.rs`), so the
//! CLI's zones follow the same rules. It needs only those two files and `serde_json`.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// What the first zone is called in every call and file.
pub const HOME: &str = "home";
/// The most zones kept.
pub const MAX_ZONES: usize = 16;
/// The longest name a zone is given, in characters.
pub const MAX_NAME: usize = 32;

/// One zone's settings. `token_program` is a program ID as text; `wallet` is a folder
/// the person chose for it (none: the CLI's own for home, `wallet_dir` for the others).
/// The names registry isn't here: there is one, on home.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Zone {
    pub name: String,
    pub channel: Option<String>,
    pub sequencer: String,
    pub wallet: Option<String>,
    pub token_program: Option<String>,
}

/// The zones, home first, and the one in use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Zones {
    list: Vec<Zone>,
    current: usize,
}

/// A channel as a sequencer names it: 64 hex characters, kept in lower case.
pub fn channel(text: &str) -> Option<String> {
    let text = text.trim();
    (text.len() == 64 && text.bytes().all(|b| b.is_ascii_hexdigit())).then(|| text.to_ascii_lowercase())
}

/// A zone's name as the person typed it: one line of 1 to `MAX_NAME` characters, with
/// nothing that hides or reorders text.
pub fn name(text: &str) -> Result<String, String> {
    let cleaned = crate::text::clean_text(text, usize::MAX);
    let spaced = text.trim().split(' ').filter(|word| !word.is_empty()).collect::<Vec<_>>().join(" ");
    if cleaned != spaced {
        return Err("a zone's name is one line, with no control or hidden characters".to_owned());
    }
    match cleaned.chars().count() {
        0 => Err("give the zone a name".to_owned()),
        length if length > MAX_NAME => Err(format!("a zone's name is at most {MAX_NAME} characters")),
        _ => Ok(cleaned),
    }
}

/// The name a zone gets when none is given: the start of its channel.
pub fn default_name(channel: &str) -> String {
    format!("Zone {}", &channel[..channel.len().min(8)])
}

/// Where an added zone keeps its wallet unless another folder was chosen:
/// `<module folder>/zones/<channel>/wallet`. Removing the zone leaves it there, so adding
/// the zone again finds the same wallet.
pub fn wallet_dir(config_dir: &Path, channel: &str) -> PathBuf {
    config_dir.join("zones").join(channel).join("wallet")
}

/// Why a sequencer can't be used for a zone. `other_chain` says it answered and names a
/// channel no zone here has, so it could be added as a zone (or, for home, taken anyway).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    pub why: String,
    pub other_chain: Option<String>,
}

/// Whether a zone may use another sequencer, and what its channel is afterwards.
///
/// `known` is the zone's channel, `read` what the new sequencer says its channel is, and
/// `holder` the name of another zone that already has that channel. The same channel
/// keeps everything. A zone whose channel was never read takes the one read now, or
/// changes unchecked when the sequencer doesn't say (as every version before did). Any
/// other channel is refused: it is another chain, and this zone's wallet folder and
/// history are not about it. `take_other` (home only) takes it all the same.
pub fn sequencer_change(
    zone: &str,
    known: Option<&str>,
    read: Result<&str, &str>,
    holder: Option<&str>,
    take_other: bool,
) -> Result<Option<String>, Refused> {
    match (known, read) {
        (Some(known), Ok(read)) if known == read => Ok(Some(read.to_owned())),
        (_, Ok(_)) if holder.is_some() => Err(Refused {
            why: format!(
                "that sequencer serves {}, a zone already set up here. Switch to it, or change its sequencer instead.",
                holder.unwrap_or_default()
            ),
            other_chain: None,
        }),
        (None, Ok(read)) => Ok(Some(read.to_owned())),
        (Some(_), Ok(read)) if take_other => Ok(Some(read.to_owned())),
        (Some(_), Ok(read)) => Err(Refused {
            why: format!(
                "that sequencer serves another chain (channel {}), not {zone}. Nothing was changed. Add it as a zone of its own to use it.",
                short(read)
            ),
            other_chain: Some(read.to_owned()),
        }),
        (Some(_), Err(why)) => Err(Refused {
            why: format!(
                "that sequencer didn't say which chain it serves, so it can't be told to be {zone}'s. Nothing was changed. ({})",
                why.trim().trim_end_matches('.')
            ),
            other_chain: None,
        }),
        (None, Err(_)) => Ok(None),
    }
}

/// Whether `text` can be a program's account ID: 32 bytes in base58 are 32 to 44 of its
/// characters. What uses the ID reads it in full; this only keeps other text out of the list.
fn account_like(text: &str) -> bool {
    const BASE58: &str = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    (32..=44).contains(&text.len()) && text.chars().all(|c| BASE58.contains(c))
}

/// A channel cut short for a sentence.
pub fn short(channel: &str) -> String {
    if channel.len() > 16 {
        format!("{}…{}", &channel[..8], &channel[channel.len() - 4..])
    } else {
        channel.to_owned()
    }
}

impl Zone {
    /// Where the zone's balance marks are kept under: its channel, or its sequencer's
    /// address while the channel isn't known.
    pub fn scope(&self) -> &str {
        self.channel.as_deref().unwrap_or(&self.sequencer)
    }

    fn json(&self, id: &str) -> Value {
        json!({
            "id": id,
            "name": self.name,
            "channel": self.channel,
            "sequencer": self.sequencer,
            "wallet": self.wallet,
            "token_program": self.token_program,
        })
    }

    /// A saved added zone, or None when it can't be used: no channel, or no sequencer
    /// the module would talk to. Fields that don't read are left empty.
    fn saved(value: &Value) -> Option<Zone> {
        let text = |key: &str| value[key].as_str().map(str::trim).filter(|text| !text.is_empty()).map(str::to_owned);
        let sequencer = text("sequencer").filter(|url| crate::text::is_http_url(url))?;
        let channel = value["channel"].as_str().and_then(channel)?;
        Some(Zone {
            name: text("name").and_then(|typed| name(&typed).ok()).unwrap_or_else(|| default_name(&channel)),
            channel: Some(channel),
            sequencer,
            wallet: text("wallet"),
            token_program: text("token_program").filter(|id| account_like(id)),
        })
    }
}

impl Zones {
    /// Only the home zone.
    pub fn new(mut home: Zone) -> Self {
        if home.name.is_empty() {
            home.name = "Home".to_owned();
        }
        Self { list: vec![home], current: 0 }
    }

    /// The zones a configuration saved. `home` is made from its flat fields, which are
    /// all a file from before zones has; the list adds home's name and channel, and the
    /// other zones. An older version may have changed home's sequencer since the list was
    /// written, so the channel is kept only for the sequencer it was read from. An added
    /// zone that can't be used (no channel, no sequencer, a channel another zone has) is
    /// left out; its wallet folder stays on disk.
    pub fn load(saved: &Value, mut home: Zone) -> Self {
        let list = saved["zones"].as_array().map(Vec::as_slice).unwrap_or_default();
        if let Some(first) = list.first() {
            if let Some(typed) = first["name"].as_str().and_then(|typed| name(typed).ok()) {
                home.name = typed;
            }
            if first["sequencer"].as_str() == Some(home.sequencer.as_str()) {
                home.channel = first["channel"].as_str().and_then(channel);
            }
        }
        let mut zones = Self::new(home);
        for zone in list.iter().skip(1).filter_map(Zone::saved) {
            let taken = zones.list.iter().any(|kept| kept.channel == zone.channel || kept.name.to_lowercase() == zone.name.to_lowercase());
            if zones.list.len() < MAX_ZONES && !taken {
                zones.list.push(zone);
            }
        }
        zones.current = saved["zone"].as_str().and_then(|id| zones.index(id)).unwrap_or(0);
        zones
    }

    /// The list as it is saved and shown, home first, each with its `id`.
    pub fn json(&self) -> Value {
        Value::Array(self.list.iter().enumerate().map(|(at, zone)| zone.json(&self.id(at))).collect())
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    /// Which zone is in use (0 is home).
    pub fn current(&self) -> usize {
        self.current
    }

    pub fn home(&self) -> &Zone {
        &self.list[0]
    }

    pub fn zone(&self) -> &Zone {
        &self.list[self.current]
    }

    pub fn at(&self, index: usize) -> Option<&Zone> {
        self.list.get(index)
    }

    pub fn at_mut(&mut self, index: usize) -> Option<&mut Zone> {
        self.list.get_mut(index)
    }

    /// What zone `index` is called: `home`, or its channel.
    pub fn id(&self, index: usize) -> String {
        match self.list.get(index) {
            Some(zone) if index > 0 => zone.channel.clone().unwrap_or_default(),
            _ => HOME.to_owned(),
        }
    }

    pub fn current_id(&self) -> String {
        self.id(self.current)
    }

    /// The zone called `id` (`home`, or a channel in either case).
    pub fn index(&self, id: &str) -> Option<usize> {
        let id = id.trim();
        if id == HOME {
            return Some(0);
        }
        let wanted = channel(id)?;
        self.list.iter().skip(1).position(|zone| zone.channel.as_deref() == Some(wanted.as_str())).map(|at| at + 1)
    }

    /// The zone called `id`, or why there is none.
    pub fn find(&self, id: &str) -> Result<usize, String> {
        self.index(id).ok_or_else(|| "there is no such zone here".to_owned())
    }

    /// The zone that has `channel`, other than `but`.
    pub fn holder(&self, channel: &str, but: Option<usize>) -> Option<usize> {
        self.list.iter().enumerate().position(|(at, zone)| Some(at) != but && zone.channel.as_deref() == Some(channel))
    }

    /// Puts `zone` in place of the one in use (its settings as they are now).
    pub fn put_current(&mut self, zone: Zone) {
        self.list[self.current] = zone;
    }

    /// Uses zone `index` from now on.
    pub fn use_zone(&mut self, index: usize) {
        if index < self.list.len() {
            self.current = index;
        }
    }

    /// Adds the zone `sequencer` serves, whose channel it said is `channel`, and returns
    /// where it is in the list. Refused when that channel is a zone here already, when
    /// the name is taken, and past `MAX_ZONES`.
    pub fn add(&mut self, typed_name: &str, sequencer: &str, channel_read: &str) -> Result<usize, String> {
        let channel = channel(channel_read).ok_or("the sequencer's channel isn't 64 hex characters")?;
        if let Some(at) = self.holder(&channel, None) {
            return Err(format!(
                "that sequencer serves {}, a zone already set up here (channel {}). To use this sequencer for it, change that zone's sequencer.",
                self.list[at].name,
                short(&channel)
            ));
        }
        if self.list.len() >= MAX_ZONES {
            return Err(format!("at most {MAX_ZONES} zones are kept; remove one first"));
        }
        let name = if typed_name.trim().is_empty() { default_name(&channel) } else { name(typed_name)? };
        self.name_free(&name, None)?;
        self.list.push(Zone { name, channel: Some(channel), sequencer: sequencer.to_owned(), wallet: None, token_program: None });
        Ok(self.list.len() - 1)
    }

    /// Takes zone `index` off the list and returns it. Home stays, and so does the zone
    /// in use.
    pub fn remove(&mut self, index: usize) -> Result<Zone, String> {
        if index == 0 {
            return Err("the home zone can't be removed: names are kept on it".to_owned());
        }
        if index >= self.list.len() {
            return Err("there is no such zone here".to_owned());
        }
        if index == self.current {
            return Err(format!("{} is the zone in use; switch to another one first", self.list[index].name));
        }
        if self.current > index {
            self.current -= 1;
        }
        Ok(self.list.remove(index))
    }

    /// Gives zone `index` another name.
    pub fn rename(&mut self, index: usize, typed: &str) -> Result<(), String> {
        let name = name(typed)?;
        self.name_free(&name, Some(index))?;
        self.list.get_mut(index).ok_or("there is no such zone here")?.name = name;
        Ok(())
    }

    fn name_free(&self, name: &str, but: Option<usize>) -> Result<(), String> {
        let taken = self.list.iter().enumerate().any(|(at, zone)| Some(at) != but && zone.name.to_lowercase() == name.to_lowercase());
        if taken {
            Err(format!("a zone here is already called {name}"))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "0101010101010101010101010101010101010101010101010101010101010101";
    const B: &str = "abababababababababababababababababababababababababababababababab";
    const REGISTRY: &str = "38jEfVV1RCyMzu1FGbYcCn5FqxCxKyY58VgzjruAh2gq";

    fn home() -> Zone {
        Zone {
            name: String::new(),
            channel: None,
            sequencer: "https://testnet.lez.logos.co".to_owned(),
            wallet: None,
            token_program: None,
        }
    }

    #[test]
    fn an_old_configuration_is_one_home_zone() {
        let zones = Zones::load(&json!({ "sequencer": "https://testnet.lez.logos.co", "registry": REGISTRY }), home());
        assert_eq!(zones.len(), 1);
        assert_eq!(zones.current_id(), HOME);
        assert_eq!(zones.home().name, "Home");
        assert_eq!(zones.home().sequencer, "https://testnet.lez.logos.co");
        assert_eq!(zones.home().scope(), "https://testnet.lez.logos.co");
        assert_eq!(zones.json()[0]["id"], HOME);
    }

    #[test]
    fn zones_are_saved_and_read_back_with_the_one_in_use() {
        let mut zones = Zones::new(Zone { channel: Some(A.to_owned()), ..home() });
        let at = zones.add("Second", "http://127.0.0.1:3299", &B.to_uppercase()).expect("added");
        assert_eq!(zones.id(at), B);
        zones.at_mut(at).unwrap().wallet = Some("/w/second".to_owned());
        zones.use_zone(at);
        let saved = json!({ "zones": zones.json(), "zone": zones.current_id() });
        let read = Zones::load(&saved, home());
        assert_eq!(read, zones);
        assert_eq!(read.zone().name, "Second");
        assert_eq!(read.zone().scope(), B);
        assert_eq!(read.zone().wallet.as_deref(), Some("/w/second"));
        assert_eq!(read.home().channel.as_deref(), Some(A));
        // A zone in use that is no longer listed falls back to home.
        let gone = Zones::load(&json!({ "zones": [zones.json()[0]], "zone": B }), home());
        assert_eq!(gone.current(), 0);
    }

    #[test]
    fn the_flat_fields_are_home_whatever_the_list_says() {
        let mut zones = Zones::new(Zone { name: "Testnet".to_owned(), channel: Some(A.to_owned()), ..home() });
        zones.add("Second", "http://127.0.0.1:3299", B).expect("added");
        let saved = json!({ "zones": zones.json(), "zone": HOME });
        // An older version changed the sequencer since: home is what it left, and the
        // channel read from the other sequencer is not carried over to this one.
        let moved = Zones::load(&saved, Zone { sequencer: "http://127.0.0.1:3040".to_owned(), ..home() });
        assert_eq!(moved.home().sequencer, "http://127.0.0.1:3040");
        assert_eq!(moved.home().channel, None);
        assert_eq!(moved.home().name, "Testnet");
        assert_eq!(moved.len(), 2);
    }

    #[test]
    fn a_saved_zone_that_cant_be_used_is_left_out() {
        let saved = json!({
            "zones": [
                { "id": HOME, "name": "Home", "channel": A, "sequencer": "https://testnet.lez.logos.co", "registry": REGISTRY },
                { "name": "No channel", "sequencer": "http://127.0.0.1:1" },
                { "name": "Plain http elsewhere", "channel": B, "sequencer": "http://example.com" },
                { "name": "Same as home", "channel": A, "sequencer": "http://127.0.0.1:2" },
                { "name": "Fine\u{202E}", "channel": B, "sequencer": "http://127.0.0.1:3", "registry": REGISTRY, "token_program": "not a program" },
                { "name": "Twice", "channel": B, "sequencer": "http://127.0.0.1:4" },
                { "name": "home", "channel": "cd".repeat(32), "sequencer": "http://127.0.0.1:5" },
                "not a zone",
            ],
            "zone": B,
        });
        let zones = Zones::load(&saved, home());
        assert_eq!(zones.len(), 2);
        // A name that hides text isn't kept, nor a token program that doesn't read.
        assert_eq!(zones.zone().name, default_name(B));
        assert_eq!(zones.zone().token_program, None);
        assert_eq!(zones.zone().sequencer, "http://127.0.0.1:3");
        // A first entry that doesn't read leaves home as the flat fields have it.
        let broken = Zones::load(&json!({ "zones": [7, saved["zones"][4]], "zone": B }), home());
        assert_eq!(broken.len(), 2);
        assert_eq!(broken.home().name, "Home");
        assert_eq!(broken.home().channel, None);
        assert_eq!(broken.current(), 1);
        assert_eq!(Zones::load(&json!({ "zones": "none" }), home()).len(), 1);
    }

    #[test]
    fn adding_removing_and_renaming() {
        let mut zones = Zones::new(Zone { channel: Some(A.to_owned()), ..home() });
        // The same chain twice is one zone, whatever address serves it.
        let again = zones.add("Copy", "http://127.0.0.1:3299", A).unwrap_err();
        assert!(again.contains("Home") && again.contains("already set up"), "{again}");
        assert!(zones.add("Second", "http://127.0.0.1:3299", "nope").is_err());
        assert!(zones.add("home", "http://127.0.0.1:3299", B).unwrap_err().contains("already called"));
        assert!(zones.add(&"x".repeat(MAX_NAME + 1), "http://127.0.0.1:3299", B).is_err());
        assert!(zones.add("line\nbreak", "http://127.0.0.1:3299", B).is_err());
        let at = zones.add("", "http://127.0.0.1:3299", B).expect("added");
        assert_eq!(zones.at(at).unwrap().name, "Zone abababab");
        zones.rename(at, "  Second   zone ").expect("renamed");
        assert_eq!(zones.at(at).unwrap().name, "Second zone");
        assert!(zones.rename(0, "second ZONE").unwrap_err().contains("already called"));
        assert!(zones.rename(0, "Home").is_ok());
        assert_eq!(zones.index(&B.to_uppercase()), Some(at));
        assert_eq!(zones.index("nothing"), None);
        // Home stays, and so does the zone in use.
        assert!(zones.remove(0).unwrap_err().contains("home"));
        zones.use_zone(at);
        assert!(zones.remove(at).unwrap_err().contains("in use"));
        zones.use_zone(0);
        assert_eq!(zones.remove(at).expect("removed").channel.as_deref(), Some(B));
        assert_eq!(zones.len(), 1);
        assert!(zones.remove(1).is_err());
    }

    #[test]
    fn removing_an_earlier_zone_keeps_the_one_in_use() {
        let mut zones = Zones::new(Zone { channel: Some(A.to_owned()), ..home() });
        let b = zones.add("B", "http://127.0.0.1:1", B).unwrap();
        let c = zones.add("C", "http://127.0.0.1:2", &"cd".repeat(32)).unwrap();
        zones.use_zone(c);
        zones.remove(b).unwrap();
        assert_eq!(zones.zone().name, "C");
        assert_eq!(zones.current_id(), "cd".repeat(32));
    }

    #[test]
    fn the_list_is_bounded() {
        let mut zones = Zones::new(home());
        for at in 1..MAX_ZONES {
            zones.add(&format!("Zone {at}"), "http://127.0.0.1:1", &format!("{at:064x}")).expect("room");
        }
        assert!(zones.add("", "http://127.0.0.1:1", B).unwrap_err().contains("at most"));
    }

    #[test]
    fn a_sequencer_is_changed_only_within_its_zone() {
        // The same channel: another sequencer of the same zone.
        assert_eq!(sequencer_change("Home", Some(A), Ok(A), None, false), Ok(Some(A.to_owned())));
        // Never read before: the channel is learned now.
        assert_eq!(sequencer_change("Home", None, Ok(B), None, false), Ok(Some(B.to_owned())));
        // Never read, and this one doesn't say: changed unchecked, as before zones.
        assert_eq!(sequencer_change("Home", None, Err("down"), None, false), Ok(None));
        // Another chain is refused, and told so it can be added as a zone.
        let other = sequencer_change("Home", Some(A), Ok(B), None, false).unwrap_err();
        assert_eq!(other.other_chain.as_deref(), Some(B));
        assert!(other.why.contains("another chain") && other.why.contains("abababab…abab"), "{}", other.why);
        // ... unless it is taken on purpose.
        assert_eq!(sequencer_change("Home", Some(A), Ok(B), None, true), Ok(Some(B.to_owned())));
        // A chain that is a zone here already is never taken twice.
        for take in [false, true] {
            let held = sequencer_change("Home", Some(A), Ok(B), Some("Second"), take).unwrap_err();
            assert!(held.why.contains("Second") && held.other_chain.is_none(), "{}", held.why);
            assert!(sequencer_change("Home", None, Ok(B), Some("Second"), take).is_err());
        }
        // A known zone never moves to a sequencer that doesn't say what it serves.
        for take in [false, true] {
            let silent = sequencer_change("Home", Some(A), Err("timed out."), None, take).unwrap_err();
            assert!(silent.why.contains("didn't say") && silent.why.ends_with("(timed out)"), "{}", silent.why);
        }
    }

    #[test]
    fn channels_and_names_as_typed() {
        assert_eq!(channel(&format!(" {} ", B.to_uppercase())).as_deref(), Some(B));
        assert_eq!(channel("0x01"), None);
        assert_eq!(channel(&"g".repeat(64)), None);
        assert_eq!(name(" My  zone ").as_deref(), Ok("My zone"));
        assert!(name("a\tb").is_err());
        assert!(name("").is_err());
        assert!(name("a\u{200B}b").is_err());
        assert_eq!(short(B), "abababab…abab");
        assert_eq!(wallet_dir(Path::new("/c"), B), PathBuf::from(format!("/c/zones/{B}/wallet")));
    }
}
