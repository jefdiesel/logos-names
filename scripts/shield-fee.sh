#!/bin/bash
# What moving tokens from a public account into a brand-new private account costs on LEZ 0.4:
# the fee reserve, who pays it, the fee actually charged and the refund, measured on a fresh
# local rc3 standalone chain (nothing real is spent). Prints MEASURED lines.
#
#   SEQ_BIN=… SEQ_CONFIG=… NAMES=… WORK=<new dir> RPC_PORT=3240 METRICS_PORT=9240 scripts/shield-fee.sh
set -euo pipefail
RPC=http://127.0.0.1:$RPC_PORT
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
MAIN=$WORK/wallet
N() { LEE_WALLET_HOME_DIR=$MAIN "$NAMES" "$@"; }
mkdir -p "$MAIN"
cat > "$MAIN/wallet_config.json" <<EOF
{ "gas_limit": 2000000, "fee_headroom_blocks": 5, "seq_block_poll_max_amount": 100, "seq_poll_max_retries": 5,
  "seq_poll_timeout": "12s", "seq_tx_poll_max_blocks": 5, "sequencer": { "sequencer_addr": "$RPC/" } }
EOF
N wallet check-health > /dev/null
F=$(N wallet account import public --private-key "$LOCAL_TEST_KEY" | id_of Public)
[ -n "$F" ] || fail "import"
pub() { N wallet account get --account-id "Public/$1" | sed -nE 's/^Balance ([0-9]+).*/\1/p'; }
priv() { N wallet account get --account-id "Private/$1" | sed -nE 's/^Balance ([0-9]+).*/\1/p'; }
# Every private account the wallet holds and its balance, after catching up with the chain.
privates() { N wallet account sync-private > /dev/null 2>&1 || true
  for id in $(N wallet account list 2>/dev/null | grep -oE "Private/[1-9A-HJ-NP-Za-km-z]{32,44}" | cut -d/ -f2); do echo "  Private/${id:0:8}… $(priv "$id")"; done; }
send() { N wallet auth-transfer send "$@" 2>&1 | grep -vE '^(Stored|Transaction data)' | cut -c1-220; }

X=$(N wallet account new private | id_of Private); [ -n "$X" ] || fail "new private account"
echo "F=Public/$F (funded)  X=Private/$X (brand new, holds nothing)"

say "1. dry run: move 10,000,000 from F into the brand-new X"
send --from "Public/$F" --to "Private/$X" --amount 10000000 --dry-run | tee "$WORK/dry.log"

say "2. the move, for real"
F0=$(pub "$F")
T0=$(date +%s)
send --from "Public/$F" --to "Private/$X" --amount 10000000 | tee "$WORK/shield.log"
T1=$(date +%s)
F1=$(pub "$F")
echo "private accounts after:"; privates | tee "$WORK/privates-1.txt"
X1=$(priv "$X")
echo "MEASURED move 10000000 into a new private account: took $((T1 - T0)) s; F paid $((F0 - F1)); X received ${X1:-?}; fee out of X $((10000000 - ${X1:-0}))"
# A refund to the payer's alias may come as a credit a few blocks later: look again.
sleep 60
echo "private accounts a minute later:"; privates | tee "$WORK/privates-1b.txt"

say "3. a move below the reserve into another brand-new account: 4,000,000 into Y"
Y=$(N wallet account new private | id_of Private)
if send --from "Public/$F" --to "Private/$Y" --amount 4000000 | tee "$WORK/small.log"; then :; fi
echo "MEASURED move 4000000 into a new private account: Y holds $(priv "$Y" || echo ?); $(grep -m1 -iE 'fee reserve|error|fails' "$WORK/small.log" || echo 'no error printed')"

say "4. the same 1,000,000 into a third brand-new account Z, with X (now funded) paying the fee"
Z=$(N wallet account new private | id_of Private)
X2=$(priv "$X"); F2=$(pub "$F")
send --from "Public/$F" --to "Private/$Z" --amount 1000000 --fee-payer "$X" | tee "$WORK/payer.log"
echo "private accounts after:"; privates | tee "$WORK/privates-2.txt"
X3=$(priv "$X"); Z1=$(priv "$Z"); F3=$(pub "$F")
echo "MEASURED move 1000000 with a funded fee payer: F paid $((F2 - F3)); Z received ${Z1:-?}; the payer X paid $(( ${X2:-0} - ${X3:-0} )) in fee"
say "done"
