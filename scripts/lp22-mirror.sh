#!/bin/bash
# Mirrors a lane worktree to its Mac mini area for the LP-0022 build loop, so the module
# suite runs there:
#   scripts/lp22-mirror.sh <worktree> <mini area>
#   e.g. scripts/lp22-mirror.sh /Users/jef/logos/lp22-inscribe /Volumes/exos162/logos/dev/lp22/inscribe
# then on the Mini:
#   cd <area>/repo/basecamp-module/rust-lib && \
#     CARGO_TARGET_DIR=/Volumes/exos162/logos/dev/lp22/target-mod \
#     /Volumes/exos162/logos/dev/fix-names/mini-lock.sh cargo test --lib
#
# The module's Cargo.toml reads the SDK from basecamp-module/logos-rust-sdk-src, a
# gitignored link the worktree doesn't have. A plain `rsync --delete` therefore leaves the
# mirror without it and `cargo test` fails ("failed to read .../logos-rust-sdk-src/Cargo.toml").
# Here the link is excluded from the rsync (so --delete keeps it) and made on the Mini when
# it is missing, pointing at the SDK staged there.
set -euo pipefail
WT=${1:?worktree}
AREA=${2:?mini area}
SDK=${LP22_SDK:-/Volumes/exos162/logos/dev/sdk-stage/logos-rust-sdk-src}
rsync -a --delete --exclude target --exclude .git --exclude logos-rust-sdk-src "$WT"/ minim4:"$AREA"/repo/
ssh minim4 /bin/bash -s -- "$AREA" "$SDK" <<'EOF'
set -euo pipefail
link=$1/repo/basecamp-module/logos-rust-sdk-src
[ -e "$2/Cargo.toml" ] || { echo "no SDK staged at $2" >&2; exit 1; }
[ -e "$link/Cargo.toml" ] || ln -sfn "$2" "$link"
echo "mirrored to $1/repo; $(ls -ld "$link" | sed 's/.* \(\/.*\)/\1/')"
EOF
