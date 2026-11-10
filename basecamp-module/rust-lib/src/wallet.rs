//! The wallet that signs a name's public changes: the one the `names` CLI keeps.
//! The module holds no keys and can't run the registry, so it hands the CLI what
//! it built, and the CLI tries it, sizes its gas, signs it and confirms it applied.
//!
//! This used to be the LEZ wallet module (`lez_core`). That one takes only account
//! IDs, so on LEZ 0.4 it can't give the seed that lets a transaction create a
//! registry account: a commitment or a registration sent through it would be
//! refused and still charged. See `docs/lez-0.4-port.md`.

use crate::changes::{Change, Mention};
use crate::cli::{self, Setup};
use crate::registry::{self, AccountId};
use serde_json::{json, Value};
use std::time::Duration;

/// Asking for the accounts or keys reads the wallet's file and nothing else.
const LOOK: Duration = Duration::from_secs(20);
/// One block, and the run that tries the change first. A slow chain can take
/// longer: the change may then still land, so the caller reads the chain back.
const SEND: Duration = Duration::from_secs(120);

/// The accounts in the wallet, each kind in the wallet's own order. A private
/// account's balance is known only to the wallet.
#[derive(Debug, Default)]
pub struct Held {
    pub public: Vec<AccountId>,
    pub private: Vec<(AccountId, u128)>,
}

fn request(words: &[&str]) -> Vec<String> {
    std::iter::once("module").chain(words.iter().copied()).map(str::to_owned).collect()
}

/// The last line of what the CLI printed, which is its answer: the wallet prints
/// its own lines before it.
fn answer(output: &str) -> Result<Value, String> {
    serde_json::from_str(cli::last_line(output)).map_err(|_| "the names CLI gave no answer it could be read by".to_owned())
}

fn parse_held(answer: &Value) -> Held {
    let id = |value: &Value| value.as_str().and_then(|text| registry::parse_account(text).ok());
    let list = |key: &str| answer[key].as_array().map(Vec::as_slice).unwrap_or_default().to_vec();
    Held {
        public: list("public").iter().filter_map(id).collect(),
        private: list("private")
            .iter()
            .filter_map(|entry| {
                let balance = entry["balance"].as_str().and_then(|text| text.parse().ok()).unwrap_or(0);
                Some((id(&entry["account"])?, balance))
            })
            .collect(),
    }
}

pub fn accounts(setup: &Setup) -> Result<Held, String> {
    setup.wallet_ready()?;
    match cli::run_text(setup, request(&["accounts"]), LOOK) {
        // A CLI too old for `module accounts` (the October testnet build) still lists
        // every account and its balance in `names balance`.
        Err(problem) if problem == cli::OLDER_CLI => {
            let mut held = Held { public: Vec::new(), private: Vec::new() };
            for row in &cli::run_accounts_any(setup)?.rows {
                let Some((private, id)) = row["account"].as_str().and_then(|text| crate::provider::account_ref(text).ok()) else { continue };
                if private {
                    held.private.push((id, row["balance"].as_str().and_then(|b| b.parse().ok()).unwrap_or(0)));
                } else {
                    held.public.push(id);
                }
            }
            Ok(held)
        }
        other => Ok(parse_held(&answer(&other?)?)),
    }
}

/// The keys others use to pay the private account `choice`: an account ID, or
/// `auto` for the wallet's first one, made if it has none.
pub fn private_keys(setup: &Setup, choice: &str) -> Result<([u8; 32], Vec<u8>), String> {
    let keys = answer(&cli::run_text(setup, request(&["keys", "--", choice.trim()]), LOOK)?)?;
    let read = |field: &str| keys[field].as_str().and_then(|text| registry::unhex(text).ok());
    let npk = read("npk").and_then(|bytes| <[u8; 32]>::try_from(bytes).ok());
    match (npk, read("vpk")) {
        (Some(npk), Some(vpk)) => Ok((npk, vpk)),
        _ => Err("the names CLI did not give that account's receiving keys".to_owned()),
    }
}

fn mention_json(mention: &Mention) -> Value {
    json!({
        "account": registry::account_text(&mention.account),
        "program": registry::account_text(&mention.program),
        "seed": mention.seed.as_ref().map(|seed| registry::hex(seed)),
        "signs": mention.signs,
    })
}

fn change_json(change: &Change) -> Value {
    json!({
        "message": registry::hex(&change.message),
        "spent": change.needs.to_string(),
        "mentions": change.mentions.iter().map(mention_json).collect::<Vec<_>>(),
    })
}

/// The transaction's hash, from the line the CLI prints when it submits one.
fn submitted(output: &str) -> Option<&str> {
    output.lines().find_map(|line| line.trim().strip_prefix("submitted "))
}

/// Signs `change` with its signer, tries it, sends it and waits for its block.
/// Returns the transaction's hash. A change that was refused or reverted is an
/// error: on LEZ 0.4 a failed transaction is charged, so it isn't one to repeat
/// blindly.
pub fn send(setup: &Setup, change: &Change) -> Result<String, String> {
    let input = change_json(change).to_string();
    let output = cli::run_with_input(setup, request(&["send"]), &input, SEND)?;
    submitted(&output).map(str::to_owned).ok_or_else(|| "the names CLI sent nothing".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cli_answer_is_read_from_its_last_line() {
        let held = parse_held(
            &answer(&format!(
                "reading the wallet\n{}\n",
                json!({ "public": [registry::account_text(&[1; 32])], "private": [{ "account": registry::account_text(&[2; 32]), "balance": "7" }] })
            ))
            .expect("an answer"),
        );
        assert_eq!(held.public, vec![[1; 32]]);
        assert_eq!(held.private, vec![([2; 32], 7)]);
        assert!(answer("no json here").is_err());
    }

    #[test]
    fn a_change_is_given_to_the_cli_with_its_evidence() {
        let change = Change {
            message: vec![0, 8],
            mentions: vec![
                Mention { account: [1; 32], program: [7; 32], seed: None, signs: true },
                Mention { account: [2; 32], program: [7; 32], seed: Some([3; 32]), signs: false },
            ],
            signer: [1; 32],
            needs: 5,
            extra: Value::Null,
        };
        let sent = change_json(&change);
        assert_eq!(sent["message"], "0008");
        assert_eq!(sent["spent"], "5");
        assert_eq!(sent["mentions"][0]["seed"], Value::Null);
        assert_eq!(sent["mentions"][0]["signs"], true);
        assert_eq!(sent["mentions"][1]["seed"], "03".repeat(32));
        assert_eq!(sent["mentions"][1]["program"], registry::account_text(&[7; 32]));
    }

    #[test]
    fn the_hash_comes_from_the_submitted_line() {
        assert_eq!(submitted("reading\nsubmitted abc\napplied in block 4\n"), Some("abc"));
        assert_eq!(submitted("nothing"), None);
    }
}
