//! `names module private-call plan|send`: a mini-app's privacy-preserving call, from one of
//! this wallet's private accounts across several programs (Wallet Provider SDK,
//! `propose_private_call`; docs/wallet/BRIDGE-SDK.md).
//!
//! The wallet never trusts what the app says a call does. `plan` works it out:
//!
//! 1. **The wallet's own dry run** (`send_tx_with_pre_check` with the wallet in dry-run mode):
//!    the exact transaction the wallet would prove, simulated as the circuit runs it, public
//!    and private parts. A call it refuses is refused here. It also gives the public cycles,
//!    the fee at the chain's head, the reserve the proof holds and the fee payer.
//! 2. **An open replay for the effects**: the same root call run by LEE's own public executor
//!    (`ValidatedStateDiff::from_fee_settlement_invocation`, which takes an authorization set
//!    instead of signatures) on a scratch state holding the programs, the public accounts as the
//!    chain has them now, and the private account as this wallet holds it, with the credits
//!    waiting for it added. Programs can't tell a private actor from a public one (they see the
//!    same pre-state, message, sender and authorization), so what the replay does to each
//!    account is what the proof will do. Asynchronous messages to the account or its declared
//!    one-time addresses stay as publications here; in the proof the wallet receives them
//!    (`select` in the wallet's `send_proven`), so their credits are added to the account.
//!    Anything the replay can't run, or a message to the account it can't read, is refused.
//!
//! The report (one `{"report": …}` line) lists the account's balances before and after (LEZ
//! and tokens of the programs it is told are token programs), the data it keeps for other
//! programs, every public account's change, the one-time addresses that receive, whether the
//! account's ID shows in the public state or events left behind, the fee, and a digest of the
//! effects. `send` plans again, refuses unless the digest is the one the person approved,
//! checks that the proof starts from the states it simulated, then proves and sends.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use anyhow::{Context as _, Result, anyhow, bail};
use lee::{
    Account, V03State, ValidatedStateDiff,
    privacy_preserving_transaction::circuit::ProgramCatalog,
    program::Program,
};
use lee_core::{
    EncryptedNote, RecoveryBinding,
    account::{AccountId, Actor},
    native_token::{self, decode_balance},
    program::{PdaSeed, Publication},
};
use sha2::{Digest as _, Sha256};
use wallet::{AccountIdentity, AccountMention, AsyncDelivery, ExecutionFailureKind};

use crate::{Client, parse_account};

/// What the module sends: the call as `provider_private::cli_input` writes it.
#[derive(serde::Deserialize)]
pub struct CallInput {
    account: String,
    programs: Vec<String>,
    #[serde(default)]
    accounts: Vec<serde_json::Value>,
    message: String,
    #[serde(default)]
    openings: Vec<String>,
    #[serde(default)]
    fee_payer: Option<String>,
    #[serde(default)]
    expect_digest: Option<String>,
    /// The fee's "at most" the person approved (the plan report's `fee.at_most`, which the
    /// Basecamp module passes back on send). The send refuses before proving when the fee at
    /// the chain's head is above it (rfp-022 docs/privacy-m5/repeat-claimers.md, review 4).
    #[serde(default)]
    approved_at_most: Option<String>,
    /// Programs whose actor states are read as the wallet's token program's (same messages and
    /// state as `wallet-tokens/`, which the RFP-022 bridge's token program shares).
    #[serde(default)]
    token_programs: Vec<String>,
}

/// A checked call.
struct Call {
    account: AccountId,
    programs: Vec<AccountId>,
    /// `(actor, how the wallet mentions it, whether it is the person's own)`, the root first.
    mentions: Vec<(Actor, AccountMention, bool)>,
    message: Vec<u8>,
    openings: Vec<[u8; 32]>,
    fee_payer: Option<AccountId>,
    token_programs: Vec<AccountId>,
}

/// What `plan` found.
pub struct Plan {
    pub report: serde_json::Value,
    pub digest: String,
    /// The private account's actor states the replay started from, by program.
    own_pre: BTreeMap<AccountId, Vec<u8>>,
}

fn hex32(text: &str, what: &str) -> Result<[u8; 32]> {
    let bytes = hex::decode(text.trim().trim_start_matches("0x")).with_context(|| format!("{what} is not hex"))?;
    bytes.try_into().map_err(|_| anyhow!("{what} is 32 bytes"))
}

fn native_program() -> AccountId {
    native_token::NATIVE_TOKEN_PROGRAM_ID
}

/// Blocks of base-fee climb a private call's fee reserve is priced for: none, so every private
/// call reserves the fee at the chain's head. An account that holds no LEZ before the call (a
/// first bridge claim, paid from the claim's own sponsor credit) can't reserve more: the LEZ rc3
/// wallet caps the reserve at what the payer holds before the call (`lez/wallet/src/lib.rs:1808`).
/// With the wallet's default 5 blocks an account that already held LEZ reserved 13/8 of it, and
/// the reserve is published, so a repeat claim was told from a first one (rfp-022
/// `docs/privacy-m5/repeat-claimers.md`). The cost is a first claim's: a base fee that rises
/// before inclusion makes the wallet price and prove again (up to twice), or the call is dropped.
pub const PRIVATE_CALL_FEE_HEADROOM_BLOCKS: u64 = 0;

/// The "at most" a private call's sheet shows and the send holds to: the fee after the wallet's
/// configured headroom of full blocks (`fees::private_running`, the wallet's own climb), never
/// below what is reserved or the fee at the head.
pub fn approved_ceiling(quote: &sequencer_service_rpc::FeeStateQuote, configured_blocks: u64, cycles: u64, reserve: u128, about: u128) -> u128 {
    crate::fees::private_running(quote, configured_blocks, cycles).at_most.max(reserve).max(about)
}

/// Before proving: the fee at the chain's head now must still be within what the person
/// approved. A plain refusal otherwise; nothing is proven, sent or spent, and no claim is used up.
pub fn within_approved(approved: Option<&str>, head_now: u128) -> Result<(), String> {
    let Some(text) = approved.map(str::trim).filter(|t| !t.is_empty()) else { return Ok(()) };
    let cap: u128 = text.parse().map_err(|_| "refused: the approved fee is not a number; nothing was proven or sent. Ask again.".to_owned())?;
    if head_now > cap {
        return Err(format!(
            "refused: the chain's fee rose to {head_now} since you approved at most {cap}; nothing was proven or sent, nothing was spent, and the claim or burn can be asked again."
        ));
    }
    Ok(())
}

/// After the send: a plain line when the wallet reserved more than was approved (it priced again
/// while the proof was made, because the chain's fee rose). The fee charged is the fee at
/// inclusion; the rest of the reserve comes back.
pub fn declared_fee_line(approved: Option<&str>, reserve: u128) -> Option<String> {
    let cap: u128 = approved?.trim().parse().ok()?;
    (reserve > cap).then(|| format!(
        "note: the chain's fee rose while the proof was made, so the wallet proved it again and reserved {reserve}, more than the {cap} you approved; what isn't charged comes back"
    ))
}

/// A private call that was sent but is in no block: most often the chain's fee rose past its
/// reserve before a block took it, or another change from the account went first. Either way
/// nothing was spent and no claim or nullifier was used up (a nullifier is spent only by an
/// applied transaction).
pub const PRIVATE_NOT_APPLIED: &str = "the transaction was not applied: it is in no block. Most often the chain's fee rose past what it reserved before a block took it, or another change from the same account went first. Nothing was spent and nothing was used up: ask again.";

/// Puts the wallet's headroom back to its configured value after a private call. Only the
/// headroom is touched, so a fee payer or refund target set on this wallet (`set_private_fee_payer`)
/// stays as it was.
fn restore_fee_headroom<C: wallet::WalletSequencerClient>(wallet: &mut wallet::WalletCore<C>) {
    let configured = wallet.config().fee_headroom_blocks;
    wallet.set_fee_headroom_blocks(configured);
}

/// The fee settings a private call is priced with, as `FeeParams` (the test below checks that only
/// the headroom differs from the defaults; the calls above set just the headroom).
#[cfg(test)]
fn private_call_fee_params() -> wallet::FeeParams {
    wallet::FeeParams { headroom_blocks: Some(PRIVATE_CALL_FEE_HEADROOM_BLOCKS), ..wallet::FeeParams::default() }
}

/// `native` (or `LEZ`) or a program ID.
fn program_of(text: &str) -> Result<AccountId> {
    match text.trim() {
        "native" | "LEZ" => Ok(native_program()),
        other => parse_account(other.strip_prefix("Public/").unwrap_or(other)).map_err(|error| anyhow!(error)),
    }
}

/// Runs `future` with this process's standard output going to a file, and returns what it
/// wrote with the future's value. The wallet's dry run prints its numbers rather than
/// returning them.
async fn captured<T>(future: impl std::future::Future<Output = T>) -> (T, String) {
    use std::io::Write as _;
    use std::os::fd::AsRawFd as _;
    let _ = std::io::stdout().flush();
    let dir = std::env::temp_dir();
    let path = dir.join(format!("names-dry-run-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.subsec_nanos())));
    let file = std::fs::OpenOptions::new().write(true).create_new(true).read(true).open(&path);
    let Ok(file) = file else { return (future.await, String::new()) };
    // SAFETY: plain descriptor calls; stdout is put back before this returns.
    let saved = unsafe { libc::dup(libc::STDOUT_FILENO) };
    if saved < 0 || unsafe { libc::dup2(file.as_raw_fd(), libc::STDOUT_FILENO) } < 0 {
        if saved >= 0 {
            unsafe { libc::close(saved) };
        }
        let _ = std::fs::remove_file(&path);
        return (future.await, String::new());
    }
    let value = future.await;
    let _ = std::io::stdout().flush();
    unsafe {
        libc::dup2(saved, libc::STDOUT_FILENO);
        libc::close(saved);
    }
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let _ = std::fs::remove_file(&path);
    (value, text)
}

/// A number after `label:` in the wallet's dry-run lines (`  fee at head:    5723160`).
fn dry_number(text: &str, label: &str) -> Option<u128> {
    text.lines().find_map(|line| {
        let rest = line.trim().strip_prefix(label)?.trim_start_matches(':').trim();
        rest.split_whitespace().next()?.parse().ok()
    })
}

fn dry_text<'a>(text: &'a str, label: &str) -> Option<&'a str> {
    text.lines().find_map(|line| Some(line.trim().strip_prefix(label)?.trim_start_matches(':').trim()))
}

/// Whether `needle` occurs in `haystack`.
fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|window| window == needle)
}

fn token_holdings(data: &[u8]) -> Option<BTreeMap<AccountId, u128>> {
    match token_core::TokenState::decode(data)? {
        token_core::TokenState::Holdings(holdings) => Some(holdings.into_iter().collect()),
        token_core::TokenState::Definition(_) => None,
    }
}

fn token_supply(data: &[u8]) -> Option<(String, u128)> {
    match token_core::TokenState::decode(data)? {
        token_core::TokenState::Definition(token_core::TokenDefinition::Fungible { name, total_supply, .. }) => Some((name, total_supply)),
        token_core::TokenState::Holdings(_) => None,
    }
}

impl Client {
    fn checked_call(&self, input: &CallInput) -> Result<Call> {
        let text = input.account.trim();
        let Some(bare) = text.strip_prefix("Private/") else { bail!("a private call is made from a private account (Private/<id>)") };
        let account = parse_account(bare).map_err(|error| anyhow!(error))?;
        if self.wallet.resolve_private_account(account).is_none() {
            bail!("this wallet has no private account {text}");
        }
        let programs: Vec<AccountId> = input.programs.iter().map(|p| program_of(p)).collect::<Result<_>>()?;
        if programs.is_empty() || programs.len() > 4 || programs.contains(&native_program()) {
            bail!("a private call runs 1 to 4 programs, not the native token");
        }
        let openings: Vec<[u8; 32]> = input.openings.iter().map(|o| hex32(o, "an opening")).collect::<Result<_>>()?;
        let own = |program: AccountId, with_openings: bool| {
            let mut mention = self.signer(account).select_program_actor_state(program);
            if with_openings {
                mention.address_openings.extend(openings.iter().copied());
            }
            (Actor::new(account, program), mention, true)
        };
        let mut mentions = vec![own(programs[0], false)];
        for entry in &input.accounts {
            // As the module writes it, every entry names its program; one that doesn't is the root's.
            let program = match entry["program"].as_str() {
                Some(text) => program_of(text)?,
                None => programs[0],
            };
            if program != native_program() && !programs.contains(&program) {
                bail!("{program} is not one of the call's programs");
            }
            if entry["actor"] == "account" {
                mentions.push(own(program, entry["openings"] == true));
                continue;
            }
            let id = match (entry["seed"].as_str(), entry["account"].as_str()) {
                (Some(seed), _) => {
                    let of = program_of(entry["of"].as_str().ok_or_else(|| anyhow!("a PDA names its program"))?)?;
                    if !programs.contains(&of) {
                        bail!("a PDA belongs to one of the call's programs");
                    }
                    let id = AccountId::for_public_pda(&of, &PdaSeed::new(hex32(seed, "a seed")?));
                    if let Some(given) = entry["account"].as_str() {
                        if parse_account(given).map_err(|error| anyhow!(error))? != id {
                            bail!("{given} is not that seed's PDA");
                        }
                    }
                    id
                }
                (None, Some(text)) => {
                    if text.starts_with("Private/") {
                        bail!("a private call reaches only its own private account");
                    }
                    parse_account(text.strip_prefix("Public/").unwrap_or(text)).map_err(|error| anyhow!(error))?
                }
                (None, None) => bail!("each account entry has \"account\", \"seed\" or \"actor\""),
            };
            if id == account {
                bail!("the private account is written {{\"actor\": \"account\"}}, never by its ID");
            }
            // Never signed: a private call signs nothing public, so nothing public links to it.
            mentions.push((Actor::new(id, program), AccountIdentity::PublicNoSign(id).select_program_actor_state(program), false));
        }
        if mentions.len() > 17 {
            bail!("a private call reaches at most 16 accounts besides its root");
        }
        let message = hex::decode(input.message.trim()).context("the message is not hex")?;
        let fee_payer = match input.fee_payer.as_deref().unwrap_or("account") {
            "account" => Some(account),
            "wallet" => None,
            other => bail!("fee_payer is account or wallet, not {other}"),
        };
        let token_programs = input.token_programs.iter().map(|p| program_of(p)).collect::<Result<_>>()?;
        Ok(Call { account, programs, mentions, message, openings, fee_payer, token_programs })
    }

    /// Each program's bytecode, read from the chain (and checked against its image ID).
    async fn call_catalog(&self, programs: &[AccountId]) -> Result<Vec<(AccountId, Program)>> {
        let mut loaded = Vec::new();
        for program in programs {
            loaded.push((*program, self.program_at(*program, "LOGOS_NAMES_CALL_ELF_UNUSED", "names-call", "a called program").await?));
        }
        Ok(loaded)
    }

    /// See the module docs.
    pub async fn plan_private_call(&mut self, input: &CallInput) -> Result<Plan> {
        println!("phase: plan");
        // A private call is proved: a build that can't prove says so before anything else.
        crate::prover::to_prove()?;
        crate::prover::to_run()?;
        let call = self.checked_call(input)?;
        let catalog = self.call_catalog(&call.programs).await?;
        self.wallet.sync_to_latest_block().await.context("could not catch the wallet up with the chain")?;

        // 1. The wallet's own dry run of the transaction it would prove.
        println!("phase: simulate");
        let wallet_mentions: Vec<AccountMention> = call.mentions.iter().map(|(_, mention, _)| mention.clone()).collect();
        let programs: ProgramCatalog = catalog.iter().cloned().collect();
        self.wallet.set_dry_run(true);
        self.wallet.set_fee_headroom_blocks(PRIVATE_CALL_FEE_HEADROOM_BLOCKS);
        let (dry, mut printed) = captured(self.wallet.send_tx_with_pre_check(
            wallet_mentions,
            0,
            call.message.clone(),
            &programs,
            None,
            AsyncDelivery::default(),
            call.fee_payer,
            |_| Ok(()),
        ))
        .await;
        self.wallet.set_dry_run(false);
        restore_fee_headroom(&mut self.wallet);
        match dry {
            // rc3 hands the dry run's report back instead of printing it; its text is the
            // lines rc1 printed, which are read below.
            Err(ExecutionFailureKind::DryRun(report)) => {
                printed.push('\n');
                printed.push_str(&report.to_string());
            }
            Err(error) => bail!("refused: the wallet's own simulation of this call fails: {error} ({error:?})"),
            Ok(_) => bail!("the wallet sent a transaction during a dry run; stop and check the wallet"),
        }
        let cycles = dry_number(&printed, "public cycles").ok_or_else(|| anyhow!("the wallet's dry run gave no cycles"))?;
        let about = dry_number(&printed, "fee at head").ok_or_else(|| anyhow!("the wallet's dry run gave no fee"))?;
        let reserve = dry_number(&printed, "reserve").ok_or_else(|| anyhow!("the wallet's dry run gave no reserve"))?;
        let payer = dry_text(&printed, "fee payer").and_then(|text| parse_account(text).ok()).ok_or_else(|| anyhow!("the wallet's dry run named no fee payer"))?;
        // The reserve is priced with no headroom (so a first and a repeat claim reserve the
        // same), which makes the dry run's own "headroom" the fee at the head. The "at most" the
        // person approves is the climb over the wallet's configured headroom instead, from a
        // fresh fee quote: a ceiling the send checks again before it proves (`within_approved`).
        // The rc3 wallet still reprices inside its proving loop when the fee rises during the
        // proof (up to twice); it has no cap there, so a rise past this ceiling meanwhile is
        // reported after the send (`declared_fee_line`), never charged in silence.
        let quote = self.wallet.client().get_fee_state().await.context("could not read the chain's fee state")?;
        let headroom = approved_ceiling(&quote, self.wallet.config().fee_headroom_blocks, cycles, reserve, about);

        // 2. The open replay.
        let own_account = self.wallet.get_account_private(call.account).map(|found| found.data).unwrap_or_default();
        let pending = self.wallet.owned_pending_messages().await.context("could not read the payments waiting for this wallet")?;
        let waiting: u128 = crate::credits_to(&pending, call.account).into_iter().map(|(_, value)| value).sum();
        let mut own_data = own_account.clone();
        if waiting > 0 {
            let native = own_data.native_balance().unwrap_or(0).saturating_add(waiting);
            own_data.set_actor_state(native_program(), native_token::encode_balance(native));
        }
        let mut accounts: BTreeMap<AccountId, lee_core::account::AccountData> = BTreeMap::new();
        for (actor, _, own) in &call.mentions {
            if *own {
                continue;
            }
            let state = self.actor_state_of(actor.account_id, actor.program_account_id).await?;
            accounts.entry(actor.account_id).or_default().set_actor_state(actor.program_account_id, state.into());
        }
        let aliases: Vec<AccountId> = call.openings.iter().map(|opening| call.account.blinded(opening)).collect();
        let mut public_accounts: Vec<(AccountId, Account)> =
            accounts.iter().map(|(id, data)| (*id, Account { data: data.clone(), ..Account::default() })).collect();
        public_accounts.push((call.account, Account { data: own_data.clone(), ..Account::default() }));
        let mut state = V03State::new()
            .with_named_programs(catalog.iter().cloned())
            .with_public_accounts(public_accounts)
            // Every address the call names may be sent an async message; the replay only needs it
            // to publish it, as settlement would with the recipient's binding.
            .with_recovery_bindings(
                std::iter::once(call.account)
                    .chain(aliases.iter().copied())
                    .chain(accounts.keys().copied())
                    .map(|address| RecoveryBinding { address, note: EncryptedNote::default() }),
            );
        let actors: BTreeSet<Actor> = call.mentions.iter().map(|(actor, _, _)| *actor).collect();
        let authorized: HashSet<AccountId> = std::iter::once(call.account).chain(aliases.iter().copied()).collect();
        let block = self.wallet.get_last_block_id().await.context("could not read the chain's height")?;
        let now = u64::try_from(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_millis()).unwrap_or(u64::MAX);
        let root = call.mentions[0].0;
        let diff = ValidatedStateDiff::from_fee_settlement_invocation(root, &call.message, &actors, &authorized, &state, block + 1, now)
            .map_err(|error| anyhow!("refused: the wallet can't explain this call: replaying it fails ({error})"))?;
        let events = state.apply_state_diff(diff).map_err(|error| anyhow!("refused: the wallet can't explain this call: {error}"))?;

        // What the account receives through publications: credits the proof takes in.
        let mut native_in: u128 = 0;
        let mut tokens_in: BTreeMap<(AccountId, AccountId), u128> = BTreeMap::new();
        let mut unread_to_own: BTreeSet<AccountId> = BTreeSet::new();
        let mut used_aliases: BTreeSet<AccountId> = BTreeSet::new();
        let mut public_bytes: Vec<u8> = Vec::new();
        let mut public_credits: Vec<serde_json::Value> = Vec::new();
        for (_, publication) in state.publications_from(0) {
            let Publication::Clear { body, .. } = publication else { continue };
            let to = body.to.account_id;
            if to == call.account || aliases.contains(&to) {
                if to != call.account {
                    used_aliases.insert(to);
                }
                let program = body.to.program_account_id;
                if program == native_program() {
                    match borsh::from_slice::<native_token::Message>(&body.message) {
                        Ok(native_token::Message::Credit(amount)) => native_in = native_in.saturating_add(amount),
                        _ => {
                            unread_to_own.insert(program);
                        }
                    }
                } else if call.token_programs.contains(&program) {
                    match borsh::from_slice::<token_core::Message>(&body.message) {
                        Ok(token_core::Message::Credit { definition, amount }) => {
                            let slot = tokens_in.entry((program, definition)).or_default();
                            *slot = slot.saturating_add(amount);
                        }
                        _ => {
                            unread_to_own.insert(program);
                        }
                    }
                } else {
                    unread_to_own.insert(program);
                }
            } else {
                // Sent to a public address, to be taken in later: shown as what it credits.
                let program = body.to.program_account_id;
                if program == native_program() {
                    if let Ok(native_token::Message::Credit(amount)) = borsh::from_slice::<native_token::Message>(&body.message) {
                        public_credits.push(serde_json::json!({ "account": to.to_string(), "program": "native", "kind": "native_credit", "amount": amount.to_string() }));
                    }
                } else if call.token_programs.contains(&program) {
                    if let Ok(token_core::Message::Credit { definition, amount }) = borsh::from_slice::<token_core::Message>(&body.message) {
                        public_credits.push(serde_json::json!({ "account": to.to_string(), "program": program.to_string(), "kind": "token_credit",
                            "definition": definition.to_string(), "amount": amount.to_string() }));
                    }
                }
                public_bytes.extend_from_slice(&borsh::to_vec(&body).unwrap_or_default());
            }
        }

        // A message to the account that isn't a credit the wallet reads (a transfer out of its
        // LEZ or tokens, say) is taken in by the proof too: never shown as nothing.
        if let Some(program) = unread_to_own.iter().find(|program| **program == native_program() || call.token_programs.contains(program)) {
            bail!("refused: the wallet can't explain this call: it sends the account a message under {program} that isn't a credit");
        }

        // The account: what it holds before and after, as the proof will leave it.
        let after_own = state.get_account_by_id(call.account).data;
        let mut own_rows = Vec::new();
        let mut own_programs: BTreeSet<AccountId> = call.mentions.iter().filter(|(_, _, own)| *own).map(|(actor, _, _)| actor.program_account_id).collect();
        own_programs.extend(tokens_in.keys().map(|(program, _)| *program));
        own_programs.extend(unread_to_own.iter().copied());
        if native_in > 0 {
            own_programs.insert(native_program());
        }
        let mut own_pre = BTreeMap::new();
        for program in &own_programs {
            let before = own_data.actor_state(*program).to_vec();
            let after = after_own.actor_state(*program).to_vec();
            own_pre.insert(*program, own_account.actor_state(*program).to_vec());
            if *program == native_program() {
                let before_n = decode_balance(&before).unwrap_or(0);
                let after_n = decode_balance(&after).unwrap_or(0).saturating_add(native_in);
                own_rows.push(serde_json::json!({ "asset": "LEZ", "before": before_n.to_string(), "after": after_n.to_string() }));
            } else if call.token_programs.contains(program) {
                let (Some(pre), Some(mut post)) = (token_holdings(&before), token_holdings(&after)) else {
                    own_rows.push(serde_json::json!({ "asset": "data", "program": program.to_string(), "before_empty": before.is_empty(), "changed": true }));
                    continue;
                };
                for ((credited_program, definition), amount) in &tokens_in {
                    if credited_program == program {
                        let slot = post.entry(*definition).or_default();
                        *slot = slot.saturating_add(*amount);
                    }
                }
                let definitions: BTreeSet<AccountId> = pre.keys().chain(post.keys()).copied().collect();
                for definition in definitions {
                    let (b, a) = (pre.get(&definition).copied().unwrap_or(0), post.get(&definition).copied().unwrap_or(0));
                    if b == a {
                        continue;
                    }
                    let name = self.actor_state_of(definition, *program).await.ok().and_then(|data| token_supply(&data)).map(|(name, _)| name);
                    own_rows.push(serde_json::json!({
                        "asset": "token", "program": program.to_string(), "definition": definition.to_string(),
                        "before": b.to_string(), "after": a.to_string(), "name": name,
                    }));
                }
            } else {
                let changed = before != after || unread_to_own.contains(program);
                own_rows.push(serde_json::json!({ "asset": "data", "program": program.to_string(), "before_empty": before.is_empty(), "changed": changed }));
            }
        }

        // The public accounts: each one's change.
        let mut public_rows = Vec::new();
        for (actor, _, own) in &call.mentions {
            if *own {
                continue;
            }
            let (id, program) = (actor.account_id, actor.program_account_id);
            let before = accounts.get(&id).map(|data| data.actor_state(program).to_vec()).unwrap_or_default();
            let after = state.get_account_by_id(id).data.actor_state(program).to_vec();
            public_bytes.extend_from_slice(&after);
            let row = if program == native_program() {
                serde_json::json!({ "account": id.to_string(), "program": "native", "kind": "native",
                    "before": decode_balance(&before).unwrap_or(0).to_string(), "after": decode_balance(&after).unwrap_or(0).to_string() })
            } else if call.token_programs.contains(&program) {
                match (token_supply(&before), token_supply(&after), token_holdings(&before), token_holdings(&after)) {
                    (Some((_, b)), Some((_, a)), _, _) => serde_json::json!({ "account": id.to_string(), "program": program.to_string(), "kind": "token_definition",
                        "definition": id.to_string(), "before": b.to_string(), "after": a.to_string() }),
                    (_, _, Some(pre), Some(post)) if pre != post => {
                        // One row per definition whose holding changed.
                        for definition in pre.keys().chain(post.keys()).copied().collect::<BTreeSet<_>>() {
                            let (b, a) = (pre.get(&definition).copied().unwrap_or(0), post.get(&definition).copied().unwrap_or(0));
                            if b != a {
                                public_rows.push(serde_json::json!({ "account": id.to_string(), "program": program.to_string(), "kind": "token_holding",
                                    "definition": definition.to_string(), "before": b.to_string(), "after": a.to_string() }));
                            }
                        }
                        continue;
                    }
                    _ => serde_json::json!({ "account": id.to_string(), "program": program.to_string(), "kind": "data", "before_empty": before.is_empty(), "changed": before != after }),
                }
            } else {
                serde_json::json!({ "account": id.to_string(), "program": program.to_string(), "kind": "data", "before_empty": before.is_empty(), "changed": before != after })
            };
            public_rows.push(row);
        }
        public_rows.extend(public_credits);
        for event in &events {
            public_bytes.extend_from_slice(&borsh::to_vec(event).unwrap_or_default());
        }
        let account_public = contains(&public_bytes, call.account.value());

        // The digest: what the person approves. Deltas, not absolute values, so another
        // transaction moving the same public balances in between doesn't void the approval;
        // anything else that changes what this call does does.
        let delta = |row: &serde_json::Value| -> serde_json::Value {
            let parse = |key: &str| row[key].as_str().and_then(|t| t.parse::<i128>().ok());
            match (parse("before"), parse("after"), parse("amount")) {
                (Some(b), Some(a), _) => serde_json::json!((a - b).to_string()),
                (_, _, Some(amount)) => serde_json::json!(amount.to_string()),
                _ => row["changed"].clone(),
            }
        };
        let mut hash = Sha256::new();
        let effect = serde_json::json!({
            "account": call.account.to_string(),
            "programs": call.programs.iter().map(ToString::to_string).collect::<Vec<_>>(),
            "message": hex::encode(Sha256::digest(&call.message)),
            "mentions": input.accounts,
            "openings": input.openings,
            "own": own_rows.iter().map(|row| serde_json::json!([row["asset"], row["program"], row["definition"], delta(row)])).collect::<Vec<_>>(),
            "public": public_rows.iter().map(|row| serde_json::json!([row["account"], row["program"], row["kind"], row["definition"], delta(row)])).collect::<Vec<_>>(),
            "aliases": used_aliases.iter().map(ToString::to_string).collect::<Vec<_>>(),
            "payer": payer.to_string(),
        });
        hash.update(effect.to_string().as_bytes());
        let digest = hex::encode(hash.finalize());

        let report = serde_json::json!({
            "account": format!("Private/{}", call.account),
            "digest": digest,
            "public_cycles": cycles,
            "fee": { "about": about.to_string(), "at_most": headroom.to_string(), "headroom": headroom.to_string(), "reserve": reserve.to_string(), "payer": format!("Private/{payer}") },
            "own": own_rows,
            "public": public_rows,
            "aliases": used_aliases.iter().map(ToString::to_string).collect::<Vec<_>>(),
            "events": events.len(),
            "account_public": account_public,
            "waiting_credits": waiting.to_string(),
            "message_len": call.message.len(),
        });
        Ok(Plan { report, digest, own_pre })
    }

    /// `names module private-call plan`: the report, as one JSON line.
    pub async fn private_call_plan(&mut self, input: &CallInput) -> Result<()> {
        let plan = self.plan_private_call(input).await?;
        println!("{}", serde_json::json!({ "report": plan.report }));
        Ok(())
    }

    /// `names module private-call send`: plans again, and proves and sends only what the
    /// person approved.
    pub async fn private_call_send(&mut self, input: &CallInput) -> Result<()> {
        let expected = input.expect_digest.as_deref().ok_or_else(|| anyhow!("a private call is sent only with the digest the person approved"))?;
        let plan = self.plan_private_call(input).await?;
        if plan.digest != expected {
            bail!("refused: what this call does has changed since it was approved (the chain moved, or the call isn't the one shown); nothing was proven or sent. Ask again.");
        }
        let head_now = plan.report["fee"]["about"].as_str().and_then(|s| s.parse::<u128>().ok()).unwrap_or(0);
        within_approved(input.approved_at_most.as_deref(), head_now).map_err(|why| anyhow!(why))?;
        crate::prover::to_prove()?;
        let call = self.checked_call(input)?;
        let catalog = self.call_catalog(&call.programs).await?;
        let programs: ProgramCatalog = catalog.into_iter().collect();
        let wallet_mentions: Vec<AccountMention> = call.mentions.iter().map(|(_, mention, _)| mention.clone()).collect();
        let account = call.account;
        let own_pre = plan.own_pre.clone();
        println!("phase: prove");
        println!("acting privately; proving it on this computer ({}), so the chain never learns your account", crate::proving_words());
        let started = std::time::Instant::now();
        self.wallet.set_fee_headroom_blocks(PRIVATE_CALL_FEE_HEADROOM_BLOCKS);
        let sent = self
            .wallet
            .send_tx_with_pre_check(wallet_mentions, 0, call.message.clone(), &programs, None, AsyncDelivery::default(), call.fee_payer, move |states| {
                // The proof must start from the states the person's summary was made from.
                // Its LEZ may grow by credits the wallet claims at the same time; nothing else may differ.
                for state in states.iter().filter(|state| state.selector.account_id == account && state.selector.program_account_id != native_program()) {
                    let planned = own_pre.get(&state.selector.program_account_id).map(Vec::as_slice).unwrap_or_default();
                    let now: &[u8] = std::borrow::Borrow::borrow(&state.data);
                    if now != planned {
                        return Err(ExecutionFailureKind::TransactionBuildError(lee::error::LeeError::InvalidInput(
                            "the private account changed since the call was planned; nothing was proven or sent".to_owned(),
                        )));
                    }
                }
                Ok(())
            })
            .await;
        restore_fee_headroom(&mut self.wallet);
        self.proved_ms = Some(u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX));
        let (hash, _) = sent.map_err(|error| crate::refused_or(error, "the sequencer did not accept the transaction"))?;
        println!("phase: submit");
        println!("submitted {hash}");
        if let Some(wallet::DeclaredFee::Private(fee)) = self.wallet.take_declared_fee() {
            if let Some(line) = declared_fee_line(input.approved_at_most.as_deref(), fee.reserve) {
                println!("{line}");
            }
        }
        println!("phase: confirm");
        let (block, _) = self.finish_landed(hash).await.map_err(|_| anyhow!(PRIVATE_NOT_APPLIED))?;
        println!("applied in block {block}");
        Ok(())
    }

    /// `names module app-secret`: the values an app asks for (`app_secret.rs`). A phrase account's
    /// come from its own key; an imported account's from this wallet's root `R` and its id, so the
    /// recovery phrase alone makes them again. The answer names the scheme (`"scheme"`): the module
    /// refuses an imported account's values from a tool that doesn't say.
    pub fn app_secret(&self, request: &serde_json::Value) -> Result<()> {
        let field = |key: &str| request[key].as_str().ok_or_else(|| anyhow!("no {key}"));
        let context = hex::decode(field("context").unwrap_or_default()).context("the context is not hex")?;
        let number = |key: &str| request[key].as_u64().ok_or_else(|| anyhow!("{key} is a number"));
        let ask = crate::app_secret::Ask::new(field("app")?, field("scope")?, context, field("tag")?, number("first")?, number("count")?)
            .map_err(|error| anyhow!(error))?;
        let written = field("account")?;
        let (private, id) = if let Some(bare) = written.strip_prefix("Private/") {
            (true, parse_account(bare).map_err(|error| anyhow!(error))?)
        } else if let Some(bare) = written.strip_prefix("Public/") {
            (false, parse_account(bare).map_err(|error| anyhow!(error))?)
        } else {
            bail!("write the account as Public/<id> or Private/<id>");
        };
        let (values, scheme) = match account_secret(&self.wallet, private, id).map_err(|why| anyhow!("{written}: {why}"))? {
            AccountSecret::Phrase(mut secret) => {
                let values = crate::app_secret::values(&secret, id.value(), &ask);
                secret.fill(0);
                (values, crate::app_secret::SCHEME_PHRASE)
            }
            AccountSecret::Imported => {
                let mut root = imported_root(&self.wallet).map_err(|why| anyhow!("{written}: {why}"))?;
                let values = crate::app_secret::imported_values(&root, id.value(), &ask);
                root.fill(0);
                (values, crate::app_secret::SCHEME_IMPORTED)
            }
        };
        println!("{}", serde_json::json!({ "account": written, "values": values.iter().map(hex::encode).collect::<Vec<_>>(), "scheme": scheme }));
        Ok(())
    }

    /// `names module account-scheme`: `{account, scheme}` for one of this wallet's accounts.
    pub fn account_scheme(&self, request: &serde_json::Value) -> Result<()> {
        let written = request["account"].as_str().ok_or_else(|| anyhow!("no account"))?;
        let (private, bare) = match (written.strip_prefix("Private/"), written.strip_prefix("Public/")) {
            (Some(bare), _) => (true, bare),
            (_, Some(bare)) => (false, bare),
            _ => bail!("write the account as Public/<id> or Private/<id>"),
        };
        let id = parse_account(bare).map_err(|error| anyhow!(error))?;
        let scheme = match account_secret(&self.wallet, private, id).map_err(|why| anyhow!("{written}: {why}"))? {
            AccountSecret::Phrase(mut secret) => {
                secret.fill(0);
                crate::app_secret::SCHEME_PHRASE
            }
            AccountSecret::Imported => crate::app_secret::SCHEME_IMPORTED,
        };
        println!("{}", serde_json::json!({ "account": written, "scheme": scheme }));
        Ok(())
    }
}

/// Where an account's app secrets come from. Its `Debug` never prints the secret.
#[derive(PartialEq, Eq)]
pub enum AccountSecret {
    /// An account of the recovery phrase's key trees: its own 32-byte secret (a public account's
    /// signing key, a private account's authorization secret key).
    Phrase([u8; 32]),
    /// An imported account (its key is held, no key tree has it): its values come from
    /// [`imported_root`] and its id.
    Imported,
}

impl std::fmt::Debug for AccountSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Phrase(_) => "Phrase(..)",
            Self::Imported => "Imported",
        })
    }
}

/// Which scheme `id` follows, by key-tree membership: an id the trees hold is a phrase account
/// even when it was also imported (the imported entries are listed first, so a first match
/// would misread it). Another wallet's account, and shared or group accounts, are refused.
pub fn account_secret<C: wallet::WalletSequencerClient>(wallet: &wallet::WalletCore<C>, private: bool, id: AccountId) -> Result<AccountSecret, String> {
    let key_chain = wallet.storage().key_chain();
    if private {
        let mut held = false;
        for found in key_chain.private_accounts() {
            if AccountId::for_private_account(&found.key_chain.nullifier_public_key, &found.key_chain.viewing_public_key, found.kind) != id {
                continue;
            }
            held = true;
            // A tree account: the tree itself has this account's key chain at that index (an imported
            // account keeps the index it had in the wallet it came from).
            let tree = found.chain_index.as_ref().and_then(|index| key_chain.private_account_key_chain_by_index(index));
            if let Some(tree) = tree {
                if AccountId::for_private_account(&tree.nullifier_public_key, &tree.viewing_public_key, found.kind) == id {
                    return Ok(AccountSecret::Phrase(tree.private_key_holder.authorization_secret_key.0));
                }
            }
        }
        if held { Ok(AccountSecret::Imported) } else { Err("this wallet has no such private account".to_owned()) }
    } else {
        let key = key_chain.pub_account_signing_key(id).ok_or("this wallet has no such public account")?;
        if key_chain.public_account_ids().any(|(found, index)| found == id && index.is_some()) {
            Ok(AccountSecret::Phrase(*key.value()))
        } else {
            Ok(AccountSecret::Imported)
        }
    }
}

/// `R`: the private key tree root's secret spending key, which only the recovery phrase gives
/// (LEZ `ChildKeysPrivate::root`; the root node is in every storage file and every restore). Read
/// from the tree, never from an imported entry.
pub fn imported_root<C: wallet::WalletSequencerClient>(wallet: &wallet::WalletCore<C>) -> Result<[u8; 32], String> {
    let root = wallet.storage().key_chain().private_account_key_chain_by_index(&Default::default()).ok_or("this wallet's key tree has no root")?;
    Ok(root.secret_spending_key.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What `Storage::restore` promises (LEZ 0.4 `wallet/src/storage/mod.rs`: the key trees are
    /// rebuilt from the phrase, `keys_public.rs`/`keys_private.rs` derive each child with
    /// HMAC-SHA512 from its parent): accounts made again in the same order are the same
    /// accounts, so their app secrets come back. Another wallet's don't match.
    #[test]
    fn a_wallet_restored_from_its_phrase_derives_the_same_values() {
        use sequencer_service_rpc::SequencerClient;
        use wallet::WalletCore;
        let root = std::env::temp_dir().join(format!("names-app-secret-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let open = |name: &str| {
            let dir = root.join(name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("wallet_config.json"), serde_json::to_vec(&wallet::config::WalletConfig::default()).unwrap()).unwrap();
            WalletCore::<SequencerClient>::new_init_storage(dir.join("wallet_config.json"), dir.join("storage.json"), None, "").unwrap()
        };
        let made = |wallet: &mut WalletCore<SequencerClient>| {
            let public = wallet.create_new_account_public(None).0;
            let first = wallet.create_new_account_private(None).0;
            let second = wallet.create_new_account_private(None).0;
            [(false, public), (true, first), (true, second)]
        };
        let (mut original, phrase) = open("original");
        let accounts = made(&mut original);
        let (mut restored, _) = open("restored");
        restored.restore_storage(&phrase);
        assert_eq!(made(&mut restored), accounts, "the same accounts, in the same order");
        let (mut other, _) = open("other");
        let others = made(&mut other);
        let ask = crate::app_secret::Ask::new("lez_bridge", "RFP022/deposit", vec![1, 2, 3], "RFP022/ck", 0, 3).unwrap();
        let secret = |wallet: &WalletCore<SequencerClient>, private: bool, id: AccountId| match account_secret(wallet, private, id).unwrap() {
            AccountSecret::Phrase(secret) => secret,
            AccountSecret::Imported => panic!("{id} is a phrase account"),
        };
        for ((private, id), (_, other_id)) in accounts.iter().zip(others.iter()) {
            let mine = crate::app_secret::values(&secret(&original, *private, *id), id.value(), &ask);
            let again = crate::app_secret::values(&secret(&restored, *private, *id), id.value(), &ask);
            assert_eq!(mine, again, "restoring gives the same values for {id}");
            let theirs = crate::app_secret::values(&secret(&other, *private, *other_id), other_id.value(), &ask);
            assert_ne!(mine, theirs);
            // The values are not the account's key, nor its key hashed with the tag alone.
            let key = secret(&original, *private, *id);
            assert!(mine.iter().all(|value| *value != key));
        }
        // Another wallet's account is refused here, not derived from.
        assert!(account_secret(&original, true, others[1].1).is_err());
        assert!(account_secret(&original, false, others[0].1).is_err());
        // An imported public account: its values come from this wallet's phrase and its id. The
        // same key imported into a wallet of another phrase gives OTHER values (the 15 Nov rule,
        // values from the imported key, gave the same: they are never this wallet's now).
        let key = lee::PrivateKey::try_new([7u8; 32]).unwrap();
        let imported = AccountId::from(&lee::PublicKey::new_from_private_key(&key));
        original.storage_mut().key_chain_mut().add_imported_public_account(key.clone());
        other.storage_mut().key_chain_mut().add_imported_public_account(key.clone());
        assert_eq!(account_secret(&original, false, imported), Ok(AccountSecret::Imported));
        let root_of = |wallet: &WalletCore<SequencerClient>| imported_root(wallet).unwrap();
        let here = crate::app_secret::imported_values(&root_of(&original), imported.value(), &ask);
        let there = crate::app_secret::imported_values(&root_of(&other), imported.value(), &ask);
        assert_ne!(here, there, "another phrase, other values");
        let old_rule = crate::app_secret::values(key.value(), imported.value(), &ask);
        assert!(here.iter().all(|v| !old_rule.contains(v)), "the imported key's own values are not this wallet's");
        // The restored wallet, WITHOUT the imported key, makes the same values from the phrase alone.
        assert_eq!(account_secret(&restored, false, imported).map(|_| ()), Err("this wallet has no such public account".to_owned()));
        assert_eq!(crate::app_secret::imported_values(&root_of(&restored), imported.value(), &ask), here);
        // ...and so does it after importing the key again.
        restored.storage_mut().key_chain_mut().add_imported_public_account(key);
        assert_eq!(account_secret(&restored, false, imported), Ok(AccountSecret::Imported));
        // A phrase account also imported (its own key added to the imported list) stays a phrase account.
        let (_, public) = accounts[0];
        let own = original.storage().key_chain().pub_account_signing_key(public).unwrap().clone();
        original.storage_mut().key_chain_mut().add_imported_public_account(own);
        assert_eq!(account_secret(&original, false, public).map(|s| matches!(s, AccountSecret::Phrase(_))), Ok(true));
        // An imported private account: imported, whatever chain index it carries from its wallet.
        let (_, other_private) = others[1];
        let found = other.storage().key_chain().private_account(other_private).unwrap();
        let (chain, index, account) = (found.key_chain.clone(), found.chain_index.clone(), found.account.clone());
        assert!(index.is_some(), "it carries its old wallet's index");
        original.storage_mut().key_chain_mut().add_imported_private_account(chain, index, account);
        assert_eq!(account_secret(&original, true, other_private), Ok(AccountSecret::Imported));
        let _ = std::fs::remove_dir_all(root);
    }

    /// `R` from a wallet restored from the public BIP-39 test phrase is the Python generator's
    /// (rfp-022 `gen_names_imported_vectors.py`): the values really come from the phrase.
    #[test]
    fn the_imported_root_is_the_phrase_s() {
        use sequencer_service_rpc::SequencerClient;
        use wallet::WalletCore;
        let doc: serde_json::Value = serde_json::from_str(include_str!("../../basecamp-module/rust-lib/testdata/app-secret-imported-v1.json")).unwrap();
        let root = std::env::temp_dir().join(format!("names-imported-root-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for case in doc["roots"].as_array().unwrap() {
            let dir = root.join(case["name"].as_str().unwrap().replace(' ', "-"));
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("wallet_config.json"), serde_json::to_vec(&wallet::config::WalletConfig::default()).unwrap()).unwrap();
            let (mut w, _) = WalletCore::<SequencerClient>::new_init_storage(dir.join("wallet_config.json"), dir.join("storage.json"), None, "").unwrap();
            w.restore_storage(&case["phrase"].as_str().unwrap().parse().unwrap());
            assert_eq!(format!("0x{}", hex::encode(imported_root(&w).unwrap())), case["root"].as_str().unwrap(), "{}", case["name"]);
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn the_wallets_dry_run_lines_are_read() {
        // As the LEZ 0.4 wallet prints them (rfp-022 run/sepolia2/attested/out-private-preflight.log).
        let printed = "Dry run: nothing proven or sent\n  public cycles:  81568\n  base fees:      exec 8 / stor 8 (height 4952)\n  fee at head:    5723160\n  headroom:       5 blocks, up to exec 13 / stor 13 (9300135)\n  reserve:        5723160 (surplus refunded)\n  fee payer:      2if1EJVcZwFT3oXxF4uewB2mk2XDXpmCWwTtBwUQ6HUC\n";
        assert_eq!(dry_number(printed, "public cycles"), Some(81568));
        assert_eq!(dry_number(printed, "fee at head"), Some(5_723_160));
        assert_eq!(dry_number(printed, "reserve"), Some(5_723_160));
        assert_eq!(dry_text(printed, "fee payer"), Some("2if1EJVcZwFT3oXxF4uewB2mk2XDXpmCWwTtBwUQ6HUC"));
        let headroom = dry_text(printed, "headroom").and_then(|t| t.rsplit_once('(').and_then(|(_, n)| n.trim_end_matches(')').trim().parse::<u128>().ok()));
        assert_eq!(headroom, Some(9_300_135));
        assert!(contains(b"xxabcxx", b"abc") && !contains(b"xxabxx", b"abc") && !contains(b"x", b""));
    }

    fn fee_quote(exec: u64, stor: u64) -> sequencer_service_rpc::FeeStateQuote {
        sequencer_service_rpc::FeeStateQuote {
            height: 7,
            base_fee_exec: exec,
            base_fee_stor: stor,
            next_base_fee_exec_floor: exec,
            next_base_fee_exec_ceiling: exec + 1,
            next_base_fee_stor_floor: stor,
            next_base_fee_stor_ceiling: stor + 1,
            max_gas_exec: fee_core::market::MAX_GAS_EXEC,
            max_gas_stor: fee_core::market::MAX_GAS_STOR,
        }
    }

    /// Review 4, finding 1: with the reserve priced at no headroom, the "at most" the person
    /// approves is still a ceiling above the fee at the head (the configured 5 blocks' climb),
    /// and the send refuses before proving once the head fee is past it.
    #[test]
    fn the_approved_at_most_is_a_ceiling_above_the_head_fee_and_the_send_holds_to_it() {
        let q = fee_quote(8, 8);
        let cycles = 81_568;
        let about = crate::fees::private_running(&q, 0, cycles).about;
        let reserve = about; // headroom 0: the reserve is the fee at the head
        let ceiling = approved_ceiling(&q, 5, cycles, reserve, about);
        assert!(ceiling > about, "the ceiling is above the head fee: {ceiling} vs {about}");
        assert_eq!(ceiling, crate::fees::private_running(&q, 5, cycles).at_most);
        // The send: within the ceiling passes; a head fee above it is refused before proving.
        assert!(within_approved(Some(&ceiling.to_string()), about).is_ok());
        assert!(within_approved(Some(&ceiling.to_string()), ceiling).is_ok());
        let refused = within_approved(Some(&ceiling.to_string()), ceiling + 1).unwrap_err();
        assert!(refused.starts_with("refused:") && refused.contains("nothing was spent"), "{refused}");
        assert!(within_approved(None, u128::MAX).is_ok(), "no approved value: as before");
        assert!(within_approved(Some("x"), 1).is_err());
        // After the send: a reserve above the approved value is said plainly, never silent.
        assert_eq!(declared_fee_line(Some(&ceiling.to_string()), ceiling), None);
        assert!(declared_fee_line(Some(&ceiling.to_string()), ceiling + 1).unwrap().contains("more than the"));
        assert!(PRIVATE_NOT_APPLIED.contains("Nothing was spent"));
    }

    /// rfp-022 `docs/privacy-m5/repeat-claimers.md`: a private call reserves the fee at the head,
    /// whatever the account holds, and changes no other fee setting.
    #[test]
    fn a_private_call_prices_no_headroom_and_sets_nothing_else() {
        assert_eq!(PRIVATE_CALL_FEE_HEADROOM_BLOCKS, 0);
        let params = private_call_fee_params();
        assert_eq!(params.headroom_blocks, Some(0));
        assert_eq!(wallet::FeeParams { headroom_blocks: None, ..params }, wallet::FeeParams::default());
        // The rc3 wallet's reserve (`lez/wallet/src/lib.rs:1808`) with no headroom: the headroom
        // reserve is the fee at the head, so an empty and a funded payer reserve the same.
        let rc3_reserve = |headroom: u128, balance: u128, at_head: u128| headroom.min(balance).max(at_head);
        let at_head = 5_723_160;
        assert_eq!(rc3_reserve(at_head, 0, at_head), rc3_reserve(at_head, 40_000_000, at_head));
        // With the default 5 blocks they differ (the dry-run sample above: 9,300,135 at the ceiling).
        assert_ne!(rc3_reserve(9_300_135, 0, at_head), rc3_reserve(9_300_135, 40_000_000, at_head));
    }
}
