//! The `membership-proof` capability of the Wallet Provider SDK (LP-0001; spec
//! `~/logos/LP0001-OWNERSHIP-PROOF-SPEC.md` §4).
//!
//! Its own grant, apart from account access and from the NFT list: an app that may only ask for
//! proofs learns a pass (`{gate, slot, not_before, expires_at, tx_hash}`) and nothing about the
//! person's accounts, balances or holdings. Every proof is approved on its own sheet (the spec's
//! wording, §4.2) before anything is proven.
//!
//! The flow: `request_membership_proof` (any app, no access needed) → the Names app shows the
//! sheet (`membership_proof_sheet`) → the person picks the private holding and approves
//! (`approve_membership_proof`) → the module's own pre-checks (memory, the account not busy, a
//! gate pinned for this NFT program) → the gate client's pre-checks (the payer's reserve, the
//! gate's image and immutable flag on chain, the simulation: a refusal is `NOT_A_MEMBER`) → the
//! module runs the fail-closed outcome check itself over what the client observed (anything
//! unexpected is `UNEXPECTED_OUTCOME` and nothing is sent) → proving → sending → the block.
//! Status (`membership_proof_status`) says which stage, with the seconds since proving started.
//!
//! The proving step is behind [`GateClient`]. The only production client is [`NoGateClient`]:
//! no gate is pinned on any network yet, so every proof stops at pre-check 4 with
//! `GATE_NOT_PINNED` and nothing is sent. UNRUN: no proof has been made through this path; the
//! real client (the gate repo's, through the names CLI) comes when the gate is deployed.

use crate::proof::{self, Bytes32, WalletError};
use crate::provider_nft::{self, MEMBERSHIP_PROOF};
use crate::{provider, registry, Names};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Mutex;

/// The longest `purpose` an app may give, in characters.
pub const MAX_PURPOSE: usize = 120;

/// What the gate client gets for one proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofJob {
    pub request: String,
    pub nft_program: Bytes32,
    pub gate: Bytes32,
    pub collection: Bytes32,
    /// The private account that holds the NFT, and pays its own fee (owner decision Q5).
    pub holder: Bytes32,
    pub payer: Bytes32,
    pub context: Bytes32,
    pub binding: Bytes32,
    pub plan: proof::Plan,
}

/// What the client found before proving: the reserve it needs and has, and the proven
/// transaction's observed shape, for the module's own outcome check.
#[derive(Debug, Clone)]
pub struct Prepared {
    pub reserve: u128,
    pub payer_balance: u128,
    pub expected: proof::Expected,
    pub public: proof::ObservedPublic,
    pub private: proof::ObservedPrivate,
}

/// The pass an app gets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pass {
    pub slot: Bytes32,
    pub tx_hash: String,
}

/// The stages the client reports while it proves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Proving,
    Sending,
    WaitingForBlock,
}

/// The proving step, behind an interface: the gate repo's client provides it.
pub trait GateClient: Send + Sync {
    /// Pre-checks 2, 4 (on chain) and 5: the payer's reserve, the gate's header, the simulation.
    fn prepare(&self, job: &ProofJob) -> Result<Prepared, WalletError>;
    /// Proves and sends the prepared transaction, reporting stages.
    fn prove_and_send(&self, job: &ProofJob, prepared: &Prepared, stage: &dyn Fn(Stage)) -> Result<Pass, WalletError>;
}

/// The production client today: no gate is pinned, so nothing is proven.
pub struct NoGateClient;

impl GateClient for NoGateClient {
    fn prepare(&self, _: &ProofJob) -> Result<Prepared, WalletError> {
        Err(WalletError::GateNotPinned)
    }
    fn prove_and_send(&self, _: &ProofJob, _: &Prepared, _: &dyn Fn(Stage)) -> Result<Pass, WalletError> {
        Err(WalletError::GateNotPinned)
    }
}

/// A gate pinned for one NFT program on one network: its id and image, and that it is immutable.
/// None is pinned yet; the conductor adds one when the gate is deployed (`set_membership_gate`
/// is not built: this is read from `nft-programs.json`'s `"gates"`, by hand).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatePin {
    pub gate: Bytes32,
    pub image: [u32; 8],
}

fn gate_pin(network: &str, program: &str) -> Option<GatePin> {
    let path = crate::config_dir()?.join("nft-programs.json");
    let all: Value = std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok())?;
    let entry = &all["gates"][network][program];
    let gate = registry::parse_account(entry["gate"].as_str()?).ok()?;
    let words: Vec<u32> = entry["image"].as_array()?.iter().filter_map(|w| w.as_u64().and_then(|w| u32::try_from(w).ok())).collect();
    Some(GatePin { gate, image: words.try_into().ok()? })
}

/// How a proof stands, in the spec's words (§4.1).
#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    AwaitingApproval,
    Checking,
    Proving { started_ms: u64 },
    Sending,
    WaitingForBlock,
    Ready(Value),
    Failed(WalletError),
}

impl Status {
    pub fn view(&self, now_ms: u64) -> Value {
        match self {
            Status::AwaitingApproval => json!({ "state": "awaiting-approval" }),
            Status::Checking => json!({ "state": "checking" }),
            Status::Proving { started_ms } => json!({ "state": "proving", "elapsed_seconds": now_ms.saturating_sub(*started_ms) / 1000 }),
            Status::Sending => json!({ "state": "sending" }),
            Status::WaitingForBlock => json!({ "state": "waiting-for-block" }),
            Status::Ready(proof) => json!({ "state": "ready", "proof": proof }),
            Status::Failed(e) => json!({ "state": "failed", "code": e.code(), "plain_message": e.plain() }),
        }
    }
}

/// Proofs under way, by request id. A proof runs on a thread of its own; its status lives here
/// until the request's record takes it (`settle`). Closing the wallet ends it (spec §4.5).
static RUNS: Mutex<Option<HashMap<String, Status>>> = Mutex::new(None);

fn runs() -> std::sync::MutexGuard<'static, Option<HashMap<String, Status>>> {
    RUNS.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn set_status(id: &str, status: Status) {
    runs().get_or_insert_with(HashMap::new).insert(id.to_owned(), status);
}

fn status_of(id: &str) -> Option<Status> {
    runs().as_ref().and_then(|m| m.get(id).cloned())
}

/// A request to prove membership: checked when it is made.
pub fn proof_request(app: &str, given: &str, now: u64) -> Result<Value, String> {
    let given: Value = serde_json::from_str(given).map_err(|_| "a membership proof request is a JSON object".to_owned())?;
    let object = given.as_object().ok_or("a membership proof request is a JSON object")?;
    for key in object.keys() {
        if !["program", "collection", "purpose", "nonce", "ttl", "onchain"].contains(&key.as_str()) {
            return Err(format!("a membership proof request has no field {}", provider::clean_text(key, 30)));
        }
    }
    if !given["onchain"].is_null() {
        return Err("on-chain use isn't built in this wallet yet: ask for an off-chain proof (no `onchain`)".to_owned());
    }
    let program = registry::account_text(&crate::nft::program_id(given["program"].as_str().unwrap_or_default())?);
    let collection = registry::account_text(&registry::parse_account(given["collection"].as_str().unwrap_or_default().trim()).map_err(|_| "the collection is written as its definition's account ID".to_owned())?);
    let purpose = given["purpose"].as_str().unwrap_or_default();
    if purpose.chars().count() > MAX_PURPOSE || purpose.chars().any(|c| c.is_control()) {
        return Err(format!("the purpose is plain text of at most {MAX_PURPOSE} characters"));
    }
    let nonce = given["nonce"].as_str().unwrap_or_default().trim().to_ascii_lowercase();
    if nonce.len() != 64 || !nonce.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("the nonce is 32 random bytes the app made, as 64 hex characters".to_owned());
    }
    let ttl = given["ttl"].as_u64().unwrap_or(proof::TTL_MS[0]);
    if proof::plan(now, ttl).is_none() {
        return Err(format!("a pass lasts at most {} hours", proof::TTL_MS[2] / 3_600_000));
    }
    let mut request = provider::base(app, MEMBERSHIP_PROOF, now)?;
    request.insert("program".into(), json!(program));
    request.insert("collection".into(), json!(collection));
    request.insert("purpose".into(), json!(purpose));
    request.insert("nonce".into(), json!(nonce));
    request.insert("ttl".into(), json!(ttl));
    request.insert("status".into(), Status::AwaitingApproval.view(now));
    Ok(Value::Object(request))
}

fn bytes32(text: &str) -> Option<Bytes32> {
    if text.len() != 64 {
        return None;
    }
    let v: Vec<u8> = (0..64).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16)).collect::<Result<_, _>>().ok()?;
    v.try_into().ok()
}

/// The zone the wallet is on, as the context names it: its channel id (64 hex), or none known.
fn zone_id(names: &Names) -> Option<Bytes32> {
    names.zones.zone().channel.as_deref().and_then(bytes32)
}

impl Names {
    // ── Any app ─────────────────────────────────────────────────────

    pub(crate) fn proof_ask(&mut self, given: &str) -> Result<Value, String> {
        let app = crate::asking_app()?;
        let now = provider::now_ms();
        let request = proof_request(&app, given, now)?;
        self.nft_listed(request["program"].as_str().unwrap_or_default())?;
        let mut store = provider_nft::Store::load(crate::config_dir().map(|d| d.join("provider-nft.json")));
        store.sweep(now);
        let view = store.add(request, now)?;
        Ok(json!({ "request": view["request"], "state": "awaiting-approval" }))
    }

    /// The asking app's view of its proof: the status, and the pass once ready. Never an account.
    pub(crate) fn proof_status(&mut self, id: &str) -> Result<Value, String> {
        let app = crate::asking_app()?;
        let mut store = provider_nft::Store::load(crate::config_dir().map(|d| d.join("provider-nft.json")));
        store.sweep(provider::now_ms());
        settle(&mut store);
        let request = store.requests.iter().find(|r| r["id"] == id.trim() && r["app"] == app.as_str() && r["kind"] == MEMBERSHIP_PROOF).ok_or("there is no such proof request of this app")?;
        Ok(app_view(request))
    }

    pub(crate) fn proof_cancel(&mut self, id: &str) -> Result<Value, String> {
        let app = crate::asking_app()?;
        let mut store = provider_nft::Store::load(crate::config_dir().map(|d| d.join("provider-nft.json")));
        settle(&mut store);
        let request = store.get_mut(id.trim()).filter(|r| r["app"] == app.as_str() && r["kind"] == MEMBERSHIP_PROOF).ok_or("there is no such proof request of this app")?;
        match request["status"]["state"].as_str() {
            Some("awaiting-approval") => {
                request["state"] = json!("cancelled");
                request["status"] = json!({ "state": "cancelled" });
            }
            // Cancel is allowed until sending (§4.5); the thread checks it before proving.
            Some("checking" | "proving") => {
                request["status"] = json!({ "state": "cancelled" });
                set_status(id.trim(), Status::Failed(WalletError::AccountBusy));
                request["state"] = json!("cancelled");
            }
            other => return Err(format!("a proof that is {} can't be cancelled", other.unwrap_or("decided"))),
        }
        request["decided"] = json!(provider::now_ms());
        let out = app_view(request);
        store.save()?;
        Ok(out)
    }

    // ── The Names app ───────────────────────────────────────────────

    /// The sheet for a proof request, in the spec's words (§4.2): what becomes public, what is
    /// not shown, the time, an earlier pass it replaces, the holdings that can prove it and who
    /// pays, and the program checks.
    pub(crate) fn proof_sheet(&mut self, id: &str) -> Result<Value, String> {
        let store = provider_nft::Store::load(crate::config_dir().map(|d| d.join("provider-nft.json")));
        let request = store.requests.iter().find(|r| r["id"] == id.trim() && r["kind"] == MEMBERSHIP_PROOF).cloned().ok_or("there is no such proof request")?;
        let app = request["app"].as_str().unwrap_or_default().to_owned();
        let program = request["program"].as_str().unwrap_or_default().to_owned();
        let collection = request["collection"].as_str().unwrap_or_default().to_owned();
        // The holdings: private accounts holding an NFT of this collection (spec §4.6: private only).
        let read: Value = serde_json::from_str(&self.nft_holdings_now()).unwrap_or_default();
        if read["busy"] == true {
            return Err(self_busy());
        }
        let mut holdings = Vec::new();
        let mut name = Value::Null;
        let mut public_only = false;
        for p in read["value"]["programs"].as_array().into_iter().flatten().filter(|p| p["program"] == program.as_str()) {
            for row in p["holdings"].as_array().into_iter().flatten().filter(|r| r["holds"] == true && r["definition"] == collection.as_str()) {
                if row["state"] == "master" && !proof::MASTER_COUNTS {
                    continue;
                }
                name = row["collection"]["name"].clone();
                if row["account_kind"] == "private" {
                    holdings.push(row["account"].clone());
                } else {
                    public_only = true;
                }
            }
        }
        let live = store.requests.iter().filter(|r| r["kind"] == MEMBERSHIP_PROOF && r["status"]["state"] == "ready" && r["status"]["proof"]["expires_at"].as_u64().is_some_and(|t| t > provider::now_ms())).map(|r| json!({ "app": r["app"], "holder": r["holder"], "expires_at": r["status"]["proof"]["expires_at"] })).collect::<Vec<_>>();
        let plan = proof::plan(provider::now_ms(), request["ttl"].as_u64().unwrap_or(proof::TTL_MS[0]));
        let label = |p: &str| provider::label_fields(crate::program_registry::label(p).as_ref());
        let gate = gate_pin(&self.nft_network(), &program).map(|pin| registry::account_text(&pin.gate));
        let collection_name = name.as_str().map(|n| provider::clean_text(n, 64)).unwrap_or_else(|| format!("collection {}", collection));
        Ok(json!({
            "request": id.trim(),
            "app": app,
            "publisher": self.publisher_of(&app),
            "first_time": !store.has(&app, MEMBERSHIP_PROOF),
            "title": format!("{app} asks you to prove you hold an NFT of {collection_name}."),
            "purpose": request["purpose"],
            "public": [
                "That someone made a membership proof now. Not which collection, and not for which app.",
                "A pass id. It stays the same for this NFT's account. Apps you show it to can tell it is the same holder as before. They cannot tell who you are.",
                format!("When the pass starts and ends: until {}.", plan.map_or(Value::Null, |p| json!(p.expires_at))),
            ],
            "not_shown": "Which NFT, which account holds it, your balances, your other NFTs.",
            "good_to_know": [
                "It takes about 10 minutes on this computer and uses most of its memory.",
                "It replaces your earlier pass for this NFT, if one is live.",
                "This account can't send anything until it is done.",
                "A fee reserve is held while it runs and mostly returned.",
            ],
            "expires_at": plan.map(|p| p.expires_at),
            "holdings": holdings,
            "public_only": public_only && holdings.is_empty(),
            "live_passes": live,
            "fee_payer": "this NFT's account",
            "checks": { "gate": gate, "gate_label": gate.as_deref().map(label), "nft_program": program, "nft_program_label": label(&program) },
            "owner_decisions": { "quant_ms": proof::QUANT_MS, "ttl_ms": proof::TTL_MS, "master_counts": proof::MASTER_COUNTS, "holding_pays": proof::HOLDING_PAYS_BY_DEFAULT, "slot": "per holding" },
        }))
    }

    /// Approves a proof with the private holding the person picked. Runs the module's own
    /// pre-checks here, then the rest on a thread of its own.
    pub(crate) fn proof_approve(&mut self, id: &str, holder: &str, client: std::sync::Arc<dyn GateClient>) -> Result<Value, String> {
        let now = provider::now_ms();
        let mut store = provider_nft::Store::load(crate::config_dir().map(|d| d.join("provider-nft.json")));
        store.sweep(now);
        let request = store.requests.iter().find(|r| r["id"] == id.trim() && r["kind"] == MEMBERSHIP_PROOF).cloned().ok_or("there is no such proof request")?;
        if request["state"] != "pending" {
            return Err(format!("that request is {} already", request["state"].as_str().unwrap_or("decided")));
        }
        if let Some(what) = self.still_checking(&request) {
            return Err(format!("still checking {what} with the Program Registry; try again in a few seconds"));
        }
        let sheet = self.proof_sheet(id)?;
        let (private, holder_id) = provider::account_ref(holder)?;
        let holder_text = provider::account_written(private, &holder_id);
        if !sheet["holdings"].as_array().is_some_and(|h| h.iter().any(|a| a == holder_text.as_str())) {
            return Err("pick one of your private accounts that holds an NFT of this collection".to_owned());
        }
        let fail = |store: &mut provider_nft::Store, e: WalletError| -> Result<Value, String> {
            let entry = store.get_mut(id.trim()).expect("found above");
            entry["state"] = json!("failed");
            entry["status"] = Status::Failed(e).view(now);
            entry["decided"] = json!(now);
            let out = entry.clone();
            store.save()?;
            Ok(json!({ "request": out["id"], "status": out["status"] }))
        };
        // Pre-check 1: memory.
        if !proof::machine_memory().is_some_and(proof::enough_memory) {
            return fail(&mut store, WalletError::ProverOutOfMemory);
        }
        // Pre-check 3: the account isn't busy (no wallet job, no other proof of this holding).
        let other_proof = store.requests.iter().any(|r| r["kind"] == MEMBERSHIP_PROOF && r["holder"] == holder_text.as_str() && ["checking", "proving", "sending", "waiting-for-block"].contains(&r["status"]["state"].as_str().unwrap_or_default()));
        if other_proof || self.jobs().map(|jobs| !jobs.running().is_empty()).unwrap_or(false) {
            return fail(&mut store, WalletError::AccountBusy);
        }
        // Pre-check 4 (here): a gate is pinned for this NFT program on this network.
        let program_text = request["program"].as_str().unwrap_or_default().to_owned();
        let Some(pin) = gate_pin(&self.nft_network(), &program_text) else {
            return fail(&mut store, WalletError::GateNotPinned);
        };
        let Some(zone) = zone_id(self) else {
            return fail(&mut store, WalletError::RpcUnreachable);
        };
        let nft_program = registry::parse_account(&program_text).map_err(|e| e.to_string())?;
        let collection = registry::parse_account(request["collection"].as_str().unwrap_or_default()).map_err(|e| e.to_string())?;
        let nonce = bytes32(request["nonce"].as_str().unwrap_or_default()).ok_or("a bad nonce")?;
        let app = request["app"].as_str().unwrap_or_default().to_owned();
        // The audience is the caller as the provider named it: the app can't set it.
        let context = proof::context_offchain(&zone, &pin.gate, app.as_bytes(), request["purpose"].as_str().unwrap_or_default().as_bytes(), &nonce);
        let plan = proof::plan(now, request["ttl"].as_u64().unwrap_or(proof::TTL_MS[0])).ok_or("a pass lasts at most 24 hours")?;
        let job = ProofJob {
            request: id.trim().to_owned(),
            nft_program,
            gate: pin.gate,
            collection,
            holder: holder_id,
            payer: holder_id,
            context,
            binding: proof::binding(&nft_program, &collection, &context),
            plan,
        };
        // The capability is given with the first approved proof, and shown and revoked on its own.
        store.give(&app, MEMBERSHIP_PROOF, now)?;
        let entry = store.get_mut(id.trim()).expect("found above");
        entry["state"] = json!("approved");
        entry["holder"] = json!(holder_text);
        entry["decided"] = json!(now);
        entry["status"] = Status::Checking.view(now);
        let out = json!({ "request": entry["id"], "status": entry["status"] });
        store.save()?;
        set_status(id.trim(), Status::Checking);
        run(job, client);
        Ok(out)
    }

    /// The live passes, for the Names app's list (spec §4.5's Passes): app, holder, until when.
    pub(crate) fn proof_passes(&mut self) -> Value {
        let mut store = provider_nft::Store::load(crate::config_dir().map(|d| d.join("provider-nft.json")));
        settle(&mut store);
        Value::from(store.requests.iter().filter(|r| r["kind"] == MEMBERSHIP_PROOF && r["state"] != "pending").map(|r| json!({ "request": r["id"], "app": r["app"], "collection": r["collection"], "holder": r["holder"], "status": r["status"] })).collect::<Vec<_>>())
    }
}

fn self_busy() -> String {
    "still reading your NFTs; this updates in a moment".to_owned()
}

/// Runs a proof on a thread of its own: the client's pre-checks, the module's outcome check, then
/// proving and sending. Every refusal is a named error; nothing is sent after one.
fn run(job: ProofJob, client: std::sync::Arc<dyn GateClient>) {
    let work = move || {
        let id = job.request.clone();
        let cancelled = || matches!(status_of(&id), Some(Status::Failed(_)));
        let prepared = match client.prepare(&job) {
            Ok(prepared) => prepared,
            Err(e) => return set_status(&id, Status::Failed(e)),
        };
        if !proof::covers_reserve(prepared.payer_balance, prepared.reserve) {
            return set_status(&id, Status::Failed(WalletError::FeeReserve));
        }
        // Pre-check 6, here and not in the client: what the transaction would do must be exactly one
        // pass written and the fee paid, for this slot and this binding. Fail closed.
        let mut expected = prepared.expected.clone();
        let mismatch = expected.gate != job.gate || expected.nft_program != job.nft_program || expected.fact.binding != job.binding || expected.fact.not_before != job.plan.not_before || expected.fact.expires_at != job.plan.expires_at;
        expected.gate = job.gate;
        if mismatch || proof::outcome_check(&expected, &prepared.public, &prepared.private).is_err() {
            return set_status(&id, Status::Failed(WalletError::UnexpectedOutcome));
        }
        if cancelled() {
            return;
        }
        set_status(&id, Status::Proving { started_ms: provider::now_ms() });
        let stage = |s: Stage| match s {
            Stage::Proving => {}
            Stage::Sending => set_status(&id, Status::Sending),
            Stage::WaitingForBlock => set_status(&id, Status::WaitingForBlock),
        };
        match client.prove_and_send(&job, &prepared, &stage) {
            Ok(pass) => set_status(
                &id,
                Status::Ready(json!({
                    "gate": registry::account_text(&job.gate),
                    "slot": registry::hex(&pass.slot),
                    "not_before": job.plan.not_before,
                    "expires_at": job.plan.expires_at,
                    "tx_hash": pass.tx_hash,
                })),
            ),
            Err(e) => set_status(&id, Status::Failed(e)),
        }
    };
    if cfg!(test) {
        work();
    } else {
        crate::served::spawn(work);
    }
}

/// Copies what the proof threads found into the requests' records (on the calling thread).
fn settle(store: &mut provider_nft::Store) {
    let now = provider::now_ms();
    let mut changed = false;
    for request in store.requests.iter_mut().filter(|r| r["kind"] == MEMBERSHIP_PROOF) {
        let Some(id) = request["id"].as_str().map(str::to_owned) else { continue };
        let Some(status) = status_of(&id) else { continue };
        let view = status.view(now);
        if request["status"] != view {
            request["status"] = view;
            changed = true;
        }
        match status {
            Status::Ready(_) => request["state"] = json!("done"),
            Status::Failed(_) if request["state"] != "cancelled" => request["state"] = json!("failed"),
            _ => {}
        }
    }
    if changed {
        let _ = store.save();
    }
}

/// What the asking app sees: the status (with the pass once ready), never the holder.
pub fn app_view(request: &Value) -> Value {
    json!({
        "request": request["id"],
        "kind": MEMBERSHIP_PROOF,
        "state": request["state"],
        "collection": request["collection"],
        "status": request["status"],
        "created": request["created"],
        "expires": request["expires"],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROGRAM: &str = "ErycMYqXUUtducaYmpHZjRgdbLXchrMiV4NZRnmRDNmu";
    const ART: &str = "HA47ERcmfSbDChGcYfBcC4SKaPQ8pJJnv4wxCLu1ejL5";

    fn ask(extra: &str) -> String {
        format!(r#"{{"program":"{PROGRAM}","collection":"{ART}","purpose":"Check in","nonce":"{}"{extra}}}"#, "ab".repeat(32))
    }

    #[test]
    fn a_request_is_checked_and_names_no_account() {
        let request = proof_request("door_ui", &ask(""), 1_763_500_234_567).unwrap();
        assert_eq!((request["kind"].as_str(), request["status"]["state"].as_str()), (Some(MEMBERSHIP_PROOF), Some("awaiting-approval")));
        assert_eq!(request["ttl"], proof::TTL_MS[0]);
        let shown = app_view(&request);
        for leak in ["holder", "account", "accounts", "balance"] {
            assert!(shown.get(leak).is_none(), "{leak} in {shown}");
        }
        for bad in [ask(r#","onchain":{"consumer":"x"}"#), ask(r#","ttl":86400001"#), ask(r#","account":"Public/x""#), format!(r#"{{"program":"{PROGRAM}","collection":"{ART}","nonce":"ab"}}"#), "[]".to_owned()] {
            assert!(proof_request("door_ui", &bad, 1).is_err(), "{bad}");
        }
    }

    #[test]
    fn statuses_read_as_the_spec_names_them() {
        assert_eq!(Status::Proving { started_ms: 1_000 }.view(91_000), json!({ "state": "proving", "elapsed_seconds": 90 }));
        assert_eq!(Status::Failed(WalletError::NotAMember).view(0)["code"], "NOT_A_MEMBER");
        assert_eq!(Status::WaitingForBlock.view(0)["state"], "waiting-for-block");
        assert_eq!(NoGateClient.prepare(&ProofJob { request: "x".into(), nft_program: [0; 32], gate: [0; 32], collection: [0; 32], holder: [0; 32], payer: [0; 32], context: [0; 32], binding: [0; 32], plan: proof::plan(0, 1).unwrap() }).unwrap_err(), WalletError::GateNotPinned);
    }
}
