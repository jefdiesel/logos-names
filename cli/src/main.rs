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
    program::{
        AccountMeta, MAX_PROGRAM_SEGMENTS, PROGRAM_LOADER_ACCOUNT_ID, PlanInput, ProgramHeader,
        ProgramSegment,
    },
};
use lee::{privacy_preserving_transaction::circuit::ProgramWithDependencies, program::Program};
use names_core::{
    Effect, GRACE_MS, Instruction, MAX_COMMIT_AGE_MS, MIN_COMMIT_AGE_MS, NAME_SUFFIX, TIME_WINDOW_MS, NameId,
    NameRecord, Page, Settings, commitment, commitment_address,
    normalize_name, owner_tag, page_address, record_address, settings_address,
    validate_name,
};
use names_seal::{
    Derived, PageContent, Profile, TextRecord, derive, open_page, open_profile, seal_page,
    seal_profile, validate_page, validate_profile, validate_text_key,
};
use wallet::{
    AccDecodeData, AccountIdentity, WalletCore,
    config::{SequencerConnectionData, WalletConfigOverrides},
    helperfunctions::{fetch_config_path, fetch_persistent_storage_path},
    program_facades::native_token_transfer::NativeTokenTransfer,
};

/// This wallet's own accounts and names, kept next to the wallet so commands don't
/// need account IDs. Written by `names setup` and `names register`.
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Defaults {
    public: Option<String>,
    private: Option<String>,
    #[serde(default)]
    names: Vec<String>,
    /// Commitments made but not yet revealed, by name, so an interrupted
    /// `register` picks up where it stopped.
    #[serde(default)]
    pending: std::collections::BTreeMap<String, Pending>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Pending {
    owner: String,
    secret: String,
    made: u64,
    /// Why the last reveal gave up, so `names mine` can say so.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    failed: Option<String>,
}

fn defaults_path() -> Result<PathBuf> {
    Ok(fetch_persistent_storage_path()?.with_file_name("names.json"))
}

fn load_defaults() -> Defaults {
    defaults_path()
        .ok()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn save_defaults(defaults: &Defaults) -> Result<()> {
    std::fs::write(defaults_path()?, serde_json::to_vec_pretty(defaults)?)?;
    Ok(())
}

fn register_log(name: &str) -> Result<PathBuf> {
    Ok(fetch_persistent_storage_path()?.with_file_name(format!("names-register-{name}.log")))
}

fn parse_hex32(text: &str) -> Option<[u8; 32]> {
    hex::decode(text).ok()?.try_into().ok()
}

/// The account a command uses when none is given: the setup's private account.
fn default_private() -> Result<AccountId> {
    load_defaults()
        .private
        .and_then(|id| id.parse().ok())
        .ok_or_else(|| anyhow!("no account given and none set up; run `names setup` first, or pass one"))
}

/// The text entry linking a name to a Logos account (an AccountLog address).
const LOGOS_ACCOUNT_KEY: &str = "logos.account";

/// The registry on the public LEZ testnet (v4, deployed 2026-10-16). Used unless
/// `--program` or `LOGOS_NAMES_PROGRAM` names another.
const DEFAULT_PROGRAM: &str = "2PXg3SSmh5hvECTz4xcfV2d5YvxwyF6yVhpnjTokzCkX";
/// The public LEZ testnet's sequencer, reached directly: no server of ours sits between a
/// user and the chain.
const DEFAULT_RPC: &str = "https://testnet.lez.logos.co";
/// The Names faucet for the public LEZ testnet, reached through the gateway.
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
        Getting started, holding a name anonymously:\n  \
        names resolve jef.lez\n  \
        names wallet account new public            (pays to get started)\n  \
        names faucet Public/<public>               (or: names bridge <amount> --to Public/<public> --from-key <key>)\n  \
        names wallet account new private           (will hold the name)\n  \
        names wallet auth-transfer send --from Public/<public> --to Private/<private> --amount 1000000\n  \
        names register <name> --owner Private/<private>\n\n\
        The chain never sees a name, and never sees a private holder: names are turned \
        into identifiers and keys on this machine, and a private holder's changes are \
        proven here. Guide: https://logos-names.wrapit.workers.dev/"
)]
struct Cli {
    /// Account ID of the deployed registry program. Defaults to the one on the public LEZ testnet.
    #[arg(long, env = "LOGOS_NAMES_PROGRAM", global = true)]
    program: Option<AccountId>,

    /// Sequencer to use. Defaults to the public LEZ testnet, unless `--program` is
    /// given, in which case the wallet's own config decides.
    #[arg(long, env = "LOGOS_NAMES_RPC", global = true)]
    rpc: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Set up this machine: a wallet with a public and a private account, funded from
    /// the faucet, with some of it moved to the private account. Run it once; after
    /// it, `register` and `pay` need no account arguments.
    Setup {
        /// How much of the faucet's payment to move to the private account.
        #[arg(long, default_value_t = 2_000_000)]
        shield: u128,
    },
    /// Show this wallet's accounts and balances.
    Balance,
    /// List the names this wallet holds.
    Mine,
    /// Claim a name that is unregistered or has lapsed, paying the registry's price.
    Register {
        #[arg(value_parser = parse_name)]
        name: String,
        /// Account that will own the name and pay for it; the wallet must hold its
        /// key. Defaults to the private account from `names setup`, which never
        /// appears on chain; each change then takes a proof of a few minutes.
        #[arg(long, value_parser = parse_account)]
        owner: Option<AccountId>,
        /// How long to hold the name: 30d, 1y or 5y.
        #[arg(long, default_value = "1y", value_parser = parse_term)]
        term: u8,
        /// Claim this time, in milliseconds, instead of reading the chain's clock.
        #[arg(long, hide = true)]
        now: Option<u64>,
        /// Stay until the name is registered. Without it, `register` returns once
        /// the commitment is made and finishes in the background (`names mine`
        /// shows when it's done).
        #[arg(long)]
        wait: bool,
    },
    /// Hold a name for longer. Anyone may pay for it.
    Renew {
        #[arg(value_parser = parse_name)]
        name: String,
        /// How long to add: 30d, 1y or 5y.
        #[arg(long, default_value = "1y", value_parser = parse_term)]
        term: u8,
        /// Account that pays; the wallet must hold its key. Defaults to the owner.
        #[arg(long, value_parser = parse_account)]
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
    /// Publish a public account on the name, so it can also be paid openly (a shop,
    /// donations). Anyone who resolves the name then sees that account, and so its
    /// balance and history. Private payments keep working.
    SetAccount {
        #[arg(value_parser = parse_name)]
        name: String,
        #[arg(value_parser = parse_account)]
        account: AccountId,
    },
    /// Remove the public account from the name; it stays payable privately.
    ClearAccount {
        #[arg(value_parser = parse_name)]
        name: String,
    },
    /// Give the name fresh private receiving keys, from a new private account in this
    /// wallet. Registering does this already; use it after a transfer, or to rotate.
    ReceivePrivately {
        #[arg(value_parser = parse_name)]
        name: String,
    },
    /// Link the name to a Logos account (an AccountLog address: the account's Ed25519
    /// public key, 64 lowercase hex characters). Anyone who resolves the name sees it.
    /// The link is confirmed only when the account's log endorses the name too: see
    /// `account-log-entry` and `verify-account`. An empty address removes it.
    SetLogosAccount {
        #[arg(value_parser = parse_name)]
        name: String,
        address: String,
    },
    /// Print the AccountLog entry (hex) by which a Logos account endorses this name, for
    /// the account's owner to append to their log and sign.
    AccountLogEntry {
        #[arg(value_parser = parse_name)]
        name: String,
    },
    /// Check that the name and a Logos account point at each other: the name's
    /// `logos.account` is the address the signed log verifies under, and the log's live
    /// entries endorse the name. Prints the account's live keys and records as JSON.
    VerifyAccount {
        #[arg(value_parser = parse_name)]
        name: String,
        /// The account's signed log (signature, then payload), as raw bytes or hex.
        #[arg(long)]
        log: PathBuf,
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
    /// Hand the name to another account. Its receiving keys, text entries and page are cleared; the new holder runs `receive-privately`.
    Transfer {
        #[arg(value_parser = parse_name)]
        name: String,
        /// The new holder, public or private. Only a tag of it reaches the chain.
        #[arg(value_parser = parse_account)]
        new_owner: AccountId },
    /// Show a name's page in your browser. It is read from the chain on this machine
    /// and served only to this machine (127.0.0.1); no server sees which name you
    /// opened. The page cannot load anything from the internet.
    Open {
        #[arg(value_parser = parse_name)]
        name: String,
        /// Port on 127.0.0.1 to serve it from (0: any free port).
        #[arg(long, default_value_t = 0)]
        port: u16,
        /// Print the address instead of opening the browser.
        #[arg(long)]
        no_browser: bool,
    },
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
    /// Pay a name. By default the payment goes to the name's private receiving keys:
    /// nobody, the payer included, learns the account behind the name, and the chain
    /// shows neither recipient nor amount; proving it takes a few minutes. With
    /// `--public`, or when the name has only a public account, it is an ordinary
    /// public transfer to that account.
    Pay {
        #[arg(value_parser = parse_name)]
        name: String,
        /// Amount in base units.
        amount: u128,
        /// Account to pay from; the wallet must hold its key. Defaults to the private
        /// account from `names setup`, which hides the payer too; a public one is seen
        /// paying.
        #[arg(long, value_parser = parse_account)]
        from: Option<AccountId>,
        /// Pay the name's public account instead of its private keys.
        #[arg(long, conflicts_with = "private")]
        public: bool,
        /// Pay the name's private keys (the default when it has them).
        #[arg(long)]
        private: bool,
    },
    /// Print everything a name resolves to, as JSON.
    Resolve {
        #[arg(value_parser = parse_name)]
        name: String,
    },
    /// Print a name's identifier and the accounts that hold its record and page.
    Address {
        #[arg(value_parser = parse_name)]
        name: String,
    },
    /// Fund a public account on the public LEZ testnet, once a day per account.
    Faucet {
        /// The account to fund, such as `Public/<id>` from `names wallet account new public`.
        account: String,
    },
    /// Move tokens from the Logos Blockchain into a LEZ public account, through your
    /// own Logos Blockchain node (Basecamp's, or a headless one). The node spends whole
    /// notes of `--from-key`, so slightly more than `amount` may move. LEZ credits it
    /// once the Logos Blockchain block is final. Without `--yes` it only shows the plan.
    Bridge {
        /// How much to move, in base units; whole notes are used, so it may be a little more.
        amount: u128,
        /// The LEZ public account to credit, such as `Public/<id>`.
        #[arg(long)]
        to: String,
        /// The Logos Blockchain public key (64 hex characters) whose notes pay, as the
        /// node's wallet lists it.
        #[arg(long)]
        from_key: String,
        /// Your Logos Blockchain node's HTTP API (Basecamp: 127.0.0.1:8080).
        #[arg(long, default_value = "http://127.0.0.1:8080")]
        node: String,
        /// The most the Logos Blockchain transaction may cost in fees.
        #[arg(long, default_value_t = 100_000)]
        max_fee: u64,
        /// Send it. Deposits cannot be undone.
        #[arg(long)]
        yes: bool,
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

/// Accepts an account as the wallet prints it (`Public/<id>`, `Private/<id>`) or bare.
fn parse_account(input: &str) -> Result<AccountId, String> {
    let bare = input
        .strip_prefix("Public/")
        .or_else(|| input.strip_prefix("Private/"))
        .unwrap_or(input);
    bare.parse()
        .map_err(|_| format!("not an account ID: {input}"))
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

struct Client {
    wallet: WalletCore,
    program: AccountId,
}

impl Client {
    /// The registry's bytecode, read from the chain itself: the program loader keeps it in a
    /// chain of segment accounts behind the program's header. A private holder's client runs
    /// the program inside its proof, so it needs the exact bytecode the chain has; the result
    /// is checked against the header's image ID, and cached next to the wallet.
    /// `LOGOS_NAMES_PROGRAM_ELF` overrides it with a local file.
    async fn program(&self) -> Result<Program> {
        if let Some(path) = std::env::var_os("LOGOS_NAMES_PROGRAM_ELF") {
            let bytes = std::fs::read(&path)
                .with_context(|| format!("cannot read {}", PathBuf::from(path).display()))?;
            return Program::new(bytes.into())
                .map_err(|error| anyhow!("the registry's bytecode does not load: {error}"));
        }
        let header = ProgramHeader::from_bytes(&self.loader_shard(self.program).await?)
            .ok_or_else(|| anyhow!("{} is not a deployed program", self.program))?;
        let cache = fetch_persistent_storage_path()?
            .with_file_name(format!("logos-names-{}.bin", self.program));
        if let Ok(bytes) = std::fs::read(&cache) {
            if let Ok(program) = Program::new(bytes.into()) {
                if program.id() == header.image_id {
                    return Ok(program);
                }
            }
        }
        println!("reading the registry's bytecode from the chain");
        let mut user_elf = Vec::new();
        let mut next = Some(header.program_first_segment);
        for _ in 0..MAX_PROGRAM_SEGMENTS {
            let Some(segment) = next else { break };
            let segment = ProgramSegment::from_bytes(&self.loader_shard(segment).await?)
                .ok_or_else(|| anyhow!("program segment {segment} does not decode"))?;
            user_elf.extend_from_slice(&segment.bytecode);
            next = segment.next_segment;
        }
        if next.is_some() {
            bail!("the program's segment chain is longer than {MAX_PROGRAM_SEGMENTS}");
        }
        // The loader stores only the user ELF; every LEZ program runs on the same kernel.
        let bytes =
            risc0_binfmt::ProgramBinary::new(&user_elf, risc0_zkos_v1compat::V1COMPAT_ELF).encode();
        let program = Program::new(bytes.clone().into())
            .map_err(|error| anyhow!("the registry's bytecode does not load: {error}"))?;
        if program.id() != header.image_id {
            bail!("the bytecode read from the chain does not match the program's image ID");
        }
        std::fs::write(&cache, &bytes).ok();
        Ok(program)
    }

    async fn loader_shard(&self, account: AccountId) -> Result<Vec<u8>> {
        let view = self
            .wallet
            .get_account_view(ProgramShardSelector::new(account, PROGRAM_LOADER_ACCOUNT_ID))
            .await
            .context("failed to read the program from the sequencer")?;
        Ok(view.data.shard(PROGRAM_LOADER_ACCOUNT_ID).to_vec())
    }

    /// The wallet's account that holds `record` as the name `id`, public or private.
    /// The chain stores only the holder's tag, so it is found by trying each one.
    fn holder(&self, name: &str, id: &NameId, record: &NameRecord) -> Result<AccountId> {
        let key_chain = self.wallet.storage().key_chain();
        key_chain
            .public_account_ids()
            .map(|(account, _)| account)
            .chain(key_chain.private_account_ids().map(|(account, _)| account))
            .find(|account| owner_tag(id, account) == record.owner)
            .ok_or_else(|| anyhow!("this wallet does not hold {name}{NAME_SUFFIX}"))
    }

    fn is_private(&self, account: AccountId) -> bool {
        self.wallet.resolve_private_account(account).is_some()
    }

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

    /// A new private account in the wallet, and the keys a payer needs to pay it
    /// privately: its nullifier and viewing public keys.
    fn new_receiving_keys(&mut self) -> Result<(AccountId, [u8; 32], Vec<u8>)> {
        if !fetch_persistent_storage_path()?.exists() {
            bail!("there is no wallet yet; make one with `names wallet account new public`");
        }
        let (account, _) = self.wallet.create_new_account_private(None);
        let keys = &self
            .wallet
            .storage()
            .key_chain()
            .private_account(account)
            .context("the new private account is missing from the wallet")?
            .key_chain;
        let (npk, vpk) = (keys.nullifier_public_key.0, keys.viewing_public_key.to_bytes().to_vec());
        self.wallet.store_persistent_data()?;
        Ok((account, npk, vpk))
    }

    /// Registers `name` with a first profile that holds the name and its receiving keys.
    /// Registers `name`: commit, wait, reveal. Without `wait` it returns `false` once
    /// the commitment is made, leaving the rest to a background `names register`.
    async fn register(
        &mut self,
        name: &str,
        owner: AccountId,
        term: u8,
        now: Option<u64>,
        wait: bool,
        rpc: Option<&str>,
    ) -> Result<bool> {
        let Derived { id, key } = derive(name);
        // Check first, so a taken name isn't given a receiving account it won't use.
        // The program refuses it anyway; this only makes the refusal come sooner.
        if let Some(held) = self.record(&id).await? {
            if held.owner == owner_tag(&id, &owner) && load_defaults().pending.contains_key(name) {
                // An earlier reveal landed after `register` stopped waiting for it.
                let mut defaults = load_defaults();
                defaults.pending.remove(name);
                save_defaults(&defaults)?;
                println!("{name}{NAME_SUFFIX} is registered");
                return Ok(true);
            }
            if local_now() < held.expires.saturating_add(GRACE_MS) {
                bail!("{name}{NAME_SUFFIX} is already registered");
            }
        }
        let (fee, paying) = self.price(owner, term).await?;
        let holder = owner_tag(&id, &owner);

        // Step 1: commit. The commitment hides the identifier, so watching
        // pending transactions doesn't let anyone claim the name first.
        let mut defaults = load_defaults();
        let resumed = defaults
            .pending
            .get(name)
            .filter(|p| p.owner == owner.to_string() && local_now() < p.made.saturating_add(MAX_COMMIT_AGE_MS))
            .and_then(|p| Some((parse_hex32(&p.secret)?, p.made)));
        let (secret, made) = match resumed {
            Some(found) => {
                println!("resuming the commitment to {name}{NAME_SUFFIX} made earlier");
                if let Some(pending) = defaults.pending.get_mut(name) {
                    pending.failed = None;
                }
                save_defaults(&defaults)?;
                found
            }
            None => {
                let mut secret = [0u8; 32];
                getrandom::fill(&mut secret).map_err(|e| anyhow!("no randomness: {e}"))?;
                let made = self.now().await;
                let promised = commitment(&id, &holder, &secret);
                println!("step 1 of 2: committing to {name}{NAME_SUFFIX}");
                self.send(
                    vec![(commitment_address(&self.program, &promised), false), (owner, true)],
                    &Instruction::Commit { commitment: promised, now: made },
                )
                .await?;
                defaults.pending.insert(
                    name.to_owned(),
                    Pending { owner: owner.to_string(), secret: hex::encode(secret), made, failed: None },
                );
                save_defaults(&defaults)?;
                (secret, made)
            }
        };

        // Step 2: reveal, once the commitment is old enough.
        let ready = made.saturating_add(MIN_COMMIT_AGE_MS);
        if !wait {
            let log = self.finish_in_background(name, owner, term, rpc)?;
            println!(
                "{name}{NAME_SUFFIX} is committed; it registers in the background from {} (about {} minutes)",
                readable(ready),
                MIN_COMMIT_AGE_MS / 60_000 + 5
            );
            println!("check with `names mine`; the log is {}", log.display());
            return Ok(false);
        }
        loop {
            let chain_now = self.now().await;
            if chain_now >= ready {
                break;
            }
            let wait = (ready - chain_now).div_ceil(1000);
            println!("waiting {wait}s before revealing (the commitment must be {} minutes old)", MIN_COMMIT_AGE_MS / 60_000);
            tokio::time::sleep(std::time::Duration::from_secs(wait.clamp(5, 30))).await;
        }
        // Every name can be paid from the start: the profile carries receiving
        // keys of a fresh private account.
        let (receiving, npk, vpk) = self.new_receiving_keys()?;
        let profile = Profile {
            name: name.to_owned(),
            npk: Some(npk),
            vpk,
            ..Profile::default()
        };
        let sealed = seal_profile(&key, &profile);
        println!("payments to {name}{NAME_SUFFIX} will arrive privately at Private/{receiving}");
        let promised = commitment(&id, &holder, &secret);
        // A reveal claims the chain's time before its proof is made, and the program
        // takes it only within TIME_WINDOW_MS of that; a slow chain can include it too
        // late. The commitment stays usable for MAX_COMMIT_AGE_MS, so try again.
        let expires_at = made.saturating_add(MAX_COMMIT_AGE_MS);
        let mut attempt = 0;
        loop {
            attempt += 1;
            let claimed = match now {
                Some(now) => now,
                None => self.now().await,
            };
            println!("step 2 of 2: registering {name}{NAME_SUFFIX} at {claimed} for a fee of {fee}");
            let sent = self
                .send_paying(
                    vec![
                        (record_address(&self.program, &id), false),
                        (page_address(&self.program, &id), false),
                        (settings_address(&self.program), false),
                        (commitment_address(&self.program, &promised), false),
                    ],
                    &paying,
                    &Instruction::Register {
                        id,
                        sealed: sealed.clone(),
                        term,
                        fee,
                        now: claimed,
                        secret,
                    },
                )
                .await;
            let Err(error) = sent else { break };
            // The wait may have given up on a reveal that landed after all.
            match self.record(&id).await {
                Ok(Some(held)) if held.owner == holder => break,
                Ok(Some(_)) => return Err(self.give_up(name, error.context("someone else holds the name"))),
                _ => {}
            }
            if attempt >= REVEAL_ATTEMPTS || self.now().await.saturating_add(TIME_WINDOW_MS) >= expires_at {
                return Err(self.give_up(name, error));
            }
            println!("the reveal did not land ({error:#}); trying again (attempt {} of {REVEAL_ATTEMPTS})", attempt + 1);
        }
        let mut defaults = load_defaults();
        defaults.pending.remove(name);
        save_defaults(&defaults)?;
        println!("{name}{NAME_SUFFIX} is registered");
        Ok(true)
    }

    /// Records why registering `name` stopped, for `names mine`, and returns the error.
    fn give_up(&self, name: &str, error: anyhow::Error) -> anyhow::Error {
        let mut defaults = load_defaults();
        if let Some(pending) = defaults.pending.get_mut(name) {
            pending.failed = Some(format!("{error:#}"));
            let _ = save_defaults(&defaults);
        }
        error.context(format!("{name}{NAME_SUFFIX} was not registered; `names register {name}` tries again"))
    }

    /// Starts `names register <name> --wait` detached from this terminal, logging to
    /// a file next to the wallet. It resumes the saved commitment.
    fn finish_in_background(&self, name: &str, owner: AccountId, term: u8, rpc: Option<&str>) -> Result<PathBuf> {
        let log = register_log(name)?;
        let file = std::fs::File::create(&log).with_context(|| format!("cannot write {}", log.display()))?;
        let term = ["30d", "1y", "5y"][usize::from(term)];
        let mut command = std::process::Command::new(std::env::current_exe()?);
        command
            .args(["register", name, "--owner", &owner.to_string(), "--term", term, "--wait"])
            .env("LOGOS_NAMES_PROGRAM", self.program.to_string())
            .stdin(std::process::Stdio::null())
            .stdout(file.try_clone()?)
            .stderr(file);
        if let Some(rpc) = rpc {
            command.env("LOGOS_NAMES_RPC", rpc);
        }
        #[cfg(unix)]
        {
            // Its own process group, so closing the terminal doesn't stop it.
            use std::os::unix::process::CommandExt as _;
            command.process_group(0);
        }
        command.spawn().context("cannot start the background registration")?;
        Ok(log)
    }
    /// Adds `term` to the time `name` is held for, paid by `payer` or else its owner.
    async fn renew(&mut self, name: &str, term: u8, payer: Option<AccountId>) -> Result<()> {
        let (Derived { id, .. }, record, _) = self.lookup(name).await?;
        let payer = match payer {
            Some(payer) => payer,
            None => self.holder(name, &id, &record)?,
        };
        let (fee, paying) = self.price(payer, term).await?;
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
    /// by the program's own account, whose key its deployer holds.
    async fn configure(
        &mut self,
        admin: AccountId,
        treasury: AccountId,
        prices: [u128; 3],
        signer: Option<AccountId>,
    ) -> Result<()> {
        let signer = match (signer, self.settings().await?) {
            (Some(signer), _) => signer,
            (None, Some(settings)) => settings.admin,
            (None, None) => self.program,
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
    async fn edit(&mut self, name: &str, change: impl FnOnce(&mut Profile) -> Result<()>) -> Result<()> {
        let (Derived { id, key }, record, mut profile) = self.lookup_held(name).await?;
        let holder = self.holder(name, &id, &record)?;
        change(&mut profile)?;
        validate_profile(&profile).map_err(|reason| anyhow!(reason))?;
        let sealed = seal_profile(&key, &profile);
        self.send(
            vec![
                (record_address(&self.program, &id), false),
                (holder, true),
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
    async fn set_page(&mut self, name: &str, page: Option<PageContent>) -> Result<()> {
        let (Derived { id, key }, record, _) = self.lookup_held(name).await?;
        let holder = self.holder(name, &id, &record)?;
        let sealed = match page {
            Some(page) => {
                validate_page(&page).map_err(|reason| anyhow!(reason))?;
                seal_page(&key, &page)
            }
            None => Vec::new(),
        };
        self.send(
            self.page_accounts(&id, holder),
            &Instruction::SetPage { id, sealed },
        )
        .await
    }

    /// Hands `name` to `new_owner`, clearing what the current owner set on it.
    async fn transfer(&mut self, name: &str, new_owner: AccountId) -> Result<()> {
        let (Derived { id, .. }, record, _) = self.lookup_held(name).await?;
        let holder = self.holder(name, &id, &record)?;
        self.send(
            self.page_accounts(&id, holder),
            &Instruction::Transfer {
                id,
                new_owner: owner_tag(&id, &new_owner),
            },
        )
        .await
    }

    /// Releases `name`, emptying its record and page.
    async fn release(&mut self, name: &str) -> Result<()> {
        let (Derived { id, .. }, record, _) = self.lookup_held(name).await?;
        let holder = self.holder(name, &id, &record)?;
        self.send(
            self.page_accounts(&id, holder),
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
    async fn send(&mut self, accounts: Vec<(AccountId, bool)>, instruction: &Instruction) -> Result<()> {
        self.send_paying(accounts, &[], instruction).await
    }
    /// As [`Self::send`], with the `paying` accounts added after `accounts`. They are
    /// passed with the native token's shard selected, where the program moves the fee.
    async fn send_paying(
        &mut self,
        accounts: Vec<(AccountId, bool)>,
        paying: &[(AccountId, bool)],
        instruction: &Instruction,
    ) -> Result<()> {
        let expected = self.simulate(&accounts, paying, instruction).await?;
        // A private signer makes the whole transaction privacy-preserving: the
        // program runs inside a proof made here, and the chain sees only its
        // effects on the public accounts, never the signer.
        let private = accounts
            .iter()
            .chain(paying)
            .any(|(account, signs)| *signs && self.is_private(*account));
        let identity = |(account, signs): &(AccountId, bool)| {
            if !*signs {
                AccountIdentity::PublicNoSign(*account)
            } else if private && let Some(owned) = self.wallet.resolve_private_account(*account) {
                owned
            } else {
                AccountIdentity::Public(*account)
            }
        };
        let mentions = accounts
            .iter()
            .map(|row| identity(row).select_program_shard(self.program))
            .chain(paying.iter().map(|row| identity(row).balance()))
            .collect();
        // The private accounts in the order the transaction carries their notes.
        let private_accounts: Vec<AccountId> = accounts
            .iter()
            .chain(paying)
            .filter(|(account, signs)| private && *signs && self.is_private(*account))
            .map(|(account, _)| *account)
            .collect();
        let mut secrets = Vec::new();
        let hash = if private {
            let program = self.program().await?;
            let program = ProgramWithDependencies::new(program, self.program, Default::default());
            println!("acting privately; generating the proof (this takes minutes)");
            let (hash, shared) = self
                .wallet
                .send_privacy_preserving_tx(mentions, borsh::to_vec(instruction)?, &program)
                .await
                .context("the sequencer did not accept the transaction")?;
            secrets = shared;
            hash
        } else {
            self.wallet
                .send_pub_tx(mentions, borsh::to_vec(instruction)?, self.program)
                .await
                .context("the sequencer did not accept the transaction")?
        };
        println!("submitted {hash}");
        let (tx, block) = self
            .wallet
            .poll_transaction(hash)
            .await
            .context("the transaction was not included in a block")?;
        // A private signer's note was spent and a new one made: record it, or the
        // next private change would spend the old note and be refused.
        if let common::transaction::LeeTransaction::PrivacyPreserving(tx) = &tx {
            let decode: Vec<AccDecodeData> = secrets
                .into_iter()
                .zip(private_accounts)
                .map(|(secret, account)| AccDecodeData::Decode(secret, account))
                .collect();
            self.wallet
                .decode_insert_privacy_preserving_transaction_results(tx, &decode)?;
            self.wallet.store_persistent_data()?;
        }

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
    async fn pay_private(&mut self, name: &str, amount: u128, from: AccountId) -> Result<()> {
        let (_, _, profile) = self.lookup(name).await?;
        let npk = profile
            .npk
            .ok_or_else(|| anyhow!("{name} has no private keys set"))?;
        let vpk = lee_core::encryption::ViewingPublicKey::from_bytes(profile.vpk)
            .map_err(|error| anyhow!("{name} publishes an invalid viewing key: {error}"))?;
        if self.is_private(from) {
            // From a private account neither side is seen: no sender, no recipient,
            // no amount.
            println!("paying {amount} privately to {name}{NAME_SUFFIX} from a private account; generating the proof");
            let (hash, [sender_secret, _]) = NativeTokenTransfer(&self.wallet)
                .send_private_transfer_to_outer_account(
                    from,
                    lee_core::NullifierPublicKey(npk),
                    vpk,
                    wallet::cli::identifier_or_random(None),
                    amount,
                )
                .await
                .context("the sequencer did not accept the transfer")?;
            println!("submitted {hash}");
            let (tx, block) = self
                .wallet
                .poll_transaction(hash)
                .await
                .context("the transfer was not included in a block")?;
            // The sender's note was spent and its change made into a new one: record it.
            if let common::transaction::LeeTransaction::PrivacyPreserving(tx) = &tx {
                self.wallet.decode_insert_privacy_preserving_transaction_results(
                    tx,
                    &[AccDecodeData::Decode(sender_secret, from)],
                )?;
                self.wallet.store_persistent_data()?;
            }
            println!("included in block {block}; the recipient sees it after syncing their private accounts");
            return Ok(());
        }
        let before = self.wallet.get_account_balance(from).await?;

        println!("paying {amount} privately to {name}{NAME_SUFFIX}; generating the proof");
        println!("(paying from a public account shows that account as the payer; pay from a Private/ account to hide it)");
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

    /// The name's page, opened with its key, if it has one.
    async fn page(&self, name: &str) -> Result<Option<PageContent>> {
        let (Derived { id, key }, _, _) = self.lookup(name).await?;
        let bytes = self.shard(page_address(&self.program, &id)).await?;
        if bytes.is_empty() {
            return Ok(None);
        }
        let page: Page = borsh::from_slice(&bytes).context("the page shard does not hold a page")?;
        Ok(open_page(&key, &page.sealed))
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
            // The chain keeps only a tag of the holder; this wallet can tell
            // whether it is one of its own accounts, and nobody can tell more.
            "held_by": self.holder(name, &id, &record).ok().map(|account| account.to_string()),
            "expires": record.expires,
            "expires_date": readable(record.expires),
            "lapsed": local_now() >= record.expires,
            // A name never shows an address: payers' wallets use its receiving keys
            // without printing them, and `names pay` pays them.
            "payable_privately": profile.npk.is_some() && !profile.vpk.is_empty(),
            // Only if the holder chose to publish one.
            "public_account": profile.account.map(|account| AccountId::new(account).to_string()),
            "texts": texts,
            "page": page,
        }))
    }

}

/// What a page shown by `names open` may do: run its own inline script and style and
/// use data: URLs, nothing else. It loads nothing from the network, so opening a
/// name tells no third party, and the sandbox gives it an origin of its own.
const VIEWER_CSP: &str = "sandbox allow-scripts allow-popups; default-src 'none'; \
    script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; font-src data:; \
    media-src data:; form-action 'none'; base-uri 'none'";

/// Serves `page` to this machine only, until interrupted.
fn serve_page(name: &str, page: &PageContent, port: u16, no_browser: bool) -> Result<()> {
    use std::io::{Read as _, Write as _};
    let listener = std::net::TcpListener::bind(("127.0.0.1", port))
        .with_context(|| format!("cannot listen on 127.0.0.1:{port}"))?;
    let port = listener.local_addr()?.port();
    let url = format!("http://127.0.0.1:{port}/");
    println!("{name}{NAME_SUFFIX} is at {url} (only this machine can reach it; Ctrl-C stops it)");
    if !no_browser {
        let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
        if std::process::Command::new(opener).arg(&url).spawn().is_err() {
            println!("open {url} in your browser");
        }
    }
    let allowed_hosts = [format!("127.0.0.1:{port}"), format!("localhost:{port}")];
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let mut request = Vec::new();
        let mut buffer = [0u8; 4096];
        while !request.windows(4).any(|w| w == b"\r\n\r\n") && request.len() < 64 * 1024 {
            match stream.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(n) => request.extend_from_slice(&buffer[..n]),
            }
        }
        let request = String::from_utf8_lossy(&request);
        let mut lines = request.lines();
        let path = lines.next().and_then(|line| line.split(' ').nth(1)).unwrap_or("");
        // Another site's page can make the browser ask a name that points at
        // 127.0.0.1 (DNS rebinding); answer only requests addressed to this server.
        let host_ok = lines
            .filter_map(|line| line.split_once(':'))
            .any(|(key, value)| key.eq_ignore_ascii_case("host") && allowed_hosts.iter().any(|h| h == value.trim()));
        let (status, content_type, body): (&str, &str, &[u8]) = if !host_ok {
            ("421 Misdirected Request", "text/plain", b"wrong host")
        } else if path == "/" {
            ("200 OK", page.content_type.as_str(), &page.body)
        } else {
            ("404 Not Found", "text/plain", b"not found")
        };
        let head = format!(
            "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n\
             Content-Security-Policy: {VIEWER_CSP}\r\nX-Content-Type-Options: nosniff\r\n\
             Referrer-Policy: no-referrer\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
            body.len()
        );
        let _ = stream.write_all(head.as_bytes()).and_then(|()| stream.write_all(body));
    }
    Ok(())
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

/// Opens the wallet at the wallet home. Without one, makes a new wallet and saves it,
/// or, for a lookup (`save` false), uses an empty one that is never saved or shown,
/// so lookups need no setup and leave nothing behind.
async fn open_wallet(overrides: Option<WalletConfigOverrides>, save: bool) -> Result<WalletCore> {
    write_default_wallet_files()?;
    let storage_path = fetch_persistent_storage_path()?;
    if storage_path.exists() {
        return WalletCore::new_update_chain(fetch_config_path()?, storage_path, overrides);
    }
    let (mut wallet, mnemonic) =
        WalletCore::new_init_storage(fetch_config_path()?, storage_path, overrides, "")?;
    if !save {
        return Ok(wallet);
    }
    // A new wallet's keys can't have anything in past blocks, so start syncing from
    // now instead of scanning the whole chain (minutes on the testnet).
    if let Ok(latest) = wallet.get_last_block_id().await {
        wallet.storage_mut().set_last_synced_block(latest);
    }
    println!("Created a new wallet. Write down its recovery phrase and keep it safe;");
    println!("it is the only way to get the wallet back:");
    println!();
    println!("  {mnemonic}");
    println!();
    wallet.store_persistent_data()?;
    Ok(wallet)
}

/// The sequencer the wallet should use instead of the one in its config: `--rpc` if
/// given, else the public LEZ testnet unless `--program` names another registry.
fn sequencer_overrides(cli: &Cli) -> Result<Option<WalletConfigOverrides>> {
    // The public testnet sometimes takes a few minutes to include a transaction; the
    // wallet's default wait gives up first and reports a failure that isn't one.
    let mut overrides = WalletConfigOverrides {
        seq_tx_poll_max_blocks: Some(TX_POLL_MAX_BLOCKS),
        ..Default::default()
    };
    let rpc = match (&cli.rpc, &cli.program) {
        (Some(rpc), _) => rpc.as_str(),
        (None, Some(_)) => return Ok(Some(overrides)),
        (None, None) => DEFAULT_RPC,
    };
    let sequencer_addr = rpc.parse().map_err(|_| anyhow!("{rpc} is not a URL"))?;
    overrides.sequencer = Some(SequencerConnectionData { sequencer_addr, basic_auth: None });
    Ok(Some(overrides))
}

/// How many times `register` sends its reveal before giving up.
const REVEAL_ATTEMPTS: u32 = 4;

/// How many blocks to wait for a transaction to be included before giving up.
const TX_POLL_MAX_BLOCKS: usize = 60;

/// Asks the Names faucet to fund `account`.
fn faucet(account: &str, rpc: &str) -> Result<()> {
    let account = account.trim().trim_start_matches("Public/");
    let before = public_balance(rpc, account).unwrap_or(0);
    let body = serde_json::json!({ "account": account }).to_string();
    let mut response = ureq::post(FAUCET_URL)
        .config()
        .http_status_as_error(false)
        .build()
        .header("Content-Type", "application/json")
        .send(body)
        .context("could not reach the faucet")?;
    let status = response.status();
    let reply: serde_json::Value = serde_json::from_str(&response.body_mut().read_to_string()?)
        .map_err(|_| anyhow!("the faucet is not answering (HTTP {status}); try again shortly"))?;
    if let Some(error) = reply["error"].as_str() {
        bail!("faucet: {error}");
    }
    let amount: u128 = reply["amount"].as_str().and_then(|a| a.parse().ok()).unwrap_or(0);
    println!(
        "the faucet sent {amount} to Public/{account} (transaction {}); waiting for it to land",
        reply["tx"].as_str().unwrap_or("?")
    );
    // Read the chain directly: inclusion on the public testnet can take a few minutes.
    for _ in 0..60 {
        std::thread::sleep(std::time::Duration::from_secs(5));
        if let Ok(now) = public_balance(rpc, account) {
            if now >= before + amount {
                println!("Public/{account} now holds {now}");
                return Ok(());
            }
        }
    }
    bail!("the transfer has not arrived after 5 minutes; check later with `names wallet account get --account-id Public/{account}`")
}

/// `names setup`: a public and a private account, the faucet's payment, and some of
/// it moved to the private account so names can be held and paid privately.
async fn setup(client: &mut Client, rpc: &str, shield: u128) -> Result<()> {
    let mut defaults = load_defaults();
    let public: AccountId = match defaults.public.as_deref().and_then(|id| id.parse().ok()) {
        Some(id) => id,
        None => client.wallet.create_new_account_public(None).0,
    };
    let private: AccountId = match defaults.private.as_deref().and_then(|id| id.parse().ok()) {
        Some(id) => id,
        None => client.wallet.create_new_account_private(None).0,
    };
    client.wallet.store_persistent_data()?;
    defaults.public = Some(public.to_string());
    defaults.private = Some(private.to_string());
    save_defaults(&defaults)?;
    println!("public account   Public/{public}");
    println!("private account  Private/{private}");

    let mut balance = client.wallet.get_account_balance(public).await.unwrap_or(0);
    if balance < shield {
        println!("asking the faucet for tokens");
        if let Err(error) = faucet(&public.to_string(), rpc) {
            println!("{error:#}");
            println!(
                "fund Public/{public} another way (`names bridge`), then run `names setup` again"
            );
            return Ok(());
        }
        balance = client.wallet.get_account_balance(public).await.unwrap_or(0);
    }
    if shield > 0 && balance >= shield {
        println!("moving {shield} to the private account; generating the proof (a few minutes)");
        let (hash, _) = NativeTokenTransfer(&client.wallet)
            .send_shielded_transfer(AccountIdentity::Public(public), private, shield)
            .await
            .context("the sequencer did not accept the transfer")?;
        client
            .wallet
            .poll_transaction(hash)
            .await
            .context("the transfer was not included in a block")?;
        client.wallet.sync_to_latest_block().await?;
        client.wallet.store_persistent_data()?;
    }
    println!("ready: `names register <name>` holds a name privately, `names pay <name>.lez <amount>` pays one");
    Ok(())
}

/// Most notes one deposit may spend: the node signs at most 32 items per transaction.
const MAX_DEPOSIT_NOTES: usize = 30;

/// Deposits whole notes of `from_key` into the LEZ channel through the user's own node,
/// with the LEZ account as the deposit's metadata: the sequencer reads it as
/// `borsh(AccountId)` and credits that account once the deposit is final.
fn bridge(
    rpc: &str,
    node: &str,
    amount: u128,
    to: &str,
    from_key: &str,
    max_fee: u64,
    yes: bool,
) -> Result<()> {
    let node = node.trim_end_matches('/');
    let to = parse_account(to).map_err(|error| anyhow!(error))?;
    let from_key = from_key.trim().to_lowercase();
    if from_key.len() != 64 || hex::decode(&from_key).is_err() {
        bail!("--from-key must be a 64-character hex public key");
    }
    let channel = rpc_call(rpc, "getChannelId", serde_json::json!({}))?;
    let channel = channel
        .as_str()
        .filter(|c| c.len() == 64)
        .ok_or_else(|| anyhow!("the sequencer did not return a channel ID"))?
        .to_owned();

    let wallet: serde_json::Value = serde_json::from_str(
        &ureq::get(&format!("{node}/wallet/{from_key}/balance"))
            .call()
            .with_context(|| format!("cannot reach your Logos Blockchain node at {node}"))?
            .body_mut()
            .read_to_string()?,
    )
    .context("the node's wallet reply is not JSON")?;
    let mut notes: Vec<(String, u128)> = wallet["notes"]
        .as_object()
        .ok_or_else(|| anyhow!("the node lists no notes for {from_key}"))?
        .iter()
        .filter_map(|(id, value)| Some((id.clone(), value.as_u64()? as u128)))
        .collect();
    notes.sort_by(|a, b| b.1.cmp(&a.1));
    let mut picked = Vec::new();
    let mut total = 0_u128;
    for (id, value) in notes {
        if total >= amount || picked.len() == MAX_DEPOSIT_NOTES {
            break;
        }
        total += value;
        picked.push(id);
    }
    if total < amount {
        bail!(
            "{} note(s) of {from_key} hold {total}, less than {amount}; one deposit spends at most {MAX_DEPOSIT_NOTES} notes, so bridge in smaller steps",
            picked.len()
        );
    }
    println!("deposit {total} from {} note(s) of {from_key}", picked.len());
    println!("  into LEZ channel {channel}, credited to Public/{to}");
    println!("  through {node}, fee at most {max_fee}");
    if !yes {
        println!("nothing sent; run again with --yes to deposit (it cannot be undone)");
        return Ok(());
    }
    let body = serde_json::json!({
        "tip": null,
        "deposit": { "channel_id": channel, "inputs": picked, "metadata": borsh::to_vec(&to)? },
        "change_public_key": from_key,
        "funding_public_keys": [from_key],
        "max_tx_fee": max_fee,
    });
    let mut response = ureq::post(&format!("{node}/channel/deposit"))
        .config()
        .http_status_as_error(false)
        .build()
        .header("Content-Type", "application/json")
        .send(body.to_string())
        .with_context(|| format!("cannot reach your Logos Blockchain node at {node}"))?;
    let status = response.status();
    let text = response.body_mut().read_to_string()?;
    if !status.is_success() {
        bail!("the node refused the deposit (HTTP {status}): {text}");
    }
    let reply: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
    println!("submitted deposit {}", reply["hash"].as_str().unwrap_or(&text));
    println!(
        "LEZ credits Public/{to} once the Logos Blockchain block holding it is final, which can take hours; check with `names wallet account get --account-id Public/{to}`"
    );
    Ok(())
}

fn rpc_call(rpc: &str, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
    let body = serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
    let reply: serde_json::Value = serde_json::from_str(
        &ureq::post(rpc)
            .header("Content-Type", "application/json")
            .send(body.to_string())
            .with_context(|| format!("cannot reach the sequencer at {rpc}"))?
            .body_mut()
            .read_to_string()?,
    )?;
    if let Some(error) = reply.get("error") {
        bail!("{method}: {error}");
    }
    Ok(reply["result"].clone())
}

/// A public account's native balance, read straight from the sequencer.
fn public_balance(rpc: &str, account: &str) -> Result<u128> {
    let result = rpc_call(rpc, "getAccountBalance", serde_json::json!({ "account_id": account }))?;
    result
        .as_u64()
        .map(u128::from)
        .or_else(|| result.as_str().and_then(|s| s.parse().ok()))
        .ok_or_else(|| anyhow!("unexpected balance reply"))
}

/// Private changes are proven by RISC Zero's `r0vm`. The installer puts it next to
/// `names`; use that copy unless `RISC0_SERVER_PATH` names another.
fn use_bundled_prover() {
    if std::env::var_os("RISC0_SERVER_PATH").is_some() {
        return;
    }
    let bundled = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.canonicalize().ok())
        .and_then(|exe| Some(exe.parent()?.join("r0vm")));
    if let Some(r0vm) = bundled.filter(|path| path.is_file()) {
        // SAFETY: called first thing in main, before any other thread starts.
        unsafe { std::env::set_var("RISC0_SERVER_PATH", r0vm) };
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    use_bundled_prover();
    let cli = Cli::parse();
    if let Command::Man { dir } = &cli.command {
        return write_man_pages(dir);
    }
    let overrides = sequencer_overrides(&cli)?;
    match cli.command {
        Command::Wallet { args } => return run_wallet(args, overrides).await,
        Command::Bridge { amount, to, from_key, node, max_fee, yes } => {
            let rpc = cli.rpc.as_deref().unwrap_or(DEFAULT_RPC);
            return bridge(rpc, &node, amount, &to, &from_key, max_fee, yes);
        }
        Command::Faucet { account } => {
            return faucet(&account, cli.rpc.as_deref().unwrap_or(DEFAULT_RPC));
        }
        _ => {}
    }
    let program = match cli.program {
        Some(program) => program,
        None => DEFAULT_PROGRAM.parse().map_err(|_| anyhow!("bad built-in program ID"))?,
    };

    if let Command::AccountLogEntry { name } = &cli.command {
        println!("{}", hex::encode(names_accountlog::name_entry(&format!("{name}{NAME_SUFFIX}"))));
        return Ok(());
    }
    if let Command::Address { name } = &cli.command {
        let Derived { id, .. } = derive(name);
        println!("id     {}", hex::encode(id));
        println!("record {}", record_address(&program, &id));
        println!("page   {}", page_address(&program, &id));
        return Ok(());
    }

    let mut client = Client {
        wallet: open_wallet(
            overrides,
            !matches!(
                cli.command,
                Command::Resolve { .. } | Command::Open { .. } | Command::Settings | Command::VerifyAccount { .. }
            ),
        )
        .await?,
        program,
    };

    match cli.command {
        Command::Setup { shield } => setup(&mut client, cli.rpc.as_deref().unwrap_or(DEFAULT_RPC), shield).await,
        Command::Balance => {
            client.wallet.sync_to_latest_block().await.ok();
            let defaults = load_defaults();
            let key_chain = client.wallet.storage().key_chain();
            let publics: Vec<AccountId> = key_chain.public_account_ids().map(|(id, _)| id).collect();
            let privates: Vec<AccountId> = key_chain.private_account_ids().map(|(id, _)| id).collect();
            let mut total_private = 0_u128;
            println!("public accounts (seen on chain):");
            for id in publics {
                let balance = client.wallet.get_account_balance(id).await.unwrap_or(0);
                let tag = if defaults.public.as_deref() == Some(&id.to_string()) { "  (default)" } else { "" };
                println!("  Public/{id}  {balance}{tag}");
            }
            println!("private accounts (only this wallet sees them):");
            for id in privates {
                let balance = client
                    .wallet
                    .get_account_private(id)
                    .and_then(|account| account.data.native_balance().ok())
                    .unwrap_or(0);
                total_private += balance;
                let tag = if defaults.private.as_deref() == Some(&id.to_string()) { "  (default)" } else { "" };
                if balance > 0 || !tag.is_empty() {
                    println!("  Private/{id}  {balance}{tag}");
                }
            }
            println!("private total: {total_private}");
            Ok(())
        }
        Command::Mine => {
            let defaults = load_defaults();
            for (name, pending) in &defaults.pending {
                match &pending.failed {
                    Some(why) => println!(
                        "{name}{NAME_SUFFIX}  NOT registered: {why}; run `names register {name}` to try again (log: {})",
                        register_log(name)?.display()
                    ),
                    None => println!(
                        "{name}{NAME_SUFFIX}  registering, ready from about {} (log: {})",
                        readable(pending.made.saturating_add(MIN_COMMIT_AGE_MS)),
                        register_log(name)?.display()
                    ),
                }
            }
            if defaults.names.is_empty() && defaults.pending.is_empty() {
                println!("no names yet; register one with `names register <name>`");
            }
            for name in defaults.names {
                match client.lookup(&name).await {
                    Ok((Derived { id, .. }, record, _)) if client.holder(&name, &id, &record).is_ok() => {
                        let state = if local_now() >= record.expires { "lapsed" } else { "held" };
                        println!("{name}{NAME_SUFFIX}  {state} until {}", readable(record.expires));
                    }
                    _ => println!("{name}{NAME_SUFFIX}  no longer held by this wallet"),
                }
            }
            Ok(())
        }
        Command::Register {
            name,
            owner,
            term,
            now,
            wait,
        } => {
            let owner = match owner {
                Some(owner) => owner,
                None => default_private()?,
            };
            if !client.register(&name, owner, term, now, wait, cli.rpc.as_deref()).await? {
                return Ok(());
            }
            let mut defaults = load_defaults();
            if !defaults.names.contains(&name) {
                defaults.names.push(name);
                save_defaults(&defaults)?;
            }
            Ok(())
        }
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
        Command::ClearAccount { name } => {
            client
                .edit(&name, |profile| {
                    profile.account = None;
                    Ok(())
                })
                .await
        }
        Command::ReceivePrivately { name } => {
            let (account, npk, vpk) = client.new_receiving_keys()?;
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
        Command::SetLogosAccount { name, address } => {
            if !address.is_empty() {
                names_accountlog::parse_address(&address).map_err(|e| anyhow!(e))?;
            }
            client
                .edit(&name, |profile| {
                    profile.texts.retain(|text| text.key != LOGOS_ACCOUNT_KEY);
                    if !address.is_empty() {
                        profile.texts.push(TextRecord { key: LOGOS_ACCOUNT_KEY.to_owned(), value: address });
                    }
                    Ok(())
                })
                .await
        }
        Command::VerifyAccount { name, log } => {
            let (_, _, profile) = client.lookup(&name).await?;
            let address = profile
                .texts
                .iter()
                .find(|text| text.key == LOGOS_ACCOUNT_KEY)
                .map(|text| text.value.clone())
                .ok_or_else(|| anyhow!("{name}{NAME_SUFFIX} names no Logos account"))?;
            let bytes = std::fs::read(&log).with_context(|| format!("cannot read {}", log.display()))?;
            // Accept the log as raw bytes or as hex text.
            let text = String::from_utf8_lossy(&bytes);
            let compact: String = text.split_whitespace().collect();
            let signed = hex::decode(&compact).unwrap_or(bytes);
            let account = names_accountlog::parse_address(&address).map_err(|e| anyhow!(e))?;
            let live = names_accountlog::verify(&account, &signed)
                .map_err(|e| anyhow!("the account log is not valid for {address}: {e}"))?;
            let full_name = format!("{name}{NAME_SUFFIX}");
            let endorsed = names_accountlog::endorses_name(&live, &full_name);
            let entries: Vec<_> = live
                .iter()
                .map(|entry| {
                    let value = match &entry.data {
                        names_accountlog::Data::Ed25519Key(key) => hex::encode(key),
                        names_accountlog::Data::Text(text) => text.clone(),
                    };
                    serde_json::json!({ "index": entry.index, "context": entry.context, "value": value })
                })
                .collect();
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "name": full_name,
                    "logos_account": address,
                    "verified": endorsed,
                    "live_entries": entries,
                }))?
            );
            if !endorsed {
                bail!("the account's log does not endorse {full_name} (no live `{}` entry)", names_accountlog::NAME_CONTEXT);
            }
            Ok(())
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
        Command::Open { name, port, no_browser } => match client.page(&name).await? {
            Some(page) => serve_page(&name, &page, port, no_browser),
            None => bail!("{name}{NAME_SUFFIX} has no page"),
        },
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
        Command::Pay { name, amount, from, public, private } => {
            let from = match from {
                Some(from) => from,
                None => default_private()?,
            };
            let (_, _, profile) = client.lookup(&name).await?;
            let has_keys = profile.npk.is_some() && !profile.vpk.is_empty();
            if public || (!private && !has_keys && profile.account.is_some()) {
                client.pay(&name, amount, from).await
            } else {
                client.pay_private(&name, amount, from).await
            }
        }
        Command::Resolve { name } => {
            println!("{}", serde_json::to_string_pretty(&client.resolve(&name).await?)?);
            Ok(())
        }
        Command::Address { .. }
        | Command::AccountLogEntry { .. }
        | Command::Wallet { .. }
        | Command::Faucet { .. }
        | Command::Bridge { .. }
        | Command::Man { .. } => {
            unreachable!("handled before the wallet is opened")
        }
    }
}
