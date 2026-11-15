//! Encrypted wallet backups (Settings → Backup): ONE file per backup, never an unencrypted copy.
//!
//! File: `LNSBAK01` (8 bytes) | header length (u32, big-endian) | header (JSON, plain) | body.
//! The header says what a future build needs to open it: `format`, `kdf` (Argon2id and its
//! parameters), `salt`, `cipher` (AES-256-GCM), `nonce`, `networks` and `created`. The body is the
//! payload encrypted with AES-256-GCM (its 16-byte tag at the end), under a key derived from the
//! password with Argon2id; everything before the body (magic, header length, header) is its
//! associated data, so changing any of it breaks it. The payload is JSON: each network's wallet files (hex) and its account summary.
//!
//! What a wallet folder contributes (`wallet_files`): its top-level `*.json` files (the store
//! `storage.json` with the keys, `wallet_config.json`, the CLI's own `names.json`) and `storage.*`
//! (`storage.nullifiers`): everything the CLI needs to open the same wallet. Left out: program
//! binaries (`*.bin`, downloaded again), the lock file, sub-folders (logs, proof caches).
//!
//! Nothing here logs, keeps or returns the password or a decrypted byte except the account
//! summary and the file names; decrypted buffers are overwritten before they are dropped. A
//! backup file is written mode 600, through a temporary file of its own in the same folder
//! (encrypted bytes only), then renamed into place.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use serde_json::{json, Map, Value};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const MAGIC: &[u8; 8] = b"LNSBAK01";
pub const FORMAT: u64 = 1;
/// Argon2id: 64 MiB, 3 passes, 1 lane (about half a second on the Mini).
pub const KDF_M_KIB: u32 = 65_536;
pub const KDF_T: u32 = 3;
pub const KDF_P: u32 = 1;
/// The shortest password a backup takes.
pub const MIN_PASSWORD: usize = 10;
/// The largest backup file read back (a wallet folder is well under a megabyte).
const MAX_FILE: u64 = 64 << 20;
/// The most files and bytes one wallet folder may put in a backup.
const MAX_WALLET_FILES: usize = 32;
const MAX_WALLET_BYTES: u64 = 16 << 20;

/// What the CLI needs from a wallet folder: the store and its siblings, nothing else.
pub fn wanted(name: &str) -> bool {
    let plain = !name.is_empty() && !name.starts_with('.') && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-');
    plain && (name.ends_with(".json") || name.starts_with("storage."))
}

/// A wallet folder's files for a backup: `[(name, bytes)]`, names sorted.
pub fn wallet_files(dir: &Path) -> Result<Vec<(String, Vec<u8>)>, String> {
    let entries = std::fs::read_dir(dir).map_err(|_| format!("the wallet folder {} can't be read", dir.display()))?;
    let mut out = Vec::new();
    let mut total = 0u64;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let kind = entry.file_type().map_err(|_| format!("{name} in the wallet folder can't be read"))?;
        if !kind.is_file() || !wanted(&name) {
            continue;
        }
        let bytes = std::fs::read(entry.path()).map_err(|_| format!("{name} in the wallet folder can't be read"))?;
        total += bytes.len() as u64;
        out.push((name, bytes));
        if out.len() > MAX_WALLET_FILES || total > MAX_WALLET_BYTES {
            return Err(format!("the wallet folder {} holds more than a wallet; nothing was backed up", dir.display()));
        }
    }
    if !out.iter().any(|(name, _)| name == "storage.json") {
        return Err(format!("there is no wallet in {}", dir.display()));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

/// An account ID as the store writes it: base58 text, or 32 bytes.
fn id_text(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        return Some(text.to_owned());
    }
    let bytes: Vec<u8> = value.as_array()?.iter().map(|b| b.as_u64().and_then(|b| u8::try_from(b).ok())).collect::<Option<_>>()?;
    (bytes.len() == 32).then(|| bs58::encode(bytes).into_string())
}

/// The accounts a store holds, read from its `key_chain.accounts` (only their kind and ID; no
/// key leaves this function): `[{account, kind, imported}]`. An imported private account's ID
/// isn't written in the store: it is listed without one.
pub fn accounts_of(store: &[u8]) -> Vec<Value> {
    let Ok(parsed) = serde_json::from_slice::<Value>(store) else { return Vec::new() };
    let list = parsed["key_chain"]["accounts"].as_array().cloned().unwrap_or_default();
    list.iter()
        .filter_map(|entry| {
            let (tag, body) = entry.as_object()?.iter().next()?;
            let (kind, imported) = match tag.as_str() {
                "Public" => ("public", false),
                "Private" => ("private", false),
                "ImportedPublic" => ("public", true),
                "ImportedPrivate" => ("private", true),
                _ => return None,
            };
            let id = id_text(&body["account_id"]).map(|id| format!("{}/{id}", if kind == "public" { "Public" } else { "Private" }));
            Some(json!({ "account": id, "kind": kind, "imported": imported }))
        })
        .collect()
}

fn key(password: &str, salt: &[u8]) -> Result<[u8; 32], String> {
    let params = Params::new(KDF_M_KIB, KDF_T, KDF_P, Some(32)).map_err(|_| "the backup's key settings are not usable".to_owned())?;
    let mut key = [0u8; 32];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|_| "the backup's key could not be made".to_owned())?;
    Ok(key)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok()).collect()
}

/// Overwrites a buffer before it is dropped (best effort: the payload held keys).
fn wipe(mut bytes: Vec<u8>) {
    for b in bytes.iter_mut() {
        // A volatile write, so the overwrite isn't optimised away.
        unsafe { std::ptr::write_volatile(b, 0) };
    }
}

/// One network for a backup: its id, the wallet folder's files, and what else to keep.
pub struct Part {
    pub network: String,
    pub files: Vec<(String, Vec<u8>)>,
}

/// The encrypted backup of `parts` under `password`, made at `created` (seconds): the file's bytes.
pub fn seal(parts: &[Part], password: &str, created: u64) -> Result<Vec<u8>, String> {
    if password.chars().count() < MIN_PASSWORD {
        return Err(format!("the password must be at least {MIN_PASSWORD} characters"));
    }
    if parts.is_empty() {
        return Err("nothing to back up".to_owned());
    }
    let mut salt = [0u8; 16];
    let mut nonce = [0u8; 12];
    getrandom::getrandom(&mut salt).map_err(|_| "no random numbers for the backup".to_owned())?;
    getrandom::getrandom(&mut nonce).map_err(|_| "no random numbers for the backup".to_owned())?;
    let networks: Vec<&str> = parts.iter().map(|p| p.network.as_str()).collect();
    let header = json!({
        "format": FORMAT, "app": "Names Wallet",
        "kdf": { "alg": "argon2id", "version": 19, "m_kib": KDF_M_KIB, "t": KDF_T, "p": KDF_P, "salt": hex(&salt) },
        "cipher": "aes-256-gcm", "nonce": hex(&nonce), "networks": networks, "created": created,
    })
    .to_string();
    let payload = json!({
        "format": FORMAT, "created": created,
        "networks": parts.iter().map(|p| {
            let store = p.files.iter().find(|(n, _)| n == "storage.json").map(|(_, b)| b.as_slice()).unwrap_or(&[]);
            json!({ "id": p.network, "accounts": accounts_of(store),
                    "files": p.files.iter().map(|(n, b)| (n.clone(), Value::from(hex(b)))).collect::<Map<String, Value>>() })
        }).collect::<Vec<_>>(),
    });
    let plain = serde_json::to_vec(&payload).map_err(|_| "the backup could not be put together".to_owned())?;
    drop(payload);
    let mut k = key(password, &salt)?;
    let cipher = Aes256Gcm::new_from_slice(&k).map_err(|_| "the backup's cipher could not start".to_owned())?;
    k.fill(0);
    // Everything before the body (magic, header length, header) is the associated data, exactly
    // the bytes `open` reads back as it.
    let mut out = Vec::with_capacity(12 + header.len() + plain.len() + 16);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(header.len() as u32).to_be_bytes());
    out.extend_from_slice(header.as_bytes());
    let sealed = cipher.encrypt(Nonce::from_slice(&nonce), Payload { msg: &plain, aad: &out });
    wipe(plain);
    let body = sealed.map_err(|_| "the backup could not be encrypted".to_owned())?;
    out.extend_from_slice(&body);
    Ok(out)
}

/// A backup file's plain header (no password needed): format, KDF settings, networks, created.
pub fn header(file: &[u8]) -> Result<Value, String> {
    let bad = || "this isn't a Names Wallet backup file, or it is damaged".to_owned();
    if file.len() < 12 || &file[..8] != MAGIC {
        return Err(bad());
    }
    let len = u32::from_be_bytes(file[8..12].try_into().map_err(|_| bad())?) as usize;
    let text = file.get(12..12 + len).ok_or_else(bad)?;
    let header: Value = serde_json::from_slice(text).map_err(|_| bad())?;
    if header["format"].as_u64() != Some(FORMAT) {
        return Err("this backup was made by a newer Names Wallet; update the app to open it".to_owned());
    }
    Ok(header)
}

/// What a backup holds, opened in memory with `password`: the payload (each network's `id`,
/// `accounts` and `files`). The caller wipes nothing: the files' bytes are hex inside it.
pub fn open(file: &[u8], password: &str) -> Result<Value, String> {
    let head = header(file)?;
    let bad = || "this isn't a Names Wallet backup file, or it is damaged".to_owned();
    let len = u32::from_be_bytes(file[8..12].try_into().map_err(|_| bad())?) as usize;
    let kdf = &head["kdf"];
    if kdf["alg"] != "argon2id" || head["cipher"] != "aes-256-gcm" {
        return Err("this backup uses a kind of encryption this version can't open".to_owned());
    }
    let m = kdf["m_kib"].as_u64().and_then(|v| u32::try_from(v).ok()).filter(|m| (8..=1 << 22).contains(m)).ok_or_else(bad)?;
    let t = kdf["t"].as_u64().and_then(|v| u32::try_from(v).ok()).filter(|t| (1..=16).contains(t)).ok_or_else(bad)?;
    let p = kdf["p"].as_u64().and_then(|v| u32::try_from(v).ok()).filter(|p| (1..=8).contains(p)).ok_or_else(bad)?;
    let salt = kdf["salt"].as_str().and_then(unhex).filter(|s| s.len() >= 16).ok_or_else(bad)?;
    let nonce = head["nonce"].as_str().and_then(unhex).filter(|n| n.len() == 12).ok_or_else(bad)?;
    let params = Params::new(m, t, p, Some(32)).map_err(|_| bad())?;
    let mut k = [0u8; 32];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params).hash_password_into(password.as_bytes(), &salt, &mut k).map_err(|_| bad())?;
    let cipher = Aes256Gcm::new_from_slice(&k).map_err(|_| bad())?;
    k.fill(0);
    let aad = &file[..12 + len];
    let plain = cipher
        .decrypt(Nonce::from_slice(&nonce), Payload { msg: &file[12 + len..], aad })
        .map_err(|_| "the password is wrong, or the file is damaged".to_owned())?;
    let parsed = serde_json::from_slice::<Value>(&plain);
    wipe(plain);
    parsed.map_err(|_| bad())
}

/// One network's files out of an opened payload: `[(name, bytes)]`, each name checked.
pub fn files_of(network: &Value) -> Result<Vec<(String, Vec<u8>)>, String> {
    let bad = || "the backup's wallet files are damaged".to_owned();
    let files = network["files"].as_object().ok_or_else(bad)?;
    let mut out = Vec::new();
    for (name, text) in files {
        if !wanted(name) {
            return Err(bad());
        }
        out.push((name.clone(), text.as_str().and_then(unhex).ok_or_else(bad)?));
    }
    if !out.iter().any(|(n, _)| n == "storage.json") {
        return Err(bad());
    }
    Ok(out)
}

/// Reads a backup file (at most 64 MiB).
pub fn read_file(path: &Path) -> Result<Vec<u8>, String> {
    let meta = std::fs::metadata(path).map_err(|_| format!("{} isn't there", path.display()))?;
    if !meta.is_file() || meta.len() > MAX_FILE {
        return Err(format!("{} isn't a backup file", path.display()));
    }
    std::fs::read(path).map_err(|_| format!("{} can't be read", path.display()))
}

#[cfg(unix)]
fn private_file(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(path)
}
#[cfg(not(unix))]
fn private_file(path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new().write(true).create_new(true).open(path)
}

#[cfg(unix)]
fn private_dir(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(path)
}
#[cfg(not(unix))]
fn private_dir(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)
}

/// Writes a sealed backup to `path` (mode 600): never over a file that is there, through a
/// temporary file of its own beside it (the encrypted bytes only), renamed into place.
pub fn write_file(path: &Path, sealed: &[u8]) -> Result<(), String> {
    if path.exists() {
        return Err(format!("{} is there already; choose another name", path.display()));
    }
    let dir = path.parent().filter(|d| !d.as_os_str().is_empty()).ok_or("a backup needs a folder to go in")?;
    private_dir(dir).map_err(|_| format!("the folder {} can't be made", dir.display()))?;
    let mut nonce = [0u8; 6];
    getrandom::getrandom(&mut nonce).map_err(|_| "no random numbers for the backup".to_owned())?;
    let temp = dir.join(format!(".names-backup-{}.part", hex(&nonce)));
    let result = (|| {
        let mut file = private_file(&temp).map_err(|_| format!("a file can't be written in {}", dir.display()))?;
        file.write_all(sealed).and_then(|_| file.sync_all()).map_err(|_| format!("the backup couldn't be written in {}", dir.display()))?;
        std::fs::rename(&temp, path).map_err(|_| format!("the backup couldn't be put at {}", path.display()))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

/// Puts a backup's wallet files into `dir`, which must hold no wallet: the folder is made (700)
/// if missing, and each file is written new (600), never over one that is there.
pub fn restore_into(dir: &Path, files: &[(String, Vec<u8>)]) -> Result<Vec<String>, String> {
    if dir.join("storage.json").exists() {
        return Err(format!("there is a wallet in {} already, so nothing was restored or changed", dir.display()));
    }
    if let Some((name, _)) = files.iter().find(|(name, _)| dir.join(name).exists()) {
        return Err(format!("{name} is in {} already, so nothing was restored or changed", dir.display()));
    }
    private_dir(dir).map_err(|_| format!("the wallet folder {} can't be made", dir.display()))?;
    let mut written = Vec::new();
    // The store last: a folder without storage.json is no wallet, so a half-done restore
    // never looks like one.
    let mut order: Vec<&(String, Vec<u8>)> = files.iter().collect();
    order.sort_by_key(|(name, _)| name == "storage.json");
    for (name, bytes) in order {
        let path = dir.join(name);
        let mut file = match private_file(&path) {
            Ok(file) => file,
            Err(_) => {
                undo(dir, &written);
                return Err(format!("{name} can't be written in {}; nothing was restored", dir.display()));
            }
        };
        if file.write_all(bytes).and_then(|_| file.sync_all()).is_err() {
            written.push(name.clone());
            undo(dir, &written);
            return Err(format!("{name} couldn't be written in {}; nothing was restored", dir.display()));
        }
        written.push(name.clone());
    }
    Ok(written)
}

fn undo(dir: &Path, written: &[String]) {
    for name in written {
        let _ = std::fs::remove_file(dir.join(name));
    }
}

/// The folder backups go to when no other is chosen: `~/Documents/Names Wallet Backups`.
pub fn default_folder() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| Path::new(&home).join("Documents").join("Names Wallet Backups"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("names-backup-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    const MARKER: &str = "MARKER-7f3c9e-NOT-ENCRYPTED";
    const PASSWORD: &str = "correct horse battery";

    /// A wallet folder like the CLI's, with a known marker in the store, an imported account,
    /// and files a backup leaves out.
    fn wallet(dir: &Path) {
        std::fs::create_dir_all(dir.join("logs")).unwrap();
        let store = json!({ "key_chain": { "accounts": [
            { "Public": { "account_id": "CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", "chain_index": "/0", "data": { "secret": MARKER } } },
            { "ImportedPublic": { "account_id": "7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR", "pub_sign_key": MARKER } },
            { "Private": { "account_id": vec![7u8; 32], "chain_index": "/1", "data": {} } },
            { "ImportedPrivate": { "account": {}, "key_chain": {} } } ] }, "last_synced_block": 5 });
        std::fs::write(dir.join("storage.json"), store.to_string()).unwrap();
        std::fs::write(dir.join("storage.nullifiers"), b"nullifiers").unwrap();
        std::fs::write(dir.join("wallet_config.json"), b"{\"sequencer\":{}}").unwrap();
        std::fs::write(dir.join("names.json"), b"{\"names\":[]}").unwrap();
        std::fs::write(dir.join("logos-names-X.bin"), b"program").unwrap();
        std::fs::write(dir.join("names-wallet.lock"), b"").unwrap();
        std::fs::write(dir.join("logs").join("log.txt"), MARKER).unwrap();
    }

    fn contains(hay: &[u8], needle: &[u8]) -> bool {
        hay.windows(needle.len()).any(|w| w == needle)
    }

    #[test]
    fn a_backup_round_trips_and_leaves_nothing_plain() {
        let home = scratch("round");
        let live = home.join("lez-0.4").join("wallet");
        wallet(&live);
        let files = wallet_files(&live).unwrap();
        let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["names.json", "storage.json", "storage.nullifiers", "wallet_config.json"], "no binaries, locks or logs");
        let sealed = seal(&[Part { network: "lez-0.4".into(), files: files.clone() }], PASSWORD, 1_794_700_000).unwrap();
        assert!(!contains(&sealed, MARKER.as_bytes()), "the marker never appears in the file");
        assert!(!contains(&sealed, PASSWORD.as_bytes()), "nor the password");
        let file = home.join("Backups").join("b.lnsbackup");
        write_file(&file, &sealed).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
        }
        // The header is plain and says how to open it.
        let head = header(&std::fs::read(&file).unwrap()).unwrap();
        assert_eq!((head["kdf"]["alg"].as_str(), head["kdf"]["m_kib"].as_u64(), head["cipher"].as_str()), (Some("argon2id"), Some(65_536), Some("aes-256-gcm")));
        assert_eq!(head["networks"], json!(["lez-0.4"]));
        // Opened, it holds the accounts (the imported ones marked) and the same files.
        let payload = open(&read_file(&file).unwrap(), PASSWORD).unwrap();
        let net = &payload["networks"][0];
        let accounts = net["accounts"].as_array().unwrap();
        assert_eq!(accounts.len(), 4);
        assert_eq!(accounts.iter().filter(|a| a["imported"] == true).count(), 2);
        assert_eq!(accounts[0]["account"], "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk");
        assert_eq!(accounts[2]["account"], format!("Private/{}", bs58::encode([7u8; 32]).into_string()));
        assert_eq!(accounts[3]["account"], Value::Null, "an imported private account's ID isn't in the store");
        // Restored into an empty tree: byte for byte.
        let target = home.join("restored").join("lez-0.4").join("wallet");
        let written = restore_into(&target, &files_of(net).unwrap()).unwrap();
        assert_eq!(written.len(), 4);
        for (name, bytes) in &files {
            assert_eq!(&std::fs::read(target.join(name)).unwrap(), bytes, "{name}");
        }
        // Never over a wallet that is there.
        assert!(restore_into(&target, &files_of(net).unwrap()).unwrap_err().contains("already"));
        assert!(restore_into(&live, &files_of(net).unwrap()).unwrap_err().contains("already"));
        // No plain copy anywhere in the scratch home (the backup folder included) but the wallets
        // themselves, nor a leftover temporary file beside the temp folder's backups.
        let leftovers: Vec<PathBuf> = std::fs::read_dir(std::env::temp_dir()).into_iter().flatten().flatten()
            .map(|entry| entry.path()).filter(|path| path.is_file() && path.to_string_lossy().contains("names-backup")).collect();
        for root in [home.clone()] {
            for entry in walk(&root).into_iter().chain(leftovers.iter().cloned()) {
                if entry.starts_with(&live) || entry.starts_with(&target) {
                    continue;
                }
                let bytes = std::fs::read(&entry).unwrap_or_default();
                assert!(!contains(&bytes, MARKER.as_bytes()) || entry.starts_with(live.join("logs")), "plain copy at {}", entry.display());
                assert!(!contains(&bytes, PASSWORD.as_bytes()), "password at {}", entry.display());
            }
        }
        std::fs::remove_dir_all(&home).unwrap();
    }

    fn walk(root: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let mut todo = vec![root.to_owned()];
        while let Some(dir) = todo.pop() {
            for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                let path = entry.path();
                match entry.file_type() {
                    Ok(t) if t.is_dir() => todo.push(path),
                    Ok(t) if t.is_file() => out.push(path),
                    _ => {}
                }
            }
        }
        out
    }

    #[test]
    fn a_wrong_password_a_changed_byte_or_a_cut_file_opens_nothing() {
        let home = scratch("bad");
        let live = home.join("w");
        wallet(&live);
        let sealed = seal(&[Part { network: "lez-0.3".into(), files: wallet_files(&live).unwrap() }], PASSWORD, 1).unwrap();
        assert!(open(&sealed, "wrong password!!").unwrap_err().contains("password is wrong"));
        let mut changed = sealed.clone();
        let last = changed.len() - 20;
        changed[last] ^= 1;
        assert!(open(&changed, PASSWORD).is_err(), "a changed byte in the body");
        let mut header_changed = sealed.clone();
        let at = header_changed.windows(7).position(|w| w == b"lez-0.3").unwrap();
        header_changed[at + 6] = b'4';
        assert!(open(&header_changed, PASSWORD).is_err(), "a changed header (it is authenticated)");
        assert!(open(&sealed[..sealed.len() - 1], PASSWORD).is_err(), "a cut file");
        assert!(open(&sealed[..10], PASSWORD).unwrap_err().contains("isn't a Names Wallet backup"));
        assert!(open(b"not a backup at all", PASSWORD).is_err());
        assert!(seal(&[Part { network: "lez-0.3".into(), files: Vec::new() }], "short", 1).unwrap_err().contains("at least"));
        // An existing file is never written over.
        let file = home.join("b.lnsbackup");
        write_file(&file, &sealed).unwrap();
        assert!(write_file(&file, &sealed).unwrap_err().contains("there already"));
        std::fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn only_wallet_files_are_taken_and_given_back() {
        assert!(wanted("storage.json") && wanted("storage.nullifiers") && wanted("wallet_config.json") && wanted("names.json"));
        assert!(!wanted("logos-names-X.bin") && !wanted("names-wallet.lock") && !wanted("../storage.json") && !wanted(".hidden.json") && !wanted("a/b.json"));
        let bad = json!({ "files": { "../../evil.json": "00", "storage.json": "7b7d" } });
        assert!(files_of(&bad).is_err(), "a name that leaves the folder is refused");
        assert!(wallet_files(&scratch("empty")).unwrap_err().contains("no wallet"));
    }
}
