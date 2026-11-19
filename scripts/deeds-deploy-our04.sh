#!/bin/bash
# lns-deeds on our 0.4 testnet (127.0.0.1:3290): the NFT program at its pinned header, then the
# registry 6Yxj5k8A…N3gA updated in place to the optional-deed image. Steps: backup | fund |
# nft | update | check-user | smoke-reg | smoke-claim. Every send names 3290 explicitly.
set -u
D=/Volumes/exos162/logos/fix-1119/lns-deeds; LD=/Volumes/exos162/logos/names-deploy-our04
L=$D/logs/live; mkdir -p $L
export LOGOS_NAMES_RPC=http://127.0.0.1:3290
. $LD/state.env; . $D/nftdeploy/state.env
NEW=$D/target/release/names; OLD=$LD/bin/names; ELF=$D/new_logos_names.bin
OPW=/Volumes/exos162/logos/rfp-022/public-chain/wallets/operator; OP=BVDcg8SbSdCTKzx4mJbSpVgEuaoPEMhxmmRwvVCGyBnD
LPW=$(cat $LD/.wallet-password); NPW=$(cat $D/nftdeploy/.wallet-password)
asOP() { LEE_WALLET_HOME_DIR=$OPW LEE_WALLET_PASSWORD= "$@"; }
asLD() { LEE_WALLET_HOME_DIR=$LD/wallet LEE_WALLET_PASSWORD=$LPW "$@"; }
asNFT() { LEE_WALLET_HOME_DIR=$D/nftdeploy/wallet LEE_WALLET_PASSWORD=$NPW "$@"; }
asT() { LEE_WALLET_HOME_DIR=$D/users-live/t LEE_WALLET_PASSWORD=$(cat $D/users-live/.pw) LOGOS_NAMES_PROGRAM=$REGISTRY "$@"; }
LOG=$L/deploy.log
run() { echo "\$ $*" >> $LOG; "$@" 2>&1 | grep -vE "^(Stored|Transaction data)" | cut -c1-400 | tee -a $LOG; return ${PIPESTATUS[0]}; }
header() { python3 - "$1" <<'PY'
import json, sys, urllib.request, base64
def b58(b):
    A = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"; n = int.from_bytes(b, "big"); s = ""
    while n: n, r = divmod(n, 58); s = A[r] + s
    return "1" * (len(b) - len(b.lstrip(b"\0"))) + s
loader = b58(bytes([0xFE] * 32))
req = json.dumps({"jsonrpc": "2.0", "id": 1, "method": "getAccountView", "params": [{"account_id": sys.argv[1], "program_account_id": loader}]}).encode()
r = json.load(urllib.request.urlopen(urllib.request.Request("http://127.0.0.1:3290", req, {"Content-Type": "application/json"}), timeout=15))
st = (((r.get("result") or {}).get("data") or {}).get("actor_states") or {}).get(loader)
if not st: print("no program"); sys.exit(0)
st = bytes(st) if isinstance(st, list) else base64.b64decode(st)
print("image", st[1:33].hex(), "immutable", st[65])
PY
}
case ${1:-} in
backup)
  ts=$(date +%Y%m%d-%H%M%S)
  cp -a $LD/wallet $LD/wallet.bak-$ts && cp -a $LD/state.env $LD/state.env.bak-$ts && cp -a $D/nftdeploy/wallet $D/nftdeploy/wallet.bak-$ts
  cp -a $LD/logos_names.bin $D/rollback_logos_names_00ad4571.bin
  echo "backed up as *.bak-$ts"; header $REGISTRY; header $NH ;;
fund)
  NPPK=$(asNFT $OLD wallet account get --account-id Public/$NP --keys | sed -n "s/^pk //p")
  run asOP $OLD wallet auth-transfer send --from Public/$OP --to-pk $P_PK --amount 40000000
  run asOP $OLD wallet auth-transfer send --from Public/$OP --to-pk $NPPK --amount 20000000
  run asLD $OLD wallet account get --account-id Public/$P
  run asNFT $OLD wallet account get --account-id Public/$NP ;;
nft)
  [ "$(header $NH)" = "no program" ] || { echo "NH already holds a program:"; header $NH; exit 0; }
  run asNFT $OLD wallet program-loader deploy --elf $D/nft.bin --header Public/$NH --segments Public/$NS1 Public/$NS2 --payer Public/$NP
  run header $NH ;;
update)
  run header $REGISTRY
  if [ -z "${L_S1:-}" ]; then
    for i in 1 2 3 4 5 6; do id=$(asLD $OLD wallet account new public | grep -oE "Public/[1-9A-HJ-NP-Za-km-z]{32,44}" | head -1 | cut -d/ -f2); echo "L_S$i=$id" >> $LD/state.env; done
    . $LD/state.env
  fi
  segs=(); for i in 1 2 3 4 5 6; do v=L_S$i; segs+=("Public/${!v}"); done
  n=$(asLD $OLD wallet --dry-run program-loader update --elf $ELF --header Public/$REGISTRY --segments "${segs[@]}" --payer Public/$P 2>&1 | grep -oE "splits into [0-9]+" | grep -oE "[0-9]+")
  echo "segments needed: ${n:-?}" | tee -a $LOG; [ -n "$n" ] || exit 1
  segs=("${segs[@]:0:$n}")
  run asLD $OLD wallet program-loader update --elf $ELF --header Public/$REGISTRY --segments "${segs[@]}" --payer Public/$P
  run header $REGISTRY ;;
check-user)
  umask 077; mkdir -p $D/users-live/t; [ -f $D/users-live/.pw ] || head -c 24 /dev/urandom | base64 | tr -d "/+=" > $D/users-live/.pw
  cat > $D/users-live/t/wallet_config.json <<J
{ "gas_limit": 2000000, "fee_headroom_blocks": 5, "seq_block_poll_max_amount": 100, "seq_poll_max_retries": 5,
  "seq_poll_timeout": "12s", "seq_tx_poll_max_blocks": 5, "sequencer": { "sequencer_addr": "http://127.0.0.1:3290/" } }
J
  asT $NEW wallet check-health > $D/users-live/t/created.txt 2>&1
  T=$(asT $NEW wallet account new public | grep -oE "Public/[1-9A-HJ-NP-Za-km-z]{32,44}" | head -1 | cut -d/ -f2)
  pk=$(asT $NEW wallet account get --account-id Public/$T --keys | sed -n "s/^pk //p")
  echo "T=$T" > $D/users-live/state.env
  run asOP $OLD wallet auth-transfer send --from Public/$OP --to-pk $pk --amount 20000000 ;;
smoke-reg)
  . $D/users-live/state.env
  run asT $NEW register ${2:-lns-deed-check} --owner Public/$T --wait
  run asT $NEW deed show ${2:-lns-deed-check} ;;
smoke-claim)
  . $D/users-live/state.env
  run asT $NEW deed claim ${2:-lns-deed-check}
  run asT $NEW deed show ${2:-lns-deed-check} ;;
*) echo "usage: $0 backup|fund|nft|update|check-user|smoke-reg [name]|smoke-claim [name]"; exit 2 ;;
esac
