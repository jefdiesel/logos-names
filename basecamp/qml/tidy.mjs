// The wallet's bridge card kept tidy (Jef, 21 Nov: "a ton of older bridge tokens … not useful",
// "the claims should show but collapsible", two withdrawals he can't use): which bridged tokens
// fold away, what a folded panel's header says, which withdrawals the bridge says can't be
// finished, and the withdrawals hidden from the card. Since 22 Nov ("hide old vault deposits. only
// show current vault"): only the current bridge's tokens, receives and withdrawals stay on the
// card; an older bridge's that still need the person go behind one "Older bridges" line, the rest
// are left out. Pure functions, tested in tidy.test.mjs.
// Basecamp's QML engine: no BigInt, no Java reserved words as names.

/// The bridges the wallet knows, oldest first (logic.mjs BRIDGE_LABELS; the module's
/// bridge_net::BRIDGE_LABELS): v2 to v7.
export const BRIDGE_ORDER = [
    "JECKBkvoc2e3dC18Wm6zZJEC2BGMwUwLsdZiQzYTnUx7",
    "4eaeK3yiqATXhpCEYEFMMtjUn1bS7HLxPwfrH31mXCvr",
    "BKXe3q5JcKeiSrKv9FUBjZw3AowbvYgyHZ3SYPgDxJBf",
    "CuV1ctZ2qJG4wQcXP4TBudVGKzNmdoYT3DpP1veHnDMA",
    "4PxaLAfPZDvpfZaR8SsC2Y7pYMPNwPPGVGMpjEVPQWed",
    "CdijfhGnf1NjyCvYTZcGijQZzZ6soNJQ75Piex1ZTM93"
]

/// A list as QML hands it over: a JS array, or a sequence (a Repeater's modelData turns a nested
/// array into one, which Array.isArray doesn't take: the card showed no tokens at all); else [].
export function asList(x) {
    if (Array.isArray(x))
        return x
    if (x && typeof x === "object" && typeof x.length === "number")
        return Array.prototype.slice.call(x)
    return []
}

/// The bridges whose vault can never release (rfp-022 vault/DEPLOYMENTS.md): v2 (its update
/// image is a placeholder) and v3 (deployed with a zero G2 state, so it accepts no update). Their
/// tokens can never go back to Ethereum.
export const NEVER_RELEASE = BRIDGE_ORDER.slice(0, 2)

/// The current bridge (Jef, 22 Nov: "only show current vault"): the one the LEZ Bridge itself uses
/// for new receives and sends (`chosen`: its config().deployment.bridge_program), as a list. []
/// while that isn't known (not read yet, or a LEZ Bridge that doesn't say): then nothing is older.
export function currentBridges(chosen) {
    var mine = String(chosen || "")
    return mine !== "" ? [mine] : []
}

/// Whether a card asset row (logic.mjs bridgeNetRows) holds anything: its base units aren't 0.
function holds(asset) {
    var base = String(asset && asset.base !== undefined ? asset.base : "")
    return base !== "" && !/^0*$/.test(base)
}

/// One account's card rows split: `shown` (native LEZ, every token of the current bridge, a token
/// whose bridge isn't said) and `folded` (an older bridge's token that still holds something and
/// can still go back to Ethereum: behind the "Older bridges" line). An older bridge's token with
/// nothing in it, or on a bridge that can never release (NEVER_RELEASE), is left out. `current`:
/// currentBridges. With no current bridge known, everything is shown.
export function tokensFolded(assets, current) {
    var list = asList(assets)
    var keep = asList(current)
    if (keep.length === 0)
        return { shown: list, folded: [] }
    var shown = [], folded = []
    list.forEach(function(a) {
        var bridge = a && a.bridge ? String(a.bridge) : ""
        if (bridge === "" || keep.indexOf(bridge) >= 0) shown.push(a)
        else if (holds(a) && NEVER_RELEASE.indexOf(bridge) < 0) folded.push(a)
    })
    return { shown: shown, folded: folded }
}

/// Whether a card row (a receive or a withdrawal) was made on an older bridge: the LEZ Bridge says
/// its deployment isn't the current one. A row that doesn't say counts as current.
export function isOlder(row) {
    var d = row && row.deployment && typeof row.deployment === "object" ? row.deployment : null
    return d !== null && d.current === false
}

/// The "Arriving from Ethereum" rows split: `rows` (the current bridge's) and `older` (an older
/// bridge's deposit still on its way or arrived: behind the "Older bridges" line). An older
/// bridge's code nothing was deposited for, a claimed one or one that can't be followed is left out.
export function arrivingSplit(rows) {
    var out = { rows: [], older: [] }
    asList(rows).forEach(function(x) {
        if (!isOlder(x)) out.rows.push(x)
        else if (x.progress && ["deposited", "final", "claimable", "noclaim"].indexOf(x.progress.stage) >= 0) out.older.push(x)
    })
    return out
}

/// The "Bridging to Ethereum" rows split: `rows` (the current bridge's) and `older` (an older
/// bridge's withdrawal not received yet that can still be finished: behind the "Older bridges"
/// line, and still followed). An older one received, or one the bridge says can't be finished
/// (deadReason), is left out. `statuses`, `said`: as bridgingReady.
export function bridgingSplit(rows, statuses, said) {
    var out = { rows: [], older: [] }
    asList(rows).forEach(function(x) {
        if (!isOlder(x)) { out.rows.push(x); return }
        var st = statuses ? statuses[x.id] : null
        var done = x.progress && x.progress.stage === "released"
        if (!done && deadReason(st, said ? said[x.id] : "") === "") out.older.push(x)
    })
    return out
}

/// The one quiet line for what an older bridge still holds: "Older bridges · 2", "" for none.
export function olderLine(count) {
    var n = Number(count) || 0
    return n > 0 ? "Older bridges · " + n : ""
}

/// What a panel's header says beside its title: "" while open; folded, how many rows it holds and
/// how many need the person ("2", "2 · 1 ready to claim", or "1 ready to claim" when that is
/// all of them). `readyWords`: "ready to claim" or
/// "ready to finish".
export function foldedNote(folded, count, ready, readyWords) {
    if (!folded)
        return ""
    var n = Number(count) || 0, r = Number(ready) || 0
    if (r > 0 && r >= n)
        return r + " " + readyWords
    return r > 0 ? n + " · " + r + " " + readyWords : String(n)
}

/// How many "Arriving from Ethereum" rows have their Claim ready (logic.mjs receiveRowsShown rows).
export function arrivingReady(rows) {
    return asList(rows).filter(function(x) { return x && x.progress && x.progress.claim === true }).length
}

/// How many "Bridging to Ethereum" rows wait on the person (approve the burn, make the proof,
/// finish it, collect it: logic.mjs sendBackProgress `tone: "ready"`), dead ones left out.
export function bridgingReady(rows, statuses, said) {
    return asList(rows).filter(function(x) {
        return x && x.progress && x.progress.tone === "ready" && deadReason(statuses ? statuses[x.id] : null, said ? said[x.id] : "") === ""
    }).length
}

// ── Withdrawals that can't be finished ──
//
// Only what the LEZ Bridge itself says decides it (rfp-022 modules/lez_bridge sendback.rs): a
// withdrawal whose root is on Ethereum, whose vault isn't frozen and whose release secret the
// wallet still makes can always be finished, whichever bridge it was made on.

/// The bridge's words that mean it can't be finished, and the one line the card says for each.
export const DEAD_SAID = [
    // sb_witness: the wallet's exit secret no longer gives this withdrawal's exit commitment, so no
    // release proof can be made for it.
    [/gives another exit secret than this send-back was made with/i,
     "Can't be finished: your wallet no longer makes the secret this withdrawal was made with."],
    // sb_dep: made on a bridge the LEZ Bridge no longer lists; nothing about it can be read.
    [/a bridge this app doesn't list any more/i,
     "Can't be followed: it was made on a bridge the LEZ Bridge no longer lists (Settings → Bridge)."]
]

/// Why a withdrawal can't be finished, as one line, or "" when nothing says so. `status`: its last
/// send_back_status (its `note` is the bridge's reason a read stopped; `update.dead`: a v7 gate
/// that will never pass its update); `said`: what the bridge answered the last press on it (a
/// failed Finish or Make the proof).
export function deadReason(status, said) {
    var s = status && typeof status === "object" ? status : {}
    if (s.stage === "released")
        return ""
    var words = [String(s.note || ""), String(said || "")]
    for (var i = 0; i < DEAD_SAID.length; i++) {
        if (words.some(function(w) { return DEAD_SAID[i][0].test(w) }))
            return DEAD_SAID[i][1]
    }
    if (s.update && typeof s.update === "object" && s.update.dead === true)
        return "Can't be finished: the bridge says Ethereum will never record this burn."
    return ""
}

/// Whether the bridge's answer to a press means the withdrawal can't be finished.
export function deadSaid(message) {
    return deadReason(null, message) !== ""
}

// ── Withdrawals hidden from the card (Hide), kept in the app settings as JSON text ──

/// The most ids kept: older ones go first.
export const HIDDEN_MAX = 64

/// The hidden ids from the kept text ("" or junk: none). Only send-back ids ("sb-" and hex).
export function hiddenRead(text) {
    var list
    try { list = JSON.parse(String(text || "[]")) } catch (error) { return [] }
    if (!Array.isArray(list))
        return []
    var out = []
    list.forEach(function(id) {
        var s = String(id)
        if (/^sb-[0-9a-f]{1,64}$/.test(s) && out.indexOf(s) < 0)
            out.push(s)
    })
    return out.slice(-HIDDEN_MAX)
}

/// The list with `id` hidden (`hide`) or shown again; the kept text is JSON.stringify of it.
export function hiddenWith(list, id, hide) {
    var now = asList(list).filter(function(x) { return x !== id })
    if (hide)
        now.push(String(id))
    return now.slice(-HIDDEN_MAX)
}

/// The "Bridging to Ethereum" rows split: `rows` shown, `hidden` how many are hidden. With
/// `showHidden` every row is shown (each hidden one then offers Show again).
export function visibleRows(rows, hidden, showHidden) {
    var list = asList(rows)
    var away = asList(hidden)
    var kept = list.filter(function(x) { return away.indexOf(x.id) < 0 })
    return { rows: showHidden ? list : kept, hidden: list.length - kept.length }
}

/// Whether a hidden withdrawal is still followed from the card: while it is in flight (signed and
/// waiting for the bridge's relay, or its burn on its way) its status check is what moves it on
/// (rfp-022 sendback.rs: send_back_status hands a signed burn to the relay), so hiding it must
/// not stop that. Every other hidden one isn't asked about until it is shown or opened.
export function followedWhenHidden(stage) {
    return stage === "approve" || stage === "burning"
}

/// The line under the panels for hidden withdrawals: "1 hidden withdrawal" / "2 hidden withdrawals".
export function hiddenLine(count) {
    var n = Number(count) || 0
    return n === 0 ? "" : n + (n === 1 ? " hidden withdrawal" : " hidden withdrawals")
}

/// A kept "yes"/"" setting as a bool, and back.
export function settingOn(value) {
    return String(value || "") === "yes"
}
