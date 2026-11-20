//! Reads the Names app polls while a page is open, answered at once from what was last read.
//!
//! The module answers one call at a time on the host's main thread (`concurrency: single`), so a
//! call that waits on the network holds every other call: on 2026-11-18 one slow read made 11
//! polls in a row time out ("No answer from the wallet yet") for about 48 s. A read here is made
//! on a thread of its own (stale-while-revalidate): the call answers with the last value and its
//! time, and a newer read starts when the last is `every` old. Before the first value is back,
//! the call waits `FIRST_WAIT` at most and then answers busy ("not taken, ask again"), which the
//! app already shows as a wait and asks again after.
//!
//! A read that fails keeps the last value: it is answered with when it was read and why it
//! couldn't be read again (`stale`, `as_of`, `warning`), never as a fresh one. A read under way
//! when the value is forgotten (the wallet changed, a send, another network) is dropped when it
//! ends, never answered as the new state's.
//!
//! Background reads of one kind take turns (`Lane`): one names CLI run against the wallet at a
//! time, one read of a host's endpoint at a time, so a page never fires a burst at an endpoint
//! that limits each address (LEZ 0.4: 50 requests per 10 s).
//!
//! Unit tests make the read on the spot by default (`inline`), so the rest of the suite reads as
//! before; the stall tests use `threaded`.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// How long a call waits for a first read before it answers busy: inside the second a poll may
/// hold the module.
pub const FIRST_WAIT: Duration = Duration::from_millis(600);

/// How much waiting for first reads the module's thread may do in a burst, and how fast that
/// comes back (per second). The app sends its calls in bursts (about 25 as Names opens, the
/// polls every few seconds) and the host answers them one after the other: a call's wait is
/// also every later call's. Without this a burst of reads that were all slow cost 600 ms each,
/// one after the other (the third stall, 2026-11-18: ~12 of them, 7.5 s, before the node's
/// 15 s). With it a whole burst waits 400 ms at most, and a read that is fast still answers on
/// the first call (it uses only what it waited).
pub const WAIT_BUDGET: Duration = Duration::from_millis(400);
pub const WAIT_REFILL_PER_S: Duration = Duration::from_millis(200);

/// What a call answers while the first read is under way (the busy envelope: not taken).
pub fn still_reading(what: &str) -> String {
    json!({ "success": false, "busy": true, "value": null, "error": format!("still reading {what}; this updates in a moment") }).to_string()
}

/// Which background reads take turns.
#[derive(Clone, Debug)]
pub enum Lane {
    /// The names CLI on the wallet: one run at a time.
    Cli,
    /// One endpoint (its URL): one request in flight from here at a time.
    Host(String),
    /// Quick local work (asking the CLI its version): never waits behind the others.
    Free,
    /// A queue of its own, by name (the reads about names, beside an endpoint's other reads).
    Own(String),
}

/// The turn `lane` takes (None: it never waits). Found on the call's thread, when the read starts.
fn lane_lock(lane: &Lane) -> Option<Arc<Mutex<()>>> {
    static LANES: Mutex<Option<HashMap<String, Arc<Mutex<()>>>>> = Mutex::new(None);
    let key = match lane {
        Lane::Cli => "cli".to_owned(),
        Lane::Host(url) => format!("host:{}", host_of(url)),
        Lane::Free => return None,
        Lane::Own(name) => format!("own:{name}"),
    };
    Some(lock(&LANES).get_or_insert_with(HashMap::new).entry(lane_scope() + &key).or_default().clone())
}

/// Runs `work` in `lane`'s turn on the calling thread: after the background read of that kind under
/// way, if any. names-stall-3 took this out (its reads take the turn inside `Served::read`);
/// names-roadmap-2's account import still runs the names tool in the CLI lane with it (merge
/// wallet-1119).
pub fn in_lane<R>(lane: &Lane, work: impl FnOnce() -> R) -> R {
    let Some(turn) = lane_lock(lane) else { return work() };
    let _turn = lock(&turn);
    work()
}

/// The lanes are the module's (one names instance per host process).
#[cfg(not(test))]
fn lane_scope() -> String {
    String::new()
}

/// In unit tests each test thread has lanes of its own, so one test's slow CLI (20 s a run in
/// the owner's test) never holds another test's reads.
#[cfg(test)]
fn lane_scope() -> String {
    format!("{:?}:", std::thread::current().id())
}

/// The scheme and host of `url`, which an endpoint's limit counts by.
pub fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    host.to_ascii_lowercase()
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

thread_local! {
    /// Whether this thread is one of the background reads (`Served`, `spawn`): such a thread
    /// may wait for an endpoint's `Retry-After`; a call's thread never does.
    static BACKGROUND: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether this thread is a background read's.
pub fn in_background() -> bool {
    BACKGROUND.with(std::cell::Cell::get)
}

/// Marks this thread as a background read's (a scoped thread a background read starts).
pub fn as_background() {
    BACKGROUND.with(|flag| flag.set(true));
}

/// Runs `work` on a thread of its own, marked as a background read's.
pub fn spawn(work: impl FnOnce() + Send + 'static) {
    std::thread::spawn(move || {
        BACKGROUND.with(|flag| flag.set(true));
        work();
    });
}

/// Every read's end gets the next number: a later read always has a larger `seq`, whatever its key.
static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn next_seq() -> u64 {
    SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1
}

/// Seconds since 1970.
pub fn now_s() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_secs())
}

#[derive(Default)]
struct Slot {
    /// Which slot this is: a read started for an earlier one (since forgotten) is dropped.
    id: u64,
    value: Option<Value>,
    error: Option<String>,
    /// When `value` was read, in seconds since 1970.
    read_at: Option<u64>,
    /// When the last read ended, well or not.
    ended: Option<Instant>,
    updating: bool,
    /// The number of the last read that ended (`next_seq`): tells a new result from one taken.
    seq: u64,
}

/// What a served read holds now.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snap {
    pub value: Option<Value>,
    pub error: Option<String>,
    pub read_at: Option<u64>,
    pub updating: bool,
    pub seq: u64,
}

impl Snap {
    /// How old the value is, in ms (0 when there is none).
    pub fn age_ms(&self) -> u64 {
        self.read_at.map_or(0, |at| now_s().saturating_sub(at).saturating_mul(1000))
    }

    /// The value, failing with the read's error, or `still_reading` while the first is under way:
    /// `Err(Ok(busy envelope))` for that, `Err(Err(why))` for a read that failed with nothing kept.
    pub fn value_or(&self, what: &str) -> Result<&Value, Result<String, String>> {
        match (&self.value, &self.error) {
            (Some(value), _) => Ok(value),
            (None, Some(why)) if !self.updating => Err(Err(why.clone())),
            _ => Err(Ok(still_reading(what))),
        }
    }
}

struct Shared {
    slots: Mutex<(u64, HashMap<String, Slot>)>,
    ended: Condvar,
    /// What is left of `WAIT_BUDGET`, and when it was last topped up.
    budget: Mutex<(Duration, Instant)>,
}

impl Shared {
    /// What the call thread may wait now: `FIRST_WAIT` at most, less what this burst waited.
    fn wait_allowed(&self) -> Duration {
        let mut budget = lock(&self.budget);
        let (left, at) = *budget;
        let refilled = WAIT_REFILL_PER_S.mul_f64(at.elapsed().as_secs_f64());
        *budget = ((left + refilled).min(WAIT_BUDGET), Instant::now());
        budget.0.min(FIRST_WAIT)
    }

    fn spend(&self, waited: Duration) {
        let mut budget = lock(&self.budget);
        budget.0 = budget.0.saturating_sub(waited);
    }
}

/// The served reads of one module instance.
#[derive(Clone)]
pub struct Served {
    shared: Arc<Shared>,
    /// Read on the spot, every call (unit tests): the value is never kept.
    pub inline: bool,
}

impl Default for Served {
    fn default() -> Self {
        Served {
            shared: Arc::new(Shared { slots: Mutex::new((0, HashMap::new())), ended: Condvar::new(), budget: Mutex::new((WAIT_BUDGET, Instant::now())) }),
            inline: cfg!(test),
        }
    }
}

impl Served {
    /// Reads on their own threads, as in the module build (the stall tests).
    #[cfg(test)]
    pub fn threaded() -> Self {
        Served { inline: false, ..Served::default() }
    }

    /// The read kept under `key`, starting a newer one (`fetch`, in `lane`) when none is under way
    /// and the last ended `every` ago or more. With no value yet, waits for the read this call
    /// started `FIRST_WAIT` at most, within what is left of the burst's `WAIT_BUDGET`; a read
    /// already under way (an earlier call started it) is never waited for again.
    pub fn read(&self, key: &str, every: Duration, lane: Lane, fetch: impl FnOnce() -> Result<Value, String> + Send + 'static) -> Snap {
        if self.inline {
            let result = fetch();
            let (value, error) = match result {
                Ok(value) => (Some(value), None),
                Err(why) => (None, Some(why)),
            };
            return Snap { read_at: value.as_ref().map(|_| now_s()), value, error, updating: false, seq: next_seq() };
        }
        let mut slots = lock(&self.shared.slots);
        if !slots.1.contains_key(key) {
            slots.0 += 1;
            let id = slots.0;
            slots.1.insert(key.to_owned(), Slot { id, ..Slot::default() });
        }
        let slot = slots.1.get_mut(key).expect("the slot was just made");
        let due = !slot.updating && slot.ended.is_none_or(|at| at.elapsed() >= every);
        // A value past `every` whose newer read starts now: waited for as a first read is, so a
        // fast read answers fresh; a slow one answers the last value, `updating`.
        let started = (due && slot.value.is_some()).then_some(slot.seq);
        if due {
            slot.updating = true;
            let (shared, key_owned, id) = (Arc::clone(&self.shared), key.to_owned(), slot.id);
            let turn = lane_lock(&lane);
            spawn(move || {
                // In `lane`'s turn: after the background read of that kind under way, if any.
                let result = {
                    let _turn = turn.as_ref().map(|turn| lock(turn));
                    fetch()
                };
                let mut slots = lock(&shared.slots);
                if let Some(slot) = slots.1.get_mut(&key_owned).filter(|slot| slot.id == id) {
                    match result {
                        Ok(value) => {
                            slot.value = Some(value);
                            slot.error = None;
                            slot.read_at = Some(now_s());
                        }
                        Err(why) => slot.error = Some(why),
                    }
                    slot.ended = Some(Instant::now());
                    slot.updating = false;
                    slot.seq = next_seq();
                }
                drop(slots);
                shared.ended.notify_all();
            });
        }
        let first = slot.value.is_none() && slot.error.is_none() && due;
        if first || started.is_some() {
            let began = Instant::now();
            let until = began + self.shared.wait_allowed();
            loop {
                let waiting = slots.1.get(key).is_some_and(|slot| {
                    slot.updating && (started.is_some_and(|seq| slot.seq == seq) || (slot.value.is_none() && slot.error.is_none()))
                });
                let left = until.saturating_duration_since(Instant::now());
                if !waiting || left.is_zero() {
                    break;
                }
                slots = self.shared.ended.wait_timeout(slots, left).unwrap_or_else(std::sync::PoisonError::into_inner).0;
            }
            self.shared.spend(began.elapsed());
        }
        Self::snap(slots.1.get(key))
    }

    fn snap(slot: Option<&Slot>) -> Snap {
        slot.map_or_else(Snap::default, |slot| Snap {
            value: slot.value.clone(),
            error: slot.error.clone(),
            read_at: slot.read_at,
            updating: slot.updating,
            seq: slot.seq,
        })
    }

    /// What is kept under `key`, without reading.
    pub fn peek(&self, key: &str) -> Snap {
        Self::snap(lock(&self.shared.slots).1.get(key))
    }

    /// Forgets one key exactly. Unlike `forget`, this does not clear other keys that happen to
    /// start with it (one name's key can prefix another's). The next call reads afresh, and a read
    /// under way for this key is dropped when it ends.
    pub fn forget_key(&self, key: &str) {
        lock(&self.shared.slots).1.remove(key);
        self.shared.ended.notify_all();
    }

    /// Forgets every read whose key starts with `prefix` ("" for all): the next call reads afresh,
    /// and a read under way for one of them is dropped when it ends.
    pub fn forget(&self, prefix: &str) {
        lock(&self.shared.slots).1.retain(|key, _| !key.starts_with(prefix));
        self.shared.ended.notify_all();
    }
}

/// A served value as a reply: `{success, value, error}`, with `read_at` (seconds) and, when the
/// last read failed, `{stale, as_of, warning}` (what `stale_marks` says) beside it; the busy
/// envelope while the first read is under way; the read's error when nothing was ever read.
/// `what` names it in a sentence ("Your balance").
pub fn reply_of(snap: &Snap, what: &str, marks: impl FnOnce(&Value) -> Value) -> String {
    match snap.value_or(&what.to_lowercase()) {
        Err(Ok(busy)) => busy,
        Err(Err(why)) => json!({ "success": false, "value": null, "error": why }).to_string(),
        Ok(value) => {
            // `updating`: a newer read is under way (the app may ask again for it).
            let mut out = json!({ "success": true, "value": value, "error": null, "read_at": snap.read_at, "updating": snap.updating });
            let own = marks(value);
            if let (Some(out), Some(own)) = (out.as_object_mut(), own.as_object()) {
                for (key, field) in own {
                    out.insert(key.clone(), field.clone());
                }
            }
            if let Some(why) = &snap.error {
                let stale = stale_marks(why, snap.age_ms(), what);
                if let (Some(out), Some(stale)) = (out.as_object_mut(), stale.as_object()) {
                    for (key, field) in stale {
                        out.insert(key.clone(), field.clone());
                    }
                }
            }
            out.to_string()
        }
    }
}

/// `reply_of` for a read kept as `{value, marks}`: the value, with its own marks beside it (a
/// partial read's `{partial, unread, warning}`) and, when the last read failed, the stale marks.
pub fn reply_marked_of(snap: &Snap, what: &str) -> String {
    let inner = Snap { value: snap.value.as_ref().map(|kept| kept["value"].clone()), ..snap.clone() };
    let marks = snap.value.as_ref().map_or(Value::Null, |kept| kept["marks"].clone());
    let mut out: Value = serde_json::from_str(&reply_of(&inner, what, |_| marks.clone())).unwrap_or(Value::Null);
    // A stale read's own warning gives way to the stale one (both are said: the age first).
    if let (Some(stale), Some(own)) = (out["warning"].as_str().map(str::to_owned), marks["warning"].as_str()) {
        if out["stale"] == true && !stale.contains(own) {
            out["warning"] = json!(format!("{stale} {own}"));
        }
    }
    out.to_string()
}

/// A span of time as a sentence says it: "40 s", "3 min", "2 h".
pub fn age_text(ms: u64) -> String {
    match ms / 1000 {
        secs @ 0..=89 => format!("{} s", secs.max(1)),
        secs @ 90..=5399 => format!("{} min", (secs + 30) / 60),
        secs => format!("{} h", (secs + 1800) / 3600),
    }
}

/// What a value kept from an earlier read says beside it while reading it again fails:
/// `{stale, as_of, warning}` (cli-mine-stale-list-unmarked-during-outage).
pub fn stale_marks(why: &str, age_ms: u64, what: &str) -> Value {
    let why = why.trim().trim_end_matches(['.', ' ']);
    let mut letters = why.chars();
    let why = letters.next().map_or_else(String::new, |first| first.to_uppercase().chain(letters).collect());
    json!({
        "stale": true,
        "as_of": now_s().saturating_sub(age_ms / 1000),
        "warning": format!("{what} as read {} ago. {why}.", age_text(age_ms)),
    })
}

// ── The tripwire: a network read or a names CLI run on the thread a call came in on ──

#[cfg(test)]
thread_local! {
    /// Set by a test while it makes a call as the host would (its thread is then the dispatch
    /// thread): every network read or CLI wait made on it is noted, by where it was made.
    pub static DISPATCH: std::cell::RefCell<Option<Vec<&'static str>>> = const { std::cell::RefCell::new(None) };
}

/// Notes a blocking read at `site` made on a call's own thread (tests only; nothing in a build).
#[inline]
pub fn blocking(site: &'static str) {
    #[cfg(test)]
    DISPATCH.with(|seen| {
        if let Some(seen) = seen.borrow_mut().as_mut() {
            seen.push(site);
        }
    });
    #[cfg(not(test))]
    let _ = site;
}

/// Runs `probe`, a quick local run of the CLI (`--version`), without noting it: it reads no
/// network, and its answer is kept per CLI file (`Names::cli_line`).
pub fn local<R>(probe: impl FnOnce() -> R) -> R {
    #[cfg(test)]
    {
        let kept = DISPATCH.with(|seen| seen.borrow_mut().take());
        let out = probe();
        DISPATCH.with(|seen| *seen.borrow_mut() = kept);
        out
    }
    #[cfg(not(test))]
    probe()
}

/// Runs `call` as the host dispatches it and returns what it answered and the blocking reads
/// it made on its own thread.
#[cfg(test)]
pub fn on_dispatch<R>(call: impl FnOnce() -> R) -> (R, Vec<&'static str>) {
    DISPATCH.with(|seen| *seen.borrow_mut() = Some(Vec::new()));
    let out = call();
    let seen = DISPATCH.with(|seen| seen.borrow_mut().take()).unwrap_or_default();
    (out, seen)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_first_read_answers_busy_when_slow_then_the_value_then_the_last_value_while_failing() {
        let served = Served::threaded();
        let slow = || {
            std::thread::sleep(Duration::from_millis(1500));
            Ok(json!(7))
        };
        let started = Instant::now();
        let snap = served.read("k", Duration::from_secs(60), Lane::Cli, slow);
        assert!(started.elapsed() < Duration::from_secs(1), "the call waits FIRST_WAIT at most");
        assert!(snap.value.is_none() && snap.updating);
        let busy: Value = serde_json::from_str(&reply_of(&snap, "Your balance", |_| Value::Null)).unwrap();
        assert_eq!(busy["busy"], true, "{busy}");
        std::thread::sleep(Duration::from_millis(1300));
        let snap = served.read("k", Duration::ZERO, Lane::Cli, || Err("could not reach the sequencer".to_owned()));
        assert_eq!(snap.value, Some(json!(7)), "the read that ended is answered");
        std::thread::sleep(Duration::from_millis(100));
        let snap = served.peek("k");
        assert_eq!(snap.value, Some(json!(7)), "a failed read keeps the last value");
        let shown: Value = serde_json::from_str(&reply_of(&snap, "Your balance", |_| Value::Null)).unwrap();
        assert_eq!(shown["value"], 7);
        assert_eq!(shown["stale"], true);
        assert!(shown["warning"].as_str().unwrap().starts_with("Your balance as read"), "{shown}");
    }

    #[test]
    fn a_fast_first_read_is_answered_on_the_first_call_and_a_forgotten_read_is_dropped() {
        let served = Served::threaded();
        let snap = served.read("a", Duration::from_secs(60), Lane::Host("http://x/".into()), || Ok(json!("fresh")));
        assert_eq!(snap.value, Some(json!("fresh")));
        let _ = served.read("b", Duration::from_secs(60), Lane::Host("http://x/".into()), || {
            std::thread::sleep(Duration::from_millis(900));
            Ok(json!("old wallet"))
        });
        served.forget("b");
        std::thread::sleep(Duration::from_millis(1000));
        assert_eq!(served.peek("b"), Snap::default(), "a read for a forgotten value isn't kept");
    }

    #[test]
    fn a_value_past_its_age_waits_for_a_fast_newer_read_and_says_updating_for_a_slow_one() {
        let served = Served::threaded();
        let every = Duration::from_millis(200);
        assert_eq!(served.read("v", every, Lane::Free, || Ok(json!(1))).value, Some(json!(1)));
        std::thread::sleep(Duration::from_millis(250));
        // Past its age, a fast read: the call answers with it.
        let snap = served.read("v", every, Lane::Free, || Ok(json!(2)));
        assert_eq!((snap.value, snap.updating), (Some(json!(2)), false));
        std::thread::sleep(Duration::from_millis(250));
        // A slow one: the last value, said to be updating.
        let snap = served.read("v", every, Lane::Free, || {
            std::thread::sleep(Duration::from_millis(1200));
            Ok(json!(3))
        });
        assert_eq!((snap.value.clone(), snap.updating), (Some(json!(2)), true));
        let shown: Value = serde_json::from_str(&reply_of(&snap, "It", |_| Value::Null)).unwrap();
        assert_eq!(shown["updating"], true);
        std::thread::sleep(Duration::from_millis(1300));
        assert_eq!(served.peek("v").value, Some(json!(3)));
    }

    /// The third stall: a burst of calls whose first reads are all slow waits `WAIT_BUDGET` in all,
    /// not 600 ms each; asked again while those reads are under way, no call waits at all.
    #[test]
    fn a_burst_of_slow_first_reads_waits_the_budget_at_most_and_never_again_for_a_read_under_way() {
        let served = Served::threaded();
        let slow = || {
            std::thread::sleep(Duration::from_millis(2000));
            Ok(json!(1))
        };
        let started = Instant::now();
        for n in 0..10 {
            assert!(served.read(&format!("k{n}"), Duration::from_secs(60), Lane::Free, slow).value.is_none());
        }
        assert!(started.elapsed() < WAIT_BUDGET + Duration::from_millis(300), "{:?}", started.elapsed());
        std::thread::sleep(Duration::from_millis(500));
        let again = Instant::now();
        for n in 0..10 {
            assert!(served.read(&format!("k{n}"), Duration::from_secs(60), Lane::Free, slow).updating);
        }
        assert!(again.elapsed() < Duration::from_millis(150), "{:?}", again.elapsed());
        std::thread::sleep(Duration::from_millis(1600));
        assert_eq!(served.peek("k9").value, Some(json!(1)));
    }

    #[test]
    fn hosts_are_told_apart_by_scheme_less_host() {
        assert_eq!(host_of("https://lez.nameswallet.app/"), "lez.nameswallet.app");
        assert_eq!(host_of("http://127.0.0.1:3040/rpc"), "127.0.0.1:3040");
    }
}
