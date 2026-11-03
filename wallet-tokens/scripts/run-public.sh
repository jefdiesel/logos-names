#!/bin/bash
# Public half of the prototype run against a throwaway local LEZ 0.4 sequencer:
# deploy, create a token, mint, transfer public to public, read balances.
# Keeps the IDs it makes in $H/ids.env (for a rerun and for run-private.sh).
#
#   H=<run dir> BIN=<wallet-tokens binary> scripts/run-public.sh
set -euo pipefail
: "${H:?set H to a run directory}"
: "${BIN:?set BIN to the wallet-tokens binary}"
export LEE_WALLET_HOME_DIR=$H/wallet
export WALLET_TOKENS_SEQUENCER=${WALLET_TOKENS_SEQUENCER:-http://127.0.0.1:3180}
# LEZ's published local-test key: holds 10T on a fresh local chain.
LOCAL_TEST_KEY=10a26a9aec7d34b82364eeae45c5294dbb0a764b000b94eeb9b58511dc487c4d
T=$BIN
F= B= P= D=
[ -f "$H/ids.env" ] && . "$H/ids.env"
save() { printf 'F=%s\nB=%s\nP=%s\nD=%s\n' "$F" "$B" "$P" "$D" > "$H/ids.env"; }
id_of() { grep -oE "$1/[1-9A-HJ-NP-Za-km-z]{32,44}" | head -1 | cut -d/ -f2; }
say() { echo; echo "## $*"; }

if [ -z "$F" ]; then
  say "wallet: create and check the sequencer"
  $T wallet check-health
  say "import the funded local-test account"
  F=$($T wallet account import public --private-key $LOCAL_TEST_KEY | tee /dev/stderr | id_of Public)
  say "a second public account (Bob), never used on chain"
  B=$($T wallet account new public | tee /dev/stderr | id_of Public)
  save
fi
echo "F=$F B=$B"
$T wallet account get --account-id Public/$F | head -5

if [ -z "$P" ]; then
  say "deploy the token program"
  P=$($T deploy --payer $F | tee /dev/stderr | sed -n 's/^program //p')
  save
fi
echo "program P=$P"

if [ -z "$D" ]; then
  say "create a token: supply 1000000 to F, F may mint"
  D=$($T create --program $P --name Demo --supply 1000000 --to $F --authority $F --payer $F | tee /dev/stderr | sed -n 's/^token //p')
  save
fi
$T definition --program $P --definition $D
say "mint 500 more to F"
$T mint --program $P --definition $D --authority $F --to $F --amount 500
$T definition --program $P --definition $D
say "transfer 250 public to public, F to B"
$T send --program $P --definition $D --from $F --to $B --amount 250
say "balances"
$T balances --program $P
say "a transfer beyond the balance is refused before sending"
if $T send --program $P --definition $D --from $B --to $F --amount 251; then exit 1; fi
