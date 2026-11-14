//! Messages running: Basecamp's `delivery_module` as the transport (the shared node on logos.test, shard 7 for
//! every `/names/1/...` topic), a listener thread that opens and files what arrives (repeats dropped by message
//! hash, history replay included), and an outbox that sends sealed messages with retries. The store is saved after
//! every change (store.rs). `changes` counts changes, so a screen can ask "anything new?" cheaply.

use super::crypto;
use super::store::{self, Load, Msg, MsgKey, Outgoing, Store};
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// What the delivery node tells.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Received { hash: String, topic: String, payload: Vec<u8> },
    Propagated { request: String },
    Sent { request: String },
    Error { request: String, error: String },
}

/// The messaging node, as this module uses it (delivery_module in Basecamp; a stand-in in tests).
pub trait Transport: Send + Sync + 'static {
    /// Makes sure the node runs on logos.test (it is shared: another app may have started it already).
    fn start(&self) -> Result<(), String>;
    fn subscribe(&self, topic: &str) -> Result<(), String>;
    /// Sends a payload; the request id its events will carry.
    fn send(&self, topic: &str, payload: &[u8]) -> Result<String, String>;
    fn events(&self) -> Result<Receiver<Event>, String>;
    /// "Connected", "PartiallyConnected", "Disconnected", or why it can't say.
    fn status(&self) -> String;
    /// The RLN state (Ready, Initializing, Disabled, Failed) and its message: sending needs a membership.
    fn rln(&self) -> String;
}

/// Retries of a send: after 15 s, 1, 4, 15 and 60 minutes; then it is "failed" (Retry in the chat sends again).
const RETRY_MS: [i64; 5] = [15_000, 60_000, 240_000, 900_000, 3_600_000];

pub struct State {
    pub dir: PathBuf,
    pub key: MsgKey,
    pub store: Option<Store>,
    /// Why the saved chats can't be read (nothing is saved until the person chooses), and whether a last saved
    /// copy is there.
    pub unreadable: Option<(String, bool)>,
    pub changes: u64,
    pub last_error: Option<String>,
}

impl State {
    fn changed(&mut self) {
        self.changes += 1;
        if let Some(s) = &self.store {
            if let Err(e) = store::save(&self.dir, s, &self.key.secret_bytes()) {
                self.last_error = Some(e);
            }
        }
    }
}

pub struct Service<T: Transport> {
    pub state: Arc<Mutex<State>>,
    pub transport: Arc<T>,
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Opens one event's payload and files it; true if the store changed.
pub fn on_received(st: &mut State, hash: &str, topic: &str, payload: &[u8], now: i64) -> bool {
    if topic != st.key.topic() {
        return false;
    }
    let Some(s) = st.store.as_mut() else { return false };      // the store didn't load: nothing is filed or dropped
    if !s.first_sight(hash) {
        return false;
    }
    match crypto::open(&st.key.secret_bytes(), payload) {
        Some(m) => {
            s.file_incoming(&m, now);
            true
        }
        None => true,                                               // (remembered as seen, so it isn't tried again)
    }
}

impl<T: Transport> Service<T> {
    /// Starts Messages with this key: loads the store, starts the node, subscribes to this key's inbox, and runs
    /// the listener and the outbox in threads of their own.
    pub fn start(dir: PathBuf, key: MsgKey, transport: T) -> Service<T> {
        let (store, unreadable) = match store::load(&dir, &key.secret_bytes()) {
            Load::Fresh(s) | Load::Loaded(s) => (Some(s), None),
            Load::Unreadable { why, prev } => (None, Some((why, prev))),
        };
        let state = Arc::new(Mutex::new(State { dir, key, store, unreadable, changes: 0, last_error: None }));
        let transport = Arc::new(transport);
        let svc = Service { state, transport };
        svc.connect();
        svc.spawn_listener();
        svc.spawn_outbox();
        svc
    }

    fn connect(&self) {
        let topic = self.state.lock().unwrap().key.topic();
        let r = self.transport.start().and_then(|_| self.transport.subscribe(&topic));
        if let Err(e) = r {
            self.state.lock().unwrap().last_error = Some(e);
        }
    }

    fn spawn_listener(&self) {
        let (state, transport) = (Arc::clone(&self.state), Arc::clone(&self.transport));
        std::thread::spawn(move || loop {
            match transport.events() {
                Ok(rx) => {
                    for ev in rx {
                        let mut st = state.lock().unwrap();
                        let changed = match ev {
                            Event::Received { hash, topic, payload } => on_received(&mut st, &hash, &topic, &payload, now_ms()),
                            Event::Propagated { request } => set_send_state(&mut st, &request, "propagated", None),
                            Event::Sent { request } => set_send_state(&mut st, &request, "sent", None),
                            Event::Error { request, error } => set_send_state(&mut st, &request, "error", Some(error)),
                        };
                        if changed {
                            st.changed();
                        }
                    }
                }
                Err(e) => state.lock().unwrap().last_error = Some(e),
            }
            std::thread::sleep(Duration::from_secs(5));                // the subscription ended: listen again
        });
    }

    fn spawn_outbox(&self) {
        let (state, transport) = (Arc::clone(&self.state), Arc::clone(&self.transport));
        std::thread::spawn(move || loop {
            send_due(&state, &*transport, now_ms());
            std::thread::sleep(Duration::from_secs(3));
        });
    }
}

/// Sends what is due in the outbox (one try each); the network's answers come as events.
pub fn send_due<T: Transport + ?Sized>(state: &Arc<Mutex<State>>, transport: &T, now: i64) {
    let due: Vec<Outgoing> = {
        let st = state.lock().unwrap();
        st.store.as_ref().map(|s| s.outbox.iter().filter(|o| o.state == "queued" && o.next_at <= now).cloned().collect()).unwrap_or_default()
    };
    for o in due {
        let payload = crypto::unhex(&o.payload).unwrap_or_default();
        let result = transport.send(&o.topic, &payload);
        let mut guard = state.lock().unwrap();
        let st = &mut *guard;
        let Some(s) = st.store.as_mut() else { return };
        let Some(e) = s.outbox.iter_mut().find(|x| x.id == o.id) else { continue };
        e.tries += 1;
        let error = match result {
            Ok(request) => {
                e.request = Some(request);
                e.state = "sending".into();
                None
            }
            Err(err) => {
                retry_or_fail(e, now);
                Some(err)
            }
        };
        let now_state = e.state.clone();
        if let Some(m) = s.messages.iter_mut().find(|m| m.dir == "out" && m.id == o.id) {
            m.state = Some(now_state);
        }
        if error.is_some() {
            st.last_error = error;
        }
        st.changed();
    }
}

fn retry_or_fail(e: &mut Outgoing, now: i64) {
    match RETRY_MS.get(e.tries as usize - 1) {
        Some(wait) => {
            e.state = "queued".into();
            e.next_at = now + wait;
        }
        None => e.state = "failed".into(),
    }
}

/// An event about a send: the outbox entry and its message follow. "sent" ends it; an error queues a retry.
/// The node's refusals that mean this computer can't send at all yet (RLN: no membership, or a registry its
/// messaging module can't reach), not a passing network problem.
pub fn cant_send_here(error: &str) -> bool {
    let e = error.to_ascii_lowercase();
    e.contains("rln") || e.contains("membership") || e.contains("retry time window") || e.contains("proof")
}

/// What the composer says while sending can't work here.
pub const CANT_SEND: &str = "Receiving works. Sending from Basecamp needs a newer messaging module and a membership";

fn set_send_state(st: &mut State, request: &str, what: &str, error: Option<String>) -> bool {
    let now = now_ms();
    let Some(s) = st.store.as_mut() else { return false };
    let Some(i) = s.outbox.iter().position(|o| o.request.as_deref() == Some(request)) else { return false };
    let id = s.outbox[i].id.clone();
    let rln = error.as_deref().is_some_and(cant_send_here);
    let state = match what {
        "error" if rln => "failed".to_owned(),                     // retrying won't help: say so instead
        "error" => {
            retry_or_fail(&mut s.outbox[i], now);
            s.outbox[i].state.clone()
        }
        other => other.to_owned(),
    };

    // propagated (on the network) or sent (validated) ends the tries; a later "sent" for it only shows on the
    // message (the outbox entry is gone, so it is matched by request id there)
    if state == "sent" || state == "propagated" {
        s.outbox.remove(i);
        s.send_blocked = None;
    } else {
        s.outbox[i].state = state.clone();
    }
    if rln {
        s.send_blocked = Some(CANT_SEND.to_owned());           // kept in the store: it holds after a restart
    }
    if let Some(m) = s.messages.iter_mut().find(|m| m.dir == "out" && m.id == id) {
        m.state = Some(state);
    }
    if let Some(e) = error {
        st.last_error = Some(e);
    }
    true
}

/// Seals `text` to `to_key` from this key (as `from`, a name or msg-<16 hex>), puts it in the chat and the outbox.
/// The chat is the recipient's (made, and in the chats, if it wasn't). Returns the message id.
pub fn queue_send(st: &mut State, to_key: &str, to_name: Option<&str>, from: &str, text: &str, now: i64) -> Result<String, String> {
    let to = crypto::parse_msg_key(to_key).ok_or("that isn't a messaging address")?;
    let to_hex = crypto::hex(&to);
    let payload = crypto::seal(&to, from, &st.key.secret_bytes(), text, now, true, crypto::Fresh::random()?)?;
    let s = st.store.as_mut().ok_or("the saved chats didn't load, so nothing is sent")?;
    if s.blocked.contains(&to_hex) {
        return Err("you blocked this messaging address".into());
    }
    let id = format!("out-{}", crypto::hex(&payload[1..17]));
    let c = s.contacts.entry(to_hex.clone()).or_insert(store::Contact { key: to_hex.clone(), name: None, accepted: true, checked: None, created: now });
    c.accepted = true;
    if let Some(n) = to_name {
        c.name = Some(n.to_owned());
    }
    s.messages.push(Msg { id: id.clone(), peer: to_hex, dir: "out".into(), from: from.into(), text: text.into(), ts: now, authentic: true, read: true, state: Some("queued".into()), received: now });
    s.outbox.push(Outgoing { id: id.clone(), topic: crypto::inbox_topic(&to), payload: crypto::hex(&payload), tries: 0, next_at: now, request: None, state: "queued".into() });
    st.changed();
    Ok(id)
}

/// delivery_module through the Logos module SDK (by name: it is Basecamp's own, shared with Chat).
#[cfg(not(test))]
pub struct Delivery;

#[cfg(not(test))]
impl Delivery {
    const MODULE: &'static str = "delivery_module";
    fn call(method: &str, args: serde_json::Value) -> Result<serde_json::Value, String> {
        let r = logos_rust_sdk::LogosModuleSDK::new()
            .plugin(Self::MODULE)
            .call_json_with_timeout(method, &args, Duration::from_secs(20))
            .map_err(|_| "Basecamp's messaging node (delivery_module) didn't answer. Open the Chat app once, so Basecamp loads it.".to_string())?;
        if r.get("success").and_then(|v| v.as_bool()) == Some(false) {
            return Err(r.get("error").and_then(|v| v.as_str()).unwrap_or("the messaging node refused").to_owned());
        }
        Ok(r.get("value").cloned().unwrap_or(r))
    }
}

/// `{"_bytes": base64url}`, how the module host passes bytes.
pub fn bytes_arg(b: &[u8]) -> serde_json::Value {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in b.chunks(3) {
        let n = chunk.iter().fold(0u32, |n, x| (n << 8) | *x as u32) << (8 * (3 - chunk.len()));
        for i in 0..=chunk.len() {
            out.push(A[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
    }
    serde_json::json!({ "_bytes": out })
}

/// The bytes in a module-host value: `{"_bytes": base64url}`, or a base64 string.
pub fn bytes_of(v: &serde_json::Value) -> Option<Vec<u8>> {
    let text = v.get("_bytes").and_then(|x| x.as_str()).or_else(|| v.as_str())?;
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in text.bytes() {
        let x = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            b'=' => continue,
            _ => return None,
        } as u32;
        acc = (acc << 6) | x;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

/// One event's positional argument (the host sends them as an array, or as arg0, arg1, …).
pub fn arg(data: &serde_json::Value, i: usize) -> serde_json::Value {
    data.get(i).cloned().or_else(|| data.get(format!("arg{i}")).cloned()).unwrap_or(serde_json::Value::Null)
}

pub fn decode_event(event: &str, data: &serde_json::Value) -> Option<Event> {
    let s = |i| arg(data, i).as_str().map(str::to_owned);
    Some(match event {
        "messageReceived" => Event::Received { hash: s(0)?, topic: s(1)?, payload: bytes_of(&arg(data, 2))? },
        "messagePropagated" => Event::Propagated { request: s(0)? },
        "messageSent" => Event::Sent { request: s(0)? },
        "messageError" => Event::Error { request: s(0)?, error: s(2).unwrap_or_default() },
        _ => return None,
    })
}

#[cfg(not(test))]
impl Transport for Delivery {
    fn start(&self) -> Result<(), String> {
        // The node is Basecamp's, shared with Chat, which starts it on logos.test; made here only if nobody has.
        if Self::call("createNode", serde_json::json!([r#"{"mode":"Core","preset":"logos.test"}"#])).is_ok() {
            Self::call("start", serde_json::json!([]))?;
        }
        Ok(())
    }
    fn subscribe(&self, topic: &str) -> Result<(), String> {
        Self::call("subscribe", serde_json::json!([topic])).map(|_| ())
    }
    fn send(&self, topic: &str, payload: &[u8]) -> Result<String, String> {
        let v = Self::call("send", serde_json::json!([topic, bytes_arg(payload)]))?;
        v.as_str().map(str::to_owned).ok_or_else(|| "the messaging node gave no request id".into())
    }
    fn events(&self) -> Result<Receiver<Event>, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        for name in ["messageReceived", "messagePropagated", "messageSent", "messageError"] {
            let mut plugin = logos_rust_sdk::LogosModuleSDK::new().plugin(Self::MODULE);
            let sub = plugin.on(name).map_err(|e| format!("couldn't listen to the messaging node: {e:?}"))?;
            let tx = tx.clone();
            std::thread::spawn(move || {
                for ev in sub {
                    if let Some(e) = decode_event(&ev.event, &ev.data) {
                        if tx.send(e).is_err() {
                            return;
                        }
                    }
                }
            });
        }
        Ok(rx)
    }
    fn status(&self) -> String {
        Self::call("getConnectionStatus", serde_json::json!([])).map(|v| v.as_str().unwrap_or("").to_owned()).unwrap_or_else(|e| e)
    }
    fn rln(&self) -> String {
        Self::call("rlnState", serde_json::json!([])).map(|v| v.to_string()).unwrap_or_else(|e| e)
    }
}

/// A node that sends nowhere and hears nothing (the module's unit tests).
#[cfg(test)]
pub struct Offline;

#[cfg(test)]
impl Transport for Offline {
    fn start(&self) -> Result<(), String> { Ok(()) }
    fn subscribe(&self, _: &str) -> Result<(), String> { Ok(()) }
    fn send(&self, _: &str, _: &[u8]) -> Result<String, String> { Err("offline".into()) }
    fn events(&self) -> Result<Receiver<Event>, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        std::mem::forget(tx);
        Ok(rx)
    }
    fn status(&self) -> String { "Disconnected".into() }
    fn rln(&self) -> String { "Disabled".into() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{channel, Sender};

    /// A stand-in node: records what is sent, and the test pushes events.
    pub struct Fake {
        pub sent: Mutex<Vec<(String, Vec<u8>)>>,
        pub fail: Mutex<bool>,
        pub tx: Mutex<Option<Sender<Event>>>,
    }
    impl Transport for Arc<Fake> {
        fn start(&self) -> Result<(), String> { Ok(()) }
        fn subscribe(&self, _: &str) -> Result<(), String> { Ok(()) }
        fn send(&self, topic: &str, payload: &[u8]) -> Result<String, String> {
            if *self.fail.lock().unwrap() {
                return Err("no peers".into());
            }
            let mut s = self.sent.lock().unwrap();
            s.push((topic.into(), payload.into()));
            Ok(format!("req{}", s.len()))
        }
        fn events(&self) -> Result<Receiver<Event>, String> {
            let (tx, rx) = channel();
            *self.tx.lock().unwrap() = Some(tx);
            Ok(rx)
        }
        fn status(&self) -> String { "Connected".into() }
        fn rln(&self) -> String { "Ready".into() }
    }

    fn dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("names-msg-svc-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn wait<F: Fn() -> bool>(f: F) -> bool {
        for _ in 0..100 {
            if f() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        false
    }

    #[test]
    fn receives_files_drops_repeats_sends_and_retries() {
        let d = dir("run");
        let me = MsgKey::new(1).unwrap();
        let bob = MsgKey::new(2).unwrap();
        let fake = Arc::new(Fake { sent: Mutex::new(vec![]), fail: Mutex::new(false), tx: Mutex::new(None) });
        let svc = Service::start(d.clone(), me.clone(), Arc::clone(&fake));
        assert!(wait(|| fake.tx.lock().unwrap().is_some()));
        let tx = fake.tx.lock().unwrap().clone().unwrap();
        // bob writes; the node delivers it live, then again as history: one message
        let p = crypto::seal(&me.public_bytes(), "bob", &bob.secret_bytes(), "hi there", 10, true, crypto::Fresh::random().unwrap()).unwrap();
        for _ in 0..2 {
            tx.send(Event::Received { hash: "0xh1".into(), topic: me.topic(), payload: p.clone() }).unwrap();
        }
        tx.send(Event::Received { hash: "0xother".into(), topic: "/names/1/msg-0000000000000000/proto".into(), payload: p.clone() }).unwrap();
        assert!(wait(|| svc.state.lock().unwrap().store.as_ref().unwrap().messages.len() == 1));
        std::thread::sleep(Duration::from_millis(200));
        {
            let st = svc.state.lock().unwrap();
            let s = st.store.as_ref().unwrap();
            assert_eq!(s.messages.len(), 1, "a repeat and another inbox's message are not filed");
            assert!(!s.contacts[&bob.public].accepted, "a stranger's first message is a request");
        }
        // a reply: sealed, queued, sent by the outbox; the node's events follow it to "sent"
        let id = queue_send(&mut svc.state.lock().unwrap(), &bob.public, Some("bob"), "alice", "hello bob", 20).unwrap();
        assert!(wait(|| fake.sent.lock().unwrap().len() == 1));
        let (topic, payload) = fake.sent.lock().unwrap()[0].clone();
        assert_eq!(topic, bob.topic());
        let opened = crypto::open(&bob.secret_bytes(), &payload).unwrap();
        assert_eq!((opened.text.as_str(), opened.authentic, opened.from_key.as_str()), ("hello bob", true, me.public.as_str()));
        tx.send(Event::Sent { request: "req1".into() }).unwrap();
        assert!(wait(|| svc.state.lock().unwrap().store.as_ref().unwrap().messages.iter().any(|m| m.id == id && m.state.as_deref() == Some("sent"))));
        assert!(svc.state.lock().unwrap().store.as_ref().unwrap().outbox.is_empty());
        assert!(svc.state.lock().unwrap().store.as_ref().unwrap().contacts[&bob.public].accepted, "writing to someone puts them in the chats");
        // a send the node refuses is retried later, then marked failed
        *fake.fail.lock().unwrap() = true;
        let id2 = queue_send(&mut svc.state.lock().unwrap(), &bob.public, None, "alice", "again", 30).unwrap();
        for i in 0..6 {
            send_due(&svc.state, &*svc.transport, now_ms() + 10_000_000 * (i + 1));
        }
        let st = svc.state.lock().unwrap();
        let m = st.store.as_ref().unwrap().messages.iter().find(|m| m.id == id2).unwrap();
        assert_eq!(m.state.as_deref(), Some("failed"));
        // everything was saved: a new start reads it back
        drop(st);
        let reread = match store::load(&d, &me.secret_bytes()) { Load::Loaded(s) => s, _ => panic!("not saved") };
        assert_eq!(reread.messages.len(), 3);
        assert!(reread.seen.contains(&"0xh1".to_string()));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn an_unreadable_store_files_and_sends_nothing() {
        let d = dir("bad");
        let me = MsgKey::new(1).unwrap();
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(store::path(&d), b"NMS1 damaged").unwrap();
        let fake = Arc::new(Fake { sent: Mutex::new(vec![]), fail: Mutex::new(false), tx: Mutex::new(None) });
        let svc = Service::start(d.clone(), me.clone(), fake);
        let mut st = svc.state.lock().unwrap();
        assert!(st.unreadable.is_some() && st.store.is_none());
        assert!(!on_received(&mut st, "h", &me.topic(), b"x", 1));
        assert!(queue_send(&mut st, &me.public, None, "alice", "hi", 1).is_err());
        assert_eq!(std::fs::read(store::path(&d)).unwrap(), b"NMS1 damaged", "never written over");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_refusal_for_rln_fails_at_once_and_says_why_until_a_send_goes_out() {
        let d = dir("rln");
        let me = MsgKey::new(1).unwrap();
        let svc = Service::start(d.clone(), me.clone(), Offline);
        let mut st = svc.state.lock().unwrap();
        let bob = MsgKey::new(2).unwrap();
        let id = queue_send(&mut st, &bob.public, None, "alice", "hi", 1).unwrap();
        st.store.as_mut().unwrap().outbox[0].request = Some("r1".into());
        assert!(set_send_state(&mut st, "r1", "error", Some("Unable to send within retry time window".into())));
        let s = st.store.as_ref().unwrap();
        assert_eq!(s.messages.iter().find(|m| m.id == id).unwrap().state.as_deref(), Some("failed"), "no retries: they can't help");
        assert_eq!(s.send_blocked.as_deref(), Some(CANT_SEND));
        assert_eq!(s.outbox[0].state, "failed");
        // a send that goes out (a newer module, a membership) clears it
        let id2 = queue_send(&mut st, &bob.public, None, "alice", "again", 2).unwrap();
        let o = st.store.as_mut().unwrap().outbox.iter_mut().find(|o| o.id == id2).unwrap();
        o.request = Some("r2".into());
        set_send_state(&mut st, "r2", "propagated", None);
        assert_eq!(st.store.as_ref().unwrap().send_blocked, None);
        assert!(!cant_send_here("no peers"));
        drop(st);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn decodes_the_hosts_events_and_bytes() {
        let b = vec![0u8, 1, 2, 250, 251, 252, 253];
        assert_eq!(bytes_of(&bytes_arg(&b)).unwrap(), b);
        let data = serde_json::json!({ "arg0": "0xh", "arg1": "/t", "arg2": bytes_arg(b"hey"), "arg3": "live", "arg4": 1 });
        assert_eq!(decode_event("messageReceived", &data), Some(Event::Received { hash: "0xh".into(), topic: "/t".into(), payload: b"hey".to_vec() }));
        let arr = serde_json::json!(["r1", "0xh", "no peers", 5]);
        assert_eq!(decode_event("messageError", &arr), Some(Event::Error { request: "r1".into(), error: "no peers".into() }));
        assert_eq!(decode_event("nodeStarted", &arr), None);
    }
}
