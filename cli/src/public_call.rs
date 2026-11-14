//! A public call that someone else pays for (`names module public-call plan|sign`).
//!
//! A connected app (a bridge's mini-app, say) hands the Names module an exact call: the program,
//! the message bytes, the actors it declares, and a fee declaration naming ANOTHER account as the
//! fee payer (a relay's). The person approves a sheet; this signs the transaction as one of the
//! wallet's public accounts and gives it back half signed. The wallet sends nothing and pays
//! nothing: the app hands the transaction to the payer, which adds its own signature and submits it.
//!
//! What the signature covers (LEZ v0.4.0-rc3, `lee` `public_transaction::Message::hash`): SHA-256
//! under the domain `/LEE/v0.3/Message/Public/` of the whole borsh message: the public context
//! (declared actors and the accounts that sign), the root call (actor and message bytes), the fee
//! declaration (payer, gas limit, tip, max fee), the nonces of BOTH signers, and the admission
//! evidence. So the signed transaction can't be given another payer, other fee caps, other bytes
//! or other actors (each changes the hash, and the account's signature no longer verifies), and it
//! is good once: it names the account's and the payer's current nonces. rc3 public transactions
//! carry no validity window; the account's next transaction (any) makes this one dead.
//!
//! `plan` builds the message and says what it is (the hash the person approves); `sign` builds it
//! again, refuses unless the hash is the approved one (an account used since, or a payer whose
//! nonce moved, gives another), and signs. `cosign` (hidden; tests) is the payer's side: it adds the
//! payer's signature, checks the whole transaction, and submits it.

use std::collections::BTreeMap;

use anyhow::{Context as _, Result, anyhow, bail};
use lee::{
    FeeDeclaration, PublicKey, PublicTransaction, Signature,
    public_transaction::{Message, WitnessSet},
};
use lee_core::{
    account::{AccountId, Actor},
    program::PdaSeed,
};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

use crate::{Client, parse_account, parse_hex32};

/// The largest message a public call carries (the module's limit too).
pub const MAX_MESSAGE: usize = 16 * 1024;
/// The most actors a call declares besides its root.
pub const MAX_ACTORS: usize = 16;
/// The largest half-signed transaction `cosign` reads.
pub const MAX_TX_BYTES: usize = 64 * 1024;

/// What the module sends (`provider_public::cli_input`).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct CallInput {
    /// `Public/<id>`: the wallet's account that signs (the root actor's account).
    pub account: String,
    /// The root program.
    pub program: String,
    /// The root call's message bytes, hex.
    pub message: String,
    /// Declared actors besides the root: `{account, program}` or `{seed, of, program, new}`;
    /// `program` is a program ID or `native` (an account's LEZ balance).
    #[serde(default)]
    pub accounts: Vec<Value>,
    pub fee: FeeInput,
    /// `sign` only: the hash the person approved.
    #[serde(default)]
    pub expect_hash: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct FeeInput {
    pub payer: String,
    pub gas_limit: u64,
    #[serde(default)]
    pub tip: String,
    pub max_fee: String,
}

/// The call, built: everything the message holds except the nonces, which are read from the chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Built {
    pub account: AccountId,
    pub root: Actor,
    pub message: Vec<u8>,
    pub actors: Vec<Actor>,
    pub evidence: Vec<lee::PublicAccountEvidence>,
    pub fee: FeeDeclaration,
}

fn program_of(text: &str) -> Result<AccountId> {
    match text.trim() {
        "native" | "LEZ" => Ok(lee_core::native_token::NATIVE_TOKEN_PROGRAM_ID),
        other => parse_account(other.strip_prefix("Public/").unwrap_or(other)).map_err(|error| anyhow!(error)),
    }
}

fn public_id(text: &str, what: &str) -> Result<AccountId> {
    let text = text.trim();
    if text.starts_with("Private/") {
        bail!("{what} is a public account");
    }
    parse_account(text.strip_prefix("Public/").unwrap_or(text)).map_err(|error| anyhow!("{what}: {error}"))
}

fn number(text: &str, what: &str) -> Result<u128> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(0);
    }
    text.parse::<u128>().map_err(|_| anyhow!("{what} is a whole number of base units"))
}

impl CallInput {
    /// The call as it will be signed (no chain read): refuses what the wallet never signs.
    pub fn build(&self) -> Result<Built> {
        let account = public_id(&self.account, "the account that signs")?;
        let program = program_of(&self.program)?;
        if program == lee_core::native_token::NATIVE_TOKEN_PROGRAM_ID {
            bail!("a public call runs a program; the native token isn't called directly");
        }
        let payer = public_id(&self.fee.payer, "the fee payer")?;
        if payer == account {
            bail!("the fee payer is the account that signs: that is an ordinary call, not one someone else pays for");
        }
        let message = hex::decode(self.message.trim()).map_err(|_| anyhow!("the message is hex"))?;
        if message.is_empty() || message.len() > MAX_MESSAGE {
            bail!("the message is 1 to {MAX_MESSAGE} bytes");
        }
        if self.accounts.len() > MAX_ACTORS {
            bail!("a call declares at most {MAX_ACTORS} accounts besides its root");
        }
        let root = Actor::new(account, program);
        let mut actors = vec![root];
        let mut evidence = Vec::new();
        for entry in &self.accounts {
            let actor_program = program_of(entry["program"].as_str().unwrap_or(&self.program))?;
            let (id, made) = match (entry["account"].as_str(), entry["seed"].as_str()) {
                (Some(text), None) => (public_id(text, "a declared account")?, None),
                // A program account: the seed decides it; an address given beside it (the
                // module writes the one it derived) must be that one.
                (written, Some(seed)) => {
                    let seed = parse_hex32(seed).ok_or_else(|| anyhow!("a seed is 32 bytes of hex"))?;
                    let of = program_of(entry["of"].as_str().unwrap_or(&self.program))?;
                    let id = AccountId::for_public_pda(&of, &PdaSeed::new(seed));
                    if let Some(written) = written {
                        if public_id(written, "a program account")? != id {
                            bail!("a program account's address isn't the one its seed gives");
                        }
                    }
                    // Evidence admits a program account the call creates; only when asked.
                    let made = (entry["new"] == true).then(|| lee::PublicAccountEvidence::Pda { program: of, seed: PdaSeed::new(seed) });
                    (id, made)
                }
                (None, None) => bail!("each declared account has \"account\" or \"seed\" (with \"of\")"),
            };
            // The payer signs the transaction too, so a program would be authorized on its
            // accounts: the wallet never declares them (a relay refuses them as well).
            if id == payer {
                bail!("the call declares an account of the fee payer; the wallet doesn't sign that");
            }
            let actor = Actor::new(id, actor_program);
            if !actors.contains(&actor) {
                actors.push(actor);
            }
            if let Some(made) = made.filter(|made| !evidence.contains(made)) {
                evidence.push(made);
            }
        }
        let fee = FeeDeclaration::new(payer, self.fee.gas_limit, u64::try_from(number(&self.fee.tip, "the tip")?).map_err(|_| anyhow!("the tip is too large"))?, number(&self.fee.max_fee, "the max fee")?);
        if self.fee.gas_limit == 0 || fee.max_fee == 0 {
            bail!("the fee declaration has a gas limit and a max fee above zero");
        }
        Ok(Built { account, root, message, actors, evidence, fee })
    }
}

impl Built {
    /// The message with these nonces (the account's and the payer's).
    pub fn message(&self, account_nonce: lee_core::account::Nonce, payer_nonce: lee_core::account::Nonce) -> Message {
        let nonces: BTreeMap<AccountId, lee_core::account::Nonce> = [(self.account, account_nonce), (self.fee.payer, payer_nonce)].into_iter().collect();
        Message::new(self.root, self.message.clone(), self.actors.clone(), nonces, Some(self.fee), self.evidence.clone())
    }

    /// What `plan` and `sign` print about it.
    pub fn report(&self, message: &Message) -> Value {
        let actor = |actor: &Actor| json!({ "account": actor.account_id.to_string(), "program": actor.program_account_id.to_string() });
        json!({
            "hash": hex::encode(message.hash()),
            "account": format!("Public/{}", self.account),
            "program": self.root.program_account_id.to_string(),
            "payer": format!("Public/{}", self.fee.payer),
            "gas_limit": self.fee.gas_limit,
            "tip": self.fee.tip.to_string(),
            "max_fee": self.fee.max_fee.to_string(),
            "nonces": message.nonces.iter().map(|(id, nonce)| (id.to_string(), json!(nonce.0.to_string()))).collect::<serde_json::Map<_, _>>(),
            "actors": self.actors.iter().map(actor).collect::<Vec<_>>(),
            "creates": self.evidence.iter().map(|e| e.account_id().to_string()).collect::<Vec<_>>(),
            "own_balance": self.actors.iter().any(|a| *a == Actor::native_balance(self.account)),
            "message_len": self.message.len(),
            "message_sha256": hex::encode(Sha256::digest(&self.message)),
        })
    }
}

/// The transaction signed by `key` alone: the account's half.
pub fn half_signed(message: Message, key: &lee::PrivateKey) -> PublicTransaction {
    let hash = message.hash();
    PublicTransaction::new(message, WitnessSet::from_raw_parts(vec![(Signature::new(key, &hash), PublicKey::new_from_private_key(key))]))
}

/// The payer's half added to `tx`.
pub fn cosigned(tx: PublicTransaction, key: &lee::PrivateKey) -> PublicTransaction {
    let message = tx.message().clone();
    let hash = message.hash();
    let mut parts = tx.witness_set().clone().into_raw_parts();
    parts.push((Signature::new(key, &hash), PublicKey::new_from_private_key(key)));
    PublicTransaction::new(message, WitnessSet::from_raw_parts(parts))
}

impl Client {
    /// Builds the call with the chain's nonces; refuses an account the wallet can't sign for and
    /// a payer it can (`plan` and `sign` both).
    async fn public_call_message(&self, input: &CallInput) -> Result<(Built, Message)> {
        let built = input.build()?;
        if self.wallet.get_account_public_signing_key(built.account).is_none() {
            bail!("Public/{} is not a public account of this wallet", built.account);
        }
        if self.wallet.get_account_public_signing_key(built.fee.payer).is_some() {
            bail!("the fee payer Public/{} is one of this wallet's own accounts: a call this wallet pays for is an ordinary call", built.fee.payer);
        }
        let nonces = self.wallet.get_accounts_nonces(&[built.account, built.fee.payer]).await.context("could not read the nonces")?;
        let message = built.message(nonces[0], nonces[1]);
        Ok((built, message))
    }

    pub async fn public_call_plan(&self, input: &CallInput) -> Result<()> {
        let (built, message) = self.public_call_message(input).await?;
        println!("{}", built.report(&message));
        Ok(())
    }

    pub async fn public_call_sign(&self, input: &CallInput) -> Result<()> {
        let expect = input.expect_hash.as_deref().ok_or_else(|| anyhow!("sign needs the approved hash"))?;
        let (built, message) = self.public_call_message(input).await?;
        let hash = hex::encode(message.hash());
        if hash != expect.trim() {
            bail!("the call is no longer the one approved (hash {hash}, approved {expect}): the account or the payer was used since; nothing was signed");
        }
        let key = self.wallet.get_account_public_signing_key(built.account).ok_or_else(|| anyhow!("the account's key is gone"))?;
        let tx = half_signed(message.clone(), key);
        let mut report = built.report(&message);
        report["transaction"] = json!(hex::encode(borsh::to_vec(&tx)?));
        println!("{report}");
        Ok(())
    }

    /// The payer's side, for tests: adds this wallet's signature as the fee payer and submits.
    pub async fn public_call_cosign(&mut self, tx_hex: &str) -> Result<()> {
        let bytes = hex::decode(tx_hex.trim()).map_err(|_| anyhow!("the transaction is hex"))?;
        if bytes.len() > MAX_TX_BYTES {
            bail!("the transaction is too long");
        }
        let tx: PublicTransaction = borsh::from_slice(&bytes).context("not a public transaction")?;
        let payer = tx.message().execution.fee.map(|fee| fee.payer).ok_or_else(|| anyhow!("the transaction names no fee payer"))?;
        let key = self.wallet.get_account_public_signing_key(payer).ok_or_else(|| anyhow!("this wallet doesn't hold the fee payer's key"))?;
        let full = cosigned(tx, key);
        full.check_stateless().map_err(|error| anyhow!("the co-signed transaction is not valid: {error}"))?;
        let hash = sequencer_service_rpc::RpcClient::send_transaction(self.wallet.client(), common::transaction::LeeTransaction::Public(full))
            .await
            .map_err(|error| anyhow!("the sequencer refused it: {error}"))?;
        println!("submitted {hash}");
        let block = self.finish(hash).await.context("it was not applied")?;
        println!("applied in block {block}");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(byte: u8) -> (lee::PrivateKey, AccountId) {
        let key = lee::PrivateKey::try_new([byte; 32]).expect("a key");
        let id = AccountId::from(&PublicKey::new_from_private_key(&key));
        (key, id)
    }

    fn input(account: AccountId, payer: AccountId) -> CallInput {
        serde_json::from_value(json!({
            "account": format!("Public/{account}"),
            "program": AccountId::new([9; 32]).to_string(),
            "message": "0102030405",
            "accounts": [
                { "account": AccountId::new([8; 32]).to_string() },
                { "account": account.to_string(), "program": "native" },
                { "seed": "11".repeat(32), "of": AccountId::new([9; 32]).to_string(), "new": true }
            ],
            "fee": { "payer": payer.to_string(), "gas_limit": 120000, "tip": "0", "max_fee": "900000" }
        }))
        .unwrap()
    }

    /// The vector: the message these inputs give, its hash, and what the signatures do.
    #[test]
    fn a_half_signed_call_verifies_and_binds_its_payer() {
        let (account_key, account) = key(7);
        let (payer_key, payer) = key(8);
        let built = input(account, payer).build().unwrap();
        let message = built.message(lee_core::account::Nonce(3), lee_core::account::Nonce(41));
        // Pinned: the message bytes and hash for these inputs on LEZ v0.4.0-rc3.
        // What these inputs give, printed so a changed vector can be read off a failing run.
        eprintln!("account {account}\npayer {payer}\nhash {}\nmessage_borsh {}", hex::encode(message.hash()), hex::encode(borsh::to_vec(&message).unwrap()));
        let vectors: Value = serde_json::from_str(include_str!("../tests/vectors/public-call-v1.json")).unwrap();
        assert_eq!(account.to_string(), vectors["account"], "account id");
        assert_eq!(payer.to_string(), vectors["payer"], "payer id");
        assert_eq!(hex::encode(message.hash()), vectors["hash"], "message hash");
        assert_eq!(hex::encode(borsh::to_vec(&message).unwrap()), vectors["message_borsh"], "message bytes");
        let report = built.report(&message);
        assert_eq!(report["creates"].as_array().unwrap().len(), 1);
        assert_eq!(report["own_balance"], true);

        let half = half_signed(message.clone(), &account_key);
        assert!(half.witness_set().is_valid_for(&message), "the account's signature verifies");
        assert!(half.check_stateless().is_err(), "half signed, it isn't a valid transaction yet");
        let back: PublicTransaction = borsh::from_slice(&borsh::to_vec(&half).unwrap()).unwrap();
        assert_eq!(back, half, "borsh round trip");

        let full = cosigned(half.clone(), &payer_key);
        let mut signers = full.check_stateless().expect("co-signed, it is valid");
        signers.sort();
        let mut want = vec![account, payer];
        want.sort();
        assert_eq!(signers, want);

        // Another payer, other caps or another nonce is another message: the account's signature
        // doesn't carry over.
        let (_, other) = key(9);
        let rebuilt = input(account, other).build().unwrap().message(lee_core::account::Nonce(3), lee_core::account::Nonce(41));
        let moved = PublicTransaction::new(rebuilt.clone(), half.witness_set().clone());
        assert!(!moved.witness_set().is_valid_for(&rebuilt), "a new payer breaks the account's signature");
        let mut capped = input(account, payer);
        capped.fee.max_fee = "900001".into();
        let recapped = capped.build().unwrap().message(lee_core::account::Nonce(3), lee_core::account::Nonce(41));
        assert!(!half.witness_set().is_valid_for(&recapped), "new caps break it");
        let later = built.message(lee_core::account::Nonce(4), lee_core::account::Nonce(41));
        assert!(!half.witness_set().is_valid_for(&later), "the next nonce breaks it");
    }

    #[test]
    fn what_the_wallet_never_signs_is_refused() {
        let (_, account) = key(7);
        let (_, payer) = key(8);
        let mut same = input(account, account);
        assert!(same.build().unwrap_err().to_string().contains("ordinary call"));
        same = input(account, payer);
        same.accounts.push(json!({ "account": payer.to_string(), "program": "native" }));
        assert!(same.build().unwrap_err().to_string().contains("fee payer"));
        let mut private = input(account, payer);
        private.account = format!("Private/{account}");
        assert!(private.build().is_err());
        let mut empty = input(account, payer);
        empty.message = String::new();
        assert!(empty.build().is_err());
        let mut native = input(account, payer);
        native.program = "native".into();
        assert!(native.build().unwrap_err().to_string().contains("native token"));
        let mut free = input(account, payer);
        free.fee.max_fee = "0".into();
        assert!(free.build().is_err());
    }

    /// The module's own input (its pinned fixture) builds: the program account's address the
    /// module derived is the one the seed gives here, and a wrong one is refused.
    #[test]
    fn the_module_s_input_builds() {
        let text = include_str!("../../basecamp-module/rust-lib/testdata/relayed-call-input.json");
        let input: CallInput = serde_json::from_str(text).unwrap();
        let built = input.build().unwrap();
        assert_eq!(built.actors.len(), 4, "the root and three declared actors");
        assert!(built.actors.contains(&Actor::native_balance(public_id("CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", "x").unwrap())));
        assert_eq!(built.evidence.len(), 1);
        assert_eq!(built.evidence[0].account_id().to_string(), "2a8YAE9u9Xb2EjuyGEogYNz6RHYZGeuEemajpcgJUuKY");
        let mut wrong: CallInput = serde_json::from_str(text).unwrap();
        wrong.accounts[2]["account"] = json!("CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk");
        assert!(wrong.build().unwrap_err().to_string().contains("isn't the one its seed gives"));
    }
}
