//! What the wallet's Messages screen asks (the module's `msg_*` methods call these). Each answer is JSON text in
//! the module's usual shape ({success, value, error}, made by the caller). Messages is one service per module
//! instance, started by `msg_turn_on` (or at the first call once a key exists).

use super::crypto;
use super::service::{self, Service, State, Transport};
use super::store::{self, MsgKey};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Mutex;

/// The running service, if Messages is on. Generic over the transport so tests run it with a stand-in.
pub struct Messages<T: Transport> {
    svc: Mutex<Option<Service<T>>>,
}

impl<T: Transport> Default for Messages<T> {
    fn default() -> Self {
        Messages { svc: Mutex::new(None) }
    }
}

fn hexkey(peer: &str) -> Result<String, String> {
    crypto::parse_msg_key(peer).map(|k| crypto::hex(&k)).ok_or_else(|| "that isn't a messaging address".into())
}

impl<T: Transport> Messages<T> {
    /// Starts the service if a key is there (an unreadable key file is an error, never a new key).
    pub fn ensure(&self, dir: &Path, transport: impl FnOnce() -> T) -> Result<bool, String> {
        let mut g = self.svc.lock().unwrap();
        if g.is_some() {
            return Ok(true);
        }
        match MsgKey::load(dir)? {
            None => Ok(false),
            Some(k) => {
                *g = Some(Service::start(dir.to_path_buf(), k, transport()));
                Ok(true)
            }
        }
    }

    /// Turn on: makes this computer's messaging key (once) and starts listening. The key's public half is what
    /// the screen then puts in the name's `msg` entry with set_text.
    pub fn turn_on(&self, dir: &Path, transport: impl FnOnce() -> T) -> Result<Value, String> {
        if MsgKey::load(dir)?.is_none() {
            MsgKey::new(service::now_ms())?.save(dir)?;
        }
        self.ensure(dir, transport)?;
        self.with(|st| Ok(json!({ "address": st.key.public, "topic": st.key.topic() })))
    }

    pub fn with<R>(&self, f: impl FnOnce(&mut State) -> Result<R, String>) -> Result<R, String> {
        let g = self.svc.lock().unwrap();
        let svc = g.as_ref().ok_or("Messages isn't on on this computer yet")?;
        let mut st = svc.state.lock().unwrap();
        f(&mut st)
    }

    pub fn status(&self, dir: &Path, transport: impl FnOnce() -> T) -> Value {
        let on = matches!(self.ensure(dir, transport), Ok(true));
        let g = self.svc.lock().unwrap();
        let Some(svc) = g.as_ref() else {
            return json!({ "on": false, "network": "logos.test", "key_error": MsgKey::load(dir).err() });
        };
        let st = svc.state.lock().unwrap();
        let s = st.store.as_ref();
        json!({
            "on": on, "network": "logos.test", "address": st.key.public, "topic": st.key.topic(),
            "connection": svc.transport.status(), "rln": svc.transport.rln(),
            "changes": st.changes, "error": st.last_error,
            "can_send": s.map_or(true, |s| s.send_blocked.is_none()), "send_note": s.and_then(|s| s.send_blocked.clone()),
            "unreadable": st.unreadable.as_ref().map(|(why, prev)| json!({ "why": why, "last_copy": prev })),
            "unread": s.map_or(0, |s| s.messages.iter().filter(|m| m.dir == "in" && !m.read && s.contacts.get(&m.peer).is_some_and(|c| c.accepted)).count()),
            "requests": s.map_or(0, |s| s.contacts.values().filter(|c| !c.accepted).count()),
            "dropped": s.map_or(0, |s| s.dropped),
        })
    }
}

fn store_of(st: &mut State) -> Result<&mut store::Store, String> {
    st.store.as_mut().ok_or_else(|| "the saved chats can't be read: use the last saved copy, or start fresh".into())
}

/// The chats (accepted contacts) or the requests, newest first: {address, name, last, at, unread, checked}.
pub fn list(st: &mut State, requests: bool) -> Result<Value, String> {
    let s = store_of(st)?;
    let mut out: Vec<Value> = s.contacts.values().filter(|c| c.accepted != requests).map(|c| {
        let mine: Vec<&store::Msg> = s.messages.iter().filter(|m| m.peer == c.key).collect();
        let last = mine.iter().max_by_key(|m| (m.ts, m.received));
        let claimed = mine.iter().rev().find(|m| m.dir == "in").map(|m| m.from.clone());
        json!({
            "address": c.key, "name": c.name, "claimed": claimed,
            "last": last.map(|m| m.text.clone()), "at": last.map_or(c.created, |m| m.ts),
            "unread": mine.iter().filter(|m| m.dir == "in" && !m.read).count(),
            "checked": c.checked.as_deref() == Some(c.key.as_str()),
        })
    }).collect();
    out.sort_by_key(|v| -v["at"].as_i64().unwrap_or(0));
    Ok(Value::Array(out))
}

pub fn messages(st: &mut State, peer: &str) -> Result<Value, String> {
    let k = hexkey(peer)?;
    let s = store_of(st)?;
    let mut mine: Vec<&store::Msg> = s.messages.iter().filter(|m| m.peer == k).collect();
    mine.sort_by_key(|m| (m.ts, m.received));
    Ok(json!(mine.iter().map(|m| json!({ "id": m.id, "dir": m.dir, "from": m.from, "text": m.text, "ts": m.ts, "state": m.state, "read": m.read })).collect::<Vec<_>>()))
}

pub fn mark_read(st: &mut State, peer: &str) -> Result<Value, String> {
    let k = hexkey(peer)?;
    let s = store_of(st)?;
    let mut n = 0;
    for m in s.messages.iter_mut().filter(|m| m.peer == k && m.dir == "in" && !m.read) {
        m.read = true;
        n += 1;
    }
    if n > 0 {
        st.changes += 1;
        save(st);
    }
    Ok(json!(n))
}

fn save(st: &mut State) {
    if let Some(s) = &st.store {
        if let Err(e) = store::save(&st.dir, s, &st.key.secret_bytes()) {
            st.last_error = Some(e);
        }
    }
}

/// Accept a request (its messages move to the chats, with the name it claimed if `name` is given: the caller
/// checks the claim on chain first), block an address (its messages and request go), unblock.
pub fn accept(st: &mut State, peer: &str, name: Option<String>) -> Result<Value, String> {
    let k = hexkey(peer)?;
    let s = store_of(st)?;
    let c = s.contacts.get_mut(&k).ok_or("there's no request from that address")?;
    c.accepted = true;
    if name.is_some() {
        c.name = name;
    }
    st.changes += 1;
    save(st);
    Ok(json!(true))
}

pub fn block(st: &mut State, peer: &str) -> Result<Value, String> {
    let k = hexkey(peer)?;
    let s = store_of(st)?;
    s.blocked.insert(k.clone());
    s.contacts.remove(&k);
    s.messages.retain(|m| m.peer != k);
    s.outbox.retain(|o| o.topic != crypto::inbox_topic(&crypto::parse_msg_key(&k).unwrap_or([0; 32])));
    st.changes += 1;
    save(st);
    Ok(json!(true))
}

pub fn unblock(st: &mut State, peer: &str) -> Result<Value, String> {
    let k = hexkey(peer)?;
    let s = store_of(st)?;
    let was = s.blocked.remove(&k);
    st.changes += 1;
    save(st);
    Ok(json!(was))
}

pub fn blocked(st: &mut State) -> Result<Value, String> {
    Ok(json!(store_of(st)?.blocked.iter().collect::<Vec<_>>()))
}

/// The safety code with someone: 12 groups of 5 digits, the same on their phone.
pub fn safety(st: &mut State, peer: &str) -> Result<Value, String> {
    let k = hexkey(peer)?;
    let code = crypto::safety_code(&st.key.public, &k)?;
    let checked = st.store.as_ref().and_then(|s| s.contacts.get(&k)).and_then(|c| c.checked.clone());
    Ok(json!({ "groups": crypto::safety_groups(&code), "qr": format!("names-safety:1:{code}"), "checked": checked.as_deref() == Some(k.as_str()) }))
}

pub fn mark_checked(st: &mut State, peer: &str, checked: bool) -> Result<Value, String> {
    let k = hexkey(peer)?;
    let s = store_of(st)?;
    let c = s.contacts.get_mut(&k).ok_or("there's no chat with that address")?;
    c.checked = checked.then(|| k.clone());
    st.changes += 1;
    save(st);
    Ok(json!(checked))
}

/// "Use the last saved copy" / "Start fresh" for saved chats that can't be read.
pub fn recover(st: &mut State, use_last_copy: bool) -> Result<Value, String> {
    let s = if use_last_copy { store::use_prev(&st.dir, &st.key.secret_bytes())? } else { store::start_fresh(&st.dir, &st.key.secret_bytes())? };
    st.store = Some(s);
    st.unreadable = None;
    st.changes += 1;
    Ok(json!(true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::service::Event;
    use std::sync::mpsc::{channel, Receiver};

    struct Quiet;
    impl Transport for Quiet {
        fn start(&self) -> Result<(), String> { Ok(()) }
        fn subscribe(&self, _: &str) -> Result<(), String> { Ok(()) }
        fn send(&self, _: &str, _: &[u8]) -> Result<String, String> { Ok("r".into()) }
        fn events(&self) -> Result<Receiver<Event>, String> { let (_tx, rx) = channel(); std::mem::forget(_tx); Ok(rx) }
        fn status(&self) -> String { "Connected".into() }
        fn rln(&self) -> String { "Ready".into() }
    }

    #[test]
    fn turn_on_lists_requests_accept_block_safety() {
        let d = std::env::temp_dir().join(format!("names-msg-api-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let m: Messages<Quiet> = Messages::default();
        assert_eq!(m.status(&d, || Quiet)["on"], false);
        let on = m.turn_on(&d, || Quiet).unwrap();
        let me = on["address"].as_str().unwrap().to_owned();
        assert_eq!(m.turn_on(&d, || Quiet).unwrap()["address"], me.as_str(), "turning on again keeps the key");
        let bob = MsgKey::new(1).unwrap();
        m.with(|st| {
            let p = crypto::seal(&st.key.public_bytes(), "bob", &bob.secret_bytes(), "hello?", 5, true, crypto::Fresh::random().unwrap()).unwrap();
            let topic = st.key.topic();
            service::on_received(st, "0xa", &topic, &p, 6);
            Ok(())
        }).unwrap();
        let reqs = m.with(|st| list(st, true)).unwrap();
        assert_eq!(reqs[0]["address"], bob.public.as_str());
        assert_eq!(reqs[0]["claimed"], "bob");
        assert_eq!(m.with(|st| list(st, false)).unwrap().as_array().unwrap().len(), 0, "a request isn't a chat");
        assert_eq!(m.status(&d, || Quiet)["requests"], 1);
        m.with(|st| accept(st, &bob.public, Some("bob".into()))).unwrap();
        let chats = m.with(|st| list(st, false)).unwrap();
        assert_eq!((chats[0]["name"].as_str(), chats[0]["unread"].as_u64()), (Some("bob"), Some(1)));
        assert_eq!(m.with(|st| mark_read(st, &bob.public)).unwrap(), 1);
        let sf = m.with(|st| safety(st, &bob.public)).unwrap();
        assert_eq!(sf["groups"].as_array().unwrap().len(), 12);
        assert_eq!(sf["checked"], false);
        m.with(|st| mark_checked(st, &bob.public, true)).unwrap();
        assert_eq!(m.with(|st| safety(st, &bob.public)).unwrap()["checked"], true);
        assert_eq!(m.with(|st| messages(st, &bob.public)).unwrap()[0]["text"], "hello?");
        m.with(|st| block(st, &bob.public)).unwrap();
        assert!(m.with(|st| list(st, false)).unwrap().as_array().unwrap().is_empty());
        assert_eq!(m.with(|st| blocked(st)).unwrap()[0], bob.public.as_str());
        m.with(|st| unblock(st, &bob.public)).unwrap();
        assert!(m.with(|st| blocked(st)).unwrap().as_array().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }
}
