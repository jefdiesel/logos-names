//! LEZ NFT module: the NFT app's engine inside Basecamp.
//!
//! Everything that touches the chain runs the `lez-nft` command-line client as
//! a background job, against the app's own wallet (`~/.config/lez-nft/wallet`
//! unless set otherwise). The app sets that wallet up once (`setup`): a public
//! account to top up from and a private account that holds the funds, mints and
//! holds tokens. A private account's transactions are proven on this machine,
//! about 5 minutes each, so every command is a job the app polls.
//!
//! Methods answer JSON text in the shape Logos modules use:
//!
//! ```json
//! { "success": true,  "value": <answer>, "error": null }
//! { "success": false, "value": null,     "error": "why" }
//! ```
//!
//! The app's view is sandboxed and can't read files outside its folder, so the
//! module also decodes images (`thumbnail`) into pixels for it to paint.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

// The builder injects the generated scaffold here: the `install` entry point,
// `RustModuleContext`, `context()` and `modules()`. Unit tests build without it.
#[cfg(not(test))]
include!(concat!(env!("CARGO_MANIFEST_DIR"), "/generated/provider_gen.rs"));

#[cfg(test)]
pub struct RustModuleContext;

const DEFAULT_RPC: &str = "https://testnet.lez.logos.co";
/// How much of a job's log `job` returns.
const TAIL_BYTES: usize = 4096;
/// Largest image the app takes, in bytes.
const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;

/// The CLI subcommands a job may run. The CLI's `wallet` passthrough is left out:
/// it can send the app wallet's funds anywhere, and the app never needs it.
const COMMANDS: [&str; 16] = [
    "setup", "accounts", "shield", "receive-code", "create", "collection", "buy", "mint", "seal-file", "open-file", "info",
    "transfer", "burn", "portfolio", "collections", "--version",
];
/// Commands that write the wallet. Two at once could each save over the other's
/// changes and lose a private account's new note, so only one runs at a time.
const WRITERS: [&str; 8] = ["setup", "accounts", "shield", "create", "buy", "mint", "transfer", "burn"];

pub trait LezNftModule: Send + 'static {
    /// `{cli, cli_found, wallet, wallet_ready, program, rpc, files}`: the CLI it
    /// runs, the app wallet's folder (and whether a wallet is in it), the
    /// program and sequencer it uses (empty for the CLI's own defaults), and
    /// where downloaded files go.
    fn get_config(&mut self) -> String;

    /// Sets the sequencer (an `https://` URL, or `http://` to this machine);
    /// empty keeps the default. The CLI path, wallet folder and program can't be
    /// set this way, since any caller could then run another program or sign
    /// for another wallet; they come from `~/.config/lez-nft/config.json`,
    /// edited by hand. Passing one that differs from it fails. The value is the
    /// new configuration.
    fn set_config(&mut self, cli: String, wallet: String, program: String, rpc: String) -> String;

    /// Starts `lez-nft --json <args>` as a job, `args_json` being a JSON array
    /// of strings. The value is `{job}`. Commands that change the wallet run one
    /// at a time: starting another while one runs fails, naming it.
    fn start(&mut self, args_json: String) -> String;

    /// How job `id` is going: `{id, args, state, value, error, log}`. `state` is
    /// `running`, `done` or `failed`; `value` is the CLI's answer once done and
    /// `log` the end of its progress output.
    fn job(&mut self, id: String) -> String;

    /// The last 20 jobs, newest first, as `job` shows each.
    fn jobs(&mut self) -> String;

    /// The image at `path`, decoded and scaled to at most `max` pixels a side
    /// (1–512): `{width, height, pixels}`, `pixels` base64 of RGBA8, row by row
    /// from the top left. PNG, JPEG, GIF and WebP.
    fn thumbnail(&mut self, path: String, max: i64) -> String;

    /// `{exists, bytes, image}` for a file the person picked: whether it is
    /// there, its size, and whether it decodes as an image the app takes.
    fn file_info(&mut self, path: String) -> String;

    /// Where the file for `cid` is kept once downloaded, `{path, exists}`;
    /// `suffix` tells a decrypted copy from the download (`plain`).
    fn cid_path(&mut self, cid: String, suffix: String) -> String;

    /// Deletes a file in the app's files folder (a used content key, say);
    /// anything outside it is refused. The value is whether it was there.
    fn remove_file(&mut self, path: String) -> String;

    /// Called once the module is wired to the host.
    fn on_context_ready(&mut self, _ctx: &RustModuleContext) {}
}

struct LezNft {
    cli: Option<String>,
    wallet: Option<String>,
    program: Option<String>,
    rpc: Option<String>,
}

impl Default for LezNft {
    fn default() -> Self {
        let mut module = Self { cli: None, wallet: None, program: None, rpc: None };
        if let Some(saved) = config_dir().and_then(|dir| std::fs::read(dir.join("config.json")).ok()) {
            if let Ok(saved) = serde_json::from_slice::<Value>(&saved) {
                let text = |key: &str| saved[key].as_str().filter(|value| !value.is_empty()).map(str::to_owned);
                module.cli = text("cli");
                module.wallet = text("wallet");
                module.program = text("program");
                module.rpc = text("rpc");
            }
        }
        module
    }
}

fn ok(value: Value) -> String {
    json!({ "success": true, "value": value, "error": null }).to_string()
}

fn fail(error: impl std::fmt::Display) -> String {
    json!({ "success": false, "value": null, "error": error.to_string() }).to_string()
}

fn answer(result: Result<Value, String>) -> String {
    result.map_or_else(fail, ok)
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Where this module keeps its settings, jobs and files.
#[cfg(not(test))]
fn config_dir() -> Option<PathBuf> {
    home().map(|home| home.join(".config/lez-nft"))
}

#[cfg(test)]
fn config_dir() -> Option<PathBuf> {
    Some(std::env::temp_dir().join(format!("lez-nft-test-config-{}", std::process::id())))
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata().is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

/// `true` while process `pid` is alive.
fn alive(pid: i32) -> bool {
    // SAFETY: signal 0 only checks that the process exists.
    pid > 0 && unsafe { libc::kill(pid, 0) } == 0
}

impl LezNft {
    /// The CLI: the one named in `config.json`, else `~/.local/bin/lez-nft`.
    /// `PATH` isn't searched, so nothing earlier on it can stand in for the CLI.
    fn cli_path(&self) -> Option<PathBuf> {
        if let Some(path) = &self.cli {
            return Some(PathBuf::from(path));
        }
        home().map(|home| home.join(".local/bin/lez-nft"))
    }

    fn wallet_dir(&self) -> Option<PathBuf> {
        self.wallet.as_ref().map(PathBuf::from).or_else(|| config_dir().map(|dir| dir.join("wallet")))
    }

    fn files_dir(&self) -> Result<PathBuf, String> {
        let dir = config_dir().ok_or("this device has no home folder")?.join("files");
        std::fs::create_dir_all(&dir).map_err(|e| format!("cannot make {}: {e}", dir.display()))?;
        Ok(dir)
    }

    fn jobs_dir(&self) -> Result<PathBuf, String> {
        let dir = config_dir().ok_or("this device has no home folder")?.join("jobs");
        std::fs::create_dir_all(&dir).map_err(|e| format!("cannot make {}: {e}", dir.display()))?;
        Ok(dir)
    }

    fn config(&self) -> Value {
        let cli = self.cli_path();
        let wallet = self.wallet_dir();
        json!({
            "cli": cli.as_ref().map(|path| path.display().to_string()),
            "cli_found": cli.as_ref().is_some_and(|path| is_executable(path)),
            "wallet": wallet.as_ref().map(|path| path.display().to_string()),
            "wallet_ready": wallet.as_ref().is_some_and(|dir| dir.join("storage.json").is_file()),
            "program": self.program.clone().unwrap_or_default(),
            "rpc": self.rpc.clone().unwrap_or_else(|| DEFAULT_RPC.to_owned()),
            "files": self.files_dir().ok().map(|dir| dir.display().to_string()),
        })
    }

    fn save(&self) {
        if let Some(dir) = config_dir() {
            let saved = json!({ "cli": self.cli, "wallet": self.wallet, "program": self.program, "rpc": self.rpc });
            std::fs::create_dir_all(&dir).ok();
            std::fs::write(dir.join("config.json"), serde_json::to_string_pretty(&saved).unwrap_or_default()).ok();
        }
    }

    fn set_config_checked(&mut self, cli: &str, wallet: &str, program: &str, rpc: &str) -> Result<Value, String> {
        let given = |value: &str| Some(value.trim().to_owned()).filter(|value| !value.is_empty());
        for (name, value, current) in [("cli", cli, &self.cli), ("wallet", wallet, &self.wallet), ("program", program, &self.program)] {
            if let Some(value) = given(value) {
                if current.as_deref() != Some(value.as_str()) {
                    return Err(format!(
                        "`{name}` can only be changed by editing ~/.config/lez-nft/config.json, so no other module can change it"
                    ));
                }
            }
        }
        let rpc = given(rpc);
        if let Some(rpc) = &rpc {
            check_rpc(rpc)?;
        }
        self.rpc = rpc;
        self.save();
        Ok(self.config())
    }

    /// Checks a job's file arguments: what it writes, and the keys and sealed
    /// files it reads, stay in the app's files folder; `seal-file` reads only
    /// an image; content is given inline, never as `@file`.
    fn check_paths(&self, args: &[String]) -> Result<(), String> {
        let files = self.files_dir()?.canonicalize().map_err(|e| e.to_string())?;
        let command = args[0].as_str();
        let mut i = 1;
        while i < args.len() {
            let flag = args[i].as_str();
            let value = args.get(i + 1).map(String::as_str);
            match (command, flag) {
                ("seal-file", "--out" | "--key-out") | ("open-file", "--in" | "--out") | ("mint", "--key-file") => {
                    in_folder(&files, value.ok_or_else(|| format!("{flag} needs a value"))?)?;
                    i += 1;
                }
                ("seal-file", "--in") => {
                    let path = value.ok_or("--in needs a value")?;
                    if !is_image(Path::new(path)) {
                        return Err(format!("only an image under {} MB can be encrypted", MAX_IMAGE_BYTES >> 20));
                    }
                    i += 1;
                }
                ("mint" | "create", "--content") => {
                    if value.is_some_and(|value| value.starts_with('@')) {
                        return Err("content is given as text, not read from a file".to_owned());
                    }
                    i += 1;
                }
                (_, flag) if flag.starts_with("--") && flag.contains('=') => {
                    return Err(format!("give {flag} as two arguments"));
                }
                _ => {}
            }
            i += 1;
        }
        Ok(())
    }

    /// A job still running that changes the wallet, of all the jobs kept.
    fn running_writer(&self) -> Option<Value> {
        let dir = self.jobs_dir().ok()?;
        std::fs::read_dir(dir)
            .ok()?
            .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
            .filter_map(|id| self.job_state(&id).ok())
            .find(|job| job["state"] == "running" && job["writer"] == true)
    }

    fn start_job(&mut self, args_json: &str) -> Result<Value, String> {
        let args: Vec<String> = serde_json::from_str(args_json).map_err(|_| "args must be a JSON array of strings")?;
        check_args(&args)?;
        self.check_paths(&args)?;
        let cli = self.cli_path().filter(|path| is_executable(path)).ok_or(
            "the lez-nft command line is not installed; put it in ~/.local/bin, or name it as `cli` in ~/.config/lez-nft/config.json",
        )?;
        let writer = WRITERS.contains(&args[0].as_str());
        if writer {
            if let Some(busy) = self.running_writer() {
                return Err(format!(
                    "the wallet is busy with `{}`; wait for it to finish",
                    busy["args"].as_array().and_then(|args| args.first()).and_then(Value::as_str).unwrap_or("a job")
                ));
            }
        }
        let wallet = self.wallet_dir().ok_or("this device has no home folder")?;
        std::fs::create_dir_all(&wallet).map_err(|e| format!("cannot make {}: {e}", wallet.display()))?;
        let id = format!("{}-{}", now_ms(), std::process::id());
        let dir = self.jobs_dir()?.join(&id);
        std::fs::create_dir_all(&dir).map_err(|e| format!("cannot make {}: {e}", dir.display()))?;
        let out = std::fs::File::create(dir.join("out.json")).map_err(|e| e.to_string())?;
        let log = std::fs::File::create(dir.join("log.txt")).map_err(|e| e.to_string())?;

        let mut command = Command::new(&cli);
        // Every argument is one argv entry; nothing passes through a shell.
        command.arg("--json").args(&args).env("LEE_WALLET_HOME_DIR", &wallet).stdin(Stdio::null());
        command.stdout(out).stderr(log);
        if let Some(program) = &self.program {
            command.env("LEZ_NFT_PROGRAM", program);
        }
        if let Some(rpc) = &self.rpc {
            command.env("LEZ_NFT_RPC", rpc);
        }
        // Private transactions are proven by RISC Zero's r0vm; Basecamp's PATH
        // doesn't reach where it is usually installed.
        if std::env::var_os("RISC0_SERVER_PATH").is_none() {
            let r0vm = [".local/bin/r0vm", ".risc0/bin/r0vm", ".cargo/bin/r0vm"]
                .iter()
                .filter_map(|rel| home().map(|home| home.join(rel)))
                .find(|path| is_executable(path));
            if let Some(r0vm) = r0vm {
                command.env("RISC0_SERVER_PATH", r0vm);
            }
        }
        {
            use std::os::unix::process::CommandExt;
            // Its own process group, so it outlives a reload of the module.
            command.process_group(0);
        }
        let mut child = command.spawn().map_err(|e| format!("cannot run {}: {e}", cli.display()))?;
        let meta = json!({ "args": args, "writer": writer, "pid": child.id(), "started": now_ms() });
        std::fs::write(dir.join("job.json"), meta.to_string()).map_err(|e| e.to_string())?;
        // Reap it when it ends, and record how.
        let status_path = dir.join("exit");
        std::thread::spawn(move || {
            let code = child.wait().ok().and_then(|status| status.code()).unwrap_or(-1);
            std::fs::write(status_path, code.to_string()).ok();
        });
        Ok(json!({ "job": id }))
    }

    fn job_state(&self, id: &str) -> Result<Value, String> {
        if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit() || b == b'-') {
            return Err("not a job id".to_owned());
        }
        let dir = self.jobs_dir()?.join(id);
        let meta: Value = std::fs::read(dir.join("job.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .ok_or_else(|| format!("no job {id}"))?;
        let out = std::fs::read_to_string(dir.join("out.json")).unwrap_or_default();
        let result = out.lines().rev().find_map(|line| serde_json::from_str::<Value>(line).ok());
        let log = std::fs::read(dir.join("log.txt")).unwrap_or_default();
        let tail = String::from_utf8_lossy(&log[log.len().saturating_sub(TAIL_BYTES)..]).into_owned();
        let pid = meta["pid"].as_i64().and_then(|pid| i32::try_from(pid).ok()).unwrap_or(0);
        let ended = dir.join("exit").is_file() || !alive(pid);
        let (state, value, error) = match result {
            Some(result) if result["success"] == true => ("done", result["value"].clone(), Value::Null),
            Some(result) => ("failed", Value::Null, result["error"].clone()),
            None if ended => {
                let last = tail.lines().rev().find(|line| !line.trim().is_empty()).unwrap_or("it stopped without an answer");
                ("failed", Value::Null, json!(last))
            }
            None => ("running", Value::Null, Value::Null),
        };
        Ok(json!({
            "id": id,
            "args": meta["args"],
            "writer": meta["writer"],
            "started": meta["started"],
            "state": state,
            "value": value,
            "error": error,
            "log": tail,
        }))
    }

    fn job_list(&self) -> Vec<Value> {
        let Ok(dir) = self.jobs_dir() else { return Vec::new() };
        let mut ids: Vec<String> = std::fs::read_dir(dir)
            .map(|entries| entries.filter_map(|entry| entry.ok()?.file_name().into_string().ok()).collect())
            .unwrap_or_default();
        ids.sort_by_key(|id| std::cmp::Reverse(id.split('-').next().and_then(|ms| ms.parse::<u64>().ok()).unwrap_or(0)));
        ids.iter().take(20).filter_map(|id| self.job_state(id).ok()).collect()
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| u64::try_from(since.as_millis()).unwrap_or(u64::MAX))
}

/// Only the CLI's own NFT commands.
fn check_args(args: &[String]) -> Result<(), String> {
    let Some(first) = args.first() else { return Err("no command".to_owned()) };
    if !COMMANDS.contains(&first.as_str()) {
        return Err(format!("not an NFT command: {first}"));
    }
    if args.iter().any(|arg| arg.contains('\0')) {
        return Err("an argument holds a NUL byte".to_owned());
    }
    Ok(())
}

/// `path` names a file directly inside `files` (which may not exist yet).
fn in_folder(files: &Path, path: &str) -> Result<(), String> {
    let path = Path::new(path);
    let name = path.file_name().ok_or("a file path needs a file name")?;
    let parent = path.parent().filter(|parent| !parent.as_os_str().is_empty()).ok_or("give the full path")?;
    let parent = parent.canonicalize().map_err(|_| format!("no folder {}", parent.display()))?;
    if parent != files || name == ".." || name == "." {
        return Err(format!("the app only reads and writes keys and sealed files in {}", files.display()));
    }
    // A symlink there could point anywhere.
    if std::fs::symlink_metadata(parent.join(name)).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return Err("that file is a symbolic link".to_owned());
    }
    Ok(())
}

/// A regular file of at most `MAX_IMAGE_BYTES` whose bytes are an image format.
fn is_image(path: &Path) -> bool {
    path.metadata().is_ok_and(|meta| meta.is_file() && meta.len() <= MAX_IMAGE_BYTES)
        && image::ImageReader::open(path)
            .ok()
            .and_then(|reader| reader.with_guessed_format().ok())
            .is_some_and(|reader| reader.format().is_some())
}

/// Decoding limits, so a crafted image can't claim enormous dimensions and
/// take all the memory.
fn image_limits() -> image::Limits {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(12_000);
    limits.max_image_height = Some(12_000);
    limits.max_alloc = Some(600 * 1024 * 1024);
    limits
}

/// The sequencer must be `https://`, or `http://` on this machine.
fn check_rpc(rpc: &str) -> Result<(), String> {
    let local = ["http://127.0.0.1", "http://localhost", "http://[::1]"]
        .iter()
        .any(|prefix| rpc.strip_prefix(prefix).is_some_and(|rest| rest.is_empty() || rest.starts_with([':', '/'])));
    if rpc.starts_with("https://") || local {
        Ok(())
    } else {
        Err("the sequencer must be an https:// address (or http:// on this machine)".to_owned())
    }
}

fn thumbnail(path: &str, max: i64) -> Result<Value, String> {
    let max = u32::try_from(max.clamp(1, 512)).unwrap_or(256);
    let bytes = std::fs::metadata(path).map_err(|e| format!("cannot read the image: {e}"))?.len();
    if bytes > MAX_IMAGE_BYTES {
        return Err(format!("the image is over {} MB", MAX_IMAGE_BYTES >> 20));
    }
    // Downloads are named by CID, with no extension, so the format comes from the bytes.
    let mut reader = image::ImageReader::open(path)
        .and_then(image::ImageReader::with_guessed_format)
        .map_err(|e| format!("cannot read the image: {e}"))?;
    reader.limits(image_limits());
    let image = reader
        .decode()
        .map_err(|e| format!("cannot read the image: {e}"))?;
    let image = if image.width() > max || image.height() > max { image.thumbnail(max, max) } else { image };
    let rgba = image.to_rgba8();
    Ok(json!({ "width": rgba.width(), "height": rgba.height(), "pixels": base64(rgba.as_raw()) }))
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = u32::from(chunk[0]) << 16 | u32::from(*chunk.get(1).unwrap_or(&0)) << 8 | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            out.push(if i <= chunk.len() { TABLE[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

/// A CID as a file name: letters and digits only.
fn check_cid(cid: &str) -> Result<(), String> {
    if cid.is_empty() || cid.len() > 128 || !cid.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(format!("not a CID: {cid}"));
    }
    Ok(())
}

impl LezNftModule for LezNft {
    fn get_config(&mut self) -> String {
        ok(self.config())
    }

    fn set_config(&mut self, cli: String, wallet: String, program: String, rpc: String) -> String {
        answer(self.set_config_checked(&cli, &wallet, &program, &rpc))
    }

    fn start(&mut self, args_json: String) -> String {
        answer(self.start_job(&args_json))
    }

    fn job(&mut self, id: String) -> String {
        answer(self.job_state(&id))
    }

    fn jobs(&mut self) -> String {
        ok(Value::Array(self.job_list()))
    }

    fn thumbnail(&mut self, path: String, max: i64) -> String {
        answer(thumbnail(&path, max))
    }

    fn file_info(&mut self, path: String) -> String {
        let meta = std::fs::metadata(&path).ok().filter(|meta| meta.is_file());
        let bytes = meta.as_ref().map_or(0, std::fs::Metadata::len);
        let image = meta.is_some() && bytes <= MAX_IMAGE_BYTES && image::ImageReader::open(&path)
            .ok()
            .and_then(|reader| reader.with_guessed_format().ok())
            .is_some_and(|reader| reader.format().is_some());
        ok(json!({ "exists": meta.is_some(), "bytes": bytes, "image": image, "too_big": bytes > MAX_IMAGE_BYTES }))
    }

    fn remove_file(&mut self, path: String) -> String {
        answer(self.remove_in_files(&path))
    }

    fn cid_path(&mut self, cid: String, suffix: String) -> String {
        answer((|| {
            check_cid(&cid)?;
            if !suffix.bytes().all(|b| b.is_ascii_alphanumeric()) {
                return Err("the suffix is letters and digits".to_owned());
            }
            let name = if suffix.is_empty() { cid } else { format!("{cid}.{suffix}") };
            let path = self.files_dir()?.join(name);
            let exists = path.metadata().is_ok_and(|meta| meta.is_file() && meta.len() > 0);
            Ok(json!({ "path": path.display().to_string(), "exists": exists }))
        })())
    }
}

impl LezNft {
    fn remove_in_files(&self, path: &str) -> Result<Value, String> {
        let files = self.files_dir()?.canonicalize().map_err(|e| e.to_string())?;
        let Ok(target) = Path::new(path).canonicalize() else { return Ok(json!(false)) };
        if target.parent() != Some(files.as_path()) {
            return Err("only files in the app's files folder can be removed".to_owned());
        }
        std::fs::remove_file(&target).map_err(|e| e.to_string())?;
        Ok(json!(true))
    }
}

#[cfg(not(test))]
#[no_mangle]
pub extern "Rust" fn logos_module_install() {
    install::<LezNft>();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| (*arg).to_owned()).collect()
    }

    #[test]
    fn only_nft_commands_run() {
        assert!(check_args(&args(&["portfolio"])).is_ok());
        assert!(check_args(&args(&["mint", "--content", "{}"])).is_ok());
        // The wallet passthrough could send the app's funds anywhere.
        assert!(check_args(&args(&["wallet", "auth-transfer", "send", "--amount", "5"])).is_err());
        assert!(check_args(&args(&["wallet", "account", "sync-private"])).is_err());
        assert!(check_args(&args(&["rm"])).is_err());
        assert!(check_args(&[]).is_err());
    }

    #[test]
    fn files_stay_in_the_files_folder() {
        let module = LezNft::default();
        let files = module.files_dir().unwrap();
        let inside = files.join("abc.key").display().to_string();
        let outside = std::env::temp_dir().join("elsewhere.key").display().to_string();
        let escape = files.join("../escape.key").display().to_string();
        assert!(module.check_paths(&args(&["seal-file", "--in", "/no/such.png", "--out", &inside])).is_err());
        assert!(module.check_paths(&args(&["open-file", "--token-id", "00", "--in", &inside, "--out", &inside])).is_ok());
        assert!(module.check_paths(&args(&["open-file", "--token-id", "00", "--in", &inside, "--out", &outside])).is_err());
        assert!(module.check_paths(&args(&["open-file", "--token-id", "00", "--in", &inside, "--out", &escape])).is_err());
        assert!(module.check_paths(&args(&["open-file", "--token-id", "00", "--in", "/etc/hosts", "--out", &inside])).is_err());
        assert!(module.check_paths(&args(&["mint", "--content", "{}", "--encrypt", "--key-file", &outside])).is_err());
        assert!(module.check_paths(&args(&["mint", "--content", "@/etc/hosts"])).is_err());
        assert!(module.check_paths(&args(&["create", "--content", "@x", "--supply", "1"])).is_err());
        assert!(module.check_paths(&args(&["open-file", &format!("--out={outside}")])).is_err());
        assert!(module.check_paths(&args(&["mint", "--content", "{\"name\":\"a\"}"])).is_ok());
    }

    #[test]
    fn only_safe_sequencers() {
        assert!(check_rpc("https://testnet.lez.logos.co").is_ok());
        assert!(check_rpc("http://127.0.0.1:3040").is_ok());
        assert!(check_rpc("http://localhost").is_ok());
        assert!(check_rpc("http://localhost.evil.com").is_err());
        assert!(check_rpc("http://example.com").is_err());
        assert!(check_rpc("file:///etc").is_err());
    }

    #[test]
    fn config_cannot_swap_the_cli_wallet_or_program() {
        let mut module = LezNft::default();
        assert!(module.set_config_checked("/tmp/evil", "", "", "").is_err());
        assert!(module.set_config_checked("", "/tmp/other-wallet", "", "").is_err());
        assert!(module.set_config_checked("", "", "SomeOtherProgram", "").is_err());
        assert!(module.set_config_checked("", "", "", "http://example.com").is_err());
        assert!(module.set_config_checked("", "", "", "https://testnet.lez.logos.co").is_ok());
        assert!(module.cli.is_none() && module.wallet.is_none() && module.program.is_none());
    }

    #[test]
    fn cids_are_plain() {
        assert!(check_cid("zDvZRwzm8aBcD123").is_ok());
        assert!(check_cid("../etc/passwd").is_err());
        assert!(check_cid("").is_err());
    }

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
    }

    #[test]
    fn a_thumbnail_is_rgba_at_most_max() {
        // Named like a download: by CID, with no extension.
        let path = std::env::temp_dir().join(format!("lez-nft-thumb-{}", std::process::id()));
        image::RgbaImage::from_pixel(40, 20, image::Rgba([1, 2, 3, 255]))
            .save_with_format(&path, image::ImageFormat::Png)
            .unwrap();
        let value = thumbnail(path.to_str().unwrap(), 10).unwrap();
        assert_eq!(value["width"], 10);
        assert_eq!(value["height"], 5);
        assert_eq!(value["pixels"].as_str().unwrap().len(), (10 * 5 * 4usize).div_ceil(3) * 4);
        std::fs::remove_file(path).ok();
    }
}
