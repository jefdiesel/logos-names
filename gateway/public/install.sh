#!/bin/sh
# Installs `names`, the Logos Names command line, with the LEZ wallet built in.
#
#   curl -fsSL https://logos-names.wrapit.workers.dev/install.sh | sh
#
# Puts the binary in ~/.local/bin (set NAMES_INSTALL_DIR to change that) and its
# man pages in ~/.local/share/man/man1 (NAMES_MAN_DIR), each checked against the
# published SHA-256, plus RISC Zero's prover r0vm from RISC Zero's GitHub release
# (NAMES_SKIP_PROVER=1 skips it). Nothing else is installed or changed.
set -eu

# Binaries come from the project's GitHub releases.
BASE="${NAMES_DOWNLOAD_BASE:-https://github.com/jefdiesel/logos-names/releases/latest/download}"
DIR="${NAMES_INSTALL_DIR:-$HOME/.local/bin}"
MAN_DIR="${NAMES_MAN_DIR:-$HOME/.local/share/man/man1}"

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

# The man pages are a bonus: if they can't be fetched, the install still stands.
if curl -fsSL "$BASE/names-man.tar.gz" -o "$tmp/man.tar.gz" 2>/dev/null &&
  curl -fsSL "$BASE/names-man.tar.gz.sha256" -o "$tmp/man.sha256" 2>/dev/null &&
  [ "$(cut -d' ' -f1 "$tmp/man.sha256")" = "$(sha "$tmp/man.tar.gz")" ] &&
  mkdir -p "$MAN_DIR" && tar -xzf "$tmp/man.tar.gz" -C "$tmp" &&
  cp "$tmp"/man1/*.1 "$MAN_DIR"/; then
  echo "Installed man pages in $MAN_DIR (man names)"
fi

# Private changes are proven locally by RISC Zero's r0vm (3.0.5, what LEZ v0.3.1
# uses). It comes from RISC Zero's own GitHub release, checked against the
# release's SHA-256, and goes next to names, which finds it there.
case "$target" in
  macos-arm64) r0=aarch64-apple-darwin; r0sha=0f676055ad88d7ede06beaec3b7f850413e091db08bb7cf9969a78d9ad168562 ;;
  linux-x86_64) r0=x86_64-unknown-linux-gnu; r0sha=936ef988b78f20e3bd9f80e375f3adc934b13addc6ae2680f2e5fc0bcc966158 ;;
esac
if [ "${NAMES_SKIP_PROVER:-}" != 1 ] && ! { [ -x "$DIR/r0vm" ] && "$DIR/r0vm" --version 2>/dev/null | grep -q " 3.0.5"; }; then
  echo "Downloading RISC Zero's prover (r0vm 3.0.5, about 70 MB) for private changes..."
  if curl -fsSL "https://github.com/risc0/risc0/releases/download/v3.0.5/cargo-risczero-$r0.tgz" -o "$tmp/r0.tgz" &&
    [ "$(sha "$tmp/r0.tgz")" = "$r0sha" ] && tar -xzf "$tmp/r0.tgz" -C "$tmp" r0vm; then
    install -m 755 "$tmp/r0vm" "$DIR/r0vm"
    echo "Installed $DIR/r0vm"
  else
    echo "Could not install r0vm; lookups and public changes still work, private ones need it." >&2
  fi
fi

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
  names faucet Public/<account>            free test tokens for it
  names register <name> --owner <account>  claim a name

Every command: names --help, or man names
Guide: https://logos-names.wrapit.workers.dev/
EOF
