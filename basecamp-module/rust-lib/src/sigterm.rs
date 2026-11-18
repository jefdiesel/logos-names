//! The names host exits on SIGTERM, even when its main thread can't.
//!
//! The host's own SIGTERM handling ends the process from its Qt event loop, so a host whose main
//! thread is stuck never ends: the hung host of 2026-11-18 ignored SIGTERM and needed kill -9.
//! peer.rs removes the way names got it stuck; this is the backstop for any other way.
//!
//! `arm` (once, when the module is installed) puts a handler in front of whatever the host had:
//! the handler only writes a byte to a pipe (async-signal-safe) and then does what the host's
//! own disposition said (its handler, the default, or nothing). A thread waiting on the pipe
//! gives the host `grace` to end on its own, then ends the process with `_exit(143)`, the status
//! a SIGTERM death reports. A host that already ends on SIGTERM is not changed: it ends first.
//! A host that installs its own handler after names does replaces this one, which is its call.

use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

/// How long the host is given to end on its own after a SIGTERM (Basecamp gives a module 10 s
/// of its 12 s unload window).
pub const GRACE: Duration = Duration::from_secs(10);

static PIPE_WRITE: AtomicI32 = AtomicI32::new(-1);
#[cfg(unix)]
static PREVIOUS: OnceLock<libc::sigaction> = OnceLock::new();
static ARMED: OnceLock<bool> = OnceLock::new();

/// Puts the backstop in place (once; later calls do nothing). Whether it is in place.
pub fn arm(grace: Duration) -> bool {
    *ARMED.get_or_init(|| install(grace))
}

#[cfg(not(unix))]
fn install(_grace: Duration) -> bool {
    false
}

#[cfg(unix)]
#[allow(unused_unsafe)]
fn install(grace: Duration) -> bool {
    unsafe {
        let mut fds = [0 as libc::c_int; 2];
        if libc::pipe(fds.as_mut_ptr()) != 0 {
            return false;
        }
        for fd in fds {
            libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
        }
        let mut previous: libc::sigaction = std::mem::zeroed();
        if libc::sigaction(libc::SIGTERM, std::ptr::null(), &mut previous) != 0 || PREVIOUS.set(previous).is_err() {
            libc::close(fds[0]);
            libc::close(fds[1]);
            return false;
        }
        let read = fds[0];
        let watch = std::thread::Builder::new().name("names-sigterm".into()).spawn(move || {
            let mut byte = 0u8;
            loop {
                let got = unsafe { libc::read(read, (&mut byte as *mut u8).cast(), 1) };
                if got == 1 {
                    break;
                }
                if got < 0 && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return;
            }
            std::thread::sleep(grace);
            unsafe { libc::_exit(128 + libc::SIGTERM) }
        });
        if watch.is_err() {
            return false;
        }
        PIPE_WRITE.store(fds[1], Ordering::SeqCst);
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = on_sigterm as *const () as usize;
        action.sa_flags = libc::SA_SIGINFO | libc::SA_RESTART | (previous.sa_flags & libc::SA_ONSTACK);
        libc::sigemptyset(&mut action.sa_mask);
        libc::sigaction(libc::SIGTERM, &action, std::ptr::null_mut()) == 0
    }
}

/// Only async-signal-safe calls: write, sigaction, raise, and the host's own handler.
#[cfg(unix)]
extern "C" fn on_sigterm(signal: libc::c_int, info: *mut libc::siginfo_t, context: *mut libc::c_void) {
    let fd = PIPE_WRITE.load(Ordering::Relaxed);
    if fd >= 0 {
        let byte = 1u8;
        unsafe { libc::write(fd, (&byte as *const u8).cast(), 1) };
    }
    let Some(previous) = PREVIOUS.get() else { return };
    let handler = previous.sa_sigaction;
    unsafe {
        if handler == libc::SIG_IGN {
            // The host ignores SIGTERM: the watch thread still ends it after the grace.
        } else if handler == libc::SIG_DFL {
            // The default for SIGTERM is to end the process at once: done as the default does,
            // so the parent sees a death by SIGTERM (delivered once this handler returns).
            let mut default: libc::sigaction = std::mem::zeroed();
            default.sa_sigaction = libc::SIG_DFL;
            libc::sigemptyset(&mut default.sa_mask);
            libc::sigaction(libc::SIGTERM, &default, std::ptr::null_mut());
            libc::raise(libc::SIGTERM);
        } else if previous.sa_flags & libc::SA_SIGINFO != 0 {
            let host: extern "C" fn(libc::c_int, *mut libc::siginfo_t, *mut libc::c_void) = std::mem::transmute(handler);
            host(signal, info, context);
        } else {
            let host: extern "C" fn(libc::c_int) = std::mem::transmute(handler);
            host(signal);
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;
    use std::process::Command;
    use std::time::Instant;

    /// Runs `child` of this test binary in a process of its own (`NAMES_SIGTERM_CHILD`), and
    /// gives back how it ended and how long it took.
    fn run(child: &str, mode: &str) -> (std::process::ExitStatus, Duration) {
        let started = Instant::now();
        let mut process = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", child, "--test-threads=1", "--nocapture"])
            .env("NAMES_SIGTERM_CHILD", mode)
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(status) = process.try_wait().unwrap() {
                return (status, started.elapsed());
            }
            if Instant::now() > deadline {
                let _ = process.kill();
                panic!("{child} ({mode}) didn't end on SIGTERM");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    static HOST_SAW: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    extern "C" fn host_handler(_signal: libc::c_int) {
        HOST_SAW.store(true, Ordering::SeqCst);
    }

    /// The child: sets the host's disposition, arms the backstop, takes a SIGTERM and then acts
    /// as the hung host did (never returns to an event loop); or, as a working host, ends.
    #[test]
    fn sigterm_child() {
        let Ok(mode) = std::env::var("NAMES_SIGTERM_CHILD") else { return };
        unsafe {
            match mode.as_str() {
                "ignored" => {
                    libc::signal(libc::SIGTERM, libc::SIG_IGN);
                }
                "handled" | "working" => {
                    libc::signal(libc::SIGTERM, host_handler as *const () as usize);
                }
                _ => {}
            }
        }
        assert!(arm(Duration::from_millis(300)));
        unsafe { libc::kill(libc::getpid(), libc::SIGTERM) };
        let since = Instant::now();
        while since.elapsed() < Duration::from_secs(15) {
            if mode == "working" && HOST_SAW.load(Ordering::SeqCst) {
                // A host whose handler works ends on its own, before the grace.
                std::process::exit(0);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn a_host_that_never_ends_on_sigterm_is_ended_after_the_grace() {
        // A host that ignores it, or whose handler only asks a stuck event loop to quit.
        for mode in ["ignored", "handled"] {
            let (status, took) = run("sigterm::tests::sigterm_child", mode);
            assert_eq!(status.code(), Some(143), "{mode}: {status:?}");
            assert!(took < Duration::from_secs(10), "{mode}: {took:?}");
        }
    }

    #[test]
    fn the_hosts_own_sigterm_handling_still_runs_first() {
        // No handler: the default, a death by SIGTERM, at once.
        let (status, took) = run("sigterm::tests::sigterm_child", "default");
        assert_eq!(status.signal(), Some(libc::SIGTERM), "{status:?}");
        assert!(took < Duration::from_secs(10), "{took:?}");
        // A handler that works: the host's handler ran and the host ended itself (status 0).
        let (status, _) = run("sigterm::tests::sigterm_child", "working");
        assert_eq!(status.code(), Some(0), "{status:?}");
    }
}
