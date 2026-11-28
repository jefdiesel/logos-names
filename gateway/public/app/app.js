// Names webapp (LNS v8 on LEZ v0.4.0): wallet, paid/deposit registration with term choices,
// renewals, secondary market (2.5% fee), subnames (create/sale/buy), on-chain sites directory,
// and encrypted chat — all directly from the browser against the LEZ JSON-RPC.

import {
  base58Decode,
  decodeNameRecord,
  deriveName,
  isValidLabel,
  isValidName,
  normalizeName,
  ownerTag as ownerTagAsync,
  priceTierForLen,
  publicView,
  readActorState,
  recordAddress,
  resolveName,
  splitSubname,
  tieredId,
} from "./names.js";

const CHAINS = {
  "https://lez.buggers.online/lez": {
    registry: "38jEfVV1RCyMzu1FGbYcCn5FqxCxKyY58VgzjruAh2gq",
    board: "8PwnCKStKQtoBxJCgopCG6CPPMJCUWba1SDG1SXtXYTf",
  },
  "https://lez.buggers.online": {
    registry: "m82S3dUDxdcMBtZtXpHXYcVhU68U7bS4AKqsi6NomJm",
    board: "CKqLLZrTUNiw5KLtQCZ9oSDXWWEtCHLpKjp7pQAaP5q4",
  },
};
const DEFAULT_PEER =
  "/dns4/msg.buggers.online/tcp/443/wss/p2p/16Uiu2HAkxrwXguVQUCPqMyXrn9VuPNce2xoK9Nr7AW2zcHQeS6dg";
const DEFAULTS = { rpc: "https://lez.buggers.online/lez", ...CHAINS["https://lez.buggers.online/lez"], peer: DEFAULT_PEER };
const PRESETS = Object.keys(CHAINS);
const STORE_KEY = "lns-app-settings";
const ONBOARD_KEY = "lns-onboarded";
const REGISTER_PENDING = "lns-register-pending";
const SUBNAME_BUY_PENDING = "lns-subname-buy-pending";
const SITES_PAGE_SIZE = 20;

const $ = (id) => document.getElementById(id);

// ---------- settings ----------

const PEER_RE = /^\/(dns4|dns6|dns|ip4|ip6)\/[^/]+\/tcp\/\d+\/(wss|tls\/ws)\/p2p\/\w+$/;

function urlSettings() {
  const out = {};
  try {
    const q = new URLSearchParams(location.search);
    const peer = q.get("peer");
    if (peer && PEER_RE.test(peer)) out.peer = peer;
    const rpc = q.get("rpc");
    if (rpc) {
      const u = new URL(rpc);
      if (u.protocol === "https:" || u.protocol === "http:") out.rpc = rpc;
    }
  } catch {}
  return out;
}

function loadSettings() {
  try {
    const saved = { ...DEFAULTS, ...JSON.parse(localStorage.getItem(STORE_KEY) || "{}"), ...urlSettings() };
    return CHAINS[saved.rpc] ? { rpc: saved.rpc, ...CHAINS[saved.rpc], peer: saved.peer } : saved;
  } catch {
    return { ...DEFAULTS, ...urlSettings() };
  }
}

function saveSettings(next) {
  try { localStorage.setItem(STORE_KEY, JSON.stringify(next)); } catch {}
}

const isProgramId = (text) => {
  try { return base58Decode(text).length === 32; } catch { return false; }
};

let settings = loadSettings();
if (Object.keys(urlSettings()).length) saveSettings(settings);

function fillSettings() {
  const preset = PRESETS.includes(settings.rpc) ? settings.rpc : "custom";
  for (const radio of document.querySelectorAll('input[name="rpc-choice"]')) radio.checked = radio.value === preset;
  $("rpc-custom").value = preset === "custom" ? settings.rpc : "";
  $("rpc-custom").hidden = preset !== "custom";
  $("registry").value = settings.registry;
  $("board").value = settings.board;
  $("peer").value = settings.peer;
  $("programs").hidden = preset !== "custom";
  $("settings-error").hidden = true;
}

function readSettingsForm() {
  const choice = document.querySelector('input[name="rpc-choice"]:checked')?.value ?? DEFAULTS.rpc;
  const rpc = (choice === "custom" ? $("rpc-custom").value : choice).trim();
  const registry = $("registry").value.trim();
  const board = $("board").value.trim();
  const peer = $("peer").value.trim() || DEFAULT_PEER;
  let url;
  try { url = new URL(rpc); } catch { return { error: "The RPC URL isn't a valid web address." }; }
  if (url.protocol !== "https:" && url.protocol !== "http:") return { error: "The RPC URL must start with https://." };
  if (!isProgramId(registry)) return { error: "The registry program isn't a valid program ID." };
  if (!isProgramId(board)) return { error: "The board program isn't a valid program ID." };
  if (!PEER_RE.test(peer)) {
    return { error: "The messaging node must be a WebSocket address like /dns4/…/tcp/443/wss/p2p/…" };
  }
  return { value: { rpc, registry, board, peer } };
}

// ---------- chain helpers ----------

class ChainError extends Error {}

async function explainFailure(error, rpc) {
  if (error instanceof ChainError) return error;
  if (!(error instanceof TypeError)) {
    return new ChainError(error?.message || "The chain answered with an error.", { cause: error });
  }
  const url = new URL(rpc);
  const local = ["localhost", "127.0.0.1", "[::1]"].includes(url.hostname);
  if (location.protocol === "https:" && url.protocol === "http:" && !local) {
    return new ChainError("This RPC uses http://, which a page on https:// can't reach. Use an https:// RPC.");
  }
  try {
    await fetch(rpc, { method: "POST", mode: "no-cors", body: "{}", cache: "no-store", referrerPolicy: "no-referrer" });
    return new ChainError("This RPC doesn't allow browsers (CORS), or it may be down. Pick another in Settings.");
  } catch {
    return new ChainError("Can't reach the chain. Check your connection or the RPC in Settings.");
  }
}

async function lookUp({ rpc, registry }, name, now = Date.now()) {
  const resolved = await resolveName(rpc, registry, name, now);
  return { view: resolved ? publicView(resolved) : null };
}

function readStorage(key) {
  try { return localStorage.getItem(key); } catch { return null; }
}
const onboarded = () => readStorage(ONBOARD_KEY) != null;
function setOnboarded(name) {
  try { localStorage.setItem(ONBOARD_KEY, JSON.stringify({ name: name || null })); } catch {}
}
function onboardName() {
  try { return JSON.parse(readStorage(ONBOARD_KEY) || "{}").name || ""; } catch { return ""; }
}
function readJson(key) {
  try { return JSON.parse(localStorage.getItem(key) || "null"); } catch { return null; }
}
function saveJson(key, value) {
  try { localStorage.setItem(key, JSON.stringify(value)); } catch {}
}
function clearKey(key) {
  try { localStorage.removeItem(key); } catch {}
}

function setStatus(id, text, tone = "") {
  const el = $(id);
  if (!el) return;
  el.textContent = text;
  el.className = "me-status" + (tone ? " " + tone : "");
  el.hidden = !text;
}

const fmtTokens = (n) => BigInt(n).toLocaleString("en-US");
const fmtDate = (ms) => (ms == null ? "Permanent" : new Date(ms).toLocaleDateString());

async function getUnlockedWallet() {
  walletMod ??= await import("./wallet.js");
  walletMod.ensureLoaded?.();
  const key = walletMod.walletKey();
  const account = walletMod.walletAccount();
  if (!key) throw new Error("Open Wallet and unlock your wallet first.");
  return { key, account };
}

async function loadRegisterMod() {
  registerMod ??= await import("./register.js");
  registerMod.setRpc(settings.rpc);
  return registerMod;
}

// ---------- navigation ----------

let messages = null;
let walletMod = null;
let registerMod = null;
let sitesMod = null;
let marketMod = null;
let step = null;

const VIEWS = {
  wallet: "step-wallet",
  name: "step-name",
  market: "step-market",
  subnames: "step-subnames",
  sites: "step-sites",
  chat: "chat",
};

function setProgress(name) {
  for (const li of document.querySelectorAll("#progress li")) {
    li.classList.toggle("on", li.dataset.step === name);
  }
}

function show(next) {
  step = next;
  for (const [key, id] of Object.entries(VIEWS)) {
    $(id).hidden = key !== next;
  }
  setProgress(next);
  if (next === "wallet") openWallet();
  else if (next === "name") openName();
  else if (next === "market") openMarket();
  else if (next === "subnames") openSubnames();
  else if (next === "sites") openSites();
  else if (next === "chat") openChat();
}

async function openWallet() {
  $("wallet-continue").hidden = true;
  try {
    walletMod ??= await import("./wallet.js");
    walletMod.open({
      settings: () => settings,
      explain: explainFailure,
      onReady: () => { $("wallet-continue").hidden = false; },
    });
  } catch (error) {
    const box = $("wallet-lock-error");
    box.textContent = "Couldn't load the wallet. Reload and try again.";
    box.hidden = false;
    console.error(error);
  }
}

function openName() {
  offerTakeback();
  const input = $("pick-name");
  const pend = readJson(REGISTER_PENDING);
  if (pend && isValidLabel(pend.name)) {
    input.value = pend.name;
    $("pick-chosen").textContent = pend.name;
    $("pick-command").textContent = `names register ${pend.name} --public`;
    $("pick-result").textContent = "";
    $("pick-claim").hidden = false;
    claimName(pend.name, pend.term ?? 1);
    return;
  }
  if (onboardName() && !input.value) input.value = onboardName();
  setTimeout(() => input.focus(), 50);
}

async function openChat() {
  const me = onboardName();
  if (me) {
    const field = $("msg-me");
    if (field && !field.value) field.value = me;
  }
  const to = normalizeName(decodeURIComponent(location.hash.slice(1)));
  if (to && !["wallet", "name", "market", "subnames", "sites", "chat"].includes(to)) {
    const field = $("msg-to");
    if (field) field.value = to;
  }
  try {
    messages ??= await import("./messages.js");
    messages.open({ claimedName: onboardName(), settings: () => settings, lookUp: (name) => lookUp(settings, name), explain: explainFailure });
  } catch (error) {
    $("msg-conn").textContent = "Couldn't load chat";
    console.error(error);
  }
}

// ---------- navigation wiring ----------

$("wallet-continue").addEventListener("click", () => show("name"));
$("wallet-skip-all").addEventListener("click", () => {
  setOnboarded(null);
  show("chat");
});

for (const li of document.querySelectorAll("#progress li")) {
  li.style.cursor = "pointer";
  li.setAttribute("role", "button");
  li.setAttribute("tabindex", "0");
  li.addEventListener("click", () => show(li.dataset.step));
  li.addEventListener("keydown", (e) => {
    if (e.key === "Enter" || e.key === " ") { e.preventDefault(); show(li.dataset.step); }
  });
}

// ---------- NAMES: check, paid registration with term table, renew, list/cancel ----------

let currentQuote = null;
let currentManaged = null;
let registering = false;

function selectedRegTerm() {
  return Number(document.querySelector('input[name="reg-term"]:checked')?.value ?? 1);
}

function renderQuotePricing() {
  if (!currentQuote) return;
  const { deposit, priceTier, prices, premium, basePrice, term } = currentQuote;
  const reg = registerMod;
  const chosenTerm = selectedRegTerm();
  $("pick-terms").hidden = deposit;
  if (deposit) {
    $("pick-tier-note").textContent = "This registry is currently in refundable deposit mode.";
    $("pick-price-summary").textContent =
      `Refundable deposit: ${fmtTokens(basePrice)} tokens plus gas. Takes ~5 minutes between commit and registration.`;
    $("pick-premium-note").hidden = true;
    return;
  }
  const tierLabel = reg?.TIER_LABELS?.[priceTier] ?? "";
  $("pick-tier-note").textContent = `Price tier: ${tierLabel} (read live from the chain).`;
  for (let t = 0; t < 4; t += 1) {
    const el = $(`price-term-${t}`);
    if (el) el.textContent = `${fmtTokens(prices[priceTier][t])} tokens`;
  }
  const chosenBase = prices[priceTier][chosenTerm];
  const total = chosenBase + premium;
  if (premium > 0n) {
    setStatus(
      "pick-premium-note",
      `Recently expired name: includes a falling re-release premium of ${fmtTokens(premium)} tokens (drops daily over 21 days).`,
    );
  } else {
    $("pick-premium-note").hidden = true;
  }
  $("pick-price-summary").textContent =
    `Total: ${fmtTokens(total)} tokens (${reg?.TERM_LABELS?.[chosenTerm] ?? ""}) plus gas. Takes ~5 minutes between committing and registering so nobody can grab it first.`;
}

for (const radio of document.querySelectorAll('input[name="reg-term"]')) {
  radio.addEventListener("change", renderQuotePricing);
}

function renderRenewPrice() {
  if (!currentManaged) return;
  const term = Number($("renew-term").value);
  const { settings: regSettings, record, priceTier } = currentManaged;
  if (regSettings?.mode === 1) {
    $("renew-price-note").textContent = "Deposit mode: gas only.";
    return;
  }
  const tier = record.priceTier ?? priceTier;
  const table = Array.isArray(regSettings?.prices?.[0]) ? regSettings.prices : registerMod?.DEFAULT_PRICES;
  const price = table?.[tier]?.[term] ?? 0n;
  $("renew-price-note").textContent = `Renewal fee: ${fmtTokens(price)} tokens plus gas.`;
}

$("renew-term").addEventListener("change", renderRenewPrice);

function updateListFeePreview() {
  const raw = $("list-price").value.trim().replace(/[_,]/g, "");
  if (!raw || !/^\d+$/.test(raw)) {
    $("list-fee-preview").textContent = "2.5% treasury fee is deducted on sale; you receive 97.5%.";
    return;
  }
  const p = BigInt(raw);
  if (p < 40n) {
    $("list-fee-preview").textContent = "Minimum listing price is 40 tokens.";
    return;
  }
  const fee = (p + 39n) / 40n;
  const seller = p - fee;
  $("list-fee-preview").textContent = `Buyer pays ${fmtTokens(p)} → you receive ${fmtTokens(seller)} (97.5%), treasury fee ${fmtTokens(fee)} (2.5%).`;
}
$("list-price").addEventListener("input", updateListFeePreview);

$("pick-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  const name = normalizeName($("pick-name").value);
  const claim = $("pick-claim");
  const manage = $("manage-box");
  claim.hidden = true;
  manage.hidden = true;
  currentQuote = null;
  currentManaged = null;
  setStatus("pick-result", "");
  if (!name || !isValidLabel(name)) {
    setStatus("pick-result", "Top-level names are lowercase letters, digits or single dashes (no --). For subnames, open the Subnames tab.", "bad");
    return;
  }
  if (name.length < 5) {
    setStatus("pick-result", "Names under 5 characters are locked.", "bad");
    return;
  }
  setStatus("pick-result", `Checking ${name}…`);
  try {
    const reg = await loadRegisterMod();
    const quote = await reg.quoteRegistration({ program: settings.registry, name, term: selectedRegTerm() });
    if (quote.available) {
      currentQuote = quote;
      setStatus("pick-result", `✓ ${name} is available.`, "ok");
      $("pick-chosen").textContent = name;
      $("pick-command").textContent = `names register ${name} --public`;
      renderQuotePricing();
      claim.hidden = false;
      setOnboarded(name);
      $("msg-me") && ($("msg-me").value = name);
      return;
    }

    // Taken: show manage panel (renew, list/cancel if owner, or buy if listed).
    setStatus("pick-result", `${name} is registered.`);
    const rec = quote.record;
    currentManaged = quote;
    $("manage-name").textContent = name;
    const perm = rec.expires >= Number.MAX_SAFE_INTEGER;
    const lapsed = !perm && quote.now >= rec.expires;
    $("manage-meta").textContent = perm
      ? "Permanent name."
      : lapsed
        ? `Expired on ${fmtDate(rec.expires)} (in 30-day grace period).`
        : `Active until ${fmtDate(rec.expires)}.`;

    walletMod ??= await import("./wallet.js");
    walletMod.ensureLoaded?.();
    const myAccount = walletMod.walletAccount();
    const isMine = myAccount
      ? rec.ownerTag === (await ownerTagAsync(quote.slotId, myAccount))
      : false;

    const listing = rec.listing;
    const listingLive = listing && !lapsed && !rec.nftShielded && (listing.expiresAt === null || quote.now < listing.expiresAt);
    if (listingLive) {
      const split = reg.saleSplit(listing.price);
      setStatus(
        "manage-listing-banner",
        `Listed for ${fmtTokens(listing.price)} tokens (seller gets ${fmtTokens(split.seller)}, 2.5% fee ${fmtTokens(split.fee)}).`,
        "ok",
      );
    } else {
      $("manage-listing-banner").hidden = true;
    }

    $("manage-buy-wrap").hidden = !(listingLive && !isMine);
    $("manage-renew-wrap").hidden = perm;
    if (!perm) renderRenewPrice();
    $("manage-list-wrap").hidden = !isMine;
    $("cancel-list-go").hidden = !listing;
    setStatus("renew-status", "");
    setStatus("list-status", "");
    updateListFeePreview();
    manage.hidden = false;
  } catch (error) {
    const why = (await explainFailure(error, settings.rpc)).message;
    setStatus("pick-result", `Couldn't check: ${why}`, "bad");
  }
});

$("pick-command-copy").addEventListener("click", (event) => copy($("pick-command").textContent, event.currentTarget));

async function claimName(name, term = selectedRegTerm()) {
  if (registering) return;
  registering = true;
  $("pick-register").disabled = true;
  try {
    const { key, account } = await getUnlockedWallet();
    const reg = await loadRegisterMod();
    const pend = readJson(REGISTER_PENDING);
    const resume = pend && pend.name === name && pend.owner === account
      ? { secretHex: pend.secretHex, made: pend.made, public: Boolean(pend.public) }
      : null;
    const useTerm = resume && pend.term !== undefined ? pend.term : term;
    await reg.registerPublic({
      privateKey: key, program: settings.registry, name, term: useTerm, resume,
      acceptClaimRisk: (until) => window.confirm(
        `Until ${new Date(until).toLocaleDateString()}, whoever held ${name} on the old chain can take it back. Continue?`,
      ),
      onStep: (stage, detail) => {
        if (stage === "commit") setStatus("pick-progress", "Committing to the name…");
        else if (stage === "committed") {
          saveJson(REGISTER_PENDING, { name, owner: account, term: useTerm, secretHex: detail.secretHex, made: detail.made, public: Boolean(detail.public) });
          setStatus("pick-progress", "Committed. Waiting ~5 minutes so nobody can take it first — safe to leave; it resumes.");
        } else if (stage === "waiting") {
          const m = Math.max(1, Math.ceil(detail.remainingMs / 60000));
          setStatus("pick-progress", `Waiting ~${m} min before registering… (safe to leave; it resumes)`);
        } else if (stage === "register") setStatus("pick-progress", "Registering the name on chain…");
        else if (stage === "done") setStatus("pick-progress", "Registered ✓", "ok");
      },
    });
    clearKey(REGISTER_PENDING);
    setOnboarded(name);
    setStatus("pick-progress", `✓ ${name} is yours.`, "ok");
  } catch (error) {
    if (error?.code === "expired") clearKey(REGISTER_PENDING);
    setStatus("pick-progress", (error && error.message) || String(error), "bad");
  } finally {
    $("pick-register").disabled = false;
    registering = false;
  }
}

$("pick-register").addEventListener("click", () => {
  const name = normalizeName($("pick-chosen").textContent || $("pick-name").value);
  if (name) claimName(name, selectedRegTerm());
});

$("renew-go").addEventListener("click", async () => {
  if (!currentManaged) return;
  $("renew-go").disabled = true;
  setStatus("renew-status", "Renewing…");
  try {
    const { key } = await getUnlockedWallet();
    const reg = await loadRegisterMod();
    const term = Number($("renew-term").value);
    const res = await reg.renewName({ privateKey: key, program: settings.registry, name: currentManaged.name, term });
    setStatus("renew-status", `✓ Renewed (${fmtDate(res.expires)}).`, "ok");
  } catch (error) {
    setStatus("renew-status", error?.message || String(error), "bad");
  } finally {
    $("renew-go").disabled = false;
  }
});

$("list-go").addEventListener("click", async () => {
  if (!currentManaged) return;
  const raw = $("list-price").value.trim().replace(/[_,]/g, "");
  if (!/^\d+$/.test(raw)) { setStatus("list-status", "Enter a whole number of tokens.", "bad"); return; }
  const daysRaw = $("list-days").value.trim();
  let expiresAt = null;
  if (daysRaw) {
    const days = Number(daysRaw);
    if (!Number.isFinite(days) || days <= 0) { setStatus("list-status", "Enter a positive number of days.", "bad"); return; }
    expiresAt = Date.now() + Math.round(days * 86_400_000);
  }
  $("list-go").disabled = true;
  setStatus("list-status", "Listing on chain…");
  try {
    const { key } = await getUnlockedWallet();
    const reg = await loadRegisterMod();
    const res = await reg.listName({
      privateKey: key, program: settings.registry, name: currentManaged.name,
      price: BigInt(raw), expiresAt, unlisted: $("list-unlisted").checked,
    });
    $("cancel-list-go").hidden = false;
    setStatus("list-status", `✓ Listed for ${fmtTokens(res.listing.price)} tokens (fee ${fmtTokens(res.split.fee)} on sale).`, "ok");
  } catch (error) {
    setStatus("list-status", error?.message || String(error), "bad");
  } finally {
    $("list-go").disabled = false;
  }
});

$("cancel-list-go").addEventListener("click", async () => {
  if (!currentManaged) return;
  $("cancel-list-go").disabled = true;
  setStatus("list-status", "Cancelling listing…");
  try {
    const { key } = await getUnlockedWallet();
    const reg = await loadRegisterMod();
    await reg.cancelListing({ privateKey: key, program: settings.registry, name: currentManaged.name });
    $("cancel-list-go").hidden = true;
    setStatus("list-status", "✓ Listing cancelled.", "ok");
  } catch (error) {
    setStatus("list-status", error?.message || String(error), "bad");
  } finally {
    $("cancel-list-go").disabled = false;
  }
});

$("manage-buy-go").addEventListener("click", async () => {
  if (!currentManaged) return;
  await executeBuyName(currentManaged.name, "manage-listing-banner", $("manage-buy-go"));
});

// Takeback (legacy rc3 claims)
async function offerTakeback() {
  try {
    const reg = await loadRegisterMod();
    const until = await reg.claimsOpenUntil(settings.registry);
    $("takeback").hidden = until === null;
    if (until !== null) $("takeback-until").textContent = new Date(until).toLocaleDateString();
  } catch {
    $("takeback").hidden = true;
  }
}

$("takeback-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  const name = normalizeName($("takeback-name").value);
  if (!name) return;
  $("takeback-go").disabled = true;
  try {
    const { key } = await getUnlockedWallet();
    await loadRegisterMod();
    const { takeBack } = await import("./claims.js");
    await takeBack({
      privateKey: key, program: settings.registry, name, onStep: (stage) => {
        setStatus("takeback-status", { list: "Reading the list of names…", claim: "Taking the name back…", page: "Restoring its page…" }[stage] ?? "");
      },
    });
    setOnboarded(name);
    setStatus("takeback-status", `✓ ${name} is yours again.`, "ok");
  } catch (error) {
    setStatus("takeback-status", error?.message || String(error), "bad");
  } finally {
    $("takeback-go").disabled = false;
  }
});

$("name-back").addEventListener("click", () => show("wallet"));
$("name-continue").addEventListener("click", () => {
  if (!onboarded()) setOnboarded($("pick-name").value ? normalizeName($("pick-name").value) : null);
  show("chat");
});
$("redo-setup").addEventListener("click", () => show("wallet"));

// ---------- MARKET ----------

async function executeBuyName(name, statusId, button) {
  button.disabled = true;
  setStatus(statusId, `Buying ${name}…`);
  try {
    const { key } = await getUnlockedWallet();
    const reg = await loadRegisterMod();
    const res = await reg.buyListedName({ privateKey: key, program: settings.registry, name });
    setOnboarded(name);
    setStatus(
      statusId,
      `✓ Bought ${name} for ${fmtTokens(res.price)} tokens (seller got ${fmtTokens(res.split.seller)}, 2.5% fee ${fmtTokens(res.split.fee)}).`,
      "ok",
    );
  } catch (error) {
    setStatus(statusId, error?.message || String(error), "bad");
  } finally {
    button.disabled = false;
  }
}

function buildListingCard(item) {
  const wrap = document.createElement("div");
  wrap.className = "dir-card";

  const title = document.createElement("div");
  title.className = "dir-head";
  const strong = document.createElement("strong");
  strong.textContent = item.name;
  const price = document.createElement("span");
  price.className = "mono";
  price.textContent = `${fmtTokens(item.price)} tokens`;
  title.append(strong, price);

  const meta = document.createElement("p");
  meta.className = "muted small-line";
  const expText = item.expiresAt ? `Listing expires ${fmtDate(item.expiresAt)}` : "No listing expiry";
  meta.textContent =
    `Seller receives ${fmtTokens(item.sellerPayout)} (97.5%) · Treasury fee ${fmtTokens(item.fee)} (2.5%) · ${expText} · Name: ${fmtDate(item.nameExpires)}`;

  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "small";
  btn.textContent = `Buy ${item.name}`;
  btn.addEventListener("click", () => executeBuyName(item.name, "market-status", btn));

  wrap.append(title, meta, btn);
  return wrap;
}

async function openMarket() {
  $("market-list").replaceChildren();
  $("market-empty").hidden = true;
  setStatus("market-status", "Loading listings from the chain…");
  try {
    marketMod ??= await import("./market.js");
    const listings = await marketMod.loadMarketListings({
      rpc: settings.rpc,
      registry: settings.registry,
      onProgress: (p) => {
        if (p.stage === "verify") setStatus("market-status", `Verifying ${p.done + 1} of ${p.total}…`);
      },
    });
    if (listings.length === 0) {
      setStatus("market-status", "");
      $("market-empty").hidden = false;
      return;
    }
    setStatus("market-status", `${listings.length} listed name${listings.length === 1 ? "" : "s"}.`, "ok");
    for (const item of listings) {
      const li = document.createElement("li");
      li.append(buildListingCard(item));
      $("market-list").append(li);
    }
  } catch (error) {
    setStatus("market-status", (await explainFailure(error, settings.rpc)).message, "bad");
  }
}

$("market-refresh").addEventListener("click", openMarket);

$("market-lookup-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  const name = normalizeName($("market-lookup-name").value);
  const direct = $("market-direct");
  direct.hidden = true;
  direct.replaceChildren();
  if (!isValidLabel(name)) {
    setStatus("market-status", "Enter a valid top-level name.", "bad");
    return;
  }
  setStatus("market-status", `Checking listing for ${name}…`);
  try {
    marketMod ??= await import("./market.js");
    const item = await marketMod.lookupListing({ rpc: settings.rpc, registry: settings.registry, name });
    if (!item) {
      setStatus("market-status", `${name} is not currently listed for sale.`);
      return;
    }
    setStatus("market-status", "");
    direct.append(buildListingCard(item));
    direct.hidden = false;
  } catch (error) {
    setStatus("market-status", (await explainFailure(error, settings.rpc)).message, "bad");
  }
});

// ---------- SUBNAMES ----------

let subBuying = false;

function openSubnames() {
  const pend = readJson(SUBNAME_BUY_PENDING);
  if (pend && splitSubname(pend.name)) {
    $("sub-buy-name").value = pend.name;
    $("sub-buy-go").hidden = false;
    runBuySubname(pend.name);
  }
}

$("sub-create-go").addEventListener("click", async () => {
  const fullName = normalizeName($("sub-create-name").value);
  const ownerAccount = $("sub-create-owner").value.trim() || null;
  const revocable = $("sub-create-revocable").checked;
  if (!splitSubname(fullName)) {
    setStatus("sub-create-status", "Enter a one-level subname like blog.yourname (no --, max 62 chars).", "bad");
    return;
  }
  $("sub-create-go").disabled = true;
  setStatus("sub-create-status", `Creating ${fullName}…`);
  try {
    const { key } = await getUnlockedWallet();
    const reg = await loadRegisterMod();
    await reg.createSubname({ privateKey: key, program: settings.registry, fullName, ownerAccount, revocable });
    setStatus("sub-create-status", `✓ Created ${fullName} (${revocable ? "revocable" : "permanent"}).`, "ok");
  } catch (error) {
    setStatus("sub-create-status", error?.message || String(error), "bad");
  } finally {
    $("sub-create-go").disabled = false;
  }
});

$("sub-revoke-go").addEventListener("click", async () => {
  const fullName = normalizeName($("sub-create-name").value);
  if (!splitSubname(fullName)) {
    setStatus("sub-create-status", "Enter the subname to revoke (like blog.yourname).", "bad");
    return;
  }
  $("sub-revoke-go").disabled = true;
  setStatus("sub-create-status", `Revoking ${fullName}…`);
  try {
    const { key } = await getUnlockedWallet();
    const reg = await loadRegisterMod();
    await reg.revokeSubname({ privateKey: key, program: settings.registry, fullName });
    setStatus("sub-create-status", `✓ Revoked ${fullName}.`, "ok");
  } catch (error) {
    setStatus("sub-create-status", error?.message || String(error), "bad");
  } finally {
    $("sub-revoke-go").disabled = false;
  }
});

async function updateSubnameSale(enabled) {
  const parentName = normalizeName($("sub-sale-parent").value);
  if (!isValidLabel(parentName)) {
    setStatus("sub-sale-status", "Enter a valid top-level name.", "bad");
    return;
  }
  const rawPrice = $("sub-sale-price").value.trim().replace(/[_,]/g, "") || "0";
  if (enabled && !/^\d+$/.test(rawPrice)) {
    setStatus("sub-sale-status", "Price must be a whole number (0 or more).", "bad");
    return;
  }
  const payout = $("sub-sale-payout").value.trim() || null;
  $("sub-sale-on").disabled = true;
  $("sub-sale-off").disabled = true;
  setStatus("sub-sale-status", enabled ? "Turning subname sales on…" : "Turning subname sales off…");
  try {
    const { key } = await getUnlockedWallet();
    const reg = await loadRegisterMod();
    await reg.setSubnameSale({
      privateKey: key, program: settings.registry, parentName, enabled,
      price: enabled ? BigInt(rawPrice) : 0n, payout,
    });
    setStatus(
      "sub-sale-status",
      enabled ? `✓ Subname sales on for ${parentName} at ${fmtTokens(BigInt(rawPrice))} tokens.` : `✓ Subname sales turned off for ${parentName}.`,
      "ok",
    );
  } catch (error) {
    setStatus("sub-sale-status", error?.message || String(error), "bad");
  } finally {
    $("sub-sale-on").disabled = false;
    $("sub-sale-off").disabled = false;
  }
}

$("sub-sale-on").addEventListener("click", () => updateSubnameSale(true));
$("sub-sale-off").addEventListener("click", () => updateSubnameSale(false));

$("sub-buy-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  const fullName = normalizeName($("sub-buy-name").value);
  const split = splitSubname(fullName);
  $("sub-buy-go").hidden = true;
  setStatus("sub-buy-status", "");
  if (!split) {
    setStatus("sub-buy-info", "Enter a one-level subname like blog.alice (no --, max 62 chars).", "bad");
    return;
  }
  setStatus("sub-buy-info", `Checking ${fullName}…`);
  try {
    const existing = await resolveName(settings.rpc, settings.registry, fullName);
    if (existing !== null) {
      setStatus("sub-buy-info", `${fullName} is already taken.`, "bad");
      return;
    }
    const { id: parentBaseId } = await deriveName(split.parent);
    const parentSlotId = await tieredId(parentBaseId, priceTierForLen(split.parent.length));
    const parentBytes = await readActorState(settings.rpc, settings.registry, await recordAddress(settings.registry, parentSlotId));
    if (parentBytes.length === 0) {
      setStatus("sub-buy-info", `${split.parent} isn't registered.`, "bad");
      return;
    }
    const parent = decodeNameRecord(parentBytes);
    if (!parent.subnameSale) {
      setStatus("sub-buy-info", `${split.parent} is not selling subnames.`, "bad");
      return;
    }
    setStatus("sub-buy-info", `✓ ${fullName} is available (permanent) for ${fmtTokens(parent.subnameSale.price)} tokens + gas.`, "ok");
    $("sub-buy-go").hidden = false;
  } catch (error) {
    setStatus("sub-buy-info", (await explainFailure(error, settings.rpc)).message, "bad");
  }
});

async function runBuySubname(fullName) {
  if (subBuying) return;
  subBuying = true;
  $("sub-buy-go").disabled = true;
  try {
    const { key, account } = await getUnlockedWallet();
    const reg = await loadRegisterMod();
    const pend = readJson(SUBNAME_BUY_PENDING);
    const resume = pend && pend.name === fullName && pend.owner === account
      ? { secretHex: pend.secretHex, made: pend.made, public: true }
      : null;
    await reg.buySubname({
      privateKey: key, program: settings.registry, fullName, resume,
      onStep: (stage, detail) => {
        if (stage === "commit") setStatus("sub-buy-status", "Committing to the subname…");
        else if (stage === "committed") {
          saveJson(SUBNAME_BUY_PENDING, { name: fullName, owner: account, secretHex: detail.secretHex, made: detail.made });
          setStatus("sub-buy-status", "Committed. Waiting ~5 minutes before completing purchase… (safe to leave; it resumes)");
        } else if (stage === "waiting") {
          const m = Math.max(1, Math.ceil(detail.remainingMs / 60000));
          setStatus("sub-buy-status", `Waiting ~${m} min before buying…`);
        } else if (stage === "buy") setStatus("sub-buy-status", "Completing subname purchase…");
        else if (stage === "done") setStatus("sub-buy-status", `✓ ${fullName} is yours (permanent).`, "ok");
      },
    });
    clearKey(SUBNAME_BUY_PENDING);
  } catch (error) {
    if (error?.code === "expired") clearKey(SUBNAME_BUY_PENDING);
    setStatus("sub-buy-status", error?.message || String(error), "bad");
  } finally {
    $("sub-buy-go").disabled = false;
    subBuying = false;
  }
}

$("sub-buy-go").addEventListener("click", () => {
  const fullName = normalizeName($("sub-buy-name").value);
  if (splitSubname(fullName)) runBuySubname(fullName);
});

// ---------- BROWSE LNS SITES ----------

let allSites = [];
let sitesPage = 0;

function renderSitesList() {
  const q = $("sites-filter").value.trim().toLowerCase();
  const filtered = q
    ? allSites.filter((s) => s.name.includes(q) || s.title.toLowerCase().includes(q))
    : allSites;
  const totalPages = Math.max(1, Math.ceil(filtered.length / SITES_PAGE_SIZE));
  sitesPage = Math.min(sitesPage, totalPages - 1);
  const start = sitesPage * SITES_PAGE_SIZE;
  const slice = filtered.slice(start, start + SITES_PAGE_SIZE);

  const ul = $("sites-list");
  ul.replaceChildren();
  $("sites-empty").hidden = filtered.length > 0;
  $("sites-pager").hidden = filtered.length <= SITES_PAGE_SIZE;
  $("sites-prev").disabled = sitesPage <= 0;
  $("sites-next").disabled = sitesPage >= totalPages - 1;

  for (const site of slice) {
    const li = document.createElement("li");
    const card = document.createElement("div");
    card.className = "dir-card";

    const head = document.createElement("div");
    head.className = "dir-head";
    const link = document.createElement("a");
    link.href = site.gatewayUrl;
    link.rel = "noopener noreferrer";
    link.target = "_blank";
    link.textContent = site.title;
    const proto = document.createElement("code");
    proto.className = "muted";
    proto.textContent = site.lnsUrl;
    head.append(link, proto);

    const meta = document.createElement("p");
    meta.className = "muted small-line";
    meta.textContent = `Updated ${fmtDate(site.updatedAt)} · ${site.gatewayUrl}`;

    card.append(head, meta);
    li.append(card);
    ul.append(li);
  }
}

async function openSites() {
  $("sites-list").replaceChildren();
  $("sites-empty").hidden = true;
  $("sites-pager").hidden = true;
  setStatus("sites-status", "Reading the on-chain sites directory…");
  try {
    sitesMod ??= await import("./sites.js");
    const { sites } = await sitesMod.loadSitesDirectory({
      rpc: settings.rpc,
      registry: settings.registry,
      onProgress: (p) => {
        if (p.stage === "fetch") setStatus("sites-status", `Reading ${p.buckets} directory pages from chain…`);
        else if (p.stage === "verify") setStatus("sites-status", `Verifying ${p.done + 1} of ${p.total} (${p.name})…`);
      },
    });
    allSites = sites;
    sitesPage = 0;
    setStatus("sites-status", sites.length ? `${sites.length} verified LNS site${sites.length === 1 ? "" : "s"}.` : "", sites.length ? "ok" : "");
    renderSitesList();
  } catch (error) {
    setStatus("sites-status", (await explainFailure(error, settings.rpc)).message, "bad");
  }
}

$("sites-refresh").addEventListener("click", openSites);
$("sites-filter").addEventListener("input", () => { sitesPage = 0; renderSitesList(); });
$("sites-prev").addEventListener("click", () => { if (sitesPage > 0) { sitesPage -= 1; renderSitesList(); } });
$("sites-next").addEventListener("click", () => { sitesPage += 1; renderSitesList(); });

// ---------- utilities & settings drawer ----------

async function copy(text, button) {
  const label = button.textContent;
  try {
    await navigator.clipboard.writeText(text);
    button.textContent = "Copied";
  } catch {
    button.textContent = "Couldn't copy; select the text instead";
  }
  setTimeout(() => (button.textContent = label), 1600);
}

function openDrawer(open) {
  $("privacy").hidden = !open;
  if (open) fillSettings();
}
$("privacy-open").addEventListener("click", () => openDrawer(true));
$("privacy-close").addEventListener("click", () => openDrawer(false));

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
  openDrawer(false);
  show(step || "wallet");
});
$("settings-reset").addEventListener("click", () => {
  settings = { ...DEFAULTS };
  clearKey(STORE_KEY);
  fillSettings();
});

// ---------- start ----------

const rawHash = normalizeName(decodeURIComponent(location.hash.slice(1)));
if (readJson(REGISTER_PENDING)) show("name");
else if (readJson(SUBNAME_BUY_PENDING)) show("subnames");
else if (rawHash && VIEWS[rawHash]) show(rawHash);
else if (rawHash && isValidName(rawHash)) show("chat");
else show("wallet");
