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
        /// The state digest the sheet was planned on (`names module nft plan`'s `pin`): refused
        /// before anything is sent if the accounts' state is no longer what the sheet showed.
        #[arg(long)]
        expect_state: Option<String>,
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
        /// The state digest the sheet was planned on (`names module nft plan`'s `pin`): refused
        /// before anything is sent if the accounts' state is no longer what the sheet showed.
        #[arg(long)]
        expect_state: Option<String>,
    },
    /// Destroy what `--from` holds: a copy, or the master and all its print rights.
    Burn {
        #[arg(long, value_parser = parse_account)]
        from: AccountId,
        #[arg(long, value_parser = parse_account)]
        fee_payer: Option<AccountId>,
        /// The state digest the sheet was planned on (`names module nft plan`'s `pin`): refused
        /// before anything is sent if the accounts' state is no longer what the sheet showed.
        #[arg(long)]
        expect_state: Option<String>,
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
        /// The state digest the sheet was planned on (`names module nft plan`'s `pin`): refused
        /// before anything is sent if the accounts' state is no longer what the sheet showed.
        #[arg(long)]
        expect_state: Option<String>,
    },
    /// Take the NFT credits waiting for this wallet's private accounts, one proven
    /// transaction each, paid by `--fee-payer`.
    Receive {
        #[arg(long, value_parser = parse_account)]
        fee_payer: AccountId,
        /// The state digest the sheet was planned on (`names module nft plan`'s `pin`): refused
        /// before anything is sent if the accounts' state is no longer what the sheet showed.
        #[arg(long)]
        expect_state: Option<String>,
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
    pub their_keys: bool,
}

/// What a sheet was planned on, as one digest: the program, the action and the state of each
/// account it touches then. The CLI computes the same again just before it sends
/// (`--expect-state`) and refuses on any difference (review finding: a stale sheet approved).
pub fn pin_of(program: AccountId, parts: &[(&str, &[u8])]) -> String {
    use sha2::Digest as _;
    let mut hasher = sha2::Sha256::new();
    hasher.update(b"names-nft-pin/v1");
    hasher.update(program.value());
    for (label, bytes) in parts {
        hasher.update((label.len() as u32).to_le_bytes());
        hasher.update(label.as_bytes());
        hasher.update((bytes.len() as u32).to_le_bytes());
        hasher.update(bytes);
    }
    hex::encode(hasher.finalize())
}

pub fn check_pin(expect: &Option<String>, pin: &str) -> Result<()> {
    match expect {
        Some(expected) if !expected.trim().eq_ignore_ascii_case(pin) => {
            bail!("Things changed since the sheet was made (an account's NFT state, or where the name points). Nothing was sent: open the sheet again.")
        }
        _ => Ok(()),
    }
}

/// The recipient as the sheet names it: `Public/<id>`, `Private/<id>`, or `<npk hex>:<vpk hex>`.
fn to_text(recipient: &AccountIdentity, reach: Reach) -> String {
    match (recipient, reach) {
        (AccountIdentity::PrivateForeign { npk, vpk, .. }, _) => format!("{}:{}", hex::encode(npk.0), hex::encode(vpk.to_bytes())),
        (_, Reach::OwnPrivate) => format!("Private/{}", recipient.account_id()),
        _ => format!("Public/{}", recipient.account_id()),
    }
}

/// Why a waiting credit of `definition` can't land on an account in `state`, as the module's code
/// and the plain sentence; `None` when it can.
pub fn left_reason(state: &[u8], definition: &nft_wire::Id) -> Option<(&'static str, &'static str)> {
    match nft_wire::readiness(state, definition) {
        Readiness::Ready => None,
        Readiness::NeedsHolding => Some(("not_ready", nft_wire::NOT_READY)),
        Readiness::Refused(why) if why == nft_wire::ALREADY_HOLDS => Some(("already_holds", why)),
        Readiness::Refused(why) => Some(("other_collection", why)),
    }
}

/// Whether a private payer holding `held` can pay: at least the fee at head (`about`), the least
/// the LEZ wallet reserves.
pub fn payer_covers(held: u128, reserve: &crate::fees::Estimate) -> bool {
    held >= reserve.about
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
            NftCommand::Init { definition, account, new, fee_payer, expect_state } => {
                // Everything that can refuse is checked before a new account is made, so a
                // refused try leaves no stray account behind.
                if let Some(kind) = new {
                    // A new account has no state yet: the sheet was planned on "a new one".
                    check_pin(&expect_state, &pin_of(program, &[("action", b"init".as_slice()), ("definition", definition.to_string().as_bytes()), ("holder", if kind == Kind::Private { b"new:private".as_slice() } else { b"new:public".as_slice() }), ("holder_state", b"".as_slice())]))?;
                    self.nft_definition(program, definition).await.map_err(plain)?;
                    match (new, fee_payer) {
                        (Some(Kind::Private), Some(payer)) => drop(self.nft_check_payer(payer).await?),
                        (Some(Kind::Private), None) => bail!("a new private account is made ready in a private transaction: give --fee-payer, one of this wallet's private accounts that holds the fee reserve"),
                        (Some(Kind::Public), Some(payer)) if self.is_private(payer) || self.wallet.get_account_public_signing_key(payer).is_none() => {
                            bail!("a public transaction's fee is paid by a public account of this wallet")
                        }
                        (Some(Kind::Public), None) => bail!("a new public account holds nothing to pay its fee with: give --fee-payer, a public account of this wallet with funds"),
                        _ => {}
                    }
                }
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
                let expect = if new.is_some() { None } else { expect_state };
                self.nft_init(program, definition, holder, fee_payer, expect).await.map_err(plain).map(drop)
            }
            NftCommand::Send { from, to, their_keys, fee_payer, expect_state } => {
                self.nft_move(program, false, from, &to, their_keys, fee_payer, expect_state).await.map_err(plain).map(drop)
            }
            NftCommand::Print { from, to, their_keys, fee_payer, expect_state } => {
                self.nft_move(program, true, from, &to, their_keys, fee_payer, expect_state).await.map_err(plain).map(drop)
            }
            NftCommand::Burn { from, fee_payer, expect_state } => self.nft_burn(program, from, fee_payer, expect_state).await.map_err(plain).map(drop),
            NftCommand::Receive { fee_payer, expect_state } => self.nft_receive(program, fee_payer, expect_state).await.map_err(plain).map(drop),
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

    async fn nft_init(&mut self, program: AccountId, definition: AccountId, holder: AccountId, fee_payer: Option<AccountId>, expect: Option<String>) -> Result<(HashType, u64)> {
        let signer = self.nft_signer(holder)?;
        if signer.is_private() {
            self.wallet.sync_to_latest_block().await.context("cannot sync the private accounts")?;
        }
        if expect.is_some() {
            let state = self.nft_state(program, holder).await?;
            check_pin(&expect, &pin_of(program, &[("action", b"init".as_slice()), ("definition", definition.to_string().as_bytes()), ("holder", text(signer.is_private(), holder).as_bytes()), ("holder_state", state.as_slice())]))?;
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

    #[allow(clippy::too_many_arguments)]
    async fn nft_move(&mut self, program: AccountId, print: bool, from: AccountId, to: &To, their_keys: bool, fee_payer: Option<AccountId>, expect: Option<String>) -> Result<(HashType, u64)> {
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
        let to_state = if reach == Reach::TheirPrivate { Vec::new() } else { self.nft_state(program, recipient.account_id()).await? };
        let action: &[u8] = if print { b"print" } else { b"send" };
        check_pin(&expect, &pin_of(program, &[("action", action), ("from", text(sender.is_private(), from).as_bytes()), ("from_state", state.as_slice()), ("to", to_text(&recipient, reach).as_bytes()), ("to_state", to_state.as_slice())]))?;
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

    async fn nft_burn(&mut self, program: AccountId, from: AccountId, fee_payer: Option<AccountId>, expect: Option<String>) -> Result<(HashType, u64)> {
        let holder = self.nft_signer(from)?;
        if holder.is_private() {
            self.wallet.sync_to_latest_block().await.context("cannot sync the private accounts")?;
        }
        let state = self.nft_state(program, from).await?;
        check_pin(&expect, &pin_of(program, &[("action", b"burn".as_slice()), ("from", text(holder.is_private(), from).as_bytes()), ("from_state", state.as_slice())]))?;
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

    async fn nft_receive(&mut self, program: AccountId, fee_payer: AccountId, expect: Option<String>) -> Result<Vec<(HashType, u64)>> {
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
        let positions: Vec<String> = pending.iter().map(|m| m.position.to_string()).collect();
        check_pin(&expect, &pin_of(program, &[("action", b"receive".as_slice()), ("credits", positions.join(",").as_bytes())]))?;
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
                if let Some((code, why)) = left_reason(&self.nft_state(program, to).await?, &definition) {
                    // One line the module reads (`left: <account> <collection> <code>`), one for people.
                    println!("left: Private/{to} {} {code}", account(definition));
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

    /// A private fee payer: one of this wallet's private accounts holding at least the fee at the
    /// chain's head, which is the least the LEZ wallet reserves (it reserves up to `at_most`, never
    /// more than the payer holds: `lez/wallet/src/lib.rs` reserve rule). One source for the
    /// figure: `fees::private` (Job A's ~5,070,616 is its `about` at base fee 8).
    async fn nft_check_payer(&mut self, payer: AccountId) -> Result<crate::fees::Estimate> {
        if !self.is_private(payer) {
            bail!("the fee payer of a private transaction must be one of this wallet's private accounts (a public payer fails for a proven transaction)");
        }
        let reserve = self.nft_reserve().await?;
        let held = self.spendable(payer).await?;
        if !payer_covers(held, &reserve) {
            bail!("Private/{payer} holds {held}; a private transaction needs at least {} for its fee (it holds up to {} while it runs)", reserve.about, reserve.at_most);
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
        let proven;
        let mut lines: Vec<String> = Vec::new();
        let mut named: Vec<&str> = Vec::new();
        let mut hidden: Vec<&str> = Vec::new();
        let mut collection = None;
        let pin: String;
        let mut to_account: Option<String> = None;
        let mut kind: Option<&str> = None;
        let mut left: Vec<serde_json::Value> = Vec::new();
        let from_private = from.is_some_and(|from| self.is_private(from));
        match action.action.as_str() {
            "init" => {
                let definition = action.definition.as_deref().map(parse_account).transpose().map_err(|error| anyhow!(error))?.ok_or_else(|| anyhow!("init needs a definition"))?;
                collection = Some(self.nft_definition(program, definition).await.map(|found| collection_json(definition, &found)));
                let new_private = action.new.as_deref() == Some("private");
                proven = from_private || new_private;
                pin = match (from, action.new.as_deref()) {
                    (Some(from), _) => {
                        let state = self.nft_state(program, from).await?;
                        pin_of(program, &[("action", b"init".as_slice()), ("definition", definition.to_string().as_bytes()), ("holder", text(from_private, from).as_bytes()), ("holder_state", state.as_slice())])
                    }
                    (None, Some(new)) => pin_of(program, &[("action", b"init".as_slice()), ("definition", definition.to_string().as_bytes()), ("holder", format!("new:{new}").as_bytes()), ("holder_state", b"".as_slice())]),
                    (None, None) => bail!("init needs an account or new"),
                };
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
                let to_state = if reach == Reach::TheirPrivate { Vec::new() } else { self.nft_state(program, recipient.account_id()).await? };
                let resolved = to_text(&recipient, reach);
                pin = pin_of(program, &[("action", action.action.as_bytes()), ("from", text(from_private, from).as_bytes()), ("from_state", state.as_slice()), ("to", resolved.as_bytes()), ("to_state", to_state.as_slice())]);
                to_account = Some(resolved);
                kind = nft_wire::holding(&state).kind();
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
                            if !action.their_keys {
                                refusal = Some("Tick that you accept that this wallet can't check their account.".to_owned());
                            }
                        }
                    }
                }
            }
            "burn" => {
                let from = from.ok_or_else(|| anyhow!("burn needs from"))?;
                let state = self.nft_state(program, from).await?;
                proven = from_private;
                pin = pin_of(program, &[("action", b"burn".as_slice()), ("from", text(from_private, from).as_bytes()), ("from_state", state.as_slice())]);
                kind = nft_wire::holding(&state).kind();
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
            "receive" => {
                proven = true;
                let pending: Vec<_> = self
                    .wallet
                    .owned_pending_messages()
                    .await
                    .context("cannot read pending messages")?
                    .into_iter()
                    .filter(|message| message.body.to.program_account_id == program && nft_wire::credit_of(&message.body.message).is_some())
                    .collect();
                let positions: Vec<String> = pending.iter().map(|m| m.position.to_string()).collect();
                pin = pin_of(program, &[("action", b"receive".as_slice()), ("credits", positions.join(",").as_bytes())]);
                // Each credit checked as the receipt will check it: only what can land is counted,
                // and each one that can't is named with its reason (review finding 3).
                let mut landing = 0;
                for message in &pending {
                    let to = message.body.to.account_id;
                    let (definition, _) = nft_wire::credit_of(&message.body.message).expect("filtered above");
                    match left_reason(&self.nft_state(program, to).await?, &definition) {
                        None => landing += 1,
                        Some((code, why)) => {
                            lines.push(format!("Waiting for Private/{to}, but it can't land there: {why}"));
                            left.push(serde_json::json!({ "account": format!("Private/{to}"), "definition": account(definition).to_string(), "code": code }));
                        }
                    }
                }
                if pending.is_empty() {
                    refusal = Some("Nothing is waiting for your private accounts.".to_owned());
                } else if landing == 0 {
                    refusal = Some("None of the waiting NFTs can land yet: see why above.".to_owned());
                }
                lines.insert(0, format!("Takes the {landing} NFT(s) that can land: one private transaction each, each with this fee."));
                named.push("Expected: like the other private transactions, no account named (receiving an NFT was not observed in Job A).");
            }
            other => bail!("no NFT action called {other}"),
        }
        // Who can pay: a proven action from a private account holding the reserve; a public one
        // from a public account of this wallet holding the most it may cost (NFT messages took
        // 28-37k gas in Job A; priced here at 40k).
        let reserve = if proven {
            self.nft_reserve().await?
        } else {
            let quote = self.wallet.client().get_fee_state().await?;
            crate::fees::public(&quote, 40_000, gas_limit_for(40_000), 128)
        };
        let mut payers = Vec::new();
        let key_chain = self.wallet.storage().key_chain();
        let candidates: Vec<(bool, AccountId)> = if proven {
            key_chain.private_account_ids().map(|(id, _)| (true, id)).collect()
        } else {
            key_chain.public_account_ids().map(|(id, _)| (false, id)).collect()
        };
        for (private, payer) in candidates {
            let held = self.spendable(payer).await.unwrap_or(0);
            payers.push(serde_json::json!({ "account": text(private, payer), "balance": held.to_string(), "enough": if proven { payer_covers(held, &reserve) } else { held >= reserve.at_most } }));
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
            "pin": pin,
            "to_account": to_account,
            "kind": kind,
            "left": left,
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

#[cfg(test)]
mod tests {
    use super::*;

    const BOB: &str = "HLEwXNBDYx6GS9H5CsawDrMTptD6R4XsJywFEMr6eim2";

    #[test]
    fn a_destination_is_a_name_an_account_or_keys_never_a_bare_id() {
        assert_eq!(parse_to("alice"), Ok(To::Name("alice".to_owned())));
        assert!(matches!(parse_to(&format!("Public/{BOB}")), Ok(To::Public(_))));
        assert!(matches!(parse_to(&format!("Private/{BOB}")), Ok(To::Private(_))));
        assert!(matches!(parse_to(&format!("{}:{}", "ab".repeat(32), "cd".repeat(8))), Ok(To::Keys { .. })));
        assert!(parse_to(BOB).unwrap_err().contains("Public/"));
        assert!(parse_to("ab:cd").is_err());
        assert!(parse_to("two words").is_err());
    }

    #[test]
    fn the_private_reserve_comes_from_the_wallets_own_fee_rule() {
        // The throwaway chain's fee state on 2026-11-19 (getFeeState, height 3539).
        let quote: sequencer_service_rpc::FeeStateQuote = serde_json::from_value(serde_json::json!({
            "height": 3539, "base_fee_exec": 8, "base_fee_stor": 8,
            "next_base_fee_exec_floor": 8, "next_base_fee_exec_ceiling": 9,
            "next_base_fee_stor_floor": 8, "next_base_fee_stor_ceiling": 9,
            "max_gas_exec": 10_000_000, "max_gas_stor": 1_000_000
        }))
        .unwrap();
        // The LEZ wallet's default headroom is 5 blocks.
        let reserve = crate::fees::private(&quote, 5);
        // Job A's "~5,070,616 reserved per private action" is exactly the fee at head:
        // (PRIVATE_VERIFY_GAS + PRIVATE_GAS_STOR) x 8 = 633,827 x 8.
        assert_eq!(reserve.about, 5_070_616);
        // Five full blocks lift both base fees 8 -> 13: the most the wallet holds while it runs.
        assert_eq!(reserve.at_most, 8_239_751);
        assert!(payer_covers(5_070_616, &reserve));
        assert!(!payer_covers(5_070_615, &reserve));
    }

    #[test]
    fn a_sheet_pin_names_the_state_and_a_change_refuses() {
        let program: AccountId = BOB.parse().unwrap();
        let p = |from_state: &[u8], to: &[u8], to_state: &[u8]| pin_of(program, &[("action", b"send".as_slice()), ("from", b"Public/x".as_slice()), ("from_state", from_state), ("to", to), ("to_state", to_state)]);
        let before = p(&[2, 1], b"Public/y", &[]);
        assert_eq!(before.len(), 64);
        assert!(check_pin(&Some(before.clone()), &before).is_ok());
        assert!(check_pin(&None, &before).is_ok(), "no pin given: a terminal run");
        // The recipient's holding changed, or the name points elsewhere, or the sender's: refused.
        for changed in [p(&[2, 1], b"Public/y", &[2, 0]), p(&[2, 1], b"Public/z", &[]), p(&[2, 0], b"Public/y", &[])] {
            assert!(check_pin(&Some(before.clone()), &changed).unwrap_err().to_string().contains("Nothing was sent"));
        }
        // Labels and lengths are in the digest: moving bytes between fields changes it.
        assert_ne!(pin_of(program, &[("a", b"xy".as_slice()), ("b", b"".as_slice())]), pin_of(program, &[("a", b"x".as_slice()), ("b", b"y".as_slice())]));
    }

    #[test]
    fn a_waiting_credit_that_cant_land_says_why() {
        let art: nft_wire::Id = [7; 32];
        assert_eq!(left_reason(&[], &art), Some(("not_ready", nft_wire::NOT_READY)));
        let owned = nft_wire::State::Copy { definition: art, owned: true }.encode();
        assert_eq!(left_reason(&owned, &art).unwrap().0, "already_holds");
        let other = nft_wire::State::Copy { definition: [8; 32], owned: false }.encode();
        assert_eq!(left_reason(&other, &art).unwrap().0, "other_collection");
        assert_eq!(left_reason(&nft_wire::State::Copy { definition: art, owned: false }.encode(), &art), None);
    }

    #[test]
    fn no_program_is_built_in_and_the_ids_round_trip() {
        assert!(program(None).unwrap_err().to_string().contains("--nft-program"));
        let bob: AccountId = BOB.parse().unwrap();
        assert_eq!(account(id(bob)), bob);
        // The shared encoding speaks the CLI's account IDs: Transfer's 32 bytes are the ID's.
        assert_eq!(&Message::Transfer { to: id(bob) }.encode()[1..], bob.value().as_slice());
    }
}
