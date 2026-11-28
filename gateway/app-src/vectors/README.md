# Checking the JavaScript encoder

`lez-tx.js` and `lns-tx.js` are hand-written copies of Rust's borsh layouts, so they are
checked against the real thing here, not by reading them.

- `src/main.rs` builds on the real LEZ `v0.4.0` types, `names_core` and the real guest.
  It is its own workspace and is in no build or deployment.
- `vectors.json` is bytes the Rust types made. `gateway/public/app/lez-tx.test.mjs`
  reproduces them.
  Signatures use fresh randomness, so for signed transactions the test checks everything
  else byte for byte (messages, next keys, keyed hashes, auth data layout) and verifies
  Rust's signatures with the JavaScript verifier.
- `engine-run.mjs` makes transactions with the browser's own code. `lns_js_vectors run`
  decodes them strictly, checks hashes and the stateless rules (signers, authority, auth
  data), and runs them through the LEZ state machine and the guest ELF at each
  transaction's declared gas limit, as the sequencer does. The engine authenticates every
  signature against the key the account holds, which is the next key the previous
  transaction committed to. Expected refusals have to be refused for the expected reason;
  a refused transaction is not applied, so it does not use up a nonce.
- `engine-report.json` is that run's outcome, cycles and RPC-shaped account views.
  `resolver/names.test.js` reads them back.

It needs a RISC Zero toolchain (rzup rust 1.94.1, cpp 2024.1.5; it was last run on a
Colab Linux runtime):

```sh
cargo build --release               # needs the repo around it: core/, methods/, shortlist/
lns_js_vectors gen > vectors.json
node engine-run.mjs scenario.json
lns_js_vectors run scenario.json engine-report.json
```

This runs the state machine with no sequencer, so the fee market, the mempool and the
RPC server aren't exercised. For those, `../../public/app/lez-tx-send.mjs` sends a native
transfer to any sequencer. It was last run against a local `v0.4.0-rc1`
sequencer; on `v0.4.0` it is untested against a live sequencer.
