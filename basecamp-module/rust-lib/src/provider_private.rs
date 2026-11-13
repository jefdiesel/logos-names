//! The Wallet Provider SDK's private side (docs/wallet/BRIDGE-SDK.md): what lets a mini-app
//! such as the RFP-022 Ethereum bridge use the person's private account without holding a key.
//!
//! - **App secrets** (`app_secret`): values the wallet derives for the calling app from one of
//!   the person's accounts (`app_secret.rs`). The first use of each (app, account, scope) is
//!   approved in the Names app; the grant can be taken back.
//! - **Private calls** (`propose_private_call`): a privacy-preserving transaction from the
//!   person's private account across several programs. The wallet never trusts the app's
//!   description: the names CLI simulates the call against the chain and reports what really
//!   changes (`names module private-call plan`), and this file turns that report into the
//!   approval sheet and refuses what the report can't explain or what takes more than the app
//!   declared.
//! - **Token sources** (`propose_token_source`): an app tells the wallet about a token program
//!   and its definitions (name, symbol, decimals), so the holdings view lists them.
//!
//! Everything here is pure: the module (`lib.rs`) runs the CLI and keeps the store.

use crate::provider::{self, account_ref, account_written, clean_text, short};
use crate::registry::{self, AccountId};
use serde_json::{json, Map, Value};

/// What a request can be, besides `provider`'s kinds.
pub const PRIVATE_CALL: &str = "private_call";
pub const SECRET: &str = "secret";
pub const TOKEN_SOURCE: &str = "token_source";

/// Programs one private call may run (the root's first).
pub const MAX_PROGRAMS: usize = 4;
/// Actors a private call reaches besides its root.
pub const MAX_MENTIONS: usize = 16;
/// One-time recipient openings a private call may declare.
pub const MAX_OPENINGS: usize = 4;
/// A private call's root message. A bridge claim through an RFP-021 attestation carries a
/// 32-sibling Merkle path (1,028 bytes), the execution header RLP (682 bytes on Sepolia) and
/// an `eth_getProof` account and storage proof (9 + 3 nodes on the v2 demo): about 7 KB, and
/// measured at 6,595 bytes for the v2 demo's leaf 1 (docs/wallet/BRIDGE-SDK.md). Mainnet
/// proofs run to about 12 account and 8 storage nodes of at most 532 bytes, about 12.5 KB in
/// all, so 16 KiB holds any honest claim while four waiting requests per app stay under
/// 128 KiB of hex in `provider.json`.
pub const MAX_PRIVATE_MESSAGE: usize = 16 * 1024;
/// Declared transfers per direction.
pub const MAX_DECLARED: usize = 8;
/// Apps' secret grants in all, and per app.
pub const MAX_SECRET_GRANTS: usize = 256;
pub const MAX_SECRET_GRANTS_PER_APP: usize = 16;
/// Programs an app may be remembered for.
pub const MAX_PROGRAM_GRANTS_PER_APP: usize = 16;
/// Token sources the wallet keeps, and definitions per source.
pub const MAX_TOKEN_SOURCES: usize = 16;
pub const MAX_SOURCE_DEFINITIONS: usize = 16;
/// `app_secret` calls an app may make in an hour once granted (each one runs the CLI).
pub const MAX_SECRET_CALLS_PER_HOUR: usize = 120;

/// Proving a private bridge claim, measured on an Apple M4 with 16 GB, 4 threads, the CPU
/// prover (rfp-022 HANDOFF §3: 666-686 s, peak footprint 9,374-9,550 MB). Shown before
/// approval; a call of another shape takes about as long, since the circuit dominates.
pub const PROVE_EXPECT: &str = "Proving runs on this computer: about 11 minutes and about 9.5 GB of memory (measured for a bridge claim on an Apple M4 with 16 GB, 4 threads). Other apps may be slow meanwhile; you can pause or cancel it until it is sent.";

/// A program ID in base58, `Public/` allowed.
fn program_id(text: &str) -> Result<AccountId, String> {
    let text = text.trim();
    let id = registry::parse_account(text.strip_prefix("Public/").unwrap_or(text))?;
    if id == registry::NATIVE_TOKEN {
        return Err("the native token isn't called directly; the wallet moves LEZ itself".to_owned());
    }
    Ok(id)
}

fn hex32(text: &str, what: &str) -> Result<[u8; 32], String> {
    registry::unhex(text.trim()).ok().and_then(|bytes| <[u8; 32]>::try_from(bytes).ok()).ok_or_else(|| format!("{what} is 32 bytes of hex"))
}

/// An amount as decimal text, up to 2^128 - 1, not zero.
fn amount(text: &str) -> Result<u128, String> {
    let digits: String = text.trim().chars().filter(|c| *c != ',').collect();
    match digits.parse::<u128>() {
        Ok(0) | Err(_) => Err(format!("{} is not an amount (decimal base units, more than 0)", clean_text(text, 40))),
        Ok(value) => Ok(value),
    }
}

/// `LEZ` or a token definition's ID, as an asset key: `LEZ` or `<definition b58>`.
fn asset_key(text: &str) -> Result<String, String> {
    match provider::asset(text)? {
        provider::Asset::Native => Ok(provider::ASSET.to_owned()),
        provider::Asset::Token(id) => Ok(registry::account_text(&id)),
    }
}

/// `1234567` → `1,234,567`.
pub fn grouped(value: u128) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (at, c) in digits.chars().enumerate() {
        if at > 0 && (digits.len() - at) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// The asks of `declared`: `{debits: [{asset, amount}], credits: [...], max_fee}`, as a map
/// asset → amount per direction, and the fee cap.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Declared {
    pub debits: Vec<(String, u128)>,
    pub credits: Vec<(String, u128)>,
    pub max_fee: Option<u128>,
}

impl Declared {
    fn read(value: &Value) -> Result<Self, String> {
        let list = |key: &str| -> Result<Vec<(String, u128)>, String> {
            let entries = value[key].as_array().map(Vec::as_slice).unwrap_or_default();
            if entries.len() > MAX_DECLARED {
                return Err(format!("declare at most {MAX_DECLARED} {key}"));
            }
            let mut out: Vec<(String, u128)> = Vec::new();
            for entry in entries {
                let asset = asset_key(entry["asset"].as_str().unwrap_or_default())?;
                let amount = amount(entry["amount"].as_str().ok_or("a declared amount is decimal text")?)?;
                match out.iter_mut().find(|(key, _)| *key == asset) {
                    Some((_, sum)) => *sum = sum.checked_add(amount).ok_or("the declared amounts overflow")?,
                    None => out.push((asset, amount)),
                }
            }
            Ok(out)
        };
        let max_fee = match &value["max_fee"] {
            Value::Null => None,
            other => Some(amount(other.as_str().ok_or("max_fee is decimal text")?)?),
        };
        Ok(Self { debits: list("debits")?, credits: list("credits")?, max_fee })
    }

    fn json(&self) -> Value {
        let list = |entries: &[(String, u128)]| Value::from(entries.iter().map(|(asset, amount)| json!({ "asset": asset, "amount": amount.to_string() })).collect::<Vec<_>>());
        json!({ "debits": list(&self.debits), "credits": list(&self.credits), "max_fee": self.max_fee.map(|fee| fee.to_string()) })
    }

    pub fn debit_of(&self, asset: &str) -> u128 {
        self.debits.iter().find(|(key, _)| key == asset).map_or(0, |(_, amount)| *amount)
    }
}

/// One actor a private call reaches, as the app wrote it, checked: the person's own account
/// (`{"actor": "account", "program"}`), a public account (`{"account", "program"}`) or a PDA
/// (`{"seed", "of", "program"}`). Programs default to the call's root program. Only the
/// person's own account is private; the wallet signs nothing public in a private call.
fn mention(entry: &Value, programs: &[AccountId]) -> Result<Value, String> {
    let fields = entry.as_object().ok_or("each account is an object: {\"actor\": \"account\", \"program\"}, {\"account\", \"program\"} or {\"seed\", \"of\", \"program\"}")?;
    for key in fields.keys() {
        if !matches!(key.as_str(), "actor" | "account" | "seed" | "of" | "program" | "openings") {
            return Err(format!("an account entry has no field {}", clean_text(key, 20)));
        }
    }
    let program = match fields.get("program").and_then(Value::as_str) {
        Some(text) if text.trim() == "native" || text.trim() == "LEZ" => registry::NATIVE_TOKEN,
        Some(text) => {
            let id = program_id(text)?;
            if !programs.contains(&id) {
                return Err(format!("{} is not one of the call's programs", short(&registry::account_text(&id))));
            }
            id
        }
        None => programs[0],
    };
    let program_text = if program == registry::NATIVE_TOKEN { "native".to_owned() } else { registry::account_text(&program) };
    if fields.get("actor").and_then(Value::as_str) == Some("account") {
        if fields.contains_key("account") || fields.contains_key("seed") {
            return Err("the person's own account is written {\"actor\": \"account\", \"program\"} only".to_owned());
        }
        return Ok(json!({ "actor": "account", "program": program_text, "openings": fields.get("openings").and_then(Value::as_bool).unwrap_or(false) }));
    }
    if fields.contains_key("openings") {
        return Err("openings belong to the person's own account only".to_owned());
    }
    match (fields.get("account").and_then(Value::as_str), fields.get("seed").and_then(Value::as_str)) {
        (Some(account), None) => {
            if account.trim().starts_with("Private/") {
                return Err("a private call reaches one private account: the person's own, written {\"actor\": \"account\"}".to_owned());
            }
            let id = registry::parse_account(account.trim().strip_prefix("Public/").unwrap_or(account.trim()))?;
            Ok(json!({ "account": registry::account_text(&id), "program": program_text }))
        }
        (None, Some(seed)) => {
            let seed = hex32(seed, "a seed")?;
            let of = program_id(fields.get("of").and_then(Value::as_str).ok_or("a PDA names its program: {\"seed\", \"of\"}")?)?;
            if !programs.contains(&of) {
                return Err("a PDA belongs to one of the call's programs".to_owned());
            }
            let account = registry::pda(&of, &seed);
            Ok(json!({ "seed": registry::hex(&seed), "of": registry::account_text(&of), "account": registry::account_text(&account), "program": program_text }))
        }
        _ => Err("each account entry has \"account\" or \"seed\" (with \"of\"), or is the person's own (\"actor\": \"account\")".to_owned()),
    }
}

/// A request to build, prove and send a privacy-preserving call from `account`, one of the
/// person's private accounts the app was given. `call_json` is the call (see
/// docs/wallet/PROVIDER-SDK.md, "Private calls"). Checked here; what it does is learnt only
/// from the CLI's simulation, which the module adds as `report` (see [`review`]).
pub fn private_call_request(app: &str, account: &str, call_json: &str, summary: &str, now: u64) -> Result<Value, String> {
    let (private, id) = account_ref(account)?;
    if !private {
        return Err("a private call is made from a private account (Private/<id>); use propose_call for a public one".to_owned());
    }
    if call_json.len() > MAX_PRIVATE_MESSAGE * 2 + 8 * 1024 {
        return Err("the call is too long".to_owned());
    }
    let call: Value = serde_json::from_str(call_json).map_err(|error| format!("the call is not JSON: {error}"))?;
    let fields = call.as_object().ok_or("the call is a JSON object")?;
    for key in fields.keys() {
        if !matches!(key.as_str(), "programs" | "accounts" | "message" | "openings" | "fee_payer" | "declared" | "reveal_account") {
            return Err(format!("a call has no field {}", clean_text(key, 20)));
        }
    }
    let programs: Vec<AccountId> = call["programs"]
        .as_array()
        .ok_or("the call lists its programs, the root's first")?
        .iter()
        .map(|entry| program_id(entry.as_str().unwrap_or_default()))
        .collect::<Result<_, _>>()?;
    if programs.is_empty() || programs.len() > MAX_PROGRAMS {
        return Err(format!("a private call runs 1 to {MAX_PROGRAMS} programs"));
    }
    if (1..programs.len()).any(|at| programs[..at].contains(&programs[at])) {
        return Err("a program is listed twice".to_owned());
    }
    let listed = call["accounts"].as_array().map(Vec::as_slice).unwrap_or_default();
    if listed.len() > MAX_MENTIONS {
        return Err(format!("a private call reaches at most {MAX_MENTIONS} accounts besides its root"));
    }
    let mentions: Vec<Value> = listed.iter().map(|entry| mention(entry, &programs)).collect::<Result<_, _>>()?;
    let message = registry::unhex(call["message"].as_str().ok_or("the call's message is hex")?.trim()).map_err(|_| "the call's message is hex".to_owned())?;
    if message.is_empty() {
        return Err("the message is empty".to_owned());
    }
    if message.len() > MAX_PRIVATE_MESSAGE {
        return Err(format!("the message is at most {MAX_PRIVATE_MESSAGE} bytes"));
    }
    let openings: Vec<[u8; 32]> = call["openings"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|entry| hex32(entry.as_str().unwrap_or_default(), "an opening"))
        .collect::<Result<_, _>>()?;
    if openings.len() > MAX_OPENINGS {
        return Err(format!("declare at most {MAX_OPENINGS} one-time addresses"));
    }
    if mentions.iter().any(|m| m["openings"] == true) && openings.is_empty() {
        return Err("an account entry asks for openings, but the call declares none".to_owned());
    }
    let fee_payer = match call["fee_payer"].as_str().unwrap_or("account") {
        "account" => "account",
        "wallet" => "wallet",
        other => return Err(format!("fee_payer is \"account\" or \"wallet\", not {}", clean_text(other, 20))),
    };
    let declared = Declared::read(&call["declared"])?;
    let mut request = Map::new();
    let id_text = registry::hex(&registry::fresh_bytes::<16>()?);
    request.insert("id".into(), json!(id_text));
    request.insert("app".into(), json!(app));
    request.insert("kind".into(), json!(PRIVATE_CALL));
    request.insert("state".into(), json!("pending"));
    request.insert("created".into(), json!(now));
    request.insert("expires".into(), json!(now + provider::PENDING_FOR_MS));
    request.insert("account".into(), json!(account_written(true, &id)));
    request.insert("programs".into(), json!(programs.iter().map(registry::account_text).collect::<Vec<_>>()));
    request.insert("mentions".into(), Value::from(mentions));
    request.insert("message".into(), json!(registry::hex(&message)));
    request.insert("message_len".into(), json!(message.len()));
    request.insert("openings".into(), json!(openings.iter().map(|o| registry::hex(o)).collect::<Vec<_>>()));
    request.insert("fee_payer".into(), json!(fee_payer));
    request.insert("declared".into(), declared.json());
    request.insert("reveal_account".into(), json!(call["reveal_account"] == true));
    request.insert("app_summary".into(), json!(clean_text(summary, provider::MAX_TEXT)));
    // Filled by the CLI's simulation (`check`: running, ready, refused or failed).
    request.insert("check".into(), json!("running"));
    request.insert("prove_expect".into(), json!(PROVE_EXPECT));
    Ok(Value::Object(request))
}

/// What the names CLI gets for `names module private-call`: the call, as the request keeps it.
/// `token_programs` are the programs whose states the CLI reads as token holdings: the
/// wallet's own token program and the token sources'.
pub fn cli_input(request: &Value, expect_digest: Option<&str>, token_programs: &[String]) -> Value {
    json!({
        "token_programs": token_programs,
        "account": request["account"],
        "programs": request["programs"],
        "accounts": request["mentions"],
        "message": request["message"],
        "openings": request["openings"],
        "fee_payer": request["fee_payer"],
        "expect_digest": expect_digest,
    })
}

/// The approval sheet for a private call, from the CLI's simulation report, and whether the
/// wallet refuses it. The report says what the call changes, measured by running it against
/// the chain with the person's own state (names CLI `private_call.rs`):
///
/// ```json
/// {"account": "Private/<id>", "digest": "<hex>", "public_cycles": 81568,
///  "fee": {"about": "…", "at_most": "…", "payer": "Private/<id>"},
///  "own": [{"asset": "LEZ", "before": "0", "after": "20000000"},
///          {"asset": "token", "program": "<b58>", "definition": "<b58>", "before": "0", "after": "100000000", "name": "…"},
///          {"asset": "data", "program": "<b58>", "before_empty": true, "changed": false}],
///  "public": [{"account": "<b58>", "program": "<b58>", "kind": "native", "before": "…", "after": "…"},
///             {"account": "<b58>", "program": "<b58>", "kind": "token_definition", "definition": "<b58>", "before": "…", "after": "…"},
///             {"account": "<b58>", "program": "<b58>", "kind": "token_holding", "definition": "<b58>", "before": "…", "after": "…"},
///             {"account": "<b58>", "program": "<b58>", "kind": "data", "before_empty": false, "changed": true}],
///  "aliases": ["<b58>"], "events": 1, "account_public": false,
///  "public_mentions": ["<b58>"]}
/// ```
///
/// `labels` maps a program ID to what the Program Registry says of it (`provider::label_fields`
/// input), `token_names` a definition ID to the name a registered token source gives it, and
/// `remembered` the programs this app was allowed before.
pub fn review(request: &Value, report: &Value, labels: &Map<String, Value>, token_names: &Map<String, Value>, remembered: &[String]) -> Value {
    let declared = Declared::read(&request["declared"]).unwrap_or_default();
    let account = request["account"].as_str().unwrap_or_default();
    let mut lines: Vec<String> = Vec::new();
    let mut refusals: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut public_lines: Vec<String> = Vec::new();

    if report["account"].as_str() != Some(account) {
        refusals.push("The simulation is of another account than the one asked for.".to_owned());
    }
    let number = |value: &Value| value.as_str().and_then(|text| text.parse::<u128>().ok());
    let token_label = |definition: &str, name: Option<&str>| -> String {
        let named = token_names.get(definition).and_then(|source| source["symbol"].as_str().or(source["name"].as_str())).or(name);
        match named {
            Some(name) if !name.trim().is_empty() => format!("{} ({})", clean_text(name, 32), short(definition)),
            _ => format!("token {}", short(definition)),
        }
    };
    let decimals_of = |definition: &str| token_names.get(definition).and_then(|source| source["decimals"].as_u64());
    let shown = |value: u128, decimals: Option<u64>| match decimals {
        Some(places) if places > 0 && places <= 38 => {
            let scale = 10_u128.pow(u32::try_from(places).unwrap_or(0));
            let fraction = format!("{:0width$}", value % scale, width = usize::try_from(places).unwrap_or(0));
            let fraction = fraction.trim_end_matches('0');
            if fraction.is_empty() {
                format!("{} ({} base units)", grouped(value / scale), grouped(value))
            } else {
                format!("{}.{} ({} base units)", grouped(value / scale), fraction, grouped(value))
            }
        }
        _ => grouped(value),
    };

    // The person's own account: every asset the wallet can value, and the data it can't.
    let mut debited_beyond = false;
    let mut credits: Vec<(String, u128)> = Vec::new();
    for row in report["own"].as_array().map(Vec::as_slice).unwrap_or_default() {
        match row["asset"].as_str() {
            Some("LEZ") | Some("token") => {
                let (Some(before), Some(after)) = (number(&row["before"]), number(&row["after"])) else {
                    refusals.push("The simulation gave an unreadable balance for your account.".to_owned());
                    continue;
                };
                let (key, what, decimals) = if row["asset"] == "LEZ" {
                    (provider::ASSET.to_owned(), "LEZ".to_owned(), None)
                } else {
                    let definition = row["definition"].as_str().unwrap_or_default().to_owned();
                    let what = token_label(&definition, row["name"].as_str());
                    let decimals = decimals_of(&definition);
                    (definition, what, decimals)
                };
                if after > before {
                    credits.push((key.clone(), after - before));
                    lines.push(format!("{} receives {} {what}.", short_account(account), shown(after - before, decimals)));
                } else if before > after {
                    let taken = before - after;
                    lines.push(format!("{} pays {} {what}.", short_account(account), shown(taken, decimals)));
                    if taken > declared.debit_of(&key) {
                        debited_beyond = true;
                        refusals.push(format!(
                            "It takes {} {what} from your account, but the app declared {}. The wallet won't sign what takes more than the app said.",
                            shown(taken, decimals),
                            shown(declared.debit_of(&key), decimals)
                        ));
                    }
                }
            }
            Some("data") => {
                let program = row["program"].as_str().unwrap_or_default();
                if row["changed"] == true {
                    if row["before_empty"] == true {
                        lines.push(format!("{} keeps new data for {} on your account.", program_name(program, labels), short(program)));
                    } else {
                        refusals.push(format!(
                            "It changes what {} keeps on your account, which the wallet can't value; it can't show you what that moves, so it refuses.",
                            program_name(program, labels)
                        ));
                    }
                }
            }
            _ => refusals.push("The simulation reported a change the wallet doesn't understand.".to_owned()),
        }
    }
    if !debited_beyond && !lines.iter().any(|line| line.contains(" pays ")) {
        lines.push("Nothing leaves your account except the fee.".to_owned());
    }
    for (asset, expected) in &declared.credits {
        let got = credits.iter().find(|(key, _)| key == asset).map_or(0, |(_, amount)| *amount);
        if got != *expected {
            warnings.push(format!(
                "The app said your account would receive {} of {}, but the simulation shows {}.",
                grouped(*expected),
                if asset == provider::ASSET { "LEZ".to_owned() } else { short(asset) },
                grouped(got)
            ));
        }
    }

    // The fee, paid inside the proof.
    let about = number(&report["fee"]["about"]);
    let at_most = number(&report["fee"]["at_most"]);
    let payer = report["fee"]["payer"].as_str().unwrap_or_default();
    match (about, at_most) {
        (Some(about), Some(at_most)) => {
            let from = if payer == account { "your account".to_owned() } else { short_account(payer) };
            lines.push(format!("Fee: about {} LEZ, at most {}, paid inside the proof from {from}; what isn't used comes back to it.", grouped(about), grouped(at_most)));
            if let Some(cap) = declared.max_fee {
                if at_most > cap {
                    refusals.push(format!("The fee can reach {} LEZ, more than the {} the app declared.", grouped(at_most), grouped(cap)));
                }
            }
        }
        _ => refusals.push("The simulation didn't price the fee.".to_owned()),
    }
    if request["fee_payer"] == "account" && !payer.is_empty() && payer != account {
        refusals.push("The app asked that the account pay its own fee, but the wallet would pay it from another account.".to_owned());
    }

    // Programs: each named, and which are new for this app.
    let mut programs = Vec::new();
    for program in request["programs"].as_array().map(Vec::as_slice).unwrap_or_default() {
        let program = program.as_str().unwrap_or_default();
        let label = provider::label_fields(labels.get(program));
        let new = !remembered.iter().any(|known| known == program);
        programs.push(json!({ "program": program, "label": label, "new_for_app": new }));
        if new {
            warnings.push(format!("This app hasn't used {} ({}) before.", program_name(program, labels), short(program)));
        }
        if label["registry"] != true {
            if !warnings.iter().any(|w| w.contains("Program Registry isn't installed")) {
                warnings.push("The Program Registry isn't installed, so who wrote these programs and whether their source is verified is unknown.".to_owned());
            }
        } else if label["stale"] == true {
            warnings.push(format!("{} now runs different code than its registered source.", program_name(program, labels)));
        } else if label["verified"] != true {
            warnings.push(format!("{}'s source hasn't been verified on this computer.", program_name(program, labels)));
        }
        if label["upgradeable"] == true {
            warnings.push(format!("{} can be upgraded, so its code may change later.", program_name(program, labels)));
        }
    }

    // What becomes public.
    for row in report["public"].as_array().map(Vec::as_slice).unwrap_or_default() {
        let at = short(row["account"].as_str().unwrap_or_default());
        let (before, after) = (number(&row["before"]), number(&row["after"]));
        match row["kind"].as_str() {
            Some("native") => {
                if let (Some(before), Some(after)) = (before, after) {
                    if before != after {
                        public_lines.push(format!("Public/{at}: LEZ {} → {}", grouped(before), grouped(after)));
                    }
                }
            }
            Some("token_definition") => {
                if let (Some(before), Some(after)) = (before, after) {
                    let definition = row["definition"].as_str().unwrap_or_default();
                    public_lines.push(format!("{}: supply {} → {}", token_label(definition, None), grouped(before), grouped(after)));
                }
            }
            Some("native_credit") => {
                if let Some(amount) = number(&row["amount"]) {
                    public_lines.push(format!("Public/{at} is sent {} LEZ (a credit it takes in later)", grouped(amount)));
                }
            }
            Some("token_credit") => {
                if let Some(amount) = number(&row["amount"]) {
                    let definition = row["definition"].as_str().unwrap_or_default();
                    public_lines.push(format!("Public/{at} is sent {} {} (a credit it takes in later)", grouped(amount), token_label(definition, None)));
                }
            }
            Some("token_holding") => {
                if let (Some(before), Some(after)) = (before, after) {
                    let definition = row["definition"].as_str().unwrap_or_default();
                    public_lines.push(format!("Public/{at}: {} {} → {}", token_label(definition, None), grouped(before), grouped(after)));
                }
            }
            _ => {
                if row["changed"] == true {
                    public_lines.push(format!("Public/{at}: {}'s data changes", program_name(row["program"].as_str().unwrap_or_default(), labels)));
                }
            }
        }
    }
    let aliases: Vec<String> = report["aliases"].as_array().map(Vec::as_slice).unwrap_or_default().iter().filter_map(|a| a.as_str().map(short)).collect();
    let mut privacy = Vec::new();
    privacy.push(format!(
        "Public: the programs it runs ({}), the public accounts it reaches and the changes above, the fee reserve, and the time.",
        programs.len()
    ));
    if !aliases.is_empty() {
        privacy.push(format!(
            "Public: the one-time address{} {} and what it receives. Nobody can tell it is yours.",
            if aliases.len() > 1 { "es" } else { "" },
            aliases.join(", ")
        ));
    }
    privacy.push(format!("Private: {account}, its balance, and the message to it (proven, not shown)."));
    let account_public = report["account_public"] != false;
    if account_public {
        if request["reveal_account"] == true {
            warnings.push(format!("Your private account's ID {} appears in public data of this transaction: anyone can link it to this call.", short_account(account)));
        } else {
            refusals.push(format!(
                "Your private account's ID {} would appear in public data, and the app didn't say so: the wallet refuses rather than reveal it.",
                short_account(account)
            ));
        }
    } else {
        privacy.push("Checked: your account's ID appears nowhere in what becomes public.".to_owned());
    }

    json!({
        "lines": lines,
        "public_changes": public_lines,
        "privacy": privacy,
        "programs": programs,
        "warnings": warnings,
        "refusals": refusals,
        "refused": !refusals.is_empty(),
        "digest": report["digest"],
        "public_cycles": report["public_cycles"],
    })
}

fn short_account(account: &str) -> String {
    match account.split_once('/') {
        Some((kind, id)) => format!("{kind}/{}", short(id)),
        None => short(account),
    }
}

/// A program as the sheet names it: its registry label's text, else its short ID.
fn program_name(program: &str, labels: &Map<String, Value>) -> String {
    labels.get(program).and_then(|label| label["text"].as_str()).map_or_else(|| short(program), |text| clean_text(text, 60))
}

/// A request to use app secrets of `scope` from `account` (`app_secret`), which the person
/// approves once.
pub fn secret_request(app: &str, account: &str, scope: &str, reason: &str, now: u64) -> Result<Value, String> {
    let (private, id) = account_ref(account)?;
    crate::app_secret::Ask::new(app, scope, Vec::new(), "x", 0, 1)?;
    let mut request = Map::new();
    request.insert("id".into(), json!(registry::hex(&registry::fresh_bytes::<16>()?)));
    request.insert("app".into(), json!(app));
    request.insert("kind".into(), json!(SECRET));
    request.insert("state".into(), json!("pending"));
    request.insert("created".into(), json!(now));
    request.insert("expires".into(), json!(now + provider::PENDING_FOR_MS));
    request.insert("account".into(), json!(account_written(private, &id)));
    request.insert("scope".into(), json!(scope));
    request.insert("reason".into(), json!(clean_text(reason, provider::MAX_TEXT)));
    request.insert(
        "explanation".into(),
        json!(format!(
            "{app} asks for secret values of its own, \"{scope}\", made from {}. The wallet makes them from this account's key, so they come back if you restore the wallet from its recovery phrase. {app} gets only these values, never the account's key, and no other app can get them. Whoever holds them can see what {app} does with them (for a bridge, which deposits are yours), but can't spend from your account: every transaction still needs your approval here. You can take this back in Settings, Connected apps.",
            account_written(private, &id)
        )),
    );
    Ok(Value::Object(request))
}

/// A request from an app to list a token program's definitions in the person's holdings.
/// `definitions_json`: `[{"definition": "<b58>", "name", "symbol", "decimals", "origin"}]`.
pub fn token_source_request(app: &str, program: &str, definitions_json: &str, now: u64) -> Result<Value, String> {
    let program = program_id(program)?;
    let listed: Value = serde_json::from_str(definitions_json).map_err(|error| format!("the definitions are not JSON: {error}"))?;
    let listed = listed.as_array().ok_or("the definitions are a JSON array")?;
    if listed.is_empty() || listed.len() > MAX_SOURCE_DEFINITIONS {
        return Err(format!("list 1 to {MAX_SOURCE_DEFINITIONS} token definitions"));
    }
    let mut definitions = Vec::new();
    for entry in listed {
        let id = registry::parse_account(entry["definition"].as_str().ok_or("each definition has its ID, \"definition\"")?.trim())?;
        let decimals = entry["decimals"].as_u64().unwrap_or(0);
        if decimals > 38 {
            return Err("decimals are 0 to 38".to_owned());
        }
        definitions.push(json!({
            "definition": registry::account_text(&id),
            "name": clean_text(entry["name"].as_str().unwrap_or_default(), 64),
            "symbol": clean_text(entry["symbol"].as_str().unwrap_or_default(), 12),
            "decimals": decimals,
            "origin": clean_text(entry["origin"].as_str().unwrap_or_default(), 120),
        }));
    }
    let mut request = Map::new();
    request.insert("id".into(), json!(registry::hex(&registry::fresh_bytes::<16>()?)));
    request.insert("app".into(), json!(app));
    request.insert("kind".into(), json!(TOKEN_SOURCE));
    request.insert("state".into(), json!("pending"));
    request.insert("created".into(), json!(now));
    request.insert("expires".into(), json!(now + provider::PENDING_FOR_MS));
    request.insert("token_program".into(), json!(registry::account_text(&program)));
    request.insert("definitions".into(), Value::from(definitions));
    Ok(Value::Object(request))
}

/// The per-app grants this file keeps in the provider's store, beside `grants`.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Grants {
    /// app → `[{account, scope, since}]`.
    pub secrets: Map<String, Value>,
    /// app → `[program]`.
    pub programs: Map<String, Value>,
    /// `[{token_program, definitions, app, since}]`.
    pub token_sources: Vec<Value>,
}

impl Grants {
    pub fn load(saved: &Value) -> Self {
        let mut grants = Self::default();
        let mut count = 0;
        for (app, list) in saved["secret_grants"].as_object().into_iter().flatten() {
            if provider::app_name(app).is_err() {
                continue;
            }
            let kept: Vec<Value> = list
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default()
                .iter()
                .filter(|grant| {
                    grant["account"].as_str().is_some_and(|account| account_ref(account).is_ok())
                        && grant["scope"].as_str().is_some_and(|scope| crate::app_secret::Ask::new(app, scope, Vec::new(), "x", 0, 1).is_ok())
                })
                .take(MAX_SECRET_GRANTS_PER_APP)
                .cloned()
                .collect();
            if !kept.is_empty() && count + kept.len() <= MAX_SECRET_GRANTS {
                count += kept.len();
                grants.secrets.insert(app.clone(), Value::from(kept));
            }
        }
        for (app, list) in saved["program_grants"].as_object().into_iter().flatten() {
            if provider::app_name(app).is_err() {
                continue;
            }
            let kept: Vec<Value> = list
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default()
                .iter()
                .filter(|program| program.as_str().is_some_and(|text| program_id(text).is_ok()))
                .take(MAX_PROGRAM_GRANTS_PER_APP)
                .cloned()
                .collect();
            if !kept.is_empty() {
                grants.programs.insert(app.clone(), Value::from(kept));
            }
        }
        grants.token_sources = saved["token_sources"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .filter(|source| source["token_program"].as_str().is_some_and(|text| program_id(text).is_ok()) && source["definitions"].is_array())
            .take(MAX_TOKEN_SOURCES)
            .cloned()
            .collect();
        grants
    }

    pub fn save_into(&self, saved: &mut Value) {
        saved["secret_grants"] = Value::Object(self.secrets.clone());
        saved["program_grants"] = Value::Object(self.programs.clone());
        saved["token_sources"] = Value::from(self.token_sources.clone());
    }

    pub fn has_secret(&self, app: &str, account: &str, scope: &str) -> bool {
        self.secrets.get(app).and_then(Value::as_array).is_some_and(|list| list.iter().any(|g| g["account"] == account && g["scope"] == scope))
    }

    pub fn grant_secret(&mut self, app: &str, account: &str, scope: &str, now: u64) -> Result<(), String> {
        if self.has_secret(app, account, scope) {
            return Ok(());
        }
        let total: usize = self.secrets.values().filter_map(Value::as_array).map(Vec::len).sum();
        let list = self.secrets.entry(app.to_owned()).or_insert_with(|| json!([]));
        let list = list.as_array_mut().ok_or("the secret grants are unreadable")?;
        if list.len() >= MAX_SECRET_GRANTS_PER_APP || total >= MAX_SECRET_GRANTS {
            return Err("this app has too many secret grants; take some back in Settings first".to_owned());
        }
        list.push(json!({ "account": account, "scope": scope, "since": now }));
        Ok(())
    }

    pub fn programs_of(&self, app: &str) -> Vec<String> {
        self.programs.get(app).and_then(Value::as_array).map(|list| list.iter().filter_map(|p| p.as_str().map(str::to_owned)).collect()).unwrap_or_default()
    }

    pub fn remember_programs(&mut self, app: &str, programs: &[String]) {
        let mut known = self.programs_of(app);
        for program in programs {
            if !known.contains(program) && known.len() < MAX_PROGRAM_GRANTS_PER_APP {
                known.push(program.clone());
            }
        }
        self.programs.insert(app.to_owned(), json!(known));
    }

    /// Takes back everything `app` was given here; the token sources it added stay, since
    /// they only name tokens the person holds.
    pub fn revoke(&mut self, app: &str) -> bool {
        let secrets = self.secrets.remove(app).is_some();
        let programs = self.programs.remove(app).is_some();
        secrets || programs
    }

    /// Takes back one secret grant.
    pub fn revoke_secret(&mut self, app: &str, account: &str, scope: &str) -> bool {
        let Some(list) = self.secrets.get_mut(app).and_then(Value::as_array_mut) else { return false };
        let before = list.len();
        list.retain(|g| !(g["account"] == account && g["scope"] == scope));
        let removed = list.len() != before;
        if list.is_empty() {
            self.secrets.remove(app);
        }
        removed
    }

    /// Adds or replaces the token source for `program`.
    pub fn add_token_source(&mut self, request: &Value, now: u64) -> Result<(), String> {
        let program = request["token_program"].as_str().ok_or("no token program")?.to_owned();
        self.token_sources.retain(|source| source["token_program"] != program.as_str());
        if self.token_sources.len() >= MAX_TOKEN_SOURCES {
            return Err(format!("the wallet keeps {MAX_TOKEN_SOURCES} token sources; remove one first"));
        }
        self.token_sources.push(json!({ "token_program": program, "definitions": request["definitions"], "app": request["app"], "since": now }));
        Ok(())
    }

    /// Definition ID → `{name, symbol, decimals, origin, token_program}`, across all sources.
    pub fn token_names(&self) -> Map<String, Value> {
        let mut names = Map::new();
        for source in &self.token_sources {
            for definition in source["definitions"].as_array().map(Vec::as_slice).unwrap_or_default() {
                if let Some(id) = definition["definition"].as_str() {
                    let mut entry = definition.clone();
                    entry["token_program"] = source["token_program"].clone();
                    names.insert(id.to_owned(), entry);
                }
            }
        }
        names
    }
}

/// The phase of a private call's CLI job, from its log: each phase prints `phase: <name>`
/// as it starts (`plan`, `simulate`, `prove`, `submit`, `confirm`).
pub fn phase_of(log: &str) -> Option<&str> {
    log.lines().rev().find_map(|line| line.trim().strip_prefix("phase: ")).map(str::trim)
}

/// Whether a job in `phase` may still be stopped without leaving something on the chain.
pub fn may_stop(phase: Option<&str>) -> bool {
    matches!(phase, None | Some("plan" | "simulate" | "prove"))
}

/// The report the CLI's last JSON line carries (`{"report": …}`), if any.
pub fn report_of(output: &str) -> Option<Value> {
    output.lines().rev().find_map(|line| serde_json::from_str::<Value>(line.trim()).ok().filter(|v| v.get("report").is_some())).map(|v| v["report"].clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    const V: &str = "2if1EJVcZwFT3oXxF4uewB2mk2XDXpmCWwTtBwUQ6HUC";
    const BRIDGE: &str = "JECKBkvoc2e3dC18Wm6zZJEC2BGMwUwLsdZiQzYTnUx7";
    const TOKEN: &str = "4VScJ7mtwaanQ5gcSZZw2iAMHYAbLrVmbYKwEGx2eSex";
    const ATTEST: &str = "4VhxvwLN5n3u3rZbsz2VNFVavLwc1niNvgxq8JJ15veK";
    const DUSDC: &str = "FBqNAwJs9T4eNDw9pQXxXrLZNcnR1sTnnBEYzhudLQZJ";
    const ALIAS: &str = "AePC3xbb5yVhcpHfUkwCqCqNtoCsgVQnBCfbz8G4vGkN";
    const CONFIG: &str = "6iArKUXxhUJqS7kCaPNhwMWt3ro71PDyBj7jwAyE2VQV";

    fn claim_call(debits: &str) -> String {
        json!({
            "programs": [BRIDGE, TOKEN, ATTEST],
            "accounts": [
                { "account": CONFIG, "program": ATTEST },
                { "seed": "11".repeat(32), "of": BRIDGE },
                { "seed": "22".repeat(32), "of": BRIDGE },
                { "account": DUSDC, "program": TOKEN },
                { "actor": "account", "program": TOKEN, "openings": true },
                { "account": CONFIG, "program": "native" },
                { "actor": "account", "program": "native", "openings": true },
            ],
            "message": "00".repeat(6595),
            "openings": ["33".repeat(32)],
            "fee_payer": "account",
            "declared": serde_json::from_str::<Value>(debits).unwrap(),
        })
        .to_string()
    }

    fn claim_report() -> Value {
        json!({
            "account": format!("Private/{V}"),
            "digest": "ab".repeat(32),
            "public_cycles": 81568,
            "fee": { "about": "5723160", "at_most": "9300135", "payer": format!("Private/{V}") },
            "own": [
                { "asset": "data", "program": BRIDGE, "before_empty": true, "changed": false },
                { "asset": "token", "program": TOKEN, "definition": DUSDC, "before": "0", "after": "100000000", "name": "Wrapped dUSDC" },
                { "asset": "LEZ", "before": "0", "after": "20000000" },
            ],
            "public": [
                { "account": CONFIG, "program": "native", "kind": "native", "before": "100000000", "after": "80000000" },
                { "account": DUSDC, "program": TOKEN, "kind": "token_definition", "definition": DUSDC, "before": "100000000", "after": "200000000" },
                { "account": CONFIG, "program": BRIDGE, "kind": "data", "before_empty": false, "changed": true },
            ],
            "aliases": [ALIAS],
            "events": 1,
            "account_public": false,
        })
    }

    fn names() -> Map<String, Value> {
        let mut grants = Grants::default();
        let source = token_source_request("lez_bridge", TOKEN, &json!([{ "definition": DUSDC, "name": "Wrapped dUSDC", "symbol": "dUSDC", "decimals": 6, "origin": "Sepolia 0xAc20…a71e" }]).to_string(), 1).unwrap();
        grants.add_token_source(&source, 1).unwrap();
        grants.token_names()
    }

    #[test]
    fn a_bridge_claim_is_checked_when_it_is_asked_for() {
        let made = private_call_request("lez_bridge", &format!("Private/{V}"), &claim_call("{\"max_fee\": \"10000000\"}"), "Claim 100 dUSDC", 7).unwrap();
        assert_eq!(made["kind"], PRIVATE_CALL);
        assert_eq!(made["message_len"], 6595);
        assert_eq!(made["programs"][0], BRIDGE);
        assert_eq!(made["mentions"][1]["account"], registry::account_text(&registry::pda(&registry::parse_account(BRIDGE).unwrap(), &[0x11; 32])));
        assert_eq!(made["mentions"][5]["program"], "native");
        assert_eq!(made["check"], "running");
        assert_eq!(made["declared"]["max_fee"], "10000000");
        let input = cli_input(&made, Some("cd"), &[TOKEN.to_owned()]);
        assert_eq!(input["token_programs"][0], TOKEN);
        assert_eq!(input["accounts"], made["mentions"]);
        assert_eq!(input["expect_digest"], "cd");
    }

    #[test]
    fn bad_private_calls_are_refused_when_asked() {
        let account = format!("Private/{V}");
        let call = |patch: Value| {
            let mut call: Value = serde_json::from_str(&claim_call("{}")).unwrap();
            for (key, value) in patch.as_object().unwrap() {
                call[key] = value.clone();
            }
            call.to_string()
        };
        for (what, from, text) in [
            ("public account", format!("Public/{V}"), claim_call("{}")),
            ("not json", account.clone(), "{".to_owned()),
            ("no programs", account.clone(), call(json!({ "programs": [] }))),
            ("five programs", account.clone(), call(json!({ "programs": [BRIDGE, TOKEN, ATTEST, CONFIG, DUSDC] }))),
            ("program twice", account.clone(), call(json!({ "programs": [BRIDGE, BRIDGE] }))),
            ("native as program", account.clone(), call(json!({ "programs": ["11111111111111111111111111111111"] }))),
            ("foreign private", account.clone(), call(json!({ "accounts": [{ "account": format!("Private/{CONFIG}") }] }))),
            ("unknown program", account.clone(), call(json!({ "accounts": [{ "account": CONFIG, "program": DUSDC }] }))),
            ("pda of other", account.clone(), call(json!({ "accounts": [{ "seed": "11".repeat(32), "of": DUSDC }] }))),
            ("openings on public", account.clone(), call(json!({ "accounts": [{ "account": CONFIG, "openings": true }] }))),
            ("openings undeclared", account.clone(), call(json!({ "openings": [] }))),
            ("big message", account.clone(), call(json!({ "message": "00".repeat(MAX_PRIVATE_MESSAGE + 1) }))),
            ("empty message", account.clone(), call(json!({ "message": "" }))),
            ("bad fee payer", account.clone(), call(json!({ "fee_payer": "someone" }))),
            ("bad debit", account.clone(), call(json!({ "declared": { "debits": [{ "asset": "LEZ", "amount": "0" }] } }))),
            ("extra field", account.clone(), call(json!({ "sign_with": "x" }))),
            ("many accounts", account.clone(), call(json!({ "accounts": vec![json!({ "account": CONFIG }); MAX_MENTIONS + 1] }))),
            ("many openings", account.clone(), call(json!({ "openings": vec!["33".repeat(32); MAX_OPENINGS + 1] }))),
        ] {
            assert!(private_call_request("app", &from, &text, "", 1).is_err(), "{what}");
        }
    }

    /// Golden: the sheet of the v2 demo's private attested claim (simulated effects as the
    /// CLI reports them for leaf 1: docs/wallet/BRIDGE-SDK.md has the real run).
    #[test]
    fn the_claim_sheet_says_what_the_simulation_shows() {
        let request = private_call_request("lez_bridge", &format!("Private/{V}"), &claim_call("{\"credits\": [{\"asset\": \"FBqNAwJs9T4eNDw9pQXxXrLZNcnR1sTnnBEYzhudLQZJ\", \"amount\": \"100000000\"}], \"max_fee\": \"10000000\"}"), "", 1).unwrap();
        let sheet = review(&request, &claim_report(), &Map::new(), &names(), &[]);
        assert_eq!(sheet["refused"], false, "{sheet:#}");
        assert_eq!(
            sheet["lines"],
            json!([
                "Private/2if1EJ…6HUC receives 100 (100,000,000 base units) dUSDC (FBqNAw…LQZJ).",
                "Private/2if1EJ…6HUC receives 20,000,000 LEZ.",
                "Nothing leaves your account except the fee.",
                "Fee: about 5,723,160 LEZ, at most 9,300,135, paid inside the proof from your account; what isn't used comes back to it."
            ])
        );
        assert_eq!(
            sheet["public_changes"],
            json!([
                "Public/6iArKU…2VQV: LEZ 100,000,000 → 80,000,000",
                "dUSDC (FBqNAw…LQZJ): supply 100,000,000 → 200,000,000",
                "Public/6iArKU…2VQV: JECKBk…nUx7's data changes"
            ])
        );
        assert_eq!(
            sheet["privacy"],
            json!([
                "Public: the programs it runs (3), the public accounts it reaches and the changes above, the fee reserve, and the time.",
                "Public: the one-time address AePC3x…vGkN and what it receives. Nobody can tell it is yours.",
                "Private: Private/2if1EJVcZwFT3oXxF4uewB2mk2XDXpmCWwTtBwUQ6HUC, its balance, and the message to it (proven, not shown).",
                "Checked: your account's ID appears nowhere in what becomes public."
            ])
        );
        // Three programs new for the app, and no registry installed.
        let warnings = sheet["warnings"].to_string();
        assert!(warnings.contains("hasn't used JECKBk…nUx7"), "{warnings}");
        assert!(warnings.contains("Program Registry isn't installed"), "{warnings}");
        // Remembered programs and a verified registry leave no warnings.
        let mut labels = Map::new();
        for program in [BRIDGE, TOKEN, ATTEST] {
            labels.insert(program.to_owned(), json!({ "text": "x ✓ verified", "kind": "deployer", "verified": true, "stale": false, "upgradeable": false }));
        }
        let sheet = review(&request, &claim_report(), &labels, &names(), &[BRIDGE.into(), TOKEN.into(), ATTEST.into()]);
        assert_eq!(sheet["warnings"], json!([]));
    }

    #[test]
    fn a_call_that_takes_more_than_declared_is_refused() {
        // A "claim" whose simulation drains the account's dUSDC and LEZ.
        let request = private_call_request("evil_app", &format!("Private/{V}"), &claim_call("{}"), "Free money!", 1).unwrap();
        let mut report = claim_report();
        report["own"] = json!([
            { "asset": "token", "program": TOKEN, "definition": DUSDC, "before": "500000000", "after": "0" },
            { "asset": "LEZ", "before": "70000000", "after": "0" },
        ]);
        let sheet = review(&request, &report, &Map::new(), &names(), &[]);
        assert_eq!(sheet["refused"], true);
        let refusals = sheet["refusals"].to_string();
        assert!(refusals.contains("takes 500 (500,000,000 base units) dUSDC"), "{refusals}");
        assert!(refusals.contains("takes 70,000,000 LEZ"), "{refusals}");
        // The person sees the drain itself, not only the refusal, and where it goes.
        assert!(sheet["lines"].to_string().contains("pays 500"), "{sheet:#}");
        let mut with_credit = report.clone();
        with_credit["public"] = json!([{ "account": CONFIG, "program": TOKEN, "kind": "token_credit", "definition": DUSDC, "amount": "500000000" },
                                       { "account": CONFIG, "program": "native", "kind": "native_credit", "amount": "70000000" }]);
        let shown = review(&request, &with_credit, &Map::new(), &names(), &[]);
        assert_eq!(shown["public_changes"], json!([
            "Public/6iArKU…2VQV is sent 500,000,000 dUSDC (FBqNAw…LQZJ) (a credit it takes in later)",
            "Public/6iArKU…2VQV is sent 70,000,000 LEZ (a credit it takes in later)"
        ]));
        // Declared, the same debit is shown and allowed.
        let request = private_call_request(
            "app",
            &format!("Private/{V}"),
            &claim_call(&json!({ "debits": [{ "asset": DUSDC, "amount": "500000000" }, { "asset": "LEZ", "amount": "70000000" }] }).to_string()),
            "",
            1,
        )
        .unwrap();
        assert_eq!(review(&request, &report, &Map::new(), &names(), &[])["refused"], false);
        // One base unit more than declared is refused.
        report["own"][1]["before"] = json!("70000001");
        assert_eq!(review(&request, &report, &Map::new(), &names(), &[])["refused"], true);
    }

    #[test]
    fn what_the_wallet_cant_explain_is_refused() {
        let request = private_call_request("app", &format!("Private/{V}"), &claim_call("{\"max_fee\": \"9000000\"}"), "", 1).unwrap();
        let refused = |report: &Value| review(&request, report, &Map::new(), &names(), &[])["refused"] == true;
        // The fee may reach more than the declared cap.
        assert!(refused(&claim_report()));
        let request = private_call_request("app", &format!("Private/{V}"), &claim_call("{}"), "", 1).unwrap();
        let refused = |report: &Value| review(&request, report, &Map::new(), &names(), &[])["refused"] == true;
        assert!(!refused(&claim_report()));
        // Data the wallet can't value, already there, changed.
        let mut report = claim_report();
        report["own"][0] = json!({ "asset": "data", "program": BRIDGE, "before_empty": false, "changed": true });
        assert!(refused(&report));
        // The account's ID shows in public data.
        let mut report = claim_report();
        report["account_public"] = json!(true);
        assert!(refused(&report));
        // A missing indicator counts as public.
        report.as_object_mut().unwrap().remove("account_public");
        assert!(refused(&report));
        // Another account simulated, an unreadable balance, an unknown row, no fee.
        for patch in [
            json!({ "account": format!("Private/{CONFIG}") }),
            json!({ "own": [{ "asset": "LEZ", "before": "x", "after": "1" }] }),
            json!({ "own": [{ "asset": "nft" }] }),
            json!({ "fee": {} }),
        ] {
            let mut report = claim_report();
            for (key, value) in patch.as_object().unwrap() {
                report[key] = value.clone();
            }
            assert!(refused(&report), "{patch}");
        }
        // A fee paid from elsewhere when the app asked the account to pay it.
        let mut report = claim_report();
        report["fee"]["payer"] = json!(format!("Private/{CONFIG}"));
        assert!(refused(&report));
        // The app's own reveal: allowed, with a warning.
        let request = private_call_request("app", &format!("Private/{V}"), &{
            let mut call: Value = serde_json::from_str(&claim_call("{}")).unwrap();
            call["reveal_account"] = json!(true);
            call.to_string()
        }, "", 1)
        .unwrap();
        let mut report = claim_report();
        report["account_public"] = json!(true);
        let sheet = review(&request, &report, &Map::new(), &names(), &[]);
        assert_eq!(sheet["refused"], false);
        assert!(sheet["warnings"].to_string().contains("appears in public data"));
    }

    #[test]
    fn secret_grants_are_kept_per_app_account_and_scope_and_can_be_taken_back() {
        let mut grants = Grants::default();
        let account = format!("Private/{V}");
        assert!(!grants.has_secret("lez_bridge", &account, "RFP022/deposit"));
        grants.grant_secret("lez_bridge", &account, "RFP022/deposit", 5).unwrap();
        assert!(grants.has_secret("lez_bridge", &account, "RFP022/deposit"));
        assert!(!grants.has_secret("lez_bridge", &account, "RFP022/exit"));
        assert!(!grants.has_secret("other_app", &account, "RFP022/deposit"));
        assert!(!grants.has_secret("lez_bridge", &format!("Public/{V}"), "RFP022/deposit"));
        for at in 1..MAX_SECRET_GRANTS_PER_APP {
            grants.grant_secret("lez_bridge", &account, &format!("s{at}"), 5).unwrap();
        }
        assert!(grants.grant_secret("lez_bridge", &account, "one-more", 5).is_err());
        // Saved and loaded, with a hand-edited bad entry dropped.
        let mut saved = json!({});
        grants.remember_programs("lez_bridge", &[BRIDGE.to_owned()]);
        grants.save_into(&mut saved);
        saved["secret_grants"]["evil app"] = json!([{ "account": account, "scope": "x" }]);
        saved["secret_grants"]["ok"] = json!([{ "account": "nope", "scope": "x" }, { "account": account, "scope": "has space" }]);
        let back = Grants::load(&saved);
        assert!(back.has_secret("lez_bridge", &account, "s3"));
        assert!(back.secrets.get("evil app").is_none() && back.secrets.get("ok").is_none());
        assert_eq!(back.programs_of("lez_bridge"), vec![BRIDGE.to_owned()]);
        let mut back = back;
        assert!(back.revoke_secret("lez_bridge", &account, "s3"));
        assert!(!back.has_secret("lez_bridge", &account, "s3"));
        assert!(back.revoke("lez_bridge"));
        assert!(!back.has_secret("lez_bridge", &account, "RFP022/deposit"));
        assert!(back.programs_of("lez_bridge").is_empty());
    }

    #[test]
    fn a_secret_request_explains_itself_and_checks_its_scope() {
        let made = secret_request("lez_bridge", &format!("Private/{V}"), "RFP022/deposit", "To make your deposits recoverable", 3).unwrap();
        assert_eq!(made["kind"], SECRET);
        assert!(made["explanation"].as_str().unwrap().contains("never the account's key"));
        assert!(secret_request("lez_bridge", &format!("Private/{V}"), "bad scope", "", 3).is_err());
        assert!(secret_request("lez_bridge", V, "ok", "", 3).is_err());
    }

    #[test]
    fn token_sources_name_tokens_and_bad_ones_are_refused() {
        let names = names();
        assert_eq!(names[DUSDC]["symbol"], "dUSDC");
        assert_eq!(names[DUSDC]["decimals"], 6);
        assert_eq!(names[DUSDC]["token_program"], TOKEN);
        for (program, definitions) in [
            ("nope", json!([{ "definition": DUSDC }])),
            (TOKEN, json!([])),
            (TOKEN, json!([{ "definition": "nope" }])),
            (TOKEN, json!([{ "definition": DUSDC, "decimals": 39 }])),
            (TOKEN, json!(vec![json!({ "definition": DUSDC }); MAX_SOURCE_DEFINITIONS + 1])),
        ] {
            assert!(token_source_request("app", program, &definitions.to_string(), 1).is_err(), "{program} {definitions}");
        }
        // The name is the app's text: cleaned.
        let made = token_source_request("app", TOKEN, &json!([{ "definition": DUSDC, "name": "USD\u{202E}C" }]).to_string(), 1).unwrap();
        assert_eq!(made["definitions"][0]["name"], "USD C");
    }

    #[test]
    fn job_phases_are_read_from_the_log() {
        assert_eq!(phase_of("phase: plan\nreading\nphase: prove\nproving 3/9"), Some("prove"));
        assert_eq!(phase_of("nothing yet"), None);
        assert!(may_stop(Some("prove")) && may_stop(None));
        assert!(!may_stop(Some("submit")) && !may_stop(Some("confirm")));
        assert_eq!(report_of("x\n{\"report\": {\"digest\": \"ab\"}}\n").unwrap()["digest"], "ab");
        assert!(report_of("{\"other\": 1}").is_none());
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(1234567), "1,234,567");
        assert_eq!(grouped(100), "100");
    }
}
