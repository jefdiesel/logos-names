#!/bin/sh
# Publishes the Names app and the names module for Basecamp.
#
#   scripts/publish-basecamp.sh
#
# Builds both .lgx packages on the Mac mini (darwin-arm64), signs them with the
# release key, attaches them to a GitHub release of this repository
# (basecamp-v<app version>), and writes a Basecamp package repository as static
# files in gateway/public/basecamp/: logos-repo.json (what people add in
# Basecamp's package manager) and index.json (the packages, in the same format
# as logos-co/logos-modules-release). Deploy the site afterwards (cf deploy, or
# scripts/publish-cli.sh, which deploys it too).
set -eu
cd "$(dirname "$0")/.."

MINI=${MINI:-minim4}
DEV=/Volumes/exos162/logos/dev
B=$DEV/names-v5-build
KEY=${NAMES_SIGNING_KEY:-$HOME/.config/logos-names/release-signing-key}
SITE=https://logos-names.wrapit.workers.dev
REPO=jefdiesel/logos-names
OUT=dist/basecamp
[ -f "$KEY" ] || { echo "signing key not found at $KEY" >&2; exit 1; }
rm -rf "$OUT" && mkdir -p "$OUT"

echo "== build the module and the app (on $MINI)"
rsync -a --delete --exclude target --exclude result-portable --exclude build.log --exclude logos-rust-sdk-src basecamp-module/ "$MINI:$DEV/names-module-v5/"
rsync -a --delete --exclude target --exclude result-portable --exclude build.log --exclude logos-rust-sdk-src basecamp-module/ "$MINI:$B/basecamp-module/"
rsync -a --delete --exclude result-portable --exclude build.log basecamp/ "$MINI:$B/basecamp/"
ssh "$MINI" "export PATH=/nix/var/nix/profiles/default/bin:/opt/homebrew/bin:\$PATH;
  cd $DEV/names-module-v5 && nix build '.#lgx-portable' -o result-portable > build.log 2>&1 &&
  cd $B/basecamp && nix build '.#lgx-portable' --override-input names path:$B/basecamp-module -o result-portable > build.log 2>&1"
scp -q "$MINI:$DEV/names-module-v5/result-portable/logos-names-module-lib.lgx" "$OUT/module.lgx"
scp -q "$MINI:$B/basecamp/result-portable/logos-names_ui-module.lgx" "$OUT/app.lgx"

module_version=$(tar xzOf "$OUT/module.lgx" manifest.json | python3 -c 'import sys,json;print(json.load(sys.stdin)["version"])')
app_version=$(tar xzOf "$OUT/app.lgx" manifest.json | python3 -c 'import sys,json;print(json.load(sys.stdin)["version"])')
mv "$OUT/module.lgx" "$OUT/names-$module_version.lgx"
mv "$OUT/app.lgx" "$OUT/names_ui-$app_version.lgx"
tag=basecamp-v$app_version

echo "== sign"
for f in "$OUT"/*.lgx; do
  ssh-keygen -Y sign -q -f "$KEY" -n logos-names-release "$f"
  shasum -a 256 "$f" | awk '{print $1}' > "$f.sha256"
done

echo "== GitHub release $tag"
notes="The Names app for Basecamp (posts, messages, pay, profile) and the names module.

In Basecamp's package manager, add the repository
$SITE/basecamp/logos-repo.json
and install Names. It needs the names command line: curl -fsSL $SITE/install.sh | sh"
gh release view "$tag" --repo "$REPO" >/dev/null 2>&1 || gh release create "$tag" --repo "$REPO" --title "Names for Basecamp $app_version" --notes "$notes"
gh release upload "$tag" --repo "$REPO" --clobber "$OUT"/*.lgx "$OUT"/*.sig "$OUT"/*.sha256

echo "== repository index"
mkdir -p gateway/public/basecamp
python3 - "$OUT" "$tag" "$SITE" "$REPO" <<'PY'
import hashlib, json, os, sys, tarfile, time
out, tag, site, repo = sys.argv[1:5]
dest = "gateway/public/basecamp"
packages = []
for file in sorted(os.listdir(out)):
    if not file.endswith(".lgx"):
        continue
    path = os.path.join(out, file)
    data = open(path, "rb").read()
    with tarfile.open(path) as archive:
        manifest = json.load(archive.extractfile("manifest.json"))
        icon = None
        for member in archive.getmembers():
            if member.name.endswith("icon.png") and member.isfile():
                icon = archive.extractfile(member).read()
                break
    url = f"https://github.com/{repo}/releases/download/{tag}/{file}"
    version = {
        "releasedAt": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "publisherRef": tag,
        "url": url,
        "urls": [url],
        "size": len(data),
        "sha256": hashlib.sha256(data).hexdigest(),
        "rootHash": manifest.get("hashes", {}).get("root", ""),
        "manifest": manifest,
    }
    if icon:
        digest = hashlib.sha256(icon).hexdigest()
        open(os.path.join(dest, digest + ".png"), "wb").write(icon)
        version["icon"] = {"path": digest + ".png", "sha256": digest, "size": len(icon)}
    packages.append({"name": manifest["name"], "versions": [version]})
index = {"schemaVersion": 2, "repositoryName": "logos-names",
         "generatedAt": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), "packages": packages}
json.dump(index, open(os.path.join(dest, "index.json"), "w"), indent=1)
json.dump({"schemaVersion": 1, "name": "logos-names", "displayName": "LNS",
           "description": "The Names app (posts, messages, pay, profile) and the names module, for LEZ names.",
           "homepage": f"https://github.com/{repo}", "indexUrl": f"{site}/basecamp/index.json",
           "trustedSigners": []}, open(os.path.join(dest, "logos-repo.json"), "w"), indent=1)
print("wrote", dest, [p["name"] for p in packages])
PY
echo "done: deploy the site to publish $SITE/basecamp/logos-repo.json"
