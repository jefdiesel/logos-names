//! The Logos Blockchain side of the wallet (LP-0022): balances, transfers, stake and
//! leader rewards, through the user's own Logos Blockchain node (Basecamp's, or a
//! headless one). The node holds the keys and proves each transfer; nothing here talks
//! to any other service.
//!
//! Staking on the Logos Blockchain (Cryptarchia) has no stake operation: every note on
//! a key the node holds plays the leader lottery once it is in an epoch's stake
//! snapshot, and leader wins pay vouchers that `claim` turns into notes. So staking is
//! holding tokens on the node's keys, unstaking is sending them elsewhere, and cashing
//! out is claiming.

use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context as _, Result};
use clap::Subcommand;
use serde_json::{json, Value};

/// Basecamp's node serves its HTTP API here.
pub const NODE: &str = "http://127.0.0.1:8080";

/// The text entry by which a name publishes its Logos Blockchain public key, so that
/// `names chain send 100 alice` finds where to send.
pub const BLOCKCHAIN_KEY: &str = "blockchain";

/// How long `send` waits for its transaction to land in a block before handing back.
const LAND_PATIENCE: Duration = Duration::from_secs(120);

#[derive(Subcommand, Debug, Clone)]
pub enum ChainCommand {
    /// The node: its height, the last final block, the epoch, mining and gas prices.
    Status {
        /// Your Logos Blockchain node's HTTP API.
        #[arg(long, env = "LOGOS_NODE", default_value = NODE)]
        node: String,
        #[arg(long)]
        json: bool,
    },
    /// A key's balance and the notes it is made of. KEY is a Logos Blockchain public key
    /// (64 hex characters) or a name that publishes one (its `blockchain` entry).
    Balance {
        key: String,
        #[arg(long, env = "LOGOS_NODE", default_value = NODE)]
        node: String,
        #[arg(long)]
        json: bool,
    },
    /// Send tokens on the Logos Blockchain to a key or to a name that publishes one.
    /// The node spends notes of `--from-key` and proves the transfer. Without `--yes`
    /// it only shows the plan.
    Send {
        /// Amount in base units.
        amount: u64,
        /// A public key (64 hex characters) or a name with a `blockchain` entry.
        to: String,
        /// The key whose notes pay, as the node's wallet lists it.
        #[arg(long)]
        from_key: String,
        /// Where the change goes (default: back to `--from-key`).
        #[arg(long)]
        change_key: Option<String>,
        #[arg(long, env = "LOGOS_NODE", default_value = NODE)]
        node: String,
        /// Send it. A transfer cannot be undone.
        #[arg(long)]
        yes: bool,
        /// Hand back once the node accepts it, without waiting for a block.
        #[arg(long)]
        no_wait: bool,
        #[arg(long)]
        json: bool,
    },
    /// Whether a transaction is in a block yet, and the fee it paid.
    Tx {
        hash: String,
        #[arg(long, env = "LOGOS_NODE", default_value = NODE)]
        node: String,
        #[arg(long)]
        json: bool,
    },
    /// Stake: the node's notes that play the leader lottery, how long new tokens wait
    /// before they count, and the leader rewards waiting to be claimed. To unstake,
    /// send the tokens to a key the node doesn't hold.
    Stake {
        #[arg(long, env = "LOGOS_NODE", default_value = NODE)]
        node: String,
        #[arg(long)]
        json: bool,
    },
    /// Claim a leader reward: the node's claim cashes ONE voucher per transaction and
    /// pays its own fee from the leader funding key, which can be more than the reward.
    /// Without `--yes` it only shows what is claimable.
    Claim {
        #[arg(long, env = "LOGOS_NODE", default_value = NODE)]
        node: String,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        json: bool,
    },
}

impl ChainCommand {
    /// The name this command must resolve to a key first, if any.
    pub fn name(&self) -> Option<&str> {
        let target = match self {
            Self::Balance { key, .. } => key,
            Self::Send { to, .. } => to,
            _ => return None,
        };
        match parse_target(target) {
            Ok(Target::Name(_)) => Some(target),
            _ => None,
        }
    }
}

/// Where a transfer goes, or whose balance to read.
#[derive(Debug, PartialEq, Eq)]
pub enum Target {
    Key(String),
    Name(String),
}

/// A 64-hex-character public key (any case, `0x` allowed), or else a name.
pub fn parse_target(input: &str) -> Result<Target, String> {
    if let Some(key) = parse_key(input) {
        return Ok(Target::Key(key));
    }
    let looks_hex = input.trim().trim_start_matches("0x").chars().all(|c| c.is_ascii_hexdigit());
    if looks_hex && input.trim().len() > 32 {
        return Err(format!("{input} is not a public key: a key is 64 hex characters"));
    }
    crate::parse_name(input).map(Target::Name)
}

/// A Logos Blockchain public key, lowercase, or `None`.
pub fn parse_key(input: &str) -> Option<String> {
    let key = input.trim().trim_start_matches("0x").to_lowercase();
    (key.len() == 64 && key.chars().all(|c| c.is_ascii_hexdigit())).then_some(key)
}

/// The key a profile publishes under `blockchain`.
pub fn key_of(profile: &names_seal::Profile) -> Option<String> {
    let entry = profile.texts.iter().find(|text| text.key == BLOCKCHAIN_KEY)?;
    parse_key(&entry.value)
}

/// The node's HTTP API.
pub struct Node {
    url: String,
}

impl Node {
    pub fn new(url: &str) -> Self {
        Self { url: url.trim_end_matches('/').to_owned() }
    }

    fn reply(&self, path: &str, response: Result<ureq::http::Response<ureq::Body>, ureq::Error>) -> Result<Value> {
        let mut response = response.with_context(|| format!("cannot reach your Logos Blockchain node at {}", self.url))?;
        let status = response.status();
        let text = response.body_mut().read_to_string()?;
        if !status.is_success() {
            bail!("the node refused {path} (HTTP {status}): {}", text.trim());
        }
        serde_json::from_str(&text).with_context(|| format!("the node's reply to {path} is not JSON: {text}"))
    }

    pub fn get(&self, path: &str) -> Result<Value> {
        let response = ureq::get(&format!("{}{path}", self.url)).config().http_status_as_error(false).build().call();
        self.reply(path, response)
    }

    /// `None` when the node answers 404 (a transaction not in a block yet).
    pub fn find(&self, path: &str) -> Result<Option<Value>> {
        let response = ureq::get(&format!("{}{path}", self.url)).config().http_status_as_error(false).build().call();
        let status = response.as_ref().map(|r| r.status().as_u16()).unwrap_or(0);
        if status == 404 {
            return Ok(None);
        }
        self.reply(path, response).map(Some)
    }

    pub fn post(&self, path: &str, body: Option<&Value>) -> Result<Value> {
        let request = ureq::post(&format!("{}{path}", self.url)).config().http_status_as_error(false).build();
        let response = match body {
            Some(body) => request.header("Content-Type", "application/json").send(body.to_string()),
            None => request.send_empty(),
        };
        self.reply(path, response)
    }
}

/// A JSON number or numeric string, as the node writes values.
fn amount(value: &Value) -> u128 {
    match value {
        Value::Number(n) => n.as_u64().map(u128::from).unwrap_or(0),
        Value::String(s) => s.parse().unwrap_or(0),
        _ => 0,
    }
}

/// `1234567` as `1,234,567`.
pub fn grouped(value: u128) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A key shortened for a line of text: `66ecf4bc…a81b`.
pub fn short(key: &str) -> String {
    if key.len() <= 16 {
        return key.to_owned();
    }
    format!("{}…{}", &key[..8], &key[key.len() - 4..])
}

/// About how long, in words.
fn duration_words(seconds: u64) -> String {
    match seconds {
        0..=89 => format!("{seconds} s"),
        90..=5399 => format!("{} min", (seconds + 30) / 60),
        5400..=172_799 => format!("{:.1} h", seconds as f64 / 3600.0),
        _ => format!("{:.1} days", seconds as f64 / 86_400.0),
    }
}

/// What one claim pays: one voucher's reward, not every voucher's.
fn claimed_by_one(vouchers: &Value) -> u128 {
    amount(&vouchers["reward_amount"])
}

/// The stake picture from the node's replies to `/leader/aged-notes`,
/// `/leader/claim/vouchers` and `/time/info`.
pub fn stake_summary(aged: &Value, vouchers: &Value, time: &Value) -> Value {
    let notes = aged["notes"].as_array().cloned().unwrap_or_default();
    let mut keys: Vec<String> = notes.iter().filter_map(|n| n["public_key"].as_str().map(str::to_owned)).collect();
    keys.sort();
    keys.dedup();
    let staked = amount(&aged["total_value"]);
    let slot = time["current_slot"].as_u64().unwrap_or(0);
    let per_epoch = time["slots_per_epoch"].as_u64().unwrap_or(0);
    let slot_ms = time["slot_duration_ms"].as_u64().unwrap_or(1000);
    let next_epoch_in_s = if per_epoch == 0 { 0 } else { (per_epoch - slot % per_epoch) * slot_ms / 1000 };
    json!({
        "staked": staked.to_string(),
        "notes": notes.len(),
        "keys": keys,
        "epoch": time["current_epoch"],
        "next_epoch_in_s": next_epoch_in_s,
        "claimable": amount(&vouchers["total_claimable"]).to_string(),
        "vouchers": vouchers["vouchers"].as_array().map_or(0, Vec::len),
        "reward_per_voucher": amount(&vouchers["reward_amount"]).to_string(),
    })
}

/// `stake_summary` for people.
pub fn stake_lines(summary: &Value) -> Vec<String> {
    let staked: u128 = summary["staked"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0);
    let notes = summary["notes"].as_u64().unwrap_or(0);
    let keys: Vec<&str> = summary["keys"].as_array().map(|k| k.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    let wait = duration_words(summary["next_epoch_in_s"].as_u64().unwrap_or(0));
    let mut lines = vec![match keys.as_slice() {
        [] => "staked: 0 (nothing has aged into an epoch snapshot yet)".to_owned(),
        [key] => format!("staked: {} in {notes} note(s) on {}", grouped(staked), short(key)),
        _ => format!("staked: {} in {notes} notes on {} keys", grouped(staked), keys.len()),
    }];
    lines.push(format!(
        "epoch {}; the next starts in about {wait}. New tokens join the stake at an epoch boundary, up to two epochs after they arrive",
        summary["epoch"]
    ));
    let claimable: u128 = summary["claimable"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0);
    let vouchers = summary["vouchers"].as_u64().unwrap_or(0);
    lines.push(if vouchers == 0 {
        "rewards: nothing to claim".to_owned()
    } else {
        format!("rewards: {} claimable from {vouchers} leader win(s); `names chain claim --yes` cashes one per claim, each paying its own fee", grouped(claimable))
    });
    lines.push("to unstake, send tokens to a key this node doesn't hold".to_owned());
    lines
}

/// One line for each fact in `/cryptarchia/info`, `/time/info`, `/pow/status` and `/mantle/gas-prices`.
pub fn status_lines(info: &Value, time: &Value, pow: Option<&Value>, gas: Option<&Value>) -> Vec<String> {
    let chain = &info["cryptarchia_info"];
    let mut lines = vec![
        format!("height {} at slot {}, {} ({})", chain["height"], chain["slot"], chain["state"].as_str().unwrap_or("?"), info["phase"].as_str().unwrap_or("?")),
        format!("final up to slot {}", chain["lib_slot"]),
        format!("epoch {}", time["current_epoch"]),
    ];
    if let Some(pow) = pow {
        lines.push(format!("mining: {}", if pow["is_mining"] == true { "on" } else { "off" }));
    }
    if let Some(gas) = gas {
        lines.push(format!("gas prices: execution {}, storage {}", gas["execution_base_gas_price"], gas["storage_gas_price"]));
    }
    lines
}

/// The transaction as the node stored it. The node stores one as soon as it accepts
/// it, so being found does not mean it is in a block: see [`spent`].
fn stored(node: &Node, hash: &str) -> Result<Option<Value>> {
    node.find(&format!("/cryptarchia/transaction/{hash}"))
}

/// The note ids a transfer spends and the total it pays out (to the recipient and
/// back as change), from the transaction's transfer ops (opcode 0).
pub fn transfer_parts(transaction: &Value) -> (Vec<String>, u128) {
    let ops = transaction["mantle_tx"]["ops"].as_array().cloned().unwrap_or_default();
    let mut inputs = Vec::new();
    let mut paid = 0;
    for op in ops.iter().filter(|op| op["opcode"] == 0) {
        inputs.extend(op["payload"]["inputs"].as_array().into_iter().flatten().filter_map(|i| i.as_str().map(str::to_owned)));
        paid += op["payload"]["outputs"].as_array().into_iter().flatten().map(|o| amount(&o["value"])).sum::<u128>();
    }
    (inputs, paid)
}

/// What a transfer paid in fees: what its inputs held, less what it pays out. `notes`
/// is the sender's notes before it sent (`{id: value}`); `None` if any input isn't there.
pub fn transfer_fee(transaction: &Value, notes: &serde_json::Map<String, Value>) -> Option<u128> {
    let (inputs, paid) = transfer_parts(transaction);
    if inputs.is_empty() {
        return None;
    }
    let held: Option<u128> = inputs.iter().map(|id| notes.get(id).map(amount)).sum();
    held?.checked_sub(paid)
}

/// Whether the sender's wallet no longer holds any of `inputs`. The wallet follows the
/// chain, not the mempool, so this is when the transfer is in a block.
fn spent(node: &Node, from: &str, inputs: &[String]) -> bool {
    match node.get(&format!("/wallet/{from}/balance")) {
        Ok(reply) => reply["notes"].as_object().is_some_and(|notes| inputs.iter().all(|id| !notes.contains_key(id))),
        Err(_) => false,
    }
}

/// Whether the node's mempool holds `hash`, or `None` if it couldn't be read.
fn in_mempool(node: &Node, hash: &str) -> Option<bool> {
    let view = node.get("/mempool/view").ok()?;
    Some(view.as_array()?.iter().any(|h| h.as_str() == Some(hash)))
}

/// Runs a chain command. `resolved` is the key a name target resolved to, with the name.
pub fn run(command: ChainCommand, resolved: Option<(String, String)>) -> Result<()> {
    let key_for = |target: &str| -> Result<(String, Option<String>)> {
        match (&resolved, parse_target(target).map_err(|e| anyhow!(e))?) {
            (_, Target::Key(key)) => Ok((key, None)),
            (Some((name, key)), Target::Name(_)) => Ok((key.clone(), Some(name.clone()))),
            (None, Target::Name(name)) => bail!("{name} needs resolving first"),
        }
    };
    match command {
        ChainCommand::Status { node, json } => {
            let node = Node::new(&node);
            let info = node.get("/cryptarchia/info")?;
            let time = node.get("/time/info")?;
            let pow = node.get("/pow/status").ok();
            let gas = node.get("/mantle/gas-prices").ok();
            if json {
                println!("{}", json!({ "info": info, "time": time, "pow": pow, "gas": gas }));
            } else {
                for line in status_lines(&info, &time, pow.as_ref(), gas.as_ref()) {
                    println!("{line}");
                }
            }
            Ok(())
        }
        ChainCommand::Balance { key, node, json } => {
            let (key, name) = key_for(&key)?;
            let reply = Node::new(&node).get(&format!("/wallet/{key}/balance"))?;
            let balance = amount(&reply["balance"]);
            let notes = reply["notes"].as_object().map_or(0, |n| n.len());
            if json {
                println!("{}", json!({ "key": key, "name": name, "balance": balance.to_string(), "notes": notes }));
            } else {
                let who = name.map(|n| format!("{n} ({})", short(&key))).unwrap_or_else(|| short(&key));
                println!("{who}: {} in {notes} note(s)", grouped(balance));
            }
            Ok(())
        }
        ChainCommand::Send { amount: value, to, from_key, change_key, node, yes, no_wait, json } => {
            let (to_key, name) = key_for(&to)?;
            let from = parse_key(&from_key).ok_or_else(|| anyhow!("--from-key must be a 64-character hex public key"))?;
            let change = match change_key {
                Some(key) => parse_key(&key).ok_or_else(|| anyhow!("--change-key must be a 64-character hex public key"))?,
                None => from.clone(),
            };
            if to_key == from {
                bail!("that sends {from} to itself");
            }
            let node = Node::new(&node);
            let before = node.get(&format!("/wallet/{from}/balance"))?;
            let held = amount(&before["balance"]);
            let notes_before = before["notes"].as_object().cloned().unwrap_or_default();
            if held < u128::from(value) {
                bail!("{} holds {}, less than {}", short(&from), grouped(held), grouped(value.into()));
            }
            let target = match &name {
                Some(name) => format!("{name} ({})", short(&to_key)),
                None => short(&to_key),
            };
            // About what it will cost, asked of the node with stand-ins of these notes (chain_fee.rs).
            let fee_about = if yes {
                None
            } else {
                let values: Vec<u128> = notes_before.values().map(amount).collect();
                crate::chain_fee::estimate(&from, &to_key, value, &values, |body| node.post("/wallet/fund", Some(body)))
            };
            if !yes && json {
                println!("{}", json!({ "plan": true, "from": from, "to": to_key, "name": name, "amount": value.to_string(), "change": change, "balance": held.to_string(), "fee_about": fee_about.map(|f| f.to_string()) }));
                return Ok(());
            }
            if !yes {
                println!("send {} from {} to {target}", grouped(value.into()), short(&from));
                println!("  change back to {}, through {}", short(&change), node.url);
                match fee_about {
                    Some(fee) => println!("  fee: about {} (the node's own figure at today's gas prices; paid from {}'s notes)", grouped(fee), short(&from)),
                    None => println!("  fee: the node couldn't say in advance (or this key can't pay the amount and its fee); it is shown once sent"),
                }
                println!("nothing sent; run again with --yes to send (it cannot be undone)");
                return Ok(());
            }
            let body = json!({
                "tip": null,
                "change_public_key": change,
                "funding_public_keys": [from],
                "recipient_public_key": to_key,
                "amount": value,
            });
            let reply = node.post("/wallet/transactions/transfer-funds", Some(&body))?;
            let hash = reply["hash"].as_str().ok_or_else(|| anyhow!("the node accepted the transfer but returned no hash: {reply}"))?.to_owned();
            // Printed at once, before any waiting, so a sent transaction's hash is never lost.
            if !json {
                println!("sent {} to {target}: transaction {hash}", grouped(value.into()));
            }
            // A failed look is not a failed send: from here on nothing fails, it reports what it saw.
            let transaction = stored(&node, &hash).ok().flatten();
            let fee = transaction.as_ref().and_then(|tx| transfer_fee(tx, &notes_before));
            let inputs = transaction.as_ref().map(|tx| transfer_parts(tx).0).unwrap_or_default();
            if !json {
                if let Some(fee) = fee {
                    println!("fee: {}", grouped(fee));
                }
            }
            let mut landed = false;
            if !no_wait && !inputs.is_empty() {
                let started = Instant::now();
                while started.elapsed() < LAND_PATIENCE {
                    std::thread::sleep(Duration::from_secs(5));
                    if spent(&node, &from, &inputs) {
                        landed = true;
                        break;
                    }
                }
            }
            if json {
                println!("{}", json!({ "hash": hash, "to": to_key, "name": name, "amount": value.to_string(), "state": if landed { "in_block" } else { "pending" }, "fee": fee.map(|f| f.to_string()) }));
            } else if landed {
                println!("in a block");
            } else if !no_wait {
                println!("not in a block yet; check with `names chain tx {hash}`");
            }
            Ok(())
        }
        ChainCommand::Tx { hash, node, json } => {
            let hash = hash.trim().trim_start_matches("0x").to_lowercase();
            let node = Node::new(&node);
            let found = stored(&node, &hash)?;
            // Stored and out of the mempool: in a block. Stored and still in it: waiting.
            let state = match (&found, in_mempool(&node, &hash)) {
                (None, _) => "unknown",
                (Some(_), Some(true)) => "pending",
                (Some(_), Some(false)) => "in_block",
                (Some(_), None) => "stored",
            };
            if json {
                println!("{}", json!({ "hash": hash, "state": state, "transaction": found }));
            } else {
                println!("{hash}: {}", match state {
                    "unknown" => "this node doesn't know it",
                    "pending" => "waiting in the mempool for a block",
                    "in_block" => "in a block",
                    _ => "accepted; the mempool couldn't be read to say whether it is in a block yet",
                });
            }
            Ok(())
        }
        ChainCommand::Stake { node, json } => {
            let node = Node::new(&node);
            let summary = stake_summary(&node.get("/leader/aged-notes")?, &node.get("/leader/claim/vouchers")?, &node.get("/time/info")?);
            if json {
                println!("{summary}");
            } else {
                for line in stake_lines(&summary) {
                    println!("{line}");
                }
            }
            Ok(())
        }
        ChainCommand::Claim { node, yes, json } => {
            let node = Node::new(&node);
            let vouchers = node.get("/leader/claim/vouchers")?;
            let claimable = amount(&vouchers["total_claimable"]);
            let count = vouchers["vouchers"].as_array().map_or(0, Vec::len);
            if count == 0 {
                if json {
                    println!("{}", json!({ "claimed": "0", "hash": null }));
                } else {
                    println!("nothing to claim");
                }
                return Ok(());
            }
            if !yes {
                if json {
                    println!("{}", json!({ "plan": true, "claimable": claimable.to_string(), "vouchers": count }));
                } else {
                    println!("{} claimable from {count} leader win(s); run again with --yes to claim one ({}; each claim pays its own fee)", grouped(claimable), grouped(amount(&vouchers["reward_amount"])));
                }
                return Ok(());
            }
            let reply = node.post("/leader/claim", None)?;
            let hash = reply["tx_hash"].as_str().unwrap_or_default();
            // One voucher per claim (the node makes a single claim op).
            let claimed = claimed_by_one(&vouchers);
            if json {
                println!("{}", json!({ "claimed": claimed.to_string(), "hash": hash, "vouchers_left": count - 1 }));
            } else {
                println!("claimed {} (one of {count} leader wins): transaction {hash}", grouped(claimed));
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "66ecf4bccaa13e4e03c3d5c0e3ebecb945c51bbf815e92727a9a4efa6b39a81b";

    #[test]
    fn targets_are_keys_or_names() {
        assert_eq!(parse_target(KEY), Ok(Target::Key(KEY.to_owned())));
        assert_eq!(parse_target(&format!("0x{}", KEY.to_uppercase())), Ok(Target::Key(KEY.to_owned())));
        assert_eq!(parse_target("@Alice"), Ok(Target::Name("alice".to_owned())));
        assert!(parse_target(&KEY[..60]).is_err(), "a short key is a typo, not a name");
    }

    #[test]
    fn only_name_targets_need_resolving() {
        let send = |to: &str| ChainCommand::Send {
            amount: 1,
            to: to.to_owned(),
            from_key: KEY.to_owned(),
            change_key: None,
            node: NODE.to_owned(),
            yes: false,
            no_wait: false,
            json: false,
        };
        assert_eq!(send("alice").name(), Some("alice"));
        assert_eq!(send(KEY).name(), None);
        assert_eq!(ChainCommand::Stake { node: NODE.to_owned(), json: false }.name(), None);
    }

    /// The node's replies, as the M2 node (0.3.4) gave them.
    #[test]
    fn stake_reads_the_nodes_replies() {
        let aged = json!({ "tip": "9a", "notes": [
            { "note_id": "2d", "value": 1_919_959, "public_key": KEY },
            { "note_id": "89", "value": 1_897_565, "public_key": KEY },
        ], "count": 2, "total_value": 3_817_524 });
        let vouchers = json!({ "tip": "9a", "vouchers": [], "reward_amount": 13140, "total_claimable": 0 });
        let time = json!({ "slot_duration_ms": 1000, "current_slot": 2_286_020, "current_epoch": 7, "slots_per_epoch": 300_000 });
        let summary = stake_summary(&aged, &vouchers, &time);
        assert_eq!(summary["staked"], "3817524");
        assert_eq!(summary["next_epoch_in_s"], 300_000 - 2_286_020 % 300_000);
        let lines = stake_lines(&summary);
        assert_eq!(lines[0], "staked: 3,817,524 in 2 note(s) on 66ecf4bc…a81b");
        assert!(lines[1].starts_with("epoch 7; the next starts in about 31.7 h."), "{}", lines[1]);
        assert_eq!(lines[2], "rewards: nothing to claim");
    }

    #[test]
    fn stake_spread_over_keys_names_no_single_key() {
        let aged = json!({ "notes": [
            { "value": 5, "public_key": KEY },
            { "value": 7, "public_key": "ab".repeat(32) },
        ], "total_value": 12 });
        let vouchers = json!({ "vouchers": [{ "commitment": "c", "nullifier": "n" }], "reward_amount": 13140, "total_claimable": 13140 });
        let lines = stake_lines(&stake_summary(&aged, &vouchers, &json!({})));
        assert_eq!(lines[0], "staked: 12 in 2 notes on 2 keys");
        assert!(lines[2].starts_with("rewards: 13,140 claimable from 1 leader win(s)"), "{}", lines[2]);
        let many = json!({ "vouchers": vec![json!({ "commitment": "c", "nullifier": "n" }); 1722], "reward_amount": "38", "total_claimable": "65436" });
        assert_eq!(claimed_by_one(&many), 38, "a claim cashes one voucher");
    }

    #[test]
    fn status_reads_the_nodes_replies() {
        let info = json!({ "cryptarchia_info": { "lib_slot": 2_247_197, "slot": 2_285_801, "height": 66446, "state": "Online" }, "phase": "Following" });
        let time = json!({ "current_epoch": 7 });
        let lines = status_lines(&info, &time, Some(&json!({ "is_mining": true })), Some(&json!({ "execution_base_gas_price": 1, "storage_gas_price": 6 })));
        assert_eq!(lines, [
            "height 66446 at slot 2285801, Online (Following)",
            "final up to slot 2247197",
            "epoch 7",
            "mining: on",
            "gas prices: execution 1, storage 6",
        ]);
    }

    #[test]
    fn numbers_read_and_print() {
        assert_eq!(grouped(1_191_220_464), "1,191,220,464");
        assert_eq!(grouped(999), "999");
        assert_eq!(amount(&json!("42")), 42);
    }

    /// The transfer sent from the M2 node on 2026-11-04 (dc77f07b…): one input note of
    /// 3,608,664, paying 1,000 and 3,605,610 back as change, so a fee of 2,054.
    #[test]
    fn a_transfers_fee_is_its_inputs_less_its_outputs() {
        let tx = json!({ "mantle_tx": { "hash": "dc77", "ops": [{ "opcode": 0, "payload": {
            "inputs": ["b271"],
            "outputs": [{ "value": 1000, "pk": "5f9f" }, { "value": 3_605_610, "pk": KEY }] } }] } });
        assert_eq!(transfer_parts(&tx), (vec!["b271".to_owned()], 3_606_610));
        let notes = json!({ "b271": 3_608_664, "0326": 1_803_521 });
        assert_eq!(transfer_fee(&tx, notes.as_object().unwrap()), Some(2054));
        assert_eq!(transfer_fee(&tx, json!({ "0326": 1 }).as_object().unwrap()), None, "an input it didn't see");
    }
}
