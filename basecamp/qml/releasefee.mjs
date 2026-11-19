// SPDX-License-Identifier: MIT OR Apache-2.0
// The release-relay fee on the Bridge to Ethereum step (rfp-022 docs/privacy-m5/release-fee.md; the
// LEZ Bridge's `send_back_fee_menu` and `send_back_plan_with`, feature `release_fee_choice`).
//
// A send-back's burn fixes the most a release relayer may keep when it finishes the release on
// Ethereum. Until this choice existed every Names Wallet burn fixed it at 0, so no paid release
// relayer could ever finish one. The default stays 0 ("finish it myself, or the bridge's settlement
// service"); the other choices are the vault's relay-fee menu for that token and amount, as the
// bridge reads it. This file only shapes what the bridge answered and which call to make; it never
// makes up a fee. Pure: the Bridge to Ethereum screen imports it (the QML hookup is the marked patch
// basecamp/patches/release-fee-main-qml.patch), and basecamp/releasefee.test.mjs checks it.

export const RELEASE_FEE_FEATURE = "release_fee_choice"
export const RELEASE_FEE_MENU_CALL = "send_back_fee_menu"     // (token, denom_index)
export const RELEASE_FEE_PLAN_CALL = "send_back_plan_with"    // (token, denom_index, account, code, options JSON)
export const RELEASE_FEE_PLAN_OLD = "send_back_plan"          // (token, denom_index, account, code): fee as the code says

export const RELEASE_FEE_HEADING = "Release fee"
export const RELEASE_FEE_READING = "Reading the fee choices…"
export const RELEASE_FEE_FAILED = "The fee choices couldn't be read. The send-back can still go with no relay fee."
export const RELEASE_FEE_OLD_BRIDGE = "This LEZ Bridge has no fee choice yet: the send-back goes with no relay fee."
export const RELEASE_FEE_NONE_LINE = "No relay fee. You finish the release yourself, or the bridge's settlement service does it for nothing."

export function releaseFeeCan(features) {
    return Array.isArray(features) && features.indexOf(RELEASE_FEE_FEATURE) >= 0
}

function choiceNumber(c) {
    var n = Number(c)
    return isFinite(n) && n >= 0 && Math.floor(n) === n ? n : -1
}

/// The sheet: {state: "off" | "reading" | "failed" | "ready", rows: [{choice, title, text, receive,
/// selected}], who, privacy, chosen, note}. `reply` is `send_back_fee_menu`'s value (null while it is
/// being read; {failed: true} when it failed); `chosen` the picked choice (default 0).
export function releaseFeeSheet(features, reply, chosen) {
    if (!releaseFeeCan(features))
        return { state: "off", rows: [], who: "", privacy: "", chosen: 0, note: RELEASE_FEE_OLD_BRIDGE }
    if (reply === null || reply === undefined)
        return { state: "reading", rows: [], who: "", privacy: "", chosen: 0, note: RELEASE_FEE_READING }
    var list = reply && Array.isArray(reply.choices) ? reply.choices : null
    if (!list || list.length === 0 || reply.failed)
        return { state: "failed", rows: [], who: "", privacy: "", chosen: 0, note: RELEASE_FEE_FAILED }
    var pick = choiceNumber(chosen)
    var known = list.some(function(c) { return choiceNumber(c.choice) === pick })
    if (!known) pick = 0
    var rows = list.map(function(c) {
        var n = choiceNumber(c.choice)
        return { choice: n, title: String(c.title || ""), text: String(c.text || ""), receive: String(c.receive_text || ""),
                 fee: String(c.fee || "0"), selected: n === pick }
    }).filter(function(r) { return r.choice >= 0 })
    return { state: "ready", rows: rows, who: String(reply.who || ""), privacy: String(reply.privacy || ""), chosen: pick, note: "" }
}

/// The one line the confirm step shows for the chosen fee.
export function releaseFeeLine(sheet) {
    var s = sheet || {}
    var row = (s.rows || []).filter(function(r) { return r.selected })[0]
    if (!row || row.choice === 0) return RELEASE_FEE_NONE_LINE
    return "Relay fee: " + row.title.charAt(0).toLowerCase() + row.title.slice(1) + ". A release relayer that finishes it may keep that much; you get the rest."
}

/// Which call plans the send-back: {method, args} or {error}. `args` are send_back_plan's four
/// strings. With the feature, every plan goes through `send_back_plan_with` (so the chosen fee,
/// 0 included, always wins over the code's own); without it a non-zero choice is refused, never
/// silently dropped.
export function releaseFeePlanCall(features, args, chosen) {
    var a = (Array.isArray(args) ? args : []).map(function(x) { return String(x) })
    var pick = choiceNumber(chosen)
    if (pick < 0) return { error: "Pick a fee first. Nothing was done." }
    if (releaseFeeCan(features))
        return { method: RELEASE_FEE_PLAN_CALL, args: a.concat([JSON.stringify({ relay_fee_choice: pick })]) }
    if (pick !== 0) return { error: RELEASE_FEE_OLD_BRIDGE + " Nothing was done." }
    return { method: RELEASE_FEE_PLAN_OLD, args: a }
}
