#!/usr/bin/env bash
# Reproducible guest builds for the Monday deploy: the LNS registry guest (logos_names, this
# repo) and the lez_nft v3 guest (lez_nft_guest, NFT_SRC), each built inside RISC Zero's
# digest-pinned Docker builder. Same recipe as rfp-022 scripts/repro-build.sh.
#
#   deploy/repro-build.sh --nft-src DIR --out DIR [--jobs N]
#
# Writes to --out:
#   expected-image-ids.txt   registry_image_id / nft_image_id / *_elf_sha256 (deploy.sh's format)
#   logos_names.bin, lez_nft_guest.bin, SHA256SUMS
# Needs: docker (linux/amd64), cargo with the repo's rust-toolchain, rzup with RISC Zero Rust
# 1.97.0 (risc0-build asks for it even when it builds in Docker). Guests compile in the builder.
set -euo pipefail
BUILDER_IMAGE=risczero/risc0-guest-builder
BUILDER_TAG=r0.1.97.0
BUILDER_DIGEST=sha256:7ae0a27fa72b10fb5c1066005bd949ae43dc4ead8a284f2d6572b99c82ab4468
ROOT=$(cd "$(dirname "$0")/.." && pwd -P)
NFT_SRC=""; OUT=""; JOBS=${CARGO_BUILD_JOBS:-4}
while [ $# -gt 0 ]; do
  case $1 in --nft-src) NFT_SRC=$2; shift 2 ;; --out) OUT=$2; shift 2 ;; --jobs) JOBS=$2; shift 2 ;;
    *) echo "unknown argument $1" >&2; exit 2 ;; esac
done
[ -n "$NFT_SRC" ] && [ -n "$OUT" ] || { echo "usage: $0 --nft-src DIR --out DIR [--jobs N]" >&2; exit 2; }
NFT_SRC=$(cd "$NFT_SRC" && pwd -P); mkdir -p "$OUT"; OUT=$(cd "$OUT" && pwd -P)
for b in docker cargo python3 sha256sum; do command -v $b >/dev/null || { echo "need $b" >&2; exit 2; }; done
[[ "$(rzup show 2>/dev/null)" == *1.97.0* ]] || { echo "need rzup with RISC Zero Rust 1.97.0 (rzup install rust 1.97.0)" >&2; exit 2; }

ref="$BUILDER_IMAGE@$BUILDER_DIGEST"
docker pull "$ref" >&2
docker image inspect --format='{{json .RepoDigests}}' "$ref" | grep -q "$ref" || { echo "builder digest mismatch" >&2; exit 1; }
docker tag "$ref" "$BUILDER_IMAGE:$BUILDER_TAG"
TAG="$BUILDER_TAG@$BUILDER_DIGEST"   # risc0-build writes FROM <image>:<tag>@<digest>: Docker refuses any other image

build() { # WORKSPACE METHODS_CRATE GUEST CONST -> prints "<image id hex> <elf path>"
  local ws=$1 pkg=$2 guest=$3 const=$4 log
  log=$(mktemp)
  echo "== building $guest in Docker ($ws, -p $pkg)" >&2
  ( cd "$ws" && RISC0_USE_DOCKER=1 RISC0_DOCKER_CONTAINER_TAG="$TAG" \
      cargo check --release --locked -j "$JOBS" -p "$pkg" --message-format=json > "$log" )
  python3 -I - "$log" "$pkg" "$const" <<'PY'
import json, os, re, struct, sys
log, pkg, const = sys.argv[1:4]
outs = []
for line in open(log):
    if not line.startswith("{"): continue
    m = json.loads(line)
    if m.get("reason") == "build-script-executed" and re.search(r"[#/]" + re.escape(pkg) + r"(@|#|$)|^" + re.escape(pkg) + r"\s", m.get("package_id", "")):
        if os.path.isfile(os.path.join(m["out_dir"], "methods.rs")): outs.append(m["out_dir"])
assert outs, f"no methods.rs for {pkg}"
text = open(os.path.join(outs[-1], "methods.rs")).read()
words = re.search(const + r"_ID\s*:\s*\[u32\s*;\s*8\]\s*=\s*\[([^\]]+)\]", text, re.S).group(1)
ws = [int(re.sub(r"_?u32$", "", w.strip()).replace("_", ""), 0) for w in words.split(",") if w.strip()]
assert len(ws) == 8
path = re.search(const + r'_PATH\s*:\s*&str\s*=\s*"([^"]+)"', text).group(1)
print(b"".join(struct.pack("<I", w) for w in ws).hex(), path)
PY
  rm -f "$log"
}

read -r REG_ID REG_ELF < <(build "$ROOT" names_methods logos_names LOGOS_NAMES)
read -r NFT_ID NFT_ELF < <(build "$NFT_SRC" lez_nft_methods lez_nft_guest LEZ_NFT_GUEST)
cp "$REG_ELF" "$OUT/logos_names.bin"; cp "$NFT_ELF" "$OUT/lez_nft_guest.bin"
( cd "$OUT" && sha256sum logos_names.bin lez_nft_guest.bin > SHA256SUMS )
REG_SHA=$(sha256sum "$OUT/logos_names.bin" | cut -d' ' -f1); NFT_SHA=$(sha256sum "$OUT/lez_nft_guest.bin" | cut -d' ' -f1)
{ echo "# Reproducible (Docker) guest builds: $BUILDER_IMAGE:$TAG"
  echo "# registry: $(git -C "$ROOT" rev-parse HEAD 2>/dev/null || echo '?') methods/guest; lez_nft: $(git -C "$NFT_SRC" rev-parse HEAD 2>/dev/null || echo '?') methods/guest"
  echo "registry_image_id = $REG_ID"; echo "nft_image_id = $NFT_ID"
  echo "registry_elf_sha256 = $REG_SHA"; echo "nft_elf_sha256 = $NFT_SHA"
} > "$OUT/expected-image-ids.txt"
cat "$OUT/expected-image-ids.txt"
