// In-browser PUBLIC name operations for the LNS v8 registry on LEZ v0.4.0.
// Public path only: private/ZK operations stay in Basecamp. Transaction builders live in lns-tx.js.
//
// Registration & subname purchase use two steps: CommitPublic (variant 10), wait
// PUBLIC_MIN_COMMIT_AGE_MS (5 min), then Register / BuySubname. Both CommitPublic and its
// reveal use PUBLIC_TIME_WINDOW_MS (1 min), so the chain clock is read right before signing
// (after Argon2id derivation and shortlist shard proof construction) and never falls back to a local clock.
//
// LengthProof: NonMembership over the depth-21 shortlist Merkle tree, proved from a single
// fetched prefix shard (`shortlist-shards.js`) verified against `SHORTLIST_ROOT`.

import {
  GRACE_MS,
  PRICE_TIER_5,
  PRICE_TIER_6_7,
  PRICE_TIER_8_PLUS,
  TERM_30D,
  TERM_1Y,
  TERM_5Y,
  TERM_PERMANENT,
  decodeNameRecord,
  decodeProfile,
  decodeSettings,
  decodeSubnameRecord,
  deriveName,
  encodeProfile,
  isValidLabel,
  isValidName,
  openSealed,
  priceTierForLen,
  readActorState,
  seal,
  splitSubname,
} from "./names.js";
import {
  feeReserve,
  formatAccountId,
  fromHex,
  getAccountBalance,
  getAccountsNonces,
  getFeeState,
  publicAccountId,
  sendTransaction,
  signPublicTransaction,
  systemProgramId,
  toHex,
  waitForTransaction,
} from "./lez-tx.js";
import {
  DEFAULT_PREMIUM_START,
  DEFAULT_PRICES,
  MIN_SALE_PRICE,
  SITE_DIR_KEEP,
  accountBytes,
  buyMessage,
  buySubnameMessage,
  cancelMessage,
  commitMessage,
  commitPublicMessage,
  commitmentAddress,
  commitmentHash,
  createSubnameMessage,
  decodeCommitment,
  listMessage,
  ownerTag,
  recordAddress,
  registerMessage,
  renewMessage,
  rereleasePremium,
  revokeSubnameMessage,
  saleSplit,
  setRecordMessage,
  setSubnameRecordMessage,
  setSubnameSaleMessage,
  settingsAddress,
  subnameCommitmentHash,
  subnameRecordAddress,
  tieredId,
} from "./lns-tx.js";
import {
  SHORTLIST_ROOT,
  lengthProofFromShards,
} from "./shortlist-shards.js";

export { SHORTLIST_ROOT, TERM_30D, TERM_1Y, TERM_5Y, TERM_PERMANENT, priceTierForLen, rereleasePremium, saleSplit };

// Gas limits: browser cannot run r0vm, so these sit above measured LEZ v0.4.0 usage
// (kept tight because a reverted public transaction charges its entire gas limit).
const REGISTER_GAS = 350000n;
const COMMIT_GAS = 60000n;
const RENEW_GAS = 200000n;
const SET_RECORD_GAS = 100000n;
const LIST_GAS = 180000n;
const CANCEL_GAS = 150000n;
const BUY_GAS = 350000n;
const SUBNAME_GAS = 250000n;
const BUY_SUBNAME_GAS = 350000n;

// Transaction wire sizes for balance pre-checks.
const REGISTER_BYTES = 3400n;
const COMMIT_BYTES = 660n;
const ACTION_BYTES = 900n;

const te = new TextEncoder();
const utf8 = (s) => te.encode(s);

export const MIN_COMMIT_AGE_MS = 15 * 60 * 1000;
export const PUBLIC_MIN_COMMIT_AGE_MS = 5 * 60 * 1000;
export const MAX_COMMIT_AGE_MS = 24 * 60 * 60 * 1000;
export const TIME_WINDOW_MS = 10 * 60 * 1000;
export const PUBLIC_TIME_WINDOW_MS = 60 * 1000;
export const DEPOSIT = 1_000_000n; // fallback deposit (deposit mode)
export const MIN_NAME_LENGTH = 5;

export const TERM_LABELS = ["30 days", "1 year", "5 years", "Permanent"];
export const TIER_LABELS = ["5 characters", "6–7 characters", "8+ characters"];

export function formatTokens(amount) {
  return BigInt(amount).toLocaleString("en-US");
}

// ---------- shortlist proof via static prefix shards ----------

let _shardsBaseUrl = new URL("./shortlist-shards/", import.meta.url).href;
export function setShortlistShardsBase(url) { _shardsBaseUrl = url; }
export function shortlistShardsBase() { return _shardsBaseUrl; }

/** NonMembership LengthProof for a 5+ char top-level name, fetched from one verified shard. */
export async function lengthProof(id32, options = {}) {
  return lengthProofFromShards(id32, { baseUrl: _shardsBaseUrl, ...options });
}

// ---------- RPC & chain clock ----------

let _rpc = "https://lez.buggers.online/lez";
export function setRpc(url) { _rpc = url; }
export function defaultRpc() { return _rpc; }

export const CLOCK_ACCOUNT = formatAccountId(utf8("/LEZ/ClockProgramAccount/0000000"));
export const CLOCK_PROGRAM = formatAccountId(systemProgramId("clock"));

/**
 * The chain's time in milliseconds. Fails closed if the clock cannot be read so a transaction
 * never uses an out-of-window local clock and burns gas.
 */
export async function chainNow(rpc = defaultRpc()) {
  const state = await readActorState(rpc, CLOCK_PROGRAM, CLOCK_ACCOUNT);
  if (state.length < 16) {
    throw new Error("Couldn't read the chain's clock. Nothing was sent.");
  }
  const timestamp = new DataView(state.buffer, state.byteOffset, state.byteLength).getBigUint64(8, true);
  if (timestamp === 0n) {
    throw new Error("The chain's clock isn't set yet. Nothing was sent.");
  }
  return Number(timestamp);
}

const reverted = (what, tx) => Object.assign(
  new Error(`${what} was included in a block but failed, so nothing changed. Its fee was charged. (${tx})`),
  { code: "reverted" },
);

// ---------- settings & quoting ----------

const ZERO_ACCOUNT_HEX = "00".repeat(32);

function optionalNftProgram(settings) {
  if (!settings?.nftProgram) return null;
  const bytes = accountBytes(settings.nftProgram);
  return toHex(bytes) === ZERO_ACCOUNT_HEX ? null : bytes;
}

function normalizePriceTable(settings) {
  if (settings && Array.isArray(settings.prices) && Array.isArray(settings.prices[0])) {
    return settings.prices;
  }
  return DEFAULT_PRICES;
}

/** Reads and decodes the registry's on-chain `Settings`, or returns null if unconfigured. */
export async function readRegistrySettings(program, rpc = defaultRpc()) {
  const account = formatAccountId(settingsAddress(accountBytes(program)));
  const bytes = await readActorState(rpc, program, account);
  if (bytes.length === 0) return null;
  return decodeSettings(bytes);
}

/**
 * Quotes registering a top-level `name` for `term` (`0` = 30d, `1` = 1y, `2` = 5y, `3` = permanent)
 * directly from the on-chain `Settings` and current `NameRecord` (including any 21-day falling
 * re-release premium if the name lapsed past its 30-day grace period).
 */
export async function quoteRegistration({ program, name, term = TERM_1Y, now = null, rpc = defaultRpc() }) {
  if (!isValidLabel(name)) throw new Error("Names are lowercase letters, digits or single dashes.");
  if (name.length < MIN_NAME_LENGTH) {
    throw Object.assign(new Error(`Names must be at least ${MIN_NAME_LENGTH} characters.`), { code: "too_short" });
  }
  const prog = accountBytes(program);
  const priceTier = priceTierForLen(name.length);
  const { id: baseId, key } = await deriveName(name);
  const slotId = tieredId(baseId, priceTier);
  const [settings, at, recBytes] = await Promise.all([
    readRegistrySettings(program, rpc),
    now !== null && now !== undefined ? Promise.resolve(now) : chainNow(rpc),
    readActorState(rpc, program, formatAccountId(recordAddress(prog, slotId))),
  ]);
  const record = recBytes.length > 0 ? decodeNameRecord(recBytes) : null;
  const active = record !== null && at < record.expires + GRACE_MS;
  const prices = normalizePriceTable(settings);
  const deposit = settings ? settings.mode === 1 : false;
  let basePrice = 0n;
  let premium = 0n;
  let treasury = null;
  if (deposit) {
    basePrice = settings?.deposits?.[0] ?? DEPOSIT;
  } else {
    basePrice = prices[priceTier][term];
    if (record !== null && !active && record.expires < Number.MAX_SAFE_INTEGER) {
      const elapsed = BigInt(at - (record.expires + GRACE_MS));
      const start = settings?.premiumStart ?? DEFAULT_PREMIUM_START;
      premium = rereleasePremium(start, elapsed);
    }
    treasury = settings?.treasury ? accountBytes(settings.treasury) : null;
  }
  return {
    name,
    available: !active,
    deposit,
    priceTier,
    term,
    prices,
    basePrice,
    premium,
    total: basePrice + premium,
    treasury,
    treasuryAddress: settings?.treasury ?? null,
    settings,
    record,
    baseId,
    slotId,
    key,
    now: at,
  };
}

// ---------- building signed registration txs ----------

export async function buildCommit({ privateKey, program, name, secret, now = null, publicCommit = true, gasLimit = COMMIT_GAS }) {
  const rpc = defaultRpc();
  const prog = accountBytes(program);
  const { id: baseId } = await deriveName(name);
  const priceTier = priceTierForLen(name.length);
  const slotId = tieredId(baseId, priceTier);
  const signer = publicAccountId(privateKey);
  const holder = ownerTag(slotId, signer);
  const commitment = commitmentHash(slotId, holder, secret);
  const [nonces, claimedNow] = await Promise.all([
    getAccountsNonces(rpc, [signer]),
    now !== null && now !== undefined ? Promise.resolve(now) : chainNow(rpc),
  ]);
  const [nonce] = nonces;
  const build = publicCommit ? commitPublicMessage : commitMessage;
  const message = build({ program: prog, signer, nonce, gasLimit, now: claimedNow, commitment });
  return { signed: signPublicTransaction(message, [privateKey]), id: baseId, slotId, holder, promised: commitment, now: claimedNow, public: publicCommit };
}

export async function buildRegister({
  privateKey, program, name, secret, now = null, term = TERM_1Y,
  deposit = null, fee = null, treasury = null, gasLimit = REGISTER_GAS, proof: given = null,
}) {
  if (!name || name.length < MIN_NAME_LENGTH) {
    throw Object.assign(new Error(`Names must be at least ${MIN_NAME_LENGTH} characters.`), { code: "too_short" });
  }
  const rpc = defaultRpc();
  const prog = accountBytes(program);
  const { id: baseId, key } = await deriveName(name);
  const priceTier = priceTierForLen(name.length);
  const slotId = tieredId(baseId, priceTier);
  const signer = publicAccountId(privateKey);
  const profile = encodeProfile({ name, account: formatAccountId(signer), npk: null, vpk: null, texts: {} });
  const sealed = await seal(key, profile, "record");
  // Slow work (Argon2id + shard fetch/proof) finishes BEFORE reading chain clock & nonce.
  const proof = given ?? await lengthProof(baseId);
  const [settings, prevBytes, nonces, claimedNow] = await Promise.all([
    readRegistrySettings(program, rpc),
    readActorState(rpc, program, formatAccountId(recordAddress(prog, slotId))),
    getAccountsNonces(rpc, [signer]),
    now !== null && now !== undefined ? Promise.resolve(now) : chainNow(rpc),
  ]);
  const [nonce] = nonces;
  const prevRecord = prevBytes.length > 0 ? decodeNameRecord(prevBytes) : null;
  const useDeposit = deposit !== null ? deposit : (settings ? settings.mode === 1 : false);
  let payFee = fee;
  let payTreasury = treasury ? accountBytes(treasury) : null;
  if (payFee === null) {
    if (useDeposit) {
      payFee = settings?.deposits?.[0] ?? DEPOSIT;
      payTreasury = null;
    } else {
      const prices = normalizePriceTable(settings);
      let owed = prices[priceTier][term];
      if (prevRecord && prevRecord.expires < Number.MAX_SAFE_INTEGER && claimedNow >= prevRecord.expires + GRACE_MS) {
        const elapsed = BigInt(claimedNow - (prevRecord.expires + GRACE_MS));
        owed += rereleasePremium(settings?.premiumStart ?? DEFAULT_PREMIUM_START, elapsed);
      }
      payFee = owed;
      payTreasury = payTreasury ?? (settings?.treasury ? accountBytes(settings.treasury) : null);
    }
  }
  const nftProgram = optionalNftProgram(settings);
  const prevNftTokenId = prevRecord?.nftTokenId ? fromHex(prevRecord.nftTokenId) : null;
  const message = registerMessage({
    program: prog,
    signer,
    nonce,
    gasLimit,
    id: baseId,
    priceTier,
    secret,
    sealed,
    term,
    fee: payFee,
    treasury: useDeposit ? null : payTreasury,
    now: claimedNow,
    deposit: useDeposit,
    lengthProof: proof,
    nftProgram,
    prevNftTokenId,
    prevSitePage: prevRecord?.siteListed ?? null,
    prevInMarket: Boolean(prevRecord?.listing?.inMarket),
  });
  return { signed: signPublicTransaction(message, [privateKey]), id: baseId, slotId, now: claimedNow, fee: payFee, deposit: useDeposit };
}

export async function confirmRegistered(rpc, program, slotId, signer, tx) {
  const state = await readActorState(rpc, program, formatAccountId(recordAddress(accountBytes(program), slotId)));
  if (state.length === 0 || decodeNameRecord(state).ownerTag !== toHex(ownerTag(slotId, signer))) {
    throw reverted("The registration", tx);
  }
}

export async function confirmCommitted(rpc, program, promised, tx, { public: expectPublic = null } = {}) {
  const state = await readActorState(rpc, program, formatAccountId(commitmentAddress(accountBytes(program), promised)));
  if (state.length === 0) throw reverted("The commitment", tx);
  let commitment;
  try {
    commitment = decodeCommitment(state);
  } catch {
    throw reverted("The commitment", tx);
  }
  if (expectPublic !== null && commitment.public !== expectPublic) throw reverted("The commitment", tx);
  return commitment;
}

export async function estimateNeed(privateKey, { amount = DEPOSIT, resume = false, gasLimit = REGISTER_GAS, dataBytes = REGISTER_BYTES } = {}) {
  const rpc = defaultRpc();
  const quote = await getFeeState(rpc);
  const mainReserve = feeReserve({ gasLimit, dataBytes, quote });
  const commitReserve = resume ? 0n : feeReserve({ gasLimit: COMMIT_GAS, dataBytes: COMMIT_BYTES, quote });
  const need = BigInt(amount) + mainReserve + commitReserve;
  const have = await getAccountBalance(rpc, publicAccountId(privateKey));
  return { need, have, amount: BigInt(amount), enough: have >= need };
}

export async function claimsOpenUntil(program, now = null) {
  const settings = await readRegistrySettings(program, defaultRpc());
  if (!settings || !settings.claimsRoot || /^0+$/.test(settings.claimsRoot)) return null;
  return settings.claimsUntil > (now ?? await chainNow(defaultRpc())) ? settings.claimsUntil : null;
}

// ---------- full public registration (CommitPublic -> 5-min wait -> Register) ----------

export async function registerPublic({
  privateKey,
  program = "38jEfVV1RCyMzu1FGbYcCn5FqxCxKyY58VgzjruAh2gq",
  name,
  term = TERM_1Y,
  resume = null,
  onStep = () => {},
  now = null,
  acceptClaimRisk = async () => true,
}) {
  const rpc = defaultRpc();
  const clock = async () => now ?? await chainNow(rpc);
  const quote = await quoteRegistration({ program, name, term, now: await clock(), rpc });
  if (!quote.available) {
    throw Object.assign(new Error(`${name} is already registered.`), { code: "taken" });
  }
  const { need, have, enough } = await estimateNeed(privateKey, { amount: quote.total, resume: Boolean(resume) });
  if (!enough) {
    const what = quote.deposit ? "a refundable deposit" : "the registration fee";
    const e = new Error(`Not enough tokens: need about ${formatTokens(need)} (${formatTokens(quote.total)} for ${what} plus gas), you have ${formatTokens(have)}.`);
    e.code = "insufficient"; e.need = need; e.have = have;
    throw e;
  }
  if (!resume) {
    const until = await claimsOpenUntil(program, await clock());
    if (until !== null && !(await acceptClaimRisk(until))) {
      throw Object.assign(new Error("Registration was cancelled."), { code: "cancelled" });
    }
  }
  let secret, made, isPublic;
  if (resume) {
    secret = fromHex(resume.secretHex);
    made = resume.made;
    isPublic = Boolean(resume.public);
  } else {
    secret = crypto.getRandomValues(new Uint8Array(32));
    isPublic = true;
    onStep("commit", "committing to the name");
    const { signed, promised, now: committedAt } = await buildCommit({ privateKey, program, name, secret, now, publicCommit: true });
    made = committedAt;
    const h = await sendTransaction(rpc, signed);
    await waitForTransaction(rpc, signed.hash);
    await confirmCommitted(rpc, program, promised, signed.hash, { public: true });
    onStep("committed", { secretHex: toHex(secret), made, public: true, term, tx: h });
  }
  const minAge = isPublic ? PUBLIC_MIN_COMMIT_AGE_MS : MIN_COMMIT_AGE_MS;
  const ready = made + minAge;
  for (let at = await clock(); at < ready; at = await clock()) {
    if (at >= made + MAX_COMMIT_AGE_MS) {
      throw Object.assign(new Error("The commitment expired (older than 24 hours). Start again."), { code: "expired" });
    }
    onStep("waiting", { remainingMs: ready - at });
    await new Promise((r) => setTimeout(r, Math.min(30000, Math.max(5000, ready - at))));
  }
  onStep("register", "registering the name");
  const { signed, slotId, now: registeredAt } = await buildRegister({
    privateKey, program, name, secret, now, term, deposit: quote.deposit,
  });
  if (registeredAt >= made + MAX_COMMIT_AGE_MS) {
    throw Object.assign(new Error("The commitment expired (older than 24 hours). Start again."), { code: "expired" });
  }
  const h = await sendTransaction(rpc, signed);
  const block = await waitForTransaction(rpc, signed.hash);
  await confirmRegistered(rpc, program, slotId, publicAccountId(privateKey), signed.hash);
  onStep("done", { tx: h, block });
  return { name, id: toHex(slotId), txHash: signed.hash, block };
}

// ---------- renew a top-level name ----------

export async function renewName({ privateKey, program, name, term = TERM_1Y, now = null, gasLimit = RENEW_GAS }) {
  if (!isValidLabel(name)) throw new Error("Only top-level names can be renewed.");
  const rpc = defaultRpc();
  const prog = accountBytes(program);
  const { id: baseId } = await deriveName(name);
  const priceTier = priceTierForLen(name.length);
  const slotId = tieredId(baseId, priceTier);
  const signer = publicAccountId(privateKey);
  const [settings, recBytes] = await Promise.all([
    readRegistrySettings(program, rpc),
    readActorState(rpc, program, formatAccountId(recordAddress(prog, slotId))),
  ]);
  if (recBytes.length === 0) throw new Error(`${name} isn't registered.`);
  const rec = decodeNameRecord(recBytes);
  if (rec.expires >= Number.MAX_SAFE_INTEGER) throw new Error(`${name} is already permanent.`);
  const prices = normalizePriceTable(settings);
  const tier = rec.priceTier ?? priceTier;
  const fee = settings?.mode === 1 ? 0n : prices[tier][term];
  const treasury = fee > 0n && settings?.treasury ? accountBytes(settings.treasury) : null;
  const { need, have, enough } = await estimateNeed(privateKey, { amount: fee, resume: true, gasLimit, dataBytes: ACTION_BYTES });
  if (!enough) throw Object.assign(new Error(`Not enough tokens: need about ${formatTokens(need)}, you have ${formatTokens(have)}.`), { code: "insufficient" });
  const [[nonce], claimedNow] = await Promise.all([
    getAccountsNonces(rpc, [signer]),
    now !== null && now !== undefined ? Promise.resolve(now) : chainNow(rpc),
  ]);
  if (claimedNow >= rec.expires + GRACE_MS) throw new Error(`${name} is past its grace period and can't be renewed.`);
  const signed = signPublicTransaction(
    renewMessage({
      program: prog, signer, nonce, gasLimit, id: slotId, term, fee, treasury, now: claimedNow,
      nftProgram: optionalNftProgram(settings),
      nftTokenId: rec.nftTokenId ? fromHex(rec.nftTokenId) : null,
      sitePage: rec.siteListed ?? null,
      inMarket: Boolean(rec.listing?.inMarket),
    }),
    [privateKey],
  );
  await sendTransaction(rpc, signed);
  const block = await waitForTransaction(rpc, signed.hash);
  const afterBytes = await readActorState(rpc, program, formatAccountId(recordAddress(prog, slotId)));
  const after = afterBytes.length > 0 ? decodeNameRecord(afterBytes) : null;
  if (!after || after.expires <= rec.expires) throw reverted("The renewal", signed.hash);
  return { name, txHash: signed.hash, block, expires: after.expires >= Number.MAX_SAFE_INTEGER ? null : after.expires };
}

// ---------- secondary marketplace: list, cancel, buy ----------

export async function listName({
  privateKey, program, name, price, payTo = null, expiresAt = null, unlisted = false, now = null, gasLimit = LIST_GAS,
}) {
  if (!isValidLabel(name)) throw new Error("Only top-level names can be listed for sale.");
  const salePrice = BigInt(price);
  if (salePrice < MIN_SALE_PRICE) throw new Error(`Price must be at least ${MIN_SALE_PRICE} tokens.`);
  const rpc = defaultRpc();
  const prog = accountBytes(program);
  const { id: baseId } = await deriveName(name);
  const slotId = tieredId(baseId, priceTierForLen(name.length));
  const signer = publicAccountId(privateKey);
  const recBytes = await readActorState(rpc, program, formatAccountId(recordAddress(prog, slotId)));
  if (recBytes.length === 0) throw new Error(`${name} isn't registered.`);
  const rec = decodeNameRecord(recBytes);
  if (rec.ownerTag !== toHex(ownerTag(slotId, signer))) throw new Error(`This wallet doesn't hold ${name}.`);
  if (rec.nftShielded) throw new Error(`${name} is shielded — unshield it before listing.`);
  const payoutBytes = payTo ? accountBytes(payTo) : signer;
  const [[nonce], claimedNow] = await Promise.all([
    getAccountsNonces(rpc, [signer]),
    now !== null && now !== undefined ? Promise.resolve(now) : chainNow(rpc),
  ]);
  const signed = signPublicTransaction(
    listMessage({
      program: prog, signer, nonce, gasLimit, id: slotId, price: salePrice, payTo: payoutBytes,
      expiresAt, now: claimedNow, displayName: unlisted ? null : name,
      prevInMarket: Boolean(rec.listing?.inMarket),
    }),
    [privateKey],
  );
  await sendTransaction(rpc, signed);
  const block = await waitForTransaction(rpc, signed.hash);
  const after = decodeNameRecord(await readActorState(rpc, program, formatAccountId(recordAddress(prog, slotId))));
  if (!after.listing || after.listing.price !== salePrice) throw reverted("The listing", signed.hash);
  return { name, txHash: signed.hash, block, listing: after.listing, split: saleSplit(salePrice) };
}

export async function cancelListing({ privateKey, program, name, gasLimit = CANCEL_GAS }) {
  if (!isValidLabel(name)) throw new Error("Not a top-level name.");
  const rpc = defaultRpc();
  const prog = accountBytes(program);
  const { id: baseId } = await deriveName(name);
  const slotId = tieredId(baseId, priceTierForLen(name.length));
  const signer = publicAccountId(privateKey);
  const recBytes = await readActorState(rpc, program, formatAccountId(recordAddress(prog, slotId)));
  if (recBytes.length === 0) throw new Error(`${name} isn't registered.`);
  const rec = decodeNameRecord(recBytes);
  if (rec.ownerTag !== toHex(ownerTag(slotId, signer))) throw new Error(`This wallet doesn't hold ${name}.`);
  if (!rec.listing) throw new Error(`${name} isn't listed for sale.`);
  const [nonce] = await getAccountsNonces(rpc, [signer]);
  const signed = signPublicTransaction(
    cancelMessage({ program: prog, signer, nonce, gasLimit, id: slotId, inMarket: Boolean(rec.listing.inMarket) }),
    [privateKey],
  );
  await sendTransaction(rpc, signed);
  const block = await waitForTransaction(rpc, signed.hash);
  const after = decodeNameRecord(await readActorState(rpc, program, formatAccountId(recordAddress(prog, slotId))));
  if (after.listing !== null) throw reverted("Cancelling the listing", signed.hash);
  return { name, txHash: signed.hash, block };
}

export async function buyListedName({ privateKey, program, name, now = null, gasLimit = BUY_GAS }) {
  if (!isValidLabel(name)) throw new Error("Only top-level names can be bought here.");
  const rpc = defaultRpc();
  const prog = accountBytes(program);
  const { id: baseId } = await deriveName(name);
  const slotId = tieredId(baseId, priceTierForLen(name.length));
  const signer = publicAccountId(privateKey);
  const [settings, recBytes] = await Promise.all([
    readRegistrySettings(program, rpc),
    readActorState(rpc, program, formatAccountId(recordAddress(prog, slotId))),
  ]);
  if (recBytes.length === 0) throw new Error(`${name} isn't registered.`);
  const rec = decodeNameRecord(recBytes);
  if (!rec.listing) throw new Error(`${name} isn't listed for sale.`);
  if (rec.nftShielded) throw new Error(`${name} is shielded and can't be bought.`);
  if (rec.ownerTag === toHex(ownerTag(slotId, signer))) throw new Error(`You already hold ${name}.`);
  if (!settings?.treasury) throw new Error("The registry has no treasury configured.");
  const { price, payTo, expiresAt, nonce: expectedNonce, inMarket } = rec.listing;
  const split = saleSplit(price);
  const { need, have, enough } = await estimateNeed(privateKey, { amount: price, resume: true, gasLimit, dataBytes: ACTION_BYTES });
  if (!enough) throw Object.assign(new Error(`Not enough tokens: need about ${formatTokens(need)}, you have ${formatTokens(have)}.`), { code: "insufficient" });
  const [[nonce], claimedNow] = await Promise.all([
    getAccountsNonces(rpc, [signer]),
    now !== null && now !== undefined ? Promise.resolve(now) : chainNow(rpc),
  ]);
  if (rec.expires < Number.MAX_SAFE_INTEGER && claimedNow >= rec.expires) throw new Error(`${name} has expired.`);
  if (expiresAt !== null && claimedNow >= expiresAt) throw new Error("This listing has expired.");
  const signed = signPublicTransaction(
    buyMessage({
      program: prog, signer, nonce, gasLimit, id: slotId, price,
      payTo: accountBytes(payTo), treasury: accountBytes(settings.treasury),
      expectedNonce, now: claimedNow, inMarket: Boolean(inMarket),
      nftProgram: optionalNftProgram(settings),
      nftTokenId: rec.nftTokenId ? fromHex(rec.nftTokenId) : null,
      sitePage: rec.siteListed ?? null,
    }),
    [privateKey],
  );
  await sendTransaction(rpc, signed);
  const block = await waitForTransaction(rpc, signed.hash);
  const after = decodeNameRecord(await readActorState(rpc, program, formatAccountId(recordAddress(prog, slotId))));
  if (after.ownerTag !== toHex(ownerTag(slotId, signer))) throw reverted("The purchase", signed.hash);
  return { name, txHash: signed.hash, block, price, split };
}

// ---------- subnames: create, revoke, toggle sale, buy ----------

export async function setSubnameSale({ privateKey, program, parentName, enabled, price = 0n, payout = null, gasLimit = SUBNAME_GAS }) {
  if (!isValidLabel(parentName)) throw new Error("Subname sales are set on a top-level name.");
  const rpc = defaultRpc();
  const prog = accountBytes(program);
  const { id: parentBaseId } = await deriveName(parentName);
  const parentSlotId = tieredId(parentBaseId, priceTierForLen(parentName.length));
  const signer = publicAccountId(privateKey);
  const recBytes = await readActorState(rpc, program, formatAccountId(recordAddress(prog, parentSlotId)));
  if (recBytes.length === 0) throw new Error(`${parentName} isn't registered.`);
  const rec = decodeNameRecord(recBytes);
  if (rec.ownerTag !== toHex(ownerTag(parentSlotId, signer))) throw new Error(`This wallet doesn't hold ${parentName}.`);
  const sale = enabled ? { price: BigInt(price), payout: payout ? accountBytes(payout) : signer } : null;
  const [nonce] = await getAccountsNonces(rpc, [signer]);
  const signed = signPublicTransaction(
    setSubnameSaleMessage({ program: prog, signer, nonce, gasLimit, id: parentSlotId, sale }),
    [privateKey],
  );
  await sendTransaction(rpc, signed);
  const block = await waitForTransaction(rpc, signed.hash);
  const after = decodeNameRecord(await readActorState(rpc, program, formatAccountId(recordAddress(prog, parentSlotId))));
  if (Boolean(after.subnameSale) !== Boolean(enabled)) throw reverted("Updating subname sales", signed.hash);
  return { parentName, txHash: signed.hash, block, sale: after.subnameSale };
}

export async function createSubname({
  privateKey, program, fullName, ownerAccount = null, revocable = true, now = null, gasLimit = SUBNAME_GAS,
}) {
  const split = splitSubname(fullName);
  if (!split) throw new Error("Enter a one-level subname like blog.alice (no --, max 62 chars).");
  const rpc = defaultRpc();
  const prog = accountBytes(program);
  const [{ id: parentBaseId }, { id: subId, key: subKey }] = await Promise.all([
    deriveName(split.parent),
    deriveName(fullName),
  ]);
  const parentSlotId = tieredId(parentBaseId, priceTierForLen(split.parent.length));
  const signer = publicAccountId(privateKey);
  const targetOwner = ownerAccount ? accountBytes(ownerAccount) : signer;
  const owner = ownerTag(subId, targetOwner);
  const profile = encodeProfile({ name: fullName, account: formatAccountId(targetOwner), npk: null, vpk: null, texts: {} });
  const sealed = await seal(subKey, profile, "record");
  const [parentBytes, [nonce], claimedNow] = await Promise.all([
    readActorState(rpc, program, formatAccountId(recordAddress(prog, parentSlotId))),
    getAccountsNonces(rpc, [signer]),
    now !== null && now !== undefined ? Promise.resolve(now) : chainNow(rpc),
  ]);
  if (parentBytes.length === 0) throw new Error(`${split.parent} isn't registered.`);
  const parent = decodeNameRecord(parentBytes);
  if (parent.ownerTag !== toHex(ownerTag(parentSlotId, signer))) throw new Error(`This wallet doesn't hold ${split.parent}.`);
  const signed = signPublicTransaction(
    createSubnameMessage({ program: prog, signer, nonce, gasLimit, parentId: parentSlotId, subId, owner, revocable, sealed, now: claimedNow }),
    [privateKey],
  );
  await sendTransaction(rpc, signed);
  const block = await waitForTransaction(rpc, signed.hash);
  const afterBytes = await readActorState(rpc, program, formatAccountId(subnameRecordAddress(prog, parentSlotId, subId)));
  if (afterBytes.length === 0 || decodeSubnameRecord(afterBytes).ownerTag !== toHex(owner)) {
    throw reverted("Creating the subname", signed.hash);
  }
  return { name: fullName, txHash: signed.hash, block, revocable };
}

export async function revokeSubname({ privateKey, program, fullName, now = null, gasLimit = SUBNAME_GAS }) {
  const split = splitSubname(fullName);
  if (!split) throw new Error("Enter a one-level subname like blog.alice.");
  const rpc = defaultRpc();
  const prog = accountBytes(program);
  const [{ id: parentBaseId }, { id: subId }] = await Promise.all([
    deriveName(split.parent),
    deriveName(fullName),
  ]);
  const parentSlotId = tieredId(parentBaseId, priceTierForLen(split.parent.length));
  const signer = publicAccountId(privateKey);
  const [[nonce], claimedNow] = await Promise.all([
    getAccountsNonces(rpc, [signer]),
    now !== null && now !== undefined ? Promise.resolve(now) : chainNow(rpc),
  ]);
  const signed = signPublicTransaction(
    revokeSubnameMessage({ program: prog, signer, nonce, gasLimit, parentId: parentSlotId, subId, now: claimedNow }),
    [privateKey],
  );
  await sendTransaction(rpc, signed);
  const block = await waitForTransaction(rpc, signed.hash);
  const afterBytes = await readActorState(rpc, program, formatAccountId(subnameRecordAddress(prog, parentSlotId, subId)));
  if (afterBytes.length !== 0) throw reverted("Revoking the subname", signed.hash);
  return { name: fullName, txHash: signed.hash, block };
}

/**
 * Buys a permanent subname (`label.parent`) from a parent with `subname_sale` enabled.
 * Uses the same CommitPublic + 5-minute wait anti-front-running flow as top-level registration.
 */
export async function buySubname({
  privateKey, program, fullName, resume = null, onStep = () => {}, now = null, gasLimit = BUY_SUBNAME_GAS,
}) {
  const split = splitSubname(fullName);
  if (!split) throw new Error("Enter a one-level subname like blog.alice.");
  const rpc = defaultRpc();
  const prog = accountBytes(program);
  const clock = async () => now ?? await chainNow(rpc);
  const [{ id: parentBaseId }, { id: subId, key: subKey }] = await Promise.all([
    deriveName(split.parent),
    deriveName(fullName),
  ]);
  const parentSlotId = tieredId(parentBaseId, priceTierForLen(split.parent.length));
  const signer = publicAccountId(privateKey);
  const parentBytes = await readActorState(rpc, program, formatAccountId(recordAddress(prog, parentSlotId)));
  if (parentBytes.length === 0) throw new Error(`${split.parent} isn't registered.`);
  const parent = decodeNameRecord(parentBytes);
  if (!parent.subnameSale) throw new Error(`${split.parent} isn't selling subnames right now.`);
  const { price, payout } = parent.subnameSale;
  const { need, have, enough } = await estimateNeed(privateKey, {
    amount: price, resume: Boolean(resume), gasLimit, dataBytes: ACTION_BYTES,
  });
  if (!enough) {
    throw Object.assign(new Error(`Not enough tokens: need about ${formatTokens(need)}, you have ${formatTokens(have)}.`), { code: "insufficient" });
  }
  const holder = ownerTag(subId, signer);
  let secret, made;
  if (resume) {
    secret = fromHex(resume.secretHex);
    made = resume.made;
  } else {
    secret = crypto.getRandomValues(new Uint8Array(32));
    onStep("commit", "committing to the subname");
    const promised = subnameCommitmentHash(parentSlotId, subId, holder, secret);
    const [[nonce], committedAt] = await Promise.all([getAccountsNonces(rpc, [signer]), clock()]);
    const commitSigned = signPublicTransaction(
      commitPublicMessage({ program: prog, signer, nonce, gasLimit: COMMIT_GAS, now: committedAt, commitment: promised }),
      [privateKey],
    );
    const h = await sendTransaction(rpc, commitSigned);
    await waitForTransaction(rpc, commitSigned.hash);
    await confirmCommitted(rpc, program, promised, commitSigned.hash, { public: true });
    made = committedAt;
    onStep("committed", { secretHex: toHex(secret), made, public: true, tx: h });
  }
  const ready = made + PUBLIC_MIN_COMMIT_AGE_MS;
  for (let at = await clock(); at < ready; at = await clock()) {
    if (at >= made + MAX_COMMIT_AGE_MS) {
      throw Object.assign(new Error("The commitment expired (older than 24 hours). Start again."), { code: "expired" });
    }
    onStep("waiting", { remainingMs: ready - at });
    await new Promise((r) => setTimeout(r, Math.min(30000, Math.max(5000, ready - at))));
  }
  onStep("buy", "buying the subname");
  const profile = encodeProfile({ name: fullName, account: formatAccountId(signer), npk: null, vpk: null, texts: {} });
  const sealed = await seal(subKey, profile, "record");
  const [[nonce], boughtAt] = await Promise.all([getAccountsNonces(rpc, [signer]), clock()]);
  if (boughtAt >= made + MAX_COMMIT_AGE_MS) {
    throw Object.assign(new Error("The commitment expired (older than 24 hours). Start again."), { code: "expired" });
  }
  const signed = signPublicTransaction(
    buySubnameMessage({
      program: prog, signer, nonce, gasLimit, parentId: parentSlotId, subId, sealed,
      price, payout: price > 0n ? accountBytes(payout) : null, now: boughtAt, secret,
    }),
    [privateKey],
  );
  const h = await sendTransaction(rpc, signed);
  const block = await waitForTransaction(rpc, signed.hash);
  const afterBytes = await readActorState(rpc, program, formatAccountId(subnameRecordAddress(prog, parentSlotId, subId)));
  if (afterBytes.length === 0 || decodeSubnameRecord(afterBytes).ownerTag !== toHex(holder)) {
    throw reverted("Buying the subname", signed.hash);
  }
  onStep("done", { tx: h, block });
  return { name: fullName, txHash: signed.hash, block, price };
}

// ---------- set-text: publish/update a text entry on a name or subname you hold ----------

export async function buildSetRecordRaw({ privateKey, program, name, sealed, gasLimit = SET_RECORD_GAS }) {
  const { id: baseId } = await deriveName(name);
  const slotId = tieredId(baseId, priceTierForLen(name.length));
  const signer = publicAccountId(privateKey);
  const [nonce] = await getAccountsNonces(defaultRpc(), [signer]);
  const message = setRecordMessage({ program: accountBytes(program), signer, nonce, gasLimit, id: slotId, sealed, siteDir: SITE_DIR_KEEP });
  return { signed: signPublicTransaction(message, [privateKey]), id: slotId };
}

/** Set (or clear, with empty value) a text entry on a top-level name or subname the wallet holds. */
export async function setText({ privateKey, program = "38jEfVV1RCyMzu1FGbYcCn5FqxCxKyY58VgzjruAh2gq", name, key, value, gasLimit = SET_RECORD_GAS }) {
  if (!isValidName(name)) throw new Error(`Invalid name: ${name}`);
  const rpc = defaultRpc();
  const prog = accountBytes(program);
  const signer = publicAccountId(privateKey);
  const split = splitSubname(name);

  if (split === null) {
    const { id: baseId, key: nameKey } = await deriveName(name);
    const slotId = tieredId(baseId, priceTierForLen(name.length));
    const state = await readActorState(rpc, program, formatAccountId(recordAddress(prog, slotId)));
    if (!state.length) { const e = new Error(`${name} isn't registered yet — claim it first.`); e.code = "unregistered"; throw e; }
    const rec = decodeNameRecord(state);
    if (rec.ownerTag !== toHex(ownerTag(slotId, signer))) {
      const e = new Error(`This device's wallet doesn't hold ${name}.`); e.code = "not_owner"; throw e;
    }
    const plain = await openSealed(nameKey, rec.sealed, "record");
    const profile = plain ? decodeProfile(plain) : { name, account: null, npk: null, vpk: null, texts: {} };
    if (value) profile.texts[key] = value; else delete profile.texts[key];
    const sealed = await seal(nameKey, encodeProfile(profile), "record");
    const [nonce] = await getAccountsNonces(rpc, [signer]);
    const signed = signPublicTransaction(
      setRecordMessage({ program: prog, signer, nonce, gasLimit, id: slotId, sealed, siteDir: SITE_DIR_KEEP }),
      [privateKey],
    );
    await sendTransaction(rpc, signed);
    const block = await waitForTransaction(rpc, signed.hash);
    const after = decodeNameRecord(await readActorState(rpc, program, formatAccountId(recordAddress(prog, slotId))));
    if (toHex(after.sealed) !== toHex(sealed)) throw reverted("The change", signed.hash);
    return { txHash: signed.hash, block };
  }

  const [{ id: parentBaseId }, { id: subId, key: subKey }] = await Promise.all([
    deriveName(split.parent),
    deriveName(name),
  ]);
  const parentSlotId = tieredId(parentBaseId, priceTierForLen(split.parent.length));
  const subAddr = formatAccountId(subnameRecordAddress(prog, parentSlotId, subId));
  const state = await readActorState(rpc, program, subAddr);
  if (!state.length) { const e = new Error(`${name} isn't registered yet.`); e.code = "unregistered"; throw e; }
  const sub = decodeSubnameRecord(state);
  if (sub.ownerTag !== toHex(ownerTag(subId, signer))) {
    const e = new Error(`This device's wallet doesn't hold ${name}.`); e.code = "not_owner"; throw e;
  }
  const plain = await openSealed(subKey, sub.sealed, "record");
  const profile = plain ? decodeProfile(plain) : { name, account: null, npk: null, vpk: null, texts: {} };
  if (value) profile.texts[key] = value; else delete profile.texts[key];
  const sealed = await seal(subKey, encodeProfile(profile), "record");
  const isRevocable = sub.kind.type === "revocable";
  const [[nonce], claimedNow] = await Promise.all([
    getAccountsNonces(rpc, [signer]),
    isRevocable ? chainNow(rpc) : Promise.resolve(null),
  ]);
  const signed = signPublicTransaction(
    setSubnameRecordMessage({ program: prog, signer, nonce, gasLimit: SUBNAME_GAS, parentId: parentSlotId, subId, sealed, now: claimedNow }),
    [privateKey],
  );
  await sendTransaction(rpc, signed);
  const block = await waitForTransaction(rpc, signed.hash);
  const after = decodeSubnameRecord(await readActorState(rpc, program, subAddr));
  if (toHex(after.sealed) !== toHex(sealed)) throw reverted("The change", signed.hash);
  return { txHash: signed.hash, block };
}
