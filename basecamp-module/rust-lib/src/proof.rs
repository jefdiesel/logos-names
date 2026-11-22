//! The private ownership proof's rules on the wallet's side (LP-0001; spec
//! `~/logos/LP0001-OWNERSHIP-PROOF-SPEC.md` §2.3-§2.7, §4).
//!
//! A port, with no new dependency, of the parts of the gate repo the wallet needs
//! (`lez-nft-gate` at f3e25ba: `gate_core` derivations, state layout and time rule;
//! `gate_wallet_rules` plan, publish bytes, the fail-closed outcome check, pre-checks and the
//! wallet's errors). The tests check it against the gate's own vector file
//! (`testdata/gate-vectors.json`, copied from `vectors/gate-vectors.json` there) and the gate's
//! own outcome-check cases, so the two cannot drift silently. When the gate repo is published,
//! this file is replaced by a dependency on those two crates.
//!
//! OWNER DECISIONS (spec §9), set to the spec's defaults and to be confirmed before the gate is
//! deployed (they are fixed forever there): `QUANT_MS` 10 minutes, the TTL menu 1 h / 6 h /
//! 24 h, a master counts as holding (`MASTER_COUNTS`), the slot keyed per holding (not per
//! holding and app), the fee paid by the holding itself by default, and the acting account of an
//! on-chain use picked by the person inside the wallet.

use sha2::{Digest as _, Sha256};
use std::collections::BTreeMap;

pub type Bytes32 = [u8; 32];

/// OWNER DECISION (Q1): the grid of `not_before`, 10 minutes.
pub const QUANT_MS: u64 = 600_000;
/// OWNER DECISION (Q1): the validity menu, 1 h, 6 h, 24 h.
pub const TTL_MS: [u64; 3] = [3_600_000, 21_600_000, 86_400_000];
/// OWNER DECISION (Q2): a master counts as holding its collection.
pub const MASTER_COUNTS: bool = true;
/// OWNER DECISION (Q5): the holding pays its own fee unless the person picks another payer.
pub const HOLDING_PAYS_BY_DEFAULT: bool = true;
pub const DOMAIN_SLOT: &[u8] = b"lp0001-gate/v1/slot";
pub const DOMAIN_CTX: &[u8] = b"lp0001-gate/v1/ctx";
pub const DOMAIN_BIND: &[u8] = b"lp0001-gate/v1/bind";
/// `AccountId::for_public_pda`'s prefix (LEZ rc3), as the gate reimplements it.
pub const LEE_PDA_PREFIX: &[u8; 32] = b"/LEE/v0.2/AccountId/PDA/\x00\x00\x00\x00\x00\x00\x00\x00";
pub const KIND_ONCHAIN: u8 = 0x01;
pub const KIND_OFFCHAIN: u8 = 0x02;
pub const STATE_VERSION: u8 = 1;
pub const TAG_HOLDER: u8 = 0;
pub const TAG_FACT: u8 = 1;
pub const NATIVE_TOKEN_PROGRAM: Bytes32 = [0; 32];
/// Total memory a machine needs to prove: a 16 GB desktop (macOS reports it as 16 GiB).
pub const MIN_PROVING_MEMORY_BYTES: u64 = 15 * 1024 * 1024 * 1024;

fn sha(parts: &[&[u8]]) -> Bytes32 {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize().into()
}

pub fn slot(secret: &Bytes32) -> Bytes32 {
    sha(&[DOMAIN_SLOT, secret])
}

pub fn slot_pda(gate: &Bytes32, slot: &Bytes32) -> Bytes32 {
    sha(&[LEE_PDA_PREFIX, gate, slot])
}

pub fn context(kind: u8, zone_id: &Bytes32, gate: &Bytes32, audience: &Bytes32, payload: &Bytes32) -> Bytes32 {
    sha(&[DOMAIN_CTX, &[kind], zone_id, gate, audience, payload])
}

pub fn binding(nft_program: &Bytes32, collection: &Bytes32, context: &Bytes32) -> Bytes32 {
    sha(&[DOMAIN_BIND, nft_program, collection, context])
}

pub fn payload_onchain(submission_id: &Bytes32, actor: &Bytes32, nonce: u64) -> Bytes32 {
    sha(&[submission_id, actor, &nonce.to_le_bytes()])
}

pub fn payload_offchain(purpose: &[u8], nonce: &Bytes32) -> Bytes32 {
    sha(&[purpose, nonce])
}

/// The off-chain audience: a hash of the caller as the wallet's provider names it. The wallet
/// fills it in from `current_caller()`; the dApp cannot set it.
pub fn audience_offchain(caller_identity: &[u8]) -> Bytes32 {
    sha(&[caller_identity])
}

pub fn context_onchain(zone_id: &Bytes32, gate: &Bytes32, consumer: &Bytes32, submission_id: &Bytes32, actor: &Bytes32, nonce: u64) -> Bytes32 {
    context(KIND_ONCHAIN, zone_id, gate, consumer, &payload_onchain(submission_id, actor, nonce))
}

pub fn context_offchain(zone_id: &Bytes32, gate: &Bytes32, caller_identity: &[u8], purpose: &[u8], nonce: &Bytes32) -> Bytes32 {
    context(KIND_OFFCHAIN, zone_id, gate, &audience_offchain(caller_identity), &payload_offchain(purpose, nonce))
}

/// A planned request: the window the wallet asks the gate for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    pub not_before: u64,
    pub ttl_class: u8,
    pub expires_at: u64,
}

/// The wallet rule: `not_before = floor(now, QUANT_MS)`, the smallest TTL class that covers the
/// request. `None` above the largest class.
pub fn plan(now_ms: u64, requested_ttl_ms: u64) -> Option<Plan> {
    let not_before = now_ms - now_ms % QUANT_MS;
    let ttl_class = TTL_MS.iter().position(|ttl| *ttl >= requested_ttl_ms)? as u8;
    let expires_at = not_before.checked_add(TTL_MS[usize::from(ttl_class)])?;
    Some(Plan { not_before, ttl_class, expires_at })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fact {
    pub binding: Bytes32,
    pub not_before: u64,
    pub expires_at: u64,
}

impl Fact {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![TAG_FACT, STATE_VERSION];
        out.extend_from_slice(&self.binding);
        out.extend_from_slice(&self.not_before.to_le_bytes());
        out.extend_from_slice(&self.expires_at.to_le_bytes());
        out
    }
}

/// A gate state, as far as the wallet reads one: the holder's (idle, or with a proof pending)
/// or a slot's fact. Strict, as the gate's own decoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateState {
    Holder { secret: Bytes32, pending: bool },
    Fact(Fact),
}

fn u64_at(bytes: &[u8]) -> u64 {
    let mut buf = [0_u8; 8];
    buf.copy_from_slice(bytes);
    u64::from_le_bytes(buf)
}

impl GateState {
    pub fn decode(bytes: &[u8]) -> Option<GateState> {
        match bytes.first()? {
            &TAG_FACT if bytes.len() == 50 && bytes[1] == STATE_VERSION => {
                let mut binding = [0; 32];
                binding.copy_from_slice(&bytes[2..34]);
                let fact = Fact { binding, not_before: u64_at(&bytes[34..42]), expires_at: u64_at(&bytes[42..50]) };
                let shaped = fact.not_before % QUANT_MS == 0 && fact.expires_at.checked_sub(fact.not_before).is_some_and(|ttl| TTL_MS.contains(&ttl));
                shaped.then_some(GateState::Fact(fact))
            }
            &TAG_HOLDER if bytes.len() >= 35 && bytes[1] == STATE_VERSION => {
                let mut secret = [0; 32];
                secret.copy_from_slice(&bytes[2..34]);
                match (bytes[34], bytes.len()) {
                    (0, 35) => Some(GateState::Holder { secret, pending: false }),
                    (1, 115) => Some(GateState::Holder { secret, pending: true }),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

/// The borsh bytes of `GateInternal::Publish { slot, fact }`: the one public write a correct
/// `Prove` makes.
pub fn publish_message(slot: &Bytes32, fact: &Fact) -> Vec<u8> {
    let mut out = vec![0];
    out.extend_from_slice(slot);
    out.extend_from_slice(&fact.binding);
    out.extend_from_slice(&fact.not_before.to_le_bytes());
    out.extend_from_slice(&fact.expires_at.to_le_bytes());
    out
}

/// The wallet's distinct errors (spec §4.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalletError {
    NotAMember,
    FeeReserve,
    AccountBusy,
    OutOfWindow,
    ProverOutOfMemory,
    RpcUnreachable,
    GateNotPinned,
    UnexpectedOutcome,
}

impl WalletError {
    pub const ALL: [WalletError; 8] = [
        WalletError::NotAMember,
        WalletError::FeeReserve,
        WalletError::AccountBusy,
        WalletError::OutOfWindow,
        WalletError::ProverOutOfMemory,
        WalletError::RpcUnreachable,
        WalletError::GateNotPinned,
        WalletError::UnexpectedOutcome,
    ];

    pub const fn code(self) -> &'static str {
        match self {
            Self::NotAMember => "NOT_A_MEMBER",
            Self::FeeReserve => "FEE_RESERVE",
            Self::AccountBusy => "ACCOUNT_BUSY",
            Self::OutOfWindow => "OUT_OF_WINDOW",
            Self::ProverOutOfMemory => "PROVER_OUT_OF_MEMORY",
            Self::RpcUnreachable => "RPC_UNREACHABLE",
            Self::GateNotPinned => "GATE_NOT_PINNED",
            Self::UnexpectedOutcome => "UNEXPECTED_OUTCOME",
        }
    }

    /// What the person reads, and what the app gets as `plain_message`.
    pub const fn plain(self) -> &'static str {
        match self {
            Self::NotAMember => "This account holds no NFT of that collection, so there is nothing to prove. Nothing was sent.",
            Self::FeeReserve => "The account paying the fee doesn't hold the fee reserve. Fund it first; that is one more private transaction.",
            Self::AccountBusy => "Your account changed while proving, or is busy with another transaction; try again.",
            Self::OutOfWindow => "The proof landed too late for its validity window; prove again.",
            Self::ProverOutOfMemory => "This computer doesn't have enough memory to make the proof (a 16 GB machine is needed).",
            Self::RpcUnreachable => "Could not reach the chain. Nothing was sent.",
            Self::GateNotPinned => "The membership gate on this network isn't the pinned, unchangeable one (or none is set up yet). Nothing was sent.",
            Self::UnexpectedOutcome => "The proven transaction would do something other than write one pass and pay the fee. It was stopped and not sent.",
        }
    }

    pub fn from_code(code: &str) -> Option<WalletError> {
        Self::ALL.into_iter().find(|e| e.code() == code)
    }
}

/// Maps a refusal text from the wallet crate, the simulation or the sequencer onto an error.
pub fn classify(text: &str) -> Option<WalletError> {
    let t = text;
    if t.contains("UNEXPECTED_OUTCOME") {
        Some(WalletError::UnexpectedOutcome)
    } else if t.contains("NOT_A_MEMBER") {
        Some(WalletError::NotAMember)
    } else if t.contains("OutOfValidityWindow") || t.contains("validity window") {
        Some(WalletError::OutOfWindow)
    } else if t.contains("Nullifier already seen") || t.contains("nullifier") && t.contains("seen") {
        Some(WalletError::AccountBusy)
    } else if t.contains("Fee reserve") || t.contains("fee reserve") || t.contains("Can not pay") {
        Some(WalletError::FeeReserve)
    } else if t.contains("GATE_NOT_PINNED") {
        Some(WalletError::GateNotPinned)
    } else if t.contains("Connection refused") || t.contains("error trying to connect") || t.contains("RPC_UNREACHABLE") {
        Some(WalletError::RpcUnreachable)
    } else if t.contains("out of memory") || t.contains("memory allocation") {
        Some(WalletError::ProverOutOfMemory)
    } else {
        None
    }
}

// ── The fail-closed outcome check (spec §4.4 item 6) ────────────────────

pub type ActorId = (Bytes32, Bytes32);

/// One crossing of the private/public boundary in the proven transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    PrivateToPublic { to: ActorId, message: Vec<u8> },
    PublicToPrivate { to: ActorId, message: Vec<u8> },
    EndPrivateSubtree,
    EndPublicSubtree,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedPublic {
    pub public_root: bool,
    pub public_actors: Vec<ActorId>,
    pub boundary: Vec<Step>,
    pub window: (Option<u64>, Option<u64>),
    pub async_messages: usize,
}

pub type States = BTreeMap<Bytes32, Vec<u8>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedPrivate {
    pub holder_before: States,
    pub holder_after: States,
    pub payer: Option<(States, States)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expected {
    pub gate: Bytes32,
    pub nft_program: Bytes32,
    pub slot: Bytes32,
    pub fact: Fact,
    pub secret: Bytes32,
    pub fee_inbox: Bytes32,
    pub max_fee: u128,
    /// Pending native credits to the holder that the wallet may claim before the root (refunds
    /// of earlier fees): its balance may rise by at most this.
    pub holder_credit: u128,
    /// The same for a separate payer.
    pub payer_credit: u128,
}

fn balance(states: &States) -> Option<u128> {
    match states.get(&NATIVE_TOKEN_PROGRAM) {
        None => Some(0),
        Some(b) if b.len() == 16 => {
            let mut buf = [0_u8; 16];
            buf.copy_from_slice(b);
            Some(u128::from_le_bytes(buf))
        }
        Some(_) => None,
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Every failure is listed; any failure means the transaction is not sent (`UNEXPECTED_OUTCOME`).
/// The same rules as `gate_wallet_rules::outcome_check`.
pub fn outcome_check(e: &Expected, public: &ObservedPublic, private: &ObservedPrivate) -> Result<(), Vec<String>> {
    let mut bad = Vec::new();
    let pda = slot_pda(&e.gate, &e.slot);
    if public.public_root {
        bad.push("the root message would be public".to_owned());
    }
    for actor in &public.public_actors {
        if *actor != (pda, e.gate) && *actor != (e.fee_inbox, NATIVE_TOKEN_PROGRAM) {
            bad.push(format!("unexpected public actor {}/{}", hex(&actor.0), hex(&actor.1)));
        }
    }
    let expected_publish = publish_message(&e.slot, &e.fact);
    let (mut writes, mut fees) = (0, 0);
    for step in &public.boundary {
        match step {
            Step::PrivateToPublic { to, message } if *to == (pda, e.gate) => {
                writes += 1;
                if *message != expected_publish {
                    bad.push("the slot write is not the expected fact".to_owned());
                }
            }
            Step::PrivateToPublic { to, .. } if *to == (e.fee_inbox, NATIVE_TOKEN_PROGRAM) => fees += 1,
            Step::PrivateToPublic { to, .. } => bad.push(format!("a write to {}/{} besides the slot", hex(&to.0), hex(&to.1))),
            Step::PublicToPrivate { to, .. } => bad.push(format!("a public message into the private part ({}/{})", hex(&to.0), hex(&to.1))),
            Step::EndPrivateSubtree | Step::EndPublicSubtree => {}
        }
    }
    if writes != 1 {
        bad.push(format!("{writes} slot writes, expected exactly 1"));
    }
    if fees != 1 {
        bad.push(format!("{fees} fee credits, expected exactly 1"));
    }
    if public.window != (Some(e.fact.not_before), Some(e.fact.expires_at)) {
        bad.push(format!("window {:?}, expected [{}, {})", public.window, e.fact.not_before, e.fact.expires_at));
    }
    if public.async_messages != 0 {
        bad.push(format!("{} pending asynchronous message(s) left behind", public.async_messages));
    }
    let (hb, ha) = (&private.holder_before, &private.holder_after);
    if hb.get(&e.nft_program) != ha.get(&e.nft_program) {
        bad.push("the holder's NFT actor changed".to_owned());
    }
    match ha.get(&e.gate).map(|b| GateState::decode(b)) {
        Some(Some(GateState::Holder { secret, pending: false })) if secret == e.secret => {}
        other => bad.push(format!("the holder's gate state after is {other:?}, expected an idle Holder with its secret")),
    }
    let programs: std::collections::BTreeSet<&Bytes32> = hb.keys().chain(ha.keys()).collect();
    for p in programs {
        if *p == e.gate || *p == e.nft_program || *p == NATIVE_TOKEN_PROGRAM {
            continue;
        }
        if hb.get(p) != ha.get(p) {
            bad.push(format!("the holder's actor under {} changed", hex(p)));
        }
    }
    // The payer: b - max_fee <= a <= b + its credits. Anyone else: b <= a <= b + its credits.
    let fee_check = |before: &States, after: &States, credit: u128, who: &str, bad: &mut Vec<String>| match (balance(before), balance(after)) {
        (Some(b), Some(a)) if a <= b.saturating_add(credit) && a >= b.saturating_sub(e.max_fee) => {}
        (b, a) => bad.push(format!("{who}'s balance {b:?} -> {a:?} is not a fee of at most {} (claimable credits {credit})", e.max_fee)),
    };
    match &private.payer {
        None => fee_check(hb, ha, e.holder_credit, "the holder", &mut bad),
        Some((pb, pa)) => {
            match (balance(hb), balance(ha)) {
                (Some(b), Some(a)) if a >= b && a <= b.saturating_add(e.holder_credit) => {}
                _ => bad.push("the holder's balance changed though another account pays".to_owned()),
            }
            fee_check(pb, pa, e.payer_credit, "the payer", &mut bad);
            let programs: std::collections::BTreeSet<&Bytes32> = pb.keys().chain(pa.keys()).collect();
            for p in programs {
                if *p != NATIVE_TOKEN_PROGRAM && pb.get(p) != pa.get(p) {
                    bad.push(format!("the payer's actor under {} changed", hex(p)));
                }
            }
        }
    }
    if bad.is_empty() {
        Ok(())
    } else {
        Err(bad)
    }
}

// ── Pre-checks (spec §4.4) ──────────────────────────────────────────────

pub const fn enough_memory(total_bytes: u64) -> bool {
    total_bytes >= MIN_PROVING_MEMORY_BYTES
}

pub const fn covers_reserve(payer_balance: u128, reserve: u128) -> bool {
    payer_balance >= reserve
}

pub fn gate_is_pinned(on_chain_image: &[u32; 8], on_chain_immutable: bool, pinned_image: &[u32; 8], gate_nft_program: &Bytes32, pinned_nft_program: &Bytes32) -> Result<(), String> {
    if on_chain_image != pinned_image {
        return Err("the gate's image id is not the pinned one".to_owned());
    }
    if !on_chain_immutable {
        return Err("the gate's header is not immutable".to_owned());
    }
    if gate_nft_program != pinned_nft_program {
        return Err("the gate serves another NFT program than the pinned one".to_owned());
    }
    Ok(())
}

/// This machine's memory, in bytes (macOS `hw.memsize`, Linux `/proc/meminfo`); `None` when it
/// can't be told, which the pre-check treats as not enough (fail closed).
pub fn machine_memory() -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        let mut size: u64 = 0;
        let mut len = std::mem::size_of::<u64>();
        let name = std::ffi::CString::new("hw.memsize").ok()?;
        // SAFETY: sysctlbyname writes at most `len` bytes into `size`, a u64.
        let rc = unsafe { libc::sysctlbyname(name.as_ptr(), (&mut size as *mut u64).cast(), &mut len, std::ptr::null_mut(), 0) };
        (rc == 0).then_some(size)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let text = std::fs::read_to_string("/proc/meminfo").ok()?;
        let kb: u64 = text.lines().find_map(|l| l.strip_prefix("MemTotal:"))?.trim().trim_end_matches("kB").trim().parse().ok()?;
        Some(kb * 1024)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn h32(text: &str) -> Bytes32 {
        let bytes: Vec<u8> = (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap()).collect();
        bytes.try_into().unwrap()
    }
    fn hexv(text: &str) -> Vec<u8> {
        (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap()).collect()
    }
    fn vectors() -> Value {
        serde_json::from_str(include_str!("../testdata/gate-vectors.json")).unwrap()
    }

    #[test]
    fn the_derivations_match_the_gates_vectors() {
        let v = vectors();
        assert_eq!(v["constants"]["QUANT_MS"], QUANT_MS.to_string());
        assert_eq!(v["constants"]["TTL_MS"], serde_json::json!(TTL_MS.map(|t| t.to_string())));
        assert_eq!(hexv(v["constants"]["LEE_PDA_PREFIX"].as_str().unwrap()), LEE_PDA_PREFIX.to_vec());
        let mut seen = 0;
        for d in v["derivations"].as_array().unwrap() {
            let s = |k: &str| d[k].as_str().unwrap_or_default();
            if d.get("secret").is_some() {
                let sl = slot(&h32(s("secret")));
                assert_eq!(sl, h32(s("slot")), "{}", s("name"));
                assert_eq!(slot_pda(&h32(s("gate")), &sl), h32(s("pda")), "{}", s("name"));
                seen += 1;
            } else if d.get("consumer").is_some() {
                let nonce: u64 = s("nonce").parse().unwrap();
                assert_eq!(payload_onchain(&h32(s("submission_id")), &h32(s("actor")), nonce), h32(s("payload")), "{}", s("name"));
                let ctx = context_onchain(&h32(s("zone_id")), &h32(s("gate")), &h32(s("consumer")), &h32(s("submission_id")), &h32(s("actor")), nonce);
                assert_eq!(ctx, h32(s("context")), "{}", s("name"));
                if d.get("binding").is_some() {
                    assert_eq!(binding(&h32(s("nft_program")), &h32(s("collection")), &ctx), h32(s("binding")), "{}", s("name"));
                }
                seen += 1;
            } else if d.get("caller_identity").is_some() {
                let caller = hexv(s("caller_identity"));
                assert_eq!(audience_offchain(&caller), h32(s("audience")), "{}", s("name"));
                let ctx = context_offchain(&h32(s("zone_id")), &h32(s("gate")), &caller, &hexv(s("purpose")), &h32(s("app_nonce")));
                assert_eq!(ctx, h32(s("context")), "{}", s("name"));
                if d.get("binding").is_some() {
                    assert_eq!(binding(&h32(s("nft_program")), &h32(s("collection")), &ctx), h32(s("binding")), "{}", s("name"));
                }
                seen += 1;
            }
        }
        assert!(seen >= 10, "{seen} derivation vectors checked");
    }

    #[test]
    fn the_states_match_the_gates_vectors() {
        let v = vectors();
        for state in v["states"].as_array().unwrap() {
            let bytes = hexv(state["bytes"].as_str().unwrap());
            match state["kind"].as_str().unwrap() {
                "fact" => {
                    let fact = Fact { binding: h32(state["binding"].as_str().unwrap()), not_before: state["not_before"].as_str().unwrap().parse().unwrap(), expires_at: state["expires_at"].as_str().unwrap().parse().unwrap() };
                    assert_eq!(fact.encode(), bytes);
                    assert_eq!(GateState::decode(&bytes), Some(GateState::Fact(fact)));
                }
                _ => assert!(matches!(GateState::decode(&bytes), Some(GateState::Holder { .. })), "{state}"),
            }
        }
        assert_eq!(GateState::decode(&[]), None);
        assert_eq!(GateState::decode(&[1, 1, 0]), None);
    }

    #[test]
    fn the_plan_follows_the_wallet_rule() {
        let now = 1_763_500_234_567;
        let p = plan(now, 3_600_000).unwrap();
        assert_eq!(p.not_before % QUANT_MS, 0);
        assert!(p.not_before <= now && now - p.not_before < QUANT_MS);
        assert_eq!((p.ttl_class, p.expires_at), (0, p.not_before + 3_600_000));
        assert_eq!(plan(now, 3_600_001).unwrap().ttl_class, 1);
        assert_eq!(plan(now, 86_400_000).unwrap().ttl_class, 2);
        assert_eq!(plan(now, 86_400_001), None);
    }

    // The gate's own outcome-check cases (gate_wallet_rules/src/tests.rs at f3e25ba), ported.
    const GATE: Bytes32 = [0x6A; 32];
    const NFT: Bytes32 = [0xAA; 32];
    const INBOX: Bytes32 = [0xFE; 32];
    const SECRET: Bytes32 = [0x5E; 32];
    const NB: u64 = 1_763_500_200_000;

    fn holder_state(pending: bool) -> Vec<u8> {
        let mut out = vec![TAG_HOLDER, STATE_VERSION];
        out.extend_from_slice(&SECRET);
        if pending {
            out.push(1);
            out.extend_from_slice(&[1; 32]);
            out.extend_from_slice(&[2; 32]);
            out.extend_from_slice(&NB.to_le_bytes());
            out.extend_from_slice(&(NB + 1).to_le_bytes());
        } else {
            out.push(0);
        }
        out
    }

    fn expected() -> Expected {
        Expected { gate: GATE, nft_program: NFT, slot: slot(&SECRET), fact: Fact { binding: [0xB1; 32], not_before: NB, expires_at: NB + 3_600_000 }, secret: SECRET, fee_inbox: INBOX, max_fee: 6_000_000, holder_credit: 0, payer_credit: 0 }
    }

    fn good() -> (ObservedPublic, ObservedPrivate) {
        let e = expected();
        let pda = slot_pda(&GATE, &e.slot);
        let public = ObservedPublic {
            public_root: false,
            public_actors: vec![(pda, GATE), (INBOX, NATIVE_TOKEN_PROGRAM)],
            boundary: vec![
                Step::PrivateToPublic { to: (pda, GATE), message: publish_message(&e.slot, &e.fact) },
                Step::EndPublicSubtree,
                Step::PrivateToPublic { to: (INBOX, NATIVE_TOKEN_PROGRAM), message: vec![1, 2, 3] },
                Step::EndPublicSubtree,
            ],
            window: (Some(NB), Some(NB + 3_600_000)),
            async_messages: 0,
        };
        let mut before = States::new();
        before.insert(NFT, vec![2, 1, 1]);
        before.insert(NATIVE_TOKEN_PROGRAM, 20_000_000_u128.to_le_bytes().to_vec());
        before.insert([0x77; 32], vec![9]);
        let mut after = before.clone();
        after.insert(GATE, holder_state(false));
        after.insert(NATIVE_TOKEN_PROGRAM, (20_000_000_u128 - 5_070_616).to_le_bytes().to_vec());
        (public, ObservedPrivate { holder_before: before, holder_after: after, payer: None })
    }

    fn fails(public: &ObservedPublic, private: &ObservedPrivate, needle: &str) {
        let errs = outcome_check(&expected(), public, private).expect_err("must stop the proof");
        assert!(errs.iter().any(|e| e.contains(needle)), "{needle}: {errs:?}");
    }

    #[test]
    fn the_outcome_check_passes_only_a_correct_prove() {
        let (p, q) = good();
        assert_eq!(outcome_check(&expected(), &p, &q), Ok(()));
        let (mut p1, q1) = good();
        p1.public_root = true;
        fails(&p1, &q1, "root message would be public");
        let (mut p2, q2) = good();
        p2.public_actors.push(([3; 32], NFT));
        fails(&p2, &q2, "unexpected public actor");
        let (mut p3, q3) = good();
        let to = p3.boundary[0].clone();
        p3.boundary.push(to);
        fails(&p3, &q3, "2 slot writes");
        let (mut p4, q4) = good();
        if let Step::PrivateToPublic { message, .. } = &mut p4.boundary[0] {
            message[40] ^= 1;
        }
        fails(&p4, &q4, "not the expected fact");
        let (mut p5, q5) = good();
        p5.window = (None, None);
        fails(&p5, &q5, "window");
        let (mut p6, q6) = good();
        p6.async_messages = 1;
        fails(&p6, &q6, "asynchronous");
        let (p7, mut q7) = good();
        q7.holder_after.insert(NFT, vec![2, 1, 0]);
        fails(&p7, &q7, "NFT actor changed");
        let (p8, mut q8) = good();
        q8.holder_after.insert(GATE, holder_state(true));
        fails(&p8, &q8, "idle Holder");
        let (p9, mut q9) = good();
        q9.holder_after.insert([0x77; 32], vec![8]);
        fails(&p9, &q9, "changed");
        let (p10, mut q10) = good();
        q10.holder_after.insert(NATIVE_TOKEN_PROGRAM, 1_u128.to_le_bytes().to_vec());
        fails(&p10, &q10, "is not a fee");
        let (mut p11, q11) = good();
        p11.boundary.insert(0, Step::PublicToPrivate { to: ([4; 32], GATE), message: vec![] });
        fails(&p11, &q11, "into the private part");
    }

    #[test]
    fn claimed_refunds_may_raise_a_balance_by_at_most_what_was_claimable() {
        let lez = |n: u128| n.to_le_bytes().to_vec();
        let e = Expected { holder_credit: 6_437_240, payer_credit: 3_169_135, ..expected() };
        let (p, good) = good();
        let holder = |after: u128| {
            let mut q = good.clone();
            q.holder_after.insert(NATIVE_TOKEN_PROGRAM, lez(after));
            q
        };
        let most = 20_000_000 + e.holder_credit;
        assert_eq!(outcome_check(&e, &p, &holder(most)), Ok(()));
        assert!(outcome_check(&e, &p, &holder(most + 1)).is_err(), "above its credits");
        assert_eq!(outcome_check(&e, &p, &holder(20_000_000 - e.max_fee)), Ok(()));
        assert!(outcome_check(&e, &p, &holder(20_000_000 - e.max_fee - 1)).is_err(), "more than the fee");
        assert!(outcome_check(&expected(), &p, &holder(20_000_001)).is_err(), "no credits, no rise");

        // Another account pays: the holder may only gain its credits, never lose.
        let paid = |holder_after: u128, payer_after: u128| {
            let mut q = holder(holder_after);
            let of = |n: u128| States::from([(NATIVE_TOKEN_PROGRAM, lez(n))]);
            q.payer = Some((of(9_000_000), of(payer_after)));
            q
        };
        assert_eq!(outcome_check(&e, &p, &paid(most, 9_000_000 + e.payer_credit)), Ok(()));
        assert!(outcome_check(&e, &p, &paid(most + 1, 4_000_000)).is_err(), "holder above its credits");
        assert!(outcome_check(&e, &p, &paid(19_999_999, 4_000_000)).is_err(), "holder lost");
        assert!(outcome_check(&e, &p, &paid(20_000_000, 9_000_001 + e.payer_credit)).is_err(), "payer above its credits");
        assert!(outcome_check(&e, &p, &paid(20_000_000, 9_000_000 - e.max_fee - 1)).is_err(), "payer more than the fee");
    }

    #[test]
    fn errors_have_codes_words_and_classify() {
        for e in WalletError::ALL {
            assert_eq!(WalletError::from_code(e.code()), Some(e));
            assert!(e.plain().ends_with('.'));
        }
        assert_eq!(classify("Guest panicked: NOT_A_MEMBER"), Some(WalletError::NotAMember));
        assert_eq!(classify("Transaction rejected: OutOfValidityWindow"), Some(WalletError::OutOfWindow));
        assert_eq!(classify("Nullifier already seen"), Some(WalletError::AccountBusy));
        assert_eq!(classify("payer holds 3 but the fee reserve is 5070616"), Some(WalletError::FeeReserve));
        assert_eq!(classify("Connection refused (os error 61)"), Some(WalletError::RpcUnreachable));
        assert_eq!(classify("something else"), None);
        assert!(enough_memory(16 * 1024 * 1024 * 1024) && !enough_memory(8 * 1024 * 1024 * 1024));
        assert!(covers_reserve(5_070_616, 5_070_616) && !covers_reserve(5_070_615, 5_070_616));
        assert!(gate_is_pinned(&[1; 8], true, &[1; 8], &NFT, &NFT).is_ok());
        assert!(gate_is_pinned(&[1; 8], false, &[1; 8], &NFT, &NFT).unwrap_err().contains("immutable"));
        assert!(gate_is_pinned(&[2; 8], true, &[1; 8], &NFT, &NFT).is_err());
        assert!(gate_is_pinned(&[1; 8], true, &[1; 8], &GATE, &NFT).is_err());
        assert!(machine_memory().is_some_and(|m| m > 0));
    }
}
