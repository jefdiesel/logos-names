// Shared postMessage protocol between the Names app (parent) and the money-key
// signer (a page on its OWN origin, embedded as an iframe). Pure + dependency-free
// so it can be unit-tested in node. The SAME file is served on both origins (a copy
// lives in public/app/); keep the two identical.
//
// Envelope: every message carries { ns, dir, ... }. `dir` is "req" (parent→signer),
// "res" (signer→parent reply) or "evt" (signer→parent, unsolicited: ready/show/hide).
// NO secret ever travels in any of these — see the signer's threat-model comment.

export const NS = "lns-signer/1";
export const RPC_TIMEOUT_MS = 90000; // signing waits on the user + a passkey + the chain

export function isAllowedOrigin(origin, allowed) {
  if (!origin || typeof origin !== "string" || origin === "null") return false;
  const list = Array.isArray(allowed) ? allowed : [allowed];
  return list.some((a) => typeof a === "string" && a.length > 0 && a === origin);
}

// ---- envelope builders ----
export const reqMsg = (id, method, params) => ({ ns: NS, dir: "req", id, method, params: params ?? {} });
export const resMsg = (id, ok, payload) =>
  ({ ns: NS, dir: "res", id, ok, ...(ok ? { result: payload ?? null } : { error: payload ?? { code: "error", message: "error" } }) });
export const evtMsg = (event, data) => ({ ns: NS, dir: "evt", event, ...(data !== undefined ? { data } : {}) });

// ---- parsers (return null when the message isn't ours, so other listeners are ignored) ----
export function parseRequest(data) {
  if (!data || typeof data !== "object" || data.ns !== NS || data.dir !== "req") return null;
  if (typeof data.id !== "string" || typeof data.method !== "string") return null;
  return { id: data.id, method: data.method, params: data.params ?? {} };
}
export function parseReply(data) {
  if (!data || typeof data !== "object" || data.ns !== NS || data.dir !== "res") return null;
  if (typeof data.id !== "string" || typeof data.ok !== "boolean") return null;
  return { id: data.id, ok: data.ok, result: data.result, error: data.error };
}
export function parseEvent(data) {
  if (!data || typeof data !== "object" || data.ns !== NS || data.dir !== "evt") return null;
  if (typeof data.event !== "string") return null;
  return { event: data.event, data: data.data };
}

// ---- shape-only validation of signAndSend params (no chain/account parsing here) ----
const DIGITS = /^[0-9]+$/;
export function validateSignAndSend(params) {
  const p = params ?? {};
  const to = typeof p.to === "string" ? p.to.trim() : "";
  if (!to) throw { code: "bad_params", message: "a recipient is required" };
  const amount = (typeof p.amount === "number" ? String(p.amount) : String(p.amount ?? "")).trim();
  if (!DIGITS.test(amount) || /^0+$/.test(amount)) {
    throw { code: "bad_params", message: "amount must be a positive whole number of base units" };
  }
  let gasLimit = null;
  if (p.gasLimit !== undefined && p.gasLimit !== null && p.gasLimit !== "") {
    gasLimit = (typeof p.gasLimit === "number" ? String(p.gasLimit) : String(p.gasLimit)).trim();
    if (!DIGITS.test(gasLimit) || /^0+$/.test(gasLimit)) {
      throw { code: "bad_params", message: "gasLimit must be a positive whole number" };
    }
  }
  const rpc = typeof p.rpc === "string" ? p.rpc.trim() : "";
  if (!/^https?:\/\/[^\s]+$/.test(rpc)) throw { code: "bad_params", message: "a valid rpc URL is required" };
  return { to, amount, gasLimit, rpc };
}
