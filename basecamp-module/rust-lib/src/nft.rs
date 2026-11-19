//! NFTs in the Names module (LP-0001; plan in `docs/wallet/NFT.md`).
//!
//! Everything that touches the wallet runs in the names CLI (`names nft …`, `names module nft
//! …`, `cli/src/nft.rs`): this file keeps the per-network list of NFT programs, checks every
//! argument before the CLI is looked for, reads holdings and sheets on a thread of their own
//! (`served`, so no call waits on the CLI on the module's thread), starts the sends as wallet
//! jobs, and keeps the person's choice to load a collection's image.
//!
//! The two-step rule: an action (Receive, Send, Print, Burn) starts only with the digest of a
//! sheet the module itself planned for exactly that action, with nothing refused, within
//! `PLAN_VALID`. The Names app shows that sheet; an app's request is shown the same sheet
//! before the person approves it (`provider_nft.rs`).

use crate::nft_wire;
use crate::{cli, provider, registry, served, Names};
use serde_json::{json, Value};
use sha2::{Digest as _, Sha256};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Programs one network may list.
pub const MAX_PROGRAMS: usize = 16;
/// The person's words for a program.
pub const MAX_LABEL: usize = 40;
/// How often a page's NFT reads are made again while it is open (the app asks no faster).
pub const NFT_EVERY: Duration = Duration::from_secs(30);
/// A sheet's plan is read again after this, and an action starts only from one this fresh.
pub const PLAN_VALID: Duration = Duration::from_secs(600);
/// `names module nft holdings` catches the wallet up first.
const LIST_PATIENCE: Duration = Duration::from_secs(150);
const INFO_PATIENCE: Duration = Duration::from_secs(30);
const PLAN_PATIENCE: Duration = Duration::from_secs(150);
/// Rows believed from one answer.
const MAX_ROWS: usize = 256;
/// The most a collection's image may be, fetched.
pub const MAX_IMAGE_BYTES: usize = 1024 * 1024;
/// Collections whose image the person allowed.
const MAX_MEDIA: usize = 256;

pub const KEY: &str = "nft";

// ── The program list (per network, user-added, empty to start) ──────────

fn list_path() -> Option<PathBuf> {
    crate::config_dir().map(|dir| dir.join("nft-programs.json"))
}

fn load(path: &Path) -> Value {
    std::fs::read(path).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).filter(Value::is_object).unwrap_or_else(|| json!({}))
}

fn save(path: &Path, value: &Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("can't keep the NFT programs: {e}"))?;
    }
    cli::write_private(path, value.to_string().as_bytes()).map_err(|e| format!("can't keep the NFT programs: {e}"))
}

/// A program's ID, checked: an account ID other than the native token.
pub fn program_id(text: &str) -> Result<registry::AccountId, String> {
    let id = registry::parse_account(text.trim()).map_err(|_| "an NFT program is written as its account ID (base58)".to_owned())?;
    if id == registry::NATIVE_TOKEN {
        return Err("that is the native token, not an NFT program".to_owned());
    }
    Ok(id)
}

/// The programs listed for `network` (a zone and a names network, `Names::served_key`'s).
pub fn programs_in(path: &Path, network: &str) -> Vec<Value> {
    load(path)["programs"][network].as_array().cloned().unwrap_or_default()
}

pub fn add_program(path: &Path, network: &str, program: &str, label: &str, now: u64) -> Result<Vec<Value>, String> {
    let id = registry::account_text(&program_id(program)?);
    let label = provider::clean_text(label, MAX_LABEL);
    let mut all = load(path);
    let mut list = programs_in(path, network);
    list.retain(|entry| entry["program"] != id.as_str());
    if list.len() >= MAX_PROGRAMS {
        return Err(format!("The wallet lists at most {MAX_PROGRAMS} NFT programs per network: remove one first."));
    }
    list.push(json!({ "program": id, "label": if label.is_empty() { Value::Null } else { json!(label) }, "added": now }));
    all["programs"][network] = json!(list);
    save(path, &all)?;
    Ok(list)
}

pub fn remove_program(path: &Path, network: &str, program: &str) -> Result<Vec<Value>, String> {
    let id = registry::account_text(&program_id(program)?);
    let mut all = load(path);
    let mut list = programs_in(path, network);
    list.retain(|entry| entry["program"] != id.as_str());
    all["programs"][network] = json!(list);
    save(path, &all)?;
    Ok(list)
}

/// Whether the person allowed loading the image of `definition` of `program`.
pub fn media_allowed(path: &Path, program: &str, definition: &str) -> bool {
    load(path)["media"][format!("{program}:{definition}")] == true
}

pub fn set_media(path: &Path, program: &str, definition: &str, allow: bool) -> Result<bool, String> {
    let program = registry::account_text(&program_id(program)?);
    let definition = registry::account_text(&registry::parse_account(definition.trim()).map_err(|_| "a collection is written as its definition's account ID".to_owned())?);
    let mut all = load(path);
    if !all["media"].is_object() {
        all["media"] = json!({});
    }
    let key = format!("{program}:{definition}");
    let media = all["media"].as_object_mut().expect("just made");
    if allow {
        if media.len() >= MAX_MEDIA && !media.contains_key(&key) {
            return Err(format!("The wallet remembers at most {MAX_MEDIA} collections whose images load."));
        }
        media.insert(key, json!(true));
    } else {
        media.remove(&key);
    }
    save(path, &all)?;
    Ok(allow)
}

// ── Actions ─────────────────────────────────────────────────────────────

/// What a sheet and a job act on; checked here, written back canonically.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    /// `init`, `send`, `print` or `burn`.
    pub action: String,
    pub from: Option<String>,
    pub to: Option<String>,
    pub definition: Option<String>,
    /// `init` into a new account: `public` or `private`.
    pub new: Option<String>,
    pub their_keys: bool,
}

/// An account written `Public/<id>` or `Private/<id>`.
fn account(text: &str) -> Result<String, String> {
    let (private, id) = provider::account_ref(text)?;
    Ok(provider::account_written(private, &id))
}

/// Where an NFT goes: an LNS name, `Public/<id>`, this wallet's `Private/<id>`, or someone's
/// receiving keys `<npk hex>:<vpk hex>`.
pub fn destination(text: &str) -> Result<String, String> {
    let text = text.trim();
    if text.starts_with("Public/") || text.starts_with("Private/") {
        return account(text);
    }
    if let Some((npk, vpk)) = text.split_once(':') {
        let hex = |part: &str| !part.is_empty() && part.len() % 2 == 0 && part.bytes().all(|b| b.is_ascii_hexdigit());
        if npk.len() == 64 && hex(npk) && hex(vpk) && vpk.len() <= 512 {
            return Ok(format!("{}:{}", npk.to_ascii_lowercase(), vpk.to_ascii_lowercase()));
        }
        return Err("receiving keys are written <npk hex>:<vpk hex>".to_owned());
    }
    if registry::parse_account(text).is_ok() {
        return Err("write an account as Public/ and then its ID".to_owned());
    }
    registry::parse_name(text).map_err(|_| format!("{} isn't an LNS name, an account or receiving keys", provider::clean_text(text, 60)))
}

impl Action {
    pub fn parse(text: &str) -> Result<Action, String> {
        let given: Value = serde_json::from_str(text).map_err(|_| "an NFT action is a JSON object".to_owned())?;
        let object = given.as_object().ok_or("an NFT action is a JSON object")?;
        for key in object.keys() {
            if !["action", "from", "to", "definition", "new", "their_keys", "fee_payer"].contains(&key.as_str()) {
                return Err(format!("an NFT action has no field {}", provider::clean_text(key, 30)));
            }
        }
        let field = |key: &str| given[key].as_str().map(str::trim).filter(|text| !text.is_empty());
        let action = field("action").unwrap_or_default().to_owned();
        let mut out = Action { action: action.clone(), from: None, to: None, definition: None, new: None, their_keys: given["their_keys"] == true };
        match action.as_str() {
            "init" => {
                out.definition = Some(registry::account_text(&registry::parse_account(field("definition").ok_or("say which collection to get ready for")?).map_err(|_| "a collection is written as its definition's account ID".to_owned())?));
                match (field("from"), field("new")) {
                    (Some(from), None) => out.from = Some(account(from)?),
                    (None, Some(kind @ ("public" | "private"))) => out.new = Some(kind.to_owned()),
                    _ => return Err("say which account to make ready, or new: public or private".to_owned()),
                }
            }
            "send" | "print" => {
                out.from = Some(account(field("from").ok_or("say which account it leaves")?)?);
                out.to = Some(destination(field("to").ok_or("say where it goes")?)?);
                if out.from == out.to {
                    return Err("an account can't send to itself".to_owned());
                }
            }
            "burn" => out.from = Some(account(field("from").ok_or("say which account burns")?)?),
            _ => return Err("an NFT action is init, send, print or burn".to_owned()),
        }
        if out.their_keys && !out.to.as_deref().is_some_and(|to| to.contains(':')) {
            out.their_keys = false;
        }
        Ok(out)
    }

    /// The canonical JSON the CLI is given and the digest is made from.
    pub fn json(&self) -> Value {
        json!({ "action": self.action, "from": self.from, "to": self.to, "definition": self.definition, "new": self.new, "their_keys": self.their_keys })
    }

    /// Whether it sends to receiving keys this wallet can't check.
    pub fn to_their_keys(&self) -> bool {
        self.to.as_deref().is_some_and(|to| to.contains(':'))
    }

    /// The `names nft …` command for it, with the private fee payer when it is proven.
    pub fn args(&self, program: &registry::AccountId, fee_payer: Option<&str>) -> Result<Vec<String>, String> {
        let mut args = vec![program_arg(program), "nft".to_owned(), self.action.clone()];
        let bare = |text: &str| text.split_once('/').map_or(text, |(_, id)| id).to_owned();
        match self.action.as_str() {
            "init" => {
                args.push(format!("--definition={}", self.definition.as_deref().unwrap_or_default()));
                match (&self.from, &self.new) {
                    (Some(from), _) => args.push(format!("--account={}", bare(from))),
                    (None, Some(kind)) => args.push(format!("--new={kind}")),
                    _ => return Err("say which account to make ready".to_owned()),
                }
            }
            "send" | "print" => {
                args.push(format!("--from={}", bare(self.from.as_deref().unwrap_or_default())));
                args.push(format!("--to={}", self.to.as_deref().unwrap_or_default()));
                if self.to_their_keys() {
                    if !self.their_keys {
                        return Err("this wallet can't check that those receiving keys' account is ready for this collection; tick that you accept the risk".to_owned());
                    }
                    args.push("--their-keys".to_owned());
                }
            }
            "burn" => args.push(format!("--from={}", bare(self.from.as_deref().unwrap_or_default()))),
            _ => return Err("an NFT action is init, send, print or burn".to_owned()),
        }
        if let Some(payer) = fee_payer {
            let (private, id) = provider::account_ref(payer)?;
            if !private {
                return Err("a private transaction's fee is paid by a private account".to_owned());
            }
            args.push(format!("--fee-payer={}", registry::account_text(&id)));
        }
        Ok(args)
    }
}

fn program_arg(program: &registry::AccountId) -> String {
    format!("--nft-program={}", registry::account_text(program))
}

/// The digest a sheet carries and an action must name: the program and the action.
pub fn digest(program: &registry::AccountId, action: &Action) -> String {
    let mut hash = Sha256::new();
    hash.update(b"names-nft-sheet-v1");
    hash.update(registry::account_text(program).as_bytes());
    hash.update(action.json().to_string().as_bytes());
    registry::hex(&hash.finalize())
}

// ── What the CLI says, read back and cleaned ────────────────────────────

fn id_text(value: &Value) -> Option<String> {
    value.as_str().and_then(|text| registry::parse_account(text).ok()).map(|id| registry::account_text(&id))
}

fn digits(value: &Value) -> Option<String> {
    value.as_str().filter(|text| !text.is_empty() && text.len() <= 39 && text.bytes().all(|b| b.is_ascii_digit())).map(str::to_owned)
}

/// A collection as the CLI read it: the maker's text, cleaned (control and direction-changing
/// characters can't reorder what the person reads), every field kept.
pub fn collection(value: &Value) -> Value {
    if !value.is_object() {
        return Value::Null;
    }
    let text = |key: &str, max: usize| value[key].as_str().map_or(Value::Null, |text| json!(provider::clean_text(text, max)));
    json!({
        "definition": id_text(&value["definition"]),
        "name": text("name", nft_wire::MAX_NAME_LEN),
        "printable_supply": digits(&value["printable_supply"]),
        "standard": match value["standard"].as_str() { Some(word @ ("Simple" | "Expanded")) => json!(word), _ => Value::Null },
        "uri": text("uri", nft_wire::MAX_URI_LEN),
        "creators": text("creators", nft_wire::MAX_CREATORS_LEN),
    })
}

/// `names module nft holdings` read back: every account's state, the waiting credits, and the
/// collections grouped with how many of the wallet's accounts hold each.
pub fn parse_holdings(output: &str) -> Result<Value, String> {
    let answer: Value = serde_json::from_str(cli::last_line(output)).map_err(|_| "the names CLI's NFT list did not decode".to_owned())?;
    let rows = |key: &str| answer[key].as_array().map(Vec::as_slice).unwrap_or_default().iter().take(MAX_ROWS).cloned().collect::<Vec<_>>();
    let holdings: Vec<Value> = rows("holdings")
        .iter()
        .filter_map(|row| {
            let account = account(row["account"].as_str()?).ok()?;
            let state = match row["state"].as_str()? {
                kind @ ("empty" | "definition" | "master" | "copy" | "unreadable") => kind,
                _ => return None,
            };
            Some(json!({
                "account": account,
                "account_kind": if account.starts_with("Private/") { "private" } else { "public" },
                "state": state,
                "holds": row["holds"] == true,
                "definition": id_text(&row["definition"]),
                "print_balance": digits(&row["print_balance"]),
                "owned": row["owned"].as_bool(),
                "unreadable": row["unreadable"].as_str().map(|why| provider::clean_text(why, 80)),
                "collection": collection(&row["collection"]),
            }))
        })
        .collect();
    let pending: Vec<Value> = rows("pending")
        .iter()
        .filter_map(|row| {
            Some(json!({
                "position": row["position"].as_u64()?,
                "definition": id_text(&row["definition"]),
                "value": row["value"].as_str().filter(|v| *v == "copy" || v.strip_prefix("master:").is_some_and(|n| n.bytes().all(|b| b.is_ascii_digit()))),
                "to": account(row["to"].as_str()?).ok()?,
            }))
        })
        .collect();
    Ok(json!({ "holdings": holdings, "pending": pending, "collections": collections(&holdings) }))
}

/// The collections the accounts hold an NFT of, each with how many of them hold one and which.
/// A holding whose collection can't be read is listed by its ID, as unreadable.
pub fn collections(holdings: &[Value]) -> Vec<Value> {
    let mut grouped: Vec<Value> = Vec::new();
    for row in holdings.iter().filter(|row| row["holds"] == true) {
        let definition = row["definition"].clone();
        match grouped.iter_mut().find(|group| group["definition"] == definition) {
            Some(group) => {
                group["count"] = json!(group["count"].as_u64().unwrap_or(0) + 1);
                group["accounts"].as_array_mut().expect("made below").push(row["account"].clone());
            }
            None => grouped.push(json!({
                "definition": definition,
                "name": row["collection"]["name"],
                "readable": !row["collection"].is_null(),
                "count": 1,
                "accounts": [row["account"]],
            })),
        }
    }
    grouped
}

/// `names module nft plan` read back: the sheet.
pub fn parse_plan(output: &str) -> Result<Value, String> {
    let plan: Value = serde_json::from_str(cli::last_line(output)).map_err(|_| "the names CLI's NFT sheet did not decode".to_owned())?;
    let lines = |key: &str| plan[key].as_array().map(Vec::as_slice).unwrap_or_default().iter().take(8).filter_map(|line| line.as_str().map(|line| json!(provider::clean_text(line, 200)))).collect::<Vec<_>>();
    let payers: Vec<Value> = plan["payers"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .take(64)
        .filter_map(|payer| Some(json!({ "account": account(payer["account"].as_str()?).ok()?, "balance": digits(&payer["balance"]), "enough": payer["enough"] == true })))
        .collect();
    Ok(json!({
        "action": plan["action"].as_str().map(|a| provider::clean_text(a, 10)),
        "from": plan["from"].as_str().and_then(|from| account(from).ok()),
        "collection": collection(&plan["collection"]),
        "lines": lines("lines"),
        "public": lines("public"),
        "private": lines("private"),
        "proven": plan["proven"] == true,
        "fee": { "about": digits(&plan["fee"]["about"]), "at_most": digits(&plan["fee"]["at_most"]), "inside_proof": plan["fee"]["inside_proof"] == true },
        "payers": payers,
        "refusal": plan["refusal"].as_str().map(|why| provider::clean_text(nft_wire::refusal(why).unwrap_or(why), 240)),
    }))
}

/// What an ended NFT job says: its transaction and block, or the plain refusal.
pub fn outcome(log: &str) -> Value {
    let tx = log.lines().rev().find_map(|line| line.trim().strip_prefix("submitted ")).map(str::trim).map(str::to_owned);
    let blocks: Vec<u64> = log.lines().filter_map(|line| line.trim().strip_prefix("applied in block ")?.trim().parse().ok()).collect();
    let refusal = log.lines().rev().find_map(|line| line.trim().strip_prefix("Error: ")).map(|error| nft_wire::refusal(error).unwrap_or(error)).map(|why| provider::clean_text(why, 240));
    json!({ "tx": tx, "block": blocks.last(), "received": blocks.len(), "nothing": log.contains("nothing to receive"), "refusal": refusal })
}

// ── Media: a collection's image, only when allowed ──────────────────────

/// The URL a collection's `uri` may be fetched from, or why not: https only, a host name or a
/// public address (never loopback, a private network or a link-local one), no user part.
pub fn media_url(uri: &str) -> Result<String, String> {
    let uri = uri.trim();
    let rest = uri.strip_prefix("https://").ok_or("only an https address is loaded; this one isn't")?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.is_empty() || authority.contains('@') {
        return Err("that address has no plain host".to_owned());
    }
    let host = authority.rsplit_once(':').filter(|(_, port)| port.bytes().all(|b| b.is_ascii_digit())).map_or(authority, |(host, _)| host).trim_matches(['[', ']']).to_ascii_lowercase();
    if host == "localhost" || host.ends_with(".localhost") || host.ends_with(".local") || host.ends_with(".internal") || !host.contains('.') && host.parse::<std::net::IpAddr>().is_err() {
        return Err("that address is on this computer or its network".to_owned());
    }
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        let local = match ip {
            std::net::IpAddr::V4(v4) => v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_unspecified() || v4.is_broadcast() || v4.octets()[0] == 100 && (64..128).contains(&v4.octets()[1]),
            std::net::IpAddr::V6(v6) => v6.is_loopback() || v6.is_unspecified() || (v6.segments()[0] & 0xfe00) == 0xfc00 || (v6.segments()[0] & 0xffc0) == 0xfe80,
        };
        if local {
            return Err("that address is on this computer or its network".to_owned());
        }
    }
    Ok(uri.to_owned())
}

/// Fetches a collection's image: no redirect, at most `MAX_IMAGE_BYTES`, a PNG decoded to pixels
/// here (`avatar::png_pixels`); nothing else is shown.
pub fn fetch_image(uri: &str) -> Result<Value, String> {
    let url = media_url(uri)?;
    let agent = ureq::AgentBuilder::new().redirects(0).timeout(Duration::from_secs(15)).build();
    let response = agent.get(&url).call().map_err(|error| match error {
        ureq::Error::Status(code, _) => format!("the image's host answered HTTP {code}"),
        ureq::Error::Transport(_) => "the image's host couldn't be reached".to_owned(),
    })?;
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut std::io::Read::take(response.into_reader(), MAX_IMAGE_BYTES as u64 + 1), &mut bytes).map_err(|_| "the image couldn't be read".to_owned())?;
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(format!("the image is larger than {} KiB, so it isn't loaded", MAX_IMAGE_BYTES / 1024));
    }
    crate::avatar::png_pixels(&bytes).ok_or_else(|| "it isn't a PNG image this wallet can show".to_owned())
}

// ── Sheets shown, by wallet and digest ──────────────────────────────────

/// The sheets this module planned, by `(wallet folder, digest)`, with when: an action starts only
/// from one of these (a served read may be forgotten, or not kept at all in unit tests).
static SHEETS: std::sync::Mutex<Vec<(String, String, Value, std::time::Instant)>> = std::sync::Mutex::new(Vec::new());

fn remember_sheet(wallet: &str, digest: &str, plan: &Value) {
    let mut sheets = SHEETS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    sheets.retain(|(w, d, _, at)| at.elapsed() < PLAN_VALID && !(w == wallet && d == digest));
    if sheets.len() >= 64 {
        sheets.remove(0);
    }
    sheets.push((wallet.to_owned(), digest.to_owned(), plan.clone(), std::time::Instant::now()));
}

fn sheet_planned(wallet: &str, digest: &str) -> Option<Value> {
    let sheets = SHEETS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    sheets.iter().find(|(w, d, _, at)| w == wallet && d == digest && at.elapsed() < PLAN_VALID).map(|(_, _, plan, _)| plan.clone())
}

fn forget_sheet(wallet: &str, digest: &str) {
    SHEETS.lock().unwrap_or_else(std::sync::PoisonError::into_inner).retain(|(w, d, _, _)| !(w == wallet && d == digest));
}

// ── The module's calls ──────────────────────────────────────────────────

impl Names {
    /// The network the NFT program list is kept for: the zone in use on the names network in use.
    pub(crate) fn nft_network(&self) -> String {
        format!("{}#{}", self.zones.current_id(), self.net.id())
    }

    pub(crate) fn nft_programs_listed(&self) -> Result<Vec<Value>, String> {
        let path = list_path().ok_or("no home folder to keep the NFT programs in")?;
        Ok(programs_in(&path, &self.nft_network())
            .into_iter()
            .map(|mut entry| {
                let label = entry["program"].as_str().and_then(crate::program_registry::label);
                entry["registry"] = provider::label_fields(label.as_ref());
                entry
            })
            .collect())
    }

    pub(crate) fn nft_program_change(&mut self, program: &str, label: &str, add: bool) -> Result<Vec<Value>, String> {
        let path = list_path().ok_or("no home folder to keep the NFT programs in")?;
        let network = self.nft_network();
        if add {
            add_program(&path, &network, program, label, provider::now_ms())?;
        } else {
            remove_program(&path, &network, program)?;
        }
        self.served.forget(KEY);
        self.nft_programs_listed()
    }

    /// A program of the list in use, or why it isn't one.
    fn nft_listed(&self, program: &str) -> Result<registry::AccountId, String> {
        let id = program_id(program)?;
        let text = registry::account_text(&id);
        let path = list_path().ok_or("no home folder to keep the NFT programs in")?;
        if !programs_in(&path, &self.nft_network()).iter().any(|entry| entry["program"] == text.as_str()) {
            return Err("that NFT program isn't in this network's list: add it under Collectibles, Programs".to_owned());
        }
        Ok(id)
    }

    /// Every listed program's holdings, each read on a thread of its own and answered at once
    /// with the last read: `{programs: [{program, registry, holdings, pending, collections,
    /// read_at, stale?, warning?, error?}]}`. Busy until each first read is back.
    pub(crate) fn nft_holdings_now(&mut self) -> String {
        let setup = match self.cli_setup().and_then(|setup| setup.wallet_ready().map(|()| setup)) {
            Ok(setup) => setup,
            Err(why) => return crate::reply(Err(why)),
        };
        let listed = match self.nft_programs_listed() {
            Ok(listed) => listed,
            Err(why) => return crate::reply(Err(why)),
        };
        let mut out = Vec::new();
        for entry in listed {
            let Some(program) = entry["program"].as_str().and_then(|text| registry::parse_account(text).ok()) else { continue };
            let key = self.served_key(KEY, &format!("holdings|{}|{}|{}", registry::account_text(&program), setup.wallet.display(), setup.sequencer));
            let reader = setup.clone();
            let snap = self.served.read(&key, NFT_EVERY, served::Lane::Cli, move || {
                let args = vec![program_arg(&program), "module".to_owned(), "nft".to_owned(), "holdings".to_owned()];
                parse_holdings(&cli::run_text(&reader, args, LIST_PATIENCE)?)
            });
            let mut row = json!({ "program": entry["program"], "label": entry["label"], "registry": entry["registry"], "read_at": snap.read_at, "updating": snap.updating });
            match snap.value_or("your NFTs") {
                Err(Ok(busy)) => return busy,
                Err(Err(why)) => row["error"] = json!(why),
                Ok(value) => {
                    for key in ["holdings", "pending", "collections"] {
                        row[key] = value[key].clone();
                    }
                    if snap.error.is_some() {
                        let marks = served::stale_marks(snap.error.as_deref().unwrap_or_default(), snap.age_ms(), "Your NFTs");
                        row["stale"] = marks["stale"].clone();
                        row["warning"] = marks["warning"].clone();
                    }
                }
            }
            out.push(row);
        }
        crate::reply(Ok(json!({ "programs": out })))
    }

    /// One collection's fields, read in the background and kept `NFT_EVERY`.
    pub(crate) fn nft_collection_now(&mut self, program: &str, definition: &str) -> String {
        let outcome = (|| {
            let program = self.nft_listed(program)?;
            let definition = registry::parse_account(definition.trim()).map_err(|_| "a collection is written as its definition's account ID".to_owned())?;
            let setup = self.cli_setup()?;
            let key = self.served_key(KEY, &format!("info|{}|{}", registry::account_text(&program), registry::account_text(&definition)));
            Ok(self.served.read(&key, NFT_EVERY, served::Lane::Cli, move || {
                let args = vec![program_arg(&program), "module".to_owned(), "nft".to_owned(), "info".to_owned(), "--".to_owned(), registry::account_text(&definition)];
                let answer: Value = serde_json::from_str(cli::last_line(&cli::run_text(&setup, args, INFO_PATIENCE)?)).map_err(|_| "the names CLI's collection did not decode".to_owned())?;
                Ok(collection(&answer))
            }))
        })();
        match outcome {
            Ok(snap) => served::reply_of(&snap, "The collection", |_| Value::Null),
            Err(why) => crate::reply(Err(why)),
        }
    }

    /// The sheet for an action: read in the background (the CLI syncs and reads the chain), with
    /// the registry's label of the program, the digest an action must name, and whether a proven
    /// one has a payer that can cover it.
    pub(crate) fn nft_sheet_now(&mut self, program: &str, action: &str) -> String {
        let outcome = (|| {
            let program = self.nft_listed(program)?;
            let action = Action::parse(action)?;
            let setup = self.cli_setup()?;
            setup.wallet_ready()?;
            let digest = digest(&program, &action);
            let key = self.served_key(KEY, &format!("plan|{digest}"));
            let input = action.json().to_string();
            let wallet_key = setup.wallet.display().to_string();
            let snap = self.served.read(&key, PLAN_VALID, served::Lane::Cli, move || {
                let args = vec![program_arg(&program), "module".to_owned(), "nft".to_owned(), "plan".to_owned()];
                parse_plan(&cli::run_with_input(&setup, args, &input, PLAN_PATIENCE)?)
            });
            Ok((program, action, digest, snap, wallet_key))
        })();
        match outcome {
            Err(why) => crate::reply(Err(why)),
            Ok((program, action, digest, snap, wallet_key)) => {
                if let Some(plan) = &snap.value {
                    remember_sheet(&wallet_key, &digest, plan);
                }
                let label = crate::program_registry::label(&registry::account_text(&program));
                served::reply_of(&snap, "The sheet", |plan| {
                    let label = provider::label_fields(label.as_ref());
                    let mut warnings = provider::label_warnings(&label, "NFT program");
                    if action.to_their_keys() {
                        warnings.push("This wallet can't check that their private account is ready for this collection; if it isn't, the NFT may never arrive.".to_owned());
                    }
                    if plan["collection"]["name"].is_null() {
                        warnings.push("The collection gives no readable name; check its ID.".to_owned());
                    }
                    json!({ "digest": digest, "program": registry::account_text(&program), "program_label": label, "warnings": warnings, "action_given": action.json() })
                })
            }
        }
    }

    /// Starts an action as a wallet job, only from a sheet planned for exactly it (`digest`)
    /// with nothing refused, and, when it is proven, with a private payer the sheet found able
    /// to cover the fee.
    pub(crate) fn nft_start_now(&mut self, program: &str, action: &str, digest_given: &str, fee_payer: &str) -> Result<Value, String> {
        let program = self.nft_listed(program)?;
        let action = Action::parse(action)?;
        let digest = digest(&program, &action);
        if digest != digest_given.trim() {
            return Err("this isn't the action the sheet showed; look at the sheet again".to_owned());
        }
        let wallet_key = self.cli_setup()?.wallet.display().to_string();
        let plan = sheet_planned(&wallet_key, &digest).ok_or("the sheet for this is gone or old; open it again")?;
        let plan = &plan;
        if let Some(why) = plan["refusal"].as_str() {
            return Err(why.to_owned());
        }
        let payer = if plan["proven"] == true {
            let payer = account(fee_payer).map_err(|_| "choose the private account that pays the fee".to_owned())?;
            if !plan["payers"].as_array().is_some_and(|payers| payers.iter().any(|row| row["account"] == payer.as_str() && row["enough"] == true)) {
                return Err("that account can't pay this fee: choose a private account the sheet lists as able to".to_owned());
            }
            Some(payer)
        } else {
            None
        };
        let args = action.args(&program, payer.as_deref())?;
        let kind = format!("nft_{}", action.action);
        let started = self.wallet_job(&kind, |_| Ok(args))?;
        // One sheet, one action: pressing again needs the sheet again.
        forget_sheet(&wallet_key, &digest);
        self.served.forget(&self.served_key(KEY, &format!("plan|{digest}")));
        Ok(started)
    }

    /// Takes the NFT credits waiting for this wallet's private accounts, paid by `fee_payer`.
    pub(crate) fn nft_receive_now(&mut self, program: &str, fee_payer: &str) -> Result<Value, String> {
        let program = self.nft_listed(program)?;
        let (private, payer) = provider::account_ref(fee_payer)?;
        if !private {
            return Err("receiving is a private transaction: a private account pays its fee".to_owned());
        }
        let args = vec![program_arg(&program), "nft".to_owned(), "receive".to_owned(), format!("--fee-payer={}", registry::account_text(&payer))];
        self.wallet_job("nft_receive", |_| Ok(args))
    }

    /// The person's choice to load a collection's image, kept across restarts.
    pub(crate) fn nft_media_set(&mut self, program: &str, definition: &str, allow: bool) -> Result<Value, String> {
        let program = registry::account_text(&self.nft_listed(program)?);
        let path = list_path().ok_or("no home folder")?;
        let allowed = set_media(&path, &program, definition, allow)?;
        self.served.forget(&self.served_key(KEY, &format!("image|{program}|")));
        Ok(json!({ "allowed": allowed }))
    }

    /// A collection's image, only when the person allowed it: fetched on a thread of its own.
    pub(crate) fn nft_image_now(&mut self, program: &str, definition: &str, uri: &str) -> String {
        let outcome = (|| {
            let program = registry::account_text(&self.nft_listed(program)?);
            let definition = registry::account_text(&registry::parse_account(definition.trim()).map_err(|_| "a collection is written as its definition's account ID".to_owned())?);
            let path = list_path().ok_or("no home folder")?;
            if !media_allowed(&path, &program, &definition) {
                return Ok(None);
            }
            let url = media_url(uri)?;
            let key = self.served_key(KEY, &format!("image|{program}|{definition}|{url}"));
            Ok(Some(self.served.read(&key, Duration::from_secs(3600), served::Lane::Own("nft-media".to_owned()), move || fetch_image(&url))))
        })();
        match outcome {
            Ok(None) => crate::reply(Ok(json!({ "allowed": false }))),
            Ok(Some(snap)) => served::reply_of(&snap, "The image", |_| Value::Null),
            Err(why) => crate::reply(Err(why)),
        }
    }

    /// After an NFT job ends: its outcome in the job's state, and what the wallet holds read again.
    pub(crate) fn nft_job_ended(&mut self, state: &mut Value, log: &str) {
        if let Some(fields) = outcome(log).as_object() {
            for (key, value) in fields {
                state[key.as_str()] = value.clone();
            }
        }
        if let Some(why) = state["refusal"].as_str().map(str::to_owned) {
            if state["state"] != "done" {
                state["problem"] = json!(why);
            }
        }
        self.served.forget(KEY);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROGRAM: &str = "ErycMYqXUUtducaYmpHZjRgdbLXchrMiV4NZRnmRDNmu";
    const ART: &str = "HA47ERcmfSbDChGcYfBcC4SKaPQ8pJJnv4wxCLu1ejL5";
    const BOB: &str = "HLEwXNBDYx6GS9H5CsawDrMTptD6R4XsJywFEMr6eim2";

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("names-nft-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("nft-programs.json")
    }

    #[test]
    fn the_program_list_is_per_network_bounded_and_kept() {
        let path = temp("list");
        assert!(programs_in(&path, "home#04").is_empty(), "ships empty");
        let list = add_program(&path, "home#04", PROGRAM, "Throwaway\u{202E} chain", 1).unwrap();
        assert_eq!(list, vec![json!({ "program": PROGRAM, "label": "Throwaway chain", "added": 1 })]);
        assert!(programs_in(&path, "zone2#04").is_empty(), "another network has its own list");
        // Adding it again keeps one entry.
        assert_eq!(add_program(&path, "home#04", PROGRAM, "", 2).unwrap().len(), 1);
        assert!(add_program(&path, "home#04", "not-an-id", "", 3).is_err());
        assert!(add_program(&path, "home#04", &registry::account_text(&registry::NATIVE_TOKEN), "", 3).is_err());
        for n in 1..MAX_PROGRAMS as u8 {
            add_program(&path, "home#04", &registry::account_text(&[n; 32]), "", 4).unwrap();
        }
        assert!(add_program(&path, "home#04", &registry::account_text(&[99; 32]), "", 5).unwrap_err().contains("at most"));
        assert_eq!(remove_program(&path, "home#04", PROGRAM).unwrap().len(), MAX_PROGRAMS - 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        // Images: off until allowed, per collection.
        assert!(!media_allowed(&path, PROGRAM, ART));
        set_media(&path, PROGRAM, ART, true).unwrap();
        assert!(media_allowed(&path, PROGRAM, ART));
        assert!(!media_allowed(&path, PROGRAM, BOB));
        set_media(&path, PROGRAM, ART, false).unwrap();
        assert!(!media_allowed(&path, PROGRAM, ART));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn actions_are_checked_and_written_one_way() {
        let init = Action::parse(&format!(r#"{{"action":"init","definition":"{ART}","new":"private"}}"#)).unwrap();
        assert_eq!(init.args(&registry::parse_account(PROGRAM).unwrap(), Some(&format!("Private/{BOB}"))).unwrap(), vec![
            format!("--nft-program={PROGRAM}"), "nft".into(), "init".into(), format!("--definition={ART}"), "--new=private".into(), format!("--fee-payer={BOB}"),
        ]);
        let send = Action::parse(&format!(r#"{{"action":"send","from":"Public/{BOB}","to":"Alice"}}"#)).unwrap();
        assert_eq!(send.to.as_deref(), Some("alice"));
        assert_eq!(send.args(&[7; 32], None).unwrap()[3..], [format!("--from={BOB}"), "--to=alice".to_owned()]);
        // A public payer for a proven one is refused.
        assert!(send.args(&[7; 32], Some(&format!("Public/{BOB}"))).is_err());
        // Someone's keys need the tick.
        let keys = format!("{}:{}", "ab".repeat(32), "cd".repeat(40));
        let unticked = Action::parse(&format!(r#"{{"action":"send","from":"Private/{BOB}","to":"{keys}"}}"#)).unwrap();
        assert!(unticked.args(&[7; 32], None).unwrap_err().contains("can't check"));
        let ticked = Action::parse(&format!(r#"{{"action":"send","from":"Private/{BOB}","to":"{keys}","their_keys":true}}"#)).unwrap();
        assert!(ticked.args(&[7; 32], None).unwrap().contains(&"--their-keys".to_owned()));
        // Refused before anything runs.
        for bad in [
            r#"{"action":"mint"}"#.to_owned(),
            format!(r#"{{"action":"send","from":"{BOB}","to":"alice"}}"#),
            format!(r#"{{"action":"send","from":"Public/{BOB}","to":"{BOB}"}}"#),
            format!(r#"{{"action":"send","from":"Public/{BOB}","to":"Public/{BOB}"}}"#),
            format!(r#"{{"action":"send","from":"Public/{BOB}","to":"--rpc=http://evil"}}"#),
            format!(r#"{{"action":"burn","from":"Public/{BOB}","amount":"5"}}"#),
            format!(r#"{{"action":"init","definition":"{ART}"}}"#),
            "[]".to_owned(),
        ] {
            assert!(Action::parse(&bad).is_err(), "{bad}");
        }
        // The digest follows the action and the program.
        let burn = Action::parse(&format!(r#"{{"action":"burn","from":"Public/{BOB}"}}"#)).unwrap();
        assert_ne!(digest(&[1; 32], &burn), digest(&[2; 32], &burn));
        assert_ne!(digest(&[1; 32], &burn), digest(&[1; 32], &send));
        assert_eq!(digest(&[1; 32], &burn), digest(&[1; 32], &Action::parse(&format!(r#"{{"from":" Public/{BOB} ","action":"burn"}}"#)).unwrap()));
    }

    #[test]
    fn holdings_are_read_back_cleaned_and_grouped() {
        let out = format!(
            "Latest block is 9\n{}",
            json!({
                "holdings": [
                    { "account": format!("Public/{BOB}"), "state": "copy", "holds": true, "definition": ART, "owned": true, "collection": { "definition": ART, "name": "Art\u{202E}x", "printable_supply": "3", "standard": "Simple", "uri": "ipfs://bafyart/meta", "creators": "me" } },
                    { "account": format!("Private/{PROGRAM}"), "state": "master", "holds": true, "definition": ART, "print_balance": "2", "collection": { "definition": ART, "name": "Art" } },
                    { "account": format!("Public/{ART}"), "state": "empty", "holds": false },
                    { "account": "Public/x", "state": "copy", "holds": true },
                    { "account": format!("Public/{ART}"), "state": "weird" },
                    { "account": format!("Public/{PROGRAM}"), "state": "copy", "holds": true, "definition": BOB, "owned": true, "collection": null },
                ],
                "pending": [ { "position": 5, "definition": ART, "value": "copy", "to": format!("Private/{BOB}") }, { "position": 6, "to": "nope" } ],
            })
        );
        let read = parse_holdings(&out).unwrap();
        assert_eq!(read["holdings"].as_array().unwrap().len(), 4, "the bad rows are dropped");
        assert_eq!(read["holdings"][0]["collection"]["name"], "Art x", "a direction change is cleaned out");
        assert_eq!(read["holdings"][1]["account_kind"], "private");
        assert_eq!(read["pending"].as_array().unwrap().len(), 1);
        let collections = read["collections"].as_array().unwrap();
        assert_eq!(collections.len(), 2);
        assert_eq!((collections[0]["definition"].as_str(), collections[0]["count"].as_u64()), (Some(ART), Some(2)));
        // A collection that can't be read is listed by its ID, as such.
        assert_eq!((collections[1]["definition"].as_str(), collections[1]["readable"].as_bool()), (Some(BOB), Some(false)));
        assert!(parse_holdings("Error: no NFT program given").is_err());
    }

    #[test]
    fn a_plan_and_a_job_say_refusals_in_plain_words() {
        let plan = parse_plan(&json!({ "action": "send", "refusal": "Guest panicked: recipient already holds an NFT", "proven": true, "payers": [{ "account": format!("Private/{BOB}"), "balance": "6000000", "enough": true }, { "account": "junk" }], "fee": { "about": "1", "at_most": "5070616" } }).to_string()).unwrap();
        assert_eq!(plan["refusal"], nft_wire::ALREADY_HOLDS);
        assert_eq!(plan["payers"].as_array().unwrap().len(), 1);
        assert_eq!(plan["fee"]["at_most"], "5070616");
        let ended = outcome("sending the NFT\nsubmitted ab12\napplied in block 7\n");
        assert_eq!((ended["tx"].as_str(), ended["block"].as_u64()), (Some("ab12"), Some(7)));
        let refused = outcome("Error: Program violated execution rules: A delivery named Actor { … }, which is neither a declared public actor nor private");
        assert_eq!(refused["refusal"], nft_wire::NOT_READY);
        assert_eq!(outcome("nothing to receive")["nothing"], true);
    }

    #[test]
    fn images_load_only_from_public_https_hosts() {
        for good in ["https://example.com/a.png", "https://cdn.example.org:8443/x?y", "https://8.8.8.8/i.png"] {
            assert!(media_url(good).is_ok(), "{good}");
        }
        for bad in [
            "http://example.com/a.png",
            "ipfs://bafyart/meta",
            "https://localhost/x",
            "https://127.0.0.1:3040/rpc",
            "https://10.0.0.5/x",
            "https://192.168.1.96/x",
            "https://169.254.1.1/x",
            "https://[::1]/x",
            "https://[fd00::1]/x",
            "https://user@example.com/x",
            "https://intranet/x",
            "https://100.118.15.0/x",
            "javascript:alert(1)",
        ] {
            assert!(media_url(bad).is_err(), "{bad}");
        }
    }
}
