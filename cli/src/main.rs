//! `names`: command-line client for the Logos Names registry.
//!
//! Signing and fee payment go through the LEZ wallet, so the wallet home
//! (`LEE_WALLET_HOME_DIR`, default `~/.lee/wallet`) must hold the key of the
//! account that owns the name. `names wallet` is that wallet's own command line,
//! built in, so nothing else needs installing.
//!
//! The chain never sees a name: every command derives the name's identifier and
//! key here, and what the name resolves to is sealed and opened on this side.

use std::path::PathBuf;

use anyhow::{Context as _, Result, anyhow, bail};
use clap::{Parser, Subcommand};
use clock_core::{CLOCK_01_PROGRAM_ACCOUNT_ID, ClockAccountData, clock_account_id};
use lee_core::{
    account::{AccountId, ProgramShardSelector},
    program::{AccountMeta, PlanInput},
};
use names_core::{
    Effect, GRACE_MS, Instruction, NAME_SUFFIX, NameId, NameRecord, Page, PrimaryName, Settings,
    normalize_name, page_address, record_address, settings_address,
    validate_name,
};
use names_seal::{
    Derived, PageContent, Profile, TextRecord, derive, open_page, open_profile, seal_page,
    seal_profile, validate_page, validate_profile, validate_text_key,
};
use wallet::{
    AccountIdentity, WalletCore,
    config::{SequencerConnectionData, WalletConfigOverrides},
    helperfunctions::{fetch_config_path, fetch_persistent_storage_path},
    program_facades::native_token_transfer::NativeTokenTransfer,
};

/// The registry on the Logos Names test chain, a LEZ v0.3 chain run for this
/// registry while the public LEZ testnet is halted. Used unless `--program` or
/// `LOGOS_NAMES_PROGRAM` names another.
const DEFAULT_PROGRAM: &str = "CsiBzfERqtLz9dYV9dPWzFPMbaUcUagYeCmvvJVmCGcF";
/// The test chain's sequencer, reached through the gateway.
const DEFAULT_RPC: &str = "https://logos-names.wrapit.workers.dev/rpc";
/// The test chain's faucet, reached through the gateway.
const FAUCET_URL: &str = "https://logos-names.wrapit.workers.dev/faucet";

#[derive(Parser)]
#[command(
    name = "names",
    version,
    about = "Register and resolve Logos names",
    long_about = "Register and resolve Logos names on the Logos Execution Zone (LEZ).\n\n\
        Looking a name up needs no setup. Anything that changes the chain signs and pays \
        through the LEZ wallet, built in as `names wallet`, whose keys live in \
        LEE_WALLET_HOME_DIR (default ~/.lee/wallet).\n\n\
        Getting started:\n  \
        names resolve jef.lez\n  \
        names wallet account new public\n  \
        names faucet Public/<account>\n  \
        names register <name> --owner <account>\n\n\
        The chain never sees a name: names are turned into identifiers and keys on this \
        machine. Guide: https://logos-names.wrapit.workers.dev/"
)]
struct Cli {
    /// Account ID of the deployed registry program. Defaults to the Names test chain's.
    #[arg(long, env = "LOGOS_NAMES_PROGRAM", global = true)]
    program: Option<AccountId>,

    /// Sequencer to use. Defaults to the Names test chain's, unless `--program` is
    /// given, in which case the wallet's own config decides.
    #[arg(long, env = "LOGOS_NAMES_RPC", global = true)]
    rpc: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Claim a name that is unregistered or has lapsed, paying the registry's price.
    Register {
        #[arg(value_parser = parse_name)]
        name: String,
        /// Public account that will own the name; the wallet must hold its key.
        #[arg(long)]
        owner: AccountId,
        /// How long to hold the name: 30d, 1y or 5y.
        #[arg(long, default_value = "1y", value_parser = parse_term)]
        term: u8,
        /// Claim this time, in milliseconds, instead of reading the chain's clock.
        #[arg(long, hide = true)]
        now: Option<u64>,
    },
    /// Hold a name for longer. Anyone may pay for it.
    Renew {
        #[arg(value_parser = parse_name)]
        name: String,
        /// How long to add: 30d, 1y or 5y.
        #[arg(long, default_value = "1y", value_parser = parse_term)]
        term: u8,
        /// Public account that pays; the wallet must hold its key. Defaults to the owner.
        #[arg(long)]
        payer: Option<AccountId>,
    },
    /// Set the registry's admin, treasury and prices. Anyone may while it has no
    /// settings; afterwards the wallet must hold the admin's key.
    Configure {
        #[arg(long)]
        admin: AccountId,
        /// Account the fees are paid to.
        #[arg(long)]
        treasury: AccountId,
        /// Prices in base units for 30d, 1y and 5y, separated by commas.
        #[arg(long, value_delimiter = ',')]
        prices: Vec<u128>,
        /// Sign with this account instead of the settings' admin.
        #[arg(long, hide = true)]
        signer: Option<AccountId>,
    },
    /// Print the registry's admin, treasury and prices, as JSON.
    Settings,
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
    /// Make a new private account in the wallet and publish its receiving keys on
    /// the name, so payments to the name stay private.
    ReceivePrivately {
        #[arg(value_parser = parse_name)]
        name: String,
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
    /// Fund a public account on the Names test chain, once a day per account.
    Faucet {
        /// The account to fund, such as `Public/<id>` from `names wallet account new public`.
        account: String,
    },
    /// The LEZ wallet's own commands: make accounts, check balances, sync private
    /// accounts and more. Run `names wallet help` for the list.
    #[command(disable_help_flag = true)]
    Wallet {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Write the man pages (`names.1`, `names-register.1`, ...) into a directory.
    #[command(hide = true)]
    Man { dir: PathBuf },
}

/// Renders a page for `names` and one for each command, leaving out this one.
fn write_man_pages(dir: &std::path::Path) -> Result<()> {
    let command = <Cli as clap::CommandFactory>::command();
    std::fs::create_dir_all(dir)?;
    clap_mangen::generate_to(command, dir)?;
    for skipped in ["names-man.1", "names-help.1"] {
        let _ = std::fs::remove_file(dir.join(skipped));
    }
    Ok(())
}

/// Accepts a name as people write it (`jef.lez`, any case) and returns the bare name.
fn parse_name(input: &str) -> Result<String, String> {
    let name = normalize_name(input);
    validate_name(&name).map(|()| name).map_err(str::to_owned)
}

fn parse_term(input: &str) -> Result<u8, String> {
    match input {
        "30d" => Ok(0),
        "1y" => Ok(1),
        "5y" => Ok(2),
        _ => Err("term must be 30d, 1y or 5y".to_owned()),
    }
}

/// This machine's clock, in milliseconds since the Unix epoch.
fn local_now() -> u64 {
    let since_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    u64::try_from(since_epoch.as_millis()).unwrap_or(u64::MAX)
}

/// A time in milliseconds as a UTC date and time, such as `2027-10-06 09:30:00Z`.
fn readable(time: u64) -> String {
    let seconds = time / 1000;
    // Days to a civil date, counting from 0000-03-01 so leap days fall at a year's end.
    let days = seconds / 86_400 + 719_468;
    let (era, day_of_era) = (days / 146_097, days % 146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 { shifted_month + 3 } else { shifted_month - 9 };
    let year = year_of_era + era * 400 + u64::from(month <= 2);
    let (hour, minute, second) = (seconds / 3600 % 24, seconds / 60 % 60, seconds % 60);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}Z")
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

    /// The record of the name `id` as a reader sees it: a name past its grace period
    /// is not registered, whatever its record still holds.
    async fn live_record(&self, id: &NameId) -> Result<Option<NameRecord>> {
        let record = self.record(id).await?;
        Ok(record.filter(|record| local_now() < record.expires.saturating_add(GRACE_MS)))
    }
    async fn settings(&self) -> Result<Option<Settings>> {
        let bytes = self.shard(settings_address(&self.program)).await?;
        if bytes.is_empty() {
            return Ok(None);
        }
        Ok(Some(
            borsh::from_slice(&bytes).context("the settings shard does not hold settings")?,
        ))
    }
    /// The chain's time in milliseconds, from the clock program's account. If that
    /// cannot be read, this machine's clock set back a minute, so that it is not ahead
    /// of the next block.
    async fn now(&self) -> u64 {
        let clock = clock_account_id();
        let selector = ProgramShardSelector::new(CLOCK_01_PROGRAM_ACCOUNT_ID, clock);
        match self.wallet.get_account_view(selector).await {
            Ok(view) => borsh::from_slice::<ClockAccountData>(view.data.shard(clock)).ok(),
            Err(_) => None,
        }
        .map_or_else(|| local_now().saturating_sub(60_000), |data| data.timestamp)
    }
    /// What `payer` owes for `term`, and the accounts that pay it: the payer, then the
    /// treasury when there is a fee. The treasury itself pays nothing.
    async fn price(&self, payer: AccountId, term: u8) -> Result<(u128, Vec<(AccountId, bool)>)> {
        let mut paying = vec![(payer, true)];
        let fee = match self.settings().await? {
            Some(settings) if settings.treasury != payer => {
                let fee = settings.prices[usize::from(term)];
                if fee != 0 {
                    paying.push((settings.treasury, false));
                }
                fee
            }
            _ => 0,
        };
        Ok((fee, paying))
    }
    /// The record of `name` and its opened profile, for reading: a name past its
    /// grace period is not registered.
    async fn lookup(&self, name: &str) -> Result<(Derived, NameRecord, Profile)> {
        let found = self.lookup_held(name).await?;
        if local_now() >= found.1.expires.saturating_add(GRACE_MS) {
            bail!("{name} is not registered");
        }
        Ok(found)
    }
    /// The record of `name` and its opened profile, whether or not the name has
    /// lapsed: its holder may still change it. A record whose sealed bytes are empty
    /// or do not open as this name's has nothing set: a blank profile.
    async fn lookup_held(&self, name: &str) -> Result<(Derived, NameRecord, Profile)> {
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
    async fn register(
        &self,
        name: &str,
        owner: AccountId,
        term: u8,
        now: Option<u64>,
    ) -> Result<()> {
        let Derived { id, key } = derive(name);
        let profile = Profile {
            name: name.to_owned(),
            ..Profile::default()
        };
        let sealed = seal_profile(&key, &profile);
        let (fee, paying) = self.price(owner, term).await?;
        let now = match now {
            Some(now) => now,
            None => self.now().await,
        };
        println!("registering {name}{NAME_SUFFIX} at {now} for a fee of {fee}");
        self.send_paying(
            vec![
                (record_address(&self.program, &id), false),
                (page_address(&self.program, &id), false),
                (settings_address(&self.program), false),
            ],
            &paying,
            &Instruction::Register {
                id,
                sealed,
                term,
                fee,
                now,
            },
        )
        .await
    }
    /// Adds `term` to the time `name` is held for, paid by `payer` or else its owner.
    async fn renew(&self, name: &str, term: u8, payer: Option<AccountId>) -> Result<()> {
        let (Derived { id, .. }, record, _) = self.lookup(name).await?;
        let (fee, paying) = self.price(payer.unwrap_or(record.owner), term).await?;
        let now = self.now().await;
        println!("renewing {name}{NAME_SUFFIX} at {now} for a fee of {fee}");
        self.send_paying(
            vec![
                (record_address(&self.program, &id), false),
                (settings_address(&self.program), false),
            ],
            &paying,
            &Instruction::Renew { id, term, fee, now },
        )
        .await
    }
    /// Sets the registry's settings, signed by their admin or, while there are none,
    /// by the admin being set.
    async fn configure(
        &self,
        admin: AccountId,
        treasury: AccountId,
        prices: [u128; 3],
        signer: Option<AccountId>,
    ) -> Result<()> {
        let signer = match (signer, self.settings().await?) {
            (Some(signer), _) => signer,
            (None, Some(settings)) => settings.admin,
            (None, None) => admin,
        };
        self.send(
            vec![(settings_address(&self.program), false), (signer, true)],
            &Instruction::Configure {
                admin,
                treasury,
                prices,
            },
        )
        .await
    }

    /// Changes the profile of `name`: reads and opens it, applies `change`, then
    /// seals the whole of it again and replaces the record's sealed bytes.
    async fn edit(&self, name: &str, change: impl FnOnce(&mut Profile) -> Result<()>) -> Result<()> {
        let (Derived { id, key }, record, mut profile) = self.lookup_held(name).await?;
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
        let (Derived { id, key }, record, _) = self.lookup_held(name).await?;
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
        let (Derived { id, .. }, record, _) = self.lookup_held(name).await?;
        self.send(
            self.page_accounts(&id, record.owner),
            &Instruction::Transfer { id, new_owner },
        )
        .await
    }

    /// Releases `name`, emptying its record and page.
    async fn release(&self, name: &str) -> Result<()> {
        let (Derived { id, .. }, record, _) = self.lookup_held(name).await?;
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
        paying: &[(AccountId, bool)],
        instruction: &Instruction,
    ) -> Result<Vec<(AccountId, Vec<u8>)>> {
        let input = PlanInput {
            self_account_id: self.program,
            caller_account_id: None,
            accounts: accounts
                .iter()
                .map(|(account, signs)| AccountMeta::new(*account, *signs, self.program))
                .chain(
                    paying
                        .iter()
                        .map(|(account, signs)| AccountMeta::native_balance(*account, *signs)),
                )
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
        self.send_paying(accounts, &[], instruction).await
    }
    /// As [`Self::send`], with the `paying` accounts added after `accounts`. They are
    /// passed with the native token's shard selected, where the program moves the fee.
    async fn send_paying(
        &self,
        accounts: Vec<(AccountId, bool)>,
        paying: &[(AccountId, bool)],
        instruction: &Instruction,
    ) -> Result<()> {
        let expected = self.simulate(&accounts, paying, instruction).await?;
        let identity = |(account, signs): &(AccountId, bool)| {
            if *signs {
                AccountIdentity::Public(*account)
            } else {
                AccountIdentity::PublicNoSign(*account)
            }
        };
        let mentions = accounts
            .iter()
            .map(|row| identity(row).select_program_shard(self.program))
            .chain(paying.iter().map(|row| identity(row).balance()))
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
            "expires": record.expires,
            "expires_date": readable(record.expires),
            "lapsed": local_now() >= record.expires,
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
        let confirmed = self.live_record(&id).await?.is_some_and(|record| {
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

/// Runs the LEZ wallet's command line with `args`, as its own `wallet` binary would.
async fn run_wallet(args: Vec<String>, overrides: Option<WalletConfigOverrides>) -> Result<()> {
    use clap::{CommandFactory as _, Parser as _};
    use wallet::cli::{Args, execute_continuous_run, execute_subcommand};

    let Args { continuous_run, command } =
        Args::parse_from(std::iter::once("names wallet".to_owned()).chain(args));
    write_default_wallet_files()?;
    let config_path = fetch_config_path()?;
    let storage_path = fetch_persistent_storage_path()?;

    if let Some(command) = command {
        let mut wallet = if storage_path.exists() {
            WalletCore::new_update_chain(config_path, storage_path, overrides)?
        } else {
            // The wallet does not encrypt its storage yet, so there is no password to ask for.
            let (wallet, mnemonic) =
                WalletCore::new_init_storage(config_path, storage_path, overrides, "")?;
            println!("Created a new wallet. Write down its recovery phrase and keep it safe;");
            println!("it is the only way to get the wallet back:");
            println!();
            println!("  {mnemonic}");
            println!();
            wallet.store_persistent_data()?;
            wallet
        };
        execute_subcommand(&mut wallet, command).await?;
    } else if continuous_run {
        let mut wallet = WalletCore::new_update_chain(config_path, storage_path, overrides)?;
        execute_continuous_run(&mut wallet).await?;
    } else {
        let mut help = Args::command().name("names wallet");
        println!("{}", help.render_long_help());
    }
    Ok(())
}

/// Writes the wallet's default config if it is missing. The wallet would make it
/// itself, but announces it on stdout, which would spoil the JSON that lookups print
/// there. A config from LEZ v0.3.0, which listed `sequencers` where v0.3.1 has one
/// `sequencer`, is rewritten in the new form, keeping its first sequencer.
fn write_default_wallet_files() -> Result<()> {
    let config_path = fetch_config_path()?;
    if let Some(home) = config_path.parent() {
        std::fs::create_dir_all(home)
            .with_context(|| format!("failed to create the wallet home {}", home.display()))?;
    }
    let mut config = serde_json::to_value(wallet::config::WalletConfig::default())?;
    if config_path.exists() {
        let existing: serde_json::Value = serde_json::from_slice(&std::fs::read(&config_path)?)
            .with_context(|| format!("{} is not JSON", config_path.display()))?;
        let Some(old) = existing.get("sequencers").and_then(|list| list.get(0)) else {
            return Ok(());
        };
        config["sequencer"] = old.clone();
        if let Some(gas_limit) = existing.get("gas_limit") {
            config["gas_limit"] = gas_limit.clone();
        }
    }
    std::fs::write(&config_path, serde_json::to_vec_pretty(&config)?)?;
    Ok(())
}

/// Opens the wallet at the wallet home. Without one, opens an empty wallet that is
/// not saved, so lookups need no setup; anything that signs then fails for want of a key.
async fn open_wallet(overrides: Option<WalletConfigOverrides>) -> Result<WalletCore> {
    write_default_wallet_files()?;
    let storage_path = fetch_persistent_storage_path()?;
    if storage_path.exists() {
        return WalletCore::new_update_chain(fetch_config_path()?, storage_path, overrides);
    }
    let (wallet, _) = WalletCore::new_init_storage(fetch_config_path()?, storage_path, overrides, "")?;
    Ok(wallet)
}

/// The sequencer the wallet should use instead of the one in its config: `--rpc` if
/// given, else the Names test chain's unless `--program` names another registry.
fn sequencer_overrides(cli: &Cli) -> Result<Option<WalletConfigOverrides>> {
    let rpc = match (&cli.rpc, &cli.program) {
        (Some(rpc), _) => rpc.as_str(),
        (None, Some(_)) => return Ok(None),
        (None, None) => DEFAULT_RPC,
    };
    let sequencer_addr = rpc.parse().map_err(|_| anyhow!("{rpc} is not a URL"))?;
    Ok(Some(WalletConfigOverrides {
        sequencer: Some(SequencerConnectionData { sequencer_addr, basic_auth: None }),
        ..Default::default()
    }))
}

/// Asks the Names test chain's faucet to fund `account`.
fn faucet(account: &str) -> Result<()> {
    let account = account.trim().trim_start_matches("Public/");
    let body = serde_json::json!({ "account": account }).to_string();
    let mut response = ureq::post(FAUCET_URL)
        .config()
        .http_status_as_error(false)
        .build()
        .header("Content-Type", "application/json")
        .send(body)
        .context("could not reach the faucet")?;
    let reply: serde_json::Value = serde_json::from_str(&response.body_mut().read_to_string()?)
        .context("the faucet's reply is not JSON")?;
    if let Some(error) = reply["error"].as_str() {
        bail!("faucet: {error}");
    }
    println!(
        "sent {} to Public/{account} in block {}",
        reply["amount"].as_str().unwrap_or("?"),
        reply["block"]
    );
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    if let Command::Man { dir } = &cli.command {
        return write_man_pages(dir);
    }
    let overrides = sequencer_overrides(&cli)?;
    match cli.command {
        Command::Wallet { args } => return run_wallet(args, overrides).await,
        Command::Faucet { account } => return faucet(&account),
        _ => {}
    }
    let program = match cli.program {
        Some(program) => program,
        None => DEFAULT_PROGRAM.parse().map_err(|_| anyhow!("bad built-in program ID"))?,
    };

    if let Command::Address { name } = &cli.command {
        let Derived { id, .. } = derive(name);
        println!("id     {}", hex::encode(id));
        println!("record {}", record_address(&program, &id));
        println!("page   {}", page_address(&program, &id));
        return Ok(());
    }

    let mut client = Client {
        wallet: open_wallet(overrides).await?,
        program,
    };

    match cli.command {
        Command::Register {
            name,
            owner,
            term,
            now,
        } => client.register(&name, owner, term, now).await,
        Command::Renew { name, term, payer } => client.renew(&name, term, payer).await,
        Command::Configure {
            admin,
            treasury,
            prices,
            signer,
        } => {
            let prices = <[u128; 3]>::try_from(prices)
                .map_err(|_| anyhow!("--prices takes three prices: 30d,1y,5y"))?;
            client.configure(admin, treasury, prices, signer).await
        }
        Command::Settings => {
            let settings = client.settings().await?.map(|settings| {
                serde_json::json!({
                    "admin": settings.admin.to_string(),
                    "treasury": settings.treasury.to_string(),
                    "prices": {
                        "30d": settings.prices[0].to_string(),
                        "1y": settings.prices[1].to_string(),
                        "5y": settings.prices[2].to_string(),
                    },
                })
            });
            println!("{}", serde_json::to_string_pretty(&settings)?);
            Ok(())
        }
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
        Command::ReceivePrivately { name } => {
            if !fetch_persistent_storage_path()?.exists() {
                bail!("there is no wallet yet; make one with `names wallet account new public`");
            }
            let (account, _) = client.wallet.create_new_account_private(None);
            let keys = &client
                .wallet
                .storage()
                .key_chain()
                .private_account(account)
                .context("the new private account is missing from the wallet")?
                .key_chain;
            let (npk, vpk) = (keys.nullifier_public_key.0, keys.viewing_public_key.to_bytes().to_vec());
            client.wallet.store_persistent_data()?;
            println!("made private account Private/{account} to receive payments to {name}{NAME_SUFFIX}");
            client
                .edit(&name, |profile| {
                    profile.npk = Some(npk);
                    profile.vpk = vpk;
                    Ok(())
                })
                .await?;
            println!("payments arrive privately; see them with `names wallet account sync-private`");
            Ok(())
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
        Command::Address { .. }
        | Command::Wallet { .. }
        | Command::Faucet { .. }
        | Command::Man { .. } => {
            unreachable!("handled before the wallet is opened")
        }
    }
}
