//! What the Basecamp module asks of this wallet. The module builds a name's changes
//! itself but keeps no keys, and can't run the registry, so it sends what it built here:
//! this signs it, runs it first as every command does, and confirms it applied.
//!
//! The LEZ wallet module can't do this on 0.4: it takes only account IDs, so it can't
//! give the evidence a transaction that creates a registry account needs.

use std::path::PathBuf;

use anyhow::{Context as _, Result, anyhow, bail};
use clap::Subcommand;
use lee_core::{account::AccountId, program::PdaSeed};
use wallet::{
    AccountIdentity, AccountMention, WalletCore,
    config::WalletConfigOverrides,
    utils::{fetch_config_path, fetch_persistent_storage_path, read_password},
};

use crate::{Client, Defaults, OLD_WALLET, new_wallet_password, parse_account, parse_hex32, quietly, save_defaults, write_default_wallet_files};

#[derive(Subcommand)]
pub enum Request {
    /// Print this wallet's accounts as JSON.
    Accounts,
    /// Print a private account's receiving keys as JSON; `auto` is the first one, made
    /// if there is none.
    Keys { account: String },
    /// Sign, try and send the change given as JSON on standard input.
    Send,
    /// Replace a wallet that no longer opens with one from the recovery phrase given on
    /// standard input. The old files stay beside it.
    Restore {
        /// How many public accounts to bring back. The recovery phrase only says how to
        /// derive them, and on a new chain nothing shows which were used.
        #[arg(long, default_value_t = 16)]
        accounts: u32,
    },
    /// Token requests (`holdings`, `pending`, `info`, `send`, `receive`); see `tokens.rs`.
    /// A token send can't go through `send`: that runs only the registry, and builds no
    /// pending credit for a private recipient.
    #[command(subcommand)]
    Token(crate::tokens::TokenRequest),
}

/// One actor a change reaches.
#[derive(serde::Deserialize)]
struct Mention {
    account: String,
    /// Whose state on `account` it is.
    program: String,
    /// A registry account's seed, the evidence that lets the transaction create it.
    seed: Option<String>,
    signs: bool,
}

#[derive(serde::Deserialize)]
struct Change {
    /// `borsh(Message)`, in hex.
    message: String,
    /// What it moves out of the signer besides the fee.
    spent: String,
    /// The root actor, then every other actor it reaches.
    mentions: Vec<Mention>,
}

impl Client {
    pub async fn module(&mut self, request: Request) -> Result<()> {
        match request {
            Request::Accounts => self.module_accounts(),
            Request::Keys { account } => self.module_keys(&account),
            Request::Send => self.module_send().await,
            Request::Restore { .. } => unreachable!("handled before the wallet is opened"),
            Request::Token(_) => unreachable!("handled with the token program chosen, in main"),
        }
    }

    fn module_accounts(&self) -> Result<()> {
        let key_chain = self.wallet.storage().key_chain();
        let public: Vec<String> = key_chain.public_account_ids().map(|(id, _)| id.to_string()).collect();
        let private: Vec<serde_json::Value> = key_chain
            .private_account_ids()
            .map(|(id, _)| {
                let balance = self.wallet.get_account_private(id).and_then(|found| found.data.native_balance().ok());
                serde_json::json!({ "account": id.to_string(), "balance": balance.unwrap_or(0).to_string() })
            })
            .collect();
        println!("{}", serde_json::json!({ "public": public, "private": private }));
        Ok(())
    }

    fn module_keys(&mut self, account: &str) -> Result<()> {
        let account: AccountId = if account == "auto" {
            let first = self.wallet.storage().key_chain().private_account_ids().map(|(id, _)| id).next();
            match first {
                Some(found) => found,
                None => self.new_receiving_keys()?.0,
            }
        } else {
            parse_account(account).map_err(|error| anyhow!(error))?
        };
        let keys = &self
            .wallet
            .storage()
            .key_chain()
            .private_account(account)
            .ok_or_else(|| anyhow!("this wallet has no private account {account}"))?
            .key_chain;
        println!(
            "{}",
            serde_json::json!({
                "account": account.to_string(),
                "npk": hex::encode(keys.nullifier_public_key.0),
                "vpk": hex::encode(keys.viewing_public_key.to_bytes()),
            })
        );
        Ok(())
    }

    async fn module_send(&mut self) -> Result<()> {
        let mut input = Vec::new();
        std::io::Read::read_to_end(&mut std::io::stdin(), &mut input).context("cannot read the change")?;
        let change: Change = serde_json::from_slice(&input).context("not a change")?;
        let mentions = change.mentions.iter().map(|mention| self.mention(mention)).collect::<Result<Vec<_>>>()?;
        let message = hex::decode(&change.message).context("the message is not hex")?;
        let spent = change.spent.parse().context("the amount spent is not a number")?;
        let program = self.program().await?;
        self.transact(mentions, message, vec![(self.program, program)], spent).await
    }

    fn mention(&self, mention: &Mention) -> Result<AccountMention> {
        let account = parse_account(&mention.account).map_err(|error| anyhow!(error))?;
        let program = parse_account(&mention.program).map_err(|error| anyhow!(error))?;
        let identity = match &mention.seed {
            Some(seed) => {
                let seed = parse_hex32(seed).ok_or_else(|| anyhow!("a seed is 32 bytes of hex"))?;
                AccountIdentity::PublicPda { program: self.program, seed: PdaSeed::new(seed) }
            }
            None if mention.signs => self.signer(account),
            None => AccountIdentity::PublicNoSign(account),
        };
        Ok(identity.select_program_actor_state(program))
    }
}

/// `path` with `suffix` added, numbered if that is taken, so nothing is overwritten.
fn beside(path: &std::path::Path, suffix: &str) -> PathBuf {
    let named = |n: u32| {
        let number = if n == 0 { String::new() } else { format!(".{n}") };
        path.with_file_name(format!("{}{suffix}{number}", path.file_name().unwrap_or_default().to_string_lossy()))
    };
    (0..).map(named).find(|candidate| !candidate.exists()).expect("an unused name")
}

/// See [`Request::Restore`]. `names wallet restore-keys` doesn't do here: it keeps only
/// accounts it finds on the chain, and on a new chain that is none of them.
pub async fn restore(accounts: u32, overrides: Option<WalletConfigOverrides>) -> Result<()> {
    write_default_wallet_files()?;
    let config_path = fetch_config_path()?;
    let storage_path = fetch_persistent_storage_path()?;
    if storage_path.exists() {
        let password = read_password(&storage_path)?;
        if WalletCore::new_update_chain(config_path.clone(), storage_path.clone(), overrides.clone(), &password).is_ok() {
            bail!("this wallet opens, so there is nothing to restore");
        }
    }
    let mnemonic = wallet::cli::read_mnemonic_from_stdin()?;
    let defaults_path = crate::defaults_path()?;
    let mut moved = Vec::new();
    for path in [&storage_path, &defaults_path] {
        if path.exists() {
            let kept = beside(path, ".lez-0.3");
            std::fs::rename(path, &kept).with_context(|| format!("cannot move {} aside", path.display()))?;
            moved.push(kept.display().to_string());
        }
    }
    let (mut wallet, _) = WalletCore::new_init_storage(config_path, storage_path, overrides, &new_wallet_password()).context(OLD_WALLET)?;
    wallet.restore_storage(&mnemonic);
    let public: Vec<AccountId> = (0..accounts.max(1)).map(|_| wallet.create_new_account_public(None).0).collect();
    let private = wallet.create_new_account_private(None).0;
    wallet.start_at_chain_tip().await.ok();
    quietly(|| wallet.store_persistent_data())?;
    let first = public.first().expect("at least one");
    save_defaults(&Defaults { public: Some(first.to_string()), private: Some(private.to_string()), ..Defaults::default() })?;
    println!(
        "{}",
        serde_json::json!({
            "public": public.iter().map(ToString::to_string).collect::<Vec<_>>(),
            "private": private.to_string(),
            "moved": moved,
        })
    );
    Ok(())
}
