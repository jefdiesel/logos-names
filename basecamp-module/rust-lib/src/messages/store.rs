//! What Messages keeps on this computer, in the module's folder (`messages/`): the messaging key (`msgkey.json`,
//! 0600: the X25519 secret and its public key, in hex) and the chats (`store.bin`, sealed with a key derived from
//! the messaging key, so a copy of the file alone shows nothing).
//!
//! Loss-safety, the phone's rules: every write goes through files::write_safely (temporary file, sync, rename,
//! the previous copy kept as `store.bin.prev`); a store file that is there but can't be opened is never written
//! over (`Load::Unreadable`: nothing is saved until the person picks "use the last saved copy" or "start
//! fresh", and each of those first sets the unreadable file aside under a name of its own).

use super::crypto;
use super::files;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

pub const STORE_VERSION: u32 = 1;
/// Message hashes remembered to drop repeats (the node replays history after a restart or a gap).
pub const SEEN_MAX: usize = 4096;
/// Requests (messages from someone not in the chats): at most this many senders, and messages per sender.
pub const REQ_SENDERS: usize = 50;
pub const REQ_PER_SENDER: usize = 3;
const MAGIC: &[u8; 4] = b"NMS1";
const STORE_INFO: &[u8] = b"names/basecamp-messages-store/v1";

/// The messaging key: this computer's own, made on turn-on. Its public key goes in the name's `msg` entry.
#[derive(Clone, Serialize, Deserialize)]
pub struct MsgKey {
    pub secret: String,
    pub public: String,
    pub created: i64,
}

impl MsgKey {
    pub fn new(now: i64) -> Result<MsgKey, String> {
        let mut secret = [0u8; 32];
        getrandom::getrandom(&mut secret).map_err(|e| e.to_string())?;
        Ok(MsgKey { secret: crypto::hex(&secret), public: crypto::hex(&crypto::public_key(&secret)), created: now })
    }
    pub fn secret_bytes(&self) -> [u8; 32] {
        crypto::unhex(&self.secret).and_then(|v| v.try_into().ok()).unwrap_or([0; 32])
    }
    pub fn public_bytes(&self) -> [u8; 32] {
        crypto::parse_msg_key(&self.public).unwrap_or([0; 32])
    }
    pub fn topic(&self) -> String {
        crypto::inbox_topic(&self.public_bytes())
    }
    /// The key file, if there is a readable one (an unreadable one is an error, never "no key").
    pub fn load(dir: &Path) -> Result<Option<MsgKey>, String> {
        let path = dir.join("msgkey.json");
        match std::fs::read(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("couldn't read the messaging key: {e}")),
            Ok(bytes) => {
                let k: MsgKey = serde_json::from_slice(&bytes).map_err(|_| "the messaging key file can't be read".to_string())?;
                let secret: [u8; 32] = crypto::unhex(&k.secret).and_then(|v| v.try_into().ok()).ok_or("the messaging key file can't be read")?;
                if crypto::hex(&crypto::public_key(&secret)) != k.public {
                    return Err("the messaging key file is damaged".into());
                }
                Ok(Some(k))
            }
        }
    }
    pub fn save(&self, dir: &Path) -> Result<(), String> {
        files::write_safely(&dir.join("msgkey.json"), &serde_json::to_vec(self).map_err(|e| e.to_string())?, false)
    }
}

/// Someone this computer has messages with, by their messaging address.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Contact {
    /// Their messaging address (hex).
    pub key: String,
    /// The LNS name it is known by: one it was started with, or a sender's claim the chain confirmed.
    #[serde(default)]
    pub name: Option<String>,
    /// In the chats (started here, written to, or accepted); false: a request.
    pub accepted: bool,
    /// The address the person compared the safety code for (null: not checked).
    #[serde(default)]
    pub checked: Option<String>,
    pub created: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Msg {
    /// Incoming: the ephemeral key (hex); outgoing: this computer's own id.
    pub id: String,
    /// The other side's messaging address.
    pub peer: String,
    /// "in" or "out".
    pub dir: String,
    /// The name it says it's from (incoming), or the name it was sent as (outgoing).
    pub from: String,
    pub text: String,
    pub ts: i64,
    /// Incoming: the sender holds `peer`'s secret key.
    #[serde(default)]
    pub authentic: bool,
    #[serde(default)]
    pub read: bool,
    /// Outgoing: "queued", "sending", "propagated", "sent" or "failed".
    #[serde(default)]
    pub state: Option<String>,
    /// When it came in (incoming), so the list sorts by arrival as well.
    #[serde(default)]
    pub received: i64,
}

/// A sealed message waiting to go out, and how its tries are going.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Outgoing {
    pub id: String,
    pub topic: String,
    pub payload: String,
    pub tries: u32,
    pub next_at: i64,
    #[serde(default)]
    pub request: Option<String>,
    pub state: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Store {
    pub v: u32,
    #[serde(default)]
    pub contacts: BTreeMap<String, Contact>,
    #[serde(default)]
    pub messages: Vec<Msg>,
    #[serde(default)]
    pub seen: VecDeque<String>,
    #[serde(default)]
    pub outbox: Vec<Outgoing>,
    #[serde(default)]
    pub blocked: BTreeSet<String>,
    /// Requests deleted unread because of the caps or a block, counted (shown as a number, never the text).
    #[serde(default)]
    pub dropped: u64,
}

pub enum Load {
    /// No store yet: a new one.
    Fresh(Store),
    Loaded(Store),
    /// There, but it can't be opened (damaged, another key's, another version). Nothing is saved over it.
    Unreadable { why: String, prev: bool },
}

fn store_key(msg_secret: &[u8; 32]) -> [u8; 32] {
    let mut out = [0u8; 32];
    hkdf::Hkdf::<sha2::Sha256>::new(None, msg_secret).expand(STORE_INFO, &mut out).expect("32 bytes");
    out
}

pub fn path(dir: &Path) -> PathBuf {
    dir.join("store.bin")
}

pub fn seal_store(store: &Store, msg_secret: &[u8; 32]) -> Result<Vec<u8>, String> {
    let mut nonce = [0u8; 24];
    getrandom::getrandom(&mut nonce).map_err(|e| e.to_string())?;
    let plain = serde_json::to_vec(store).map_err(|e| e.to_string())?;
    let sealed = XChaCha20Poly1305::new((&store_key(msg_secret)).into())
        .encrypt(XNonce::from_slice(&nonce), Payload { msg: &plain, aad: MAGIC })
        .map_err(|_| "couldn't seal the store".to_string())?;
    Ok([MAGIC.as_slice(), &nonce, &sealed].concat())
}

pub fn open_store(bytes: &[u8], msg_secret: &[u8; 32]) -> Result<Store, String> {
    if bytes.len() < 4 + 24 + 16 || &bytes[..4] != MAGIC {
        return Err("not a saved chats file".into());
    }
    let plain = XChaCha20Poly1305::new((&store_key(msg_secret)).into())
        .decrypt(XNonce::from_slice(&bytes[4..28]), Payload { msg: &bytes[28..], aad: MAGIC })
        .map_err(|_| "the saved chats can't be opened with this messaging key".to_string())?;
    let store: Store = serde_json::from_slice(&plain).map_err(|_| "the saved chats file is damaged".to_string())?;
    if store.v != STORE_VERSION {
        return Err(format!("the saved chats are version {}, this app reads {STORE_VERSION}", store.v));
    }
    Ok(store)
}

pub fn load(dir: &Path, msg_secret: &[u8; 32]) -> Load {
    let p = path(dir);
    match std::fs::read(&p) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Load::Fresh(Store { v: STORE_VERSION, ..Default::default() }),
        Err(e) => Load::Unreadable { why: format!("couldn't read the saved chats: {e}"), prev: files::prev_of(&p).exists() },
        Ok(bytes) => match open_store(&bytes, msg_secret) {
            Ok(s) => Load::Loaded(s),
            Err(why) => Load::Unreadable { why, prev: files::prev_of(&p).exists() },
        },
    }
}

pub fn save(dir: &Path, store: &Store, msg_secret: &[u8; 32]) -> Result<(), String> {
    files::write_safely(&path(dir), &seal_store(store, msg_secret)?, true)
}

/// "Use the last saved copy": sets the unreadable file aside and puts the previous copy in its place, if that one
/// opens. Nothing changes otherwise.
pub fn use_prev(dir: &Path, msg_secret: &[u8; 32]) -> Result<Store, String> {
    let p = path(dir);
    let prev = std::fs::read(files::prev_of(&p)).map_err(|_| "there is no earlier copy".to_string())?;
    let store = open_store(&prev, msg_secret)?;
    if p.exists() {
        files::set_aside(&p, "unreadable")?;
    }
    files::write_safely(&p, &prev, false)?;
    Ok(store)
}

/// "Start fresh": sets the unreadable file aside (kept, never deleted) and starts an empty store.
pub fn start_fresh(dir: &Path, msg_secret: &[u8; 32]) -> Result<Store, String> {
    let p = path(dir);
    if p.exists() {
        files::set_aside(&p, "set-aside")?;
    }
    let store = Store { v: STORE_VERSION, ..Default::default() };
    save(dir, &store, msg_secret)?;
    Ok(store)
}

impl Store {
    /// Remembers a message hash; false if it was seen already (a repeat: history replay, or a second copy).
    pub fn first_sight(&mut self, hash: &str) -> bool {
        if hash.is_empty() || self.seen.iter().any(|h| h == hash) {
            return false;
        }
        self.seen.push_back(hash.to_owned());
        while self.seen.len() > SEEN_MAX {
            self.seen.pop_front();
        }
        true
    }

    /// Files an opened incoming message: into its chat if the sender is in the chats, else as a request (within
    /// the caps); dropped from a blocked address, or past the caps (counted). Returns whether it was kept.
    pub fn file_incoming(&mut self, m: &crypto::Opened, now: i64) -> bool {
        if self.blocked.contains(&m.from_key) || !m.authentic {
            // (an unauthenticated message can't be told from anyone: it is dropped, as the phone files it nowhere)
            self.dropped += 1;
            return false;
        }
        if self.messages.iter().any(|x| x.dir == "in" && x.id == m.id) {
            return false;
        }
        let known = self.contacts.get(&m.from_key).is_some_and(|c| c.accepted);
        if !known {
            let senders = self.contacts.values().filter(|c| !c.accepted).count();
            let theirs = self.messages.iter().filter(|x| x.peer == m.from_key).count();
            let new_sender = !self.contacts.contains_key(&m.from_key);
            if (new_sender && senders >= REQ_SENDERS) || theirs >= REQ_PER_SENDER {
                self.dropped += 1;
                return false;
            }
            self.contacts.entry(m.from_key.clone()).or_insert(Contact { key: m.from_key.clone(), name: None, accepted: false, checked: None, created: now });
        }
        self.messages.push(Msg { id: m.id.clone(), peer: m.from_key.clone(), dir: "in".into(), from: m.from.clone(), text: m.text.clone(), ts: m.ts, authentic: m.authentic, read: false, state: None, received: now });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("names-msg-store-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }
    fn opened(id: &str, key: &str, text: &str) -> crypto::Opened {
        crypto::Opened { id: id.into(), from: "bob".into(), from_key: key.into(), text: text.into(), ts: 1, authentic: true }
    }

    #[test]
    fn the_store_is_sealed_and_an_unreadable_one_is_never_written_over() {
        let d = dir("seal");
        let k = MsgKey::new(1).unwrap();
        k.save(&d).unwrap();
        assert_eq!(MsgKey::load(&d).unwrap().unwrap().public, k.public);
        assert_eq!(std::fs::metadata(d.join("msgkey.json")).unwrap().permissions().mode() & 0o777, 0o600);
        let mut s = match load(&d, &k.secret_bytes()) { Load::Fresh(s) => s, _ => panic!() };
        s.file_incoming(&opened("e1", &"b".repeat(64), "secret words"), 5);
        save(&d, &s, &k.secret_bytes()).unwrap();
        save(&d, &s, &k.secret_bytes()).unwrap();
        let raw = std::fs::read(path(&d)).unwrap();
        assert!(!String::from_utf8_lossy(&raw).contains("secret words"), "the file shows nothing");
        assert!(matches!(load(&d, &k.secret_bytes()), Load::Loaded(x) if x == s));
        // another key: unreadable, not empty
        let other = MsgKey::new(2).unwrap();
        assert!(matches!(load(&d, &other.secret_bytes()), Load::Unreadable { prev: true, .. }));
        // damaged: unreadable; the last saved copy brings it back, and the damaged file is kept aside
        std::fs::write(path(&d), b"garbage").unwrap();
        assert!(matches!(load(&d, &k.secret_bytes()), Load::Unreadable { .. }));
        assert_eq!(use_prev(&d, &k.secret_bytes()).unwrap(), s);
        assert!(matches!(load(&d, &k.secret_bytes()), Load::Loaded(_)));
        assert!(std::fs::read_dir(&d).unwrap().any(|e| e.unwrap().file_name().to_string_lossy().contains(".unreadable-")));
        // start fresh keeps the old file aside too
        let fresh = start_fresh(&d, &k.secret_bytes()).unwrap();
        assert!(fresh.messages.is_empty());
        assert!(std::fs::read_dir(&d).unwrap().any(|e| e.unwrap().file_name().to_string_lossy().contains(".set-aside-")));
        // an unreadable key file is an error, not "no key"
        std::fs::write(d.join("msgkey.json"), b"{").unwrap();
        assert!(MsgKey::load(&d).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn repeats_requests_caps_and_blocks() {
        let mut s = Store { v: 1, ..Default::default() };
        assert!(s.first_sight("h1") && !s.first_sight("h1") && !s.first_sight(""));
        for i in 0..SEEN_MAX + 10 {
            s.first_sight(&format!("x{i}"));
        }
        assert_eq!(s.seen.len(), SEEN_MAX);
        let k = "c".repeat(64);
        for i in 0..5 {
            s.file_incoming(&opened(&format!("e{i}"), &k, "hi"), 1);
        }
        assert_eq!(s.messages.len(), REQ_PER_SENDER, "a request keeps at most 3 messages");
        assert!(!s.contacts[&k].accepted);
        assert!(!s.file_incoming(&opened("e0", &k, "again"), 1), "the same message twice is one");
        s.contacts.get_mut(&k).unwrap().accepted = true;
        assert!(s.file_incoming(&opened("e9", &k, "now a chat"), 1));
        for i in 0..REQ_SENDERS + 3 {
            s.file_incoming(&opened(&format!("n{i}"), &format!("{i:064x}"), "new"), 1);
        }
        assert_eq!(s.contacts.values().filter(|c| !c.accepted).count(), REQ_SENDERS);
        let b = "d".repeat(64);
        s.blocked.insert(b.clone());
        assert!(!s.file_incoming(&opened("z", &b, "blocked"), 1));
        let mut fake = opened("u", &"e".repeat(64), "unproven");
        fake.authentic = false;
        assert!(!s.file_incoming(&fake, 1));
        assert!(s.dropped >= 2 + 3 + 2);
    }
}
