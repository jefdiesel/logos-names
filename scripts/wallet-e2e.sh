#!/usr/bin/env bash
# End-to-end test of the Names Wallet against a real LEZ v0.4.0-rc1 sequencer in standalone
# mode, on a fresh throwaway chain. CI (.github/workflows/wallet-e2e.yml) and a local rehearsal
# run this same script.
#
#   1. Registry: deploy the Names registry guest through the program loader, configure it
#      (deposit mode), register a name publicly (commit, the 15-minute wait, reveal).
#   2. Payments: `names pay` to the name's public account, and a plain transfer to an account;
#      balances are checked.
#   3. Tokens: deploy the prototype token program (wallet-tokens/), then `names token`
#      create, mint, send public to public, list, and an overspend refused.
#   4. Testimonial: deploy testimonial/, post one testimonial and list it with its client; a
#      second post by the same account is refused.
#
# Steps 3 and 4 run while the registration waits out its commitment, from another wallet, so
# the 15 minutes the registry program enforces (MIN_COMMIT_AGE_MS) are not added on top.
#
# Private flows are not run here: each one proves on this machine for several minutes
# (see docs/wallet/TOKENS.md section 9 for the private token run).
#
# Nothing here is secret: the payer is LEZ's published local-test genesis account, which a
# fresh chain made from networks/local.json funds. Never point this at a public network.
#
# Inputs (environment):
#   SEQ_BIN          sequencer_service built with `--features standalone` (LEZ v0.4.0-rc1)
#   SEQ_CONFIG       its networks/local.json
#   NAMES            the `names` CLI from this checkout (cargo build --release -p names_cli)
#   NAMES_ELF        the registry guest (logos_names.bin, from cargo build -p names_methods)
#   WALLET_TOKENS    the prototype token CLI (cargo build --release --manifest-path wallet-tokens/Cargo.toml)
#   TESTIMONIAL      the testimonial client (cargo build --release -p testimonial_client)
#   TESTIMONIAL_ELF  the testimonial guest (testimonial-imageid <path> writes it)
#   WORK             a new, empty directory for the chain and the wallets
#   RPC_PORT, METRICS_PORT  (default 3230, 9230), bound to 127.0.0.1 only
set -euo pipefail

for v in SEQ_BIN SEQ_CONFIG NAMES NAMES_ELF WALLET_TOKENS TESTIMONIAL TESTIMONIAL_ELF WORK; do
  [ -n "${!v:-}" ] || { echo "set $v" >&2; exit 2; }
done
for f in "$SEQ_BIN" "$SEQ_CONFIG" "$NAMES" "$NAMES_ELF" "$WALLET_TOKENS" "$TESTIMONIAL" "$TESTIMONIAL_ELF"; do
  [ -f "$f" ] || { echo "no such file: $f" >&2; exit 2; }
done
RPC_PORT=${RPC_PORT:-3230}
METRICS_PORT=${METRICS_PORT:-9230}
RPC=http://127.0.0.1:$RPC_PORT
# LEZ's published local-test key (PRIVATE_KEY_PUB_ACC_A in lez/testnet_initial_state): local chains only.
LOCAL_TEST_KEY=10a26a9aec7d34b82364eeae45c5294dbb0a764b000b94eeb9b58511dc487c4d
SEGMENT=$((96 * 1024)) # program_loader_core::MAX_SEGMENT_DATA_LEN
NAME=walletci
SUBMISSION=lp-0021/names-wallet

rpc() { curl -sf -m 10 -X POST "$RPC" -H 'Content-Type: application/json' \
  -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$1\",\"params\":${2:-[]}}"; }
say() { echo; echo "## $(date -u +%H:%M:%S) $*"; }
fail() { echo "FAIL: $*" >&2; exit 1; }
id_of() { grep -oE "$1/[1-9A-HJ-NP-Za-km-z]{32,44}" | head -1 | cut -d/ -f2; }

if rpc getLastBlockId >/dev/null 2>&1; then
  fail "something already answers on $RPC; this script only runs its own fresh chain"
fi
if [ -e "$WORK" ] && [ -n "$(ls -A "$WORK")" ]; then
  fail "$WORK is not empty; give a new directory"
fi
mkdir -p "$WORK"
WORK=$(cd "$WORK" && pwd)

SEQ_PID= REG_PID=
cleanup() {
  [ -n "$REG_PID" ] && kill "$REG_PID" 2>/dev/null || true
  if [ -n "$SEQ_PID" ]; then
    kill "$SEQ_PID" 2>/dev/null || true
    wait "$SEQ_PID" 2>/dev/null || true
    echo "sequencer stopped"
  fi
}
trap cleanup EXIT

say "fresh standalone sequencer on $RPC"
"$SEQ_BIN" setup "$WORK/seq" --config "$SEQ_CONFIG" \
  --funding-public-key 0101010101010101010101010101010101010101010101010101010101010100 --create-channel \
  > "$WORK/seq-setup.log" 2>&1 || { cat "$WORK/seq-setup.log"; fail "sequencer setup"; }
RUST_LOG=warn "$SEQ_BIN" start "$WORK/seq" --rpc-listen-address "127.0.0.1:$RPC_PORT" \
  --metrics-listen-address "127.0.0.1:$METRICS_PORT" > "$WORK/sequencer.log" 2>&1 < /dev/null &
SEQ_PID=$!
for _ in $(seq 1 60); do rpc getLastBlockId >/dev/null 2>&1 && break; sleep 2; done
rpc getLastBlockId >/dev/null || { tail -40 "$WORK/sequencer.log"; fail "the sequencer did not answer"; }
echo "up (pid $SEQ_PID)"

# Every client is pointed at this chain explicitly: the CLI's default is the public testnet.
export LEE_WALLET_PASSWORD=
export LOGOS_NAMES_RPC=$RPC
export WALLET_TOKENS_SEQUENCER=$RPC
export TESTIMONIAL_RPC=$RPC
MAIN=$WORK/wallet       # F (funded), B, X, the program headers and segments
OWNER=$WORK/wallet-r    # R, who registers the name (its own wallet: it runs in parallel)
N() { LEE_WALLET_HOME_DIR=$MAIN "$NAMES" "$@"; }
NR() { LEE_WALLET_HOME_DIR=$OWNER "$NAMES" "$@"; }

# A new wallet, without printing a recovery phrase (these are throwaway keys anyway). The
# config is written first: the wallet's built-in default sequencer is the public testnet.
new_wallet() {
  mkdir -p "$1"
  cat > "$1/wallet_config.json" <<EOF
{
  "gas_limit": 2000000,
  "private_fee_headroom_blocks": 5,
  "seq_block_poll_max_amount": 100,
  "seq_poll_max_retries": 5,
  "seq_poll_timeout": "12s",
  "seq_tx_poll_max_blocks": 5,
  "sequencer": { "sequencer_addr": "$RPC/" }
}
EOF
  LEE_WALLET_HOME_DIR=$1 "$NAMES" wallet check-health > /dev/null
  grep -q "127.0.0.1:$RPC_PORT" "$1/wallet_config.json" || fail "the wallet config does not point at $RPC"
}
new_public() { LEE_WALLET_HOME_DIR=$1 "$NAMES" wallet account new public | id_of Public; }
# A public account's native balance, as the wallet reads it from the chain.
balance() {
  LEE_WALLET_HOME_DIR=$MAIN "$NAMES" wallet account get --account-id "Public/$1" | sed -nE 's/^Balance ([0-9]+).*/\1/p'
}
transfer() { # from to amount [--to-pk]: --to names an account this wallet holds
  local to=(--to "Public/$2")
  [ "${4:-}" = --to-pk ] && to=(--to-pk "$R_PK")
  N wallet auth-transfer send --from "Public/$1" "${to[@]}" --amount "$3" | grep -vE '^(Stored|Transaction data)' | cut -c1-200
}
deploy_program() { # elf -> header id (the program's account ID)
  local elf=$1 size count header segs=() i
  size=$(wc -c < "$elf" | tr -d ' ')
  count=$(( (size + SEGMENT - 1) / SEGMENT ))
  header=$(new_public "$MAIN")
  for i in $(seq 1 "$count"); do segs+=("Public/$(new_public "$MAIN")"); done
  echo "deploying $(basename "$elf") ($size bytes, $count segments) at $header" >&2
  N wallet program-loader deploy --elf "$elf" --header "Public/$header" --segments "${segs[@]}" --payer "Public/$F" \
    | grep -vE '^(Stored|Transaction data)' | cut -c1-200 >&2
  echo "$header"
}

say "wallets: import the funded local-test account, make the others"
new_wallet "$MAIN"
new_wallet "$OWNER"
F=$(N wallet account import public --private-key "$LOCAL_TEST_KEY" | id_of Public)
[ -n "$F" ] || fail "import of the local-test key"
B=$(new_public "$MAIN")   # receives a plain transfer, then tokens
R=$(new_public "$OWNER")  # registers the name, and is published as its public account
# R is not in F's wallet and has never been used, so a transfer to it carries its public key.
R_PK=$(NR wallet account get --account-id "Public/$R" --keys | sed -n 's/^pk //p')
[ -n "$R_PK" ] || fail "no public key for R"
echo "F=$F (funded)  B=$B  R=$R"
F0=$(balance "$F")
echo "F holds $F0"
[ "${F0:-0}" -gt 0 ] || fail "the local-test account is not funded on this chain"

# ---------------------------------------------------------------- 1. registry
say "1. deploy the Names registry"
REG=$(deploy_program "$NAMES_ELF")
export LOGOS_NAMES_PROGRAM=$REG
for _ in $(seq 1 30); do N settings >/dev/null 2>&1 && break; sleep 5; done
echo "registry $REG"

say "fund the registry's own account (it signs the first Configure) and R"
transfer "$F" "$REG" 10000000
transfer "$F" "$R" 50000000 --to-pk
[ "$(balance "$R")" = 50000000 ] || fail "R was not funded"

say "configure: deposit mode, admin and treasury F, deposits 1000000 (a normal name) / 3000000 (three characters)"
N configure --admin "$F" --treasury "$F" --mode deposit --deposits 1000000,3000000 | grep -vE '^(Stored|Transaction data)'
N settings | tee "$WORK/settings.json"
grep -q "$F" "$WORK/settings.json" || fail "the settings do not name F as admin"

say "register $NAME publicly to R (commit now, reveal after 15 minutes, in the background)"
NR register "$NAME" --owner "Public/$R" --wait > "$WORK/register.log" 2>&1 &
REG_PID=$!

# ---------------------------------------------------------------- 3. tokens
say "3. deploy the prototype token program"
# (the wallet prints each segment transaction in full, hundreds of KB: left out)
P=$(LEE_WALLET_HOME_DIR=$MAIN "$WALLET_TOKENS" deploy --payer "$F" | grep -vE '^(Stored|Transaction data)' \
  | tee /dev/stderr | sed -n 's/^program //p')
[ -n "$P" ] || fail "token program deploy"
export NAMES_TOKEN_PROGRAM=$P
echo "token program $P"

say "names token create Demo 1000000 (F may mint)"
D=$(N token create Demo 1000000 --payer "Public/$F" --mintable | tee /dev/stderr | sed -n 's/^token //p')
[ -n "$D" ] || fail "token create"
N token info "$D"
say "names token mint 500 to F"
N token mint "$D" 500
say "names token send 250 Public/F -> Public/B"
N token send "$D" 250 --from "Public/$F" --to "Public/$B"
say "names token list"
N token list
N module token holdings | tail -1 > "$WORK/holdings.json"
python3 - "$WORK/holdings.json" "$F" "$B" "$D" <<'EOF'
import json, sys
held = json.load(open(sys.argv[1]))
f, b, d = sys.argv[2:5]
got = {h["account"].split("/")[-1]: int(h["balance"]) for h in held if h["definition"] == d}
assert got.get(f) == 1000250, got
assert got.get(b) == 250, got
print("holdings ok:", got)
EOF
say "an overspend is refused before sending (B holds 250, sends 251)"
if N token send "$D" 251 --from "Public/$B" --to "Public/$F"; then fail "the overspend was accepted"; fi
echo "refused, as it should be"

# ---------------------------------------------------------------- 4. testimonial
say "4. deploy the testimonial program"
TST=$(deploy_program "$TESTIMONIAL_ELF")
export TESTIMONIAL_PROGRAM=$TST
echo "testimonial program $TST"
say "post a testimonial as F"
LEE_WALLET_HOME_DIR=$MAIN "$TESTIMONIAL" post --submission "$SUBMISSION" --username ci \
  --text "Paid a name and sent tokens from the Names Wallet (CI end-to-end)." --author "Public/$F"
say "a second post by the same account is refused"
if LEE_WALLET_HOME_DIR=$MAIN "$TESTIMONIAL" post --submission "$SUBMISSION" --text again --author "Public/$F"; then
  fail "a second testimonial by F was accepted"
fi
say "list"
LEE_WALLET_HOME_DIR=$MAIN "$TESTIMONIAL" list --submission "$SUBMISSION"
LEE_WALLET_HOME_DIR=$MAIN "$TESTIMONIAL" list --submission "$SUBMISSION" --json > "$WORK/testimonials.json"
python3 - "$WORK/testimonials.json" "$F" <<'EOF'
import json, sys
d = json.load(open(sys.argv[1]))
text = json.dumps(d)
assert sys.argv[2] in text, "F's testimonial is not listed"
assert "CI end-to-end" in text, "the text is not listed"
print("testimonial listed")
EOF

# ---------------------------------------------------------------- 1. (end) and 2. payments
say "wait for the registration (the commitment must be 15 minutes old)"
R0=50000000
if ! wait "$REG_PID"; then REG_PID=; cat "$WORK/register.log"; fail "register"; fi
REG_PID=
grep -vE '^(Stored|Transaction data)' "$WORK/register.log" | cut -c1-200
N resolve "$NAME" | tee "$WORK/resolve.json"
NR mine
R1=$(balance "$R")
echo "R held $R0, now $R1 (deposit 1000000 plus fees)"
[ "$R1" -le $((R0 - 1000000)) ] || fail "no deposit was taken from R"

say "2. payments: R publishes itself as $NAME's public account, then F runs names pay $NAME"
NR set-account "$NAME" "Public/$R" | grep -vE '^(Stored|Transaction data)'
N resolve "$NAME" | grep -q "$R" || fail "$NAME does not resolve to R's account"
X0=$(balance "$R")
N pay "$NAME" 123456 --from "Public/$F" --public
X1=$(balance "$R")
echo "R held $X0, now $X1"
[ "$X1" -eq $((X0 + 123456)) ] || fail "names pay did not credit the name's account"

say "a transfer to an account: F -> B"
B0=$(balance "$B"); B0=${B0:-0}
F1=$(balance "$F")
transfer "$F" "$B" 777000
B1=$(balance "$B"); F2=$(balance "$F")
echo "B held $B0, now $B1; F held $F1, now $F2"
[ "$B1" -eq $((B0 + 777000)) ] || fail "B was not credited"
[ "$F2" -le $((F1 - 777000)) ] || fail "F was not debited"

say "the chain is still healthy"
rpc getLastBlockId
echo
say "PASS: registry, payments, tokens and testimonial on a fresh rc1 standalone chain"
