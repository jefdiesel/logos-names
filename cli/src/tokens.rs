//! Fungible tokens: `names token …` and the module's `names module token …` requests.
//!
//! No official token program runs on LEZ 0.4 yet (`docs/wallet/TOKENS.md`). Everything
//! program-specific sits behind [`TokenCodec`]: how a transfer, a mint and a new token are
//! encoded and which accounts each one reaches, and how holdings, definitions and pending
//! credits decode. [`PrototypeCodec`] speaks the prototype in `wallet-tokens/`; the official
//! program, when it lands, is one more implementation and a `--token-codec` value. The
//! program's account ID is never built in: it comes from `--token-program`
//! (`NAMES_TOKEN_PROGRAM`) or `token_program` in `names.json`.
//!
//! The wallet plumbing is the native token's: the sender's actor of the token program sends
//! the message, a recipient this wallet can witness is declared and credited in the same
//! transaction, and someone else's private account (a name's receiving keys) gets a pending
//! credit that its wallet takes with `names token receive`. The LEZ wallet's sweep only
//! takes native credits, so token credits need that explicit step.

use std::collections::BTreeMap;

use anyhow::{Context as _, Result, anyhow, bail};
use clap::Subcommand;
use common::HashType;
use lee::privacy_preserving_transaction::circuit::ProgramCatalog;
use lee_core::{
    NullifierPublicKey, PrivateAccountKind, account::AccountId, encryption::ViewingPublicKey,
};
use wallet::{AccountIdentity, AccountMention, AsyncDelivery};

use crate::{Client, gas_limit_for, load_defaults, parse_account, parse_name};

/// Balance per token definition.
pub type Holdings = BTreeMap<AccountId, u128>;

/// What a token definition says about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Definition {
    pub name: String,
    pub supply: u128,
    /// Who may mint more; `None` is a fixed supply.
    pub authority: Option<AccountId>,
}

/// How a call reaches an account, in the order the transaction declares them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// The root: the message goes to this account's actor, which must authorize it.
    Signer(AccountId),
    /// Declared so the call can reach it; it does not authorize anything.
    Reached(AccountId),
    /// Credited by the call: in the same transaction when this wallet can witness it,
    /// otherwise later, as a pending credit.
    Credited(AccountId),
}

/// A token program call: the message and the accounts it reaches, the root first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub message: Vec<u8>,
    pub roles: Vec<Role>,
}

/// Everything about a token program that differs between programs. The wallet side
/// (accounts, proving, pending credits) stays the same as long as the program keeps the
/// native token's shape: holdings in the holder's own actor, credits as async messages.
pub trait TokenCodec {
    /// Move `amount` of `definition` from `from`, which signs, to `to`.
    fn transfer(&self, definition: AccountId, from: AccountId, to: AccountId, amount: u128)
    -> Call;
    /// Mint `amount` of `definition` to `to`, signed by `authority`.
    fn mint(
        &self,
        definition: AccountId,
        authority: AccountId,
        to: AccountId,
        amount: u128,
    ) -> Call;
    /// Create a token at the new account `definition`, which signs, with `supply` credited
    /// to `supply_to`; `authority` may mint more.
    fn create(
        &self,
        definition: AccountId,
        name: &str,
        supply: u128,
        authority: Option<AccountId>,
        supply_to: AccountId,
    ) -> Call;
    /// What an account holds, from its state under the program. Empty state holds nothing;
    /// so does a definition.
    fn holdings(&self, state: &[u8]) -> Result<Holdings>;
    /// The definition an account's state holds, if it is one.
    fn definition(&self, state: &[u8]) -> Option<Definition>;
    /// The definition and amount a pending message credits, if it is a credit.
    fn credit(&self, message: &[u8]) -> Option<(AccountId, u128)>;
    /// Longest token name, in bytes.
    fn max_name_len(&self) -> usize;
    /// Most token kinds one account may hold, if the program caps it.
    fn max_holdings(&self) -> Option<usize>;
}

/// The prototype program in `wallet-tokens/` (wire format in `docs/wallet/TOKENS.md`).
pub struct PrototypeCodec;

impl TokenCodec for PrototypeCodec {
    fn transfer(
        &self,
        definition: AccountId,
        from: AccountId,
        to: AccountId,
        amount: u128,
    ) -> Call {
        Call {
            message: encode(&token_core::Message::Transfer {
                definition,
                to,
                amount,
            }),
            roles: vec![Role::Signer(from), Role::Credited(to)],
        }
    }

    fn mint(
        &self,
        definition: AccountId,
        authority: AccountId,
        to: AccountId,
        amount: u128,
    ) -> Call {
        // Sent to the authority's own actor, which forwards it to the definition; a
        // definition that is its own authority takes it directly.
        let mut roles = vec![Role::Signer(authority)];
        if definition != authority {
            roles.push(Role::Reached(definition));
        }
        roles.push(Role::Credited(to));
        Call {
            message: encode(&token_core::Message::Mint {
                definition,
                to,
                amount,
            }),
            roles,
        }
    }

    fn create(
        &self,
        definition: AccountId,
        name: &str,
        supply: u128,
        authority: Option<AccountId>,
        supply_to: AccountId,
    ) -> Call {
        Call {
            message: encode(&token_core::Message::NewFungibleDefinition {
                name: name.to_owned(),
                total_supply: supply,
                mint_authority: authority,
                supply_to,
            }),
            roles: vec![Role::Signer(definition), Role::Credited(supply_to)],
        }
    }

    fn holdings(&self, state: &[u8]) -> Result<Holdings> {
        match token_core::TokenState::decode(state) {
            Some(token_core::TokenState::Holdings(holdings)) => Ok(holdings),
            Some(token_core::TokenState::Definition(_)) => Ok(Holdings::new()),
            None => bail!("the account holds state the token program does not write"),
        }
    }

    fn definition(&self, state: &[u8]) -> Option<Definition> {
        match token_core::TokenState::decode(state)? {
            token_core::TokenState::Definition(token_core::TokenDefinition::Fungible {
                name,
                total_supply,
                authority,
                ..
            }) => Some(Definition {
                name,
                supply: total_supply,
                authority,
            }),
            token_core::TokenState::Holdings(_) => None,
        }
    }

    fn credit(&self, message: &[u8]) -> Option<(AccountId, u128)> {
        match borsh::from_slice(message).ok()? {
            token_core::Message::Credit { definition, amount } => Some((definition, amount)),
            _ => None,
        }
    }

    fn max_name_len(&self) -> usize {
        token_core::MAX_NAME_LEN
    }

    fn max_holdings(&self) -> Option<usize> {
        Some(token_core::MAX_HOLDINGS)
    }
}

fn encode(message: &token_core::Message) -> Vec<u8> {
    borsh::to_vec(message).expect("a token message serializes")
}

/// The codecs `--token-codec` may name.
pub const CODECS: [&str; 1] = ["prototype"];

fn codec(name: &str) -> Result<Box<dyn TokenCodec>> {
    match name {
        "prototype" => Ok(Box::new(PrototypeCodec)),
        other => bail!(
            "no token codec called {other}; known: {}",
            CODECS.join(", ")
        ),
    }
}

/// The token program in use and how to speak to it.
pub struct Tokens {
    pub program: AccountId,
    pub codec: Box<dyn TokenCodec>,
}

impl Tokens {
    /// The program from `--token-program` (or `NAMES_TOKEN_PROGRAM`), else `names.json`'s
    /// `token_program`. Nothing is built in: no token program is deployed on the public
    /// testnet yet.
    pub fn select(program: Option<AccountId>, codec_name: &str) -> Result<Self> {
        let program = match program {
            Some(program) => program,
            None => load_defaults()
                .token_program
                .as_deref()
                .map(parse_account)
                .transpose()
                .map_err(|error| anyhow!("names.json's token_program: {error}"))?
                .ok_or_else(|| {
                    anyhow!(
                        "no token program given: pass --token-program <account ID> or set NAMES_TOKEN_PROGRAM \
                         (no token program is deployed on the public testnet yet)"
                    )
                })?,
        };
        Ok(Self {
            program,
            codec: codec(codec_name)?,
        })
    }
}

/// Where a token send goes, as given on the command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum To {
    /// A name, resolved like `names pay` does.
    Name(String),
    /// A public account (`Public/<id>`, or a bare account ID).
    Public(AccountId),
    /// One of this wallet's private accounts (`Private/<id>`).
    Private(AccountId),
    /// Someone's private account by its receiving keys, `<npk hex>:<vpk hex>`; the credit
    /// stays pending until their wallet receives it.
    Keys { npk: [u8; 32], vpk: Vec<u8> },
}

pub fn parse_to(input: &str) -> Result<To, String> {
    let input = input.trim();
    if let Some(id) = input.strip_prefix("Public/") {
        return parse_account(id).map(To::Public);
    }
    if let Some(id) = input.strip_prefix("Private/") {
        return parse_account(id).map(To::Private);
    }
    if let Some((npk, vpk)) = input.split_once(':') {
        let npk = hex::decode(npk)
            .ok()
            .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok());
        if let (Some(npk), Ok(vpk)) = (npk, hex::decode(vpk)) {
            if vpk.is_empty() {
                return Err("the viewing key after the colon is empty".to_owned());
            }
            return Ok(To::Keys { npk, vpk });
        }
    }
    if let Ok(account) = input.parse::<AccountId>() {
        return Ok(To::Public(account));
    }
    parse_name(input).map(To::Name).map_err(|error| {
        format!("{input} is not a name, Public/<id>, Private/<id> or <npk hex>:<vpk hex> ({error})")
    })
}

/// The account a token leaves from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// The public account from `names setup`.
    SetupPublic,
    /// The private account from `names setup`.
    SetupPrivate,
    Account(AccountId),
}

pub fn parse_from(input: &str) -> Result<Origin, String> {
    match input.trim().to_ascii_lowercase().as_str() {
        "public" => Ok(Origin::SetupPublic),
        "private" => Ok(Origin::SetupPrivate),
        _ => parse_account(input.trim()).map(Origin::Account),
    }
}

/// An amount in base units: a positive whole number that fits in 128 bits. `_` and `,`
/// may group digits.
pub fn parse_amount(input: &str) -> Result<u128, String> {
    let digits: String = input
        .trim()
        .chars()
        .filter(|c| !matches!(c, '_' | ','))
        .collect();
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!("{input} is not a whole number of base units"));
    }
    match digits.parse::<u128>() {
        Ok(0) => Err("the amount must be more than 0".to_owned()),
        Ok(amount) => Ok(amount),
        Err(_) => Err(format!(
            "{input} is more than a token amount can be (2^128 - 1)"
        )),
    }
}

/// A sender holding `balance` can send `amount`.
pub fn check_spend(balance: u128, amount: u128, from: &str, token: &str) -> Result<()> {
    if amount == 0 {
        bail!("the amount must be more than 0");
    }
    if balance < amount {
        bail!("{from} holds {balance} {token}, less than {amount}");
    }
    Ok(())
}

/// A recipient holding `held` can take `amount` of `definition`: its balance doesn't
/// overflow, and a new kind of token fits under the program's cap.
pub fn check_credit(
    held: &Holdings,
    definition: &AccountId,
    amount: u128,
    max_holdings: Option<usize>,
    to: &str,
) -> Result<()> {
    match held.get(definition) {
        Some(balance) if balance.checked_add(amount).is_none() => {
            bail!("{to} holds {balance} of it already; {amount} more would overflow its balance")
        }
        Some(_) => Ok(()),
        None if max_holdings.is_some_and(|cap| held.len() >= cap) => {
            bail!(
                "{to} already holds {} kinds of token, the most the token program allows",
                held.len()
            )
        }
        None => Ok(()),
    }
}

/// `by` may mint `amount` more of `definition`.
pub fn check_mint(definition: &Definition, by: AccountId, amount: u128) -> Result<()> {
    match definition.authority {
        None => bail!(
            "{} has a fixed supply: nobody may mint more",
            definition.name
        ),
        Some(authority) if authority != by => {
            bail!(
                "only {authority}, the mint authority of {}, may mint it",
                definition.name
            )
        }
        Some(_) => {}
    }
    if definition.supply.checked_add(amount).is_none() {
        bail!(
            "minting {amount} would overflow the supply of {} ({})",
            definition.name,
            definition.supply
        );
    }
    Ok(())
}

/// A token name the program takes.
pub fn check_token_name(name: &str, max: usize) -> Result<()> {
    if name.is_empty() || name.len() > max {
        bail!("a token name takes 1 to {max} bytes");
    }
    Ok(())
}

#[derive(Subcommand)]
pub enum TokenCommand {
    /// What this wallet's accounts hold of every token (public accounts read from the
    /// chain, private ones from this wallet's notes after a sync), and the private credits
    /// waiting for `names token receive`.
    List {
        /// Only this account.
        #[arg(long, value_parser = parse_account)]
        account: Option<AccountId>,
        /// Print JSON, for apps.
        #[arg(long)]
        json: bool,
    },
    /// A token's name, supply and mint authority.
    Info {
        #[arg(value_parser = parse_account)]
        definition: AccountId,
        #[arg(long)]
        json: bool,
    },
    /// Send a token. To a name, it goes where `names pay` would send: to the name's
    /// private receiving keys (a pending credit its holder receives), or with `--public`,
    /// or when the name has only a public account, to that account. From a private account,
    /// or to private keys, the transaction is proven on this computer (several minutes).
    Send {
        /// The token: its definition's account ID.
        #[arg(value_parser = parse_account)]
        definition: AccountId,
        /// Amount in base units.
        #[arg(value_parser = parse_amount)]
        amount: u128,
        /// A name, `Public/<id>`, one of this wallet's `Private/<id>`, or someone's private
        /// receiving keys as `<npk hex>:<vpk hex>`.
        #[arg(long, value_parser = parse_to)]
        to: To,
        /// `public` or `private` (the accounts from `names setup`), or an account of this
        /// wallet. Defaults to the private one, as `names pay` does.
        #[arg(long, value_parser = parse_from, default_value = "private")]
        from: Origin,
        /// To a name: its public account instead of its private keys.
        #[arg(long, conflicts_with = "private")]
        public: bool,
        /// To a name: its private keys (the default when it has them).
        #[arg(long)]
        private: bool,
    },
    /// Receive the token credits sent to this wallet's private receiving keys. Each one is
    /// a proven transaction, paid from a private account's native balance.
    Receive,
    /// Create a token (for testing and demos): a new public account of this wallet
    /// becomes its definition.
    Create {
        /// Its name, as wallets show it.
        name: String,
        /// The initial supply, in base units.
        supply: u128,
        /// Who receives the initial supply: `public`, `private` or an account. Defaults to
        /// the payer.
        #[arg(long, value_parser = parse_from)]
        to: Option<Origin>,
        /// Let the payer mint more later.
        #[arg(long, conflicts_with = "authority")]
        mintable: bool,
        /// Let this account mint more later.
        #[arg(long, value_parser = parse_account)]
        authority: Option<AccountId>,
        /// The public account that pays the fee; defaults to the one from `names setup`.
        #[arg(long, value_parser = parse_account)]
        payer: Option<AccountId>,
    },
    /// Mint more of a token (for testing and demos); the wallet must hold the mint
    /// authority's key.
    Mint {
        #[arg(value_parser = parse_account)]
        definition: AccountId,
        #[arg(value_parser = parse_amount)]
        amount: u128,
        /// Who receives it: `public`, `private` or an account. Defaults to the authority.
        #[arg(long, value_parser = parse_from)]
        to: Option<Origin>,
    },
}

/// The module's token requests. Each prints its answer as JSON on its last line.
#[derive(Subcommand)]
pub enum TokenRequest {
    /// `[{account, private, definition, name, balance}]`.
    Holdings {
        #[arg(long)]
        account: Option<String>,
    },
    /// `[{position, definition, name, amount, to}]`: credits waiting to be received.
    Pending,
    /// `{definition, name, supply, authority}`.
    Info { definition: String },
    /// Sends the transfer given as JSON on standard input:
    /// `{definition, amount, to, from?, public?, private?}`, `amount` a decimal string.
    /// Answers `{tx, block}`.
    Send,
    /// Receives every pending credit; answers `[{tx, block, definition, amount, to}]`.
    Receive,
}

#[derive(serde::Deserialize)]
struct SendRequest {
    definition: String,
    amount: String,
    to: String,
    #[serde(default)]
    from: Option<String>,
    #[serde(default)]
    public: bool,
    #[serde(default)]
    private: bool,
}

/// One token one account holds.
#[derive(serde::Serialize)]
pub struct HoldingRow {
    account: String,
    private: bool,
    definition: String,
    name: Option<String>,
    balance: String,
}

/// A credit waiting to be received.
#[derive(serde::Serialize)]
pub struct PendingRow {
    position: u64,
    definition: String,
    name: Option<String>,
    amount: String,
    to: String,
}

fn account_text(private: bool, account: AccountId) -> String {
    format!("{}/{account}", if private { "Private" } else { "Public" })
}

impl Client {
    pub async fn token(&mut self, tokens: &Tokens, command: TokenCommand) -> Result<()> {
        match command {
            TokenCommand::List { account, json } => {
                let rows = self.token_holdings(tokens, account).await?;
                let pending = self.token_pending(tokens).await?;
                if json {
                    println!(
                        "{}",
                        serde_json::json!({ "holdings": rows, "pending": pending })
                    );
                    return Ok(());
                }
                if rows.is_empty() {
                    println!("no tokens held");
                }
                let mut last = None;
                for row in &rows {
                    if last != Some(&row.account) {
                        println!("{}", row.account);
                        last = Some(&row.account);
                    }
                    let name = row.name.as_deref().unwrap_or("(unknown token)");
                    println!("  {}  {name}  {}", row.balance, row.definition);
                }
                if !pending.is_empty() {
                    println!("waiting to be received (`names token receive`):");
                    for row in &pending {
                        let name = row.name.as_deref().unwrap_or("(unknown token)");
                        println!(
                            "  {}  {name}  {}  to Private/{}",
                            row.amount, row.definition, row.to
                        );
                    }
                }
                Ok(())
            }
            TokenCommand::Info { definition, json } => {
                let found = self.token_definition(tokens, definition).await?;
                if json {
                    println!("{}", definition_json(definition, &found));
                } else {
                    println!("name      {}", found.name);
                    println!("supply    {}", found.supply);
                    match found.authority {
                        Some(authority) => println!("authority {authority}"),
                        None => println!("authority none (fixed supply)"),
                    }
                }
                Ok(())
            }
            TokenCommand::Send {
                definition,
                amount,
                to,
                from,
                public,
                private,
            } => {
                let from = self.from_account(from)?;
                self.token_send(tokens, definition, amount, &to, from, public, private)
                    .await
                    .map(drop)
            }
            TokenCommand::Receive => self.token_receive(tokens).await.map(drop),
            TokenCommand::Create {
                name,
                supply,
                to,
                mintable,
                authority,
                payer,
            } => {
                let payer = match payer {
                    Some(payer) => payer,
                    None => self.from_account(Origin::SetupPublic)?,
                };
                let to = match to {
                    Some(to) => self.from_account(to)?,
                    None => payer,
                };
                let authority = if mintable { Some(payer) } else { authority };
                self.token_create(tokens, &name, supply, to, authority, payer)
                    .await
            }
            TokenCommand::Mint {
                definition,
                amount,
                to,
            } => {
                let to = to.map(|to| self.from_account(to)).transpose()?;
                self.token_mint(tokens, definition, amount, to).await
            }
        }
    }

    pub async fn module_token(&mut self, tokens: &Tokens, request: TokenRequest) -> Result<()> {
        match request {
            TokenRequest::Holdings { account } => {
                let account = account
                    .as_deref()
                    .map(parse_account)
                    .transpose()
                    .map_err(|error| anyhow!(error))?;
                let rows = self.token_holdings(tokens, account).await?;
                println!("{}", serde_json::to_string(&rows)?);
            }
            TokenRequest::Pending => {
                let rows = self.token_pending(tokens).await?;
                println!("{}", serde_json::to_string(&rows)?);
            }
            TokenRequest::Info { definition } => {
                let definition = parse_account(&definition).map_err(|error| anyhow!(error))?;
                let found = self.token_definition(tokens, definition).await?;
                println!("{}", definition_json(definition, &found));
            }
            TokenRequest::Send => {
                let mut input = Vec::new();
                std::io::Read::read_to_end(&mut std::io::stdin(), &mut input)
                    .context("cannot read the request")?;
                let request: SendRequest =
                    serde_json::from_slice(&input).context("not a token send request")?;
                let definition =
                    parse_account(&request.definition).map_err(|error| anyhow!(error))?;
                let amount = parse_amount(&request.amount).map_err(|error| anyhow!(error))?;
                let to = parse_to(&request.to).map_err(|error| anyhow!(error))?;
                let from = parse_from(request.from.as_deref().unwrap_or("private"))
                    .map_err(|error| anyhow!(error))?;
                let from = self.from_account(from)?;
                let (hash, block) = self
                    .token_send(
                        tokens,
                        definition,
                        amount,
                        &to,
                        from,
                        request.public,
                        request.private,
                    )
                    .await?;
                println!(
                    "{}",
                    serde_json::json!({ "tx": hash.to_string(), "block": block })
                );
            }
            TokenRequest::Receive => {
                let received = self.token_receive(tokens).await?;
                let rows: Vec<serde_json::Value> = received
                    .into_iter()
                    .map(|(hash, block, credit, to)| {
                        serde_json::json!({
                            "tx": hash.to_string(),
                            "block": block,
                            "definition": credit.map(|(definition, _)| definition.to_string()),
                            "amount": credit.map(|(_, amount)| amount.to_string()),
                            "to": to.to_string(),
                        })
                    })
                    .collect();
                println!("{}", serde_json::Value::Array(rows));
            }
        }
        Ok(())
    }

    /// The account `from` names.
    fn from_account(&self, from: Origin) -> Result<AccountId> {
        let defaults = load_defaults();
        let setup = |id: Option<String>, kind: &str| {
            id.and_then(|id| id.parse().ok()).ok_or_else(|| {
                anyhow!("no {kind} account set up; run `names setup` first, or give an account")
            })
        };
        match from {
            Origin::SetupPublic => setup(defaults.public, "public"),
            Origin::SetupPrivate => setup(defaults.private, "private"),
            Origin::Account(account) => Ok(account),
        }
    }

    /// How `account` takes part in a token call: a private account of this wallet, a public
    /// one whose key this wallet holds (it signs, which also admits an account the chain has
    /// not seen), or someone else's public account.
    fn token_identity(&self, account: AccountId) -> AccountIdentity {
        if let Some(private) = self.wallet.resolve_private_account(account) {
            return private;
        }
        if self
            .wallet
            .get_account_public_signing_key(account)
            .is_some()
        {
            AccountIdentity::Public(account)
        } else {
            AccountIdentity::PublicNoSign(account)
        }
    }

    /// The token program's state on `account`, read where it lives: a private account's
    /// from this wallet's notes, a public one's from the chain.
    async fn token_state(&self, tokens: &Tokens, account: AccountId) -> Result<Vec<u8>> {
        if self.is_private(account) {
            return Ok(self
                .wallet
                .get_account_private(account)
                .map(|found| found.data.actor_state(tokens.program).to_vec())
                .unwrap_or_default());
        }
        self.actor_state_of(account, tokens.program).await
    }

    async fn held(&self, tokens: &Tokens, account: AccountId) -> Result<Holdings> {
        let state = self.token_state(tokens, account).await?;
        tokens
            .codec
            .holdings(&state)
            .with_context(|| format!("reading what {account} holds"))
    }

    pub async fn token_definition(
        &self,
        tokens: &Tokens,
        definition: AccountId,
    ) -> Result<Definition> {
        let state = self.actor_state_of(definition, tokens.program).await?;
        tokens.codec.definition(&state).ok_or_else(|| {
            anyhow!(
                "{definition} is not a token of the token program {}",
                tokens.program
            )
        })
    }

    async fn token_name(
        &self,
        tokens: &Tokens,
        names: &mut BTreeMap<AccountId, Option<String>>,
        definition: AccountId,
    ) -> Option<String> {
        if let Some(known) = names.get(&definition) {
            return known.clone();
        }
        let name = self
            .token_definition(tokens, definition)
            .await
            .ok()
            .map(|found| found.name);
        names.insert(definition, name.clone());
        name
    }

    async fn token_holdings(
        &mut self,
        tokens: &Tokens,
        only: Option<AccountId>,
    ) -> Result<Vec<HoldingRow>> {
        self.wallet
            .sync_to_latest_block()
            .await
            .context("cannot sync the private accounts")?;
        let key_chain = self.wallet.storage().key_chain();
        let mut accounts: Vec<(bool, AccountId)> = key_chain
            .public_account_ids()
            .map(|(id, _)| (false, id))
            .chain(key_chain.private_account_ids().map(|(id, _)| (true, id)))
            .collect();
        if let Some(only) = only {
            accounts.retain(|(_, id)| *id == only);
            if accounts.is_empty() {
                // Someone else's public account can be read too.
                accounts.push((false, only));
            }
        }
        let mut names = BTreeMap::new();
        let mut rows = Vec::new();
        for (private, account) in accounts {
            for (definition, balance) in self.held(tokens, account).await? {
                rows.push(HoldingRow {
                    account: account_text(private, account),
                    private,
                    definition: definition.to_string(),
                    name: self.token_name(tokens, &mut names, definition).await,
                    balance: balance.to_string(),
                });
            }
        }
        Ok(rows)
    }

    async fn token_pending(&mut self, tokens: &Tokens) -> Result<Vec<PendingRow>> {
        let pending = self
            .wallet
            .owned_pending_messages()
            .await
            .context("cannot read pending messages")?;
        let mut names = BTreeMap::new();
        let mut rows = Vec::new();
        for message in pending
            .iter()
            .filter(|message| message.body.to.program_account_id == tokens.program)
        {
            let Some((definition, amount)) = tokens.codec.credit(&message.body.message) else {
                continue;
            };
            rows.push(PendingRow {
                position: message.position,
                definition: definition.to_string(),
                name: self.token_name(tokens, &mut names, definition).await,
                amount: amount.to_string(),
                to: message.body.to.account_id.to_string(),
            });
        }
        Ok(rows)
    }

    /// The token program's bytecode, from the chain (cached next to the wallet, checked
    /// against its image ID), for proving; `NAMES_TOKEN_PROGRAM_ELF` overrides it.
    async fn token_catalog(&self, tokens: &Tokens) -> Result<ProgramCatalog> {
        let program = self
            .program_at(
                tokens.program,
                "NAMES_TOKEN_PROGRAM_ELF",
                "names-token",
                "the token program",
            )
            .await?;
        Ok(ProgramCatalog::from([(tokens.program, program)]))
    }

    /// The identity a recipient takes part as, resolving a name like `names pay` does.
    async fn recipient(
        &self,
        to: &To,
        public: bool,
        private: bool,
    ) -> Result<(AccountIdentity, String)> {
        Ok(match to {
            To::Name(name) => {
                let identity = self.payee(name, public, private).await?;
                let shown = match &identity {
                    AccountIdentity::PrivateForeign { .. } => {
                        format!("{name}'s private receiving keys")
                    }
                    other => format!("{name} (Public/{})", other.account_id()),
                };
                (identity, shown)
            }
            To::Public(account) => (self.token_identity(*account), format!("Public/{account}")),
            To::Private(account) => {
                let identity = self.wallet.resolve_private_account(*account).ok_or_else(|| {
                    anyhow!("Private/{account} is not this wallet's; pay someone else's private account by its keys, <npk hex>:<vpk hex>")
                })?;
                (identity, format!("Private/{account}"))
            }
            To::Keys { npk, vpk } => {
                let vpk = ViewingPublicKey::from_bytes(vpk.clone())
                    .map_err(|error| anyhow!("the viewing key is not valid: {error}"))?;
                let identity = AccountIdentity::PrivateForeign {
                    npk: NullifierPublicKey(*npk),
                    vpk,
                    kind: PrivateAccountKind::Regular,
                };
                (identity, "those private receiving keys".to_owned())
            }
        })
    }

    /// Checks that `recipient` can take `amount` of `definition`, where its holdings can be read.
    async fn check_recipient(
        &self,
        tokens: &Tokens,
        recipient: &AccountIdentity,
        definition: AccountId,
        amount: u128,
        shown: &str,
    ) -> Result<()> {
        if matches!(recipient, AccountIdentity::PrivateForeign { .. }) {
            return Ok(());
        }
        let held = self.held(tokens, recipient.account_id()).await?;
        check_credit(
            &held,
            &definition,
            amount,
            tokens.codec.max_holdings(),
            shown,
        )
    }

    #[allow(clippy::too_many_arguments)]
    async fn token_send(
        &mut self,
        tokens: &Tokens,
        definition: AccountId,
        amount: u128,
        to: &To,
        from: AccountId,
        public: bool,
        private: bool,
    ) -> Result<(HashType, u64)> {
        let token = self.token_definition(tokens, definition).await?;
        let sender = self.token_identity(from);
        if matches!(sender, AccountIdentity::PublicNoSign(_)) {
            bail!("this wallet does not hold the key of {from}");
        }
        if sender.is_private() {
            self.wallet
                .sync_to_latest_block()
                .await
                .context("cannot sync the private accounts")?;
        }
        let shown_from = account_text(sender.is_private(), from);
        let balance = self
            .held(tokens, from)
            .await?
            .get(&definition)
            .copied()
            .unwrap_or(0);
        check_spend(balance, amount, &shown_from, &token.name)?;
        let (recipient, shown_to) = self.recipient(to, public, private).await?;
        if recipient.account_id() == from {
            bail!("{shown_from} would be sending to itself");
        }
        self.check_recipient(tokens, &recipient, definition, amount, &shown_to)
            .await?;
        println!(
            "sending {amount} {} from {shown_from} to {shown_to}",
            token.name
        );
        let call = tokens
            .codec
            .transfer(definition, from, recipient.account_id(), amount);
        self.token_call(tokens, sender, call, Some(recipient), None)
            .await
    }

    async fn token_receive(
        &mut self,
        tokens: &Tokens,
    ) -> Result<Vec<(HashType, u64, Option<(AccountId, u128)>, AccountId)>> {
        let pending: Vec<_> = self
            .wallet
            .owned_pending_messages()
            .await
            .context("cannot read pending messages")?
            .into_iter()
            .filter(|message| message.body.to.program_account_id == tokens.program)
            .collect();
        if pending.is_empty() {
            println!("nothing to receive");
            return Ok(Vec::new());
        }
        let catalog = self.token_catalog(tokens).await?;
        let mut received = Vec::new();
        for message in pending {
            let credit = tokens.codec.credit(&message.body.message);
            let to = message.body.to.account_id;
            match credit {
                Some((definition, amount)) => {
                    let name = self
                        .token_definition(tokens, definition)
                        .await
                        .map(|found| found.name)
                        .unwrap_or_default();
                    println!(
                        "receiving {amount} {name} ({definition}); proving it on this computer (several minutes)"
                    );
                }
                None => println!(
                    "receiving a token message at position {}; proving it on this computer",
                    message.position
                ),
            }
            let (hash, _) = self
                .wallet
                .receive_pending_message_with(
                    message,
                    Vec::new(),
                    &catalog,
                    None,
                    AsyncDelivery::default(),
                )
                .await
                .context("the sequencer did not accept the receipt")?;
            println!("submitted {hash}");
            let block = self
                .finish(hash)
                .await
                .context("the receipt was not applied")?;
            println!("applied in block {block}");
            received.push((hash, block, credit, to));
        }
        Ok(received)
    }

    async fn token_create(
        &mut self,
        tokens: &Tokens,
        name: &str,
        supply: u128,
        to: AccountId,
        authority: Option<AccountId>,
        payer: AccountId,
    ) -> Result<()> {
        check_token_name(name, tokens.codec.max_name_len())?;
        if self.is_private(payer) || self.wallet.get_account_public_signing_key(payer).is_none() {
            bail!("the payer must be a public account of this wallet");
        }
        let recipient = self.token_identity(to);
        self.check_recipient(
            tokens,
            &recipient,
            AccountId::new([0; 32]),
            supply,
            &to.to_string(),
        )
        .await?;
        let (definition, _) = self.wallet.create_new_account_public(None);
        crate::quietly(|| self.wallet.store_persistent_data())?;
        println!("creating {name} at {definition}, {supply} to {to}");
        let call = tokens.codec.create(definition, name, supply, authority, to);
        self.token_call(
            tokens,
            AccountIdentity::Public(definition),
            call,
            Some(recipient),
            Some(payer),
        )
        .await?;
        println!("token {definition}");
        Ok(())
    }

    async fn token_mint(
        &mut self,
        tokens: &Tokens,
        definition: AccountId,
        amount: u128,
        to: Option<AccountId>,
    ) -> Result<()> {
        let token = self.token_definition(tokens, definition).await?;
        let authority = token
            .authority
            .ok_or_else(|| anyhow!("{} has a fixed supply: nobody may mint more", token.name))?;
        check_mint(&token, authority, amount)?;
        let signer = self.token_identity(authority);
        if matches!(signer, AccountIdentity::PublicNoSign(_)) {
            bail!(
                "this wallet does not hold the key of {authority}, the mint authority of {}",
                token.name
            );
        }
        let to = to.unwrap_or(authority);
        let recipient = self.token_identity(to);
        self.check_recipient(tokens, &recipient, definition, amount, &to.to_string())
            .await?;
        println!("minting {amount} {} to {to}", token.name);
        let call = tokens.codec.mint(definition, authority, to, amount);
        self.token_call(tokens, signer, call, Some(recipient), None)
            .await?;
        Ok(())
    }

    /// Sends `call` with `root` as its signer and `recipient` as the account it credits, and
    /// returns the transaction and its block. A call that touches only public accounts is
    /// tried here first and sent with a gas limit sized from that run, as every `names`
    /// change is; one with a private account, or a pending credit, is proven here.
    async fn token_call(
        &mut self,
        tokens: &Tokens,
        root: AccountIdentity,
        call: Call,
        recipient: Option<AccountIdentity>,
        payer: Option<AccountId>,
    ) -> Result<(HashType, u64)> {
        let (mentions, delivery) = self.token_mentions(tokens, root, &call.roles, recipient)?;
        let proven = mentions.iter().any(|mention| mention.identity.is_private())
            || !delivery.recoveries.is_empty()
            || !delivery.seals.is_empty();
        let hash = if proven {
            if std::env::var_os("LOGOS_NAMES_DRY_RUN").is_some() {
                bail!(
                    "LOGOS_NAMES_DRY_RUN is set, so nothing was sent (a private change is not tried first)"
                );
            }
            let catalog = self.token_catalog(tokens).await?;
            println!(
                "proving it on this computer (several minutes), so the chain never learns the private side"
            );
            let started = std::time::Instant::now();
            let (hash, _) = self
                .wallet
                .send_tx_with_pre_check(
                    mentions,
                    0,
                    call.message,
                    &catalog,
                    None,
                    delivery,
                    payer,
                    |_| Ok(()),
                )
                .await
                .context("the sequencer did not accept the transaction")?;
            println!("proved in {}s", started.elapsed().as_secs());
            hash
        } else {
            let program = self
                .program_at(
                    tokens.program,
                    "NAMES_TOKEN_PROGRAM_ELF",
                    "names-token",
                    "the token program",
                )
                .await?;
            let cycles = self
                .simulate(&mentions, &call.message, vec![(tokens.program, program)])
                .await?;
            let limit = gas_limit_for(cycles);
            let paying: Vec<AccountMention> = match payer {
                Some(payer) => vec![AccountIdentity::Public(payer).balance()],
                None => mentions.clone(),
            };
            self.check_payer(&paying, call.message.len(), 0, limit)
                .await?;
            if std::env::var_os("LOGOS_NAMES_DRY_RUN").is_some() {
                println!("gas: {cycles} cycles, limit {limit}");
                bail!("LOGOS_NAMES_DRY_RUN is set, so nothing was sent");
            }
            let saved = self.wallet.config().clone();
            let mut config = saved.clone();
            config.gas_limit = limit;
            self.wallet.set_config(config);
            let sent = self
                .wallet
                .send_pub_tx_paid_by(mentions, 0, call.message, payer)
                .await;
            self.wallet.set_config(saved);
            sent.context("the sequencer did not accept the transaction")?
        };
        println!("submitted {hash}");
        let block = self
            .finish(hash)
            .await
            .context("the transaction was not applied")?;
        println!("applied in block {block}");
        Ok((hash, block))
    }

    /// The actors `roles` declare, and what the wallet needs to deliver a credit it leaves
    /// pending. As the LEZ wallet does for native transfers (`credit_destination`): a
    /// recipient this wallet can witness takes part now; someone else's private account is
    /// not declared, and the credit is sealed to it (private sender) or bound to a recovery
    /// note (public sender).
    fn token_mentions(
        &self,
        tokens: &Tokens,
        root: AccountIdentity,
        roles: &[Role],
        recipient: Option<AccountIdentity>,
    ) -> Result<(Vec<AccountMention>, AsyncDelivery)> {
        let mut mentions: Vec<AccountMention> = Vec::new();
        let mut delivery = AsyncDelivery::default();
        let sender_private = root.is_private();
        let mut root = Some(root);
        let mut recipient = recipient;
        for role in roles {
            let declared = |id: &AccountId, mentions: &[AccountMention]| {
                mentions
                    .iter()
                    .any(|mention| mention.identity.account_id() == *id)
            };
            match role {
                Role::Signer(id) => {
                    let identity = root
                        .take()
                        .ok_or_else(|| anyhow!("a token call has one signer"))?;
                    if identity.account_id() != *id || !mentions.is_empty() {
                        bail!("the token call's signer must come first and be {id}");
                    }
                    mentions.push(identity.select_program_actor_state(tokens.program));
                }
                Role::Reached(id) => {
                    if !declared(id, &mentions) {
                        mentions.push(
                            AccountIdentity::PublicNoSign(*id)
                                .select_program_actor_state(tokens.program),
                        );
                    }
                }
                Role::Credited(id) => {
                    let identity = recipient
                        .take()
                        .ok_or_else(|| anyhow!("a token call credits one account"))?;
                    if identity.account_id() != *id {
                        bail!("the token call credits {id}, not the recipient given");
                    }
                    if declared(id, &mentions) {
                        continue;
                    }
                    let mention = identity.clone().select_program_actor_state(tokens.program);
                    if matches!(identity, AccountIdentity::PrivateForeign { .. }) {
                        let (_, mut pending) = if sender_private {
                            self.wallet.seal_destination(mention)
                        } else {
                            self.wallet.async_destination(mention)
                        }
                        .map_err(|error| {
                            anyhow!("cannot address the private recipient: {error}")
                        })?;
                        pending.promotions.listed_only = true;
                        delivery = pending;
                    } else {
                        mentions.push(mention.receiving());
                    }
                }
            }
        }
        if mentions.is_empty() {
            bail!("a token call needs a signer");
        }
        Ok((mentions, delivery))
    }
}

fn definition_json(definition: AccountId, found: &Definition) -> serde_json::Value {
    serde_json::json!({
        "definition": definition.to_string(),
        "name": found.name,
        "supply": found.supply.to_string(),
        "authority": found.authority.map(|authority| authority.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(byte: u8) -> AccountId {
        AccountId::new([byte; 32])
    }

    #[test]
    fn the_prototype_codec_writes_the_programs_messages() {
        let codec = PrototypeCodec;
        let transfer = codec.transfer(id(1), id(2), id(3), 250);
        assert_eq!(transfer.message[0], 0, "Transfer is variant 0");
        assert_eq!(
            borsh::from_slice::<token_core::Message>(&transfer.message).unwrap(),
            token_core::Message::Transfer {
                definition: id(1),
                to: id(3),
                amount: 250
            }
        );
        assert_eq!(
            transfer.roles,
            vec![Role::Signer(id(2)), Role::Credited(id(3))]
        );

        let create = codec.create(id(4), "Demo", 1_000_000, Some(id(2)), id(2));
        assert_eq!(create.message[0], 1, "NewFungibleDefinition is variant 1");
        assert_eq!(
            borsh::from_slice::<token_core::Message>(&create.message).unwrap(),
            token_core::Message::NewFungibleDefinition {
                name: "Demo".to_owned(),
                total_supply: 1_000_000,
                mint_authority: Some(id(2)),
                supply_to: id(2),
            }
        );
        assert_eq!(
            create.roles,
            vec![Role::Signer(id(4)), Role::Credited(id(2))]
        );

        let mint = codec.mint(id(1), id(2), id(3), 5);
        assert_eq!(mint.message[0], 2, "Mint is variant 2");
        assert_eq!(
            mint.roles,
            vec![
                Role::Signer(id(2)),
                Role::Reached(id(1)),
                Role::Credited(id(3))
            ]
        );
        // A definition that is its own authority takes the mint itself.
        let own = codec.mint(id(1), id(1), id(3), 5);
        assert_eq!(own.roles, vec![Role::Signer(id(1)), Role::Credited(id(3))]);
    }

    #[test]
    fn the_prototype_codec_reads_holdings_definitions_and_credits() {
        let codec = PrototypeCodec;
        assert!(
            codec.holdings(&[]).unwrap().is_empty(),
            "empty state holds nothing"
        );
        let held: Holdings = [(id(1), 7), (id(2), u128::MAX)].into();
        let state = borsh::to_vec(&token_core::TokenState::Holdings(held.clone())).unwrap();
        assert_eq!(codec.holdings(&state).unwrap(), held);
        assert_eq!(codec.definition(&state), None);

        let definition =
            token_core::TokenState::Definition(token_core::TokenDefinition::Fungible {
                name: "Demo".to_owned(),
                total_supply: 9,
                metadata_id: None,
                authority: Some(id(3)),
            });
        let state = borsh::to_vec(&definition).unwrap();
        assert!(
            codec.holdings(&state).unwrap().is_empty(),
            "a definition holds no balances"
        );
        assert_eq!(
            codec.definition(&state),
            Some(Definition {
                name: "Demo".to_owned(),
                supply: 9,
                authority: Some(id(3))
            })
        );
        assert!(
            codec.holdings(&[9, 9, 9]).is_err(),
            "state the program never writes is refused"
        );

        let credit = borsh::to_vec(&token_core::Message::Credit {
            definition: id(1),
            amount: 200,
        })
        .unwrap();
        assert_eq!(credit[0], 5, "Credit is variant 5");
        assert_eq!(codec.credit(&credit), Some((id(1), 200)));
        assert_eq!(
            codec.credit(&codec.transfer(id(1), id(2), id(3), 1).message),
            None
        );
        assert_eq!(codec.credit(&[0xff]), None);
    }

    #[test]
    fn a_recipient_is_a_name_an_account_or_keys() {
        let account = id(7);
        assert_eq!(
            parse_to(&format!("Public/{account}")),
            Ok(To::Public(account))
        );
        assert_eq!(parse_to(&account.to_string()), Ok(To::Public(account)));
        assert_eq!(
            parse_to(&format!("Private/{account}")),
            Ok(To::Private(account))
        );
        assert_eq!(parse_to("alice"), Ok(To::Name("alice".to_owned())));
        assert_eq!(parse_to("@Alice"), Ok(To::Name("alice".to_owned())));
        // `lns:` is a way of writing a name, not a key pair.
        assert_eq!(parse_to("lns:alice"), Ok(To::Name("alice".to_owned())));
        let keys = format!("{}:{}", "ab".repeat(32), "02".repeat(33));
        assert_eq!(
            parse_to(&keys),
            Ok(To::Keys {
                npk: [0xab; 32],
                vpk: vec![2; 33]
            })
        );
        assert!(
            parse_to(&format!("{}:", "ab".repeat(32))).is_err(),
            "keys need a viewing key"
        );
        assert!(parse_to("Public/nope").is_err());
        assert!(parse_to("not a name!").is_err());
    }

    #[test]
    fn a_sender_is_a_setup_account_or_an_account() {
        assert_eq!(parse_from("public"), Ok(Origin::SetupPublic));
        assert_eq!(parse_from("Private"), Ok(Origin::SetupPrivate));
        assert_eq!(
            parse_from(&format!("Private/{}", id(4))),
            Ok(Origin::Account(id(4)))
        );
        assert!(parse_from("alice").is_err());
    }

    #[test]
    fn amounts_are_positive_and_fit() {
        assert_eq!(parse_amount("250"), Ok(250));
        assert_eq!(parse_amount("1_000,000"), Ok(1_000_000));
        assert_eq!(parse_amount(&u128::MAX.to_string()), Ok(u128::MAX));
        assert!(parse_amount("0").is_err());
        assert!(parse_amount("-1").is_err());
        assert!(parse_amount("1.5").is_err());
        assert!(parse_amount("").is_err());
        assert!(
            parse_amount("340282366920938463463374607431768211456").is_err(),
            "2^128 overflows"
        );
    }

    #[test]
    fn sends_and_credits_are_checked_before_proving() {
        assert!(check_spend(250, 250, "B", "Demo").is_ok());
        assert!(check_spend(250, 251, "B", "Demo").is_err());
        assert!(check_spend(250, 0, "B", "Demo").is_err());

        let held: Holdings = [(id(1), u128::MAX - 1)].into();
        assert!(check_credit(&held, &id(1), 1, Some(32), "B").is_ok());
        assert!(
            check_credit(&held, &id(1), 2, Some(32), "B").is_err(),
            "the balance would overflow"
        );
        let full: Holdings = (0..32).map(|n| (id(n), 1)).collect();
        assert!(
            check_credit(&full, &id(5), 1, Some(32), "B").is_ok(),
            "a kind it holds already fits"
        );
        assert!(
            check_credit(&full, &id(200), 1, Some(32), "B").is_err(),
            "a 33rd kind does not"
        );
        assert!(check_credit(&full, &id(200), 1, None, "B").is_ok());
    }

    #[test]
    fn mints_are_checked_before_sending() {
        let token = Definition {
            name: "Demo".to_owned(),
            supply: u128::MAX - 5,
            authority: Some(id(2)),
        };
        assert!(check_mint(&token, id(2), 5).is_ok());
        assert!(
            check_mint(&token, id(2), 6).is_err(),
            "the supply would overflow"
        );
        assert!(
            check_mint(&token, id(3), 1).is_err(),
            "only the authority mints"
        );
        let fixed = Definition {
            authority: None,
            ..token
        };
        assert!(check_mint(&fixed, id(2), 1).is_err());
        assert!(check_token_name("Demo", 64).is_ok());
        assert!(check_token_name("", 64).is_err());
        assert!(check_token_name(&"x".repeat(65), 64).is_err());
    }

    #[test]
    fn only_known_codecs_are_chosen() {
        assert!(codec("prototype").is_ok());
        assert!(codec("official").is_err());
        assert!(Tokens::select(Some(id(9)), "prototype").is_ok());
    }
}
