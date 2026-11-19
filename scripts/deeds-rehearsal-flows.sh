#!/bin/bash
# lns-deeds rehearsal flows on the throwaway rc3 chain (127.0.0.1:3360, dev receipts).
# Phases as last run: upgrade2 | reg | flows | more | rereg. Run before them from an earlier copy
# of this script (same helpers): `upgrade` (the first in-place update, to an intermediate image
# c8514dd2… that minted on a RegisterWithDeed variant, since dropped), `mkc` (user C) and
# `start-regs` (the old CLI's plain registration of lns-deeds-oldclient, block 455). The deployed
# registry was made first with deploy-our04.sh (D=rehearsal, REHEARSAL=1) and the old image.
set -u
D=/Volumes/exos162/logos/fix-1119/lns-deeds; R=$D/rehearsal; L=$D/logs
. $R/state.env; . $D/users/state.env; . $D/nftdeploy/state.env
export LOGOS_NAMES_RPC=http://127.0.0.1:3360 LOGOS_NAMES_PROGRAM=$H RISC0_DEV_MODE=1 RISC0_PROVER=local
NEW=$D/target/release/names; OLD=$R/bin/names
PRE=lns-deeds-preexisting; FRESH=lns-deeds-fresh; OLDNAME=lns-deeds-oldclient
RPW=$(cat $R/.wallet-password); UPW=$(cat $D/users/.pw)
inw() { local w=$1 pw=$2; shift 2; LEE_WALLET_HOME_DIR=$w LEE_WALLET_PASSWORD=$pw "$@"; }
asU() { inw $R/wallet-smoke "$RPW" "$@"; }
asP() { inw $R/wallet "$RPW" "$@"; }
asA() { inw $D/users/a "$UPW" "$@"; }
asB() { inw $D/users/b "$UPW" "$@"; }
asC() { inw $D/users/c "$UPW" "$@"; }
LOG=$L/flows.log
step() { echo; echo "=== $*" | tee -a $LOG; }
run() { echo "\$ $*" >> $LOG; "$@" 2>&1 | grep -vE "^(Stored|Transaction data)" | cut -c1-400 | tee -a $LOG; return ${PIPESTATUS[0]}; }
expect_fail() { echo "\$ (expect refusal) $*" >> $LOG; if "$@" >> $LOG.tmp 2>&1; then cat $LOG.tmp >> $LOG; rm -f $LOG.tmp; echo "UNEXPECTED: it was accepted" | tee -a $LOG; return 1; fi; grep -vE "^(Stored|Transaction data)" $LOG.tmp | cut -c1-400 | tail -6 | tee -a $LOG; rm -f $LOG.tmp; echo "refused, as expected" | tee -a $LOG; }
header() { python3 - "$1" <<'PY'
import json, sys, urllib.request, base64
def b58(b):
    A = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"; n = int.from_bytes(b, "big"); s = ""
    while n: n, r = divmod(n, 58); s = A[r] + s
    return "1" * (len(b) - len(b.lstrip(b"\0"))) + s
loader = b58(bytes([0xFE] * 32))
req = json.dumps({"jsonrpc": "2.0", "id": 1, "method": "getAccountView", "params": [{"account_id": sys.argv[1], "program_account_id": loader}]}).encode()
r = json.load(urllib.request.urlopen(urllib.request.Request("http://127.0.0.1:3360", req, {"Content-Type": "application/json"}), timeout=15))
st = r["result"]["data"]["actor_states"][loader]; st = bytes(st) if isinstance(st, list) else base64.b64decode(st)
print("image", st[1:33].hex(), "immutable", st[65])
PY
}
state_of() { python3 - "$1" "$2" <<'PY'
import json, sys, urllib.request, base64
req = json.dumps({"jsonrpc": "2.0", "id": 1, "method": "getAccountView", "params": [{"account_id": sys.argv[1], "program_account_id": sys.argv[2]}]}).encode()
r = json.load(urllib.request.urlopen(urllib.request.Request("http://127.0.0.1:3360", req, {"Content-Type": "application/json"}), timeout=15))
st = ((r.get("result") or {}).get("data") or {}).get("actor_states", {}).get(sys.argv[2])
st = bytes(st) if isinstance(st, list) else (base64.b64decode(st) if st else b"")
print(len(st), st.hex())
PY
}
json_get() { python3 -c 'import json,sys; d=json.load(sys.stdin); 
for k in sys.argv[1].split("."): d=d[k] if d is not None else None
print(d)' "$1"; }

raw() { inw $1 "$UPW" /Volumes/exos162/logos/lp0001/job-a/out/nft-drive-v2 --payer Public/$2 raw --program $NH --root $3 --hex $4; }
b58hex() { python3 -c "
A='123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'; n=0
for c in '$1': n=n*58+A.index(c)
print(n.to_bytes(32,'big').hex())"; }
export NFT_DRIVE_SEQUENCER=http://127.0.0.1:3360

case ${1:-} in
upgrade2)
  step "second in-place update: the optional-claim image"
  run header $H
  for i in 1 2 3 4 5 6; do id=$(asP $OLD wallet account new public | grep -oE "Public/[1-9A-HJ-NP-Za-km-z]{32,44}" | head -1 | cut -d/ -f2); echo "U2_S$i=$id" >> $R/state.env; done
  . $R/state.env
  segs=(); for i in 1 2 3 4 5 6; do v=U2_S$i; segs+=("Public/${!v}"); done
  n=$(asP $OLD wallet --dry-run program-loader update --elf $D/new_logos_names.bin --header Public/$H --segments "${segs[@]}" --payer Public/$P 2>&1 | grep -oE "splits into [0-9]+" | grep -oE "[0-9]+")
  echo "segments needed: ${n:-6}" | tee -a $LOG
  segs=("${segs[@]:0:${n:-6}}")
  run asP $OLD wallet program-loader update --elf $D/new_logos_names.bin --header Public/$H --segments "${segs[@]}" --payer Public/$P
  run header $H
  ;;
reg)
  step "R1. register $FRESH (A, new CLI): no deed is minted"
  run asA $NEW register $FRESH --owner Public/$A --wait
  run asA $NEW deed show $FRESH
  ;;
flows)
  step "F1. the deployed CLI still reads the name registered before the update"
  run asU $OLD resolve $PRE
  step "F2. transfer with no deed (A -> B, new CLI): as today"
  run asA $NEW transfer $FRESH Public/$B
  run asB $NEW deed show $FRESH
  step "F3. B (the holder) claims the deed, free"
  BB=$(asB $NEW wallet account get --account-id Public/$B | sed -nE 's/^Balance ([0-9]+).*/\1/p')
  run asB $NEW deed claim $FRESH
  BA=$(asB $NEW wallet account get --account-id Public/$B | sed -nE 's/^Balance ([0-9]+).*/\1/p')
  echo "B's balance $BB -> $BA (the difference is the gas fee only)" | tee -a $LOG
  run asB $NEW deed show $FRESH
  step "F4. a second claim is refused"
  expect_fail asB $NEW deed claim $FRESH
  step "F5. a claim by someone who doesn't hold the name is refused (A signs; the CLI's own holder check skipped with --signer, so the program refuses)"
  expect_fail asA $NEW deed claim $FRESH --signer Public/$A
  step "F6. transfer with the deed, public -> public (B -> A): the deed moves"
  run asB $NEW transfer $FRESH Public/$A
  run asA $NEW deed show $FRESH
  AH=$(asA $NEW deed show $FRESH 2>/dev/null | json_get holding); DEF=$(asA $NEW deed show $FRESH 2>/dev/null | json_get definition)
  echo "A's deed account $AH, definition $DEF" | tee -a $LOG
  step "F7. the deployed CLI cannot transfer a deeded name (it doesn't declare the deed)"
  expect_fail asA $OLD transfer $FRESH Public/$B
  step "F8. moving or burning the deed directly through the NFT program is refused (holder A, and B)"
  expect_fail raw $D/users/a $A $AH 02$(b58hex $A)
  expect_fail raw $D/users/a $A $AH 0401000000000000000000000000000000
  expect_fail raw $D/users/b $B $AH 02$(b58hex $B)
  run state_of $AH $NH
  step "F9. transfer with the deed, public -> private (A -> Private/$PV in B's wallet): the deed moves"
  run asA $NEW transfer $FRESH Private/$PV
  run asB $NEW deed show $FRESH
  PH=$(asB $NEW deed show $FRESH 2>/dev/null | json_get holding)
  step "F10. the private holder releases (private transaction, dev receipts): the deed burns"
  run asB $NEW wallet auth-transfer send --from Public/$B --to Private/$PV --amount 200000000
  run asB $NEW release $FRESH
  run asB $NEW resolve $FRESH
  echo "definition $DEF and the private holder's deed account $PH after the release:" | tee -a $LOG
  run state_of $DEF $NH
  run state_of $PH $NH
  echo "DEF1=$DEF" > $D/users/fresh1.env
  step "F11. the name registered before deeds: its holder claims one on the updated registry"
  run asU $NEW deed show $PRE
  run asU $NEW deed claim $PRE
  run asU $NEW deed show $PRE
  step "F12. the deployed CLI's plain registration (made against the update) is a deedless v6 record"
  tail -6 $L/reg-old.log | tee -a $LOG
  run asC $NEW deed show $OLDNAME
  run state_of $(asC $NEW resolve $OLDNAME >/dev/null 2>&1; python3 -c "print('')") $H >/dev/null 2>&1 || true
  run asC $OLD transfer $OLDNAME Public/$A
  run asA $OLD resolve $OLDNAME
  ;;
rereg)
  step "R2. $FRESH registered again (A) and a fresh deed claimed: new definition"
  . $D/users/fresh1.env
  run asA $NEW register $FRESH --owner Public/$A --wait
  run asA $NEW deed show $FRESH
  run asA $NEW deed claim $FRESH
  run asA $NEW deed show $FRESH
  DEF2=$(asA $NEW deed show $FRESH 2>/dev/null | json_get definition)
  echo "first definition $DEF1, second $DEF2" | tee -a $LOG
  [ "$DEF1" != "$DEF2" ] && echo "new definition: OK" | tee -a $LOG
  ;;
more)
  step "M1. the deployed CLI transfers a deedless name after the update (C -> A)"
  run asC $OLD transfer $OLDNAME Public/$A
  run asA $OLD resolve $OLDNAME
  step "M2. a claim by someone who doesn't hold the name is refused by the program (B signs for A's name)"
  expect_fail asB $NEW deed claim $OLDNAME --signer Public/$B
  run asA $NEW deed show $OLDNAME
  step "M3. the holder (A) claims it; a second claim is refused"
  run asA $NEW deed claim $OLDNAME
  expect_fail asA $NEW deed claim $OLDNAME
  run asA $NEW deed show $OLDNAME
  step "M4. the holder moves or burns the deed directly through the NFT program: refused by the NFT program"
  expect_fail asA $NEW deed probe-direct $OLDNAME
  expect_fail asA $NEW deed probe-direct $OLDNAME --burn
  run asA $NEW deed show $OLDNAME
  ;;
*) echo "usage: $0 upgrade2|reg|flows|rereg"; exit 2 ;;
esac
