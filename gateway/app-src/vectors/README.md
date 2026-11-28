# Checking the JavaScript encoder

`lez-tx.js` and `lns-tx.js` are hand-written copies of Rust's borsh layouts, so they are
checked against the real thing here, not by reading them.

- `src/main.rs` builds on the real LEZ `v0.4.0` types, `names_core` v8, `lez_nft_core` v3 and
  both real guests. It is its own workspace and is in no build or deployment. Like
  `tools/engine-test`, it needs lez_nft (branch `v3-lez04`) checked out next to this repo:
  `<parent>/<this repo>` and `<parent>/lez-nft`.
- `vectors.json` is bytes the Rust types made: every v8 instruction, the v8 derivations
  (tiered slot ids, subname, market and site PDAs, NFT token and lez_nft config addresses,
  register/claim generations), the 2.5% split and the re-release premium curve, and v8 states
  (record, settings, subname, market and site buckets). `gateway/public/app/lez-tx.test.mjs`
  and `resolver/names.test.js` reproduce them; no expected hex is written by hand.
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
  `resolver/names.test.js` reads them back. `run` deploys both guests, has the NFT admin
  reserve the LNS collection and the registry configure itself (fee mode), then runs the
  browser's commit, paid register (NFT minted), site listing, market list, buy (2.5% split),
  transfer, release (NFT burned), claim, CommitPublic and register.js's own flow.
- `shard-proofs.mjs` proves names from the 256 shards the way the app does;
  `lns_js_vectors check-proofs shard-proofs.json` verifies those proofs with names_core's
  verifier against `SHORTLIST_ROOT` and checks every name the app refused is locked.

It needs a RISC Zero toolchain and r0vm 3.1 (last run 2026-11-28 on a Colab Linux runtime
with rzup rust 1.97.0, cpp 2024.1.5, r0vm 3.1.0):

```sh
cargo build --release               # needs the repo around it: core/, methods/, shortlist/
lns_js_vectors gen > vectors.json
node engine-run.mjs scenario.json
lns_js_vectors run scenario.json engine-report.json
node shard-proofs.mjs ../../../shortlist/shortlist.bin shard-proofs.json jefdiesel alice-lns satoshi-nakamoto hello jef logos ab x google
lns_js_vectors check-proofs shard-proofs.json
```

This runs the state machine with no sequencer, so the fee market, the mempool and the
RPC server aren't exercised. For those, `../../public/app/lez-tx-send.mjs` sends a native
transfer to any sequencer. It was last run against a local `v0.4.0-rc1`
sequencer; on `v0.4.0` it is untested against a live sequencer.
