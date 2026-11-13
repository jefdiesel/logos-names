//! `testimonial`: post a testimonial to the LEZ 0.4 testimonial program, and list a
//! submission's testimonials with the counts the LP-0021 adoption criteria are judged on.
//!
//! Reads (`list`, `show`, `address`) are plain sequencer calls and need no wallet. `post`
//! signs with the LEZ wallet at `LEE_WALLET_HOME_DIR` (default `~/.lee/wallet`), which must
//! hold the author's key; the author pays the fee.

mod rpc;

use anyhow::{Context as _, Result, anyhow, bail};
use clap::{Parser, Subcommand};
use clock_core::{CLOCK_PROGRAM_ACCOUNT_ID, ClockAccountData, clock_account_id};
use common::{HashType, transaction::LeeTransaction};
use lee::{
    PublicTransaction, Simulation,
    privacy_preserving_transaction::circuit::ProgramCatalog,
    program::Program,
    public_transaction::{Message as TxMessage, WitnessSet},
    simulate_public_transaction,
};
use lee_core::account::{AccountId, Actor};
use rpc::Rpc;
use sequencer_service_rpc::SequencerClient;
use serde_json::json;
use testimonial_core::{
    ADOPTION_MONTHS, ADOPTION_PER_MONTH, ADOPTION_TOTAL, Instruction, Message, Testimonial, format_time,
    head_address, head_seed, month_of, node_address, node_seed, summarize, validate_post, walk,
};
use wallet::{
    AccountIdentity, AccountMention, WalletCore, WalletSequencerClient as _,
    config::{SequencerConnectionData, WalletConfigOverrides},
    max_fee_for,
    program_facades::program_loader::ProgramLoader,
    utils::{fetch_config_path, fetch_persistent_storage_path, read_password},
};

/// The least gas limit a post is sent with; the limit is the simulated cycles × 1.5.
const MIN_GAS_LIMIT: u64 = 30_000;
/// How many blocks to wait for a sent transaction.
const TX_POLL_MAX_BLOCKS: usize = 60;

#[derive(Parser)]
#[command(name = "testimonial", version, about = "Testimonials on LEZ 0.4: post one, or list a submission's")]
struct Cli {
    /// The LEZ sequencer to talk to.
    #[arg(long, env = "TESTIMONIAL_RPC", default_value = "http://127.0.0.1:3040", global = true)]
    rpc: String,
    /// The testimonial program's account ID (its header account).
    #[arg(long, env = "TESTIMONIAL_PROGRAM", global = true)]
    program: Option<String>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Post a testimonial for a submission, signed by `--author` (one per account and submission).
    Post {
        /// The submission's identifier, e.g. `lp-0021/names-wallet`.
        #[arg(long)]
        submission: String,
        /// The testimonial (at most 1024 bytes).
        #[arg(long)]
        text: String,
        /// A display name (at most 32 bytes; not proven).
        #[arg(long)]
        username: Option<String>,
        /// The account that signs and pays: one this wallet holds.
        #[arg(long)]
        author: String,
        /// Try it against the chain and print the gas, without sending.
        #[arg(long)]
        dry_run: bool,
    },
    /// List a submission's testimonials, newest first, with per-month counts.
    List {
        #[arg(long)]
        submission: String,
        /// Machine-readable output.
        #[arg(long)]
        json: bool,
        /// Also read each author's current nonce (how many transactions it has sent), a hint
        /// of activity beyond the testimonial.
        #[arg(long)]
        nonces: bool,
    },
    /// Show one account's testimonial for a submission.
    Show {
        #[arg(long)]
        submission: String,
        #[arg(long)]
        author: String,
    },
    /// Print the addresses a submission's list (and an author's testimonial) live at.
    Address {
        #[arg(long)]
        submission: String,
        #[arg(long)]
        author: Option<String>,
    },
}

fn account(s: &str) -> Result<AccountId> {
    let bare = s.strip_prefix("Public/").or_else(|| s.strip_prefix("Private/")).unwrap_or(s);
    bare.parse().map_err(|_| anyhow!("{s} is not an account ID"))
}

fn program_id(cli: &Cli) -> Result<AccountId> {
    let p = cli.program.as_deref().ok_or_else(|| anyhow!("pass --program <id> or set TESTIMONIAL_PROGRAM"))?;
    account(p)
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match &cli.command {
        Command::Post { submission, text, username, author, dry_run } => {
            post(&cli, submission, username.as_deref(), text, account(author)?, *dry_run).await
        }
        Command::List { submission, json, nonces } => list(&cli, submission, *json, *nonces),
        Command::Show { submission, author } => show(&cli, submission, account(author)?),
        Command::Address { submission, author } => {
            let program = program_id(&cli)?;
            println!("head        {}", head_address(&program, submission));
            if let Some(author) = author {
                println!("testimonial {}", node_address(&program, submission, &account(author)?));
            }
            Ok(())
        }
    }
}

// --- reads ----------------------------------------------------------------------------------

fn print_testimonial(address: &AccountId, t: &Testimonial, nonce: Option<u128>) {
    let name = t.username.as_deref().map(|u| format!(" \"{u}\"")).unwrap_or_default();
    let nonce = nonce.map(|n| format!("  (nonce {n})")).unwrap_or_default();
    println!("#{:<4} {}  {}{name}{nonce}", t.seq, format_time(t.at), t.author);
    println!("      at {address}");
    for line in t.text.lines() {
        println!("      | {line}");
    }
}

fn list(cli: &Cli, submission: &str, as_json: bool, nonces: bool) -> Result<()> {
    let program = program_id(cli)?;
    let rpc = Rpc::new(&cli.rpc);
    let listing = walk(&program, submission, |a| rpc.actor_state(a, &program)).map_err(|e| anyhow!(e))?;
    let summary = summarize(&listing);
    let author_nonces: Vec<Option<u128>> = if nonces {
        listing
            .items
            .iter()
            .map(|(_, t)| rpc.account(&t.author).map(|a| a.map(|a| a.nonce)))
            .collect::<Result<_>>()?
    } else {
        vec![None; listing.items.len()]
    };
    if as_json {
        let items: Vec<_> = listing
            .items
            .iter()
            .zip(&author_nonces)
            .map(|((address, t), nonce)| {
                json!({
                    "seq": t.seq,
                    "address": address.to_string(),
                    "author": t.author.to_string(),
                    "at": t.at,
                    "at_utc": format_time(t.at),
                    "month": month_of(t.at),
                    "username": t.username,
                    "text": t.text,
                    "author_nonce": nonce.map(|n| n.to_string()),
                })
            })
            .collect();
        let out = json!({
            "program": program.to_string(),
            "submission": submission,
            "head": listing.head_address.to_string(),
            "count": summary.total,
            "distinct_authors": summary.distinct_authors,
            "first_at": summary.first_at,
            "last_at": summary.last_at,
            "per_month": summary.per_month,
            "qualifying_months": summary.qualifying_months,
            "meets_adoption": summary.meets_adoption,
            "rpc_calls": rpc.calls(),
            "testimonials": items,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }
    println!("submission  {submission}");
    println!("program     {program}");
    println!("head        {}", listing.head_address);
    if listing.head.is_none() {
        println!("\nno testimonials yet");
        return Ok(());
    }
    println!(
        "\n{} testimonials from {} distinct accounts, {} to {}",
        summary.total,
        summary.distinct_authors,
        summary.first_at.map(format_time).unwrap_or_default(),
        summary.last_at.map(format_time).unwrap_or_default()
    );
    println!("per month (UTC, by the on-chain time):");
    for (month, n) in &summary.per_month {
        let mark = if *n >= ADOPTION_PER_MONTH { "  ok" } else { "" };
        println!("  {month}  {n:>5}{mark}");
    }
    println!(
        "adoption bar ({ADOPTION_TOTAL} testimonials from {ADOPTION_TOTAL} accounts, {ADOPTION_MONTHS}+ months of {ADOPTION_PER_MONTH}+): {}",
        if summary.meets_adoption { "met" } else { "not met yet" }
    );
    println!();
    for ((address, t), nonce) in listing.items.iter().zip(author_nonces) {
        print_testimonial(address, t, nonce);
    }
    println!("\n({} sequencer reads, checked: one record per account, numbered {}..0, no gaps)", rpc.calls(), summary.total.saturating_sub(1));
    Ok(())
}

fn show(cli: &Cli, submission: &str, author: AccountId) -> Result<()> {
    let program = program_id(cli)?;
    let rpc = Rpc::new(&cli.rpc);
    let address = node_address(&program, submission, &author);
    let state = rpc.actor_state(&address, &program)?;
    if state.is_empty() {
        bail!("{author} has no testimonial for {submission} (it would be at {address})");
    }
    let t: Testimonial = borsh::from_slice(&state).context("not a testimonial")?;
    print_testimonial(&address, &t, None);
    Ok(())
}

// --- post -----------------------------------------------------------------------------------

/// Writes the wallet's default config pointed at `rpc` if it is missing (never at the LEZ
/// wallet's built-in default sequencer).
fn write_default_wallet_config(rpc: &str) -> Result<()> {
    let config_path = fetch_config_path()?;
    if let Some(home) = config_path.parent() {
        std::fs::create_dir_all(home).with_context(|| format!("failed to create the wallet home {}", home.display()))?;
    }
    if !config_path.exists() {
        let sequencer = SequencerConnectionData { sequencer_addr: rpc.parse().map_err(|_| anyhow!("{rpc} is not a URL"))?, basic_auth: None };
        let config = wallet::config::WalletConfig { sequencer, ..Default::default() };
        std::fs::write(&config_path, serde_json::to_vec_pretty(&serde_json::to_value(config)?)?)?;
    }
    Ok(())
}

async fn open_wallet(rpc: &str) -> Result<WalletCore<SequencerClient>> {
    let overrides = Some(WalletConfigOverrides {
        sequencer: Some(SequencerConnectionData { sequencer_addr: rpc.parse().map_err(|_| anyhow!("{rpc} is not a URL"))?, basic_auth: None }),
        seq_tx_poll_max_blocks: Some(TX_POLL_MAX_BLOCKS),
        ..Default::default()
    });
    write_default_wallet_config(rpc)?;
    let storage_path = fetch_persistent_storage_path()?;
    if !storage_path.exists() {
        bail!(
            "there is no wallet at {}: set LEE_WALLET_HOME_DIR to one holding the author's key",
            storage_path.display()
        );
    }
    let password = read_password(&storage_path)?;
    WalletCore::new_update_chain(fetch_config_path()?, storage_path, overrides, &password)
        .context("cannot open the wallet (a LEZ 0.3 wallet does not load on 0.4)")
}

async fn actor_state(w: &WalletCore<SequencerClient>, account: AccountId, program: AccountId) -> Result<Vec<u8>> {
    let view = w.get_account_view(Actor::new(account, program)).await.context("reading from the sequencer")?;
    Ok(view.map_or_else(Vec::new, |found| found.data.actor_state(program).to_vec()))
}

/// The chain's time (the clock program's last block time), or this machine's minus a minute.
async fn chain_now(w: &WalletCore<SequencerClient>) -> u64 {
    let local = || {
        u64::try_from(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis())
            .unwrap_or(u64::MAX)
            .saturating_sub(60_000)
    };
    actor_state(w, CLOCK_PROGRAM_ACCOUNT_ID, clock_account_id())
        .await
        .ok()
        .and_then(|s| borsh::from_slice::<ClockAccountData>(&s).ok())
        .map_or_else(local, |d| d.timestamp)
}

/// Runs the transaction against current chain state with the deployed bytecode, without
/// sending it: a post the program would refuse fails here, before any fee. Returns cycles.
async fn simulate(w: &WalletCore<SequencerClient>, mentions: &[AccountMention], message: &[u8], program: (AccountId, Program)) -> Result<u64> {
    let actors: Vec<Actor> = mentions.iter().map(AccountMention::actor).collect();
    let mut states = std::collections::HashMap::new();
    for a in &actors {
        states.insert(*a, actor_state(w, a.account_id, a.program_account_id).await?.into());
    }
    let signers: Vec<AccountId> = mentions
        .iter()
        .filter(|m| m.authorizes && m.identity.public_account_id().is_some())
        .map(|m| m.identity.account_id())
        .filter(|a| w.get_account_public_signing_key(*a).is_some())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let nonces = w.get_accounts_nonces(&signers).await?;
    let root = actors[0];
    let tx = TxMessage::new(root, message.to_vec(), actors, signers.into_iter().zip(nonces).collect(), None, Vec::new());
    let tx = PublicTransaction::new(tx.clone(), WitnessSet::for_message(&tx, &[]));
    let simulation = Simulation { public_actor_states: states, admitted_accounts: None };
    let catalog: ProgramCatalog = std::iter::once(program).collect();
    let budget = w.client().get_fee_state().await?.max_gas_exec;
    let (cycles, result) = tokio::task::spawn_blocking(move || simulate_public_transaction(&tx, &simulation, &catalog, budget)).await?;
    result.map_err(|e| anyhow!("refused: {e}"))?;
    Ok(cycles)
}

/// Waits for `hash` and returns its block, failing if it reverted. A block includes reverted
/// transactions too, so inclusion alone proves nothing.
async fn finish(w: &mut WalletCore<SequencerClient>, hash: HashType) -> Result<u64> {
    let (tx, block_id) = w.poll_transaction(hash).await.context("the transaction was not included in a block")?;
    let block = w.get_block(block_id).await?.ok_or_else(|| anyhow!("block {block_id} is missing"))?;
    let reverted = block
        .body
        .transactions
        .iter()
        .position(|included| included.hash() == hash)
        .is_some_and(|p| block.body.is_reverted(p));
    if let LeeTransaction::PrivacyPreserving(tx) = &tx {
        // A private author's note was spent and a new one made; record it.
        let key_chain = w.storage_mut().key_chain_mut();
        let mut index = key_chain.build_latest_nullifier_index();
        key_chain.follow_transaction(&tx.message, reverted, false, &mut index);
        w.store_persistent_data()?;
    }
    if reverted {
        bail!("{hash} was included in block {block_id} but reverted; its fee was charged and nothing else applied");
    }
    Ok(block_id)
}

async fn post(cli: &Cli, submission: &str, username: Option<&str>, text: &str, author: AccountId, dry_run: bool) -> Result<()> {
    let program = program_id(cli)?;
    validate_post(submission, username, text).map_err(|e| anyhow!(e))?;
    let node = node_address(&program, submission, &author);
    let rpc = Rpc::new(&cli.rpc);
    if !rpc.actor_state(&node, &program)?.is_empty() {
        bail!("{author} already posted a testimonial for {submission} (at {node}); there is one per account");
    }
    let mut w = open_wallet(&cli.rpc).await?;
    let identity = w.resolve_private_account(author).unwrap_or(AccountIdentity::Public(author));
    let private = identity.is_private();
    if private && std::env::var_os("TESTIMONIAL_TRY_PRIVATE").is_none() {
        // The LEZ 0.4 wallet presents a private sender to public actors under a fresh random
        // alias, and the program refuses an alias (it would let one account post many times).
        // Proving takes minutes, so say so before starting.
        bail!(
            "{author} is a private account: the LEZ 0.4 wallet presents it under a fresh alias, which the program refuses so that each account posts once; post from a public account (TESTIMONIAL_TRY_PRIVATE=1 tries anyway)"
        );
    }
    if !private && w.get_account_public_signing_key(author).is_none() {
        bail!("this wallet holds no key for {author}");
    }
    let bytecode = ProgramLoader(&w)
        .program(program)
        .await
        .context("reading the testimonial program from the sequencer")?
        .ok_or_else(|| anyhow!("{program} is not a deployed program"))?;
    let now = chain_now(&w).await;
    let instruction = Instruction::Post {
        submission: submission.into(),
        username: username.map(Into::into),
        text: text.into(),
        now,
    };
    let message = borsh::to_vec(&Message::Instruction(instruction))?;
    let pda = |seed| AccountIdentity::PublicPda { program, seed }.select_program_actor_state(program);
    let mentions = vec![identity.select_program_actor_state(program), pda(head_seed(submission)), pda(node_seed(submission, &author))];
    println!("posting for {submission} as {author} (time {})", format_time(now));
    let hash = if private {
        // Only with TESTIMONIAL_TRY_PRIVATE: the program runs inside a proof made here. With
        // the stock wallet's blinded presentation the head refuses it (see README).
        if dry_run {
            bail!("--dry-run tries public posts only");
        }
        println!("private author: proving on this computer (minutes)");
        let catalog: ProgramCatalog = std::iter::once((program, bytecode)).collect();
        w.send_privacy_preserving_tx(mentions, 0, message, &catalog).await.context("the sequencer did not accept the transaction")?.0
    } else {
        let cycles = simulate(&w, &mentions, &message, (program, bytecode)).await?;
        let limit = cycles.saturating_add(cycles.div_ceil(2)).max(MIN_GAS_LIMIT);
        let quote = w.client().get_fee_state().await?;
        let reserve = u128::from(limit) * u128::from(quote.next_base_fee_exec_ceiling)
            + (message.len() as u128 + 1_000) * u128::from(quote.next_base_fee_stor_ceiling);
        if reserve > max_fee_for(limit) {
            bail!("the chain's fees are too high right now (reserve {reserve})");
        }
        let balance = w.get_account_balance(author).await?;
        println!("simulated: ok, {cycles} cycles, gas limit {limit}, fee reserve ~{reserve} (balance {balance})");
        if dry_run {
            println!("dry run: nothing sent");
            return Ok(());
        }
        if balance < reserve {
            bail!("{author} holds {balance}; about {reserve} is held for the fee while this runs");
        }
        let saved = w.config().clone();
        let mut config = saved.clone();
        config.gas_limit = limit;
        w.set_config(config);
        let sent = w.send_pub_tx(mentions, 0, message).await;
        w.set_config(saved);
        sent.context("the sequencer did not accept the transaction")?
    };
    println!("submitted {hash}");
    let block = finish(&mut w, hash).await?;
    let state = rpc.actor_state(&node, &program)?;
    let t: Testimonial = borsh::from_slice(&state).map_err(|_| anyhow!("applied in block {block}, but {node} holds no testimonial"))?;
    println!("applied in block {block}; read back:");
    print_testimonial(&node, &t, None);
    Ok(())
}
