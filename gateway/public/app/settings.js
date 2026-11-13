// Which chain (sequencer RPC, registry, board) and messaging node the web app uses.
//
// Only the Settings drawer changes them. A link never does: ?rpc= and ?peer= in the
// address are ignored. They used to be read and saved for good, so a link could point
// this browser at a sequencer of the sender's choosing, which can answer lookups with
// anything (pressure-test finding C2, 2026-11-13). Settings saved before this change
// carry no `savedBy` mark, may have been written by such a link, and are dropped once.

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
export const STORE_KEY = "lns-app-settings";
export const PEER_RE = /^\/(dns4|dns6|dns|ip4|ip6)\/[^/]+\/tcp\/\d+\/(wss|tls\/ws)\/p2p\/\w+$/;
// Marks a record written by the Settings drawer.
const SAVED_BY = "settings-drawer";

const text = (value) => typeof value === "string" && value.length > 0;

/**
 * The settings to use: the Settings drawer's saved choice, or the defaults.
 * `_location` is accepted and ignored on purpose: nothing in a link changes these.
 */
export function loadSettings(storage, _location) {
  let saved = null;
  try {
    saved = JSON.parse(storage?.getItem(STORE_KEY) ?? "null");
  } catch {
    saved = undefined; // unreadable: drop it below
  }
  if (saved !== null && (saved?.savedBy !== SAVED_BY || !text(saved.rpc))) {
    try {
      storage?.removeItem(STORE_KEY);
    } catch {}
    saved = null;
  }
  if (!saved) return { ...DEFAULTS };
  const peer = text(saved.peer) && PEER_RE.test(saved.peer) ? saved.peer : DEFAULT_PEER;
  if (CHAINS[saved.rpc]) return { rpc: saved.rpc, ...CHAINS[saved.rpc], peer };
  if (!text(saved.registry) || !text(saved.board)) return { ...DEFAULTS };
  return { rpc: saved.rpc, registry: saved.registry, board: saved.board, peer };
}

/** Saves a choice made in the Settings drawer. Call it from nowhere else. */
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
  try {
    storage?.removeItem(STORE_KEY);
  } catch {}
  return { ...DEFAULTS };
}
