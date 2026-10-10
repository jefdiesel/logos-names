#!/bin/sh
# Installs `names`, the Logos Names command line, with the LEZ wallet built in.
#
#   curl -fsSL https://logos-names.wrapit.workers.dev/install.sh | sh
#
# Puts the binary in ~/.local/bin (set NAMES_INSTALL_DIR to change that) and
# checks it against the published SHA-256. Nothing else is installed or changed.
set -eu

BASE="${NAMES_DOWNLOAD_BASE:-https://logos-names.wrapit.workers.dev/dl}"
DIR="${NAMES_INSTALL_DIR:-$HOME/.local/bin}"

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) target=macos-arm64 ;;
  Linux-x86_64) target=linux-x86_64 ;;
  *)
    echo "names: no build for $(uname -s) $(uname -m) yet (macOS on Apple silicon and Linux x86_64 only)." >&2
    exit 1
    ;;
esac

if command -v sha256sum >/dev/null 2>&1; then sha() { sha256sum "$1" | cut -d' ' -f1; }
else sha() { shasum -a 256 "$1" | cut -d' ' -f1; }; fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "Downloading names for $target..."
curl -fsSL "$BASE/names-$target" -o "$tmp/names"
curl -fsSL "$BASE/names-$target.sha256" -o "$tmp/names.sha256"
expected="$(cut -d' ' -f1 "$tmp/names.sha256")"
actual="$(sha "$tmp/names")"
if [ "$expected" != "$actual" ]; then
  echo "names: checksum mismatch (expected $expected, got $actual); not installing." >&2
  exit 1
fi

mkdir -p "$DIR"
install -m 755 "$tmp/names" "$DIR/names"
echo "Installed $DIR/names ($("$DIR/names" --version))"

if [ "$target" = linux-x86_64 ] && ! ldconfig -p 2>/dev/null | grep -q libpcsclite.so.1; then
  echo
  echo "It needs the PC/SC library: sudo apt install libpcsclite1 (Debian/Ubuntu)"
fi

case ":$PATH:" in
  *":$DIR:"*) ;;
  *)
    echo
    echo "$DIR is not on your PATH. Add it, for example:"
    echo "  echo 'export PATH=\"$DIR:\$PATH\"' >> ~/.zshrc   # or ~/.bashrc"
    ;;
esac

cat <<'EOF'

Try it:
  names resolve jef.lez                    look a name up (no setup needed)
  names wallet account new public          make a wallet and an account
  names register <name> --owner <account>  claim a name (the account needs funds)

Guide: https://logos-names.wrapit.workers.dev/
EOF
