// Which chain (sequencer RPC, registry, board) and messaging node the web app uses.
//
// Only the Settings drawer changes them. A link never does: ?rpc= and ?peer= in the
// address are ignored. They used to be read and saved for good, so a link could point
// this browser at a sequencer of the sender's choosing, which can answer lookups with
// anything (pressure-test finding C2, 2026-11-13).
//
// The choice lives under STORE_KEY, a key older builds never read or write: an older
// build on this origin (a rollback, a stale cache) still saves whatever ?rpc= says, so
// nothing under its key, LEGACY_KEY, is trusted. dropLegacySettings removes it once and
// says whether it pointed somewhere other than the defaults. Whatever is stored is
// checked again at load, like a fresh choice in the drawer.

export const CHAINS = {
  "https://lez.buggers.online/lez": {
    registry: "38jEfVV1RCyMzu1FGbYcCn5FqxCxKyY58VgzjruAh2gq",
    board: "8PwnCKStKQtoBxJCgopCG6CPPMJCUWba1SDG1SXtXYTf",
  },
  "https://lez.buggers.online": {
    registry: "m82S3dUDxdcMBtZtXpHXYcVhU68U7bS4AKqsi6NomJm",
    board: "CKqLLZrTUNiw5KLtQCZ9oSDXWWEtCHLpKjp7pQAaP5q4",
  },
};
// The Logos messaging (Waku) node chat connects to: the default "door" on a stable
// named tunnel. Changeable in Settings only.
export const DEFAULT_PEER =
  "/dns4/msg.buggers.online/tcp/443/wss/p2p/16Uiu2HAkxrwXguVQUCPqMyXrn9VuPNce2xoK9Nr7AW2zcHQeS6dg";
export const DEFAULTS = Object.freeze({
  rpc: "https://lez.buggers.online/lez",
  ...CHAINS["https://lez.buggers.online/lez"],
  peer: DEFAULT_PEER,
});
export const PRESETS = Object.keys(CHAINS);
export const STORE_KEY = "lns-app-settings-v2";
export const LEGACY_KEY = "lns-app-settings";
export const PEER_RE = /^\/(dns4|dns6|dns|ip4|ip6)\/[^/]+\/tcp\/\d+\/(wss|tls\/ws)\/p2p\/\w+$/;
// Marks a record written by the Settings drawer.
const SAVED_BY = "settings-drawer";
const LOCAL_HOSTS = ["localhost", "127.0.0.1", "[::1]"];
const B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

/** Whether `text` is base58 for exactly 32 bytes, as a program ID is. */
export function isProgramId(text) {
  if (typeof text !== "string" || text.length === 0 || text.length > 64) return false;
  let value = 0n;
  let zeros = 0;
  let leading = true;
  for (const char of text) {
    const digit = B58.indexOf(char);
    if (digit < 0) return false;
    if (leading && digit === 0) zeros += 1;
    else leading = false;
    value = value * 58n + BigInt(digit);
  }
  let bytes = 0;
  for (; value > 0n; value >>= 8n) bytes += 1;
  return zeros + bytes === 32;
}

/**
 * Checks a choice of chain and messaging node: `{ value }` with the settings to use,
 * or `{ error }` saying what is wrong. A preset chain always uses its own programs.
 */
export function checkSettings({ rpc, registry, board, peer }) {
  if (typeof rpc !== "string") return { error: "The RPC URL isn't a valid web address." };
  let url;
  try {
    url = new URL(rpc);
  } catch {
    return { error: "The RPC URL isn't a valid web address." };
  }
  if (url.protocol !== "https:" && url.protocol !== "http:") return { error: "The RPC URL must start with https://." };
  if (url.protocol === "http:" && !LOCAL_HOSTS.includes(url.hostname)) {
    return { error: "The RPC URL must start with https:// (http:// only works for this computer, like http://127.0.0.1)." };
  }
  if (url.username || url.password) return { error: "The RPC URL can't contain a user name or password." };
  if (typeof peer !== "string" || !PEER_RE.test(peer)) {
    return { error: "The messaging node must be a WebSocket address like /dns4/…/tcp/443/wss/p2p/…" };
  }
  if (Object.hasOwn(CHAINS, rpc)) return { value: { rpc, ...CHAINS[rpc], peer } };
  if (!isProgramId(registry)) return { error: "The registry program isn't a valid program ID." };
  if (!isProgramId(board)) return { error: "The board program isn't a valid program ID." };
  return { value: { rpc, registry, board, peer } };
}

function forget(storage, key) {
  try {
    storage?.removeItem(key);
  } catch {}
}

/**
 * Removes settings saved by builds before this one (a link may have written them).
 * Returns `{ rpc }` (and `peer` if it differed) when they pointed somewhere other than
 * the defaults, so the page can say so once; otherwise `null`.
 */
export function dropLegacySettings(storage) {
  let old = null;
  try {
    old = JSON.parse(storage?.getItem(LEGACY_KEY) ?? "null");
  } catch {
    old = null;
  }
  forget(storage, LEGACY_KEY);
  if (!old || typeof old !== "object") return null;
  const dropped = {};
  if (typeof old.rpc === "string" && old.rpc !== DEFAULTS.rpc) dropped.rpc = old.rpc;
  if (typeof old.peer === "string" && old.peer !== DEFAULT_PEER) dropped.peer = old.peer;
  return Object.keys(dropped).length ? dropped : null;
}

/**
 * The settings to use: the Settings drawer's saved choice, if it still passes
 * `checkSettings`, or the defaults. `_location` is accepted and ignored on purpose:
 * nothing in a link changes these.
 */
export function loadSettings(storage, _location) {
  let saved = null;
  try {
    saved = JSON.parse(storage?.getItem(STORE_KEY) ?? "null");
  } catch {
    saved = undefined; // unreadable: drop it below
  }
  if (saved === null) return { ...DEFAULTS };
  const checked = saved?.savedBy === SAVED_BY ? checkSettings(saved) : { error: "not from the drawer" };
  if (checked.error) {
    forget(storage, STORE_KEY);
    return { ...DEFAULTS };
  }
  return checked.value;
}

/** Saves a choice made in the Settings drawer (after `checkSettings`). Call it from nowhere else. */
export function saveSettings(storage, next) {
  const { rpc, registry, board, peer } = next;
  try {
    storage?.setItem(STORE_KEY, JSON.stringify({ rpc, registry, board, peer, savedBy: SAVED_BY }));
  } catch {
    // private windows: settings last until the page closes
  }
}

/** Forgets the saved choice and returns the defaults. */
export function resetSettings(storage) {
  forget(storage, STORE_KEY);
  return { ...DEFAULTS };
}
