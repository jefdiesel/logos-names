#!/bin/bash
# Deploys the Logos Names registry to the Names test chain on this machine,
# configures it, and registers "jef" and "logos". Resumes from state files.
set -euo pipefail
C=$HOME/lez-chain
N=$C/bin/names
BIN=$C/logos_names-ci-542ae051.bin
SHA=542ae05161522b6ffe6f86da977d34cc1f0273bf196d91bd95e26615ee8d11c1
SEGMENTS=4
PRICES=10000,100000,400000
export LEE_WALLET_HOME_DIR=$C/faucet-wallet
H=$LEE_WALLET_HOME_DIR
PAYER=Public/BJzLuDRny4wMmMCiJg8j3oajcv4wkZP9QcRNuefFKgn8
OWNER=${PAYER#Public/}
RPC=http://127.0.0.1:3040
echo "$SHA  $BIN" | shasum -a 256 -c - >/dev/null || { echo "guest binary does not match the CI build"; exit 1; }
quiet() { grep -vE "^(Stored|Transaction data)" | cut -c1-200; }
deployed() {
  curl -s -m 20 $RPC -H 'Content-Type: application/json' \
    -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"getAccount\",\"params\":[\"$1\"]}" \
    | python3 -c 'import json,sys; sys.exit(0 if any(json.load(sys.stdin)["result"]["data"]["shards"].values()) else 1)'
}
if [ ! -f $H/program-id.txt ]; then
  if [ ! -f $H/program-accounts.txt ]; then
    for i in $(seq 0 $SEGMENTS); do
      $N wallet account new public 2>&1 | grep -oE "Public/[1-9A-HJ-NP-Za-km-z]{32,44}" | head -1
    done > $H/program-accounts.txt
  fi
  IDS=($(cat $H/program-accounts.txt))
  echo "deploying, header ${IDS[0]}"
  $N wallet program-loader deploy --elf $BIN --header ${IDS[0]} --segments "${IDS[@]:1}" --payer $PAYER 2>&1 | quiet | tail -4
  for try in $(seq 1 30); do deployed ${IDS[0]#Public/} && break; sleep 5; done
  deployed ${IDS[0]#Public/} || { echo "program did not appear"; exit 1; }
  echo "${IDS[0]#Public/}" > $H/program-id.txt
fi
export LOGOS_NAMES_PROGRAM=$(cat $H/program-id.txt)
echo "program: $LOGOS_NAMES_PROGRAM"
if ! $N settings 2>/dev/null | grep -q "$OWNER"; then
  $N configure --admin $OWNER --treasury $OWNER --prices $PRICES 2>&1 | quiet
fi
$N settings 2>&1 | grep -q "$OWNER" || { echo "the registry's admin is not ours; stop and look"; exit 1; }
for name in jef logos; do
  $N resolve $name >/dev/null 2>&1 || $N register $name --owner $OWNER --term 1y 2>&1 | quiet
done
$N set-page jef $C/jef-page.html 2>&1 | quiet
$N resolve jef
