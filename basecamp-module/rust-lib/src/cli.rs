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
/// installer puts it), else the first `names` on `PATH`.
pub fn find_program(configured: Option<&str>) -> Option<PathBuf> {
    if let Some(path) = configured {
        return Some(PathBuf::from(path));
    }
    let installed = std::env::var_os("HOME").map(|home| Path::new(&home).join(".local/bin/names"));
    if let Some(path) = installed.filter(|path| is_executable(path)) {
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
                "there is no names CLI wallet in {}; run `names setup`, or choose the CLI's wallet folder in App settings",
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
    if amount.is_empty() || amount.len() > 30 || !amount.bytes().all(|b| b.is_ascii_digit()) || amount.bytes().all(|b| b == b'0') {
        return Err("enter a whole amount, such as 1000".to_owned());
    }
    Ok(vec!["pay".to_owned(), "--".to_owned(), name, amount])
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
/// balance: `{account, balance}` (decimal text), or null if there is none.
pub fn run_balance(setup: &Setup) -> Result<Value, String> {
    setup.wallet_ready()?;
    Ok(parse_balance(&run_text(setup, vec!["balance".to_owned()], BALANCE_PATIENCE)?))
}

/// The default private account in `names balance` output:
/// `  Private/<id>  <balance>  (default)`.
pub fn parse_balance(output: &str) -> Value {
    for line in output.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("Private/") else { continue };
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
    let mut child = setup
        .command(command)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not run the names CLI at {}: {error}", setup.program.display()))?;
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
    if code != Some(0) {
        let why = if last_line(&err).is_empty() { last_line(&out) } else { last_line(&err) };
        return Err(format!("the names CLI failed: {why}"));
    }
    Ok(out)
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
            return Err(format!(
                "a change to {name}{} is still being saved (job {running}); wait for it to finish",
                registry::NAME_SUFFIX
            ));
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
                for extension in ["json", "log", "page"] {
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
        if !valid_id(id) {
            return Err(format!("not a job id: {id}"));
        }
        let log = std::fs::File::create(self.path(id, "log"))
            .map_err(|error| format!("cannot write the job's log: {error}"))?;
        let log_err = log.try_clone().map_err(|error| format!("cannot write the job's log: {error}"))?;
        let mut process = setup.command(command);
        // Its own process group: a signal meant for the host leaves it be.
        process.stdout(log).stderr(log_err).process_group(0);
        let mut child = match process.spawn() {
            Ok(child) => child,
            Err(error) => {
                if let Some(scratch) = &scratch {
                    let _ = std::fs::remove_file(scratch);
                }
                return Err(format!("could not run the names CLI at {}: {error}", setup.program.display()));
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
        assert!(empty.wallet_ready().unwrap_err().contains("names setup"));
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
}
