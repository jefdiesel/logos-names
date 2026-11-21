// The Names Wallet SDK: the connector a Basecamp ui_qml app copies in to use the person's
// wallet: Names Wallet, or any wallet that implements this SDK (docs/wallet/PROVIDER-SDK.md,
// "Implementing the SDK in another wallet"). The app never names a wallet: `getWallet` finds
// the one it uses among the installed packages, and every call goes there.
//
// Copy this file and NamesWallet.qml (the small QML side: Basecamp's bridge, one timer per
// wait, the remembered choice) into your app's qml/ folder, unchanged, as the design kit is
// copied. Then:
//
//     NamesWallet { id: names; app: "my_app_ui"; bridge: typeof logos !== "undefined" ? logos : null }
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
/// The SDK's calls and reply shapes, as a wallet declares it implements them
/// (`get_config().wallet_sdk.version`). A wallet speaking the same major version works.
export const SDK_NAME = "names-wallet"
export const SDK_VERSION = "1.0"
/// The wallet used when it is installed and nothing else is chosen; also the one assumed when
/// Basecamp's package list can't be read. Names 0.18 predates the declaration.
export const DEFAULT_WALLET = { module: "names", app: "names_ui", display_name: "Names" }

/// Where the wallet stands for this app (`wallet.state`):
/// checking: not known yet · none: no wallet installed · choose: several are, and the person
/// hasn't picked one for this app (`wallets`, then `chooseWallet`) · missing: the wallet is
/// installed but not running (or there is no Basecamp bridge) · otherZone: it is on another
/// network than the app's · disconnected: no access yet · asking: a connect request waits in
/// the wallet · connected: the app was given accounts.
export const STATES = ["checking", "none", "choose", "missing", "otherZone", "disconnected", "asking", "connected"]

/// The shared plain words. An app may show its own instead; these are the defaults.
export const WORDS = {
    none: "No wallet installed: install Names from Basecamp's app list.",
    choose: "More than one wallet is installed. Choose the one this app uses.",
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

/// Basecamp's installed packages (package_manager `getInstalledPackages`), out of whatever the
/// host wrapped them in (JSON text, perhaps twice; `{success, value}`), or null when the answer
/// isn't a list.
export function installedRows(raw) {
    var r = raw
    for (var depth = 0; depth < 4; depth++) {
        if (typeof r === "string") {
            try { r = JSON.parse(r) } catch (e) { return null }
        } else if (isObject(r) && !Array.isArray(r) && "value" in r) {
            r = r.value
        } else break
    }
    return Array.isArray(r) ? r : null
}

function rowName(row) { return String((row && (row.moduleName || row.name)) || "") }
function rowLabel(row) {
    var t = row ? (typeof row.displayName === "string" ? row.displayName : typeof row.display_name === "string" ? row.display_name : "") : ""
    t = String(t).replace(/[\u0000-\u001f\u007f\u200b-\u200f\u202a-\u202e\u2066-\u2069]/g, "").trim()
    return t.length > 40 ? t.substr(0, 39) + "…" : t
}

/// The wallets among the installed packages: core modules whose metadata.json says
/// `"category": "wallet"`, and Names (`names`) itself. `[{module, display_name, app}]`, Names
/// first; `app` is known for Names only until the wallet's `get_config` says it.
export function walletRows(rows) {
    var out = []
    ;(rows || []).forEach(function(row) {
        var name = rowName(row)
        if (!name || (row.type && row.type !== "core")) return
        if (name !== DEFAULT_WALLET.module && String(row.category || "").toLowerCase() !== "wallet") return
        if (out.some(function(w) { return w.module === name })) return
        var isNames = name === DEFAULT_WALLET.module
        out.push({ module: name, display_name: rowLabel(row) || (isNames ? DEFAULT_WALLET.display_name : name), app: isNames ? DEFAULT_WALLET.app : "" })
    })
    out.sort(function(a, b) { return (a.module === DEFAULT_WALLET.module ? 0 : 1) - (b.module === DEFAULT_WALLET.module ? 0 : 1) })
    return out
}

/// What a wallet's `get_config` says of the SDK: `{version, app}`, or null when it doesn't
/// implement it. Names 0.18 (before the declaration) counts as SDK 1.0.
export function declared(module, config) {
    var d = config && isObject(config.wallet_sdk) ? config.wallet_sdk : null
    if (d && d.name === SDK_NAME && d.version) return { version: String(d.version), app: d.app ? String(d.app) : "" }
    if (module === DEFAULT_WALLET.module && config && Array.isArray(config.features)) return { version: "1.0", app: DEFAULT_WALLET.app }
    return null
}

/// Whether a wallet's SDK version works with this file: the same major version.
export function speaks(version) {
    return String(version || "").split(".")[0] === SDK_VERSION.split(".")[0]
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
///   call(module, method, args, callback(raw), timeoutMs) → false when there is no Basecamp bridge;
///   later(f, ms) runs f once after ms (one timer each); open(app) brings that app forward;
///   recall(key) → text and remember(key, text): this app's own memory (the chosen wallet);
///   changed(wallet) after wallet.state, .current, .wallets, .accounts, .lastBalances, .features
///   or .config change.
export function create(host) {
    var h = host || {}
    var wallet = {
        version: VERSION,
        state: "checking", problem: "", config: null, features: [], accounts: [], lastBalances: [],
        chain: "", current: null, wallets: []
    }
    var choosing = []
    function changed() { if (h.changed) h.changed(wallet) }
    function setState(state, problem) {
        wallet.state = state
        wallet.problem = problem || ""
        changed()
    }
    function later(f, ms) { if (h.later) h.later(f, ms); else f() }

    // ── Which wallet ───────────────────────────────────────────────────────────

    /// The installed wallets: `done([{module, display_name, sdk_version, app}])`, each asked
    /// what it implements (`get_config().wallet_sdk`); `sdk_version` is null for one that
    /// didn't answer (installed, not running) or doesn't implement the SDK. When Basecamp's
    /// package list can't be read, Names is assumed installed.
    wallet.listWallets = function(done) {
        installed(function(list) {
            var left = list.length
            if (!left) { done([]); return }
            list.forEach(function(w) {
                wallet.call("get_config", [], function(config) {
                    var d = declared(w.module, config)
                    w.sdk_version = d ? d.version : null
                    if (d && d.app) w.app = d.app
                    if (--left === 0) done(list)
                }, function() {
                    w.sdk_version = null
                    if (--left === 0) done(list)
                }, { module: w.module, quiet: true })
            })
        })
    }

    function installed(done) {
        var asked = h.call ? h.call("package_manager", "getInstalledPackages", [], function(raw) {
            var rows = installedRows(raw)
            done(rows === null ? [copy(DEFAULT_WALLET)] : walletRows(rows))
        }, 15000) : false
        if (asked === false) done([copy(DEFAULT_WALLET)])
    }

    function copy(w) { return { module: w.module, display_name: w.display_name, app: w.app, sdk_version: w.sdk_version === undefined ? null : w.sdk_version } }

    /// The wallet this app uses: `done(wallet)` (`{module, display_name, app, sdk_version}`), or
    /// `done(null)` with state `none` (no wallet installed). With several and none chosen for
    /// this app yet, the state is `choose` (`wallets` lists them) and `done` runs once the person
    /// picks one (`chooseWallet`). One installed:
    /// it. Several: the one chosen for this app before (remembered), else ask.
    wallet.getWallet = function(done) {
        var end = done || function() {}
        if (wallet.current) { end(wallet.current); return }
        installed(function(list) {
            wallet.wallets = list
            if (!list.length) { setState("none", WORDS.none); end(null); return }
            var kept = h.recall ? String(h.recall("wallet") || "") : ""
            var pick = list.length === 1 ? list[0] : list.filter(function(w) { return w.module === kept })[0]
            if (pick) { use(pick); end(pick); return }
            choosing.push(end)
            setState("choose", WORDS.choose)
        })
    }

    function use(w) {
        wallet.current = w
        changed()
    }

    /// The person picked `module` (one of `wallet.wallets`) for this app: remembered, and the
    /// waiting `getWallet`/`check` go on with it.
    wallet.chooseWallet = function(module) {
        var w = (wallet.wallets || []).filter(function(x) { return x.module === module })[0]
        if (!w) return false
        if (h.remember) h.remember("wallet", w.module)
        use(w)
        setState("checking")
        var waiting = choosing
        choosing = []
        waiting.forEach(function(f) { f(w) })
        return true
    }

    /// Forget the choice: the next `check` finds the wallet again (and asks when there are
    /// several).
    wallet.forgetWallet = function() {
        if (h.remember) h.remember("wallet", "")
        wallet.current = null
        wallet.accounts = []
        setState("checking")
    }

    /// One call of the wallet this app uses. `done(value)` on success; `failed(r)` otherwise,
    /// with `r.error` in plain words and `r.busy`, `r.missing`, `r.unclear`, `r.uncertain` or
    /// `r.none` saying which. Busy is asked again (nothing happened); an unclear answer only for
    /// READS.
    wallet.call = function(method, args, done, failed, options) {
        var o = options || {}
        var no = failed || function() {}
        if (!o.module && !wallet.current) {
            wallet.getWallet(function(w) {
                if (w) wallet.call(method, args, done, failed, o)
                else if (wallet.state === "none") no({ ok: false, none: true, error: WORDS.none })
            })
            return
        }
        var module = o.module || wallet.current.module
        var tries = o.tries || 0
        var asked = h.call ? h.call(module, method, textArgs(args), function(raw) {
            var r = reply(raw)
            if (r.ok) { if (done) done(r.value); return }
            var read = READS.indexOf(method) >= 0
            if ((r.busy || (r.unclear && read)) && tries < RETRIES) {
                later(function() { wallet.call(method, args, done, failed, { tries: tries + 1, timeout: o.timeout, module: o.module, quiet: o.quiet }) }, RETRY_MS * (tries + 1))
                return
            }
            if (r.missing && !o.quiet) setState("missing", WORDS.missing)
            if (r.unclear && !read) { r.uncertain = true; r.error = WORDS.uncertain }
            no(r)
        }, o.timeout || 30000) : false
        if (asked === false) { if (!o.quiet) setState("missing", WORDS.missing); no({ ok: false, missing: true, error: WORDS.noBridge }) }
    }

    /// Whether this Names build has `feature` (from `get_config`, read by `check`).
    wallet.has = function(feature) { return (wallet.features || []).indexOf(feature) >= 0 }

    /// Where the wallet stands: which one (`getWallet`), its config and features, the zone
    /// (`options.chain`: the chain's channel, 64 hex, the app is built for), and the accounts
    /// given. `done(wallet)`; with several wallets and none chosen yet, after the person chooses.
    wallet.check = function(options, done) {
        var o = options || {}
        if (o.chain !== undefined) wallet.chain = String(o.chain || "")
        var end = done || function() {}
        if (!wallet.current) {
            wallet.getWallet(function(w) { if (w) wallet.check({}, end); else if (wallet.state === "none") end(wallet) })
            return
        }
        wallet.call("get_config", [], function(config) {
            wallet.config = config
            wallet.features = (config && config.features) || []
            var d = declared(wallet.current.module, config)
            wallet.current.sdk_version = d ? d.version : null
            if (d && d.app) wallet.current.app = d.app
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

    /// Bring the wallet's app forward (Basecamp's `basecamp.apps.launch`; declare it in
    /// metadata.json `uses`). The wallet shows your app again once the person answers.
    wallet.open = function() { if (h.open && wallet.current && wallet.current.app) h.open(wallet.current.app) }

    /// Read `request` (its id, or the request) every POLL_MS until it is no longer open
    /// (options.status: another method to read it with, options.cancel: to withdraw it).
    /// handlers: update(request) each read while open, done(request) at the end (approved,
    /// done, rejected, expired, cancelled or failed: see requestLine), failed(r) when Names
    /// refuses to say. Returns `{stop(), withdraw(done, failed)}`.
    wallet.watch = function(request, handlers, options) {
        var hs = handlers || {}
        var o = options || {}
        var statusMethod = o.status || "request_status"
        var id = isObject(request) ? request.request : String(request || "")
        var watcher = { stopped: false, request: isObject(request) ? request : { request: id, state: "pending" } }
        function poll() {
            if (watcher.stopped) return
            wallet.call(statusMethod, [id], function(v) {
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
        watcher.withdraw = function(done, failed) { wallet.withdraw(id, done, failed, o.cancel) }
        later(poll, hs.every || POLL_MS)
        return watcher
    }

    /// Withdraw a request while it is pending (`cancel_request`, or `cancelMethod`). The watch
    /// then ends `cancelled`.
    wallet.withdraw = function(requestId, done, failed, cancelMethod) {
        wallet.call(cancelMethod || "cancel_request", [isObject(requestId) ? requestId.request : requestId], done, failed)
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

    /// Any propose_* call that answers a request: sends it, takes the person to the wallet
    /// (options.open, default true) and watches it. handlers: sent(request) once it waits in
    /// the wallet, update(request), done(request) (see requestLine for its state), failed(r).
    /// options.status / options.cancel: the methods that read and withdraw it, for a kind with
    /// its own (NFT requests: `nft_request_status`, `cancel_nft_request`). Returns nothing;
    /// `sent` gets the request and the watcher (`{stop, withdraw}`).
    wallet.propose = function(method, args, handlers, options) {
        var o = options || {}
        var hs = handlers || {}
        wallet.call(method, args, function(v) {
            var watcher = wallet.watch(v, hs, o)
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
