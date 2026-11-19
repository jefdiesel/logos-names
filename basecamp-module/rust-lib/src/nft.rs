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
/// How long a plan read is reused before a sheet asks the chain again: seconds, so a sheet
/// opened again reads the state as it is now (review: a stale refusal or recipient).
pub const PLAN_EVERY: Duration = Duration::from_secs(5);
/// How long the person has to approve a sheet, measured from when its plan was read.
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
    let entry = &load(path)["media"][format!("{program}:{definition}")];
    *entry == true || entry["allowed"] == true
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
        media.insert(key, json!({ "allowed": true, "at": provider::now_ms() }));
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
    /// `init`, `send`, `print`, `burn`, or `receive` (the credits waiting for private accounts).
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

/// A nullifier public key, in hex: 32 bytes.
pub const NPK_HEX: usize = 64;
/// A viewing public key, in hex: an ML-KEM-768 encapsulation key of 1184 bytes
/// (`MlKem768EncapsulationKey::LEN`, lee_core v0.4.0-rc3; Job A §8c A2.2 "its 1,184-byte vpk").
pub const VPK_HEX: usize = 2 * 1184;

/// Where an NFT goes: an LNS name, `Public/<id>`, this wallet's `Private/<id>`, or someone's
/// receiving keys `<npk hex>:<vpk hex>`.
pub fn destination(text: &str) -> Result<String, String> {
    let text = text.trim();
    if text.starts_with("Public/") || text.starts_with("Private/") {
        return account(text);
    }
    if let Some((npk, vpk)) = text.split_once(':') {
        let hex = |part: &str| !part.is_empty() && part.len() % 2 == 0 && part.bytes().all(|b| b.is_ascii_hexdigit());
        if npk.len() == NPK_HEX && hex(npk) && vpk.len() == VPK_HEX && hex(vpk) {
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
            "receive" => {}
            _ => return Err("an NFT action is init, send, print, burn or receive".to_owned()),
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
            "receive" if fee_payer.is_none() => return Err("receiving is a private transaction: choose the private account that pays its fee".to_owned()),
            "receive" => {}
            _ => return Err("an NFT action is init, send, print, burn or receive".to_owned()),
        }
        // Which kind of account may pay is the sheet's to say (`Names::nft_start_now`).
        if let Some(payer) = fee_payer {
            let (_, id) = provider::account_ref(payer)?;
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
        "pin": plan["pin"].as_str().filter(|p| p.len() == 64 && p.bytes().all(|b| b.is_ascii_hexdigit())),
        "to_account": plan["to_account"].as_str().and_then(|to| destination(to).ok()),
        "kind": match plan["kind"].as_str() { Some(k @ ("copy" | "master")) => json!(k), _ => Value::Null },
        "left": left_rows(&plan["left"]),
    }))
}

/// Waiting credits that can't land, checked: `[{account, definition, code}]`, `code` one of
/// `not_ready`, `already_holds`, `other_collection`.
fn left_rows(value: &Value) -> Value {
    Value::from(
        value
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .take(32)
            .filter_map(|row| {
                let code = row["code"].as_str().filter(|c| ["not_ready", "already_holds", "other_collection"].contains(c))?;
                Some(json!({ "account": account(row["account"].as_str()?).ok()?, "definition": id_text(&row["definition"]), "code": code }))
            })
            .collect::<Vec<_>>(),
    )
}

/// What an ended NFT job says: its transaction and block, or the plain refusal.
pub fn outcome(log: &str) -> Value {
    let tx = log.lines().rev().find_map(|line| line.trim().strip_prefix("submitted ")).map(str::trim).map(str::to_owned);
    let blocks: Vec<u64> = log.lines().filter_map(|line| line.trim().strip_prefix("applied in block ")?.trim().parse().ok()).collect();
    let refusal = log.lines().rev().find_map(|line| line.trim().strip_prefix("Error: ")).map(|error| nft_wire::refusal(error).unwrap_or(error)).map(|why| provider::clean_text(why, 240));
    // The account an `init` made ready (`new account …` or `making … ready`): what the sender needs.
    let ready = log.lines().rev().find_map(|line| {
        let line = line.trim();
        line.strip_prefix("new account ").or_else(|| line.strip_prefix("making ").and_then(|rest| rest.split(" ready for ").next()))
    });
    let account = ready.and_then(|text| account(text.split_whitespace().next().unwrap_or_default()).ok());
    // Credits a receipt left waiting, from the CLI's machine line `left: <account> <collection> <code>`.
    let left: Vec<Value> = log
        .lines()
        .filter_map(|line| {
            let mut parts = line.trim().strip_prefix("left: ")?.split_whitespace();
            let (to, definition, code) = (parts.next()?, parts.next()?, parts.next()?);
            Some(json!({ "account": to, "definition": definition, "code": code }))
        })
        .collect();
    json!({ "tx": tx, "block": blocks.last(), "received": blocks.len(), "nothing": log.contains("nothing to receive"), "refusal": refusal, "account": account, "left": left_rows(&Value::from(left)) })
}

/// What giving out a private account's receiving keys reveals, in plain words (the app shows it
/// beside Copy).
pub const KEYS_REVEAL: &str = "Whoever has these keys can send NFTs and payments to this private account, and can work out its account ID. They can't spend from it or see what it holds. Everything sent to these keys can be told apart as going to the same account, and the keys can be passed on.";

/// `names module keys <id>` read back: the account's npk and vpk, checked, as one string.
pub fn parse_keys(output: &str, id: &registry::AccountId) -> Result<Value, String> {
    let answer: Value = serde_json::from_str(cli::last_line(output)).map_err(|_| "the names CLI's keys did not decode".to_owned())?;
    let hex = |key: &str, exact: Option<usize>| {
        answer[key].as_str().filter(|h| exact.is_none_or(|n| h.len() == n) && h.bytes().all(|b| b.is_ascii_hexdigit())).map(str::to_ascii_lowercase)
    };
    if answer["account"].as_str().and_then(|a| registry::parse_account(a).ok()) != Some(*id) {
        return Err("the names CLI gave the keys of another account".to_owned());
    }
    let (Some(npk), Some(vpk)) = (hex("npk", Some(NPK_HEX)), hex("vpk", Some(VPK_HEX))) else {
        return Err("the names CLI gave no receiving keys".to_owned());
    };
    Ok(json!({ "account": format!("Private/{}", registry::account_text(id)), "keys": format!("{npk}:{vpk}"), "reveals": KEYS_REVEAL }))
}

// ── Media: a collection's image, only when allowed ──────────────────────
//
// A collection's `uri` is its maker's text, so fetching it is a request the wallet makes on a
// stranger's say-so: to the person's own network (SSRF), to a slow or huge host, or for content
// that isn't an image. The rules, all enforced here and tested below:
// - https only, a plain host, no user part; at most `MAX_REDIRECTS` redirects, each one checked
//   again as if it were the first address;
// - every address the host name resolves to must be public (never loopback, private, link-local,
//   shared, multicast, documentation or unspecified, v4 or v6, v4-mapped included): the check is
//   the resolver ureq connects through, so the address checked is the address connected to;
// - a connect limit, a whole-request limit, and at most `MAX_IMAGE_BYTES` read;
// - `Content-Type: image/png` and the PNG signature, then the same PNG decoder as avatars, into
//   pixels; nothing else (SVG included) is ever shown, and nothing is rendered as a page;
// - no cookies (ureq's cookie store is not built in), no Referer (ureq sends none), a fixed
//   User-Agent that names nothing of the person's;
// - only after the person allowed it for that collection (`set_media`, recorded with when).

/// Redirects followed, each checked again.
pub const MAX_REDIRECTS: usize = 3;
const CONNECT_LIMIT: Duration = Duration::from_secs(5);
const REQUEST_LIMIT: Duration = Duration::from_secs(15);

/// The URL a collection's `uri` may be fetched from, or why not: https only, a host name or a
/// public address (never loopback, a private network or a link-local one), no user part.
pub fn media_url(uri: &str) -> Result<String, String> {
    let uri = uri.trim();
    let rest = uri.strip_prefix("https://").ok_or("only an https address is loaded; this one isn't")?;
    if uri.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err("that address has spaces or control characters in it".to_owned());
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.is_empty() || authority.contains('@') {
        return Err("that address has no plain host".to_owned());
    }
    let host = host_of(authority);
    if host == "localhost" || host.ends_with(".localhost") || host.ends_with(".local") || host.ends_with(".internal") || !host.contains('.') && host.parse::<std::net::IpAddr>().is_err() {
        return Err("that address is on this computer or its network".to_owned());
    }
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        if !public_ip(&ip) {
            return Err("that address is on this computer or its network".to_owned());
        }
    }
    Ok(uri.to_owned())
}

/// The host of an authority (`host`, `host:port`, `[v6]:port`), lower case, brackets removed.
fn host_of(authority: &str) -> String {
    let host = if let Some(rest) = authority.strip_prefix('[') {
        rest.split(']').next().unwrap_or_default()
    } else {
        authority.rsplit_once(':').filter(|(_, port)| port.bytes().all(|b| b.is_ascii_digit())).map_or(authority, |(host, _)| host)
    };
    host.to_ascii_lowercase()
}

/// Whether an address is on the public internet: none of loopback, private, link-local, shared
/// (100.64/10), multicast, broadcast, documentation, benchmarking or unspecified, nor the v6
/// forms of those (unique-local, link-local, v4-mapped and v4-compatible of a non-public v4).
pub fn public_ip(ip: &std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            let [a, b, c, _] = v4.octets();
            !(v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_multicast()
                || v4.is_documentation()
                || a == 0
                || a == 100 && (64..128).contains(&b)
                || a == 198 && (b == 18 || b == 19)
                || a == 192 && b == 0 && c == 0
                || a >= 240)
        }
        std::net::IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return public_ip(&std::net::IpAddr::V4(v4));
            }
            let first = v6.segments()[0];
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (first & 0xfe00) == 0xfc00
                || (first & 0xffc0) == 0xfe80
                || first == 0x2001 && v6.segments()[1] == 0x0db8
                || v6.segments()[..6].iter().all(|s| *s == 0))
        }
    }
}

/// The addresses a host may be reached at: all of them public, or none at all. ureq connects only
/// to what this returns, so a name that resolves to a private address is never connected to.
pub fn checked_addresses(netloc: &str, resolve: &dyn Fn(&str) -> std::io::Result<Vec<std::net::SocketAddr>>) -> std::io::Result<Vec<std::net::SocketAddr>> {
    let found = resolve(netloc)?;
    if found.is_empty() {
        return Err(std::io::Error::other("the image's host name has no address"));
    }
    if let Some(local) = found.iter().find(|addr| !public_ip(&addr.ip())) {
        return Err(std::io::Error::other(format!("the image's host name points at {}, which is on this computer or its network", local.ip())));
    }
    Ok(found)
}

/// Where a redirect goes, checked as a first address would be: an absolute https URL, or a path
/// on the same host.
pub fn redirect_target(current: &str, location: &str) -> Result<String, String> {
    let location = location.trim();
    let next = if location.starts_with("https://") {
        location.to_owned()
    } else if location.starts_with('/') && !location.starts_with("//") {
        let rest = current.strip_prefix("https://").ok_or("not an https address")?;
        let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
        format!("https://{authority}{location}")
    } else {
        return Err("the image's host redirected somewhere that isn't an https address".to_owned());
    };
    media_url(&next)
}

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

/// The pixels of a fetched body, only when it says it is a PNG and is one: `image/png`, the PNG
/// signature, and the avatar decoder's limits. SVG, HTML and every other type are refused.
pub fn png_payload(content_type: &str, bytes: &[u8]) -> Result<Value, String> {
    let kind = content_type.split(';').next().unwrap_or_default().trim().to_ascii_lowercase();
    if kind != "image/png" {
        return Err(format!("the host sent {} rather than a PNG image, so it isn't shown", if kind.is_empty() { "no content type".to_owned() } else { provider::clean_text(&kind, 40) }));
    }
    if !bytes.starts_with(&PNG_SIGNATURE) {
        return Err("the host said PNG but sent something else, so it isn't shown".to_owned());
    }
    crate::avatar::png_pixels(bytes).ok_or_else(|| "it isn't a PNG image this wallet can show (at most 1024 pixels a side)".to_owned())
}

/// Fetches a collection's image under the rules above, resolving names with `resolve`.
pub fn fetch_image_with(uri: &str, resolve: impl Fn(&str) -> std::io::Result<Vec<std::net::SocketAddr>> + Send + Sync + 'static) -> Result<Value, String> {
    let resolve = std::sync::Arc::new(resolve);
    let checked = std::sync::Arc::clone(&resolve);
    let agent = ureq::AgentBuilder::new()
        .redirects(0)
        .timeout_connect(CONNECT_LIMIT)
        .timeout(REQUEST_LIMIT)
        .user_agent("names-wallet")
        .resolver(move |netloc: &str| checked_addresses(netloc, &*checked))
        .build();
    let mut url = media_url(uri)?;
    for _ in 0..=MAX_REDIRECTS {
        let response = match agent.get(&url).set("Accept", "image/png").call() {
            Ok(response) => response,
            Err(ureq::Error::Status(code, _)) => return Err(format!("the image's host answered HTTP {code}")),
            Err(ureq::Error::Transport(transport)) => {
                let why = transport.to_string();
                return Err(if why.contains("on this computer or its network") || why.contains("has no address") {
                    "the image's host name points at this computer or its network, so it isn't loaded".to_owned()
                } else {
                    "the image's host couldn't be reached in time".to_owned()
                });
            }
        };
        if (300..400).contains(&response.status()) {
            let location = response.header("Location").unwrap_or_default().to_owned();
            url = redirect_target(&url, &location)?;
            continue;
        }
        if response.header("Content-Length").and_then(|n| n.parse::<u64>().ok()).is_some_and(|n| n > MAX_IMAGE_BYTES as u64) {
            return Err(format!("the image is larger than {} KiB, so it isn't loaded", MAX_IMAGE_BYTES / 1024));
        }
        let content_type = response.header("Content-Type").unwrap_or_default().to_owned();
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut std::io::Read::take(response.into_reader(), MAX_IMAGE_BYTES as u64 + 1), &mut bytes).map_err(|_| "the image couldn't be read in time".to_owned())?;
        if bytes.len() > MAX_IMAGE_BYTES {
            return Err(format!("the image is larger than {} KiB, so it isn't loaded", MAX_IMAGE_BYTES / 1024));
        }
        return png_payload(&content_type, &bytes);
    }
    Err(format!("the image's host redirected more than {MAX_REDIRECTS} times"))
}

/// Fetches a collection's image with the system's name resolution, checked.
pub fn fetch_image(uri: &str) -> Result<Value, String> {
    fetch_image_with(uri, |netloc: &str| std::net::ToSocketAddrs::to_socket_addrs(netloc).map(|found| found.collect()))
}

// ── Sheets shown, by wallet and digest ──────────────────────────────────

/// The sheets this module planned, by `(wallet folder, digest)`, with when: an action starts only
/// from one of these (a served read may be forgotten, or not kept at all in unit tests).
/// Each is stamped with when its plan was READ (seconds since 1970), never when it was last shown.
static SHEETS: std::sync::Mutex<Vec<(String, String, Value, u64)>> = std::sync::Mutex::new(Vec::new());

fn fresh(read_at: u64) -> bool {
    served::now_s().saturating_sub(read_at) < PLAN_VALID.as_secs()
}

fn remember_sheet(wallet: &str, digest: &str, plan: &Value, read_at: u64) {
    let mut sheets = SHEETS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    sheets.retain(|(w, d, _, at)| fresh(*at) && !(w == wallet && d == digest));
    if sheets.len() >= 64 {
        sheets.remove(0);
    }
    sheets.push((wallet.to_owned(), digest.to_owned(), plan.clone(), read_at));
}

fn sheet_planned(wallet: &str, digest: &str) -> Option<Value> {
    let sheets = SHEETS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    sheets.iter().find(|(w, d, _, at)| w == wallet && d == digest && fresh(*at)).map(|(_, _, plan, _)| plan.clone())
}

/// Actions started, by wallet and pin, so a late answer and a second press can't start the same
/// action twice (the token-move double-send class): a pin names the state it was planned on, so
/// once the first one ran, only a new plan can start another.
static STARTED: std::sync::Mutex<Vec<(String, String, u64)>> = std::sync::Mutex::new(Vec::new());

fn started_before(wallet: &str, pin: &str) -> bool {
    let mut started = STARTED.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    started.retain(|(_, _, at)| fresh(*at));
    started.iter().any(|(w, p, _)| w == wallet && p == pin)
}

/// A job ended (done or failed): what it was started from may be started again from a new plan.
fn clear_started(wallet: &str) {
    STARTED.lock().unwrap_or_else(std::sync::PoisonError::into_inner).retain(|(w, _, _)| w != wallet);
}

fn mark_started(wallet: &str, pin: &str) {
    STARTED.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push((wallet.to_owned(), pin.to_owned(), served::now_s()));
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
    pub(crate) fn nft_listed(&self, program: &str) -> Result<registry::AccountId, String> {
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
            let snap = self.served.read(&key, PLAN_EVERY, served::Lane::Cli, move || {
                let args = vec![program_arg(&program), "module".to_owned(), "nft".to_owned(), "plan".to_owned()];
                parse_plan(&cli::run_with_input(&setup, args, &input, PLAN_PATIENCE)?)
            });
            Ok((program, action, digest, snap, wallet_key))
        })();
        match outcome {
            Err(why) => crate::reply(Err(why)),
            Ok((program, action, digest, snap, wallet_key)) => {
                // Only a plan read now is shown and remembered: an old one while a newer read is on
                // its way, or one whose last read failed, is never shown as current.
                if snap.value.is_some() && (snap.updating || snap.error.is_some()) {
                    return served::still_reading("the sheet");
                }
                if let (Some(plan), Some(read_at)) = (&snap.value, snap.read_at) {
                    remember_sheet(&wallet_key, &digest, plan, read_at);
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

    /// Forgets a sheet's plan, so the next opening plans afresh (called when a sheet opens and closes).
    pub(crate) fn nft_sheet_forget(&mut self, program: &str, action: &str) -> Result<Value, String> {
        let program = program_id(program)?;
        let action = Action::parse(action)?;
        let digest = digest(&program, &action);
        let wallet_key = self.cli_setup()?.wallet.display().to_string();
        forget_sheet(&wallet_key, &digest);
        self.served.forget(&self.served_key(KEY, &format!("plan|{digest}")));
        Ok(json!({ "forgot": true }))
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
        // The payer: one the sheet listed as able to pay. A proven action needs one (a private
        // account, never the wallet's own pick); a public one without one is paid by its signer.
        let proven = plan["proven"] == true;
        let payer = match (fee_payer.trim(), proven) {
            ("", true) => return Err("choose the private account that pays the fee".to_owned()),
            ("", false) => None,
            // The signer paying for itself is the plain public case: no separate payer is passed.
            (given, false) if action.from.as_deref() == Some(given) => None,
            (given, _) => {
                let payer = account(given).map_err(|_| "choose an account the sheet lists to pay the fee".to_owned())?;
                if payer.starts_with("Private/") != proven || !plan["payers"].as_array().is_some_and(|payers| payers.iter().any(|row| row["account"] == payer.as_str() && row["enough"] == true)) {
                    return Err("that account can't pay this fee: choose one the sheet lists as able to".to_owned());
                }
                Some(payer)
            }
        };
        // What the sheet showed is what runs: the recipient the plan resolved (a name is not read
        // again), and the state it was planned on, which the CLI checks again just before it sends
        // (`--expect-state`), refusing on any difference.
        let pin = plan["pin"].as_str().filter(|p| p.len() == 64 && p.bytes().all(|b| b.is_ascii_hexdigit())).ok_or("the sheet didn't say what it was planned on; open it again")?.to_owned();
        if started_before(&wallet_key, &pin) {
            return Err("this was already started from this sheet; look at its result before doing it again".to_owned());
        }
        let mut pinned = action.clone();
        if let Some(resolved) = plan["to_account"].as_str() {
            pinned.to = Some(destination(resolved)?);
        }
        let mut args = pinned.args(&program, payer.as_deref())?;
        args.push(format!("--expect-state={pin}"));
        let kind = format!("nft_{}", action.action);
        let started = self.wallet_job(&kind, |_| Ok(args))?;
        mark_started(&wallet_key, &pin);
        // One sheet, one action: pressing again needs the sheet again.
        forget_sheet(&wallet_key, &digest);
        self.served.forget(&self.served_key(KEY, &format!("plan|{digest}")));
        Ok(started)
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

    /// A private account's receiving keys, for a sender outside the wallet (`names module keys`),
    /// read on a thread of its own: `{account, keys: "<npk hex>:<vpk hex>", reveals}`.
    pub(crate) fn nft_keys_now(&mut self, account_text: &str) -> String {
        let outcome = (|| {
            let (private, id) = provider::account_ref(account_text)?;
            if !private {
                return Err("a public account is given as itself (Public/<id>); only a private one has receiving keys".to_owned());
            }
            let setup = self.cli_setup()?;
            setup.wallet_ready()?;
            let key = self.served_key(KEY, &format!("keys|{}|{}", registry::account_text(&id), setup.wallet.display()));
            Ok(self.served.read(&key, Duration::from_secs(3600), served::Lane::Cli, move || {
                let args = vec!["module".to_owned(), "keys".to_owned(), registry::account_text(&id)];
                parse_keys(&cli::run_text(&setup, args, INFO_PATIENCE)?, &id)
            }))
        })();
        match outcome {
            Ok(snap) => served::reply_of(&snap, "The receiving keys", |_| Value::Null),
            Err(why) => crate::reply(Err(why)),
        }
    }

    /// After an NFT job ends: its outcome in the job's state, and what the wallet holds read again.
    pub(crate) fn nft_job_ended(&mut self, state: &mut Value, log: &str) {
        // One wallet job at a time: once this one ended, a retry from a new plan may start.
        if let Ok(setup) = self.cli_setup() {
            clear_started(&setup.wallet.display().to_string());
        }
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
        assert!(load(&path)["media"][format!("{PROGRAM}:{ART}")]["at"].as_u64().is_some(), "the consent is recorded with when");
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
        // Receive needs a payer; any action takes one (its kind is the sheet's to check).
        let receive = Action::parse(r#"{"action":"receive"}"#).unwrap();
        assert!(receive.args(&[7; 32], None).is_err());
        assert_eq!(receive.args(&[7; 32], Some(&format!("Private/{BOB}"))).unwrap()[2..], ["receive".to_owned(), format!("--fee-payer={BOB}")]);
        assert!(send.args(&[7; 32], Some(&format!("Public/{BOB}"))).unwrap().contains(&format!("--fee-payer={BOB}")));
        // Someone's keys need the tick.
        let keys = format!("{}:{}", "ab".repeat(32), "cd".repeat(1184));
        // A vpk of any other length is not one.
        for short in [40, 256, 1183, 1185] {
            assert!(destination(&format!("{}:{}", "ab".repeat(32), "cd".repeat(short))).is_err(), "{short}");
        }
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
    fn a_sheet_is_good_for_minutes_from_its_read_and_an_action_starts_once() {
        let now = served::now_s();
        remember_sheet("w-test", "d1", &json!({ "pin": "a" }), now - 30);
        assert!(sheet_planned("w-test", "d1").is_some());
        // Stamped with its read time: a plan read 11 minutes ago is gone, however recently shown.
        remember_sheet("w-test", "d2", &json!({ "pin": "b" }), now - 660);
        assert!(sheet_planned("w-test", "d2").is_none());
        forget_sheet("w-test", "d1");
        assert!(sheet_planned("w-test", "d1").is_none());
        assert!(!started_before("w-test", "p1"));
        mark_started("w-test", "p1");
        assert!(started_before("w-test", "p1"));
        assert!(!started_before("w-other", "p1"));
        // When the job ends (a failure included), a retry from a new plan may start.
        clear_started("w-test");
        assert!(!started_before("w-test", "p1"));
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
        // A receipt that left a credit waiting says which and why, never a bare 0.
        let left = outcome(&format!("left: Private/{BOB} {ART} not_ready\nleft waiting, the NFT for Private/{BOB}: That account isn't ready"));
        assert_eq!((left["received"].as_u64(), left["left"][0]["code"].as_str()), (Some(0), Some("not_ready")), "{left}");
        assert_eq!(left["left"][0]["account"], format!("Private/{BOB}"));
        // The account an init made ready, for the sender.
        assert_eq!(outcome(&format!("new account Private/{BOB}\nmaking Private/{BOB} ready for Art ({ART})\nsubmitted ab\napplied in block 3"))["account"], format!("Private/{BOB}"));
        assert_eq!(outcome(&format!("making Public/{BOB} ready for Art ({ART})\nsubmitted ab"))["account"], format!("Public/{BOB}"));
    }

    #[test]
    fn a_host_name_is_reached_only_at_public_addresses_and_redirects_are_checked_again() {
        use std::net::{IpAddr, SocketAddr};
        let at = |ips: &'static [&'static str]| move |_: &str| Ok(ips.iter().map(|ip| SocketAddr::new(ip.parse::<IpAddr>().unwrap(), 443)).collect::<Vec<_>>());
        assert!(checked_addresses("img.example.com:443", &at(&["93.184.216.34"])).is_ok());
        for local in [&["127.0.0.1"][..], &["93.184.216.34", "10.1.2.3"], &["192.168.1.96"], &["169.254.169.254"], &["::1"], &["fd00::5"], &["fe80::1"], &["::ffff:127.0.0.1"], &["100.118.15.0"], &["0.0.0.0"]] {
            let err = checked_addresses("evil.example.com:443", &at(local)).unwrap_err().to_string();
            assert!(err.contains("on this computer or its network"), "{local:?}: {err}");
        }
        assert!(checked_addresses("nowhere.example:443", &at(&[])).is_err());
        // A rebinding name fails before any connection: the fetch refuses on the resolver's word.
        let refused = fetch_image_with("https://rebind.example.com/a.png", |_: &str| Ok(vec![SocketAddr::new("127.0.0.1".parse().unwrap(), 443)])).unwrap_err();
        assert!(refused.contains("this computer or its network"), "{refused}");
        // Redirects: absolute https or a same-host path, each checked again.
        assert_eq!(redirect_target("https://img.example.com/a.png", "/b.png").unwrap(), "https://img.example.com/b.png");
        assert_eq!(redirect_target("https://img.example.com/a.png", "https://cdn.example.org/c.png").unwrap(), "https://cdn.example.org/c.png");
        for bad in ["http://img.example.com/x.png", "https://127.0.0.1/x", "//evil.example.com/x", "b.png", "file:///etc/passwd", "https://10.0.0.1/admin"] {
            assert!(redirect_target("https://img.example.com/a.png", bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn receiving_keys_are_read_back_checked() {
        let id = registry::parse_account(BOB).unwrap();
        // Real sizes: a 32-byte npk and an ML-KEM-768 vpk of 1184 bytes.
        let out = format!("Latest block is 9\n{}", json!({ "account": BOB, "npk": "AB".repeat(32), "vpk": "cd".repeat(1184), "fresh": false }));
        let keys = parse_keys(&out, &id).unwrap();
        assert_eq!(keys["keys"], format!("{}:{}", "ab".repeat(32), "cd".repeat(1184)));
        assert!(parse_keys(&json!({ "account": BOB, "npk": "ab".repeat(32), "vpk": "cd".repeat(256) }).to_string(), &id).is_err(), "a short vpk");
        assert_eq!(keys["account"], format!("Private/{BOB}"));
        assert!(keys["reveals"].as_str().unwrap().contains("can't spend"));
        assert!(parse_keys(&json!({ "account": ART, "npk": "ab".repeat(32), "vpk": "cd" }).to_string(), &id).is_err(), "another account's keys");
        assert!(parse_keys(&json!({ "account": BOB, "npk": "ab", "vpk": "cd" }).to_string(), &id).is_err(), "a short npk");
    }

    #[test]
    fn only_a_real_png_is_shown() {
        let png = {
            let mut bytes = Vec::new();
            let mut encoder = png::Encoder::new(&mut bytes, 2, 2);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.write_header().unwrap().write_image_data(&[255; 16]).unwrap();
            bytes
        };
        assert_eq!(png_payload("image/png", &png).unwrap()["width"], 2);
        assert_eq!(png_payload("image/PNG; charset=binary", &png).unwrap()["height"], 2);
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" onload="alert(1)"/>"#;
        assert!(png_payload("image/svg+xml", svg).unwrap_err().contains("rather than a PNG"));
        assert!(png_payload("image/png", svg).unwrap_err().contains("said PNG"));
        assert!(png_payload("text/html", &png).is_err());
        assert!(png_payload("", &png).unwrap_err().contains("no content type"));
        let mut truncated = png.clone();
        truncated.truncate(20);
        assert!(png_payload("image/png", &truncated).is_err());
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
            "https://[::ffff:10.0.0.1]/x",
            "https://example.com/a b.png",
            "javascript:alert(1)",
        ] {
            assert!(media_url(bad).is_err(), "{bad}");
        }
    }
}
