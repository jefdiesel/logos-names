// SPDX-License-Identifier: MIT OR Apache-2.0
// Checks the claim delay's logic (qml/claimdelay.mjs, which qml/Main.qml imports) and that the
// page uses it as designed. Run with: node basecamp/claimdelay.test.mjs
import * as D from "./qml/claimdelay.mjs";
import { readFileSync } from "node:fs";

let failures = 0;
const check = (label, got, want) => {
  const ok = JSON.stringify(got) === JSON.stringify(want);
  if (!ok) failures++;
  console.log((ok ? "ok   " : "FAIL ") + label + (ok ? "" : `\n   got  ${JSON.stringify(got)}\n   want ${JSON.stringify(want)}`));
};

const H = 3600 * 1000;
// 2026-11-18 09:00 UTC, a Wednesday.
const now = Date.UTC(2026, 10, 18, 9, 0, 0);
const features = ["receive", "claim_delay"];
const priv = { id: "dep-1", account: "Private/AAA", account_kind: "private", claim: "private" };
const pub = { id: "dep-2", account: "Public/BBB", account_kind: "public", claim: "relayed" };
const plan = (claimAtMs, extra) => Object.assign({ id: "dep-1", delay: "on", why: "private", claim_at: Math.floor(claimAtMs / 1000), delay_s: 7200 }, extra || {});

// The clock line: local time (here UTC and UTC+1 given explicitly), the day only past 20 hours.
check("line, same day", D.claimDelayLine(now + 2 * H + 30 * 60000, now, 0), "Claiming at about 11:30 to hide which deposit is yours.");
check("line, another zone", D.claimDelayLine(now + 2 * H, now, 60), "Claiming at about 12:00 to hide which deposit is yours.");
check("line, after midnight but under 20 h: no day", D.claimDelayLine(now + 19 * H, now, 0), "Claiming at about 04:00 to hide which deposit is yours.");
check("line, over 20 h: tomorrow", D.claimDelayLine(now + 23 * H, now, 0), "Claiming tomorrow at about 08:00 to hide which deposit is yours.");
const lateNow = Date.UTC(2026, 10, 18, 0, 10, 0);
check("line, over 20 h but the same day: today", D.claimDelayLine(lateNow + 21 * H, lateNow, 0), "Claiming today at about 21:10 to hide which deposit is yours.");
check("parts: weekday past tomorrow", D.claimTimeParts(now + 47 * H, now, 0), { clock: "08:00", day: "Friday" });
check("parts: default offset is this computer's", D.claimTimeParts(now, now).clock.length, 5);

// The page's state.
check("off: the bridge lacks the feature (today's page)", D.claimDelayState({ features: ["receive"], row: priv, plan: plan(now + H), nowMs: now }).why, "no_feature");
check("off: no row", D.claimDelayState({ features, row: null, plan: null, nowMs: now }).hold, false);
const pubState = D.claimDelayState({ features, row: pub, plan: null, nowMs: now });
check("off: a public account, with its reason", [pubState.on, pubState.hold, pubState.why, pubState.detail], [false, false, "public", D.CLAIM_DELAY_PUBLIC_LINE]);
check("reading: the button waits for the plan", (({ hold, line, why }) => ({ hold, line, why }))(D.claimDelayState({ features, row: priv, plan: null, nowMs: now })), { hold: true, line: D.CLAIM_DELAY_READING, why: "reading" });
// Review 4, finding 2: a failed plan call fails CLOSED: the claim waits, with a plain line and Try again.
const unread = D.claimDelayState({ features, row: priv, plan: { failed: true }, nowMs: now });
check("failed plan call: holds, says so, offers Try again", [unread.hold, unread.retry, unread.why, unread.line], [true, true, "unread", D.CLAIM_DELAY_UNREAD]);
check("failed plan call: receive_status's plan is used instead", D.claimDelayState({ features, row: priv, plan: { failed: true }, fallback: plan(now + 2 * H), nowMs: now, offsetMin: 0 }).line, "Claiming at about 11:00 to hide which deposit is yours.");
check("failed plan call, a public account: the plain button", D.claimDelayState({ features, row: pub, plan: { failed: true }, nowMs: now }).hold, false);
check("features not read yet, a private receive: waits", [D.claimDelayState({ features: [], featuresKnown: false, row: priv, plan: null, nowMs: now }).hold, D.claimDelayState({ features: [], featuresKnown: false, row: priv, plan: null, nowMs: now }).why], [true, "reading"]);
check("features read, no claim_delay: the plain button", D.claimDelayState({ features: ["receive"], featuresKnown: true, row: priv, plan: null, nowMs: now }).hold, false);
check("controls, unread: Try again on the line, Claim now beside it, the cost", D.claimDelayControls(unread), { lineAction: "retry", nowLink: true, cost: true, note: false });
// Review 4, finding 3: "Claim now instead" is page-local: it lifts the hold for this opening only.
const over = D.claimDelayState({ features, row: priv, plan: plan(now + 20 * H), override: true, nowMs: now });
check("claim now: the page claims now, the plan kept by the bridge", [over.on, over.hold, over.why], [false, false, "chose_now"]);
check("claim now not pressed: the same plan holds again", D.claimDelayState({ features, row: priv, plan: plan(now + 20 * H), override: false, nowMs: now }).hold, true);
const waiting = D.claimDelayState({ features, row: priv, plan: plan(now + 2 * H), nowMs: now, offsetMin: 0 });
check("on, waiting", [waiting.on, waiting.due, waiting.hold, waiting.line, waiting.detail === D.CLAIM_DELAY_ON_DETAIL], [true, false, true, "Claiming at about 11:00 to hide which deposit is yours.", true]);
const due = D.claimDelayState({ features, row: priv, plan: plan(now - 1000), nowMs: now });
check("on, due: no hold, the starting line", [due.on, due.due, due.hold, due.line], [true, true, false, D.CLAIM_DELAY_DUE_LINE]);
check("exactly at the time is due", D.claimDelayState({ features, row: priv, plan: plan(now), nowMs: now }).due, true);
const chose = D.claimDelayState({ features, row: priv, plan: plan(now, { delay: "off", why: "chose_now" }), nowMs: now });
check("off: the person chose now", [chose.on, chose.hold, chose.why, chose.detail], [false, false, "chose_now", D.CLAIM_DELAY_NOW_DETAIL]);
check("off: claimed", D.claimDelayState({ features, row: priv, plan: { delay: "off", why: "claimed" }, nowMs: now }).why, "claimed");
check("off: not claimable yet (no plan drawn)", D.claimDelayState({ features, row: priv, plan: { delay: "pending", why: "not_claimable" }, nowMs: now }).why, "pending");

// When a check by itself may run: page open AND Basecamp in front; at most every 30 s; once per opening.
const base = { on: true, due: true, open: true, front: true, stage: "start", started: false, able: true, error: "", lastAt: 0, nowMs: now };
check("starts when due, open and in front", D.claimDelayAutoStart(base), true);
check("never while Basecamp is behind", D.claimDelayAutoStart(Object.assign({}, base, { front: false })), false);
check("never with the page closed", D.claimDelayAutoStart(Object.assign({}, base, { open: false })), false);
check("not before the time", D.claimDelayAutoStart(Object.assign({}, base, { due: false })), false);
check("not twice on one opening", D.claimDelayAutoStart(Object.assign({}, base, { started: true })), false);
check("not while something else shows (a claim under way, an error)", [D.claimDelayAutoStart(Object.assign({}, base, { stage: "checking" })), D.claimDelayAutoStart(Object.assign({}, base, { error: "x" }))], [false, false]);
check("not when the bridge can't claim", D.claimDelayAutoStart(Object.assign({}, base, { able: false })), false);
// Review 4, finding 4: a check made while the page was still looking does not hold the start back.
check("a recent check does not delay the start", [D.claimDelayAutoStart(Object.assign({}, base, { lastAt: now - 10 })), D.claimDelayAutoStart(Object.assign({}, base, { lastAt: now - 29999 }))], [true, true]);
check("off: no check at all", D.claimDelayWatching(Object.assign({}, base, { on: false })), false);
check("watching only while open and in front", [D.claimDelayWatching(base), D.claimDelayWatching(Object.assign({}, base, { front: false })), D.claimDelayWatching(Object.assign({}, base, { open: false }))], [true, false, false]);

// The words: plain, honest about the page having to be open, never "official" or "lookup".
const words = [D.CLAIM_DELAY_NOW_LABEL, D.CLAIM_DELAY_NOW_COST, D.CLAIM_DELAY_PAGE_NOTE, D.CLAIM_DELAY_DUE_LINE, D.CLAIM_DELAY_ON_DETAIL, D.CLAIM_DELAY_NOW_DETAIL, D.CLAIM_DELAY_PUBLIC_LINE, D.CLAIM_DELAY_READING].join(" ");
check("no forbidden words", /official|lookup/i.test(words), false);
check("the page note says it needs the page open and an approval", /next time you have this page open/.test(D.CLAIM_DELAY_PAGE_NOTE) && /approve/.test(D.CLAIM_DELAY_PAGE_NOTE) && /about 10 minutes/.test(D.CLAIM_DELAY_PAGE_NOTE), true);

// Main.qml uses it as designed.
const qml = readFileSync(new URL("./qml/Main.qml", import.meta.url), "utf8");
check("Main.qml imports it", /^import "claimdelay\.mjs" as ClaimDelay$/m.test(qml), true);
check("the plan is read from the bridge (never drawn here)", qml.includes("ClaimDelay.CLAIM_DELAY_CALL") && !/Math\.random/.test(qml), true);
// No timer of its own: the private wait's 30 s timer (Logic.PRIVATE_CHECK_MS, only while its page is
// open and Basecamp in front) also runs while the claim time is watched, and its tick checks it.
check("the 30 s of the rule is the private wait's", D.CLAIM_DELAY_CHECK_MS, 30000);
check("watched only on the rule", /readonly property bool pcDelayWatchNow: ClaimDelay\.claimDelayWatching\(/.test(qml), true);
check("the private wait's timer runs while it is watched", /readonly property bool privateWaitOnView: section === "wallet" && receiveFrontmost\s*\n\s*&& \(\(walletView === "pclaim" && \([^\n]*\|\| pcDelayWatchNow\)\)/.test(qml), true);
check("its tick checks the claim time", /if \(walletView === "pclaim"\) \{\s*\n\s*pcDelayTick\(\)\s*\n\s*pcPoll\(\)/.test(qml), true);
check("a due claim starts exactly as pressing Claim does", /ClaimDelay\.claimDelayAutoStart\([\s\S]{0,400}?root\.pcStart\(\)/.test(qml), true);
check("the Claim button waits while the plan holds it", /visible: \(pclaimScreen\.stage === "start" \|\| pclaimScreen\.stage === "checking" \|\| pclaimScreen\.stage === "asking"\) && !\(pclaimScreen\.stage === "start" && root\.pcDelayView\.hold\)/.test(qml), true);


// The home card's line (claimDelayCardLine): from receive_status's claim_delay; never a claim.
const cardStatus = (claimAtMs) => ({ stage: "claimable", claim_delay: plan(claimAtMs) });
check("card: planned", D.claimDelayCardLine({ features, row: priv, status: cardStatus(now + 2 * H + 30 * 60000), nowMs: now, offsetMin: 0 }), "Planned claim at about 11:30, to hide which deposit is yours. Open it then to claim.");
check("card: far off says the day", D.claimDelayCardLine({ features, row: priv, status: cardStatus(now + 22 * H), nowMs: now, offsetMin: 0 }), "Planned claim tomorrow at about 07:00, to hide which deposit is yours. Open it then to claim.");
check("card: due", D.claimDelayCardLine({ features, row: priv, status: cardStatus(now - 1000), nowMs: now, offsetMin: 0 }), D.CLAIM_DELAY_CARD_DUE);
check("card: no plan in the status", D.claimDelayCardLine({ features, row: priv, status: { stage: "claimable" }, nowMs: now }), "");
check("card: public receive", D.claimDelayCardLine({ features, row: pub, status: cardStatus(now + H), nowMs: now }), "");
check("card: chose now", D.claimDelayCardLine({ features, row: priv, status: { claim_delay: { delay: "off", why: "chose_now" } }, nowMs: now }), "");
check("card: old bridge", D.claimDelayCardLine({ features: ["receive"], row: priv, status: cardStatus(now + H), nowMs: now }), "");
check("card: no status yet", D.claimDelayCardLine({ features, row: priv, status: undefined, nowMs: now }), "");
check("card words: no forbidden words", /official|lookup/i.test(D.CLAIM_DELAY_CARD_DUE), false);

// Review 4 on the page itself (Main.qml).
check("finding 3: Claim now writes nothing to the bridge before the claim", /function pcDelayClaimNow\(\) \{[\s\S]{0,300}?pcDelayOverride = true\s*\n\s*pcStart\(\)/.test(qml) && !/now: true/.test(qml), true);
check("finding 4: the wallet's jobs answer ticks the claim time", (qml.match(/if \(pcDeposit === id\) pcDelayTick\(\)/g) || []).length, 2);
check("finding 2: features read after the page opened read the plan", /onBridgeFeaturesChanged: if \(walletView === "pclaim"[^\n]*pcDelayLoad\(pcDeposit\)/.test(qml), true);
check("finding 2: a failed read keeps receive_status's plan as the fallback", /fallback: pcDeposit !== "" && receiveStatuses\[pcDeposit\]/.test(qml), true);

// wallet-1120 bundles the LEZ Bridge next-1119b (27f7608), whose FEATURES has claim_delay: its
// receive_claim_plan / receive_status `claim_delay` value as claim_delay.rs `view` writes it, every
// field, read as the page and the card read it.
{
  const nowS = Math.floor(now / 1000);
  const view = (delay, why, claimAt, extra) => Object.assign({ id: "dep-1", delay, why, text: "…", now: nowS, due: delay === "on" && claimAt !== null && nowS >= claimAt,
    seen_claimable_at: nowS - 600, claim_at: claimAt, delay_s: claimAt === null ? null : claimAt - (nowS - 600), planned_claim_at: null, chose_now_at: null, min_s: 3600, max_s: 86400 }, extra || {});
  const bundled = ["receive", "claim_delay", "release_fee_choice", "relay_list", "release_relayer", "burn_opening", "address_book"];
  const waits = D.claimDelayState({ features: bundled, featuresKnown: true, row: priv, plan: view("on", "private", nowS + 7200), nowMs: now, offsetMin: 0 });
  check("27f7608 view: a private receive waits for its time", [waits.on, waits.hold, waits.due, waits.line !== ""], [true, true, false, true]);
  check("27f7608 view: due", D.claimDelayState({ features: bundled, row: priv, plan: view("on", "private", nowS - 1), nowMs: now }).due, true);
  check("27f7608 view: pending (not claimable yet), public, claimed and chose_now are off",
    [D.claimDelayState({ features: bundled, row: priv, plan: view("pending", "not_claimable", null), nowMs: now }).why, D.claimDelayState({ features: bundled, row: priv, plan: view("off", "public", null), nowMs: now }).why,
     D.claimDelayState({ features: bundled, row: priv, plan: view("off", "claimed", nowS - 10), nowMs: now }).why, D.claimDelayState({ features: bundled, row: priv, plan: view("off", "chose_now", nowS, { chose_now_at: nowS }), nowMs: now }).why],
    ["pending", "public", "claimed", "chose_now"]);
  check("27f7608 view: the card's line from receive_status.claim_delay", D.claimDelayCardLine({ features: bundled, row: priv, status: { claim_delay: view("on", "private", nowS + 7200) }, nowMs: now, offsetMin: 0 }),
    "Planned claim at about 11:00, to hide which deposit is yours. Open it then to claim.");
  // The bundled bridge's pin (logic.mjs BUNDLED_BRIDGE) is the build whose FEATURES these are.
  const logic = readFileSync(new URL("./qml/logic.mjs", import.meta.url), "utf8");
  check("the bundled bridge is next-1119b 27f7608", /build: "27f7608" \}/.test(logic), true);
}

console.log(failures === 0 ? "ALL PASS" : failures + " FAILED");
process.exit(failures ? 1 : 0);
