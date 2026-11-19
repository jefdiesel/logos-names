//! Every call names makes into another Basecamp module goes through here: the Logos
//! Blockchain node (`blockchain_module`), the Program Registry (`program_registry`) and
//! `package_manager`. Nothing else in the crate touches the SDK's `LogosModuleSDK`.
//!
//! Why (the hang of 2026-11-18, builds 04be0aa and a93fe34, Basecamp 0.3.2). Inside the names
//! host every module call runs on the host's Qt main thread, which is also the thread every
//! names call is dispatched on (`concurrency: single`):
//! - `PluginProxy::new` → `lp_client_create` builds the client on the Qt main thread
//!   (`logos::runOnQtMainThread`, a `BlockingQueuedConnection` when called from any other
//!   thread), and `lp_invoke` runs the call there too (`LogosAPIClient::invokeRemoteMethod` →
//!   `runOnOwnerThread`). A worker thread that calls another module therefore waits, in
//!   `QSemaphore::acquire`, until the main thread serves it.
//! - A call to a module that isn't loaded waits its whole budget on the main thread
//!   (`RemoteTransportConnection::requestObject` → `waitForSource`, 15 s for the node's reads),
//!   holding the names instance lock: every other names call is answered "busy" meanwhile, and
//!   the Names app polls, so the wallet answered nothing while the node was off.
//! - The Program Registry and package_manager were asked from threads names spawned
//!   (`std::thread::spawn` → `LogosModuleSDK::new().plugin(..)`): those are the four threads the
//!   sample of the stuck host showed in `lp_client_create` → `runOnQtMainThread` →
//!   `QSemaphore::acquire`. Once the main thread stops serving queued calls they never return,
//!   and neither does anything that waits on them.
//!
//! The rules here:
//! 1. Only the call thread (the host's main thread) creates a client or calls another module.
//!    From any other thread a call is refused before it reaches the SDK (`Failure::OffThread`),
//!    so no thread names owns can ever park in `runOnQtMainThread`.
//! 2. A module that isn't loaded is known at once, without the SDK and without Qt: its endpoint
//!    (the QtRO registry socket `logos_<module>_<instance>`, `LogosInstance::id`) is probed with
//!    a plain connect, as the protocol's own `endpointHasListener` does (`endpoint_presence`).
//!    Nothing listening: `Failure::NotLoaded` in microseconds, never a 15 s wait.
//! 3. A module that is there but gave no answer is not asked again for `HOLD`
//!    (`Failure::Held`), unless it is seen to come back up first.
//! 4. Asks whose answers are only kept for later (the Program Registry's labels and
//!    publishers, package_manager's list) are asynchronous (`ask`): sent from the call thread,
//!    answered later by the host's event loop, never waited for.

use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Whether a module is loaded, as its endpoint says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    /// Something listens at its endpoint.
    Up,
    /// Nothing does: the module isn't loaded (or not yet).
    Down,
    /// It can't be told here (no instance id, endpoints kept elsewhere, the probe didn't end).
    Unknown,
}

/// Why a call to another module has no answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// The module isn't loaded. Nothing was sent.
    NotLoaded,
    /// It gave no answer to an ask less than `HOLD` ago, so it wasn't asked. Nothing was sent.
    Held,
    /// Asked from a thread other than the call thread: refused before the SDK. Nothing was sent.
    OffThread,
    /// Sent, and no answer came (the wait ran out, the module went away mid-call): it may have
    /// been taken, so what it does may have happened.
    NoAnswer(String),
    /// It answered with a refusal (bad arguments, an unknown method, a refused token).
    Refused(String),
}

pub type Done = Box<dyn FnOnce(Result<Value, Failure>) + Send + 'static>;

/// The host side of a module call: the SDK in a module build, a stand-in in tests.
pub trait Host: Sync {
    fn presence(&self, module: &str) -> Presence;
    /// Whether this thread is the one module calls may be made from.
    fn on_call_thread(&self) -> bool;
    fn call(&self, module: &str, method: &str, args: &Value, wait: Duration) -> Result<Value, Failure>;
    /// Sends the call and returns at once; `done` is called once, later, with its answer
    /// (or at once, when it couldn't be sent).
    fn ask(&self, module: &str, method: &str, args: &Value, wait: Duration, done: Done);
}

/// How long a probe's answer is kept (a probe costs a connect, so this only spares a burst).
pub const PRESENCE_KEEP: Duration = Duration::from_secs(1);
/// How long a module that gave no answer is left alone.
pub const HOLD: Duration = Duration::from_secs(30);

/// What is known about the other modules. It holds no lock while a call is on its way: an
/// answer delivered by the event loop inside another call's wait may come back to it.
pub struct Gate {
    seen: Mutex<HashMap<String, (Presence, Instant)>>,
    failed: Mutex<HashMap<String, Instant>>,
    /// How long a module that gave no answer is left alone (`HOLD`; none in most unit tests,
    /// whose stand-in node "gives no answer" to every method it wasn't told about).
    hold: Duration,
}

impl Default for Gate {
    fn default() -> Self {
        Gate::holding(HOLD)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl Gate {
    pub fn holding(hold: Duration) -> Self {
        Gate { seen: Mutex::default(), failed: Mutex::default(), hold }
    }

    pub fn presence(&self, host: &dyn Host, module: &str) -> Presence {
        if let Some((presence, at)) = lock(&self.seen).get(module).copied() {
            if at.elapsed() < PRESENCE_KEEP {
                return presence;
            }
        }
        let presence = host.presence(module);
        let was = lock(&self.seen).insert(module.to_owned(), (presence, Instant::now())).map(|(was, _)| was);
        // Seen coming up: what it didn't answer while it was starting is forgotten.
        if presence == Presence::Up && was == Some(Presence::Down) {
            lock(&self.failed).remove(module);
        }
        presence
    }

    fn admit(&self, host: &dyn Host, module: &str) -> Result<(), Failure> {
        if !host.on_call_thread() {
            return Err(Failure::OffThread);
        }
        if self.presence(host, module) == Presence::Down {
            return Err(Failure::NotLoaded);
        }
        if lock(&self.failed).get(module).is_some_and(|at| at.elapsed() < self.hold) {
            return Err(Failure::Held);
        }
        Ok(())
    }

    fn learn(&self, module: &str, answer: &Result<Value, Failure>) {
        match answer {
            Err(Failure::NoAnswer(_)) => {
                lock(&self.failed).insert(module.to_owned(), Instant::now());
            }
            Ok(_) | Err(Failure::Refused(_)) => {
                lock(&self.failed).remove(module);
            }
            Err(_) => {}
        }
    }

    /// Calls `module` and waits for its answer, up to `wait`, when it may be called (see the
    /// rules above); otherwise fails at once.
    pub fn call(&self, host: &dyn Host, module: &str, method: &str, args: &Value, wait: Duration) -> Result<Value, Failure> {
        self.admit(host, module)?;
        let answer = host.call(module, method, args, wait);
        self.learn(module, &answer);
        answer
    }

    /// Asks `module` without waiting: `done` gets the answer later, from the host's event loop,
    /// or at once when it may not be asked. `done` must not need a lock its caller holds.
    pub fn ask(self: &Arc<Self>, host: &dyn Host, module: &str, method: &str, args: &Value, wait: Duration, done: Done) {
        if let Err(failure) = self.admit(host, module) {
            return done(Err(failure));
        }
        let gate = Arc::clone(self);
        let name = module.to_owned();
        host.ask(
            module,
            method,
            args,
            wait,
            Box::new(move |answer| {
                gate.learn(&name, &answer);
                done(answer)
            }),
        );
    }
}

/// `module` called, its answer waited for (see `Gate::call`).
pub fn call(module: &str, method: &str, args: &Value, wait: Duration) -> Result<Value, Failure> {
    with_gate(|gate| gate.call(host(), module, method, args, wait))
}

/// `module` asked, its answer given to `done` later (see `Gate::ask`).
pub fn ask(module: &str, method: &str, args: &Value, wait: Duration, done: Done) {
    let gate = with_gate(Arc::clone);
    gate.ask(host(), module, method, args, wait, done)
}

/// Whether `module` is loaded, as far as can be told now (never waits).
pub fn presence(module: &str) -> Presence {
    with_gate(|gate| gate.presence(host(), module))
}

#[cfg(not(test))]
fn with_gate<R>(f: impl FnOnce(&Arc<Gate>) -> R) -> R {
    static GATE: std::sync::OnceLock<Arc<Gate>> = std::sync::OnceLock::new();
    f(GATE.get_or_init(Default::default))
}

// Each test thread has its own, so tests running at once don't see each other's failures; it
// holds nothing unless the test asks (`stand_in::hold_failures`).
#[cfg(test)]
thread_local! {
    static TEST_GATE: std::cell::RefCell<Arc<Gate>> = std::cell::RefCell::new(Arc::new(Gate::holding(Duration::ZERO)));
}

#[cfg(test)]
fn with_gate<R>(f: impl FnOnce(&Arc<Gate>) -> R) -> R {
    let gate = TEST_GATE.with(|gate| Arc::clone(&gate.borrow()));
    f(&gate)
}

#[cfg(not(test))]
fn host() -> &'static dyn Host {
    &qt::Qt
}

#[cfg(test)]
fn host() -> &'static dyn Host {
    &stand_in::StandIn
}

/// What a failed SDK call was, from its error: no answer (the module isn't reachable, the wait
/// ran out) or a refusal by a module that answered.
pub fn classify(error: &logos_rust_sdk::LogosError) -> Failure {
    use logos_rust_sdk::LogosError;
    match error {
        LogosError::PluginCallFailed { message, .. } => classify_message(message),
        LogosError::InvalidTimeout { .. } | LogosError::InvalidString(_) | LogosError::JsonError(_) => Failure::Refused(error.to_string()),
        // "Failed to create protocol client", a closed channel: nothing answered.
        _ => Failure::NoAnswer(error.to_string()),
    }
}

/// `classify` for a call's error text: the protocol's error object (`{code, message, origin}`,
/// as `lp_invoke` gives it) or its message alone (as `lp_invoke_async` gives it).
pub fn classify_message(message: &str) -> Failure {
    let parsed = serde_json::from_str::<Value>(message).ok();
    let code = parsed.as_ref().and_then(|value| value["code"].as_str()).unwrap_or_default().to_owned();
    let text = parsed.as_ref().and_then(|value| value["message"].as_str()).unwrap_or(message).to_owned();
    if ["dispatch_failed", "invalid_args", "unknown_method", "unauthorized"].contains(&code.as_str()) {
        return Failure::Refused(text);
    }
    // The SDK's own argument checks, as a provider's dispatch words them.
    let lower = text.to_ascii_lowercase();
    let unreachable = ["unavailable", "acquire", "timed out", "timeout", "not loaded", "transport", "unreachable", "no listener"];
    let refusal = ["expected ", "unknown method", "invalid arg", "wrong number of arg"];
    if code.is_empty() && !unreachable.iter().any(|word| lower.contains(word)) && refusal.iter().any(|word| lower.contains(word)) {
        return Failure::Refused(text);
    }
    // Anything else is taken as no answer: a call that may have reached the module is never
    // reported as one it refused (the chain methods keep such a send as maybe made).
    Failure::NoAnswer(text)
}

/// Whether the module whose QtRO registry socket is `logos_<module>_<instance>` in `dir` is
/// listening there. The rule is the protocol's (`LogosInstance::id`, and Qt's `QLocalServer`
/// puts a relative name under `QDir::tempPath()`), and it is checked against this module's own
/// endpoint (`own`, which names is listening on while it runs): when that isn't found the host
/// keeps endpoints elsewhere and nothing is claimed (Unknown). The probe is a plain connect,
/// dropped at once, as `RemoteTransportConnection::endpointHasListener` does; it runs on a
/// thread of its own and is given `within` to end (a connect to a full backlog can block).
#[cfg(unix)]
pub fn endpoint_presence(dir: &std::path::Path, instance: &str, own: &str, module: &str, within: Duration) -> Presence {
    let safe = |text: &str| !text.is_empty() && text.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if !safe(instance) || !safe(module) || !safe(own) {
        return Presence::Unknown;
    }
    if !dir.join(format!("logos_{own}_{instance}")).exists() {
        return Presence::Unknown;
    }
    let path = dir.join(format!("logos_{module}_{instance}"));
    let (tell, told) = std::sync::mpsc::channel();
    let probe = std::thread::Builder::new().name("names-endpoint-probe".into()).spawn(move || {
        let presence = match std::os::unix::net::UnixStream::connect(&path) {
            Ok(_) => Presence::Up,
            Err(error) if matches!(error.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused) => Presence::Down,
            Err(_) => Presence::Unknown,
        };
        let _ = tell.send(presence);
    });
    if probe.is_err() {
        return Presence::Unknown;
    }
    told.recv_timeout(within).unwrap_or(Presence::Unknown)
}

#[cfg(not(unix))]
pub fn endpoint_presence(_dir: &std::path::Path, _instance: &str, _own: &str, _module: &str, _within: Duration) -> Presence {
    Presence::Unknown
}

/// Where Qt puts a relative local-socket name: `QDir::tempPath()`, which is `$TMPDIR` (or
/// `/tmp`), cleaned. (On macOS with no TMPDIR Qt asks Foundation; then names finds no endpoint
/// of its own there and says Unknown, as it should.)
pub fn socket_dir() -> std::path::PathBuf {
    let dir = std::env::var("TMPDIR").ok().filter(|dir| !dir.is_empty()).unwrap_or_else(|| "/tmp".to_owned());
    let trimmed = dir.trim_end_matches('/');
    std::path::PathBuf::from(if trimmed.is_empty() { "/" } else { trimmed })
}

/// Whether this is the process's main thread, which is the host's Qt main thread: the one
/// whose event loop serves every module call (`QCoreApplication` is made in the host's main()).
#[cfg(target_os = "macos")]
pub fn main_thread() -> bool {
    unsafe { libc::pthread_main_np() != 0 }
}

#[cfg(target_os = "linux")]
pub fn main_thread() -> bool {
    unsafe { libc::syscall(libc::SYS_gettid) as u32 == std::process::id() }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn main_thread() -> bool {
    true
}

/// The SDK, in a module build.
#[cfg(not(test))]
mod qt {
    use super::{classify, Done, Failure, Host, Presence};
    use logos_rust_sdk::LogosModuleSDK;
    use serde_json::Value;
    use std::time::Duration;

    pub struct Qt;

    impl Host for Qt {
        fn presence(&self, module: &str) -> Presence {
            let Ok(instance) = std::env::var("LOGOS_INSTANCE_ID") else { return Presence::Unknown };
            super::endpoint_presence(&super::socket_dir(), &instance, "names", module, Duration::from_millis(250))
        }

        /// The main thread (the 2026-11-18 sample shows names calls dispatched there:
        /// `main → QCoreApplication::exec → … → logos_module_dispatch`), or any thread a names
        /// call is being dispatched on (the host pushed its caller there). A thread names spawned
        /// is neither.
        fn on_call_thread(&self) -> bool {
            super::main_thread() || logos_rust_sdk::current_caller_json().is_some()
        }

        fn call(&self, module: &str, method: &str, args: &Value, wait: Duration) -> Result<Value, Failure> {
            LogosModuleSDK::new().plugin(module).call_json_with_timeout(method, args, wait).map_err(|error| classify(&error))
        }

        fn ask(&self, module: &str, method: &str, args: &Value, wait: Duration, done: Done) {
            LogosModuleSDK::new()
                .plugin(module)
                .call_json_async_with_timeout(method, args, wait, move |answer| done(answer.map_err(|error| classify(&error))));
        }
    }
}

/// The host in unit tests. A test thread is the call thread (libtest names its threads; one
/// spawned by names code has no name). Each module is up unless the test says it is down
/// (`set_down`); the node answers what `node::ANSWERS` holds, as before.
#[cfg(test)]
pub mod stand_in {
    use super::{Done, Failure, Host, Presence};
    use serde_json::Value;
    use std::cell::RefCell;
    use std::collections::HashSet;
    use std::time::Duration;

    thread_local! {
        static DOWN: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
    }

    /// From now on in this test thread a module that gave no answer is held, as in a module build.
    pub fn hold_failures() {
        super::TEST_GATE.with(|gate| *gate.borrow_mut() = std::sync::Arc::new(super::Gate::default()));
    }

    /// `module` is down (`down`) or up for the rest of this test thread.
    pub fn set_down(module: &str, down: bool) {
        DOWN.with(|set| {
            let mut set = set.borrow_mut();
            if down {
                set.insert(module.to_owned());
            } else {
                set.remove(module);
            }
        });
    }

    pub struct StandIn;

    impl Host for StandIn {
        fn presence(&self, module: &str) -> Presence {
            if DOWN.with(|set| set.borrow().contains(module)) {
                Presence::Down
            } else {
                Presence::Up
            }
        }

        fn on_call_thread(&self) -> bool {
            std::thread::current().name().is_some()
        }

        fn call(&self, module: &str, method: &str, args: &Value, _wait: Duration) -> Result<Value, Failure> {
            match module {
                crate::chain::MODULE => crate::node::answer(method, args),
                _ => Err(Failure::NoAnswer(format!("no stand-in for {module}"))),
            }
        }

        fn ask(&self, module: &str, _method: &str, _args: &Value, _wait: Duration, done: Done) {
            done(Err(Failure::NoAnswer(format!("no stand-in for {module}"))))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Condvar;

    /// A host as the names host is: one Qt main thread serves every call. A module that isn't
    /// loaded costs a call its whole wait (`waitForSource`), and a call or a client made from
    /// another thread waits for the main thread (`runOnQtMainThread`, BlockingQueuedConnection):
    /// here it waits until `serve` lets it, as it would for a main thread that is never free.
    struct QtLike {
        main: std::thread::ThreadId,
        up: Mutex<HashMap<String, Duration>>,
        calls: AtomicUsize,
        waited: Mutex<Duration>,
        off_thread: AtomicUsize,
        served: (Mutex<bool>, Condvar),
        queued: Mutex<Vec<(String, Done)>>,
    }

    impl QtLike {
        fn new() -> Arc<Self> {
            Arc::new(QtLike {
                main: std::thread::current().id(),
                up: Mutex::new(HashMap::new()),
                calls: AtomicUsize::new(0),
                waited: Mutex::new(Duration::ZERO),
                off_thread: AtomicUsize::new(0),
                served: (Mutex::new(false), Condvar::new()),
                queued: Mutex::new(Vec::new()),
            })
        }

        /// `module` is loaded and answers after `slow`.
        fn load(&self, module: &str, slow: Duration) {
            lock(&self.up).insert(module.to_owned(), slow);
        }

        fn unload(&self, module: &str) {
            lock(&self.up).remove(module);
        }

        /// The main thread's event loop runs once: queued answers are delivered.
        fn run_loop(&self) {
            let queued: Vec<(String, Done)> = std::mem::take(&mut *lock(&self.queued));
            for (module, done) in queued {
                let slow = lock(&self.up).get(&module).copied();
                match slow {
                    Some(_) => done(Ok(json!(format!("{module} answered")))),
                    None => done(Err(Failure::NoAnswer("Timeout waiting for replica".into()))),
                }
            }
        }

        fn wait_for_main_thread(&self) {
            self.off_thread.fetch_add(1, Ordering::SeqCst);
            let (served, freed) = &self.served;
            let guard = lock(served);
            let _unused = freed.wait_while(guard, |served| !*served).unwrap_or_else(|e| e.into_inner());
        }
    }

    impl Host for QtLike {
        fn presence(&self, module: &str) -> Presence {
            if lock(&self.up).contains_key(module) {
                Presence::Up
            } else {
                Presence::Down
            }
        }

        fn on_call_thread(&self) -> bool {
            std::thread::current().id() == self.main
        }

        fn call(&self, module: &str, _method: &str, _args: &Value, wait: Duration) -> Result<Value, Failure> {
            if std::thread::current().id() != self.main {
                self.wait_for_main_thread();
            }
            self.calls.fetch_add(1, Ordering::SeqCst);
            let slow = lock(&self.up).get(module).copied();
            match slow {
                Some(slow) => {
                    std::thread::sleep(slow.min(wait));
                    *lock(&self.waited) += slow.min(wait);
                    if slow > wait {
                        Err(Failure::NoAnswer("timed out".into()))
                    } else {
                        Ok(json!(format!("{module} answered")))
                    }
                }
                None => {
                    // Nothing listening: the whole budget, then "Timeout waiting for replica".
                    std::thread::sleep(wait);
                    *lock(&self.waited) += wait;
                    Err(Failure::NoAnswer("Timeout waiting for replica".into()))
                }
            }
        }

        fn ask(&self, module: &str, _method: &str, _args: &Value, _wait: Duration, done: Done) {
            if std::thread::current().id() != self.main {
                self.wait_for_main_thread();
            }
            self.calls.fetch_add(1, Ordering::SeqCst);
            lock(&self.queued).push((module.to_owned(), done));
        }
    }

    const WAIT: Duration = Duration::from_millis(400);

    /// (1) The node isn't loaded: every call that needs it fails at once with NotLoaded, again
    /// and again, and nothing is sent or waited for (before: 15 s each, every poll).
    #[test]
    fn a_module_that_isnt_loaded_fails_at_once_and_is_never_waited_for() {
        let host = QtLike::new();
        let gate = Arc::new(Gate::default());
        let started = Instant::now();
        for _ in 0..50 {
            assert_eq!(gate.call(&*host, "blockchain_module", "get_cryptarchia_info", &json!([]), WAIT), Err(Failure::NotLoaded));
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(started.elapsed() < WAIT, "50 calls took {:?}", started.elapsed());
        assert_eq!(host.calls.load(Ordering::SeqCst), 0, "nothing reached the host");
        assert_eq!(*lock(&host.waited), Duration::ZERO);
    }

    /// (1) Loaded but giving no answer (wedged, or starting): one wait, then held for HOLD,
    /// not one 15 s wait every poll; and seen coming back up, it is asked again at once.
    #[test]
    fn a_module_that_gave_no_answer_is_held_then_asked_again_once_it_is_back() {
        let host = QtLike::new();
        let gate = Arc::new(Gate::default());
        host.load("blockchain_module", Duration::from_secs(5));
        assert!(matches!(gate.call(&*host, "blockchain_module", "get_time_info", &json!([]), WAIT), Err(Failure::NoAnswer(_))));
        let started = Instant::now();
        for _ in 0..20 {
            assert_eq!(gate.call(&*host, "blockchain_module", "get_time_info", &json!([]), WAIT), Err(Failure::Held));
        }
        assert!(started.elapsed() < Duration::from_millis(100));
        assert_eq!(host.calls.load(Ordering::SeqCst), 1, "held, not asked again");
        // It goes away and comes back (a restart): the hold is dropped.
        host.unload("blockchain_module");
        std::thread::sleep(PRESENCE_KEEP + Duration::from_millis(50));
        assert_eq!(gate.call(&*host, "blockchain_module", "get_time_info", &json!([]), WAIT), Err(Failure::NotLoaded));
        host.load("blockchain_module", Duration::ZERO);
        std::thread::sleep(PRESENCE_KEEP + Duration::from_millis(50));
        assert_eq!(gate.call(&*host, "blockchain_module", "get_time_info", &json!([]), WAIT), Ok(json!("blockchain_module answered")));
    }

    /// (2) The deadlock's precondition: a thread names spawned asking another module. With the
    /// host's main thread never free, such a thread waits for ever in runOnQtMainThread (here:
    /// `wait_for_main_thread`). The gate refuses it before the host, so it ends at once.
    #[test]
    fn a_worker_thread_never_reaches_the_host_so_it_cannot_wait_on_the_main_thread() {
        let host = QtLike::new();
        let gate = Arc::new(Gate::default());
        host.load("program_registry", Duration::ZERO);
        host.load("package_manager", Duration::ZERO);
        let finished = Arc::new(AtomicUsize::new(0));
        let workers: Vec<_> = ["program_registry", "program_registry", "package_manager", "program_registry"]
            .into_iter()
            .map(|module| {
                let (host, gate, finished) = (Arc::clone(&host), Arc::clone(&gate), Arc::clone(&finished));
                std::thread::spawn(move || {
                    let called = gate.call(&*host, module, "program_label", &json!(["x"]), WAIT);
                    let asked = Arc::new(Mutex::new(None));
                    let into = Arc::clone(&asked);
                    gate.ask(&*host, module, "verify_app", &json!(["x"]), WAIT, Box::new(move |answer| *lock(&into) = Some(answer)));
                    finished.fetch_add(1, Ordering::SeqCst);
                    let asked = lock(&asked).take();
                    (called, asked)
                })
            })
            .collect();
        let deadline = Instant::now() + Duration::from_secs(5);
        while finished.load(Ordering::SeqCst) < 4 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        // Freed only now, so a worker that did reach the host can't hang the test run.
        *lock(&host.served.0) = true;
        host.served.1.notify_all();
        assert_eq!(finished.load(Ordering::SeqCst), 4, "every worker ended on its own");
        assert_eq!(host.off_thread.load(Ordering::SeqCst), 0, "no worker reached the host");
        for worker in workers {
            assert_eq!(worker.join().unwrap(), (Err(Failure::OffThread), Some(Err(Failure::OffThread))));
        }
    }

    /// (2) The names host starts before the node does (Basecamp restarted with the Names tab
    /// open): every call answers at once while it is down, nothing waits, and the first call
    /// after it comes up reaches it.
    #[test]
    fn names_started_before_the_node_answers_at_once_then_reaches_the_node_once_it_is_up() {
        let host = QtLike::new();
        let gate = Arc::new(Gate::default());
        let started = Instant::now();
        for _ in 0..10 {
            assert_eq!(gate.call(&*host, "blockchain_module", "wallet_get_known_addresses", &json!([]), WAIT), Err(Failure::NotLoaded));
            let got = Arc::new(Mutex::new(None));
            let into = Arc::clone(&got);
            gate.ask(&*host, "program_registry", "program_label", &json!(["x"]), WAIT, Box::new(move |answer| *lock(&into) = Some(answer)));
            assert_eq!(lock(&got).take(), Some(Err(Failure::NotLoaded)), "answered at once, without the host");
        }
        assert!(started.elapsed() < WAIT, "{:?}", started.elapsed());
        assert_eq!(host.calls.load(Ordering::SeqCst), 0);
        host.load("blockchain_module", Duration::ZERO);
        std::thread::sleep(PRESENCE_KEEP + Duration::from_millis(50));
        assert_eq!(gate.call(&*host, "blockchain_module", "wallet_get_known_addresses", &json!([]), WAIT), Ok(json!("blockchain_module answered")));
    }

    /// A slow module doesn't hold up a call that doesn't need it: an ask returns at once and its
    /// answer comes from the event loop later; a call to another module is answered meanwhile.
    #[test]
    fn a_slow_module_does_not_delay_an_unrelated_call() {
        let host = QtLike::new();
        let gate = Arc::new(Gate::default());
        host.load("program_registry", Duration::from_secs(3));
        host.load("package_manager", Duration::ZERO);
        let answered = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&answered);
        let started = Instant::now();
        gate.ask(&*host, "program_registry", "verify_app", &json!(["x"]), Duration::from_secs(15), Box::new(move |answer| {
            assert!(answer.is_ok());
            flag.store(true, Ordering::SeqCst);
        }));
        assert_eq!(gate.call(&*host, "package_manager", "getInstalledModules", &json!([]), WAIT), Ok(json!("package_manager answered")));
        assert!(started.elapsed() < Duration::from_millis(200), "{:?}", started.elapsed());
        assert!(!answered.load(Ordering::SeqCst), "the slow answer isn't waited for");
        host.run_loop();
        assert!(answered.load(Ordering::SeqCst), "it is delivered by the event loop");
    }

    #[test]
    fn errors_are_told_apart_as_no_answer_or_a_refusal() {
        let unavailable = r#"{"code":"object_unavailable","message":"failed to acquire remote object 'blockchain_module' (module not loaded, not published, or transport failure)","origin":"blockchain_module"}"#;
        assert!(matches!(classify_message(unavailable), Failure::NoAnswer(_)));
        assert!(matches!(classify_message("call blockchain_module.get_time_info timed out after 15000 ms"), Failure::NoAnswer(_)));
        assert!(matches!(classify_message(r#"{"code":"unknown_method","message":"no such method","origin":"x"}"#), Failure::Refused(_)));
        assert!(matches!(classify_message("expected string at arg0, got number"), Failure::Refused(_)));
        // Its words are another module's: cleaned (after they are classified).
        match classify_message(r#"{"code":"unknown_method","message":"no such\u202e method\n","origin":"x"}"#) {
            Failure::Refused(text) => assert_eq!(text, "no such method"),
            other => panic!("{other:?}"),
        }
        match classify_message("timed out\u{200B}\u{202E} here") {
            Failure::NoAnswer(text) => assert_eq!(text, "timed out here"),
            other => panic!("{other:?}"),
        }
        let client = logos_rust_sdk::LogosError::Other("Failed to create protocol client for blockchain_module".into());
        assert!(matches!(classify(&client), Failure::NoAnswer(_)));
    }

    #[cfg(unix)]
    #[test]
    fn the_endpoint_probe_tells_loaded_from_not_loaded_without_the_sdk() {
        use std::os::unix::net::UnixListener;
        let dir = std::env::temp_dir().join(format!("np{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let within = Duration::from_millis(500);
        // No endpoint of our own there: the rule isn't this host's, nothing is claimed.
        assert_eq!(endpoint_presence(&dir, "abc123", "names", "blockchain_module", within), Presence::Unknown);
        let _own = UnixListener::bind(dir.join("logos_names_abc123")).unwrap();
        assert_eq!(endpoint_presence(&dir, "abc123", "names", "blockchain_module", within), Presence::Down);
        let node = UnixListener::bind(dir.join("logos_blockchain_module_abc123")).unwrap();
        assert_eq!(endpoint_presence(&dir, "abc123", "names", "blockchain_module", within), Presence::Up);
        // The module stopped and left its socket file behind: nothing listens, so down.
        drop(node);
        assert_eq!(endpoint_presence(&dir, "abc123", "names", "blockchain_module", within), Presence::Down);
        assert_eq!(endpoint_presence(&dir, "../x", "names", "blockchain_module", within), Presence::Unknown);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
