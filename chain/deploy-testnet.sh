#!/bin/bash
# Deploys registry v3 (anonymous holders) to the public LEZ testnet, configures
# it, and registers jef and logos held by a private account. Resumable: state
# lives in $V; finished steps are skipped on a re-run.
set -uo pipefail
export PATH=$HOME/.cargo/bin:$HOME/.risc0/bin:/opt/homebrew/bin:$PATH
DEV=/Volumes/exos162/logos/dev
N=$DEV/target-names/release/names
BIN=$DEV/logos_names-ci-506bfbfd.bin   # CI guest for commit 6a08124
SHA=506bfbfd0c4dd388c33c8169f6dd24a14620464b9f583aa15042825bbfc3e9c4
SEGMENTS=${SEGMENTS:-4}
PRICES=10000,100000,400000
export LEE_WALLET_HOME_DIR=$DEV/wallet-testnet
export LOGOS_NAMES_RPC=https://testnet.lez.logos.co   # names wallet ignores wallet_config without this
export LOGOS_NAMES_PROGRAM_ELF=$BIN
V=$LEE_WALLET_HOME_DIR/v3; mkdir -p $V
PAYER=$(cat $LEE_WALLET_HOME_DIR/deploy-account.txt); OWNER=${PAYER#Public/}
step() { echo; echo "=== $(date +%T) $*"; }
fail() { echo "FAIL: $*"; exit 1; }
quiet() { grep -vE "^(Stored|Transaction data)|^ |^\{|^\}|Received new acc" | cut -c1-200; }
acct() { $N wallet account new "$1" 2>&1 | grep -oE "(Public|Private)/[1-9A-HJ-NP-Za-km-z]{32,44}" | head -1; }
echo "$SHA  $BIN" | shasum -a 256 -c - >/dev/null || fail "guest binary does not match the CI build"
deployed() {
  curl -s -m 20 $LOGOS_NAMES_RPC -H 'Content-Type: application/json' \
    -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"getAccount\",\"params\":[\"$1\"]}" \
    | python3 -c 'import json,sys; sys.exit(0 if any(json.load(sys.stdin)["result"]["data"]["shards"].values()) else 1)'
}

if [ ! -f $V/program-id.txt ]; then
  step "deploy ($SEGMENTS segments)"
  [ -f $V/program-accounts.txt ] || for i in $(seq 0 $SEGMENTS); do acct public; done > $V/program-accounts.txt
  IDS=($(cat $V/program-accounts.txt))
  [ ${#IDS[@]} -eq $((SEGMENTS + 1)) ] || fail "expected $((SEGMENTS + 1)) program accounts, got ${#IDS[@]}"
  OUT=$($N wallet program-loader deploy --elf $BIN --header ${IDS[0]} --segments "${IDS[@]:1}" --payer $PAYER 2>&1 | quiet)
  echo "$OUT" | tail -5
  echo "$OUT" | grep -q "splits into" && fail "wrong segment count: rm $V/program-accounts.txt and rerun with SEGMENTS=<n>"
  for t in $(seq 1 40); do deployed ${IDS[0]#Public/} && break; sleep 10; done
  deployed ${IDS[0]#Public/} || fail "program did not appear on chain"
  echo "${IDS[0]#Public/}" > $V/program-id.txt
fi
export LOGOS_NAMES_PROGRAM=$(cat $V/program-id.txt)
echo "program: $LOGOS_NAMES_PROGRAM"

step "configure"
if ! $N settings 2>/dev/null | grep -q "$OWNER"; then
  $N configure --admin $OWNER --treasury $OWNER --prices $PRICES 2>&1 | quiet
fi
$N settings 2>&1 | grep -q "$OWNER" || fail "registry admin is not the deploy account; stop and look"

step "private holder account"
[ -f $V/holder.txt ] || acct private > $V/holder.txt
P=$(cat $V/holder.txt); [ -n "$P" ] || fail "no private account"
echo "P=$P"
$N wallet account sync-private 2>&1 | tail -1
BAL=$($N wallet account get --account-id "$P" 2>&1 | grep -oE "Balance [0-9]+" | grep -oE "[0-9]+")
if [ "${BAL:-0}" -lt 20000000 ]; then
  step "shield 40M from the deploy account into P"
  $N wallet auth-transfer send --from "$PAYER" --to "$P" --amount 40000000 2>&1 | quiet | tail -3
  $N wallet account sync-private 2>&1 | tail -1
  $N wallet account get --account-id "$P" 2>&1 | grep -i balance
fi

for name in jef logos; do
  if ! $N resolve $name >/dev/null 2>&1; then
    step "register $name held by P (proof)"
    $N register $name --owner "$P" --term 1y 2>&1 | quiet | tail -4 || fail "register $name"
  fi
  $N resolve $name | grep -E '"held_by"|"expires_date"'
done

step "set jef's page (proof)"
$N set-page jef $DEV/jef-page.html 2>&1 | quiet | tail -3
$N resolve jef | head -30
step DONE
