// Vault: the Messages device key (X25519 secret, 32 bytes) encrypted at rest.
// Saving the key REQUIRES a passkey (WebAuthn PRF). There is no password option.
// The only non-passkey path is "use without saving" — the key lives in memory for
// the session and nothing is persisted (handled in messages.js, not here).
//
// The secret is only ever held in memory once unlocked. Build the browser file:
//   cd gateway/app-src && npm install
//   npx rolldown vault.js --format esm --file ../public/app/vault.js
// (rolldown is gateway's; @noble/hashes is pinned in app-src/package.json.)
//
// Record on disk (byte fields lower-hex unless noted), replacing the old { secretKey }:
//   { v:1, kdf:"passkey", cred(base64url), prfSalt, iv, ct }
// ct = AES-GCM(iv, key) over the 32-byte secret, key = HKDF-SHA256(passkey PRF output).
// Lose the passkey and the saved key is unrecoverable — that's fine, the messaging
// key is rotatable: make a new one and republish it with `set-text msg`.

import { hkdf } from "@noble/hashes/hkdf.js";
import { sha256 } from "@noble/hashes/sha2.js";

export const VAULT_VERSION = 1;
const PRF_INFO = new TextEncoder().encode("names/msg/vault/prf/v1");

// ---------- hex / base64url ----------

export function toHex(bytes) {
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}
export function fromHex(hex) {
  if (typeof hex !== "string" || !/^(?:[0-9a-fA-F]{2})*$/.test(hex)) throw new Error("not hex");
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(hex.slice(2 * i, 2 * i + 2), 16);
  return out;
}
function toB64url(bytes) {
  let s = "";
  for (const b of bytes) s += String.fromCharCode(b);
  return btoa(s).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}
function fromB64url(s) {
  const b = atob(s.replace(/-/g, "+").replace(/_/g, "/"));
  const out = new Uint8Array(b.length);
  for (let i = 0; i < b.length; i++) out[i] = b.charCodeAt(i);
  return out;
}

function randomBytes(n) {
  const out = new Uint8Array(n);
  crypto.getRandomValues(out);
  return out;
}

// ---------- AES-GCM over the 32-byte secret, under a 32-byte key ----------
// These are pure (no WebAuthn), so the encrypt/decrypt path is testable with an
// injected key standing in for the passkey's PRF output.

async function aesKey(raw32) {
  return crypto.subtle.importKey("raw", raw32, { name: "AES-GCM" }, false, ["encrypt", "decrypt"]);
}

/** Encrypt a 32-byte secret under a 32-byte key. Returns { iv, ct } (hex). */
export async function encryptUnderKey(secret32, keyBytes32) {
  if (!(secret32 instanceof Uint8Array) || secret32.length !== 32) throw new Error("need a 32-byte secret");
  if (!(keyBytes32 instanceof Uint8Array) || keyBytes32.length !== 32) throw new Error("need a 32-byte key");
  const iv = randomBytes(12);
  const key = await aesKey(keyBytes32);
  const ct = new Uint8Array(await crypto.subtle.encrypt({ name: "AES-GCM", iv }, key, secret32));
  return { iv: toHex(iv), ct: toHex(ct) };
}

/** Decrypt { iv, ct } under a 32-byte key. Throws "wrong key" if it doesn't match. */
export async function decryptUnderKey(record, keyBytes32) {
  const key = await aesKey(keyBytes32);
  let plain;
  try {
    plain = await crypto.subtle.decrypt({ name: "AES-GCM", iv: fromHex(record.iv) }, key, fromHex(record.ct));
  } catch {
    throw new Error("wrong key");
  }
  const secret = new Uint8Array(plain);
  if (secret.length !== 32) throw new Error("vault is corrupt");
  return secret;
}

/** The AES key for a vault = HKDF-SHA256 of the passkey's PRF output. Pure, testable. */
export function keyFromPrf(prfSecret) {
  const s = prfSecret instanceof Uint8Array ? prfSecret : new Uint8Array(prfSecret);
  return hkdf(sha256, s, new Uint8Array(0), PRF_INFO, 32);
}

// ---------- record shape ----------

export function isVaultRecord(obj) {
  return !!obj && obj.v === VAULT_VERSION && obj.kdf === "passkey" &&
    typeof obj.ct === "string" && typeof obj.iv === "string" && typeof obj.cred === "string";
}
export function isLegacyPlaintext(obj) {
  return !!obj && typeof obj.secretKey === "string" && obj.ct === undefined;
}

// ---------- passkey (WebAuthn PRF) — the only way to persist ----------

/** Is the WebAuthn machinery present at all? (PRF itself is only known at use time.) */
export function passkeySupported() {
  return typeof PublicKeyCredential !== "undefined" && typeof navigator !== "undefined" && !!navigator.credentials;
}

async function prfSecret({ allowId = null, prfSalt, create = false, rpId, userName }) {
  if (create) {
    const cred = await navigator.credentials.create({
      publicKey: {
        rp: { name: "Logos Names", id: rpId },
        user: { id: randomBytes(16), name: userName || "names-messages", displayName: userName || "Names Messages" },
        challenge: randomBytes(32),
        pubKeyCredParams: [{ type: "public-key", alg: -7 }, { type: "public-key", alg: -257 }],
        authenticatorSelection: { residentKey: "preferred", userVerification: "preferred" },
        extensions: { prf: {} },
        timeout: 60000,
      },
    });
    if (!cred) throw new Error("passkey creation was cancelled");
    // A fresh credential rarely returns PRF output on create; read it with a get().
    const got = await prfSecret({ allowId: new Uint8Array(cred.rawId), prfSalt });
    return { credId: new Uint8Array(cred.rawId), secret: got.secret };
  }
  const assertion = await navigator.credentials.get({
    publicKey: {
      challenge: randomBytes(32),
      allowCredentials: allowId ? [{ type: "public-key", id: allowId }] : [],
      userVerification: "preferred",
      extensions: { prf: { eval: { first: prfSalt } } },
      timeout: 60000,
    },
  });
  const results = assertion?.getClientExtensionResults?.().prf?.results;
  if (!results?.first) throw new Error("this browser or key can't securely save the key (no passkey PRF support)");
  return { credId: new Uint8Array(assertion.rawId), secret: new Uint8Array(results.first) };
}

/** Enroll a passkey and encrypt the secret under its PRF output. Browser-only. */
export async function enrollPasskey(secret32, { rpId, userName } = {}) {
  if (!passkeySupported()) throw new Error("passkeys aren't available here");
  const prfSalt = randomBytes(32);
  const { credId, secret } = await prfSecret({ prfSalt, create: true, rpId, userName });
  const { iv, ct } = await encryptUnderKey(secret32, keyFromPrf(secret));
  return { v: VAULT_VERSION, kdf: "passkey", cred: toB64url(credId), prfSalt: toHex(prfSalt), iv, ct };
}

/** Unlock a passkey record. Browser-only. Throws if the passkey is gone/declined. */
export async function unlockPasskey(record) {
  if (!isVaultRecord(record)) throw new Error("not a passkey vault");
  const { secret } = await prfSecret({ allowId: fromB64url(record.cred), prfSalt: fromHex(record.prfSalt) });
  return decryptUnderKey(record, keyFromPrf(secret));
}
