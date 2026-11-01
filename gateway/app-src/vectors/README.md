# Checking the JavaScript encoder

`lez-tx.js` and `lns-tx.js` are hand-written copies of Rust's borsh layouts, so they are
checked against the real thing here, not by reading them.

- `src/main.rs` builds on the real LEZ `v0.4.0-rc1` types, `names_core` and the real guest.
  It is its own workspace and is in no build or deployment.
- `vectors.json` is bytes the Rust types made. `gateway/public/app/lez-tx.test.mjs`
  reproduces them.
- `engine-run.mjs` makes transactions with the browser's own code. `lns_js_vectors run`
  decodes them strictly, checks hashes and signatures, and runs them through the LEZ state
  machine and the guest ELF. Expected refusals have to be refused for the expected reason.
- `engine-report.json` is that run's outcome, cycles and RPC-shaped account views.
  `resolver/names.test.js` reads them back.

It needs a RISC Zero toolchain, so build it on the Mini:

```sh
cargo build --release --offline     # needs the repo around it: core/, methods/, shortlist/
lns_js_vectors gen > vectors.json
node engine-run.mjs scenario.json
lns_js_vectors run scenario.json engine-report.json
```

This runs the state machine with no sequencer, so the fee market, the mempool and the
RPC server aren't exercised. For those, `../../public/app/lez-tx-send.mjs` sends a native
transfer to any sequencer. It was run against a local `v0.4.0-rc1` sequencer
(`sequencer_service` with the `standalone` feature and `networks/local.json`, with the
key's public key added to the genesis accounts).
