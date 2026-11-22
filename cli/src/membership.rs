//! `names proof prepare|prove`: the LP-0001 gate's `Prove` for the Basecamp module (spec §4.4).
//!
//! The module decides, this proves. `prove` makes the proven transaction, prints what the
//! module's outcome check needs, and waits on standard input inside the wallet's
//! `send_transaction` ([`crate::rpc::Rpc::hold_send`]): only the exact line `send` lets it go,
//! once. End of input, any other line or a signal ends it with nothing sent.
//!
//! Ported from gate-cli's `prove`, without its TEST ONLY switches. Lines for the module start
//! with [`EVENT`]; other lines are the wallet's own. The `proven` line holds the slot secret
//! (inside the holder's state), so `prove` refuses to write to a terminal or a file.

use std::io::{BufRead as _, Write as _};

use anyhow::{anyhow, bail, Context as _, Result};
use clap::{Args, Subcommand};
use common::transaction::LeeTransaction;
use lee::privacy_preserving_transaction::circuit::ProgramCatalog;
use lee::{Account, AccountId};
use lee_core::account::Actor;
use lee_core::execution_state::BoundaryStep;
use lee_core::program::{PdaSeed, ProgramHeader, PROGRAM_LOADER_ACCOUNT_ID};
use serde_json::{json, Value};
use sha2::{Digest as _, Sha256};
use wallet::AccountIdentity;
use wallet::{AsyncDelivery, DryRunReport, ExecutionFailureKind};

/// What a line for the module starts with.
pub const EVENT: &str = "proof-event ";
/// The one line that lets a proven transaction go.
const SEND: &str = "send\n";
const DOMAIN_SLOT: &[u8] = b"lp0001-gate/v1/slot";
/// The holder's idle gate state: tag, version, secret, no proof pending.
const IDLE_LEN: usize = 35;

#[derive(Args, Debug, Clone)]
pub struct Job {
    #[arg(long, value_parser = crate::parse_account)]
    gate: AccountId,
    /// The image the wallet pinned for the gate: its ImageID, 64 hex.
    #[arg(long, value_parser = hex32)]
    image: [u8; 32],
    #[arg(long, value_parser = crate::parse_account)]
    program: AccountId,
    #[arg(long, value_parser = crate::parse_account)]
    collection: AccountId,
    #[arg(long, value_parser = crate::parse_account)]
    holder: AccountId,
    #[arg(long, value_parser = crate::parse_account)]
    payer: AccountId,
    #[arg(long, value_parser = hex32)]
    context: [u8; 32],
    #[arg(long)]
    not_before: u64,
    #[arg(long)]
    ttl_class: u8,
}

#[derive(Subcommand, Debug)]
pub enum ProofCommand {
    /// The checks before proving: the gate on chain, the holding, the simulation, the reserve.
    Prepare(Job),
    /// Proves, shows the proven transaction, and sends it only on the line `send`.
    Prove(Job),
}

fn hex32(text: &str) -> Result<[u8; 32], String> {
    hex::decode(text).ok().and_then(|bytes| bytes.try_into().ok()).ok_or_else(|| "64 hex characters".to_owned())
}

fn image_bytes(words: &[u32; 8]) -> [u8; 32] {
    let mut out = [0; 32];
    for (chunk, word) in out.chunks_exact_mut(4).zip(words) {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    out
}

fn slot_of(secret: &[u8; 32]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(DOMAIN_SLOT);
    hash.update(secret);
    hash.finalize().into()
}

/// The borsh bytes of `GateMessage::Prove` (gate_interface).
fn prove_message(collection: &[u8; 32], context: &[u8; 32], not_before: u64, ttl_class: u8, new_secret: Option<&[u8; 32]>) -> Vec<u8> {
    let mut out = vec![0];
    out.extend_from_slice(collection);
    out.extend_from_slice(context);
    out.extend_from_slice(&not_before.to_le_bytes());
    out.push(ttl_class);
    match new_secret {
        Some(secret) => {
            out.push(1);
            out.extend_from_slice(secret);
        }
        None => out.push(0),
    }
    out
}

/// The secret in the holder's gate state: none yet, or an idle one. Anything else is refused.
fn secret_in(state: &[u8]) -> Result<Option<[u8; 32]>> {
    match state {
        [] => Ok(None),
        [0, 1, secret @ .., 0] if state.len() == IDLE_LEN => Ok(Some(secret.try_into()?)),
        _ => bail!("the holder's gate state isn't an idle one; refusing"),
    }
}

/// Writes one line for the module.
fn event(value: &Value) -> Result<(), String> {
    let mut out = std::io::stdout().lock();
    writeln!(out, "{EVENT}{value}").and_then(|()| out.flush()).map_err(|error| format!("the module can't be told: {error}"))
}

/// Whether the module answered exactly `send`. End of input is no.
fn may_send(input: impl std::io::BufRead) -> bool {
    let mut line = String::new();
    input.take(16).read_line(&mut line).is_ok() && line == SEND
}

/// `prove` hands the secret to its parent only: standard output must be a pipe.
fn out_is_a_pipe() -> bool {
    use std::os::fd::{AsFd as _, OwnedFd};
    use std::os::unix::fs::FileTypeExt as _;
    let Ok(fd) = std::io::stdout().as_fd().try_clone_to_owned() else { return false };
    std::fs::File::from(OwnedFd::from(fd)).metadata().is_ok_and(|meta| meta.file_type().is_fifo() || meta.file_type().is_socket())
}

/// In this process on four threads, or not at all: never an r0vm, never all cores.
fn proving_is_bounded() -> bool {
    let is = |name: &str, want: &str| std::env::var(name).is_ok_and(|value| value == want);
    cfg!(feature = "local-prove")
        && is("RISC0_PROVER", "local")
        && is("RISC0_EXECUTOR", "local")
        && is("RAYON_NUM_THREADS", "4")
        && std::env::var_os("RISC0_SERVER_PATH").is_none()
}

fn actor_json(actor: &Actor) -> Value {
    json!([hex::encode(actor.account_id.value()), hex::encode(actor.program_account_id.value())])
}

fn step_json(step: &BoundaryStep) -> Value {
    match step {
        BoundaryStep::PrivateToPublic(d) => json!({ "kind": "private_to_public", "to": actor_json(&d.envelope.to), "message": hex::encode(&d.envelope.message) }),
        BoundaryStep::PublicToPrivate(d) => json!({ "kind": "public_to_private", "to": actor_json(&d.envelope.to), "message": hex::encode(&d.envelope.message) }),
        BoundaryStep::EndPrivateSubtree => json!({ "kind": "end_private" }),
        BoundaryStep::EndPublicSubtree => json!({ "kind": "end_public" }),
    }
}

fn states_json(account: &Account) -> Value {
    Value::Object(account.data.actor_states.iter().map(|(program, state)| (hex::encode(program.value()), json!(hex::encode(state.to_vec())))).collect())
}

/// Says which refusal it was to the module, then ends.
fn refuse<T>(code: &str, why: String) -> Result<T> {
    // The pipe may be gone; the exit status says it too.
    let _ = event(&json!({ "event": "failed", "code": code }));
    bail!("{code}: {why}")
}

impl crate::Client {
    pub async fn membership(&mut self, command: ProofCommand) -> Result<()> {
        let (job, send) = match command {
            ProofCommand::Prepare(job) => (job, false),
            ProofCommand::Prove(job) => (job, true),
        };
        if !proving_is_bounded() {
            bail!("a membership proof runs only in this process on 4 threads (a local-prove build with RISC0_PROVER=local, RISC0_EXECUTOR=local, RAYON_NUM_THREADS=4 and no RISC0_SERVER_PATH). Nothing was sent.");
        }
        if send && !out_is_a_pipe() {
            bail!("names proof prove is for the Names module: its output holds a secret, so it only writes to a pipe. Nothing was sent.");
        }

        // Check 4: the gate on chain is the pinned, immutable image.
        let header = match self.actor_state_of(job.gate, PROGRAM_LOADER_ACCOUNT_ID).await {
            Ok(state) => ProgramHeader::from_loader_actor_state(&state),
            Err(error) => return refuse("RPC_UNREACHABLE", format!("{error:#}")),
        };
        let Some(header) = header else { return refuse("GATE_NOT_PINNED", format!("{} is not a program", job.gate)) };
        let image = image_bytes(&header.image_id);
        if image != job.image || !header.immutable {
            return refuse("GATE_NOT_PINNED", "the gate on chain isn't the pinned, immutable image".to_owned());
        }

        if let Err(error) = self.wallet.sync_to_latest_block().await {
            return refuse("RPC_UNREACHABLE", format!("{error:#}"));
        }
        let (Some(holder), Some(_)) = (self.wallet.resolve_private_account(job.holder), self.wallet.resolve_private_account(job.payer)) else {
            bail!("the holder and the fee payer must be private accounts of this wallet");
        };
        let before = self.wallet.get_account_private(job.holder).unwrap_or_default();
        let payer_before = self.wallet.get_account_private(job.payer).unwrap_or_default();
        let holds = match crate::nft_wire::State::decode(before.data.actor_state(job.program).as_ref()) {
            Ok(Some(crate::nft_wire::State::Copy { definition, owned })) => owned && definition == *job.collection.value(),
            Ok(Some(crate::nft_wire::State::Master { definition, print_balance })) => print_balance >= 1 && definition == *job.collection.value(),
            _ => false,
        };
        if !holds {
            return refuse("NOT_A_MEMBER", "this account's own note holds no NFT of that collection".to_owned());
        }
        let kept = secret_in(before.data.actor_state(job.gate).as_ref())?;
        let secret = match kept {
            Some(secret) => secret,
            None => {
                let mut fresh = [0; 32];
                getrandom::fill(&mut fresh).map_err(|error| anyhow!("no randomness: {error}"))?;
                fresh
            }
        };
        let slot = slot_of(&secret);
        let bytes = prove_message(job.collection.value(), &job.context, job.not_before, job.ttl_class, kept.is_none().then_some(&secret));
        let mentions = || {
            vec![
                holder.clone().select_program_actor_state(job.gate),
                holder.clone().select_program_actor_state(job.program),
                AccountIdentity::PublicPda { program: job.gate, seed: PdaSeed::new(slot) }.select_program_actor_state(job.gate).receiving(),
            ]
        };
        let gate_code = self.program_at(job.gate, "LOGOS_NAMES_CALL_ELF_UNUSED", "names-gate", "the membership gate").await?;
        if image_bytes(&gate_code.id()) != job.image {
            return refuse("GATE_NOT_PINNED", "the gate's bytecode isn't the pinned image".to_owned());
        }
        let nft_code = self.program_at(job.program, "LOGOS_NAMES_CALL_ELF_UNUSED", "names-nft", "the NFT program").await?;
        let catalog = ProgramCatalog::from([(job.gate, gate_code), (job.program, nft_code)]);

        // Check 5: the wallet's dry run. Check 2 needs its reserve.
        self.wallet.set_dry_run(true);
        let dry = self.wallet.send_tx_with_pre_check(mentions(), 0, bytes.clone(), &catalog, None, AsyncDelivery::default(), Some(job.payer), |_| Ok(())).await;
        self.wallet.set_dry_run(false);
        let reserve = match dry {
            Err(ExecutionFailureKind::DryRun(report)) => match *report {
                DryRunReport::Private { fee, headroom_reserve, .. } => headroom_reserve.max(fee.reserve),
                DryRunReport::Public { .. } => return refuse("UNEXPECTED_OUTCOME", "the simulation priced a public transaction".to_owned()),
            },
            Err(error) => {
                let said = error.to_string();
                let code = if said.contains("NOT_A_MEMBER") { "NOT_A_MEMBER" } else { "UNEXPECTED_OUTCOME" };
                return refuse(code, said);
            }
            Ok(_) => bail!("the wallet sent a transaction during a dry run; stop and check the wallet"),
        };
        let balance = payer_before.data.native_balance().unwrap_or(0);
        if balance < reserve {
            return refuse("FEE_RESERVE", format!("the payer holds {balance}, the reserve is {reserve}"));
        }
        if !send {
            return event(&json!({ "event": "prepared", "image": hex::encode(image), "immutable": header.immutable, "reserve": reserve.to_string(), "balance": balance.to_string() })).map_err(|error| anyhow!(error));
        }

        let pending = self.wallet.owned_pending_messages().await.context("cannot read the payments waiting for this wallet")?;
        let credit = |account: AccountId| -> u128 { pending.iter().filter(|m| m.recipient.account_id() == account).filter_map(|m| m.native_credit()).sum() };
        let separate_payer = job.payer != job.holder;
        let (holder_credit, payer_credit) = (credit(job.holder), if separate_payer { credit(job.payer) } else { 0 });
        let key_chain = self.wallet.storage().key_chain().clone();
        let (holder_id, payer_id) = (job.holder, job.payer);
        let nft_before = before.data.actor_state(job.program).to_vec();
        self.wallet.client().hold_send(Box::new(move |tx: &LeeTransaction| {
            let LeeTransaction::PrivacyPreserving(proven) = tx else { return Err("the transaction isn't private".to_owned()) };
            let (message, execution) = (&proven.message, &proven.message.execution);
            let mut after = key_chain.clone();
            let mut index = after.build_latest_nullifier_index();
            after.follow_transaction(message, false, false, &mut index);
            let state_after = |account: AccountId| after.private_account(account).map(|found| states_json(found.account)).ok_or("an account left the key chain");
            let payer = if separate_payer { json!([states_json(&payer_before), state_after(payer_id)?]) } else { Value::Null };
            event(&json!({
                "event": "proven",
                // Saved by the module before it says send, so a restart can ask the chain (never prove or pay twice).
                "tx_hash": tx.hash().to_string(),
                "secret": hex::encode(secret),
                "reserve": reserve.to_string(),
                "holder_credit": holder_credit.to_string(),
                "payer_credit": payer_credit.to_string(),
                "public": {
                    "public_root": execution.public_root.is_some(),
                    "actors": message.public_context.actors.iter().map(actor_json).collect::<Vec<_>>(),
                    "boundary": execution.boundary.iter().map(step_json).collect::<Vec<_>>(),
                    "window": [execution.validity.timestamps.start(), execution.validity.timestamps.end()],
                    "async_messages": execution.async_messages.len(),
                },
                "private": { "holder_before": states_json(&before), "holder_after": state_after(holder_id)?, "payer": payer },
            }))?;
            if may_send(std::io::stdin().lock()) { Ok(()) } else { Err("the module didn't say send".to_owned()) }
        }));

        event(&json!({ "event": "proving" })).map_err(|error| anyhow!(error))?;
        let nft_actor = Actor::new(job.holder, job.program);
        let sent = self
            .wallet
            .send_tx_with_pre_check(mentions(), 0, bytes, &catalog, None, AsyncDelivery::default(), Some(job.payer), |states| {
                // The NFT actor proven over is the one checked above.
                match states.iter().any(|row| row.selector == nft_actor && row.data.as_ref() != nft_before.as_slice()) {
                    true => Err(ExecutionFailureKind::AccountDataError(holder_id)),
                    false => Ok(()),
                }
            })
            .await;
        let (hash, _) = sent.context("the proof was not sent, or its send has no answer")?;
        event(&json!({ "event": "submitted", "tx_hash": hash.to_string() })).map_err(|error| anyhow!(error))?;
        let block = self.finish(hash).await.context("the proof was sent and is not confirmed")?;
        event(&json!({ "event": "applied", "tx_hash": hash.to_string(), "block": block })).map_err(|error| anyhow!(error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_exact_line_send_lets_a_proof_go() {
        assert!(may_send(&b"send\n"[..]));
        for no in [&b""[..], b"send", b"send \n", b" send\n", b"SEND\n", b"send\r\n", b"cancel\n", b"\nsend\n", b"send now\n", b"\xff\n", b"sendsendsendsendsendsend\n"] {
            assert!(!may_send(no), "{no:?}");
        }
    }

    #[test]
    fn the_message_is_the_gate_interfaces_borsh() {
        let first = prove_message(&[1; 32], &[2; 32], 0x0102, 2, Some(&[3; 32]));
        assert_eq!(first.len(), 1 + 32 + 32 + 8 + 1 + 1 + 32);
        assert_eq!((first[0], &first[65..73], first[73], first[74]), (0, &[2, 1, 0, 0, 0, 0, 0, 0][..], 2, 1));
        assert_eq!(&first[75..], [3; 32]);
        let later = prove_message(&[1; 32], &[2; 32], 0x0102, 2, None);
        assert_eq!((later.len(), later[74]), (75, 0));
    }

    #[test]
    fn only_an_empty_or_idle_gate_state_is_proved_from() {
        assert_eq!(secret_in(&[]).unwrap(), None);
        let mut idle = vec![0, 1];
        idle.extend_from_slice(&[7; 32]);
        idle.push(0);
        assert_eq!(secret_in(&idle).unwrap(), Some([7; 32]));
        for at in [0, 1, IDLE_LEN - 1] {
            let mut bad = idle.clone();
            bad[at] = 9;
            assert!(secret_in(&bad).is_err());
        }
        assert!(secret_in(&idle[..34]).is_err());
    }
}
