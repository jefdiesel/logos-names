//! Changes made through the person's own `names` command-line client.
//!
//! A name held by a private account can't be changed through the LEZ wallet
//! module: its changes are proven on this machine, which the CLI does (a few
//! minutes each) with the keys in its own wallet directory. This runs that CLI
//! as a separate process, never through a shell: every argument is passed as
//! one argv entry, options as `--key=value` and the positionals after `--`, so
//! a value that starts with `-` or holds quotes or newlines arrives verbatim.
//!
//! A proof outlasts a module call, so each change runs as a job in the
//! background: its output goes to a log under the jobs directory, and `state`
//! reads how it is going. At most one job runs per name.

use crate::changes::check_text;
use crate::registry;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// The file a wallet directory holds once the wallet has been made.
pub const STORAGE_FILE: &str = "storage.json";
/// `names mine` is answered while the caller waits, so it gives up early.
const MINE_PATIENCE: Duration = Duration::from_secs(15);
/// A feed reads a few thousand recent blocks.
const FEED_PATIENCE: Duration = Duration::from_secs(120);
/// `names balance` first catches the wallet up with the chain.
const BALANCE_PATIENCE: Duration = Duration::from_secs(90);
/// `names bridge` makes no proof: it asks the sequencer and the person's own
/// node. Generous all the same, since a send stopped midway may still go through.
const BRIDGE_PATIENCE: Duration = Duration::from_secs(120);
/// Basecamp's Logos Blockchain node.
pub const DEFAULT_BRIDGE_NODE: &str = "http://127.0.0.1:8080";
/// Most notes one deposit spends (the CLI's MAX_DEPOSIT_NOTES).
const MAX_DEPOSIT_NOTES: u64 = 30;
/// A change that takes longer than this is stopped.
const JOB_PATIENCE: Duration = Duration::from_secs(45 * 60);
/// How much of a job's log `state` returns.
const TAIL_BYTES: u64 = 4096;
const TAIL_LINES: usize = 20;
/// Finished jobs are forgotten after a week.
const KEEP_FINISHED_MS: u64 = 7 * 24 * 60 * 60 * 1000;

/// Jobs whose process this module started and is still waiting on.
static WATCHED: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| u64::try_from(since.as_millis()).unwrap_or(u64::MAX))
}

pub fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

/// The CLI to run: the configured path, else `~/.local/bin/names` (where the
/// installer puts it), else the copy `cli_install` put in the module's own
/// folder, else the first `names` on `PATH`.
pub fn find_program(configured: Option<&str>) -> Option<PathBuf> {
    if let Some(path) = configured {
        return Some(PathBuf::from(path));
    }
    let installed = std::env::var_os("HOME").map(|home| Path::new(&home).join(".local/bin/names"));
    if let Some(path) = installed.filter(|path| is_executable(path)) {
        return Some(path);
    }
    if let Some(path) = crate::install_dir().map(|dir| dir.join("names")).filter(|path| is_executable(path)) {
        return Some(path);
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|dir| dir.join("names")).find(|path| is_executable(path))
}

/// The CLI's wallet directory: the configured one, else what the CLI itself
/// would use (`LEE_WALLET_HOME_DIR`, default `~/.lee/wallet`).
pub fn find_wallet(configured: Option<&str>) -> Option<PathBuf> {
    if let Some(dir) = configured {
        return Some(PathBuf::from(dir));
    }
    if let Some(dir) = std::env::var_os("LEE_WALLET_HOME_DIR").filter(|dir| !dir.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    std::env::var_os("HOME").map(|home| Path::new(&home).join(".lee/wallet"))
}

/// Where the CLI is, which wallet it signs with, and which registry it uses:
/// the module's own, so the app never shows one registry and edits another.
#[derive(Clone)]
pub struct Setup {
    pub program: PathBuf,
    pub wallet: PathBuf,
    pub registry: Option<String>,
    pub sequencer: String,
}

impl Setup {
    /// The whole argv after the program: the registry and sequencer, then `command`.
    pub fn argv(&self, command: Vec<String>) -> Vec<String> {
        let mut argv = Vec::with_capacity(command.len() + 2);
        if let Some(registry) = &self.registry {
            argv.push(format!("--program={registry}"));
        }
        argv.push(format!("--rpc={}", self.sequencer));
        argv.extend(command);
        argv
    }

    fn command(&self, command: Vec<String>) -> Command {
        let mut process = Command::new(&self.program);
        process
            .args(self.argv(command))
            .env("LEE_WALLET_HOME_DIR", &self.wallet)
            .stdin(Stdio::null());
        process
    }

    /// Fails unless the wallet directory holds a wallet. The CLI would make a
    /// new one otherwise, printing its recovery phrase where nobody reads it.
    pub fn wallet_ready(&self) -> Result<(), String> {
        if self.wallet.join(STORAGE_FILE).is_file() {
            Ok(())
        } else {
            Err(format!(
                "there is no wallet in {} yet; create your wallet first (Welcome, Create my wallet), or choose the CLI's wallet folder in App settings",
                self.wallet.display()
            ))
        }
    }
}

// ── The commands ────────────────────────────────────────────────────────

/// `names register`: from the CLI's private account, so the holder never
/// appears on chain. It returns once the commitment is made; the CLI reveals
/// it in the background (`names mine` says when).
pub fn register_args(name: &str, public: bool) -> Result<Vec<String>, String> {
    let mut args = vec!["register".to_owned()];
    if public {
        args.push("--public".to_owned());
    }
    args.extend(["--".to_owned(), registry::parse_name(name)?]);
    Ok(args)
}

/// A board post reference: its ID (the address of its account) or
/// `<name>/<n>`, checked like anything typed.
pub fn post_ref(post: &str) -> Result<String, String> {
    let post = post.trim();
    let Some((name, n)) = post.rsplit_once('/') else {
        return registry::parse_account(post)
            .map(|account| registry::account_text(&account))
            .map_err(|_| format!("not a post: {post}"));
    };
    let name = registry::parse_name(name)?;
    let n: u64 = n.parse().map_err(|_| format!("not a post number: {n}"))?;
    Ok(format!("{name}/{n}"))
}

/// `names post`.
pub fn post_args(name: &str, text: &str, cids: &[String], reply: &str) -> Result<Vec<String>, String> {
    let name = registry::parse_name(name)?;
    if text.len() > 1024 {
        return Err("a post holds at most 1,024 bytes of text".to_owned());
    }
    if text.trim().is_empty() && cids.is_empty() {
        return Err("a post needs text or a file".to_owned());
    }
    if cids.len() > 4 || cids.iter().any(|cid| cid.is_empty() || cid.len() > 100 || !cid.bytes().all(|b| b.is_ascii_alphanumeric())) {
        return Err("up to four CIDs, each letters and digits".to_owned());
    }
    let mut args = vec!["post".to_owned(), format!("--as={name}")];
    args.extend(cids.iter().map(|cid| format!("--cid={cid}")));
    if !reply.trim().is_empty() {
        args.push(format!("--reply={}", post_ref(reply)?));
    }
    args.extend(["--".to_owned(), text.to_owned()]);
    Ok(args)
}

/// `names like`.
pub fn like_args(name: &str, post: &str, undo: bool) -> Result<Vec<String>, String> {
    let mut args = vec!["like".to_owned(), format!("--as={}", registry::parse_name(name)?)];
    if undo {
        args.push("--undo".to_owned());
    }
    args.extend(["--".to_owned(), post_ref(post)?]);
    Ok(args)
}

/// `names pay`, privately from the CLI's private account.
pub fn pay_args(name: &str, amount: &str) -> Result<Vec<String>, String> {
    let name = registry::parse_name(name)?;
    let amount = amount.trim().replace(',', "");
    if !is_amount(&amount) {
        return Err("enter a whole amount, such as 1000".to_owned());
    }
    Ok(vec!["pay".to_owned(), "--".to_owned(), name, amount])
}

/// A whole amount above zero, in decimal digits and nothing else.
pub fn is_amount(amount: &str) -> bool {
    !amount.is_empty() && amount.len() <= 30 && amount.bytes().all(|b| b.is_ascii_digit()) && !amount.bytes().all(|b| b == b'0')
}

// ── Bridging from the Logos Blockchain ──────────────────────────────────

/// A Logos Blockchain public key: exactly 64 hex characters, in lower case.
/// It is not a secret; the node's wallet lists its notes under it.
pub fn bridge_key(key: &str) -> Result<String, String> {
    let key = key.trim().to_ascii_lowercase();
    if key.len() != 64 || !key.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("a Logos Blockchain public key is 64 characters, 0-9 and a-f".to_owned());
    }
    Ok(key)
}

/// The person's own Logos Blockchain node: `http(s)://127.0.0.1:<port>` or
/// `http(s)://localhost:<port>`, with nothing else, so the module can't be
/// pointed at another machine. Empty means Basecamp's node.
pub fn bridge_node(node: &str) -> Result<String, String> {
    let node = node.trim();
    if node.is_empty() {
        return Ok(DEFAULT_BRIDGE_NODE.to_owned());
    }
    let refuse = || Err(format!("the node is an address on this computer, such as {DEFAULT_BRIDGE_NODE}"));
    let bare = node.strip_suffix('/').unwrap_or(node);
    let Some((scheme, rest)) = bare.split_once("://") else { return refuse() };
    if scheme != "http" && scheme != "https" {
        return refuse();
    }
    let Some((host, port)) = rest.split_once(':') else { return refuse() };
    if host != "127.0.0.1" && host != "localhost" {
        return refuse();
    }
    if port.is_empty() || port.len() > 5 || !port.bytes().all(|b| b.is_ascii_digit()) {
        return refuse();
    }
    match port.parse::<u32>() {
        Ok(1..=65535) => Ok(format!("{scheme}://{host}:{port}")),
        _ => refuse(),
    }
}

/// `names bridge`: deposit `amount` from the notes of `from_key` into LEZ,
/// credited to the public account `to`. Without `send` the CLI only shows the
/// plan; with it, `--yes`, and the deposit is made and cannot be undone.
pub fn bridge_args(amount: &str, to: &str, from_key: &str, node: &str, send: bool) -> Result<Vec<String>, String> {
    let amount = bridge_amount(amount)?;
    let to = registry::account_text(&registry::parse_account(to)?);
    let mut args = vec![
        "bridge".to_owned(),
        format!("--to=Public/{to}"),
        format!("--from-key={}", bridge_key(from_key)?),
        format!("--node={}", bridge_node(node)?),
    ];
    if send {
        args.push("--yes".to_owned());
    }
    args.extend(["--".to_owned(), amount]);
    Ok(args)
}

/// An amount to bridge: decimal digits only, above zero.
pub fn bridge_amount(amount: &str) -> Result<String, String> {
    let amount = amount.trim();
    if !is_amount(amount) {
        return Err("enter a whole amount, such as 1000".to_owned());
    }
    Ok(amount.to_owned())
}

/// The CLI wallet's default public account, as `names setup` saved it in the
/// wallet's `names.json` (what `names balance` marks `(default)`).
pub fn default_public(setup: &Setup) -> Result<String, String> {
    saved_account(&setup.wallet, "public")
        .ok_or_else(|| "your wallet has no public account yet; create your wallet first (Welcome, Create my wallet)".to_owned())
}

/// What `names bridge` printed: `{sent, lines, hash, total, notes, fee, to}`.
/// `total`, `notes` and `fee` come from its plan line (`deposit <total> from
/// <notes> note(s) of <key>` and `..., fee at most <fee>`), `hash` from
/// `submitted deposit <hash>` (null unless it is hex).
pub fn parse_bridge(output: &str, sent: bool) -> Value {
    let lines: Vec<&str> = output.lines().map(str::trim).filter(|line| !line.is_empty()).collect();
    let digits = |word: Option<&str>| word.filter(|w| !w.is_empty() && w.bytes().all(|b| b.is_ascii_digit())).map(str::to_owned);
    let (mut total, mut notes, mut fee, mut hash, mut to) = (None, None, None, None, None);
    for line in &lines {
        if let Some(rest) = line.strip_prefix("deposit ") {
            let mut words = rest.split_whitespace();
            total = digits(words.next());
            notes = digits(words.nth(1));
        }
        if let Some((_, rest)) = line.split_once("fee at most ") {
            fee = digits(rest.split_whitespace().next());
        }
        if let Some((_, rest)) = line.split_once("credited to Public/") {
            to = rest.split_whitespace().next().and_then(|id| registry::parse_account(id).ok()).map(|id| registry::account_text(&id));
        }
        if let Some(rest) = line.strip_prefix("submitted deposit ") {
            let rest = rest.trim();
            let rest = rest.strip_prefix("0x").unwrap_or(rest);
            if !rest.is_empty() && rest.len() <= 128 && rest.bytes().all(|b| b.is_ascii_hexdigit()) {
                hash = Some(rest.to_ascii_lowercase());
            }
        }
    }
    json!({ "sent": sent, "lines": lines, "hash": hash, "total": total, "notes": notes, "fee": fee, "to": to })
}

/// Why `names bridge` failed, in plain words, from what it wrote.
pub fn bridge_problem(output: &str, node: &str) -> String {
    let text = output;
    if text.contains("cannot reach your Logos Blockchain node") {
        // The node answered, but not with the key's notes (ureq: `http status: 404`).
        let status = text.split_once("http status: ").and_then(|(_, rest)| rest.split_whitespace().next());
        return match status {
            Some("404") => "Your node's wallet doesn't have that key. Use a public key your node's wallet lists.".to_owned(),
            Some("400") => "Your node says that isn't a valid key. Copy the public key your node's wallet lists.".to_owned(),
            Some(code) => format!("Your Logos Blockchain node at {node} answered with an error (HTTP {code})."),
            None => format!("Your Logos Blockchain node at {node} isn't answering. Check that it's running, or check the node address."),
        };
    }
    if text.contains("cannot reach the sequencer") {
        return "LEZ isn't answering right now. Try again in a minute.".to_owned();
    }
    if text.contains("lists no notes") {
        return "Your node has nothing under that key. Check the key: it's the public key your node's wallet shows.".to_owned();
    }
    if let Some(line) = text.lines().find(|line| line.contains("bridge in smaller steps")) {
        let words: Vec<&str> = line.split_whitespace().collect();
        let notes = words.iter().position(|w| *w == "note(s)").and_then(|i| i.checked_sub(1)).and_then(|i| words[i].trim_start_matches("Error:").parse::<u64>().ok());
        let held = words.iter().position(|w| *w == "hold").and_then(|i| words.get(i + 1)).map(|w| w.trim_end_matches(','));
        if notes == Some(MAX_DEPOSIT_NOTES) {
            return format!("That takes more than {MAX_DEPOSIT_NOTES} notes, the most one deposit can use. Bridge in smaller steps.");
        }
        return match held {
            Some(held) if held.bytes().all(|b| b.is_ascii_digit()) => format!("That key holds only {held}. Bridge {held} or less."),
            _ => "That key doesn't hold that much.".to_owned(),
        };
    }
    if let Some(line) = text.lines().find(|line| line.contains("the node refused the deposit")) {
        return format!("Your node refused the deposit: {}", line.split_once("refused the deposit").map_or("", |(_, why)| why).trim_start_matches([':', ' ']));
    }
    if text.contains("did not return a channel ID") {
        return "LEZ didn't say where deposits go. Try again later.".to_owned();
    }
    // The CLI's own first error line says it best.
    let first = text.lines().map(str::trim).find(|line| line.starts_with("Error:")).or_else(|| text.lines().map(str::trim).rfind(|line| !line.is_empty()));
    format!("the names CLI failed: {}", first.unwrap_or("no reason given").trim_start_matches("Error:").trim())
}

/// Runs `names bridge` (only the plan unless `send`) to the wallet's default
/// public account and returns what `parse_bridge` makes of it, or why it failed.
pub fn run_bridge(setup: &Setup, amount: &str, from_key: &str, node: &str, send: bool) -> Result<Value, String> {
    setup.wallet_ready()?;
    // Checked before the wallet is read, so bad input fails the same way every time.
    let node = bridge_node(node)?;
    bridge_amount(amount)?;
    bridge_key(from_key)?;
    let to = default_public(setup)?;
    let args = bridge_args(amount, &to, from_key, &node, send)?;
    let (code, out, err) = run_capture(setup, args, BRIDGE_PATIENCE)?;
    if code != Some(0) {
        return Err(bridge_problem(&format!("{err}\n{out}"), &node));
    }
    Ok(parse_bridge(&out, send))
}

// ── The wallet: making it, and moving tokens to its private account ─────

/// The job name for work on the wallet itself (moving tokens to private,
/// paying an account). A name never starts with `-`, so it is nobody's name.
pub const WALLET_JOB: &str = "-wallet";
/// `names setup --shield=0` makes the wallet and its two accounts and moves
/// nothing, so it never proves. It may first catch up with the chain.
const SETUP_PATIENCE: Duration = Duration::from_secs(180);
/// What `names setup` moves to the private account unless told otherwise
/// (its own default): a few names' deposits.
pub const DEFAULT_SHIELD: &str = "5000000";

/// `names setup --shield=<amount>`: the wallet and its accounts, and, with an
/// amount above zero and that much in the public account, that much moved to
/// the private account. Empty is the CLI's default, 5,000,000.
pub fn setup_args(shield: &str) -> Result<Vec<String>, String> {
    let shield = shield.trim().replace(',', "");
    let shield = if shield.is_empty() { DEFAULT_SHIELD.to_owned() } else { shield };
    if shield != "0" && !is_amount(&shield) {
        return Err("enter a whole amount, such as 5000000".to_owned());
    }
    Ok(vec!["setup".to_owned(), format!("--shield={shield}")])
}

/// What `names setup` printed: `{public, private, phrase}`. `phrase` is the
/// new wallet's recovery phrase, or null when the wallet already existed (the
/// CLI prints it only when it makes one). Every part is checked: an account
/// must parse, and a phrase is 12 to 24 words of a-z.
pub fn parse_setup(output: &str) -> Value {
    let account = |prefix: &str, kind: &str| {
        output.lines().map(str::trim).find_map(|line| {
            let rest = line.strip_prefix(prefix)?.trim_start();
            let id = rest.strip_prefix(kind)?.split_whitespace().next()?;
            registry::parse_account(id).ok().map(|id| registry::account_text(&id))
        })
    };
    let mut lines = output.lines().map(str::trim);
    let phrase = lines
        .by_ref()
        .find(|line| line.contains("the only way to get the wallet back"))
        .and_then(|_| lines.find(|line| !line.is_empty()))
        .filter(|line| {
            let words: Vec<&str> = line.split(' ').collect();
            (12..=24).contains(&words.len()) && words.iter().all(|word| !word.is_empty() && word.bytes().all(|b| b.is_ascii_lowercase()))
        })
        .map(str::to_owned);
    json!({
        "public": account("public account", "Public/"),
        "private": account("private account", "Private/"),
        "phrase": phrase,
    })
}

/// Makes the CLI's wallet, if there is none, and its public and private
/// accounts, moving nothing (`names setup --shield=0`). Waits for it.
/// Returns `parse_setup`'s `{public, private, phrase}`.
///
/// The output is held only in memory and never written to a log: a new
/// wallet's recovery phrase is in it. If the CLI fails after making the
/// wallet, the phrase is still returned (with `problem` saying what went
/// wrong), since it can't be shown again.
pub fn run_create_wallet(setup: &Setup) -> Result<Value, String> {
    let (code, out, err) = run_capture(setup, setup_args("0")?, SETUP_PATIENCE)?;
    let mut made = parse_setup(&out);
    // Only stderr explains a failure: stdout may end with the phrase.
    let why = || {
        let first = err.lines().map(str::trim).find(|line| line.starts_with("Error:")).unwrap_or_else(|| last_line(&err));
        let first = first.trim_start_matches("Error:").trim();
        if first.is_empty() { "the names CLI stopped without saying why".to_owned() } else { format!("the names CLI failed: {first}") }
    };
    if code != Some(0) {
        if made["phrase"].is_string() {
            made["problem"] = Value::from(why());
            return Ok(made);
        }
        return Err(why());
    }
    if made["public"].is_null() || made["private"].is_null() {
        return Err("the names CLI made the wallet but didn't say its accounts; open the wallet again in a moment".to_owned());
    }
    Ok(made)
}

/// How a `names setup` job that moves tokens to private ended, from its log:
/// `{moved, problem}`. The CLI exits 0 without moving anything when the
/// public account holds too little, and says so.
pub fn shield_outcome(log: &str, done: bool) -> Value {
    if let Some(line) = log.lines().find(|line| line.contains("send it at least")) {
        let word_after = |key: &str| {
            line.split_once(key).and_then(|(_, rest)| rest.split(|c: char| !c.is_ascii_digit()).find(|w| !w.is_empty())).map(str::to_owned)
        };
        let (holds, needs) = (word_after(" holds "), word_after("at least "));
        let problem = match (holds, needs) {
            (Some(holds), Some(needs)) => format!(
                "Your public account holds {}, and moving to private needs at least {}. Get more tokens, then try again.",
                group(&holds),
                group(&needs)
            ),
            _ => "Your public account doesn't hold enough yet. Get more tokens, then try again.".to_owned(),
        };
        return json!({ "moved": false, "problem": problem });
    }
    if done {
        return json!({ "moved": log.contains("moving "), "problem": null });
    }
    let first = log.lines().map(str::trim).find(|line| line.starts_with("Error:")).unwrap_or_else(|| last_line(log));
    json!({ "moved": false, "problem": format!("Moving to private didn't go through: {}", first.trim_start_matches("Error:").trim()) })
}

/// Digits grouped in threes: 5000000 is 5,000,000.
fn group(digits: &str) -> String {
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// The CLI wallet's default private account, as `names setup` saved it.
pub fn default_private(setup: &Setup) -> Result<String, String> {
    saved_account(&setup.wallet, "private").ok_or_else(|| "your wallet has no private account yet; create your wallet first (Welcome, Create my wallet)".to_owned())
}

/// An account `names setup` saved in the wallet's `names.json`, checked.
pub fn saved_account(wallet: &Path, which: &str) -> Option<String> {
    let saved: Value = std::fs::read_to_string(wallet.join("names.json"))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(Value::Null);
    saved[which]
        .as_str()
        .and_then(|account| registry::parse_account(account).ok())
        .map(|account| registry::account_text(&account))
}

// ── Paying an account ───────────────────────────────────────────────────

/// Who a payment goes to: a name, or a public account written
/// `Public/<account id>`. A bare ID is refused, so an ID is never taken for a
/// name or the other way round, and so is a private account (it can't be paid
/// by its ID; its name can be).
#[derive(Debug, PartialEq, Eq)]
pub enum PayTo {
    Name(String),
    Account(String),
}

pub fn pay_to(to: &str) -> Result<PayTo, String> {
    let to = to.trim();
    if let Some(id) = to.strip_prefix("Public/") {
        return registry::parse_account(id)
            .ok()
            .filter(|_| !id.contains('/') && !id.contains(char::is_whitespace))
            .map(|id| PayTo::Account(registry::account_text(&id)))
            .ok_or_else(|| "that isn't a public account; it's Public/ and then the account ID, letters and digits".to_owned());
    }
    if to.starts_with("Private/") {
        return Err("a private account can't be paid by its ID; pay its name instead".to_owned());
    }
    if registry::parse_account(to).is_ok() {
        return Err("to pay an account, write it as Public/ and then its ID".to_owned());
    }
    registry::parse_name(to).map(PayTo::Name)
}

/// `names wallet auth-transfer send` to the public account `to` (written
/// `Public/<id>`) from the wallet's default public account, or with
/// `from_private` its private one. The chain shows the account paid and the
/// amount; from the public account it shows the payer too.
pub fn pay_account_args(setup: &Setup, to: &str, amount: &str, from_private: bool) -> Result<Vec<String>, String> {
    let PayTo::Account(to) = pay_to(to)? else {
        return Err("that is a name; pay it as a name".to_owned());
    };
    let amount = amount.trim().replace(',', "");
    if !is_amount(&amount) {
        return Err("enter a whole amount, such as 1000".to_owned());
    }
    let from = if from_private {
        format!("Private/{}", default_private(setup)?)
    } else {
        let public = default_public(setup)?;
        if public == to {
            return Err("that's your own public account".to_owned());
        }
        format!("Public/{public}")
    };
    Ok(vec![
        "wallet".to_owned(),
        "auth-transfer".to_owned(),
        "send".to_owned(),
        format!("--from={from}"),
        format!("--to=Public/{to}"),
        format!("--amount={amount}"),
    ])
}

/// `names hide`, and the name whose job it is (`board` for a post given by ID).
pub fn hide_args(post: &str) -> Result<(String, Vec<String>), String> {
    let post = post_ref(post)?;
    let name = post.rsplit_once('/').map_or_else(|| "board".to_owned(), |(name, _)| name.to_owned());
    Ok((name, vec!["hide".to_owned(), "--".to_owned(), post]))
}

/// Runs `names feed --json`, for the names in `following` or, if none, everyone.
pub fn run_feed(setup: &Setup, following: &[String]) -> Result<Value, String> {
    setup.wallet_ready()?;
    let mut args = vec!["feed".to_owned(), "--json".to_owned()];
    if !following.is_empty() {
        args.push(format!("--following={}", following.join(",")));
    }
    serde_json::from_str(last_line(&run_text(setup, args, FEED_PATIENCE)?)).map_err(|_| "the names CLI's feed did not decode".to_owned())
}

/// Runs `names posts-of --json`: `{total, posts}`, the name's posts newest first
/// below post number `before` (its newest when `before` is negative).
pub fn run_posts_of(setup: &Setup, name: &str, before: i64) -> Result<Value, String> {
    setup.wallet_ready()?;
    let mut args = vec!["posts-of".to_owned(), "--json".to_owned(), "--count=10".to_owned()];
    if before >= 0 {
        args.push(format!("--before={before}"));
    }
    args.extend(["--".to_owned(), registry::parse_name(name)?]);
    serde_json::from_str(last_line(&run_text(setup, args, FEED_PATIENCE)?)).map_err(|_| "the names CLI's posts did not decode".to_owned())
}

/// Runs `names thread --json` for `post`.
pub fn run_thread(setup: &Setup, post: &str) -> Result<Value, String> {
    setup.wallet_ready()?;
    let args = vec!["thread".to_owned(), "--json".to_owned(), "--".to_owned(), post_ref(post)?];
    serde_json::from_str(last_line(&run_text(setup, args, FEED_PATIENCE)?)).map_err(|_| "the names CLI's thread did not decode".to_owned())
}

pub fn mine_args() -> Vec<String> {
    vec!["mine".to_owned()]
}

/// `names set-text`: an empty value removes the entry.
pub fn set_text_args(name: &str, key: &str, value: &str) -> Result<Vec<String>, String> {
    let name = registry::parse_name(name)?;
    let key = key.trim();
    check_text(key, value)?;
    Ok(vec!["set-text".to_owned(), "--".to_owned(), name, key.to_owned(), value.to_owned()])
}

/// `names set-page` with the body in `file`, or `names clear-page` without one.
pub fn set_page_args(name: &str, content_type: &str, file: Option<&Path>) -> Result<Vec<String>, String> {
    let name = registry::parse_name(name)?;
    let Some(file) = file else {
        return Ok(vec!["clear-page".to_owned(), "--".to_owned(), name]);
    };
    let content_type = content_type.trim();
    if content_type.chars().any(char::is_control) || content_type.len() > 128 {
        return Err("a content type is one line of at most 128 characters".to_owned());
    }
    let mut args = vec!["set-page".to_owned()];
    if !content_type.is_empty() {
        args.push(format!("--content-type={content_type}"));
    }
    args.extend(["--".to_owned(), name, file.to_string_lossy().into_owned()]);
    Ok(args)
}

/// What `names profile` is given for a set of entries, and the entries left out.
#[derive(Debug, PartialEq, Eq)]
pub struct ProfileArgs {
    pub args: Vec<String>,
    pub skipped: Vec<String>,
}

/// Whether `value` is the absolute path of a PNG file on this machine.
fn is_png_file(value: &str) -> bool {
    let path = Path::new(value);
    if !path.is_absolute() || !path.is_file() {
        return false;
    }
    let mut start = [0_u8; 8];
    std::fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut start))
        .is_ok_and(|()| start == *b"\x89PNG\r\n\x1a\n")
}

/// `names profile` for `entries` (key to value; an empty value removes the
/// key). The card keys go to their own options, any other key to `--set`. The
/// CLI reads an avatar from a file, so `avatar` is passed only when its value
/// is the path of a PNG file here; with `replace`, a `data:image/` URL is kept
/// as it is. Any other avatar value is left out and listed in `skipped`.
pub fn profile_args(name: &str, entries: &[(String, String)], replace: bool) -> Result<ProfileArgs, String> {
    let name = registry::parse_name(name)?;
    let mut args = vec!["profile".to_owned()];
    let mut skipped = Vec::new();
    let mut changes = 0;
    for (key, value) in entries {
        let key = key.as_str();
        check_text(key, value)?;
        if value.is_empty() {
            args.push(format!("--clear={key}"));
            changes += 1;
            continue;
        }
        let one_of = |allowed: &[&str]| {
            if allowed.contains(&value.as_str()) {
                Ok(format!("--{key}={value}"))
            } else {
                Err(format!("{key} is one of {}", allowed.join(", ")))
            }
        };
        let arg = match key {
            "avatar" if is_png_file(value) => format!("--avatar={value}"),
            "avatar" if replace && value.starts_with("data:image/") => format!("--set=avatar={value}"),
            "avatar" => {
                skipped.push(key.to_owned());
                continue;
            }
            "display" | "bio" | "cid" => format!("--{key}={value}"),
            "theme" => one_of(&["light", "dark", "auto"])?,
            "font" => one_of(&["sans", "serif", "mono", "rounded"])?,
            "color" => {
                let ok = value.len() == 7 && value.starts_with('#') && value[1..].chars().all(|c| c.is_ascii_hexdigit());
                if !ok {
                    return Err("color is #rrggbb".to_owned());
                }
                format!("--color={value}")
            }
            _ => format!("--set={key}={value}"),
        };
        args.push(arg);
        changes += 1;
    }
    if replace {
        args.push("--replace".to_owned());
    } else if changes == 0 {
        return Err(if skipped.is_empty() {
            "nothing to change".to_owned()
        } else {
            "the names CLI takes an avatar as a PNG file: give the file's full path".to_owned()
        });
    }
    args.extend(["--".to_owned(), name]);
    Ok(ProfileArgs { args, skipped })
}

/// The names `names mine` lists as held by its wallet. Its output is text:
/// `jef.lez  held by a deposit (no expiry)`, `jef.lez  held until ...` or
/// `jef.lez  lapsed until ...`. A registration under way (`jef.lez  registering, ...`)
/// or stopped (`jef.lez  NOT registered: ...`) is listed with `registering` or
/// `failed` true; every other line (a name no longer held, `no names yet`) is skipped.
pub fn parse_mine(output: &str) -> Vec<Value> {
    let mut seen = BTreeSet::new();
    let mut names = Vec::new();
    for line in output.lines() {
        let Some((shown, state)) = line.trim().split_once("  ") else { continue };
        // Older CLIs write `jef.lez`, newer ones `jef`.
        let bare = shown.strip_suffix(registry::LEGACY_SUFFIX).unwrap_or(shown);
        // The CLI's output is checked like anything typed.
        let Ok(name) = registry::parse_name(bare) else { continue };
        if name != bare {
            continue;
        }
        let state = state.trim();
        let (lapsed, registering, failed) = if state.starts_with("held ") {
            (false, false, false)
        } else if state.starts_with("lapsed ") {
            (true, false, false)
        } else if state.starts_with("registering") {
            (false, true, false)
        } else if state.starts_with("NOT registered") {
            (false, false, true)
        } else {
            continue;
        };
        // The log path the CLI adds is of no use to an app.
        let state = state.split(" (log: ").next().unwrap_or(state);
        // Newer CLIs say what holds the name; older ones don't (null).
        let private = if state.ends_with("; held privately") {
            json!(true)
        } else if state.ends_with("; held by a public account") {
            json!(false)
        } else {
            Value::Null
        };
        let state = state.split("; held ").next().unwrap_or(state);
        if seen.insert(name.clone()) {
            names.push(json!({
                "name": name,
                "display": format!("{name}{}", registry::NAME_SUFFIX),
                "signer": "cli",
                "state": state,
                "lapsed": lapsed,
                "registering": registering,
                "failed": failed,
                "private": private,
            }));
        }
    }
    names
}

/// The last line of `text` that says something.
pub fn last_line(text: &str) -> &str {
    text.lines().rev().map(str::trim).find(|line| !line.is_empty()).unwrap_or("")
}

/// Waits for `child`, stopping it after `patience`. The exit code, or why there is none.
fn wait(child: &mut Child, patience: Duration) -> Result<Option<i32>, String> {
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status.code()),
            Ok(None) if started.elapsed() >= patience => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("the names CLI took more than {} s and was stopped", patience.as_secs()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(200)),
            Err(error) => return Err(format!("lost track of the names CLI: {error}")),
        }
    }
}

/// Runs `names mine` and returns the names it holds. A wallet directory with
/// no wallet in it holds none, and is left as it is.
pub fn run_mine(setup: &Setup) -> Result<Vec<Value>, String> {
    if setup.wallet_ready().is_err() {
        return Ok(Vec::new());
    }
    Ok(parse_mine(&run_text(setup, mine_args(), MINE_PATIENCE)?))
}

/// Runs `names balance` and returns the default private account and its
/// balance: `{account, balance, public}` (decimal text), or null if there is
/// none. `public` is the default public account the same way, or null.
pub fn run_balance(setup: &Setup) -> Result<Value, String> {
    setup.wallet_ready()?;
    let output = run_text(setup, vec!["balance".to_owned()], BALANCE_PATIENCE)?;
    let mut balance = parse_balance(&output);
    if balance.is_object() {
        balance["public"] = parse_public_balance(&output);
    }
    Ok(balance)
}

/// The default private account in `names balance` output:
/// `  Private/<id>  <balance>  (default)`.
pub fn parse_balance(output: &str) -> Value {
    parse_default(output, "Private/")
}

/// The default public account in `names balance` output:
/// `  Public/<id>  <balance>  (default)`.
pub fn parse_public_balance(output: &str) -> Value {
    parse_default(output, "Public/")
}

fn parse_default(output: &str, kind: &str) -> Value {
    for line in output.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix(kind) else { continue };
        if !line.ends_with("(default)") {
            continue;
        }
        let mut parts = rest.split_whitespace();
        let (Some(account), Some(balance)) = (parts.next(), parts.next()) else { continue };
        if registry::parse_account(account).is_err() || !balance.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        return json!({ "account": account, "balance": balance });
    }
    Value::Null
}

/// Runs the CLI with `command` and returns what it printed, or why it failed.
fn run_text(setup: &Setup, command: Vec<String>, patience: Duration) -> Result<String, String> {
    let (code, out, err) = run_capture(setup, command, patience)?;
    if code != Some(0) {
        let why = if last_line(&err).is_empty() { last_line(&out) } else { last_line(&err) };
        return Err(format!("the names CLI failed: {why}"));
    }
    Ok(out)
}

/// Runs the CLI with `command`: its exit code, what it printed, and what it
/// wrote to stderr.
fn run_capture(setup: &Setup, command: Vec<String>, patience: Duration) -> Result<(Option<i32>, String, String), String> {
    capture(setup.command(command), &setup.program, patience)
}

/// Runs `process` (the program at `program`) and waits for it, at most
/// `patience`: its exit code, what it printed, and what it wrote to stderr.
fn capture(mut process: Command, program: &Path, patience: Duration) -> Result<(Option<i32>, String, String), String> {
    let mut child = process
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not run the names CLI at {}: {error}", program.display()))?;
    let drain = |stream: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut text = String::new();
            if let Some(mut stream) = stream {
                let _ = stream.read_to_string(&mut text);
            }
            text
        })
    };
    let out = drain(child.stdout.take().map(|s| Box::new(s) as Box<dyn Read + Send>));
    let err = drain(child.stderr.take().map(|s| Box::new(s) as Box<dyn Read + Send>));
    let code = wait(&mut child, patience)?;
    let (out, err) = (out.join().unwrap_or_default(), err.join().unwrap_or_default());
    Ok((code, out, err))
}

// ── Installing the CLI ──────────────────────────────────────────────────

/// The official installer, `gateway/public/install.sh`, as it was when this
/// module was built (`rust-lib/install.sh` is a copy; a test checks they
/// match). Running the copy built in, rather than one downloaded now, means a
/// swapped download can't change what runs or the release key it checks
/// against: the installer verifies each file's ed25519 signature with
/// `ssh-keygen -Y verify` against the key embedded in it, and its SHA-256.
pub const INSTALLER: &str = include_str!("../install.sh");
/// The job name for installing. A name never starts with `-`.
pub const INSTALL_JOB: &str = "-install";
/// `names --version` answers at once.
const VERSION_PATIENCE: Duration = Duration::from_secs(20);
/// Settings a person's network may need, passed on to the installer's curl.
const PROXY_VARS: [&str; 8] = ["http_proxy", "https_proxy", "all_proxy", "no_proxy", "HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "NO_PROXY"];

/// `<program> --version`, if it runs and says it is the names CLI: `names 0.5.0`.
pub fn version(program: &Path) -> Result<String, String> {
    if !is_executable(program) {
        return Err(format!("there is no program at {}", program.display()));
    }
    let mut process = Command::new(program);
    process.arg("--version").stdin(Stdio::null());
    let (code, out, err) = capture(process, program, VERSION_PATIENCE)?;
    let line = last_line(&out);
    if code == Some(0) && line.starts_with("names ") {
        return Ok(line.to_owned());
    }
    let said = if last_line(&err).is_empty() { line } else { last_line(&err) };
    Err(format!("{} does not start: {said}", program.display()))
}

/// Whether RISC Zero's prover is where the CLI looks for it: next to it, or
/// wherever `RISC0_SERVER_PATH` says.
pub fn has_prover(program: &Path) -> bool {
    if std::env::var_os("RISC0_SERVER_PATH").is_some_and(|path| is_executable(Path::new(&path))) {
        return true;
    }
    let program = program.canonicalize().unwrap_or_else(|_| program.to_path_buf());
    program.parent().is_some_and(|dir| is_executable(&dir.join("r0vm")))
}

/// The installer's process: `/bin/sh <script>`, putting the CLI and the
/// prover in `bin` and the man pages in `man`. Nothing typed reaches it: its
/// environment is cleared and given only these folders, `HOME`, `TMPDIR` and
/// any proxy settings, so a stray `NAMES_DOWNLOAD_BASE` can't redirect it. `bin`
/// is put first on its `PATH`, which also keeps the installer from adding a
/// line to the person's shell startup file (it only does that when its folder
/// isn't on `PATH`).
pub fn install_process(script: &Path, bin: &Path, man: &Path) -> Command {
    let mut process = Command::new("/bin/sh");
    process
        .arg(script)
        .env_clear()
        .env("PATH", format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", bin.display()))
        .env("NAMES_INSTALL_DIR", bin)
        .env("NAMES_MAN_DIR", man)
        .stdin(Stdio::null());
    for var in ["HOME", "TMPDIR"].into_iter().chain(PROXY_VARS) {
        if let Some(value) = std::env::var_os(var) {
            process.env(var, value);
        }
    }
    process
}

/// Why an install failed, in plain words, from its log.
pub fn install_problem(log: &str) -> String {
    let has = |text: &str| log.contains(text);
    if has("no build for") {
        return "The names program doesn't run on this computer yet. It runs on Macs with Apple silicon and on Linux (x86_64).".to_owned();
    }
    if has("bad signature") || has("no signature published") {
        return "The download didn't pass the signature check, so nothing was installed. Try again later.".to_owned();
    }
    if has("checksum mismatch") {
        return "The download was damaged on the way, so nothing was installed. Try again.".to_owned();
    }
    if has("Could not resolve host") || has("Failed to connect") || has("Couldn't connect") || has("curl: (6)") || has("curl: (7)") || has("curl: (28)") || has("curl: (35)") {
        return "Couldn't reach the internet. Check your connection, then try again.".to_owned();
    }
    if has("curl: (22)") {
        return "The download isn't available right now. Try again later.".to_owned();
    }
    if has("libpcsclite") {
        return "It installed, but it needs the PC/SC library to start. On Debian or Ubuntu: sudo apt install libpcsclite1. On Fedora: sudo dnf install pcsc-lite-libs.".to_owned();
    }
    if let Some(line) = log.lines().find(|line| line.contains("does not start")) {
        return format!("It installed, but it doesn't start: {}", line.split_once("does not start:").map_or("", |(_, why)| why).trim());
    }
    if has("No space left") {
        return "There isn't enough free disk space. Free some up, then try again.".to_owned();
    }
    format!("The install stopped: {}", last_line(log))
}

/// What went wrong that the install survived, from its log: the prover
/// couldn't be fetched, so private changes can't be made yet.
pub fn install_warning(log: &str) -> Option<String> {
    log.contains("Could not install r0vm")
        .then(|| "The prover didn't download, so private changes can't be made yet. Press Install again to retry.".to_owned())
}

impl Jobs {
    /// Starts the installer `script` as a job, putting the CLI and the prover
    /// in `bin` and the man pages in `man`: `{id, name, kind, state, started}`.
    /// The script is written to the jobs folder and removed once it ends.
    pub fn start_install(&self, script: &str, bin: &Path, man: &Path) -> Result<Value, String> {
        use std::os::unix::fs::PermissionsExt;
        let id = self.prepare(INSTALL_JOB)?;
        for dir in [bin, man] {
            std::fs::create_dir_all(dir).map_err(|error| format!("cannot make {}: {error}", dir.display()))?;
        }
        let file = self.scratch(&id, "sh");
        std::fs::write(&file, script).map_err(|error| format!("cannot write the installer: {error}"))?;
        let _ = std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600));
        self.start_process(&id, INSTALL_JOB, "install", install_process(&file, bin, man), "the installer", Some(file))
    }
}

// ── Jobs ────────────────────────────────────────────────────────────────

/// A job's id: the name, then when it started. Only these characters reach a path.
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 96 && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn alive(pid: u64) -> bool {
    Command::new("/bin/kill")
        .args(["-0", &pid.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// The jobs directory: `<id>.json` (what the job is and how it ended),
/// `<id>.log` (the CLI's output) and, while it runs, the files it reads.
pub struct Jobs {
    pub dir: PathBuf,
}

impl Jobs {
    fn path(&self, id: &str, extension: &str) -> PathBuf {
        self.dir.join(format!("{id}.{extension}"))
    }

    fn record(&self, id: &str) -> Option<Value> {
        let text = std::fs::read_to_string(self.path(id, "json")).ok()?;
        serde_json::from_str(&text).ok()
    }

    fn write(&self, id: &str, record: &Value) {
        let _ = std::fs::write(self.path(id, "json"), record.to_string());
    }

    /// Checks that no job runs for `name`, clears out old jobs, and returns a
    /// fresh job id, with the directory made.
    pub fn prepare(&self, name: &str) -> Result<String, String> {
        std::fs::create_dir_all(&self.dir)
            .map_err(|error| format!("cannot make {}: {error}", self.dir.display()))?;
        if let Some(running) = self.running_for(name) {
            return Err(match name {
                INSTALL_JOB => format!("the names program is still being installed (job {running}); wait for it to finish"),
                WALLET_JOB => format!("your wallet is still busy with something else (job {running}); wait for it to finish"),
                _ => format!("a change to {name}{} is still being saved (job {running}); wait for it to finish", registry::NAME_SUFFIX),
            });
        }
        self.forget_old();
        let mut id = format!("{name}-{}", now_ms());
        while self.path(&id, "json").exists() {
            id.push('-');
            id.push('1');
        }
        Ok(id)
    }

    /// A job's file for the CLI to read, such as a page's body.
    pub fn scratch(&self, id: &str, extension: &str) -> PathBuf {
        self.path(id, extension)
    }

    fn ids(&self) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else { return Vec::new() };
        entries
            .filter_map(|entry| {
                let file = entry.ok()?.file_name().into_string().ok()?;
                file.strip_suffix(".json").filter(|id| valid_id(id)).map(str::to_owned)
            })
            .collect()
    }

    /// The running job for `name`, if there is one.
    pub fn running_for(&self, name: &str) -> Option<String> {
        self.ids().into_iter().find(|id| {
            self.record(id).is_some_and(|record| record["name"] == name)
                && self.state(id).is_ok_and(|state| state["state"] == "running")
        })
    }

    /// Every job still running: `{id, name, kind, state, started}` each.
    pub fn running(&self) -> Vec<Value> {
        self.ids()
            .into_iter()
            .filter_map(|id| self.state(&id).ok())
            .filter(|state| state["state"] == "running")
            .collect()
    }

    fn forget_old(&self) {
        let now = now_ms();
        for id in self.ids() {
            let finished = self.record(&id).and_then(|record| record["finished"].as_u64());
            if finished.is_some_and(|at| now.saturating_sub(at) > KEEP_FINISHED_MS) {
                for extension in ["json", "log", "page", "sh"] {
                    let _ = std::fs::remove_file(self.path(&id, extension));
                }
            }
        }
    }

    /// Starts `names <command>` for `name` as job `id` and returns at once:
    /// `{id, name, kind, state, started}`. `scratch` is removed once it ends.
    pub fn start(
        &self,
        setup: &Setup,
        id: &str,
        name: &str,
        kind: &str,
        command: Vec<String>,
        scratch: Option<PathBuf>,
    ) -> Result<Value, String> {
        let what = format!("the names CLI at {}", setup.program.display());
        self.start_process(id, name, kind, setup.command(command), &what, scratch)
    }

    /// Starts `process` (`what` says what it runs) as job `id` for `name`, and
    /// returns at once, as `start` does.
    fn start_process(
        &self,
        id: &str,
        name: &str,
        kind: &str,
        mut process: Command,
        what: &str,
        scratch: Option<PathBuf>,
    ) -> Result<Value, String> {
        if !valid_id(id) {
            return Err(format!("not a job id: {id}"));
        }
        let log = std::fs::File::create(self.path(id, "log"))
            .map_err(|error| format!("cannot write the job's log: {error}"))?;
        let log_err = log.try_clone().map_err(|error| format!("cannot write the job's log: {error}"))?;
        // Its own process group: a signal meant for the host leaves it be.
        process.stdout(log).stderr(log_err).process_group(0);
        let mut child = match process.spawn() {
            Ok(child) => child,
            Err(error) => {
                if let Some(scratch) = &scratch {
                    let _ = std::fs::remove_file(scratch);
                }
                return Err(format!("could not run {what}: {error}"));
            }
        };
        let started = now_ms();
        let mut record = json!({
            "id": id,
            "name": name,
            "kind": kind,
            "started": started,
            "pid": child.id(),
            "state": "running",
            "exit": null,
            "finished": null,
        });
        self.write(id, &record);
        if let Ok(mut watched) = WATCHED.lock() {
            watched.insert(id.to_owned());
        }
        let jobs = Jobs { dir: self.dir.clone() };
        let job = id.to_owned();
        std::thread::spawn(move || {
            let outcome = wait(&mut child, JOB_PATIENCE);
            record["finished"] = Value::from(now_ms());
            match outcome {
                Ok(code) => {
                    record["exit"] = code.map_or(Value::Null, Value::from);
                    record["state"] = Value::from(if code == Some(0) { "done" } else { "failed" });
                }
                Err(why) => {
                    record["state"] = Value::from("failed");
                    record["error"] = Value::from(why);
                }
            }
            jobs.write(&job, &record);
            if let Some(scratch) = scratch {
                let _ = std::fs::remove_file(scratch);
            }
            if let Ok(mut watched) = WATCHED.lock() {
                watched.remove(&job);
            }
        });
        Ok(json!({ "id": id, "name": name, "kind": kind, "state": "running", "started": started }))
    }

    /// The last lines of a job's log.
    fn tail(&self, id: &str) -> String {
        let Ok(mut file) = std::fs::File::open(self.path(id, "log")) else { return String::new() };
        let length = file.metadata().map_or(0, |meta| meta.len());
        let _ = file.seek(SeekFrom::Start(length.saturating_sub(TAIL_BYTES)));
        let mut bytes = Vec::new();
        let _ = file.read_to_end(&mut bytes);
        let text = String::from_utf8_lossy(&bytes);
        let lines: Vec<&str> = text.lines().collect();
        lines[lines.len().saturating_sub(TAIL_LINES)..].join("\n")
    }

    /// `{id, name, kind, state, started, finished, exit, log_tail}`, where
    /// `state` is `running`, `done` or `failed`.
    pub fn state(&self, id: &str) -> Result<Value, String> {
        if !valid_id(id) {
            return Err(format!("not a job id: {id}"));
        }
        let mut record = self.record(id).ok_or_else(|| format!("there is no job {id}"))?;
        if record["state"] == "running" {
            let watched = WATCHED.lock().is_ok_and(|watched| watched.contains(id));
            // Started before the module last restarted: nothing waits on it.
            if !watched && !record["pid"].as_u64().is_some_and(alive) {
                record["state"] = Value::from("failed");
                record["error"] = Value::from("the names module restarted while this ran, so how it ended is unknown; read the name again");
            }
        }
        let tail = self.tail(id);
        record["log_tail"] = Value::from(match record["error"].as_str() {
            Some(why) if tail.is_empty() => why.to_owned(),
            Some(why) => format!("{tail}\n{why}"),
            None => tail,
        });
        if let Some(fields) = record.as_object_mut() {
            fields.remove("pid");
        }
        Ok(record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("logos-names-cli-{label}-{}-{}", std::process::id(), now_ms()));
        std::fs::create_dir_all(&dir).expect("a temp dir");
        dir
    }

    /// A stand-in CLI: a shell script that prints each argument as `[arg]`,
    /// then the wallet it was given, then runs `tail`.
    fn fake_cli(dir: &Path, tail: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join("names");
        let script = format!(
            "#!/bin/sh\nfor arg in \"$@\"; do printf '[%s]\\n' \"$arg\"; done\necho \"wallet=$LEE_WALLET_HOME_DIR\"\n{tail}\n"
        );
        std::fs::write(&path, script).expect("the script");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("executable");
        path
    }

    fn setup(dir: &Path, tail: &str) -> Setup {
        let wallet = dir.join("wallet");
        std::fs::create_dir_all(&wallet).expect("a wallet dir");
        std::fs::write(wallet.join(STORAGE_FILE), "{}").expect("a wallet");
        Setup {
            program: fake_cli(dir, tail),
            wallet,
            registry: Some("38jEfVV1RCyMzu1FGbYcCn5FqxCxKyY58VgzjruAh2gq".into()),
            sequencer: "https://testnet.lez.logos.co".into(),
        }
    }

    fn finished(jobs: &Jobs, id: &str) -> Value {
        for _ in 0..200 {
            let state = jobs.state(id).expect("a job");
            if state["state"] != "running" {
                return state;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("job {id} did not finish");
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_owned()).collect()
    }

    #[test]
    fn set_text_passes_values_verbatim_after_the_terminator() {
        let hostile = "it's \"quoted\"\n$(rm -rf ~); `id` | --replace";
        assert_eq!(
            set_text_args("Jef.LEZ", " chat ", hostile).unwrap(),
            strings(&["set-text", "--", "jef", "chat", hostile])
        );
        // A value that looks like an option is still a positional after `--`.
        assert_eq!(set_text_args("jef", "bio", "--help").unwrap()[4], "--help");
        assert!(set_text_args("jef", "Bad Key", "x").is_err());
        assert!(set_text_args("jef", "a=b", "x").is_err());
    }

    #[test]
    fn names_with_shell_metacharacters_are_refused() {
        for name in ["jef; rm -rf ~", "$(id)", "jef`id`", "a|b", "jef\nbob", "../jef", "-jef", "jef lez", "\"jef\"", "jef&"] {
            assert!(set_text_args(name, "chat", "x").is_err(), "{name:?}");
            assert!(set_page_args(name, "text/plain", None).is_err(), "{name:?}");
            assert!(profile_args(name, &[("bio".into(), "x".into())], false).is_err(), "{name:?}");
        }
    }

    #[test]
    fn profile_entries_map_to_the_cli_options() {
        let entries = vec![
            ("display".to_owned(), "-Jef \"the\" one\nline two".to_owned()),
            ("bio".to_owned(), String::new()),
            ("theme".to_owned(), "dark".to_owned()),
            ("color".to_owned(), "#96d678".to_owned()),
            ("font".to_owned(), "mono".to_owned()),
            ("cid".to_owned(), "zDv=x".to_owned()),
            ("chat".to_owned(), "a=b; c".to_owned()),
            ("avatar".to_owned(), "data:image/png;base64,AAAA".to_owned()),
        ];
        let made = profile_args("jef.lez", &entries, false).unwrap();
        assert_eq!(
            made.args,
            strings(&[
                "profile",
                "--display=-Jef \"the\" one\nline two",
                "--clear=bio",
                "--theme=dark",
                "--color=#96d678",
                "--font=mono",
                "--cid=zDv=x",
                "--set=chat=a=b; c",
                "--",
                "jef",
            ])
        );
        assert_eq!(made.skipped, ["avatar"]);
        // With replace, an avatar already in the record is kept.
        let kept = profile_args("jef", &entries[7..], true).unwrap();
        assert_eq!(kept.args, strings(&["profile", "--set=avatar=data:image/png;base64,AAAA", "--replace", "--", "jef"]));
        // Only an avatar that is not a file: nothing for the CLI to do.
        assert!(profile_args("jef", &entries[7..], false).unwrap_err().contains("PNG file"));
        assert!(profile_args("jef", &[], false).is_err());
        assert!(profile_args("jef", &[("theme".into(), "neon".into())], false).is_err());
        assert!(profile_args("jef", &[("color".into(), "red".into())], false).is_err());
        assert!(profile_args("jef", &[("Bad".into(), "x".into())], false).is_err());
    }

    #[test]
    fn an_avatar_is_passed_only_as_a_png_file() {
        let dir = scratch_dir("avatar");
        let png = dir.join("me.png");
        std::fs::write(&png, b"\x89PNG\r\n\x1a\nrest").unwrap();
        let other = dir.join("me.txt");
        std::fs::write(&other, b"not a png").unwrap();
        let png_text = png.to_string_lossy().into_owned();
        let made = profile_args("jef", &[("avatar".into(), png_text.clone())], false).unwrap();
        assert_eq!(made.args, strings(&["profile", &format!("--avatar={png_text}"), "--", "jef"]));
        assert!(made.skipped.is_empty());
        for value in [other.to_string_lossy().into_owned(), "me.png".to_owned(), "/no/such.png".to_owned()] {
            assert!(profile_args("jef", &[("avatar".into(), value)], false).is_err());
        }
        assert_eq!(profile_args("jef", &[("avatar".into(), String::new())], false).unwrap().args[1], "--clear=avatar");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_page_is_set_from_a_file_or_cleared() {
        let file = Path::new("/tmp/jobs/jef-1.page");
        assert_eq!(
            set_page_args("jef", " text/html; charset=utf-8 ", Some(file)).unwrap(),
            strings(&["set-page", "--content-type=text/html; charset=utf-8", "--", "jef", "/tmp/jobs/jef-1.page"])
        );
        assert_eq!(set_page_args("jef", "", None).unwrap(), strings(&["clear-page", "--", "jef"]));
        assert!(set_page_args("jef", "text/html\n--replace", Some(file)).is_err());
    }

    #[test]
    fn the_module_registry_and_sequencer_lead_the_argv() {
        let dir = scratch_dir("argv");
        let setup = setup(&dir, "exit 0");
        assert_eq!(
            setup.argv(mine_args()),
            strings(&["--program=38jEfVV1RCyMzu1FGbYcCn5FqxCxKyY58VgzjruAh2gq", "--rpc=https://testnet.lez.logos.co", "mine"])
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn board_commands_check_their_input() {
        assert_eq!(
            post_args("Jef", "-hi", &["zDvZ1".into()], "ada/3").unwrap(),
            strings(&["post", "--as=jef", "--cid=zDvZ1", "--reply=ada/3", "--", "-hi"])
        );
        assert!(post_args("jef", "", &[], "").is_err());
        assert!(post_args("jef", &"x".repeat(1025), &[], "").is_err());
        assert!(post_args("jef", "hi", &["a b".into()], "").is_err());
        assert!(post_args("jef", "hi", &[], "ada").is_err());
        assert!(post_args("jef; rm", "hi", &[], "").is_err());
        assert_eq!(like_args("jef", "@ada/0", true).unwrap(), strings(&["like", "--as=jef", "--undo", "--", "ada/0"]));
        assert_eq!(hide_args("jef/2").unwrap(), ("jef".to_owned(), strings(&["hide", "--", "jef/2"])));
        assert!(hide_args("jef/x").is_err());
        let id = "7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR";
        assert_eq!(like_args("jef", id, false).unwrap(), strings(&["like", "--as=jef", "--", id]));
        assert_eq!(hide_args(id).unwrap().0, "board");
        assert!(post_args("jef", "hi", &[], "not-a-post").is_err());
        assert_eq!(pay_args("Ada", "1,000").unwrap(), strings(&["pay", "--", "ada", "1000"]));
        assert!(pay_args("ada", "0").is_err());
        assert!(pay_args("ada", "-5").is_err());
        assert!(pay_args("ada", "1e9").is_err());
    }

    #[test]
    fn held_names_are_read_from_mine() {
        let output = "carol.lez  registering, ready in 4 min (log: /w/names-register-carol.log)\n\
                      bob.lez  NOT registered: refused; run `names register bob` to try again (log: x)\n\
                      jef.lez  held by a deposit (no expiry)\n\
                      ada.lez  lapsed until 2026-01-01 00:00\n\
                      old.lez  no longer held by this wallet\n\
                      Evil.lez  held by a deposit (no expiry)\n\
                      x;y.lez  held by a deposit (no expiry)\n\
                      jef.lez  held by a deposit (no expiry)\n\
                      no names yet; register one with `names register <name>`\n";
        let names = parse_mine(output);
        let listed: Vec<&str> = names.iter().map(|name| name["name"].as_str().unwrap()).collect();
        assert_eq!(listed, ["carol", "bob", "jef", "ada"]);
        assert_eq!(names[0]["registering"], true);
        assert_eq!(names[0]["state"], "registering, ready in 4 min");
        assert_eq!(names[1]["failed"], true);
        assert_eq!(names[2]["signer"], "cli");
        assert_eq!(names[2]["registering"], false);
        assert_eq!(names[3]["lapsed"], true);
        assert_eq!(register_args("Jef.LEZ", false).unwrap(), strings(&["register", "--", "jef"]));
        assert_eq!(register_args("jef", true).unwrap(), strings(&["register", "--public", "--", "jef"]));
        let newer = parse_mine("a.lez  held by a deposit (no expiry); held privately\nb.lez  held by a deposit (no expiry); held by a public account\n");
        assert_eq!((newer[0]["private"].clone(), newer[1]["private"].clone()), (json!(true), json!(false)));
        assert_eq!(newer[0]["state"], "held by a deposit (no expiry)");
        assert_eq!(names[2]["private"], Value::Null);
        let balance = "public accounts (seen on chain):\n  Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk  5\nprivate accounts (only this wallet sees them):\n  Private/yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z  250000000  (default)\nprivate total: 250000000\n";
        assert_eq!(parse_balance(balance), json!({ "account": "yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z", "balance": "250000000" }));
        assert_eq!(parse_balance("private total: 0\n"), Value::Null);
        assert!(register_args("--help", false).is_err());
        assert!(parse_mine("no names yet; register one with `names register <name>`\n").is_empty());
    }

    const KEY: &str = "a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90";
    const PUBLIC: &str = "CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk";

    #[test]
    fn the_default_public_account_is_read_from_balance() {
        let balance = "public accounts (seen on chain):\n  Public/7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR  9\n  Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk  5000  (default)\nprivate accounts (only this wallet sees them):\n  Private/yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z  250000000  (default)\nprivate total: 250000000\n";
        assert_eq!(parse_public_balance(balance), json!({ "account": PUBLIC, "balance": "5000" }));
        // The private one is unchanged by it.
        assert_eq!(parse_balance(balance), json!({ "account": "yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z", "balance": "250000000" }));
        // No default public account, or a malformed one: null.
        assert_eq!(parse_public_balance("public accounts (seen on chain):\n  Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk  5\n"), Value::Null);
        assert_eq!(parse_public_balance("  Public/not-an-id  5  (default)\n"), Value::Null);
        assert_eq!(parse_public_balance("  Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk  -5  (default)\n"), Value::Null);
        // A private line is never taken for the public one.
        assert_eq!(parse_public_balance("  Private/yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z  1  (default)\n"), Value::Null);
    }

    #[test]
    fn balance_runs_the_cli_and_adds_the_public_account() {
        let dir = scratch_dir("balance");
        let setup = setup(&dir, "echo '  Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk  70  (default)'; echo '  Private/yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z  250  (default)'");
        assert_eq!(
            run_balance(&setup).unwrap(),
            json!({ "account": "yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z", "balance": "250", "public": { "account": PUBLIC, "balance": "70" } })
        );
        let none = Setup { program: fake_cli(&dir, "echo 'private total: 0'"), ..setup };
        assert_eq!(run_balance(&none).unwrap(), Value::Null);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn bridge_arguments_are_checked_strictly() {
        let made = bridge_args("1000", &format!("Public/{PUBLIC}"), &KEY.to_uppercase(), "", false).unwrap();
        assert_eq!(
            made,
            strings(&["bridge", &format!("--to=Public/{PUBLIC}"), &format!("--from-key={KEY}"), "--node=http://127.0.0.1:8080", "--", "1000"])
        );
        // Only a send carries --yes.
        assert!(!made.iter().any(|arg| arg == "--yes"));
        let sent = bridge_args(" 5 ", PUBLIC, KEY, "http://localhost:18080/", true).unwrap();
        assert_eq!(
            sent,
            strings(&["bridge", &format!("--to=Public/{PUBLIC}"), &format!("--from-key={KEY}"), "--node=http://localhost:18080", "--yes", "--", "5"])
        );
        for amount in ["", "0", "000", "-5", "1e9", "1,000", "1.5", "5 5", "--yes", &"9".repeat(31)] {
            assert!(bridge_args(amount, PUBLIC, KEY, "", false).is_err(), "{amount:?}");
        }
        for key in ["", &KEY[1..], &format!("{KEY}0"), &KEY.replace('a', "g"), &format!("{} ", &KEY[..63]), "--yes", &format!("../{}", &KEY[3..])] {
            assert!(bridge_args("5", PUBLIC, key, "", false).is_err(), "{key:?}");
        }
        for to in ["", "jef", "Public/x", "--yes", "CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk; id"] {
            assert!(bridge_args("5", to, KEY, "", false).is_err(), "{to:?}");
        }
        for node in [
            "http://127.0.0.1",
            "http://127.0.0.1:",
            "http://127.0.0.1:8080/x",
            "http://127.0.0.1:8080//",
            "http://127.0.0.1:8080@evil.com",
            "http://127.0.0.1:8080?x=1",
            "http://127.0.0.1:8080#x",
            "http://127.0.0.1.evil.com:80",
            "http://evil.com:8080",
            "http://user@127.0.0.1:8080",
            "http://[::1]:8080",
            "http://0.0.0.0:8080",
            "file://127.0.0.1:8080",
            "ftp://localhost:21",
            "127.0.0.1:8080",
            "http://localhost:0",
            "http://localhost:65536",
            "http://localhost:99999",
            "http://localhost:+80",
            "http://LOCALHOST:8080",
            "http://localhost:8080\n--yes",
        ] {
            assert!(bridge_node(node).is_err(), "{node:?}");
        }
        assert_eq!(bridge_node(" https://127.0.0.1:443 ").unwrap(), "https://127.0.0.1:443");
        assert_eq!(bridge_node("http://localhost:65535").unwrap(), "http://localhost:65535");
    }

    #[test]
    fn bridge_output_is_read_into_plain_parts() {
        let plan = format!(
            "deposit 1200 from 3 note(s) of {KEY}\n  into LEZ channel {}, credited to Public/{PUBLIC}\n  through http://127.0.0.1:8080, fee at most 100000\nnothing sent; run again with --yes to deposit (it cannot be undone)\n",
            "ab".repeat(32)
        );
        let read = parse_bridge(&plan, false);
        assert_eq!(read["sent"], false);
        assert_eq!((read["total"].clone(), read["notes"].clone(), read["fee"].clone()), (json!("1200"), json!("3"), json!("100000")));
        assert_eq!(read["to"], PUBLIC);
        assert_eq!(read["hash"], Value::Null);
        assert_eq!(read["lines"].as_array().unwrap().len(), 4);
        let sent = parse_bridge(&format!("{plan}submitted deposit 9F3A00\nLEZ credits Public/{PUBLIC} once ...\n"), true);
        assert_eq!(sent["hash"], "9f3a00");
        assert_eq!(parse_bridge("submitted deposit 0xAB12\n", true)["hash"], "ab12");
        // A reply that is not a hash is not shown as one.
        assert_eq!(parse_bridge("submitted deposit {\"error\":1}\n", true)["hash"], Value::Null);
    }

    #[test]
    fn bridge_failures_are_said_plainly() {
        let node = "http://127.0.0.1:8080";
        assert!(bridge_problem("Error: cannot reach your Logos Blockchain node at http://127.0.0.1:8080\n\nCaused by:\n    Connection refused (os error 61)\n", node).contains("isn't answering"));
        // As the real CLI writes them, against a node that is up.
        let status = |code: &str| bridge_problem(&format!("Error: cannot reach your Logos Blockchain node at {node}\n\nCaused by:\n    http status: {code}\n"), node);
        assert!(status("404").contains("doesn't have that key"));
        assert!(status("400").contains("isn't a valid key"));
        assert!(status("500").contains("HTTP 500"));
        let short = format!("Error: 30 note(s) of {KEY} hold 900, less than 5000; one deposit spends at most 30 notes, so bridge in smaller steps\n");
        assert!(bridge_problem(&short, node).contains("smaller steps"));
        let poor = format!("Error: 2 note(s) of {KEY} hold 900, less than 5000; one deposit spends at most 30 notes, so bridge in smaller steps\n");
        assert_eq!(bridge_problem(&poor, node), "That key holds only 900. Bridge 900 or less.");
        assert!(bridge_problem(&format!("Error: the node lists no notes for {KEY}\n"), node).contains("nothing under that key"));
        assert!(bridge_problem("Error: cannot reach the sequencer at https://testnet.lez.logos.co\n", node).contains("LEZ isn't answering"));
        assert_eq!(bridge_problem("Error: the node refused the deposit (HTTP 400): bad fee\n", node), "Your node refused the deposit: (HTTP 400): bad fee");
        assert_eq!(bridge_problem("Error: something new\n\nCaused by:\n    deep\n", node), "the names CLI failed: something new");
    }

    #[test]
    fn bridge_runs_the_cli_to_the_default_public_account() {
        let dir = scratch_dir("bridge");
        let plan = format!("echo 'deposit 1200 from 3 note(s) of {KEY}'; echo '  into LEZ channel abc, credited to Public/{PUBLIC}'; echo '  through http://127.0.0.1:8080, fee at most 100000'");
        let setup = setup(&dir, &plan);
        // No public account saved yet: refused before the CLI runs.
        assert!(run_bridge(&setup, "1000", KEY, "", false).unwrap_err().contains("create your wallet"));
        std::fs::write(setup.wallet.join("names.json"), format!("{{\"public\":\"{PUBLIC}\",\"private\":null}}")).unwrap();
        let read = run_bridge(&setup, "1000", KEY, "", false).unwrap();
        let lines: Vec<&str> = read["lines"].as_array().unwrap().iter().map(|line| line.as_str().unwrap()).collect();
        let at = lines.iter().position(|line| *line == "[bridge]").expect("the bridge command");
        assert_eq!(
            &lines[at..at + 6],
            [
                "[bridge]",
                &format!("[--to=Public/{PUBLIC}]") as &str,
                &format!("[--from-key={KEY}]"),
                "[--node=http://127.0.0.1:8080]",
                "[--]",
                "[1000]",
            ]
        );
        assert!(!lines.contains(&"[--yes]"));
        assert_eq!(read["total"], "1200");
        assert_eq!(read["sent"], false);
        // The stand-in CLI is the only one ever given --yes in a test.
        let sent = Setup { program: fake_cli(&dir, "echo 'submitted deposit 00ff'"), ..setup.clone() };
        let done = run_bridge(&sent, "1000", KEY, "http://localhost:9000", true).unwrap();
        assert!(done["lines"].as_array().unwrap().contains(&json!("[--yes]")));
        assert!(done["lines"].as_array().unwrap().contains(&json!("[--node=http://localhost:9000]")));
        assert_eq!(done["hash"], "00ff");
        let down = Setup {
            program: fake_cli(&dir, "echo 'Error: cannot reach your Logos Blockchain node at http://127.0.0.1:8080' >&2; echo '' >&2; echo 'Caused by:' >&2; echo '    refused' >&2; exit 1"),
            ..setup.clone()
        };
        assert!(run_bridge(&down, "1000", KEY, "", false).unwrap_err().contains("isn't answering"));
        // Bad input never reaches the CLI.
        let never = Setup { program: dir.join("not-run"), ..setup };
        assert!(run_bridge(&never, "1000", KEY, "http://evil.com:80", false).unwrap_err().contains("this computer"));
        assert!(run_bridge(&never, "0", KEY, "", false).is_err());
        assert!(run_bridge(&never, "5", "zz", "", false).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn mine_runs_the_cli_and_an_empty_wallet_dir_holds_nothing() {
        let dir = scratch_dir("mine");
        let setup = setup(&dir, "echo 'jef.lez  held by a deposit (no expiry)'; echo 'x.lez  no longer held by this wallet'");
        let names = run_mine(&setup).unwrap();
        assert_eq!(names.len(), 1);
        assert_eq!(names[0]["name"], "jef");
        let failing = Setup { program: fake_cli(&dir, "echo 'Error: the sequencer is down' >&2; exit 1"), ..setup };
        assert!(run_mine(&failing).unwrap_err().contains("the sequencer is down"));
        // No wallet in the directory: the CLI is not run, so none is made.
        let empty = Setup { wallet: dir.join("empty"), program: dir.join("not-run"), ..failing };
        assert_eq!(run_mine(&empty).unwrap(), Vec::<Value>::new());
        assert!(empty.wallet_ready().unwrap_err().contains("create your wallet"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_job_runs_in_the_background_and_reports_how_it_ended() {
        let dir = scratch_dir("jobs");
        let jobs = Jobs { dir: dir.join("jobs") };
        let hostile = "a \"b\" 'c'\n$(touch /tmp/pwned) ;|&";

        let ok = setup(&dir, "exit 0");
        let id = jobs.prepare("jef").unwrap();
        let started = jobs.start(&ok, &id, "jef", "set_text", set_text_args("jef", "chat", hostile).unwrap(), None).unwrap();
        assert_eq!(started["state"], "running");
        let done = finished(&jobs, &id);
        assert_eq!(done["state"], "done");
        assert_eq!(done["exit"], 0);
        let tail = done["log_tail"].as_str().unwrap();
        assert!(tail.contains(&format!("[{hostile}]")), "{tail}");
        assert!(tail.contains("[--]\n[jef]\n[chat]\n"), "{tail}");
        assert!(tail.contains(&format!("wallet={}", ok.wallet.display())), "{tail}");
        assert!(done.get("pid").is_none());

        let refusing = Setup { program: fake_cli(&dir, "echo 'Error: this wallet does not hold jef.lez' >&2; exit 1"), ..ok };
        let id = jobs.prepare("jef").unwrap();
        jobs.start(&refusing, &id, "jef", "set_text", set_text_args("jef", "chat", "x").unwrap(), None).unwrap();
        let failed = finished(&jobs, &id);
        assert_eq!(failed["state"], "failed");
        assert_eq!(failed["exit"], 1);
        assert_eq!(last_line(failed["log_tail"].as_str().unwrap()), "Error: this wallet does not hold jef.lez");

        assert!(jobs.state("../etc/passwd").is_err());
        assert!(jobs.state("nope-1").unwrap_err().contains("no job"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn one_job_runs_per_name_and_its_files_are_cleaned_up() {
        let dir = scratch_dir("one");
        let jobs = Jobs { dir: dir.join("jobs") };
        let slow = setup(&dir, "sleep 1");
        let id = jobs.prepare("jef").unwrap();
        let body = jobs.scratch(&id, "page");
        std::fs::write(&body, "<p>hi</p>").unwrap();
        let args = set_page_args("jef", "text/html", Some(&body)).unwrap();
        jobs.start(&slow, &id, "jef", "set_page", args, Some(body.clone())).unwrap();
        assert_eq!(jobs.running_for("jef").as_deref(), Some(id.as_str()));
        assert!(jobs.prepare("jef").unwrap_err().contains("still being saved"));
        // Another name is not held up.
        assert!(jobs.prepare("ada").is_ok());
        assert_eq!(finished(&jobs, &id)["state"], "done");
        assert!(!body.exists());
        assert!(jobs.prepare("jef").is_ok());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_job_nothing_waits_on_is_failed_once_its_process_is_gone() {
        let dir = scratch_dir("orphan");
        let jobs = Jobs { dir: dir.clone() };
        // As left by a module that stopped mid-job: running, with a dead pid.
        jobs.write("jef-5", &json!({ "id": "jef-5", "name": "jef", "state": "running", "pid": 999_999_999_u64, "started": 1 }));
        std::fs::write(jobs.path("jef-5", "log"), "proving...\n").unwrap();
        let state = jobs.state("jef-5").unwrap();
        assert_eq!(state["state"], "failed");
        assert!(last_line(state["log_tail"].as_str().unwrap()).contains("restarted"));
        assert!(jobs.running_for("jef").is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_missing_cli_is_reported_and_leaves_nothing_behind() {
        let dir = scratch_dir("missing");
        let jobs = Jobs { dir: dir.join("jobs") };
        let missing = Setup { program: dir.join("no-such-names"), ..setup(&dir, "exit 0") };
        let id = jobs.prepare("jef").unwrap();
        let body = jobs.scratch(&id, "page");
        std::fs::write(&body, "x").unwrap();
        let error = jobs.start(&missing, &id, "jef", "set_page", vec!["clear-page".into()], Some(body.clone())).unwrap_err();
        assert!(error.contains("could not run the names CLI"));
        assert!(!body.exists());
        assert!(jobs.running_for("jef").is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    // ── Getting started without a terminal ──────────────────────────────

    const PHRASE: &str = "abandon ability able about above absent absorb abstract absurd abuse access accident";
    const PRIVATE: &str = "yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z";

    fn setup_output(phrase: bool) -> String {
        let made = if phrase {
            format!("Created a new wallet. Write down its recovery phrase and keep it safe;\nit is the only way to get the wallet back:\n\n  {PHRASE}\n\n")
        } else {
            String::new()
        };
        format!("{made}public account   Public/{PUBLIC}\nprivate account  Private/{PRIVATE}\nready: `names register <name>` holds a name privately\n")
    }

    #[test]
    fn setup_output_is_read_into_accounts_and_the_phrase() {
        assert_eq!(parse_setup(&setup_output(true)), json!({ "public": PUBLIC, "private": PRIVATE, "phrase": PHRASE }));
        // An existing wallet prints no phrase.
        assert_eq!(parse_setup(&setup_output(false)), json!({ "public": PUBLIC, "private": PRIVATE, "phrase": null }));
        // Anything that isn't 12 to 24 words of a-z is not taken for one.
        for odd in ["Error: boom", "abandon ability", "abandon Ability able about above absent absorb abstract absurd abuse access accident", "a1 b c d e f g h i j k l"] {
            let text = format!("it is the only way to get the wallet back:\n\n  {odd}\npublic account   Public/{PUBLIC}\n");
            assert_eq!(parse_setup(&text)["phrase"], Value::Null, "{odd}");
        }
        // Malformed accounts are null.
        assert_eq!(parse_setup("public account   Public/nope\nprivate account  Public/x\n"), json!({ "public": null, "private": null, "phrase": null }));
    }

    #[test]
    fn setup_arguments_never_shield_unless_asked() {
        assert_eq!(setup_args("0").unwrap(), strings(&["setup", "--shield=0"]));
        assert_eq!(setup_args("").unwrap(), strings(&["setup", "--shield=5000000"]));
        assert_eq!(setup_args(" 7,000,000 ").unwrap(), strings(&["setup", "--shield=7000000"]));
        for bad in ["-1", "1.5", "5 5", "--yes", "1e6", "00"] {
            assert!(setup_args(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn creating_the_wallet_runs_setup_moving_nothing() {
        let dir = scratch_dir("create");
        let mut setup = setup(&dir, &format!("printf '%s' '{}'", setup_output(true).replace('\'', "'\\''")));
        // No wallet yet: the CLI makes it, so it's not required first.
        std::fs::remove_file(setup.wallet.join(STORAGE_FILE)).unwrap();
        let made = run_create_wallet(&setup).unwrap();
        assert_eq!((made["public"].clone(), made["private"].clone(), made["phrase"].clone()), (json!(PUBLIC), json!(PRIVATE), json!(PHRASE)));
        // The phrase is written nowhere: no job, no log.
        assert!(std::fs::read_dir(&dir).unwrap().all(|entry| !entry.unwrap().file_name().to_string_lossy().ends_with(".log")));
        // The arguments were exactly setup --shield=0.
        setup.program = fake_cli(&dir, "exit 0");
        let echo = run_capture(&setup, setup_args("0").unwrap(), SETUP_PATIENCE).unwrap().1;
        assert!(echo.contains("[setup]\n[--shield=0]\n"), "{echo}");
        // A failure after the wallet was made still hands over the phrase.
        setup.program = fake_cli(&dir, &format!("printf '%s' '{}'; echo 'Error: the sequencer is down' >&2; exit 1", setup_output(true).replace('\'', "'\\''")));
        let late = run_create_wallet(&setup).unwrap();
        assert_eq!(late["phrase"], PHRASE);
        assert!(late["problem"].as_str().unwrap().contains("the sequencer is down"));
        // A failure before: the reason comes from stderr only, never stdout.
        setup.program = fake_cli(&dir, &format!("echo '  {PHRASE}'; echo 'Error: no space' >&2; exit 1"));
        let refused = run_create_wallet(&setup).unwrap_err();
        assert!(refused.contains("no space") && !refused.contains("abandon"), "{refused}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_move_to_private_says_whether_anything_moved() {
        let poor = "public account   Public/x\nPublic/x holds 1200; send it at least 5000000 testnet tokens from your LEZ wallet (or with `names bridge`), then run `names setup` again.\n";
        let read = shield_outcome(poor, true);
        assert_eq!(read["moved"], false);
        assert_eq!(read["problem"], "Your public account holds 1,200, and moving to private needs at least 5,000,000. Get more tokens, then try again.");
        let moved = shield_outcome("moving 5000000 to the private account; generating the proof (a few minutes)\nready: ...\n", true);
        assert_eq!(moved, json!({ "moved": true, "problem": null }));
        let failed = shield_outcome("moving 5000000 to the private account\nError: the transfer was not included in a block\n", false);
        assert_eq!(failed["moved"], false);
        assert!(failed["problem"].as_str().unwrap().contains("not included"));
        assert_eq!(group("1234567"), "1,234,567");
        assert_eq!(group("100"), "100");
    }

    #[test]
    fn a_payment_goes_to_a_name_or_a_public_account_strictly() {
        assert_eq!(pay_to("Jef").unwrap(), PayTo::Name("jef".into()));
        assert_eq!(pay_to(&format!(" Public/{PUBLIC} ")).unwrap(), PayTo::Account(PUBLIC.into()));
        assert!(pay_to(PUBLIC).unwrap_err().contains("Public/"));
        assert!(pay_to(&format!("Private/{PRIVATE}")).unwrap_err().contains("pay its name"));
        for bad in ["Public/", "Public/nope", &format!("Public/{PUBLIC}x"), &format!("public/{PUBLIC}"), &format!("Public/{PUBLIC}/x"), &format!("Public/ {PUBLIC}"), "Public/--yes", "", "a b"] {
            assert!(pay_to(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn paying_an_account_runs_the_wallets_transfer() {
        let dir = scratch_dir("payacct");
        let setup = setup(&dir, "exit 0");
        let to = format!("Public/{}", "7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR");
        // No accounts saved yet.
        assert!(pay_account_args(&setup, &to, "5", false).unwrap_err().contains("create your wallet"));
        std::fs::write(setup.wallet.join("names.json"), format!("{{\"public\":\"{PUBLIC}\",\"private\":\"{PRIVATE}\"}}")).unwrap();
        assert_eq!(
            pay_account_args(&setup, &to, "1,000", false).unwrap(),
            strings(&["wallet", "auth-transfer", "send", &format!("--from=Public/{PUBLIC}"), &format!("--to={to}"), "--amount=1000"])
        );
        assert_eq!(pay_account_args(&setup, &to, "5", true).unwrap()[3], format!("--from=Private/{PRIVATE}"));
        assert!(pay_account_args(&setup, &format!("Public/{PUBLIC}"), "5", false).unwrap_err().contains("your own"));
        assert!(pay_account_args(&setup, "jef", "5", false).is_err());
        for amount in ["0", "", "-5", "1.5", "--yes", "5 5"] {
            assert!(pay_account_args(&setup, &to, amount, false).is_err(), "{amount:?}");
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_built_in_installer_is_the_published_one() {
        let published = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../gateway/public/install.sh");
        // Only in the repository; a build from basecamp-module alone can't see it.
        if let Ok(text) = std::fs::read_to_string(&published) {
            assert_eq!(text, INSTALLER, "copy gateway/public/install.sh to basecamp-module/rust-lib/install.sh");
        }
        // It checks signatures against its own key, and honours the folders it's given.
        assert!(INSTALLER.contains("ssh-keygen -Y verify") && INSTALLER.contains("allowed_signers"));
        assert!(INSTALLER.contains("NAMES_INSTALL_DIR") && INSTALLER.contains("NAMES_MAN_DIR"));
        // It edits a shell startup file only when its folder isn't on PATH,
        // which install_process makes sure it is.
        assert!(INSTALLER.contains("case \":$PATH:\" in\n  *\":$DIR:\"*) ;;"));
    }

    #[test]
    fn the_installer_runs_with_only_the_folders_it_needs() {
        let dir = scratch_dir("install");
        let (bin, man) = (dir.join("data/bin"), dir.join("data/man"));
        let jobs = Jobs { dir: dir.join("jobs") };
        let script = "#!/bin/sh\nenv > \"$NAMES_INSTALL_DIR/../env\"\necho \"args=$#\"\nprintf '#!/bin/sh\\necho \"names 1.2.3\"\\n' > \"$NAMES_INSTALL_DIR/names\"\nchmod 755 \"$NAMES_INSTALL_DIR/names\"\n";
        let started = jobs.start_install(script, &bin, &man).unwrap();
        assert_eq!((started["name"].clone(), started["kind"].clone()), (json!(INSTALL_JOB), json!("install")));
        let id = started["id"].as_str().unwrap().to_owned();
        let ended = finished(&jobs, &id);
        assert_eq!(ended["state"], "done", "{ended}");
        assert!(ended["log_tail"].as_str().unwrap().contains("args=0"));
        let env = std::fs::read_to_string(dir.join("data/env")).unwrap();
        let path_line = env.lines().find(|line| line.starts_with("PATH=")).unwrap();
        assert!(path_line.starts_with(&format!("PATH={}:", bin.display())), "{path_line}");
        assert!(env.contains(&format!("NAMES_INSTALL_DIR={}", bin.display())) && env.contains(&format!("NAMES_MAN_DIR={}", man.display())));
        for var in ["NAMES_DOWNLOAD_BASE", "NAMES_SKIP_PROVER", "LEE_WALLET_HOME_DIR", "SHELL"] {
            assert!(!env.lines().any(|line| line.starts_with(&format!("{var}="))), "{var} leaked in");
        }
        // The script file is gone once it has run.
        assert!(!jobs.scratch(&id, "sh").exists());
        assert_eq!(version(&bin.join("names")).unwrap(), "names 1.2.3");
        assert!(!has_prover(&bin.join("names")) || std::env::var_os("RISC0_SERVER_PATH").is_some());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_program_that_isnt_the_names_cli_has_no_version() {
        let dir = scratch_dir("version");
        assert!(version(&dir.join("missing")).is_err());
        let other = fake_cli(&dir, "exit 0");
        assert!(version(&other).is_err());
        let broken = fake_cli(&dir, "echo 'dyld: Library not loaded' >&2; exit 134");
        assert!(version(&broken).unwrap_err().contains("Library not loaded"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn install_failures_are_said_plainly() {
        let says = |log: &str, want: &str| assert!(install_problem(log).contains(want), "{log:?} -> {}", install_problem(log));
        says("names: no build for Linux aarch64 yet (macOS on Apple silicon and Linux x86_64 only).", "doesn't run on this computer");
        says("Downloading names for macos-arm64...\nnames: bad signature on names-macos-arm64; not installing.", "signature check");
        says("names: no signature published for names-macos-arm64; not installing.", "signature check");
        says("names: checksum mismatch (expected a, got b); not installing.", "damaged");
        says("Downloading names for macos-arm64...\ncurl: (6) Could not resolve host: github.com", "internet");
        says("curl: (7) Failed to connect to github.com port 443 after 3 ms: Couldn't connect to server", "internet");
        says("curl: (22) The requested URL returned error: 404", "isn't available");
        says("Installed /x/names, but it needs the PC/SC library (libpcsclite.so.1) to start.", "PC/SC");
        says("Installed /x/names, but it does not start: Killed: 9", "Killed: 9");
        says("something new", "something new");
        assert!(install_warning("Could not install r0vm; lookups and public changes still work").is_some());
        assert!(install_warning("Installed /x/r0vm").is_none());
    }

    /// Runs the real, built-in installer the way `cli_install` does, into
    /// `$LOGOS_NAMES_REAL_INSTALL_DIR` (it downloads the signed release, about
    /// 80 MB): `cargo test real_install -- --ignored`.
    #[test]
    #[ignore = "downloads the release"]
    fn real_install() {
        let root = PathBuf::from(std::env::var_os("LOGOS_NAMES_REAL_INSTALL_DIR").expect("LOGOS_NAMES_REAL_INSTALL_DIR"));
        let (bin, man) = (root.join("bin"), root.join("man"));
        let jobs = Jobs { dir: root.join("jobs") };
        let id = jobs.start_install(INSTALLER, &bin, &man).unwrap()["id"].as_str().unwrap().to_owned();
        let started = Instant::now();
        let ended = loop {
            let state = jobs.state(&id).unwrap();
            if state["state"] != "running" {
                break state;
            }
            assert!(started.elapsed() < Duration::from_secs(900), "too slow");
            std::thread::sleep(Duration::from_secs(1));
        };
        println!("{}", ended["log_tail"].as_str().unwrap());
        assert_eq!(ended["state"], "done", "{ended}");
        println!("{}", version(&bin.join("names")).unwrap());
        assert!(has_prover(&bin.join("names")));
    }

    /// Makes a throwaway wallet with the real CLI the way `cli_create_wallet`
    /// does (`names setup --shield=0`: no transaction, nothing proven), in
    /// `$LOGOS_NAMES_REAL_WALLET_DIR` with the CLI at `$LOGOS_NAMES_REAL_CLI`.
    #[test]
    #[ignore = "runs the real CLI"]
    fn real_create_wallet() {
        let var = |name: &str| PathBuf::from(std::env::var_os(name).expect(name));
        let setup = Setup {
            program: var("LOGOS_NAMES_REAL_CLI"),
            wallet: var("LOGOS_NAMES_REAL_WALLET_DIR"),
            registry: Some("38jEfVV1RCyMzu1FGbYcCn5FqxCxKyY58VgzjruAh2gq".into()),
            sequencer: "https://testnet.lez.logos.co".into(),
        };
        let made = run_create_wallet(&setup).unwrap();
        let words = made["phrase"].as_str().map_or(0, |phrase| phrase.split(' ').count());
        println!("public {} private {} phrase words {words} problem {}", made["public"], made["private"], made["problem"]);
        assert!(made["public"].is_string() && made["private"].is_string());
        assert_eq!(saved_account(&setup.wallet, "public").as_deref(), made["public"].as_str());
        let again = run_create_wallet(&setup).unwrap();
        assert_eq!((again["public"].clone(), again["phrase"].clone()), (made["public"].clone(), Value::Null));
    }
}
