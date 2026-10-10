#!/bin/sh
# Builds and publishes the `names` command line through the gateway.
#
#   scripts/publish-cli.sh
#
# macOS (Apple silicon): built on the Mac mini (see README, "Build and test").
# Linux x86_64: the artifact of the latest successful "CLI release build" run
# (`gh workflow run cli-release.yml` first if the CLI changed).
# Both land in gateway/public/dl/ with SHA-256 files; then the gateway is deployed.
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

echo "== linux x86_64 (CI artifact)"
run=$(gh run list --workflow cli-release.yml --status success --limit 1 --json databaseId --jq '.[0].databaseId')
[ -n "$run" ] || { echo "no successful cli-release run; start one with: gh workflow run cli-release.yml" >&2; exit 1; }
tmp=$(mktemp -d)
gh run download "$run" --name names-linux-x86_64 --dir "$tmp"
cp "$tmp/names-linux-x86_64" "$DL/names-linux-x86_64"
rm -rf "$tmp"

for f in "$DL"/names-macos-arm64 "$DL"/names-linux-x86_64; do
  chmod 755 "$f"
  (cd "$DL" && shasum -a 256 "$(basename "$f")" > "$(basename "$f").sha256")
  cat "$f.sha256"
done

echo "== deploy gateway"
cd gateway
NAMES_PROGRAM=${NAMES_PROGRAM:-DugXYzVqYsXJumqsjaGtoZeJ742WmdRHGpByi8pNJm2z} npx cf deploy
