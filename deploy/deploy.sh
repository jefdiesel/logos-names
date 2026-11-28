#!/bin/bash
# Deploys lez_nft v3 and the LNS registry (names_core v8) on a LEZ v0.4.0 chain, then
# configures them: the NFT program first (it reserves the LNS collection for the registry),
# then the registry (which binds the NFT program for good). Resumable: every account and id is
# kept in the state file the moment it exists, and each step reads the chain before it sends,
# so a rerun picks up where it stopped and never deploys twice.
#
#   deploy/deploy.sh --params FILE [--send] [STEP ...]
#
# STEP: accounts deploy-nft deploy-registry configure-nft configure-registry verify
#       (default: all of them, in that order). Without --send nothing is sent: each step prints
#       the command it would run (the dry run).
#
# Two kinds of params file:
#   live       deploy/params.toml. A send refuses unless every value in LIVE_REQUIRED_CONFIRMATIONS
#              (lib.py) is true in [confirmed_by_jef], the image ids match expected-image-ids.txt,
#              the RPC is not a placeholder, and the chain's channel id is the expected one.
#   rehearsal  `rehearsal = true` (written by deploy/rehearse.sh). Sends only to a loopback RPC.
#
# Keys: none are read from or written to the params file. The deployer wallet is the folder in
# LEE_WALLET_HOME_DIR (password in LEE_WALLET_PASSWORD, or the wallet asks). It ends up holding
# the payer, both program header accounts (the program ids, and their UPGRADE keys: back this
# folder up) and the segment accounts. The admin/treasury/nft_admin accounts may live in it too
# (the registry Configure is signed by the registry header; the NFT Configure by nft_admin).
#
# Env: NAMES_CLI, NFT_CLI, IMAGEID (lns_imageid), NAMES_ELF, NFT_ELF override [tooling]/[artifacts].
set -euo pipefail
umask 077
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/.." && pwd)
PY=${PY:-python3}
LIB="$PY $HERE/lib.py"

PARAMS=""; SEND=0; STEPS=()
while [ $# -gt 0 ]; do
  case $1 in
    --params) PARAMS=$2; shift 2 ;;
    --send) SEND=1; shift ;;
    -h|--help) sed -n '2,26p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    accounts|deploy-nft|deploy-registry|configure-nft|configure-registry|verify) STEPS+=("$1"); shift ;;
    *) echo "unknown argument: $1 (see --help)" >&2; exit 2 ;;
  esac
done
[ -n "$PARAMS" ] || { echo "--params FILE is required" >&2; exit 2; }
[ ${#STEPS[@]} -gt 0 ] || STEPS=(accounts deploy-nft deploy-registry configure-nft configure-registry verify)

say() { printf '\n== %s\n' "$*"; }
info() { printf '   %s\n' "$*"; }
die() { printf 'STOP: %s\n' "$*" >&2; exit 1; }
id_of() { grep -oE "$1/[1-9A-HJ-NP-Za-km-z]{32,44}" | head -1 | cut -d/ -f2; }
quiet() { grep -vE '^(Stored|Transaction data)' | cut -c1-220; }

# ---- params -------------------------------------------------------------------------------
VFLAG=--dry-run; [ $SEND = 1 ] && VFLAG=
V=$($LIB validate "$PARAMS" "$REPO" $VFLAG) || { echo "$V" | $PY -c 'import json,sys; d=json.load(sys.stdin); [print("  error:", e) for e in d["errors"]]' 2>/dev/null || echo "$V"; die "params check failed ($PARAMS)"; }
p() { echo "$V" | $PY -c "import json,sys; d=json.load(sys.stdin); v=$1; print(v if not isinstance(v,(list,dict)) else json.dumps(v))"; }
echo "$V" | $PY -c 'import json,sys; [print("  warning:", w) for w in json.load(sys.stdin)["warnings"]]'
REHEARSAL=$(p 'd["rehearsal"]')
RPC=$(p 'd["network"].get("rpc_url","")')
NET=$(p 'd["network"].get("name","net")')
ADMIN=$(p 'd["accounts"].get("admin","")'); TREASURY=$(p 'd["accounts"].get("treasury","")')
NFT_ADMIN=$(p 'd["accounts"].get("nft_admin","")'); PAYER=$(p 'd["accounts"].get("deployer_payer","")')
MODE=$(p 'd["mode_name"]'); DEPOSITS=$(p '",".join(str(x) for x in d["deposits"])')
PRICES=$(p '",".join(str(x) for row in d["prices"] for x in row)'); PREMIUM=$(p 'd["premium_start"]')
REG_FUND=$(p 'd["raw_accounts"].get("registry_gas_fund",10000000)'); TREASURY_FUND=$(p 'd["raw_accounts"].get("treasury_init_fund",1000000)')
CHANNEL=$(p 'd["network"].get("expected_channel_id","")')
rel() { case $1 in /*) echo "$1" ;; *) echo "$(cd "$(dirname "$PARAMS")" && pwd)/$1" ;; esac; }
NAMES_CLI=${NAMES_CLI:-$(rel "$(p 'd["tooling"].get("names_cli","")')")}
NFT_CLI=${NFT_CLI:-$(rel "$(p 'd["tooling"].get("nft_cli","")')")}
IMAGEID=${IMAGEID:-$(rel "$(p 'd["tooling"].get("imageid_cmd","")')")}
NAMES_ELF=${NAMES_ELF:-$(rel "$(p 'd["artifacts"].get("registry_elf","")')")}
NFT_ELF=${NFT_ELF:-$(rel "$(p 'd["artifacts"].get("nft_elf","")')")}
STATE=${STATE:-$(rel "$(p 'd["tooling"].get("state_file","state/deploy-state.env")')")}
[ -n "${LEE_WALLET_HOME_DIR:-}" ] || die "set LEE_WALLET_HOME_DIR to the deployer wallet folder"

if [ $SEND = 1 ]; then
  if [ "$REHEARSAL" = True ]; then
    case $RPC in http://127.0.0.1:*|http://localhost:*|http://\[::1\]:*) ;; *) die "a rehearsal sends only to a loopback RPC, not $RPC" ;; esac
  fi
fi
for b in "$NAMES_CLI" "$NFT_CLI" "$IMAGEID"; do [ -x "$b" ] || die "not executable: $b"; done
for f in "$NAMES_ELF" "$NFT_ELF"; do [ -f "$f" ] || die "no guest ELF at $f"; done
mkdir -p "$(dirname "$STATE")"; touch "$STATE"; chmod 600 "$STATE"
# shellcheck source=/dev/null
. "$STATE"
save() { grep -v "^$1=" "$STATE" > "$STATE.new" 2>/dev/null || true; echo "$1=$2" >> "$STATE.new"; mv "$STATE.new" "$STATE"; printf -v "$1" '%s' "$2"; }

export LOGOS_NAMES_RPC=$RPC LEZ_NFT_RPC=$RPC
N() { "$NAMES_CLI" "$@"; }
NFT() { "$NFT_CLI" --rpc "$RPC" "$@"; }
rpc() { curl -sf -m 15 -X POST "$RPC" -H 'Content-Type: application/json' -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$1\",\"params\":${2:-[]}}"; }
balance() { N wallet account get --account-id "Public/$1" 2>/dev/null | sed -nE 's/^Balance:? ?([0-9]+).*/\1/p' | head -1; }
# The account's initial key, for crediting a public account the chain has not seen yet. LEZ v0.4.0
# prints it as `initial key <hex>` and takes it as `--to-initial-key` (rc3 printed `pk <hex>`).
pk_of() { N wallet account get --account-id "Public/$1" --keys 2>/dev/null | sed -nE 's/^(initial key|pk) ([0-9a-f]{64,66}).*/\2/p' | head -1; }
header_image() { $LIB header-image "$RPC" "$1" 2>/dev/null || true; }
segments_needed() { local size; size=$(wc -c < "$1" | tr -d ' '); echo $(( (size + 98303) / 98304 )); }
run() { # print, and send only with --send
  info "\$ $*"
  [ $SEND = 1 ] || return 0
  "$@" 2>&1 | quiet
  return "${PIPESTATUS[0]}"
}

# ---- preflight ----------------------------------------------------------------------------
say "preflight: $NET at $RPC ($([ "$REHEARSAL" = True ] && echo rehearsal || echo LIVE)$([ $SEND = 1 ] && echo ', sending' || echo ', dry run: nothing is sent'))"
rpc getLastBlockId >/dev/null || die "the sequencer at $RPC does not answer"
info "height $(rpc getLastBlockId | $PY -c 'import json,sys; print(json.load(sys.stdin)["result"])')"
if [ -n "$CHANNEL" ]; then
  got=$(rpc getChannelId | $PY -c 'import json,sys; print(json.load(sys.stdin).get("result",""))' 2>/dev/null || true)
  [ "$got" = "$CHANNEL" ] || die "channel id $got is not the expected $CHANNEL: wrong chain"
elif [ "$REHEARSAL" != True ] && [ $SEND = 1 ]; then
  die "[network].expected_channel_id is empty: fill it from the official chain before a live send"
fi
read -r NAMES_IMAGE NAMES_SHA _ < <("$IMAGEID" "$NAMES_ELF")
read -r NFT_IMAGE NFT_SHA _ < <("$IMAGEID" "$NFT_ELF")
info "registry guest $NAMES_ELF: image $NAMES_IMAGE sha256 $NAMES_SHA"
info "nft guest      $NFT_ELF: image $NFT_IMAGE sha256 $NFT_SHA"
if [ "$REHEARSAL" != True ]; then
  EXP=$(rel "$(p 'd["artifacts"].get("expected_image_ids_file","expected-image-ids.txt")')")
  if [ -f "$EXP" ]; then
    want_r=$(sed -nE 's/^registry_image_id *= *"?([0-9a-f]{64})"?.*/\1/p' "$EXP"); want_n=$(sed -nE 's/^nft_image_id *= *"?([0-9a-f]{64})"?.*/\1/p' "$EXP")
    [ "$want_r" = "$NAMES_IMAGE" ] && [ "$want_n" = "$NFT_IMAGE" ] || { [ $SEND = 1 ] && die "image ids differ from $EXP (registry $want_r, nft $want_n)"; info "WARNING: image ids differ from $EXP"; }
  else
    [ $SEND = 1 ] && die "no $EXP: put the reproducible build's image ids there first"
    info "WARNING: no $EXP yet (needed for a live send)"
  fi
fi
[ -n "$PAYER" ] || die "[accounts].deployer_payer is empty"
info "payer Public/$PAYER balance $(balance "$PAYER" || echo '?')"

# ---- steps --------------------------------------------------------------------------------
new_account() { # VAR: make a public account in the deployer wallet once, keep it in the state file
  local var=$1
  [ -n "${!var:-}" ] && return 0
  if [ $SEND = 0 ]; then info "would make a public account for $var"; return 0; fi
  local id; id=$(N wallet account new public 2>&1 | id_of Public)
  [ -n "$id" ] || die "could not make $var"
  save "$var" "$id"; info "$var = Public/$id"
}

step_accounts() {
  say "1. accounts in the deployer wallet (kept in $STATE)"
  new_account NFT_HEADER; new_account REG_HEADER
  local i n
  n=$(segments_needed "$NFT_ELF"); for i in $(seq 1 "$n"); do new_account "NFT_SEG$i"; done; save NFT_SEGS "$n"
  n=$(segments_needed "$NAMES_ELF"); for i in $(seq 1 "$n"); do new_account "REG_SEG$i"; done; save REG_SEGS "$n"
  info "NFT program id (= its header, upgrade key)      Public/${NFT_HEADER:-<new>}  ($NFT_SEGS segments)"
  info "registry program id (= its header, upgrade key) Public/${REG_HEADER:-<new>}  ($REG_SEGS segments)"
}

deploy_one() { # LABEL ELF HEADER SEG_PREFIX COUNT WANT_IMAGE
  local label=$1 elf=$2 header=$3 pre=$4 n=$5 want=$6 have segs=() i v
  have=$(header_image "$header")
  if [ -n "$have" ]; then
    [ "$have" = "$want" ] || die "$label header Public/$header holds image $have, not $want"
    info "$label already deployed at Public/$header (image $have)"; return 0
  fi
  for i in $(seq 1 "$n"); do v=$pre$i; segs+=("Public/${!v:-<seg$i>}"); done
  # Upgradeable (Jef, 2026-11-28): no --immutable; the header key in this wallet can `update`.
  run N wallet program-loader deploy --elf "$elf" --header "Public/$header" --segments "${segs[@]}" --payer "Public/$PAYER" || die "$label deploy failed"
  [ $SEND = 1 ] || return 0
  have=$(header_image "$header")
  [ "$have" = "$want" ] || die "$label header holds '$have' after deploy, expected $want"
  info "deployed $label: Public/$header, image $have"
}
step_deploy_nft() { say "2. deploy lez_nft v3"; deploy_one "lez_nft" "$NFT_ELF" "${NFT_HEADER:-<nft-header>}" NFT_SEG "${NFT_SEGS:-1}" "$NFT_IMAGE"; }
step_deploy_registry() { say "3. deploy the registry"; deploy_one "registry" "$NAMES_ELF" "${REG_HEADER:-<reg-header>}" REG_SEG "${REG_SEGS:-1}" "$NAMES_IMAGE"; }

nft_config_set() { # the NFT Config account exists once Configure has landed
  local cfg; cfg=$($LIB ids "$REG_HEADER" "$NFT_HEADER" | $PY -c 'import json,sys; print(json.load(sys.stdin)["nft_config"])')
  [ -n "$($PY -c "
import sys; sys.path.insert(0, '$HERE'); import lib
print(lib.get_actor_state('$RPC', '$cfg', '$NFT_HEADER').hex())" 2>/dev/null)" ]
}

step_configure_nft() {
  say "4. configure lez_nft: admin Public/$NFT_ADMIN, authorise the registry, reserve its LNS collection"
  [ -n "$NFT_ADMIN" ] || die "[accounts].nft_admin is empty"
  local coll; coll=$($LIB ids "${REG_HEADER:-11111111111111111111111111111111}" "${NFT_HEADER:-11111111111111111111111111111111}" | $PY -c 'import json,sys; print(json.load(sys.stdin)["lns_collection_id"])')
  if [ $SEND = 1 ] && nft_config_set; then info "lez_nft already configured"; return 0; fi
  # The first Configure makes its signer the admin: send it right after the deploy.
  run NFT --program "${NFT_HEADER:-<nft>}" configure --from "Public/$NFT_ADMIN" --admin "Public/$NFT_ADMIN" --authority "${REG_HEADER:-<reg>}:$coll" || die "NFT configure failed"
  [ $SEND = 1 ] || return 0
  nft_config_set || die "lez_nft Config account is still empty after Configure"
}

step_configure_registry() {
  say "5. configure the registry (signed by its own header account; binds the NFT program for good)"
  export LOGOS_NAMES_PROGRAM=${REG_HEADER:-}
  if [ $SEND = 1 ] && N --program "$REG_HEADER" settings 2>/dev/null | $PY -c 'import json,sys; s=json.load(sys.stdin); sys.exit(0 if s.get("admin") else 1)' 2>/dev/null; then
    info "the registry already has settings"; return 0
  fi
  [ $SEND = 1 ] || { info "would fund Public/<reg-header> with $REG_FUND for its Configure gas"; }
  if [ $SEND = 1 ]; then
    local hb; hb=$(balance "$REG_HEADER"); hb=${hb:-0}
    if [ "$hb" -lt $((REG_FUND / 2)) ]; then
      run N wallet auth-transfer send --from "Public/$PAYER" --to-initial-key "$(pk_of "$REG_HEADER")" --amount "$REG_FUND" || die "funding the registry header failed"
    fi
    # Credits to a public account LEZ 0.4 has never seen revert: admit the treasury first.
    local tb; tb=$(balance "$TREASURY"); if [ -z "$tb" ] || [ "$tb" = 0 ]; then
      local tpk; tpk=$(pk_of "$TREASURY")
      if [ -n "$tpk" ]; then run N wallet auth-transfer send --from "Public/$PAYER" --to-initial-key "$tpk" --amount "$TREASURY_FUND"
      elif [ -n "${TREASURY_PUBLIC_KEY:-}" ]; then run N wallet auth-transfer send --from "Public/$PAYER" --to-initial-key "$TREASURY_PUBLIC_KEY" --amount "$TREASURY_FUND"
      else die "treasury Public/$TREASURY is new to the chain and not in this wallet: export TREASURY_PUBLIC_KEY=<its initial key, as 'wallet account get --keys' prints it>"; fi
    fi
  fi
  run N --program "${REG_HEADER:-<reg>}" configure --signer "${REG_HEADER:-<reg>}" --admin "$ADMIN" --treasury "$TREASURY" \
    --mode "$MODE" --deposits "$DEPOSITS" --prices "$PRICES" --premium-start "$PREMIUM" --nft-program "${NFT_HEADER:-<nft>}" || die "registry configure failed"
}

step_verify() {
  say "6. read everything back"
  [ $SEND = 1 ] || { info "(dry run: nothing to read back)"; return 0; }
  local hr hn; hr=$(header_image "$REG_HEADER"); hn=$(header_image "$NFT_HEADER")
  info "registry Public/$REG_HEADER image $hr $([ "$hr" = "$NAMES_IMAGE" ] && echo OK || echo MISMATCH)"
  info "lez_nft  Public/$NFT_HEADER image $hn $([ "$hn" = "$NFT_IMAGE" ] && echo OK || echo MISMATCH)"
  N --program "$REG_HEADER" settings > "$(dirname "$STATE")/settings.json"
  $PY - "$(dirname "$STATE")/settings.json" "$ADMIN" "$TREASURY" "$MODE" "$PRICES" "$PREMIUM" "$NFT_HEADER" <<'PY'
import json, sys
s = json.load(open(sys.argv[1])); admin, treasury, mode, prices, premium, nft = sys.argv[2:8]
flat = []
pr = s.get("prices")
if isinstance(pr, dict):
    for tier, alt in (("tier_5", "5"), ("tier_6_7", "6-7"), ("tier_8_plus", "8+")):
        row = pr.get(tier) or pr.get(alt) or {}
        flat += [str(row.get(t)) for t in ("30d", "1y", "5y", "permanent")]
elif isinstance(pr, list):
    flat = [str(x) for row in pr for x in (row if isinstance(row, list) else [row])]
checks = {
    "admin": s.get("admin") == admin, "treasury": s.get("treasury") == treasury,
    "mode": s.get("mode") == mode, "prices": flat == prices.split(","),
    "premium_start": str(s.get("premium_start")) == premium, "nft_program": s.get("nft_program") == nft,
}
for k, ok in checks.items(): print(f"   settings.{k}: {'OK' if ok else 'MISMATCH'}")
if not all(checks.values()): print(json.dumps(s, indent=2)); sys.exit(1)
PY
  [ "$hr" = "$NAMES_IMAGE" ] && [ "$hn" = "$NFT_IMAGE" ] || die "image mismatch"
  nft_config_set || die "lez_nft is not configured"
  local out; out=$(dirname "$STATE")/deployed-ids.env
  { echo "# $NET, written by deploy.sh $(date -u '+%F %T') UTC. Public ids only."
    echo "LNS_REGISTRY_PROGRAM_ID=$REG_HEADER"; echo "LNS_NFT_PROGRAM_ID=$NFT_HEADER"
    echo "LNS_REGISTRY_IMAGE_ID=$NAMES_IMAGE"; echo "LNS_NFT_IMAGE_ID=$NFT_IMAGE"
    echo "LEZ_RPC_URL=$RPC"; echo "LNS_TREASURY=$TREASURY"; echo "LNS_ADMIN=$ADMIN"
    $LIB ids "$REG_HEADER" "$NFT_HEADER" | $PY -c 'import json,sys; d=json.load(sys.stdin); print("LNS_COLLECTION_ID="+d["lns_collection_id"]); print("LNS_SETTINGS_ACCOUNT="+d["registry_settings"]); print("LNS_NFT_CONFIG_ACCOUNT="+d["nft_config"])'
  } > "$out"
  chmod 644 "$out"; info "wrote $out"; sed 's/^/   /' "$out"
}

for s in "${STEPS[@]}"; do
  case $s in
    accounts) step_accounts ;; deploy-nft) step_deploy_nft ;; deploy-registry) step_deploy_registry ;;
    configure-nft) step_configure_nft ;; configure-registry) step_configure_registry ;; verify) step_verify ;;
  esac
done
say "done ($([ $SEND = 1 ] && echo sent || echo 'dry run, nothing sent'))"
