//! NFTs of Logos's reference NFT program: `names nft …` and the module's `names module nft …`.
//!
//! The program is `logos-blockchain/lez-programs` `programs/nft` (branch `refactor/lez-v0.4.x`,
//! dea23c2); its wire format is the shared `nft_wire.rs` (one file with the Basecamp module).
//! Every deployment is a different program, so its account ID is never built in: it comes from
//! `--nft-program` (`NAMES_NFT_PROGRAM`), which the module always passes. `docs/wallet/NFT.md`
//! has the plan; the account declarations follow the Job A client `nft-drive` (one per
//! message), the sending follows `tokens.rs`.
//!
//! Two rules beyond the token path:
//! - A proven transaction (any private account, or a public sender to someone's private keys)
//!   never lets the LEZ wallet pick its private fee payer: Job A saw its automatic pick named in
//!   the clear. `--fee-payer` is required for one, and checked against the reserve first.
//! - A recipient this wallet can read (a public account, one of its own private accounts) is
//!   checked to be a holding of the collection that holds nothing before anything is sent.
//!   Someone else's private account can't be read: sending to its keys needs `--their-keys`.

use anyhow::{Context as _, Result, anyhow, bail};
use clap::Subcommand;
use common::HashType;
use lee::privacy_preserving_transaction::circuit::ProgramCatalog;
use lee_core::{
    NullifierPublicKey, PrivateAccountKind, account::AccountId, encryption::ViewingPublicKey,
};
use wallet::{AccountIdentity, AccountMention, AsyncDelivery, WalletSequencerClient as _};

use crate::nft_wire::{self, Holding, Message, Readiness, To as Reach};
use crate::{Client, gas_limit_for, parse_account, parse_name};

/// The NFT program in use; never built in.
pub fn program(given: Option<AccountId>) -> Result<AccountId> {
    given.ok_or_else(|| {
        anyhow!("no NFT program given: pass --nft-program <account ID> or set NAMES_NFT_PROGRAM (every deployment of the NFT program is a different program)")
    })
}

fn id(account: AccountId) -> nft_wire::Id {
    *account.value()
}

fn account(id: nft_wire::Id) -> AccountId {
    AccountId::new(id)
}

/// Where an NFT goes, as typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum To {
    /// An LNS name: its public account (NFT.md §7).
    Name(String),
    Public(AccountId),
    /// One of this wallet's private accounts.
    Private(AccountId),
    /// Someone's private account by its receiving keys.
    Keys { npk: [u8; 32], vpk: Vec<u8> },
}

pub fn parse_to(input: &str) -> Result<To, String> {
    let input = input.trim();
    if let Some(rest) = input.strip_prefix("Public/") {
        return parse_account(rest).map(To::Public);
    }
    if let Some(rest) = input.strip_prefix("Private/") {
        return parse_account(rest).map(To::Private);
    }
    if let Some((npk, vpk)) = input.split_once(':') {
        let npk = hex::decode(npk).ok().and_then(|bytes| <[u8; 32]>::try_from(bytes).ok());
        if let (Some(npk), Ok(vpk)) = (npk, hex::decode(vpk)) {
            if !vpk.is_empty() {
                return Ok(To::Keys { npk, vpk });
            }
        }
        return Err("receiving keys are <npk hex>:<vpk hex>".to_owned());
    }
    if input.parse::<AccountId>().is_ok() {
        return Err(format!("write Public/{input} (or Private/{input} for one of this wallet's private accounts)"));
    }
    parse_name(input).map(To::Name).map_err(|error| format!("{input} is not an LNS name, Public/<id>, Private/<id> or <npk hex>:<vpk hex> ({error})"))
}

/// A new account's kind, for `init --new`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Kind {
    Public,
    Private,
}

#[derive(Subcommand)]
pub enum NftCommand {
    /// What this wallet's accounts hold under the NFT program, and the NFT credits waiting
    /// for its private accounts.
    List {
        #[arg(long)]
        json: bool,
    },
    /// A collection (a definition): its name, supply and metadata.
    Info {
        #[arg(value_parser = parse_account)]
        definition: AccountId,
        #[arg(long)]
        json: bool,
    },
    /// Make an account ready to receive an NFT of a collection (InitializeHolding). An
    /// account takes NFTs of one collection only, under each NFT program.
    Init {
        #[arg(long, value_parser = parse_account)]
        definition: AccountId,
        /// An account of this wallet with nothing under this program yet.
        #[arg(long, value_parser = parse_account, conflicts_with = "new")]
        account: Option<AccountId>,
        /// Make a new account for it.
        #[arg(long, value_enum)]
        new: Option<Kind>,
        /// The private account that pays a proven transaction's fee.
        #[arg(long, value_parser = parse_account)]
        fee_payer: Option<AccountId>,
    },
    /// Send the NFT `--from` holds (a copy, or the master with all its print rights).
    Send {
        #[arg(long, value_parser = parse_account)]
        from: AccountId,
        /// An LNS name (its public account), `Public/<id>`, one of this wallet's
        /// `Private/<id>`, or someone's receiving keys `<npk hex>:<vpk hex>`.
        #[arg(long, value_parser = parse_to)]
        to: To,
        /// Allow sending to receiving keys this wallet can't check (NFT.md §7).
        #[arg(long)]
        their_keys: bool,
        #[arg(long, value_parser = parse_account)]
        fee_payer: Option<AccountId>,
    },
    /// Destroy what `--from` holds: a copy, or the master and all its print rights.
    Burn {
        #[arg(long, value_parser = parse_account)]
        from: AccountId,
        #[arg(long, value_parser = parse_account)]
        fee_payer: Option<AccountId>,
    },
    /// Print one copy from the master `--from` to a ready holding.
    Print {
        #[arg(long, value_parser = parse_account)]
        from: AccountId,
        #[arg(long, value_parser = parse_to)]
        to: To,
        #[arg(long)]
        their_keys: bool,
        #[arg(long, value_parser = parse_account)]
        fee_payer: Option<AccountId>,
    },
    /// Take the NFT credits waiting for this wallet's private accounts, one proven
    /// transaction each, paid by `--fee-payer`.
    Receive {
        #[arg(long, value_parser = parse_account)]
        fee_payer: AccountId,
    },
    /// Create a collection, for tests and demos: two new public accounts of this wallet
    /// become its definition and its master; `--payer` (public) pays.
    Create {
        name: String,
        supply: u128,
        #[arg(long, default_value = "")]
        uri: String,
        #[arg(long, default_value = "")]
        creators: String,
        #[arg(long)]
        expanded: bool,
        #[arg(long, value_parser = parse_account)]
        payer: Option<AccountId>,
    },
}

/// The module's NFT requests; each prints its answer as JSON on its last line.
#[derive(Subcommand)]
pub enum NftRequest {
    /// `{holdings: [...], pending: [...]}`, as `list --json`.
    Holdings,
    /// `{definition, name, printable_supply, standard, uri, creators}`.
    Info { definition: String },
    /// The sheet for the action given as JSON on standard input, without sending anything:
    /// `{action, from, to?, definition?, new?, fee_payer?, their_keys?}`.
    Plan,
}

/// An action, as the module and the app describe it (the same JSON for a plan and a job).
#[derive(serde::Deserialize, Debug, Clone)]
pub struct Action {
    pub action: String,
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
    #[serde(default)]
    pub definition: Option<String>,
    #[serde(default)]
    pub new: Option<String>,
    #[serde(default)]
    pub fee_payer: Option<String>,
    #[serde(default)]
    pub their_keys: bool,
}

fn text(private: bool, account: AccountId) -> String {
    format!("{}/{account}", if private { "Private" } else { "Public" })
}

/// One account's state as `list --json` writes it.
fn holding_row(private: bool, account: AccountId, state: &[u8], collection: Option<serde_json::Value>) -> serde_json::Value {
    let h = nft_wire::holding(state);
    let (kind, why) = match &h {
        Holding::Empty => ("empty", None),
        Holding::Definition(_) => ("definition", None),
        Holding::Master { .. } => ("master", None),
        Holding::Copy { .. } => ("copy", None),
        Holding::Unreadable(why) => ("unreadable", Some(*why)),
    };
    serde_json::json!({
        "account": text(private, account),
        "private": private,
        "state": kind,
        "holds": h.holds(),
        "definition": h.definition().map(|d| self::account(d).to_string()),
        "print_balance": match h { Holding::Master { print_balance, .. } => Some(print_balance.to_string()), _ => None },
        "owned": match h { Holding::Copy { owned, .. } => Some(owned), _ => None },
        "unreadable": why,
        "collection": collection,
    })
}

fn collection_json(definition: AccountId, found: &nft_wire::Definition) -> serde_json::Value {
    serde_json::json!({
        "definition": definition.to_string(),
        "name": found.name,
        "printable_supply": found.printable_supply.to_string(),
        "standard": found.metadata.standard.word(),
        "uri": found.metadata.uri,
        "creators": found.metadata.creators,
    })
}

/// The plain words for a failure: a known refusal of the program or LEZ, else the error.
fn plain(error: anyhow::Error) -> anyhow::Error {
    let whole = format!("{error:#}");
    match nft_wire::refusal(&whole) {
        Some(sentence) => anyhow!("{sentence} ({})", whole.lines().last().unwrap_or_default()),
        None => error,
    }
}

impl Client {
    pub async fn nft(&mut self, program: AccountId, command: NftCommand) -> Result<()> {
        match command {
            NftCommand::List { json } => {
                let listed = self.nft_list(program).await?;
                if json {
                    println!("{listed}");
                    return Ok(());
                }
                for row in listed["holdings"].as_array().into_iter().flatten() {
                    if row["state"] == "empty" {
                        continue;
                    }
                    let name = row["collection"]["name"].as_str().unwrap_or("(collection unreadable)");
                    let what = match (row["state"].as_str(), row["holds"].as_bool()) {
                        (Some("master"), Some(true)) => format!("master, {} print rights", row["print_balance"].as_str().unwrap_or("?")),
                        (Some("copy"), Some(true)) => "copy".to_owned(),
                        (Some("definition"), _) => "the definition".to_owned(),
                        (Some("unreadable"), _) => "unreadable state".to_owned(),
                        _ => "ready, holds nothing".to_owned(),
                    };
                    println!("{}  {name}  {what}  {}", row["account"].as_str().unwrap_or_default(), row["definition"].as_str().unwrap_or_default());
                }
                for row in listed["pending"].as_array().into_iter().flatten() {
                    println!("waiting (`names nft receive`): {} to {}", row["definition"].as_str().unwrap_or_default(), row["to"].as_str().unwrap_or_default());
                }
                Ok(())
            }
            NftCommand::Info { definition, json } => {
                let found = self.nft_definition(program, definition).await?;
                if json {
                    println!("{}", collection_json(definition, &found));
                } else {
                    println!("name      {}\nsupply    {}\nstandard  {}\nuri       {}\ncreators  {}", found.name, found.printable_supply, found.metadata.standard.word(), found.metadata.uri, found.metadata.creators);
                }
                Ok(())
            }
            NftCommand::Init { definition, account, new, fee_payer } => {
                let holder = match (account, new) {
                    (Some(account), _) => account,
                    (None, Some(Kind::Public)) => self.wallet.create_new_account_public(None).0,
                    (None, Some(Kind::Private)) => self.wallet.create_new_account_private(None).0,
                    (None, None) => bail!("give --account <id> or --new public|private"),
                };
                if new.is_some() {
                    crate::save_wallet(&mut self.wallet)?;
                    println!("new account {}", text(new == Some(Kind::Private), holder));
                }
                self.nft_init(program, definition, holder, fee_payer).await.map_err(plain).map(drop)
            }
            NftCommand::Send { from, to, their_keys, fee_payer } => {
                self.nft_move(program, false, from, &to, their_keys, fee_payer).await.map_err(plain).map(drop)
            }
            NftCommand::Print { from, to, their_keys, fee_payer } => {
                self.nft_move(program, true, from, &to, their_keys, fee_payer).await.map_err(plain).map(drop)
            }
            NftCommand::Burn { from, fee_payer } => self.nft_burn(program, from, fee_payer).await.map_err(plain).map(drop),
            NftCommand::Receive { fee_payer } => self.nft_receive(program, fee_payer).await.map_err(plain).map(drop),
            NftCommand::Create { name, supply, uri, creators, expanded, payer } => {
                self.nft_create(program, name, supply, uri, creators, expanded, payer).await
            }
        }
    }

    pub async fn module_nft(&mut self, program: AccountId, request: NftRequest) -> Result<()> {
        match request {
            NftRequest::Holdings => println!("{}", self.nft_list(program).await?),
            NftRequest::Info { definition } => {
                let definition = parse_account(&definition).map_err(|error| anyhow!(error))?;
                let found = self.nft_definition(program, definition).await?;
                println!("{}", collection_json(definition, &found));
            }
            NftRequest::Plan => {
                let mut input = Vec::new();
                std::io::Read::read_to_end(&mut std::io::stdin(), &mut input).context("cannot read the request")?;
                let action: Action = serde_json::from_slice(&input).context("not an NFT action")?;
                println!("{}", self.nft_plan(program, &action).await?);
            }
        }
        Ok(())
    }

    /// The program's state on `account`: a private account's from this wallet's notes (after
    /// a sync), a public one's from the chain.
    async fn nft_state(&self, program: AccountId, account: AccountId) -> Result<Vec<u8>> {
        if self.is_private(account) {
            return Ok(self.wallet.get_account_private(account).map(|found| found.data.actor_state(program).to_vec()).unwrap_or_default());
        }
        self.actor_state_of(account, program).await
    }

    pub async fn nft_definition(&self, program: AccountId, definition: AccountId) -> Result<nft_wire::Definition> {
        let state = self.actor_state_of(definition, program).await?;
        match nft_wire::holding(&state) {
            Holding::Definition(found) => Ok(found),
            Holding::Unreadable(why) => bail!("{definition}'s state under the NFT program {program} can't be read: {why}"),
            _ => bail!("{definition} is not a collection of the NFT program {program}"),
        }
    }

    async fn nft_list(&mut self, program: AccountId) -> Result<serde_json::Value> {
        self.wallet.sync_to_latest_block().await.context("cannot sync the private accounts")?;
        let key_chain = self.wallet.storage().key_chain();
        let accounts: Vec<(bool, AccountId)> = key_chain
            .public_account_ids()
            .map(|(id, _)| (false, id))
            .chain(key_chain.private_account_ids().map(|(id, _)| (true, id)))
            .collect();
        let mut collections = std::collections::BTreeMap::new();
        let mut rows = Vec::new();
        for (private, holder) in accounts {
            let state = self.nft_state(program, holder).await?;
            let collection = match nft_wire::holding(&state).definition() {
                Some(definition) => {
                    let definition = account(definition);
                    if !collections.contains_key(&definition) {
                        let found = self.nft_definition(program, definition).await.ok().map(|found| collection_json(definition, &found));
                        collections.insert(definition, found);
                    }
                    collections[&definition].clone()
                }
                None => None,
            };
            rows.push(holding_row(private, holder, &state, collection));
        }
        let pending = self.wallet.owned_pending_messages().await.context("cannot read pending messages")?;
        let pending: Vec<serde_json::Value> = pending
            .iter()
            .filter(|message| message.body.to.program_account_id == program)
            .map(|message| {
                let credit = nft_wire::credit_of(&message.body.message);
                serde_json::json!({
                    "position": message.position,
                    "definition": credit.map(|(d, _)| account(d).to_string()),
                    "value": credit.map(|(_, v)| match v { nft_wire::Value::Copy => "copy".to_owned(), nft_wire::Value::Master(n) => format!("master:{n}") }),
                    "to": format!("Private/{}", message.body.to.account_id),
                })
            })
            .collect();
        Ok(serde_json::json!({ "program": program.to_string(), "holdings": rows, "pending": pending }))
    }

    /// How `account` takes part as a signer: a private account of this wallet, or a public
    /// one whose key it holds.
    fn nft_signer(&self, account: AccountId) -> Result<AccountIdentity> {
        if let Some(private) = self.wallet.resolve_private_account(account) {
            return Ok(private);
        }
        if self.wallet.get_account_public_signing_key(account).is_some() {
            return Ok(AccountIdentity::Public(account));
        }
        bail!("this wallet does not hold the key of {account}")
    }

    /// A recipient: how it takes part, how it's shown, and what kind of reach it is.
    async fn nft_recipient(&self, to: &To) -> Result<(AccountIdentity, String, Reach)> {
        Ok(match to {
            To::Name(name) => {
                let identity = self.payee(name, true, false).await?;
                if identity.is_private() {
                    bail!("{name} publishes no public account; send to its receiving keys only if you are sure it is ready (NFT.md §7)");
                }
                let shown = format!("{name} (Public/{})", identity.account_id());
                (identity, shown, Reach::Public)
            }
            To::Public(account) => {
                let identity = if self.wallet.get_account_public_signing_key(*account).is_some() { AccountIdentity::Public(*account) } else { AccountIdentity::PublicNoSign(*account) };
                (identity, format!("Public/{account}"), Reach::Public)
            }
            To::Private(account) => {
                let identity = self.wallet.resolve_private_account(*account).ok_or_else(|| {
                    anyhow!("Private/{account} is not this wallet's; send to someone else's private account by its keys, <npk hex>:<vpk hex>")
                })?;
                (identity, format!("Private/{account}"), Reach::OwnPrivate)
            }
            To::Keys { npk, vpk } => {
                let vpk = ViewingPublicKey::from_bytes(vpk.clone()).map_err(|error| anyhow!("the viewing key is not valid: {error}"))?;
                let identity = AccountIdentity::PrivateForeign { npk: NullifierPublicKey(*npk), vpk, kind: PrivateAccountKind::Regular };
                (identity, "those private receiving keys".to_owned(), Reach::TheirPrivate)
            }
        })
    }

    /// Checks a recipient this wallet can read: a holding of `definition` that holds nothing.
    async fn nft_check_recipient(&self, program: AccountId, recipient: &AccountIdentity, reach: Reach, definition: nft_wire::Id) -> Result<Readiness> {
        if reach == Reach::TheirPrivate {
            return Ok(Readiness::Ready);
        }
        let state = self.nft_state(program, recipient.account_id()).await?;
        Ok(nft_wire::readiness(&state, &definition))
    }

    async fn nft_init(&mut self, program: AccountId, definition: AccountId, holder: AccountId, fee_payer: Option<AccountId>) -> Result<(HashType, u64)> {
        let signer = self.nft_signer(holder)?;
        if signer.is_private() {
            self.wallet.sync_to_latest_block().await.context("cannot sync the private accounts")?;
        }
        let collection = self.nft_definition(program, definition).await?;
        match nft_wire::readiness(&self.nft_state(program, holder).await?, &id(definition)) {
            Readiness::NeedsHolding => {}
            Readiness::Ready => bail!("{} is already ready for {}: nothing to do", text(signer.is_private(), holder), collection.name),
            Readiness::Refused(why) => bail!("{why}"),
        }
        println!("making {} ready for {} ({definition})", text(signer.is_private(), holder), collection.name);
        let mentions = vec![
            signer.select_program_actor_state(program),
            AccountIdentity::PublicNoSign(definition).select_program_actor_state(program),
        ];
        self.nft_call(program, mentions, Message::InitializeHolding { definition: id(definition) }, AsyncDelivery::default(), fee_payer).await
    }

    async fn nft_move(&mut self, program: AccountId, print: bool, from: AccountId, to: &To, their_keys: bool, fee_payer: Option<AccountId>) -> Result<(HashType, u64)> {
        let sender = self.nft_signer(from)?;
        if sender.is_private() {
            self.wallet.sync_to_latest_block().await.context("cannot sync the private accounts")?;
        }
        let state = self.nft_state(program, from).await?;
        let definition = if print {
            nft_wire::can_print(&state).map_err(|why| anyhow!(why))?
        } else {
            nft_wire::can_send(&state).map_err(|why| anyhow!(why))?.0
        };
        let (recipient, shown, reach) = self.nft_recipient(to).await?;
        if recipient.account_id() == from {
            bail!("{} would be sending to itself", text(sender.is_private(), from));
        }
        if reach == Reach::TheirPrivate && !their_keys {
            bail!("this wallet can't check that those receiving keys' account is ready for this collection; if it isn't, the NFT may never arrive. Pass --their-keys to send anyway");
        }
        match self.nft_check_recipient(program, &recipient, reach, definition).await? {
            Readiness::Ready => {}
            Readiness::NeedsHolding => bail!(nft_wire::NOT_READY),
            Readiness::Refused(why) => bail!(why),
        }
        let collection = self.nft_definition(program, account(definition)).await?;
        let (named, hidden) = nft_wire::reveals(sender.is_private(), reach);
        println!("{} {} of {} from {} to {shown}", if print { "printing a copy" } else { "sending" }, if print { "" } else { "the NFT" }, collection.name, text(sender.is_private(), from));
        for line in named.iter().chain(hidden.iter()) {
            println!("  {line}");
        }
        let to_id = id(recipient.account_id());
        let message = if print { Message::PrintNft { to: to_id } } else { Message::Transfer { to: to_id } };
        let mut delivery = AsyncDelivery::default();
        let mut mentions = vec![sender.clone().select_program_actor_state(program)];
        match reach {
            // Declared, so the credit runs in this transaction and a wrong recipient rolls it back.
            Reach::Public | Reach::OwnPrivate => mentions.push(recipient.select_program_actor_state(program).receiving()),
            // Not declared: sealed to it (private sender) or bound to a recovery note (public
            // sender), as the LEZ wallet does for native credits (`credit_destination`).
            Reach::TheirPrivate => {
                let mention = recipient.select_program_actor_state(program);
                let (_, mut pending) = if sender.is_private() { self.wallet.seal_destination(mention) } else { self.wallet.async_destination(mention) }
                    .map_err(|error| anyhow!("cannot address the private recipient: {error}"))?;
                pending.promotions.listed_only = true;
                delivery = pending;
            }
        }
        self.nft_call(program, mentions, message, delivery, fee_payer).await
    }

    async fn nft_burn(&mut self, program: AccountId, from: AccountId, fee_payer: Option<AccountId>) -> Result<(HashType, u64)> {
        let holder = self.nft_signer(from)?;
        if holder.is_private() {
            self.wallet.sync_to_latest_block().await.context("cannot sync the private accounts")?;
        }
        let state = self.nft_state(program, from).await?;
        let amount = nft_wire::burn_all(&state).map_err(|why| anyhow!(why))?;
        let definition = account(nft_wire::holding(&state).definition().expect("a holding that holds has a definition"));
        let collection = self.nft_definition(program, definition).await?;
        println!("burning {} of {} at {}", if amount == 1 && nft_wire::holding(&state).kind() == Some("copy") { "the copy".to_owned() } else { format!("the master and its {amount} print rights") }, collection.name, text(holder.is_private(), from));
        // The definition is reached through state: declared, or the program refuses (Job A §8).
        let mentions = vec![
            holder.select_program_actor_state(program),
            AccountIdentity::PublicNoSign(definition).select_program_actor_state(program),
        ];
        self.nft_call(program, mentions, Message::Burn { amount }, AsyncDelivery::default(), fee_payer).await
    }

    async fn nft_receive(&mut self, program: AccountId, fee_payer: AccountId) -> Result<Vec<(HashType, u64)>> {
        let pending: Vec<_> = self
            .wallet
            .owned_pending_messages()
            .await
            .context("cannot read pending messages")?
            .into_iter()
            .filter(|message| message.body.to.program_account_id == program && nft_wire::credit_of(&message.body.message).is_some())
            .collect();
        if pending.is_empty() {
            println!("nothing to receive");
            return Ok(Vec::new());
        }
        self.nft_check_payer(fee_payer).await?;
        let catalog = self.nft_catalog(program).await?;
        let mut fee = wallet::FeeParams::default();
        fee.payer = Some(fee_payer);
        self.wallet.set_fee_params(fee);
        let mut received = Vec::new();
        for message in pending {
            let to = message.body.to.account_id;
            // A credit lands only on a holding of its collection that holds nothing; one that
            // can't land is left waiting rather than proven to fail.
            if let Some((definition, _)) = nft_wire::credit_of(&message.body.message) {
                let why = match nft_wire::readiness(&self.nft_state(program, to).await?, &definition) {
                    Readiness::Ready => None,
                    Readiness::NeedsHolding => Some(nft_wire::NOT_READY),
                    Readiness::Refused(why) => Some(why),
                };
                if let Some(why) = why {
                    println!("left waiting, the NFT for Private/{to}: {why}");
                    continue;
                }
            }
            println!("receiving an NFT at Private/{to}; proving it on this computer (several minutes)");
            let (hash, _) = self
                .wallet
                .receive_pending_message_with(message, Vec::new(), &catalog, None, AsyncDelivery::default())
                .await
                .context("the sequencer did not accept the receipt")?;
            println!("submitted {hash}");
            let block = self.finish(hash).await.context("the receipt was not applied")?;
            println!("applied in block {block}");
            received.push((hash, block));
        }
        Ok(received)
    }

    #[allow(clippy::too_many_arguments)]
    async fn nft_create(&mut self, program: AccountId, name: String, supply: u128, uri: String, creators: String, expanded: bool, payer: Option<AccountId>) -> Result<()> {
        if name.is_empty() || name.len() > nft_wire::MAX_NAME_LEN || uri.len() > nft_wire::MAX_URI_LEN || creators.len() > nft_wire::MAX_CREATORS_LEN || supply == 0 {
            bail!("a collection takes a name of 1-64 bytes, a uri and creators of at most 256 bytes each, and a supply of at least 1");
        }
        let payer = payer.ok_or_else(|| anyhow!("give --payer, a public account of this wallet"))?;
        let (definition, _) = self.wallet.create_new_account_public(None);
        let (master, _) = self.wallet.create_new_account_public(None);
        crate::save_wallet(&mut self.wallet)?;
        println!("creating {name} at {definition}, master {master}");
        let metadata = nft_wire::Metadata { standard: if expanded { nft_wire::Standard::Expanded } else { nft_wire::Standard::Simple }, uri, creators };
        let message = Message::NewDefinition { name, printable_supply: supply, metadata, master: id(master) };
        let mentions = vec![
            AccountIdentity::Public(definition).select_program_actor_state(program),
            AccountIdentity::Public(master).select_program_actor_state(program),
        ];
        self.nft_call(program, mentions, message, AsyncDelivery::default(), Some(payer)).await?;
        println!("collection {definition}\nmaster {master}");
        Ok(())
    }

    /// The NFT program's bytecode, for proving and for trying a public call first.
    async fn nft_catalog(&self, program: AccountId) -> Result<ProgramCatalog> {
        let deployed = self.program_at(program, "NAMES_NFT_PROGRAM_ELF", "names-nft", "the NFT program").await?;
        Ok(ProgramCatalog::from([(program, deployed)]))
    }

    /// The private fee reserve now, from the chain's fee state (`fees::private`).
    async fn nft_reserve(&self) -> Result<crate::fees::Estimate> {
        let quote = self.wallet.client().get_fee_state().await?;
        Ok(crate::fees::private(&quote, self.wallet.config().fee_headroom_blocks))
    }

    /// A private fee payer: one of this wallet's private accounts holding the reserve.
    async fn nft_check_payer(&mut self, payer: AccountId) -> Result<crate::fees::Estimate> {
        if !self.is_private(payer) {
            bail!("the fee payer of a private transaction must be one of this wallet's private accounts (a public payer fails for a proven transaction)");
        }
        let reserve = self.nft_reserve().await?;
        let held = self.spendable(payer).await?;
        if held < reserve.at_most {
            bail!("Private/{payer} holds {held}; a private transaction reserves up to {} for its fee", reserve.at_most);
        }
        Ok(reserve)
    }

    /// Sends one NFT message: tried first and sized when every account is public, proven here
    /// otherwise, with the private fee payer given (never the wallet's own pick).
    async fn nft_call(&mut self, program: AccountId, mentions: Vec<AccountMention>, message: Message, delivery: AsyncDelivery, fee_payer: Option<AccountId>) -> Result<(HashType, u64)> {
        let bytes = message.encode();
        let proven = mentions.iter().any(|mention| mention.identity.is_private()) || !delivery.recoveries.is_empty() || !delivery.seals.is_empty();
        let hash = if proven {
            crate::prover::to_prove()?;
            let payer = fee_payer.ok_or_else(|| anyhow!("a private transaction needs --fee-payer: one of this wallet's private accounts that holds the fee reserve (the wallet won't pick one, because its pick can be named on chain)"))?;
            let reserve = self.nft_check_payer(payer).await?;
            println!("{}", crate::fees::estimate_line(&reserve, "paid inside the proof"));
            if std::env::var_os("LOGOS_NAMES_DRY_RUN").is_some() {
                bail!("LOGOS_NAMES_DRY_RUN is set, so nothing was sent (a private change is not tried first)");
            }
            let catalog = self.nft_catalog(program).await?;
            println!("proving it on this computer (several minutes), so the chain never learns the private side");
            let started = std::time::Instant::now();
            let (hash, _) = self
                .wallet
                .send_tx_with_pre_check(mentions, 0, bytes, &catalog, None, delivery, Some(payer), |_| Ok(()))
                .await
                .context("the sequencer did not accept the transaction")?;
            println!("proved in {}s", started.elapsed().as_secs());
            hash
        } else {
            let deployed = self.program_at(program, "NAMES_NFT_PROGRAM_ELF", "names-nft", "the NFT program").await?;
            let cycles = self.simulate(&mentions, &bytes, vec![(program, deployed)]).await?;
            let limit = gas_limit_for(cycles);
            let paying: Vec<AccountMention> = match fee_payer {
                Some(payer) => vec![AccountIdentity::Public(payer).balance()],
                None => mentions.clone(),
            };
            self.check_payer(&paying, bytes.len(), 0, limit, cycles).await?;
            if std::env::var_os("LOGOS_NAMES_DRY_RUN").is_some() {
                bail!("LOGOS_NAMES_DRY_RUN is set, so nothing was sent");
            }
            let saved = self.wallet.config().clone();
            let mut config = saved.clone();
            config.gas_limit = limit;
            self.wallet.set_config(config);
            let sent = self.wallet.send_pub_tx_paid_by(mentions, 0, bytes, fee_payer).await;
            self.wallet.set_config(saved);
            sent.context("the sequencer did not accept the transaction")?
        };
        println!("submitted {hash}");
        let block = self.finish(hash).await.context("the transaction was not applied")?;
        println!("applied in block {block}");
        Ok((hash, block))
    }

    /// The sheet for `action`: what it does, what becomes public, whether it can go ahead, and
    /// which private accounts can pay a proven one. Sends nothing.
    pub async fn nft_plan(&mut self, program: AccountId, action: &Action) -> Result<serde_json::Value> {
        self.wallet.sync_to_latest_block().await.context("cannot sync the private accounts")?;
        let from = action.from.as_deref().map(|text| parse_account(text.trim_start_matches("Public/").trim_start_matches("Private/"))).transpose().map_err(|error| anyhow!(error))?;
        let mut refusal: Option<String> = None;
        let mut proven = false;
        let mut lines: Vec<String> = Vec::new();
        let mut named: Vec<&str> = Vec::new();
        let mut hidden: Vec<&str> = Vec::new();
        let mut collection = None;
        let from_private = from.is_some_and(|from| self.is_private(from));
        match action.action.as_str() {
            "init" => {
                let definition = action.definition.as_deref().map(parse_account).transpose().map_err(|error| anyhow!(error))?.ok_or_else(|| anyhow!("init needs a definition"))?;
                collection = Some(self.nft_definition(program, definition).await.map(|found| collection_json(definition, &found)));
                let new_private = action.new.as_deref() == Some("private");
                proven = from_private || new_private;
                if let Some(from) = from {
                    refusal = match nft_wire::readiness(&self.nft_state(program, from).await?, &id(definition)) {
                        Readiness::NeedsHolding => None,
                        Readiness::Ready => Some("That account is already ready for this collection.".to_owned()),
                        Readiness::Refused(why) => Some(why.to_owned()),
                    };
                }
                lines.push("An account takes NFTs of one collection, after it says so in a small transaction.".to_owned());
                if proven {
                    named.push("The collection is public; the private account is not named.");
                } else {
                    named.push("The account and the collection are public.");
                }
            }
            "send" | "print" => {
                let from = from.ok_or_else(|| anyhow!("{} needs from", action.action))?;
                let state = self.nft_state(program, from).await?;
                let definition = if action.action == "print" { nft_wire::can_print(&state) } else { nft_wire::can_send(&state).map(|(d, _)| d) };
                let to = parse_to(action.to.as_deref().unwrap_or_default()).map_err(|error| anyhow!(error))?;
                let (recipient, shown, reach) = self.nft_recipient(&to).await?;
                proven = from_private || reach != Reach::Public;
                (named, hidden) = nft_wire::reveals(from_private, reach);
                // The asset leaving, as the prize's prompts must name it: there is no serial.
                match (action.action.as_str(), nft_wire::can_send(&state)) {
                    ("print", _) => lines.push("A new copy, printed by this master; the master keeps the rest.".to_owned()),
                    (_, Ok((_, nft_wire::Value::Copy))) => lines.push("Leaving: a copy of this collection.".to_owned()),
                    (_, Ok((_, nft_wire::Value::Master(rights)))) => lines.push(format!("Leaving: the master, with all {rights} print rights.")),
                    (_, Err(_)) => {}
                }
                lines.push(format!("To {shown}"));
                match definition {
                    Err(why) => refusal = Some(why.to_owned()),
                    Ok(definition) => {
                        collection = Some(self.nft_definition(program, account(definition)).await.map(|found| collection_json(account(definition), &found)));
                        match self.nft_check_recipient(program, &recipient, reach, definition).await? {
                            Readiness::Ready => {}
                            Readiness::NeedsHolding => refusal = Some(nft_wire::NOT_READY.to_owned()),
                            Readiness::Refused(why) => refusal = Some(why.to_owned()),
                        }
                        if reach == Reach::TheirPrivate {
                            lines.push("This wallet can't check that their account is ready for this collection. If it isn't, the NFT may never arrive.".to_owned());
                        }
                    }
                }
            }
            "burn" => {
                let from = from.ok_or_else(|| anyhow!("burn needs from"))?;
                let state = self.nft_state(program, from).await?;
                proven = from_private;
                match nft_wire::burn_all(&state) {
                    Err(why) => refusal = Some(why.to_owned()),
                    Ok(amount) => {
                        let definition = account(nft_wire::holding(&state).definition().expect("it holds"));
                        collection = Some(self.nft_definition(program, definition).await.map(|found| collection_json(definition, &found)));
                        lines.push(if nft_wire::holding(&state).kind() == Some("copy") { "Destroys this copy for good.".to_owned() } else { format!("Destroys the master and its {amount} print rights for good.") });
                    }
                }
                named.push(if proven { "The collection is public; the private account is not named." } else { "The account, the collection and the burn are public." });
            }
            other => bail!("no NFT action called {other}"),
        }
        let reserve = self.nft_reserve().await?;
        let mut payers = Vec::new();
        if proven {
            let privates: Vec<AccountId> = self.wallet.storage().key_chain().private_account_ids().map(|(id, _)| id).collect();
            for payer in privates {
                let held = self.spendable(payer).await.unwrap_or(0);
                payers.push(serde_json::json!({ "account": text(true, payer), "balance": held.to_string(), "enough": held >= reserve.at_most }));
            }
        }
        let collection = match collection {
            Some(Ok(found)) => Some(found),
            Some(Err(error)) => {
                refusal.get_or_insert_with(|| format!("{error:#}"));
                None
            }
            None => None,
        };
        Ok(serde_json::json!({
            "program": program.to_string(),
            "action": action.action,
            "from": from.map(|from| text(from_private, from)),
            "collection": collection,
            "lines": lines,
            "public": named,
            "private": hidden,
            "proven": proven,
            "fee": { "about": reserve.about.to_string(), "at_most": reserve.at_most.to_string(), "inside_proof": proven },
            "payers": payers,
            "refusal": refusal,
        }))
    }
}
