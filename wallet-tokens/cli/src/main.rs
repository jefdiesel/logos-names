//! `wallet-tokens`: a prototype client for the prototype token program on LEZ 0.4.
//!
//! It deploys the program, creates a token, mints, sends (public or private, either side),
//! lists holdings and receives pending private credits, all through the stock LEZ `wallet`
//! crate (tag v0.4.0-rc1). The sends reuse the wallet's native-transfer path with the token
//! program in place of the native one; see `docs/wallet/TOKENS.md`.
//!
//! Safety: the wallet's built-in default sequencer is the public testnet. This tool always
//! overrides it with `--sequencer` (default `http://127.0.0.1:3180`) and refuses any address
//! that is not loopback.

use std::path::PathBuf;

use anyhow::{Context as _, Result, anyhow, bail};
use clap::{Parser, Subcommand};
use common::{HashType, transaction::LeeTransaction};
use lee::{PublicKey, privacy_preserving_transaction::circuit::ProgramCatalog, program::Program};
use lee_core::{
    NullifierPublicKey, PrivateAccountKind,
    account::{AccountId, Actor, Balance},
    encryption::ViewingPublicKey,
};
use sequencer_service_rpc::SequencerClient;
use token_core::{Message, TokenDefinition, TokenState};
use wallet::{
    AccountIdentity, AsyncDelivery, ExecutionFailureKind, WalletCore,
    config::{SequencerConnectionData, WalletConfigOverrides},
    program_facades::program_loader::ProgramLoader,
    utils::{fetch_config_path, fetch_persistent_storage_path, read_password},
};

/// `program_loader_core::MAX_SEGMENT_DATA_LEN` at v0.4.0-rc1
/// (lez/programs/program_loader/core/src/lib.rs:24).
const MAX_SEGMENT_DATA_LEN: usize = 96 * 1024;

#[derive(Parser)]
#[command(name = "wallet-tokens", about = "Prototype token client for LEZ 0.4")]
struct Cli {
    /// The sequencer. Loopback only: this is a prototype for local chains.
    #[arg(long, env = "WALLET_TOKENS_SEQUENCER", default_value = "http://127.0.0.1:3180")]
    sequencer: String,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// The LEZ wallet's own command line (accounts, pending messages, native transfers).
    Wallet {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Deploy the token program built into this binary (or `--elf`); prints its account ID.
    Deploy {
        #[arg(long)]
        payer: AccountId,
        #[arg(long)]
        elf: Option<PathBuf>,
    },
    /// Create a fungible token at a new account of this wallet; prints the definition ID.
    Create {
        #[arg(long)]
        program: AccountId,
        #[arg(long)]
        name: String,
        #[arg(long)]
        supply: Balance,
        /// The account credited with the whole initial supply.
        #[arg(long)]
        to: AccountId,
        /// Who may mint more; leave out for a fixed supply.
        #[arg(long)]
        authority: Option<AccountId>,
        /// A funded public account of this wallet that pays the fee.
        #[arg(long)]
        payer: AccountId,
    },
    /// Mint more of a token; `--authority` must be its mint authority and sign.
    Mint {
        #[arg(long)]
        program: AccountId,
        #[arg(long)]
        definition: AccountId,
        #[arg(long)]
        authority: AccountId,
        #[arg(long)]
        to: AccountId,
        #[arg(long)]
        amount: Balance,
    },
    /// Send a token from an account of this wallet, public or private.
    Send {
        #[arg(long)]
        program: AccountId,
        #[arg(long)]
        definition: AccountId,
        #[arg(long)]
        from: AccountId,
        /// A public account, or a private account of this wallet.
        #[arg(long, conflicts_with_all = ["to_pk", "to_npk", "to_vpk"])]
        to: Option<AccountId>,
        /// A public account never used yet, by its public key (it must be admitted).
        #[arg(long)]
        to_pk: Option<PublicKey>,
        /// Someone else's private account: its nullifier public key (hex)...
        #[arg(long, requires = "to_vpk")]
        to_npk: Option<String>,
        /// ...and viewing public key (hex). The credit stays pending until they receive it.
        #[arg(long, requires = "to_npk")]
        to_vpk: Option<String>,
        #[arg(long)]
        amount: Balance,
    },
    /// Every token this wallet's accounts hold (public read from the chain, private from the
    /// wallet's notes after a sync).
    Balances {
        #[arg(long)]
        program: AccountId,
    },
    /// A token definition.
    Definition {
        #[arg(long)]
        program: AccountId,
        #[arg(long)]
        definition: AccountId,
    },
    /// Receive the pending token credits sent to this wallet's private accounts.
    Receive {
        #[arg(long)]
        program: AccountId,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let overrides = local_only(&cli.sequencer)?;
    match cli.command {
        Command::Wallet { args } => run_wallet(args, overrides).await,
        command => {
            let mut wallet = open_wallet(overrides)?;
            run(&mut wallet, command).await
        }
    }
}

/// The config override that pins the sequencer, after checking it is loopback.
fn local_only(sequencer: &str) -> Result<WalletConfigOverrides> {
    let url: url::Url = sequencer.parse().context("--sequencer is not a URL")?;
    let host = url.host_str().unwrap_or_default();
    if !matches!(host, "127.0.0.1" | "localhost" | "[::1]" | "::1") {
        bail!("{sequencer} is not loopback; this prototype only talks to a local chain");
    }
    Ok(WalletConfigOverrides {
        sequencer: Some(SequencerConnectionData {
            sequencer_addr: url,
            basic_auth: None,
        }),
        ..WalletConfigOverrides::default()
    })
}

fn open_wallet(overrides: WalletConfigOverrides) -> Result<WalletCore<SequencerClient>> {
    let storage_path = fetch_persistent_storage_path()?;
    if !storage_path.exists() {
        bail!("no wallet yet: run `wallet-tokens wallet check-health` first to create one");
    }
    let password = read_password(&storage_path)?;
    WalletCore::new_update_chain(fetch_config_path()?, storage_path, Some(overrides), &password)
}

/// Runs the LEZ wallet's command line, as its `wallet` binary does (lez/wallet/src/main.rs),
/// with the sequencer pinned. A new wallet's recovery phrase is not printed: this is a
/// throwaway test wallet.
async fn run_wallet(args: Vec<String>, overrides: WalletConfigOverrides) -> Result<()> {
    use clap::Parser as _;
    use wallet::cli::{Args, execute_subcommand};

    let Args { command, dry_run, fee_payer, refund_to, .. } =
        Args::parse_from(std::iter::once("wallet-tokens wallet".to_owned()).chain(args));
    let config_path = fetch_config_path()?;
    let storage_path = fetch_persistent_storage_path()?;
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let command = command.ok_or_else(|| anyhow!("give a wallet command"))?;
    let mut wallet = if storage_path.exists() {
        let password = read_password(&storage_path)?;
        WalletCore::new_update_chain(config_path, storage_path, Some(overrides), &password)?
    } else {
        let password = std::env::var(wallet::PASSWORD_ENV_VAR).unwrap_or_default();
        let (mut wallet, _mnemonic) =
            WalletCore::new_init_storage(config_path, storage_path, Some(overrides), &password)?;
        wallet.start_at_chain_tip().await.ok();
        wallet.store_persistent_data()?;
        println!("created a new wallet (recovery phrase not shown)");
        wallet
    };
    wallet.set_dry_run(dry_run);
    wallet.set_private_fee_payer(fee_payer);
    wallet.set_private_refund_target(refund_to);
    match execute_subcommand(&mut wallet, command).await {
        Err(error)
            if matches!(error.root_cause().downcast_ref(), Some(ExecutionFailureKind::DryRun)) =>
        {
            Ok(())
        }
        result => result.map(|_| ()),
    }
}

async fn run(wallet: &mut WalletCore<SequencerClient>, command: Command) -> Result<()> {
    match command {
        Command::Wallet { .. } => unreachable!("handled before the wallet opens"),
        Command::Deploy { payer, elf } => {
            let bytecode = match elf {
                Some(path) => std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?,
                None => wallet_tokens_methods::WALLET_TOKENS_ELF.to_vec(),
            };
            let program = Program::new(bytecode.clone().into()).map_err(|e| anyhow!("bad ELF: {e}"))?;
            println!("image id {}", hex(&program.id()));
            // As lez examples/program_deployment/src/lib.rs: a header and one account per segment.
            let header = wallet.create_new_account_public(None).0;
            let segments: Vec<AccountId> = std::iter::repeat_with(|| wallet.create_new_account_public(None).0)
                .take(bytecode.len().div_ceil(MAX_SEGMENT_DATA_LEN))
                .collect();
            wallet.store_persistent_data()?;
            println!("deploying {} bytes in {} segment(s) at header {header}", bytecode.len(), segments.len());
            ProgramLoader(wallet).deploy(header, &segments, bytecode, false, Some(payer)).await?;
            let deployed = ProgramLoader(wallet)
                .program(header)
                .await?
                .ok_or_else(|| anyhow!("{header} holds no program after the deploy"))?;
            if deployed.id() != program.id() {
                bail!("{header} holds image {} instead", hex(&deployed.id()));
            }
            println!("program {header}");
            Ok(())
        }
        Command::Create { program, name, supply, to, authority, payer } => {
            let definition = wallet.create_new_account_public(None).0;
            wallet.store_persistent_data()?;
            println!("definition account {definition}");
            let message = Message::NewFungibleDefinition {
                name,
                total_supply: supply,
                mint_authority: authority,
                supply_to: to,
            };
            // Root: the new definition's actor, signed by its key. The supply's credit is async;
            // declaring the recipient's actor runs it in the same transaction.
            let mut accounts = vec![AccountIdentity::Public(definition).select_program_actor_state(program)];
            accounts.push(identity(wallet, to).select_program_actor_state(program).receiving().without_authorization());
            let hash = wallet
                .send_pub_tx_paid_by(accounts, 0, borsh::to_vec(&message)?, Some(payer))
                .await?;
            finish(wallet, hash).await?;
            println!("token {definition}");
            Ok(())
        }
        Command::Mint { program, definition, authority, to, amount } => {
            let message = Message::Mint { definition, to, amount };
            let mut accounts = vec![identity(wallet, authority).select_program_actor_state(program)];
            if definition != authority {
                accounts.push(AccountIdentity::PublicNoSign(definition).select_program_actor_state(program));
            }
            accounts.push(identity(wallet, to).select_program_actor_state(program).receiving().without_authorization());
            send(wallet, program, accounts, &message, AsyncDelivery::default()).await
        }
        Command::Send { program, definition, from, to, to_pk, to_npk, to_vpk, amount } => {
            let sender = identity(wallet, from);
            let recipient = match (to, to_pk, to_npk, to_vpk) {
                (Some(to), ..) => identity(wallet, to),
                (None, Some(pk), ..) => AccountIdentity::PublicForeign(pk),
                (None, None, Some(npk), Some(vpk)) => AccountIdentity::PrivateForeign {
                    npk: NullifierPublicKey(hex_array(&npk)?),
                    vpk: ViewingPublicKey::from_bytes(hex_bytes(&vpk)?)
                        .map_err(|e| anyhow!("--to-vpk: {e}"))?,
                    kind: PrivateAccountKind::Regular,
                },
                _ => bail!("give --to, --to-pk, or --to-npk with --to-vpk"),
            };
            let held = holdings(wallet, &sender, program).await?;
            let balance = held.get(&definition).copied().unwrap_or(0);
            if balance < amount {
                bail!("{from} holds {balance} of {definition}");
            }
            let message = Message::Transfer { definition, to: recipient.account_id(), amount };
            // What `wallet::program_facades::credit_destination` does for native transfers
            // (lez/wallet/src/program_facades/mod.rs:25-47), with the token program: a
            // recipient this wallet can witness takes part now; a foreign private one gets a
            // pending message sealed (private sender) or bound to a recovery note (public).
            let mention = recipient.clone().select_program_actor_state(program);
            let pending = matches!(recipient, AccountIdentity::PrivateForeign { .. });
            let (declared, delivery) = if pending {
                let (_, mut delivery) = if sender.is_private() {
                    wallet.seal_destination(mention)?
                } else {
                    wallet.async_destination(mention)?
                };
                delivery.promotions.listed_only = true;
                (None, delivery)
            } else {
                (Some(mention.receiving()), AsyncDelivery::default())
            };
            let mut accounts = vec![sender.select_program_actor_state(program)];
            accounts.extend(declared);
            send(wallet, program, accounts, &message, delivery).await
        }
        Command::Balances { program } => {
            wallet.sync_to_latest_block().await?;
            let key_chain = wallet.storage().key_chain();
            let public: Vec<AccountId> = key_chain.public_account_ids().map(|(id, _)| id).collect();
            let private: Vec<AccountId> = key_chain.private_account_ids().map(|(id, _)| id).collect();
            for (prefix, ids) in [("Public", public), ("Private", private)] {
                for id in ids {
                    let identity = identity(wallet, id);
                    for (definition, balance) in holdings(wallet, &identity, program).await? {
                        println!("{prefix}/{id}  {definition}  {balance}");
                    }
                }
            }
            Ok(())
        }
        Command::Definition { program, definition } => {
            let state = public_state(wallet, definition, program).await?;
            match TokenState::decode(&state) {
                Some(TokenState::Definition(TokenDefinition::Fungible { name, total_supply, authority, .. })) => {
                    println!("name {name}\nsupply {total_supply}");
                    println!("authority {}", authority.map_or("none (fixed supply)".to_owned(), |a| a.to_string()));
                    Ok(())
                }
                _ => bail!("{definition} is not a token definition of {program}"),
            }
        }
        Command::Receive { program } => {
            let catalog = catalog(wallet, program).await?;
            let pending: Vec<_> = wallet
                .owned_pending_messages()
                .await?
                .into_iter()
                .filter(|pending| pending.body.to.program_account_id == program)
                .collect();
            println!("{} pending token message(s)", pending.len());
            for message in pending {
                let to = message.body.to.account_id;
                let what = borsh::from_slice::<Message>(&message.body.message).ok();
                println!("receiving {what:?} at Private/{to} (position {})", message.position);
                let (hash, _) = wallet
                    .receive_pending_message_with(message, Vec::new(), &catalog, None, AsyncDelivery::default())
                    .await?;
                finish(wallet, hash).await?;
            }
            Ok(())
        }
    }
}

/// How `account` takes part: a private account of this wallet, else public.
fn identity(wallet: &WalletCore<SequencerClient>, account: AccountId) -> AccountIdentity {
    wallet.resolve_private_account(account).unwrap_or(AccountIdentity::Public(account))
}

/// The token program's state on a public account.
async fn public_state(
    wallet: &WalletCore<SequencerClient>,
    account: AccountId,
    program: AccountId,
) -> Result<Vec<u8>> {
    let view = wallet.get_account_view(Actor::new(account, program)).await?;
    Ok(view.map_or_else(Vec::new, |found| found.data.actor_state(program).to_vec()))
}

/// What `account` holds under the token program, read where it lives.
async fn holdings(
    wallet: &WalletCore<SequencerClient>,
    account: &AccountIdentity,
    program: AccountId,
) -> Result<std::collections::BTreeMap<AccountId, Balance>> {
    let id = account.account_id();
    let state = if account.is_private() {
        wallet
            .get_account_private(id)
            .map(|found| found.data.actor_state(program).to_vec())
            .unwrap_or_default()
    } else {
        public_state(wallet, id, program).await?
    };
    match TokenState::decode(&state) {
        Some(TokenState::Holdings(holdings)) => Ok(holdings),
        Some(TokenState::Definition(_)) => Ok(Default::default()),
        None => bail!("{id} holds state the token program does not write"),
    }
}

/// The deployed token program, for proving private executions.
async fn catalog(wallet: &WalletCore<SequencerClient>, program: AccountId) -> Result<ProgramCatalog> {
    let deployed = ProgramLoader(wallet)
        .program(program)
        .await?
        .ok_or_else(|| anyhow!("{program} is not a deployed program"))?;
    Ok(ProgramCatalog::from([(program, deployed)]))
}

/// Sends `message` to the actor `accounts[0]`, publicly or with a proof made here.
async fn send(
    wallet: &mut WalletCore<SequencerClient>,
    program: AccountId,
    accounts: Vec<wallet::AccountMention>,
    message: &Message,
    delivery: AsyncDelivery,
) -> Result<()> {
    let private = accounts.iter().any(|mention| mention.identity.is_private())
        || !delivery.recoveries.is_empty()
        || !delivery.seals.is_empty();
    let catalog = catalog(wallet, program).await?;
    if private {
        println!("privacy-preserving: proving on this computer");
    }
    let started = std::time::Instant::now();
    let (hash, _) = wallet
        .send_tx_with_pre_check(accounts, 0, borsh::to_vec(message)?, &catalog, None, delivery, None, |_| Ok(()))
        .await?;
    if private {
        println!("proved and submitted in {:.0?}", started.elapsed());
    }
    finish(wallet, hash).await
}

/// Waits for inclusion, refuses a reverted transaction, and records a private one's notes
/// (as the Names CLI's `finish` does, cli/src/main.rs).
async fn finish(wallet: &mut WalletCore<SequencerClient>, hash: HashType) -> Result<()> {
    println!("submitted {hash}");
    let (tx, block_id) = wallet.poll_transaction(hash).await.context("not included")?;
    let block = wallet
        .get_block(block_id)
        .await?
        .ok_or_else(|| anyhow!("block {block_id} is missing"))?;
    let reverted = block
        .body
        .transactions
        .iter()
        .position(|included| included.hash() == hash)
        .is_some_and(|position| block.body.is_reverted(position));
    if let LeeTransaction::PrivacyPreserving(tx) = &tx {
        let key_chain = wallet.storage_mut().key_chain_mut();
        let mut index = key_chain.build_latest_nullifier_index();
        key_chain.follow_transaction(&tx.message, reverted, false, &mut index);
        wallet.store_persistent_data()?;
    }
    if reverted {
        bail!("{hash} was included in block {block_id} but reverted");
    }
    println!("applied in block {block_id}");
    Ok(())
}

fn hex(words: &[u32; 8]) -> String {
    words.iter().flat_map(|w| w.to_le_bytes()).map(|b| format!("{b:02x}")).collect()
}

fn hex_bytes(text: &str) -> Result<Vec<u8>> {
    let text = text.trim().trim_start_matches("0x");
    if text.len() % 2 != 0 {
        bail!("odd-length hex");
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).map_err(|e| anyhow!("bad hex: {e}")))
        .collect()
}

fn hex_array(text: &str) -> Result<[u8; 32]> {
    hex_bytes(text)?.try_into().map_err(|_| anyhow!("expected 32 bytes of hex"))
}
