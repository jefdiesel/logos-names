#!/bin/bash
# Offline checks of the deploy kit (no chain, no binaries): lib.py's self-test, the live
# params file in dry-run mode, and that each guard refuses what it should. The end-to-end run
# against a real local chain is deploy/rehearse.sh.
set -uo pipefail
HERE=$(cd "$(dirname "$0")" && pwd); REPO=$(cd "$HERE/.." && pwd); PY=${PY:-python3}
T=$(mktemp -d); trap 'rm -rf "$T"' EXIT
pass=0; fail=0
check() { if eval "$2" >/dev/null 2>&1; then echo "ok   $1"; pass=$((pass+1)); else echo "FAIL $1"; fail=$((fail+1)); fi; }
refuses() { # NAME CMD WHY: CMD must fail, and its output must say WHY (so it failed for that reason)
  local out; if out=$(eval "$2" 2>&1); then echo "FAIL $1 (accepted)"; fail=$((fail+1))
  elif ! grep -q -- "$3" <<<"$out"; then echo "FAIL $1 (refused, but not for: $3)"; fail=$((fail+1))
  else echo "ok   $1"; pass=$((pass+1)); fi; }
V() { $PY "$HERE/lib.py" validate "$1" "$REPO" ${2:-}; }
edit() { $PY - "$HERE/params.toml" "$T/p.toml" "$1" "$2" <<'PY'
import re, sys
s = open(sys.argv[1]).read(); s = re.sub(sys.argv[3], sys.argv[4], s, count=1, flags=re.M); open(sys.argv[2], "w").write(s)
PY
}

for f in deploy.sh rehearse.sh post-deploy.sh test-dry-run.sh; do check "bash -n $f" "bash -n '$HERE/$f'"; done
check "lib.py self-test (SPEC.md vectors, PDAs, Configure bytes, premium curve, params.toml immutables)" "$PY '$HERE/lib.py' self-test"
check "params.toml passes in dry-run mode (placeholders are warnings)" "V '$HERE/params.toml' --dry-run"
refuses "params.toml refuses a live send (Monday values not confirmed / placeholders)" "V '$HERE/params.toml'" "not confirmed by Jef"
edit '^shortlist_root = "9778.*$' 'shortlist_root = "00"'; refuses "a shortlist root that is not the compiled one" "V '$T/p.toml' --dry-run" "shortlist_root"
edit '^grace_days = 30$' 'grace_days = 7'; refuses "a grace period that is not the compiled one" "V '$T/p.toml' --dry-run" "grace_days"
edit '^deed_image = "lns.*$' 'deed_image = "https://example.com/x.png"'; refuses "a deed image that is not the compiled one" "V '$T/p.toml' --dry-run" "deed_image"
edit '^\[keys\]$' '[keys]\nprivate_key = "x"'; refuses "a private_key field anywhere" "V '$T/p.toml' --dry-run" "keys.private_key"
edit '^\[keys\]$' '[keys]\npassword = "x"'; refuses "a password field anywhere" "V '$T/p.toml' --dry-run" "keys.password"
edit '^rehearsal = false$' 'rehearsal = true'; refuses "a rehearsal file whose RPC is not loopback" "V '$T/p.toml' --dry-run" "loopback RPC"
edit '^upgradeable = true$' 'upgradeable = false'; refuses "upgradeable = false on testnet" "V '$T/p.toml' --dry-run" "upgradeable must be true"
edit '^sale_fee_bps = true$' 'sale_fee_bps = "true"'; refuses "a confirmation that is not exactly true" "V '$T/p.toml'" "sale_fee_bps"
edit '^rpc_url = "PLACEHOLDER.*$' 'rpc_url = "http://127.0.0.1:3040"'; refuses "a live file pointing at a loopback RPC" "V '$T/p.toml' --dry-run" "live params file points at a loopback"
echo "$pass passed, $fail failed"
[ $fail = 0 ]
