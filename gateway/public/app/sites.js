// Browse LNS sites directly from the LEZ v0.4.0 JSON-RPC (no backend).
// Reads `SiteDirHeader` + `SiteBucket(bucket, page)` PDAs from the v8 registry,
// verifies every claimed name against its slot_id with Argon2id (spoof-proof),
// caches verified `(name -> slotIdHex)` mappings locally so repeat visits are fast,
// and drops past-grace entries.

import {
  GRACE_MS,
  SITE_BUCKETS,
  decodeSiteBucket,
  decodeSiteDirHeader,
  deriveName,
  isValidLabel,
  priceTierForLen,
  readActorState,
  siteBucketAddress,
  siteDirHeaderAddress,
  tieredId,
} from "./names.js";

const VERIFIED_CACHE_KEY = "lns-verified-ids-v8";
const DEFAULT_GATEWAY_DOMAIN = "lnsweb.online";

const toHex = (bytes) => Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");

function loadVerifiedCache() {
  try {
    const raw = JSON.parse(localStorage.getItem(VERIFIED_CACHE_KEY) || "{}");
    return raw && typeof raw === "object" ? raw : {};
  } catch {
    return {};
  }
}

function saveVerifiedCache(cache) {
  try {
    const entries = Object.entries(cache);
    const trimmed = entries.length > 2000 ? Object.fromEntries(entries.slice(-2000)) : cache;
    localStorage.setItem(VERIFIED_CACHE_KEY, JSON.stringify(trimmed));
  } catch {
    // private browsing / quota exceeded
  }
}

/**
 * Verifies that `name` hashes to `expectedSlotIdHex` (Argon2id + v7 `tieredId`).
 * Prevents a malicious holder from publishing a fake name in a `SiteBucket` or `MarketBucket`.
 */
export async function verifyNameMatchesSlotId(name, expectedSlotIdHex, cache = null) {
  if (!isValidLabel(name)) return false;
  const store = cache ?? loadVerifiedCache();
  if (store[name] === expectedSlotIdHex) return true;
  if (store[name] && store[name] !== expectedSlotIdHex) return false;
  const { id: baseId } = await deriveName(name);
  const slotHex = toHex(await tieredId(baseId, priceTierForLen(name.length)));
  store[name] = slotHex;
  if (!cache) saveVerifiedCache(store);
  return slotHex === expectedSlotIdHex;
}

/**
 * Loads all live, Argon2id-verified sites from the v8 registry's on-chain `SiteDirHeader`
 * and `SiteBucket` pages. No backend or indexer is contacted.
 */
export async function loadSitesDirectory({
  rpc,
  registry,
  now = Date.now(),
  gatewayDomain = DEFAULT_GATEWAY_DOMAIN,
  onProgress = () => {},
}) {
  const headerAddr = await siteDirHeaderAddress(registry);
  const headerBytes = await readActorState(rpc, registry, headerAddr);
  if (headerBytes.length === 0) {
    return { totalListed: 0, sites: [] };
  }
  const header = decodeSiteDirHeader(headerBytes);
  const reads = [];
  for (let bucket = 0; bucket < SITE_BUCKETS; bucket += 1) {
    const maxPage = header.activePage[bucket] ?? 0;
    for (let page = 0; page <= maxPage; page += 1) {
      reads.push({ bucket, page });
    }
  }
  onProgress({ stage: "fetch", buckets: reads.length });

  const rawEntries = [];
  const results = await Promise.all(
    reads.map(async ({ bucket, page }) => {
      const addr = await siteBucketAddress(registry, bucket, page);
      const bytes = await readActorState(rpc, registry, addr);
      if (bytes.length === 0) return [];
      try {
        return decodeSiteBucket(bytes).entries;
      } catch {
        return [];
      }
    }),
  );
  for (const list of results) rawEntries.push(...list);

  // Drop past-grace entries and deduplicate by slot id (keeping latest updatedAt).
  const byId = new Map();
  for (const entry of rawEntries) {
    if (!isValidLabel(entry.name)) continue;
    const permanent = entry.nameExpires >= Number.MAX_SAFE_INTEGER;
    if (!permanent && now >= entry.nameExpires + GRACE_MS) continue;
    const prev = byId.get(entry.id);
    if (!prev || entry.updatedAt > prev.updatedAt) byId.set(entry.id, entry);
  }

  const candidates = [...byId.values()].sort((a, b) => b.updatedAt - a.updatedAt);
  const cache = loadVerifiedCache();
  const sites = [];
  for (let i = 0; i < candidates.length; i += 1) {
    const entry = candidates[i];
    onProgress({ stage: "verify", done: i, total: candidates.length, name: entry.name });
    const ok = await verifyNameMatchesSlotId(entry.name, entry.id, cache);
    if (!ok) continue;
    sites.push({
      id: entry.id,
      name: entry.name,
      title: entry.title || entry.name,
      updatedAt: entry.updatedAt,
      expires: entry.nameExpires >= Number.MAX_SAFE_INTEGER ? null : entry.nameExpires,
      gatewayUrl: `https://${entry.name}.${gatewayDomain}/`,
      lnsUrl: `lns://${entry.name}`,
    });
  }
  saveVerifiedCache(cache);
  onProgress({ stage: "done", total: sites.length });
  return { totalListed: header.totalListed, sites };
}
