#!/bin/bash
# The Monday deploy, rehearsed end to end on a throwaway local LEZ v0.4.0 chain:
#   boot a standalone sequencer (the testnet kit), deploy.sh --send (both programs, NFT
#   Configure, registry Configure, read back), then what users do: change a price, register a
#   public name and a private name (their waits overlap), check the tied NFT, list + buy with the
#   2.5% split, publish a cid site (as `lns publish` does) and see it in the sites directory, opt
#   out with discover=off and back in, add an on-chain page, make a subname, and upgrade the
#   registry with its header key.
#
#   deploy/rehearse.sh [--fresh] [--skip-private] [--from STEP]
#
# Env (all required unless noted):
#   LEZ_SRC     a LEZ v0.4.0 checkout with the testnet kit in testnet/ (jefdiesel/logos-execution-zone
#               branch testnet-0.4 layout)
#   SEQ_BIN, WALLET_BIN   sequencer_service (features standalone,prove) and wallet (lee/prove) built from it
#   NAMES_CLI, NFT_CLI, IMAGEID, NAMES_ELF, NFT_ELF   as for deploy.sh
#   R           rehearsal home (default deploy/rehearsal; gitignored). --fresh wipes it.
#   PORT        RPC port (default 3940)
# Sends only to 127.0.0.1. Every key it makes lives under $R and is for this chain only.
set -euo pipefail
umask 077
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/.." && pwd)
PY=${PY:-python3}
R=${R:-$HERE/rehearsal}
PORT=${PORT:-3940}
RPC=http://127.0.0.1:$PORT
FRESH=0; PRIV=1; FROM=boot
while [ $# -gt 0 ]; do
  case $1 in --fresh) FRESH=1; shift ;; --skip-private) PRIV=0; shift ;; --from) FROM=$2; shift 2 ;; *) echo "unknown $1" >&2; exit 2 ;; esac
done
: "${LEZ_SRC:?}" "${SEQ_BIN:?}" "${WALLET_BIN:?}" "${NAMES_CLI:?}" "${NFT_CLI:?}" "${IMAGEID:?}" "${NAMES_ELF:?}" "${NFT_ELF:?}"
export NAMES_CLI NFT_CLI IMAGEID NAMES_ELF NFT_ELF
[ $FRESH = 1 ] && { [ -f "$R/chain/run/sequencer.pid" ] && kill "$(cat "$R/chain/run/sequencer.pid")" 2>/dev/null; rm -rf "$R"; }
mkdir -p "$R"; chmod 700 "$R"

T0=$(date +%s)
RESULTS=$R/results.txt; [ "$FROM" = boot ] && : > "$RESULTS"; touch "$RESULTS"   # --from STEP appends
say() { printf '\n######## %s  (+%ss)\n' "$*" "$(( $(date +%s) - T0 ))"; }
ok() { echo "PASS $*" | tee -a "$RESULTS"; }
bad() { echo "FAIL $*" | tee -a "$RESULTS"; FAILED=1; }
FAILED=0
die() { echo "STOP: $*" >&2; bad "$*"; exit 1; }
id_of() { grep -oE "$1/[1-9A-HJ-NP-Za-km-z]{32,44}" | head -1 | cut -d/ -f2; }
quiet() { grep -vE '^(Stored|Transaction data)' | cut -c1-240; }
rpc() { curl -sf -m 15 -X POST "$RPC" -H 'Content-Type: application/json' -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$1\",\"params\":${2:-[]}}"; }
reached() { case $FROM in boot) return 0 ;; esac; [ "$FROM" = "$1" ] && FROM=boot; [ "$FROM" = boot ]; }

# ---- 0. chain -----------------------------------------------------------------------------
KH=$R/chain
kitw() { "$LEZ_SRC/testnet/wallet.sh" --home "$KH" "$@"; }
if reached boot; then
  say "0. boot a standalone LEZ v0.4.0 sequencer on 127.0.0.1:$PORT"
  SEQ_BIN=$SEQ_BIN WALLET_BIN=$WALLET_BIN "$LEZ_SRC/testnet/up.sh" --genesis-key default --home "$KH" --port "$PORT" --block-time 5 2>&1 | tail -12
  rpc getLastBlockId >/dev/null && ok "chain up at $RPC" || die "no chain"
fi
# shellcheck source=/dev/null
. "$KH/kit.env"   # OP = the funded operator account (LEZ's published local test key)

# ---- wallets ------------------------------------------------------------------------------
new_wallet() { # DIR
  [ -f "$1/storage.json" ] && return 0
  mkdir -p "$1"; chmod 700 "$1"
  cat > "$1/wallet_config.json" <<J
{ "gas_limit": 2000000, "fee_headroom_blocks": 5, "seq_block_poll_max_amount": 100, "seq_poll_max_retries": 5,
  "seq_poll_timeout": "12s", "seq_tx_poll_max_blocks": 8, "sequencer": { "sequencer_addr": "$RPC/" } }
J
  LEE_WALLET_HOME_DIR=$1 "$NAMES_CLI" wallet check-health > "$1/created.txt" 2>&1 || true
  chmod 600 "$1/created.txt"
}
export LEE_WALLET_PASSWORD=""
# The names CLI talks to LOGOS_NAMES_RPC (default: the public testnet), not the wallet config's
# sequencer: point it at the local chain before any wallet call.
export LOGOS_NAMES_RPC=$RPC LEZ_NFT_RPC=$RPC
W=$R/deployer; WU1=$R/user-pub; WU2=$R/user-priv; WU3=$R/buyer
for d in "$W" "$WU1" "$WU2" "$WU3"; do new_wallet "$d"; done
ST=$R/ids.env; touch "$ST"
# shellcheck source=/dev/null
. "$ST"
save() { grep -v "^$1=" "$ST" > "$ST.new" 2>/dev/null || true; echo "$1=$2" >> "$ST.new"; mv "$ST.new" "$ST"; printf -v "$1" '%s' "$2"; }
acct() { # VAR WALLET
  [ -n "${!1:-}" ] && return 0
  local id; id=$(LEE_WALLET_HOME_DIR=$2 "$NAMES_CLI" wallet account new public 2>&1 | id_of Public)
  [ -n "$id" ] || die "could not make $1"; save "$1" "$id"
}
pk() { LEE_WALLET_HOME_DIR=$1 "$NAMES_CLI" wallet account get --account-id "Public/$2" --keys 2>/dev/null | sed -nE 's/^(initial key|pk) ([0-9a-f]{64,66}).*/\2/p' | head -1; }
bal() { LEE_WALLET_HOME_DIR=$W "$NAMES_CLI" wallet account get --account-id "Public/$1" 2>/dev/null | sed -nE 's/^Balance:? ?([0-9]+).*/\1/p' | head -1; }
fund() { # WALLET ID AMOUNT (from the operator)
  kitw auth-transfer send --from "Public/$OP" --to-initial-key "$(pk "$1" "$2")" --amount "$3" 2>&1 | quiet | tail -2
}
acct PAYER "$W"; acct ADMIN "$W"; acct TREASURY "$W"
acct U1 "$WU1"; acct U3 "$WU3"
if reached fund; then
  say "1. fund the deployer, admin, treasury and users from the operator"
  fund "$W" "$PAYER" 3000000000; fund "$W" "$ADMIN" 200000000; fund "$W" "$TREASURY" 1000000
  fund "$WU1" "$U1" 200000000; fund "$WU3" "$U3" 200000000
  echo "payer $(bal "$PAYER"), admin $(bal "$ADMIN"), treasury $(bal "$TREASURY"), U1 $(bal "$U1"), U3 $(bal "$U3")"
  [ "$(bal "$PAYER")" -gt 0 ] && ok "accounts funded" || die "funding failed"
fi

# ---- 2. deploy ----------------------------------------------------------------------------
P=$R/params.toml
$PY - "$HERE/params.toml" "$P" "$RPC" "$PAYER" "$ADMIN" "$TREASURY" "$NAMES_CLI" "$NFT_CLI" "$IMAGEID" "$NAMES_ELF" "$NFT_ELF" "$R/deploy-state.env" <<'PY'
import re, sys
src, dst, rpc, payer, admin, treasury, names, nft, imageid, reg_elf, nft_elf, state = sys.argv[1:13]
s = open(src).read()
s = s.replace("rehearsal = false", "rehearsal = true", 1)
def setv(section, key, val, s):
    # Only inside [section]: [confirmed_by_jef] has keys with the same names (rpc_url, admin, ...).
    head = re.search(rf'(?m)^\[{re.escape(section)}\]\s*$', s)
    assert head, f"no [{section}] in params.toml"
    nxt = re.search(r'(?m)^\[', s[head.end():])
    end = head.end() + (nxt.start() if nxt else len(s) - head.end())
    body, n = re.subn(rf'(?m)^{key} = .*$', f'{key} = "{val}"', s[head.end():end], count=1)
    assert n == 1, f"no {key} in [{section}]"
    return s[:head.end()] + body + s[end:]
for sec, k, v in [("network", "rpc_url", rpc), ("accounts", "deployer_payer", payer), ("accounts", "nft_admin", admin),
                  ("accounts", "admin", admin), ("accounts", "treasury", treasury),
                  ("tooling", "names_cli", names), ("tooling", "nft_cli", nft), ("tooling", "imageid_cmd", imageid),
                  ("artifacts", "registry_elf", reg_elf), ("artifacts", "nft_elf", nft_elf),
                  ("tooling", "state_file", state), ("keys", "upgrade_wallet_dir", "rehearsal")]:
    s = setv(sec, k, v, s)
s = s.replace('name = "lez-v0.4-testnet"', 'name = "local-rehearsal"', 1)
open(dst, "w").write(s)
PY
if reached deploy; then
  say "2. deploy.sh dry run, then --send"
  LEE_WALLET_HOME_DIR=$W "$HERE/deploy.sh" --params "$P" accounts | tail -4
  LEE_WALLET_HOME_DIR=$W "$HERE/deploy.sh" --params "$P" --send && ok "deploy.sh: both programs deployed and configured, read back" || die "deploy.sh failed"
fi
# shellcheck source=/dev/null
. "$R/deploy-state.env"
REG=$REG_HEADER; NFTP=$NFT_HEADER
export LOGOS_NAMES_RPC=$RPC LOGOS_NAMES_PROGRAM=$REG
N() { LEE_WALLET_HOME_DIR=$1 "$NAMES_CLI" --program "$REG" "${@:2}"; }
IDS=$($PY "$HERE/lib.py" ids "$REG" "$NFTP")
COLL=$(echo "$IDS" | $PY -c 'import json,sys; print(json.load(sys.stdin)["lns_collection_id"])')

# ---- 3. price change ----------------------------------------------------------------------
PUBNAME=${PUBNAME:-rehearsalpub$(date +%H%M)}
PRIVNAME=${PRIVNAME:-rehearsalpriv$(date +%H%M)}
save PUBNAME "$PUBNAME"; save PRIVNAME "$PRIVNAME"
NEWPRICE=150000   # 8+ characters, 30 days (default 100000)
if reached price; then
  say "3. admin changes one price cell (8+ characters, 30 days -> $NEWPRICE)"
  N "$W" set-price --tier 8+ --term 30d "$NEWPRICE" 2>&1 | quiet | tail -3
  got=$(N "$W" settings | $PY -c 'import json,sys; s=json.load(sys.stdin); p=s["prices"]; print((p.get("tier_8_plus") or p.get("8+"))["30d"] if isinstance(p,dict) else p[2][0])')
  [ "$got" = "$NEWPRICE" ] && ok "set-price: settings show $got" || bad "set-price: settings show $got"
fi

# ---- 4. registrations (public 5-min wait and private 15-min wait run side by side) ---------
if reached register; then
  say "4. register $PUBNAME publicly (U1) and $PRIVNAME privately (U2), in parallel"
  t_before=$(bal "$TREASURY")
  ( N "$WU1" register "$PUBNAME" --owner "Public/$U1" --term 30d --wait > "$R/reg-pub.log" 2>&1; echo "EXIT $?" >> "$R/reg-pub.log" ) &
  PUBPID=$!
  if [ $PRIV = 1 ]; then
    ( set +e
      LEE_WALLET_HOME_DIR=$WU2 "$NAMES_CLI" --program "$REG" setup > "$R/setup-priv.log" 2>&1
      U2P=$(id_of Public < "$R/setup-priv.log"); [ -n "$U2P" ] || U2P=$(LEE_WALLET_HOME_DIR=$WU2 "$NAMES_CLI" wallet account list 2>/dev/null | id_of Public)
      kitw auth-transfer send --from "Public/$OP" --to-initial-key "$(pk "$WU2" "$U2P")" --amount 300000000 >> "$R/setup-priv.log" 2>&1
      LEE_WALLET_HOME_DIR=$WU2 "$NAMES_CLI" --program "$REG" setup >> "$R/setup-priv.log" 2>&1
      LEE_WALLET_HOME_DIR=$WU2 "$NAMES_CLI" --program "$REG" register "$PRIVNAME" --term 30d --wait > "$R/reg-priv.log" 2>&1; echo "EXIT $?" >> "$R/reg-priv.log" ) &
    PRIVPID=$!
  fi
  wait $PUBPID || true
  tail -4 "$R/reg-pub.log"
  owner=$(N "$WU1" resolve "$PUBNAME" 2>/dev/null | $PY -c 'import json,sys; r=json.load(sys.stdin); print(r.get("held_by") or r.get("account") or r.get("owner") or "")' 2>/dev/null || true)
  grep -q "EXIT 0" "$R/reg-pub.log" && ok "public registration of $PUBNAME (5-minute wait) landed; held by $owner" || bad "public registration (see $R/reg-pub.log)"
  t_after=$(bal "$TREASURY")
  [ $((t_after - t_before)) = "$NEWPRICE" ] && ok "treasury got exactly the new price $NEWPRICE" || bad "treasury delta $((t_after - t_before)), expected $NEWPRICE"
  N "$WU1" resolve "$PUBNAME" > "$R/resolve-pub.json" 2>&1 || true
  TOKEN=$($PY -c 'import json,sys; r=json.load(open(sys.argv[1])); n=r.get("nft") or {}; print(n.get("token_id",""))' "$R/resolve-pub.json" 2>/dev/null || true)
  if [ -n "$TOKEN" ]; then
    "$NFT_CLI" --rpc "$RPC" --program "$NFTP" --json info --token-id "$TOKEN" > "$R/nft-pub.json" 2>&1 || true
    if grep -q "$COLL" "$R/nft-pub.json"; then ok "tied NFT $TOKEN minted in the LNS collection $COLL (= lib.py lns_collection_id)"
    else bad "NFT $TOKEN: collection not $COLL (see $R/nft-pub.json)"; fi
    grep -q 'lns://deed/<name>' "$R/nft-pub.json" && ok "deed metadata names lns://deed/<name>" || bad "deed metadata (see $R/nft-pub.json)"
  else bad "resolve shows no NFT token for $PUBNAME (see $R/resolve-pub.json)"; fi
fi

# ---- 5. list and buy ----------------------------------------------------------------------
PRICE=1000000
if reached market; then
  say "5. U1 lists $PUBNAME for $PRICE; U3 buys it"
  N "$WU1" list "$PUBNAME" "$PRICE" --pay-to "Public/$U1" --public-name 2>&1 | quiet | tail -3
  N "$WU3" market --json > "$R/market.json" 2>&1 || true
  grep -q "$PUBNAME" "$R/market.json" && ok "listing shows in the market directory" || bad "market directory (see $R/market.json)"
  s_before=$(bal "$U1"); t_before=$(bal "$TREASURY")
  N "$WU3" buy "$PUBNAME" --buyer "Public/$U3" 2>&1 | quiet | tail -3
  s_after=$(bal "$U1"); t_after=$(bal "$TREASURY")
  [ $((s_after - s_before)) = 975000 ] && ok "seller got 975000 (97.5%)" || bad "seller delta $((s_after - s_before)), expected 975000"
  [ $((t_after - t_before)) = 25000 ] && ok "treasury got 25000 (2.5%)" || bad "treasury delta $((t_after - t_before)), expected 25000"
  N "$WU3" resolve "$PUBNAME" > "$R/resolve-bought.json" 2>&1 || true
  grep -q "$U3" "$R/resolve-bought.json" && ok "$PUBNAME now resolves to the buyer" || bad "after the buy (see $R/resolve-bought.json)"
fi

# ---- 6. sites: a cid site (what `lns publish` writes), discover=off, an on-chain page, a subname
CID=${CID:-zDvZRwzkxrehearsal1cidsite0000000000000000000000}
sites_has() { N "$WU3" sites --json > "$R/sites.json" 2>&1 || true; grep -q "\"name\": \"$1\"" "$R/sites.json"; }
if reached site; then
  say "6. U3 publishes a cid site on $PUBNAME the way lns publish does (names profile --cid), then discover=off, then a page"
  N "$WU3" profile "$PUBNAME" --cid "$CID" --display "Rehearsal site" 2>&1 | quiet | tail -2
  N "$WU3" resolve "$PUBNAME" > "$R/resolve-cid.json" 2>&1 || true
  sites_has "$PUBNAME" && ok "cid site (no on-chain page) listed in the sites directory" || bad "cid site not in the sites directory (see $R/sites.json)"
  grep -q '"title": "Rehearsal site"' "$R/sites.json" && ok "directory entry: title from the display text" || bad "directory entry title (see $R/sites.json)"
  N "$WU3" profile "$PUBNAME" --no-discover 2>&1 | quiet | tail -2
  sites_has "$PUBNAME" && bad "discover=off: still listed (see $R/sites.json)" || ok "discover=off removes the site from the directory"
  N "$WU3" profile "$PUBNAME" --discover 2>&1 | quiet | tail -2
  sites_has "$PUBNAME" && ok "clearing discover=off lists the cid site again" || bad "re-list after --discover (see $R/sites.json)"
  printf '<!doctype html><title>%s</title><h1>Rehearsal site</h1><p>Published on a local LEZ v0.4.0 chain.</p>\n' "$PUBNAME" > "$R/index.html"
  N "$WU3" set-page "$PUBNAME" "$R/index.html" 2>&1 | quiet | tail -3
  N "$WU3" resolve "$PUBNAME" > "$R/resolve-site.json" 2>&1 || true
  N "$WU3" sites > "$R/sites.txt" 2>&1 || true
  sites_has "$PUBNAME" && ok "on-chain page published; site still listed once" || bad "sites directory after set-page (see $R/sites.json)"
  N "$WU3" subname create "blog.$PUBNAME" --owner "Public/$U3" --permanent 2>&1 | quiet | tail -2 || true
  N "$WU3" resolve "blog.$PUBNAME" > "$R/resolve-sub.json" 2>&1 && ok "subname blog.$PUBNAME resolves" || bad "subname (see $R/resolve-sub.json)"
fi

# ---- 7. upgrade with the header key -------------------------------------------------------
if reached upgrade; then
  say "7. upgrade the registry in place with its header key (same ELF: proves the key, keeps the id)"
  n=$(( ($(wc -c < "$NAMES_ELF") + 98303) / 98304 )); segs=()
  for i in $(seq 1 "$n"); do id=$(LEE_WALLET_HOME_DIR=$W "$NAMES_CLI" wallet account new public 2>&1 | id_of Public); segs+=("Public/$id"); done
  LEE_WALLET_HOME_DIR=$W "$NAMES_CLI" wallet program-loader update --elf "$NAMES_ELF" --header "Public/$REG" --segments "${segs[@]}" --payer "Public/$PAYER" 2>&1 | quiet | tail -3
  have=$($PY "$HERE/lib.py" header-image "$RPC" "$REG" || true)
  want=$("$IMAGEID" "$NAMES_ELF" | cut -d' ' -f1)
  [ "$have" = "$want" ] && ok "registry header still holds image $have after update" || bad "header image after update: $have"
  N "$WU3" resolve "$PUBNAME" > /dev/null 2>&1 && N "$W" settings > /dev/null 2>&1 && ok "names and settings intact after the upgrade" || bad "after the upgrade"
fi

# ---- 8. private registration finishes -----------------------------------------------------
if [ $PRIV = 1 ] && [ -n "${PRIVPID:-}" ]; then
  say "8. wait for the private registration of $PRIVNAME (15-minute wait + proof)"
  wait "$PRIVPID" || true
  tail -4 "$R/reg-priv.log"
  if grep -q "EXIT 0" "$R/reg-priv.log" && LEE_WALLET_HOME_DIR=$WU2 "$NAMES_CLI" --program "$REG" resolve "$PRIVNAME" > "$R/resolve-priv.json" 2>&1; then
    ok "private registration of $PRIVNAME landed (proof: $( [ "${RISC0_DEV_MODE:-0}" = 1 ] && echo 'DEV MODE, fake' || echo real ))"
  else bad "private registration (see $R/reg-priv.log)"; fi
fi

say "results ($(( ($(date +%s) - T0) / 60 )) min)"
cat "$RESULTS"
echo "registry Public/$REG, lez_nft Public/$NFTP, chain $RPC, home $R"
exit $FAILED
