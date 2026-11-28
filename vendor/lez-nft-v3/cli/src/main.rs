//! `lez-nft`: command-line client for the LEZ NFT program (v3, LEZ v0.4.0).
//!
//! Signing goes through the LEZ wallet at `LEE_WALLET_HOME_DIR` (default
//! `~/.lee/wallet`). A command signed by a private account is proven on this
//! machine and the chain never learns the account; a public one is sent as is.
//! `lez-nft wallet` is the wallet's own command line, built in.
//!
//! With `--json`, progress goes to stderr and the result is one JSON object on
//! stdout, for the standalone NFT module.
//!
//! Sealed (encrypted) tokens: the content key is wrapped for each reader with
//! ML-KEM-768 to their private account's viewing key, the same KEM the wallet
//! uses for notes. Content and image are AES-256-GCM under that key.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use aes_gcm::{Aes256Gcm, KeyInit as _, Nonce, aead::Aead as _};
use anyhow::{Context as _, Result, anyhow, bail};
use clap::{Parser, Subcommand};
use common::transaction::LeeTransaction;
use lee::{
    privacy_preserving_transaction::circuit::{Dependency, ProgramCatalog, ProgramKind},
    program::Program,
};
use lee_core::{
    EphemeralPublicKey, NullifierPublicKey, PrivateAccountKind, SharedSecretKey,
    account::{Account, AccountId, Actor},
    encryption::ViewingPublicKey,
    execution_state::BoundaryStep,
    program::PdaSeed,
};
use lez_nft_core::{
    AuthorityOp, AuthorizedProgram, CollectionId, CollectionRecord, Effect, Instruction,
    LinkedAuthority, Message, NftRecord, Payee, TokenId, collection_address,
    collection_seed, config_address, config_seed, deed_address, deed_seed, linked_owner_tag,
    owner_tag, record_owner_tag, serial_token_id, token_address, token_seed,
};
use sequencer_service_rpc::SequencerClient;
use serde_json::{Value, json};
use wallet::{
    AccountIdentity, AccountMention, AsyncDelivery, WalletCore, WalletSequencerClient as _,
    config::{SequencerConnectionData, WalletConfigOverrides},
    program_facades::{
        CreditDelivery, native_token_transfer::NativeTokenTransfer, program_loader::ProgramLoader,
    },
    utils::{fetch_config_path, fetch_persistent_storage_path, read_password},
};

/// Program v3 placeholder (set `LEZ_NFT_PROGRAM` or `--program` after deploying v3 on LEZ v0.4.0).
const PROGRAM_ID: &str = "9W9huNeAds11pdGpMijrdhTkSQ8upNudRnrZJbNNWfoW";
/// First block to scan for v3 deployments (override via `LEZ_NFT_DEPLOY_BLOCK`).
const DEPLOY_BLOCK: u64 = 1;
const DEFAULT_RPC: &str = "https://testnet.lez.logos.co";
const RECEIVE_PREFIX_V3: &str = "lez-nft-recv3:";
const NONCE_LEN: usize = 12;
const CHECKSUM_LEN: usize = 4;

static JSON: AtomicBool = AtomicBool::new(false);

/// Progress for people: stdout normally, stderr under `--json`.
macro_rules! say {
    ($($arg:tt)*) => {
        if JSON.load(Ordering::Relaxed) { eprintln!($($arg)*) } else { println!($($arg)*) }
    };
}

#[derive(Parser)]
#[command(name = "lez-nft", about = "Launch, mint, hold, shield and send NFTs on the Logos Execution Zone (v3)")]
struct Cli {
    /// Program account ID.
    #[arg(long, env = "LEZ_NFT_PROGRAM", default_value = PROGRAM_ID, global = true)]
    program: AccountId,

    /// Sequencer RPC URL.
    #[arg(long, env = "LEZ_NFT_RPC", default_value = DEFAULT_RPC, global = true)]
    rpc: String,

    /// Print the result as one JSON object on stdout.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

/// Who signs: an account in the wallet (`Public/…` or `Private/…`), `public`
/// for the first public account, or `private` for the first private one (made
/// if the wallet has none).
const FROM_HELP: &str = "Signing account: Public/…, Private/…, `public` or `private`";

#[derive(Subcommand)]
enum Command {
    /// Set up this wallet once: makes it if missing, with one public account
    /// (to top up from) and one private account (to hold funds and mint from).
    /// Prints the private account's receive code. Safe to run again.
    Setup,

    /// The wallet's accounts and balances (private balances after a sync).
    Accounts,

    /// Move funds from the wallet's public account into its private account.
    Shield {
        #[arg(long)]
        amount: u128,
    },

    /// A code others use to send you a token, sealed or not, at a private account.
    ReceiveCode {
        /// A private account in the wallet; default the first one (made if none).
        #[arg(long)]
        account: Option<String>,
    },

    /// Initialize or update program config (authorized linked authorities such as LNS).
    Configure {
        /// Admin account (must sign on initial configure).
        #[arg(long, default_value = "public", help = FROM_HELP)]
        from: String,
        /// Optional new admin account (defaults to signer).
        #[arg(long)]
        admin: Option<String>,
        /// Authorized linking program IDs (`<program_id>` or `<program_id>:<collection_id_hex>`).
        #[arg(long = "authority")]
        authorities: Vec<String>,
    },

    /// Open a collection for sale.
    Create {
        /// The collection's content: JSON text, or @file.
        #[arg(long)]
        content: String,
        #[arg(long)]
        supply: u32,
        /// Price per token in the native token's base units; 0 is free.
        #[arg(long, default_value_t = 0)]
        price: u128,
        /// Who receives payments: a public account, a receive code, or `private`
        /// for this wallet's first private account (default: the first public account).
        #[arg(long)]
        payee: Option<String>,
        #[arg(long, default_value = "public", help = FROM_HELP)]
        from: String,
        /// Send to an address even if nothing shows it is real.
        #[arg(long)]
        force: bool,
    },

    /// Show a collection.
    Collection {
        #[arg(long, value_parser = parse_hex32)]
        id: [u8; 32],
    },

    /// Mint the next token of a collection, paying its price.
    Buy {
        #[arg(long, value_parser = parse_hex32)]
        collection: [u8; 32],
        #[arg(long, default_value = "public", help = FROM_HELP)]
        from: String,
        /// Send it to someone else: an account or a receive code.
        #[arg(long)]
        to: Option<String>,
        /// Send to an address even if nothing shows it is real.
        #[arg(long)]
        force: bool,
    },

    /// Mint a one-off token.
    Mint {
        /// The token's content: JSON text, or @file.
        #[arg(long)]
        content: String,
        #[arg(long, default_value = "public", help = FROM_HELP)]
        from: String,
        /// The owner: an account or a receive code. Default the signer.
        #[arg(long)]
        to: Option<String>,
        /// Seal the content: only the owner (and a private minter) can read it.
        #[arg(long)]
        encrypt: bool,
        /// The content key made by `seal-file`, so content and image share it.
        #[arg(long, requires = "encrypt")]
        key_file: Option<PathBuf>,
        /// Token ID (64 hex); default random.
        #[arg(long, value_parser = parse_hex32)]
        token_id: Option<[u8; 32]>,
        /// Send to an address even if nothing shows it is real.
        #[arg(long)]
        force: bool,
    },

    /// Encrypt a file with a new content key, for a sealed mint.
    SealFile {
        #[arg(long = "in")]
        input: PathBuf,
        #[arg(long)]
        out: PathBuf,
        /// Where to write the key (default `<out>.key`).
        #[arg(long)]
        key_out: Option<PathBuf>,
    },

    /// Decrypt a sealed token's file with the key this wallet can open.
    OpenFile {
        #[arg(long, value_parser = parse_hex32)]
        token_id: [u8; 32],
        #[arg(long = "in")]
        input: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },

    /// Show a token, opening sealed content if this wallet can.
    Info {
        #[arg(long, value_parser = parse_hex32)]
        token_id: [u8; 32],
    },

    /// Give a token away. A sealed token is resealed to the new owner, who must
    /// be given as a receive code. For a linked LNS token, pass any extra registry
    /// PDAs required by `NftTransferHook` via `--registry-actor`.
    Transfer {
        #[arg(long, value_parser = parse_hex32)]
        token_id: [u8; 32],
        #[arg(long)]
        to: String,
        /// Extra public registry actor accounts to declare (e.g. settings, page, market bucket).
        #[arg(long = "registry-actor")]
        registry_actors: Vec<AccountId>,
        /// Send to an address even if nothing shows it is real.
        #[arg(long)]
        force: bool,
    },

    /// Destroy an unlinked token.
    Burn {
        #[arg(long, value_parser = parse_hex32)]
        token_id: [u8; 32],
    },

    /// Shield an unlinked token into a private LP-0001 deed PDA.
    ShieldDeed {
        #[arg(long, value_parser = parse_hex32)]
        token_id: [u8; 32],
        /// Private account in this wallet whose keys derive the deed PDA (defaults to holder if private, else first private account).
        #[arg(long)]
        deed_owner: Option<String>,
    },

    /// Unshield an unlinked token from its private LP-0001 deed PDA.
    UnshieldDeed {
        #[arg(long, value_parser = parse_hex32)]
        token_id: [u8; 32],
        /// Private account in this wallet whose keys derive the deed PDA (defaults to holder if private, else first private account).
        #[arg(long)]
        deed_owner: Option<String>,
    },

    /// Every token this wallet holds, found by scanning the program's history.
    Portfolio,

    /// Every collection on the program.
    Collections,

    /// The LEZ wallet's own command line.
    #[command(disable_help_flag = true)]
    Wallet {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

fn parse_hex32(s: &str) -> Result<[u8; 32], String> {
    hex::decode(s)
        .map_err(|e| e.to_string())?
        .try_into()
        .map_err(|_| "expected exactly 32 bytes (64 hex chars)".to_owned())
}

fn parse_account(s: &str) -> Result<AccountId> {
    let bare = s.strip_prefix("Public/").or_else(|| s.strip_prefix("Private/")).unwrap_or(s);
    bare.parse::<AccountId>().map_err(|e| anyhow!("{s} is not an account: {e}"))
}

fn parse_authority(s: &str) -> Result<AuthorizedProgram> {
    if let Some((prog, cid_hex)) = s.split_once(':') {
        let program = parse_account(prog)?;
        let cid = parse_hex32(cid_hex).map_err(|e| anyhow!("invalid collection id: {e}"))?;
        Ok(AuthorizedProgram {
            program,
            collection_id: Some(cid),
        })
    } else {
        Ok(AuthorizedProgram {
            program: parse_account(s)?,
            collection_id: None,
        })
    }
}

fn read_arg(value: &str) -> Result<Vec<u8>> {
    match value.strip_prefix('@') {
        Some(path) => std::fs::read(path).with_context(|| format!("reading {path}")),
        None => Ok(value.as_bytes().to_vec()),
    }
}

fn random32() -> [u8; 32] {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the OS gives random bytes");
    bytes
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after UNIX_EPOCH")
        .as_millis()
        .try_into()
        .expect("timestamp fits u64")
}

// ---------------------------------------------------------------- recipients

/// Who a token goes to: an account, and for a receive code also its receiving
/// keys, so sealed content can be opened and private payments routed.
struct Recipient {
    account: AccountId,
    vpk: Option<ViewingPublicKey>,
    npk: Option<NullifierPublicKey>,
}

fn receive_checksum(payload: &[u8]) -> [u8; CHECKSUM_LEN] {
    use sha2::Digest as _;
    let hash = sha2::Sha256::new()
        .chain_update(RECEIVE_PREFIX_V3.as_bytes())
        .chain_update(payload)
        .finalize();
    hash[..CHECKSUM_LEN].try_into().expect("4 bytes")
}

fn receive_code(npk: &NullifierPublicKey, vpk: &ViewingPublicKey) -> String {
    let mut bytes = Vec::with_capacity(32 + vpk.to_bytes().len() + CHECKSUM_LEN);
    bytes.extend_from_slice(&npk.0);
    bytes.extend_from_slice(vpk.to_bytes());
    let checksum = receive_checksum(&bytes);
    bytes.extend_from_slice(&checksum);
    format!("{RECEIVE_PREFIX_V3}{}", bs58::encode(bytes).into_string())
}

fn parse_recipient(s: &str) -> Result<Recipient> {
    let s = s.trim();
    if let Some(code) = s.strip_prefix(RECEIVE_PREFIX_V3) {
        let mut bytes = bs58::decode(code).into_vec().context("the receive code is not base58")?;
        if bytes.len() <= 32 + CHECKSUM_LEN {
            bail!("the receive code is too short; copy all of it");
        }
        let checksum = bytes.split_off(bytes.len() - CHECKSUM_LEN);
        if checksum != receive_checksum(&bytes) {
            bail!("the receive code is damaged (its checksum does not match); copy it again");
        }
        let npk = NullifierPublicKey(bytes[..32].try_into().expect("32 bytes"));
        let vpk = ViewingPublicKey::from_bytes(bytes[32..].to_vec())
            .map_err(|e| anyhow!("the receive code's viewing key does not decode: {e:?}"))?;
        let account = AccountId::for_regular_private_account(&npk, &vpk);
        return Ok(Recipient {
            account,
            vpk: Some(vpk),
            npk: Some(npk),
        });
    }
    Ok(Recipient {
        account: parse_account(s)?,
        vpk: None,
        npk: None,
    })
}

// ---------------------------------------------------------------- sealing

#[derive(borsh::BorshSerialize, borsh::BorshDeserialize)]
struct Sealed {
    version: u8,
    wraps: Vec<Wrap>,
}

/// The content key for one reader: an ML-KEM ciphertext to their viewing key,
/// and the key under AES-GCM keyed from the shared secret.
#[derive(borsh::BorshSerialize, borsh::BorshDeserialize)]
struct Wrap {
    kem: Vec<u8>,
    key: Vec<u8>,
}

fn wrap_cipher(secret: &SharedSecretKey) -> Aes256Gcm {
    use sha2::Digest as _;
    let key = sha2::Sha256::new()
        .chain_update(b"lez-nft/v3/wrap")
        .chain_update(secret.0)
        .finalize();
    Aes256Gcm::new(&key)
}

fn seal_key(key: &[u8; 32], readers: &[&ViewingPublicKey]) -> Vec<u8> {
    let wraps = readers
        .iter()
        .map(|vpk| {
            let (secret, kem) = SharedSecretKey::encapsulate(vpk);
            // Each shared secret is fresh, so a fixed nonce is never reused.
            let key = wrap_cipher(&secret)
                .encrypt(&Nonce::default(), key.as_slice())
                .expect("AES-GCM encrypts");
            Wrap { kem: kem.0, key }
        })
        .collect();
    borsh::to_vec(&Sealed { version: 1, wraps }).expect("borsh infallible")
}

fn encrypt(key: &[u8; 32], plaintext: &[u8]) -> Vec<u8> {
    let mut nonce = [0u8; NONCE_LEN];
    getrandom::fill(&mut nonce).expect("the OS gives random bytes");
    let mut out = nonce.to_vec();
    out.extend(
        Aes256Gcm::new(key.into())
            .encrypt(&nonce.into(), plaintext)
            .expect("AES-GCM encrypts"),
    );
    out
}

fn decrypt(key: &[u8; 32], data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < NONCE_LEN {
        bail!("the ciphertext is too short");
    }
    let (nonce, body) = data.split_at(NONCE_LEN);
    Aes256Gcm::new(key.into())
        .decrypt(Nonce::from_slice(nonce), body)
        .map_err(|_| anyhow!("the data does not decrypt with this token's key"))
}

// ---------------------------------------------------------------- the client

struct Client {
    wallet: WalletCore<SequencerClient>,
    program: AccountId,
    /// Held while a command that saves the wallet runs (see `lock_wallet`).
    _lock: Option<std::fs::File>,
}

fn lock_wallet() -> Result<std::fs::File> {
    use std::os::fd::AsRawFd as _;
    let path = fetch_persistent_storage_path()?.with_file_name("lez-nft.lock");
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("cannot make {}", dir.display()))?;
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .with_context(|| format!("cannot open {}", path.display()))?;
    // SAFETY: flock on a descriptor this function owns.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        say!("waiting for another lez-nft command using this wallet to finish");
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
            bail!("cannot lock {}", path.display());
        }
    }
    Ok(file)
}

impl Client {
    async fn open(rpc: &str, program: AccountId, writes: bool) -> Result<Self> {
        write_default_wallet_files()?;
        let lock = if writes { Some(lock_wallet()?) } else { None };
        let config_path = fetch_config_path()?;
        let storage_path = fetch_persistent_storage_path()?;
        let wallet = if storage_path.exists() {
            let password = read_password(&storage_path)?;
            let mut tries = 0;
            loop {
                match WalletCore::new_update_chain(
                    config_path.clone(),
                    storage_path.clone(),
                    Some(overrides(rpc)?),
                    &password,
                ) {
                    Ok(wallet) => break wallet,
                    Err(error) if !writes && tries < 20 => {
                        tries += 1;
                        let _ = error;
                        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                    }
                    Err(error) => return Err(error).context("failed to open the wallet"),
                }
            }
        } else {
            let (mut wallet, mnemonic) =
                WalletCore::new_init_storage(config_path, storage_path, Some(overrides(rpc)?), "")?;
            say!(
                "Created a new wallet. Its recovery phrase, the only way to get it back:\n\n  {mnemonic}\n"
            );
            wallet
                .start_at_chain_tip()
                .await
                .context("cannot read the chain's height")?;
            quietly(|| wallet.store_persistent_data())?;
            wallet
        };
        Ok(Self {
            wallet,
            program,
            _lock: lock,
        })
    }

    fn public_accounts(&self) -> Vec<AccountId> {
        self.wallet
            .storage()
            .key_chain()
            .public_account_ids()
            .map(|(id, _)| id)
            .collect()
    }

    fn private_accounts(&self) -> Vec<AccountId> {
        self.wallet
            .storage()
            .key_chain()
            .private_account_ids()
            .map(|(id, _)| id)
            .collect()
    }

    fn is_private(&self, account: AccountId) -> bool {
        self.wallet.resolve_private_account(account).is_some()
    }

    async fn funded_public(&self) -> Result<AccountId> {
        let accounts = self.public_accounts();
        let mut best: Option<(AccountId, u128)> = None;
        for account in &accounts {
            let balance = self.wallet.get_account_balance(*account).await.unwrap_or(0);
            if best.is_none_or(|(_, most)| balance > most) {
                best = Some((*account, balance));
            }
        }
        best.map(|(account, _)| account)
            .ok_or_else(|| anyhow!("this wallet has no public account; make one with `lez-nft setup`"))
    }

    async fn signer(&mut self, from: &str) -> Result<AccountId> {
        match from {
            "public" => self.funded_public().await,
            "private" => match self.private_accounts().into_iter().next() {
                Some(account) => Ok(account),
                None => self.new_private_account(),
            },
            other => {
                let account = parse_account(other)?;
                let held = self.public_accounts().contains(&account) || self.is_private(account);
                if !held {
                    bail!("this wallet does not hold {other}");
                }
                Ok(account)
            }
        }
    }

    fn signer_identity(&self, account: AccountId) -> AccountIdentity {
        self.wallet
            .resolve_private_account(account)
            .unwrap_or(AccountIdentity::Public(account))
    }

    fn new_private_account(&mut self) -> Result<AccountId> {
        let (account, _) = self.wallet.create_new_account_private(None);
        quietly(|| self.wallet.store_persistent_data())?;
        say!("made a private account: Private/{account}");
        Ok(account)
    }

    fn own_vpk(&self, account: AccountId) -> Option<ViewingPublicKey> {
        let found = self.wallet.storage().key_chain().private_account(account)?;
        Some(found.key_chain.viewing_public_key.clone())
    }

    fn own_keys(&self, account: AccountId) -> Result<(NullifierPublicKey, ViewingPublicKey)> {
        let found = self
            .wallet
            .storage()
            .key_chain()
            .private_account(account)
            .ok_or_else(|| anyhow!("Private/{account} is not a private account of this wallet"))?;
        Ok((
            found.key_chain.nullifier_public_key,
            found.key_chain.viewing_public_key.clone(),
        ))
    }

    fn own_payee(&self, account: AccountId) -> Result<Payee> {
        let (npk, vpk) = self.own_keys(account)?;
        Ok(Payee::Private {
            npk: npk.0,
            vpk: vpk.to_bytes().to_vec(),
        })
    }

    fn own_receive_code(&self, account: AccountId) -> Result<String> {
        let (npk, vpk) = self.own_keys(account)?;
        Ok(receive_code(&npk, &vpk))
    }

    fn open_key(&self, sealed: &[u8]) -> Option<[u8; 32]> {
        let sealed: Sealed = borsh::from_slice(sealed).ok()?;
        for found in self.wallet.storage().key_chain().private_accounts() {
            let vsk = &found.key_chain.private_key_holder.viewing_secret_key;
            for wrap in &sealed.wraps {
                let Some(secret) =
                    SharedSecretKey::decapsulate(&EphemeralPublicKey(wrap.kem.clone()), &vsk.d, &vsk.z)
                else {
                    continue;
                };
                if let Ok(key) = wrap_cipher(&secret).decrypt(&Nonce::default(), wrap.key.as_slice()) {
                    if let Ok(key) = key.try_into() {
                        return Some(key);
                    }
                }
            }
        }
        None
    }

    async fn check_recipient(&self, recipient: &Recipient, force: bool) -> Result<()> {
        if force || recipient.vpk.is_some() {
            return Ok(());
        }
        let account = recipient.account;
        if self.public_accounts().contains(&account) || self.is_private(account) {
            return Ok(());
        }
        let balance = self
            .wallet
            .get_account_balance(account)
            .await
            .context("failed to read the account from the sequencer")?;
        let nonces = self
            .wallet
            .get_accounts_nonces(&[account])
            .await
            .context("failed to read the account from the sequencer")?;
        let used = balance > 0 || nonces.first().is_some_and(|nonce| *nonce != Default::default());
        if !used {
            bail!(
                "{account} has never held funds or sent a transaction, so it may be mistyped and anything sent there \
                 would be lost. Check it, ask for their receive code instead, or add --force to send anyway"
            );
        }
        Ok(())
    }

    async fn actor_state(&self, account: AccountId) -> Result<Vec<u8>> {
        let view = self
            .wallet
            .get_account_view(Actor::new(account, self.program))
            .await
            .context("failed to read the account from the sequencer")?;
        Ok(view
            .map(|acc| acc.data.actor_state(self.program).to_vec())
            .unwrap_or_default())
    }

    async fn token(&self, token_id: &TokenId) -> Result<Option<NftRecord>> {
        let bytes = self.actor_state(token_address(&self.program, token_id)).await?;
        if bytes.is_empty() {
            return Ok(None);
        }
        Ok(Some(
            borsh::from_slice(&bytes).context("the token account does not hold a token")?,
        ))
    }

    async fn collection(&self, id: &CollectionId) -> Result<Option<CollectionRecord>> {
        let bytes = self
            .actor_state(collection_address(&self.program, id))
            .await?;
        if bytes.is_empty() {
            return Ok(None);
        }
        Ok(Some(
            borsh::from_slice(&bytes).context("the collection account does not hold a collection")?,
        ))
    }

    fn holder(&self, record: &NftRecord) -> Option<AccountId> {
        self.public_accounts()
            .into_iter()
            .chain(self.private_accounts())
            .find(|account| record_owner_tag(record, account) == record.owner)
    }

    // ------------------------------------------------------------ sending

    async fn load_program(&self, program_id: AccountId) -> Result<Program> {
        if program_id == self.program {
            if let Some(path) = std::env::var_os("LEZ_NFT_PROGRAM_ELF") {
                let bytes = std::fs::read(&path)
                    .with_context(|| format!("cannot read {}", PathBuf::from(path).display()))?;
                return Program::new(bytes.into())
                    .map_err(|e| anyhow!("the program's bytecode does not load: {e}"));
            }
        }
        ProgramLoader(&self.wallet)
            .program(program_id)
            .await
            .with_context(|| format!("failed to load program {program_id} from chain"))?
            .ok_or_else(|| anyhow!("{program_id} is not a deployed program"))
    }

    fn pda_mention(&self, seed: PdaSeed) -> AccountMention {
        AccountIdentity::PublicPda {
            program: self.program,
            seed,
        }
        .select_program_actor_state(self.program)
        .without_authorization()
    }

    async fn send_message(
        &mut self,
        mentions: Vec<AccountMention>,
        extra_programs: &[AccountId],
        async_delivery: AsyncDelivery,
        instruction: &Instruction,
    ) -> Result<(String, u64)> {
        let private = mentions.iter().any(|m| m.identity.is_private())
            || !async_delivery.recoveries.is_empty()
            || !async_delivery.seals.is_empty();
        let mut catalog = ProgramCatalog::default();
        if private {
            say!("signing privately: proving it on this computer, so the chain never learns the account");
            let nft_prog = self.load_program(self.program).await?;
            catalog.programs.insert(
                self.program,
                Dependency {
                    program: nft_prog,
                    kind: ProgramKind::Disclosed,
                },
            );
            for &extra in extra_programs {
                if extra != self.program && !catalog.programs.contains_key(&extra) {
                    let prog = self.load_program(extra).await?;
                    catalog.programs.insert(
                        extra,
                        Dependency {
                            program: prog,
                            kind: ProgramKind::Disclosed,
                        },
                    );
                }
            }
        }
        let payload = borsh::to_vec(&Message::Instruction(instruction.clone()))?;
        let (hash, _secrets) = self
            .wallet
            .send_tx(mentions, 0, payload, &catalog, None, async_delivery)
            .await
            .map_err(|e| anyhow!("the sequencer did not accept the transaction: {e:?}"))?;
        say!("submitted {hash}");
        let (tx, block) = self
            .wallet
            .poll_transaction(hash)
            .await
            .context("the transaction was not included in a block")?;
        let receipt = self
            .wallet
            .client()
            .get_transaction_receipt(hash)
            .await
            .context("failed to read transaction receipt")?
            .ok_or_else(|| anyhow!("transaction {hash} in block {block} has no receipt"))?;
        let reverted = !receipt.success;
        if let LeeTransaction::PrivacyPreserving(pp_tx) = &tx {
            let key_chain = self.wallet.storage_mut().key_chain_mut();
            let mut index = key_chain.build_latest_nullifier_index();
            key_chain.follow_transaction(&pp_tx.message, reverted, false, &mut index);
            quietly(|| self.wallet.store_persistent_data())?;
        }
        if reverted {
            bail!("transaction {hash} reverted in block {block}");
        }
        say!("applied in block {block}");
        Ok((hash.to_string(), block))
    }

    // ------------------------------------------------------------ reading

    async fn describe(&self, record: &NftRecord) -> Result<Value> {
        let holder = self.holder(record);
        let (content, collection) = match record.collection {
            Some((id, serial)) => {
                let content = if !record.content.is_empty() {
                    record.content.clone()
                } else {
                    self.collection(&id).await?.map(|c| c.content).unwrap_or_default()
                };
                (content, json!({ "id": hex::encode(id), "serial": serial }))
            }
            None => (record.content.clone(), Value::Null),
        };
        let sealed = !record.sealed.is_empty();
        let opened = if sealed {
            self.open_key(&record.sealed)
                .and_then(|key| decrypt(&key, &content).ok())
        } else {
            Some(content)
        };
        let linked = record.linked.as_ref().map(|l| {
            json!({
                "program": l.program.to_string(),
                "target": hex::encode(l.target),
                "authority_actor": l.authority_actor.to_string(),
                "record_actor": l.record_actor.to_string(),
            })
        });
        Ok(json!({
            "version": record.version,
            "token_id": hex::encode(record.token_id),
            "address": token_address(&self.program, &record.token_id).to_string(),
            "owner_tag": hex::encode(record.owner),
            "holder": holder.map(|account| account_label(self, account)),
            "collection": collection,
            "linked": linked,
            "shielded": record.shielded,
            "expires": record.expires,
            "frozen": record.frozen,
            "deed_account": record.deed_account.map(|a| a.to_string()),
            "sealed": sealed,
            "readable": opened.is_some(),
            "content": opened.as_deref().map(content_value),
        }))
    }

    async fn scan(&self) -> Result<ScanCache> {
        use futures::StreamExt as _;
        let path = fetch_persistent_storage_path()?
            .with_file_name(format!("lez-nft-v3-scan-{}.json", self.program));
        let mut cache: ScanCache = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        let deploy_block = std::env::var("LEZ_NFT_DEPLOY_BLOCK")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEPLOY_BLOCK);
        let first = cache.last.map_or(deploy_block, |last| last + 1);
        let last = self
            .wallet
            .get_last_block_id()
            .await
            .context("cannot read the chain's height")?;
        if first > last {
            return Ok(cache);
        }
        say!("scanning blocks {first}-{last}");
        let rpc_client = self.wallet.client_owned();
        let ranges: Vec<(u64, u64)> = (first..=last)
            .step_by(200)
            .map(|start| (start, (start + 199).min(last)))
            .collect();
        let fetched: Vec<Result<Vec<common::block::Block>>> = futures::stream::iter(ranges)
            .map(|(start, end)| {
                let rpc_client = rpc_client.clone();
                async move {
                    rpc_client
                        .get_block_range(start, end)
                        .await
                        .map_err(|e| anyhow!("get_block_range({start}, {end}): {e}"))
                }
            })
            .buffered(8)
            .collect()
            .await;
        for blocks in fetched {
            for block in blocks? {
                for (pos, tx) in block.body.transactions.iter().enumerate() {
                    if !block.body.is_reverted(pos) {
                        self.touched(tx, &mut cache);
                    }
                }
            }
        }
        cache.last = Some(last);
        std::fs::write(&path, serde_json::to_vec(&cache)?).ok();
        Ok(cache)
    }

    fn touched(&self, tx: &LeeTransaction, cache: &mut ScanCache) {
        let program = &self.program;
        let cfg_addr = config_address(program);
        let mut token = |id: &TokenId| cache.tokens.insert(token_address(program, id).to_string());
        match tx {
            LeeTransaction::Public(tx) => {
                let root = &tx.message().execution.root;
                if root.to.program_account_id == *program {
                    if let Ok(Message::Instruction(instr)) = borsh::from_slice::<Message>(&root.message) {
                        match instr {
                            Instruction::CreateCollection { collection_id, .. } => {
                                cache.collections.insert(
                                    collection_address(program, &collection_id).to_string(),
                                );
                            }
                            Instruction::MintFromCollection { collection_id, serial, .. } => {
                                token(&serial_token_id(&collection_id, serial));
                            }
                            Instruction::Mint { token_id, .. }
                            | Instruction::Transfer { token_id, .. } => {
                                token(&token_id);
                            }
                            _ => {}
                        }
                    }
                }
                for actor in tx.message().public_context.actors.keys() {
                    if actor.program_account_id == *program && actor.account_id != cfg_addr {
                        let addr = actor.account_id.to_string();
                        if !cache.collections.contains(&addr) {
                            cache.tokens.insert(addr);
                        }
                    }
                }
            }
            LeeTransaction::PrivacyPreserving(tx) => {
                for step in &tx.message().execution.boundary {
                    if let BoundaryStep::PrivateToPublic(delivery) = step {
                        if delivery.envelope.to.program_account_id != *program {
                            continue;
                        }
                        let addr = delivery.envelope.to.account_id.to_string();
                        match borsh::from_slice::<Message>(&delivery.envelope.message) {
                            Ok(Message::Effect(Effect::CreateCollection(_))) => {
                                cache.collections.insert(addr);
                            }
                            Ok(
                                Message::Effect(Effect::Mint(_) | Effect::Transfer { .. })
                                | Message::AuthoritySync(
                                    AuthorityOp::Mint { .. } | AuthorityOp::Transfer { .. },
                                ),
                            ) => {
                                cache.tokens.insert(addr);
                            }
                            _ => {}
                        }
                    }
                }
                for actor in tx.message().public_context.actors.keys() {
                    if actor.program_account_id == *program && actor.account_id != cfg_addr {
                        let addr = actor.account_id.to_string();
                        if !cache.collections.contains(&addr) {
                            cache.tokens.insert(addr);
                        }
                    }
                }
            }
        }
    }

    async fn raw_actor_state(&self, address: &str) -> Result<Vec<u8>> {
        self.actor_state(address.parse().map_err(|e| anyhow!("{address}: {e}"))?)
            .await
    }

    async fn describe_collection(&self, record: &CollectionRecord) -> Value {
        let mine = self
            .public_accounts()
            .into_iter()
            .chain(self.private_accounts())
            .find(|account| lez_nft_core::creator_tag(&record.collection_id, account) == record.creator);
        json!({
            "id": hex::encode(record.collection_id),
            "address": collection_address(&self.program, &record.collection_id).to_string(),
            "price": record.price.to_string(),
            "payee": match &record.payee {
                Some(Payee::Public(account)) => json!(format!("Public/{account}")),
                Some(Payee::Private { .. }) => json!("private"),
                None => Value::Null,
            },
            "supply": record.supply,
            "minted": record.minted,
            "sold_out": record.minted >= record.supply,
            "mine": mine.map(|account| account_label(self, account)),
            "content": content_value(&record.content),
        })
    }
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct ScanCache {
    last: Option<u64>,
    tokens: BTreeSet<String>,
    collections: BTreeSet<String>,
}

fn account_label(client: &Client, account: AccountId) -> String {
    if client.is_private(account) {
        format!("Private/{account}")
    } else {
        format!("Public/{account}")
    }
}

fn content_value(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(bytes).into_owned()))
}

fn overrides(rpc: &str) -> Result<WalletConfigOverrides> {
    let sequencer_addr = rpc.parse().map_err(|_| anyhow!("{rpc} is not a URL"))?;
    Ok(WalletConfigOverrides {
        sequencer: Some(SequencerConnectionData {
            sequencer_addr,
            basic_auth: None,
        }),
        seq_tx_poll_max_blocks: Some(60),
        ..Default::default()
    })
}

// ---------------------------------------------------------------- commands

async fn run(cli: Cli) -> Result<Value> {
    let program = cli.program;
    match cli.command {
        Command::Wallet { .. } => unreachable!("handled in main"),

        Command::Setup => {
            let mut client = Client::open(&cli.rpc, program, true).await?;
            let public = match client.public_accounts().is_empty() {
                false => client.funded_public().await?,
                true => {
                    let (account, _) = client.wallet.create_new_account_public(None);
                    quietly(|| client.wallet.store_persistent_data())?;
                    account
                }
            };
            let private = client.signer("private").await?;
            client.wallet.sync_to_latest_block().await.ok();
            quietly(|| client.wallet.store_persistent_data())?;
            let public_balance = client.wallet.get_account_balance(public).await.unwrap_or(0);
            let private_balance = client
                .wallet
                .get_account_private(private)
                .and_then(|found| found.data.native_balance().ok())
                .unwrap_or(0);
            let code = client.own_receive_code(private)?;
            say!("public  Public/{public}  {public_balance}  (send funds here, then shield them)");
            say!("private Private/{private}  {private_balance}  (mints and holds from here)");
            Ok(json!({
                "public": { "account": format!("Public/{public}"), "balance": public_balance.to_string() },
                "private": { "account": format!("Private/{private}"), "balance": private_balance.to_string() },
                "receive_code": code,
            }))
        }

        Command::Shield { amount } => {
            let mut client = Client::open(&cli.rpc, program, true).await?;
            let public = client.signer("public").await?;
            let private = client.signer("private").await?;
            say!("moving {amount} from Public/{public} to Private/{private}: proving it here");
            let (hash, _) = NativeTokenTransfer(&client.wallet)
                .transfer(
                    AccountIdentity::Public(public),
                    AccountIdentity::PrivateOwned(private),
                    amount,
                    CreditDelivery::Automatic,
                )
                .await
                .map_err(|e| anyhow!("the sequencer did not accept the transfer: {e:?}"))?;
            say!("submitted {hash}");
            let (tx, block) = client
                .wallet
                .poll_transaction(hash)
                .await
                .context("the transfer was not included in a block")?;
            let reverted = !client
                .wallet
                .client()
                .get_transaction_receipt(hash)
                .await?
                .ok_or_else(|| anyhow!("transaction {hash} has no receipt"))?
                .success;
            if let LeeTransaction::PrivacyPreserving(pp_tx) = &tx {
                let key_chain = client.wallet.storage_mut().key_chain_mut();
                let mut index = key_chain.build_latest_nullifier_index();
                key_chain.follow_transaction(&pp_tx.message, reverted, false, &mut index);
                quietly(|| client.wallet.store_persistent_data())?;
            }
            if reverted {
                bail!("transfer {hash} reverted in block {block}");
            }
            say!("applied in block {block}");
            Ok(json!({ "tx": hash.to_string(), "block": block }))
        }

        Command::Accounts => {
            let mut client = Client::open(&cli.rpc, program, true).await?;
            if !client.private_accounts().is_empty() {
                client.wallet.sync_to_latest_block().await.ok();
                quietly(|| client.wallet.store_persistent_data())?;
            }
            let mut public = Vec::new();
            for account in client.public_accounts() {
                let balance = client.wallet.get_account_balance(account).await.unwrap_or(0);
                say!("Public/{account}  {balance}");
                public.push(json!({ "account": format!("Public/{account}"), "balance": balance.to_string() }));
            }
            let mut private = Vec::new();
            for account in client.private_accounts() {
                let balance = client
                    .wallet
                    .get_account_private(account)
                    .and_then(|found| found.data.native_balance().ok())
                    .unwrap_or(0);
                say!("Private/{account}  {balance}");
                private.push(json!({ "account": format!("Private/{account}"), "balance": balance.to_string() }));
            }
            Ok(json!({ "public": public, "private": private }))
        }

        Command::ReceiveCode { account } => {
            let mut client = Client::open(&cli.rpc, program, true).await?;
            let account = match account {
                Some(account) => parse_account(&account)?,
                None => client.signer("private").await?,
            };
            let code = client.own_receive_code(account)?;
            say!("{code}");
            Ok(json!({ "account": format!("Private/{account}"), "code": code }))
        }

        Command::Configure {
            from,
            admin,
            authorities,
        } => {
            let mut client = Client::open(&cli.rpc, program, true).await?;
            let signer = client.signer(&from).await?;
            let admin = match admin {
                Some(a) => parse_account(&a)?,
                None => signer,
            };
            let authorities: Vec<AuthorizedProgram> = authorities
                .iter()
                .map(|s| parse_authority(s))
                .collect::<Result<_>>()?;
            let instruction = Instruction::Configure { admin, authorities };
            let mentions = vec![
                client
                    .signer_identity(signer)
                    .select_program_actor_state(program),
                client.pda_mention(config_seed()),
            ];
            let (tx, block) = client
                .send_message(mentions, &[], AsyncDelivery::default(), &instruction)
                .await?;
            Ok(json!({ "admin": admin.to_string(), "tx": tx, "block": block }))
        }

        Command::Create {
            content,
            supply,
            price,
            payee,
            from,
            force,
        } => {
            let mut client = Client::open(&cli.rpc, program, true).await?;
            let creator = client.signer(&from).await?;
            let payee = match (price, payee.as_deref()) {
                (0, _) => None,
                (_, None | Some("public")) => Some(Payee::Public(client.signer("public").await?)),
                (_, Some("private")) => {
                    let account = client.signer("private").await?;
                    Some(client.own_payee(account)?)
                }
                (_, Some(given)) => {
                    let recipient = parse_recipient(given)?;
                    client.check_recipient(&recipient, force).await?;
                    match (recipient.npk, recipient.vpk) {
                        (Some(npk), Some(vpk)) => Some(Payee::Private {
                            npk: npk.0,
                            vpk: vpk.to_bytes().to_vec(),
                        }),
                        _ if client.is_private(recipient.account) => {
                            Some(client.own_payee(recipient.account)?)
                        }
                        _ => Some(Payee::Public(recipient.account)),
                    }
                }
            };
            let collection_id = random32();
            say!("collection {}", hex::encode(collection_id));
            let instruction = Instruction::CreateCollection {
                collection_id,
                payee,
                price,
                supply,
                content: read_arg(&content)?,
            };
            let mentions = vec![
                client
                    .signer_identity(creator)
                    .select_program_actor_state(program),
                client.pda_mention(config_seed()),
                client.pda_mention(collection_seed(&collection_id)),
            ];
            let (tx, block) = client
                .send_message(mentions, &[], AsyncDelivery::default(), &instruction)
                .await?;
            Ok(json!({ "collection_id": hex::encode(collection_id), "tx": tx, "block": block }))
        }

        Command::Collection { id } => {
            let client = Client::open(&cli.rpc, program, false).await?;
            let record = client
                .collection(&id)
                .await?
                .ok_or_else(|| anyhow!("no collection {}", hex::encode(id)))?;
            let value = client.describe_collection(&record).await;
            say!("{}", serde_json::to_string_pretty(&value)?);
            Ok(value)
        }

        Command::Buy {
            collection,
            from,
            to,
            force,
        } => {
            let mut client = Client::open(&cli.rpc, program, true).await?;
            let buyer = client.signer(&from).await?;
            let recipient = to.as_deref().map(parse_recipient).transpose()?;
            if let Some(recipient) = &recipient {
                client.check_recipient(recipient, force).await?;
            }
            let attempts = if client.is_private(buyer) { 1 } else { 3 };
            let mut last_error = None;
            for _ in 0..attempts {
                let record = client
                    .collection(&collection)
                    .await?
                    .ok_or_else(|| anyhow!("no collection {}", hex::encode(collection)))?;
                if record.minted >= record.supply {
                    bail!("the collection is sold out ({} of {})", record.minted, record.supply);
                }
                let serial = record.minted + 1;
                let token_id = serial_token_id(&collection, serial);

                if record.price > 0 && client.is_private(buyer) {
                    client.wallet.sync_to_latest_block().await.ok();
                    let balance = client
                        .wallet
                        .get_account_private(buyer)
                        .and_then(|found| found.data.native_balance().ok())
                        .unwrap_or(0);
                    if balance < record.price {
                        bail!(
                            "Private/{buyer} holds {balance}; this mint costs {}. Move funds into it first",
                            record.price
                        );
                    }
                }

                let buyer_id = client.signer_identity(buyer);
                let mut mentions = vec![
                    buyer_id.clone().select_program_actor_state(program),
                    client.pda_mention(collection_seed(&collection)),
                    client.pda_mention(token_seed(&token_id)),
                ];
                let mut payee_opening = None;
                let mut async_delivery = AsyncDelivery::default();
                if record.price > 0 {
                    mentions.push(buyer_id.clone().balance());
                    match &record.payee {
                        Some(Payee::Public(payee)) => {
                            if *payee != buyer {
                                mentions.push(
                                    AccountIdentity::PublicNoSign(*payee)
                                        .balance()
                                        .without_authorization(),
                                );
                            }
                        }
                        Some(Payee::Private { npk, vpk }) => {
                            let opening = random32();
                            payee_opening = Some(opening);
                            let vpk = ViewingPublicKey::from_bytes(vpk.clone())
                                .map_err(|e| anyhow!("collection payee vpk does not decode: {e:?}"))?;
                            let dest = AccountMention {
                                address_openings: BTreeSet::from([opening]),
                                ..AccountIdentity::PrivateForeign {
                                    npk: NullifierPublicKey(*npk),
                                    vpk,
                                    kind: PrivateAccountKind::Regular,
                                }
                                .balance()
                            };
                            let (_, mut delivery) = if client.is_private(buyer) {
                                client.wallet.seal_destination(dest)?
                            } else {
                                client.wallet.async_destination(dest)?
                            };
                            delivery.promotions.listed_only = true;
                            async_delivery = delivery;
                        }
                        None => {}
                    }
                }
                say!("minting #{serial} of {} for {}", record.supply, record.price);
                let instruction = Instruction::MintFromCollection {
                    collection_id: collection,
                    serial,
                    price: record.price,
                    payee: record.payee.clone(),
                    payee_opening,
                    owner: recipient.as_ref().map(|r| owner_tag(&token_id, &r.account)),
                };
                match client
                    .send_message(mentions, &[], async_delivery, &instruction)
                    .await
                {
                    Ok((tx, block)) => {
                        return Ok(json!({
                            "token_id": hex::encode(token_id),
                            "serial": serial,
                            "tx": tx,
                            "block": block,
                        }));
                    }
                    Err(error) => {
                        say!("{error:#}");
                        last_error = Some(error);
                    }
                }
            }
            Err(last_error.expect("at least one attempt"))
        }

        Command::Mint {
            content,
            from,
            to,
            encrypt: seal,
            key_file,
            token_id,
            force,
        } => {
            let mut client = Client::open(&cli.rpc, program, true).await?;
            let minter = client.signer(&from).await?;
            let recipient = to.as_deref().map(parse_recipient).transpose()?;
            if let Some(recipient) = &recipient {
                client.check_recipient(recipient, force).await?;
            }
            let token_id = token_id.unwrap_or_else(random32);
            let mut content = read_arg(&content)?;
            let mut sealed = Vec::new();
            if seal {
                let owner_vpk = match &recipient {
                    Some(recipient) => recipient.vpk.clone().ok_or_else(|| {
                        anyhow!(
                            "a sealed token's owner must be given as a receive code (`lez-nft receive-code`)"
                        )
                    })?,
                    None => client.own_vpk(minter).ok_or_else(|| {
                        anyhow!(
                            "a sealed token needs a private owner: use --from private or --to <receive code>"
                        )
                    })?,
                };
                let minter_vpk = client.own_vpk(minter);
                let mut readers = vec![&owner_vpk];
                if let Some(vpk) = minter_vpk
                    .as_ref()
                    .filter(|vpk| vpk.to_bytes() != owner_vpk.to_bytes())
                {
                    readers.push(vpk);
                }
                let key = match &key_file {
                    Some(path) => read_key(path)?,
                    None => random32(),
                };
                content = encrypt(&key, &content);
                sealed = seal_key(&key, &readers);
            }
            say!("token {}", hex::encode(token_id));
            let instruction = Instruction::Mint {
                token_id,
                owner: recipient.as_ref().map(|r| owner_tag(&token_id, &r.account)),
                content,
                sealed,
            };
            let mentions = vec![
                client
                    .signer_identity(minter)
                    .select_program_actor_state(program),
                client.pda_mention(token_seed(&token_id)),
            ];
            let (tx, block) = client
                .send_message(mentions, &[], AsyncDelivery::default(), &instruction)
                .await?;
            Ok(json!({ "token_id": hex::encode(token_id), "tx": tx, "block": block }))
        }

        Command::SealFile {
            input,
            out,
            key_out,
        } => {
            let key = random32();
            let plain = std::fs::read(&input).with_context(|| format!("reading {}", input.display()))?;
            std::fs::write(&out, encrypt(&key, &plain))
                .with_context(|| format!("writing {}", out.display()))?;
            let key_out = key_out.unwrap_or_else(|| PathBuf::from(format!("{}.key", out.display())));
            write_private(&key_out, hex::encode(key).as_bytes())?;
            say!(
                "encrypted {} to {}; key in {}",
                input.display(),
                out.display(),
                key_out.display()
            );
            Ok(json!({ "out": out, "key_file": key_out }))
        }

        Command::OpenFile {
            token_id,
            input,
            out,
        } => {
            let client = Client::open(&cli.rpc, program, false).await?;
            let record = client
                .token(&token_id)
                .await?
                .ok_or_else(|| anyhow!("no token {}", hex::encode(token_id)))?;
            let key = client
                .open_key(&record.sealed)
                .ok_or_else(|| anyhow!("this wallet cannot open that token"))?;
            let data = std::fs::read(&input).with_context(|| format!("reading {}", input.display()))?;
            std::fs::write(&out, decrypt(&key, &data)?)
                .with_context(|| format!("writing {}", out.display()))?;
            say!("decrypted to {}", out.display());
            Ok(json!({ "out": out }))
        }

        Command::Info { token_id } => {
            let client = Client::open(&cli.rpc, program, false).await?;
            let record = client
                .token(&token_id)
                .await?
                .ok_or_else(|| anyhow!("no token {}", hex::encode(token_id)))?;
            let value = client.describe(&record).await?;
            say!("{}", serde_json::to_string_pretty(&value)?);
            Ok(value)
        }

        Command::Transfer {
            token_id,
            to,
            registry_actors,
            force,
        } => {
            let mut client = Client::open(&cli.rpc, program, true).await?;
            let record = client
                .token(&token_id)
                .await?
                .ok_or_else(|| anyhow!("no token {}", hex::encode(token_id)))?;
            if record.shielded {
                bail!("token is shielded; unshield it before transferring");
            }
            if record.frozen {
                bail!("token is frozen");
            }
            let owner = client
                .holder(&record)
                .ok_or_else(|| anyhow!("this wallet does not hold that token"))?;
            let recipient = parse_recipient(&to)?;
            client.check_recipient(&recipient, force).await?;
            let sealed = if record.sealed.is_empty() {
                None
            } else {
                let vpk = recipient
                    .vpk
                    .as_ref()
                    .ok_or_else(|| anyhow!("the token is sealed: give the new owner as a receive code"))?;
                let key = client
                    .open_key(&record.sealed)
                    .ok_or_else(|| anyhow!("this wallet cannot open that token"))?;
                Some(seal_key(&key, &[vpk]))
            };

            let mut mentions = vec![
                client
                    .signer_identity(owner)
                    .select_program_actor_state(program),
                client.pda_mention(token_seed(&token_id)),
            ];
            let mut extra_programs = Vec::new();
            let (new_owner, now, linked_target) = match &record.linked {
                Some(LinkedAuthority {
                    program: reg_prog,
                    target,
                    record_actor,
                    ..
                }) => {
                    mentions.push(
                        AccountIdentity::PublicNoSign(*record_actor)
                            .select_program_actor_state(*reg_prog)
                            .without_authorization(),
                    );
                    for &extra in &registry_actors {
                        if extra != *record_actor {
                            mentions.push(
                                AccountIdentity::PublicNoSign(extra)
                                    .select_program_actor_state(*reg_prog)
                                    .without_authorization(),
                            );
                        }
                    }
                    extra_programs.push(*reg_prog);
                    (
                        linked_owner_tag(target, &recipient.account),
                        Some(now_ms()),
                        Some(*target),
                    )
                }
                None => (owner_tag(&token_id, &recipient.account), None, None),
            };
            let instruction = Instruction::Transfer {
                token_id,
                new_owner,
                sealed,
                now,
                linked_target,
            };
            let (tx, block) = client
                .send_message(mentions, &extra_programs, AsyncDelivery::default(), &instruction)
                .await?;
            Ok(json!({ "token_id": hex::encode(token_id), "tx": tx, "block": block }))
        }

        Command::Burn { token_id } => {
            let mut client = Client::open(&cli.rpc, program, true).await?;
            let record = client
                .token(&token_id)
                .await?
                .ok_or_else(|| anyhow!("no token {}", hex::encode(token_id)))?;
            if record.linked.is_some() {
                bail!("linked token cannot be burned directly; release it via its linked registry");
            }
            let owner = client
                .holder(&record)
                .ok_or_else(|| anyhow!("this wallet does not hold that token"))?;
            let instruction = Instruction::Burn { token_id };
            let mentions = vec![
                client
                    .signer_identity(owner)
                    .select_program_actor_state(program),
                client.pda_mention(token_seed(&token_id)),
            ];
            let (tx, block) = client
                .send_message(mentions, &[], AsyncDelivery::default(), &instruction)
                .await?;
            Ok(json!({ "token_id": hex::encode(token_id), "tx": tx, "block": block }))
        }

        Command::ShieldDeed {
            token_id,
            deed_owner,
        } => {
            let mut client = Client::open(&cli.rpc, program, true).await?;
            let record = client
                .token(&token_id)
                .await?
                .ok_or_else(|| anyhow!("no token {}", hex::encode(token_id)))?;
            if record.linked.is_some() {
                bail!("linked token must be shielded through its linked registry");
            }
            let owner = client
                .holder(&record)
                .ok_or_else(|| anyhow!("this wallet does not hold that token"))?;
            let deed_holder = match deed_owner {
                Some(ref s) => parse_account(s)?,
                None if client.is_private(owner) => owner,
                None => client.signer("private").await?,
            };
            let (npk, vpk) = client.own_keys(deed_holder)?;
            let seed = deed_seed(&token_id);
            let deed_pda = deed_address(&program, &token_id, &npk, &vpk);
            if client.wallet.resolve_private_account(deed_pda).is_none() {
                client
                    .wallet
                    .storage_mut()
                    .key_chain_mut()
                    .insert_private_account(
                        deed_pda,
                        PrivateAccountKind::Pda {
                            account_id: program,
                            seed,
                        },
                        Account::default(),
                    )?;
                quietly(|| client.wallet.store_persistent_data())?;
            }
            let instruction = Instruction::Shield {
                token_id,
                deed_account: deed_pda,
            };
            let mentions = vec![
                client
                    .signer_identity(owner)
                    .select_program_actor_state(program),
                client.pda_mention(token_seed(&token_id)),
                AccountIdentity::PrivateOwned(deed_pda)
                    .select_program_actor_state(program)
                    .without_authorization(),
            ];
            let (tx, block) = client
                .send_message(mentions, &[], AsyncDelivery::default(), &instruction)
                .await?;
            Ok(json!({
                "token_id": hex::encode(token_id),
                "deed_account": deed_pda.to_string(),
                "tx": tx,
                "block": block,
            }))
        }

        Command::UnshieldDeed {
            token_id,
            deed_owner,
        } => {
            let mut client = Client::open(&cli.rpc, program, true).await?;
            let record = client
                .token(&token_id)
                .await?
                .ok_or_else(|| anyhow!("no token {}", hex::encode(token_id)))?;
            if record.linked.is_some() {
                bail!("linked token must be unshielded through its linked registry");
            }
            let owner = client
                .holder(&record)
                .ok_or_else(|| anyhow!("this wallet does not hold that token"))?;
            let deed_holder = match deed_owner {
                Some(ref s) => parse_account(s)?,
                None if client.is_private(owner) => owner,
                None => client.signer("private").await?,
            };
            let (npk, vpk) = client.own_keys(deed_holder)?;
            let deed_pda = deed_address(&program, &token_id, &npk, &vpk);
            let instruction = Instruction::Unshield {
                token_id,
                deed_account: deed_pda,
            };
            let mentions = vec![
                client
                    .signer_identity(owner)
                    .select_program_actor_state(program),
                client.pda_mention(token_seed(&token_id)),
                AccountIdentity::PrivateOwned(deed_pda)
                    .select_program_actor_state(program)
                    .without_authorization(),
            ];
            let (tx, block) = client
                .send_message(mentions, &[], AsyncDelivery::default(), &instruction)
                .await?;
            Ok(json!({
                "token_id": hex::encode(token_id),
                "deed_account": deed_pda.to_string(),
                "tx": tx,
                "block": block,
            }))
        }

        Command::Portfolio => {
            let client = Client::open(&cli.rpc, program, false).await?;
            let cache = client.scan().await?;
            let mut held = Vec::new();
            for address in &cache.tokens {
                let bytes = client.raw_actor_state(address).await?;
                let Ok(record) = borsh::from_slice::<NftRecord>(&bytes) else {
                    continue;
                };
                if client.holder(&record).is_some() {
                    held.push(client.describe(&record).await?);
                }
            }
            for token in &held {
                say!(
                    "{}  {}",
                    token["token_id"].as_str().unwrap_or_default(),
                    token["content"]
                );
            }
            if held.is_empty() {
                say!("this wallet holds no tokens");
            }
            Ok(json!({ "tokens": held, "scanned_to": cache.last }))
        }

        Command::Collections => {
            let client = Client::open(&cli.rpc, program, false).await?;
            let cache = client.scan().await?;
            let mut list = Vec::new();
            for address in &cache.collections {
                let bytes = client.raw_actor_state(address).await?;
                let Ok(record) = borsh::from_slice::<CollectionRecord>(&bytes) else {
                    continue;
                };
                let value = client.describe_collection(&record).await;
                say!(
                    "{}  {}/{}  {}",
                    value["id"].as_str().unwrap_or_default(),
                    record.minted,
                    record.supply,
                    value["content"]
                );
                list.push(value);
            }
            Ok(json!({ "collections": list, "scanned_to": cache.last }))
        }
    }
}

fn read_key(path: &Path) -> Result<[u8; 32]> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    parse_hex32(text.trim()).map_err(|e| anyhow!("{}: {e}", path.display()))
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt as _;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("writing {}", path.display()))?;
    file.write_all(bytes)?;
    Ok(())
}

#[tokio::main]
async fn main() {
    use_bundled_prover();
    let cli = Cli::parse();
    JSON.store(cli.json, Ordering::Relaxed);
    if let Command::Wallet { args } = cli.command {
        let lock = lock_wallet();
        let result = match (overrides(&cli.rpc), lock) {
            (Err(error), _) | (_, Err(error)) => Err(error),
            (Ok(overrides), Ok(_lock)) => run_wallet(args, Some(overrides)).await,
        };
        if let Err(error) = result {
            eprintln!("error: {error:#}");
            std::process::exit(1);
        }
        return;
    }
    let json = cli.json;
    match run(cli).await {
        Ok(value) => {
            if json {
                println!("{}", json!({ "success": true, "value": value, "error": null }));
            }
        }
        Err(error) => {
            if json {
                println!(
                    "{}",
                    json!({ "success": false, "value": null, "error": format!("{error:#}") })
                );
            } else {
                eprintln!("error: {error:#}");
            }
            std::process::exit(1);
        }
    }
}

// ---------------------------------------------------------------- plumbing

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

async fn run_wallet(args: Vec<String>, overrides: Option<WalletConfigOverrides>) -> Result<()> {
    use clap::{CommandFactory as _, Parser as _};
    use wallet::cli::{Args, execute_continuous_run, execute_subcommand};

    let Args {
        continuous_run,
        command,
        ..
    } = Args::parse_from(std::iter::once("lez-nft wallet".to_owned()).chain(args));
    write_default_wallet_files()?;
    let config_path = fetch_config_path()?;
    let storage_path = fetch_persistent_storage_path()?;
    if let Some(command) = command {
        let mut wallet = if storage_path.exists() {
            let password = read_password(&storage_path)?;
            WalletCore::new_update_chain(config_path, storage_path, overrides, &password)?
        } else {
            let (wallet, mnemonic) =
                WalletCore::new_init_storage(config_path, storage_path, overrides, "")?;
            println!(
                "Created a new wallet. Its recovery phrase, the only way to get it back:\n\n  {mnemonic}\n"
            );
            quietly(|| wallet.store_persistent_data())?;
            wallet
        };
        execute_subcommand(&mut wallet, command).await?;
    } else if continuous_run {
        let password = read_password(&storage_path)?;
        let mut wallet =
            WalletCore::new_update_chain(config_path, storage_path, overrides, &password)?;
        execute_continuous_run(&mut wallet).await?;
    } else {
        println!("{}", Args::command().name("lez-nft wallet").render_long_help());
    }
    Ok(())
}

fn write_default_wallet_files() -> Result<()> {
    let config_path = fetch_config_path()?;
    if let Some(home) = config_path.parent() {
        std::fs::create_dir_all(home)
            .with_context(|| format!("failed to create the wallet home {}", home.display()))?;
    }
    if !config_path.exists() {
        let config = serde_json::to_value(wallet::config::WalletConfig::default())?;
        std::fs::write(&config_path, serde_json::to_vec_pretty(&config)?)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> (NullifierPublicKey, ViewingPublicKey) {
        let vpk = ViewingPublicKey::from_bytes(vec![0; lez_nft_core::VPK_LEN])
            .expect("a zero key decodes");
        (NullifierPublicKey([5; 32]), vpk)
    }

    #[test]
    fn receive_code_round_trips() {
        let (npk, vpk) = sample();
        let code = receive_code(&npk, &vpk);
        assert!(code.starts_with(RECEIVE_PREFIX_V3));
        let recipient = parse_recipient(&code).expect("parses");
        assert_eq!(
            recipient.account,
            AccountId::for_regular_private_account(&npk, &vpk)
        );
    }

    #[test]
    fn damaged_receive_code_is_refused() {
        let (npk, vpk) = sample();
        let code = receive_code(&npk, &vpk);
        let mut chars: Vec<char> = code.chars().collect();
        let i = RECEIVE_PREFIX_V3.len() + 40;
        chars[i] = if chars[i] == '2' { '3' } else { '2' };
        let damaged: String = chars.into_iter().collect();
        assert!(parse_recipient(&damaged).is_err());
        assert!(parse_recipient(&code[..code.len() - 3]).is_err());
    }
}
