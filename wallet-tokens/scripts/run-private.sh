#!/bin/bash
# Private half of the prototype run, after run-public.sh: token transfers public to private,
# private to public, private to someone's private keys (a pending credit), receiving it, and
# an on-chain refusal. Every private step proves on this machine.
#
#   H=<run dir> BIN=<wallet-tokens binary> scripts/run-private.sh
set -euo pipefail
: "${H:?set H to a run directory}"
: "${BIN:?set BIN to the wallet-tokens binary}"
export LEE_WALLET_HOME_DIR=$H/wallet
export WALLET_TOKENS_SEQUENCER=${WALLET_TOKENS_SEQUENCER:-http://127.0.0.1:3180}
T=$BIN
. "$H/ids.env"
V=${V:-} W=${W:-}
id_of() { grep -oE "$1/[1-9A-HJ-NP-Za-km-z]{32,44}" | head -1 | cut -d/ -f2; }
say() { echo; echo "## $(date +%T) $*"; }
save() { printf 'F=%s\nB=%s\nP=%s\nD=%s\nV=%s\nW=%s\n' "$F" "$B" "$P" "$D" "$V" "$W" > "$H/ids.env"; }

if [ -z "$V" ]; then
  say "two private accounts: V (ours, will pay fees), W (receives as if someone else's)"
  V=$($T wallet account new private | tee /dev/stderr | id_of Private)
  W=$($T wallet account new private | tee /dev/stderr | id_of Private)
  save
  say "shield 50000000 native to V for its in-proof fees (native transfer, proven)"
  $T wallet auth-transfer send --from Public/$F --to Private/$V --amount 50000000
fi
echo "V=$V W=$W"
say "token: 1000 public F -> private V (V's own account, credited in the same transaction)"
$T send --program $P --definition $D --from $F --to $V --amount 1000
say "balances"
$T balances --program $P
say "token: 300 private V -> public B"
$T send --program $P --definition $D --from $V --to $B --amount 300
say "balances"
$T balances --program $P
say "token: 200 private V -> W given only by its npk/vpk (a pending, sealed credit)"
$T wallet account show-keys --account-id Private/$W > "$H/w.keys"
$T send --program $P --definition $D --from $V --to-npk "$(sed -n 1p "$H/w.keys")" --to-vpk "$(sed -n 2p "$H/w.keys")" --amount 200
say "pending messages and balances before receiving"
$T wallet pending list
$T balances --program $P
say "W receives its pending token credit"
$T receive --program $P
say "balances"
$T balances --program $P
say "on chain, a stranger's mint is refused (B is not the authority; the client does not pre-check)"
if $T mint --program $P --definition $D --authority $B --to $B --amount 1; then echo "UNEXPECTED: mint applied"; exit 1; fi
$T definition --program $P --definition $D
