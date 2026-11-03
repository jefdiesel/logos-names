// The Faucet mini-app's own logic, kept apart from the QML so the Node test
// (../../logic.test.mjs) runs the same code the app does. The QML engine imports this
// as an ECMAScript module: `import "logic.mjs" as Logic`.
//
// The faucet protocol is the Names faucet's (chain/faucet.py, and the names module's
// `cli_faucet`): POST {"account": "<base58>"} → 200 {account, amount, tx}, or an HTTP
// error with {"error": "<reason>"}. GET on the same URL → {amount, per_account}.

/// The Names faucet: run by the Logos Names project for the LEZ testnet. Optional.
export const DEFAULT_FAUCET_URL = "https://lez.buggers.online/faucet"
const MINUTE = 60 * 1000
const HOUR = 60 * MINUTE
const DAY = 24 * HOUR

/// Why this faucet URL is refused, or "": https anywhere, http only to this computer.
export function urlProblem(url) {
    var m = /^(https?):\/\/(\[[0-9a-fA-F:]+\]|[^\/:?#@\s]+)(:\d{1,5})?(\/[^\s]*)?$/.exec(String(url).trim())
    if (!m) return "The faucet address is an https URL, such as " + DEFAULT_FAUCET_URL + "."
    var host = m[2].toLowerCase()
    var local = host === "localhost" || host === "[::1]" || /^127\.\d+\.\d+\.\d+$/.test(host)
    if (m[1] === "http" && !local) return "Use https for a faucet on another computer (http only for one on this computer)."
    return ""
}

/// The host a URL talks to, for saying which service is contacted.
export function hostOf(url) {
    var m = /^https?:\/\/(\[[^\]]+\]|[^\/:?#@\s]+)/.exec(String(url).trim())
    return m ? m[1].toLowerCase() : ""
}

/// The granted accounts a faucet can pay: public ones (it sends a public transfer).
export function fundable(granted) {
    return (granted || []).map(function(entry) { return typeof entry === "string" ? entry : entry.account })
        .filter(function(account) { return /^Public\/[1-9A-HJ-NP-Za-km-z]{32,44}$/.test(String(account)) })
}

/// `Public/<id>` → `<id>`, what the faucet is sent.
export function bare(account) { return String(account).replace(/^Public\//, "") }

/// A Retry-After header (seconds, or an HTTP date) as a time in ms, or 0.
export function retryAfter(header, now) {
    var text = String(header || "").trim()
    if (text === "") return 0
    if (/^\d+$/.test(text)) return now + parseInt(text, 10) * 1000
    var at = Date.parse(text)
    return isNaN(at) ? 0 : Math.max(now, at)
}

/// What a faucet answer means for the person.
///
/// `reply` is the backend's answer: {status, body, retry_after, error}. `lastGrant` is when
/// this app last saw this account funded (ms, 0 if never). Returns
/// {ok, amount, tx, message, retryAt, retryText, final}: `retryAt` is when asking again
/// can work (0 if unknown), `final` that asking again won't help soon.
export function interpret(reply, now, lastGrant) {
    var status = reply && typeof reply.status === "number" ? reply.status : 0
    var body = reply && reply.body && typeof reply.body === "object" ? reply.body : {}
    var said = typeof body.error === "string" ? body.error : ""
    var hinted = retryAfter(reply ? reply.retry_after : "", now)
    var out = { ok: false, amount: "", tx: "", message: "", retryAt: hinted, retryText: "", final: false }
    if (status === 0) {
        out.message = "Couldn't reach the faucet" + (reply && reply.error ? " (" + reply.error + ")" : "") + ". Check your connection and the faucet address."
        out.retryAt = out.retryAt || now + MINUTE
    } else if (status >= 200 && status < 300) {
        if (typeof body.tx === "string" && body.tx !== "") {
            out.ok = true
            out.amount = String(body.amount || "")
            out.tx = body.tx
            out.message = "The faucet sent " + (out.amount ? grouped(out.amount) + " " : "") + "LEZ. It arrives within a block or two."
        } else out.message = "The faucet sent an unreadable answer."
    } else if (status === 429) {
        if (/funded in the last day/.test(said)) {
            out.message = "This account was funded in the last day; the faucet pays each account once a day."
            if (!out.retryAt && lastGrant > 0) out.retryAt = lastGrant + DAY
            if (!out.retryAt) out.retryText = "about a day after its last grant"
        } else if (/address/.test(said)) {
            out.message = "Your internet address was given tokens in the last day; the faucet pays each address once a day."
            if (!out.retryAt) out.retryText = "about a day after that grant"
        } else if (/daily limit/.test(said)) {
            out.message = "The faucet has given out all it gives in a day."
            if (!out.retryAt) out.retryText = "in a few hours, or tomorrow"
        } else {
            out.message = "The faucet is limiting requests" + (said ? ": " + said : "") + "."
            if (!out.retryAt) out.retryText = "later"
        }
    } else if (status === 409 && /empty/.test(said)) {
        out.message = "The faucet is empty until its next refill. You can bridge tokens from the Logos Blockchain in the Names app instead."
        out.final = true
    } else if (status === 409 || status === 422 || status === 502 || status === 503 || status === 504) {
        out.message = said ? sentence(said) : "The faucet is busy."
        out.retryAt = out.retryAt || now + MINUTE
    } else if (status === 400) {
        out.message = said ? sentence(said) : "The faucet refused the account."
        out.final = true
    } else if (status === 404) {
        out.message = "There is no faucet at that address."
        out.final = true
    } else {
        out.message = "The faucet answered with an error (HTTP " + status + ")" + (said ? ": " + said : "") + "."
        out.retryAt = out.retryAt || now + MINUTE
    }
    if (out.retryAt && !out.retryText) out.retryText = waitText(out.retryAt - now, out.retryAt)
    return out
}

/// "in 40 s", "in 12 min", "in 3 h 5 min (at 14:05)".
export function waitText(ms, at) {
    if (ms <= 0) return "now"
    if (ms < MINUTE) return "in " + Math.ceil(ms / 1000) + " s"
    if (ms < HOUR) return "in " + Math.ceil(ms / MINUTE) + " min"
    var h = Math.floor(ms / HOUR), m = Math.ceil((ms - h * HOUR) / MINUTE)
    var when = new Date(at)
    return "in " + h + " h" + (m > 0 ? " " + m + " min" : "") + " (at " + pad(when.getHours()) + ":" + pad(when.getMinutes()) + ")"
}

function pad(n) { return (n < 10 ? "0" : "") + n }

function sentence(text) {
    var t = String(text).trim()
    t = t.charAt(0).toUpperCase() + t.substr(1)
    return /[.?!]$/.test(t) ? t : t + "."
}

/// What the faucet says about itself (GET): {amount, per_account}, or null.
export function describe(reply) {
    var body = reply && reply.status === 200 && reply.body && typeof reply.body === "object" ? reply.body : null
    if (!body) return null
    return { amount: /^\d+$/.test(String(body.amount)) ? String(body.amount) : "", per_account: typeof body.per_account === "string" ? body.per_account : "" }
}

/// Whether the decimal `a` is at least `b` (balances are 128-bit decimal text).
export function atLeast(a, b) {
    a = String(a).replace(/^0+/, "")
    b = String(b).replace(/^0+/, "")
    return a.length !== b.length ? a.length > b.length : a >= b
}

/// Whether the decimal `a` is more than `b`.
export function above(a, b) { return atLeast(a, b) && String(a).replace(/^0+/, "") !== String(b).replace(/^0+/, "") }

/// 2000000 → "2,000,000".
export function grouped(digits) {
    var s = String(digits)
    return /^\d+$/.test(s) ? s.replace(/\B(?=(\d{3})+(?!\d))/g, ",") : s
}

/// The balance `account_balances` reports for `account`, or null.
export function balanceOf(balances, account) {
    for (var i = 0; i < (balances || []).length; i++)
        if (balances[i].account === account) return balances[i].balance
    return null
}

/// `abcdef…wxyz`.
export function short(text) {
    var s = String(text)
    return s.length <= 14 ? s : s.substr(0, 6) + "…" + s.substr(s.length - 4)
}
