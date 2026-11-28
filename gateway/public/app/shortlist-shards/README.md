# Locked-name list shards

The web app proves a name is not on the locked-name list (all 1–4 character names and the
reserved names: 1,845,228 leaves, depth 21, root `9778bed4…990c` = `core` `SHORTLIST_ROOT`)
from one of 256 static shards, `00.bin`–`ff.bin`, fetched from this folder
(`shortlist-shards.js`: `new URL("./shortlist-shards/", import.meta.url)`, so on the site
`/app/shortlist-shards/`). Only `manifest.json` is in git; the 256 files (61,104,522 bytes,
about 240 KB each) are not.

- Built copy: `$SHORTLIST_BUILD_DIR/lns-04-b/shortlist-shards/` (2026-11-28,
  from `$SHORTLIST_BUILD_DIR/shortlist-v6/out/shortlist.bin`, the same bytes as
  `shortlist/shortlist.bin` here: sha256 `ab033108…2a97`).
- Rebuild: `node gateway/app-src/build-shortlist-shards.mjs shortlist/shortlist.bin <out-dir>`
  (about 15 s). The output is deterministic; `shortlist.test.mjs` checks this `manifest.json`
  is exactly what the builder makes from the repo's list.
- Deploy: copy the 256 `.bin` files next to this `manifest.json` before publishing the site
  (static assets, no Worker). Without them registration stops with "Couldn't load
  locked-name shard …".
