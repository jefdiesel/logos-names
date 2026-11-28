// Secondary marketplace reader (direct from the LEZ v0.4.0 JSON-RPC, no backend).
// Reads `MarketBucket(0..15)` for public listings and single `NameRecord`s for OTC lookups.
// Verifies every `displayName` against its `slot_id` with Argon2id (cached locally).

import {
  MARKET_BUCKETS,
  decodeMarketBucket,
  decodeNameRecord,
  deriveName,
  isValidLabel,
  marketBucketAddress,
  priceTierForLen,
  readActorState,
  recordAddress,
  tieredId,
} from "./names.js";
import { saleSplit } from "./lns-tx.js";
import { verifyNameMatchesSlotId } from "./sites.js";

const toHex = (bytes) => Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");

/**
 * Loads all active public listings from the 16 `MarketBucket` PDAs.
 * Drops expired listings, lapsed names, and any entry whose `displayName` fails Argon2id verification.
 */
export async function loadMarketListings({ rpc, registry, now = Date.now(), onProgress = () => {} }) {
  onProgress({ stage: "fetch" });
  const buckets = await Promise.all(
    Array.from({ length: MARKET_BUCKETS }, async (_, bucket) => {
      const addr = await marketBucketAddress(registry, bucket);
      const bytes = await readActorState(rpc, registry, addr);
      if (bytes.length === 0) return [];
      try {
        return decodeMarketBucket(bytes).entries;
      } catch {
        return [];
      }
    }),
  );

  const candidates = [];
  for (const list of buckets) {
    for (const entry of list) {
      if (!entry.displayName || !isValidLabel(entry.displayName)) continue;
      if (entry.nameExpires < Number.MAX_SAFE_INTEGER && now >= entry.nameExpires) continue;
      if (entry.expiresAt !== null && now >= entry.expiresAt) continue;
      candidates.push(entry);
    }
  }

  candidates.sort((a, b) => (a.price < b.price ? -1 : a.price > b.price ? 1 : 0));
  const verified = [];
  for (let i = 0; i < candidates.length; i += 1) {
    const entry = candidates[i];
    onProgress({ stage: "verify", done: i, total: candidates.length, name: entry.displayName });
    if (!(await verifyNameMatchesSlotId(entry.displayName, entry.id))) continue;
    const split = saleSplit(entry.price);
    verified.push({
      id: entry.id,
      name: entry.displayName,
      price: entry.price,
      fee: split.fee,
      sellerPayout: split.seller,
      payTo: entry.payTo,
      expiresAt: entry.expiresAt,
      nameExpires: entry.nameExpires >= Number.MAX_SAFE_INTEGER ? null : entry.nameExpires,
      priceTier: entry.priceTier,
      nonce: entry.nonce,
      nftTokenId: entry.nftTokenId,
      inMarket: true,
    });
  }
  onProgress({ stage: "done", total: verified.length });
  return verified;
}

/** Looks up a single name's live listing directly from its `NameRecord` (works for both public and unlisted OTC sales). */
export async function lookupListing({ rpc, registry, name, now = Date.now() }) {
  if (!isValidLabel(name)) throw new Error("Only top-level names can be bought here.");
  const { id: baseId } = await deriveName(name);
  const slotId = await tieredId(baseId, priceTierForLen(name.length));
  const bytes = await readActorState(rpc, registry, await recordAddress(registry, slotId));
  if (bytes.length === 0) return null;
  const rec = decodeNameRecord(bytes);
  if (!rec.listing) return null;
  const permanent = rec.expires >= Number.MAX_SAFE_INTEGER;
  if (!permanent && now >= rec.expires) return null;
  if (rec.listing.expiresAt !== null && now >= rec.listing.expiresAt) return null;
  if (rec.nftShielded) return null;
  const split = saleSplit(rec.listing.price);
  return {
    id: toHex(slotId),
    name,
    price: rec.listing.price,
    fee: split.fee,
    sellerPayout: split.seller,
    payTo: rec.listing.payTo,
    expiresAt: rec.listing.expiresAt,
    nameExpires: permanent ? null : rec.expires,
    priceTier: rec.priceTier ?? priceTierForLen(name.length),
    nonce: rec.listing.nonce,
    nftTokenId: rec.nftTokenId ?? null,
    inMarket: Boolean(rec.listing.inMarket),
  };
}
