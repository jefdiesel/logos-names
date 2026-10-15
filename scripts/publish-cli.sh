#!/bin/sh
# Builds and publishes the `names` command line.
#
#   scripts/publish-cli.sh
#
# macOS (Apple silicon): built on the Mac mini (see README, "Build and test").
# Linux x86_64: the artifact of the latest successful "CLI release build" run
# (`gh workflow run cli-release.yml` first if the CLI changed).
# Both binaries and the man pages (names-man.tar.gz), with SHA-256 files, go to
# a GitHub release of this repository, which install.sh downloads from. The MCP
# server is bundled as gateway/public/mcp.js, and the gateway (guide, install.sh)
# is deployed.
set -eu
cd "$(dirname "$0")/.."

MINI=${MINI:-minim4}
DEV=/Volumes/exos162/logos/dev
DL=dist
rm -rf "$DL" && mkdir -p "$DL"

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

echo "== MCP server, bundled into one file"
(cd gateway && npx rolldown ../mcp/server.js --platform node --format esm --file public/mcp.js)

for f in "$DL"/names-macos-arm64 "$DL"/names-linux-x86_64 "$DL"/names-man.tar.gz gateway/public/mcp.js; do
  case "$f" in *.tar.gz) chmod 644 "$f" ;; *) chmod 755 "$f" ;; esac
  (cd "$(dirname "$f")" && shasum -a 256 "$(basename "$f")" > "$(basename "$f").sha256")
  cat "$f.sha256"
done

echo "== GitHub release"
TAG=cli-$(date -u +%Y%m%d-%H%M)
gh release create "$TAG" --latest --title "names CLI $TAG" \
  --notes "names for macOS (Apple silicon) and Linux (x86_64), with man pages. Install: curl -fsSL https://logos-names.wrapit.workers.dev/install.sh | sh" \
  "$DL"/names-macos-arm64 "$DL"/names-macos-arm64.sha256 \
  "$DL"/names-linux-x86_64 "$DL"/names-linux-x86_64.sha256 \
  "$DL"/names-man.tar.gz "$DL"/names-man.tar.gz.sha256

echo "== deploy gateway"
cd gateway
# The public LEZ testnet, and the Names faucet on the M2 (chain/faucet.py) reached
# through its tunnel.
# The tunnel's address is kept out of the repo, in .chain-origin (gitignored).
CHAIN_ORIGIN=${CHAIN_ORIGIN:-$(cat ../.chain-origin 2>/dev/null || true)}
[ -n "$CHAIN_ORIGIN" ] || { echo "set CHAIN_ORIGIN, or put the faucet tunnel's address in .chain-origin" >&2; exit 1; }
NAMES_PROGRAM=${NAMES_PROGRAM:-66nUvzJtTxA729aUpTcAWDrb7cV9Heb2bDzceHQhLYad} \
  LEZ_RPC_URL=${LEZ_RPC_URL:-https://testnet.lez.logos.co} FAUCET_URL=${FAUCET_URL:-$CHAIN_ORIGIN/faucet} \
  npx cf deploy
