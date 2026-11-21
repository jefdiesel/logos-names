// Vesting (the left menu's Vesting page): schedules made in Vest that pay this wallet's public
// accounts, read from the optional `vesting` module (Vest's own module; basecamp/lidl/vesting.lidl).
// Names only shows them; claiming happens in Vest. Kept here so `node basecamp/vesting.test.mjs`
// checks the rules without a QML engine. Words: "LNS names", never "official", never "lookup".

const ID = /^[1-9A-HJ-NP-Za-km-z]{32,44}$/

/// The bare account IDs `incoming` takes, from `Public/<id>` texts (Logic.publicAccounts): each
/// once, in order, anything that isn't a base58 account ID left out.
export function accountIds(accounts) {
    var out = []
    ;(Array.isArray(accounts) ? accounts : []).forEach(function(a) {
        var id = String(typeof a === "string" ? a : (a && a.account) || "").replace(/^Public\//, "")
        if (ID.test(id) && out.indexOf(id) < 0)
            out.push(id)
    })
    return out
}

const MISSING = /module not connected|module not reachable|no such module|unknown module|module not found|not loaded|is not installed/i
const NO_METHOD = /method not found|unknown[_ ]method|no such method|has no method|not a method/i
const BUSY = /timed? ?out|timeout|busy/i

/// A raw `vesting` reply, sorted: `{kind, value, error}`. The module answers JSON text
/// `{"ok":true,...}` or `{"ok":false,"error":...}`, which Basecamp may encode once or twice more.
/// `kind`: "ok" (`value` is the module's answer), "missing" (no Vest module here, or not running),
/// "nomethod" (a Vest module without this call: an older Vest), "busy" (no answer yet: it may be
/// working on an earlier call; asked again on the next beat), "error" (its own words, `error`).
export function reply(raw) {
    if (raw === undefined || raw === null || String(raw) === "")
        return { kind: "busy", value: null, error: "" }
    var parsed = raw
    for (var depth = 0; depth < 3 && typeof parsed === "string"; depth++) {
        try { parsed = JSON.parse(parsed) } catch (e) { break }
    }
    // A host envelope around the module's text: what it holds is read the same way.
    if (parsed && typeof parsed === "object" && typeof parsed.success === "boolean") {
        if (parsed.success)
            return reply(parsed.value === undefined || parsed.value === null ? "" : typeof parsed.value === "string" ? parsed.value : JSON.stringify(parsed.value))
        parsed = { error: parsed.error }
    }
    if (parsed && typeof parsed === "object" && typeof parsed.ok === "boolean") {
        if (parsed.ok)
            return { kind: "ok", value: parsed, error: "" }
        var why = errorText(parsed.error)
        return { kind: NO_METHOD.test(why) ? "nomethod" : "error", value: null, error: why }
    }
    // The host's own words: {"error": "Call refused", "message": "…"} or plain text.
    var said = parsed && typeof parsed === "object" ? errorText(parsed.message || parsed.error) + " " + errorText(parsed.message ? parsed.error : "")
        : String(parsed)
    if (MISSING.test(said)) return { kind: "missing", value: null, error: said.trim() }
    if (NO_METHOD.test(said)) return { kind: "nomethod", value: null, error: said.trim() }
    if (BUSY.test(said) || !(parsed && typeof parsed === "object") || said.trim() === "") return { kind: "busy", value: null, error: said.trim() }
    return { kind: "error", value: null, error: "Basecamp refused the call: " + errorText(parsed.message || parsed.error) }
}

function errorText(e) {
    if (e === undefined || e === null) return ""
    if (typeof e === "string") return e
    if (typeof e === "object" && typeof e.message === "string") return e.message
    return JSON.stringify(e)
}

/// A schedule's group: "claimable", "finished" (the module's `closed`) or "unvested" (anything else
/// the module says: nothing to claim yet).
export function groupOf(s) {
    var st = String(s && s.status || "")
    return st === "claimable" ? "claimable" : st === "closed" ? "finished" : "unvested"
}

/// When the next part unlocks, in ms (0: not given). A real, never an int (QML).
export function unlockMs(s) {
    var n = Number(s && s.next_unlock_ms)
    return isFinite(n) && n > 0 ? n : 0
}

/// The schedules in their three groups, each schedule once (by address): Claimable now, Unvested
/// (the soonest unlock first, those without a date last) and Finished.
export function groups(schedules) {
    var out = { claimable: [], unvested: [], finished: [] }
    var seen = {}
    ;(Array.isArray(schedules) ? schedules : []).forEach(function(s) {
        if (!s || typeof s !== "object") return
        var key = String(s.address || "")
        if (key !== "" && seen[key]) return
        seen[key] = true
        out[groupOf(s)].push(s)
    })
    out.unvested.sort(function(a, b) {
        var x = unlockMs(a), y = unlockMs(b)
        return x === y ? 0 : x === 0 ? 1 : y === 0 ? -1 : x - y
    })
    return out
}

/// The menu's count: schedules with something to claim now, and unvested ones not yet seen on the
/// Vesting page (`seen`: their addresses). 0 shows no badge.
export function badgeCount(schedules, seen) {
    var known = Array.isArray(seen) ? seen : []
    var g = groups(schedules)
    return g.claimable.length + g.unvested.filter(function(s) { return known.indexOf(String(s.address || "")) < 0 }).length
}

/// The badge as drawn: "" for none, "99+" past 99.
export function badgeText(n) {
    var k = Number(n) || 0
    return k <= 0 ? "" : k > 99 ? "99+" : String(k)
}

/// `seen` with every listed unvested and claimable schedule added (the page was looked at).
export function seenWith(seen, schedules) {
    var out = (Array.isArray(seen) ? seen : []).slice()
    var g = groups(schedules)
    g.claimable.concat(g.unvested).forEach(function(s) {
        var a = String(s.address || "")
        if (a !== "" && out.indexOf(a) < 0) out.push(a)
    })
    return out
}

/// Whether Vest's scan is behind the chain's tip (`discover` is then asked for more blocks).
export function behind(answer) {
    if (!answer || typeof answer !== "object") return false
    var to = Number(answer.to_block), tip = Number(answer.tip)
    return isFinite(to) && isFinite(tip) && tip > 0 && to < tip
}

function grouped(n) {
    return String(Math.floor(Number(n) || 0)).replace(/\B(?=(\d{3})+(?!\d))/g, ",")
}

/// The scan's line while Vest reads blocks: "Looking for schedules… block 41,200 of 52,310"; "" when
/// it is caught up.
export function scanLine(answer) {
    return behind(answer) ? "Looking for schedules… block " + grouped(answer.to_block) + " of " + grouped(answer.tip) : ""
}

/// Whether a display amount is more than nothing ("0", "0 LEZ", "0.000" are nothing).
export function someAmount(text) {
    return /[1-9]/.test(String(text || "").replace(/[^0-9.,]/g, ""))
}

/// What a row says under its amount, in words. `dateOf(ms)`: the local date and time (QML's).
/// `{amount, line}`: the total, and claimable / next unlock / finished.
export function rowLines(s, dateOf) {
    var shown = function(t) { var v = String(t === undefined || t === null ? "" : t).trim(); return v === "" ? "0" : v }
    var total = shown(s && s.total_display)
    var claimed = shown(s && s.claimed_display)
    var g = groupOf(s)
    var at = unlockMs(s)
    var when = at > 0 && typeof dateOf === "function" ? dateOf(at) : ""
    if (g === "claimable") {
        var line = "Claimable now: " + shown(s.claimable_display)
        if (someAmount(claimed)) line += " · claimed so far: " + claimed
        if (when !== "") line += " · next unlock " + when
        return { amount: total, line: line }
    }
    if (g === "finished")
        return { amount: total, line: someAmount(claimed) ? "Finished · claimed " + claimed : "Finished · nothing left to claim" }
    return { amount: total, line: when !== "" ? "Nothing to claim until " + when : "Nothing to claim yet" + (someAmount(claimed) ? " · claimed so far: " + claimed : "") }
}

/// What the page shows. `s`: {net ("0.4" or not), installed (true, false, or null: not known),
/// kind (the last `incoming` reply's kind, "" before any), answered (an answer was read before:
/// a busy read then keeps showing it), accounts (how many are asked about), accountsKnown (false
/// while the wallet's accounts are still being read), count (schedules
/// listed)}. One of: "network", "missing", "idle" (installed, its module not running), "older",
/// "noaccounts", "looking" (first read on its way), "busy" (no answer yet), "problem", "list",
/// "empty".
export function pageState(s) {
    var st = s || {}
    if (st.net !== "0.4") return "network"
    if (st.installed === false) return "missing"
    if (st.kind === "missing") return st.installed === true ? "idle" : "missing"
    if (st.kind === "nomethod") return "older"
    if (!(st.accounts > 0)) return st.accountsKnown === false ? "looking" : "noaccounts"
    if (st.kind === "error") return "problem"
    if (st.kind === "") return "looking"
    if (st.kind === "busy" && !st.answered) return "busy"
    return st.count > 0 ? "list" : "empty"
}

/// The plain line for a page state that isn't the list.
export function stateText(state) {
    switch (state) {
    case "network": return "Vest schedules are on LEZ 0.4. Switch to it in Settings → Network to see them here."
    case "missing": return "Vest finds schedules that pay you. Install Vest from the app catalog to see them here."
    case "idle": return "Vest is installed, but it isn't running yet. Open Vest once to start it."
    case "busy": return "Vest hasn't answered yet; it may be busy with a claim. Names asks again every minute."
    case "older": return "This Vest can't list schedules for Names yet. Update Vest from the app catalog to see them here."
    case "noaccounts": return "Names lists schedules that pay your public accounts, and this wallet has none yet."
    case "looking": return "Checking for schedules that pay you…"
    case "empty": return "No schedule pays your accounts yet. When someone makes one for you in Vest, it shows here by itself."
    }
    return ""
}
