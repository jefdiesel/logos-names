#!/bin/bash
# After deploy.sh: place the held names (locked 1-4 character and reserved names only the admin
# may register), hand each to its owner, and run one public smoke registration.
#
#   deploy/post-deploy.sh --params FILE [--send] [--skip-held] [--skip-smoke]
#
# Without --send it prints what it would do. Reads deploy.sh's state file for the program ids.
# The admin's key must be in LEE_WALLET_HOME_DIR (the deployer wallet). Held names are registered
# permanent by the admin (public commit, 5-minute wait each, one at a time so the admin's nonce
# never collides), then `names transfer`red to [[held_names]].owner when one is set; a name with
# an empty owner stays with the admin. In fee mode the admin pays the tier price to the treasury
# unless admin == treasury.
#
# Smoke: SMOKE_WALLET (a separate wallet folder with a funded public account) registers
# [post_deploy].smoke_public_name for smoke_term and the treasury's balance must rise by the price.
set -euo pipefail
umask 077
HERE=$(cd "$(dirname "$0")" && pwd); REPO=$(cd "$HERE/.." && pwd); PY=${PY:-python3}
PARAMS=""; SEND=0; HELD=1; SMOKE=1
while [ $# -gt 0 ]; do
  case $1 in --params) PARAMS=$2; shift 2 ;; --send) SEND=1; shift ;; --skip-held) HELD=0; shift ;; --skip-smoke) SMOKE=0; shift ;;
    *) echo "unknown argument $1" >&2; exit 2 ;; esac
done
[ -n "$PARAMS" ] || { echo "--params FILE is required" >&2; exit 2; }
say() { printf '\n== %s\n' "$*"; }; info() { printf '   %s\n' "$*"; }; die() { printf 'STOP: %s\n' "$*" >&2; exit 1; }
id_of() { grep -oE "$1/[1-9A-HJ-NP-Za-km-z]{32,44}" | head -1 | cut -d/ -f2; }
VFLAG=--dry-run; [ $SEND = 1 ] && VFLAG=
V=$($PY "$HERE/lib.py" validate "$PARAMS" "$REPO" $VFLAG) || die "params check failed: run deploy.sh first"
p() { echo "$V" | $PY -c "import json,sys; d=json.load(sys.stdin); v=$1; print(v if not isinstance(v,(list,dict)) else json.dumps(v))"; }
rel() { case $1 in /*) echo "$1" ;; *) echo "$(cd "$(dirname "$PARAMS")" && pwd)/$1" ;; esac; }
RPC=$(p 'd["network"].get("rpc_url","")'); ADMIN=$(p 'd["accounts"].get("admin","")'); TREASURY=$(p 'd["accounts"].get("treasury","")')
NAMES_CLI=${NAMES_CLI:-$(rel "$(p 'd["tooling"].get("names_cli","")')")}
STATE=${STATE:-$(rel "$(p 'd["tooling"].get("state_file","state/deploy-state.env")')")}
[ -f "$STATE" ] || die "no $STATE: run deploy.sh --send first"
# shellcheck source=/dev/null
. "$STATE"
[ -n "${REG_HEADER:-}" ] || die "no registry id in $STATE"
export LOGOS_NAMES_RPC=$RPC LOGOS_NAMES_PROGRAM=$REG_HEADER
N() { "$NAMES_CLI" --program "$REG_HEADER" "$@"; }
holder() { N resolve "$1" 2>/dev/null | $PY -c 'import json,sys; r=json.load(sys.stdin); print(r.get("account") or r.get("holder") or ("registered" if r.get("name") else ""))' 2>/dev/null || true; }
bal() { N wallet account get --account-id "Public/$1" 2>/dev/null | sed -nE 's/^Balance:? ?([0-9]+).*/\1/p' | head -1; }
run() { info "\$ $*"; [ $SEND = 1 ] || return 0; "$@" 2>&1 | grep -vE '^(Stored|Transaction data)' | cut -c1-220; return "${PIPESTATUS[0]}"; }

if [ $HELD = 1 ]; then
  say "held names (admin Public/$ADMIN places them permanent, then hands them over)"
  echo "$V" | $PY -c 'import json,sys; [print(h.get("name",""), h.get("owner","") or "-", h.get("term","permanent")) for h in json.load(sys.stdin)["held_names"]]' |
  while read -r name owner term; do
    [ -n "$name" ] || continue
    cur=$(holder "$name")
    if [ -z "$cur" ]; then
      run N register "$name" --owner "Public/$ADMIN" --term "$term" --wait || die "placing $name failed"
    else info "$name is registered already ($cur)"; fi
    if [ "$owner" != - ] && [ "$owner" != PLACEHOLDER ]; then
      cur=$(holder "$name")
      if [ $SEND = 0 ] || [ "$cur" = "$ADMIN" ] || [ "$cur" = registered ]; then run N transfer "$name" "$owner" || die "handing $name to $owner failed"
      else info "$name is not held by the admin any more ($cur): not transferring"; fi
    else info "$name has no owner in params: it stays with the admin"; fi
  done
fi

if [ $SMOKE = 1 ]; then
  NAME=$(p 'd["post_deploy"].get("smoke_public_name","")'); TERM_=$(p 'd["post_deploy"].get("smoke_term","30d")')
  say "smoke: register $NAME publicly for $TERM_ from SMOKE_WALLET"
  [ -n "${SMOKE_WALLET:-}" ] || { info "SMOKE_WALLET not set: skipping"; exit 0; }
  U=$(LEE_WALLET_HOME_DIR=$SMOKE_WALLET "$NAMES_CLI" wallet account list 2>/dev/null | id_of Public)
  [ -n "$U" ] || die "no public account in $SMOKE_WALLET"
  before=$(bal "$TREASURY")
  LEE_WALLET_HOME_DIR=$SMOKE_WALLET run N register "$NAME" --owner "Public/$U" --term "$TERM_" --wait || die "smoke registration failed"
  [ $SEND = 1 ] || exit 0
  after=$(bal "$TREASURY")
  info "treasury: $before -> $after (+$((after - before)))"
  LEE_WALLET_HOME_DIR=$SMOKE_WALLET N resolve "$NAME"
fi
