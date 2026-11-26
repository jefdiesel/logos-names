#!/usr/bin/env bash
# Names Wallet demo (λPrize LP-0021): run it from a clean clone, with no changes and no keys.
#
#   ./demo.sh
#
# What it does, in order:
#   1. Checks the tools it needs (git, curl, Rust, Node 18+; on Linux also pkg-config and the
#      PC/SC library for Keycard support) and installs the RISC Zero toolchain with rzup if it
#      is missing.
#   2. Wallet Provider SDK and the two reference mini-apps (faucet, testimonial): runs their
#      tests against a stand-in names module, so the connect, account choice, approval and
#      request flows are exercised without a GUI. Seconds.
#   3. Builds a LEZ sequencer at the version this repository targets (the same tag and commit as
#      CI) in standalone mode. About 40 minutes the first time; kept in .demo/ for next time.
#   4. Builds the names CLI, the Names registry guest, the testimonial program and client, and
#      the prototype token CLI.
#   5. Starts a fresh, throwaway LEZ chain on 127.0.0.1 and runs scripts/wallet-e2e.sh on it:
#      registry deploy and a name registered, payments, tokens (create, mint, send, overspend
#      refused), a testimonial posted and listed (a second one refused), accounts and fees, a
#      network drop and restart. The registry's 15-minute commit wait is part of it.
#   6. If nix is installed, also runs the Basecamp module's Rust tests (the wallet side of the
#      SDK: providers, approvals, grants). Skipped without nix; CI always runs them.
#
# Nothing here is secret and nothing touches a public network: the chain is local and new, and
# its payer is LEZ's published local-test genesis account. The GUI (Basecamp) part is shown in
# the narrated video; this script covers everything that runs without a screen.
#
# Environment (all optional):
#   DEMO_DIR    where builds and the chain go (default: .demo/ in this clone)
#   DEMO_QUICK  1 = only steps 1 and 2 (no chain, no long builds)
#   RPC_PORT, METRICS_PORT  ports for the local chain (default 3230, 9230; 127.0.0.1 only)
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
cd "$ROOT"
DEMO_DIR=${DEMO_DIR:-$ROOT/.demo}
mkdir -p "$DEMO_DIR"
START=$(date +%s)

say()  { printf '\n\033[1m== %s\033[0m\n' "$*"; }
ok()   { printf '   ok: %s\n' "$*"; }
fail() { printf '\nDEMO FAILED: %s\n' "$*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || fail "$1 is not installed. $2"; }
elapsed() { local s=$(( $(date +%s) - START )); printf '%dm%02ds' $((s / 60)) $((s % 60)); }

# One source of truth for the LEZ version: the CI workflow.
WF=.github/workflows/wallet-e2e.yml
LEZ_TAG=$(sed -n 's/^ *LEZ_TAG: *"\{0,1\}\([^" #]*\)"\{0,1\}.*/\1/p' "$WF" | head -1)
LEZ_COMMIT=$(sed -n 's/^ *LEZ_COMMIT: *"\{0,1\}\([0-9a-f]*\)"\{0,1\}.*/\1/p' "$WF" | head -1)
[ -n "$LEZ_TAG" ] && [ -n "$LEZ_COMMIT" ] || fail "could not read LEZ_TAG / LEZ_COMMIT from $WF"

# ---------------------------------------------------------------------------------------------
say "1/6 Tools"
need git  "Install git."
need curl "Install curl."
need cargo "Install Rust from https://rustup.rs (then open a new shell)."
need node "Install Node.js 18 or newer from https://nodejs.org."
node_major=$(node -p 'process.versions.node.split(".")[0]')
[ "$node_major" -ge 18 ] || fail "Node.js $node_major is too old; 18 or newer is needed."
if [ "$(uname -s)" = Linux ]; then
  need pkg-config "On Debian/Ubuntu: sudo apt-get install -y pkg-config libpcsclite-dev"
  pkg-config --exists libpcsclite || fail "the PC/SC library is missing. On Debian/Ubuntu: sudo apt-get install -y libpcsclite-dev"
fi
ok "git, curl, cargo $(cargo --version | awk '{print $2}'), node $(node --version)"

if [ "${DEMO_QUICK:-0}" != 1 ]; then
  export PATH="$HOME/.risc0/bin:$HOME/.cargo/bin:$PATH"
  if ! cargo risczero --version >/dev/null 2>&1; then
    echo "   The RISC Zero toolchain is missing; installing it with rzup into ~/.risc0 ..."
    command -v rzup >/dev/null 2>&1 || curl -sSfL https://risczero.com/install | bash
    rzup install cargo-risczero 3.0.5
    rzup install rust
  fi
  # The local sequencer runs programs with r0vm, which rzup installs next to cargo-risczero.
  command -v r0vm >/dev/null 2>&1 || ls "$HOME"/.risc0/extensions/*/r0vm >/dev/null 2>&1 \
    || fail "RISC Zero's r0vm is missing (the sequencer needs it). Install it with: rzup install r0vm"
  ok "$(cargo risczero --version | head -1), r0vm present"
fi

# ---------------------------------------------------------------------------------------------
say "2/6 Wallet Provider SDK and the reference mini-apps (no GUI)"
node --test sdk/NamesWalletSDK.test.mjs >"$DEMO_DIR/sdk-test.log" 2>&1 \
  || { tail -30 "$DEMO_DIR/sdk-test.log"; fail "SDK tests (log: $DEMO_DIR/sdk-test.log)"; }
ok "SDK: $(grep -E '^(#|ℹ) pass' "$DEMO_DIR/sdk-test.log" | awk '{print $3}') tests pass (connect, accounts, approvals, requests)"
for t in apps/faucet/logic.test.mjs apps/testimonial/logic.test.mjs basecamp/logic.test.mjs; do
  log="$DEMO_DIR/$(echo "$t" | tr '/' '_').log"
  node "$t" >"$log" 2>&1 || { tail -30 "$log"; fail "$t (log: $log)"; }
  ok "$t"
done

if [ "${DEMO_QUICK:-0}" = 1 ]; then
  say "DEMO_QUICK=1: stopping after the SDK and mini-app checks ($(elapsed))"
  exit 0
fi

# ---------------------------------------------------------------------------------------------
say "3/6 LEZ sequencer $LEZ_TAG ($LEZ_COMMIT), standalone mode"
SEQ_DIR="$DEMO_DIR/lez-seq-$LEZ_COMMIT"
if [ -x "$SEQ_DIR/sequencer_service" ] && [ -f "$SEQ_DIR/local.json" ]; then
  ok "already built in $SEQ_DIR"
else
  src="$DEMO_DIR/lez-src"
  rm -rf "$src"
  git clone --quiet --depth 1 --branch "$LEZ_TAG" \
    https://github.com/logos-blockchain/logos-execution-zone.git "$src"
  got=$(git -C "$src" rev-parse --short=8 HEAD)
  [ "$got" = "$LEZ_COMMIT" ] || fail "LEZ tag $LEZ_TAG is commit $got, expected $LEZ_COMMIT"
  echo "   building (about 40 minutes the first time) ..."
  (cd "$src" && cargo build --release --features standalone -p sequencer_service) \
    >"$DEMO_DIR/lez-build.log" 2>&1 || { tail -40 "$DEMO_DIR/lez-build.log"; fail "LEZ build (log: $DEMO_DIR/lez-build.log)"; }
  mkdir -p "$SEQ_DIR"
  cp "$src/target/release/sequencer_service" "$SEQ_DIR/"
  cp "$src/lez/sequencer/service/networks/local.json" "$SEQ_DIR/"
  rm -rf "$src"
  ok "built into $SEQ_DIR"
fi

# ---------------------------------------------------------------------------------------------
say "4/6 Names Wallet: CLI, registry guest, testimonial program and client, token CLI"
BIN="$DEMO_DIR/bin"
mkdir -p "$BIN"
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$ROOT/target}
{
  cargo build --release -p names_cli -p names_methods -p testimonial_client -p testimonial_engine_test
  cargo run --release -p testimonial_engine_test --bin testimonial-imageid -- "$BIN/testimonial.bin"
  cargo build --release --manifest-path wallet-tokens/Cargo.toml
} >"$DEMO_DIR/names-build.log" 2>&1 || { tail -40 "$DEMO_DIR/names-build.log"; fail "Names build (log: $DEMO_DIR/names-build.log)"; }
elf=$(find "$CARGO_TARGET_DIR/riscv-guest" -path '*names_methods*' -name logos_names.bin | head -1)
[ -n "$elf" ] || fail "the registry guest logos_names.bin was not built"
cp "$elf" "$BIN/logos_names.bin"
ok "names CLI $(sed -n 's/^version = "\(.*\)"/\1/p' cli/Cargo.toml | head -1), guests in $BIN"

# ---------------------------------------------------------------------------------------------
say "5/6 The wallet end to end on a fresh local LEZ chain (includes the 15-minute commit wait)"
WORK="$DEMO_DIR/run-$(date +%Y%m%d-%H%M%S)"
export SEQ_BIN="$SEQ_DIR/sequencer_service" SEQ_CONFIG="$SEQ_DIR/local.json"
export NAMES="$CARGO_TARGET_DIR/release/names" NAMES_ELF="$BIN/logos_names.bin"
export WALLET_TOKENS="$CARGO_TARGET_DIR/release/wallet-tokens"
export TESTIMONIAL="$CARGO_TARGET_DIR/release/testimonial" TESTIMONIAL_ELF="$BIN/testimonial.bin"
export WORK
scripts/wallet-e2e.sh 2>&1 | tee "$DEMO_DIR/wallet-e2e.log" \
  || fail "wallet end to end (log: $DEMO_DIR/wallet-e2e.log, chain and wallets: $WORK)"
ok "every step passed (chain and wallets kept in $WORK)"

# ---------------------------------------------------------------------------------------------
say "6/6 Basecamp module tests (the wallet side of the SDK)"
if command -v nix >/dev/null 2>&1; then
  rev=$(sed -n 's|.*logos-module-builder.url = "github:logos-co/logos-module-builder/\([0-9a-f]*\)".*|\1|p' basecamp-module/flake.nix)
  [ -n "$rev" ] || fail "could not read the module builder revision from basecamp-module/flake.nix"
  [ -e basecamp-module/logos-rust-sdk-src ] || \
    nix --extra-experimental-features 'nix-command flakes' build \
      "github:logos-co/logos-module-builder/$rev#rust-sdk-src" -o basecamp-module/logos-rust-sdk-src
  (cd basecamp-module/rust-lib && cargo test --release --locked) >"$DEMO_DIR/module-test.log" 2>&1 \
    || { tail -40 "$DEMO_DIR/module-test.log"; fail "module tests (log: $DEMO_DIR/module-test.log)"; }
  ok "$(grep -E '^test result' "$DEMO_DIR/module-test.log" | awk '{s+=$4} END {print s}') module tests pass"
else
  echo "   skipped: nix is not installed (the tests need the Logos Rust SDK, which nix fetches)."
  echo "   CI runs them on every push: .github/workflows/wallet-e2e.yml, job \"module\"."
fi

say "DEMO PASSED in $(elapsed)"
