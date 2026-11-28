#!/usr/bin/env bash
# =============================================================================
# Test suite for the LNS v8 + lez_nft v3 deploy kit (`deploy.sh --dry-run` &
# `post-deploy.sh --dry-run`) against a local standalone sequencer config.
# =============================================================================
#
# Runs in two modes:
#   1. Full local-sequencer mode (when SEQ_BIN and SEQ_CONFIG are set — e.g.
#      LEZ v0.4 `sequencer_service` + `networks/local.json`):
#      - Boots a fresh throwaway standalone sequencer on 127.0.0.1:$RPC_PORT
#      - Imports LEZ's published local-test genesis account into a throwaway wallet
#      - Runs `deploy.sh --dry-run` against the live local sequencer
#      - Verifies exit 0, expected output, AND that payer nonce & balance are
#        100% unchanged (proving zero transactions were sent)
#      - Runs `post-deploy.sh --dry-run` against the same local sequencer
#   2. Standalone offline mode (when SEQ_BIN is not set):
#      - Validates a provided (or synthetic) standalone `local.json` config
#      - Runs all unit self-tests, negative safety-latch tests, and full
#        `deploy.sh --dry-run` + `post-deploy.sh --dry-run` with temporary
#        fixture ELFs and a stub `imageid_cmd`
#
# Inputs (optional environment):
#   SEQ_BIN       Path to LEZ `sequencer_service` built with `--features standalone`
#   SEQ_CONFIG    Path to LEZ `networks/local.json`
#   NAMES_CLI     Path to `names` CLI binary (used with SEQ_BIN to import local-test account)
#   RPC_PORT      Default 3239 (127.0.0.1 only)
#   METRICS_PORT  Default 9239 (127.0.0.1 only)
#
# Nothing here is secret: the local-test key below is LEZ's published
# `PRIVATE_KEY_PUB_ACC_A` from `lez/testnet_initial_state` (local standalone only).
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
LIB="$SCRIPT_DIR/lib.py"
DEPLOY_SH="$SCRIPT_DIR/deploy.sh"
POST_DEPLOY_SH="$SCRIPT_DIR/post-deploy.sh"

RPC_PORT="${RPC_PORT:-3239}"
METRICS_PORT="${METRICS_PORT:-9239}"
LOCAL_RPC="http://127.0.0.1:${RPC_PORT}"

# LEZ's published local-test key (PRIVATE_KEY_PUB_ACC_A in lez/testnet_initial_state):
# throwaway local standalone chains ONLY. Never use on any public network.
LOCAL_TEST_KEY=$("$(git -C "$(dirname "$0")" rev-parse --show-toplevel)/tools/lez-test-key.sh" PRIVATE_KEY_PUB_ACC_A)

say()  { printf '\n=== %s ===\n' "$*"; }
pass() { printf 'PASS: %s\n' "$*"; }
fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }

WORK="$(mktemp -d "${TMPDIR:-/tmp}/lns-deploy-dryrun.XXXXXX")"
SEQ_PID=""
cleanup() {
  if [ -n "$SEQ_PID" ]; then
    kill "$SEQ_PID" 2>/dev/null || true
    wait "$SEQ_PID" 2>/dev/null || true
  fi
  rm -rf "$WORK"
}
trap cleanup EXIT

# =============================================================================
# Test 1 — Python crypto / PDA / Borsh / SPEC.md §10 vectors self-test
# =============================================================================
say "Test 1: deploy/lib.py --self-test"
python3 "$LIB" --self-test
pass "lib.py self-test"

# =============================================================================
# Build temporary fixture artifacts (dummy ELFs, stub imageid_cmd, expected IDs,
# synthetic shortlist if repo/shortlist/shortlist.bin is not present)
# =============================================================================
say "Preparing temporary dry-run fixtures in $WORK"

REG_ELF="$WORK/logos_names.bin"
NFT_ELF="$WORK/lez_nft_guest.bin"
dd if=/dev/zero bs=1024 count=200 of="$REG_ELF" 2>/dev/null
printf 'LNS_V8_DRY_RUN_FIXTURE' >> "$REG_ELF"
dd if=/dev/zero bs=1024 count=120 of="$NFT_ELF" 2>/dev/null
printf 'LEZ_NFT_V3_DRY_RUN_FIXTURE' >> "$NFT_ELF"

STUB_IMAGEID="$WORK/stub-imageid.sh"
cat > "$STUB_IMAGEID" <<'EOF'
#!/usr/bin/env bash
# Deterministic 64-hex digest stub for dry-run testing only
python3 -c 'import hashlib, sys; print(hashlib.sha256(open(sys.argv[1], "rb").read()).hexdigest())' "$1"
EOF
chmod +x "$STUB_IMAGEID"

REG_ID_HEX="$("$STUB_IMAGEID" "$REG_ELF")"
NFT_ID_HEX="$("$STUB_IMAGEID" "$NFT_ELF")"

EXPECTED_IDS="$WORK/expected-image-ids.txt"
cat > "$EXPECTED_IDS" <<EOF
registry_image_id = $REG_ID_HEX
nft_image_id = $NFT_ID_HEX
registry_elf_sha256 = $REG_ID_HEX
nft_elf_sha256 = $NFT_ID_HEX
EOF

SHORTLIST_BIN="$REPO_DIR/shortlist/shortlist.bin"
if [ ! -f "$SHORTLIST_BIN" ]; then
  SHORTLIST_BIN="$WORK/shortlist.bin"
  python3 -c 'import sys
with open(sys.argv[1], "wb") as f:
    f.truncate(1845228 * 33)' "$SHORTLIST_BIN"
fi

# Deterministic valid 32-byte Base58 test accounts (public IDs only)
read -r ACCT_DEPLOYER ACCT_NFT_ADMIN ACCT_ADMIN ACCT_TREASURY ACCT_JEF ACCT_LOGOS ACCT_WEB ACCT_ANDROID ACCT_SMOKE_PUB ACCT_SMOKE_PRIV <<EOF
$(python3 -c 'import sys
sys.path.insert(0, sys.argv[1])
import lib
ids = [lib.base58_encode(bytes([i]) * 32) for i in range(11, 21)]
print(" ".join(ids))' "$SCRIPT_DIR")
EOF

# Optional standalone local.json config (use SEQ_CONFIG if provided, else write local standalone fixture)
LOCAL_SEQ_CFG="${SEQ_CONFIG:-$WORK/local.json}"
if [ -z "${SEQ_CONFIG:-}" ]; then
  cat > "$LOCAL_SEQ_CFG" <<EOF
{
  "network": "local-standalone",
  "rpc_listen_address": "127.0.0.1:${RPC_PORT}",
  "metrics_listen_address": "127.0.0.1:${METRICS_PORT}"
}
EOF
fi
python3 -c 'import json, sys; d = json.load(open(sys.argv[1])); assert isinstance(d, dict)' "$LOCAL_SEQ_CFG"
pass "standalone sequencer config parsed ($LOCAL_SEQ_CFG)"

write_params() {
  local out_file="$1" confirmed="$2" rpc_url="$3" shortlist_root="$4" extra_line="${5:-}"
  cat > "$out_file" <<EOF
confirmed_by_jef = $confirmed
$extra_line

[network]
name = "local-standalone-dry-run"
rpc_url = "$rpc_url"
expected_channel_id = ""

[artifacts]
registry_elf = "$REG_ELF"
nft_elf = "$NFT_ELF"
expected_image_ids_file = "$EXPECTED_IDS"
shortlist_file = "$SHORTLIST_BIN"

[tooling]
names_cli = "${NAMES_CLI:-$REPO_DIR/target/release/names}"
nft_cli = "$REPO_DIR/../lez-nft/target/release/lez-nft"
imageid_cmd = "$STUB_IMAGEID"

[accounts]
deployer_payer = "Public/$ACCT_DEPLOYER"
nft_admin = "Public/$ACCT_NFT_ADMIN"
admin = "Public/$ACCT_ADMIN"
treasury = "Public/$ACCT_TREASURY"
registry_gas_fund = 10_000_000
nft_header_gas_fund = 10_000_000
treasury_init_fund = 1_000_000

[immutable]
record_version = 8
shortlist_root = "$shortlist_root"
shortlist_depth = 21
shortlist_leaves = 1845228
sale_fee_bps = 250
min_sale_price = 40
grace_days = 30
premium_days = 21
public_wait_minutes = 5
private_wait_minutes = 15
commit_max_age_hours = 24
term_days = [30, 365, 1825]
nft_program_binding_is_permanent = true

[registry]
mode = "fee"
deposits = [0, 0]
premium_start = 4_000_000_000

[registry.prices]
tier_5      = [2_000_000, 20_000_000, 80_000_000, 400_000_000]
tier_6_7    = [  500_000,  5_000_000, 20_000_000, 100_000_000]
tier_8_plus = [  100_000,  1_000_000,  4_000_000,  20_000_000]

[registry.claims]
enabled = false
claims_root = "0000000000000000000000000000000000000000000000000000000000000000"
claims_until = 0
claims_program = "11111111111111111111111111111111"

[[held_names]]
name = "jef"
owner = "Private/$ACCT_JEF"
term = "permanent"
note = "Jef"

[[held_names]]
name = "logos"
owner = "Private/$ACCT_LOGOS"
term = "permanent"
note = "Logos"

[[held_names]]
name = "web"
owner = "Public/$ACCT_WEB"
term = "permanent"
note = "Web"

[[held_names]]
name = "android"
owner = "Public/$ACCT_ANDROID"
term = "permanent"
note = "Android"

[post_deploy]
smoke_public_name = "lns-smoke-pub-v04"
smoke_private_name = "lns-smoke-priv-v04"
smoke_term = "30d"
smoke_wallet_dir = ""
smoke_public_owner = "Public/$ACCT_SMOKE_PUB"
smoke_private_owner = "Private/$ACCT_SMOKE_PRIV"
nft_check_name = ""
EOF
}

GOOD_ROOT="9778bed49005fe7401d25685a27d9cf3e681fa0582b562336cdb8ee9661d990c"
BAD_ROOT="0000000000000000000000000000000000000000000000000000000000000001"

# =============================================================================
# Test 2 — Negative safety-latch guards
# =============================================================================
say "Test 2: Negative safety guards (confirmed_by_jef, shortlist_root, secrets, image IDs)"

P_UNCONFIRMED="$WORK/params-unconfirmed.toml"
write_params "$P_UNCONFIRMED" "false" "$LOCAL_RPC" "$GOOD_ROOT"
if bash "$DEPLOY_SH" --params "$P_UNCONFIRMED" --state "$WORK/state-bad1.json" >/dev/null 2>&1; then
  fail "deploy.sh should have REFUSED live run when confirmed_by_jef = false"
fi
pass "live deploy refused when confirmed_by_jef = false"

P_BADROOT="$WORK/params-badroot.toml"
write_params "$P_BADROOT" "true" "" "$BAD_ROOT"
if bash "$DEPLOY_SH" --params "$P_BADROOT" --dry-run >/dev/null 2>&1; then
  fail "deploy.sh should have REFUSED mismatched shortlist_root"
fi
pass "deploy.sh refused mismatched shortlist_root"

P_SECRET="$WORK/params-secret.toml"
write_params "$P_SECRET" "true" "" "$GOOD_ROOT" 'private_key = "deadbeef"'
if bash "$DEPLOY_SH" --params "$P_SECRET" --dry-run >/dev/null 2>&1; then
  fail "deploy.sh should have REFUSED params.toml containing private_key"
fi
pass "deploy.sh refused params.toml containing secret field"

P_BADIMG="$WORK/params-badimg.toml"
write_params "$P_BADIMG" "false" "" "$GOOD_ROOT"
BAD_IDS="$WORK/expected-bad.txt"
cat > "$BAD_IDS" <<EOF
registry_image_id = ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
nft_image_id = $NFT_ID_HEX
EOF
if EXPECTED_IMAGE_IDS="$BAD_IDS" bash "$DEPLOY_SH" --params "$P_BADIMG" --dry-run >/dev/null 2>&1; then
  fail "deploy.sh should have REFUSED mismatched registry_image_id"
fi
pass "deploy.sh refused mismatched image_id"

# =============================================================================
# Test 3 — Optional live local standalone sequencer boot (when SEQ_BIN is set)
# =============================================================================
TARGET_RPC=""
PAYER_BEFORE_BAL=""
PAYER_BEFORE_NONCE=""

if [ -n "${SEQ_BIN:-}" ]; then
  say "Test 3: Booting local standalone sequencer ($SEQ_BIN + $LOCAL_SEQ_CFG)"
  [ -x "$SEQ_BIN" ] || fail "SEQ_BIN not executable: $SEQ_BIN"
  [ -n "${SEQ_CONFIG:-}" ] || fail "SEQ_CONFIG (networks/local.json) must be set when SEQ_BIN is set"

  "$SEQ_BIN" setup "$WORK/seq" --config "$LOCAL_SEQ_CFG" \
    --funding-public-key 0101010101010101010101010101010101010101010101010101010101010100 --create-channel \
    > "$WORK/seq-setup.log" 2>&1 || { cat "$WORK/seq-setup.log" >&2; fail "sequencer setup failed"; }

  RUST_LOG=warn "$SEQ_BIN" start "$WORK/seq" \
    --rpc-listen-address "127.0.0.1:${RPC_PORT}" \
    --metrics-listen-address "127.0.0.1:${METRICS_PORT}" \
    >> "$WORK/sequencer.log" 2>&1 < /dev/null &
  SEQ_PID=$!

  for _ in $(seq 1 40); do
    if python3 "$LIB" rpc-probe "$LOCAL_RPC" >/dev/null 2>&1; then break; fi
    sleep 1
  done
  python3 "$LIB" rpc-probe "$LOCAL_RPC" || { tail -40 "$WORK/sequencer.log" >&2; fail "local sequencer did not answer"; }
  TARGET_RPC="$LOCAL_RPC"

  # If NAMES_CLI is available, import LEZ's published local-test account into a throwaway wallet
  if [ -n "${NAMES_CLI:-}" ] && [ -x "$NAMES_CLI" ]; then
    WALLET_DIR="$WORK/wallet"
    mkdir -p "$WALLET_DIR"
    cat > "$WALLET_DIR/wallet_config.json" <<EOF
{
  "gas_limit": 2000000,
  "private_fee_headroom_blocks": 5,
  "seq_block_poll_max_amount": 100,
  "seq_poll_max_retries": 5,
  "seq_poll_timeout": "12s",
  "seq_tx_poll_max_blocks": 5,
  "sequencer": { "sequencer_addr": "$LOCAL_RPC/" }
}
EOF
    export LEE_WALLET_HOME_DIR="$WALLET_DIR"
    export LEE_WALLET_PASSWORD=""
    IMPORTED="$( "$NAMES_CLI" wallet account import public --private-key "$LOCAL_TEST_KEY" 2>&1 | grep -oE 'Public/[1-9A-HJ-NP-Za-km-z]{32,44}' | head -1 | cut -d/ -f2 || true )"
    if [ -n "$IMPORTED" ]; then
      ACCT_DEPLOYER="$IMPORTED"
      BAL_JSON="$(python3 "$LIB" balance "$TARGET_RPC" "$ACCT_DEPLOYER")"
      PAYER_BEFORE_BAL="$(python3 -c 'import json, sys; print(json.loads(sys.argv[1])["balance"])' "$BAL_JSON")"
      PAYER_BEFORE_NONCE="$(python3 -c 'import json, sys; print(json.loads(sys.argv[1])["nonce"])' "$BAL_JSON")"
      pass "local sequencer live with funded local-test deployer $ACCT_DEPLOYER (bal=$PAYER_BEFORE_BAL, nonce=$PAYER_BEFORE_NONCE)"
    fi
  fi
else
  say "Test 3: SEQ_BIN not set — running offline --dry-run (set SEQ_BIN + SEQ_CONFIG for live standalone sequencer test)"
fi

# =============================================================================
# Test 4 — Full deploy.sh --dry-run
# =============================================================================
say "Test 4: Running deploy.sh --dry-run"

P_DRY="$WORK/params-dry.toml"
write_params "$P_DRY" "false" "$TARGET_RPC" "$GOOD_ROOT"

DRY_OUT="$WORK/deploy-dry.out"
bash "$DEPLOY_SH" --params "$P_DRY" --state "$WORK/state-dry.json" --dry-run | tee "$DRY_OUT"

grep -q "DRY RUN COMPLETE — 0 transactions sent" "$DRY_OUT" || fail "missing dry-run completion banner"
grep -q "borsh message:     411 bytes" "$DRY_OUT" || fail "expected 411-byte Configure Borsh payload"
grep -q "$GOOD_ROOT" "$DRY_OUT" || fail "expected shortlist_root in dry-run output"
[ ! -f "$WORK/state-dry.json" ] || fail "dry-run must NOT create a state file"

if [ -n "$PAYER_BEFORE_BAL" ]; then
  AFTER_JSON="$(python3 "$LIB" balance "$TARGET_RPC" "$ACCT_DEPLOYER")"
  PAYER_AFTER_BAL="$(python3 -c 'import json, sys; print(json.loads(sys.argv[1])["balance"])' "$AFTER_JSON")"
  PAYER_AFTER_NONCE="$(python3 -c 'import json, sys; print(json.loads(sys.argv[1])["nonce"])' "$AFTER_JSON")"
  [ "$PAYER_AFTER_BAL" = "$PAYER_BEFORE_BAL" ] || fail "deployer balance changed during --dry-run!"
  [ "$PAYER_AFTER_NONCE" = "$PAYER_BEFORE_NONCE" ] || fail "deployer nonce changed during --dry-run!"
  pass "confirmed 0 transactions sent on live local sequencer (balance & nonce unchanged)"
fi
pass "deploy.sh --dry-run"

# =============================================================================
# Test 5 — Full post-deploy.sh --dry-run
# =============================================================================
say "Test 5: Running post-deploy.sh --dry-run"

POST_OUT="$WORK/post-deploy-dry.out"
bash "$POST_DEPLOY_SH" --params "$P_DRY" --dry-run | tee "$POST_OUT"

for expected_token in "name='jef'" "name='logos'" "name='web'" "name='android'" "lns-smoke-pub-v04" "lns-smoke-priv-v04" "LOGOS_BOARD_REGISTRY" "LNS_REGISTRY_PROGRAM_ID" "LNS_NFT_PROGRAM_ID"; do
  grep -q "$expected_token" "$POST_OUT" || fail "post-deploy.sh --dry-run missing expected output: $expected_token"
done
pass "post-deploy.sh --dry-run"

say "ALL DRY-RUN TESTS PASSED"
