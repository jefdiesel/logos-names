// The Names Wallet SDK: the connector a Basecamp ui_qml app copies in to use the person's Names
// Wallet (the `names` module's Wallet Provider SDK, docs/wallet/PROVIDER-SDK.md).
//
// Copy this file and NamesWallet.qml (the small QML side: Basecamp's bridge and one timer per
// wait) into your app's qml/ folder, unchanged, as the design kit is copied. Then:
//
//     NamesWallet { id: names; bridge: typeof logos !== "undefined" ? logos : null }
//     names.check({}, function(w) { ... w.state, w.accounts ... })
//     names.connect("Why your app asks", {}, { done: function(request) { ... } })
//     names.proposeCall(program, accountsJson, instructionHex, summary, { done: ..., failed: ... })
//     names.resolve("alice", function(r) { if (r.ok) use(r.account) })
//
// This file is plain JavaScript (no QML), so it runs under node for tests and in Qt's engine;
// it is written for both (var, no optional chaining, no spread). Nothing in it is about one app:
// an app's own wording stays in the app. Every argument it sends is text: Basecamp's call
// checker refuses a QML number for an integer parameter.

export const VERSION = "1.0.0"

/// Where Names Wallet stands for this app (`wallet.state`):
/// checking: not known yet · missing: Names isn't installed or open · otherZone: Names is on
/// another network than the app's · disconnected: no access yet · asking: a connect request
/// waits in Names · connected: the app was given accounts.
export const STATES = ["checking", "missing", "otherZone", "disconnected", "asking", "connected"]

/// The shared plain words. An app may show its own instead; these are the defaults.
export const WORDS = {
    missing: "Names Wallet isn't installed, or isn't open. Install Names from Basecamp's app list, open it once, then come back here.",
    otherZone: "Names Wallet is on another network. Switch it in Names (Settings, Network), then come back.",
    busy: "Names Wallet is busy.",
    noAnswer: "No answer from Names Wallet yet.",
    restarting: "Names Wallet isn't answering normally (it may be restarting). Try again in a moment.",
    uncertain: "Names Wallet didn't answer, so it may have taken the request. Look in Names before asking again.",
    noBridge: "This app runs inside Basecamp, with the Names app installed.",
    refused: "Names Wallet said no."
}

/// The calls that only read: asked again after a busy or unclear answer. `request_access`
/// and `drop_access` are here too, since asking twice gives the same answer (a waiting
/// request comes back as it is). A propose_* call is never sent twice on an unclear answer.
export const READS = ["get_config", "granted_accounts", "account_balances", "request_status", "resolve",
    "reverse", "recipient", "token_sources", "granted_nfts", "nft_membership", "nft_request_status",
    "membership_proof_status", "request_access", "request_access_with", "drop_access"]

/// How many times a busy or unclear answer is asked again, and the first wait (ms); each wait
/// is one step longer (1.5 s, 3 s, 4.5 s, ...).
export const RETRIES = 5
export const RETRY_MS = 1500
/// How often a waiting request is read again (ms).
export const POLL_MS = 2000

function isObject(x) { return x !== null && typeof x === "object" }

/// Every argument as text.
export function textArgs(args) {
    return (args || []).map(function(a) {
        if (a === null || a === undefined) return ""
        if (typeof a === "string") return a
        if (typeof a === "number" || typeof a === "boolean") return String(a)
        return JSON.stringify(a)
    })
}

/// One answer of the names module as `{ok, value}` or `{ok: false, error, ...}`:
/// `busy` (nothing happened: ask again), `missing` (Names isn't installed or open),
/// `unclear` (no answer, or not the SDK's shape: Names may be restarting; the call may or may
/// not have been taken) or a refusal with Names' own words. Basecamp may hand the module's JSON
/// text encoded once more.
export function reply(raw) {
    var parsed = raw
    for (var depth = 0; depth < 2 && typeof parsed === "string"; depth++) {
        try { parsed = JSON.parse(parsed) } catch (error) { break }
    }
    if (isObject(parsed) && parsed.busy === true) return { ok: false, busy: true, error: WORDS.busy }
    if (isObject(parsed) && parsed.success !== undefined)
        return parsed.success ? { ok: true, value: parsed.value } : { ok: false, error: String(parsed.error || WORDS.refused) }
    if (raw === undefined || raw === null || String(raw) === "") return { ok: false, unclear: true, error: WORDS.noAnswer }
    var said = typeof raw === "string" ? raw : JSON.stringify(raw)
    if (/not connected|not reachable|no such|unknown (module|method)|method not found|not found|not loaded|not installed|call refused/i.test(said || ""))
        return { ok: false, missing: true, error: WORDS.missing }
    return { ok: false, unclear: true, error: WORDS.restarting }
}

/// Whether a request is still open (keep reading it).
export function requestOpen(request) {
    var s = (request || {}).state
    return s === "pending" || s === "running"
}

/// What a request's state means, in one line ("" while there is nothing to say). `what` is
/// the thing asked for ("the payment"); default "the request".
export function requestLine(request, what) {
    var r = request || {}
    var thing = what || "the request"
    if (r.state === "pending") return "Waiting for you to approve " + thing + " in Names Wallet."
    if (r.state === "running") return "Names Wallet is sending " + thing + "…"
    if (r.state === "rejected") return "You declined " + thing + " in Names Wallet. Nothing was sent."
    if (r.state === "expired") return "Nobody approved " + thing + " within 10 minutes, so Names Wallet dropped it. Nothing was sent."
    if (r.state === "cancelled") return "You withdrew " + thing + ". Nothing was sent."
    if (r.state === "failed") return "Names Wallet couldn't send " + thing + (r.error ? ": " + r.error : "") + "."
    return ""
}

/// Whether `get_config`'s zone is the chain `channel` (64 hex) the app is built for. True when
/// either is unknown: then there is nothing to compare.
export function zoneMatches(config, channel) {
    var zone = config && config.zone ? config.zone : null
    var have = zone && zone.channel ? String(zone.channel).toLowerCase() : ""
    if (!channel || !have) return true
    return have === String(channel).toLowerCase()
}

/// The granted accounts with their last balances: `[{account, account_kind, balance}]`,
/// balance "0" until read.
export function withBalances(accounts, balances) {
    var b = {}
    ;(balances || []).forEach(function(x) { if (x && x.account) b[x.account] = x.balance })
    return (accounts || []).map(function(a) {
        return { account: a.account, account_kind: a.account_kind, balance: b[a.account] || "0" }
    })
}

/// A name's record (`resolve`'s value) as an app needs it: `{name, display, account, forward,
/// lapsed}`; `account` is the public account ID (base58) the name publishes, or null.
export function nameRecord(value) {
    var v = value || {}
    return {
        name: String(v.name || ""), display: String(v.display || v.name || ""),
        account: v.account ? String(v.account) : null,
        forward: v.forward ? String(v.forward) : null,
        lapsed: v.lapsed === true
    }
}

/// The SDK for one app. `host` gives what only QML has (NamesWallet.qml makes it):
///   call(method, args, callback(raw), timeoutMs) → false when there is no Basecamp bridge;
///   later(f, ms) runs f once after ms (one timer each); open() brings Names forward;
///   changed(wallet) after wallet.state, .accounts, .lastBalances, .features or .config change.
export function create(host) {
    var h = host || {}
    var wallet = {
        version: VERSION,
        state: "checking", problem: "", config: null, features: [], accounts: [], lastBalances: [],
        chain: ""
    }
    function changed() { if (h.changed) h.changed(wallet) }
    function setState(state, problem) {
        wallet.state = state
        wallet.problem = problem || ""
        changed()
    }
    function later(f, ms) { if (h.later) h.later(f, ms); else f() }

    /// One call of the names module. `done(value)` on success; `failed(r)` otherwise, with
    /// `r.error` in plain words and `r.busy`, `r.missing`, `r.unclear` or `r.uncertain` saying
    /// which. Busy is asked again (nothing happened); an unclear answer only for READS.
    wallet.call = function(method, args, done, failed, options) {
        var o = options || {}
        var no = failed || function() {}
        var tries = o.tries || 0
        var asked = h.call ? h.call(method, textArgs(args), function(raw) {
            var r = reply(raw)
            if (r.ok) { if (done) done(r.value); return }
            var read = READS.indexOf(method) >= 0
            if ((r.busy || (r.unclear && read)) && tries < RETRIES) {
                later(function() { wallet.call(method, args, done, failed, { tries: tries + 1, timeout: o.timeout }) }, RETRY_MS * (tries + 1))
                return
            }
            if (r.missing) setState("missing", WORDS.missing)
            if (r.unclear && !read) { r.uncertain = true; r.error = WORDS.uncertain }
            no(r)
        }, o.timeout || 30000) : false
        if (asked === false) { setState("missing", WORDS.missing); no({ ok: false, missing: true, error: WORDS.noBridge }) }
    }

    /// Whether this Names build has `feature` (from `get_config`, read by `check`).
    wallet.has = function(feature) { return (wallet.features || []).indexOf(feature) >= 0 }

    /// Where Names Wallet stands: its config and features, the zone (`options.chain`: the
    /// chain's channel, 64 hex, the app is built for), and the accounts given. `done(wallet)`.
    wallet.check = function(options, done) {
        var o = options || {}
        if (o.chain !== undefined) wallet.chain = String(o.chain || "")
        var end = done || function() {}
        wallet.call("get_config", [], function(config) {
            wallet.config = config
            wallet.features = (config && config.features) || []
            if (!zoneMatches(config, wallet.chain)) { setState("otherZone", WORDS.otherZone); end(wallet); return }
            wallet.call("granted_accounts", [], function(g) {
                wallet.accounts = (g && g.connected && g.accounts) || []
                if (wallet.accounts.length) setState("connected")
                else if (wallet.state !== "asking") setState("disconnected")
                else changed()
                end(wallet)
            }, function(r) { wallet.error = r.error; end(wallet) })
        }, function(r) { wallet.error = r.error; end(wallet) })
    }

    /// The LEZ balance of each account given: `done([{account, account_kind, balance, updating,
    /// error}])` (balance is decimal text, null until first read).
    wallet.balances = function(done, failed) {
        wallet.call("account_balances", [], function(b) {
            wallet.lastBalances = b || []
            changed()
            if (done) done(wallet.lastBalances)
        }, failed)
    }

    /// Bring the Names app forward (Basecamp's `basecamp.apps.launch`; declare it in
    /// metadata.json `uses`). Names shows the app again once the person answers.
    wallet.open = function() { if (h.open) h.open() }

    /// Read `request` (its id, or the request) every POLL_MS until it is no longer open.
    /// handlers: update(request) each read while open, done(request) at the end (approved,
    /// done, rejected, expired, cancelled or failed: see requestLine), failed(r) when Names
    /// refuses to say. Returns `{stop(), withdraw(done, failed)}`.
    wallet.watch = function(request, handlers) {
        var hs = handlers || {}
        var id = isObject(request) ? request.request : String(request || "")
        var watcher = { stopped: false, request: isObject(request) ? request : { request: id, state: "pending" } }
        function poll() {
            if (watcher.stopped) return
            wallet.call("request_status", [id], function(v) {
                if (watcher.stopped) return
                watcher.request = v
                if (requestOpen(v)) { if (hs.update) hs.update(v); later(poll, hs.every || POLL_MS); return }
                watcher.stopped = true
                if (hs.done) hs.done(v)
            }, function(r) {
                if (watcher.stopped) return
                watcher.stopped = true
                if (hs.failed) hs.failed(r)
            })
        }
        watcher.stop = function() { watcher.stopped = true }
        watcher.withdraw = function(done, failed) { wallet.withdraw(id, done, failed) }
        later(poll, hs.every || POLL_MS)
        return watcher
    }

    /// Withdraw a request while it is pending (`cancel_request`). The watch then ends `cancelled`.
    wallet.withdraw = function(requestId, done, failed) {
        wallet.call("cancel_request", [isObject(requestId) ? requestId.request : requestId], done, failed)
    }

    /// Connect: ask for access (`reason`, your words, up to 280 characters), take the person to
    /// Names, wait for the answer, then read the accounts given (`check`).
    /// options: accountHint ("Public/<id>", used with request_access_with when Names has it),
    /// open (default true). handlers: waiting(request) when it waits in Names and on each read,
    /// done(request) at the end (state "approved" when given; the wallet is checked again
    /// first), failed(r).
    wallet.connect = function(reason, options, handlers) {
        var o = options || {}
        var hs = handlers || {}
        var hint = o.accountHint ? String(o.accountHint) : ""
        var method = hint && wallet.has("request_access_with") ? "request_access_with" : "request_access"
        var args = method === "request_access_with" ? [reason, JSON.stringify({ account_hint: hint })] : [reason]
        var finish = function(v) {
            if (v && v.state === "approved") { wallet.check({}, function() { if (hs.done) hs.done(v) }); return }
            if (wallet.state === "asking") setState("disconnected")
            if (hs.done) hs.done(v)
        }
        wallet.call(method, args, function(v) {
            if (v && v.state === "approved") { finish(v); return }
            setState("asking")
            if (hs.waiting) hs.waiting(v)
            wallet.watch(v, { update: hs.waiting, done: finish, failed: function(r) {
                setState("disconnected")
                if (hs.failed) hs.failed(r)
            } })
            if (o.open !== false) wallet.open()
        }, function(r) { if (hs.failed) hs.failed(r) })
    }

    /// Give up the access given (`drop_access`).
    wallet.disconnect = function(done, failed) {
        wallet.call("drop_access", [], function(v) {
            wallet.accounts = []
            wallet.lastBalances = []
            setState("disconnected")
            if (done) done(v)
        }, failed)
    }

    /// Any propose_* call that answers a request: sends it, takes the person to Names
    /// (options.open, default true) and watches it. handlers: sent(request) once it waits in
    /// Names, update(request), done(request) (see requestLine for its state), failed(r).
    /// Returns nothing; `sent` gets the request and the watcher (`{stop, withdraw}`).
    wallet.propose = function(method, args, handlers, options) {
        var o = options || {}
        var hs = handlers || {}
        wallet.call(method, args, function(v) {
            var watcher = wallet.watch(v, hs)
            if (hs.sent) hs.sent(v, watcher)
            if (o.open !== false) wallet.open()
        }, function(r) { if (hs.failed) hs.failed(r) })
    }

    /// A program call signed by an account the person picks (`propose_call`).
    wallet.proposeCall = function(program, accountsJson, instructionHex, summary, handlers, options) {
        var accounts = typeof accountsJson === "string" ? accountsJson : JSON.stringify(accountsJson || [])
        wallet.propose("propose_call", [program, accounts, instructionHex, summary], handlers, options)
    }

    /// A payment (`propose_transfer`): `asset` "LEZ" (or a token definition), `amount` in base
    /// units as text, `destination` an LNS name or "Public/<id>".
    wallet.proposeTransfer = function(asset, amount, destination, handlers, options) {
        wallet.propose("propose_transfer", [asset || "LEZ", amount, destination], handlers, options)
    }

    /// An LNS name → the account it publishes, through Names' own `resolve`. A name that
    /// forwards (SPEC.md §7) is followed one hop, as a payment to it is. `done(r)`:
    ///   {ok: true, name, display, account, forwardedFrom, lapsed}   account: base58 public ID
    ///   {ok: false, code, name, error}   code: "not_registered", "no_account" (the name
    ///   publishes no account), "refused" (not a name: Names' words), "missing" or "unavailable"
    wallet.resolve = function(name, done) {
        var asked = String(name == null ? "" : name).trim()
        var end = done || function() {}
        var fail = function(r) {
            end({ ok: false, code: r.missing ? "missing" : (r.busy || r.unclear) ? "unavailable" : "refused", name: asked, error: r.error })
        }
        var read = function(n, from) {
            wallet.call("resolve", [n], function(v) {
                if (!v) { end({ ok: false, code: "not_registered", name: from || n, error: from ? from + " points to " + n + ", which isn't registered." : "No one holds the LNS name " + n + "." }); return }
                var rec = nameRecord(v)
                if (rec.forward && !from) { read(rec.forward, rec.name || n); return }
                if (!rec.account) { end({ ok: false, code: "no_account", name: rec.name || n, error: (rec.name || n) + " publishes no account to pay." }); return }
                end({ ok: true, name: rec.name || n, display: rec.display, account: rec.account, forwardedFrom: from || null, lapsed: rec.lapsed })
            }, fail)
        }
        read(asked, null)
    }

    /// An account ("Public/<id>" or the base58 ID) → the LNS name it set as its own, through
    /// Names' own `reverse` (checked against the name's record, so it can't be spoofed).
    /// `done(r)`: {ok: true, account, name, display} (name and display null when it set none),
    /// or {ok: false, code, account, error} as resolve's.
    wallet.reverse = function(account, done) {
        var asked = String(account == null ? "" : account).trim()
        var end = done || function() {}
        wallet.call("reverse", [asked], function(v) {
            end({ ok: true, account: asked, name: v && v.name ? String(v.name) : null, display: v && v.display ? String(v.display) : null })
        }, function(r) {
            end({ ok: false, code: r.missing ? "missing" : (r.busy || r.unclear) ? "unavailable" : "refused", account: asked, error: r.error })
        })
    }

    return wallet
}
