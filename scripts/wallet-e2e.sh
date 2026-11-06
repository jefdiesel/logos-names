#!/usr/bin/env bash
# End-to-end test of the Names Wallet against a real LEZ v0.4.0-rc1 sequencer in standalone
# mode, on a fresh throwaway chain. CI (.github/workflows/wallet-e2e.yml) and a local rehearsal
# run this same script.
#
#   1. Registry: deploy the Names registry guest through the program loader, configure it
#      (deposit mode), register a name publicly (commit, the 15-minute wait, reveal). A locked
#      name (`apps`: four characters) is refused to anyone but the admin; the admin registers
#      it and transfers it to R, the launch-day path for the locked list.
#   2. Payments: `names pay` to the name's public account, and a plain transfer to an account;
#      balances are checked.
#   3. Tokens: deploy the prototype token program (wallet-tokens/), then `names token`
#      create, mint, send public to public, list, and an overspend refused.
#   4. Testimonial: deploy testimonial/, post one testimonial and list it with its client; a
#      second post by the same account is refused.
#   5. Accounts and fees: the fee quote, the fee lines a send prints (the fee paid matches
#      what left the payer), and making, naming and choosing accounts (`names module account-*`).
#   6. A network drop and a restart: with the sequencer stopped, reads and sends fail quickly
#      and say so while the wallet's own settings still change; restarted on the same chain,
#      every balance and setting is as it was and a transfer goes through.
#   7. Apps verified by publisher (lez-program-registry docs/APPS.md), when LEZREG names the
#      registry's `lezreg` CLI: the name publishes an app record for a package that names it,
#      `lezreg app verify` finds that package verified and a copy with another root a mismatch.
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
#   LEZREG           optional: the registry's `lezreg` CLI, for step 7 (skipped without it)
#   STORAGE_BIN      optional: a Logos Storage binary; step 7 then also publishes a package by
#                    CID and installs it with `lezreg app get` (checked by sha256 and root)
#   WORK             a new, empty directory for the chain and the wallets
#   RPC_PORT, METRICS_PORT  (default 3230, 9230), bound to 127.0.0.1 only
set -euo pipefail
# The locked-name list: the repository's copy, so no run downloads the published one.
export NAMES_SHORTLIST="${NAMES_SHORTLIST:-$(cd "$(dirname "$0")/.." && pwd)/shortlist/shortlist.bin}"

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
height() { rpc getLastBlockId | python3 -c 'import json, sys; print(json.load(sys.stdin)["result"])'; }
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
  [ -n "${LOCKED_PID:-}" ] && kill "$LOCKED_PID" 2>/dev/null || true
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
start_sequencer() {
  RUST_LOG=warn "$SEQ_BIN" start "$WORK/seq" --rpc-listen-address "127.0.0.1:$RPC_PORT" \
    --metrics-listen-address "127.0.0.1:$METRICS_PORT" >> "$WORK/sequencer.log" 2>&1 < /dev/null &
  SEQ_PID=$!
  for _ in $(seq 1 60); do rpc getLastBlockId >/dev/null 2>&1 && break; sleep 2; done
  rpc getLastBlockId >/dev/null || { tail -40 "$WORK/sequencer.log"; fail "the sequencer did not answer"; }
  echo "up (pid $SEQ_PID)"
}
start_sequencer

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
say "1. a locked name: refused to R, registered by the admin F (in the background)"
LOCKED=apps
if NR register "$LOCKED" --owner "Public/$R" > "$WORK/locked-refused.log" 2>&1; then fail "R registered the locked name $LOCKED"; fi
grep -q "is locked" "$WORK/locked-refused.log" || { cat "$WORK/locked-refused.log"; fail "R's attempt was not refused as locked"; }
echo "refused to R: $(grep -o 'is locked[^:]*' "$WORK/locked-refused.log" | head -1)"
N register "$LOCKED" --owner "Public/$F" --wait > "$WORK/locked.log" 2>&1 &
LOCKED_PID=$!

say "wait for the registration (the commitment must be 15 minutes old)"
R0=50000000
if ! wait "$REG_PID"; then REG_PID=; cat "$WORK/register.log"; fail "register"; fi
REG_PID=
grep -vE '^(Stored|Transaction data)' "$WORK/register.log" | cut -c1-200
N resolve "$NAME" | tee "$WORK/resolve.json"
NR mine
# The account the name is paid into is told apart in the account list by its npk, the key
# the name's profile publishes: it must be the receiving account's own (`module keys`).
RECEIVING=$(sed -nE "s/^payments to $NAME(\.lez)? will arrive privately at (Private\/[^ ]+)$/\2/p" "$WORK/register.log" | tail -1)
[ -n "$RECEIVING" ] || fail "register named no receiving account"
NR module account-list > "$WORK/accounts-r.json"
NR module keys "$RECEIVING" > "$WORK/keys-r.json"
python3 - "$WORK/accounts-r.json" "$WORK/keys-r.json" "$RECEIVING" <<'PY' || fail "the account list's npk for $RECEIVING"
import json, sys
rows, keys, receiving = json.load(open(sys.argv[1])), json.load(open(sys.argv[2])), sys.argv[3]
row = next((r for r in rows if r["account"] == receiving), None)
assert row is not None, f"{receiving} is not in the account list"
assert row["npk"] == keys["npk"] and len(row["npk"]) == 64, (row["npk"], keys["npk"])
assert all(r["npk"] is None for r in rows if r["kind"] == "public"), "a public account has an npk"
print(f"{receiving}: account-list npk {row['npk'][:8]}… matches its receiving keys")
PY
say "1. the admin's locked name lands, and is transferred to R"
if ! wait "$LOCKED_PID"; then LOCKED_PID=; cat "$WORK/locked.log"; fail "the admin's registration of $LOCKED"; fi
LOCKED_PID=
grep -vE '^(Stored|Transaction data)' "$WORK/locked.log" | cut -c1-200 | tail -4
N resolve "$LOCKED" | tee "$WORK/locked-resolve.json" | grep -q "\"held_by\": \"$F\"" || fail "F does not hold $LOCKED"
N transfer "$LOCKED" "Public/$R" | grep -vE '^(Stored|Transaction data)' | cut -c1-200
NR resolve "$LOCKED" | grep -q "\"held_by\": \"$R\"" || fail "R does not hold $LOCKED after the transfer"
N resolve "$LOCKED" | grep -q '"held_by": null' || fail "F still shows as holding $LOCKED"
echo "$LOCKED: registered by the admin, now held by R"
R1=$(balance "$R")
echo "R held $R0, now $R1 (deposit 1000000 plus fees)"
[ "$R1" -le $((R0 - 1000000)) ] || fail "no deposit was taken from R"
# The fees the registration printed account for all R lost besides the deposit.
REG_FEES=$(sed -nE 's/^fee paid: ([0-9]+)$/\1/p' "$WORK/register.log" | awk '{ s += $1 } END { print s + 0 }')
echo "registration fees paid: $REG_FEES"
[ "$R1" -eq $((R0 - 1000000 - REG_FEES)) ] || fail "the registration's fee paid lines don't add up: R lost $((R0 - R1)), deposit 1000000, fees $REG_FEES"

say "2. payments: R publishes itself as $NAME's public account, then F runs names pay $NAME"
NR set-account "$NAME" "Public/$R" | grep -vE '^(Stored|Transaction data)'
N resolve "$NAME" | grep -q "$R" || fail "$NAME does not resolve to R's account"
X0=$(balance "$R")
FP0=$(balance "$F")
N pay "$NAME" 123456 --from "Public/$F" --public | tee "$WORK/pay.log"
X1=$(balance "$R")
FP1=$(balance "$F")
echo "R held $X0, now $X1"
[ "$X1" -eq $((X0 + 123456)) ] || fail "names pay did not credit the name's account"
# 5. The fee lines the app reads: an estimate before sending, what was paid after.
grep -qE '^fee: about [0-9]+, at most [0-9]+ \(' "$WORK/pay.log" || fail "names pay printed no fee estimate"
PAID=$(sed -nE 's/^fee paid: ([0-9]+)$/\1/p' "$WORK/pay.log")
echo "F held $FP0, now $FP1; fee paid $PAID"
[ -n "$PAID" ] || fail "names pay printed no fee paid"
[ "$FP1" -eq $((FP0 - 123456 - PAID)) ] || fail "the fee paid does not match what left F"

say "a transfer to an account: F -> B"
B0=$(balance "$B"); B0=${B0:-0}
F1=$(balance "$F")
transfer "$F" "$B" 777000
B1=$(balance "$B"); F2=$(balance "$F")
echo "B held $B0, now $B1; F held $F1, now $F2"
[ "$B1" -eq $((B0 + 777000)) ] || fail "B was not credited"
[ "$F2" -le $((F1 - 777000)) ] || fail "F was not debited"

say "5. accounts and fees: the fee quote, then make, name and choose accounts"
N module fee | tee "$WORK/fee.json"
S=$(N module account-new public --label=Shop | tee "$WORK/new.json" | id_of Public)
[ -n "$S" ] || fail "account-new made no public account"
N module account-label "Public/$S" "Shop 2" > /dev/null
V=$(N module account-new private --label=Vault | id_of Private)
[ -n "$V" ] || fail "account-new made no private account"
N module account-use "Public/$F" > /dev/null
if N module account-use "$F" > /dev/null 2>&1; then fail "a bare account ID was taken"; fi
if N module account-new both > /dev/null 2>&1; then fail "an account kind other than public or private was taken"; fi
N module account-list > "$WORK/accounts.json"
python3 - "$WORK/fee.json" "$WORK/new.json" "$WORK/accounts.json" "$F" "$S" "$V" <<'PY'
import json, sys
fee, new, accounts = (json.load(open(path)) for path in sys.argv[1:4])
F, S, V = sys.argv[4:7]
for kind in ("public_transfer", "private_transfer"):
    about, most = int(fee[kind]["about"]), int(fee[kind]["at_most"])
    assert 0 < about <= most, (kind, fee)
# The first public account made in a wallet with no default became the default.
assert new["label"] == "Shop" and new["default"] is True and len(new["public_key"]) == 64, new
by = {row["account"]: row for row in accounts}
assert by[f"Public/{S}"]["label"] == "Shop 2", by[f"Public/{S}"]
assert by[f"Public/{F}"]["default"] is True and by[f"Public/{S}"]["default"] is False, accounts
assert by[f"Private/{V}"]["label"] == "Vault" and by[f"Private/{V}"]["default"] is True, accounts
assert by[f"Private/{V}"]["balance"] == "0" and by[f"Private/{V}"]["public_key"] is None, accounts
assert int(by[f"Public/{F}"]["balance"]) > 0, accounts
print("accounts and fee quote ok:", len(accounts), "accounts")
PY
N balance | grep -E "Public/$F .*\(default\)" || fail "names balance does not show F as the default public account"

say "6. a network drop: the sequencer stops"
F_BEFORE=$(balance "$F"); B_BEFORE=$(balance "$B")
HEIGHT=$(height)
kill "$SEQ_PID"; wait "$SEQ_PID" 2>/dev/null || true; SEQ_PID=
rpc getLastBlockId >/dev/null 2>&1 && fail "the sequencer still answers"
started=$(date +%s)
if N module fee > "$WORK/offline-fee.log" 2>&1; then fail "a fee quote came back with no sequencer"; fi
if transfer "$F" "$B" 1000 > "$WORK/offline-send.log" 2>&1; then fail "a transfer went through with no sequencer"; fi
took=$(( $(date +%s) - started ))
echo "with no sequencer, a read and a send failed in ${took}s:"; tail -1 "$WORK/offline-fee.log" | cut -c1-200; tail -1 "$WORK/offline-send.log" | cut -c1-200
[ "$took" -le 240 ] || fail "failing without a sequencer took ${took}s"
# The wallet's own settings don't need the chain.
N module account-label "Public/$S" "Shop 3" > /dev/null || fail "a nickname could not be changed offline"
N module account-use "Public/$S" > /dev/null || fail "the paying account could not be chosen offline"
N module account-use "Public/$F" > /dev/null

say "6. the sequencer restarts on the same chain"
start_sequencer
AFTER=$(height)
echo "height $HEIGHT before the stop, $AFTER after the restart"
[ "$AFTER" -ge "$HEIGHT" ] || fail "the chain went back from $HEIGHT to $AFTER"
[ "$(balance "$F")" = "$F_BEFORE" ] || fail "F's balance changed across the restart"
[ "$(balance "$B")" = "$B_BEFORE" ] || fail "B's balance changed across the restart"
N module account-list > "$WORK/accounts-after.json"
python3 - "$WORK/accounts-after.json" "$F" "$S" "$V" <<'PY'
import json, sys
accounts = {row["account"]: row for row in json.load(open(sys.argv[1]))}
F, S, V = sys.argv[2:5]
assert accounts[f"Public/{S}"]["label"] == "Shop 3", accounts[f"Public/{S}"]
assert accounts[f"Public/{F}"]["default"] is True, accounts[f"Public/{F}"]
assert accounts[f"Private/{V}"]["label"] == "Vault", accounts[f"Private/{V}"]
print("accounts kept across the drop and the restart")
PY
transfer "$F" "$B" 4242
[ "$(balance "$B")" -eq $((B_BEFORE + 4242)) ] || fail "a transfer after the restart did not land"

if [ -n "${LEZREG:-}" ]; then
  say "7. apps: $NAME publishes demo_ui, and lezreg checks packages against it"
  ROOT=$(printf 'demo_ui 1.0.0' | shasum -a 256 | cut -d' ' -f1)
  printf '{"name":"demo_ui","version":"1.0.0","author":"lns:%s","hashes":{"root":"%s"}}' "$NAME" "$ROOT" > "$WORK/demo-manifest.json"
  printf '{"name":"demo_ui","version":"1.0.0","author":"lns:%s","hashes":{"root":"%s"}}' "$NAME" "$(printf x | shasum -a 256 | cut -d' ' -f1)" > "$WORK/copy-manifest.json"
  VALUE=$("$LEZREG" --json app record "$WORK/demo-manifest.json" | python3 -c 'import json,sys; print(json.load(sys.stdin)["value"])')
  echo "app.demo_ui = $VALUE"
  NR set-text "$NAME" app.demo_ui "$VALUE" | grep -vE '^(Stored|Transaction data)' | cut -c1-200
  "$LEZREG" --rpc "$RPC" --lns "$REG" app verify "$WORK/demo-manifest.json" || fail "the published package was not verified"
  if "$LEZREG" --rpc "$RPC" --lns "$REG" app verify "$WORK/copy-manifest.json"; then fail "a copy with another root was verified"; fi
  # `app verify` exits 1 for anything but verified, so its JSON is kept before it is read.
  COPY=$("$LEZREG" --json --rpc "$RPC" --lns "$REG" app verify "$WORK/copy-manifest.json" || true)
  echo "$COPY" | grep -q '"status": "mismatch"' || fail "a copy was not reported as a mismatch: $COPY"
  "$LEZREG" --rpc "$RPC" --lns "$REG" app show "$NAME" | tee "$WORK/apps.txt"
  grep -q "^demo_ui  1.0.0  root $ROOT" "$WORK/apps.txt" || fail "app show does not list demo_ui"
  if [ -n "${STORAGE_BIN:-}" ]; then
    say "7. apps: a package published by CID on Logos Storage, installed with lezreg app get"
    mkdir -p "$WORK/storage" && chmod 700 "$WORK/storage"
    "$STORAGE_BIN" --data-dir="$WORK/storage" --api-port=8189 --listen-port=8179 --nat=extip:127.0.0.1 --block-ttl=86400 > "$WORK/storage.log" 2>&1 &
    STORAGE_PID=$!
    API=http://127.0.0.1:8189/api/storage/v1
    for _ in $(seq 1 30); do curl -sf "$API/debug/info" >/dev/null && break; sleep 1; done
    python3 - "$WORK/cid_ui-1.0.0.lgx" "$NAME" <<'MAKE'
import io, json, os, sys, tarfile
manifest = {"name": "cid_ui", "version": "1.0.0", "author": "lns:" + sys.argv[2], "hashes": {"root": "cd" * 32}}
with tarfile.open(sys.argv[1], "w:gz") as t:
    for name, data in (("manifest.json", json.dumps(manifest).encode()), ("payload.bin", os.urandom(200_000))):
        info = tarfile.TarInfo(name); info.size = len(data); t.addfile(info, io.BytesIO(data))
MAKE
    VALUE=$("$LEZREG" --json app record "$WORK/cid_ui-1.0.0.lgx" --upload --storage-api "$API" | python3 -c 'import json,sys; print(json.load(sys.stdin)["value"])')
    echo "app.cid_ui = $VALUE"
    echo "$VALUE" | grep -q '"cid":"z' || fail "the record has no CID"
    NR set-text "$NAME" app.cid_ui "$VALUE" | grep -vE '^(Stored|Transaction data)' | cut -c1-200
    "$LEZREG" --rpc "$RPC" --lns "$REG" app get "$NAME" cid_ui -o "$WORK/installed.lgx" --storage-api "$API" || fail "app get"
    cmp "$WORK/cid_ui-1.0.0.lgx" "$WORK/installed.lgx" || fail "app get wrote other bytes than were published"
    kill "$STORAGE_PID" 2>/dev/null || true
  fi
else
  echo "(step 7 skipped: set LEZREG to the registry's lezreg CLI to check apps by publisher)"
fi

say "8. back and forth: B sends to F, and F's balance takes it"
# B pays fees in this step (a send, a set-account, a transfer): fund it first.
transfer "$F" "$B" 5000000
BB0=$(balance "$B"); FB0=$(balance "$F")
transfer "$B" "$F" 1000
BB1=$(balance "$B"); FB1=$(balance "$F")
echo "B held $BB0, now $BB1; F held $FB0, now $FB1"
[ "$FB1" -eq $((FB0 + 1000)) ] || fail "F was not credited by B"
[ "$BB1" -le $((BB0 - 1000)) ] || fail "B was not debited"

say "8. a name changes hands: R transfers $LOCKED to B"
NR transfer "$LOCKED" "Public/$B" | grep -vE '^(Stored|Transaction data)' | cut -c1-200
N resolve "$LOCKED" | grep -q "\"held_by\": \"$B\"" || fail "B does not hold $LOCKED after R's transfer"
NR resolve "$LOCKED" | grep -q '"held_by": null' || fail "R still shows as holding $LOCKED"
echo "$LOCKED: R -> B"

say "8. receive-privately: R, who no longer holds $LOCKED, is refused and left no new account; B sets it up"
PRIV_R0=$(NR wallet account list 2>/dev/null | grep -c "Private/" || true)
if NR receive-privately "$LOCKED" > "$WORK/rp-refused.log" 2>&1; then fail "R set up a name it no longer holds"; fi
grep -q "does not hold" "$WORK/rp-refused.log" || { cat "$WORK/rp-refused.log"; fail "the refusal didn't say why"; }
PRIV_R1=$(NR wallet account list 2>/dev/null | grep -c "Private/" || true)
[ "$PRIV_R1" -eq "$PRIV_R0" ] || fail "a refused receive-privately still made a private account ($PRIV_R0 -> $PRIV_R1)"
N receive-privately "$LOCKED" | grep -vE '^(Stored|Transaction data)' | cut -c1-200
N resolve "$LOCKED" | grep -q '"payable_privately": true' || fail "$LOCKED has no receiving keys after B's receive-privately"
N mine | grep -q "^$LOCKED" || fail "names mine doesn't list $LOCKED after receive-privately"
echo "$LOCKED: B receives privately, and names mine lists it"

say "8. the new holder is paid by the name: B publishes Public/B on $LOCKED, R pays $LOCKED"
N set-account "$LOCKED" "Public/$B" | grep -vE '^(Stored|Transaction data)' | cut -c1-200
N resolve "$LOCKED" | grep -q "\"public_account\": \"$B\"" || fail "$LOCKED does not resolve to B's account"
BB2=$(balance "$B")
NR pay "$LOCKED" 5000 --from "Public/$R" --public | grep -vE '^(Stored|Transaction data)' | cut -c1-200
BB3=$(balance "$B")
echo "B held $BB2, now $BB3"
[ "$BB3" -eq $((BB2 + 5000)) ] || fail "paying $LOCKED by name did not reach its new holder"

say "8. and back: B transfers $LOCKED to R, which clears what B published"
N transfer "$LOCKED" "Public/$R" | grep -vE '^(Stored|Transaction data)' | cut -c1-200
NR resolve "$LOCKED" | tee "$WORK/locked-back.json" | grep -q "\"held_by\": \"$R\"" || fail "R does not hold $LOCKED again"
grep -q '"public_account": null' "$WORK/locked-back.json" || fail "the transfer back kept B's account on $LOCKED"
echo "$LOCKED: B -> R, B's account cleared"

say "the chain is still healthy"
rpc getLastBlockId
echo
say "PASS: registry, payments, tokens, testimonial, accounts and fees, a drop and a restart${LEZREG:+, apps by publisher}${STORAGE_BIN:+ and by CID}, sends back and forth, a name handed on and back, on a fresh rc1 standalone chain"
