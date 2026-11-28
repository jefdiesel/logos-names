#!/usr/bin/env bash
# =============================================================================
# LNS Registry v8 + lez_nft v3 — Official LEZ v0.4 Testnet Deployer
# =============================================================================
#
# Deploys `lez_nft` v3 and `names_core` v8 ONCE to the target LEZ v0.4 network,
# funds the initial accounts, configures `lez_nft` v3 to authorize the registry
# and reserve the LNS collection, sends the initial `Instruction::Configure`
# (permanently binding `Settings.nft_program`), and verifies on-chain Settings.
#
# Safety guarantees:
#   - Every FIXED FOREVER value is read from `deploy/params.toml` and cross-
#     checked against `repo/core/src/` and `repo/shortlist/root.hex`.
#   - Refuses to run a live deploy until `confirmed_by_jef = true` in params.toml.
#   - Refuses to deploy if either ELF image ID does not match expected-image-ids.txt.
#   - Refuses if params.toml contains any private key, mnemonic, or password field.
#   - Never writes keys or passwords to disk; wallet tooling reads keys at run time
#     via `LEE_WALLET_HOME_DIR` / `LEE_WALLET_PASSWORD` (or interactive TTY prompt).
#   - `--dry-run` validates everything, prints every transaction and exact Borsh
#     payload, and sends ZERO transactions.
#   - Resumable: persists deployed public program IDs in `deploy/state/deploy-state.json`
#     after each irreversible step so a mid-run RPC hiccup won't deploy duplicates.
#
# Usage:
#   ./deploy/deploy.sh [--params deploy/params.toml] [--rpc URL] \
#                      [--state deploy/state/deploy-state.json] \
#                      [--dry-run] [--use-cli-configure] [--skip-nft-configure]
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
LIB="$SCRIPT_DIR/lib.py"

PARAMS_FILE="$SCRIPT_DIR/params.toml"
STATE_FILE="$SCRIPT_DIR/state/deploy-state.json"
RPC_OVERRIDE=""
DRY_RUN=0
USE_CLI_CONFIGURE=0
SKIP_NFT_CONFIGURE=0

usage() {
  cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Options:
  --params <path>        Path to params.toml (default: deploy/params.toml)
  --rpc <url>            Sequencer JSON-RPC URL (overrides LEZ_RPC_URL and params.toml)
  --state <path>         Resumable state manifest JSON (default: deploy/state/deploy-state.json)
  --dry-run              Validate, check image IDs, print every tx & Borsh hex; send NOTHING
  --use-cli-configure    Use 'names configure' instead of exact v8 'names module send'
  --skip-nft-configure   Skip lez_nft v3 Configure step (only if configured manually)
  -h, --help             Show this help

Environment overrides (optional, no secrets written to disk):
  LEZ_RPC_URL            Sequencer RPC URL (if --rpc not given)
  LEE_WALLET_HOME_DIR    Wallet directory holding deployer / admin / nft_admin keys
  LEE_WALLET_PASSWORD    Wallet password (omit to prompt interactively)
  NAMES_CLI              Override [tooling].names_cli
  NFT_CLI                Override [tooling].nft_cli
  IMAGEID_CMD            Override [tooling].imageid_cmd
  NFT_CONFIGURE_CMD      Custom command to send lez_nft::Instruction::Configure
  NAMES_ELF              Override [artifacts].registry_elf
  NFT_ELF                Override [artifacts].nft_elf
  EXPECTED_IMAGE_IDS     Override [artifacts].expected_image_ids_file
  NAMES_SHORTLIST        Override [artifacts].shortlist_file
  TREASURY_PUBLIC_KEY    Hex public key for treasury if not held in deployer wallet
  DEPLOY_YES=1           Skip the final interactive 'Type DEPLOY' prompt (non-TTY)
EOF
}

while [ $# -gt 0 ]; do
  case "$1" in
    --params) PARAMS_FILE="$2"; shift 2 ;;
    --rpc) RPC_OVERRIDE="$2"; shift 2 ;;
    --state) STATE_FILE="$2"; shift 2 ;;
    --dry-run) DRY_RUN=1; shift ;;
    --use-cli-configure) USE_CLI_CONFIGURE=1; shift ;;
    --skip-nft-configure) SKIP_NFT_CONFIGURE=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
done

say()  { printf '\n==> %s\n' "$*"; }
info() { printf '    %s\n' "$*"; }
warn() { printf 'WARN: %s\n' "$*" >&2; }
fail() { printf 'ERROR: %s\n' "$*" >&2; exit 1; }

command -v python3 >/dev/null 2>&1 || fail "python3 is required"
[ -f "$LIB" ] || fail "missing helper: $LIB"
[ -f "$PARAMS_FILE" ] || fail "params file not found: $PARAMS_FILE (copy deploy/params.example.toml -> deploy/params.toml)"

PARAMS_ABS="$(cd "$(dirname "$PARAMS_FILE")" && pwd)/$(basename "$PARAMS_FILE")"
PARAMS_DIR="$(dirname "$PARAMS_ABS")"
STATE_DIR="$(dirname "$STATE_FILE")"

resolve_path() {
  local raw="$1"
  if [[ "$raw" = /* ]]; then
    printf '%s\n' "$raw"
  else
    python3 -c 'import os, sys; print(os.path.normpath(os.path.join(sys.argv[1], sys.argv[2])))' "$PARAMS_DIR" "$raw"
  fi
}

json_get() {
  local json_str="$1" expr="$2"
  python3 -c "import json, sys; d = json.loads(sys.argv[1]); v = $expr; print('' if v is None else v)" "$json_str"
}

state_get() {
  local key="$1"
  [ -f "$STATE_FILE" ] || { printf ''; return 0; }
  python3 -c 'import json, sys
d = json.load(open(sys.argv[1]))
v = d.get(sys.argv[2], "")
print("" if v is None else v)' "$STATE_FILE" "$key"
}

state_set() {
  local key="$1" val="$2"
  [ "$DRY_RUN" -eq 1 ] && return 0
  mkdir -p "$STATE_DIR"
  python3 -c 'import json, os, sys
path, key, val = sys.argv[1], sys.argv[2], sys.argv[3]
data = json.load(open(path)) if os.path.exists(path) else {}
data[key] = val
tmp = path + ".tmp"
with open(tmp, "w", encoding="utf-8") as f:
    json.dump(data, f, indent=2, sort_keys=True)
    f.write("\n")
os.chmod(tmp, 0o600)
os.replace(tmp, path)' "$STATE_FILE" "$key" "$val"
}

strip_prefix() {
  local s="$1"
  s="${s#Public/}"
  s="${s#Private/}"
  printf '%s' "$s"
}

id_of() {
  grep -oE "$1/[1-9A-HJ-NP-Za-km-z]{32,44}" | head -1 | cut -d/ -f2
}

compute_image_id() {
  local elf="$1" label="$2" cmd_str="$3" raw_out="" hex=""
  if [ -n "$cmd_str" ]; then
    # Split IMAGEID_CMD safely into array; pass elf as final argument.
    # shellcheck disable=SC2206
    local cmd_arr=($cmd_str)
    raw_out="$("${cmd_arr[@]}" "$elf" 2>&1)" || fail "imageid command failed for $label ($elf): $raw_out"
  else
    fail "cannot compute RISC0 image ID for $label: set [tooling].imageid_cmd in params.toml or export IMAGEID_CMD (command that prints the 64-hex image ID for <elf>)"
  fi
  hex="$(printf '%s\n' "$raw_out" | grep -oE '[0-9a-fA-F]{64}' | tail -1 | tr 'A-F' 'a-f' || true)"
  [ -n "$hex" ] || fail "imageid command produced no 64-hex digest for $label: $raw_out"
  printf '%s' "$hex"
}

# =============================================================================
# Banner
# =============================================================================
if [ "$DRY_RUN" -eq 1 ]; then
  say "LNS v8 + lez_nft v3 DEPLOY — DRY RUN (ZERO TRANSACTIONS WILL BE SENT)"
else
  say "LNS v8 + lez_nft v3 DEPLOY — LIVE DEPLOYMENT"
fi
info "repo:   $REPO_DIR"
info "params: $PARAMS_ABS"
info "state:  $STATE_FILE"

# =============================================================================
# Step 1 — Validate params.toml, confirmed_by_jef gate, and compile-time constants
# =============================================================================
say "Step 1/9: Validating params.toml and compile-time immutables"

VALIDATE_ARGS=("$PARAMS_ABS" "$REPO_DIR")
[ "$DRY_RUN" -eq 1 ] && VALIDATE_ARGS+=("--dry-run")

set +e
VALIDATE_JSON="$(python3 "$LIB" validate "${VALIDATE_ARGS[@]}")"
VALIDATE_RC=$?
set -e

python3 -c 'import json, sys
d = json.loads(sys.argv[1])
for w in d.get("warnings", []):
    print("WARN:", w, file=sys.stderr)
for e in d.get("errors", []):
    print("ERROR:", e, file=sys.stderr)' "$VALIDATE_JSON"

if [ "$VALIDATE_RC" -ne 0 ]; then
  fail "params.toml validation failed (see errors above)"
fi

CONFIRMED="$(json_get "$VALIDATE_JSON" 'd["confirmed_by_jef"]')"
NET_NAME="$(json_get "$VALIDATE_JSON" 'd["network"].get("name", "lez-v0.4-testnet")')"
PARAM_RPC="$(json_get "$VALIDATE_JSON" 'd["network"].get("rpc_url", "")')"
EXPECTED_CHANNEL="$(json_get "$VALIDATE_JSON" 'd["network"].get("expected_channel_id", "")')"

DEPLOYER_PAYER="$(json_get "$VALIDATE_JSON" 'd["accounts"]["deployer_payer"]')"
NFT_ADMIN="$(json_get "$VALIDATE_JSON" 'd["accounts"]["nft_admin"]')"
ADMIN_ID="$(json_get "$VALIDATE_JSON" 'd["accounts"]["admin"]')"
TREASURY_ID="$(json_get "$VALIDATE_JSON" 'd["accounts"]["treasury"]')"

REG_GAS_FUND="$(python3 -c 'import sys; sys.path.insert(0, sys.argv[1]); import lib; d = lib.load_toml(lib.Path(sys.argv[2])); print(d.get("accounts", {}).get("registry_gas_fund", 10_000_000))' "$SCRIPT_DIR" "$PARAMS_ABS")"
NFT_GAS_FUND="$(python3 -c 'import sys; sys.path.insert(0, sys.argv[1]); import lib; d = lib.load_toml(lib.Path(sys.argv[2])); print(d.get("accounts", {}).get("nft_header_gas_fund", 10_000_000))' "$SCRIPT_DIR" "$PARAMS_ABS")"
TREASURY_INIT_FUND="$(python3 -c 'import sys; sys.path.insert(0, sys.argv[1]); import lib; d = lib.load_toml(lib.Path(sys.argv[2])); print(d.get("accounts", {}).get("treasury_init_fund", 1_000_000))' "$SCRIPT_DIR" "$PARAMS_ABS")"

MODE_NAME="$(json_get "$VALIDATE_JSON" 'd["mode_name"]')"
PREMIUM_START="$(json_get "$VALIDATE_JSON" 'd["premium_start"]')"
SHORTLIST_ROOT="$(json_get "$VALIDATE_JSON" 'd["immutable"]["shortlist_root"]')"
SHORTLIST_LEAVES="$(json_get "$VALIDATE_JSON" 'd["immutable"]["shortlist_leaves"]')"
SALE_FEE_BPS="$(json_get "$VALIDATE_JSON" 'd["immutable"]["sale_fee_bps"]')"
GRACE_DAYS="$(json_get "$VALIDATE_JSON" 'd["immutable"]["grace_days"]')"
PREMIUM_DAYS="$(json_get "$VALIDATE_JSON" 'd["immutable"]["premium_days"]')"
PUB_WAIT="$(json_get "$VALIDATE_JSON" 'd["immutable"]["public_wait_minutes"]')"
PRIV_WAIT="$(json_get "$VALIDATE_JSON" 'd["immutable"]["private_wait_minutes"]')"

# Resolve RPC URL: --rpc > LEZ_RPC_URL > [network].rpc_url
RPC_URL="${RPC_OVERRIDE:-${LEZ_RPC_URL:-$PARAM_RPC}}"
RPC_URL="${RPC_URL%/}"

# Resolve paths & binaries
REG_ELF="${NAMES_ELF:-$(resolve_path "$(json_get "$VALIDATE_JSON" 'd["artifacts"].get("registry_elf", "")')")}"
NFT_ELF="${NFT_ELF:-$(resolve_path "$(json_get "$VALIDATE_JSON" 'd["artifacts"].get("nft_elf", "")')")}"
EXPECTED_IDS_FILE="${EXPECTED_IMAGE_IDS:-$(resolve_path "$(json_get "$VALIDATE_JSON" 'd["artifacts"].get("expected_image_ids_file", "expected-image-ids.txt")')")}"
SHORTLIST_FILE="${NAMES_SHORTLIST:-$(resolve_path "$(json_get "$VALIDATE_JSON" 'd["artifacts"].get("shortlist_file", "../shortlist/shortlist.bin")')")}"

NAMES_CLI="${NAMES_CLI:-$(resolve_path "$(json_get "$VALIDATE_JSON" 'd["tooling"].get("names_cli", "../target/release/names")')")}"
NFT_CLI="${NFT_CLI:-$(resolve_path "$(json_get "$VALIDATE_JSON" 'd["tooling"].get("nft_cli", "../../lez-nft/target/release/lez-nft")')")}"
IMAGEID_CMD="${IMAGEID_CMD:-$(json_get "$VALIDATE_JSON" 'd["tooling"].get("imageid_cmd", "")')}"

info "ok: params.toml validated (confirmed_by_jef=$CONFIRMED, network=$NET_NAME)"
info "ok: all compile-time [immutable] constants match repo/core/src/ and repo/shortlist/root.hex"

# =============================================================================
# Step 2 — Build & artifact check (shortlist.bin, ELFs, expected image IDs)
# =============================================================================
say "Step 2/9: Checking guest ELFs, RISC0 image IDs, and shortlist.bin"

[ -f "$REG_ELF" ] || fail "registry ELF not found: $REG_ELF"
[ -f "$NFT_ELF" ] || fail "lez_nft ELF not found: $NFT_ELF"
[ -f "$EXPECTED_IDS_FILE" ] || fail "expected image IDs file not found: $EXPECTED_IDS_FILE (copy expected-image-ids.example.txt -> expected-image-ids.txt)"
[ -f "$SHORTLIST_FILE" ] || fail "shortlist file not found: $SHORTLIST_FILE"

EXPECTED_SHORTLIST_BYTES=$(( SHORTLIST_LEAVES * 33 ))
ACTUAL_SHORTLIST_BYTES="$(wc -c < "$SHORTLIST_FILE" | tr -d ' ')"
if [ "$ACTUAL_SHORTLIST_BYTES" -ne "$EXPECTED_SHORTLIST_BYTES" ]; then
  fail "shortlist.bin size mismatch: got $ACTUAL_SHORTLIST_BYTES bytes, expected $EXPECTED_SHORTLIST_BYTES ($SHORTLIST_LEAVES * 33)"
fi
info "ok: shortlist.bin ($ACTUAL_SHORTLIST_BYTES bytes, $SHORTLIST_LEAVES leaves, root=$SHORTLIST_ROOT)"

REG_IMAGE_ID="$(compute_image_id "$REG_ELF" "registry_v8" "$IMAGEID_CMD")"
NFT_IMAGE_ID="$(compute_image_id "$NFT_ELF" "lez_nft_v3" "$IMAGEID_CMD")"

set +e
IMG_JSON="$(python3 "$LIB" check-images "$EXPECTED_IDS_FILE" "$REG_ELF" "$NFT_ELF" "$REG_IMAGE_ID" "$NFT_IMAGE_ID")"
IMG_RC=$?
set -e

python3 -c 'import json, sys
d = json.loads(sys.argv[1])
for k in ("nft", "registry"):
    x = d[k]
    print(f"    {k:8s} ELF:       {x[\"elf\"]} ({x[\"bytes\"]} bytes, {x[\"segments\"]} segments)")
    print(f"    {k:8s} sha256:    {x[\"elf_sha256\"]}")
    print(f"    {k:8s} image_id:  {x[\"image_id\"]}")
    print(f"    {k:8s} expected:  {x[\"expected_image_id\"]}")
for e in d.get("errors", []):
    print("ERROR:", e, file=sys.stderr)' "$IMG_JSON"

if [ "$IMG_RC" -ne 0 ]; then
  fail "ELF image ID check failed against $EXPECTED_IDS_FILE"
fi

REG_SEGMENTS="$(json_get "$IMG_JSON" 'd["registry"]["segments"]')"
NFT_SEGMENTS="$(json_get "$IMG_JSON" 'd["nft"]["segments"]')"
REG_ELF_SHA="$(json_get "$IMG_JSON" 'd["registry"]["elf_sha256"]')"
NFT_ELF_SHA="$(json_get "$IMG_JSON" 'd["nft"]["elf_sha256"]')"
info "ok: both guest image IDs match $EXPECTED_IDS_FILE"

# =============================================================================
# Step 3 — Network & wallet preflight
# =============================================================================
say "Step 3/9: Network & wallet preflight"

CHANNEL_ID=""
TREASURY_ADMITTED="false"

if [ -z "$RPC_URL" ]; then
  if [ "$DRY_RUN" -eq 1 ]; then
    warn "RPC URL is empty; skipping live sequencer probe in --dry-run (pass --rpc <url> to probe)"
  else
    fail "RPC URL is required for live deploy: pass --rpc <url>, set LEZ_RPC_URL, or set [network].rpc_url"
  fi
else
  info "probing sequencer at $RPC_URL ..."
  set +e
  PROBE_JSON="$(python3 "$LIB" rpc-probe "$RPC_URL" "$EXPECTED_CHANNEL")"
  PROBE_RC=$?
  set -e
  if [ "$PROBE_RC" -ne 0 ]; then
    printf '%s\n' "$PROBE_JSON" >&2
    fail "sequencer probe failed for $RPC_URL"
  fi
  LAST_BLOCK="$(json_get "$PROBE_JSON" 'd["last_block_id"]')"
  CHANNEL_ID="$(json_get "$PROBE_JSON" 'd["channel_id"]')"
  info "ok: sequencer live (last_block=$LAST_BLOCK, channel_id=$CHANNEL_ID)"

  # Check wallet_config.json points at the same sequencer when LEE_WALLET_HOME_DIR is set.
  if [ -n "${LEE_WALLET_HOME_DIR:-}" ] && [ -f "$LEE_WALLET_HOME_DIR/wallet_config.json" ]; then
    WALLET_RPC="$(python3 -c 'import json, sys
c = json.load(open(sys.argv[1]))
print((c.get("sequencer") or {}).get("sequencer_addr", "").rstrip("/"))' "$LEE_WALLET_HOME_DIR/wallet_config.json")"
    if [ -n "$WALLET_RPC" ] && [ "$WALLET_RPC" != "$RPC_URL" ]; then
      fail "wallet_config.json sequencer_addr ($WALLET_RPC) != target RPC ($RPC_URL)! Update $LEE_WALLET_HOME_DIR/wallet_config.json first."
    fi
    info "ok: wallet_config.json matches target RPC ($WALLET_RPC)"
  fi

  # Check balances of deployer_payer, nft_admin, admin, treasury
  for pair in "deployer_payer:$DEPLOYER_PAYER" "nft_admin:$NFT_ADMIN" "admin:$ADMIN_ID" "treasury:$TREASURY_ID"; do
    label="${pair%%:*}"
    acct="${pair#*:}"
    BAL_JSON="$(python3 "$LIB" balance "$RPC_URL" "$acct")"
    bal="$(json_get "$BAL_JSON" 'd["balance"]')"
    nonce="$(json_get "$BAL_JSON" 'd["nonce"]')"
    admitted="$(json_get "$BAL_JSON" 'd["admitted"]')"
    info "$(printf '%-15s %s  balance=%s  nonce=%s  admitted=%s' "$label" "$acct" "$bal" "$nonce" "$admitted")"
    if [ "$label" = "treasury" ]; then
      TREASURY_ADMITTED="$(printf '%s' "$admitted" | tr 'A-Z' 'a-z')"
    fi
    if [ "$label" = "deployer_payer" ] && [ "$bal" = "0" ] && [ "$DRY_RUN" -eq 0 ]; then
      fail "deployer_payer ($acct) has 0 balance on $RPC_URL"
    fi
  done
fi

if [ "$DRY_RUN" -eq 0 ]; then
  [ -x "$NAMES_CLI" ] || fail "names CLI not executable at $NAMES_CLI (set NAMES_CLI or [tooling].names_cli)"
  if [ "$SKIP_NFT_CONFIGURE" -eq 0 ] && [ -z "${NFT_CONFIGURE_CMD:-}" ]; then
    [ -x "$NFT_CLI" ] || fail "lez-nft CLI not executable at $NFT_CLI (set NFT_CLI, NFT_CONFIGURE_CMD, or --skip-nft-configure)"
  fi
  info "checking wallet health via $NAMES_CLI wallet check-health ..."
  "$NAMES_CLI" wallet check-health >/dev/null || fail "wallet check-health failed (check LEE_WALLET_HOME_DIR and LEE_WALLET_PASSWORD)"
  info "ok: wallet health check passed"
fi

# =============================================================================
# Step 4 — Human-auditable deployment plan summary
# =============================================================================
say "Step 4/9: Deployment Plan Summary"
cat <<EOF
  Network:         $NET_NAME (${RPC_URL:-<not set>})
  Channel ID:      ${CHANNEL_ID:-<unprobed>}
  Mode:            $([ "$DRY_RUN" -eq 1 ] && echo "DRY RUN (no transactions)" || echo "LIVE DEPLOY")

  [FIXED FOREVER — COMPILE-TIME]
    record_version:       8
    shortlist_root:       $SHORTLIST_ROOT ($SHORTLIST_LEAVES leaves)
    sale_fee_bps:         $SALE_FEE_BPS (2.5% = 1/40 to treasury, 0 on plain transfers)
    grace_days:           $GRACE_DAYS days
    premium_days:         $PREMIUM_DAYS days (daily halving)
    commit_waits:         ${PUB_WAIT}m public (1m window) / ${PRIV_WAIT}m private (10m window)
    registry_image_id:    $REG_IMAGE_ID ($REG_SEGMENTS segments)
    nft_image_id:         $NFT_IMAGE_ID ($NFT_SEGMENTS segments)

  [FIXED FOREVER — INITIAL CONFIGURE]
    Settings.nft_program: bound ONCE to deployed lez_nft v3 program ID (immutable!)

  [ADMIN-ADJUSTABLE — INITIAL SETTINGS]
    deployer_payer:       Public/$DEPLOYER_PAYER
    nft_admin:            Public/$NFT_ADMIN
    admin:                Public/$ADMIN_ID
    treasury:             Public/$TREASURY_ID (admitted=$TREASURY_ADMITTED)
    mode:                 $MODE_NAME
    premium_start:        $PREMIUM_START LEZ
    prices [30d,1y,5y,perm]:
EOF
python3 -c 'import json, sys
d = json.loads(sys.argv[1])
labels = ["tier_5 (<=5c)", "tier_6_7    ", "tier_8_plus "]
for lbl, row in zip(labels, d["prices"]):
    print(f"      {lbl}: {row}")
c = d["claims"]
print(f"    claims:               enabled={c[\"enabled\"]} root={c[\"root\"]} until={c[\"until\"]} program={c[\"program\"]}")' "$VALIDATE_JSON"

if [ "$DRY_RUN" -eq 0 ] && [ "${DEPLOY_YES:-0}" != "1" ]; then
  if [ -t 0 ]; then
    printf '\nFinal check: this will deploy irreversible v8 + lez_nft v3 contracts to %s.\n' "$RPC_URL"
    printf 'Type DEPLOY to continue: '
    read -r reply
    [ "$reply" = "DEPLOY" ] || fail "aborted by operator"
  else
    fail "non-interactive live deploy requires DEPLOY_YES=1 (in addition to confirmed_by_jef = true)"
  fi
fi

# Helper wrappers for wallet commands
run_names() {
  LOGOS_NAMES_RPC="$RPC_URL" NAMES_SHORTLIST="$SHORTLIST_FILE" "$NAMES_CLI" "$@"
}

new_public_account() {
  run_names wallet account new public | id_of Public
}

deploy_elf_program() {
  local label="$1" elf="$2" seg_count="$3" header segs=() i seg
  header="$(new_public_account)"
  [ -n "$header" ] || fail "failed to create header account for $label"
  for i in $(seq 1 "$seg_count"); do
    seg="$(new_public_account)"
    [ -n "$seg" ] || fail "failed to create segment $i/$seg_count for $label"
    segs+=("Public/$seg")
  done
  info "sending program-loader deploy for $label: header=Public/$header ($seg_count segments, payer=Public/$DEPLOYER_PAYER)" >&2
  run_names wallet program-loader deploy \
    --elf "$elf" \
    --header "Public/$header" \
    --segments "${segs[@]}" \
    --payer "Public/$DEPLOYER_PAYER" >&2
  printf '%s' "$header"
}

# Deterministic valid Base58 placeholder IDs for --dry-run (0x07*32 and 0x09*32)
DRY_REG_ID="$(python3 -c 'import sys; sys.path.insert(0, sys.argv[1]); import lib; print(lib.base58_encode(b"\x07" * 32))' "$SCRIPT_DIR")"
DRY_NFT_ID="$(python3 -c 'import sys; sys.path.insert(0, sys.argv[1]); import lib; print(lib.base58_encode(b"\x09" * 32))' "$SCRIPT_DIR")"

# =============================================================================
# Step 5 — Deploy lez_nft v3
# =============================================================================
say "Step 5/9: Deploy lez_nft v3 program"

NFT_PROGRAM_ID="$(state_get nft_program_id)"
if [ "$DRY_RUN" -eq 1 ]; then
  NFT_PROGRAM_ID="${NFT_PROGRAM_ID:-$DRY_NFT_ID}"
  info "[DRY RUN] TX 1: program-loader deploy lez_nft v3"
  info "  elf:      $NFT_ELF ($NFT_SEGMENTS segments, image_id=$NFT_IMAGE_ID)"
  info "  payer:    Public/$DEPLOYER_PAYER"
  info "  cmd:      $NAMES_CLI wallet program-loader deploy --elf $NFT_ELF --header Public/<new-nft-header> --segments <$NFT_SEGMENTS new public accounts> --payer Public/$DEPLOYER_PAYER"
  info "  (using placeholder NFT_PROGRAM_ID=$NFT_PROGRAM_ID for downstream dry-run display)"
else
  if [ -n "$NFT_PROGRAM_ID" ]; then
    info "resuming: nft_program_id already in state: $NFT_PROGRAM_ID"
  else
    NFT_PROGRAM_ID="$(deploy_elf_program "lez_nft_v3" "$NFT_ELF" "$NFT_SEGMENTS")"
    state_set "nft_program_id" "$NFT_PROGRAM_ID"
    state_set "nft_image_id" "$NFT_IMAGE_ID"
    state_set "nft_elf_sha256" "$NFT_ELF_SHA"
  fi
  python3 "$LIB" verify-program "$RPC_URL" "$NFT_PROGRAM_ID" >/dev/null || fail "deployed lez_nft header $NFT_PROGRAM_ID not found on chain"
  info "ok: lez_nft v3 deployed at $NFT_PROGRAM_ID"
fi

# =============================================================================
# Step 6 — Deploy LNS Registry v8
# =============================================================================
say "Step 6/9: Deploy LNS Registry v8 program"

REGISTRY_PROGRAM_ID="$(state_get registry_program_id)"
if [ "$DRY_RUN" -eq 1 ]; then
  REGISTRY_PROGRAM_ID="${REGISTRY_PROGRAM_ID:-$DRY_REG_ID}"
  info "[DRY RUN] TX 2: program-loader deploy names_core v8"
  info "  elf:      $REG_ELF ($REG_SEGMENTS segments, image_id=$REG_IMAGE_ID)"
  info "  payer:    Public/$DEPLOYER_PAYER"
  info "  cmd:      $NAMES_CLI wallet program-loader deploy --elf $REG_ELF --header Public/<new-registry-header> --segments <$REG_SEGMENTS new public accounts> --payer Public/$DEPLOYER_PAYER"
  info "  (using placeholder REGISTRY_PROGRAM_ID=$REGISTRY_PROGRAM_ID for downstream dry-run display)"
else
  if [ -n "$REGISTRY_PROGRAM_ID" ]; then
    info "resuming: registry_program_id already in state: $REGISTRY_PROGRAM_ID"
  else
    REGISTRY_PROGRAM_ID="$(deploy_elf_program "names_core_v8" "$REG_ELF" "$REG_SEGMENTS")"
    state_set "registry_program_id" "$REGISTRY_PROGRAM_ID"
    state_set "registry_image_id" "$REG_IMAGE_ID"
    state_set "registry_elf_sha256" "$REG_ELF_SHA"
  fi
  python3 "$LIB" verify-program "$RPC_URL" "$REGISTRY_PROGRAM_ID" >/dev/null || fail "deployed registry header $REGISTRY_PROGRAM_ID not found on chain"
  info "ok: LNS Registry v8 deployed at $REGISTRY_PROGRAM_ID"
fi

# =============================================================================
# Step 7 — Fund program headers (for initial Configure gas) and admit treasury
# =============================================================================
say "Step 7/9: Fund registry header, NFT header, and admit treasury on chain"

fund_account() {
  local label="$1" to_id="$2" amount="$3" state_key="$4" use_pk="${5:-}"
  if [ "$amount" -le 0 ]; then
    info "skipping $label funding (amount=0)"
    return 0
  fi
  if [ "$DRY_RUN" -eq 1 ]; then
    if [ -n "$use_pk" ]; then
      info "[DRY RUN] TX fund $label: auth-transfer --from Public/$DEPLOYER_PAYER --to-pk $use_pk --amount $amount"
    else
      info "[DRY RUN] TX fund $label: auth-transfer --from Public/$DEPLOYER_PAYER --to Public/$to_id --amount $amount"
    fi
    return 0
  fi
  if [ "$(state_get "$state_key")" = "done" ]; then
    info "resuming: $label already funded"
    return 0
  fi
  info "funding $label (Public/$to_id) with $amount LEZ from Public/$DEPLOYER_PAYER ..."
  if [ -n "$use_pk" ]; then
    run_names wallet auth-transfer send --from "Public/$DEPLOYER_PAYER" --to-pk "$use_pk" --amount "$amount"
  else
    run_names wallet auth-transfer send --from "Public/$DEPLOYER_PAYER" --to "Public/$to_id" --amount "$amount"
  fi
  state_set "$state_key" "done"
}

fund_account "registry_header" "$REGISTRY_PROGRAM_ID" "$REG_GAS_FUND" "funded_registry_header"
fund_account "nft_header" "$NFT_PROGRAM_ID" "$NFT_GAS_FUND" "funded_nft_header"

if [ "$TREASURY_ADMITTED" = "true" ]; then
  info "ok: treasury (Public/$TREASURY_ID) is already admitted on chain — skipping init transfer"
else
  fund_account "treasury (chain admission)" "$TREASURY_ID" "$TREASURY_INIT_FUND" "funded_treasury" "${TREASURY_PUBLIC_KEY:-}"
fi

# =============================================================================
# Step 8 — Configure lez_nft v3 (authorize registry & reserve LNS collection)
# =============================================================================
say "Step 8/9: Configure lez_nft v3 (authorize LNS Registry v8 + reserve LNS collection)"

if [ "$SKIP_NFT_CONFIGURE" -eq 1 ]; then
  warn "skipping lez_nft v3 Configure (--skip-nft-configure); ensure lez_nft authorizes $REGISTRY_PROGRAM_ID BEFORE first registration!"
elif [ "$DRY_RUN" -eq 1 ]; then
  if [ -n "${NFT_CONFIGURE_CMD:-}" ]; then
    info "[DRY RUN] TX lez_nft Configure (custom NFT_CONFIGURE_CMD):"
    info "  RPC_URL=${RPC_URL:-<rpc>} NFT_PROGRAM_ID=$NFT_PROGRAM_ID NFT_ADMIN_ID=$NFT_ADMIN REGISTRY_PROGRAM_ID=$REGISTRY_PROGRAM_ID $NFT_CONFIGURE_CMD"
  else
    info "[DRY RUN] TX lez_nft Configure:"
    info "  program:    $NFT_PROGRAM_ID"
    info "  admin:      Public/$NFT_ADMIN"
    info "  authorizes: registry=$REGISTRY_PROGRAM_ID (with collection_id = lns_collection_id(registry))"
    info "  cmd:        $NFT_CLI --rpc ${RPC_URL:-<rpc>} configure --program $NFT_PROGRAM_ID --admin Public/$NFT_ADMIN --authorize-registry $REGISTRY_PROGRAM_ID"
  fi
else
  if [ "$(state_get nft_configured)" = "done" ]; then
    info "resuming: lez_nft v3 already configured"
  else
    if [ -n "${NFT_CONFIGURE_CMD:-}" ]; then
      info "running custom NFT_CONFIGURE_CMD ..."
      RPC_URL="$RPC_URL" \
      LEZ_RPC_URL="$RPC_URL" \
      NFT_PROGRAM_ID="$NFT_PROGRAM_ID" \
      NFT_ADMIN_ID="$NFT_ADMIN" \
      REGISTRY_PROGRAM_ID="$REGISTRY_PROGRAM_ID" \
      DEPLOYER_PAYER_ID="$DEPLOYER_PAYER" \
      bash -c "$NFT_CONFIGURE_CMD"
    else
      info "configuring lez_nft v3 via $NFT_CLI ..."
      "$NFT_CLI" --rpc "$RPC_URL" configure \
        --program "$NFT_PROGRAM_ID" \
        --admin "Public/$NFT_ADMIN" \
        --authorize-registry "$REGISTRY_PROGRAM_ID"
    fi
    state_set "nft_configured" "done"
    info "ok: lez_nft v3 configured"
  fi
fi

# =============================================================================
# Step 9 — Initial LNS Registry v8 Configure (PERMANENTLY binds nft_program!)
# =============================================================================
say "Step 9/9: Configure LNS Registry v8 (permanently binds Settings.nft_program)"

CFG_JSON="$(python3 "$LIB" build-configure "$PARAMS_ABS" "$REPO_DIR" "$REGISTRY_PROGRAM_ID" "$NFT_PROGRAM_ID" "$REGISTRY_PROGRAM_ID")"
SETTINGS_PDA="$(json_get "$CFG_JSON" 'd["settings_pda"]')"
SETTINGS_SEED="$(json_get "$CFG_JSON" 'd["settings_seed"]')"
CFG_LEN="$(json_get "$CFG_JSON" 'd["message_len"]')"
CFG_HEX="$(json_get "$CFG_JSON" 'd["message_hex"]')"

info "registry program:  $REGISTRY_PROGRAM_ID (signs initial Configure)"
info "settings PDA:      $SETTINGS_PDA (seed=$SETTINGS_SEED)"
info "nft_program (FIXED FOREVER): $NFT_PROGRAM_ID"
info "borsh message:     $CFG_LEN bytes"
info "borsh hex:         $CFG_HEX"

if [ "$DRY_RUN" -eq 1 ]; then
  info "[DRY RUN] TX LNS Configure (names module send payload):"
  python3 -c 'import json, sys; print(json.dumps(json.loads(sys.argv[1])["module_send_json"], indent=2))' "$CFG_JSON"
  say "DRY RUN COMPLETE — 0 transactions sent. Review every value above before setting confirmed_by_jef = true."
  exit 0
fi

if [ "$(state_get registry_configured)" = "done" ]; then
  info "resuming: registry already configured; verifying on-chain Settings ..."
else
  if [ "$USE_CLI_CONFIGURE" -eq 1 ]; then
    info "sending initial Configure via '$NAMES_CLI configure' (--use-cli-configure) ..."
    # Requires v8-updated `names` CLI supporting --nft-program, --premium-start, and 3x4 prices.
    run_names --rpc "$RPC_URL" --program "$REGISTRY_PROGRAM_ID" configure \
      --signer "Public/$REGISTRY_PROGRAM_ID" \
      --admin "Public/$ADMIN_ID" \
      --treasury "Public/$TREASURY_ID" \
      --mode "$MODE_NAME" \
      --premium-start "$PREMIUM_START" \
      --nft-program "$NFT_PROGRAM_ID"
  else
    info "sending exact 411-byte v8 Configure via '$NAMES_CLI module send' ..."
    python3 -c 'import json, sys; print(json.dumps(json.loads(sys.argv[1])["module_send_json"]))' "$CFG_JSON" \
      | run_names --rpc "$RPC_URL" --program "$REGISTRY_PROGRAM_ID" module send
  fi
  state_set "registry_configured" "done"
fi

# Verify on-chain Settings match params.toml bit-for-bit
info "verifying on-chain Settings PDA ($SETTINGS_PDA) ..."
VERIFY_JSON="$(python3 "$LIB" verify-settings "$RPC_URL" "$REGISTRY_PROGRAM_ID" "$NFT_PROGRAM_ID" "$PARAMS_ABS" "$REPO_DIR")"
printf '%s\n' "$VERIFY_JSON"

# Finalize manifest & deployed-ids.env (public IDs only, chmod 0600)
ID_INFO="$(python3 "$LIB" id-info "$REGISTRY_PROGRAM_ID" "$NFT_PROGRAM_ID")"
REG_HEX="$(json_get "$ID_INFO" 'd["registry_hex"]')"
NFT_HEX="$(json_get "$ID_INFO" 'd["nft_hex"]')"

state_set "network" "$NET_NAME"
state_set "rpc_url" "$RPC_URL"
state_set "channel_id" "$CHANNEL_ID"
state_set "settings_pda" "$SETTINGS_PDA"
state_set "registry_program_hex" "$REG_HEX"
state_set "nft_program_hex" "$NFT_HEX"
state_set "admin" "$ADMIN_ID"
state_set "treasury" "$TREASURY_ID"
state_set "nft_admin" "$NFT_ADMIN"
state_set "shortlist_root" "$SHORTLIST_ROOT"
state_set "deployed_at_utc" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"

ENV_OUT="$STATE_DIR/deployed-ids.env"
umask 077
cat > "$ENV_OUT" <<EOF
# Public deployment IDs generated by deploy/deploy.sh (NO SECRETS)
LEZ_NETWORK="$NET_NAME"
LEZ_RPC_URL="$RPC_URL"
LEZ_CHANNEL_ID="$CHANNEL_ID"
LNS_REGISTRY_PROGRAM_ID="$REGISTRY_PROGRAM_ID"
LNS_REGISTRY_PROGRAM_HEX="$REG_HEX"
LNS_SETTINGS_PDA="$SETTINGS_PDA"
LNS_NFT_PROGRAM_ID="$NFT_PROGRAM_ID"
LNS_NFT_PROGRAM_HEX="$NFT_HEX"
LNS_ADMIN_ID="$ADMIN_ID"
LNS_TREASURY_ID="$TREASURY_ID"
LNS_NFT_ADMIN_ID="$NFT_ADMIN"
LNS_SHORTLIST_ROOT="$SHORTLIST_ROOT"
EOF
chmod 0600 "$ENV_OUT"

say "DEPLOY COMPLETE & VERIFIED"
info "LNS Registry v8: $REGISTRY_PROGRAM_ID (hex: $REG_HEX)"
info "lez_nft v3:      $NFT_PROGRAM_ID (hex: $NFT_HEX)"
info "Settings PDA:    $SETTINGS_PDA"
info "State manifest:  $STATE_FILE"
info "Env summary:     $ENV_OUT"
info "Next step:       ./deploy/post-deploy.sh --params $PARAMS_ABS --state $STATE_FILE"
