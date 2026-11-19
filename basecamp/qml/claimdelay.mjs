// SPDX-License-Identifier: MIT OR Apache-2.0
// The claim delay of a private receive, as the "Claim privately" page shows it (rfp-022
// docs/privacy-m5/claim-delay.md; the LEZ Bridge's `receive_claim_plan`, feature `claim_delay`).
//
// The LEZ Bridge draws ONE random claim time per private receive (1 to 24 hours after this wallet
// first saw it claimable) and keeps it; this file never draws or changes it, so the time survives a
// restart. The page waits for that time, and when it has come while the page is open and Basecamp
// is in front, it starts the claim exactly as pressing "Claim privately" does: the wallet still
// asks the person to approve it, then proves it here. Nothing runs in the background.
// Pure: Main.qml imports it, and basecamp/claimdelay.test.mjs checks it.

import { PRIVATE_PROOF_TIME } from "./logic.mjs"

/// The bridge's `config().features` word, and its call (id, options JSON).
export const CLAIM_DELAY_FEATURE = "claim_delay"
export const CLAIM_DELAY_CALL = "receive_claim_plan"
/// A check by itself runs at most this often, and only while the page is open and in front.
export const CLAIM_DELAY_CHECK_MS = 30000
/// Past this much, the line also says the day.
export const CLAIM_DELAY_DAY_AFTER_MS = 20 * 3600 * 1000

export const CLAIM_DELAY_NOW_LABEL = "Claim now instead"
export const CLAIM_DELAY_NOW_BUSY = "Starting…"
export const CLAIM_DELAY_NOW_COST = "Claiming now makes this claim easier to match to your deposit by its timing."
export const CLAIM_DELAY_PAGE_NOTE = "The claim starts the next time you have this page open after that time. You then approve it, and the proof takes " + PRIVATE_PROOF_TIME + " on this computer."
export const CLAIM_DELAY_DUE_LINE = "The planned claim time has come: starting the claim."
export const CLAIM_DELAY_READING = "Reading the claim time…"
export const CLAIM_DELAY_ON_DETAIL = "Claim time: a random wait of 1 to 24 hours after this wallet saw the deposit arrive, chosen once and kept. Deposits of the same amount made meanwhile hide which one is yours."
export const CLAIM_DELAY_NOW_DETAIL = "You chose to claim now, without the random wait."
export const CLAIM_DELAY_UNREAD = "Couldn't read this claim's planned time, so the claim waits. Try again, or claim now instead."
export const CLAIM_DELAY_RETRY_LABEL = "Try again"
export const CLAIM_DELAY_PUBLIC_LINE = "No wait for a public account: its claim shows which deposit it is anyway, so waiting would hide nothing."
export const CLAIM_DELAY_CARD_DUE = "Its planned claim time has come: open it to claim privately."

export function claimDelayCan(features) {
    return Array.isArray(features) && features.indexOf(CLAIM_DELAY_FEATURE) >= 0
}

function isPrivate(row) {
    return !!row && (row.account_kind === "private" || row.claim === "private" || /^Private\//.test(String(row.account || "")))
}

function pad2(n) {
    return (n < 10 ? "0" : "") + n
}

/// The local clock time of `atMs`, "HH:MM", and the day word when it is more than 20 hours after
/// `nowMs`: "today", "tomorrow", else the weekday. `offsetMin`: minutes east of UTC (default: this
/// computer's at that time).
export function claimTimeParts(atMs, nowMs, offsetMin) {
    var off = offsetMin === undefined || offsetMin === null ? -new Date(atMs).getTimezoneOffset() : Number(offsetMin)
    var local = new Date(Number(atMs) + off * 60000)
    var clock = pad2(local.getUTCHours()) + ":" + pad2(local.getUTCMinutes())
    var day = ""
    if (Number(atMs) - Number(nowMs) > CLAIM_DELAY_DAY_AFTER_MS) {
        var dayOf = function(ms) { return Math.floor((Number(ms) + off * 60000) / 86400000) }
        var gap = dayOf(atMs) - dayOf(nowMs)
        day = gap === 0 ? "today" : gap === 1 ? "tomorrow" : ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"][local.getUTCDay()]
    }
    return { clock: clock, day: day }
}

/// "Claiming at about 14:30 to hide which deposit is yours." (with the day when it is far off:
/// "Claiming tomorrow at about 14:30 …", "Claiming on Friday at about 14:30 …").
export function claimDelayLine(atMs, nowMs, offsetMin) {
    var p = claimTimeParts(atMs, nowMs, offsetMin)
    var when = p.day === "" ? "" : p.day === "today" || p.day === "tomorrow" ? p.day + " " : "on " + p.day + " "
    return "Claiming " + when + "at about " + p.clock + " to hide which deposit is yours."
}

/// What the page shows for receive `row` (a receives() row) given `plan`, the bridge's
/// `receive_claim_plan` answer (null: not answered yet; {failed: true}: the call failed):
/// {on, due, hold, atMs, why, line, detail}.
/// - on: the wait applies (a private receive, the bridge has the feature, its plan says "on").
/// - hold: the Claim button waits (reading the plan, or on and not yet due).
/// - why: "on", "reading", "no_feature", "public", "chose_now", "claimed", "pending", "unread".
/// Off for any reason is today's page: the Claim button as before.
export function claimDelayState(s) {
    var st = s || {}
    var off = function(why, detail) { return { on: false, due: false, hold: false, retry: false, atMs: 0, why: why, line: "", detail: detail || "" } }
    var reading = { on: false, due: false, hold: true, retry: false, atMs: 0, why: "reading", line: CLAIM_DELAY_READING, detail: "" }
    // "Claim now instead" pressed on this opening: the page claims now; the bridge keeps its plan
    // until the claim is really sent (review 4, finding 3), so a refused or declined claim waits again.
    if (st.override === true) return off("chose_now", CLAIM_DELAY_NOW_DETAIL)
    // The bridge's features not read yet: a private receive waits for them (fails closed).
    if (st.featuresKnown === false && st.row && isPrivate(st.row)) return reading
    if (!claimDelayCan(st.features)) return off("no_feature")
    if (!st.row) return off("unread")
    if (!isPrivate(st.row)) return off("public", CLAIM_DELAY_PUBLIC_LINE)
    var p = st.plan
    if (p === null || p === undefined) return reading
    // The plan call failed: the same plan as receive_status last carried it, else hold with a
    // plain line, Try again, and "Claim now instead" as the way past (review 4, finding 2).
    if (typeof p !== "object" || p.failed) {
        var f = st.fallback
        if (f && typeof f === "object" && typeof f.delay === "string") p = f
        else return { on: false, due: false, hold: true, retry: true, atMs: 0, why: "unread", line: CLAIM_DELAY_UNREAD, detail: "" }
    }
    if (p.delay === "off") return off(p.why === "chose_now" ? "chose_now" : p.why === "public" ? "public" : "claimed",
                                      p.why === "chose_now" ? CLAIM_DELAY_NOW_DETAIL : p.why === "public" ? CLAIM_DELAY_PUBLIC_LINE : "")
    if (p.delay !== "on" || !(Number(p.claim_at) > 0)) return off("pending")
    var atMs = Number(p.claim_at) * 1000
    var now = Number(st.nowMs) || 0
    var due = now >= atMs
    return { on: true, due: due, hold: !due, retry: false, atMs: atMs, why: "on",
             line: due ? CLAIM_DELAY_DUE_LINE : claimDelayLine(atMs, now, st.offsetMin), detail: CLAIM_DELAY_ON_DETAIL }
}

/// Whether the page's own check runs (its 30 s timer): only while the page is open and Basecamp is
/// in front, the wait is on, the claim not started, and it hasn't started itself on this opening.
export function claimDelayWatching(s) {
    var st = s || {}
    return !!st.on && !!st.open && !!st.front && st.stage === "start" && !st.started
}

/// Whether the check starts the claim now: due, the page open and in front, nothing started or
/// failed, the claim possible, not already started by itself on this opening. The 30 s timer paces
/// the checks; there is no second throttle (review 4, finding 4: a tick that came while the page was
/// still looking up the wallet's jobs must not hold the start back 30 s). The claim it starts is the
/// same as pressing Claim: the person approves it.
export function claimDelayAutoStart(s) {
    var st = s || {}
    return claimDelayWatching(st) && !!st.due && st.able !== false && String(st.error || "") === ""
}

/// The page's controls for a view: the status line's own action ("now": Claim now instead,
/// "retry": read the plan again, or ""), whether a separate "Claim now instead" shows (beside Try
/// again), and whether the cost line and the page note show.
export function claimDelayControls(view) {
    var v = view || {}
    var unread = v.why === "unread" && v.hold === true
    var waiting = v.on === true && v.hold === true
    return { lineAction: unread ? "retry" : waiting ? "now" : "", nowLink: unread, cost: unread || waiting, note: waiting }
}

/// The home card's line for a receive whose claim time is planned (rfp-022 docs/PRIVACY-M5.md §1):
/// "" when there is none. The card never starts a claim; it only says when the "Claim privately"
/// page will. `s`: {features, row (a card row: id, account, account_kind), status (its
/// receive_status answer, which carries `claim_delay`), nowMs, offsetMin}. The card's Claim button
/// stays as it is: pressing it opens the page, which waits or offers "Claim now instead".
export function claimDelayCardLine(s) {
    var st = s || {}
    var plan = st.status && typeof st.status === "object" ? st.status.claim_delay : undefined
    if (plan === undefined || plan === null) return ""
    var v = claimDelayState({ features: st.features, row: st.row, plan: plan, nowMs: st.nowMs, offsetMin: st.offsetMin })
    if (!v.on) return ""
    if (v.due) return CLAIM_DELAY_CARD_DUE
    var p = claimTimeParts(v.atMs, Number(st.nowMs) || 0, st.offsetMin)
    var when = p.day === "" ? "" : p.day === "today" || p.day === "tomorrow" ? p.day + " " : "on " + p.day + " "
    return "Planned claim " + when + "at about " + p.clock + ", to hide which deposit is yours. Open it then to claim."
}
