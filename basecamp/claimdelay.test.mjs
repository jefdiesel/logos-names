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
check("off: the plan call failed (today's page)", D.claimDelayState({ features, row: priv, plan: { failed: true }, nowMs: now }).hold, false);
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
check("not within 30 s of the last check", [D.claimDelayAutoStart(Object.assign({}, base, { lastAt: now - 29999 })), D.claimDelayAutoStart(Object.assign({}, base, { lastAt: now - 30000 }))], [false, true]);
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

console.log(failures === 0 ? "ALL PASS" : failures + " FAILED");
process.exit(failures ? 1 : 0);
