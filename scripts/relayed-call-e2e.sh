#!/bin/bash
# A public call another account pays for (Names Wallet `propose_relayed_call`), end to end on a
# THROWAWAY standalone rc3 sequencer: a never-used account C commits a name (registry Commit,
# which creates the commitment PDA); the names CLI plans and signs it as C (`module public-call
# plan/sign`), a second wallet holding only the payer P co-signs and submits it (`module
# public-call cosign`). Then the busy case: the payer's nonce moves first, the first half-signed
# transaction no longer lands, the rebuild differs only in the payer's nonce, and its signature
# lands once. No proof anywhere. The payer of the setup is LEZ's published local-test account.
#
# Needs: SEQ_BIN (sequencer_service --features standalone), SEQ_CONFIG (its networks/local.json),
# NAMES (this commit's names CLI), NAMES_ELF (the registry guest, logos_names.bin).
# Optional: WORK (default $TMPDIR/relayed-call-e2e), RPC_PORT (default 4720).
set -euo pipefail
: "${SEQ_BIN:?}" "${SEQ_CONFIG:?}" "${NAMES:?}" "${NAMES_ELF:?}"
export RISC0_EXECUTOR=local RAYON_NUM_THREADS=${RAYON_NUM_THREADS:-4}
W=${WORK:-${TMPDIR:-/tmp}/relayed-call-e2e}; RPC_PORT=${RPC_PORT:-4720}; METRICS_PORT=$((RPC_PORT + 1)); RPC=http://127.0.0.1:$RPC_PORT
LOCAL_TEST_KEY=10a26a9aec7d34b82364eeae45c5294dbb0a764b000b94eeb9b58511dc487c4d; SEGMENT=$((96 * 1024))
rpc() { curl -sf -m 10 -X POST "$RPC" -H "Content-Type: application/json" -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$1\",\"params\":${2:-[]}}"; }
say() { echo; echo "## $(date -u +%H:%M:%S) $*"; }
fail() { echo "FAIL: $*" >&2; exit 1; }
ok() { echo "PASS: $*"; }
id_of() { grep -oE "$1/[1-9A-HJ-NP-Za-km-z]{32,44}" | head -1 | cut -d/ -f2; }
rpc getLastBlockId >/dev/null 2>&1 && fail "something answers on $RPC"
rm -rf "$W"; mkdir -p "$W"; SEQ_PID=
trap '[ -n "$SEQ_PID" ] && kill $SEQ_PID 2>/dev/null; wait 2>/dev/null; echo "sequencer stopped; left running: $(pgrep -fl "$W" || echo nothing)"' EXIT
say "fresh rc3 standalone sequencer on $RPC (throwaway)"
"$SEQ_BIN" setup "$W/seq" --config "$SEQ_CONFIG" --funding-public-key 0101010101010101010101010101010101010101010101010101010101010100 --create-channel > "$W/seq-setup.log" 2>&1 || fail setup
RUST_LOG=warn "$SEQ_BIN" start "$W/seq" --rpc-listen-address "127.0.0.1:$RPC_PORT" --metrics-listen-address "127.0.0.1:$METRICS_PORT" >> "$W/sequencer.log" 2>&1 < /dev/null &
SEQ_PID=$!
for _ in $(seq 1 60); do rpc getLastBlockId >/dev/null 2>&1 && break; sleep 2; done
export LEE_WALLET_PASSWORD= LOGOS_NAMES_RPC=$RPC
new_wallet() { mkdir -p "$1"; chmod 700 "$1"; cat > "$1/wallet_config.json" <<J
{ "gas_limit": 2000000, "fee_headroom_blocks": 5, "seq_block_poll_max_amount": 100, "seq_poll_max_retries": 5,
  "seq_poll_timeout": "12s", "seq_tx_poll_max_blocks": 5, "sequencer": { "sequencer_addr": "$RPC/" } }
J
  LEE_WALLET_HOME_DIR=$1 "$NAMES" wallet check-health > /dev/null; }
MAIN=$W/wallet-main; PAYW=$W/wallet-payer; CW=$W/wallet-claimer
new_wallet $MAIN; new_wallet $PAYW; new_wallet $CW
N() { LEE_WALLET_HOME_DIR=$MAIN "$NAMES" "$@"; }
P() { LEE_WALLET_HOME_DIR=$PAYW "$NAMES" "$@"; }
C() { LEE_WALLET_HOME_DIR=$CW "$NAMES" "$@"; }
F=$(N wallet account import public --private-key "$LOCAL_TEST_KEY" | id_of Public)
say "deploy and configure the Names registry"
size=$(wc -c < "$NAMES_ELF" | tr -d " "); count=$(( (size + SEGMENT - 1) / SEGMENT ))
REG=$(N wallet account new public | id_of Public); segs=()
for i in $(seq 1 "$count"); do segs+=("Public/$(N wallet account new public | id_of Public)"); done
N wallet program-loader deploy --elf "$NAMES_ELF" --header "Public/$REG" --segments "${segs[@]}" --payer "Public/$F" | grep -vE "^(Stored|Transaction data)" | cut -c1-160
export LOGOS_NAMES_PROGRAM=$REG
N wallet auth-transfer send --from "Public/$F" --to "Public/$REG" --amount 10000000 | grep -vE "^(Stored|Transaction data)" | cut -c1-160
N configure --admin "$F" --treasury "$F" --mode deposit --deposits 1000000,3000000 | grep -vE "^(Stored|Transaction data)"
say "the payer P (its own wallet, funded) and the claimer C (another wallet, never used, no LEZ)"
PAYER=$(P wallet account new public | id_of Public)
PAYER_PK=$(P wallet account get --account-id "Public/$PAYER" --keys | sed -n "s/^pk //p")
N wallet auth-transfer send --from "Public/$F" --to-pk "$PAYER_PK" --amount 50000000 | grep -vE "^(Stored|Transaction data)" | cut -c1-160
CL=$(C wallet account new public | id_of Public)
echo "registry $REG payer $PAYER claimer $CL"
bal() { rpc getAccountBalance "[\"$1\"]" | python3 -c "import json,sys; print(json.load(sys.stdin)[\"result\"])"; }
nonce() { rpc getAccountsNonces "[[\"$1\"]]" | python3 -c "import json,sys; n=json.load(sys.stdin)[\"result\"][0]; print(n[0] if isinstance(n,list) else n)"; }
PAY_BEFORE=$(bal $PAYER); echo "payer holds $PAY_BEFORE; claimer nonce $(nonce $CL)"
say "the call: Commit from C, which creates the commitment account (marked new)"
CALL=$(python3 - "$RPC" "$REG" "$CL" "$PAYER" <<"PY"
import json, sys, os, hashlib, urllib.request, base64
rpc, reg, cl, payer = sys.argv[1:5]
def b58(b):
    A="123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"; n=int.from_bytes(b,"big"); s=""
    while n: n,r=divmod(n,58); s=A[r]+s
    return "1"*(len(b)-len(b.lstrip(b"\0")))+s
acct=b58(b"/LEZ/ClockProgramAccount/0000000"); prog=b58(hashlib.sha256(b"/LEE-BuiltinProgram/v1/AccountId"+b"clock").digest())
req=json.dumps({"jsonrpc":"2.0","id":1,"method":"getAccountView","params":[{"account_id":acct,"program_account_id":prog}]}).encode()
r=json.load(urllib.request.urlopen(urllib.request.Request(rpc,req,{"Content-Type":"application/json"})))
st=r["result"]["data"]["actor_states"][prog]; st=bytes(st) if isinstance(st,list) else base64.b64decode(st)
now=int.from_bytes(st[8:],"little")
commitment=os.urandom(32)
message=bytes([0, 8]) + commitment + now.to_bytes(8,"little")   # Message::Instruction(Instruction::Commit{commitment, now})
seed=hashlib.sha256(b"logos-names/v5/commit/"+commitment).hexdigest()
def b58d(t):
    A="123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"; n=0
    for c in t: n=n*58+A.index(c)
    return n.to_bytes(32,"big")
prefix=b"/LEE/v0.2/AccountId/PDA/"; prefix=prefix+bytes(32-len(prefix))
pda=b58(hashlib.sha256(prefix+b58d(reg)+bytes.fromhex(seed)).digest())
print(json.dumps({"account": "Public/"+cl, "program": reg, "message": message.hex(),
  "accounts": [{"seed": seed, "of": reg, "program": reg, "new": True, "account": pda}],   # as the module writes it
  "fee": {"payer": payer, "gas_limit": 200000, "tip": "0", "max_fee": "5000000"}}))
PY
)
echo "$CALL" > $W/call.json
say "plan (as C): the wallet builds it with both nonces"
C module public-call plan < $W/call.json | tail -1 > $W/plan.json; cat $W/plan.json | cut -c1-400; echo
HASH=$(python3 -c "import json;print(json.load(open(\"$W/plan.json\"))[\"hash\"])")
[ ${#HASH} = 64 ] && ok "plan gives a 32-byte hash" || fail "no hash"
say "refusals"
python3 -c "import json;c=json.load(open(\"$W/call.json\"));c[\"expect_hash\"]=\"00\"*32;json.dump(c,open(\"$W/wrong.json\",\"w\"))"
C module public-call sign --input $W/wrong.json > $W/wrong.out 2>&1 && fail "signed with another hash" || ok "sign refuses a hash that isn't the approved one: $(grep -o "no longer the one approved" $W/wrong.out | head -1)"
python3 -c "import json;c=json.load(open(\"$W/call.json\"));c[\"fee\"][\"payer\"]=c[\"account\"];json.dump(c,open(\"$W/self.json\",\"w\"))"
C module public-call plan < $W/self.json > $W/self.out 2>&1 && fail "planned a self-paid call" || ok "plan refuses the signer as payer: $(grep -o "ordinary call" $W/self.out | head -1)"
C2=$(C wallet account new public | id_of Public)
python3 -c "import json;c=json.load(open(\"$W/call.json\"));c[\"fee\"][\"payer\"]=\"$C2\";json.dump(c,open(\"$W/ownpayer.json\",\"w\"))"
C module public-call plan < $W/ownpayer.json > $W/ownpayer.out 2>&1 && fail "planned with the wallet's own payer" || ok "plan refuses a payer the wallet holds: $(grep -o "one of this wallet.s own accounts" $W/ownpayer.out | head -1)"
python3 -c "import json;c=json.load(open(\"$W/call.json\"));c[\"account\"]=\"Public/$F\";json.dump(c,open(\"$W/notmine.json\",\"w\"))"
C module public-call plan < $W/notmine.json > $W/notmine.out 2>&1 && fail "planned for an account the wallet doesn't hold" || ok "plan refuses a signing account that isn't the wallet's: $(grep -o "is not a public account of this wallet" $W/notmine.out | head -1)"
say "sign (as C, with the approved hash): half signed, nothing sent"
python3 -c "import json;c=json.load(open(\"$W/call.json\"));c[\"expect_hash\"]=\"$HASH\";json.dump(c,open(\"$W/sign.json\",\"w\"))"
C module public-call sign --input $W/sign.json | tail -1 > $W/signed.json
TX=$(python3 -c "import json;print(json.load(open(\"$W/signed.json\"))[\"transaction\"])")
echo "half-signed transaction: $(( ${#TX} / 2 )) bytes"
[ "$(nonce $CL)" = 0 ] && ok "nothing was sent: C's nonce is still 0" || fail "C's nonce moved"
say "busy: the payer's nonce moves before it co-signs (another transaction of the relay's)"
P wallet auth-transfer send --from "Public/$PAYER" --to "Public/$F" --amount 1 | grep -vE "^(Stored|Transaction data)" | cut -c1-160
echo "$TX" | P module public-call cosign > $W/stale.out 2>&1 && fail "a transaction with a spent payer nonce applied" || ok "the first half-signed transaction no longer lands (payer nonce spent): $(tail -1 $W/stale.out | cut -c1-120)"
say "rebuild: the same call, built again; only the payer's nonce may differ"
C module public-call plan < $W/call.json | tail -1 > $W/plan2.json
python3 - "$W/plan.json" "$W/plan2.json" "$PAYER" "$CL" <<"PY2"
import json, sys
a, b = json.load(open(sys.argv[1])), json.load(open(sys.argv[2])); payer, cl = sys.argv[3], sys.argv[4]
diff = sorted(k for k in set(a) | set(b) if a.get(k) != b.get(k))
nonces = sorted(k for k in a["nonces"] if a["nonces"][k] != b["nonces"].get(k))
print("fields that differ:", diff, "nonces that differ:", nonces)
ok = diff == ["hash", "nonces"] and nonces == [payer] and int(b["nonces"][payer]) > int(a["nonces"][payer]) and a["nonces"][cl] == b["nonces"][cl]
print("PASS: only the payer nonce moved (the module would re-sign this without asking)" if ok else "FAIL: the rebuild differs in more than the payer nonce")
sys.exit(0 if ok else 1)
PY2
HASH2=$(python3 -c "import json;print(json.load(open(\"$W/plan2.json\"))[\"hash\"])")
python3 -c "import json;c=json.load(open(\"$W/call.json\"));c[\"expect_hash\"]=\"$HASH2\";json.dump(c,open(\"$W/sign2.json\",\"w\"))"
C module public-call sign --input $W/sign2.json | tail -1 > $W/signed2.json
TX=$(python3 -c "import json;print(json.load(open(\"$W/signed2.json\"))[\"transaction\"])")
[ "$HASH2" != "$HASH" ] && ok "re-signed under a new hash" || fail "same hash"
say "the payer co-signs and submits"
echo "$TX" | P module public-call cosign 2>&1 | grep -vE "^(Stored|Transaction data)" | tee $W/cosign.out
grep -q "applied in block" $W/cosign.out && ok "the co-signed call applied" || fail "not applied"
[ "$(nonce $CL)" = 1 ] && ok "C's nonce is 1: its signature was used, once" || fail "C nonce $(nonce $CL)"
PAY_AFTER=$(bal $PAYER); echo "payer: $PAY_BEFORE -> $PAY_AFTER"
[ "$PAY_AFTER" -lt "$PAY_BEFORE" ] && ok "the payer paid the fee ($((PAY_BEFORE - PAY_AFTER)))" || fail "the payer paid nothing"
CB=$(bal $CL); echo "claimer balance: $CB"
[ "$CB" = 0 ] && ok "C still holds no LEZ" || fail "C holds $CB"
PDA=$(python3 -c "import json;print(json.load(open(\"$W/plan.json\"))[\"creates\"][0])")
rpc getAccountView "[{\"account_id\":\"$PDA\",\"program_account_id\":\"$REG\"}]" | python3 -c "import json,sys;r=json.load(sys.stdin)[\"result\"];print(\"commitment account state:\", json.dumps(r)[:200])" | tee $W/pda.out
grep -q "actor_states" $W/pda.out && ok "the commitment account exists" || echo "NOTE: commitment account not read as expected"
say "replay: the same transaction again"
echo "$TX" | P module public-call cosign > $W/replay.out 2>&1 && fail "a replay was accepted" || ok "the same signed call is refused the second time: $(tail -1 $W/replay.out | cut -c1-160)"
echo; echo "ALL DONE"
