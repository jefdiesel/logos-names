# Upstream issue (draft, not posted): a Rust module freezes when a module it calls isn't loaded

For logos-co/logos-rust-sdk (items 1 and 2), and logos-co/logos-protocol and
logos-co/logos-module-builder (item 3). Written 2026-11-18 from the names module's hang on Basecamp
0.3.2. Versions: logos-module-builder de169fd, logos-rust-sdk b572e81, logos-protocol 8bbc027.

---

**Title:** Rust SDK: calls made off the Qt main thread wait on it with no bound, and a call to a module that isn't loaded holds the main thread for its whole timeout

**What happens.** We have a Rust core module (`concurrency: single`) that calls `blockchain_module`
and `program_registry`. When `blockchain_module` isn't loaded:

- Every call to it from a names method waits 15 s on the module's main thread. The host logs
  `RemoteTransportConnection: no listener at "local:logos_blockchain_module_…" -- request for
  "blockchain_module" will block up to 15000 ms and then fail. Is the module loaded?`, then
  `Timeout waiting for replica`. The module answers nothing else while it waits.
- If the module's host starts before `blockchain_module` (Basecamp restarted with the app's tab
  open), the host stops answering for good. It ignores SIGTERM and needs `kill -9`. A `sample`
  showed four threads in `std thread start → logos_rust_sdk::plugin::PluginProxy::new →
  lp_client_create → logos::runOnQtMainThread → QMetaObject::invokeMethodImpl (blocking queued)
  → QSemaphore::acquire`. That is consistent with the threads the module spawned to ask
  `program_registry` and `package_manager` without blocking its own calls. In a saved sample of
  one stuck host, the main thread was not in a module call at all. It was in a blocking HTTPS
  read our own code made in a names method (since fixed in the module). Anything that keeps the
  main thread busy strands those workers.

**Why.**

1. `PluginProxy::new` makes the client at once (`shared_client` → `lp_client_create`). With a
   Qt-affine transport, that is `runOnQtMainThread`, a `BlockingQueuedConnection`. From a worker
   thread it waits for the Qt main thread with **no timeout**. The call's own timeout only starts
   after the client exists. The client cache holds `Weak` handles, so a proxy made for one call
   makes a new client each time, and so another hop to the main thread. A worker can't bound its
   wait, so "do it on a thread so the dispatch thread doesn't block" turns into a thread that
   never returns once the main thread stops serving queued calls.
2. `lp_invoke` from a worker is marshalled the same way (`LogosAPIClient::invokeRemoteMethod` →
   `runOnOwnerThread`, blocking queued). The wait for the module then runs on the main thread, so
   a worker's call to an unloaded module holds the main thread for the full budget anyway.
3. There is no way to ask "is module X loaded?" without making a client and waiting.
   `RemoteTransportConnection::requestObject` at 8bbc027 already knows nothing is listening
   (`endpointHasListener`). It logs that, then waits the whole budget anyway. Current
   logos-protocol main bounds that wait with `kNoListenerGraceMs`. logos-module-builder de169fd,
   which Basecamp 0.3.2's modules are built with, still pins 8bbc027.

**What we did in the module** (names, branch `names-hang-fix`). Every outbound call goes through
one gate:
- Only the main thread may call. A call from any other thread is refused before the SDK.
- An unloaded module is found with a plain `connect()` to `$TMPDIR/logos_<module>_<instance>`,
  the same probe `endpointHasListener` does, without Qt and without a client.
- Reads whose answer is only kept for later use `call_json_async_with_timeout` from the main
  thread, not worker threads.
- A module that gave no answer is not asked again for 30 s.
- A SIGTERM backstop ends the host 10 s after SIGTERM if it hasn't ended itself.

This works, but every module author would have to know all of it.

**Asks.**
- logos-protocol: expose a non-blocking presence check, e.g. `int lp_target_reachable(const
  char* target_module)`, the `endpointHasListener` probe with no client and no wait. Also return
  `LP_ERR_UNAVAILABLE` at once from `lp_invoke` when nothing listens. (Main's grace period
  helps, but it is still a wait on the dispatch thread.)
- logos-rust-sdk: make the client lazily on the first call, not in `PluginProxy::new`. Bound
  the main-thread hop (`runOnQtMainThread` / `runOnOwnerThread`) by the call's timeout, or
  refuse a synchronous call from a non-main thread with an error, instead of a semaphore wait
  with no bound. Document that in a Qt host every call runs on the main thread, so a worker
  thread doesn't make a call non-blocking: `call_json_async*` from the main thread does. Expose
  the presence check as `PluginProxy::is_reachable()`.
- logos-module-builder: move the logos-protocol pin past the no-listener grace fix, so modules
  built for Basecamp 0.3.x get it.
- Host (logos_host): if its SIGTERM handling runs through the Qt event loop, add a timed fallback
  (`_exit` after the unload window), so a host whose main thread is stuck still exits.
