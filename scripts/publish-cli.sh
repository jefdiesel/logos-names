#!/bin/sh
# Builds and publishes the `names` command line through the gateway.
#
#   scripts/publish-cli.sh
#
# macOS (Apple silicon): built on the Mac mini (see README, "Build and test").
# Linux x86_64: the artifact of the latest successful "CLI release build" run
# (`gh workflow run cli-release.yml` first if the CLI changed).
# Both land in gateway/public/dl/ with SHA-256 files, with the man pages
# (names-man.tar.gz) and the MCP server bundled as gateway/public/mcp.js;
# then the gateway is deployed.
set -eu
cd "$(dirname "$0")/.."

MINI=${MINI:-minim4}
DEV=/Volumes/exos162/logos/dev
DL=gateway/public/dl
mkdir -p "$DL"

echo "== macOS arm64 (on $MINI)"
rsync -a --delete --exclude target --exclude .git --exclude node_modules ./ "$MINI:$DEV/logos-names/"
ssh "$MINI" "export PATH=\$HOME/.cargo/bin:\$HOME/.risc0/bin:/opt/homebrew/bin:\$PATH;
  cd $DEV/logos-names && CARGO_TARGET_DIR=$DEV/target-names cargo build --locked --release -p names_cli"
scp -q "$MINI:$DEV/target-names/release/names" "$DL/names-macos-arm64"

echo "== man pages (from the macOS build; they are the same everywhere)"
ssh "$MINI" "rm -rf /tmp/names-man && $DEV/target-names/release/names man /tmp/names-man/man1 &&
  tar -C /tmp/names-man -czf /tmp/names-man.tar.gz man1"
scp -q "$MINI:/tmp/names-man.tar.gz" "$DL/names-man.tar.gz"

echo "== linux x86_64 (CI artifact)"
run=$(gh run list --workflow cli-release.yml --status success --limit 1 --json databaseId --jq '.[0].databaseId')
[ -n "$run" ] || { echo "no successful cli-release run; start one with: gh workflow run cli-release.yml" >&2; exit 1; }
tmp=$(mktemp -d)
gh run download "$run" --name names-linux-x86_64 --dir "$tmp"
cp "$tmp/names-linux-x86_64" "$DL/names-linux-x86_64"
rm -rf "$tmp"

echo "== registry bytecode, for clients proving private changes (CI guest of commit 6a08124)"
PROGRAM_ID=${NAMES_PROGRAM:-66nUvzJtTxA729aUpTcAWDrb7cV9Heb2bDzceHQhLYad}
PROGRAM_SHA=${PROGRAM_SHA:-506bfbfd0c4dd388c33c8169f6dd24a14620464b9f583aa15042825bbfc3e9c4}
mkdir -p "$DL/program"
scp -q "$MINI:$DEV/logos_names-ci-$(echo "$PROGRAM_SHA" | cut -c1-8).bin" "$DL/program/$PROGRAM_ID.bin"
echo "$PROGRAM_SHA  $DL/program/$PROGRAM_ID.bin" | shasum -a 256 -c -

echo "== MCP server, bundled into one file"
(cd gateway && npx rolldown ../mcp/server.js --platform node --format esm --file public/mcp.js)

for f in "$DL"/names-macos-arm64 "$DL"/names-linux-x86_64 "$DL"/names-man.tar.gz gateway/public/mcp.js; do
  case "$f" in *.tar.gz) chmod 644 "$f" ;; *) chmod 755 "$f" ;; esac
  (cd "$(dirname "$f")" && shasum -a 256 "$(basename "$f")" > "$(basename "$f").sha256")
  cat "$f.sha256"
done

echo "== deploy gateway"
cd gateway
# The public LEZ testnet, and the Names faucet on the M2 (chain/faucet.py) reached
# through its tunnel.
CHAIN_ORIGIN=${CHAIN_ORIGIN:-https://lez.buggers.online}
NAMES_PROGRAM=${NAMES_PROGRAM:-66nUvzJtTxA729aUpTcAWDrb7cV9Heb2bDzceHQhLYad} \
  LEZ_RPC_URL=${LEZ_RPC_URL:-https://testnet.lez.logos.co} FAUCET_URL=${FAUCET_URL:-$CHAIN_ORIGIN/faucet} \
  npx cf deploy
