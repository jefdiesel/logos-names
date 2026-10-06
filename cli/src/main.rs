//! `names`: command-line client for the Logos Names registry.
//!
//! Signing and fee payment go through the LEZ wallet, so the wallet home
//! (`LEE_WALLET_HOME_DIR`) must hold the key of the account that owns the name.
//!
//! The chain never sees a name: every command derives the name's identifier and
//! key here, and what the name resolves to is sealed and opened on this side.

use std::path::PathBuf;

use anyhow::{Context as _, Result, anyhow, bail};
use clap::{Parser, Subcommand};
use lee_core::{
    account::{AccountId, ProgramShardSelector},
    program::{AccountMeta, PlanInput},
};
use names_core::{
    Effect, Instruction, NAME_SUFFIX, NameId, NameRecord, Page, PrimaryName, normalize_name,
    page_address, record_address, validate_name,
};
use names_seal::{
    Derived, PageContent, Profile, TextRecord, derive, open_page, open_profile, seal_page,
    seal_profile, validate_page, validate_profile, validate_text_key,
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
        #[arg(value_parser = parse_name)]
        name: String,
        /// Public account that will own the name; the wallet must hold its key.
        #[arg(long)]
        owner: AccountId,
    },
    /// Point the name at a public account.
    SetAccount {
        #[arg(value_parser = parse_name)]
        name: String,
        account: AccountId },
    /// Publish the keys needed for private transfers to this name.
    SetKeys {
        #[arg(value_parser = parse_name)]
        name: String,
        /// Nullifier public key, 32 bytes hex.
        #[arg(long)]
        npk: String,
        /// File holding the hex-encoded viewing public key (1184 bytes).
        #[arg(long)]
        vpk_file: PathBuf,
    },
    /// Remove the public account and private keys from the name.
    ClearAddresses {
        #[arg(value_parser = parse_name)]
        name: String,
    },
    /// Set a text entry such as `chat`, `inbox` or `agent`; an empty value removes it.
    SetText {
        #[arg(value_parser = parse_name)]
        name: String,
        key: String,
        value: String,
    },
    /// Give a name up: its record and page are cleared and anyone may register it again.
    Release {
        #[arg(value_parser = parse_name)]
        name: String,
    },
    /// Hand the name to another account. Its addresses, text entries and page are cleared.
    Transfer {
        #[arg(value_parser = parse_name)]
        name: String,
        new_owner: AccountId },
    /// Publish a file as the name's page.
    SetPage {
        #[arg(value_parser = parse_name)]
        name: String,
        file: PathBuf,
        #[arg(long, default_value = "text/html; charset=utf-8")]
        content_type: String,
    },
    /// Remove the name's page.
    ClearPage {
        #[arg(value_parser = parse_name)]
        name: String,
    },
    /// Set the name an account is shown as; an empty name removes it. This
    /// publishes the name on the chain in the clear.
    SetPrimary {
        #[arg(value_parser = parse_name_or_empty)]
        name: String,
        #[arg(long)]
        owner: AccountId,
    },
    /// Send native tokens to a name: to its public account, or with `--private`
    /// to its private receiving keys.
    Pay {
        #[arg(value_parser = parse_name)]
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
    Resolve {
        #[arg(value_parser = parse_name)]
        name: String,
    },
    /// Print the name an account is shown as, checked against the forward record.
    Reverse { account: AccountId },
    /// Print a name's identifier and the accounts that hold its record and page.
    Address {
        #[arg(value_parser = parse_name)]
        name: String,
    },
}

/// Accepts a name as people write it (`jef.lez`, any case) and returns the bare name.
fn parse_name(input: &str) -> Result<String, String> {
    let name = normalize_name(input);
    validate_name(&name).map(|()| name).map_err(str::to_owned)
}

fn parse_name_or_empty(input: &str) -> Result<String, String> {
    if input.trim().is_empty() {
        Ok(String::new())
    } else {
        parse_name(input)
    }
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

    async fn record(&self, id: &NameId) -> Result<Option<NameRecord>> {
        let bytes = self.shard(record_address(&self.program, id)).await?;
        if bytes.is_empty() {
            return Ok(None);
        }
        Ok(Some(
            borsh::from_slice(&bytes).context("the record shard does not hold a name record")?,
        ))
    }

    /// The record of `name` and its opened profile. A record whose sealed bytes are
    /// empty or do not open as this name's has nothing set: a blank profile.
    async fn lookup(&self, name: &str) -> Result<(Derived, NameRecord, Profile)> {
        let derived = derive(name);
        let record = self
            .record(&derived.id)
            .await?
            .ok_or_else(|| anyhow!("{name} is not registered"))?;
        let profile = open_profile(&derived.key, &record.sealed, name).unwrap_or_else(|| Profile {
            name: name.to_owned(),
            ..Profile::default()
        });
        Ok((derived, record, profile))
    }

    /// Registers `name` with a first profile that holds only the name.
    async fn register(&self, name: &str, owner: AccountId) -> Result<()> {
        let Derived { id, key } = derive(name);
        let profile = Profile {
            name: name.to_owned(),
            ..Profile::default()
        };
        let sealed = seal_profile(&key, &profile);
        self.send(
            vec![(record_address(&self.program, &id), false), (owner, true)],
            &Instruction::Register { id, sealed },
        )
        .await
    }

    /// Changes the profile of `name`: reads and opens it, applies `change`, then
    /// seals the whole of it again and replaces the record's sealed bytes.
    async fn edit(&self, name: &str, change: impl FnOnce(&mut Profile) -> Result<()>) -> Result<()> {
        let (Derived { id, key }, record, mut profile) = self.lookup(name).await?;
        change(&mut profile)?;
        validate_profile(&profile).map_err(|reason| anyhow!(reason))?;
        let sealed = seal_profile(&key, &profile);
        self.send(
            vec![
                (record_address(&self.program, &id), false),
                (record.owner, true),
            ],
            &Instruction::SetRecord { id, sealed },
        )
        .await
    }

    /// The usual `[record, page, owner]` accounts of the name `id`.
    fn page_accounts(&self, id: &NameId, owner: AccountId) -> Vec<(AccountId, bool)> {
        vec![
            (record_address(&self.program, id), false),
            (page_address(&self.program, id), false),
            (owner, true),
        ]
    }

    /// Seals `page` as the page of `name`; `None` removes the page.
    async fn set_page(&self, name: &str, page: Option<PageContent>) -> Result<()> {
        let (Derived { id, key }, record, _) = self.lookup(name).await?;
        let sealed = match page {
            Some(page) => {
                validate_page(&page).map_err(|reason| anyhow!(reason))?;
                seal_page(&key, &page)
            }
            None => Vec::new(),
        };
        self.send(
            self.page_accounts(&id, record.owner),
            &Instruction::SetPage { id, sealed },
        )
        .await
    }

    /// Hands `name` to `new_owner`, clearing what the current owner set on it.
    async fn transfer(&self, name: &str, new_owner: AccountId) -> Result<()> {
        let (Derived { id, .. }, record, _) = self.lookup(name).await?;
        self.send(
            self.page_accounts(&id, record.owner),
            &Instruction::Transfer { id, new_owner },
        )
        .await
    }

    /// Releases `name`, emptying its record and page.
    async fn release(&self, name: &str) -> Result<()> {
        let (Derived { id, .. }, record, _) = self.lookup(name).await?;
        self.send(
            self.page_accounts(&id, record.owner),
            &Instruction::Release { id },
        )
        .await
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

    /// Pays `amount` to the public account `name` points at.
    async fn pay(&self, name: &str, amount: u128, from: AccountId) -> Result<()> {
        let (_, _, profile) = self.lookup(name).await?;
        let to = profile
            .account
            .map(AccountId::new)
            .ok_or_else(|| anyhow!("{name} has no public account set"))?;
        let before = self.wallet.get_account_balance(to).await?;

        println!("paying {amount} to {name}{NAME_SUFFIX} ({to})");
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
        let (_, _, profile) = self.lookup(name).await?;
        let npk = profile
            .npk
            .ok_or_else(|| anyhow!("{name} has no private keys set"))?;
        let vpk = lee_core::encryption::ViewingPublicKey::from_bytes(profile.vpk)
            .map_err(|error| anyhow!("{name} publishes an invalid viewing key: {error}"))?;
        let before = self.wallet.get_account_balance(from).await?;

        println!("paying {amount} privately to {name}{NAME_SUFFIX}; generating the proof");
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
        let (Derived { id, key }, record, profile) = self.lookup(name).await?;
        let page_bytes = self.shard(page_address(&self.program, &id)).await?;
        let page = if page_bytes.is_empty() {
            None
        } else {
            let page: Page =
                borsh::from_slice(&page_bytes).context("the page shard does not hold a page")?;
            open_page(&key, &page.sealed)
        };
        let page = page.map_or(serde_json::Value::Null, |page| {
            serde_json::json!({
                "content_type": page.content_type,
                "bytes": page.body.len(),
            })
        });
        let texts: serde_json::Map<String, serde_json::Value> = profile
            .texts
            .into_iter()
            .map(|text| (text.key, text.value.into()))
            .collect();
        Ok(serde_json::json!({
            "name": name,
            "owner": record.owner.to_string(),
            "account": profile.account.map(|account| AccountId::new(account).to_string()),
            "npk": profile.npk.map(hex::encode),
            "vpk": (!profile.vpk.is_empty()).then(|| hex::encode(&profile.vpk)),
            "texts": texts,
            "page": page,
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
        // The claim is whatever the account wrote; only a valid name can be looked up.
        if validate_name(&primary.name).is_err() {
            return Ok(None);
        }
        let Derived { id, key } = derive(&primary.name);
        let confirmed = self.record(&id).await?.is_some_and(|record| {
            record.owner == account
                || open_profile(&key, &record.sealed, &primary.name)
                    .is_some_and(|profile| profile.account == Some(account.into_value()))
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
        let Derived { id, .. } = derive(name);
        println!("id     {}", hex::encode(id));
        println!("record {}", record_address(&program, &id));
        println!("page   {}", page_address(&program, &id));
        return Ok(());
    }

    let client = Client {
        wallet: WalletCore::from_env().await?,
        program,
    };

    match cli.command {
        Command::Register { name, owner } => client.register(&name, owner).await,
        Command::SetAccount { name, account } => {
            client
                .edit(&name, |profile| {
                    profile.account = Some(account.into_value());
                    Ok(())
                })
                .await
        }
        Command::SetKeys {
            name,
            npk,
            vpk_file,
        } => {
            let npk = parse_hex::<32>(&npk, "npk")?;
            let vpk = hex::decode(std::fs::read_to_string(&vpk_file)?.trim())
                .context("the vpk file is not valid hex")?;
            client
                .edit(&name, |profile| {
                    profile.npk = Some(npk);
                    profile.vpk = vpk;
                    Ok(())
                })
                .await
        }
        Command::ClearAddresses { name } => {
            client
                .edit(&name, |profile| {
                    profile.account = None;
                    profile.npk = None;
                    profile.vpk = Vec::new();
                    Ok(())
                })
                .await
        }
        Command::SetText { name, key, value } => {
            validate_text_key(&key).map_err(|reason| anyhow!(reason))?;
            client
                .edit(&name, |profile| {
                    let existing = profile.texts.iter().position(|text| text.key == key);
                    match (existing, value.is_empty()) {
                        (Some(index), true) => {
                            profile.texts.remove(index);
                        }
                        (Some(index), false) => profile.texts[index].value = value,
                        (None, true) => bail!("{name} has no `{key}` entry to remove"),
                        (None, false) => profile.texts.push(TextRecord { key, value }),
                    }
                    Ok(())
                })
                .await
        }
        Command::Transfer { name, new_owner } => client.transfer(&name, new_owner).await,
        Command::Release { name } => client.release(&name).await,
        Command::SetPage {
            name,
            file,
            content_type,
        } => {
            let body = std::fs::read(&file)
                .with_context(|| format!("cannot read {}", file.display()))?;
            client
                .set_page(&name, Some(PageContent { content_type, body }))
                .await
        }
        Command::ClearPage { name } => client.set_page(&name, None).await,
        Command::SetPrimary { name, owner } => {
            if !name.is_empty() {
                eprintln!(
                    "warning: this publishes {name}{NAME_SUFFIX} on the chain in the clear, as \
                     the name of {owner}"
                );
            }
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
                Some(name) => println!("{name}{NAME_SUFFIX}"),
                None => println!("(no confirmed primary name)"),
            }
            Ok(())
        }
        Command::Address { .. } => unreachable!("handled before the wallet is opened"),
    }
}
