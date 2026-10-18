// Names, version 1: read-only. Looks a name up straight from the chain's
// JSON-RPC in this browser (SPEC 12.3): the name travels after `#`, never to the
// host of these files, and no relay stands between the page and the chain.
//
// names.js is resolver/names.js bundled with @noble/hashes (Argon2id); regenerate it with
//   cd gateway && npx rolldown ../resolver/names.js --format esm --file public/app/names.js
// board.js reads a name's posts the way `names posts` does.

import {
  GRACE_MS,
  base58Decode,
  decodeNameRecord,
  decodePage,
  decodePageContent,
  decodeProfile,
  deriveName,
  isValidName,
  normalizeName,
  openSealed,
  pageAddress,
  publicView,
  readShard,
  recordAddress,
} from "./names.js";
import { postCount, postsOf } from "./board.js";

// Each chain the app knows, with the registry and board deployed on it, so
// picking one is a single tap. Another RPC takes typed program IDs.
const CHAINS = {
  "https://lez.buggers.online": {
    registry: "m82S3dUDxdcMBtZtXpHXYcVhU68U7bS4AKqsi6NomJm",
    board: "CKqLLZrTUNiw5KLtQCZ9oSDXWWEtCHLpKjp7pQAaP5q4",
  },
  "https://testnet.lez.logos.co": {
    registry: "38jEfVV1RCyMzu1FGbYcCn5FqxCxKyY58VgzjruAh2gq",
    board: "8PwnCKStKQtoBxJCgopCG6CPPMJCUWba1SDG1SXtXYTf",
  },
};
// The test chain while it is the only one browsers can reach.
const DEFAULTS = { rpc: "https://lez.buggers.online", ...CHAINS["https://lez.buggers.online"] };
const PRESETS = Object.keys(CHAINS);
const STORE_KEY = "lns-app-settings";
const PAGE_SIZE = 10;
// The policy of SPEC 12.2, less `sandbox` (a <meta> can't carry it; the iframe's
// sandbox attribute does that, without scripts). It comes first, before any byte of
// the page, so nothing in the page can load before it applies.
const PAGE_POLICY =
  "default-src 'none'; style-src 'unsafe-inline'; img-src data:; font-src data:; " +
  "media-src data:; form-action 'none'; base-uri 'none'";
const PAGE_HEAD =
  `<!doctype html><meta charset="utf-8">` +
  `<meta http-equiv="Content-Security-Policy" content="${PAGE_POLICY}">` +
  `<meta name="referrer" content="no-referrer"><base target="_blank">`;
const AVATAR = /^data:image\/(png|jpeg|gif|webp);base64,[A-Za-z0-9+/]+={0,2}$/;
const COLOR = /^#[0-9a-fA-F]{6}$/;
const SHOWN_ABOVE = new Set(["display", "bio", "avatar"]);
const MONO = new Set(["logos.account", "chat", "cid", "agent", "inbox"]);

const $ = (id) => document.getElementById(id);
const el = (tag, props = {}, ...children) => {
  const node = Object.assign(document.createElement(tag), props);
  node.append(...children);
  return node;
};

// ---------- settings ----------

function loadSettings() {
  try {
    const saved = { ...DEFAULTS, ...JSON.parse(localStorage.getItem(STORE_KEY) || "{}") };
    // A known chain always uses its own programs.
    return CHAINS[saved.rpc] ? { rpc: saved.rpc, ...CHAINS[saved.rpc] } : saved;
  } catch {
    return { ...DEFAULTS };
  }
}

function saveSettings(settings) {
  try {
    localStorage.setItem(STORE_KEY, JSON.stringify(settings));
  } catch {
    // Private windows and blocked storage: the settings last until the page closes.
  }
}

const isProgramId = (text) => {
  try {
    return base58Decode(text).length === 32;
  } catch {
    return false;
  }
};

let settings = loadSettings();

function fillSettings() {
  const preset = PRESETS.includes(settings.rpc) ? settings.rpc : "custom";
  for (const radio of document.querySelectorAll('input[name="rpc-choice"]')) radio.checked = radio.value === preset;
  $("rpc-custom").value = preset === "custom" ? settings.rpc : "";
  $("rpc-custom").hidden = preset !== "custom";
  $("registry").value = settings.registry;
  $("board").value = settings.board;
  $("programs").hidden = preset !== "custom";
  $("settings-error").hidden = true;
}

function readSettingsForm() {
  const choice = document.querySelector('input[name="rpc-choice"]:checked')?.value ?? DEFAULTS.rpc;
  const rpc = (choice === "custom" ? $("rpc-custom").value : choice).trim();
  const registry = $("registry").value.trim();
  const board = $("board").value.trim();
  let url;
  try {
    url = new URL(rpc);
  } catch {
    return { error: "The RPC URL isn't a valid web address." };
  }
  if (url.protocol !== "https:" && url.protocol !== "http:") return { error: "The RPC URL must start with https://." };
  if (!isProgramId(registry)) return { error: "The registry program isn't a valid program ID." };
  if (!isProgramId(board)) return { error: "The board program isn't a valid program ID." };
  return { value: { rpc, registry, board } };
}

function toggleSettings(open) {
  $("settings").hidden = !open;
  $("settings-button").setAttribute("aria-expanded", String(open));
  if (open) fillSettings();
}

$("settings-button").addEventListener("click", () => toggleSettings($("settings").hidden));
for (const radio of document.querySelectorAll('input[name="rpc-choice"]')) {
  radio.addEventListener("change", () => {
    const custom = radio.value === "custom" && radio.checked;
    $("rpc-custom").hidden = !custom;
    $("programs").hidden = !custom;
    if (CHAINS[radio.value] && radio.checked) {
      $("registry").value = CHAINS[radio.value].registry;
      $("board").value = CHAINS[radio.value].board;
    }
    if (custom) $("rpc-custom").focus();
  });
}
$("settings-save").addEventListener("click", () => {
  const { value, error } = readSettingsForm();
  if (error) {
    $("settings-error").textContent = error;
    $("settings-error").hidden = false;
    return;
  }
  settings = value;
  saveSettings(settings);
  toggleSettings(false);
  route();
});
$("settings-reset").addEventListener("click", () => {
  settings = { ...DEFAULTS };
  try {
    localStorage.removeItem(STORE_KEY);
  } catch {}
  fillSettings();
});

// ---------- reading the chain ----------

class ChainError extends Error {}

/** Why a request to `rpc` failed, in words a person can act on. */
async function explainFailure(error, rpc) {
  if (error instanceof ChainError) return error;
  if (!(error instanceof TypeError)) {
    return new ChainError("The chain answered with an error.", { cause: error });
  }
  const url = new URL(rpc);
  const local = ["localhost", "127.0.0.1", "[::1]"].includes(url.hostname);
  if (location.protocol === "https:" && url.protocol === "http:" && !local) {
    return new ChainError("This RPC uses http://, which a page on https:// can't reach. Use an https:// RPC.");
  }
  // The browser says the same for "blocked by CORS" and "nothing there". A request
  // the page can't read (no-cors) tells them apart: it gets through only if the
  // server answered.
  try {
    await fetch(rpc, { method: "POST", mode: "no-cors", body: "{}", cache: "no-store", referrerPolicy: "no-referrer" });
    // A server that should allow browsers but is down behind a proxy (an error page
    // without CORS headers) looks the same from here, hence the second sentence.
    return new ChainError(
      "This RPC doesn't allow browsers (CORS). If it normally does, it may be down. " +
        "Pick another in Settings, or use the names command on your computer.",
    );
  } catch {
    return new ChainError("Can't reach the chain. Check your connection or the RPC in Settings.");
  }
}

async function shard(rpc, program, account) {
  try {
    return await readShard(rpc, program, account);
  } catch (error) {
    throw await explainFailure(error, rpc);
  }
}

async function openAs(key, sealed, kind, decode) {
  if (sealed.length === 0) return null;
  const plain = await openSealed(key, sealed, kind);
  if (plain === null) return null;
  try {
    return decode(plain);
  } catch {
    return null;
  }
}

/**
 * What `name` resolves to, as resolveName in names.js does it, but keeping the
 * derived identifier and key so the page and posts don't each derive again
 * (Argon2id, 64 MiB, slow on a phone). They stay in memory only.
 */
async function lookUp({ rpc, registry }, name, now = Date.now()) {
  const { id, key } = await deriveName(name);
  const bytes = await shard(rpc, registry, await recordAddress(registry, id));
  if (bytes.length === 0) return { id, key, view: null };
  const record = decodeNameRecord(bytes);
  if (now >= record.expires + GRACE_MS) return { id, key, view: null };
  const expires = record.expires >= Number.MAX_SAFE_INTEGER ? null : record.expires;
  const held = { name, expires, lapsed: expires !== null && expires <= now };
  const profile = await openAs(key, record.sealed, "record", decodeProfile);
  const empty = { account: null, npk: null, vpk: null, texts: {} };
  const view = publicView(profile === null || profile.name !== name ? { ...held, ...empty } : { ...held, ...profile });
  return { id, key, view };
}

async function readPage({ rpc, registry }, id, key) {
  const bytes = await shard(rpc, registry, await pageAddress(registry, id));
  if (bytes.length === 0) return null;
  return openAs(key, decodePage(bytes).sealed, "page", decodePageContent);
}

// ---------- showing it ----------

function status(text, { error = false, detail = "" } = {}) {
  const box = $("status");
  box.replaceChildren(el("strong", { textContent: text }));
  if (detail) box.append(el("p", { textContent: detail }));
  box.classList.toggle("error", error);
  box.hidden = false;
}

function clearView() {
  for (const id of ["status", "card", "page-box", "posts-box"]) $(id).hidden = true;
  $("page").removeAttribute("srcdoc");
  $("posts").replaceChildren();
}

function renderCard(view) {
  const { texts } = view;
  const color = COLOR.test(texts.color ?? "") ? texts.color : null;
  $("card").style.setProperty("--name", color ?? "var(--accent)");
  $("display").textContent = texts.display?.trim() || view.name;
  $("bare").textContent = texts.display?.trim() ? view.name : "";
  $("bare").hidden = !texts.display?.trim();
  const avatar = $("avatar");
  if (AVATAR.test(texts.avatar ?? "")) {
    avatar.src = texts.avatar;
    avatar.alt = `${view.name}'s picture`;
    avatar.hidden = false;
  } else {
    avatar.removeAttribute("src");
    avatar.hidden = true;
  }
  $("bio").textContent = texts.bio ?? "";
  $("bio").hidden = !texts.bio;

  const facts = [];
  facts.push(view.payablePrivately ? ["Can be paid privately", true] : ["No private payments", false]);
  if (view.publicAccount) facts.push(["Has a public account", true]);
  if (view.expires === null) facts.push(["Held by deposit", false]);
  else if (view.lapsed) facts.push([`Lapsed on ${new Date(view.expires).toLocaleDateString()}; still in grace`, false]);
  else facts.push([`Held until ${new Date(view.expires).toLocaleDateString()}`, false]);
  $("facts").replaceChildren(...facts.map(([text, yes]) => el("li", { textContent: text, className: yes ? "yes" : "" })));

  const rows = [];
  if (view.publicAccount) {
    rows.push(el("dt", { textContent: "Public account" }), el("dd", { textContent: view.publicAccount, className: "mono" }));
  }
  for (const [key, value] of Object.entries(texts)) {
    if (SHOWN_ABOVE.has(key)) continue;
    const dd = el("dd", { className: MONO.has(key) ? "mono" : "" });
    if (key === "color" && COLOR.test(value)) {
      const swatch = el("span", { className: "swatch" });
      swatch.style.background = value;
      dd.append(swatch);
    }
    dd.append(value);
    rows.push(el("dt", { textContent: key }), dd);
  }
  $("texts").replaceChildren(...rows);
  $("card").hidden = false;
}

function renderPage(page) {
  const box = $("page-box");
  const note = $("page-note");
  const frame = $("page");
  if (page === null) return;
  const type = page.contentType.split(";")[0].trim().toLowerCase();
  const text = new TextDecoder().decode(page.body);
  box.hidden = false;
  if (type === "text/html") {
    frame.hidden = false;
    note.hidden = true;
    frame.srcdoc = PAGE_HEAD + text;
  } else if (type === "text/plain") {
    frame.hidden = false;
    note.hidden = true;
    const escaped = text.replace(/[&<>]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" })[c]);
    frame.srcdoc = `${PAGE_HEAD}<pre style="white-space:pre-wrap;font:15px/1.5 ui-monospace,monospace;margin:16px">${escaped}</pre>`;
  } else {
    frame.hidden = true;
    note.textContent = `This name's page is ${type}, which this app doesn't show.`;
    note.hidden = false;
  }
}

function renderPost(post, id) {
  const when = new Date(post.time).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
  let meta = `#${post.id.n} · ${when}`;
  if (post.parent) {
    const own = post.parent.author.every((byte, i) => byte === id[i]);
    meta += own ? ` · reply to #${post.parent.n}` : " · reply";
  }
  const item = el("li", {}, el("div", { className: "meta", textContent: meta }));
  if (post.text) item.append(el("p", { className: "text", textContent: post.text }));
  for (const cid of post.cids) item.append(el("p", { className: "cid", textContent: `file ${cid}` }));
  return item;
}

// ---------- flow ----------

let current = null; // { token, name, id, top }
let token = 0;

async function showName(name) {
  const mine = ++token;
  const active = () => mine === token;
  const config = settings;
  clearView();
  $("intro").hidden = true;
  $("name").value = name;
  if (!isValidName(name)) {
    status("That isn't a name.", { error: true, detail: "Names are 1–32 lowercase letters, digits or dashes, not starting or ending with a dash." });
    return;
  }
  status(`Looking up ${name}…`, { detail: "Working out where the name lives. This takes a moment." });
  $("go").disabled = true;
  try {
    const { id, key, view } = await lookUp(config, name);
    if (!active()) return;
    if (view === null) {
      status(`${name} isn't registered.`, { detail: "Nobody holds this name on this chain." });
      return;
    }
    $("status").hidden = true;
    renderCard(view);
    current = { name, id, top: 0 };
    const [page, count] = await Promise.all([
      readPage(config, id, key).catch((error) => ({ error })),
      shardCount(config, id).catch((error) => ({ error })),
    ]);
    if (!active()) return;
    if (page?.error) {
      $("page-box").hidden = false;
      $("page").hidden = true;
      $("page-note").textContent = `Couldn't read the page. ${page.error.message}`;
      $("page-note").hidden = false;
    } else {
      renderPage(page);
    }
    $("posts-box").hidden = false;
    if (count?.error) {
      postsNote(`Couldn't read the posts. ${count.error.message}`);
      return;
    }
    current.top = count;
    if (count === 0) {
      postsNote("No posts yet.");
      return;
    }
    await morePosts(config, mine);
  } catch (error) {
    if (!active()) return;
    status(error instanceof ChainError ? error.message : "Something went wrong reading this name.", {
      error: true,
      detail: error instanceof ChainError ? "" : String(error?.message ?? error),
    });
  } finally {
    if (active()) $("go").disabled = false;
  }
}

async function shardCount({ rpc, board }, id) {
  try {
    return await postCount(rpc, board, id);
  } catch (error) {
    throw await explainFailure(error, rpc);
  }
}

function postsNote(text) {
  $("posts-note").textContent = text;
  $("posts-note").hidden = !text;
}

async function morePosts(config, mine) {
  const more = $("more");
  more.disabled = true;
  more.textContent = "Loading…";
  try {
    const { name, id, top } = current;
    let posts;
    let next;
    try {
      ({ posts, next } = await postsOf(config.rpc, config.board, name, id, top, PAGE_SIZE));
    } catch (error) {
      throw await explainFailure(error, config.rpc);
    }
    if (mine !== token) return;
    current.top = next;
    $("posts").append(...posts.map((post) => renderPost(post, id)));
    postsNote($("posts").children.length === 0 && next === 0 ? "No posts to show." : "");
    more.hidden = next === 0;
  } catch (error) {
    if (mine !== token) return;
    postsNote(`Couldn't read more posts. ${error.message}`);
  } finally {
    more.disabled = false;
    more.textContent = "View more";
  }
}

$("more").addEventListener("click", () => {
  if (current) morePosts(settings, token);
});

$("lookup").addEventListener("submit", (event) => {
  event.preventDefault();
  const name = normalizeName($("name").value);
  if (!name) return;
  if (decodeURIComponent(location.hash.slice(1)) === name) route();
  else location.hash = name;
});

function route() {
  const name = normalizeName(decodeURIComponent(location.hash.slice(1)));
  if (!name) {
    token += 1;
    clearView();
    $("intro").hidden = false;
    $("name").value = "";
    return;
  }
  showName(name);
}

window.addEventListener("hashchange", route);
route();
