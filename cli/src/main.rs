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
use common::{HashType, transaction::LeeTransaction};
use lee::{
    PublicTransaction, Simulation,
    privacy_preserving_transaction::circuit::ProgramCatalog,
    program::Program,
    public_transaction::{Message as TxMessage, WitnessSet},
    simulate_public_transaction,
};
use lee_core::{
    NullifierPublicKey, PrivateAccountKind,
    account::{AccountId, Actor},
    program::{PROGRAM_LOADER_ACCOUNT_ID, PdaSeed, ProgramHeader},
};
use names_core::{
    GRACE_MS, Instruction, LengthProof, MAX_COMMIT_AGE_MS, MIN_COMMIT_AGE_MS, MODE_DEPOSIT, MODE_FEE,
    LEGACY_SUFFIX, Message as RegistryMessage, NAME_SUFFIX, TIER_LOCKED, TIER_THREE, TIME_WINDOW_MS, NameId,
    commitment_seed, deposit_address, name_seeds, settings_seed, shortlist,
    NameRecord, Page, Settings, commitment,
    normalize_name, owner_tag, page_address, record_address, settings_address,
    validate_name,
};
use names_seal::{
    Derived, PageContent, Profile, TextRecord, derive, open_page, open_profile, seal_page,
    seal_profile, validate_page, validate_profile, validate_text_key,
};
use sequencer_service_rpc::SequencerClient;
use wallet::{
    AccountIdentity, AccountMention, PASSWORD_ENV_VAR, WalletCore, WalletSequencerClient as _,
    config::{SequencerConnectionData, WalletConfigOverrides},
    max_fee_for,
    program_facades::{
        CreditDelivery, native_token_transfer::NativeTokenTransfer, program_loader::ProgramLoader,
    },
    utils::{fetch_config_path, fetch_persistent_storage_path, read_password},
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
    /// Names to make public as soon as their registration finishes (`--public`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    then_public: Vec<String>,
    /// This wallet's anonymous name, held by its own private account (`names anon`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    anon: Option<String>,
    /// The token program `names token` uses when `--token-program` is not given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    token_program: Option<String>,
    /// Nicknames for this wallet's accounts, by `Public/<id>` or `Private/<id>`, shown in
    /// the app (`names module account-label`). The LEZ wallet's own labels can't be changed
    /// or removed once set, so these are kept here.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    labels: std::collections::BTreeMap<String, String>,
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

/// Where the locked-name list (SPEC.md §8.1) is published: one fixed file per list, named
/// by its root, so a release can never swap it. Only registering needs it, so it isn't built
/// into the CLI: the first registration downloads it (about 61 MB) and keeps it. Everyone gets
/// the same whole file, so nobody learns which name a registration is for.
const SHORTLIST_URL: &str =
    "https://github.com/jefdiesel/logos-names/releases/download/shortlist-9778bed4/shortlist.bin";
/// The most the list may be: a download larger than this is refused.
const MAX_SHORTLIST: u64 = 128 * 1024 * 1024;

/// Where the list is kept once downloaded: `NAMES_SHORTLIST` if set (a file you already have,
/// such as the repository's `shortlist/shortlist.bin`), else the user's cache folder, under the
/// root it must have.
fn shortlist_path() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("NAMES_SHORTLIST") {
        return Ok(PathBuf::from(path));
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).ok_or_else(|| anyhow!("no home folder for the locked-name list"))?;
    let cache = std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from).unwrap_or_else(|| {
        if cfg!(target_os = "macos") { home.join("Library/Caches") } else { home.join(".cache") }
    });
    Ok(cache.join("logos-names").join(format!("shortlist-{}.bin", hex::encode(&names_core::SHORTLIST_ROOT[..4]))))
}

/// Downloads the list to `path`: to a temporary file beside it, renamed once it is whole.
/// It is checked against the program's root by [`short_list`] before any use.
fn download_shortlist(path: &std::path::Path) -> Result<()> {
    let url = std::env::var("NAMES_SHORTLIST_URL").unwrap_or_else(|_| SHORTLIST_URL.to_owned());
    eprintln!("downloading the list of locked names (about 61 MB, once; only registering needs it) from {url}");
    let dir = path.parent().ok_or_else(|| anyhow!("no folder for the locked-name list"))?;
    std::fs::create_dir_all(dir).with_context(|| format!("cannot make {}", dir.display()))?;
    let bytes = ureq::get(&url)
        .call()
        .with_context(|| format!("cannot download the locked-name list from {url}"))?
        .body_mut()
        .with_config()
        .limit(MAX_SHORTLIST)
        .read_to_vec()
        .context("the locked-name list download broke off")?;
    let partial = path.with_extension("part");
    std::fs::write(&partial, &bytes).with_context(|| format!("cannot write {}", partial.display()))?;
    std::fs::rename(&partial, path).with_context(|| format!("cannot keep {}", path.display()))?;
    Ok(())
}

/// The locked-name list, read once: downloaded the first time a registration needs it, and
/// refused unless it rebuilds to the root the registry program has built in.
fn short_list() -> Result<&'static shortlist::ShortList> {
    static LIST: std::sync::OnceLock<Result<shortlist::ShortList, String>> = std::sync::OnceLock::new();
    LIST.get_or_init(|| {
        let load = || -> Result<shortlist::ShortList> {
            let path = shortlist_path()?;
            if !path.is_file() {
                if std::env::var_os("NAMES_SHORTLIST").is_some() {
                    bail!("NAMES_SHORTLIST names {}, which isn't a file", path.display());
                }
                download_shortlist(&path)?;
            }
            let bytes = std::fs::read(&path).with_context(|| format!("cannot read {}", path.display()))?;
            if bytes.len() % 33 != 0 {
                bail!("{} is not a locked-name list", path.display());
            }
            let leaves = bytes
                .chunks_exact(33)
                .map(|chunk| shortlist::Leaf { id: chunk[..32].try_into().expect("32 bytes"), tier: chunk[32] })
                .collect();
            let list = shortlist::ShortList::from_sorted_leaves(leaves).map_err(|reason| anyhow!("{reason}"))?;
            if list.root() != names_core::SHORTLIST_ROOT {
                // A wrong or damaged file: drop the cached copy so the next try downloads it again.
                if std::env::var_os("NAMES_SHORTLIST").is_none() {
                    let _ = std::fs::remove_file(&path);
                }
                bail!("{} is not the list the registry program commits to (its root differs)", path.display());
            }
            Ok(list)
        };
        load().map_err(|error| format!("{error:#}"))
    })
    .as_ref()
    .map_err(|reason| anyhow!("the locked-name list: {reason}"))
}

/// The proof of where `id` sits relative to the short-name list, and its tier.
fn length_proof(id: &NameId) -> Result<(LengthProof, u8)> {
    let list = short_list()?;
    let proof = list.prove(id).map_err(|reason| anyhow!("no length proof: {reason}"))?;
    let tier = shortlist::verify(&names_core::SHORTLIST_ROOT, id, &proof)
        .map_err(|reason| anyhow!("the length proof does not hold: {reason}"))?;
    Ok((proof, tier))
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
const DEFAULT_PROGRAM: &str = "38jEfVV1RCyMzu1FGbYcCn5FqxCxKyY58VgzjruAh2gq";
/// The public LEZ testnet's sequencer, reached directly: no server of ours sits between a
/// user and the chain.
const DEFAULT_RPC: &str = "https://testnet.lez.logos.co";

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
        names resolve jef\n  \
        names setup                                (makes a public and a private account)\n  \
        fund the public account with testnet tokens from your LEZ wallet (or `names bridge`)\n  \
        names setup                                (again: moves some to the private account)\n  \
        names register <name>\n\n\
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

    /// Account ID of the token program `names token` uses. None is built in: no token
    /// program runs on the public LEZ testnet yet. Falls back to `token_program` in
    /// `names.json` next to the wallet.
    #[arg(long, env = "NAMES_TOKEN_PROGRAM", global = true, value_parser = parse_account)]
    token_program: Option<AccountId>,

    /// How to speak to the token program: `prototype` is the one in `wallet-tokens/`.
    #[arg(long, env = "NAMES_TOKEN_CODEC", global = true, default_value = "prototype", value_parser = tokens::CODECS)]
    token_codec: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Set up this machine: a wallet with a public and a private account. Fund the
    /// public one with testnet tokens and run it again, and it moves some of them to
    /// the private account; after that, `register` and `pay` need no account arguments.
    Setup {
        /// How much of the public account's tokens to move to the private account.
        /// The default covers a few names' deposits and private changes: on LEZ 0.4 each
        /// private transaction reserves its fee in the proof, about 5,400,000 at the
        /// lowest base fees.
        #[arg(long, default_value_t = 50_000_000)]
        shield: u128,
    },
    /// Show this wallet's accounts and balances.
    Balance,
    /// Claim the payments waiting for this wallet: a payment to your name's private keys
    /// waits as a pending credit until you claim it. Each claim is a private transaction
    /// (several minutes of proving) whose fee another private account of yours pays.
    Receive {
        /// The private account that pays each claim's fee; by default the one holding the most.
        #[arg(long, value_parser = parse_account)]
        fee_from: Option<AccountId>,
        /// Only list what is waiting.
        #[arg(long)]
        list: bool,
        /// Claim even what is worth less than claiming it costs (a small fee refund).
        #[arg(long)]
        all: bool,
    },
    /// List the names this wallet holds. A name someone transferred to you is listed
    /// once you `--add` it: the chain is checked that this wallet holds it.
    Mine {
        /// A name this wallet holds but doesn't list yet, such as one transferred to it.
        #[arg(long, value_parser = parse_name)]
        add: Option<String>,
    },
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
        /// Once registered (privately), move it to a new public account funded
        /// privately for gas, as `names make-public` does: its posts then take one
        /// block, and nothing links that account to you.
        #[arg(long)]
        public: bool,
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
        /// Prices in base units for 30d, 1y and 5y, separated by commas (fee mode).
        #[arg(long, value_delimiter = ',', default_value = "0,0,0")]
        prices: Vec<u128>,
        /// `deposit` (names are held by a refundable deposit) or `fee` (names are
        /// rented for a term and fees go to the treasury).
        #[arg(long, default_value = "deposit", value_parser = ["deposit", "fee"])]
        mode: String,
        /// Deposits in base units for a normal name and a three-character name,
        /// separated by a comma (deposit mode).
        #[arg(long, value_delimiter = ',', default_value = "0,0")]
        deposits: Vec<u128>,
        /// Sign with this account instead of the settings' admin.
        #[arg(long, hide = true)]
        signer: Option<AccountId>,
        /// A snapshot of the earlier chain (`names snapshot`): holders may claim the
        /// names in it. Sets the claims root and program; see `claims-root`.
        #[arg(long, conflicts_with_all = ["claims_root", "claims_program"])]
        claims_snapshot: Option<PathBuf>,
        /// The claims list's root, 64 hex characters, instead of `--claims-snapshot`.
        #[arg(long, requires = "claims_program")]
        claims_root: Option<String>,
        /// The registry on the earlier chain the claims list was taken from.
        #[arg(long, requires = "claims_root")]
        claims_program: Option<AccountId>,
        /// Claims are accepted before this time, in milliseconds since the Unix epoch;
        /// until then a claim overrides a registration. Without any `--claims-*`
        /// option the current claim settings stay.
        #[arg(long)]
        claims_until: Option<u64>,
    },
    /// Print the root of the claims list a snapshot of the earlier chain makes, for
    /// `configure`, and how many names it holds.
    ClaimsRoot { snapshot: PathBuf },
    /// Take back, for free, a name this wallet held on the earlier chain, with its
    /// profile and page as they were. Until the claim period ends this overrides a
    /// registration of the name.
    Claim {
        #[arg(value_parser = parse_name)]
        name: String,
        /// The snapshot file the registry's claims list was made from.
        #[arg(long, env = "LOGOS_NAMES_SNAPSHOT")]
        snapshot: PathBuf,
    },
    /// Print the registry's admin, mode, deposits, treasury and prices, as JSON.
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
    /// Set the name's profile in one transaction: any of the card entries, plus
    /// other entries with `--set key=value`, and `--clear key` to remove one.
    /// Everything not mentioned stays as it is, unless `--replace` makes this push
    /// the whole profile.
    Profile {
        #[arg(value_parser = parse_name)]
        name: String,
        /// Display name.
        #[arg(long)]
        display: Option<String>,
        /// A short text about you.
        #[arg(long)]
        bio: Option<String>,
        /// An image file (PNG, JPEG, GIF or WebP), stored in the record itself:
        /// keep it small (the whole record is at most 12 KiB).
        #[arg(long)]
        avatar: Option<PathBuf>,
        /// Card theme: light, dark or auto.
        #[arg(long, value_parser = ["light", "dark", "auto"])]
        theme: Option<String>,
        /// Card accent colour, #rrggbb.
        #[arg(long, value_parser = parse_color)]
        color: Option<String>,
        /// Card font: sans, serif, mono or rounded.
        #[arg(long, value_parser = ["sans", "serif", "mono", "rounded"])]
        font: Option<String>,
        /// A Logos Storage content ID.
        #[arg(long)]
        cid: Option<String>,
        /// Any other entry, as key=value. Repeatable.
        #[arg(long = "set", value_parser = parse_key_value)]
        set: Vec<(String, String)>,
        /// Remove an entry. Repeatable.
        #[arg(long)]
        clear: Vec<String>,
        /// Make this the whole profile: entries not given here are dropped. The
        /// name's payment keys and public account stay.
        #[arg(long)]
        replace: bool,
    },
    /// Point NAME to another name (SPEC.md §7, Forwarding): paying, messaging or viewing NAME
    /// then uses TO, one hop. `--clear` removes the forward.
    Forward {
        #[arg(value_parser = parse_name)]
        name: String,
        #[arg(value_parser = parse_name, required_unless_present = "clear")]
        to: Option<String>,
        #[arg(long, conflicts_with = "to")]
        clear: bool,
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
        #[arg(value_parser = parse_new_owner)]
        new_owner: AccountId },
    /// Show a name in your browser: a profile card made from its entries, linking to
    /// its page (served at `/page`) when it has one. It is read from the chain
    /// on this machine and served only to this machine (127.0.0.1); no server sees
    /// which name you opened, and nothing shown can load anything from the internet.
    Open {
        #[arg(value_parser = parse_name)]
        name: String,
        /// Port on 127.0.0.1 to serve it from (0: any free port).
        #[arg(long, default_value_t = 0)]
        port: u16,
        /// Print the address instead of opening the browser.
        #[arg(long)]
        no_browser: bool,
        /// Your own Logos Storage node, which the card links a `cid` entry to.
        #[arg(long, env = "LOGOS_STORAGE_API", default_value = "http://127.0.0.1:8080")]
        storage: String,
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
        /// Amount in base units, above zero; `1,000` may group it.
        #[arg(value_parser = tokens::parse_amount)]
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
    /// The Logos Blockchain, through your own node: status, balances, sending to a
    /// key or a name, stake and leader rewards. Run `names chain help`.
    #[command(subcommand)]
    Chain(chain::ChainCommand),
    /// Fungible tokens: list what this wallet holds, send to a name or an account,
    /// receive private credits. Needs `--token-program` (or NAMES_TOKEN_PROGRAM).
    #[command(subcommand)]
    Token(tokens::TokenCommand),
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
    /// Post on the LNS board as one of your names, privately: the chain sees the
    /// name, never the account that holds it. Prints the post as `<name>/<n>`.
    Post {
        /// The name to post as; this wallet must hold it privately.
        #[arg(long = "as", value_parser = parse_name)]
        as_name: String,
        /// What to say (at most 1,024 bytes).
        text: String,
        /// A Logos Storage CID to attach, such as an image; up to four.
        #[arg(long = "cid")]
        cids: Vec<String>,
        /// Reply to this post: its ID, or `<name>/<n>`.
        #[arg(long, value_parser = board::parse_post)]
        reply: Option<board::PostRef>,
    },
    /// Like a post on the board, written `<name>/<n>`, as one of your names.
    Like {
        #[arg(long = "as", value_parser = parse_name)]
        as_name: String,
        #[arg(value_parser = board::parse_post)]
        post: board::PostRef,
        /// Take the like back.
        #[arg(long)]
        undo: bool,
    },
    /// How many names were registered, renewed or given up in recent blocks
    /// (a count only: which names stays hidden).
    Stats {
        /// How many recent blocks to read (a block is about 10 seconds).
        #[arg(long, default_value_t = 20000)]
        blocks: u64,
    },
    /// The board's recent posts, newest first.
    Feed {
        /// Only posts by these names (the Following feed).
        #[arg(long, value_delimiter = ',', value_parser = parse_name)]
        following: Vec<String>,
        /// How many recent blocks to read (a block is about 10 seconds).
        #[arg(long, default_value_t = 20000)]
        blocks: u64,
        /// Print JSON, for apps.
        #[arg(long)]
        json: bool,
    },
    /// A name's posts, newest first, read straight from the board (no block scan).
    PostsOf {
        #[arg(value_parser = parse_name)]
        name: String,
        /// Start below this post number (for the next page); the newest without it.
        #[arg(long)]
        before: Option<u64>,
        #[arg(long, default_value_t = 10)]
        count: u64,
        #[arg(long)]
        json: bool,
    },
    /// A post and its replies, written `<name>/<n>`, oldest first.
    Thread {
        #[arg(value_parser = board::parse_post)]
        post: board::PostRef,
        #[arg(long, default_value_t = 20000)]
        blocks: u64,
        #[arg(long)]
        json: bool,
    },
    /// Your anonymous name: made the first time, as `anon-<word>-<trait>`, and held
    /// by a new private account that a private transfer funds, so nothing on chain
    /// links it to your other names. Prints the name; registering it takes about 20
    /// minutes and runs in the background (`names mine` shows it).
    Anon {
        /// Make it a public anon: registered privately, then moved to a new public
        /// account funded privately, so it posts in one block and nothing links it to you.
        #[arg(long)]
        public: bool,
    },
    /// Move a name you hold privately to a new public account in this wallet, so
    /// its posts take one block (paying a little gas) instead of a 4-minute proof.
    /// Anyone who knows the name can then find that account; it is funded with
    /// 10,000,000 for gas and should hold nothing else. Payments to the name stay
    /// private. Takes about 9 minutes.
    MakePublic {
        #[arg(value_parser = parse_name)]
        name: String,
    },
    /// Take down one of your posts, written `<name>/<n>`. Its text stays in the
    /// chain's history; apps stop showing it.
    Hide {
        #[arg(value_parser = board::parse_post)]
        post: board::PostRef,
    },
    /// What the Basecamp module asks of this wallet: it keeps no keys and has no prover.
    #[command(subcommand, hide = true)]
    Module(module::Request),
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

mod board;
mod claims;
mod chain;
mod fees;
mod module;
mod tokens;

/// The text entry that points a name to another (SPEC.md §7, Forwarding).
const FORWARD_KEY: &str = "forward";

/// Where `name`'s profile forwards to: a valid name other than `name`, or `None`.
fn forward_of(name: &str, profile: &Profile) -> Option<String> {
    let value = profile.texts.iter().find(|text| text.key == FORWARD_KEY)?.value.as_str();
    parse_name(value).ok().filter(|target| target != name)
}

/// Accepts a name as people write it (`jef`, `@jef`, `lns:jef` or `jef.lez`, any case) and returns the bare name.
fn parse_name(input: &str) -> Result<String, String> {
    let name = normalize_name(input);
    validate_name(&name).map_err(str::to_owned)?;
    // A name that is also an account ID would be ambiguous in a "send to" field (SPEC 2).
    if name.parse::<AccountId>().is_ok() {
        return Err(format!("{name} is also an account ID, so it can't be a name"));
    }
    Ok(name)
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

/// How `names balance` ends: an error when it could not catch up with the chain or read
/// a public account (`unread` of them), so a script never takes `unknown` (or a 0) for
/// what an account holds.
fn balance_read(synced: bool, unread: usize) -> Result<()> {
    if unread > 0 {
        bail!("the sequencer didn't answer, so {unread} public balance(s) are unknown; try again when it is reachable");
    }
    if !synced {
        bail!("could not catch up with the chain (the sequencer didn't answer); the balances above may be out of date");
    }
    Ok(())
}

/// The new holder of a name: an account as `parse_account` reads it, written with its
/// prefix once, and not one that was most likely mangled on the way, since a hand-over
/// can't be undone: the all-zero account, or an ID whose letters are all one case
/// (base58 IDs mix both; a lower-cased one can still decode to 32 bytes).
fn parse_new_owner(input: &str) -> Result<AccountId, String> {
    let bare = input
        .strip_prefix("Public/")
        .or_else(|| input.strip_prefix("Private/"))
        .unwrap_or(input);
    if bare.contains('/') {
        return Err(format!("not an account ID: {input} (write Public/ or Private/ once, then the ID)"));
    }
    let account = parse_account(input)?;
    if account.value().iter().all(|byte| *byte == 0) {
        return Err("that is the empty account (all 1s), which no one holds".to_owned());
    }
    let letters: Vec<char> = bare.chars().filter(char::is_ascii_alphabetic).collect();
    if letters.len() >= 8 && (letters.iter().all(char::is_ascii_lowercase) || letters.iter().all(char::is_ascii_uppercase)) {
        return Err(format!(
            "{input} is all one case, so it was probably changed when it was copied (account IDs mix upper and lower case); paste it exactly as it was given"
        ));
    }
    Ok(account)
}

fn parse_color(input: &str) -> Result<String, String> {
    let ok = input.len() == 7 && input.starts_with('#') && input[1..].chars().all(|c| c.is_ascii_hexdigit());
    if ok { Ok(input.to_ascii_lowercase()) } else { Err("colour must be #rrggbb".to_owned()) }
}

fn parse_key_value(input: &str) -> Result<(String, String), String> {
    let (key, value) = input.split_once('=').ok_or("expected key=value")?;
    validate_text_key(key).map_err(str::to_owned)?;
    Ok((key.to_owned(), value.to_owned()))
}

/// An image file as a data: URL, recognised by its first bytes.
fn image_data_url(path: &std::path::Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    let kind = if bytes.starts_with(b"\x89PNG") {
        "png"
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        "jpeg"
    } else if bytes.starts_with(b"GIF8") {
        "gif"
    } else if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "webp"
    } else {
        bail!("{} is not a PNG, JPEG, GIF or WebP image", path.display());
    };
    Ok(format!("data:image/{kind};base64,{}", base64_encode(&bytes)))
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = u32::from(chunk[0]) << 16
            | u32::from(*chunk.get(1).unwrap_or(&0)) << 8
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(TABLE[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
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
/// How long until `time` (milliseconds), from this machine's clock, in words.
fn until(time: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX));
    match time.saturating_sub(now) / 60_000 {
        0 => "finishing now: the last step takes a few minutes".to_owned(),
        1 => "the last step starts in about a minute".to_owned(),
        minutes => format!("the last step starts in about {minutes} minutes"),
    }
}

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
    wallet: WalletCore<SequencerClient>,
    program: AccountId,
}

/// What registering a name costs. `amount` leaves the payer, to the name's deposit
/// account in deposit mode, and otherwise to `treasury`.
struct Cost {
    amount: u128,
    deposit: bool,
    treasury: Option<AccountId>,
}

impl Client {
    /// The registry's bytecode, read from the chain itself. A client runs it to try a
    /// transaction before sending it, and a private holder's client inside its proof, so
    /// it needs the exact bytecode the chain has; the result is checked against the
    /// header's image ID, and cached next to the wallet. `LOGOS_NAMES_PROGRAM_ELF`
    /// overrides it with a local file.
    async fn program(&self) -> Result<Program> {
        self.program_at(self.program, "LOGOS_NAMES_PROGRAM_ELF", "logos-names", "the registry").await
    }

    /// The bytecode of the program deployed at `id`, as [`Self::program`] reads the
    /// registry's: `env` names a local file that overrides it, `cache` prefixes the
    /// cached copy next to the wallet, `what` names it in messages.
    async fn program_at(&self, id: AccountId, env: &str, cache: &str, what: &str) -> Result<Program> {
        if let Some(path) = std::env::var_os(env) {
            let bytes = std::fs::read(&path)
                .with_context(|| format!("cannot read {}", PathBuf::from(path).display()))?;
            return Program::new(bytes.into())
                .map_err(|error| anyhow!("{what}'s bytecode does not load: {error}"));
        }
        let header = ProgramHeader::from_bytes(&self.actor_state_of(id, PROGRAM_LOADER_ACCOUNT_ID).await?)
            .ok_or_else(|| anyhow!("{id} is not a deployed program"))?;
        let cache = fetch_persistent_storage_path()?.with_file_name(format!("{cache}-{id}.bin"));
        if let Ok(bytes) = std::fs::read(&cache) {
            if let Ok(program) = Program::new(bytes.into()) {
                if program.id() == header.image_id {
                    return Ok(program);
                }
            }
        }
        println!("reading {what}'s bytecode from the chain");
        let program = ProgramLoader(&self.wallet)
            .program(id)
            .await
            .context("failed to read the program from the sequencer")?
            .ok_or_else(|| anyhow!("{id} is not a deployed program"))?;
        std::fs::write(&cache, program.elf()).ok();
        Ok(program)
    }

    /// `program`'s state on `account`; empty if it has none.
    async fn actor_state_of(&self, account: AccountId, program: AccountId) -> Result<Vec<u8>> {
        let view = self
            .wallet
            .get_account_view(Actor::new(account, program))
            .await
            .context("failed to read the account from the sequencer")?;
        Ok(view.map_or_else(Vec::new, |found| found.data.actor_state(program).to_vec()))
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

    /// What `account` can spend: a private account's balance as this wallet
    /// knows it after catching up with the chain, or a public one's on chain.
    async fn spendable(&mut self, account: AccountId) -> u128 {
        if self.is_private(account) {
            self.wallet.sync_to_latest_block().await.ok();
            self.wallet
                .get_account_private(account)
                .and_then(|found| found.data.native_balance().ok())
                .unwrap_or(0)
        } else {
            self.wallet.get_account_balance(account).await.unwrap_or(0)
        }
    }

    fn is_private(&self, account: AccountId) -> bool {
        self.wallet.resolve_private_account(account).is_some()
    }

    /// The registry's state on `account`.
    async fn actor_state(&self, account: AccountId) -> Result<Vec<u8>> {
        self.actor_state_of(account, self.program).await
    }

    /// How `account` takes part in a transaction: as a private account of this
    /// wallet, or as a public one that signs.
    fn signer(&self, account: AccountId) -> AccountIdentity {
        self.wallet.resolve_private_account(account).unwrap_or(AccountIdentity::Public(account))
    }

    /// `account`'s own actor of the registry, which a transaction sends to.
    fn own_actor(&self, account: AccountId) -> AccountMention {
        self.signer(account).select_program_actor_state(self.program)
    }

    /// The registry account behind `seed`, as the actor of the registry on it. The
    /// seed is what lets a transaction create it.
    fn registry_actor(&self, seed: PdaSeed) -> AccountMention {
        AccountIdentity::PublicPda { program: self.program, seed }.select_program_actor_state(self.program)
    }

    /// The native balance of the registry account behind `seed`.
    fn registry_balance(&self, seed: PdaSeed) -> AccountMention {
        AccountIdentity::PublicPda { program: self.program, seed }.balance()
    }

    /// The signer's actor, then the name's record and page.
    fn on_record_and_page(&self, id: &NameId, signer: AccountId) -> Vec<AccountMention> {
        let [record, page, _] = name_seeds(id);
        vec![self.own_actor(signer), self.registry_actor(record), self.registry_actor(page)]
    }

    async fn record(&self, id: &NameId) -> Result<Option<NameRecord>> {
        let bytes = self.actor_state(record_address(&self.program, id)).await?;
        if bytes.is_empty() {
            return Ok(None);
        }
        Ok(Some(
            borsh::from_slice(&bytes).context("the record does not hold a name record")?,
        ))
    }

    async fn settings(&self) -> Result<Option<Settings>> {
        let bytes = self.actor_state(settings_address(&self.program)).await?;
        if bytes.is_empty() {
            return Ok(None);
        }
        Ok(Some(
            borsh::from_slice(&bytes).context("the settings account does not hold settings")?,
        ))
    }
    /// The chain's time in milliseconds, from the clock program's account. If that
    /// cannot be read, this machine's clock set back a minute, so that it is not ahead
    /// of the next block.
    async fn now(&self) -> u64 {
        self.actor_state_of(CLOCK_01_PROGRAM_ACCOUNT_ID, clock_account_id())
            .await
            .ok()
            .and_then(|state| borsh::from_slice::<ClockAccountData>(&state).ok())
            .map_or_else(|| local_now().saturating_sub(60_000), |data| data.timestamp)
    }

    /// What `payer` owes for `term`, and the treasury it goes to when there is a fee.
    /// The treasury itself pays nothing.
    async fn price(&self, payer: AccountId, term: u8) -> Result<(u128, Option<AccountId>)> {
        Ok(match self.settings().await? {
            Some(settings) if settings.mode == MODE_FEE && settings.treasury != payer => {
                let fee = settings.prices[usize::from(term)];
                (fee, (fee != 0).then_some(settings.treasury))
            }
            _ => (0, None),
        })
    }

    /// What registering a name of tier `tier` costs `payer`: in deposit mode the deposit
    /// for its length, otherwise the fee for `term`.
    async fn registration_cost(&self, tier: u8, payer: AccountId, term: u8) -> Result<Cost> {
        match self.settings().await? {
            Some(settings) if settings.mode == MODE_DEPOSIT => {
                // The admin places names without a deposit.
                let amount = if payer == settings.admin { 0 } else { settings.deposits[usize::from(tier == TIER_THREE)] };
                Ok(Cost { amount, deposit: true, treasury: None })
            }
            _ => {
                let (amount, treasury) = self.price(payer, term).await?;
                Ok(Cost { amount, deposit: false, treasury })
            }
        }
    }

    /// The balance actors a registration of `id` or a renewal touches: the payer's,
    /// and that of the account paid, which must be declared for it to be credited in
    /// the same transaction.
    fn paying(&self, id: &NameId, payer: AccountId, cost: &Cost) -> Vec<AccountMention> {
        if cost.amount == 0 {
            return Vec::new();
        }
        let paid = match cost.treasury {
            Some(treasury) => AccountIdentity::PublicNoSign(treasury).balance(),
            None => self.registry_balance(name_seeds(id)[2]),
        };
        vec![self.signer(payer).balance(), paid]
    }

    /// The name to use for `name` (SPEC.md §7, Forwarding): the name its `forward` entry
    /// names, one hop, or `name` itself. Says so on standard error when it forwards, since
    /// a payer must see where a payment goes.
    async fn following(&self, name: &str) -> Result<String> {
        let (_, _, profile) = self.lookup(name).await?;
        let Some(target) = forward_of(name, &profile) else {
            return Ok(name.to_owned());
        };
        self.lookup(&target)
            .await
            .map_err(|_| anyhow!("{name} points to {target}, which isn't registered"))?;
        eprintln!("{name} → {target} (it forwards there)");
        Ok(target)
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
        quietly(|| self.wallet.store_persistent_data())?;
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
        let (length, tier) = length_proof(&id)?;
        if tier == TIER_LOCKED {
            let admin = self.settings().await?.map(|settings| settings.admin);
            if admin != Some(owner) {
                bail!(
                    "{name}{NAME_SUFFIX} is locked (one to four characters, or on the reserved list): only the registry's admin registers it, then transfers it to whoever it is for"
                );
            }
        }
        let cost = self.registration_cost(tier, owner, term).await?;
        let (fee, deposit) = (cost.amount, cost.deposit);
        if let Some(settings) = self.settings().await? {
            if settings.claims_root != [0; 32] && self.now().await < settings.claims_until {
                println!(
                    "warning: names from the earlier chain can still be claimed, until {}. If {name}{NAME_SUFFIX} was held there, its holder's claim replaces this registration, and the deposit stays in the name's account for them",
                    readable(settings.claims_until)
                );
            }
        }
        if deposit && fee > 0 {
            println!(
                "{name}{NAME_SUFFIX} takes a deposit of {fee}, held for as long as you keep the name and refunded when you release it"
            );
            println!("(the deposit stops name squatting: holding many names ties up many deposits; nobody keeps it)");
        }
        // The deposit or fee is only taken at the reveal, 15 minutes on: check
        // now, so a short account doesn't commit and fail at the end.
        if fee > 0 {
            let have = self.spendable(owner).await;
            if have < fee {
                bail!(
                    "{owner} holds {have}, less than the {fee} that registering {name}{NAME_SUFFIX} takes; add funds to it first"
                );
            }
        }
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
                    vec![self.own_actor(owner), self.registry_actor(commitment_seed(&promised))],
                    &Instruction::Commit { commitment: promised, now: made },
                    0,
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
                "{name}{NAME_SUFFIX} is committed; it finishes by itself in about {} minutes, from {}",
                MIN_COMMIT_AGE_MS / 60_000 + 5,
                readable(ready)
            );
            println!(
                "  it waits until the commitment is {} minutes old, so nobody who saw it can take the name first,",
                MIN_COMMIT_AGE_MS / 60_000
            );
            println!("  then proves the last step on this computer (about 4 minutes); keep this computer on and awake");
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
            let kind = if deposit { "a deposit" } else { "a fee" };
            println!("step 2 of 2: registering {name}{NAME_SUFFIX} at {claimed} for {kind} of {fee}");
            let mut mentions = self.on_record_and_page(&id, owner);
            mentions.push(self.registry_actor(settings_seed()));
            mentions.push(self.registry_actor(commitment_seed(&promised)));
            mentions.extend(self.paying(&id, owner, &cost));
            let sent = self
                .send(
                    mentions,
                    &Instruction::Register {
                        id,
                        sealed: sealed.clone(),
                        term,
                        fee,
                        treasury: cost.treasury,
                        now: claimed,
                        secret,
                        deposit,
                        length: length.clone(),
                    },
                    fee,
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
        if record.expires == u64::MAX {
            bail!("{name}{NAME_SUFFIX} is held by a deposit: it doesn't expire, so there is nothing to renew");
        }
        let payer = match payer {
            Some(payer) => payer,
            None => self.holder(name, &id, &record)?,
        };
        let (fee, treasury) = self.price(payer, term).await?;
        let now = self.now().await;
        println!("renewing {name}{NAME_SUFFIX} at {now} for a fee of {fee}");
        let [record, ..] = name_seeds(&id);
        let mut mentions = vec![
            self.own_actor(payer),
            self.registry_actor(record),
            self.registry_actor(settings_seed()),
        ];
        mentions.extend(self.paying(&id, payer, &Cost { amount: fee, deposit: false, treasury }));
        self.send(mentions, &Instruction::Renew { id, term, fee, treasury, now }, fee).await
    }

    /// Sets the registry's settings, signed by their admin or, while there are none,
    /// by the program's own account, whose key its deployer holds.
    #[allow(clippy::too_many_arguments)]
    async fn configure(
        &mut self,
        admin: AccountId,
        treasury: AccountId,
        prices: [u128; 3],
        mode: u8,
        deposits: [u128; 2],
        signer: Option<AccountId>,
        claims: claims::ClaimSettings,
    ) -> Result<()> {
        let settings = self.settings().await?;
        let signer = match (signer, &settings) {
            (Some(signer), _) => signer,
            (None, Some(settings)) => settings.admin,
            (None, None) => self.program,
        };
        // A change of prices must not end a claim period.
        let claims = claims.over(settings.as_ref());
        self.send(
            vec![self.own_actor(signer), self.registry_actor(settings_seed())],
            &Instruction::Configure {
                admin,
                treasury,
                prices,
                mode,
                deposits,
                claims_root: claims.root,
                claims_until: claims.until,
                claims_program: claims.program,
            },
            0,
        )
        .await
    }

    /// Takes back `name` from the earlier chain's snapshot, for free, with its sealed
    /// profile and page exactly as they were: they are sealed under the name's key,
    /// which has not changed.
    async fn claim(&mut self, name: &str, snapshot: &std::path::Path) -> Result<()> {
        let settings = self.settings().await?.ok_or_else(|| anyhow!("this registry has no settings"))?;
        if settings.claims_root == [0; 32] {
            bail!("this registry takes no claims");
        }
        let now = self.now().await;
        if now >= settings.claims_until {
            bail!("the claim period ended at {}", readable(settings.claims_until));
        }
        let snapshot = claims::Snapshot::load(snapshot)?;
        if snapshot.program != settings.claims_program || snapshot.root() != settings.claims_root {
            bail!("that snapshot is not the one this registry's claims list was made from");
        }
        let Derived { id, .. } = derive(name);
        let (proof, record) = snapshot
            .claim(&id)
            .ok_or_else(|| anyhow!("{name}{NAME_SUFFIX} is not in the snapshot"))?;
        let key_chain = self.wallet.storage().key_chain();
        let holder = key_chain
            .public_account_ids()
            .map(|(account, _)| account)
            .chain(key_chain.private_account_ids().map(|(account, _)| account))
            .find(|account| owner_tag(&id, account) == record.owner)
            .ok_or_else(|| {
                anyhow!(
                    "no account in this wallet held {name}{NAME_SUFFIX} on the earlier chain. A public account comes back \
                     from the old wallet's recovery phrase (`names wallet restore-keys`, then `names wallet account new \
                     public` until its ID appears); a private one does not, because LEZ 0.4 derives other private account IDs"
                )
            })?;
        if let Some(held) = self.record(&id).await? {
            if held.claimed {
                bail!("{name}{NAME_SUFFIX} was already claimed");
            }
            println!("{name}{NAME_SUFFIX} was registered here since; this claim replaces that registration");
        }
        println!("claiming {name}{NAME_SUFFIX}, held by {holder} on the earlier chain");
        let mut mentions = self.on_record_and_page(&id, holder);
        mentions.push(self.registry_actor(settings_seed()));
        self.send(mentions, &Instruction::Claim { id, sealed: record.sealed.clone(), now, proof }, 0).await?;
        let mut defaults = load_defaults();
        if !defaults.names.contains(&name.to_owned()) {
            defaults.names.push(name.to_owned());
            save_defaults(&defaults)?;
        }
        println!("{name}{NAME_SUFFIX} is yours again");
        // The claim emptied the page; the old one goes back in a transaction of its own,
        // since a page can be far larger than a claim.
        if let Some(page) = snapshot.page(&id) {
            println!("restoring its page");
            self.send(
                self.on_record_and_page(&id, holder),
                &Instruction::SetPage { id, sealed: page },
                0,
            )
            .await?;
        }
        Ok(())
    }

    /// Changes the profile of `name`: reads and opens it, applies `change`, then
    /// seals the whole of it again and replaces the record's sealed bytes.
    async fn edit(&mut self, name: &str, change: impl FnOnce(&mut Profile) -> Result<()>) -> Result<()> {
        let (Derived { id, key }, record, mut profile) = self.lookup_held(name).await?;
        let holder = self.holder(name, &id, &record)?;
        change(&mut profile)?;
        validate_profile(&profile).map_err(|reason| anyhow!(reason))?;
        let sealed = seal_profile(&key, &profile);
        let [record, ..] = name_seeds(&id);
        self.send(
            vec![self.own_actor(holder), self.registry_actor(record)],
            &Instruction::SetRecord { id, sealed },
            0,
        )
        .await
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
        self.send(self.on_record_and_page(&id, holder), &Instruction::SetPage { id, sealed }, 0).await
    }

    /// Hands `name` to `new_owner`, clearing what the current owner set on it.
    async fn transfer(&mut self, name: &str, new_owner: AccountId) -> Result<()> {
        if new_owner == self.program {
            bail!("that is the registry program's account, which can't hold a name");
        }
        let (Derived { id, .. }, record, _) = self.lookup_held(name).await?;
        let holder = self.holder(name, &id, &record)?;
        self.send(
            self.on_record_and_page(&id, holder),
            &Instruction::Transfer {
                id,
                new_owner: owner_tag(&id, &new_owner),
            },
            0,
        )
        .await
    }

    /// Releases `name`, emptying its record and page.
    async fn release(&mut self, name: &str) -> Result<()> {
        let (Derived { id, .. }, record, _) = self.lookup_held(name).await?;
        let holder = self.holder(name, &id, &record)?;
        // The deposit account's whole balance comes back to whoever releases.
        let held = deposit_address(&self.program, &id);
        let refund = self.wallet.get_account_balance(held).await.unwrap_or(0);
        let mut mentions = self.on_record_and_page(&id, holder);
        if refund > 0 {
            println!("releasing {name}{NAME_SUFFIX}; its deposit of {refund} comes back to you");
            mentions.push(self.registry_balance(name_seeds(&id)[2]));
            mentions.push(self.signer(holder).balance());
        }
        self.send(mentions, &Instruction::Release { id, refund }, 0).await
    }

    /// Sends `instruction` from the signer whose actor is `mentions[0]`; `spent` is
    /// what it moves out of the signer's balance besides the fee.
    async fn send(&mut self, mentions: Vec<AccountMention>, instruction: &Instruction, spent: u128) -> Result<()> {
        let program = self.program().await?;
        let message = borsh::to_vec(&RegistryMessage::Instruction(instruction.clone()))?;
        self.transact(mentions, message, vec![(self.program, program)], spent).await
    }

    /// Sends `message` to the actor `mentions[0]` and confirms it applied. `mentions`
    /// must declare every actor the call reaches, and `programs` every program it runs.
    /// A private signer makes the whole transaction privacy-preserving: the programs run
    /// inside a proof made here, and the chain sees only their effects on public
    /// accounts, never the signer.
    async fn transact(
        &mut self,
        mentions: Vec<AccountMention>,
        message: Vec<u8>,
        programs: Vec<(AccountId, Program)>,
        spent: u128,
    ) -> Result<()> {
        let dry_run = std::env::var_os("LOGOS_NAMES_DRY_RUN").is_some();
        let paying = mentions.clone();
        let before = self.held_by(&paying).await;
        let hash = if mentions.iter().any(|mention| mention.identity.is_private()) {
            let quote = self.wallet.client().get_fee_state().await?;
            let estimate = fees::private(&quote, self.wallet.config().private_fee_headroom_blocks);
            println!("{}", fees::estimate_line(&estimate, "paid inside the proof, from a private account"));
            if dry_run {
                bail!("LOGOS_NAMES_DRY_RUN is set, so nothing was sent (a private change is not tried first)");
            }
            println!("acting privately; proving it on this computer (about 4 minutes), so the chain never learns your account");
            let catalog: ProgramCatalog = programs.into_iter().collect();
            self.wallet
                .send_privacy_preserving_tx(mentions, 0, message, &catalog)
                .await
                .context("the sequencer did not accept the transaction")?
                .0
        } else {
            // A call that fails is billed what it ran and keeps nothing, so try it here
            // first: the same run tells what a refused call would be refused for, and
            // sizes the gas limit, whose whole reserve is held from the payer meanwhile.
            let cycles = self.simulate(&mentions, &message, programs).await?;
            let limit = gas_limit_for(cycles);
            self.check_payer(&mentions, message.len(), spent, limit, cycles).await?;
            if dry_run {
                bail!("LOGOS_NAMES_DRY_RUN is set, so nothing was sent");
            }
            let saved = self.wallet.config().clone();
            let mut config = saved.clone();
            config.gas_limit = limit;
            self.wallet.set_config(config);
            let sent = self.wallet.send_pub_tx(mentions, 0, message).await;
            self.wallet.set_config(saved);
            sent.context("the sequencer did not accept the transaction")?
        };
        println!("submitted {hash}");
        let block = self.finish(hash).await.context("the transaction was not applied")?;
        println!("applied in block {block}");
        if let Some(line) = fees::paid_line(before, self.held_by(&paying).await, spent) {
            println!("{line}");
        }
        Ok(())
    }

    /// What the accounts that can pay for a change over `mentions` hold together: its
    /// signing public accounts this wallet holds keys for, read from the chain, and every private account of this
    /// wallet, as the wallet last saw them. Measured before and after a change, the
    /// difference less what it moved is its fee (LEZ 0.4 keeps no receipt of fees).
    async fn held_by(&self, mentions: &[AccountMention]) -> u128 {
        let mut held = 0_u128;
        let key_chain = self.wallet.storage().key_chain();
        for (id, _) in key_chain.private_account_ids() {
            let balance = self.wallet.get_account_private(id).and_then(|account| account.data.native_balance().ok());
            held = held.saturating_add(balance.unwrap_or(0));
        }
        // Only this wallet's own public accounts: a change also has program-owned accounts
        // sign (a registry's deposit account, say), and what moves into those is not a fee.
        let signers: std::collections::BTreeSet<AccountId> = mentions
            .iter()
            .filter(|mention| mention.authorizes)
            .filter_map(|mention| mention.identity.public_account_id())
            .filter(|account| self.wallet.get_account_public_signing_key(*account).is_some())
            .collect();
        for id in signers {
            held = held.saturating_add(self.wallet.get_account_balance(id).await.unwrap_or(0));
        }
        held
    }

    /// Waits for `hash` to be included and returns its block. A block includes reverted
    /// transactions too, so inclusion alone proves nothing. A private account's note was
    /// spent and a new one made: it is recorded here, or the next private change would
    /// spend the old note and be refused.
    async fn finish(&mut self, hash: HashType) -> Result<u64> {
        let (tx, block_id) = self
            .wallet
            .poll_transaction(hash)
            .await
            .context("the transaction was not included in a block")?;
        let block = self
            .wallet
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
            let key_chain = self.wallet.storage_mut().key_chain_mut();
            let mut index = key_chain.build_latest_nullifier_index();
            key_chain.follow_transaction(&tx.message, reverted, false, &mut index);
            quietly(|| self.wallet.store_persistent_data())?;
        }
        if reverted {
            bail!("{hash} was included in block {block_id} but reverted: it failed on chain or raced with another change; its fee was charged and nothing else applied");
        }
        Ok(block_id)
    }

    /// Runs the call against current chain state with the deployed bytecode, without
    /// sending it, and returns the cycles it takes. A call the program would refuse
    /// fails here, before any fee is paid. No proving, so it takes about a second.
    async fn simulate(
        &self,
        mentions: &[AccountMention],
        message: &[u8],
        programs: Vec<(AccountId, Program)>,
    ) -> Result<u64> {
        let actors: Vec<Actor> = mentions.iter().map(AccountMention::actor).collect();
        let mut states = std::collections::HashMap::new();
        for actor in &actors {
            let state = self.actor_state_of(actor.account_id, actor.program_account_id).await?;
            states.insert(*actor, state.into());
        }
        let signers: std::collections::BTreeSet<AccountId> = mentions
            .iter()
            .filter(|mention| mention.authorizes && mention.identity.public_account_id().is_some())
            .map(|mention| mention.identity.account_id())
            .filter(|account| self.wallet.get_account_public_signing_key(*account).is_some())
            .collect();
        let signers: Vec<AccountId> = signers.into_iter().collect();
        let nonces = self.wallet.get_accounts_nonces(&signers).await?;
        let root = actors.first().copied().ok_or_else(|| anyhow!("a call needs a root actor"))?;
        let tx = TxMessage::new(
            root,
            message.to_vec(),
            actors,
            signers.into_iter().zip(nonces).collect(),
            None,
            Vec::new(),
        );
        let witness = WitnessSet::for_message(&tx, &[]);
        let tx = PublicTransaction::new(tx, witness);
        let simulation = Simulation { public_actor_states: states, admitted_accounts: None };
        let catalog: ProgramCatalog = programs.into_iter().collect();
        // No transaction may declare more gas than a block holds.
        let budget = self.wallet.client().get_fee_state().await?.max_gas_exec;
        let (cycles, result) = tokio::task::spawn_blocking(move || {
            quietly(|| simulate_public_transaction(&tx, &simulation, &catalog, budget))
        })
        .await?;
        result.map_err(|error| anyhow!("refused: {error}"))?;
        Ok(cycles)
    }

    /// Checks that the account paying a public call's fee holds what the call takes from
    /// it (a fee or deposit) plus the fee reserve at `limit`, so a short account is told
    /// so here instead of refused by the sequencer. The reserve is priced at the highest
    /// base fees the next block can have.
    async fn check_payer(&self, mentions: &[AccountMention], bytes: usize, spent: u128, limit: u64, cycles: u64) -> Result<()> {
        // The wallet has the first funded signing account pay the fee.
        let Some(payer) = mentions.iter().find(|mention| mention.authorizes).map(|mention| mention.identity.account_id())
        else {
            return Ok(());
        };
        let quote = self.wallet.client().get_fee_state().await?;
        // The transaction's bytes are billed too: signatures, nonces and actors besides the message.
        let estimate = fees::public(&quote, cycles, limit, bytes);
        let reserve = estimate.at_most;
        println!("{}", fees::estimate_line(&estimate, &format!("gas: {cycles} cycles, limit {limit}")));
        if std::env::var_os("LOGOS_NAMES_DRY_RUN").is_some() {
            println!("fee reserve: {reserve} from Public/{payer}, plus {spent} the call moves");
        }
        let cap = max_fee_for(limit);
        if reserve > cap {
            bail!("the chain's fees are too high right now: this call's fee reserve of {reserve} is above the wallet's cap of {cap}");
        }
        let balance = self.wallet.get_account_balance(payer).await?;
        if balance < spent.saturating_add(reserve) {
            if spent > 0 {
                bail!(
                    "Public/{payer} holds {balance}; this takes {spent}, and about {reserve} more is held for the fee while it runs"
                );
            }
            bail!("Public/{payer} holds {balance}; about {reserve} is held for the fee while this runs");
        }
        Ok(())
    }

    /// Pays `amount` to the public account `name` points at.
    async fn pay(&mut self, name: &str, amount: u128, from: AccountId) -> Result<()> {
        let (_, _, profile) = self.lookup(name).await?;
        let to = profile
            .account
            .map(AccountId::new)
            .ok_or_else(|| anyhow!("{name} has no public account set"))?;
        let before = self.wallet.get_account_balance(to).await?;
        // LEZ 0.4 credits a public account the chain doesn't know yet only when the sender
        // supplies its public key (`wallet auth-transfer send --to-pk`), and a name publishes
        // the account id, not the key. Such a transfer reverts, and the payer still pays its
        // fee, so refuse before sending. Paying the name privately works whatever the account.
        let unused = before == 0
            && self.wallet.get_accounts_nonces(&[to]).await?.first().copied().unwrap_or_default()
                == lee_core::account::Nonce::default();
        if unused {
            let how = if profile.npk.is_some() && !profile.vpk.is_empty() {
                format!("pay {name} privately instead (`names pay {name} {amount}` without --public)")
            } else {
                format!("{name} publishes no private keys either, so its holder must use that account once (or publish private keys) before it can be paid")
            };
            bail!("{name}{NAME_SUFFIX}'s public account {to} has never been used on this chain, and LEZ can't credit it without its owner's public key; {how}");
        }

        println!("paying {amount} to {name}{NAME_SUFFIX} ({to})");
        let paying = vec![self.signer(from).balance()];
        let held = self.held_by(&paying).await;
        self.print_transfer_fee(from).await?;
        // From a private account this is a deshielded transfer: the chain sees the
        // public account credited, but not who paid.
        let (hash, _) = if self.is_private(from) {
            println!("from a private account; proving it on this computer (about 4 minutes), so the chain never learns your account");
            NativeTokenTransfer(&self.wallet)
                .transfer(self.signer(from), AccountIdentity::PublicNoSign(to), amount, CreditDelivery::Automatic)
                .await
        } else {
            // The fee reserve leaves the account before the transfer runs, and a
            // transfer that then fails still pays for what it ran; 64 is the
            // wallet's assumed worst base fee.
            let balance = self.wallet.get_account_balance(from).await?;
            let reserve = u128::from(NATIVE_TRANSFER_GAS_LIMIT + 1_000) * 64;
            if balance < amount.saturating_add(reserve) {
                bail!("Public/{from} holds {balance}; sending {amount} needs about {reserve} more for the fee");
            }
            let saved = self.wallet.config().clone();
            let mut config = saved.clone();
            config.gas_limit = NATIVE_TRANSFER_GAS_LIMIT;
            self.wallet.set_config(config);
            let sent = NativeTokenTransfer(&self.wallet)
                .transfer(
                    AccountIdentity::Public(from),
                    AccountIdentity::PublicNoSign(to),
                    amount,
                    CreditDelivery::Automatic,
                )
                .await;
            self.wallet.set_config(saved);
            sent
        }
        .context("the sequencer did not accept the transfer")?;
        println!("submitted {hash}");
        let block = self.finish(hash).await.context("the transfer was not applied")?;

        // A block includes failed transactions too, so confirm the money arrived.
        let after = self.wallet.get_account_balance(to).await?;
        if from != to && after < before.saturating_add(amount) {
            bail!("transfer {hash} was included in block {block} but {name} was not credited");
        }
        println!("paid in block {block}");
        if let Some(line) = fees::paid_line(held, self.held_by(&paying).await, amount) {
            println!("{line}");
        }
        Ok(())
    }

    /// Prints what a native transfer from `from` costs in fees.
    async fn print_transfer_fee(&self, from: AccountId) -> Result<()> {
        let quote = self.wallet.client().get_fee_state().await?;
        let (estimate, how) = if self.is_private(from) {
            (fees::private(&quote, self.wallet.config().private_fee_headroom_blocks), "a private transfer, paid inside the proof")
        } else {
            (fees::native_public(&quote, NATIVE_TRANSFER_GAS_LIMIT), "a public transfer")
        };
        println!("{}", fees::estimate_line(&estimate, how));
        Ok(())
    }

    /// Pays `amount` to the private receiving keys `name` publishes.
    async fn pay_private(&mut self, name: &str, amount: u128, from: AccountId) -> Result<()> {
        let (_, _, profile) = self.lookup(name).await?;
        let recipient = private_keys_of(name, &profile)?;
        let paying = vec![self.signer(from).balance()];
        let held = self.held_by(&paying).await;
        self.print_transfer_fee(from).await?;
        // A payment to someone's private keys arrives as a pending credit, and claiming it
        // is a private transaction of its own (SC-1): below its fee it is worth nothing.
        let quote = self.wallet.client().get_fee_state().await?;
        let claim = fees::private(&quote, self.wallet.config().private_fee_headroom_blocks);
        if amount <= claim.about {
            println!(
                "note: claiming this costs the holder a private transaction (about {}, paid from another private account of theirs), more than the {amount} you are paying",
                claim.about
            );
        }
        if self.is_private(from) {
            // From a private account neither side is seen: no sender, no recipient,
            // no amount.
            println!("paying {amount} privately to {name}{NAME_SUFFIX} from a private account; generating the proof");
            let (hash, _) = NativeTokenTransfer(&self.wallet)
                .transfer(self.signer(from), recipient, amount, CreditDelivery::Automatic)
                .await
                .context("the sequencer did not accept the transfer")?;
            println!("submitted {hash}");
            let block = self.finish(hash).await.context("the transfer was not applied")?;
            println!("included in block {block}; it waits for {name}{NAME_SUFFIX}'s holder as a pending credit, which they claim with `names receive`");
            if let Some(line) = fees::paid_line(held, self.held_by(&paying).await, amount) {
                println!("{line}");
            }
            return Ok(());
        }
        let before = self.wallet.get_account_balance(from).await?;

        println!("paying {amount} privately to {name}{NAME_SUFFIX}; generating the proof");
        println!("(paying from a public account shows that account as the payer; pay from a Private/ account to hide it)");
        let (hash, _) = NativeTokenTransfer(&self.wallet)
            .transfer(AccountIdentity::Public(from), recipient, amount, CreditDelivery::Automatic)
            .await
            .context("the sequencer did not accept the transfer")?;
        println!("submitted {hash}");
        let block = self.finish(hash).await.context("the transfer was not applied")?;

        // The recipient's balance is private, so the sender's debit is all that can be
        // checked here. It is a weak check: a fee alone can exceed a small amount.
        let debited = before.saturating_sub(self.wallet.get_account_balance(from).await?);
        if debited < amount {
            bail!("transfer {hash} was included in block {block} but {from} was not debited");
        }
        println!("included in block {block}; {from} was debited {debited} (the amount, plus any fee)");
        println!("it waits for {name}{NAME_SUFFIX}'s holder as a pending credit, which they claim with `names receive`");
        if let Some(line) = fees::paid_line(held, self.held_by(&paying).await, amount) {
            println!("{line}");
        }
        Ok(())
    }

    /// Claims the native payments waiting for this wallet's private accounts (payments to
    /// a name's private keys arrive this way). Each claim is a private transaction whose
    /// fee comes from `fee_from`, or the private account of this wallet that holds the
    /// most: the receiving account starts empty and can't pay it (SC-1).
    async fn receive(&mut self, fee_from: Option<AccountId>, list: bool, all: bool) -> Result<()> {
        self.wallet.sync_to_latest_block().await.ok();
        let pending: Vec<_> = self
            .wallet
            .owned_pending_messages()
            .await
            .context("cannot read pending payments")?
            .into_iter()
            .filter(|message| message.native_credit().is_some())
            .collect();
        if pending.is_empty() {
            println!("nothing to receive");
            return Ok(());
        }
        let total: u128 = pending.iter().filter_map(|message| message.native_credit()).sum();
        println!("{} payment(s) waiting, {total} in all:", pending.len());
        for message in &pending {
            println!(
                "  {} to Private/{} (position {})",
                message.native_credit().unwrap_or(0),
                message.body.to.account_id,
                message.position
            );
        }
        if list {
            return Ok(());
        }
        let quote = self.wallet.client().get_fee_state().await?;
        let fee = fees::private(&quote, self.wallet.config().private_fee_headroom_blocks);
        // A credit worth less than claiming it costs (a private fee's refund, most often) is
        // left: claiming it would lose money. `--all` claims it anyway.
        let (pending, left): (Vec<_>, Vec<_>) =
            pending.into_iter().partition(|message| all || message.native_credit().unwrap_or(0) > fee.about);
        for message in &left {
            println!(
                "left waiting: {} to Private/{}: claiming it costs about {}, more than it holds (`names receive --all` claims it anyway)",
                message.native_credit().unwrap_or(0),
                message.body.to.account_id,
                fee.about
            );
        }
        if pending.is_empty() {
            println!("nothing worth claiming");
            return Ok(());
        }
        let receivers: std::collections::BTreeSet<AccountId> =
            pending.iter().map(|message| message.body.to.account_id).collect();
        let private_balance = |client: &Self, id: AccountId| {
            client
                .wallet
                .get_account_private(id)
                .and_then(|account| account.data.native_balance().ok())
                .unwrap_or(0)
        };
        let payer = match fee_from {
            Some(account) => {
                if !self.is_private(account) {
                    bail!("a claim's fee is paid inside its proof, so --fee-from must be a private account of this wallet");
                }
                account
            }
            None => {
                let ids: Vec<AccountId> =
                    self.wallet.storage().key_chain().private_account_ids().map(|(id, _)| id).collect();
                ids.into_iter()
                    .filter(|id| !receivers.contains(id))
                    .max_by_key(|id| private_balance(self, *id))
                    .ok_or_else(|| {
                        anyhow!("no other private account of this wallet can pay the claim fee; move some funds into one with `names setup --shield=<amount>`")
                    })?
            }
        };
        let can_pay = private_balance(self, payer);
        if can_pay < fee.at_most {
            bail!(
                "a claim costs at most {} (about {}), paid from Private/{payer}, which holds {can_pay}; move more into a private account first (`names setup --shield=<amount>`)",
                fee.at_most,
                fee.about
            );
        }
        println!("{}", fees::estimate_line(&fee, &format!("each claim, paid from Private/{payer} inside the proof")));
        self.wallet.set_private_fee_payer(Some(payer));
        for message in pending {
            let amount = message.native_credit().unwrap_or(0);
            let to = message.body.to.account_id;
            println!("receiving {amount} into Private/{to}; proving it on this computer (several minutes)");
            let (hash, _) = self
                .wallet
                .receive_pending_message(message)
                .await
                .context("the sequencer did not accept the claim")?;
            println!("submitted {hash}");
            let block = self.finish(hash).await.context("the claim was not applied")?;
            println!("received in block {block}; Private/{to} now holds {}", private_balance(self, to));
        }
        quietly(|| self.wallet.store_persistent_data())?;
        Ok(())
    }

    /// Where paying `name` goes, as `names pay` decides it (see [`pays_publicly`]): its
    /// public account, or its private receiving keys.
    async fn payee(&self, name: &str, public: bool, private: bool) -> Result<AccountIdentity> {
        let name = &self.following(name).await?;
        let (_, _, profile) = self.lookup(name).await?;
        if pays_publicly(&profile, public, private) {
            let to = profile
                .account
                .map(AccountId::new)
                .ok_or_else(|| anyhow!("{name} has no public account set"))?;
            return Ok(AccountIdentity::PublicNoSign(to));
        }
        private_keys_of(name, &profile)
    }

    /// The name's page, opened with its key, if it has one.
    async fn page(&self, name: &str) -> Result<Option<PageContent>> {
        let (Derived { id, key }, _, _) = self.lookup(name).await?;
        let bytes = self.actor_state(page_address(&self.program, &id)).await?;
        if bytes.is_empty() {
            return Ok(None);
        }
        let page: Page = borsh::from_slice(&bytes).context("the page account does not hold a page")?;
        Ok(open_page(&key, &page.sealed))
    }

    async fn resolve(&self, name: &str) -> Result<serde_json::Value> {
        let (Derived { id, key }, record, profile) = self.lookup(name).await?;
        let page_bytes = self.actor_state(page_address(&self.program, &id)).await?;
        let page = if page_bytes.is_empty() {
            None
        } else {
            let page: Page =
                borsh::from_slice(&page_bytes).context("the page account does not hold a page")?;
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
            "expires": (record.expires != u64::MAX).then_some(record.expires),
            // A name held by a deposit never expires (its expiry is u64::MAX).
            "expires_date": if record.expires == u64::MAX { "never".to_owned() } else { readable(record.expires) },
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

/// Whether paying a name goes to its public account: when asked (`--public`), or when the
/// name publishes no private receiving keys but a public account and `--private` was not
/// asked. Otherwise payments go to its private keys.
fn pays_publicly(profile: &Profile, public: bool, private: bool) -> bool {
    let has_keys = profile.npk.is_some() && !profile.vpk.is_empty();
    public || (!private && !has_keys && profile.account.is_some())
}

/// The name's private receiving keys as a payment destination. A credit to them stays
/// pending until the holder's wallet receives it.
fn private_keys_of(name: &str, profile: &Profile) -> Result<AccountIdentity> {
    let npk = profile.npk.ok_or_else(|| anyhow!("{name} has no private keys set"))?;
    let vpk = lee_core::encryption::ViewingPublicKey::from_bytes(profile.vpk.clone())
        .map_err(|error| anyhow!("{name} publishes an invalid viewing key: {error}"))?;
    Ok(AccountIdentity::PrivateForeign { npk: NullifierPublicKey(npk), vpk, kind: PrivateAccountKind::Regular })
}

/// What a page shown by `names open` may do: run its own inline script and style and
/// use data: URLs, nothing else. It loads nothing from the network, so opening a
/// name tells no third party, and the sandbox gives it an origin of its own.
const VIEWER_CSP: &str = "sandbox allow-scripts allow-popups; default-src 'none'; \
    script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; font-src data:; \
    media-src data:; form-action 'none'; base-uri 'none'";

/// Serves `page` to this machine only, until interrupted.
/// Escapes text for HTML: entries are written by whoever holds the name.
fn html_escape(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '&' => "&amp;".to_owned(),
            '<' => "&lt;".to_owned(),
            '>' => "&gt;".to_owned(),
            '"' => "&quot;".to_owned(),
            '\'' => "&#39;".to_owned(),
            c => c.to_string(),
        })
        .collect()
}

/// A profile card built on this machine from the name's own entries (`display`,
/// `bio`, `avatar` and the rest): nothing beyond the record is published or fetched.
fn profile_card(name: &str, profile: &Profile, has_page: bool, storage: &str) -> String {
    let text = |key: &str| profile.texts.iter().find(|t| t.key == key).map(|t| t.value.as_str());
    let display = text("display").unwrap_or(name);
    // Only an inline image: anything else would make the browser fetch it.
    let avatar = text("avatar")
        .filter(|v| {
            ["data:image/png;base64,", "data:image/jpeg;base64,", "data:image/gif;base64,", "data:image/webp;base64,"]
                .iter()
                .any(|prefix| v.starts_with(prefix))
        })
        .map(|v| format!("<img class=\"avatar\" src=\"{}\" alt=\"\">", html_escape(v)))
        .unwrap_or_default();
    let bio = text("bio").map(|v| format!("<p class=\"bio\">{}</p>", html_escape(v))).unwrap_or_default();
    let mut rows = Vec::new();
    if profile.npk.is_some() && !profile.vpk.is_empty() {
        rows.push(("pay privately".to_owned(), format!("<code>names pay {}{NAME_SUFFIX} &lt;amount&gt;</code>", html_escape(name))));
    }
    if let Some(account) = profile.account {
        rows.push(("public account".to_owned(), format!("<code>{}</code>", AccountId::new(account))));
    }
    for entry in &profile.texts {
        if ["display", "bio", "avatar", "color", "theme", "font"].contains(&entry.key.as_str()) {
            continue;
        }
        let mut value = format!("<code>{}</code>", html_escape(&entry.value));
        // Linked to the viewer's own Storage node only, never a public gateway, and
        // only for a plain CID, so an entry can't make the link point elsewhere.
        if entry.key == "cid" && !entry.value.is_empty() && entry.value.chars().all(|c| c.is_ascii_alphanumeric()) {
            value.push_str(&format!(
                " <a href=\"{}/api/storage/v1/data/{}/network/stream\">open from your Storage node</a>",
                html_escape(storage.trim_end_matches('/')),
                entry.value
            ));
        }
        if entry.key == "logos.account" {
            value.push_str(" <span class=\"note\">confirm with <code>names verify-account</code></span>");
        }
        rows.push((entry.key.clone(), value));
    }
    let rows: String = rows
        .iter()
        .map(|(key, value)| format!("<tr><th>{}</th><td>{value}</td></tr>", html_escape(key)))
        .collect();
    let page_link = if has_page { "<p class=\"more\"><a href=\"/page\">open the name's page</a></p>" } else { "" };
    // Styling entries are checked against fixed forms, so none can inject CSS.
    let accent = text("color")
        .filter(|v| v.len() == 7 && v.starts_with('#') && v[1..].chars().all(|c| c.is_ascii_hexdigit()))
        .unwrap_or("#3b6fd8");
    let font = match text("font") {
        Some("serif") => "ui-serif,Georgia,serif",
        Some("mono") => "ui-monospace,SFMono-Regular,Menlo,monospace",
        Some("rounded") => "ui-rounded,'SF Pro Rounded',system-ui,sans-serif",
        _ => "system-ui,-apple-system,'Segoe UI',sans-serif",
    };
    let light = "--bg:#f5f5f7;--card:#fff;--text:#1d1d1f;--muted:#6e6e73;--line:#e5e5ea";
    let dark = "--bg:#111;--card:#1c1c1e;--text:#f2f2f7;--muted:#98989f;--line:#2c2c2e";
    let theme = match text("theme") {
        Some("dark") => format!(":root{{{dark}}}"),
        Some("auto") => format!(":root{{{light}}}@media(prefers-color-scheme:dark){{:root{{{dark}}}}}"),
        _ => format!(":root{{{light}}}"),
    };
    format!(
        "<!doctype html><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\">\
         <title>{name}{NAME_SUFFIX}</title><style>{theme}:root{{--accent:{accent}}}\
         body{{font:16px/1.5 {font};margin:0;padding:3rem 1rem;color:var(--text);background:var(--bg)}}\
         .card{{max-width:34rem;margin:0 auto;background:var(--card);border-radius:18px;padding:2rem;\
         box-shadow:0 1px 3px rgba(0,0,0,.08);border-top:4px solid var(--accent)}}\
         .avatar{{width:96px;height:96px;border-radius:50%;object-fit:cover;image-rendering:pixelated;\
         border:3px solid var(--accent)}}h1{{margin:.75rem 0 0;font-size:1.6rem}}\
         .handle{{color:var(--accent);margin:0;font-weight:600}}.bio{{white-space:pre-wrap;margin:1rem 0}}\
         table{{border-collapse:collapse;width:100%;margin-top:1rem}}tr+tr td,tr+tr th{{border-top:1px solid var(--line)}}\
         th{{text-align:left;font-weight:500;color:var(--muted);padding:.6rem 1rem .6rem 0;vertical-align:top;white-space:nowrap}}\
         td{{padding:.6rem 0;word-break:break-all}}code{{font-size:.88em}}a{{color:var(--accent)}}\
         .note{{color:var(--muted);font-size:.85em}}.more{{margin:1.5rem 0 0}}</style>\
         <div class=\"card\">{avatar}<h1>{}</h1><p class=\"handle\">{name}{NAME_SUFFIX}</p>{bio}<table>{rows}</table>{page_link}</div>",
        html_escape(display),
        name = html_escape(name),
    )
}

fn serve_page(name: &str, page: Option<&PageContent>, card: &str, port: u16, no_browser: bool) -> Result<()> {
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
        } else if path == "/page" && page.is_some() {
            let page = page.expect("checked");
            ("200 OK", page.content_type.as_str(), &page.body)
        } else if path == "/" || path == "/profile" {
            ("200 OK", "text/html; charset=utf-8", card.as_bytes())
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

/// Runs `run` with this process's standard output sent nowhere. The LEZ wallet prints
/// its internals when it saves ("Stored persistent accounts at ...") and when it takes
/// in a private account's new state ("Received new acc Account { ... }"), which means
/// nothing to someone using `names` and would spoil JSON printed on stdout.
fn quietly<T>(run: impl FnOnce() -> T) -> T {
    use std::io::Write as _;
    use std::os::fd::AsRawFd as _;
    let _ = std::io::stdout().flush();
    let Ok(null) = std::fs::OpenOptions::new().write(true).open("/dev/null") else {
        return run();
    };
    // SAFETY: plain descriptor calls; stdout is put back before this returns.
    let saved = unsafe { libc::dup(libc::STDOUT_FILENO) };
    if saved < 0 || unsafe { libc::dup2(null.as_raw_fd(), libc::STDOUT_FILENO) } < 0 {
        if saved >= 0 {
            unsafe { libc::close(saved) };
        }
        return run();
    }
    let result = run();
    let _ = std::io::stdout().flush();
    unsafe {
        libc::dup2(saved, libc::STDOUT_FILENO);
        libc::close(saved);
    }
    result
}

/// Runs the LEZ wallet's command line with `args`, as its own `wallet` binary would.
async fn run_wallet(args: Vec<String>, overrides: Option<WalletConfigOverrides>) -> Result<()> {
    use clap::{CommandFactory as _, Parser as _};
    use wallet::{
        ExecutionFailureKind,
        cli::{Args, execute_continuous_run, execute_subcommand},
    };

    let Args { continuous_run, dry_run, fee_headroom, fee_payer, refund_to, command } =
        Args::parse_from(std::iter::once("names wallet".to_owned()).chain(args));
    let mut overrides = overrides.unwrap_or_default();
    if fee_headroom.is_some() {
        overrides.private_fee_headroom_blocks = fee_headroom;
    }
    let overrides = Some(overrides);
    write_default_wallet_files()?;
    let config_path = fetch_config_path()?;
    let storage_path = fetch_persistent_storage_path()?;

    if let Some(command) = command {
        let mut wallet = if storage_path.exists() {
            let password = read_password(&storage_path)?;
            WalletCore::new_update_chain(config_path, storage_path, overrides, &password).context(OLD_WALLET)?
        } else {
            let (mut wallet, mnemonic) =
                WalletCore::new_init_storage(config_path, storage_path, overrides, &new_wallet_password())?;
            wallet.start_at_chain_tip().await.ok();
            println!("Created a new wallet. Write down its recovery phrase and keep it safe;");
            println!("it is the only way to get the wallet back:");
            println!();
            println!("  {mnemonic}");
            println!();
            quietly(|| wallet.store_persistent_data())?;
            tighten_wallet_files();
            wallet
        };
        wallet.set_dry_run(dry_run);
        wallet.set_private_fee_payer(fee_payer);
        wallet.set_private_refund_target(refund_to);
        match execute_subcommand(&mut wallet, command).await {
            // A dry run ends the command on the error channel once it has reported.
            Err(error) if matches!(error.root_cause().downcast_ref(), Some(ExecutionFailureKind::DryRun)) => {}
            result => {
                result?;
            }
        }
    } else if continuous_run {
        let password = read_password(&storage_path)?;
        let mut wallet = WalletCore::new_update_chain(config_path, storage_path, overrides, &password).context(OLD_WALLET)?;
        execute_continuous_run(&mut wallet).await?;
    } else {
        let mut help = Args::command().name("names wallet");
        println!("{}", help.render_long_help());
    }
    Ok(())
}

/// Why an existing wallet may not open: LEZ 0.4 cannot read a 0.3 wallet's file.
const OLD_WALLET: &str = "cannot open the wallet; a wallet from LEZ 0.3 does not load on 0.4: move it aside and bring its \
    public accounts back from its recovery phrase with `names wallet restore-keys`";

/// The password of a new wallet: `LEE_WALLET_PASSWORD` if set, else none, which leaves
/// its storage unencrypted. Nothing prompts, because apps run `names` without a terminal.
fn new_wallet_password() -> String {
    std::env::var(PASSWORD_ENV_VAR).unwrap_or_default()
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
    tighten_wallet_files();
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
    tighten_wallet_files();
    Ok(())
}

/// Makes the wallet home readable by its owner alone: the folder 0700, the wallet, its
/// config and `names.json` 0600. Older CLIs left them 0644 in a 0755 folder (SF-4).
fn tighten_wallet_files() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let Ok(storage) = fetch_persistent_storage_path() else { return };
        let Some(home) = storage.parent() else { return };
        let set = |path: &std::path::Path, mode: u32| {
            if let Ok(meta) = std::fs::symlink_metadata(path) {
                // Never through a symlink, and only what is too open.
                if !meta.file_type().is_symlink() && meta.permissions().mode() & 0o077 != 0 {
                    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
                }
            }
        };
        set(home, 0o700);
        for file in [storage.clone(), storage.with_file_name("names.json")] {
            set(&file, 0o600);
        }
        if let Ok(config) = fetch_config_path() {
            set(&config, 0o600);
        }
    }
}

/// Opens the wallet at the wallet home. Without one, makes a new wallet and saves it,
/// or, for a lookup (`save` false), uses an empty one that is never saved or shown,
/// so lookups need no setup and leave nothing behind.
async fn open_wallet(overrides: Option<WalletConfigOverrides>, save: bool, create: bool) -> Result<WalletCore<SequencerClient>> {
    write_default_wallet_files()?;
    let storage_path = fetch_persistent_storage_path()?;
    // Only `names setup` makes a wallet: any other command that would then fail left a
    // new wallet behind and its recovery phrase in whatever logged it (SC-4).
    if save && !create && !storage_path.exists() {
        bail!("there is no wallet here yet: run `names setup` first (it makes one and shows its recovery phrase once)");
    }
    if storage_path.exists() {
        let password = read_password(&storage_path)?;
        match WalletCore::new_update_chain(fetch_config_path()?, storage_path.clone(), overrides.clone(), &password) {
            Ok(wallet) => return Ok(wallet),
            Err(error) if save => return Err(error).context(OLD_WALLET),
            // Reading the chain signs nothing, so a wallet that won't load (one from
            // LEZ 0.3) mustn't stop it: use a throwaway one, which is never saved.
            Err(_) => {}
        }
    }
    let (mut wallet, mnemonic) =
        WalletCore::new_init_storage(fetch_config_path()?, storage_path, overrides, &new_wallet_password())?;
    if !save {
        return Ok(wallet);
    }
    // A new wallet's keys can't have anything in past blocks, so start syncing from
    // now instead of scanning the whole chain (minutes on the testnet).
    wallet.start_at_chain_tip().await.ok();
    println!("Created a new wallet. Write down its recovery phrase and keep it safe;");
    println!("it is the only way to get the wallet back:");
    println!();
    println!("  {mnemonic}");
    println!();
    quietly(|| wallet.store_persistent_data())?;
    tighten_wallet_files();
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
/// The least gas limit a registry call is sent with, so a small mis-estimate (the
/// chain changing between the run here and the sequencer's) can't fail it. The
/// whole limit is held from the payer while the call runs (at up to 9 a unit, so
/// 200,000 held 1.8M even for a commit that uses 23,214), and the 1.5x margin
/// already covers drift, so this floor is kept small. Measured on the testnet
/// (2026-10-19): commit 23,214, release 51,322, profile change 65,590, register
/// about 165,000.
const MIN_GAS_LIMIT: u64 = 30_000;

/// The gas limit for a call measured at `cycles`: 1.5 x, rounded up, at least
/// [`MIN_GAS_LIMIT`].
fn gas_limit_for(cycles: u64) -> u64 {
    cycles.saturating_add(cycles.div_ceil(2)).max(MIN_GAS_LIMIT)
}

/// Execution gas for a public native-token transfer: the native token is a built-in
/// that meters no cycles (only the transaction's bytes are billed, outside this), and
/// the whole limit is reserved from the sender while the transfer runs.
const NATIVE_TRANSFER_GAS_LIMIT: u64 = 1_000;

/// `names setup`: a public and a private account, and, once the public one is funded,
/// some of its tokens moved to the private account so names can be held and paid privately.
async fn setup(client: &mut Client, shield: u128) -> Result<()> {
    let mut defaults = load_defaults();
    let public: AccountId = match defaults.public.as_deref().and_then(|id| id.parse().ok()) {
        Some(id) => id,
        None => client.wallet.create_new_account_public(None).0,
    };
    let private: AccountId = match defaults.private.as_deref().and_then(|id| id.parse().ok()) {
        Some(id) => id,
        None => client.wallet.create_new_account_private(None).0,
    };
    quietly(|| client.wallet.store_persistent_data())?;
    defaults.public = Some(public.to_string());
    defaults.private = Some(private.to_string());
    save_defaults(&defaults)?;
    println!("public account   Public/{public}");
    println!("private account  Private/{private}");

    let balance = client.wallet.get_account_balance(public).await.unwrap_or(0);
    if balance < shield {
        println!(
            "Public/{public} holds {balance}; send it at least {shield} testnet tokens from your LEZ \
             wallet (or with `names bridge`), then run `names setup` again. A name is held by a \
             refundable deposit (`names settings` shows it), so the private account needs at least that"
        );
        return Ok(());
    }
    if shield > 0 {
        println!("moving {shield} to the private account; generating the proof (a few minutes)");
        let (hash, _) = NativeTokenTransfer(&client.wallet)
            .transfer(
                AccountIdentity::Public(public),
                AccountIdentity::PrivateOwned(private),
                shield,
                CreditDelivery::Automatic,
            )
            .await
            .context("the sequencer did not accept the transfer")?;
        client.finish(hash).await.context("the transfer was not applied")?;
        client.wallet.sync_to_latest_block().await?;
        quietly(|| client.wallet.store_persistent_data())?;
    }
    println!("ready: `names register <name>` holds a name privately, `names pay <name> <amount>` pays one");
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
        Command::Wallet { args } => {
            // Native-token transfers meter no execution gas, and the whole limit is
            // reserved while one runs, so keep the limit tiny for them.
            let mut overrides = overrides.unwrap_or_default();
            if args.iter().any(|arg| arg == "auth-transfer") && overrides.gas_limit.is_none() {
                overrides.gas_limit = Some(NATIVE_TRANSFER_GAS_LIMIT);
            }
            return run_wallet(args, Some(overrides)).await;
        }
        Command::Bridge { amount, to, from_key, node, max_fee, yes } => {
            let rpc = cli.rpc.as_deref().unwrap_or(DEFAULT_RPC);
            return bridge(rpc, &node, amount, &to, &from_key, max_fee, yes);
        }
        // Only a name target needs the registry; everything else is between you and your node.
        Command::Chain(command) if command.name().is_none() => return chain::run(command.clone(), None),
        // Runs where the wallet does not open, so it can't wait for the client below.
        Command::Module(module::Request::Restore { accounts }) => return module::restore(accounts, overrides).await,
        // Needs no wallet: the Basecamp module asks for a proof here, so it needn't carry the list.
        Command::Module(module::Request::LengthProof { id }) => {
            let id = parse_hex32(id.trim()).ok_or_else(|| anyhow!("an identifier is 32 bytes of hex"))?;
            let (proof, tier) = length_proof(&id)?;
            println!("{}", module::length_proof_json(&proof, tier));
            return Ok(());
        }
        _ => {}
    }
    let program = match cli.program {
        Some(program) => program,
        None => DEFAULT_PROGRAM.parse().map_err(|_| anyhow!("bad built-in program ID"))?,
    };

    if let Command::ClaimsRoot { snapshot } = &cli.command {
        let snapshot = claims::Snapshot::load(snapshot)?;
        println!("root    {}", hex::encode(snapshot.root()));
        println!("program {}", snapshot.program);
        println!("height  {}", snapshot.height);
        println!("names   {}", snapshot.leaves().len());
        return Ok(());
    }
    if let Command::AccountLogEntry { name } = &cli.command {
        println!("{}", hex::encode(names_accountlog::name_entry(&format!("{name}{LEGACY_SUFFIX}"))));
        return Ok(());
    }
    if let Command::Address { name } = &cli.command {
        let Derived { id, .. } = derive(name);
        println!("id     {}", hex::encode(id));
        println!("record {}", record_address(&program, &id));
        println!("page   {}", page_address(&program, &id));
        println!("deposit {}", deposit_address(&program, &id));
        return Ok(());
    }

    let mut client = Client {
        wallet: open_wallet(
            overrides,
            !matches!(
                cli.command,
                Command::Resolve { .. }
                    | Command::Chain(_)
                    | Command::Open { .. }
                    | Command::Settings
                    | Command::VerifyAccount { .. }
                    | Command::Token(tokens::TokenCommand::Info { .. })
                    | Command::Module(
                        module::Request::AccountNew { .. }
                            | module::Request::AccountLabel { .. }
                            | module::Request::AccountUse { .. }
                            | module::Request::Fee
                    )
            ),
            matches!(cli.command, Command::Setup { .. }),
        )
        .await?,
        program,
    };

    match cli.command {
        Command::Setup { shield } => setup(&mut client, shield).await,
        Command::Chain(command) => {
            let asked = command.name().expect("key targets ran before the wallet opened").to_owned();
            let name = client.following(&parse_name(&asked).map_err(|e| anyhow!(e))?).await?;
            let (_, _, profile) = client.lookup(&name).await?;
            let key = chain::key_of(&profile).ok_or_else(|| {
                anyhow!("{name} publishes no Logos Blockchain key; its holder can add one with `names set-text {name} {} <key>`", chain::BLOCKCHAIN_KEY)
            })?;
            chain::run(command, Some((name, key)))
        }
        Command::Token(command) => {
            let tokens = tokens::Tokens::select(cli.token_program, &cli.token_codec)?;
            client.token(&tokens, command).await
        }
        Command::Module(module::Request::Token(request)) => {
            let tokens = tokens::Tokens::select(cli.token_program, &cli.token_codec)?;
            client.module_token(&tokens, request).await
        }
        Command::Module(request) => client.module(request).await,
        Command::Balance => {
            // A sequencer that doesn't answer is said so, never read as balances of 0: the
            // Basecamp module takes every rise between two reads for money that arrived.
            let synced = client.wallet.sync_to_latest_block().await;
            let defaults = load_defaults();
            let key_chain = client.wallet.storage().key_chain();
            let publics: Vec<AccountId> = key_chain.public_account_ids().map(|(id, _)| id).collect();
            let privates: Vec<AccountId> = key_chain.private_account_ids().map(|(id, _)| id).collect();
            let mut total_private = 0_u128;
            let mut unread = 0_usize;
            println!("public accounts (seen on chain):");
            for id in publics {
                let tag = if defaults.public.as_deref() == Some(&id.to_string()) { "  (default)" } else { "" };
                match client.wallet.get_account_balance(id).await {
                    Ok(balance) => println!("  Public/{id}  {balance}{tag}"),
                    Err(_) => {
                        unread += 1;
                        println!("  Public/{id}  unknown{tag}");
                    }
                }
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
            // Payments to your name's private keys wait until you claim them (SC-1); those
            // worth less than a claim costs (a fee's refund) are told apart.
            if let Ok(pending) = client.wallet.owned_pending_messages().await {
                let credits: Vec<u128> = pending.iter().filter_map(|message| message.native_credit()).collect();
                let claim = match client.wallet.client().get_fee_state().await {
                    Ok(quote) => fees::private(&quote, client.wallet.config().private_fee_headroom_blocks).about,
                    Err(_) => 0,
                };
                let (worth, small): (Vec<u128>, Vec<u128>) = credits.into_iter().partition(|amount| *amount > claim);
                if !worth.is_empty() {
                    println!(
                        "waiting to be claimed: {} payment(s), {} in all; run `names receive`",
                        worth.len(),
                        worth.iter().sum::<u128>()
                    );
                }
                if !small.is_empty() {
                    println!(
                        "also waiting, worth less than claiming costs (about {claim}): {} credit(s), {} in all",
                        small.len(),
                        small.iter().sum::<u128>()
                    );
                }
            }
            balance_read(synced.is_ok(), unread)
        }
        Command::Receive { fee_from, list, all } => client.receive(fee_from, list, all).await,
        Command::Mine { add } => {
            if let Some(name) = add {
                let (Derived { id, .. }, record, _) = client.lookup(&name).await?;
                client
                    .holder(&name, &id, &record)
                    .with_context(|| format!("this wallet doesn't hold {name}{NAME_SUFFIX}, so it isn't added"))?;
                let mut defaults = load_defaults();
                if !defaults.names.contains(&name) {
                    defaults.names.push(name.clone());
                    save_defaults(&defaults)?;
                }
                println!("added {name}{NAME_SUFFIX}: this wallet holds it");
            }
            let defaults = load_defaults();
            for (name, pending) in &defaults.pending {
                match &pending.failed {
                    Some(why) => println!(
                        "{name}{NAME_SUFFIX}  NOT registered: {why}; run `names register {name}` to try again (log: {})",
                        register_log(name)?.display()
                    ),
                    None => println!(
                        "{name}{NAME_SUFFIX}  registering, {} (log: {})",
                        until(pending.made.saturating_add(MIN_COMMIT_AGE_MS)),
                        register_log(name)?.display()
                    ),
                }
            }
            if defaults.names.is_empty() && defaults.pending.is_empty() {
                println!("no names yet; register one with `names register <name>`");
                println!("(a name someone transferred to you shows here once you run `names mine --add <name>`)");
            }
            for name in defaults.names {
                match client.lookup(&name).await {
                    Ok((Derived { id, .. }, record, _)) if client.holder(&name, &id, &record).is_ok() => {
                        // Apps read this suffix to tell a private holder from a public one.
                        let by = match client.holder(&name, &id, &record) {
                            Ok(account) if client.is_private(account) => "; held privately",
                            _ => "; held by a public account",
                        };
                        if record.expires == u64::MAX {
                            println!("{name}{NAME_SUFFIX}  held by a deposit (no expiry){by}");
                        } else {
                            let state = if local_now() >= record.expires { "lapsed" } else { "held" };
                            println!("{name}{NAME_SUFFIX}  {state} until {}{by}", readable(record.expires));
                        }
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
            public,
        } => {
            let owner = match owner {
                Some(owner) => owner,
                None => default_private()?,
            };
            if public {
                let mut defaults = load_defaults();
                if !defaults.then_public.contains(&name) {
                    defaults.then_public.push(name.clone());
                    save_defaults(&defaults)?;
                }
            }
            if !client.register(&name, owner, term, now, wait, cli.rpc.as_deref()).await? {
                return Ok(());
            }
            let mut defaults = load_defaults();
            if !defaults.names.contains(&name) {
                defaults.names.push(name.clone());
            }
            let convert = defaults.then_public.contains(&name);
            defaults.then_public.retain(|queued| *queued != name);
            save_defaults(&defaults)?;
            // Registered privately first, so nothing showed who registered it; now
            // moved to a public account that a private transfer funds.
            if convert {
                client.make_public(&name).await?;
            }
            Ok(())
        }
        Command::Renew { name, term, payer } => client.renew(&name, term, payer).await,
        Command::Claim { name, snapshot } => client.claim(&name, &snapshot).await,
        Command::Configure {
            admin,
            treasury,
            prices,
            mode,
            deposits,
            signer,
            claims_snapshot,
            claims_root,
            claims_program,
            claims_until,
        } => {
            let claims = claims::ClaimSettings::from_args(claims_snapshot, claims_root, claims_program, claims_until)?;
            let prices = <[u128; 3]>::try_from(prices)
                .map_err(|_| anyhow!("--prices takes three prices: 30d,1y,5y"))?;
            let deposits = <[u128; 2]>::try_from(deposits)
                .map_err(|_| anyhow!("--deposits takes two amounts: normal,three-character"))?;
            let mode = if mode == "deposit" { MODE_DEPOSIT } else { MODE_FEE };
            client.configure(admin, treasury, prices, mode, deposits, signer, claims).await
        }
        Command::Settings => {
            let settings = client.settings().await?.map(|settings| {
                serde_json::json!({
                    "admin": settings.admin.to_string(),
                    "mode": if settings.mode == MODE_DEPOSIT { "deposit" } else { "fee" },
                    "deposits": {
                        "normal": settings.deposits[0].to_string(),
                        "three_characters": settings.deposits[1].to_string(),
                    },
                    "treasury": settings.treasury.to_string(),
                    "prices": {
                        "30d": settings.prices[0].to_string(),
                        "1y": settings.prices[1].to_string(),
                        "5y": settings.prices[2].to_string(),
                    },
                    "claims": {
                        "root": hex::encode(settings.claims_root),
                        "until": settings.claims_until,
                        "program": settings.claims_program.to_string(),
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
            // Checked first, so a name this wallet doesn't hold leaves no new account behind (SC-7).
            let (Derived { id, .. }, record, _) = client.lookup(&name).await?;
            client.holder(&name, &id, &record)?;
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
            // A name handed to this wallet is listed by `names mine` from now on.
            let mut defaults = load_defaults();
            if !defaults.names.contains(&name) {
                defaults.names.push(name.clone());
                save_defaults(&defaults)?;
            }
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
            // Log entries carry the name in its long form, `jef.lez` (SPEC 7); a bare one counts too.
            let full_name = format!("{name}{LEGACY_SUFFIX}");
            let endorsed = names_accountlog::endorses_name(&live, &full_name) || names_accountlog::endorses_name(&live, &name);
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
        Command::Profile { name, display, bio, avatar, theme, color, font, cid, set, clear, replace } => {
            let mut changes: Vec<(String, String)> = Vec::new();
            for (key, value) in [("display", display), ("bio", bio), ("theme", theme), ("color", color), ("font", font), ("cid", cid)] {
                if let Some(value) = value {
                    changes.push((key.to_owned(), value));
                }
            }
            if let Some(path) = avatar {
                changes.push(("avatar".to_owned(), image_data_url(&path)?));
            }
            changes.extend(set);
            if changes.is_empty() && clear.is_empty() && !replace {
                bail!("nothing to change: give at least one entry, --set, --clear or --replace");
            }
            client
                .edit(&name, |profile| {
                    if replace {
                        profile.texts.clear();
                    }
                    for key in &clear {
                        profile.texts.retain(|text| &text.key != key);
                    }
                    for (key, value) in changes {
                        match profile.texts.iter_mut().find(|text| text.key == key) {
                            Some(text) => text.value = value,
                            None => profile.texts.push(TextRecord { key, value }),
                        }
                    }
                    Ok(())
                })
                .await
        }
        Command::SetText { name, key, value } => {
            validate_text_key(&key).map_err(|reason| anyhow!(reason))?;
            let value = if key == chain::BLOCKCHAIN_KEY && !value.is_empty() {
                chain::parse_key(&value).ok_or_else(|| anyhow!("a `{}` entry is a Logos Blockchain public key: 64 hex characters", chain::BLOCKCHAIN_KEY))?
            } else {
                value
            };
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
        Command::Forward { name, to, clear } => {
            let to = if clear { String::new() } else { to.expect("clap requires TO without --clear") };
            if to == name {
                bail!("a name can't forward to itself");
            }
            if !to.is_empty() {
                client.lookup(&to).await.map_err(|_| anyhow!("{to} is not registered, so {name} can't point to it"))?;
            }
            client
                .edit(&name, |profile| {
                    let existing = profile.texts.iter().position(|text| text.key == FORWARD_KEY);
                    match (existing, to.is_empty()) {
                        (Some(index), true) => {
                            profile.texts.remove(index);
                        }
                        (Some(index), false) => profile.texts[index].value = to.clone(),
                        (None, true) => bail!("{name} doesn't forward anywhere"),
                        (None, false) => profile.texts.push(TextRecord { key: FORWARD_KEY.to_owned(), value: to.clone() }),
                    }
                    Ok(())
                })
                .await
        }
        Command::Transfer { name, new_owner } => client.transfer(&name, new_owner).await,
        Command::Release { name } => client.release(&name).await,
        Command::Open { name, port, no_browser, storage } => {
            let name = client.following(&name).await?;
            let (_, _, profile) = client.lookup(&name).await?;
            let page = client.page(&name).await?;
            let card = profile_card(&name, &profile, page.is_some(), &storage);
            serve_page(&name, page.as_ref(), &card, port, no_browser)
        }
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
            let name = client.following(&name).await?;
            let (_, _, profile) = client.lookup(&name).await?;
            if pays_publicly(&profile, public, private) {
                client.pay(&name, amount, from).await
            } else {
                client.pay_private(&name, amount, from).await
            }
        }
        Command::Resolve { name } => {
            let used = client.following(&name).await?;
            let mut resolved = client.resolve(&used).await?;
            if used != name {
                resolved["forwarded_from"] = name.into();
            }
            println!("{}", serde_json::to_string_pretty(&resolved)?);
            Ok(())
        }
        Command::Post { as_name, text, cids, reply } => client.post(&as_name, text, cids, reply).await,
        Command::Like { as_name, post, undo } => client.like(&as_name, post, undo).await,
        Command::Hide { post } => client.hide(post).await,
        Command::Anon { public } => client.anon(public, cli.rpc.as_deref()).await,
        Command::MakePublic { name } => client.make_public(&name).await,
        Command::Stats { blocks } => client.stats(blocks).await,
        Command::Feed { following, blocks, json } => {
            board::print_posts(&client.feed(blocks, &following).await?, json)
        }
        Command::PostsOf { name, before, count, json } => {
            let (total, posts) = client.posts_of(&name, before, count.min(50)).await?;
            if json {
                println!("{}", serde_json::json!({ "total": total, "posts": posts }));
                Ok(())
            } else {
                println!("{name} has {total} post{}", if total == 1 { "" } else { "s" });
                board::print_posts(&posts, false)
            }
        }
        Command::Thread { post, blocks, json } => board::print_posts(&client.thread(post, blocks).await?, json),
        Command::Address { .. }
        | Command::ClaimsRoot { .. }
        | Command::AccountLogEntry { .. }
        | Command::Wallet { .. }
        | Command::Bridge { .. }
        | Command::Man { .. } => {
            unreachable!("handled before the wallet is opened")
        }
    }
}

#[cfg(test)]
mod shortlist_tests {
    /// The list shipped in the CLI is the one the program's root commits to, and it
    /// sorts names into the right tiers.
    #[test]
    fn the_published_list_matches_the_program() {
        // The repository's copy, as CI and the e2e run use it (NAMES_SHORTLIST).
        // SAFETY: set before any thread reads the environment in this test binary's tests.
        unsafe { std::env::set_var("NAMES_SHORTLIST", concat!(env!("CARGO_MANIFEST_DIR"), "/../shortlist/shortlist.bin")) };
        let list = super::short_list().expect("the list loads");
        assert_eq!(list.root(), names_core::SHORTLIST_ROOT);
        // 1-4 characters and the reserved list are locked; anything else is a normal name.
        for (name, tier) in [
            ("ab", names_core::TIER_LOCKED),
            ("jef", names_core::TIER_LOCKED),
            ("apps", names_core::TIER_LOCKED),
            ("logos", names_core::TIER_LOCKED),
            ("names", names_core::TIER_LOCKED),
            ("nameswallet", names_core::TIER_LOCKED),
            ("logosappstore", names_core::TIER_LOCKED),
            ("walletci", names_core::TIER_NORMAL),
        ] {
            let (_, got) = super::length_proof(&names_seal::derive(name).id).expect("a proof");
            assert_eq!(got, tier, "{name}");
        }
    }
}

/// What a bridge deposit's metadata is: the Names module builds it from the account's 32
/// bytes, so they must be exactly what `names bridge` sends (`borsh::to_vec(&to)`).
#[cfg(test)]
mod bridge_metadata_tests {
    use super::{AccountId, parse_account};

    #[test]
    fn a_deposits_metadata_is_the_accounts_32_bytes_raw() {
        let bytes: [u8; 32] = std::array::from_fn(|i| u8::try_from(i * 7 + 1).unwrap_or(0));
        assert_eq!(borsh::to_vec(&AccountId::new(bytes)).expect("borsh"), bytes.to_vec(), "no length prefix, no tag");
        let to = parse_account("Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk").expect("an account");
        assert_eq!(borsh::to_vec(&to).expect("borsh").len(), 32);
    }
}

#[cfg(test)]
mod new_owner_tests {
    use super::parse_new_owner;

    /// transfer-case-mangled-id-loses-name: `names transfer xferalpha Public/<lower-cased
    /// id>` and the all-zero account reached the send (only the dry run stopped them).
    #[test]
    fn a_new_holder_that_was_mangled_or_is_empty_is_refused() {
        let held = "9Dk3PQD467hXAbsbMzrFYVPu5HFvUqKkU7VdoQD9oirA";
        assert!(parse_new_owner(&format!("Public/{held}")).is_ok());
        assert!(parse_new_owner(&format!("Private/{held}")).is_ok());
        assert!(parse_new_owner(held).is_ok(), "the CLI still takes a bare ID");
        let lower = held.to_ascii_lowercase();
        assert!(lower.parse::<super::AccountId>().is_ok(), "it still decodes");
        for bad in [format!("Public/{lower}"), lower.clone(), held.to_ascii_uppercase(), "11111111111111111111111111111111".to_owned()] {
            assert!(parse_new_owner(&bad).is_err(), "{bad}");
        }
        assert!(parse_new_owner(&format!("Private/Public/{held}")).is_err(), "a prefix is written once");
    }
}

#[cfg(test)]
mod balance_tests {
    use super::balance_read;

    /// The Basecamp module logged every account's whole balance as an arrival after one
    /// `names balance` with the sequencer down printed 0 for each and exited 0.
    #[test]
    fn a_balance_the_sequencer_did_not_give_fails_the_command() {
        assert!(balance_read(true, 0).is_ok());
        let unread = balance_read(true, 3).expect_err("unknown balances fail").to_string();
        assert!(unread.contains("3 public balance(s) are unknown"), "{unread}");
        assert!(balance_read(false, 0).is_err(), "not caught up with the chain fails too");
    }
}

#[cfg(test)]
mod pay_tests {
    use super::{Profile, TextRecord, forward_of, pays_publicly, private_keys_of};

    /// SPEC.md §7, Forwarding: a valid other name, normalised; anything else is ignored.
    #[test]
    fn a_forward_names_another_valid_name() {
        let with = |value: &str| Profile {
            texts: vec![TextRecord { key: "forward".into(), value: value.into() }],
            ..Profile::default()
        };
        assert_eq!(forward_of("nameswallet", &with("names")).as_deref(), Some("names"));
        assert_eq!(forward_of("nameswallet", &with("  @Names ")).as_deref(), Some("names"), "normalised");
        assert_eq!(forward_of("names", &with("names")), None, "not to itself");
        for bad in ["", "-bad", "a b", "Has.Dots.lez.x", &"x".repeat(33)] {
            assert_eq!(forward_of("a", &with(bad)), None, "{bad:?}");
        }
        assert_eq!(forward_of("a", &Profile::default()), None);
    }

    /// `names pay` and `names token send` pick a name's destination the same way.
    #[test]
    fn a_name_is_paid_privately_unless_it_has_only_a_public_account() {
        let both = Profile { npk: Some([1; 32]), vpk: vec![2; 33], account: Some([3; 32]), ..Profile::default() };
        assert!(!pays_publicly(&both, false, false), "private keys are the default");
        assert!(pays_publicly(&both, true, false), "--public asks for the account");
        let public_only = Profile { account: Some([3; 32]), ..Profile::default() };
        assert!(pays_publicly(&public_only, false, false));
        assert!(!pays_publicly(&public_only, false, true), "--private insists on keys");
        assert!(private_keys_of("alice", &public_only).is_err(), "and there are none");
        let keys_only = Profile { npk: Some([1; 32]), vpk: vec![2; 33], ..Profile::default() };
        assert!(!pays_publicly(&keys_only, false, false));
    }
}
