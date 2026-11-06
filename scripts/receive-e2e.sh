#!/bin/bash
# `names receive` on LEZ 0.4, end to end on a fresh local rc1 standalone chain (nothing real
# is spent). A payment to someone's private keys (what `names pay <name>` sends to a name's
# receiving keys) arrives as a pending credit, and the receiving account starts empty, so it
# can't pay the private fee a claim costs. `names receive` claims it with another private
# account of the same wallet paying that fee (campaign finding SC-1, 2026-11-06).
#
#   1. R makes P (the receiving account, empty) and Q, funded from a public account.
#   2. S pays P's keys 12,000,000 from its own private account (a private payment's fee is
#      paid inside the proof, by a private account of the sender): P still holds 0, and both
#      `names balance` and `names receive --list` say a payment waits.
#   3. `names receive`: P holds exactly 12,000,000, Q paid the fee, nothing waits after.
#   4. Refusals: no other private account to pay the fee; a public --fee-from.
#
# Each private step proves on this machine for several minutes: on the Mac mini, run it only
# when no other proof runs.
#
#   SEQ_BIN=… SEQ_CONFIG=… NAMES=… WORK=<new dir> RPC_PORT=3250 METRICS_PORT=9250 scripts/receive-e2e.sh
set -euo pipefail
for v in SEQ_BIN SEQ_CONFIG NAMES WORK RPC_PORT METRICS_PORT; do [ -n "${!v:-}" ] || { echo "set $v" >&2; exit 2; }; done
RPC=http://127.0.0.1:$RPC_PORT
# LEZ's published local-test key (PRIVATE_KEY_PUB_ACC_A): a fresh local chain funds it. Local chains only.
LOCAL_TEST_KEY=10a26a9aec7d34b82364eeae45c5294dbb0a764b000b94eeb9b58511dc487c4d
rpc() { curl -sf -m 10 -X POST "$RPC" -H 'Content-Type: application/json' -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$1\",\"params\":${2:-[]}}"; }
say() { echo; echo "## $(date -u +%H:%M:%S) $*"; }
fail() { echo "FAIL: $*" >&2; exit 1; }
id_of() { grep -oE "$1/[1-9A-HJ-NP-Za-km-z]{32,44}" | head -1 | cut -d/ -f2; }
rpc getLastBlockId >/dev/null 2>&1 && fail "something already answers on $RPC"
[ -e "$WORK" ] && [ -n "$(ls -A "$WORK")" ] && fail "$WORK is not empty"
mkdir -p "$WORK"; WORK=$(cd "$WORK" && pwd)
SEQ_PID=
cleanup() { [ -n "$SEQ_PID" ] && { kill "$SEQ_PID" 2>/dev/null || true; wait "$SEQ_PID" 2>/dev/null || true; echo "sequencer stopped"; }; }
trap cleanup EXIT

say "fresh standalone sequencer on $RPC"
"$SEQ_BIN" setup "$WORK/seq" --config "$SEQ_CONFIG" \
  --funding-public-key 0101010101010101010101010101010101010101010101010101010101010100 --create-channel > "$WORK/seq-setup.log" 2>&1 \
  || { cat "$WORK/seq-setup.log"; fail "sequencer setup"; }
RUST_LOG=warn "$SEQ_BIN" start "$WORK/seq" --rpc-listen-address "127.0.0.1:$RPC_PORT" --metrics-listen-address "127.0.0.1:$METRICS_PORT" \
  >> "$WORK/sequencer.log" 2>&1 < /dev/null &
SEQ_PID=$!
for _ in $(seq 1 60); do rpc getLastBlockId >/dev/null 2>&1 && break; sleep 2; done
rpc getLastBlockId >/dev/null || fail "the sequencer did not answer"

export LEE_WALLET_PASSWORD= LOGOS_NAMES_RPC=$RPC
new_wallet() {
  mkdir -p "$1"
  cat > "$1/wallet_config.json" <<EOF
{ "gas_limit": 2000000, "private_fee_headroom_blocks": 5, "seq_block_poll_max_amount": 100, "seq_poll_max_retries": 5,
  "seq_poll_timeout": "12s", "seq_tx_poll_max_blocks": 5, "sequencer": { "sequencer_addr": "$RPC/" } }
EOF
  LEE_WALLET_HOME_DIR=$1 "$NAMES" wallet check-health > /dev/null
}
S() { LEE_WALLET_HOME_DIR=$WORK/sender "$NAMES" "$@"; }
R() { LEE_WALLET_HOME_DIR=$WORK/receiver "$NAMES" "$@"; }
new_wallet "$WORK/sender"; new_wallet "$WORK/receiver"
clean() { grep -vE '^(Stored|Transaction data)' | cut -c1-220; }
priv() { R wallet account get --account-id "Private/$1" | sed -nE 's/^Balance ([0-9]+).*/\1/p'; }

say "1. R: P to receive (empty), Q funded from a public account"
F=$(S wallet account import public --private-key "$LOCAL_TEST_KEY" | id_of Public)
FR=$(R wallet account import public --private-key "$LOCAL_TEST_KEY" | id_of Public)
[ -n "$F" ] && [ "$F" = "$FR" ] || fail "import"
P=$(R wallet account new private | id_of Private); Q=$(R wallet account new private | id_of Private)
[ -n "$P" ] && [ -n "$Q" ] || fail "new private accounts"
R wallet auth-transfer send --from "Public/$F" --to "Private/$Q" --amount 30000000 2>&1 | clean
R wallet account sync-private > /dev/null 2>&1 || true
Q0=$(priv "$Q"); P0=$(priv "$P")
echo "P=Private/$P holds ${P0:-0}; Q=Private/$Q holds $Q0"
[ "${Q0:-0}" -gt 6000000 ] || fail "Q was not funded"
# The sender's own private account, as names setup makes one: it pays from there.
SP=$(S wallet account new private | id_of Private); [ -n "$SP" ] || fail "S's private account"
S wallet auth-transfer send --from "Public/$F" --to "Private/$SP" --amount 60000000 2>&1 | clean
S wallet account sync-private > /dev/null 2>&1 || true

say "2. S pays P's keys 12,000,000 (a payment to a name's receiving keys looks just like this)"
R wallet account show-keys --account-id "Private/$P" > "$WORK/p-keys.txt"
S wallet auth-transfer send --from "Private/$SP" --to-keys "$WORK/p-keys.txt" --amount 12000000 2>&1 | clean
R wallet account sync-private > /dev/null 2>&1 || true
P1=$(priv "$P"); echo "P holds ${P1:-0} before claiming"
[ "${P1:-0}" -eq 0 ] || fail "P was credited without a claim (expected a pending credit)"
R balance | tee "$WORK/balance-before.txt" | tail -3
grep -q '^waiting to be claimed: 1 payment(s), 12000000 in all' "$WORK/balance-before.txt" || fail "names balance didn't say a payment waits"
R receive --list | tee "$WORK/list.txt"
grep -q "12000000 to Private/$P" "$WORK/list.txt" || fail "names receive --list didn't list the payment"

say "4a. refused: a public account can't pay a claim's fee (inside the proof)"
if R receive --fee-from "Public/$F" > "$WORK/refused-public.txt" 2>&1; then fail "a public --fee-from was taken"; fi
grep -q "must be a private account" "$WORK/refused-public.txt" || { cat "$WORK/refused-public.txt"; fail "the refusal didn't say why"; }

say "3. names receive: Q pays the fee, P gets the whole 12,000,000"
T0=$(date +%s)
R receive 2>&1 | clean | tee "$WORK/receive.txt"
T1=$(date +%s)
R wallet account sync-private > /dev/null 2>&1 || true
P2=$(priv "$P"); Q2=$(priv "$Q")
echo "MEASURED claim: took $((T1 - T0)) s; P ${P1:-0} -> $P2; Q $Q0 -> $Q2 (fee $((Q0 - Q2)))"
[ "$P2" -eq 12000000 ] || fail "P holds $P2, not the 12000000 paid"
[ "$Q2" -lt "$Q0" ] || fail "Q paid no fee"
# The claim's fee was taken at its "at most" and the rest comes back as a credit of its own,
# worth less than claiming it would cost: it is left, not claimed at a loss.
R wallet account sync-private > /dev/null 2>&1 || true
R receive | tee "$WORK/again.txt"
grep -qE "nothing to receive|nothing worth claiming" "$WORK/again.txt" || fail "names receive claimed again after the payment"
grep -q "left waiting" "$WORK/again.txt" && echo "(a fee refund is left waiting, worth less than claiming it costs)"
R balance > "$WORK/balance-after.txt"
grep -q '^waiting to be claimed' "$WORK/balance-after.txt" && fail "names balance still says a payment worth claiming waits"

say "4b. refused: a wallet whose only private account is the one being paid"
new_wallet "$WORK/lonely"
L() { LEE_WALLET_HOME_DIR=$WORK/lonely "$NAMES" "$@"; }
LP=$(L wallet account new private | id_of Private)
L wallet account show-keys --account-id "Private/$LP" > "$WORK/lp-keys.txt"
S wallet account sync-private > /dev/null 2>&1 || true
S wallet auth-transfer send --from "Private/$SP" --to-keys "$WORK/lp-keys.txt" --amount 10000000 2>&1 | clean
if L receive > "$WORK/refused-lonely.txt" 2>&1; then fail "a claim with nothing to pay its fee went ahead"; fi
grep -qE "no other private account|move more into a private account" "$WORK/refused-lonely.txt" || { cat "$WORK/refused-lonely.txt"; fail "the refusal didn't say why"; }
tail -1 "$WORK/refused-lonely.txt"

say "PASS: a payment to private keys waits, names balance and receive --list say so, names receive claims all of it with another account paying the fee"
