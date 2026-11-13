#!/usr/bin/env bash
# Throwaway rc1 chain on 127.0.0.1:3220 for the mini-apps live check: a funded wallet with
# two public accounts (F, B) and the testimonial program deployed. Writes $L/env.
# Adapted from scripts/wallet-e2e.sh. Local chain only: the payer is LEZ's published
# local-test key.
set -euo pipefail
export PATH=$HOME/.cargo/bin:/opt/homebrew/bin:$PATH
L=${LIVE_DIR:?set LIVE_DIR to a new scratch directory holding bin/names and bin/testimonial.bin}
SEQ_BIN=/Volumes/exos162/logos/lez-0.4-rc1/target/release/sequencer_service
SEQ_CONFIG=/Volumes/exos162/logos/lez-0.4-rc1/lez/sequencer/service/networks/local.json
NAMES=$L/bin/names
RPC=http://127.0.0.1:3220
LOCAL_TEST_KEY=10a26a9aec7d34b82364eeae45c5294dbb0a764b000b94eeb9b58511dc487c4d
SEGMENT=$((96 * 1024))
rpc() { curl -sf -m 10 -X POST "$RPC" -H 'Content-Type: application/json' -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$1\",\"params\":${2:-[]}}"; }
id_of() { grep -oE "$1/[1-9A-HJ-NP-Za-km-z]{32,44}" | head -1 | cut -d/ -f2; }
fail() { echo "FAIL: $*" >&2; exit 1; }

if rpc getLastBlockId >/dev/null 2>&1; then fail "something already answers on $RPC"; fi
"$SEQ_BIN" setup "$L/seq" --config "$SEQ_CONFIG" \
  --funding-public-key 0101010101010101010101010101010101010101010101010101010101010100 --create-channel > "$L/seq-setup.log" 2>&1
RUST_LOG=warn nohup "$SEQ_BIN" start "$L/seq" --rpc-listen-address 127.0.0.1:3220 \
  --metrics-listen-address 127.0.0.1:9220 > "$L/sequencer.log" 2>&1 < /dev/null &
echo $! > "$L/sequencer.pid"
for _ in $(seq 1 60); do rpc getLastBlockId >/dev/null 2>&1 && break; sleep 2; done
rpc getLastBlockId >/dev/null || fail "sequencer did not answer"
echo "sequencer up, pid $(cat "$L/sequencer.pid")"

export LEE_WALLET_PASSWORD= LOGOS_NAMES_RPC=$RPC
W=$L/wallet
mkdir -p "$W"
cat > "$W/wallet_config.json" <<EOF
{
  "gas_limit": 2000000,
  "fee_headroom_blocks": 5,
  "seq_block_poll_max_amount": 100,
  "seq_poll_max_retries": 5,
  "seq_poll_timeout": "12s",
  "seq_tx_poll_max_blocks": 5,
  "sequencer": { "sequencer_addr": "$RPC/" }
}
EOF
N() { LEE_WALLET_HOME_DIR=$W "$NAMES" "$@"; }
N wallet check-health > /dev/null
new_public() { N wallet account new public | id_of Public; }
F=$(N wallet account import public --private-key "$LOCAL_TEST_KEY" | id_of Public)
B=$(new_public)
echo "F=$F B=$B"
N wallet auth-transfer send --from "Public/$F" --to "Public/$B" --amount 50000000 | grep -vE '^(Stored|Transaction data)' | cut -c1-160

elf=$L/bin/testimonial.bin
count=$(( ($(wc -c < "$elf") + SEGMENT - 1) / SEGMENT ))
header=$(new_public)
segs=()
for i in $(seq 1 "$count"); do segs+=("Public/$(new_public)"); done
N wallet program-loader deploy --elf "$elf" --header "Public/$header" --segments "${segs[@]}" --payer "Public/$F" \
  | grep -vE '^(Stored|Transaction data)' | cut -c1-160
echo "testimonial program $header"
cat > "$L/env" <<EOF
F=$F
B=$B
TST=$header
W=$W
RPC=$RPC
EOF
cat "$L/env"
