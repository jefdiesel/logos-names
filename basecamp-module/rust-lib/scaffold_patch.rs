// The one change made to the scaffold the builder generates (logos-rust-sdk lidl-gen, single
// mode): its instance lock is taken with `try_lock` instead of `lock`, at all three places that
// take it. Used by build.rs on the generated file, and by a unit test on a copy of it.
//
// Why: a names call that calls another module (the Logos Blockchain node, the Program Registry)
// waits for the answer in a nested Qt event loop on the module's one thread. A second names call
// delivered inside that loop used to block on the lock the first call holds, on the same thread:
// the module froze for good (logos-d1's trace on the M2, 2026-11-14). Now that call is answered at
// once:
// - dispatch: the busy answer (`crate::scaffold_busy::answer`) before any module code runs, read
//   only from the method's name: no caller read, no state touched, nothing stored, so "busy"
//   always means "not taken, ask again". A lock poisoned by a panic is refused
//   (`crate::scaffold_busy::stopped`), never taken over.
// - ensure: returns (the instance exists, the ready hook has run).
// - the unload probe (logos_install!, which this module doesn't use): answers 0 (Synchronous).
//
// The substitution is checked: each of the three generated lines must be there exactly once, and
// afterwards no `INSTANCE.0.lock()` may be left and exactly three `INSTANCE.0.try_lock()` must
// be there. Anything else fails the build: a changed generator is never built unpatched.

/// The three lines as lidl-gen (logos-rust-sdk b572e81) writes them, each with what replaces it.
pub const SCAFFOLD_SITES: [(&str, &str, &str); 3] = [
    (
        "ensure_impl",
        "    fn ensure_impl<T: NamesModule + Default>(require_emit: bool) {\n        let mut guard = INSTANCE.0.lock().unwrap();\n",
        "    fn ensure_impl<T: NamesModule + Default>(require_emit: bool) {\n        // names: try_lock (scaffold_patch.rs). Held: a call is running and this one came in\n        // through its nested event loop; the instance exists, so there is nothing to do.\n        let mut guard = match INSTANCE.0.try_lock() { Ok(guard) => guard, Err(_) => return };\n",
    ),
    (
        "dispatch_impl",
        "    fn dispatch_impl<T: NamesModule + Default>(method: &str, args: &[serde_json::Value]) -> Option<serde_json::Value> {\n        let mut guard = INSTANCE.0.lock().unwrap();\n",
        "    fn dispatch_impl<T: NamesModule + Default>(method: &str, args: &[serde_json::Value]) -> Option<serde_json::Value> {\n        // names: try_lock (scaffold_patch.rs). Held: answered busy at once, before any module\n        // code runs. Poisoned: refused, never taken over.\n        let mut guard = match INSTANCE.0.try_lock() {\n            Ok(guard) => guard,\n            Err(std::sync::TryLockError::WouldBlock) => return Some(crate::scaffold_busy::answer(method)),\n            Err(std::sync::TryLockError::Poisoned(_)) => return Some(crate::scaffold_busy::stopped(method)),\n        };\n",
    ),
    (
        "unload probe (logos_install!)",
        "            let guard = INSTANCE.0.lock().unwrap();\n            let Some(any) = guard.as_ref() else { return 0 };\n",
        "            // names: try_lock (scaffold_patch.rs). Held or poisoned: Synchronous (0), never a wait.\n            let guard = match INSTANCE.0.try_lock() { Ok(guard) => guard, Err(_) => return 0 };\n            let Some(any) = guard.as_ref() else { return 0 };\n",
    ),
];

/// The generated scaffold with the three sites changed, or why it can't be.
pub fn patch_scaffold(generated: &str) -> Result<String, String> {
    let mut out = generated.to_owned();
    for (site, from, to) in SCAFFOLD_SITES {
        match out.matches(from).count() {
            1 => out = out.replacen(from, to, 1),
            n => return Err(format!("the generated scaffold has the {site} line {n} times, not once: the generator changed, so scaffold_patch.rs must be checked against it")),
        }
    }
    let locks = out.matches("INSTANCE.0.lock()").count();
    let tries = out.matches("INSTANCE.0.try_lock()").count();
    if locks != 0 || tries != 3 {
        return Err(format!("after the change the scaffold takes its instance lock with lock() {locks} times and try_lock() {tries} times (want 0 and 3)"));
    }
    Ok(out)
}
