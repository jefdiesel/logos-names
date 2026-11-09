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
    AccountIdentity, AccountMention, WalletCore, WalletSequencerClient as _,
    config::WalletConfigOverrides,
    utils::{fetch_config_path, fetch_persistent_storage_path, read_password},
};

use crate::{
    Client, Defaults, NATIVE_TRANSFER_GAS_LIMIT, OLD_WALLET, fees, load_defaults, new_wallet_password, parse_account, parse_hex32,
    quietly, save_defaults, write_default_wallet_files,
};

/// The longest nickname an account takes.
const MAX_LABEL: usize = 40;

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
    /// Print every account of this wallet as JSON: `[{account, kind, label, balance,
    /// default, public_key}]`. `balance` is decimal text, read from the chain for a public
    /// account and as the wallet last synced it for a private one. `default` marks the
    /// account `names` pays and registers from when none is given. `public_key` (hex,
    /// public accounts only) is what a sender needs to pay an account the chain has not
    /// seen yet. `npk` (hex, private accounts only) is the nullifier public key a name's
    /// profile publishes to be paid there, so it tells which name an account receives for.
    AccountList,
    /// Make a new `public` or `private` account, with an optional nickname; prints it as
    /// one `account-list` entry. Nothing is sent: a new account exists on the chain once
    /// it is paid (a private one) or first used (a public one).
    AccountNew {
        #[arg(value_parser = ["public", "private"])]
        kind: String,
        #[arg(long, default_value = "")]
        label: String,
    },
    /// Give one of this wallet's accounts (`Public/<id>` or `Private/<id>`) a nickname;
    /// an empty one clears it.
    AccountLabel {
        account: String,
        #[arg(default_value = "")]
        label: String,
    },
    /// Pay and register from `account` (`Public/<id>` or `Private/<id>`) from now on: it
    /// becomes the default of its kind. Names already held stay with the accounts that
    /// hold them, and payments to a name still go where the name points.
    AccountUse { account: String },
    /// The chain's fee market and what a native transfer costs at it, as JSON: `{height,
    /// base_fee_exec, base_fee_stor, public_transfer: {about, at_most}, private_transfer}`.
    Fee,
    /// Where the name identifier ID (64 hex) sits relative to the locked-name list, as JSON
    /// (`length_proof_json`). Needs no wallet. The Basecamp module asks here instead of
    /// carrying the 61 MB list, and checks the proof against the root itself.
    LengthProof { id: String },
}

/// A length proof as JSON: `{tier, kind: "member", index, path}` or `{tier, kind:
/// "non_member", left: {id, tier}, left_index, left_path, right: {id, tier}, right_path}`,
/// hashes in hex.
pub fn length_proof_json(proof: &names_core::LengthProof, tier: u8) -> serde_json::Value {
    let path = |p: &[[u8; 32]]| p.iter().map(hex::encode).collect::<Vec<_>>();
    match proof {
        names_core::LengthProof::Membership(m) => serde_json::json!({
            "tier": tier, "kind": "member", "index": m.index, "leaf_tier": m.tier, "path": path(&m.path),
        }),
        names_core::LengthProof::NonMembership(n) => serde_json::json!({
            "tier": tier, "kind": "non_member",
            "left": { "id": hex::encode(n.left.id), "tier": n.left.tier }, "left_index": n.left_index, "left_path": path(&n.left_path),
            "right": { "id": hex::encode(n.right.id), "tier": n.right.tier }, "right_path": path(&n.right_path),
        }),
    }
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
            Request::Restore { .. } | Request::LengthProof { .. } => unreachable!("handled before the wallet is opened"),
            Request::Token(_) => unreachable!("handled with the token program chosen, in main"),
            Request::AccountList => self.module_account_list().await,
            Request::AccountNew { kind, label } => self.module_account_new(&kind, &label).await,
            Request::AccountLabel { account, label } => self.module_account_label(&account, &label),
            Request::AccountUse { account } => self.module_account_use(&account).await,
            Request::Fee => {
                let quote = self.wallet.client().get_fee_state().await?;
                let headroom = self.wallet.config().private_fee_headroom_blocks;
                println!("{}", fees::quote_json(&quote, headroom, NATIVE_TRANSFER_GAS_LIMIT));
                Ok(())
            }
        }
    }

    /// One of this wallet's accounts, written with its kind: `(private, id)`.
    fn owned(&self, text: &str) -> Result<(bool, AccountId)> {
        let private = if text.starts_with("Private/") {
            true
        } else if text.starts_with("Public/") {
            false
        } else {
            bail!("write the account as Public/<id> or Private/<id>");
        };
        let id = parse_account(text).map_err(|error| anyhow!(error))?;
        let key_chain = self.wallet.storage().key_chain();
        let held = if private {
            key_chain.private_account_ids().any(|(found, _)| found == id)
        } else {
            key_chain.public_account_ids().any(|(found, _)| found == id)
        };
        if !held {
            bail!("this wallet has no account {text}");
        }
        Ok((private, id))
    }

    /// `account` as an `account-list` entry.
    async fn account_entry(&self, private: bool, id: AccountId, defaults: &Defaults) -> serde_json::Value {
        let account = format!("{}/{id}", if private { "Private" } else { "Public" });
        // A private account's nullifier public key is what a name's profile publishes to be
        // paid there, so the app can tell which name an account receives for. It is public.
        let npk = private
            .then(|| self.wallet.storage().key_chain().private_account(id).map(|found| hex::encode(found.key_chain.nullifier_public_key.0)))
            .flatten();
        let (balance, default, public_key) = if private {
            let balance = self.wallet.get_account_private(id).and_then(|found| found.data.native_balance().ok());
            (balance, defaults.private.as_deref() == Some(&id.to_string()), None)
        } else {
            let balance = self.wallet.get_account_balance(id).await.ok();
            let key = self
                .wallet
                .get_account_public_signing_key(id)
                .map(|key| hex::encode(lee::PublicKey::new_from_private_key(key).value()));
            (balance, defaults.public.as_deref() == Some(&id.to_string()), key)
        };
        serde_json::json!({
            "account": account,
            "kind": if private { "private" } else { "public" },
            "label": defaults.labels.get(&account),
            "balance": balance.map(|balance| balance.to_string()),
            "default": default,
            "public_key": public_key,
            "npk": npk,
        })
    }

    async fn module_account_list(&self) -> Result<()> {
        let defaults = load_defaults();
        let key_chain = self.wallet.storage().key_chain();
        let accounts: Vec<(bool, AccountId)> = key_chain
            .public_account_ids()
            .map(|(id, _)| (false, id))
            .chain(key_chain.private_account_ids().map(|(id, _)| (true, id)))
            .collect();
        let mut list = Vec::new();
        for (private, id) in accounts {
            list.push(self.account_entry(private, id, &defaults).await);
        }
        println!("{}", serde_json::Value::Array(list));
        Ok(())
    }

    async fn module_account_new(&mut self, kind: &str, label: &str) -> Result<()> {
        let label = checked_label(label)?;
        let private = kind == "private";
        let id = crate::new_account_saved(&mut self.wallet, private)?;
        let mut defaults = load_defaults();
        let account = format!("{}/{id}", if private { "Private" } else { "Public" });
        if !label.is_empty() {
            defaults.labels.insert(account, label);
        }
        // A wallet with no default of this kind (one made outside `names setup`) takes it.
        let slot = if private { &mut defaults.private } else { &mut defaults.public };
        if slot.is_none() {
            *slot = Some(id.to_string());
        }
        save_defaults(&defaults)?;
        println!("{}", self.account_entry(private, id, &defaults).await);
        Ok(())
    }

    fn module_account_label(&self, account: &str, label: &str) -> Result<()> {
        let label = checked_label(label)?;
        let (private, id) = self.owned(account)?;
        let account = format!("{}/{id}", if private { "Private" } else { "Public" });
        let mut defaults = load_defaults();
        if label.is_empty() {
            defaults.labels.remove(&account);
        } else {
            defaults.labels.insert(account.clone(), label.clone());
        }
        save_defaults(&defaults)?;
        println!("{}", serde_json::json!({ "account": account, "label": (!label.is_empty()).then_some(label) }));
        Ok(())
    }

    async fn module_account_use(&self, account: &str) -> Result<()> {
        let (private, id) = self.owned(account)?;
        let mut defaults = load_defaults();
        if private {
            defaults.private = Some(id.to_string());
        } else {
            defaults.public = Some(id.to_string());
        }
        save_defaults(&defaults)?;
        println!("{}", self.account_entry(private, id, &defaults).await);
        Ok(())
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
        // The module's change is any registry call: priced as a profile change's cycles.
        let cycles = borsh::from_slice::<names_core::Message>(&message)
            .ok()
            .and_then(|message| match message {
                names_core::Message::Instruction(instruction) => Some(crate::call_cycles(&instruction)),
                _ => None,
            })
            .unwrap_or(65_590);
        self.transact(mentions, message, vec![(self.program, program)], spent, cycles).await
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

/// A nickname as given, trimmed: at most [`MAX_LABEL`] characters, none of them control
/// characters. Empty means none.
fn checked_label(label: &str) -> Result<String> {
    let label = label.trim();
    if label.chars().count() > MAX_LABEL {
        bail!("a nickname is at most {MAX_LABEL} characters");
    }
    if label.chars().any(char::is_control) {
        bail!("a nickname is one line of text");
    }
    Ok(label.to_owned())
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
