// The wallet's bridge card kept tidy (Jef, 21 Nov: "a ton of older bridge tokens … not useful",
// "the claims should show but collapsible", two withdrawals he can't use): which bridged tokens
// fold away, what a folded panel's header says, which withdrawals the bridge says can't be
// finished, and the withdrawals hidden from the card. Pure functions, tested in tidy.test.mjs.
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

/// The bridges whose tokens always stay on the card: the one new receives and sends use
/// (`chosen`: the LEZ Bridge's config().deployment.bridge_program, "" when it doesn't say), and the
/// newest one the wallet reads (`read`: bridge_net_balances' read.bridges, [{program}]): the
/// newest known one by BRIDGE_ORDER, else the last one listed. [] when neither is known.
export function currentBridges(chosen, read) {
    var out = []
    var mine = String(chosen || "")
    if (mine !== "")
        out.push(mine)
    var listed = asList(read).map(function(b) { return b && b.program ? String(b.program) : "" }).filter(function(p) { return p !== "" })
    var newest = ""
    var rank = -1
    listed.forEach(function(p) {
        var at = BRIDGE_ORDER.indexOf(p)
        if (at > rank) { rank = at; newest = p }
    })
    if (newest === "" && listed.length > 0)
        newest = listed[listed.length - 1]
    if (newest !== "" && out.indexOf(newest) < 0)
        out.push(newest)
    return out
}

/// Whether a card asset row (logic.mjs bridgeNetRows) holds anything: its base units aren't 0.
function holds(asset) {
    var base = String(asset && asset.base !== undefined ? asset.base : "")
    return base !== "" && !/^0*$/.test(base)
}

/// One account's card rows split in two: `shown` (native LEZ, every token of a current bridge,
/// anything held, a token whose bridge isn't known) and `folded` (an older bridge's token with
/// nothing in it). `current`: currentBridges. With no current bridge known nothing folds.
export function tokensFolded(assets, current) {
    var list = asList(assets)
    var keep = asList(current)
    if (keep.length === 0)
        return { shown: list, folded: [] }
    var shown = [], folded = []
    list.forEach(function(a) {
        var bridge = a && a.bridge ? String(a.bridge) : ""
        var older = bridge !== "" && keep.indexOf(bridge) < 0
        if (older && !holds(a)) folded.push(a)
        else shown.push(a)
    })
    return { shown: shown, folded: folded }
}

/// The one quiet line for an account's folded tokens: "Older bridge tokens · 10", "" for none.
export function olderLine(count) {
    var n = Number(count) || 0
    return n > 0 ? "Older bridge tokens · " + n : ""
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

/// The line under the panels for hidden withdrawals: "1 hidden withdrawal" / "2 hidden withdrawals".
export function hiddenLine(count) {
    var n = Number(count) || 0
    return n === 0 ? "" : n + (n === 1 ? " hidden withdrawal" : " hidden withdrawals")
}

/// A kept "yes"/"" setting as a bool, and back.
export function settingOn(value) {
    return String(value || "") === "yes"
}
