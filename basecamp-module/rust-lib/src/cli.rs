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
/// The largest snapshot fetched: the whole registry of the earlier chain.
const MAX_SNAPSHOT: u64 = 64 * 1024 * 1024;
/// Most notes one deposit spends (the CLI's MAX_DEPOSIT_NOTES).
const MAX_DEPOSIT_NOTES: u64 = 30;
/// A change that takes longer than this is stopped.
const JOB_PATIENCE: Duration = Duration::from_secs(45 * 60);
/// How much of a job's log `state` returns.
const TAIL_BYTES: u64 = 4096;
const TAIL_LINES: usize = 20;
/// How much of a job's output is read for its fee lines.
const FEE_LOG_BYTES: u64 = 256 * 1024;
/// How long reading the wallet's accounts may take (it catches up with the chain first).
const ACCOUNTS_PATIENCE: Duration = Duration::from_secs(150);
/// How long a length proof may take: the first downloads the locked-name list (about 61 MB).
#[cfg_attr(test, allow(dead_code))]
const SHORTLIST_PATIENCE: Duration = Duration::from_secs(10 * 60);
/// How long a change to the wallet's own files, or a fee quote, may take.
const QUICK_PATIENCE: Duration = Duration::from_secs(30);
/// The longest nickname an account takes (the CLI's `MAX_LABEL`).
pub const MAX_LABEL: usize = 40;
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

/// Makes `dir` (and any missing parents) and leaves `dir` itself readable by this
/// user only.
pub fn private_dir(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
}

/// Writes `bytes` to `path` so only this user can read them, in one step: a new
/// 0600 file beside it (never an existing one, so never through a symlink), then a
/// rename over `path`, which replaces a symlink rather than following it. A reader
/// sees the old file or the new one, never half of one.
pub fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    use std::os::unix::fs::DirBuilderExt;
    let dir = path.parent().ok_or_else(|| std::io::Error::other("no folder to write in"))?;
    // A missing folder is made private; an existing one is the caller's business.
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let temp = dir.join(format!(".{name}.{}.{}.tmp", std::process::id(), now_ms()));
    let written = (|| {
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&temp, path)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    written
}

/// A new file only this user can read, failing if anything (a symlink included)
/// is already at `path`.
pub fn create_private(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(path)
}

/// Whether `path` is safe to run or sign with: an absolute path to a file (or, with
/// `dir`, a directory) owned by this user (or by root, with `root_ok`, for a program
/// installed system-wide), that nobody else can write. Otherwise another account on
/// the machine could swap in its own program or wallet.
pub fn is_owned_safely(path: &Path, dir: bool, root_ok: bool) -> bool {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let Some(me) = std::env::var_os("HOME").and_then(|home| std::fs::metadata(home).ok()).map(|home| home.uid()) else {
        return false;
    };
    path.is_absolute()
        && path.metadata().is_ok_and(|meta| {
            (if dir { meta.is_dir() } else { meta.is_file() })
                && (meta.uid() == me || (root_ok && meta.uid() == 0))
                && meta.permissions().mode() & 0o022 == 0
        })
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

/// Refuses text with a control character in it other than a line break or a tab: a
/// NUL can't be passed to a program at all (the spawn failed with an OS error naming
/// the CLI's path), and the rest only hide what a post or entry says.
pub fn plain_text(text: &str, what: &str) -> Result<(), String> {
    if text.chars().any(|c| c.is_control() && c != '\n' && c != '\t') {
        return Err(format!("{what} can't hold control characters (such as a NUL); remove them and try again"));
    }
    Ok(())
}

/// Refuses a page body of text (a `text/…` content type, or none, the CLI's default
/// being text) with a control character in it other than a line break (`\n`, `\r`) or a
/// tab: a NUL, an escape sequence or a bell only hide what the page says.
pub fn page_text(content_type: &str, body: &str) -> Result<(), String> {
    let kind = content_type.trim().to_ascii_lowercase();
    if (kind.is_empty() || kind.starts_with("text/")) && body.chars().any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t')) {
        return Err("a page of text can't hold control characters (such as a NUL); remove them and try again".to_owned());
    }
    Ok(())
}

/// `names post`.
pub fn post_args(name: &str, text: &str, cids: &[String], reply: &str) -> Result<Vec<String>, String> {
    let name = registry::parse_name(name)?;
    plain_text(text, "a post")?;
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
    let amount = amount_text(amount)?;
    Ok(vec!["pay".to_owned(), "--".to_owned(), name, amount])
}

/// `names pay` from the wallet's default public account rather than its
/// private one: the chain then shows that account paying.
pub fn pay_from_public_args(setup: &Setup, name: &str, amount: &str) -> Result<Vec<String>, String> {
    let mut args = pay_args(name, amount)?;
    args.insert(1, format!("--from=Public/{}", default_public(setup)?));
    Ok(args)
}

/// Every amount a person types, read one way: a whole number above 0 that fits in 128
/// bits, `,` or `_` only between groups of three digits (`1,000`), so `1,5` is refused,
/// never read as 15 (SC-2). The words are the app's (logic.mjs `amountRead`) and the
/// names CLI's (`tokens::parse_amount`); all three are tested against amounts.json.
pub fn amount(text: &str) -> Result<u128, String> {
    let typed = text.trim();
    if typed.is_empty() {
        return Err("Enter an amount, such as 1,000.".to_owned());
    }
    if typed.strip_prefix('-').is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit())) {
        return Err(format!("{typed} is negative. Enter an amount above 0."));
    }
    if !typed.bytes().all(|b| b.is_ascii_digit() || b == b',' || b == b'_') || !typed.bytes().any(|b| b.is_ascii_digit()) {
        return Err(format!("{typed} isn't a whole number. Enter digits only, such as 1000 or 1,000."));
    }
    let groups: Vec<&str> = typed.split([',', '_']).collect();
    // A first group starting with 0 (`0,001`) is no thousands grouping either.
    if groups.len() > 1 && !((1..=3).contains(&groups[0].len()) && !groups[0].starts_with('0') && groups[1..].iter().all(|group| group.len() == 3)) {
        return Err(format!("{typed} isn't a whole number: commas go only between groups of three digits, such as 1,000."));
    }
    let digits = groups.concat();
    let digits = digits.trim_start_matches('0');
    if digits.is_empty() {
        return Err("Enter an amount above 0.".to_owned());
    }
    digits.parse().map_err(|_| format!("{typed} is more than an amount can be."))
}

/// [`amount`] written plainly, as the CLI is given it: `1,000` is `1000`.
pub fn amount_text(text: &str) -> Result<String, String> {
    amount(text).map(|amount| amount.to_string())
}

#[cfg(test)]
#[test]
fn every_amount_is_read_as_the_app_and_the_cli_read_it() {
    let table: Value = serde_json::from_str(include_str!("amounts.json")).unwrap();
    for case in table["accept"].as_array().unwrap() {
        let (typed, want) = (case[0].as_str().unwrap(), case[1].as_str().unwrap());
        assert_eq!(amount_text(typed).as_deref(), Ok(want), "{typed:?}");
    }
    for case in table["refuse"].as_array().unwrap() {
        let (typed, want) = (case[0].as_str().unwrap(), case[1].as_str().unwrap());
        assert_eq!(amount(typed), Err(want.to_owned()), "{typed:?}");
    }
    let args = bridge_args("5,000", "7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR", &"39".repeat(32), "", true).unwrap();
    assert_eq!(args.last().map(String::as_str), Some("5000"));
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
    let amount = amount_text(amount)?;
    // An account, or its public key (`BridgeTo::arg`), which the CLI takes as it is.
    let to = match crate::deposit::public_key_text(to) {
        Some(key) => registry::hex(&key),
        None => format!("Public/{}", registry::account_text(&registry::parse_account(to)?)),
    };
    let mut args = vec![
        "bridge".to_owned(),
        format!("--to={to}"),
        format!("--from-key={}", bridge_key(from_key)?),
        format!("--node={}", bridge_node(node)?),
    ];
    if send {
        args.push("--yes".to_owned());
    }
    args.extend(["--".to_owned(), amount]);
    Ok(args)
}

/// The CLI wallet's default public account, as `names setup` saved it in the
/// wallet's `names.json` (what `names balance` marks `(default)`).
pub fn default_public(setup: &Setup) -> Result<String, String> {
    saved_account(&setup.wallet, "public")
        .ok_or_else(|| "your wallet has no public account yet; create your wallet first (Welcome, Create my wallet)".to_owned())
}

/// The Names faucet: a little LEZ for a new wallet's public account, so a new
/// person can hold a name without bridging first. It pays each account once a day.
pub const FAUCET_URL: &str = "https://lez.buggers.online/faucet";

/// Asks the faucet at `url` to pay `account` (base58): `{account, amount, tx}`,
/// or the faucet's own reason in plain words (funded today, empty, busy).
pub fn request_faucet(url: &str, account: &str) -> Result<Value, String> {
    let sent = ureq::post(url)
        .timeout(std::time::Duration::from_secs(60))
        .send_json(json!({ "account": account }));
    let reply = match sent {
        Ok(reply) => reply,
        Err(ureq::Error::Status(_, reply)) => {
            let body: Value = reply.into_json().unwrap_or(Value::Null);
            return Err(body["error"].as_str().unwrap_or("the faucet refused").to_owned());
        }
        Err(_) => return Err("could not reach the faucet; check your internet and try again".to_owned()),
    };
    let body: Value = reply.into_json().map_err(|_| "the faucet sent an unreadable reply".to_owned())?;
    if body["tx"].as_str().is_none() {
        return Err("the faucet sent an unreadable reply".to_owned());
    }
    Ok(body)
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
    // A refusal from the deposit's metadata (`deposit::Refusal`, `deposit::UNKNOWN_LEZ`) is
    // already in plain words.
    if let Some(line) = text.lines().map(str::trim).find(|line| line.ends_with("Nothing was sent.")) {
        return line.trim_start_matches("Error:").trim().to_owned();
    }
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

/// Where a bridge goes: the public LEZ account (base58) and, when it was given by its
/// public key, that key. LEZ 0.4 credits a bridge only to an account named by its key
/// (`deposit::metadata`), which the wallet has for its own accounts and only the owner can
/// give for another's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeTo {
    pub account: String,
    pub key: Option<[u8; 32]>,
}

impl BridgeTo {
    /// How `names bridge --to` takes it: the key when there is one (it names the account).
    pub fn arg(&self) -> String {
        self.key.map_or_else(|| format!("Public/{}", self.account), |key| registry::hex(&key))
    }
}

/// The public LEZ account a bridge credits, as typed: `Public/` and its ID, written once
/// (`Public/Private/<id>` was read as `Public/<id>`), as base58, or the account's public
/// key (64 hex characters, a point LEZ accepts); `None` when empty (the wallet's default).
pub fn bridge_to(to: &str) -> Result<Option<BridgeTo>, String> {
    let to = to.trim();
    if to.is_empty() {
        return Ok(None);
    }
    if let Some(key) = crate::deposit::public_key_text(to) {
        if !crate::deposit::valid_public_key(&key) {
            return Err(crate::deposit::Refusal::NotAKey { key }.words(&registry::account_text));
        }
        return Ok(Some(BridgeTo { account: registry::account_text(&crate::deposit::key_account(&key)), key: Some(key) }));
    }
    let id = to
        .strip_prefix("Public/")
        .ok_or("the bridge credits a public LEZ account: Public/ and its ID, or the account's public key (64 hex characters)")?;
    let account = registry::bare_account(id).map_err(|_| "that isn't a public LEZ account: Public/ once, then its ID".to_owned())?;
    Ok(Some(BridgeTo { account: registry::account_text(&account), key: None }))
}

/// Runs `names bridge` (only the plan unless `send`) to the wallet's default
/// public account and returns what `parse_bridge` makes of it, or why it failed.
pub fn run_bridge(setup: &Setup, amount: &str, from_key: &str, node: &str, send: bool) -> Result<Value, String> {
    run_bridge_to(setup, amount, from_key, "", node, send)
}

/// `run_bridge` into the public account `to` (`Public/<id>`); empty is the wallet's default one.
pub fn run_bridge_to(setup: &Setup, amount: &str, from_key: &str, to: &str, node: &str, send: bool) -> Result<Value, String> {
    setup.wallet_ready()?;
    // Checked before the wallet is read, so bad input fails the same way every time.
    let node = bridge_node(node)?;
    self::amount(amount)?;
    bridge_key(from_key)?;
    let to = match bridge_to(to)? {
        Some(to) => to.arg(),
        None => default_public(setup)?,
    };
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
/// What `names setup` moves to the private account unless told otherwise (its
/// own default): a few names' deposits and private changes. On LEZ 0.4 each
/// private transaction reserves its fee in the proof, about 5,400,000 at the lowest
/// base fees, and an account that can't cover it can't act.
pub const DEFAULT_SHIELD: &str = "50000000";

/// `names setup --shield=<amount>`: the wallet and its accounts, and, with an
/// amount above zero and that much in the public account, that much moved to
/// the private account. Empty is the CLI's default, 50,000,000.
pub fn setup_args(shield: &str) -> Result<Vec<String>, String> {
    let typed = shield.trim();
    // 0 (or 00) only makes the accounts; anything else is an amount like every other.
    let shield = if typed.is_empty() {
        DEFAULT_SHIELD.to_owned()
    } else if typed.bytes().all(|b| b == b'0') {
        "0".to_owned()
    } else {
        amount_text(typed)?
    };
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
/// `names claim`: takes back a name held on the earlier chain, for free. `snapshot`
/// is the snapshot file the registry's claims list was made from; the CLI checks it
/// against the registry's settings.
pub fn claim_args(name: &str, snapshot: &Path) -> Result<Vec<String>, String> {
    let snapshot = snapshot.to_str().ok_or("the snapshot's path is not text")?;
    Ok(vec!["claim".to_owned(), format!("--snapshot={snapshot}"), "--".to_owned(), registry::parse_name(name)?])
}

/// Fetches the snapshot at `url`, a static file, at most `MAX_SNAPSHOT` bytes.
pub fn fetch_snapshot(url: &str, wait: Duration) -> Result<Vec<u8>, String> {
    let reply = ureq::get(url)
        .timeout(wait.clamp(Duration::from_secs(1), Duration::from_secs(30)))
        .call()
        .map_err(|error| format!("could not fetch the list of names from the earlier chain: {error}"))?;
    let mut body = Vec::new();
    Read::read_to_end(&mut Read::take(reply.into_reader(), MAX_SNAPSHOT + 1), &mut body)
        .map_err(|error| format!("the list of names broke off: {error}"))?;
    if body.len() as u64 > MAX_SNAPSHOT {
        return Err("the list of names is larger than any snapshot".to_owned());
    }
    Ok(body)
}

/// Words that say a wallet could not be read, as the CLI puts it (`OLD_WALLET`).
const OLD_WALLET: &str = "does not load on 0.4";

/// Whether the wallet is one from before LEZ 0.4, which the CLI can't open.
pub fn wallet_is_old(setup: &Setup) -> bool {
    run_capture(setup, vec!["module".to_owned(), "accounts".to_owned()], VERSION_PATIENCE)
        .is_ok_and(|(code, _, err)| code != Some(0) && err.contains(OLD_WALLET))
}

/// Brings a wallet that no longer opens back from its recovery phrase: `{public:
/// [..], private, moved: [..]}`. The old files stay beside the new ones. The phrase
/// goes over standard input, and neither it nor anything printed reaches a log.
pub fn run_restore_wallet(setup: &Setup, phrase: &str) -> Result<Value, String> {
    let phrase = phrase.split_whitespace().collect::<Vec<_>>().join(" ");
    let words = phrase.split(' ').count();
    if !(12..=24).contains(&words) || !phrase.bytes().all(|b| b.is_ascii_lowercase() || b == b' ') {
        return Err("a recovery phrase is 12 to 24 words".to_owned());
    }
    let output = run_with_input(setup, vec!["module".to_owned(), "restore".to_owned()], &format!("{phrase}\n"), SETUP_PATIENCE)?;
    serde_json::from_str(last_line(&output)).map_err(|_| "the names CLI restored the wallet but didn't say its accounts".to_owned())
}

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
        // The CLI groups these figures (5,000,000) since 0.15.x; older ones wrote 5000000.
        let word_after = |key: &str| {
            let (_, rest) = line.split_once(key)?;
            let figure: String = rest.chars().take_while(|c| c.is_ascii_digit() || *c == ',').filter(char::is_ascii_digit).collect();
            (!figure.is_empty()).then_some(figure)
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
        // The "moving ..." line can scroll out of the log's tail behind the wallet's
        // "Stored persistent accounts" lines; "ready:" ends a setup that moved.
        return json!({ "moved": log.contains("moving ") || log.contains("ready: "), "problem": null });
    }
    json!({ "moved": false, "problem": job_problem("shield", log) })
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
    let amount = amount_text(amount)?;
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

// ── Tokens (`names token`, docs/wallet/TOKENS.md) ───────────────────────

/// `names token list` catches the wallet up with the chain first, for its
/// private accounts' notes, then reads each public account and definition.
const TOKEN_LIST_PATIENCE: Duration = Duration::from_secs(150);
/// `names module token info` reads one account from the chain.
const TOKEN_INFO_PATIENCE: Duration = Duration::from_secs(30);
/// The longest token name shown; the prototype program allows 64 bytes.
const MAX_TOKEN_NAME: usize = 64;
/// Most rows of holdings or pending credits believed from one answer.
const MAX_TOKEN_ROWS: usize = 256;

/// The flag naming the token program. `names token` has none built in, and
/// the module passes its own, so the app never shows one program and sends
/// with another.
fn token_program_arg(program: &registry::AccountId) -> String {
    format!("--token-program={}", registry::account_text(program))
}

/// A token's definition, written as its account ID.
pub fn token_definition(definition: &str) -> Result<registry::AccountId, String> {
    let definition = definition.trim();
    let id = registry::parse_account(definition).map_err(|_| "that isn't a token: a token is written as its definition's account ID".to_owned())?;
    if id == registry::NATIVE_TOKEN {
        return Err("that is the native token, LEZ; pay it from Pay".to_owned());
    }
    Ok(id)
}

/// Who a token goes to, written as `names token send --to` takes it: a name,
/// `Public/<id>`, or one of this wallet's own `Private/<id>`. A bare ID is
/// refused, as for a payment, so an ID is never taken for a name.
pub fn token_to(to: &str) -> Result<String, String> {
    let to = to.trim();
    if let Some(id) = to.strip_prefix("Private/") {
        return registry::parse_account(id)
            .ok()
            .filter(|_| !id.contains('/') && !id.contains(char::is_whitespace))
            .map(|id| format!("Private/{}", registry::account_text(&id)))
            .ok_or_else(|| "that isn't a private account; it's Private/ and then the account ID".to_owned());
    }
    match pay_to(to)? {
        PayTo::Name(name) => Ok(name),
        PayTo::Account(id) => Ok(format!("Public/{id}")),
    }
}

/// `names token send`: `amount` of `definition` to `to`, from the wallet's
/// main private account (`from_private`) or its main public one. To a name it
/// goes to the name's private receiving keys unless `to_public` (or the name
/// has only a public account). Everything is checked here first.
pub fn token_send_args(
    program: &registry::AccountId,
    definition: &str,
    amount: &str,
    to: &str,
    from_private: bool,
    to_public: bool,
) -> Result<Vec<String>, String> {
    let definition = token_definition(definition)?;
    let amount = amount_text(amount)?;
    let to = token_to(to)?;
    let mut args = vec![
        token_program_arg(program),
        "token".to_owned(),
        "send".to_owned(),
        format!("--to={to}"),
        format!("--from={}", if from_private { "private" } else { "public" }),
    ];
    if to_public && !to.contains('/') {
        args.push("--public".to_owned());
    }
    args.extend(["--".to_owned(), registry::account_text(&definition), amount]);
    Ok(args)
}

/// `names token receive`: takes every token credit waiting for this wallet's
/// private keys, one proven transaction each.
pub fn token_receive_args(program: &registry::AccountId) -> Vec<String> {
    vec![token_program_arg(program), "token".to_owned(), "receive".to_owned()]
}

/// Text from a token's definition, which whoever made the token wrote:
/// control and direction-changing characters can't reorder what the person reads.
fn token_text(value: &Value) -> Value {
    value.as_str().map_or(Value::Null, |text| Value::from(crate::provider::clean_text(text, MAX_TOKEN_NAME)))
}

/// A decimal amount as the CLI printed it, or None.
fn token_digits(value: &Value) -> Option<String> {
    value.as_str().filter(|text| !text.is_empty() && text.len() <= 39 && text.bytes().all(|b| b.is_ascii_digit())).map(str::to_owned)
}

/// `names token list --json` read back: `{holdings: [{account, account_kind,
/// definition, name, balance}], pending: [{position, definition, name, amount,
/// to}]}`, each row checked, accounts written `Public/<id>` or `Private/<id>`.
pub fn parse_token_list(output: &str) -> Result<Value, String> {
    let answer: Value = serde_json::from_str(last_line(output)).map_err(|_| "the names CLI's token list did not decode".to_owned())?;
    let rows = |key: &str| answer[key].as_array().map(Vec::as_slice).unwrap_or_default().iter().take(MAX_TOKEN_ROWS).cloned().collect::<Vec<_>>();
    let id = |value: &Value| value.as_str().and_then(|text| registry::parse_account(text).ok()).map(|id| registry::account_text(&id));
    let holdings: Vec<Value> = rows("holdings")
        .iter()
        .filter_map(|row| {
            let (private, account) = crate::provider::account_ref(row["account"].as_str()?).ok()?;
            Some(json!({
                "account": crate::provider::account_written(private, &account),
                "account_kind": if private { "private" } else { "public" },
                "definition": id(&row["definition"])?,
                "name": token_text(&row["name"]),
                "balance": token_digits(&row["balance"])?,
            }))
        })
        .collect();
    let pending: Vec<Value> = rows("pending")
        .iter()
        .filter_map(|row| {
            Some(json!({
                "position": row["position"].as_u64()?,
                "definition": id(&row["definition"])?,
                "name": token_text(&row["name"]),
                "amount": token_digits(&row["amount"])?,
                "to": format!("Private/{}", id(&row["to"])?),
            }))
        })
        .collect();
    Ok(json!({ "holdings": holdings, "pending": pending }))
}

/// Runs `names token list --json`: what every account of the wallet holds,
/// and the credits waiting to be received.
pub fn run_token_list(setup: &Setup, program: &registry::AccountId) -> Result<Value, String> {
    setup.wallet_ready()?;
    let args = vec![token_program_arg(program), "token".to_owned(), "list".to_owned(), "--json".to_owned()];
    parse_token_list(&run_text(setup, args, TOKEN_LIST_PATIENCE)?)
}

/// Runs `names module token info`: `{definition, name, supply, authority}`.
pub fn run_token_info(setup: &Setup, program: &registry::AccountId, definition: &registry::AccountId) -> Result<Value, String> {
    let args = vec![
        token_program_arg(program),
        "module".to_owned(),
        "token".to_owned(),
        "info".to_owned(),
        "--".to_owned(),
        registry::account_text(definition),
    ];
    let answer: Value = serde_json::from_str(last_line(&run_text(setup, args, TOKEN_INFO_PATIENCE)?))
        .map_err(|_| "the names CLI's token info did not decode".to_owned())?;
    let supply = token_digits(&answer["supply"]).ok_or("the names CLI gave no token supply")?;
    let authority = answer["authority"].as_str().and_then(|text| registry::parse_account(text).ok()).map(|id| registry::account_text(&id));
    Ok(json!({
        "definition": registry::account_text(definition),
        "name": token_text(&answer["name"]),
        "supply": supply,
        "authority": authority,
    }))
}

/// What a finished token job printed: the last transaction it sent and the
/// block it was applied in, how many were applied, and whether there was
/// nothing to receive.
pub fn token_outcome(log: &str) -> Value {
    let tx = log.lines().rev().find_map(|line| line.trim().strip_prefix("submitted ")).map(str::trim);
    let blocks: Vec<u64> = log.lines().filter_map(|line| line.trim().strip_prefix("applied in block ")?.trim().parse().ok()).collect();
    json!({
        "tx": tx,
        "block": blocks.last(),
        "applied": blocks.len(),
        "nothing": log.lines().any(|line| line.trim() == "nothing to receive"),
    })
}

/// Whether a `names receive` run claimed nothing: it said so ("nothing to receive", or
/// "nothing worth claiming" when every credit was worth less than claiming it).
pub fn claimed_nothing(log: &str) -> bool {
    log.lines().map(str::trim).any(|line| line == "nothing to receive" || line == "nothing worth claiming")
}

/// What a `names receive` run claimed, from its log: the private account that paid the fee
/// (`fee paid from Private/<id>`) and each claim that landed, `(Private/<id>, amount)`, from
/// `receiving N into Private/<id>` followed by `received in block …`.
pub fn parse_claims(log: &str) -> (Option<String>, Vec<(String, u128)>) {
    let mut payer = None;
    let mut claims = Vec::new();
    let mut going: Option<(String, u128)> = None;
    for line in log.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("receiving ") {
            let mut words = rest.split_whitespace();
            let amount = words.next().and_then(|amount| amount.parse::<u128>().ok());
            let into = words.nth(1).map(|to| to.trim_end_matches(';'));
            going = amount.zip(into.filter(|to| to.starts_with("Private/") && registry::parse_account(to).is_ok())).map(|(amount, to)| (to.to_owned(), amount));
        } else if line.starts_with("received in block ") {
            claims.extend(going.take());
        } else if let Some(account) = line.strip_prefix("fee paid from ") {
            if account.starts_with("Private/") && registry::parse_account(account).is_ok() {
                payer = Some(account.to_owned());
            }
        }
    }
    (payer, claims)
}

/// What a `names receive` run left waiting, from its log: `(Private/<id>, amount)` for each
/// `left waiting: N to Private/<id>: …` line, whether it was worth less than claiming it, its
/// payer couldn't also pay for it, or the run stopped before it. A fee's refund is not a
/// payment left waiting: the claims' payer sweeps its own refunds into its next proof, so
/// one listed before the claims may be spent by them (`names balance` tells refunds apart).
pub fn parse_left(log: &str) -> Vec<(String, u128)> {
    log.lines()
        .map(str::trim)
        .filter_map(|line| {
            let rest = line.strip_prefix("left waiting: ")?;
            if rest.contains(", a fee's refund") {
                return None;
            }
            let mut words = rest.split_whitespace();
            let amount = words.next()?.parse::<u128>().ok()?;
            let to = words.nth(1)?.trim_end_matches([':', ',']);
            (to.starts_with("Private/") && registry::parse_account(to).is_ok()).then(|| (to.to_owned(), amount))
        })
        .collect()
}

/// The fee lines a CLI run printed (`cli/src/fees.rs` in the names CLI): `{about, at_most,
/// paid}`, decimal text, or null if it printed none. A run that sent several changes
/// prints one estimate and one fee paid for each; they are added up. `paid` is null
/// until a change has applied, and when the CLI could not tell it.
pub fn parse_fee(log: &str) -> Value {
    let mut about: Option<u128> = None;
    let mut at_most: Option<u128> = None;
    let mut paid: Option<u128> = None;
    let add = |sum: &mut Option<u128>, text: &str| {
        if let Ok(amount) = text.trim().parse::<u128>() {
            *sum = Some(sum.unwrap_or(0).saturating_add(amount));
        }
    };
    for line in log.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("fee paid: ") {
            add(&mut paid, rest);
        } else if let Some(rest) = line.strip_prefix("fee: about ") {
            let Some((first, rest)) = rest.split_once(", at most ") else { continue };
            let most = rest.split_whitespace().next().unwrap_or_default();
            if first.trim().parse::<u128>().is_ok() && most.parse::<u128>().is_ok() {
                add(&mut about, first);
                add(&mut at_most, most);
            }
        }
    }
    if about.is_none() && paid.is_none() {
        return Value::Null;
    }
    let text = |amount: Option<u128>| amount.map_or(Value::Null, |amount| Value::from(amount.to_string()));
    json!({ "about": text(about), "at_most": text(at_most), "paid": text(paid) })
}

/// A nickname for an account, trimmed: at most [`MAX_LABEL`] characters on one line.
pub fn account_label(label: &str) -> Result<String, String> {
    let label = label.trim();
    if label.chars().count() > MAX_LABEL {
        return Err(format!("a nickname is at most {MAX_LABEL} characters"));
    }
    if label.chars().any(char::is_control) {
        return Err("a nickname is one line of text".to_owned());
    }
    Ok(label.to_owned())
}

/// The CLI's JSON answer: its last line.
fn answer(output: &str, what: &str) -> Result<Value, String> {
    serde_json::from_str(last_line(output)).map_err(|_| format!("the names CLI's {what} did not decode"))
}

/// An `account-list` entry as the CLI gave it, checked: `{account, kind, label, balance,
/// default, public_key, npk}`. `npk` (hex, private accounts only, null from a CLI too old to
/// give it) is the key a name's profile publishes to be paid there.
fn account_entry(entry: &Value) -> Result<Value, String> {
    let account = entry["account"].as_str().ok_or("an account without an ID")?;
    let (private, id) = crate::provider::account_ref(account)?;
    let digits = |value: &Value| value.as_str().filter(|text| !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())).map(str::to_owned);
    let hex64 = |value: &Value| value.as_str().filter(|key| key.len() == 64 && key.bytes().all(|b| b.is_ascii_hexdigit())).map(str::to_ascii_lowercase);
    let key = hex64(&entry["public_key"]);
    let npk = hex64(&entry["npk"]);
    Ok(json!({
        "account": crate::provider::account_written(private, &id),
        "kind": if private { "private" } else { "public" },
        "label": entry["label"].as_str().and_then(|label| account_label(label).ok()).filter(|label| !label.is_empty()),
        "balance": digits(&entry["balance"]),
        "default": entry["default"] == true,
        "public_key": if private { None } else { key },
        "npk": if private { npk } else { None },
    }))
}

/// How long a call waits for a slow read (the CLI catching the wallet up with the chain)
/// before it answers that the read goes on: the host gives a call 20 s.
#[cfg(not(test))]
const CALL_WAIT: Duration = Duration::from_secs(12);
#[cfg(test)]
const CALL_WAIT: Duration = Duration::from_secs(5);
/// A call's whole time for one wallet read: the CLI's run and the module's own checks of
/// what it printed, inside the host's 20 s (balance-probe-after-12s-wait-exceeds-host-timeout).
#[cfg(not(test))]
pub const CALL_BUDGET: Duration = Duration::from_secs(16);
#[cfg(test)]
pub const CALL_BUDGET: Duration = Duration::from_secs(8);
/// How long a slow read's result, nobody having taken it, still answers the next call.
const SLOW_RESULT_FRESH: Duration = Duration::from_secs(120);

/// What a call that waited `CALL_WAIT` for a slow read answers while the sequencer answers.
pub const STILL_READING: &str = "The wallet is still catching up with the chain; this answers in a moment. Try again shortly.";

/// What the CLI said: its exit code, what it printed, and what it wrote to stderr.
type CliOutput = (Option<i32>, String, String);

/// A health check of the sequencer (`getLastBlockId`) on its own thread, started with a
/// wallet read. A read that can't finish within a call is told apart by it: a wallet
/// catching up with a sequencer that answers, or a sequencer that doesn't answer
/// (still-reading-message-wrong-for-a-hung-sequencer).
pub struct Probe(std::sync::Arc<(Mutex<Option<Result<(), String>>>, std::sync::Condvar)>);

impl Probe {
    #[cfg(not(test))]
    pub fn start(sequencer: &str) -> Self {
        let probe = Probe(std::sync::Arc::new((Mutex::new(None), std::sync::Condvar::new())));
        let (slot, sequencer) = (probe.0.clone(), sequencer.to_owned());
        std::thread::spawn(move || {
            let answer = registry::sequencer_reachable(&sequencer);
            *slot.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(answer);
            slot.1.notify_all();
        });
        probe
    }

    /// Tests' sequencer is a stand-in on the test's own thread: asked there, at once.
    #[cfg(test)]
    pub fn start(sequencer: &str) -> Self {
        Probe(std::sync::Arc::new((Mutex::new(Some(registry::sequencer_reachable(sequencer))), std::sync::Condvar::new())))
    }

    /// The check's answer, waiting for it until `until` at most; None if it hasn't come.
    pub fn answer(&self, until: Instant) -> Option<Result<(), String>> {
        let (slot, ended) = &*self.0;
        let guard = slot.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let wait = until.saturating_duration_since(Instant::now());
        let (guard, _) = ended.wait_timeout_while(guard, wait, |answer| answer.is_none()).unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.clone()
    }

    /// Why the sequencer didn't answer the check, if it didn't (waiting until `until`).
    pub fn problem(&self, until: Instant) -> Option<String> {
        self.answer(until).and_then(Result::err)
    }
}

/// A wallet read on its own thread: its result once it ends, and when; and whether a call
/// asked it to stop (the sequencer doesn't answer, so it can only hang or read wrong).
struct SlowRead {
    result: Mutex<Option<(Instant, Result<CliOutput, String>)>>,
    ended: std::sync::Condvar,
    stop: std::sync::atomic::AtomicBool,
}

/// The slow reads under way or ended and not yet taken, by wallet, sequencer and command.
static SLOW_READS: Mutex<std::collections::BTreeMap<String, std::sync::Arc<SlowRead>>> = Mutex::new(std::collections::BTreeMap::new());

/// Runs the CLI with `command` for a wallet read (the first `names balance` of a wallet
/// behind the chain can take minutes) on its own thread. A call waits for it until
/// `CALL_WAIT` or `deadline`, whichever comes first. Past that it answers without it:
/// `STILL_READING` while the sequencer answers `probe`, and the read goes on, its result
/// kept for the next call; "could not reach the sequencer" when it doesn't, and the read
/// is stopped. A read that ended in an error or a failed exit is never kept for a later
/// call, which reads afresh (slow-read-error-survives-recovery).
fn run_bounded(setup: &Setup, command: Vec<String>, patience: Duration, deadline: registry::Deadline, probe: &Probe) -> Result<CliOutput, String> {
    let key = format!("{}\u{0}{}\u{0}{}\u{0}{}", setup.program.display(), setup.wallet.display(), setup.sequencer, command.join("\u{0}"));
    let read = {
        let mut reads = SLOW_READS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let unusable = reads.get(&key).is_some_and(|read| {
            read.result.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_ref().is_some_and(|(at, result)| {
                at.elapsed() > SLOW_RESULT_FRESH || !matches!(result, Ok((Some(0), _, _)))
            })
        });
        if unusable {
            reads.remove(&key);
        }
        reads
            .entry(key.clone())
            .or_insert_with(|| {
                let read = std::sync::Arc::new(SlowRead {
                    result: Mutex::new(None),
                    ended: std::sync::Condvar::new(),
                    stop: std::sync::atomic::AtomicBool::new(false),
                });
                let (setup, command, slot) = (setup.clone(), command, read.clone());
                std::thread::spawn(move || {
                    let result = feed(setup.command(command), &setup.program, "", patience, Some(&slot.stop));
                    *slot.result.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some((Instant::now(), result));
                    slot.ended.notify_all();
                });
                read
            })
            .clone()
    };
    let forget = || {
        let mut reads = SLOW_READS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if reads.get(&key).is_some_and(|kept| std::sync::Arc::ptr_eq(kept, &read)) {
            reads.remove(&key);
        }
    };
    let wait = (Instant::now() + CALL_WAIT).min(deadline.at).saturating_duration_since(Instant::now());
    let guard = read.result.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let (mut guard, _) = read.ended.wait_timeout_while(guard, wait, |result| result.is_none()).unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((_, result)) = guard.take() {
        drop(guard);
        forget();
        return result;
    }
    drop(guard);
    // The check started with the read and waits `PROBE_WAIT` at most: it has answered.
    match probe.answer(Instant::now() + Duration::from_secs(1)) {
        Some(Err(why)) => {
            read.stop.store(true, std::sync::atomic::Ordering::SeqCst);
            forget();
            Err(why)
        }
        _ => Err(STILL_READING.to_owned()),
    }
}

/// An account as a sentence names it: `Public/CCVd…pKnk`.
pub fn short_account(account: &str) -> String {
    let (prefix, id) = account.split_once('/').unwrap_or(("", account));
    let chars: Vec<char> = id.chars().collect();
    let id = if chars.len() > 10 {
        format!("{}…{}", chars[..4].iter().collect::<String>(), chars[chars.len() - 4..].iter().collect::<String>())
    } else {
        id.to_owned()
    };
    if prefix.is_empty() { id } else { format!("{prefix}/{id}") }
}

/// Up to three accounts or names in a sentence, then how many more.
fn listed(items: &[String]) -> String {
    let shown: Vec<&str> = items.iter().take(3).map(String::as_str).collect();
    match items.len() {
        0..=3 => shown.join(", "),
        more => format!("{} and {} more", shown.join(", "), more - 3),
    }
}

/// A reason without its closing full stop, to go inside a sentence.
fn clause(why: &str) -> &str {
    why.trim().trim_end_matches(['.', ' '])
}

/// The public accounts among `rows` (account-list entries) whose balance is unknown: the
/// sequencer didn't give it.
fn unread_public(rows: &[Value]) -> Vec<String> {
    rows.iter()
        .filter(|row| row["kind"] == "public" && row["balance"].is_null())
        .filter_map(|row| row["account"].as_str().map(str::to_owned))
        .collect()
}

/// What a read in which some accounts' balances are unknown says beside them.
fn unread_warning(unread: &[String], why: Option<&str>) -> Option<String> {
    if unread.is_empty() {
        return None;
    }
    let shown: Vec<String> = unread.iter().map(|account| short_account(account)).collect();
    let what = if unread.len() == 1 { "its balance shows" } else { "their balances show" };
    Some(format!(
        "The sequencer didn't give the balance of {} ({}), so {what} as unknown. Try again shortly.",
        listed(&shown),
        clause(why.unwrap_or("it didn't answer"))
    ))
}

/// The public rows that read 0 or unknown (null), read again from the sequencer by the
/// module itself, a few at a time, before `deadline`. The October CLI prints 0 for a
/// balance it could not read and exits 0, so its 0 is believed only when the sequencer says
/// so too; a row the sequencer doesn't give becomes unknown (null), and the read's own
/// reason is returned (selective-degradation-cold-read-false-arrivals,
/// balance-accounts-zeroed-with-success-on-partial-failure). An unknown row is read too, so
/// it says why in the sequencer's words, never a bare "it didn't answer", and holds what the
/// sequencer gives once it answers (cli-balance-bare-it-didnt-answer).
fn recheck_zeros(rows: &mut [Value], sequencer: &str, deadline: registry::Deadline) -> Option<String> {
    recheck_public(rows, sequencer, deadline, true)
}

/// `recheck_zeros`, for the unknown rows only (`zeros` false: a CLI whose 0 is a reading).
fn recheck_public(rows: &mut [Value], sequencer: &str, deadline: registry::Deadline, zeros: bool) -> Option<String> {
    let zeros: Vec<usize> = (0..rows.len())
        .filter(|at| rows[*at]["kind"] == "public" && (rows[*at]["balance"].is_null() || (zeros && rows[*at]["balance"] == "0")))
        .collect();
    let reader = registry::Registry::by(sequencer, registry::NATIVE_TOKEN, deadline);
    let mut why = None;
    for batch in zeros.chunks(8) {
        let answers: Vec<Result<u128, String>> = std::thread::scope(|scope| {
            let readers: Vec<_> = batch
                .iter()
                .map(|at| {
                    let account = rows[*at]["account"].as_str().and_then(|text| crate::provider::account_ref(text).ok()).map(|(_, id)| id);
                    let reader = &reader;
                    scope.spawn(move || account.ok_or_else(|| "not an account".to_owned()).and_then(|id| reader.balance(&id)))
                })
                .collect();
            readers.into_iter().map(|reader| reader.join().unwrap_or_else(|_| Err("a read failed".to_owned()))).collect()
        });
        for (at, answer) in batch.iter().zip(answers) {
            match answer {
                Ok(balance) => rows[*at]["balance"] = Value::from(balance.to_string()),
                Err(problem) => {
                    rows[*at]["balance"] = Value::Null;
                    why.get_or_insert(problem);
                }
            }
        }
    }
    why
}

/// Every account of a wallet as read now: the rows (`account-list` entries, a balance null
/// where the sequencer didn't give it), which accounts those are, and why.
#[derive(Debug, Clone)]
pub struct AccountsRead {
    pub rows: Vec<Value>,
    pub unread: Vec<String>,
    pub why: Option<String>,
}

impl AccountsRead {
    /// What goes beside the rows in a reply: nothing when every account was read, else
    /// `{partial, unread, warning}`.
    pub fn marks(&self) -> Value {
        match unread_warning(&self.unread, self.why.as_deref()) {
            None => Value::Null,
            Some(warning) => json!({ "partial": true, "unread": self.unread, "warning": warning }),
        }
    }
}

/// `names module account-list`: every account of the wallet, the public ones' balances as
/// the sequencer gave them (null where it didn't), and which those are.
pub fn run_account_list(setup: &Setup) -> Result<AccountsRead, String> {
    setup.wallet_ready()?;
    let deadline = registry::Deadline::after(CALL_BUDGET);
    let probe = Probe::start(&setup.sequencer);
    let (code, out, err) = run_bounded(setup, vec!["module".to_owned(), "account-list".to_owned()], ACCOUNTS_PATIENCE, deadline, &probe)?;
    if code != Some(0) {
        let problem = failure(&err, &out);
        return Err(if problem == OLDER_CLI { problem } else { probe.problem(deadline.at).unwrap_or(problem) });
    }
    let list = answer(&out, "account list")?;
    let mut rows: Vec<Value> = list.as_array().ok_or("the names CLI's account list is not a list")?.iter().map(account_entry).collect::<Result<_, _>>()?;
    // A balance the CLI couldn't read is read again here: the sequencer's own answer, or its
    // own reason, never a bare "it didn't answer".
    let mut why = None;
    if !unread_public(&rows).is_empty() {
        why = probe.problem(deadline.at);
        if let Some(problem) = recheck_public(&mut rows, &setup.sequencer, deadline, false) {
            why.get_or_insert(problem);
        }
    }
    let unread = unread_public(&rows);
    let why = if unread.is_empty() { None } else { why };
    Ok(AccountsRead { rows, unread, why })
}

/// `names module account-new`: a new `public` or `private` account.
pub fn run_account_new(setup: &Setup, kind: &str, label: &str) -> Result<Value, String> {
    setup.wallet_ready()?;
    if kind != "public" && kind != "private" {
        return Err("an account is public or private".to_owned());
    }
    let label = account_label(label)?;
    // `--label=` keeps a nickname that starts with `-` from reading as an option.
    let args = vec!["module".to_owned(), "account-new".to_owned(), format!("--label={label}"), kind.to_owned()];
    account_entry(&answer(&run_text(setup, args, QUICK_PATIENCE)?, "new account")?)
}

/// `names module account-label`: gives `account` a nickname, or clears it.
pub fn run_account_label(setup: &Setup, account: &str, label: &str) -> Result<Value, String> {
    setup.wallet_ready()?;
    let (private, id) = crate::provider::account_ref(account)?;
    let label = account_label(label)?;
    let args = vec![
        "module".to_owned(),
        "account-label".to_owned(),
        "--".to_owned(),
        crate::provider::account_written(private, &id),
        label,
    ];
    answer(&run_text(setup, args, QUICK_PATIENCE)?, "nickname")
}

/// `names module account-use`: pays and registers from `account` from now on.
pub fn run_account_use(setup: &Setup, account: &str) -> Result<Value, String> {
    setup.wallet_ready()?;
    let (private, id) = crate::provider::account_ref(account)?;
    let args = vec!["module".to_owned(), "account-use".to_owned(), "--".to_owned(), crate::provider::account_written(private, &id)];
    account_entry(&answer(&run_text(setup, args, QUICK_PATIENCE)?, "account switch")?)
}

/// `names module length-proof <id>`: where the name identifier `id` sits relative to the
/// locked-name list, as the CLI's JSON. Needs no wallet; the CLI builds the list's tree, a
/// couple of seconds.
#[cfg_attr(test, allow(dead_code))]
pub fn run_length_proof(setup: &Setup, id: &[u8; 32]) -> Result<Value, String> {
    let args = vec!["module".to_owned(), "length-proof".to_owned(), crate::registry::hex(id)];
    // The first one downloads the list (about 61 MB), once.
    answer(&run_text(setup, args, SHORTLIST_PATIENCE)?, "length proof")
}

/// `names module fee`: the chain's fee market and what a transfer costs at it.
pub fn run_fee(setup: &Setup) -> Result<Value, String> {
    let quote = answer(&run_text(setup, vec!["module".to_owned(), "fee".to_owned()], QUICK_PATIENCE)?, "fee quote")?;
    let digits = |value: &Value| value.as_str().filter(|text| !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())).map(str::to_owned);
    let estimate = |value: &Value| -> Result<Value, String> {
        Ok(json!({
            "about": digits(&value["about"]).ok_or("the fee quote has no estimate")?,
            "at_most": digits(&value["at_most"]).ok_or("the fee quote has no estimate")?,
        }))
    };
    Ok(json!({
        "height": quote["height"].as_u64(),
        "base_fee_exec": quote["base_fee_exec"].as_u64(),
        "base_fee_stor": quote["base_fee_stor"].as_u64(),
        "public_transfer": estimate(&quote["public_transfer"])?,
        "private_transfer": estimate(&quote["private_transfer"])?,
    }))
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

/// `names transfer <name> <new holder>`: the new holder is a LEZ account, `Public/…` or
/// `Private/…` and its ID, written once. Only a tag of it reaches the chain, and a
/// hand-over can't be undone, so a bare ID (public or private?), the empty account and
/// an ID re-typed in one case are refused.
pub fn transfer_args(name: &str, new_owner: &str) -> Result<Vec<String>, String> {
    let name = registry::parse_name(name)?;
    let owner = new_owner.trim();
    let (kind, id) = match (owner.strip_prefix("Public/"), owner.strip_prefix("Private/")) {
        (Some(id), _) => ("Public", id),
        (_, Some(id)) => ("Private", id),
        _ if registry::parse_account(owner).is_ok() => {
            return Err("write Public/ or Private/ before the new holder's ID, so it is clear which kind of account gets the name".to_owned())
        }
        _ => return Err("the new holder is a LEZ account: Public/ or Private/ and its ID".to_owned()),
    };
    let account = registry::bare_account(id).map_err(|error| {
        if id.contains('/') { error } else { "the new holder is a LEZ account: Public/ or Private/ and its ID".to_owned() }
    })?;
    registry::plausible_holder(id, &account)?;
    Ok(vec!["transfer".to_owned(), "--".to_owned(), name, format!("{kind}/{}", registry::account_text(&account))])
}

/// `names receive-privately <name>`: payments to a name this wallet holds (one handed
/// to it, say) start arriving at new receiving keys of its own.
pub fn receive_privately_args(name: &str) -> Result<Vec<String>, String> {
    Ok(vec!["receive-privately".to_owned(), "--".to_owned(), registry::parse_name(name)?])
}

/// `names set-text`: an empty value removes the entry.
pub fn set_text_args(name: &str, key: &str, value: &str) -> Result<Vec<String>, String> {
    let name = registry::parse_name(name)?;
    let key = key.trim();
    check_text(key, value)?;
    plain_text(value, "an entry")?;
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
    // A name asked to be public that is still private: why, by name (`<name>  not public yet: …`).
    let mut not_public = std::collections::BTreeMap::new();
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
        if let Some(why) = state.strip_prefix("not public yet: ") {
            not_public.insert(name, why.to_owned());
            continue;
        }
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
                "not_public": null,
            }));
        }
    }
    // Registered and held privately though asked to be public (register-as-public-silently-
    // stays-private): `not_public` says why, in the CLI's words; null for every other name.
    for entry in &mut names {
        if let Some(why) = entry["name"].as_str().and_then(|name| not_public.get(name)) {
            entry["not_public"] = Value::from(why.clone());
        }
    }
    names
}

/// Why a CLI run that failed failed, in one line built from its error: the `Error:` line
/// (the CLI's own words), never the last line of its cause chain ("2: Connection refused
/// (os error 61)"). When the causes show the sequencer couldn't be reached at all, that is
/// what it says, with what to do (transfer-failure-text-is-last-cause-line).
pub fn failure_cause(log: &str) -> String {
    let lines: Vec<&str> = log.lines().map(str::trim).filter(|line| !line.is_empty()).collect();
    let error = lines.iter().position(|line| line.starts_with("Error:"));
    // The error and the causes under it; without one, the last thing said.
    let told = match error {
        Some(at) => &lines[at..],
        None => &lines[lines.len().saturating_sub(1)..],
    };
    const UNREACHABLE: [&str; 6] = ["Connection refused", "tcp connect error", "client error (Connect)", "dns error", "cannot reach the sequencer", "Network is unreachable"];
    if told.iter().any(|line| UNREACHABLE.iter().any(|mark| line.contains(mark))) {
        return "couldn't reach the sequencer. Check that it is running and reachable, then try again".to_owned();
    }
    let first = told.first().map_or("", |line| line.trim_start_matches("Error:").trim());
    if first.is_empty() { "it stopped without saying why".to_owned() } else { first.to_owned() }
}

/// What a CLI job of `kind` that failed says: what didn't happen, then why
/// ([`failure_cause`]). Every job kind fails the same way.
pub fn job_problem(kind: &str, log: &str) -> String {
    let what = match kind {
        "pay" | "pay_account" => "The payment didn't go through",
        "token_send" => "The token transfer didn't go through",
        "token_receive" => "Receiving didn't finish",
        "receive" => "Claiming didn't finish",
        "shield" => "Moving to private didn't go through",
        "transfer" => "Giving the name away didn't go through",
        "register" => "Registering the name didn't finish",
        "claim" => "Taking the name back didn't finish",
        "receive_privately" => "Setting up private payments didn't finish",
        "profile" | "set_text" => "Saving the profile didn't finish",
        "set_page" => "Publishing the page didn't finish",
        "make_public" => "Making it public didn't finish",
        "post" => "Posting didn't go through",
        "like" | "hide" => "It didn't go through",
        _ => "It didn't go through",
    };
    format!("{what}: {}", failure_cause(log))
}

/// The last line of `text` that says something.
pub fn last_line(text: &str) -> &str {
    text.lines().rev().map(str::trim).find(|line| !line.is_empty()).unwrap_or("")
}

/// Waits for `child`, stopping it after `patience`. The exit code, or why there is none.
fn wait(child: &mut Child, patience: Duration) -> Result<Option<i32>, String> {
    wait_or_stop(child, patience, None)
}

/// `wait`, also stopping the child as soon as `stop` is set (a read whose sequencer
/// stopped answering).
fn wait_or_stop(child: &mut Child, patience: Duration, stop: Option<&std::sync::atomic::AtomicBool>) -> Result<Option<i32>, String> {
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status.code()),
            Ok(None) if stop.is_some_and(|stop| stop.load(std::sync::atomic::Ordering::SeqCst)) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("the read was stopped: the sequencer didn't answer".to_owned());
            }
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

/// The names `names mine` printed it could not read (`unknown`, from a CLI that says so)
/// and those it called gone (`no longer held by this wallet`), in that order.
pub fn mine_doubts(output: &str) -> (Vec<String>, Vec<String>) {
    let (mut unknown, mut gone) = (Vec::new(), Vec::new());
    for line in output.lines() {
        let Some((shown, state)) = line.trim().split_once("  ") else { continue };
        let bare = shown.strip_suffix(registry::LEGACY_SUFFIX).unwrap_or(shown);
        let Ok(name) = registry::parse_name(bare) else { continue };
        if name != bare {
            continue;
        }
        let state = state.trim();
        if state.starts_with("unknown") {
            unknown.push(name);
        } else if state.starts_with("no longer held by this wallet") {
            gone.push(name);
        }
    }
    (unknown, gone)
}

/// The accounts `names wallet account list` names, as `Public/<id>` or `Private/<id>`, in order.
pub fn listed_accounts(output: &str) -> Vec<String> {
    output
        .split_whitespace()
        .filter_map(|word| {
            let word = word.trim_end_matches([',', ')']);
            let (kind, id) = word.split_once('/')?;
            (matches!(kind, "Public" | "Private") && registry::parse_account(id).is_ok()).then(|| word.to_owned())
        })
        .collect()
}

/// The wallet's accounts, as `names wallet account list` names them (`Public/…`, `Private/…`),
/// asked by `deadline`. The list is the wallet's own: no read of the sequencer.
pub fn wallet_accounts(setup: &Setup, deadline: registry::Deadline) -> Result<Vec<String>, String> {
    let patience = deadline.left().clamp(Duration::from_secs(1), QUICK_PATIENCE);
    let listed = run_text(setup, vec!["wallet".to_owned(), "account".to_owned(), "list".to_owned()], patience)?;
    Ok(listed_accounts(&listed))
}

/// A time in milliseconds since the Unix epoch as the CLI writes it: `2026-11-09 17:05:00Z`.
pub fn readable(time: u64) -> String {
    let seconds = time / 1000;
    // Days to a civil date, counting from 0000-03-01 so leap days fall at a year's end.
    let days = seconds / 86_400 + 719_468;
    let (era, day_of_era) = (days / 146_097, days % 146_097);
    let year_of_era = (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 { shifted_month + 3 } else { shifted_month - 9 };
    let year = year_of_era + era * 400 + u64::from(month <= 2);
    let (hour, minute, second) = (seconds / 3600 % 24, seconds / 60 % 60, seconds % 60);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}Z")
}

/// A name the wallet holds as `names mine` lists it, made by the module from the record it
/// read itself: `holder` is the account (`Public/…` or `Private/…`) whose tag the record holds.
pub fn held_entry(name: &str, record: &registry::Resolved, holder: &str, now: u64) -> Value {
    let lapsed = record.expires != registry::NEVER && now >= record.expires;
    let state = if record.expires == registry::NEVER {
        "held by a deposit (no expiry)".to_owned()
    } else {
        format!("{} until {}", if lapsed { "lapsed" } else { "held" }, readable(record.expires))
    };
    json!({
        "name": name,
        "display": format!("{name}{}", registry::NAME_SUFFIX),
        "signer": "cli",
        "state": state,
        "lapsed": lapsed,
        "registering": false,
        "failed": false,
        "private": holder.starts_with("Private/"),
        "not_public": null,
    })
}

/// What the module makes of the names the CLI couldn't vouch for (`unknown`, or "no longer
/// held by this wallet"), from their records as the sequencer gives them to the module itself.
#[derive(Debug, Default)]
struct Confirmed {
    /// The names the wallet does hold, as `names mine` lists them.
    held: Vec<Value>,
    /// The names whose record the sequencer didn't give, and why.
    unread: Vec<String>,
    why: Option<String>,
}

/// Reads the record of each of `doubts` by `deadline`. A name with no record, or held by
/// another account, is gone; one held by an account of this wallet is listed from its record:
/// the October CLI calls every name it couldn't read or decode "no longer held by this wallet"
/// and exits 0, so that word alone is never taken (cli-mine-empty-list-when-account-views-fail),
/// and a record the module reads is never blamed on the sequencer
/// (october-cli-mine-blames-the-sequencer). Fails only when the wallet's accounts can't be listed.
fn confirm_names(setup: &Setup, doubts: &[String], deadline: registry::Deadline) -> Result<Confirmed, String> {
    let mut confirmed = Confirmed::default();
    if doubts.is_empty() {
        return Ok(confirmed);
    }
    let Some(program) = setup.registry.as_deref().and_then(|program| registry::parse_account(program).ok()) else {
        confirmed.unread = doubts.to_vec();
        confirmed.why = Some("no registry is set to read them from".to_owned());
        return Ok(confirmed);
    };
    let reader = registry::Registry::by(&setup.sequencer, program, deadline);
    let mut accounts: Option<Vec<String>> = None;
    for name in doubts {
        let derived = registry::derive(name);
        match reader.record(name, &derived) {
            Ok(None) => {}
            Ok(Some(record)) if record.gone(registry::local_now()) => {}
            Ok(Some(record)) => {
                if accounts.is_none() {
                    accounts = Some(wallet_accounts(setup, deadline).map_err(|problem| format!("the names CLI couldn't list this wallet's accounts: {}", clause(&problem)))?);
                }
                let holder = accounts.iter().flatten().find(|account| {
                    account.split_once('/').and_then(|(_, id)| registry::parse_account(id).ok()).is_some_and(|id| record.is_held_by(&derived.id, &id))
                });
                if let Some(holder) = holder {
                    confirmed.held.push(held_entry(name, &record, holder, registry::local_now()));
                }
            }
            Err(problem) => {
                confirmed.unread.push(name.clone());
                confirmed.why.get_or_insert(problem);
            }
        }
    }
    Ok(confirmed)
}

/// Runs `names mine` and returns the names it holds. A wallet directory with no wallet in
/// it holds none, and is left as it is. Each name the CLI couldn't vouch for is read by the
/// module itself (`confirm_names`); one the sequencer doesn't give is no answer, not a name
/// gone: the read fails, naming them, and the last list stays.
pub fn run_mine(setup: &Setup) -> Result<Vec<Value>, String> {
    if setup.wallet_ready().is_err() {
        return Ok(Vec::new());
    }
    let deadline = registry::Deadline::after(CALL_BUDGET);
    let probe = Probe::start(&setup.sequencer);
    let (code, out, err) = run_bounded(setup, mine_args(), MINE_PATIENCE, deadline, &probe)?;
    let (unknown, gone) = mine_doubts(&out);
    if code != Some(0) && unknown.is_empty() {
        return Err(probe.problem(deadline.at).unwrap_or_else(|| failure(&err, &out)));
    }
    let doubts: Vec<String> = unknown.into_iter().chain(gone).collect();
    let confirmed = confirm_names(setup, &doubts, deadline).map_err(|why| format!("Your names couldn't be read now: {why}. Try again shortly"))?;
    if !confirmed.unread.is_empty() {
        let why = confirmed.why.or_else(|| probe.problem(deadline.at));
        return Err(format!(
            "The sequencer didn't give the record of {} ({}), so your names couldn't be read now. Try again shortly",
            listed(&confirmed.unread),
            clause(why.as_deref().unwrap_or("it didn't answer"))
        ));
    }
    let mut names = parse_mine(&out);
    for entry in confirmed.held {
        if !names.iter().any(|known| known["name"] == entry["name"]) {
            names.push(entry);
        }
    }
    Ok(names)
}

/// Runs `names balance` and returns the default private account and its balance:
/// `{account, balance, public}` (decimal text), or null if there is none. `public` is the
/// default public account the same way, or null. A balance the sequencer didn't give is null.
pub fn run_balance(setup: &Setup) -> Result<Value, String> {
    setup.wallet_ready()?;
    Ok(balance_read(setup, registry::Deadline::after(CALL_BUDGET))?.reply())
}

/// `run_balance`'s answer from `names balance` output: `{account, balance, public,
/// waiting, small, accounts}`. With no default private account, `account` and `balance` are
/// null and the rest are still told (they were all null: cli-balance-null-without-default-private).
/// `small` is what waits but is worth less than claiming it costs, `{count, total, claim}`.
pub fn balance_reply(output: &str) -> Value {
    balance_reply_of(output, &parse_all_balances(output))
}

/// The same, with each account's balance from `rows` (as the module checked them).
fn balance_reply_of(output: &str, rows: &[Value]) -> Value {
    let mut balance = default_of(rows, "private");
    if !balance.is_object() {
        balance = json!({ "account": null, "balance": null });
    }
    balance["public"] = default_of(rows, "public");
    balance["waiting"] = parse_waiting(output);
    balance["small"] = parse_small_credits(output);
    // Every account it lists, `{account, kind, balance, default}`, default or not.
    let accounts: Vec<Value> = rows
        .iter()
        .map(|row| json!({ "account": row["account"], "kind": row["kind"], "balance": row["balance"], "default": row["default"] }))
        .collect();
    // Nothing at all to tell (no accounts yet): null, as before.
    if accounts.is_empty() && ["account", "public", "waiting", "small"].iter().all(|field| balance[*field].is_null()) {
        return Value::Null;
    }
    balance["accounts"] = Value::from(accounts);
    balance
}

/// The line `also waiting, worth less than claiming costs (about C): N credit(s), T in all`
/// as `{count, total, claim}`, or null.
pub fn parse_small_credits(output: &str) -> Value {
    output
        .lines()
        .find_map(|line| line.trim().strip_prefix("also waiting, worth less than claiming costs (about "))
        .and_then(|rest| {
            let (claim, rest) = rest.split_once("): ")?;
            let mut words = rest.split_whitespace();
            let count = words.next()?;
            let total = words.nth(1)?;
            [claim, count, total]
                .iter()
                .all(|text| !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()))
                .then(|| json!({ "count": count, "total": total, "claim": claim }))
        })
        .unwrap_or(Value::Null)
}

/// The payments `names balance` says wait to be claimed, `{count, total}`, from its line
/// `waiting to be claimed: N payment(s), T in all; ...`; null when none wait (or the CLI
/// is older and doesn't say).
pub fn parse_waiting(output: &str) -> Value {
    output
        .lines()
        .find_map(|line| line.trim().strip_prefix("waiting to be claimed: "))
        .and_then(|rest| {
            let mut words = rest.split_whitespace();
            let count = words.next()?;
            let total = words.nth(1)?;
            (count.bytes().all(|b| b.is_ascii_digit()) && total.bytes().all(|b| b.is_ascii_digit()))
                .then(|| json!({ "count": count, "total": total }))
        })
        .unwrap_or(Value::Null)
}

/// The default account of `kind` (`private` or `public`) among `rows`, `{account (its bare
/// ID), balance}`, or null if there is none.
fn default_of(rows: &[Value], kind: &str) -> Value {
    rows.iter().find(|row| row["kind"] == kind && row["default"] == true).map_or(Value::Null, |row| {
        let account = row["account"].as_str().unwrap_or_default();
        json!({ "account": account.split_once('/').map_or(account, |(_, id)| id), "balance": row["balance"] })
    })
}

/// Every account in `names balance` output, as `account-list` entries: the way an
/// older CLI (the October testnet build, with no `module account-list`) tells them.
/// It gives no nicknames and no keys, so those are null. A balance the CLI printed as
/// `unknown` (it could not read it) is null.
pub fn parse_all_balances(output: &str) -> Vec<Value> {
    let mut rows = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        let (private, rest) = match (line.strip_prefix("Private/"), line.strip_prefix("Public/")) {
            (Some(rest), _) => (true, rest),
            (_, Some(rest)) => (false, rest),
            _ => continue,
        };
        let mut parts = rest.split_whitespace();
        let (Some(account), Some(balance)) = (parts.next(), parts.next()) else { continue };
        let balance = if balance.bytes().all(|b| b.is_ascii_digit()) {
            Value::from(balance)
        } else if balance == "unknown" && !private {
            Value::Null
        } else {
            continue;
        };
        if registry::parse_account(account).is_err() {
            continue;
        }
        rows.push(json!({
            "account": format!("{}/{account}", if private { "Private" } else { "Public" }),
            "kind": if private { "private" } else { "public" },
            "label": null,
            "balance": balance,
            "default": line.ends_with("(default)"),
            "public_key": null,
            "npk": null,
        }));
    }
    rows
}

/// One `names balance` read, checked: what the CLI printed, every account with its balance
/// (null where the sequencer didn't give it), which accounts those are and why, and
/// whether the sequencer answered its health check during the read.
#[derive(Debug, Clone)]
pub struct BalanceRead {
    output: String,
    pub rows: Vec<Value>,
    pub unread: Vec<String>,
    pub why: Option<String>,
    pub healthy: bool,
}

impl BalanceRead {
    /// Every balance read and the sequencer answering all along: only such a read moves the
    /// History's balance marks (selective-degradation-cold-read-false-arrivals).
    pub fn complete(&self) -> bool {
        self.unread.is_empty() && self.healthy
    }

    /// The reply's value, as `balance_reply` gives it.
    pub fn reply(&self) -> Value {
        balance_reply_of(&self.output, &self.rows)
    }

    /// Every account and what it holds, `(Public/… or Private/…, balance)`.
    pub fn balances(&self) -> Vec<(String, u128)> {
        self.rows
            .iter()
            .filter_map(|row| Some((row["account"].as_str()?.to_owned(), row["balance"].as_str()?.parse::<u128>().ok()?)))
            .collect()
    }

    /// What goes beside the balances in a reply: nothing for a complete read; else
    /// `{partial, unread, warning}`, or a warning that the sequencer stopped answering.
    pub fn marks(&self) -> Value {
        if let Some(warning) = unread_warning(&self.unread, self.why.as_deref()) {
            return json!({ "partial": true, "unread": self.unread, "warning": warning });
        }
        if !self.healthy {
            return json!({ "warning": "The sequencer didn't answer a health check during this read, so these balances may be out of date. They update once it answers." });
        }
        Value::Null
    }
}

/// The last `names balance` read, per wallet and sequencer: the app asks for the balance,
/// the wallet and the accounts at once, and each would run the CLI (and catch the wallet
/// up with the chain, seconds) otherwise. The module answers one call at a time, so three
/// of those in a row held every other call (the node's, the app grants) past its timeout.
static BALANCE_SEEN: Mutex<Option<((PathBuf, PathBuf, String), Instant, BalanceRead)>> = Mutex::new(None);

/// How long one `names balance` answers for.
const BALANCE_FRESH: Duration = Duration::from_secs(15);

/// `names balance`, read and checked (at most `BALANCE_FRESH` old), before `deadline`.
/// The CLI runs while the sequencer's health is checked beside it; each public 0 it prints
/// is read again from the sequencer (the October CLI prints 0 for a balance it could not
/// read), and so is each `unknown`: it stays unknown only when the module can't read it
/// either, and then says why in the sequencer's words. Fails when no public balance could be
/// read, or the CLI failed for another reason.
pub fn balance_read(setup: &Setup, deadline: registry::Deadline) -> Result<BalanceRead, String> {
    let key = (setup.program.clone(), setup.wallet.clone(), setup.sequencer.clone());
    if let Some((seen_key, at, read)) = BALANCE_SEEN.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_ref() {
        if *seen_key == key && at.elapsed() < BALANCE_FRESH {
            return Ok(read.clone());
        }
    }
    let probe = Probe::start(&setup.sequencer);
    let (code, output, err) = run_bounded(setup, vec!["balance".to_owned()], BALANCE_PATIENCE, deadline, &probe)?;
    let mut rows = parse_all_balances(&output);
    // A CLI that says it couldn't read some public balances exits 1 and still lists the
    // rest; any other failure is the read's.
    let some_unknown = rows.iter().any(|row| row["kind"] == "public" && row["balance"].is_null());
    if code != Some(0) && !(some_unknown && err.contains("public balance(s) are unknown")) {
        return Err(probe.problem(deadline.at).unwrap_or_else(|| failure(&err, &output)));
    }
    let mut why = if some_unknown { probe.problem(deadline.at) } else { None };
    if let Some(problem) = recheck_zeros(&mut rows, &setup.sequencer, deadline) {
        why.get_or_insert(problem);
    }
    let unread = unread_public(&rows);
    let why = if unread.is_empty() { None } else { why };
    let public = rows.iter().filter(|row| row["kind"] == "public").count();
    if public > 0 && unread.len() == public {
        return Err(why.unwrap_or_else(|| "could not reach the sequencer".to_owned()));
    }
    let healthy = matches!(probe.answer(deadline.at), Some(Ok(())));
    let read = BalanceRead { output, rows, unread, why, healthy };
    *BALANCE_SEEN.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some((key, Instant::now(), read.clone()));
    Ok(read)
}

/// Every account `names balance` lists and what it holds, `(Public/… or Private/…, balance)`,
/// from the shared read (at most `BALANCE_FRESH` old), and only from a complete one: a
/// balance the sequencer didn't give is never taken for anything.
/// Read by `deadline`: the asking call's, which all its reads share.
pub fn balances_by(setup: &Setup, deadline: registry::Deadline) -> Result<Vec<(String, u128)>, String> {
    let read = balance_read(setup, deadline)?;
    if !read.complete() {
        return Err(read.marks()["warning"].as_str().unwrap_or("the sequencer didn't answer").to_owned());
    }
    Ok(read.balances())
}

/// After the CLI changed the wallet, the next balance is read afresh.
pub fn forget_balance() {
    *BALANCE_SEEN.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = None;
}

/// Private accounts' npks, as an older CLI's `wallet account get --keys` prints them:
/// they never change, so each is asked once.
static NPKS: Mutex<std::collections::BTreeMap<String, String>> = Mutex::new(std::collections::BTreeMap::new());

/// The private accounts `names wallet account list` names (`/1/0 Private/<id>`), in order.
pub fn listed_private(output: &str) -> Vec<String> {
    output
        .split_whitespace()
        .filter_map(|word| word.strip_prefix("Private/"))
        .filter(|id| registry::parse_account(id).is_ok())
        .map(|id| format!("Private/{id}"))
        .collect()
}

/// The `npk <hex>` line of `names wallet account get --keys`.
pub fn parse_npk(output: &str) -> Option<String> {
    output.lines().find_map(|line| line.trim().strip_prefix("npk ")).map(str::trim)
        .filter(|hex| hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit())).map(str::to_ascii_lowercase)
}

/// Every account of the wallet: `module account-list`, or, from a CLI too old for
/// it, what `names balance` lists, with each private account's npk from
/// `wallet account get --keys` (so names still find the account they're paid into).
/// A balance the sequencer didn't give is null, and the read names those accounts.
pub fn run_accounts_any(setup: &Setup) -> Result<AccountsRead, String> {
    match run_account_list(setup) {
        Err(problem) if problem == OLDER_CLI => {
            let read = balance_read(setup, registry::Deadline::after(CALL_BUDGET))?;
            let mut rows = read.rows.clone();
            // `names balance` leaves out private accounts that hold nothing, a name's new
            // receiving account among them; `wallet account list` has every one.
            let listed = run_text(setup, vec!["wallet".to_owned(), "account".to_owned(), "list".to_owned()], QUICK_PATIENCE).unwrap_or_default();
            for account in listed_private(&listed) {
                if !rows.iter().any(|row| row["account"] == account.as_str()) {
                    rows.push(json!({ "account": account, "kind": "private", "label": null, "balance": "0", "default": false, "public_key": null, "npk": null }));
                }
            }
            for row in rows.iter_mut().filter(|row| row["kind"] == "private") {
                let account = row["account"].as_str().unwrap_or_default().to_owned();
                let known = NPKS.lock().unwrap_or_else(std::sync::PoisonError::into_inner).get(&account).cloned();
                let npk = known.or_else(|| {
                    let args = vec!["wallet".to_owned(), "account".to_owned(), "get".to_owned(), format!("--account-id={account}"), "--keys".to_owned()];
                    let npk = run_text(setup, args, QUICK_PATIENCE).ok().as_deref().and_then(parse_npk)?;
                    NPKS.lock().unwrap_or_else(std::sync::PoisonError::into_inner).insert(account.clone(), npk.clone());
                    Some(npk)
                });
                row["npk"] = npk.map_or(Value::Null, Value::from);
            }
            Ok(AccountsRead { rows, unread: read.unread, why: read.why })
        }
        other => other,
    }
}

/// Runs the CLI with `command` and returns what it printed, or why it failed.
pub fn run_text(setup: &Setup, command: Vec<String>, patience: Duration) -> Result<String, String> {
    let (code, out, err) = run_capture(setup, command, patience)?;
    if code != Some(0) {
        return Err(failure(&err, &out));
    }
    Ok(out)
}

/// Runs the CLI with `command`: its exit code, what it printed, and what it
/// wrote to stderr.
fn run_capture(setup: &Setup, command: Vec<String>, patience: Duration) -> Result<(Option<i32>, String, String), String> {
    capture(setup.command(command), &setup.program, patience)
}

/// Runs the CLI with `command`, giving it `input` on standard input: what it
/// printed, or why it failed. Nothing on standard input shows in the process list,
/// or in a log.
pub fn run_with_input(setup: &Setup, command: Vec<String>, input: &str, patience: Duration) -> Result<String, String> {
    let (code, out, err) = feed(setup.command(command), &setup.program, input, patience, None)?;
    if code != Some(0) {
        return Err(failure(&err, &out));
    }
    Ok(out)
}

/// Why the CLI failed, in its own words; a CLI older than the command asked of it
/// (clap's "unrecognized subcommand", whose last line only points at --help) says so.
fn failure(err: &str, out: &str) -> String {
    if err.contains("unrecognized subcommand") {
        return OLDER_CLI.to_owned();
    }
    let why = if last_line(err).is_empty() { last_line(out) } else { last_line(err) };
    format!("the names CLI failed: {why}")
}

/// What a names CLI too old for the command asked of it means to the person.
pub const OLDER_CLI: &str = "Your names program is older than this app and can't do this yet: it comes with the LEZ 0.4 update. The rest of the app still works.";

/// Runs `process` (the program at `program`) and waits for it, at most
/// `patience`: its exit code, what it printed, and what it wrote to stderr.
fn capture(process: Command, program: &Path, patience: Duration) -> Result<(Option<i32>, String, String), String> {
    feed(process, program, "", patience, None)
}

/// The same, with `input` on its standard input when there is any.
fn feed(mut process: Command, program: &Path, input: &str, patience: Duration, stop: Option<&std::sync::atomic::AtomicBool>) -> Result<CliOutput, String> {
    if !input.is_empty() {
        process.stdin(Stdio::piped());
    }
    let mut child = process
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not run the names CLI at {}: {error}", program.display()))?;
    // Written from a thread: a large page can fill the pipe before the CLI reads it.
    if let Some(mut stdin) = child.stdin.take() {
        let input = input.to_owned();
        std::thread::spawn(move || {
            let _ = std::io::Write::write_all(&mut stdin, input.as_bytes());
        });
    }
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
    let code = wait_or_stop(&mut child, patience, stop)?;
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

/// Whether this CLI can sign for the module: one from before LEZ 0.4 can't, and
/// can't talk to the chain either.
/// Whether the CLI at `program` has the command `command` (`<program> <command> --help`
/// answers). The October testnet build has no `receive`: run anyway, it failed as a job
/// with clap's "For more information, try '--help'." (receive-on-october-cli-starts-failing-job).
pub fn has_command(program: &Path, command: &str) -> bool {
    let mut process = Command::new(program);
    process.args([command, "--help"]).stdin(Stdio::null());
    capture(process, program, VERSION_PATIENCE).is_ok_and(|(code, _, _)| code == Some(0))
}

pub fn speaks_module(program: &Path) -> bool {
    let mut process = Command::new(program);
    process.args(["module", "--help"]).stdin(Stdio::null());
    capture(process, program, VERSION_PATIENCE).is_ok_and(|(code, _, _)| code == Some(0))
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
        let id = self.prepare(INSTALL_JOB)?;
        for dir in [bin, man] {
            std::fs::create_dir_all(dir).map_err(|error| format!("cannot make {}: {error}", dir.display()))?;
        }
        let file = self.scratch(&id, "sh");
        write_private(&file, script.as_bytes()).map_err(|error| format!("cannot write the installer: {error}"))?;
        self.start_process(&id, INSTALL_JOB, "install", install_process(&file, bin, man), "the installer", Some(file))
    }
}

// ── Jobs ────────────────────────────────────────────────────────────────

/// A job's id: the name, then when it started. Only these characters reach a path.
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 96 && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// The registrations the names CLI is finishing in the background for the wallet in `wallet`
/// (`names register` without `--wait` leaves one running, which proves and saves the wallet):
/// each as a running job, `{id, name, kind: "register", state: "running", started, background:
/// true}`. Only a process that still runs and is a `names register` counts, so one that died
/// (the computer was switched off) never holds the wallet.
pub fn background_registrations(wallet: &Path) -> Vec<Value> {
    let saved: Value = std::fs::read_to_string(wallet.join("names.json"))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(Value::Null);
    let Some(running) = saved["background"].as_object() else { return Vec::new() };
    running
        .iter()
        .filter_map(|(name, pid)| {
            let name = registry::parse_name(name).ok()?;
            let pid = pid.as_u64()?;
            if !alive(pid) || !is_registering(pid, &name) {
                return None;
            }
            Some(json!({
                "id": format!("register-{name}-background"),
                "name": name,
                "kind": "register",
                "state": "running",
                "started": saved["pending"][&name]["made"].clone(),
                "background": true,
            }))
        })
        .collect()
}

/// Whether process `pid` is a `names register <name>`, not some other process that got its id.
fn is_registering(pid: u64, name: &str) -> bool {
    Command::new("/bin/ps")
        .args(["-o", "command=", "-p", &pid.to_string()])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .is_ok_and(|out| {
            let command = String::from_utf8_lossy(&out.stdout);
            let words: Vec<&str> = command.split_whitespace().collect();
            words.windows(2).any(|pair| pair[0] == "register" && pair[1] == name)
        })
}

/// What to say when the wallet is busy with `job` (a running job, or a registration finishing
/// in the background), so nothing else that saves the wallet starts meanwhile.
pub fn busy_with(job: &Value) -> String {
    if job["background"] == true {
        return format!(
            "your wallet is busy finishing the registration of {}{}: it proves its last step on this computer and saves the wallet; wait for it to finish (`cli_mine` says when)",
            job["name"].as_str().unwrap_or("a name"),
            registry::NAME_SUFFIX
        );
    }
    format!(
        "your wallet is busy with \"{}\" (job {}); wait for it to finish",
        job["kind"].as_str().unwrap_or("a change"),
        job["id"].as_str().unwrap_or_default()
    )
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

/// How a job that ended while no one watched it (the module restarted) went, from its
/// log: `done` when the CLI or the wallet printed that its change is in a block, else
/// `unknown` (it may have landed after the log stopped). With the reason.
pub fn ended_unwatched(log: &str) -> (&'static str, &'static str) {
    let landed = log.lines().map(str::trim).any(|line| {
        line.starts_with("applied in block ") || line.starts_with("Transaction is included in block ") || line.starts_with("ready: ")
    });
    if landed {
        ("done", "the names module restarted while this ran; its log says it went through")
    } else {
        ("unknown", "the names module restarted while this ran, so how it ended is unknown; read the name or the balance again")
    }
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
        let _ = write_private(&self.path(id, "json"), record.to_string().as_bytes());
    }

    /// Checks that no job runs for `name`, clears out old jobs, and returns a
    /// fresh job id, with the directory made.
    pub fn prepare(&self, name: &str) -> Result<String, String> {
        private_dir(&self.dir).map_err(|error| format!("cannot make {}: {error}", self.dir.display()))?;
        if let Some(running) = self.running_for(name) {
            return Err(match name {
                INSTALL_JOB => format!("the names program is still being installed (job {running}); wait for it to finish"),
                WALLET_JOB => format!("your wallet is still busy with something else (job {running}); wait for it to finish"),
                _ => format!("a change to {name}{} is still being saved (job {running}); wait for it to finish", registry::NAME_SUFFIX),
            });
        }
        // Every other job runs the CLI on the one wallet, and each saves the wallet when it
        // ends from the copy it read at the start: two at once would undo one another's
        // changes (a new account, a spent private note). So one wallet job at a time (A-1).
        if name != INSTALL_JOB {
            if let Some(job) = self.running().into_iter().find(|job| job["name"] != INSTALL_JOB) {
                return Err(format!(
                    "your wallet is busy with \"{}\" (job {}); wait for it to finish",
                    job["kind"].as_str().unwrap_or("a change"),
                    job["id"].as_str().unwrap_or_default()
                ));
            }
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
        let log = create_private(&self.path(id, "log"))
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
                // No job was made, so no log of one is left either.
                let _ = std::fs::remove_file(self.path(id, "log"));
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

    /// The job's output, its last [`FEE_LOG_BYTES`] if it is longer.
    pub fn whole_log(&self, id: &str) -> String {
        let Ok(mut file) = std::fs::File::open(self.path(id, "log")) else { return String::new() };
        let length = file.metadata().map_or(0, |meta| meta.len());
        let _ = file.seek(SeekFrom::Start(length.saturating_sub(FEE_LOG_BYTES)));
        let mut bytes = Vec::new();
        let _ = file.read_to_end(&mut bytes);
        String::from_utf8_lossy(&bytes).into_owned()
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
            if !watched && !record["pid"].as_u64().is_some_and(alive) {
                // The watcher writes the outcome before it lets go of the job, so a job it
                // just finished has its outcome on disk now: read it again before deciding.
                match self.record(id).filter(|again| again["state"] != "running") {
                    Some(ended) => record = ended,
                    // Started before the module last restarted: nothing waits on it, so how
                    // it ended is read from what it printed, or is unknown, never "failed".
                    None => {
                        let (state, why) = ended_unwatched(&self.whole_log(id));
                        record["state"] = Value::from(state);
                        record["error"] = Value::from(why);
                    }
                }
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

    #[test]
    fn a_claims_payer_and_credits_are_read_from_its_log() {
        let a = "Private/DetM9mjtAHfGp4rqpRNVWBkMjUr2rtEdCUGq3T8rNCdQ";
        let b = "Private/9Dk3Yx4ZQnHxAZ2c4pkuDZ7cHKu8M1NC2vWqzTzS2r8q";
        let log = format!(
            "fee: about 5070616, at most 8239751 (each claim, paid from {a} inside the proof)\nreceiving 1000 into {b}; proving it on this computer (several minutes)\nsubmitted ab\nreceived in block 969; {b} now holds 955121\nreceiving 5 into {a}; proving it\nError: the claim was not applied\nfee paid: 5070616\nfee paid from {a}\n"
        );
        let (payer, claims) = parse_claims(&log);
        assert_eq!(payer.as_deref(), Some(a));
        assert_eq!(claims, vec![(b.to_owned(), 1000)], "only the claim that landed");
    }

    #[test]
    fn fee_lines_add_up_and_nothing_else_counts() {
        assert_eq!(parse_fee("submitted x\napplied in block 3"), Value::Null);
        let log = "fee: about 10, at most 20 (gas: 1 cycles, limit 2)\nfee paid: 9\nstep 2\nfee: about 5, at most 7 (paid inside the proof)\nfee: about x, at most 7 (bad)\nfee paid: nope";
        assert_eq!(parse_fee(log), json!({ "about": "15", "at_most": "27", "paid": "9" }));
        assert_eq!(parse_fee("fee: about 5, at most 7 (a public transfer)"), json!({ "about": "5", "at_most": "7", "paid": null }));
    }

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
        // nul-in-post-text-raw-error: a NUL reached the spawn (an OS error naming the CLI's path).
        for bad in ["a\u{0}b", "bell\u{7}", "esc\u{1b}[2J"] {
            assert!(post_args("carol", bad, &[], "").unwrap_err().contains("control characters"), "{bad:?}");
            assert!(set_text_args("carol", "bio", bad).unwrap_err().contains("control characters"), "{bad:?}");
        }
        assert!(post_args("carol", "two\nlines\tand a tab", &[], "").is_ok());
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
        let to = "9uwkVJuXScDwLKVtQMcHFU9Hvb3uZjiEDVvDv4PYjCfM";
        assert_eq!(transfer_args("Ada", &format!("Public/{to}")).unwrap(), strings(&["transfer", "--", "ada", &format!("Public/{to}")]));
        assert_eq!(transfer_args("ada", &format!("Private/{to}")).unwrap()[3], format!("Private/{to}"));
        assert!(transfer_args("ada", "bob").is_err(), "a name is not a holder");
        // account-prefix-double-strip: a second prefix inside was dropped and the rest sent.
        assert_eq!(bridge_to(&format!("Public/{to}")).unwrap(), Some(BridgeTo { account: to.to_owned(), key: None }));
        assert_eq!(bridge_to("  ").unwrap(), None);
        // By the account's public key (what LEZ 0.4 needs for an account the wallet doesn't
        // hold): the account is the key's, and the CLI is given the key.
        let key = "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9";
        let by_key = bridge_to(key).unwrap().unwrap();
        assert_eq!(by_key.account, registry::account_text(&crate::deposit::key_account(&crate::deposit::public_key_text(key).unwrap())));
        assert_eq!(by_key.arg(), key);
        assert_eq!(bridge_args("5", &by_key.arg(), &"39".repeat(32), "", false).unwrap()[1], format!("--to={key}"));
        // A key LEZ would not accept is refused before anything runs (BIP-340 vector 5).
        let off = bridge_to("eefdea4cdb677750a420fee807eacf21eb9898ae79b9768766e4faa04a2d4a34").unwrap_err();
        assert!(off.contains("isn't a public key LEZ accepts") && off.ends_with("Nothing was sent."), "{off}");
        for bad in [format!("Public/Private/{to}"), format!("Public/Public/{to}"), format!("Private/{to}"), to.to_owned()] {
            assert!(bridge_to(&bad).is_err(), "{bad}");
        }
        assert_eq!(receive_privately_args("Ada").unwrap(), strings(&["receive-privately", "--", "ada"]));
        // A decimal comma or a stray one is refused, not read as more (SC-2, A-2).
        for bad in ["1,5", ",7", "1,,000", "1000,", "12,34", "1,0000", "0,001", "0,100", "00,001", "0_001"] {
            assert!(pay_args("ada", bad).is_err(), "{bad}");
            assert!(amount_text(bad).is_err(), "{bad}");
        }
        assert_eq!(pay_args("ada", "12,345,678").unwrap()[3], "12345678");
        assert_eq!(amount_text("1_000").unwrap(), "1000");
    }

    /// transfer-case-mangled-id-loses-name: a lower-cased ID that still decoded to 32
    /// bytes (9dk3pqd4…) and the all-zero account were sent, and the name ended held by
    /// nobody. account-prefix-double-strip: `Private/Public/<id>` went out as
    /// `Private/<id>`, and a bare ID as `Public/<id>`.
    #[test]
    fn a_hand_over_refuses_mangled_empty_doubled_and_bare_holders() {
        let held = "9Dk3PQD467hXAbsbMzrFYVPu5HFvUqKkU7VdoQD9oirA";
        assert!(transfer_args("xferalpha", &format!("Public/{held}")).is_ok());
        let lower = held.to_ascii_lowercase();
        assert!(registry::parse_account(&lower).is_ok(), "the mangled ID still decodes, so decoding is no check");
        for (bad, why) in [
            (format!("Public/{lower}"), "all one case"),
            (format!("Private/{lower}"), "all one case"),
            ("Public/11111111111111111111111111111111".to_owned(), "empty account"),
            (format!("Private/Public/{held}"), "once"),
            (format!("Public/Private/{held}"), "once"),
            (format!("Public/Public/{held}"), "once"),
            (held.to_owned(), "Public/ or Private/ before"),
        ] {
            let error = transfer_args("xferalpha", &bad).expect_err(&bad);
            assert!(error.contains(why), "{bad}: {error}");
        }
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

    /// register-as-public-silently-stays-private: the name was registered, making it public
    /// stopped, and nothing the app read said so. `cli_mine` now carries the CLI's reason.
    #[test]
    fn a_name_asked_to_be_public_that_stayed_private_says_why() {
        let output = "r4pubrace  held by a deposit (no expiry); held privately\n\
                      r4pubrace  not public yet: making it public stopped: the wallet refused this change before sending it; `names make-public r4pubrace` tries again\n\
                      other  held by a deposit (no expiry); held privately\n";
        let names = parse_mine(output);
        assert_eq!(names.len(), 2);
        assert_eq!(names[0]["private"], true);
        assert!(names[0]["not_public"].as_str().unwrap().starts_with("making it public stopped: the wallet refused"), "{}", names[0]);
        assert_eq!(names[1]["not_public"], Value::Null);
    }

    /// partial-claim-job-and-pages-say-nothing-true: what a claim left waiting is read from its
    /// log, so the waiting account's page shows it.
    #[test]
    fn what_a_claim_left_waiting_is_read_from_its_log() {
        let r2 = &format!("Private/{PUBLIC}");
        let log = format!(
            "left waiting: 3169135 to Private/9Dk3Yx4ZQnHxAZ2c4pkuDZ7cHKu8M1NC2vWqzTzS2r8q, a fee's refund: claiming it costs about 5070616\n\
             left waiting: 1000000 to {r2}: not claimed\n\
             left waiting: junk to {r2}: not claimed\n\
             left waiting: 5 to Public/{PUBLIC}: no\n"
        );
        // The fee's refund is not a payment waiting: the payer's next proof may sweep it.
        assert_eq!(parse_left(&log), vec![(r2.to_owned(), 1_000_000)]);
    }

    /// wallet-job-allowed-while-detached-registration-pending: a registration the CLI finishes
    /// in the background holds the wallet while its process runs, and only then.
    #[test]
    fn a_registration_finishing_in_the_background_holds_the_wallet_while_it_runs() {
        let dir = scratch_dir("background");
        // A script run as `names register racea --wait`: ps shows it with those words.
        let script = dir.join("names");
        std::fs::write(&script, "#!/bin/sh\nsleep 30\n").unwrap();
        std::fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        let mut sleeper = Command::new(&script).args(["register", "racea", "--wait"]).spawn().unwrap();
        let write = |pid: u32| {
            let saved = json!({ "pending": { "racea": { "made": 7 } }, "background": { "racea": pid, "Bad Name": pid } });
            std::fs::write(dir.join("names.json"), saved.to_string()).unwrap();
        };
        write(sleeper.id());
        std::thread::sleep(Duration::from_millis(200));
        let running = background_registrations(&dir);
        let shown = is_registering(u64::from(sleeper.id()), "racea");
        let _ = sleeper.kill();
        let _ = sleeper.wait();
        // Some systems show `sleep` for the process whatever it was started as: then only the
        // liveness half is checked here.
        if shown {
            assert_eq!(running.len(), 1, "{running:?}");
            assert_eq!((running[0]["name"].as_str(), running[0]["kind"].as_str(), running[0]["started"].as_u64()), (Some("racea"), Some("register"), Some(7)));
            assert!(busy_with(&running[0]).contains("finishing the registration of racea"));
        }
        // Once the process is gone, the wallet is free again.
        assert!(background_registrations(&dir).is_empty());
        // Another process that got the id is not taken for it.
        write(std::process::id());
        assert!(background_registrations(&dir).is_empty());
        assert!(busy_with(&json!({ "kind": "shield", "id": "-wallet-1" })).contains("busy with \"shield\" (job -wallet-1)"));
    }

    const KEY: &str = "a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90";
    const PUBLIC: &str = "CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk";

    /// The default private account `names balance` printed, as the reply gives it.
    fn parse_balance(output: &str) -> Value {
        default_of(&parse_all_balances(output), "private")
    }

    /// The default public account the same way.
    fn parse_public_balance(output: &str) -> Value {
        default_of(&parse_all_balances(output), "public")
    }

    /// The 0.4 CLI's `unknown` is a balance not read (null), never a number; the October
    /// CLI's `no longer held` and the 0.4 CLI's `unknown` lines are told apart for mine.
    #[test]
    fn an_unknown_balance_and_an_unread_name_are_told_apart() {
        let rows = parse_all_balances("  Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk  unknown  (default)\n  Private/yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z  unknown\n");
        assert_eq!(rows.len(), 1, "a private balance is never unknown: {rows:?}");
        assert_eq!(rows[0]["balance"], Value::Null);
        assert_eq!(unread_public(&rows), vec![format!("Public/{PUBLIC}")]);
        let warning = unread_warning(&unread_public(&rows), Some("could not reach the sequencer: it answered HTTP 503 instead of a reading.")).expect("a warning");
        assert_eq!(warning, "The sequencer didn't give the balance of Public/CCVd…pKnk (could not reach the sequencer: it answered HTTP 503 instead of a reading), so its balance shows as unknown. Try again shortly.");
        let (unknown, gone) = mine_doubts("jef  unknown: the sequencer didn't answer\nanon  no longer held by this wallet\nkept  held by a deposit (no expiry)\n");
        assert_eq!((unknown, gone), (vec!["jef".to_owned()], vec!["anon".to_owned()]));
        assert!(parse_mine("jef  unknown: the sequencer didn't answer\n").is_empty(), "an unread name is not listed as held");
        assert_eq!(listed(&["a".into(), "b".into(), "c".into(), "d".into(), "e".into()]), "a, b, c and 2 more");
    }

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
            json!({ "account": "yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z", "balance": "250", "public": { "account": PUBLIC, "balance": "70" }, "waiting": null, "small": null,
                    "accounts": [{ "account": format!("Public/{PUBLIC}"), "kind": "public", "balance": "70", "default": true },
                                 { "account": "Private/yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z", "kind": "private", "balance": "250", "default": true }] })
        );
        // A new CLI at the same path answers within BALANCE_FRESH from the last read; afresh after.
        let none = Setup { program: fake_cli(&dir, "echo 'private total: 0'"), ..setup };
        forget_balance();
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
        // Grouped like every other amount (cli-bridge-refuses-grouped-amount): 1,000 is 1000, 1,5 is refused.
        assert_eq!(bridge_args("1,000", PUBLIC, KEY, "", false).unwrap().last().map(String::as_str), Some("1000"));
        for amount in ["", "0", "000", "-5", "1e9", "1,5", "1.5", "5 5", "--yes", &"9".repeat(40)] {
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
        // The CLI's refusals from the deposit's metadata are said as it said them.
        assert_eq!(bridge_problem(&format!("Error: {}\n", crate::deposit::UNKNOWN_LEZ), node), crate::deposit::UNKNOWN_LEZ);
        let needs = crate::deposit::Refusal::NeedsKey { account: [5; 32] }.words(&registry::account_text);
        assert_eq!(bridge_problem(&format!("Error: {needs}\n"), node), needs);
        // A CLI without the command (the October testnet build has no `module`) says it's older.
        let clap = "error: unrecognized subcommand 'module'\n\nUsage: names [OPTIONS] <COMMAND>\n\nFor more information, try '--help'.\n";
        assert_eq!(failure(clap, ""), OLDER_CLI);
        // That CLI's `names balance`, as it prints it, read as every account.
        let balance = "Latest block is 77650\npublic accounts (seen on chain):\n  Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk  38858810170  (default)\n  Public/FaPur3x8Y4YQ2t5x9KhV3F4cTn8u2Ym1Wq6Lr7Zs9Pd  44317823\nprivate accounts (only this wallet sees them):\n  Private/yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z  40599000  (default)\n  Private/Z1NuMj9vQ3wK8mR5tY2pL6hX4cB7nF1dS3gA9eJ2uV5  777\nprivate total: 40600777\n";
        assert_eq!(parse_npk("Balance 1000, nonce 2\nnpk 11CFF86C4bbd8f5d1e0e2b7c3a9f6d4e2c1b0a99887766554433221100ffeedd\nvpk d1c1\n").as_deref(),
                   Some("11cff86c4bbd8f5d1e0e2b7c3a9f6d4e2c1b0a99887766554433221100ffeedd"));
        assert_eq!(parse_npk("Balance 1000\n"), None);
        let listed = "/ Private/52QsitjhXQF2KpNBTBqxLSz9XR6ofZVAsdwprxx9HHNR\n/0 Private/yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z\n/2 Private/not-an-id\n";
        assert_eq!(listed_private(listed), vec!["Private/52QsitjhXQF2KpNBTBqxLSz9XR6ofZVAsdwprxx9HHNR", "Private/yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z"]);
        let rows = parse_all_balances(balance);
        assert_eq!(rows.len(), 4, "{rows:?}");
        assert_eq!(rows[0]["account"], "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk");
        assert_eq!((rows[0]["default"].as_bool(), rows[1]["default"].as_bool()), (Some(true), Some(false)));
        assert_eq!((rows[2]["kind"].as_str(), rows[2]["balance"].as_str(), rows[3]["balance"].as_str()), (Some("private"), Some("40599000"), Some("777")));
        assert_eq!(failure("Error: no such name\n", ""), "the names CLI failed: Error: no such name");
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
        // Another public account picked as the destination; never a private one.
        let other = "7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR";
        let picked = run_bridge_to(&setup, "1000", KEY, &format!("Public/{other}"), "", false).unwrap();
        assert!(picked["lines"].as_array().unwrap().contains(&json!(format!("[--to=Public/{other}]"))), "{picked}");
        assert!(run_bridge_to(&setup, "1000", KEY, &format!("Private/{other}"), "", false).unwrap_err().contains("public LEZ account"));
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
        // Another name waits too: both would save the one wallet (A-1).
        assert!(jobs.prepare("ada").unwrap_err().contains("your wallet is busy"));
        assert!(jobs.prepare(WALLET_JOB).unwrap_err().contains("your wallet is busy"));
        // Installing the CLI doesn't touch the wallet.
        assert!(jobs.prepare(INSTALL_JOB).is_ok());
        assert_eq!(finished(&jobs, &id)["state"], "done");
        assert!(!body.exists());
        assert!(jobs.prepare("jef").is_ok());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// module-reload-mid-send-marks-landed-send-failed: a send whose module reloaded
    /// while it ran was recorded failed for good, though its log said "Transaction is
    /// included in block 14" and the 100 arrived. Now unknown, or done from its log.
    #[test]
    fn a_job_nothing_waits_on_is_settled_from_its_log_once_its_process_is_gone() {
        let dir = scratch_dir("orphan");
        let jobs = Jobs { dir: dir.clone() };
        // As left by a module that stopped mid-job: running, with a dead pid.
        jobs.write("jef-5", &json!({ "id": "jef-5", "name": "jef", "state": "running", "pid": 999_999_999_u64, "started": 1 }));
        std::fs::write(jobs.path("jef-5", "log"), "proving...\n").unwrap();
        let state = jobs.state("jef-5").unwrap();
        assert_eq!(state["state"], "unknown");
        assert!(last_line(state["log_tail"].as_str().unwrap()).contains("restarted"));
        assert!(jobs.running_for("jef").is_none());
        jobs.write("-wallet-6", &json!({ "id": "-wallet-6", "name": "-wallet", "state": "running", "pid": 999_999_999_u64, "started": 1 }));
        std::fs::write(jobs.path("-wallet-6", "log"), "Transaction is included in block 14\nTransaction data is Public(...)\n").unwrap();
        assert_eq!(jobs.state("-wallet-6").unwrap()["state"], "done");
        for log in ["submitted ab12\napplied in block 9\nfee paid: 4088\n", "ready: Private/x holds 5\n"] {
            assert_eq!(ended_unwatched(log).0, "done", "{log}");
        }
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
        assert!(!jobs.path(&id, "log").exists(), "no orphan log (nul-in-post-text-raw-error)");
        assert!(jobs.running_for("jef").is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// cli-balance-null-without-default-private: with names.json's private set to null,
    /// cli_balance answered value null, though `names balance` listed a public account,
    /// three private ones and credits waiting.
    #[test]
    fn the_balance_is_told_without_a_default_private_account() {
        let output = "public accounts (seen on chain):\n  Public/HE1J2z3HkZdk2KX4yGrrGM1cSbbfSdBmoLspqtTv8KUK  66977128\nprivate accounts (only this wallet sees them):\n  Private/E3j7HPx3ZhoqRnB9BtMsGjWixbms3n9kamGKXKkjiuy5  31760249\nprivate total: 59806708\nalso waiting, worth less than claiming costs (about 5070616): 3 credit(s), 11457780 in all\n";
        let reply = balance_reply(output);
        assert_eq!((reply["account"].clone(), reply["balance"].clone(), reply["public"].clone(), reply["waiting"].clone()), (Value::Null, Value::Null, Value::Null, Value::Null), "{reply}");
        assert_eq!(reply["small"], json!({ "count": "3", "total": "11457780", "claim": "5070616" }));
        let listed: Vec<(&str, &str)> = reply["accounts"].as_array().unwrap().iter().map(|a| (a["account"].as_str().unwrap(), a["balance"].as_str().unwrap())).collect();
        assert_eq!(listed, [("Public/HE1J2z3HkZdk2KX4yGrrGM1cSbbfSdBmoLspqtTv8KUK", "66977128"), ("Private/E3j7HPx3ZhoqRnB9BtMsGjWixbms3n9kamGKXKkjiuy5", "31760249")]);
        let with = balance_reply(&output.replace("31760249", "31760249  (default)").replace("66977128", "66977128  (default)"));
        assert_eq!((with["account"].as_str(), with["balance"].as_str(), with["public"]["balance"].as_str()), (Some("E3j7HPx3ZhoqRnB9BtMsGjWixbms3n9kamGKXKkjiuy5"), Some("31760249"), Some("66977128")), "{with}");
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
        assert_eq!(setup_args("").unwrap(), strings(&["setup", "--shield=50000000"]));
        assert_eq!(setup_args(" 7,000,000 ").unwrap(), strings(&["setup", "--shield=7000000"]));
        // cli-wording-leftovers: `00` is 0, as `0` is.
        assert_eq!(setup_args("00").unwrap(), strings(&["setup", "--shield=0"]));
        for bad in ["-1", "1.5", "5 5", "--yes", "1e6", "0,5"] {
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

    /// transfer-failure-text-is-last-cause-line: a transfer with the sequencer down told
    /// "It didn't go through: 2: Connection refused (os error 61)."
    #[test]
    fn a_failed_job_says_its_error_not_its_last_cause() {
        let down = "Error: failed to read the account from the sequencer\n\nCaused by:\n    0: client error (Connect)\n    1: tcp connect error\n    2: Connection refused (os error 61)\n";
        assert_eq!(job_problem("transfer", down), "Giving the name away didn't go through: couldn't reach the sequencer. Check that it is running and reachable, then try again");
        let refused = "Synced to block 9\nError: Public/CCVd holds 0 Demo, less than 5\n\nCaused by:\n    0: the token program said no\n";
        assert_eq!(job_problem("token_send", refused), "The token transfer didn't go through: Public/CCVd holds 0 Demo, less than 5");
        assert_eq!(job_problem("pay_account", "Error: Failed to get data from sequencer\n"), "The payment didn't go through: Failed to get data from sequencer");
        // A "connection refused" retried before the run went on is not the failure.
        assert_eq!(failure_cause("retrying: Connection refused\nError: the name is taken\n"), "the name is taken");
        assert_eq!(failure_cause("killed by signal 9\n"), "killed by signal 9");
        assert_eq!(failure_cause(""), "it stopped without saying why");
        assert_eq!(job_problem("something_new", "Error: boom\n"), "It didn't go through: boom");
    }

    #[test]
    fn a_move_to_private_says_whether_anything_moved() {
        let poor = "public account   Public/x\nPublic/x holds 1200; send it at least 5000000 testnet tokens from your LEZ wallet (or with `names bridge`), then run `names setup` again.\n";
        let read = shield_outcome(poor, true);
        assert_eq!(read["moved"], false);
        assert_eq!(read["problem"], "Your public account holds 1,200, and moving to private needs at least 5,000,000. Get more tokens, then try again.");
        // cli-wording-leftovers: the CLI now groups the figures and fails a shield it couldn't make.
        let grouped = "public account   Public/x\nError: Public/x holds 1,200; send it at least 5,000,000 testnet tokens from your LEZ wallet (or with `names bridge`), then run `names setup` again. Nothing was moved.\n";
        assert_eq!(shield_outcome(grouped, false), read);
        let moved = shield_outcome("moving 5000000 to the private account; generating the proof (a few minutes)\nready: ...\n", true);
        assert_eq!(moved, json!({ "moved": true, "problem": null }));
        let scrolled = "Stored persistent accounts at /w/storage.json\n".repeat(40) + "Synced to block 36710\nready: `names register <name>` holds a name privately\n";
        assert_eq!(shield_outcome(&scrolled, true)["moved"], true);
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

    // ── Tokens ──────────────────────────────────────────────────────────

    const TOKEN: &str = "CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk";
    const HOLDER: &str = "7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR";

    #[test]
    fn token_amounts_reach_128_bits_and_no_further() {
        assert_eq!(amount_text(" 1,000_000 "), Ok("1000000".to_owned()));
        assert_eq!(amount_text(&u128::MAX.to_string()), Ok(u128::MAX.to_string()));
        assert_eq!(amount_text("007"), Ok("7".to_owned()));
        for bad in ["", "0", "000", "-1", "1.5", "1e3", "340282366920938463463374607431768211456", "--5", "five"] {
            assert!(amount_text(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_token_goes_to_a_name_or_a_written_account() {
        assert_eq!(token_to("Jef.lez"), Ok("jef".to_owned()));
        assert_eq!(token_to(&format!("Public/{HOLDER}")), Ok(format!("Public/{HOLDER}")));
        assert_eq!(token_to(&format!(" Private/{HOLDER} ")), Ok(format!("Private/{HOLDER}")));
        for bad in [HOLDER.to_owned(), format!("Private/{HOLDER}/x"), "Private/nope".to_owned(), "--from=public".to_owned(), "$(id)".to_owned(), String::new()] {
            assert!(token_to(&bad).is_err(), "{bad}");
        }
        assert!(token_definition("nope").is_err());
        assert!(token_definition(&registry::account_text(&registry::NATIVE_TOKEN)).unwrap_err().contains("native"));
    }

    #[test]
    fn a_token_send_is_built_from_checked_parts() {
        let program = [3; 32];
        let flag = format!("--token-program={}", registry::account_text(&program));
        assert_eq!(
            token_send_args(&program, TOKEN, "1,000", "Jef", true, false).unwrap(),
            strings(&[&flag, "token", "send", "--to=jef", "--from=private", "--", TOKEN, "1000"])
        );
        // `to_public` only means something for a name.
        assert_eq!(
            token_send_args(&program, TOKEN, "5", "jef", false, true).unwrap(),
            strings(&[&flag, "token", "send", "--to=jef", "--from=public", "--public", "--", TOKEN, "5"])
        );
        let to = format!("--to=Public/{HOLDER}");
        assert_eq!(
            token_send_args(&program, TOKEN, "5", &format!("Public/{HOLDER}"), false, true).unwrap(),
            strings(&[&flag, "token", "send", &to, "--from=public", "--", TOKEN, "5"])
        );
        for (definition, amount, to) in [("nope", "5", "jef"), (TOKEN, "0", "jef"), (TOKEN, "5", HOLDER), (TOKEN, "5", "-x")] {
            assert!(token_send_args(&program, definition, amount, to, false, false).is_err(), "{definition} {amount} {to}");
        }
        assert_eq!(token_receive_args(&program), strings(&[&flag, "token", "receive"]));
    }

    #[test]
    fn the_token_list_is_read_from_the_last_line_and_checked() {
        let output = format!(
            "Latest block is 12\n{}\n",
            json!({
                "holdings": [
                    { "account": format!("Public/{HOLDER}"), "private": false, "definition": TOKEN, "name": "Demo\u{202E}xx", "balance": "1000250" },
                    { "account": format!("Private/{HOLDER}"), "private": true, "definition": TOKEN, "name": null, "balance": "200" },
                    { "account": HOLDER, "private": false, "definition": TOKEN, "name": "bare", "balance": "1" },
                    { "account": format!("Public/{HOLDER}"), "private": false, "definition": "nope", "name": "x", "balance": "1" },
                    { "account": format!("Public/{HOLDER}"), "private": false, "definition": TOKEN, "name": "x", "balance": "-1" },
                ],
                "pending": [
                    { "position": 21, "definition": TOKEN, "name": "Demo", "amount": "200", "to": HOLDER },
                    { "position": "x", "definition": TOKEN, "name": "Demo", "amount": "200", "to": HOLDER },
                ],
            })
        );
        let list = parse_token_list(&output).unwrap();
        assert_eq!(
            list["holdings"],
            json!([
                { "account": format!("Public/{HOLDER}"), "account_kind": "public", "definition": TOKEN, "name": "Demo xx", "balance": "1000250" },
                { "account": format!("Private/{HOLDER}"), "account_kind": "private", "definition": TOKEN, "name": null, "balance": "200" },
            ])
        );
        assert_eq!(list["pending"], json!([{ "position": 21, "definition": TOKEN, "name": "Demo", "amount": "200", "to": format!("Private/{HOLDER}") }]));
        assert!(parse_token_list("Error: no token program given").is_err());
    }

    #[test]
    fn a_token_jobs_log_says_what_it_sent() {
        let sent = token_outcome("sending 5 Demo\nproved in 515s\nsubmitted 18322d72\napplied in block 74\n");
        assert_eq!(sent, json!({ "tx": "18322d72", "block": 74, "applied": 1, "nothing": false }));
        let received = token_outcome("receiving 1\nsubmitted aa\napplied in block 3\nreceiving 2\nsubmitted bb\napplied in block 9\n");
        assert_eq!((received["tx"].as_str(), received["block"].as_u64(), received["applied"].as_u64()), (Some("bb"), Some(9), Some(2)));
        assert_eq!(token_outcome("nothing to receive\n")["nothing"], true);
        assert_eq!(token_outcome("Error: refused")["tx"], Value::Null);
    }

    #[test]
    fn token_info_passes_the_program_and_cleans_the_name() {
        let dir = scratch_dir("token-info");
        let answer = json!({ "definition": TOKEN, "name": "Demo\u{200B}", "supply": "1000", "authority": HOLDER, "extra": 1 });
        let setup = setup(&dir, &format!("echo '{answer}'"));
        let info = run_token_info(&setup, &[3; 32], &registry::parse_account(TOKEN).unwrap()).unwrap();
        assert_eq!(info, json!({ "definition": TOKEN, "name": "Demo", "supply": "1000", "authority": HOLDER }));
        let refusing = Setup { program: fake_cli(&dir, "echo 'Error: not a token' >&2; exit 1"), ..setup };
        assert!(run_token_info(&refusing, &[3; 32], &[4; 32]).unwrap_err().contains("not a token"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
