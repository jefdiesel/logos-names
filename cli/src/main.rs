//! `names`: command-line client for the Logos Names registry.
//!
//! Signing and fee payment go through the LEZ wallet, so the wallet home
//! (`LEE_WALLET_HOME_DIR`) must hold the key of the account that owns the name.

use std::path::PathBuf;

use anyhow::{Context as _, Result, anyhow, bail};
use clap::{Parser, Subcommand};
use lee_core::{
    account::{AccountId, ProgramShardSelector},
    program::{AccountMeta, PlanInput},
};
use names_core::{
    Effect, Instruction, NameRecord, Page, PrimaryName, page_address, record_address,
    validate_name,
};
use wallet::{
    AccountIdentity, WalletCore, program_facades::native_token_transfer::NativeTokenTransfer,
};

#[derive(Parser)]
#[command(name = "names", about = "Register and resolve Logos names")]
struct Cli {
    /// Account ID of the deployed registry program.
    #[arg(long, env = "LOGOS_NAMES_PROGRAM", global = true)]
    program: Option<AccountId>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Claim an unregistered name.
    Register {
        name: String,
        /// Public account that will own the name; the wallet must hold its key.
        #[arg(long)]
        owner: AccountId,
    },
    /// Point the name at a public account.
    SetAccount { name: String, account: AccountId },
    /// Publish the keys needed for private transfers to this name.
    SetKeys {
        name: String,
        /// Nullifier public key, 32 bytes hex.
        #[arg(long)]
        npk: String,
        /// File holding the hex-encoded viewing public key (1184 bytes).
        #[arg(long)]
        vpk_file: PathBuf,
    },
    /// Remove the public account and private keys from the name.
    ClearAddresses { name: String },
    /// Set a text entry such as `chat`, `inbox` or `agent`; an empty value removes it.
    SetText {
        name: String,
        key: String,
        value: String,
    },
    /// Hand the name to another account.
    Transfer { name: String, new_owner: AccountId },
    /// Publish a file as the name's page.
    SetPage {
        name: String,
        file: PathBuf,
        #[arg(long, default_value = "text/html; charset=utf-8")]
        content_type: String,
    },
    /// Remove the name's page.
    ClearPage { name: String },
    /// Set the name an account is shown as; an empty name removes it.
    SetPrimary {
        name: String,
        #[arg(long)]
        owner: AccountId,
    },
    /// Send native tokens to a name: to its public account, or with `--private`
    /// to its private receiving keys.
    Pay {
        name: String,
        /// Amount in base units.
        amount: u128,
        /// Public account to pay from; the wallet must hold its key.
        #[arg(long)]
        from: AccountId,
        /// Pay the name's private keys with a shielded transfer. The amount and
        /// recipient stay off the public record; generating the proof takes a while.
        #[arg(long)]
        private: bool,
    },
    /// Print everything a name resolves to, as JSON.
    Resolve { name: String },
    /// Print the name an account is shown as, checked against the forward record.
    Reverse { account: AccountId },
    /// Print the accounts that hold a name's record and page.
    Address { name: String },
}

struct Client {
    wallet: WalletCore,
    program: AccountId,
}

impl Client {
    async fn shard(&self, account: AccountId) -> Result<Vec<u8>> {
        let view = self
            .wallet
            .get_account_view(ProgramShardSelector::new(account, self.program))
            .await
            .context("failed to read the account from the sequencer")?;
        Ok(view.data.shard(self.program).to_vec())
    }

    async fn record(&self, name: &str) -> Result<Option<NameRecord>> {
        let bytes = self.shard(record_address(&self.program, name)).await?;
        if bytes.is_empty() {
            return Ok(None);
        }
        Ok(Some(
            borsh::from_slice(&bytes).context("the record shard does not hold a name record")?,
        ))
    }

    async fn owner_of(&self, name: &str) -> Result<AccountId> {
        Ok(self
            .record(name)
            .await?
            .ok_or_else(|| anyhow!("{name} is not registered"))?
            .owner)
    }

    /// Runs the program's own `plan` and `apply` against current chain state, returning
    /// the data each touched shard should hold afterwards. A transaction the program
    /// would refuse fails here, with the program's reason, before any fee is paid.
    async fn simulate(
        &self,
        accounts: &[(AccountId, bool)],
        instruction: &Instruction,
    ) -> Result<Vec<(AccountId, Vec<u8>)>> {
        let input = PlanInput {
            self_account_id: self.program,
            caller_account_id: None,
            accounts: accounts
                .iter()
                .map(|(account, signs)| AccountMeta::new(*account, *signs, self.program))
                .collect(),
            instruction_data: borsh::to_vec(instruction)?,
        };
        let effects = refusal_as_error(|| {
            names_core::plan(&input, instruction.clone())
                .output()
                .effects
                .clone()
        })?;

        let mut expected = Vec::with_capacity(effects.len());
        for effect in effects {
            let account = effect.selector.account_id;
            let pre_data = self.shard(account).await?;
            let decoded: Effect = borsh::from_slice(&effect.data)?;
            let post_data = refusal_as_error(|| names_core::apply(decoded, &pre_data))?;
            expected.push((account, post_data.unwrap_or(pre_data)));
        }
        Ok(expected)
    }

    /// Submits `instruction` over `accounts` (each flagged with whether it signs) and
    /// confirms its changes landed. A block includes failed transactions too, so
    /// inclusion alone proves nothing: the touched shards are read back and compared.
    async fn send(&self, accounts: Vec<(AccountId, bool)>, instruction: &Instruction) -> Result<()> {
        let expected = self.simulate(&accounts, instruction).await?;

        let mentions = accounts
            .iter()
            .map(|(account, signs)| {
                let identity = if *signs {
                    AccountIdentity::Public(*account)
                } else {
                    AccountIdentity::PublicNoSign(*account)
                };
                identity.select_program_shard(self.program)
            })
            .collect();
        let hash = self
            .wallet
            .send_pub_tx(mentions, borsh::to_vec(instruction)?, self.program)
            .await
            .context("the sequencer did not accept the transaction")?;
        println!("submitted {hash}");
        let (_, block) = self
            .wallet
            .poll_transaction(hash)
            .await
            .context("the transaction was not included in a block")?;

        for (account, data) in expected {
            if self.shard(account).await? != data {
                bail!(
                    "transaction {hash} was included in block {block} but its changes were not \
                     applied to {account}; it failed on chain or raced with another change"
                );
            }
        }
        println!("applied in block {block}");
        Ok(())
    }

    /// Sends an instruction over the usual `[record, owner]` accounts.
    async fn update(&self, name: &str, owner: AccountId, instruction: &Instruction) -> Result<()> {
        self.send(
            vec![
                (record_address(&self.program, name), false),
                (owner, true),
            ],
            instruction,
        )
        .await
    }

    async fn set_page(&self, name: String, content_type: String, body: Vec<u8>) -> Result<()> {
        let owner = self.owner_of(&name).await?;
        let accounts = vec![
            (record_address(&self.program, &name), false),
            (page_address(&self.program, &name), false),
            (owner, true),
        ];
        self.send(
            accounts,
            &Instruction::SetPage {
                name,
                content_type,
                body,
            },
        )
        .await
    }

    /// Pays `amount` to the public account `name` points at.
    async fn pay(&self, name: &str, amount: u128, from: AccountId) -> Result<()> {
        let record = self
            .record(name)
            .await?
            .ok_or_else(|| anyhow!("{name} is not registered"))?;
        let to = record
            .account
            .ok_or_else(|| anyhow!("{name} has no public account set"))?;
        let before = self.wallet.get_account_balance(to).await?;

        println!("paying {amount} to {name} ({to})");
        let hash = NativeTokenTransfer(&self.wallet)
            .send_public_transfer(
                AccountIdentity::Public(from),
                AccountIdentity::PublicNoSign(to),
                amount,
            )
            .await
            .context("the sequencer did not accept the transfer")?;
        println!("submitted {hash}");
        let (_, block) = self
            .wallet
            .poll_transaction(hash)
            .await
            .context("the transfer was not included in a block")?;

        // A block includes failed transactions too, so confirm the money arrived.
        let after = self.wallet.get_account_balance(to).await?;
        if from != to && after < before.saturating_add(amount) {
            bail!("transfer {hash} was included in block {block} but {name} was not credited");
        }
        println!("paid in block {block}");
        Ok(())
    }

    /// Pays `amount` to the private receiving keys `name` publishes.
    async fn pay_private(&self, name: &str, amount: u128, from: AccountId) -> Result<()> {
        let record = self
            .record(name)
            .await?
            .ok_or_else(|| anyhow!("{name} is not registered"))?;
        let npk = record
            .npk
            .ok_or_else(|| anyhow!("{name} has no private keys set"))?;
        let vpk = lee_core::encryption::ViewingPublicKey::from_bytes(record.vpk)
            .map_err(|error| anyhow!("{name} publishes an invalid viewing key: {error}"))?;
        let before = self.wallet.get_account_balance(from).await?;

        println!("paying {amount} privately to {name}; generating the proof");
        let (hash, _) = NativeTokenTransfer(&self.wallet)
            .send_shielded_transfer_to_outer_account(
                AccountIdentity::Public(from),
                lee_core::NullifierPublicKey(npk),
                vpk,
                // Each payment lands on a fresh private account only the recipient can link.
                wallet::cli::identifier_or_random(None),
                amount,
            )
            .await
            .context("the sequencer did not accept the transfer")?;
        self.wallet.store_persistent_data()?;
        println!("submitted {hash}");
        let (_, block) = self
            .wallet
            .poll_transaction(hash)
            .await
            .context("the transfer was not included in a block")?;

        // The recipient's balance is private, so the sender's debit is all that can be
        // checked here. It is a weak check: a fee alone can exceed a small amount.
        let debited = before.saturating_sub(self.wallet.get_account_balance(from).await?);
        if debited < amount {
            bail!("transfer {hash} was included in block {block} but {from} was not debited");
        }
        println!("included in block {block}; {from} was debited {debited} (amount plus fee)");
        println!("the recipient sees it after syncing their private accounts");
        Ok(())
    }

    async fn resolve(&self, name: &str) -> Result<serde_json::Value> {
        let Some(record) = self.record(name).await? else {
            bail!("{name} is not registered");
        };
        let page_bytes = self.shard(page_address(&self.program, name)).await?;
        let page = if page_bytes.is_empty() {
            serde_json::Value::Null
        } else {
            let page: Page =
                borsh::from_slice(&page_bytes).context("the page shard does not hold a page")?;
            serde_json::json!({
                "content_type": page.content_type,
                "bytes": page.body.len(),
            })
        };
        let texts: serde_json::Map<String, serde_json::Value> = record
            .texts
            .into_iter()
            .map(|text| (text.key, text.value.into()))
            .collect();
        Ok(serde_json::json!({
            "name": record.name,
            "owner": record.owner.to_string(),
            "account": record.account.map(|account| account.to_string()),
            "npk": record.npk.map(hex::encode),
            "vpk": (!record.vpk.is_empty()).then(|| hex::encode(&record.vpk)),
            "texts": texts,
            "page": page,
            "record_address": record_address(&self.program, name).to_string(),
            "page_address": page_address(&self.program, name).to_string(),
        }))
    }

    /// The account's primary name, or `None` unless the forward record points back at it.
    async fn reverse(&self, account: AccountId) -> Result<Option<String>> {
        let bytes = self.shard(account).await?;
        if bytes.is_empty() {
            return Ok(None);
        }
        let primary: PrimaryName =
            borsh::from_slice(&bytes).context("the account's shard does not hold a primary name")?;
        let confirmed = self.record(&primary.name).await?.is_some_and(|record| {
            record.owner == account || record.account == Some(account)
        });
        Ok(confirmed.then_some(primary.name))
    }
}

/// Runs program logic that reports refusal by panicking, turning the panic into an error.
fn refusal_as_error<T>(run: impl FnOnce() -> T) -> Result<T> {
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(run));
    std::panic::set_hook(previous_hook);
    outcome.map_err(|payload| {
        let reason = payload
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| payload.downcast_ref::<&str>().copied())
            .unwrap_or("the program refused the transaction");
        anyhow!("refused: {reason}")
    })
}

fn parse_hex<const N: usize>(text: &str, what: &str) -> Result<[u8; N]> {
    hex::decode(text.trim())
        .with_context(|| format!("{what} is not valid hex"))?
        .try_into()
        .map_err(|_| anyhow!("{what} must be {N} bytes"))
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let program = cli
        .program
        .ok_or_else(|| anyhow!("pass --program or set LOGOS_NAMES_PROGRAM"))?;

    if let Command::Address { name } = &cli.command {
        validate_name(name).map_err(|reason| anyhow!(reason))?;
        println!("record {}", record_address(&program, name));
        println!("page   {}", page_address(&program, name));
        return Ok(());
    }

    let client = Client {
        wallet: WalletCore::from_env().await?,
        program,
    };

    match cli.command {
        Command::Register { name, owner } => {
            validate_name(&name).map_err(|reason| anyhow!(reason))?;
            client
                .update(&name.clone(), owner, &Instruction::Register { name })
                .await
        }
        Command::SetAccount { name, account } => {
            let owner = client.owner_of(&name).await?;
            client
                .update(&name.clone(), owner, &Instruction::SetAccount { name, account })
                .await
        }
        Command::SetKeys {
            name,
            npk,
            vpk_file,
        } => {
            let owner = client.owner_of(&name).await?;
            let npk = parse_hex::<32>(&npk, "npk")?;
            let vpk = hex::decode(std::fs::read_to_string(&vpk_file)?.trim())
                .context("the vpk file is not valid hex")?;
            client
                .update(
                    &name.clone(),
                    owner,
                    &Instruction::SetPrivateKeys { name, npk, vpk },
                )
                .await
        }
        Command::ClearAddresses { name } => {
            let owner = client.owner_of(&name).await?;
            client
                .update(&name.clone(), owner, &Instruction::ClearAddresses { name })
                .await
        }
        Command::SetText { name, key, value } => {
            let owner = client.owner_of(&name).await?;
            client
                .update(
                    &name.clone(),
                    owner,
                    &Instruction::SetText { name, key, value },
                )
                .await
        }
        Command::Transfer { name, new_owner } => {
            let owner = client.owner_of(&name).await?;
            client
                .update(
                    &name.clone(),
                    owner,
                    &Instruction::Transfer { name, new_owner },
                )
                .await
        }
        Command::SetPage {
            name,
            file,
            content_type,
        } => {
            let body = std::fs::read(&file)
                .with_context(|| format!("cannot read {}", file.display()))?;
            client.set_page(name, content_type, body).await
        }
        Command::ClearPage { name } => {
            client
                .set_page(name, "application/octet-stream".to_owned(), Vec::new())
                .await
        }
        Command::SetPrimary { name, owner } => {
            client
                .send(vec![(owner, true)], &Instruction::SetPrimary { name })
                .await
        }
        Command::Pay {
            name,
            amount,
            from,
            private,
        } => {
            if private {
                client.pay_private(&name, amount, from).await
            } else {
                client.pay(&name, amount, from).await
            }
        }
        Command::Resolve { name } => {
            println!("{}", serde_json::to_string_pretty(&client.resolve(&name).await?)?);
            Ok(())
        }
        Command::Reverse { account } => {
            match client.reverse(account).await? {
                Some(name) => println!("{name}"),
                None => println!("(no confirmed primary name)"),
            }
            Ok(())
        }
        Command::Address { .. } => unreachable!("handled before the wallet is opened"),
    }
}
