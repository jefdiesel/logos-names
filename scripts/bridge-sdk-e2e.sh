#!/bin/bash
# Dry-run end-to-end of the bridge SDK's wallet side on a local LEZ 0.4 sequencer: the names CLI
# (as the Names module runs it) plans the RFP-022 v2 demo's private attested claim of leaf 1,
# a drain disguised as a call, and derives app secrets. NOTHING IS PROVEN OR SENT: only
# `module private-call plan` (the wallet's dry run plus the open replay) and `module app-secret`.
#
#   scripts/bridge-sdk-e2e.sh <names binary> <work dir with wallet/ and claim-leaf1.json> [rpc]
#
# <work dir>/wallet must be a COPY of a test wallet whose sequencer is loopback (the script
# refuses any other), holding the claimer V. Output: <work dir>/out/*.json and a PASS/FAIL list.
set -uo pipefail
NAMES=${1:?names binary}
W=${2:?work dir}
RPC=${3:-http://127.0.0.1:3250}
V=2if1EJVcZwFT3oXxF4uewB2mk2XDXpmCWwTtBwUQ6HUC
P=6NAoZKXE14LJGeeASmpzpwvsuDLUDB2r6v2a2dXa4f4f
TP=4VScJ7mtwaanQ5gcSZZw2iAMHYAbLrVmbYKwEGx2eSex
DUSDC=FBqNAwJs9T4eNDw9pQXxXrLZNcnR1sTnnBEYzhudLQZJ
DETH=CswQqK9jzUHf5sraiM8G9HfgMT2bcJ8GhVpbAA9trwQ6
case "$RPC" in http://127.0.0.1:*|http://localhost:*) ;; *) echo "refusing a non-loopback sequencer: $RPC"; exit 2;; esac
grep -q '"sequencer_addr": "http://127.0.0.1' "$W/wallet/wallet_config.json" || { echo "the wallet copy's sequencer is not loopback; fix it first"; exit 2; }
export LEE_WALLET_HOME_DIR=$W/wallet TMPDIR=$W/tmp LOGOS_NAMES_RPC=$RPC
unset RISC0_PROVER
mkdir -p "$W/out" "$W/tmp"; chmod 700 "$W" "$W/out"
pass=0; fail=0
ok() { echo "PASS $*"; pass=$((pass+1)); }
no() { echo "FAIL $*"; fail=$((fail+1)); }
names() { "$NAMES" --rpc "$RPC" "$@"; }

# The CLI input, as the module writes it (provider_private::cli_input).
python3 -I - "$W/claim-leaf1.json" "$W/out/leaf1.input.json" "$V" "$TP" <<'EOF'
import json, os, sys
call = json.load(open(sys.argv[1]))
call.update({"account": "Private/" + sys.argv[3], "token_programs": [sys.argv[4]]})
fd = os.open(sys.argv[2], os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
with os.fdopen(fd, "w") as f: json.dump(call, f)
EOF

echo "## 1. plan the private attested claim of leaf 1 (dry run)"
start=$(date +%s)
names module private-call plan < "$W/out/leaf1.input.json" > "$W/out/leaf1.plan.log" 2>&1; code=$?
echo "exit $code in $(( $(date +%s) - start ))s"
python3 -I - "$W/out/leaf1.plan.log" "$W/out/leaf1.report.json" <<'EOF'
import json, sys
lines = [l for l in open(sys.argv[1]) if l.startswith('{"report"')]
if lines:
    json.dump(json.loads(lines[-1])["report"], open(sys.argv[2], "w"), indent=1)
EOF
if [ -s "$W/out/leaf1.report.json" ]; then ok "the claim plans"; else no "the claim plans"; tail -5 "$W/out/leaf1.plan.log"; fi
python3 -I - "$W/out/leaf1.report.json" "$V" "$DUSDC" <<'EOF' && ok "the report shows 100 dUSDC and 20,000,000 LEZ to V, the float's -20,000,000, V's ID not public" || no "the report's numbers"
import json, sys
r = json.load(open(sys.argv[1])); V, D = sys.argv[2], sys.argv[3]
own = {(x["asset"], x.get("definition")): x for x in r["own"]}
tok = own[("token", D)]; lez = own[("LEZ", None)]
assert int(tok["after"]) - int(tok["before"]) == 100_000_000, tok
assert int(lez["after"]) - int(lez["before"]) == 20_000_000, lez
natives = [x for x in r["public"] if x["kind"] == "native"]
assert any(int(x["after"]) - int(x["before"]) == -20_000_000 for x in natives), natives
sup = [x for x in r["public"] if x["kind"] == "token_definition" and x["definition"] == D]
assert sup and int(sup[0]["after"]) - int(sup[0]["before"]) == 100_000_000, sup
assert r["account_public"] is False and r["account"] == "Private/" + V
assert r["aliases"] and int(r["fee"]["about"]) > 0
print(json.dumps({"fee": r["fee"], "aliases": r["aliases"], "public_cycles": r["public_cycles"], "message_len": r["message_len"], "digest": r["digest"][:16]}))
EOF
grep -q "$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["message"][2:66])' "$W/out/leaf1.input.json" 2>/dev/null)" "$W/out/leaf1.plan.log" && no "the plan's log carries message bytes" || ok "the plan's log carries no message bytes"

echo "## 2. plan it again: same digest (nothing random in what the person approves)"
names module private-call plan < "$W/out/leaf1.input.json" > "$W/out/leaf1.plan2.log" 2>&1
d1=$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["digest"])' "$W/out/leaf1.report.json" 2>/dev/null)
d2=$(grep '^{"report"' "$W/out/leaf1.plan2.log" | python3 -I -c 'import json,sys; print(json.loads(sys.stdin.read())["report"]["digest"])' 2>/dev/null)
[ -n "$d1" ] && [ "$d1" = "$d2" ] && ok "the digest is stable" || no "the digest is stable ($d1 vs $d2)"

echo "## 3. a send whose digest isn't the approved one is refused before proving"
python3 -I - "$W/out/leaf1.input.json" "$W/out/leaf1.badsend.json" <<'EOF'
import json, os, sys
c = json.load(open(sys.argv[1])); c["expect_digest"] = "00" * 32
fd = os.open(sys.argv[2], os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
with os.fdopen(fd, "w") as f: json.dump(c, f)
EOF
names module private-call send --input="$W/out/leaf1.badsend.json" > "$W/out/leaf1.badsend.log" 2>&1
grep -q "has changed since it was approved" "$W/out/leaf1.badsend.log" && ! grep -q "phase: prove" "$W/out/leaf1.badsend.log" && ok "a changed digest stops it before proving" || { no "a changed digest stops it before proving"; tail -3 "$W/out/leaf1.badsend.log"; }

echo "## 4. a drain disguised as a call: V's whole dUSDC to P"
python3 -I - "$W/out/drain.input.json" "$V" "$TP" "$DUSDC" "$P" <<'EOF'
import json, os, sys
B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
def b58(t):
    n = 0
    for c in t: n = n * 58 + B58.index(c)
    raw = n.to_bytes((n.bit_length() + 7) // 8, "big")
    return b"\0" * (len(t) - len(t.lstrip("1"))) + raw
out, V, TP, D, P = sys.argv[1:]
msg = bytes([0]) + b58(D) + b58(P) + (100_000_000).to_bytes(16, "little")   # token Transfer (tag 0)
call = {"account": "Private/" + V, "programs": [TP], "accounts": [{"account": P, "program": TP}], "message": msg.hex(), "openings": [],
        "fee_payer": "account", "token_programs": [TP]}
fd = os.open(out, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
with os.fdopen(fd, "w") as f: json.dump(call, f)
EOF
names module private-call plan < "$W/out/drain.input.json" > "$W/out/drain.plan.log" 2>&1
grep '^{"report"' "$W/out/drain.plan.log" | python3 -I -c '
import json, sys
r = json.loads(sys.stdin.read())["report"]
t = [x for x in r["own"] if x["asset"] == "token"]
assert t and int(t[0]["after"]) - int(t[0]["before"]) == -100_000_000, r["own"]
json.dump(r, open(sys.argv[1], "w"), indent=1)
print("drain shown:", t[0])' "$W/out/drain.report.json" && ok "the drain shows as V paying 100,000,000 dUSDC" || { no "the drain shows"; tail -4 "$W/out/drain.plan.log"; }

echo "## 5. app secrets: deterministic, separated by app and scope, refused for a foreign account"
secret() { printf '%s' "$1" | names module app-secret 2>/dev/null | tail -1; }
A='{"app":"lez_bridge","account":"Private/'$V'","scope":"RFP022/deposit","context":"abcd","tag":"RFP022/ck","first":0,"count":3}'
B='{"app":"other_app","account":"Private/'$V'","scope":"RFP022/deposit","context":"abcd","tag":"RFP022/ck","first":0,"count":3}'
C='{"app":"lez_bridge","account":"Private/'$P'","scope":"RFP022/deposit","context":"abcd","tag":"RFP022/ck","first":0,"count":1}'
a1=$(secret "$A"); a2=$(secret "$A"); b1=$(secret "$B")
[ -n "$a1" ] && [ "$a1" = "$a2" ] && ok "the same ask gives the same 3 values" || no "app secret determinism ($a1)"
[ -n "$b1" ] && [ "$a1" != "$b1" ] && ok "another app gets other values" || no "app separation"
printf '%s' "$C" | names module app-secret > "$W/out/secret-foreign.log" 2>&1 && no "a Private/ that is a public account is refused" || ok "a private ask for a public account is refused"
echo "$a1" | python3 -I -c 'import json,sys; v=json.loads(sys.stdin.read())["values"]; assert len(v)==3 and all(len(x)==64 for x in v)' && ok "values are 3 × 32 bytes" || no "value shape"

echo "## 6. bridged tokens: the wallet's own token read, pointed at the bridge's token program"
names --token-program="$TP" token list --json > "$W/out/holdings.log" 2>&1
tail -1 "$W/out/holdings.log" | python3 -I -c '
import json, sys
v = json.loads(sys.stdin.read()); V, D = sys.argv[1], sys.argv[2]
rows = [r for r in v.get("holdings", []) if r.get("definition") == D]
mine = [r for r in rows if r["account"] == "Private/" + V]
assert mine and int(mine[0]["balance"]) == 100_000_000, rows
print("holdings:", [(r["account"][:14], r["balance"], r.get("name")) for r in rows])' "$V" "$DUSDC" && ok "the wallet lists V's 100,000,000 bridged dUSDC through its own token code" || { no "bridged holdings"; tail -3 "$W/out/holdings.log"; }

echo "pass $pass fail $fail"
[ "$fail" -eq 0 ]
