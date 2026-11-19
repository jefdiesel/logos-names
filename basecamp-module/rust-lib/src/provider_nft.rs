//! NFTs for other apps through the Wallet Provider SDK (LP-0001).
//!
//! Two capabilities, each granted and revoked on its own, and kept apart from account access
//! (`provider.rs`) and from any future membership-proof request:
//!
//! - **NFT list** (`request_nft_access`): the person ticks which accounts' NFTs the app may
//!   see. With it, `granted_nfts` lists what those accounts hold (collection, master or copy,
//!   metadata) and `nft_membership` says whether they hold one of a collection. A private
//!   account is listed only if the person ticked it (LP-0001: "cannot enumerate a private
//!   account's NFTs without an explicit user approval for that account").
//! - **NFT transfer** (`propose_nft_transfer`): one NFT, from an account the app was given for
//!   the list, to an LNS name or `Public/<id>`. The person approves it on the same sheet the
//!   Names app shows for its own Send (`Names::nft_sheet_now`), and it starts only with that
//!   sheet's digest (`Names::nft_start_now`): the two steps are the same by construction. There
//!   is no collection-wide approval.
//!
//! The queue is its own file (`provider-nft.json`, 0600, written atomically), with the same
//! limits as the other requests, so the existing approval path is not changed.

use crate::nft;
use crate::{cli, provider, registry, Names};
use serde_json::{json, Map, Value};
use sha2::Digest as _;
use std::path::PathBuf;
#[cfg(test)]
use std::path::Path;

pub const NFT_LIST: &str = "nft_list";
pub const NFT_TRANSFER: &str = "nft_transfer";
/// A request for the transfer capability (`request_nft_transfers`).
pub const NFT_TRANSFERS: &str = "nft_transfers";
/// The membership-proof capability (provider_proof.rs): its own grant, kept here.
pub const MEMBERSHIP_PROOF: &str = "membership_proof";
/// What each capability lets the app learn or do, in the words the grants list uses.
pub const TRANSFERS_DISCLOSES: &str = "may ask you to send an NFT from an account it can list; you approve each one";
pub const PROOF_DISCLOSES: &str = "may ask you to prove you hold an NFT of a collection; learns a pass id, nothing about your accounts";
/// What the NFT list discloses, in the words the grant is shown with.
pub const LIST_DISCLOSES: &str = "which NFTs these accounts hold, with their collections and metadata";

fn path() -> Option<PathBuf> {
    crate::config_dir().map(|dir| dir.join("provider-nft.json"))
}

/// The NFT requests and grants, kept apart from the other provider requests.
#[derive(Debug, Default)]
pub struct Store {
    pub requests: Vec<Value>,
    pub grants: Map<String, Value>,
    /// Capabilities besides the list, by app: `{app: {nft_transfers: {since}, membership_proof: {since}}}`.
    pub capabilities: Map<String, Value>,
    /// The per-install secret that makes a private account's per-app handle (hex, 32 bytes).
    handle_secret: Option<String>,
    path: Option<PathBuf>,
}

impl Store {
    pub fn load(path: Option<PathBuf>) -> Store {
        let found: Value = path.as_deref().and_then(|p| std::fs::read(p).ok()).and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
        Store {
            requests: found["requests"].as_array().cloned().unwrap_or_default(),
            grants: found["grants"].as_object().cloned().unwrap_or_default(),
            capabilities: found["capabilities"].as_object().cloned().unwrap_or_default(),
            handle_secret: found["handle_secret"].as_str().map(str::to_owned),
            path,
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let Some(path) = &self.path else { return Ok(()) };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("can't keep NFT requests: {e}"))?;
        }
        cli::write_private(path, json!({ "requests": self.requests, "grants": self.grants, "capabilities": self.capabilities, "handle_secret": self.handle_secret }).to_string().as_bytes()).map_err(|e| format!("can't keep NFT requests: {e}"))
    }

    /// Expires what waited too long, and forgets what was decided long ago.
    pub fn sweep(&mut self, now: u64) {
        for request in &mut self.requests {
            if request["state"] == "pending" && request["expires"].as_u64().is_some_and(|at| at <= now) {
                request["state"] = json!("expired");
                request["decided"] = json!(now);
                if request["kind"] == MEMBERSHIP_PROOF {
                    request["status"] = json!({ "state": "expired" });
                }
            }
        }
        self.requests.retain(|request| request["state"] == "pending" || request["state"] == "running" || request["decided"].as_u64().is_none_or(|at| now.saturating_sub(at) < provider::KEEP_DECIDED_MS));
        while self.requests.len() > provider::MAX_KEPT {
            match self.requests.iter().position(|request| request["state"] != "pending" && request["state"] != "running") {
                Some(at) => {
                    self.requests.remove(at);
                }
                None => break,
            }
        }
    }

    pub fn add(&mut self, request: Value, now: u64) -> Result<Value, String> {
        self.sweep(now);
        let app = request["app"].as_str().unwrap_or_default().to_owned();
        let pending = |app: Option<&str>| self.requests.iter().filter(|r| r["state"] == "pending" && app.is_none_or(|app| r["app"] == app)).count();
        if pending(Some(&app)) >= provider::MAX_PENDING_PER_APP {
            return Err(format!("this app already has {} NFT requests waiting; wait for the person to answer them", provider::MAX_PENDING_PER_APP));
        }
        if pending(None) >= provider::MAX_PENDING {
            return Err("the wallet has too many requests waiting; try again later".to_owned());
        }
        let hour = self.requests.iter().filter(|r| r["app"] == app.as_str() && r["created"].as_u64().is_some_and(|at| now.saturating_sub(at) < 3_600_000)).count();
        if hour >= provider::MAX_PER_HOUR {
            return Err("this app has asked too often in the last hour".to_owned());
        }
        let view = view(&request);
        self.requests.push(request);
        self.save()?;
        Ok(view)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Value> {
        self.requests.iter_mut().find(|request| request["id"] == id)
    }

    /// The accounts `app` may list the NFTs of.
    pub fn granted(&self, app: &str) -> Vec<String> {
        self.grants.get(app).and_then(|grant| grant["accounts"].as_array()).map(|accounts| accounts.iter().filter_map(|a| a.as_str().map(str::to_owned)).collect()).unwrap_or_default()
    }

    /// What `app` is told for one of the accounts it was given: a public account as itself, a
    /// private one as an opaque handle (`Private/~<40 hex>`) made for this app alone, so the app
    /// never learns the private account's ID, which other transactions may name on chain, and two
    /// apps can't compare what they hold (review finding 11).
    pub fn handle(&mut self, app: &str, account: &str) -> String {
        if !account.starts_with("Private/") {
            return account.to_owned();
        }
        if self.handle_secret.is_none() {
            self.handle_secret = registry::fresh_bytes::<32>().ok().map(|b| registry::hex(&b));
        }
        let secret = self.handle_secret.clone().unwrap_or_default();
        let mut hasher = sha2::Sha256::new();
        for part in [b"names-nft-handle/v1".as_slice(), secret.as_bytes(), app.as_bytes(), account.as_bytes()] {
            hasher.update((part.len() as u32).to_le_bytes());
            hasher.update(part);
        }
        let digest = hasher.finalize();
        format!("Private/~{}", &registry::hex(&digest)[..40])
    }

    /// The account a handle (or a public account) stands for, among those `app` was given.
    pub fn unhandle(&mut self, app: &str, shown: &str) -> Option<String> {
        let shown = shown.trim();
        self.granted(app).into_iter().find(|account| self.handle(app, account) == shown)
    }

    /// Cancels `app`'s waiting requests of these kinds (a revoked capability leaves none behind).
    pub fn cancel_pending(&mut self, app: &str, kinds: &[&str], now: u64) {
        for request in self.requests.iter_mut().filter(|r| r["app"] == app && r["state"] == "pending" && kinds.iter().any(|k| r["kind"] == *k)) {
            request["state"] = json!("cancelled");
            request["decided"] = json!(now);
            if request["kind"] == MEMBERSHIP_PROOF {
                request["status"] = json!({ "state": "cancelled" });
            }
        }
    }

    /// Whether `app` holds `capability` (`nft_transfers`, `membership_proof`).
    pub fn has(&self, app: &str, capability: &str) -> bool {
        self.capabilities.get(app).is_some_and(|caps| caps[capability].is_object())
    }

    pub fn give(&mut self, app: &str, capability: &str, now: u64) -> Result<(), String> {
        if !self.capabilities.contains_key(app) && self.capabilities.len() >= provider::MAX_APPS {
            return Err("too many apps hold NFT capabilities; revoke one first".to_owned());
        }
        let caps = self.capabilities.entry(app.to_owned()).or_insert_with(|| json!({}));
        caps[capability] = json!({ "since": now });
        Ok(())
    }

    /// Takes one capability away (`nft_list` is the list grant). Whether there was one.
    pub fn take(&mut self, app: &str, capability: &str) -> bool {
        if capability == NFT_LIST {
            return self.grants.remove(app).is_some();
        }
        let Some(caps) = self.capabilities.get_mut(app).and_then(Value::as_object_mut) else { return false };
        let had = caps.remove(capability).is_some();
        if caps.is_empty() {
            self.capabilities.remove(app);
        }
        had
    }

    pub fn grant(&mut self, app: &str, accounts: &[String], now: u64) -> Result<(), String> {
        if accounts.is_empty() || accounts.len() > provider::MAX_GRANTED {
            return Err(format!("tick 1 to {} accounts", provider::MAX_GRANTED));
        }
        if !self.grants.contains_key(app) && self.grants.len() >= provider::MAX_APPS {
            return Err("too many apps have NFT access; revoke one first".to_owned());
        }
        self.grants.insert(app.to_owned(), json!({ "accounts": accounts, "since": now, "discloses": LIST_DISCLOSES }));
        Ok(())
    }
}

/// What the asking app sees of its request: never the wallet's details.
pub fn view(request: &Value) -> Value {
    let mut out = json!({
        "request": request["id"], "kind": request["kind"], "state": request["state"],
        "created": request["created"], "expires": request["expires"], "decided": request["decided"],
        "error": request["error"],
    });
    if request["kind"] == NFT_LIST {
        // Public accounts as themselves; private ones only counted (handles come from granted_nfts).
        let given = request["accounts"].as_array().cloned().unwrap_or_default();
        out["accounts"] = Value::from(given.iter().filter(|a| a.as_str().is_some_and(|a| a.starts_with("Public/"))).cloned().collect::<Vec<_>>());
        out["private_accounts"] = json!(given.iter().filter(|a| a.as_str().is_some_and(|a| a.starts_with("Private/"))).count());
    } else if request["kind"] == MEMBERSHIP_PROOF {
        for key in ["collection", "program", "purpose", "status", "proof", "code"] {
            out[key] = request[key].clone();
        }
    } else {
        for key in ["program", "destination", "definition", "tx"] {
            out[key] = request[key].clone();
        }
        // The account as the app named it (a private one by its handle), never the real ID.
        out["from"] = if request["from_shown"].is_string() { request["from_shown"].clone() } else { request["from"].clone() };
    }
    out
}

/// A request to be allowed to propose NFT transfers.
pub fn transfers_request(app: &str, reason: &str, now: u64) -> Result<Value, String> {
    let mut request = provider::base(app, NFT_TRANSFERS, now)?;
    request.insert("reason".into(), json!(provider::clean_text(reason, provider::MAX_TEXT)));
    Ok(Value::Object(request))
}

/// A request to list some accounts' NFTs.
pub fn list_request(app: &str, reason: &str, now: u64) -> Result<Value, String> {
    let mut request = provider::base(app, NFT_LIST, now)?;
    request.insert("reason".into(), json!(provider::clean_text(reason, provider::MAX_TEXT)));
    Ok(Value::Object(request))
}

/// A request to send the NFT `from` holds under `program` to `destination` (an LNS name or
/// `Public/<id>`; an app can't name someone's private keys, whose readiness can't be checked).
pub fn transfer_request(app: &str, program: &str, from: &str, destination: &str, summary: &str, now: u64) -> Result<Value, String> {
    let program = registry::account_text(&nft::program_id(program)?);
    let (private, id) = provider::account_ref(from)?;
    let from = provider::account_written(private, &id);
    let destination = match cli::pay_to(destination)? {
        cli::PayTo::Name(name) => name,
        cli::PayTo::Account(id) => format!("Public/{id}"),
    };
    if destination == from {
        return Err("an account can't send to itself".to_owned());
    }
    let mut request = provider::base(app, NFT_TRANSFER, now)?;
    request.insert("program".into(), json!(program));
    request.insert("from".into(), json!(from));
    request.insert("destination".into(), json!(destination));
    request.insert("summary".into(), json!(provider::clean_text(summary, provider::MAX_TEXT)));
    Ok(Value::Object(request))
}

/// The action a transfer request asks for, as the sheet plans it.
pub fn action_of(request: &Value) -> String {
    json!({ "action": "send", "from": request["from"], "to": request["destination"] }).to_string()
}

/// The NFTs of `holdings` (as `nft_holdings` answers) on `accounts`: what holds, never empty
/// or definition states, never another account's.
pub fn nfts_of(programs: &Value, accounts: &[String]) -> Vec<Value> {
    let mut out = Vec::new();
    for program in programs["programs"].as_array().into_iter().flatten() {
        for row in program["holdings"].as_array().into_iter().flatten() {
            let Some(account) = row["account"].as_str() else { continue };
            if row["holds"] != true || !accounts.iter().any(|granted| granted == account) {
                continue;
            }
            out.push(json!({
                "program": program["program"],
                "account": account,
                "account_kind": row["account_kind"],
                "definition": row["definition"],
                "token": row["state"],
                "print_balance": row["print_balance"],
                "collection": row["collection"],
            }));
        }
    }
    out
}

impl Names {
    fn nft_store(&self) -> Store {
        let mut store = Store::load(path());
        store.sweep(provider::now_ms());
        store
    }

    // ── Any app ─────────────────────────────────────────────────────

    pub(crate) fn nft_access_ask(&mut self, reason: &str) -> Result<Value, String> {
        let app = crate::asking_app()?;
        let now = provider::now_ms();
        let mut store = self.nft_store();
        if !store.granted(&app).is_empty() {
            let shown: Vec<String> = store.granted(&app).iter().map(|a| store.handle(&app, a)).collect();
            store.save()?;
            return Ok(json!({ "request": null, "state": "approved", "accounts": shown }));
        }
        if let Some(waiting) = store.requests.iter().find(|r| r["app"] == app.as_str() && r["kind"] == NFT_LIST && r["state"] == "pending") {
            return Ok(view(waiting));
        }
        store.add(list_request(&app, reason, now)?, now)
    }

    /// Asks for the transfer capability, apart from the list.
    pub(crate) fn nft_transfers_ask(&mut self, reason: &str) -> Result<Value, String> {
        let app = crate::asking_app()?;
        let now = provider::now_ms();
        let mut store = self.nft_store();
        if store.has(&app, NFT_TRANSFERS) {
            return Ok(json!({ "request": null, "state": "approved" }));
        }
        if let Some(waiting) = store.requests.iter().find(|r| r["app"] == app.as_str() && r["kind"] == NFT_TRANSFERS && r["state"] == "pending") {
            return Ok(view(waiting));
        }
        store.add(transfers_request(&app, reason, now)?, now)
    }

    /// The NFTs of the accounts the app was given for the list.
    pub(crate) fn nft_granted_list(&mut self) -> String {
        let app = match crate::asking_app() {
            Ok(app) => app,
            Err(why) => return crate::reply(Err(why)),
        };
        let mut store = self.nft_store();
        let accounts = store.granted(&app);
        if accounts.is_empty() {
            return crate::reply(Err("ask to see NFTs first (request_nft_access)".to_owned()));
        }
        let read = self.nft_holdings_now();
        let Ok(parsed) = serde_json::from_str::<Value>(&read) else { return read };
        if parsed["success"] != true {
            // Busy, or the read failed: said as it is, with nothing of the wallet's in it.
            return if parsed["busy"] == true { read } else { crate::reply(Err("the wallet couldn't read the NFTs just now".to_owned())) };
        }
        let mut nfts = nfts_of(&parsed["value"], &accounts);
        for nft in &mut nfts {
            let shown = store.handle(&app, nft["account"].as_str().unwrap_or_default());
            nft["account"] = json!(shown);
        }
        let shown: Vec<String> = accounts.iter().map(|a| store.handle(&app, a)).collect();
        if store.save().is_err() {
            return crate::reply(Err("the wallet couldn't keep its handles".to_owned()));
        }
        crate::reply(Ok(json!({ "accounts": shown, "nfts": nfts })))
    }

    /// Whether the accounts the app was given hold an NFT of `definition` under `program`.
    pub(crate) fn nft_member(&mut self, program: &str, definition: &str) -> String {
        let listed = self.nft_granted_list();
        let Ok(parsed) = serde_json::from_str::<Value>(&listed) else { return listed };
        if parsed["success"] != true {
            return listed;
        }
        let (Ok(program), Ok(definition)) = (nft::program_id(program), registry::parse_account(definition.trim())) else {
            return crate::reply(Err("give the NFT program's and the collection's account IDs".to_owned()));
        };
        let (program, definition) = (registry::account_text(&program), registry::account_text(&definition));
        let count = parsed["value"]["nfts"].as_array().into_iter().flatten().filter(|nft| nft["program"] == program.as_str() && nft["definition"] == definition.as_str()).count();
        crate::reply(Ok(json!({ "member": count > 0, "count": count })))
    }

    pub(crate) fn nft_transfer_ask(&mut self, program: &str, from: &str, destination: &str, summary: &str) -> Result<Value, String> {
        let app = crate::asking_app()?;
        let now = provider::now_ms();
        let mut store = self.nft_store();
        // The app names a private account by its handle: turned back into the account here.
        let real = store.unhandle(&app, from).unwrap_or_else(|| from.trim().to_owned());
        if real.starts_with("Private/~") {
            return Err("ask to see that account's NFTs first (request_nft_access), then propose it".to_owned());
        }
        let mut request = transfer_request(&app, program, &real, destination, summary, now)?;
        request["from_shown"] = json!(from.trim());
        if !store.granted(&app).iter().any(|account| request["from"] == account.as_str()) {
            return Err("ask to see that account's NFTs first (request_nft_access), then propose it".to_owned());
        }
        if !store.has(&app, NFT_TRANSFERS) {
            return Err("ask to propose NFT transfers first (request_nft_transfers); seeing NFTs doesn't allow it".to_owned());
        }
        // The program must be one the person listed for this network.
        if !self.nft_list()?.iter().any(|entry| entry["program"] == request["program"]) {
            return Err("that NFT program isn't in the person's list for this network".to_owned());
        }
        store.add(request, now)
    }

    pub(crate) fn nft_status(&mut self, id: &str) -> Result<Value, String> {
        let app = crate::asking_app()?;
        let mut store = self.nft_store();
        self.nft_settle(&mut store);
        let request = store.requests.iter().find(|r| r["id"] == id.trim() && r["app"] == app.as_str()).ok_or("there is no such request of this app")?;
        Ok(view(request))
    }

    pub(crate) fn nft_cancel(&mut self, id: &str) -> Result<Value, String> {
        let app = crate::asking_app()?;
        let mut store = self.nft_store();
        let request = store.get_mut(id.trim()).filter(|r| r["app"] == app.as_str()).ok_or("there is no such request of this app")?;
        if request["state"] != "pending" {
            return Err("only a waiting request can be withdrawn".to_owned());
        }
        request["state"] = json!("cancelled");
        request["decided"] = json!(provider::now_ms());
        let out = view(request);
        store.save()?;
        Ok(out)
    }

    pub(crate) fn nft_drop(&mut self) -> Result<Value, String> {
        let app = crate::asking_app()?;
        let mut store = self.nft_store();
        let revoked = store.grants.remove(&app).is_some() | store.capabilities.remove(&app).is_some();
        store.save()?;
        Ok(json!({ "revoked": revoked }))
    }

    /// A running transfer's request follows its job: done (with its transaction) or failed.
    fn nft_settle(&mut self, store: &mut Store) {
        let mut changed = false;
        for request in store.requests.iter_mut().filter(|r| r["state"] == "running") {
            let Some(job) = request["job"].as_str().map(str::to_owned) else { continue };
            let Ok(mut state) = self.jobs().and_then(|jobs| jobs.state(&job)) else { continue };
            self.job_outcome(&mut state);
            match state["state"].as_str() {
                Some("done") => {
                    request["state"] = json!("done");
                    request["tx"] = state["tx"].clone();
                }
                Some("running") => continue,
                _ => {
                    request["state"] = json!("failed");
                    // The plain refusal, never the wallet's own message.
                    request["error"] = json!(state["refusal"].as_str().filter(|why| crate::nft_wire::is_plain(why)).unwrap_or("the transfer didn't go through"));
                }
            }
            request["decided"] = json!(provider::now_ms());
            changed = true;
        }
        if changed {
            let _ = store.save();
        }
    }

    // ── The Names app ───────────────────────────────────────────────

    pub(crate) fn nft_pending(&mut self) -> Result<Value, String> {
        let mut store = self.nft_store();
        self.nft_settle(&mut store);
        let mut out = Vec::new();
        for request in store.requests.iter().filter(|r| r["state"] == "pending") {
            let mut shown = request.clone();
            shown["publisher"] = json!(self.publisher_of(request["app"].as_str().unwrap_or_default()));
            let checking = self.still_checking(request);
            shown["checking"] = json!(checking.is_some());
            shown["checking_what"] = json!(checking);
            if request["kind"] == NFT_LIST {
                shown["discloses"] = json!(LIST_DISCLOSES);
            } else if request["kind"] == NFT_TRANSFERS {
                shown["discloses"] = json!(TRANSFERS_DISCLOSES);
            } else if request["kind"] == MEMBERSHIP_PROOF {
                shown["discloses"] = json!(PROOF_DISCLOSES);
            } else {
                shown["action"] = json!(action_of(request));
                shown["granted_for"] = json!(store.granted(request["app"].as_str().unwrap_or_default()));
            }
            out.push(shown);
        }
        Ok(Value::from(out))
    }

    /// Approves an NFT request: a list with the accounts ticked (comma-separated), a transfer
    /// with the digest of the sheet the person saw and the private payer they chose, if any.
    pub(crate) fn nft_approve(&mut self, id: &str, accounts: &str, digest: &str, fee_payer: &str) -> Result<Value, String> {
        let now = provider::now_ms();
        let mut store = self.nft_store();
        let request = store.requests.iter().find(|r| r["id"] == id.trim()).cloned().ok_or("there is no such request")?;
        if request["state"] != "pending" {
            return Err(format!("that request is {} already", request["state"].as_str().unwrap_or("decided")));
        }
        let app = request["app"].as_str().unwrap_or_default().to_owned();
        // As every other request: never approved while the Program Registry is still asked who
        // publishes the app (or about the program a transfer runs), rule M-2 (review finding 9).
        if let Some(what) = self.still_checking(&request) {
            return Err(format!("still checking {what} with the Program Registry; try again in a few seconds"));
        }
        if request["kind"] == NFT_LIST {
            let mut ticked = Vec::new();
            for account in accounts.split(',').map(str::trim).filter(|a| !a.is_empty()) {
                let (private, id) = provider::account_ref(account)?;
                let written = provider::account_written(private, &id);
                if !ticked.contains(&written) {
                    ticked.push(written);
                }
            }
            let mine = self.nft_accounts_of_wallet()?;
            if let Some(stranger) = ticked.iter().find(|account| !mine.contains(account)) {
                return Err(format!("{stranger} isn't an account of this wallet"));
            }
            store.grant(&app, &ticked, now)?;
            let entry = store.get_mut(id.trim()).expect("found above");
            entry["state"] = json!("approved");
            entry["accounts"] = json!(ticked);
            entry["decided"] = json!(now);
            let out = view(entry);
            store.save()?;
            return Ok(out);
        }
        if request["kind"] == NFT_TRANSFERS {
            store.give(&app, NFT_TRANSFERS, now)?;
            let entry = store.get_mut(id.trim()).expect("found above");
            entry["state"] = json!("approved");
            entry["decided"] = json!(now);
            let out = view(entry);
            store.save()?;
            return Ok(out);
        }
        if request["kind"] == MEMBERSHIP_PROOF {
            return Err("a membership proof is approved on its own sheet (approve_membership_proof)".to_owned());
        }
        // A transfer: still within the access the app has now.
        if !store.granted(&app).iter().any(|account| request["from"] == account.as_str()) || !store.has(&app, NFT_TRANSFERS) {
            return Err("the app no longer may propose NFT transfers from that account".to_owned());
        }
        let program = request["program"].as_str().unwrap_or_default().to_owned();
        let started = self.nft_start_now(&program, &action_of(&request), digest, fee_payer)?;
        let entry = store.get_mut(id.trim()).expect("found above");
        entry["state"] = json!("running");
        entry["job"] = started["id"].clone();
        entry["decided"] = json!(now);
        let out = view(entry);
        store.save()?;
        Ok(json!({ "request": out, "job": started }))
    }

    pub(crate) fn nft_reject(&mut self, id: &str) -> Result<Value, String> {
        let mut store = self.nft_store();
        let request = store.get_mut(id.trim()).ok_or("there is no such request")?;
        if request["state"] != "pending" {
            return Err(format!("that request is {} already", request["state"].as_str().unwrap_or("decided")));
        }
        request["state"] = json!("rejected");
        request["decided"] = json!(provider::now_ms());
        if request["kind"] == MEMBERSHIP_PROOF {
            request["status"] = json!({ "state": "rejected" });
        }
        let out = view(request);
        store.save()?;
        Ok(out)
    }

    pub(crate) fn nft_grants_shown(&mut self) -> Value {
        let store = self.nft_store();
        let mut rows: Vec<Value> = store.grants.iter().map(|(app, grant)| json!({ "app": app, "accounts": grant["accounts"], "since": grant["since"], "capability": NFT_LIST, "discloses": LIST_DISCLOSES })).collect();
        for (app, caps) in &store.capabilities {
            for (capability, discloses) in [(NFT_TRANSFERS, TRANSFERS_DISCLOSES), (MEMBERSHIP_PROOF, PROOF_DISCLOSES)] {
                if caps[capability].is_object() {
                    rows.push(json!({ "app": app, "accounts": null, "since": caps[capability]["since"], "capability": capability, "discloses": discloses }));
                }
            }
        }
        Value::from(rows)
    }

    /// Takes one capability from an app (`nft_list`, `nft_transfers`, `membership_proof`).
    pub(crate) fn nft_revoke_one(&mut self, app: &str, capability: &str) -> Result<Value, String> {
        if ![NFT_LIST, NFT_TRANSFERS, MEMBERSHIP_PROOF].contains(&capability.trim()) {
            return Err("a capability is nft_list, nft_transfers or membership_proof".to_owned());
        }
        let mut store = self.nft_store();
        let revoked = store.take(app.trim(), capability.trim());
        let kinds: &[&str] = match capability.trim() {
            NFT_LIST => &[NFT_LIST],
            NFT_TRANSFERS => &[NFT_TRANSFERS, NFT_TRANSFER],
            _ => &[MEMBERSHIP_PROOF],
        };
        store.cancel_pending(app.trim(), kinds, provider::now_ms());
        store.save()?;
        Ok(json!({ "revoked": revoked }))
    }

    pub(crate) fn nft_revoke(&mut self, app: &str) -> Result<Value, String> {
        let mut store = self.nft_store();
        let revoked = store.grants.remove(app.trim()).is_some() | store.capabilities.remove(app.trim()).is_some();
        store.cancel_pending(app.trim(), &[NFT_LIST, NFT_TRANSFERS, NFT_TRANSFER, MEMBERSHIP_PROOF], provider::now_ms());
        store.save()?;
        Ok(json!({ "revoked": revoked }))
    }

    /// Every account of this wallet, written `Public/<id>` or `Private/<id>`, from the last
    /// NFT read (it lists every account, empty ones too).
    fn nft_accounts_of_wallet(&mut self) -> Result<Vec<String>, String> {
        let read: Value = serde_json::from_str(&self.nft_holdings_now()).map_err(|_| "the wallet couldn't list its accounts".to_owned())?;
        if read["success"] != true {
            return Err(read["error"].as_str().unwrap_or("the wallet couldn't list its accounts").to_owned());
        }
        let mut out: Vec<String> = Vec::new();
        for program in read["value"]["programs"].as_array().into_iter().flatten() {
            for row in program["holdings"].as_array().into_iter().flatten() {
                if let Some(account) = row["account"].as_str() {
                    if !out.iter().any(|known| known == account) {
                        out.push(account.to_owned());
                    }
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
pub fn store_at(path: &Path) -> Store {
    Store::load(Some(path.to_path_buf()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROGRAM: &str = "ErycMYqXUUtducaYmpHZjRgdbLXchrMiV4NZRnmRDNmu";
    const BOB: &str = "HLEwXNBDYx6GS9H5CsawDrMTptD6R4XsJywFEMr6eim2";

    #[test]
    fn a_transfer_request_names_one_nft_and_a_checkable_destination() {
        let request = transfer_request("gallery_ui", PROGRAM, &format!("Private/{BOB}"), "Alice", "gift\u{202E}", 1).unwrap();
        assert_eq!((request["kind"].as_str(), request["destination"].as_str(), request["summary"].as_str()), (Some(NFT_TRANSFER), Some("alice"), Some("gift")));
        assert_eq!(action_of(&request), json!({ "action": "send", "from": format!("Private/{BOB}"), "to": "alice" }).to_string());
        // Someone's private keys, a bare ID, itself: refused.
        for bad in [format!("{}:{}", "ab".repeat(32), "cd".repeat(8)), BOB.to_owned(), format!("Private/{BOB}")] {
            assert!(transfer_request("gallery_ui", PROGRAM, &format!("Private/{BOB}"), &bad, "", 1).is_err(), "{bad}");
        }
        assert!(transfer_request("gallery_ui", "nope", &format!("Public/{BOB}"), "alice", "", 1).is_err());
        // What the app sees: never the job or the summary's source.
        let shown = view(&request);
        assert!(shown.get("job").is_none() && shown.get("summary").is_none());
    }

    #[test]
    fn the_queue_is_bounded_and_grants_are_per_app() {
        let dir = std::env::temp_dir().join(format!("names-provider-nft-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut store = store_at(&dir.join("provider-nft.json"));
        for n in 0..provider::MAX_PENDING_PER_APP {
            store.add(list_request("a_ui", &format!("{n}"), 10).unwrap(), 10).unwrap();
        }
        assert!(store.add(list_request("a_ui", "more", 10).unwrap(), 10).unwrap_err().contains("waiting"));
        // Expired after ten minutes.
        store.sweep(10 + provider::PENDING_FOR_MS);
        assert!(store.requests.iter().all(|r| r["state"] == "expired"));
        assert!(store.grant("a_ui", &[], 1).is_err());
        store.grant("a_ui", &[format!("Public/{BOB}")], 1).unwrap();
        assert_eq!(store.granted("a_ui"), vec![format!("Public/{BOB}")]);
        assert!(store.granted("b_ui").is_empty());
        store.save().unwrap();
        assert_eq!(store_at(&dir.join("provider-nft.json")).granted("a_ui").len(), 1, "kept across a restart");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_app_sees_only_the_nfts_of_accounts_it_was_given() {
        let programs = json!({ "programs": [{ "program": PROGRAM, "holdings": [
            { "account": format!("Public/{BOB}"), "holds": true, "state": "copy", "definition": "D", "collection": { "name": "Art" } },
            { "account": format!("Private/{BOB}"), "holds": true, "state": "master", "definition": "D" },
            { "account": format!("Public/{PROGRAM}"), "holds": false, "state": "empty" },
        ] }] });
        let shown = nfts_of(&programs, &[format!("Public/{BOB}"), format!("Public/{PROGRAM}")]);
        assert_eq!(shown.len(), 1, "the private one wasn't ticked; the empty one holds nothing");
        assert_eq!(shown[0]["token"], "copy");
    }
}
