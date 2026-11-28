#!/usr/bin/env bash
# =============================================================================
# LNS Registry v8 + lez_nft v3 — Post-Deploy Verification & Held-Name Placement
# =============================================================================
#
# Runs after `deploy/deploy.sh` succeeds:
#   Phase 1: Admin registers held names (`jef`, `logos`, `web`, `android`, and any
#            additional `[[held_names]]` in params.toml) and transfers each to its
#            designated owner, verifying on-chain NameRecord v8 ownership.
#   Phase 2: End-to-end PUBLIC registration smoke test (5-min commit wait, real
#            non-admin payer -> treasury fee increment verified).
#   Phase 3: End-to-end PRIVATE registration smoke test (15-min commit wait +
#            local RISC0 proof, private holder tag + treasury fee verified).
#   Phase 4: Verify tied 1-of-1 NFT mint & lockstep link between NameRecord v8
#            and lez_nft v3 NftRecord on chain via JSON-RPC.
#   Phase 5: Print ready-to-paste client configuration blocks for Web App,
#            Names Wallet (Basecamp), Gateway & MCP, Android App, CLI, Board, Docs.
#
# Never stores keys or passwords; reads them at run time from existing wallet
# tooling (`LEE_WALLET_HOME_DIR` / `LEE_WALLET_PASSWORD` or interactive prompt).
#
# Usage:
#   ./deploy/post-deploy.sh [--params deploy/params.toml] \
#                           [--state deploy/state/deploy-state.json] \
#                           [--rpc URL] [--registry ID] [--nft ID] \
#                           [--dry-run] [--skip-held] [--skip-public-smoke] \
#                           [--skip-private-smoke] [--skip-nft-check] \
#                           [--only-print-config]
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
LIB="$SCRIPT_DIR/lib.py"

PARAMS_FILE="$SCRIPT_DIR/params.toml"
STATE_FILE="$SCRIPT_DIR/state/deploy-state.json"
RPC_OVERRIDE=""
REGISTRY_OVERRIDE=""
NFT_OVERRIDE=""
DRY_RUN=0
SKIP_HELD=0
SKIP_PUBLIC_SMOKE=0
SKIP_PRIVATE_SMOKE=0
SKIP_NFT_CHECK=0
ONLY_PRINT_CONFIG=0

usage() {
  cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Options:
  --params <path>        Path to params.toml (default: deploy/params.toml)
  --state <path>         Deploy state JSON from deploy.sh (default: deploy/state/deploy-state.json)
  --rpc <url>            Sequencer RPC URL (overrides state/env/params)
  --registry <id>        Deployed LNS Registry v8 program ID (overrides state)
  --nft <id>             Deployed lez_nft v3 program ID (overrides state)
  --dry-run              Print every planned command & check without sending transactions
  --skip-held            Skip Phase 1 (held names registration & transfer)
  --skip-public-smoke    Skip Phase 2 (public E2E registration smoke test)
  --skip-private-smoke   Skip Phase 3 (private E2E registration smoke test)
  --skip-nft-check       Skip Phase 4 (lez_nft v3 lockstep link verification)
  --only-print-config    Skip Phases 1-4 and only print client config blocks (Phase 5)
  -h, --help             Show this help
EOF
}

while [ $# -gt 0 ]; do
  case "$1" in
    --params) PARAMS_FILE="$2"; shift 2 ;;
    --state) STATE_FILE="$2"; shift 2 ;;
    --rpc) RPC_OVERRIDE="$2"; shift 2 ;;
    --registry) REGISTRY_OVERRIDE="$2"; shift 2 ;;
    --nft) NFT_OVERRIDE="$2"; shift 2 ;;
    --dry-run) DRY_RUN=1; shift ;;
    --skip-held) SKIP_HELD=1; shift ;;
    --skip-public-smoke) SKIP_PUBLIC_SMOKE=1; shift ;;
    --skip-private-smoke) SKIP_PRIVATE_SMOKE=1; shift ;;
    --skip-nft-check) SKIP_NFT_CHECK=1; shift ;;
    --only-print-config) ONLY_PRINT_CONFIG=1; shift ;;
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
[ -f "$PARAMS_FILE" ] || fail "params file not found: $PARAMS_FILE"

PARAMS_ABS="$(cd "$(dirname "$PARAMS_FILE")" && pwd)/$(basename "$PARAMS_FILE")"
PARAMS_DIR="$(dirname "$PARAMS_ABS")"

resolve_path() {
  local raw="$1"
  if [[ "$raw" = /* ]]; then
    printf '%s\n' "$raw"
  else
    python3 -c 'import os, sys; print(os.path.normpath(os.path.join(sys.argv[1], sys.argv[2])))' "$PARAMS_DIR" "$raw"
  fi
}

state_get() {
  local key="$1"
  [ -f "$STATE_FILE" ] || { printf ''; return 0; }
  python3 -c 'import json, sys
d = json.load(open(sys.argv[1]))
v = d.get(sys.argv[2], "")
print("" if v is None else v)' "$STATE_FILE" "$key"
}

json_get() {
  local json_str="$1" expr="$2"
  python3 -c "import json, sys; d = json.loads(sys.argv[1]); v = $expr; print('' if v is None else v)" "$json_str"
}

# Validate params (dry-run mode so post-deploy works even in dry-run preview)
VALIDATE_JSON="$(python3 "$LIB" validate "$PARAMS_ABS" "$REPO_DIR" --dry-run)" || fail "params.toml failed validation"

NET_NAME="$(json_get "$VALIDATE_JSON" 'd["network"].get("name", "lez-v0.4-testnet")')"
PARAM_RPC="$(json_get "$VALIDATE_JSON" 'd["network"].get("rpc_url", "")')"
ADMIN_ID="$(json_get "$VALIDATE_JSON" 'd["accounts"]["admin"]')"
TREASURY_ID="$(json_get "$VALIDATE_JSON" 'd["accounts"]["treasury"]')"
SHORTLIST_ROOT="$(json_get "$VALIDATE_JSON" 'd["immutable"]["shortlist_root"]')"

NAMES_CLI="${NAMES_CLI:-$(resolve_path "$(json_get "$VALIDATE_JSON" 'd["tooling"].get("names_cli", "../target/release/names")')")}"
SHORTLIST_FILE="${NAMES_SHORTLIST:-$(resolve_path "$(json_get "$VALIDATE_JSON" 'd["artifacts"].get("shortlist_file", "../shortlist/shortlist.bin")')")}"

STATE_RPC="$(state_get rpc_url)"
STATE_REG="$(state_get registry_program_id)"
STATE_NFT="$(state_get nft_program_id)"
CHANNEL_ID="$(state_get channel_id)"

RPC_URL="${RPC_OVERRIDE:-${LEZ_RPC_URL:-${STATE_RPC:-$PARAM_RPC}}}"
RPC_URL="${RPC_URL%/}"
REGISTRY_ID="${REGISTRY_OVERRIDE:-$STATE_REG}"
NFT_ID="${NFT_OVERRIDE:-$STATE_NFT}"

if [ "$DRY_RUN" -eq 1 ]; then
  [ -n "$REGISTRY_ID" ] || REGISTRY_ID="$(python3 -c 'import sys; sys.path.insert(0, sys.argv[1]); import lib; print(lib.base58_encode(b"\x07" * 32))' "$SCRIPT_DIR")"
  [ -n "$NFT_ID" ] || NFT_ID="$(python3 -c 'import sys; sys.path.insert(0, sys.argv[1]); import lib; print(lib.base58_encode(b"\x09" * 32))' "$SCRIPT_DIR")"
else
  [ -n "$RPC_URL" ] || fail "RPC URL is required (pass --rpc or ensure $STATE_FILE has rpc_url)"
  [ -n "$REGISTRY_ID" ] || fail "Registry program ID is required (pass --registry or run deploy.sh first)"
  [ -n "$NFT_ID" ] || fail "lez_nft program ID is required (pass --nft or run deploy.sh first)"
fi

if [ "$ONLY_PRINT_CONFIG" -eq 0 ] && [ "$DRY_RUN" -eq 0 ]; then
  [ -x "$NAMES_CLI" ] || fail "names CLI not executable at $NAMES_CLI"
  [ -f "$SHORTLIST_FILE" ] || fail "shortlist file not found at $SHORTLIST_FILE"
fi

run_names() {
  LOGOS_NAMES_RPC="$RPC_URL" NAMES_SHORTLIST="$SHORTLIST_FILE" "$NAMES_CLI" --rpc "$RPC_URL" --program "$REGISTRY_ID" "$@"
}

run_smoke_names() {
  local wallet_dir="$1"
  shift
  if [ -n "$wallet_dir" ]; then
    LEE_WALLET_HOME_DIR="$wallet_dir" LOGOS_NAMES_RPC="$RPC_URL" NAMES_SHORTLIST="$SHORTLIST_FILE" \
      "$NAMES_CLI" --rpc "$RPC_URL" --program "$REGISTRY_ID" "$@"
  else
    run_names "$@"
  fi
}

# Extract 64-hex Argon2id base_id from `names address <name>` output.
name_base_id_hex() {
  local name="$1" out="" hex=""
  out="$(run_names address "$name" 2>&1)" || fail "'names address $name' failed: $out"
  hex="$(printf '%s\n' "$out" | grep -oE '[0-9a-fA-F]{64}' | head -1 | tr 'A-F' 'a-f' || true)"
  [ -n "$hex" ] || fail "could not parse 64-hex base_id from 'names address $name': $out"
  printf '%s' "$hex"
}

term_index() {
  case "$1" in
    30d) echo 0 ;;
    1y) echo 1 ;;
    5y) echo 2 ;;
    permanent) echo 3 ;;
    *) fail "invalid term: $1 (must be 30d, 1y, 5y, or permanent)" ;;
  esac
}

tier_index_for_len() {
  local n="$1"
  if [ "$n" -le 5 ]; then echo 0
  elif [ "$n" -le 7 ]; then echo 1
  else echo 2
  fi
}

price_for_name_term() {
  local name="$1" term="$2" ti ci
  ti="$(tier_index_for_len "${#name}")"
  ci="$(term_index "$term")"
  python3 -c 'import json, sys; d = json.loads(sys.argv[1]); print(d["prices"][int(sys.argv[2])][int(sys.argv[3])])' \
    "$VALIDATE_JSON" "$ti" "$ci"
}

if [ "$DRY_RUN" -eq 1 ]; then
  say "POST-DEPLOY — DRY RUN (ZERO TRANSACTIONS WILL BE SENT)"
else
  say "POST-DEPLOY — LIVE EXECUTION"
fi
info "network:  $NET_NAME (${RPC_URL:-<not set>})"
info "registry: $REGISTRY_ID"
info "lez_nft:  $NFT_ID"
info "admin:    Public/$ADMIN_ID"
info "treasury: Public/$TREASURY_ID"

# Verify on-chain Settings before doing any live mutations
if [ "$ONLY_PRINT_CONFIG" -eq 0 ] && [ "$DRY_RUN" -eq 0 ]; then
  say "Preflight: verifying on-chain Settings match params.toml"
  python3 "$LIB" verify-settings "$RPC_URL" "$REGISTRY_ID" "$NFT_ID" "$PARAMS_ABS" "$REPO_DIR" >/dev/null \
    || fail "on-chain Settings do not match params.toml — refusing to proceed"
  info "ok: on-chain Settings verified"
fi

# =============================================================================
# Phase 1 — Held names (admin registers & transfers to designated owners)
# =============================================================================
FIRST_HELD_NAME=""
FIRST_HELD_OWNER=""

if [ "$ONLY_PRINT_CONFIG" -eq 0 ] && [ "$SKIP_HELD" -eq 0 ]; then
  say "Phase 1/5: Held names placement (admin registers & transfers)"

  # Parse & strictly validate every [[held_names]] entry before touching the chain
  HELD_PLAN="$(python3 -c 'import json, re, sys
sys.path.insert(0, sys.argv[1])
import lib
d = json.loads(sys.argv[2])
dry = sys.argv[3] == "1"
held = d.get("held_names", [])
errors = []
warnings = []
rows = []
for i, item in enumerate(held):
    name = str(item.get("name", "")).strip().lower()
    owner = str(item.get("owner", "")).strip()
    term = str(item.get("term", "permanent")).strip().lower()
    note = str(item.get("note", "")).strip()
    if not name or not re.fullmatch(r"[a-z0-9][a-z0-9-]{0,30}[a-z0-9]|[a-z0-9]", name) or "--" in name:
        errors.append(f"held_names[{i}]: invalid name {name!r}")
    if term not in lib.TERM_NAMES:
        errors.append(f"held_names[{i}] ({name}): invalid term {term!r}")
    if not owner:
        msg = f"held_names[{i}] ({name}): owner is empty — fill in Public/<id> or Private/<id> before live run"
        if dry:
            warnings.append(msg)
        else:
            errors.append(msg)
    else:
        try:
            lib.parse_account_id(owner, allow_zero=False)
        except ValueError as exc:
            errors.append(f"held_names[{i}] ({name}): invalid owner {owner!r}: {exc}")
    rows.append({"name": name, "owner": owner, "term": term, "note": note})
print(json.dumps({"ok": not errors, "errors": errors, "warnings": warnings, "rows": rows}))' "$SCRIPT_DIR" "$VALIDATE_JSON" "$DRY_RUN")"

  python3 -c 'import json, sys
d = json.loads(sys.argv[1])
for w in d["warnings"]: print("WARN:", w, file=sys.stderr)
for e in d["errors"]: print("ERROR:", e, file=sys.stderr)
sys.exit(0 if d["ok"] else 1)' "$HELD_PLAN" || fail "held_names validation failed"

  HELD_COUNT="$(json_get "$HELD_PLAN" 'len(d["rows"])')"
  info "configured held names: $HELD_COUNT"

  TOTAL_HELD_FEE=0
  if [ "$HELD_COUNT" -gt 0 ]; then
  for idx in $(seq 0 $((HELD_COUNT - 1))); do
    H_NAME="$(json_get "$HELD_PLAN" "d['rows'][$idx]['name']")"
    H_OWNER="$(json_get "$HELD_PLAN" "d['rows'][$idx]['owner']")"
    H_TERM="$(json_get "$HELD_PLAN" "d['rows'][$idx]['term']")"
    H_NOTE="$(json_get "$HELD_PLAN" "d['rows'][$idx]['note']")"
    [ -n "$FIRST_HELD_NAME" ] || { FIRST_HELD_NAME="$H_NAME"; FIRST_HELD_OWNER="$H_OWNER"; }

    FEE="$(price_for_name_term "$H_NAME" "$H_TERM")"
    if [ "$ADMIN_ID" != "$TREASURY_ID" ]; then
      TOTAL_HELD_FEE=$(( TOTAL_HELD_FEE + FEE ))
    fi

    if [ "$DRY_RUN" -eq 1 ]; then
      info "[DRY RUN] held[$((idx + 1))/$HELD_COUNT]: name='$H_NAME' term=$H_TERM owner='${H_OWNER:-<UNSET>}' fee_if_admin_ne_treasury=$FEE ($H_NOTE)"
      info "  1) $NAMES_CLI --rpc ${RPC_URL:-<rpc>} --program $REGISTRY_ID register $H_NAME --public --owner Public/$ADMIN_ID --term $H_TERM --wait"
      info "  2) $NAMES_CLI --rpc ${RPC_URL:-<rpc>} --program $REGISTRY_ID transfer $H_NAME ${H_OWNER:-<UNSET>} --owner Public/$ADMIN_ID"
      continue
    fi

    # Live idempotent check
    BASE_HEX="$(name_base_id_hex "$H_NAME")"
    INSPECT_TARGET="$(python3 "$LIB" inspect-record "$RPC_URL" "$REGISTRY_ID" "$BASE_HEX" "${#H_NAME}" "$H_OWNER")"
    EXISTS="$(json_get "$INSPECT_TARGET" 'd["exists"]')"
    MATCHES_TARGET="$(json_get "$INSPECT_TARGET" 'd["owner_matches"]')"

    if [ "$EXISTS" = "True" ] && [ "$MATCHES_TARGET" = "True" ]; then
      info "ok (already placed): '$H_NAME' is already owned by $H_OWNER"
      continue
    fi

    if [ "$EXISTS" = "True" ]; then
      INSPECT_ADMIN="$(python3 "$LIB" inspect-record "$RPC_URL" "$REGISTRY_ID" "$BASE_HEX" "${#H_NAME}" "Public/$ADMIN_ID")"
      MATCHES_ADMIN="$(json_get "$INSPECT_ADMIN" 'd["owner_matches"]')"
      if [ "$MATCHES_ADMIN" = "True" ]; then
        info "resuming: '$H_NAME' already registered to admin; transferring to $H_OWNER ..."
      else
        fail "name '$H_NAME' is already registered to an unexpected owner (not admin and not $H_OWNER)!"
      fi
    else
      info "registering held name '$H_NAME' (term=$H_TERM) as admin Public/$ADMIN_ID (5-min public commit wait) ..."
      run_names register "$H_NAME" --public --owner "Public/$ADMIN_ID" --term "$H_TERM" --wait
    fi

    # Transfer to target owner (skip if target owner IS admin itself)
    TARGET_BARE="${H_OWNER#Public/}"
    TARGET_BARE="${TARGET_BARE#Private/}"
    if [ "$TARGET_BARE" = "$ADMIN_ID" ]; then
      info "ok: '$H_NAME' target owner is admin itself; no transfer needed"
    else
      info "transferring '$H_NAME' from Public/$ADMIN_ID -> $H_OWNER ..."
      run_names transfer "$H_NAME" "$H_OWNER" --owner "Public/$ADMIN_ID"
    fi

    # Verify final ownership on chain
    FINAL_INSPECT="$(python3 "$LIB" inspect-record "$RPC_URL" "$REGISTRY_ID" "$BASE_HEX" "${#H_NAME}" "$H_OWNER")"
    [ "$(json_get "$FINAL_INSPECT" 'd["owner_matches"]')" = "True" ] \
      || fail "post-transfer verification failed for '$H_NAME': $FINAL_INSPECT"
    info "ok: '$H_NAME' verified on chain -> $H_OWNER (nft_token_id=$(json_get "$FINAL_INSPECT" 'd["record"]["nft_token_id"]'))"
  done
  fi

  if [ "$ADMIN_ID" != "$TREASURY_ID" ] && [ "$TOTAL_HELD_FEE" -gt 0 ]; then
    warn "admin != treasury in fee mode: registering all $HELD_COUNT held names requires $TOTAL_HELD_FEE LEZ in admin's balance (paid to treasury)."
  fi
fi

# Read [post_deploy] smoke config
SMOKE_WALLET_DIR="$(json_get "$VALIDATE_JSON" 'd["post_deploy"].get("smoke_wallet_dir", "")')"
SMOKE_PUB_NAME="$(json_get "$VALIDATE_JSON" 'd["post_deploy"].get("smoke_public_name", "lns-smoke-pub-v04")')"
SMOKE_PRIV_NAME="$(json_get "$VALIDATE_JSON" 'd["post_deploy"].get("smoke_private_name", "lns-smoke-priv-v04")')"
SMOKE_TERM="$(json_get "$VALIDATE_JSON" 'd["post_deploy"].get("smoke_term", "30d")')"
SMOKE_PUB_OWNER="$(json_get "$VALIDATE_JSON" 'd["post_deploy"].get("smoke_public_owner", "")')"
SMOKE_PRIV_OWNER="$(json_get "$VALIDATE_JSON" 'd["post_deploy"].get("smoke_private_owner", "")')"
NFT_CHECK_NAME="$(json_get "$VALIDATE_JSON" 'd["post_deploy"].get("nft_check_name", "")')"

# =============================================================================
# Phase 2 — Public registration E2E smoke test (5-min wait + treasury fee check)
# =============================================================================
if [ "$ONLY_PRINT_CONFIG" -eq 0 ] && [ "$SKIP_PUBLIC_SMOKE" -eq 0 ]; then
  say "Phase 2/5: Public registration E2E smoke test ('$SMOKE_PUB_NAME', term=$SMOKE_TERM)"
  PUB_FEE="$(price_for_name_term "$SMOKE_PUB_NAME" "$SMOKE_TERM")"

  if [ "$DRY_RUN" -eq 1 ]; then
    info "[DRY RUN] public smoke name='$SMOKE_PUB_NAME' owner='${SMOKE_PUB_OWNER:-<UNSET>}' term=$SMOKE_TERM expected_fee=$PUB_FEE"
    info "  1) check treasury balance before"
    info "  2) $NAMES_CLI --rpc ${RPC_URL:-<rpc>} --program $REGISTRY_ID register $SMOKE_PUB_NAME --public --owner ${SMOKE_PUB_OWNER:-<UNSET>} --term $SMOKE_TERM --wait"
    info "  3) assert treasury balance delta == $PUB_FEE and NameRecord v8 owner matches"
    info "  4) $NAMES_CLI --rpc ${RPC_URL:-<rpc>} --program $REGISTRY_ID resolve $SMOKE_PUB_NAME"
  else
    [ -n "$SMOKE_PUB_OWNER" ] || fail "[post_deploy].smoke_public_owner is empty (set a non-admin, non-treasury Public/<id> or pass --skip-public-smoke)"
    PUB_BARE="${SMOKE_PUB_OWNER#Public/}"
    if [ "$PUB_BARE" = "$ADMIN_ID" ] || [ "$PUB_BARE" = "$TREASURY_ID" ]; then
      fail "smoke_public_owner must NOT be admin or treasury (must test real shortlist non-membership proof + fee transfer)"
    fi
    TREAS_BEFORE="$(json_get "$(python3 "$LIB" balance "$RPC_URL" "$TREASURY_ID")" 'd["balance"]')"
    info "treasury balance before: $TREAS_BEFORE LEZ (expecting +$PUB_FEE LEZ)"
    info "running public registration (5-min commit wait) ..."
    run_smoke_names "$SMOKE_WALLET_DIR" register "$SMOKE_PUB_NAME" --public --owner "$SMOKE_PUB_OWNER" --term "$SMOKE_TERM" --wait

    TREAS_AFTER="$(json_get "$(python3 "$LIB" balance "$RPC_URL" "$TREASURY_ID")" 'd["balance"]')"
    DELTA=$(( TREAS_AFTER - TREAS_BEFORE ))
    info "treasury balance after:  $TREAS_AFTER LEZ (delta=+$DELTA LEZ)"
    [ "$DELTA" -eq "$PUB_FEE" ] || warn "treasury delta ($DELTA) != expected fee ($PUB_FEE) — check if other traffic hit treasury concurrently"

    PUB_BASE_HEX="$(name_base_id_hex "$SMOKE_PUB_NAME")"
    PUB_INSPECT="$(python3 "$LIB" inspect-record "$RPC_URL" "$REGISTRY_ID" "$PUB_BASE_HEX" "${#SMOKE_PUB_NAME}" "$SMOKE_PUB_OWNER")"
    [ "$(json_get "$PUB_INSPECT" 'd["owner_matches"]')" = "True" ] || fail "public smoke NameRecord check failed: $PUB_INSPECT"
    run_smoke_names "$SMOKE_WALLET_DIR" resolve "$SMOKE_PUB_NAME"
    info "ok: public registration E2E passed for '$SMOKE_PUB_NAME'"
  fi
fi

# =============================================================================
# Phase 3 — Private registration E2E smoke test (15-min wait + RISC0 proof)
# =============================================================================
if [ "$ONLY_PRINT_CONFIG" -eq 0 ] && [ "$SKIP_PRIVATE_SMOKE" -eq 0 ]; then
  say "Phase 3/5: Private registration E2E smoke test ('$SMOKE_PRIV_NAME', term=$SMOKE_TERM)"
  PRIV_FEE="$(price_for_name_term "$SMOKE_PRIV_NAME" "$SMOKE_TERM")"

  if [ "$DRY_RUN" -eq 1 ]; then
    info "[DRY RUN] private smoke name='$SMOKE_PRIV_NAME' owner='${SMOKE_PRIV_OWNER:-<UNSET>}' term=$SMOKE_TERM expected_fee=$PRIV_FEE"
    info "  1) verify r0vm / local RISC0 prover available"
    info "  2) $NAMES_CLI --rpc ${RPC_URL:-<rpc>} --program $REGISTRY_ID register $SMOKE_PRIV_NAME --private --owner ${SMOKE_PRIV_OWNER:-<UNSET>} --term $SMOKE_TERM --wait"
    info "  3) assert treasury balance delta == $PRIV_FEE and NameRecord v8 owner_tag matches private owner"
  else
    [ -n "$SMOKE_PRIV_OWNER" ] || fail "[post_deploy].smoke_private_owner is empty (set a Private/<id> or pass --skip-private-smoke)"
    command -v r0vm >/dev/null 2>&1 || warn "r0vm not found in PATH — private proving may fail unless configured elsewhere"

    TREAS_BEFORE="$(json_get "$(python3 "$LIB" balance "$RPC_URL" "$TREASURY_ID")" 'd["balance"]')"
    info "treasury balance before: $TREAS_BEFORE LEZ (expecting +$PRIV_FEE LEZ)"
    info "running private registration (15-min commit wait + local RISC0 proof) ..."
    run_smoke_names "$SMOKE_WALLET_DIR" register "$SMOKE_PRIV_NAME" --private --owner "$SMOKE_PRIV_OWNER" --term "$SMOKE_TERM" --wait

    TREAS_AFTER="$(json_get "$(python3 "$LIB" balance "$RPC_URL" "$TREASURY_ID")" 'd["balance"]')"
    DELTA=$(( TREAS_AFTER - TREAS_BEFORE ))
    info "treasury balance after:  $TREAS_AFTER LEZ (delta=+$DELTA LEZ)"
    [ "$DELTA" -eq "$PRIV_FEE" ] || warn "treasury delta ($DELTA) != expected fee ($PRIV_FEE) — check concurrent traffic"

    PRIV_BASE_HEX="$(name_base_id_hex "$SMOKE_PRIV_NAME")"
    PRIV_INSPECT="$(python3 "$LIB" inspect-record "$RPC_URL" "$REGISTRY_ID" "$PRIV_BASE_HEX" "${#SMOKE_PRIV_NAME}" "$SMOKE_PRIV_OWNER")"
    [ "$(json_get "$PRIV_INSPECT" 'd["owner_matches"]')" = "True" ] || fail "private smoke NameRecord check failed: $PRIV_INSPECT"
    info "ok: private registration E2E passed for '$SMOKE_PRIV_NAME'"
  fi
fi

# =============================================================================
# Phase 4 — Verify tied 1-of-1 NFT mint & lockstep link on lez_nft v3
# =============================================================================
if [ "$ONLY_PRINT_CONFIG" -eq 0 ] && [ "$SKIP_NFT_CHECK" -eq 0 ]; then
  CHECK_NAME="$NFT_CHECK_NAME"
  CHECK_OWNER=""
  if [ -z "$CHECK_NAME" ]; then
    if [ "$SKIP_PUBLIC_SMOKE" -eq 0 ] && [ -n "$SMOKE_PUB_NAME" ]; then
      CHECK_NAME="$SMOKE_PUB_NAME"
      CHECK_OWNER="$SMOKE_PUB_OWNER"
    elif [ -n "$FIRST_HELD_NAME" ]; then
      CHECK_NAME="$FIRST_HELD_NAME"
      CHECK_OWNER="$FIRST_HELD_OWNER"
    fi
  fi

  say "Phase 4/5: Tied NFT mint & lockstep link verification ('$CHECK_NAME')"
  if [ "$DRY_RUN" -eq 1 ]; then
    info "[DRY RUN] will derive base_id for '$CHECK_NAME' via 'names address $CHECK_NAME' and run:"
    info "  python3 deploy/lib.py verify-nft-link ${RPC_URL:-<rpc>} $REGISTRY_ID $NFT_ID <base_id_hex> ${#CHECK_NAME} ${CHECK_OWNER:-<owner>}"
    info "  checks: NameRecord.nft_token_id == NftRecord.token_id, owner == owner, expires == expires,"
    info "          shielded == nft_shielded, linked.program == registry, linked.authority_actor == settings_pda,"
    info "          linked.target == slot_id, linked.record_actor == record_pda"
  else
    [ -n "$CHECK_NAME" ] || fail "no name selected for NFT link check (set [post_deploy].nft_check_name)"
    CHECK_BASE_HEX="$(name_base_id_hex "$CHECK_NAME")"
    NFT_LINK_JSON="$(python3 "$LIB" verify-nft-link "$RPC_URL" "$REGISTRY_ID" "$NFT_ID" "$CHECK_BASE_HEX" "${#CHECK_NAME}" "$CHECK_OWNER")"
    printf '%s\n' "$NFT_LINK_JSON"
    info "ok: tied 1-of-1 NFT mint & lockstep link verified for '$CHECK_NAME'"
  fi
fi

# =============================================================================
# Phase 5 — Print new program IDs & copy-paste blocks for every client
# =============================================================================
say "Phase 5/5: Client Configuration Values (copy-paste for rollout)"

ID_INFO="$(python3 "$LIB" id-info "$REGISTRY_ID" "$NFT_ID")"
REG_HEX="$(json_get "$ID_INFO" 'd["registry_hex"]')"
NFT_HEX="$(json_get "$ID_INFO" 'd["nft_hex"]')"
SETTINGS_PDA="$(json_get "$ID_INFO" 'd["settings_pda"]')"

cat <<EOF

===============================================================================
NEW LEZ v0.4 TESTNET PROGRAM IDS  ($NET_NAME)
===============================================================================
  LEZ_RPC_URL              = ${RPC_URL:-<SET_ON_MONDAY>}
  LEZ_CHANNEL_ID           = ${CHANNEL_ID:-<FROM_SEQUENCER>}
  LNS_REGISTRY_PROGRAM_ID  = $REGISTRY_ID
  LNS_REGISTRY_PROGRAM_HEX = $REG_HEX
  LNS_SETTINGS_PDA         = $SETTINGS_PDA
  LNS_NFT_PROGRAM_ID       = $NFT_ID
  LNS_NFT_PROGRAM_HEX      = $NFT_HEX
  LNS_ADMIN_ID             = Public/$ADMIN_ID
  LNS_TREASURY_ID          = Public/$TREASURY_ID
  LNS_SHORTLIST_ROOT       = $SHORTLIST_ROOT

1. CLI  (repo/cli/src/main.rs)
   - DEFAULT_RPC:      "${RPC_URL:-<SET_ON_MONDAY>}"
   - DEFAULT_PROGRAM:  "$REGISTRY_ID"
   - DEFAULT_NFT:      "$NFT_ID"
   - RETIRED_PROGRAMS: move old v5/v6 testnet ID into RETIRED_PROGRAMS

2. Names Wallet / Basecamp Module  (repo/basecamp-module/rust-lib/src/lib.rs)
   - DEFAULT_SEQUENCER:  "${RPC_URL:-<SET_ON_MONDAY>}"
   - DEFAULT_REGISTRY:   "$REGISTRY_ID"
   - DEFAULT_NFT:        "$NFT_ID"
   - RETIRED_REGISTRIES: move old v5/v6 testnet ID into RETIRED_REGISTRIES

3. Web App  (repo/gateway/public/app/app.js & repo/gateway/public/app/register.js & nameswallet.app)
   - CHAINS / DEFAULTS:
       rpc:        "${RPC_URL:-<SET_ON_MONDAY>}"
       program:    "$REGISTRY_ID"
       nftProgram: "$NFT_ID"
   - register.js:
       default program = "$REGISTRY_ID"
       default _rpc    = "${RPC_URL:-<SET_ON_MONDAY>}"
       SHORTLIST_ROOT  = "$SHORTLIST_ROOT" (depth 21)

4. Gateway & MCP Server  (repo/mcp/server.js, repo/gateway/public/index.html, M2 serve.mjs / lnsweb.online)
   - repo/mcp/server.js:
       DEFAULT_RPC     = "${RPC_URL:-<SET_ON_MONDAY>}"
       DEFAULT_PROGRAM = "$REGISTRY_ID"
   - M2 gateway env:
       LOGOS_NAMES_RPC="${RPC_URL:-<SET_ON_MONDAY>}"
       LOGOS_NAMES_PROGRAM="$REGISTRY_ID"
       LOGOS_NFT_PROGRAM="$NFT_ID"

5. Android App  (external Android repository)
   - LNS_REGISTRY_PROGRAM_ID = "$REGISTRY_ID"
   - LNS_NFT_PROGRAM_ID      = "$NFT_ID"
   - LEZ_RPC_URL             = "${RPC_URL:-<SET_ON_MONDAY>}"
   - LEZ_CHANNEL_ID          = "${CHANNEL_ID:-<FROM_SEQUENCER>}"
   - LNS_SHORTLIST_ROOT      = "$SHORTLIST_ROOT"

6. LNS Board  (repo/board/core/src/lib.rs — compile-time env, rebuild & redeploy AFTER registry)
   - Build env:
       LOGOS_BOARD_REGISTRY="$REG_HEX"
   - Then deploy the rebuilt board guest and update DEFAULT_BOARD in repo/cli/src/board.rs.

7. Documentation
   - repo/SPEC.md §11 (Live deployment)
   - repo/README.md & repo/docs/INTEGRATING.md
===============================================================================
EOF
