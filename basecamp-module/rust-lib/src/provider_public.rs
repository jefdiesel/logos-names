//! A public call someone else pays for (Wallet Provider SDK, `propose_relayed_call`), and the
//! programs an app declares it will ask the wallet to sign for (`declare_programs`).
//!
//! An app hands the wallet an exact call: the program, the message bytes, the actors it declares,
//! and a fee declaration whose payer is ANOTHER account (a relay's). The wallet builds the
//! transaction itself (names CLI `module public-call plan`), shows a sheet made from that build,
//! never from the app's words, and once the person approves, signs it as the account (`sign`) and
//! hands it back half signed. The wallet sends nothing and pays nothing.
//!
//! What the account's signature covers on LEZ v0.4.0-rc3 (CLI `public_call.rs`): the whole
//! message, payer, fee caps and both signers' nonces included. So the payer can't swap itself
//! for another, raise the caps or change a byte, and the transaction is good once on this chain.
//! rc3 binds no chain id: after a reset of the bridge test network, or on another LEZ 0.4 chain
//! where the account's nonce comes back to what it was, the payer could submit it again (a known
//! limit, upstream).

use crate::provider::{self, account_ref, account_written};
use crate::registry::{self, AccountId};
use crate::text::clean_text;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

pub const RELAYED_CALL: &str = "relayed_call";
pub const PROGRAMS: &str = "programs";

/// The largest message (the CLI's limit too).
pub const MAX_MESSAGE: usize = 16 * 1024;
/// The most actors besides the root.
pub const MAX_ACTORS: usize = 16;
/// The most programs an app declares.
pub const MAX_DECLARED_PROGRAMS: usize = 8;

/// The privacy lines an app may choose from: a key, never free text.
pub const PRIVACY: [(&str, &str); 2] = [
    (
        "public_call",
        "Public: this account, the program it runs, the bytes it sends and what it changes, and the account that paid the fee.",
    ),
    ("bridge_public_claim", "A public claim publishes the claim secret and links the Sepolia deposit to this account."),
];

/// How long after the approval a relayed call may be signed again (`resign_relayed_call`).
pub const RESIGN_WINDOW_MS: u64 = 10 * 60 * 1000;
/// How many times it may be.
pub const MAX_RESIGNS: u64 = 3;
/// What a refused re-sign says; `{why}` is filled in.
pub const ASK_AGAIN: &str = "{why}: the wallet won't sign it again without the person. Propose it again (propose_relayed_call).";
/// Why a re-sign was refused, as a code an app can act on without reading the words:
/// `unchanged` (the payer's nonce hasn't moved: send the transaction you already have),
/// `landed_or_used` (the account's nonce moved: the call may have landed; check first) or
/// `ask_again` (anything else changed, or the window or the count is used up: propose it again).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResignRefusal {
    Unchanged(String),
    /// The signing account's nonce moved: the approved call may have landed, or the account
    /// was used otherwise. Check the chain before proposing it again.
    LandedOrUsed(String),
    AskAgain(String),
}

impl ResignRefusal {
    pub fn code(&self) -> &'static str {
        match self {
            ResignRefusal::Unchanged(_) => "unchanged",
            ResignRefusal::LandedOrUsed(_) => "landed_or_used",
            ResignRefusal::AskAgain(_) => "ask_again",
        }
    }

    pub fn text(&self) -> &str {
        match self {
            ResignRefusal::Unchanged(text) | ResignRefusal::LandedOrUsed(text) | ResignRefusal::AskAgain(text) => text,
        }
    }
}

/// What a build must keep from the approved one for a re-sign: everything the person saw.
const SAME_FOR_RESIGN: [&str; 11] =
    ["account", "program", "payer", "gas_limit", "tip", "max_fee", "actors", "creates", "own_balance", "message_len", "message_sha256"];

fn nonce_of(report: &Value, account: &str) -> Option<u128> {
    report["nonces"][account.trim_start_matches("Public/")].as_str()?.parse().ok()
}

/// The fee payer's nonce in a build (`report["nonces"]`).
pub fn payer_nonce(report: &Value) -> Option<u128> {
    nonce_of(report, report["payer"].as_str()?)
}

/// Whether `now` (a new build of an approved relayed call) may be signed without asking the
/// person again, and its hash if so. Only the payer's nonce may differ from `approved`, and it
/// must be newer than the last one signed (`last_payer_nonce`): every field the sheet showed,
/// the signing account's nonce and the set of signers must be the same.
pub fn resign_check(approved: &Value, last_payer_nonce: Option<u128>, now: &Value) -> Result<String, ResignRefusal> {
    let again = |why: &str| ResignRefusal::AskAgain(ASK_AGAIN.replace("{why}", why));
    for field in SAME_FOR_RESIGN {
        if approved[field] != now[field] || approved[field].is_null() {
            return Err(again(&format!("the call's {field} is not the one approved")));
        }
    }
    let keys = |report: &Value| report["nonces"].as_object().map(|n| n.keys().cloned().collect::<Vec<_>>()).unwrap_or_default();
    if keys(approved).len() != 2 || keys(approved) != keys(now) {
        return Err(again("the call's signers are not the ones approved"));
    }
    let account = approved["account"].as_str().unwrap_or_default();
    match (nonce_of(approved, account), nonce_of(now, account)) {
        (Some(before), Some(after)) if before == after => {}
        _ => {
            return Err(ResignRefusal::LandedOrUsed(
                "your account was used since the approval (its nonce moved), so the approved call may have landed: check before proposing it again".to_owned(),
            ))
        }
    }
    let last = last_payer_nonce.or_else(|| payer_nonce(approved)).ok_or_else(|| again("the approved call has no payer nonce"))?;
    match payer_nonce(now) {
        Some(nonce) if nonce > last => {}
        Some(_) => return Err(ResignRefusal::Unchanged("the payer's nonce hasn't moved: the transaction already given is still the one to send".to_owned())),
        None => return Err(again("the new build has no payer nonce")),
    }
    now["hash"]
        .as_str()
        .filter(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit()) && Some(*h) != approved["hash"].as_str())
        .map(str::to_owned)
        .ok_or_else(|| again("the new build gave no new hash"))
}

/// What the wallet says about who sees the signed transaction afterwards. Fixed.
pub const AFTER: &str = "Your wallet sends nothing and pays nothing: it gives the signed transaction back to the app. It can't see what happens to it after that. The payer can only submit exactly these bytes, and only once on this chain: changing the program, an account, the fee or one byte of the message breaks your signature. After a reset of this test network, or on another LEZ 0.4 chain where your account's nonce comes back to what it is now, the payer could submit it again.";

fn privacy_line(key: &str) -> Option<&'static str> {
    PRIVACY.iter().find(|(k, _)| *k == key).map(|(_, line)| *line)
}

/// A public account written `Public/<id>` or bare; a private one is refused.
fn public_ref(text: &str, what: &str) -> Result<AccountId, String> {
    let text = text.trim();
    if text.starts_with("Private/") {
        return Err(format!("{what} is a public account"));
    }
    registry::parse_account(text.strip_prefix("Public/").unwrap_or(text))
}

fn program_id(text: &str) -> Result<AccountId, String> {
    let text = text.trim();
    let id = registry::parse_account(text.strip_prefix("Public/").unwrap_or(text))?;
    if id == registry::NATIVE_TOKEN {
        return Err("the native token isn't called directly".to_owned());
    }
    Ok(id)
}

fn short(id: &str) -> String {
    let id = id.trim_start_matches("Public/");
    if id.chars().count() <= 12 {
        id.to_owned()
    } else {
        format!("{}…{}", id.chars().take(6).collect::<String>(), id.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect::<String>())
    }
}

fn whole(value: &Value, what: &str) -> Result<u128, String> {
    let text = match value {
        Value::String(text) => text.trim().to_owned(),
        Value::Number(n) => n.to_string(),
        Value::Null => "0".to_owned(),
        _ => return Err(format!("{what} is a whole number")),
    };
    text.parse::<u128>().map_err(|_| format!("{what} is a whole number of base units"))
}

/// The wallet's own programs, which the relayed path never signs for: `(program ID, kind)`, kind
/// `names_registry` (either network's) or `token_program` (the one the wallet's token methods use).
pub type OwnPrograms = [(String, &'static str)];

/// Refuses `id` if it is one of the wallet's own programs, saying why in plain words.
fn not_own(id: &str, own: &OwnPrograms) -> Result<(), String> {
    match own.iter().find(|(program, _)| program == id).map(|(_, kind)| *kind) {
        Some("names_registry") => Err(format!(
            "{id} is the Names registry: a call to it can change, give away or release the names your accounts hold, so the wallet never signs one an app hands it this way. Change names in the Names app."
        )),
        Some(_) => Err(format!(
            "{id} is the wallet's token program: a call to it can move the tokens your accounts hold, so the wallet never signs one an app hands it this way. Send tokens from the Wallet."
        )),
        None => Ok(()),
    }
}

/// Programs a relayed call may never have as its ROOT, though another program may reach them
/// (as a declared actor's program): where the person's tokens sit (the bridge's token program on
/// LEZ 0.4, the wallet's own token program). A root call to them, signed by the holder, can move
/// the tokens; reached from another program they refuse a transfer (it isn't the root signer).
pub type RootOnly = [(String, &'static str)];

fn not_root(id: &str, root_only: &RootOnly) -> Result<(), String> {
    match root_only.iter().find(|(program, _)| program == id) {
        Some(_) => Err(format!(
            "{id} holds tokens your accounts have (on LEZ 0.4, the bridged tokens): a call made directly to it can move them, so the wallet never signs one an app hands it this way. A call to another program may still reach it (a bridge claim does)."
        )),
        None => Ok(()),
    }
}

/// Every program a stored relayed call names, checked again as it is shown, approved or signed
/// again: the root against `own` and `root_only`, each declared actor's program and each program
/// account's owner against `own`. A request stored by an earlier build, or proposed before the
/// configuration changed, is held to the same rule as a new one.
pub fn programs_allowed(request: &Value, own: &OwnPrograms, root_only: &RootOnly) -> Result<(), String> {
    let root = request["program"].as_str().unwrap_or_default();
    not_own(root, own)?;
    not_root(root, root_only)?;
    for entry in request["accounts"].as_array().map(Vec::as_slice).unwrap_or_default() {
        for key in ["program", "of"] {
            if let Some(program) = entry[key].as_str().filter(|program| *program != "native") {
                not_own(program, own)?;
            }
        }
    }
    Ok(())
}

/// `[program, …]` as an app declares them: 1 to `MAX_DECLARED_PROGRAMS` program IDs, none of them
/// the wallet's own (`own`).
pub fn programs_request(app: &str, programs_json: &str, reason: &str, own: &OwnPrograms, now: u64) -> Result<Value, String> {
    if programs_json.len() > 4096 {
        return Err("the list is too long".to_owned());
    }
    let list: Value = serde_json::from_str(programs_json).map_err(|error| format!("the programs are not JSON: {error}"))?;
    let list = list.as_array().ok_or("the programs are a JSON array of program IDs")?;
    if list.is_empty() || list.len() > MAX_DECLARED_PROGRAMS {
        return Err(format!("declare 1 to {MAX_DECLARED_PROGRAMS} programs"));
    }
    let mut programs: Vec<String> = Vec::new();
    for entry in list {
        let id = registry::account_text(&program_id(entry.as_str().ok_or("each program is an ID")?)?);
        not_own(&id, own)?;
        if !programs.contains(&id) {
            programs.push(id);
        }
    }
    let mut request = provider::base(app, PROGRAMS, now)?;
    request.insert("programs".into(), json!(programs));
    request.insert("app_summary".into(), json!(clean_text(reason, provider::MAX_TEXT)));
    Ok(Value::Object(request))
}

/// The request for a relayed call from `account` (one of the wallet's public accounts the app
/// was given). `call_json`: `{program, message, accounts, fee: {payer, gas_limit, tip, max_fee},
/// privacy}` (docs/wallet/PROVIDER-SDK.md, "Calls someone else pays for"). `declared` are the
/// programs the app declared (`declare_programs`). The CLI's build (`report`) is added after.
pub fn relayed_call_request(
    app: &str,
    account: &str,
    call_json: &str,
    summary: &str,
    declared: &[String],
    own: &OwnPrograms,
    root_only: &RootOnly,
    now: u64,
) -> Result<Value, String> {
    let (private, id) = account_ref(account)?;
    if private {
        return Err("a relayed call is signed by a public account (Public/<id>)".to_owned());
    }
    if call_json.len() > MAX_MESSAGE * 2 + 8 * 1024 {
        return Err("the call is too long".to_owned());
    }
    let call: Value = serde_json::from_str(call_json).map_err(|error| format!("the call is not JSON: {error}"))?;
    let fields = call.as_object().ok_or("the call is a JSON object")?;
    for key in fields.keys() {
        if !matches!(key.as_str(), "program" | "message" | "accounts" | "fee" | "privacy") {
            return Err(format!("a call has no field {}", clean_text(key, 20)));
        }
    }
    let program = registry::account_text(&program_id(call["program"].as_str().ok_or("the call names its program")?)?);
    // The wallet's own programs are never reached this way, whatever was declared before, and
    // a program that holds the person's tokens is never the root.
    not_own(&program, own)?;
    not_root(&program, root_only)?;
    if !declared.contains(&program) {
        return Err(format!("{} is not a program this app declared; ask with declare_programs first", short(&program)));
    }
    let message = registry::unhex(call["message"].as_str().ok_or("the call's message is hex")?.trim()).map_err(|_| "the call's message is hex".to_owned())?;
    if message.is_empty() || message.len() > MAX_MESSAGE {
        return Err(format!("the message is 1 to {MAX_MESSAGE} bytes"));
    }
    let listed = call["accounts"].as_array().map(Vec::as_slice).unwrap_or_default();
    if listed.len() > MAX_ACTORS {
        return Err(format!("a call declares at most {MAX_ACTORS} accounts besides its root"));
    }
    let fee = &call["fee"];
    let payer = fee["payer"].as_str().ok_or("the fee names its payer: {\"payer\": \"Public/<id>\", …}")?;
    let payer_id = public_ref(payer, "the fee payer")?;
    if payer_id == id {
        return Err("the fee payer is the account that signs; that is an ordinary call (propose_call)".to_owned());
    }
    let gas_limit = u64::try_from(whole(&fee["gas_limit"], "gas_limit")?).map_err(|_| "gas_limit is too large".to_owned())?;
    let max_fee = whole(&fee["max_fee"], "max_fee")?;
    let tip = whole(&fee["tip"], "tip")?;
    if gas_limit == 0 || max_fee == 0 {
        return Err("the fee has a gas_limit and a max_fee above zero".to_owned());
    }
    let mut accounts = Vec::new();
    for entry in listed {
        let entry_fields = entry.as_object().ok_or("each account is {\"account\", \"program\"} or {\"seed\", \"of\", \"program\", \"new\"}")?;
        for key in entry_fields.keys() {
            if !matches!(key.as_str(), "account" | "program" | "seed" | "of" | "new") {
                return Err(format!("an account entry has no field {}", clean_text(key, 20)));
            }
        }
        let entry_program = match entry["program"].as_str().map(str::trim) {
            None => program.clone(),
            Some("native") | Some("LEZ") => "native".to_owned(),
            Some(text) => {
                let id = registry::account_text(&program_id(text)?);
                not_own(&id, own)?;
                if !declared.contains(&id) {
                    return Err(format!("{} is not a program this app declared", short(&id)));
                }
                id
            }
        };
        match (entry["account"].as_str(), entry["seed"].as_str()) {
            (Some(text), None) => {
                let account_id = public_ref(text, "a public call reaches public accounts only: an account")?;
                if account_id == payer_id {
                    return Err("the call declares an account of the fee payer; the wallet doesn't sign that".to_owned());
                }
                accounts.push(json!({ "account": registry::account_text(&account_id), "program": entry_program }));
            }
            (None, Some(seed)) => {
                let seed = registry::unhex(seed.trim()).ok().and_then(|bytes| <[u8; 32]>::try_from(bytes).ok()).ok_or("a seed is 32 bytes of hex")?;
                let of = registry::account_text(&program_id(entry["of"].as_str().unwrap_or(&program))?);
                not_own(&of, own)?;
                if !declared.contains(&of) {
                    return Err("a program account belongs to a declared program".to_owned());
                }
                let of_id = registry::parse_account(&of)?;
                accounts.push(json!({
                    "seed": registry::hex(&seed), "of": of, "program": entry_program, "new": entry["new"] == true,
                    "account": registry::account_text(&registry::pda(&of_id, &seed)),
                }));
            }
            _ => return Err("each account entry has \"account\" or \"seed\" (with \"of\")".to_owned()),
        }
    }
    let privacy = call["privacy"].as_str().unwrap_or("public_call");
    if privacy_line(privacy).is_none() {
        return Err(format!("privacy is one of {}", PRIVACY.iter().map(|(k, _)| *k).collect::<Vec<_>>().join(", ")));
    }
    let mut request = provider::base(app, RELAYED_CALL, now)?;
    request.insert("account".into(), json!(account_written(false, &id)));
    request.insert("program".into(), json!(program));
    request.insert("message".into(), json!(registry::hex(&message)));
    request.insert("message_len".into(), json!(message.len()));
    request.insert("message_sha256".into(), json!(registry::hex(&Sha256::digest(&message))));
    request.insert("accounts".into(), Value::from(accounts));
    request.insert(
        "fee".into(),
        json!({ "payer": account_written(false, &payer_id), "gas_limit": gas_limit, "tip": tip.to_string(), "max_fee": max_fee.to_string() }),
    );
    request.insert("privacy".into(), json!(privacy));
    request.insert("app_summary".into(), json!(clean_text(summary, provider::MAX_TEXT)));
    request.insert("check".into(), json!("running"));
    Ok(Value::Object(request))
}

/// What the names CLI gets for `module public-call plan|sign`.
pub fn cli_input(request: &Value, expect_hash: Option<&str>) -> Value {
    json!({
        "account": request["account"],
        "program": request["program"],
        "message": request["message"],
        "accounts": request["accounts"],
        "fee": request["fee"],
        "expect_hash": expect_hash,
    })
}

/// The approval sheet of a relayed call, from the CLI's build (`report`), never from the app's
/// words: `{lines, warnings, privacy, after, hash, refused, why}`. `label` is what the Program
/// Registry says of the program (`provider::label_fields` input), `network` how the network is
/// named (the site's rule: version · name). Refused when the build doesn't match the request.
pub fn sheet(request: &Value, report: &Value, label: Option<&Value>, network: &str, own: &OwnPrograms, root_only: &RootOnly) -> Value {
    let app = clean_text(request["app"].as_str().unwrap_or_default(), 40);
    let mut why: Vec<String> = Vec::new();
    // The same rule as when it was asked, applied again now: never shown as approvable otherwise.
    if let Err(refused) = programs_allowed(request, own, root_only) {
        why.push(refused);
    }
    for (field, want) in [("account", &request["account"]), ("program", &request["program"]), ("payer", &request["fee"]["payer"])] {
        if report[field] != *want {
            why.push(format!("the wallet's build names another {field} than the request"));
        }
    }
    if report["message_sha256"] != request["message_sha256"] || report["message_len"] != request["message_len"] {
        why.push("the wallet's build carries other bytes than the request".to_owned());
    }
    for (field, want) in [("max_fee", &request["fee"]["max_fee"]), ("tip", &request["fee"]["tip"])] {
        if report[field] != *want {
            why.push(format!("the wallet's build has another {field}"));
        }
    }
    if report["gas_limit"] != request["fee"]["gas_limit"] {
        why.push("the wallet's build has another gas limit".to_owned());
    }
    let hash = report["hash"].as_str().filter(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit())).map(str::to_owned);
    if hash.is_none() {
        why.push("the wallet's build gave no hash".to_owned());
    }
    // Everything shown comes from the wallet's own build of the transaction (`report`, made by
    // the names CLI from the same fields it signs), checked above against the request.
    let account = report["account"].as_str().unwrap_or_default();
    let program = report["program"].as_str().unwrap_or_default();
    let payer = report["payer"].as_str().unwrap_or_default();
    let label = provider::label_fields(label);
    // The wallet decodes no program's message bytes here: every program is shown as unknown,
    // with what the Program Registry says of it, and the exact bytes' hash.
    let registry_says = match label["text"].as_str() {
        Some(text) => format!("the Program Registry calls it {}", clean_text(text, 60)),
        None if label["registry"] == true => "it is not in the Program Registry".to_owned(),
        None => "the Program Registry isn't installed, so it has no label".to_owned(),
    };
    let named = format!("unknown program {program} ({registry_says})");
    let mut lines = vec![
        format!("{app} asks your wallet to sign a call that another account pays for."),
        format!("Network: {network}"),
        format!("Program: {named}"),
        format!("Signs: {account}, your account"),
        format!(
            "Fee: paid by {payer}, not by you: at most {} (gas limit {}, tip {}).",
            crate::provider_private::grouped(whole(&report["max_fee"], "").unwrap_or(0)),
            crate::provider_private::grouped(u128::from(report["gas_limit"].as_u64().unwrap_or(0))),
            crate::provider_private::grouped(whole(&report["tip"], "").unwrap_or(0)),
        ),
    ];
    let mut warnings = vec![format!(
        "Your wallet can't decode what this call tells {} to do: it shows only the exact bytes' hash. Approve it only if you trust {app} and this program.",
        short(program)
    )];
    let others: Vec<&Value> = report["actors"].as_array().map(Vec::as_slice).unwrap_or_default().iter().skip(1).collect();
    for actor in &others {
        let id = actor["account"].as_str().unwrap_or_default();
        let actor_program = actor["program"].as_str().unwrap_or_default();
        if format!("Public/{id}") == account {
            // Its LEZ balance is warned about below, from the build's own `own_balance`.
            // rc3 authorizes every declared actor of a signing account, so this is the account
            // acting, not just its data being read.
            if actor_program != registry::account_text(&registry::NATIVE_TOKEN) {
                warnings.push(format!("The call acts as your account toward {}: it may move what your account holds there.", short(actor_program)));
            }
        } else if actor_program == registry::account_text(&registry::NATIVE_TOKEN) {
            lines.push(format!("Reaches the LEZ balance of Public/{id}."));
        } else {
            lines.push(format!("Reaches Public/{id} (data for {}).", short(actor_program)));
        }
    }
    if report["own_balance"] == true {
        warnings.push(format!("The program is given your account's LEZ balance: it can move LEZ out of {account}."));
    }
    for made in report["creates"].as_array().map(Vec::as_slice).unwrap_or_default() {
        lines.push(format!("Creates the program's account Public/{}.", made.as_str().unwrap_or_default()));
    }
    lines.push(format!(
        "Exactly these bytes are signed: {} (the message is {} bytes, SHA-256 {}).",
        hash.as_deref().unwrap_or("?"),
        report["message_len"].as_u64().unwrap_or(0),
        report["message_sha256"].as_str().unwrap_or_default()
    ));
    warnings.extend(provider::label_warnings(&label, "program"));
    json!({
        "lines": lines,
        "warnings": warnings,
        "privacy": privacy_line(request["privacy"].as_str().unwrap_or("public_call")).unwrap_or(PRIVACY[0].1),
        "after": AFTER,
        "app_says": request["app_summary"],
        "hash": hash,
        "refused": !why.is_empty(),
        "why": why,
    })
}

/// The programs each app declared, kept in the provider's store (`declared_programs`).
pub fn load_declared(saved: &Value) -> Map<String, Value> {
    let mut out = Map::new();
    for (app, list) in saved["declared_programs"].as_object().into_iter().flatten() {
        if provider::app_name(app).is_err() {
            continue;
        }
        let kept: Vec<Value> = list
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .filter(|p| p.as_str().is_some_and(|text| program_id(text).is_ok()))
            .take(MAX_DECLARED_PROGRAMS)
            .cloned()
            .collect();
        if !kept.is_empty() {
            out.insert(app.clone(), Value::from(kept));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const BRIDGE: &str = "JECKBkvoc2e3dC18Wm6zZJEC2BGMwUwLsdZiQzYTnUx7";
    const ACCOUNT: &str = "Public/7kHXTZHzMCz3PgiQVA9PE5DZLa18xpYvijdPCPK4J6ak";
    const PAYER: &str = "Public/DGu98d9jZfTAuJKr9y3vWC1DDAujYUaddb6ugTpgSBNs";
    const OTHER: &str = "CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk";
    const REGISTRY_03: &str = "38jEfVV1RCyMzu1FGbYcCn5FqxCxKyY58VgzjruAh2gq";
    const REGISTRY_04: &str = "6Yxj5k8AiuBSuMAYVQFGrYKjJGUSSaWWgoB9D9XXN3gA";
    const TOKENS: &str = "3gF78HRJmUu52BGbZq2Ve8C4hdQhQ1oYpEgrGBWSKZE4";

    fn own() -> Vec<(String, &'static str)> {
        vec![(REGISTRY_03.to_owned(), "names_registry"), (REGISTRY_04.to_owned(), "names_registry"), (TOKENS.to_owned(), "token_program")]
    }

    fn call(extra: Value) -> String {
        let mut call = json!({
            "program": BRIDGE, "message": "00ff", "privacy": "bridge_public_claim",
            "accounts": [{ "account": OTHER }, { "seed": "22".repeat(32), "new": true }],
            "fee": { "payer": PAYER, "gas_limit": 120000, "tip": "0", "max_fee": "900000" }
        });
        for (k, v) in extra.as_object().unwrap() {
            call[k] = v.clone();
        }
        call.to_string()
    }

    fn report_for(request: &Value) -> Value {
        json!({
            "hash": "ab".repeat(32), "account": request["account"], "program": request["program"], "payer": request["fee"]["payer"],
            "gas_limit": request["fee"]["gas_limit"], "tip": request["fee"]["tip"], "max_fee": request["fee"]["max_fee"],
            "message_len": request["message_len"], "message_sha256": request["message_sha256"],
            "actors": [{ "account": ACCOUNT.trim_start_matches("Public/"), "program": BRIDGE }, { "account": OTHER, "program": BRIDGE }],
            "creates": [request["accounts"][1]["account"]], "own_balance": false,
        })
    }

    #[test]
    fn a_relayed_call_is_built_from_declared_programs_only() {
        let declared = vec![BRIDGE.to_owned()];
        let request = relayed_call_request("lez_bridge", ACCOUNT, &call(json!({})), "Claim 100 dUSDC", &declared, &own(), &[], 1).unwrap();
        assert_eq!(request["kind"], RELAYED_CALL);
        assert_eq!(request["fee"]["payer"], PAYER);
        assert_eq!(request["accounts"][1]["new"], true);
        let input = cli_input(&request, Some("ab"));
        assert_eq!(input["fee"]["max_fee"], "900000");
        assert_eq!(input["expect_hash"], "ab");
        // Refused before anything is built.
        assert!(relayed_call_request("lez_bridge", ACCOUNT, &call(json!({})), "", &[], &own(), &[], 1).unwrap_err().contains("declare_programs"));
        assert!(relayed_call_request("lez_bridge", ACCOUNT, &call(json!({ "fee": { "payer": ACCOUNT, "gas_limit": 1, "max_fee": "1" } })), "", &declared, &own(), &[], 1)
            .unwrap_err()
            .contains("ordinary call"));
        assert!(relayed_call_request("lez_bridge", ACCOUNT, &call(json!({ "accounts": [{ "account": PAYER }] })), "", &declared, &own(), &[], 1)
            .unwrap_err()
            .contains("fee payer"));
        assert!(relayed_call_request("lez_bridge", ACCOUNT, &call(json!({ "privacy": "say anything" })), "", &declared, &own(), &[], 1).unwrap_err().contains("privacy is one of"));
        assert!(relayed_call_request("lez_bridge", "Private/7kHXTZHzMCz3PgiQVA9PE5DZLa18xpYvijdPCPK4J6ak", &call(json!({})), "", &declared, &own(), &[], 1).is_err());
        assert!(relayed_call_request("lez_bridge", ACCOUNT, &call(json!({ "message": "" })), "", &declared, &own(), &[], 1).is_err());
        assert!(relayed_call_request("lez_bridge", ACCOUNT, &call(json!({ "fee": { "payer": PAYER, "gas_limit": 0, "max_fee": "1" } })), "", &declared, &own(), &[], 1).is_err());
        assert!(relayed_call_request("lez_bridge", ACCOUNT, &call(json!({ "extra": 1 })), "", &declared, &own(), &[], 1).unwrap_err().contains("no field"));
        assert!(relayed_call_request("lez_bridge", ACCOUNT, &call(json!({ "accounts": [{ "account": "x", "program": OTHER }] })), "", &declared, &own(), &[], 1).is_err());
    }

    #[test]
    fn the_sheet_says_what_the_wallet_built_in_plain_words() {
        let declared = vec![BRIDGE.to_owned()];
        let summary = "Claim \u{202E}CDSUd 001\u{202C} for you\u{200B}\nnow";
        let request = relayed_call_request("lez_bridge", ACCOUNT, &call(json!({})), summary, &declared, &own(), &[], 1).unwrap();
        let sheet = sheet(&request, &report_for(&request), Some(&json!({ "text": "RFP-022 bridge", "verified": true })), "LEZ 0.4 · bridge test network", &own(), &[]);
        assert_eq!(sheet["refused"], false, "{sheet}");
        let lines: Vec<&str> = sheet["lines"].as_array().unwrap().iter().map(|l| l.as_str().unwrap()).collect();
        assert_eq!(lines[0], "lez_bridge asks your wallet to sign a call that another account pays for.");
        assert_eq!(lines[1], "Network: LEZ 0.4 · bridge test network");
        assert_eq!(lines[2], format!("Program: unknown program {BRIDGE} (the Program Registry calls it RFP-022 bridge)"));
        assert!(sheet["warnings"][0].as_str().unwrap().starts_with("Your wallet can't decode what this call tells JECKBk"), "{sheet}");
        assert_eq!(lines[3], format!("Signs: {ACCOUNT}, your account"));
        assert_eq!(lines[4], format!("Fee: paid by {PAYER}, not by you: at most 900,000 (gas limit 120,000, tip 0)."));
        assert!(lines.iter().any(|l| l.starts_with("Reaches Public/CCVd76")));
        assert!(lines.iter().any(|l| l.starts_with("Creates the program's account Public/")));
        assert!(lines.last().unwrap().starts_with(&format!("Exactly these bytes are signed: {}", "ab".repeat(32))));
        assert_eq!(sheet["privacy"], "A public claim publishes the claim secret and links the Sepolia deposit to this account.");
        assert_eq!(sheet["after"], AFTER);
        // The app's words: one line, no bidi overrides or zero-width characters.
        let said = sheet["app_says"].as_str().unwrap();
        assert!(!said.contains('\u{202E}') && !said.contains('\u{200B}') && !said.contains('\n'), "{said:?}");

        // A build that differs from the request is refused, field by field.
        let mut report = report_for(&request);
        report["payer"] = json!(ACCOUNT);
        report["max_fee"] = json!("1");
        let refused = super::sheet(&request, &report, None, "LEZ 0.4 · bridge test network", &own(), &[]);
        assert_eq!(refused["refused"], true);
        assert_eq!(refused["why"].as_array().unwrap().len(), 2, "{refused}");
        assert!(refused["lines"][2].as_str().unwrap().contains("the Program Registry isn't installed"));
        // Its own balance given to the program is a warning.
        let mut report = report_for(&request);
        report["own_balance"] = json!(true);
        let warned = super::sheet(&request, &report, None, "LEZ 0.4 · bridge test network", &own(), &[]);
        assert!(warned["warnings"][1].as_str().unwrap().contains("can move LEZ out of"), "{warned}");
    }

    #[test]
    fn declared_programs_are_checked_ids() {
        let request = programs_request("lez_bridge", &json!([BRIDGE, BRIDGE]).to_string(), "to claim deposits", &own(), 1).unwrap();
        assert_eq!(request["programs"], json!([BRIDGE]));
        assert!(programs_request("lez_bridge", "[]", "", &own(), 1).is_err());
        assert!(programs_request("lez_bridge", &json!(["not an id"]).to_string(), "", &own(), 1).is_err());
        let saved = json!({ "declared_programs": { "lez_bridge": [BRIDGE, "junk"], "Bad App!": [BRIDGE] } });
        let loaded = load_declared(&saved);
        assert_eq!(loaded.get("lez_bridge").unwrap(), &json!([BRIDGE]));
        assert!(loaded.get("Bad App!").is_none());
    }

    /// A sheet never shows what the request says where the wallet's build says otherwise: any
    /// field that differs refuses it, and the lines are the build's.
    #[test]
    fn a_build_that_differs_from_the_request_is_refused_never_shown_as_the_request() {
        let declared = vec![BRIDGE.to_owned()];
        let request = relayed_call_request("lez_bridge", ACCOUNT, &call(json!({})), "", &declared, &own(), &[], 1).unwrap();
        for (field, value) in [
            ("account", json!(PAYER)),
            ("program", json!(OTHER)),
            ("payer", json!(ACCOUNT)),
            ("max_fee", json!("1")),
            ("tip", json!("5")),
            ("gas_limit", json!(1)),
            ("message_sha256", json!("00".repeat(32))),
            ("message_len", json!(3)),
            ("hash", json!("not a hash")),
        ] {
            let mut report = report_for(&request);
            report[field] = value;
            let shown = sheet(&request, &report, None, "LEZ 0.4 · bridge test network", &own(), &[]);
            assert_eq!(shown["refused"], true, "{field}: {shown}");
        }
    }

    /// What the module hands the CLI, pinned in a file the names CLI's own test builds from
    /// (`cli/src/public_call.rs`, `the_module_s_input_builds`): a plain account, an account's
    /// LEZ balance and a program account the call creates.
    #[test]
    fn the_cli_input_is_the_shared_fixture() {
        let declared = vec![BRIDGE.to_owned()];
        let accounts = json!([{ "account": OTHER }, { "account": OTHER, "program": "native" }, { "seed": "22".repeat(32), "new": true }]);
        let request = relayed_call_request("lez_bridge", ACCOUNT, &call(json!({ "accounts": accounts })), "", &declared, &own(), &[], 1).unwrap();
        let fixture: Value = serde_json::from_str(include_str!("../testdata/relayed-call-input.json")).unwrap();
        assert_eq!(cli_input(&request, None), fixture);
    }

    fn built(payer_nonce: &str, account_nonce: &str, hash: &str) -> Value {
        json!({
            "hash": hash, "account": ACCOUNT, "program": BRIDGE, "payer": PAYER, "gas_limit": 120000, "tip": "0", "max_fee": "900000",
            "nonces": { ACCOUNT.trim_start_matches("Public/"): account_nonce, PAYER.trim_start_matches("Public/"): payer_nonce },
            "actors": [{ "account": ACCOUNT.trim_start_matches("Public/"), "program": BRIDGE }], "creates": [], "own_balance": false,
            "message_len": 2, "message_sha256": "ee".repeat(32),
        })
    }

    /// A re-sign without the person: only the payer's nonce may have moved, forward. Every
    /// other field the person approved forces a new approval, as does the account's own nonce.
    #[test]
    fn a_re_sign_is_allowed_only_for_a_newer_payer_nonce() {
        let approved = built("41", "3", &"aa".repeat(32));
        assert_eq!(resign_check(&approved, None, &built("42", "3", &"bb".repeat(32))), Ok("bb".repeat(32)));
        assert_eq!(resign_check(&approved, Some(42), &built("45", "3", &"cc".repeat(32))), Ok("cc".repeat(32)), "after an earlier re-sign");
        let same = resign_check(&approved, Some(42), &built("42", "3", &"bb".repeat(32))).unwrap_err();
        assert_eq!((same.code(), same.text().contains("hasn't moved")), ("unchanged", true));
        assert!(resign_check(&approved, None, &built("41", "3", &"aa".repeat(32))).unwrap_err().text().contains("hasn't moved"));
        assert!(resign_check(&approved, None, &built("40", "3", &"bb".repeat(32))).is_err(), "an older payer nonce");
        let moved = resign_check(&approved, None, &built("42", "4", &"bb".repeat(32))).unwrap_err();
        assert_eq!((moved.code(), moved.text().contains("your account was used")), ("landed_or_used", true));
        assert!(resign_check(&approved, None, &built("42", "3", &"aa".repeat(32))).unwrap_err().text().contains("no new hash"));
        for (field, value) in [
            ("account", json!(PAYER)),
            ("program", json!(OTHER)),
            ("payer", json!(ACCOUNT)),
            ("gas_limit", json!(120001)),
            ("tip", json!("1")),
            ("max_fee", json!("900001")),
            ("actors", json!([])),
            ("creates", json!([OTHER])),
            ("own_balance", json!(true)),
            ("message_len", json!(3)),
            ("message_sha256", json!("ff".repeat(32))),
        ] {
            let mut changed = built("42", "3", &"bb".repeat(32));
            changed[field] = value;
            let refused = resign_check(&approved, None, &changed).unwrap_err();
            assert_eq!(refused.code(), "ask_again", "{field}");
            assert!(refused.text().contains(field) && refused.text().contains("Propose it again"), "{field}: {refused:?}");
        }
        let mut third = built("42", "3", &"bb".repeat(32));
        third["nonces"]["x"] = json!("1");
        assert!(resign_check(&approved, None, &third).unwrap_err().text().contains("signers"));
    }

    /// A1: the wallet's own Names registry (either network's) and its token program are never
    /// reached by a relayed call: not declarable, not as the root, not as a declared actor's
    /// program or a program account's owner, whatever was declared before. Other programs pass.
    #[test]
    fn the_wallet_s_own_programs_are_never_signed_for_this_way() {
        for (own_id, words) in [(REGISTRY_03, "Names registry"), (REGISTRY_04, "Names registry"), (TOKENS, "token program")] {
            let declared = programs_request("lez_bridge", &json!([BRIDGE, own_id]).to_string(), "", &own(), 1).unwrap_err();
            assert!(declared.contains(words) && declared.contains("never signs"), "{declared}");
            // Even if an older wallet had let the app declare it.
            let both = vec![BRIDGE.to_owned(), own_id.to_owned()];
            let root = relayed_call_request("lez_bridge", ACCOUNT, &call(json!({ "program": own_id, "accounts": [] })), "", &both, &own(), &[], 1).unwrap_err();
            assert!(root.contains(words), "{root}");
            let actor = relayed_call_request("lez_bridge", ACCOUNT, &call(json!({ "accounts": [{ "account": OTHER, "program": own_id }] })), "", &both, &own(), &[], 1).unwrap_err();
            assert!(actor.contains(words), "{actor}");
            let pda = relayed_call_request("lez_bridge", ACCOUNT, &call(json!({ "accounts": [{ "seed": "33".repeat(32), "of": own_id }] })), "", &both, &own(), &[], 1).unwrap_err();
            assert!(pda.contains(words), "{pda}");
        }
        assert!(programs_request("lez_bridge", &json!([BRIDGE]).to_string(), "", &own(), 1).is_ok(), "the bridge still passes");
        assert!(relayed_call_request("lez_bridge", ACCOUNT, &call(json!({})), "", &[BRIDGE.to_owned()], &own(), &[], 1).is_ok());
    }

    /// S1: an actor of the person's own account under another program is the account acting, a
    /// warning, not a line about data.
    #[test]
    fn an_own_actor_under_another_program_is_a_warning() {
        let declared = vec![BRIDGE.to_owned()];
        let request = relayed_call_request("lez_bridge", ACCOUNT, &call(json!({})), "", &declared, &own(), &[], 1).unwrap();
        let mut report = report_for(&request);
        report["actors"] = json!([{ "account": ACCOUNT.trim_start_matches("Public/"), "program": BRIDGE }, { "account": ACCOUNT.trim_start_matches("Public/"), "program": TOKENS }]);
        let shown = sheet(&request, &report, None, "LEZ 0.4 · bridge test network", &own(), &[]);
        assert!(shown["warnings"].as_array().unwrap().iter().any(|w| w.as_str().unwrap().starts_with("The call acts as your account toward 3gF78H")), "{shown}");
        assert!(!shown["lines"].as_array().unwrap().iter().any(|l| l.as_str().unwrap().contains("data for 3gF78H")));
    }

    /// D2: the program holding the person's bridged tokens is never the root of a relayed call,
    /// but a call to the bridge may still declare it (every bridge claim does).
    #[test]
    fn the_bridged_tokens_program_is_refused_as_root_only() {
        let bridged = "4VScJ7mtwaanQ5gcSZZw2iAMHYAbLrVmbYKwEGx2eSex";
        let root_only = vec![(bridged.to_owned(), "bridged_tokens")];
        let declared = vec![BRIDGE.to_owned(), bridged.to_owned()];
        assert!(programs_request("lez_bridge", &json!([BRIDGE, bridged]).to_string(), "", &own(), 1).is_ok(), "declaring it is fine");
        let root = relayed_call_request("lez_bridge", ACCOUNT, &call(json!({ "program": bridged, "accounts": [] })), "", &declared, &own(), &root_only, 1).unwrap_err();
        assert!(root.contains("holds tokens your accounts have"), "{root}");
        let claim = relayed_call_request(
            "lez_bridge",
            ACCOUNT,
            &call(json!({ "accounts": [{ "account": ACCOUNT, "program": bridged }, { "account": OTHER, "program": bridged }] })),
            "",
            &declared,
            &own(),
            &root_only,
            1,
        );
        assert!(claim.is_ok(), "a claim that declares it passes: {claim:?}");
    }

    /// D1: the rule holds at approval time too. A request stored by an earlier build (here, made
    /// by hand) to the Names registry, or to the bridged tokens' program as root, is refused on its
    /// sheet; a fine one stays approvable.
    #[test]
    fn a_stored_request_is_checked_again_on_its_sheet() {
        let declared = vec![BRIDGE.to_owned()];
        let fine = relayed_call_request("lez_bridge", ACCOUNT, &call(json!({})), "", &declared, &own(), &[], 1).unwrap();
        assert_eq!(sheet(&fine, &report_for(&fine), None, "LEZ 0.4", &own(), &[])["refused"], false);
        let mut to_registry = fine.clone();
        to_registry["program"] = json!(REGISTRY_04);
        let mut report = report_for(&to_registry);
        report["program"] = json!(REGISTRY_04);
        let shown = sheet(&to_registry, &report, None, "LEZ 0.4", &own(), &[]);
        assert_eq!(shown["refused"], true, "{shown}");
        assert!(shown["why"][0].as_str().unwrap().contains("Names registry"), "{shown}");
        let mut actor = fine.clone();
        actor["accounts"] = json!([{ "account": OTHER, "program": TOKENS }]);
        assert_eq!(sheet(&actor, &report_for(&actor), None, "LEZ 0.4", &own(), &[])["refused"], true);
        let bridged = "4VScJ7mtwaanQ5gcSZZw2iAMHYAbLrVmbYKwEGx2eSex";
        let mut tokens_root = fine.clone();
        tokens_root["program"] = json!(bridged);
        let mut report = report_for(&tokens_root);
        report["program"] = json!(bridged);
        assert_eq!(sheet(&tokens_root, &report, None, "LEZ 0.4", &own(), &[(bridged.to_owned(), "bridged_tokens")])["refused"], true);
    }
}
