// The pieces of logic the Names view keeps for itself, kept apart from the QML so
// the Node test (logic.test.mjs) runs the same code the app does. The QML engine
// imports this as an ECMAScript module: `import "logic.mjs" as Logic`.

/// Whether the decimal `a` is at least `b`. Balances and deposits are decimal
/// text, past what a JS number holds exactly.
export function atLeast(a, b) {
    a = String(a).replace(/^0+/, "")
    b = String(b).replace(/^0+/, "")
    return a.length !== b.length ? a.length > b.length : a >= b
}

/// What a read says about itself beside its value, to show with it: a partial read (some
/// balances the sequencer didn't give, shown as unknown) or a stale one (the last list,
/// kept while reading it again fails). "" for a whole, fresh read or no reply.
export function readNote(reply) {
    if (!reply || typeof reply !== "object" || reply.success !== true)
        return ""
    if (reply.partial !== true && reply.stale !== true && !reply.warning)
        return ""
    return String(reply.warning || (reply.stale ? "This may be out of date." : "Some balances are unknown."))
}

/// An entry's value for reading: a long unbroken value is shortened in the middle.
export function entryShown(value) {
    var text = String(value)
    return text.length > 40 && !/\s/.test(text) && !/^https?:/.test(text) ? text.substr(0, 12) + "…" + text.substr(text.length - 8) : text
}

/// Whether an amount (decimal text) is more than nothing.
export function someAmount(digits) {
    return digits !== null && digits !== undefined && !/^0*$/.test(String(digits))
}

/// The largest amount: LEZ and token balances are 128-bit numbers.
export const AMOUNT_MAX = "340282366920938463463374607431768211455"
/// The largest Logos Blockchain amount: its notes hold 64-bit numbers.
export const CHAIN_AMOUNT_MAX = "18446744073709551615"

/// Every amount a person types, in every field: `{digits, problem}`. `digits` is the
/// amount written plainly ("1000"), or "" when `problem` says why it isn't one. The
/// rules and the words are the names CLI's (`tokens::parse_amount`) and the module's
/// (`cli::amount`), checked against the same table (basecamp-module/rust-lib/src/
/// amounts.json): whole numbers above 0, `,` or `_` only between groups of three
/// digits, so "1,5" is refused and never read as 15. `max` (decimal text) defaults to
/// AMOUNT_MAX.
export function amountRead(text, max) {
    var typed = String(text === null || text === undefined ? "" : text).trim()
    var refuse = function(problem) { return { digits: "", problem: problem } }
    if (typed === "")
        return refuse("Enter an amount, such as 1,000.")
    if (/^-[0-9]/.test(typed))
        return refuse(typed + " is negative. Enter an amount above 0.")
    if (!/^[0-9,_]*[0-9][0-9,_]*$/.test(typed))
        return refuse(typed + " isn't a whole number. Enter digits only, such as 1000 or 1,000.")
    var groups = typed.split(/[,_]/)
    // A first group starting with 0 (0,001) is no thousands grouping either.
    if (groups.length > 1 && !(groups[0].length >= 1 && groups[0].length <= 3 && groups[0].charAt(0) !== "0"
                               && groups.slice(1).every(function(group) { return group.length === 3 })))
        return refuse(typed + " isn't a whole number: commas go only between groups of three digits, such as 1,000.")
    var digits = groups.join("").replace(/^0+/, "")
    if (digits === "")
        return refuse("Enter an amount above 0.")
    var most = String(max || AMOUNT_MAX)
    if (digits.length > most.length || (digits.length === most.length && digits > most))
        return refuse(typed + " is more than an amount can be.")
    return { digits: digits, problem: "" }
}

/// Why `text` isn't an amount, or "" when it is one (amountRead).
export function amountProblem(text, max) {
    return amountRead(text, max).problem
}

/// `text` as the amount it is, written plainly, or "" when it isn't one (amountRead).
export function amountDigits(text, max) {
    return amountRead(text, max).digits
}

/// Why `text` can't be where a token goes (a name, `Public/<id>` or one of
/// this wallet's `Private/<id>`), or "" if it looks right. The module checks
/// it again.
export function tokenToProblem(text) {
    var to = String(text === null || text === undefined ? "" : text).trim()
    if (to === "")
        return "Enter a name, or Public/ and an account ID."
    if (/^(Public|Private)\//.test(to))
        return /^(Public|Private)\/[1-9A-HJ-NP-Za-km-z]{32,44}$/.test(to) ? "" : "That isn't an account. It's Public/ (or Private/) and then the account ID, letters and digits."
    if (/^[1-9A-HJ-NP-Za-km-z]{32,44}$/.test(to))
        return "To send to an account, write Public/ and then its ID."
    if (!/^@?[a-z0-9-]{1,32}(\.lez)?$/i.test(to))
        return "A name is 1-32 letters, digits and hyphens."
    return ""
}

/// Whether a token send is proven on this computer (minutes) rather than sent
/// publicly in one block: from the private account, or to anything but a
/// public account (a name's tokens go to its private keys).
export function tokenSendProves(fromPrivate, to) {
    return !!fromPrivate || !/^Public\//.test(String(to).trim())
}

/// A token as people read it: its definition's name, or a stand-in when the
/// definition gives none.
export function tokenShown(name) {
    var text = String(name === null || name === undefined ? "" : name).trim()
    return text === "" ? "unnamed token" : text
}

/// Decimal text with thousands grouped: "1,234,567".
function groupDigits(digits) {
    return String(digits).replace(/\B(?=(\d{3})+(?!\d))/g, ",")
}

/// The network fee a transfer will cost, from `cli_fee`'s quote: "about 8,000
/// (at most 18,000 held while it runs)". From a private account the fee is paid
/// inside the proof. "" while there is no quote.
export function feeEstimate(quote, fromPrivate) {
    if (!quote)
        return ""
    var estimate = fromPrivate ? quote.private_transfer : quote.public_transfer
    if (!estimate || !/^\d+$/.test(String(estimate.about)) || !/^\d+$/.test(String(estimate.at_most)))
        return ""
    var about = "about " + groupDigits(estimate.about)
    if (String(estimate.at_most) === String(estimate.about))
        return about
    return about + (fromPrivate ? " (at most " + groupDigits(estimate.at_most) + " if fees climb before it lands)"
                                : " (at most " + groupDigits(estimate.at_most) + " held while it runs)")
}

/// What a finished job's `fee` says it cost: "Fee paid: 7,950." or "".
export function feePaid(fee) {
    if (!fee || !/^\d+$/.test(String(fee.paid)))
        return ""
    return "Fee paid: " + groupDigits(fee.paid) + "."
}

/// What a job that failed says, in one line: the module's `problem` (built from the CLI's
/// error), else `lead` and the CLI's `Error:` line, never the last line of its cause chain
/// ("2: Connection refused (os error 61)", transfer-failure-text-is-last-cause-line).
export function jobProblem(job, lead) {
    var value = job || {}
    if (value.problem)
        return String(value.problem)
    var lines = String(value.log_tail || "").split("\n").map(function(line) { return line.trim() }).filter(function(line) { return line !== "" })
    var error = lines.filter(function(line) { return /^Error:/.test(line) })[0]
    var why = error !== undefined ? error.replace(/^Error:\s*/, "") : lines.length > 0 ? lines[lines.length - 1] : ""
    return lead + ": " + (why !== "" ? why : "it stopped without saying why")
}

/// An account as listed in Settings: its nickname, else its kind; the default
/// account of each kind is the one the wallet pays and registers from.
/// An account's title: only a registered name ever names an account, so without one it
/// is "Private account" or "Public account" (nicknames and display names never title it).
export function accountTitle(entry) {
    if (!entry)
        return ""
    return entry.kind === "private" ? "Private account" : "Public account"
}

/// The accounts sorted for Settings: private ones first, the default of each
/// kind leading its kind, the rest in wallet order.
export function accountsShown(list) {
    var rows = Array.isArray(list) ? list.slice() : []
    var rank = function(entry) { return (entry.kind === "private" ? 0 : 2) + (entry["default"] ? 0 : 1) }
    return rows.map(function(entry, i) { return [rank(entry), i, entry] })
        .sort(function(a, b) { return a[0] - b[0] || a[1] - b[1] })
        .map(function(pair) { return pair[2] })
}

/// The approval sheet's publisher line, from the Program Registry's check of the asking app
/// (`verify_app`): `{text, tone}`, tone `good`, `warn` or `plain`; null to show nothing.
export function publisherLine(publisher) {
    if (!publisher || typeof publisher !== "object")
        return null
    var name = String(publisher.publisher || "")
    switch (publisher.status) {
    case "verified":
        return { text: name + " ✓ verified publisher", tone: "good" }
    case "mismatch":
        return { text: "Claims " + name + ", but " + name + " doesn't vouch for this copy. It may be a fake.", tone: "warn" }
    case "not_published":
        return { text: "Claims " + name + ", but " + name + " doesn't list this app.", tone: "warn" }
    case "no_such_name":
        return { text: "Claims " + name + ", which isn't a registered name.", tone: "warn" }
    case "unclaimed":
        return { text: "Not published under any LNS name, so who made it can't be checked.", tone: "plain" }
    case "unavailable":
        return { text: "Couldn't check who publishes it (the Program Registry module isn't installed or didn't answer).", tone: "plain" }
    }
    return null
}

// ── The Logos Blockchain (chain_* in the names module) ─────────────────

/// About how long `seconds` is, in words.
export function durationWords(seconds) {
    var s = Math.max(0, Math.round(Number(seconds) || 0))
    if (s < 90) return s + " s"
    if (s < 5400) return Math.round(s / 60) + " min"
    if (s < 172800) return (s / 3600).toFixed(1) + " h"
    return (s / 86400).toFixed(1) + " days"
}

/// A key or account ID cut down for a line of text: `66ecf4bc…a81b`. An account written
/// with its kind keeps it whole and cuts only the ID (`Public/CCVd76Tk…pKnk`), so the
/// prefix never eats what tells two accounts apart (account-page-address-line-useless).
export function keyShown(key) {
    var text = String(key || "")
    var account = /^(Public|Private)\/(.+)$/.exec(text)
    if (account)
        return account[1] + "/" + keyShown(account[2])
    return text.length > 16 ? text.substr(0, 8) + "…" + text.substr(text.length - 4) : text
}

/// Why `text` isn't something a Logos Blockchain send can go to (a 64-hex key, or a
/// name), or "" if it is one. Names are checked by the module; this catches typos.
export function chainToProblem(text) {
    var to = String(text === null || text === undefined ? "" : text).trim().replace(/^0x/i, "")
    if (to === "") return "Enter a key or a name."
    if (/^[0-9a-fA-F]+$/.test(to) && to.length > 32)
        return to.length === 64 ? "" : "A Logos Blockchain key is 64 hex characters; that one has " + to.length + "."
    return /^@?[a-zA-Z0-9-]{1,32}(\.lez)?$/.test(to) ? "" : "Enter a key (64 hex characters) or a name."
}

/// The stake, as `chain_stake` gives it, in lines for the Stake part.
export function stakeLines(stake) {
    if (!stake) return []
    var keys = stake.keys || []
    var lines = []
    if (keys.length === 0)
        lines.push("Nothing staked yet: tokens on your node's keys join at an epoch boundary.")
    else if (keys.length === 1)
        lines.push(grouped(stake.staked) + " staked in " + stake.notes + (stake.notes === 1 ? " note" : " notes") + " on " + keyShown(keys[0]) + ".")
    else
        lines.push(grouped(stake.staked) + " staked in " + stake.notes + " notes on " + keys.length + " keys.")
    var epoch = "Epoch " + stake.epoch
    if (stake.next_epoch_in_s !== null && stake.next_epoch_in_s !== undefined)
        epoch += "; the next starts in about " + durationWords(stake.next_epoch_in_s)
    lines.push(epoch + ". New tokens join the stake at an epoch boundary, up to two epochs after they arrive.")
    return lines
}

/// The rewards line: what a claim would cash out, or that there's nothing.
export function rewardsLine(stake) {
    if (!stake || !stake.vouchers) return "Nothing to claim. Each block your node leads earns a reward to claim here."
    var line = grouped(stake.claimable) + " to claim from " + stake.vouchers + (stake.vouchers === 1 ? " block" : " blocks") + " your node led."
    // The node's claim cashes one voucher a time and pays its own fee from the leader funding key.
    if (stake.reward_per_voucher)
        line += " Each claim cashes one block's reward (" + grouped(stake.reward_per_voucher) + ") and pays its own fee, which can be more than that."
    return line
}

/// `1234567` (decimal text or a number) as `1,234,567`.
export function grouped(digits) {
    var text = String(digits === null || digits === undefined ? "" : digits)
    return /^\d+$/.test(text) ? text.replace(/\B(?=(\d{3})+(?!\d))/g, ",") : text
}

/// Two amounts in decimal text added, exactly (they pass what a JS number holds).
export function addDigits(a, b) {
    var x = String(a || "0").replace(/^0+(?=\d)/, ""), y = String(b || "0").replace(/^0+(?=\d)/, "")
    if (!/^\d+$/.test(x) || !/^\d+$/.test(y)) return null
    var out = "", carry = 0
    for (var i = x.length - 1, j = y.length - 1; i >= 0 || j >= 0 || carry; i--, j--) {
        var sum = (i >= 0 ? x.charCodeAt(i) - 48 : 0) + (j >= 0 ? y.charCodeAt(j) - 48 : 0) + carry
        out = String(sum % 10) + out
        carry = sum >= 10 ? 1 : 0
    }
    return out
}

/// Everything the wallet holds, one row per asset and account, for the Wallet
/// screen: `[{id, kind, title, where, balance, definition, fromPrivate,
/// stakable}]`. `kind` is `lez`, `token` or `chain`; `balance` decimal text, or
/// null while unknown. `cliBalance` is cli_balance's reply ({balance, public}),
/// `holdings` cli_token_holdings', `watched` the token IDs added by address,
/// `chainAccounts` chain_accounts' ([{key, balance}]) and `chainDown` whether the
/// node couldn't be read.
export function walletAssets(cliBalance, holdings, watched, tokenNames, chainAccounts, chainDown) {
    var rows = []
    if (cliBalance && typeof cliBalance === "object") {
        rows.push({ id: "lez-private", kind: "lez", title: "LEZ", where: "Private", balance: cliBalance.balance === undefined ? null : String(cliBalance.balance), definition: "", fromPrivate: true, stakable: false })
        if (cliBalance["public"])
            rows.push({ id: "lez-public", kind: "lez", title: "LEZ", where: "Public", balance: String(cliBalance["public"].balance), definition: "", fromPrivate: false, stakable: false })
    }
    var held = {}
    ;(holdings || []).forEach(function(h) {
        held[h.definition] = true
        var priv = h.account_kind === "private"
        rows.push({ id: "token-" + h.definition + "-" + (priv ? "private" : "public"), kind: "token", title: tokenShown(h.name), where: priv ? "Private" : "Public",
                    balance: String(h.balance), definition: h.definition, fromPrivate: priv, stakable: false })
    })
    ;(watched || []).forEach(function(definition) {
        if (held[definition]) return
        held[definition] = true
        var name = tokenNames && tokenNames[definition]
        rows.push({ id: "token-" + definition, kind: "token", title: name ? tokenShown(name) : keyShown(definition), where: "None yet",
                    balance: "0", definition: definition, fromPrivate: true, stakable: false })
    })
    if (chainAccounts || chainDown) {
        var total = chainAccounts ? (chainAccounts || []).reduce(function(sum, a) { return a.balance === null || sum === null ? sum : addDigits(sum, a.balance) }, "0") : null
        rows.push({ id: "chain", kind: "chain", title: "Logos Blockchain", where: chainDown ? "Node not running" : "On your node",
                    balance: chainDown ? null : total, definition: "", fromPrivate: false, stakable: true })
    }
    return rows
}

/// `a - b` for amounts in decimal text, exactly; null when b > a or either isn't digits.
export function subDigits(a, b) {
    var x = String(a || "0").replace(/^0+(?=\d)/, ""), y = String(b || "0").replace(/^0+(?=\d)/, "")
    if (!/^\d+$/.test(x) || !/^\d+$/.test(y) || !atLeast(x, y)) return null
    var out = "", borrow = 0
    for (var i = x.length - 1, j = y.length - 1; i >= 0; i--, j--) {
        var d = (x.charCodeAt(i) - 48) - (j >= 0 ? y.charCodeAt(j) - 48 : 0) - borrow
        borrow = d < 0 ? 1 : 0
        out = String(d < 0 ? d + 10 : d) + out
    }
    return out.replace(/^0+(?=\d)/, "")
}

/// The Wallet home: one group per chain (LEZ, tokens on LEZ, the Logos
/// Blockchain), each with its total and one row per address. `accounts` is
/// cli_accounts' list (every LEZ account) or null, then `cliBalance` gives the
/// two default ones; `stake` is chain_stake's reply or null. A row:
/// `{id, kind, title, sub, address, balance, fromPrivate, definition, key,
/// canSend, account}`; `canSend` is false for a LEZ account that isn't the
/// default of its kind (the CLI pays from the default).
export function walletGroups(cliBalance, accounts, holdings, watched, tokenNames, chainAccounts, chainDown, stake, namesByNpk, mainName, mainLabel) {
    // Accounts are named by registered names only: the account a name is paid into carries
    // it; the paying account is "Private account" unless a name is paid into it too.
    var groups = []
    var lez = []
    var named = 0
    if (Array.isArray(accounts) && accounts.length > 0) {
        accountsShown(accounts).forEach(function(a) {
            var priv = a.kind === "private"
            var label = String(a.label === null || a.label === undefined ? "" : a.label).trim()
            // The names this account receives for: their profiles publish its npk.
            var names = priv && a.npk && namesByNpk && namesByNpk[String(a.npk).toLowerCase()] ? namesByNpk[String(a.npk).toLowerCase()] : []
            named += names.length
            // Only a registered name names an account (the one its payments arrive at).
            var title = names.length > 0 ? names.join(", ") : (priv ? "Private account" : "Public account")
            var sub = (priv ? "Private" : "Public")
                + (names.length > 0 ? " · receives payments to " + names.join(", ") : "")
                + (a["default"] ? (priv ? " · pays and registers" : " · your main public account") : names.length > 0 ? "" : " · " + keyShown(a.account.replace(/^(Public|Private)\//, "")))
            lez.push({ id: "lez-" + a.account, kind: "lez", title: title, sub: sub, name: names.length > 0 ? names[0] : "",
                       address: a.account, balance: a.balance === null || a.balance === undefined ? null : String(a.balance), fromPrivate: priv, definition: "", key: "",
                       canSend: !!a["default"], account: a.account })
        })
    } else if (cliBalance && typeof cliBalance === "object") {
        // No default private account (account null): only the public one is shown.
        if (cliBalance.account)
            lez.push({ id: "lez-private", kind: "lez", title: "Private account", name: "", sub: "Private · pays and registers", address: "Private/" + cliBalance.account,
                       balance: cliBalance.balance === undefined || cliBalance.balance === null ? null : String(cliBalance.balance), fromPrivate: true, definition: "", key: "", canSend: true, account: "" })
        if (cliBalance["public"])
            lez.push({ id: "lez-public", kind: "lez", title: "Public account", sub: "Public · your main public account", address: "Public/" + cliBalance["public"].account,
                       balance: String(cliBalance["public"].balance), fromPrivate: false, definition: "", key: "", canSend: true, account: "" })
    }
    if (lez.length > 0) {
        var lezTotal = lez.reduce(function(sum, r) { return sum === null || r.balance === null ? sum : addDigits(sum, r.balance) }, "0")
        groups.push({ id: "lez", title: "LEZ", letter: "L", total: lezTotal, tone: "plain", rows: lez,
                      note: (lez.length === 1 ? "1 account" : lez.length + " accounts") + (named === 0 ? "" : named === 1 ? " · 1 name" : " · " + named + " names") })
    }

    var tokens = walletAssets(null, holdings, watched, tokenNames, null, false).map(function(r) {
        return { id: r.id, kind: "token", title: r.title, sub: r.where + " · token on LEZ", address: r.definition, balance: r.balance, fromPrivate: r.fromPrivate,
                 definition: r.definition, key: "", canSend: r.balance !== "0", account: "" }
    })
    if (tokens.length > 0)
        groups.push({ id: "tokens", title: "Tokens on LEZ", letter: "T", total: null, note: tokens.length === 1 ? "1 token" : tokens.length + " tokens", tone: "plain", rows: tokens })

    if (chainAccounts || chainDown) {
        var keys = (chainAccounts || []).map(function(a) {
            return { id: "chain-" + a.key, kind: "chain", title: chainKeyTitle(a), sub: chainKeySub(a), address: a.key,
                     balance: a.balance === null || a.balance === undefined ? null : String(a.balance), fromPrivate: false, definition: "", key: a.key, canSend: a.ready !== false,
                     later: a.ready === false, account: "" }
        })
        var later = keys.filter(function(r) { return !r.canSend }).length
        var total = chainAccounts ? keys.reduce(function(sum, r) { return sum === null || r.balance === null ? sum : addDigits(sum, r.balance) }, "0") : null
        var waiting = stake && total !== null ? subDigits(total, stake.staked) : null
        var note = chainDown ? "Node not running" : (keys.length === 1 ? "1 key on your node" : keys.length + " keys on your node")
            + (later === 0 ? "" : " · " + later + " from its next start")
            + (stake === null || stake === undefined || total === null ? "" : waiting === "0" ? " · all staked" : waiting !== null ? " · " + grouped(waiting) + " not counted yet" : " · " + grouped(stake.staked) + " staked")
        groups.push({ id: "chain", title: "Logos Blockchain", letter: "B", total: total, note: note, tone: chainDown ? "alarm" : stake ? "stake" : "plain", rows: keys })
    }
    return groups
}

/// A Logos Blockchain account's title: its label, else what it is.
/// `a` is a chain_accounts row ({key, balance, label?, title?, ready?}).
export function chainKeyTitle(a) {
    var label = String(a && a.label ? a.label : "").trim()
    return label !== "" ? label : a && a.title ? "Account" : "Node key"
}

/// A Logos Blockchain account's second line: the key, and when the node uses it.
export function chainKeySub(a) {
    return "Public · " + keyShown(a.key) + (a.ready === false ? " · your node uses it from its next start" : "")
}

/// Why an account the node takes at its next start offers no address, Receive or Bridge yet.
export function chainLaterNote() {
    return "Your node uses this account from its next start: stop and start it in Basecamp's Blockchain app. "
        + "Until then it can't send, and don't have anything sent to it: what reaches it before your node uses it isn't seen. "
        + "Its address, Receive and Bridge show here once your node uses it."
}

/// The chain_accounts rows the node uses now (not one it takes at its next start), or
/// null while they haven't been read.
export function chainUsable(chainAccounts) {
    return chainAccounts ? chainAccounts.filter(function(a) { return a.ready !== false }) : null
}

/// The Staking table: what the node holds, what is staked, what isn't counted yet.
export function stakeTable(stake, chainAccounts) {
    if (!stake) return null
    var held = (chainAccounts || []).reduce(function(sum, a) { return sum === null || a.balance === null ? sum : addDigits(sum, a.balance) }, "0")
    var waiting = held === null ? null : subDigits(held, stake.staked)
    return { held: held, staked: String(stake.staked), waiting: waiting === null ? "0" : waiting, claimable: String(stake.claimable || "0"), vouchers: stake.vouchers || 0 }
}

// ── The Wallet's history (activity in the module) ─────────────────────

/// An account, key or name in an activity line: short.
function whoShown(text) {
    var t = String(text || "")
    var m = /^(Public|Private)\/(.+)$/.exec(t)
    if (m)
        return m[1] + " " + keyShown(m[2])
    return /^[0-9a-f]{64}$/.test(t) ? "key " + keyShown(t) : t
}

/// How long ago `at` (ms) was, from `now` (ms).
export function agoShown(at, now) {
    // The module keeps seconds; milliseconds are taken too.
    var when = Number(at) < 1e12 ? Number(at) * 1000 : Number(at)
    var s = Math.max(0, Math.round((Number(now) - when) / 1000))
    if (s < 60) return "just now"
    if (s < 3600) return Math.round(s / 60) + " min ago"
    if (s < 86400) return Math.round(s / 3600) + " h ago"
    return Math.round(s / 86400) + " d ago"
}

/// Whether a LEZ entry is proven on this computer: a move to private and a claim always
/// are; anything else when it is marked private or paid from a private account.
function proven(e) {
    if (e.chain !== "lez") return false
    if (e.kind === "shield" || e.kind === "receive") return true
    return e["private"] === true || /^Private\//.test(String(e.from || ""))
}

/// How long a bridge takes to reach LEZ once it is in a block.
var BRIDGE_ARRIVES_MS = 12 * 3600 * 1000

/// One history entry in words: {title, sub, amount, sign, state, tone}. `sign` is "-" for
/// what left, "+" for what came in, "" for a move between your own accounts; `tone` is
/// "good", "warn", "bad" or "plain" for the state. `account`, when given, is the account
/// whose page shows the entry: the direction and amount are then that account's own
/// (incoming-payment-shown-as-outgoing-on-receiving-accounts-page,
/// claim-entry-not-on-credited-accounts-page-and-shown-as-credit-on-payer).
export function activityLine(entry, now, account) {
    var e = entry || {}
    var page = String(account || "")
    var kind = String(e.kind || "")
    if (page !== "") {
        var own = accountSide(e, kind, page)
        if (own)
            return lezNamed(own(activityLine(e, now)), e)
    }
    var amount = e.amount ? grouped(e.amount) : ""
    var title = "", sub = "", sign = "-"
    switch (kind) {
    case "send": title = "Sent on the Logos Blockchain"; sub = "to " + (e.name ? e.name : whoShown(e.to)); break
    case "inscribe": title = "Inscribed a testimonial"; sub = "on the Logos Blockchain, from " + whoShown(e.from); sign = ""; break
    case "split": title = "Made a note of exactly " + amount; sub = "on " + whoShown(e.from) + (e.fee ? " · fee " + grouped(e.fee) : ""); sign = ""; break
    case "bridge": title = "Bridge to LEZ"; sub = "from " + whoShown(e.from) + " into " + whoShown(e.to); break
    case "pay": title = "Paid " + (e.name || "a name"); sub = "on LEZ"; break
    case "resign": title = "Signed again for " + clean(e.app || "an app"); sub = "on LEZ 0.4, without asking: only the payer's nonce had moved (payer nonce " + clean(e.payer_nonce) + ", re-sign " + clean(e.resigns) + " of 3)"; sign = ""; break
    case "pay_account": title = "Paid " + whoShown(e.to); sub = e["private"] ? "on LEZ, from your private account" : "on LEZ, from your public account"; break
    case "shield": title = "Moved to private"; sub = "on LEZ, from your public account"; sign = ""; break
    case "receive": title = e.claimed === false ? "Nothing claimed" : landedIn(e) ? "Claimed waiting payments" : "Claim of waiting payments"; sub = e.claimed === false ? "on LEZ, what waits is worth less than claiming it costs" : "on LEZ, into your private accounts"; sign = "+"
        if (e.claimed !== false && e.left && typeof e.left === "object" && Object.keys(e.left).length > 0)
            sub += " · some still wait"
        break
    case "token_send": title = "Sent a token"; sub = "to " + whoShown(e.to); break
    case "transfer": title = "Gave " + (e.name || "a name") + " away"; sub = "to " + whoShown(e.to); sign = ""; break
    case "receive_privately": title = "Private payments to " + (e.name || "a name"); sub = "on LEZ, new receiving keys"; sign = ""; break
    case "arrived":
        title = "Arrived"
        sub = "in " + whoShown(e.to) + (e.chain === "logos" ? " on the Logos Blockchain" : e.chain === "lez-0.4" ? " on LEZ 0.4, the bridge test network" : " on LEZ")
        // On the bridge test network a token's rise is told in its own units.
        if (e.chain === "lez-0.4" && e.token) {
            var units = e.amount ? e.amount : e.at_least
            amount = units ? decimalShown(units, e.decimals) + " " + String(e.symbol || "") : ""
            if (!e.amount && e.at_least) { sub += " · at least " + amount; amount = "" }
        } else if (!e.amount && e.at_least)
            sub += " · at least " + grouped(e.at_least)
        sign = "+"; break
    default: title = kind; sub = ""
    }
    var stateText = "", tone = "plain"
    switch (String(e.state || "")) {
    // Only a private change is proven; a public one on LEZ is only sent (activity-public-send-says-proving).
    case "sending": stateText = proven(e) ? "Proving…" : "Sending…"; tone = "warn"; break
    case "in_block":
        // Past its window a bridge is overdue, not still "about 12 h" away (activity-bridge-row-wording).
        var sent = Number(e.at) < 1e12 ? Number(e.at) * 1000 : Number(e.at)
        stateText = kind !== "bridge" ? "In a block"
            : e.followed === false ? "In a block · its arrival on LEZ can't be followed here"
            : Number(now) - sent > BRIDGE_ARRIVES_MS ? "In a block · should have arrived by now; check the balance"
            : "In a block · arrives in about 12 h"
        tone = kind === "bridge" ? "warn" : "good"; break
    // Several bridges matched by one credit can't be told from one payment of their total
    // (single-bridge-false-arrival-on-foreign-credit-at-least-amount): said so.
    // A bridge arrives when LEZ mints its own deposit; `maybe` is kept by entries an older
    // version matched by amount.
    case "arrived": stateText = kind !== "bridge" ? "" : e.maybe === true ? "A credit of the same total as your bridges reached LEZ" : e.late === true ? "Arrived on LEZ, late" : "Arrived on LEZ"; tone = "good"; break
    // A bridge LEZ hasn't credited within 2 days; it is still followed, and arrives if LEZ credits it.
    case "expired": stateText = e.followed === false ? "Not seen on LEZ after 2 days · check the balance" : "Not credited on LEZ after 2 days · still followed; check the balance"; tone = "warn"; break
    case "done": stateText = "Done"; tone = "good"; break
    case "failed": stateText = "Didn't go through"; tone = "bad"; break
    // Some of its steps landed and the rest stopped (partial-claim-job-and-pages-say-nothing-true).
    case "partial": stateText = "Partly done · see what's left"; tone = "warn"; break
    // The module restarted while it ran, or its record is gone: it may have gone through.
    case "unknown": stateText = "Outcome unknown · check the balance"; tone = "warn"; break
    }
    // A LEZ job's fee is {about, at_most, paid}; a Logos Blockchain send's is the paid fee itself.
    var fee = e.fee && typeof e.fee === "object" ? e.fee.paid : e.fee
    if (fee && kind !== "split" && e.state !== "sending")
        sub += " · fee " + grouped(fee)
    // Money came in only once it landed; a failed or unknown change moved nothing known.
    if ((sign === "+" && !landedIn(e)) || e.state === "failed" || e.state === "unknown")
        sign = ""
    // Which zone, once there is more than one to tell apart (the module gives `zone_name`
    // then; null for a zone removed since).
    if (typeof e.zone_name === "string" && e.zone_name !== "")
        sub += (sub ? " · " : "") + "zone " + e.zone_name
    else if (e.zone_name === null && e.zone)
        sub += (sub ? " · " : "") + "a zone removed since"
    // The bridge test network's rows are new UI: told in ink, never in green (`quiet`).
    var quiet = e.chain === "lez-0.4"
    if (quiet && tone === "good") tone = "plain"
    var line = { title: title, sub: sub, amount: amount, sign: amount === "" ? "" : sign, state: stateText, tone: tone, ago: agoShown(e.at, now) }
    if (quiet) line.quiet = true
    return lezNamed(line, e)
}

/// `line` of entry `e` with its LEZ named: a home-zone entry is on LEZ 0.3, the public Logos
/// testnet. Another zone names itself (`zone …`), and the bridge test network's entries
/// already say LEZ 0.4. Saying it twice changes nothing.
function lezNamed(line, e) {
    // An entry made while the names network was LEZ 0.4 carries that network's home id.
    var net = e.zone === "bridge-0.4" ? "LEZ 0.4" : "LEZ 0.3"
    if (!(e.chain === "lez" || e.kind === "bridge") || (e.zone && e.zone !== "bridge-0.4"))
        return line
    var named = function(text) { return String(text).replace(/\b(on|to) LEZ\b(?! 0\.)/g, "$1 " + net) }
    line.title = named(line.title)
    line.sub = named(line.sub)
    line.state = named(line.state)
    return line
}

/// Whether entry `e`'s money landed: a change done, a bridge arrived on LEZ, a Logos
/// Blockchain send in a block, an arrival. Only then is it received.
function landedIn(e) {
    var state = String(e.state || "")
    switch (String(e.kind || "")) {
    case "arrived": return true
    case "bridge": return state === "arrived"
    case "send": return state === "in_block" || state === "done"
    // A claim that stopped partway credited the accounts in its `received`: those landed.
    case "receive": return state === "done" || state === "partial"
    }
    return state === "done"
}

/// How `page` (an account or key) sees entry `e` when that differs from the wallet's view:
/// a function that turns the wallet's line into the page's, or null.
function accountSide(e, kind, page) {
    var received = e.received && typeof e.received === "object" ? e.received : {}
    var fee = e.fee && typeof e.fee === "object" ? e.fee.paid : e.fee
    var left = e.left && typeof e.left === "object" ? e.left : {}
    if (kind === "receive" && left[page] && !received[page])
        // A credit to this account the claim left for later: it still waits, nothing moved.
        return function(line) {
            line.title = "Still waiting to be claimed"
            line.sub = "on LEZ, not claimed yet: claim it with Receive"
            line.amount = grouped(left[page])
            line.sign = ""
            return line
        }
    // Sending, failed or unknown, money into the page isn't called received and carries
    // no "+"; its state says what happened (payee-page-failed-or-pending-payment-reads-received-plus).
    var landed = landedIn(e)
    if (kind === "receive" && e.claimed !== false) {
        var credit = received[page]
        if (credit)
            // Credited here: this account's own amount; the fee is told only where it was paid.
            return function(line) {
                line.amount = grouped(credit)
                line.sign = landed ? "+" : ""
                if (!landed)
                    line.title = "Claim of waiting payments"
                if (e.from !== page && fee)
                    line.sub = line.sub.replace(" · fee " + grouped(fee), "")
                return line
            }
        if (e.from === page)
            // The payer only paid the claim's fee.
            return function(line) {
                line.title = "Paid the fee to claim waiting payments"
                line.sub = "on LEZ, into your other private accounts"
                line.amount = fee ? grouped(fee) : ""
                line.sign = fee ? "-" : ""
                return line
            }
        return null
    }
    var incoming = kind === "pay_account" || kind === "token_send" || kind === "bridge" || kind === "send"
    if (incoming && e.to === page && e.from !== page)
        // Money into this account: received, and its fee was the payer's.
        return function(line) {
            var from = e.from ? " from " + whoShown(e.from) : ""
            if (kind === "pay_account")
                line.title = (landed ? "Received" : "Payment") + from
            else if (kind === "token_send")
                line.title = (landed ? "Received a token" : "Token payment") + from
            else if (kind === "send")
                line.title = landed ? "Received on the Logos Blockchain" : "Payment on the Logos Blockchain"
            if (kind === "pay_account" || kind === "token_send")
                line.sub = "on LEZ"
            else if (fee)
                line.sub = line.sub.replace(" · fee " + grouped(fee), "")
            line.sign = line.amount === "" || !landed ? "" : "+"
            return line
        }
    return null
}

/// The entries that involve `who` (a LEZ account, a node key or a name): as payer, payee,
/// name, or an account a claim credited.
export function activityFor(rows, who, zone) {
    var list = Array.isArray(rows) ? rows : []
    // The same keys make the same LEZ account ID on every zone, and what it holds there
    // is other money: a LEZ account's page lists the zone in use (`zone`) only.
    var lezPage = zone !== undefined && /^(Public|Private)\//.test(String(who))
    return list.filter(function(e) {
        if (lezPage && (e.chain === "lez" || e.kind === "bridge") && (e.zone || "home") !== zone)
            return false
        var credited = e.received && typeof e.received === "object" && Object.prototype.hasOwnProperty.call(e.received, who)
        var waiting = e.left && typeof e.left === "object" && Object.prototype.hasOwnProperty.call(e.left, who)
        return e.from === who || e.to === who || e.name === who || credited || waiting
    })
}


/// A Logos Blockchain plan's fee for its confirm step: the node's own figure for this
/// very transaction (`fee_about`, decimal text, from chain_send / chain_split / chain_bridge
/// plans) or, without one, when the fee will be known.
export function chainFeeLine(plan) {
    if (!plan)
        return ""
    if (plan.in_flight === true)
        return "Another transfer from this key is still waiting to land, and its notes can't be spent twice. Wait a minute for it to land, then try again."
    if (plan.cannot_pay_fee === true)
        return "That key's notes can't pay this amount and its fee: send a little less."
    if (/^\d+$/.test(String(plan.fee_about)))
        return "About " + groupDigits(plan.fee_about) + ", paid from the key's notes: your node's figure at its gas prices now. The fee paid is shown once it's sent."
    return "Your node couldn't say in advance. The fee paid is shown once it's sent."
}

/// What one leader-reward claim costs, from chain_claim's plan: its `fee_about` beside the
/// one reward it cashes, with the module's `warning` when the fee is more.
export function claimFeeLine(plan) {
    if (!plan || !/^\d+$/.test(String(plan.fee_about)))
        return ""
    var line = "Each claim costs about " + groupDigits(plan.fee_about) + " in fees and cashes one reward of " + groupDigits(plan.reward_per_voucher || "0") + "."
    return plan.warning ? line + " " + plan.warning : line
}

// ── An app's inscription request (Wallet Provider SDK, LP-0022) ───────

/// The heading of an inscription request's sheet.
export function inscribeTitle(request) {
    return request ? String(request.app || "An app") + " asks you to inscribe a testimonial on the Logos Blockchain" : ""
}

/// One chain account offered for an inscription, in words: {title, sub}.
export function inscribeChoice(choice) {
    var c = choice || {}
    return { title: chainKeyTitle(c), sub: keyShown(c.key) + " · holds " + grouped(c.balance || "0") }
}

/// The account ticked when an inscription request is first seen: the first that holds
/// anything (an empty one can't pay the fee), or none.
export function inscribeFirstChoice(choices) {
    var holding = (choices || []).filter(function(c) { return /^\d+$/.test(String(c.balance)) && !/^0+$/.test(String(c.balance)) })
    return holding.length > 0 ? holding[0].key : ""
}

/// The sheet's fee line for the chosen account: {text, tone}. `plan` is chain_inscribe's
/// plan for it (null while it is read), `problem` why the node refused the plan.
export function inscribeFee(chosen, plan, problem) {
    if (!chosen)
        return { text: "Choose the account that pays it", tone: "plain" }
    if (problem)
        return { text: String(problem), tone: "bad" }
    if (!plan)
        return { text: "Asking your node…", tone: "plain" }
    if (/^\d+$/.test(String(plan.fee_estimate)))
        return { text: "About " + groupDigits(plan.fee_estimate) + ", paid from this account: your node's figure at its gas prices now. The fee paid is shown once it's sent.", tone: "plain" }
    return { text: "Your node couldn't say in advance. The fee paid is shown once it's sent.", tone: "plain" }
}

/// Whether the inscription on the sheet may be approved: an account is chosen, the node
/// priced it for that account (or couldn't say), and nothing is on its way.
export function inscribeCanApprove(state) {
    var s = state || {}
    if (!s.request || s.busy || !s.chosen || s.request.node)
        return false
    if (s.risky && !s.understood)
        return false
    return !!s.plan && !s.problem
}

// ── The bridge test network (LEZ 0.4), read only ─────────────────────────

/// What the two networks are called, everywhere in the app.
export var NET_03 = { id: "0.3", version: "LEZ 0.3", name: "public Logos testnet" }
export var NET_04 = { id: "0.4", version: "LEZ 0.4", name: "bridge test network" }

/// Every public account the wallet lists, as `Public/<id>`, once each, in order: from
/// `cli_accounts`, else from `cli_balance` (its `accounts`, then its default public one).
/// Empty accounts count too: what one holds on LEZ 0.3 says nothing about LEZ 0.4.
export function publicAccounts(cliBalance, accounts) {
    var out = []
    function add(text) {
        var id = String(text || "").replace(/^Public\//, "")
        if (/^[1-9A-HJ-NP-Za-km-z]{32,44}$/.test(id) && out.indexOf("Public/" + id) < 0)
            out.push("Public/" + id)
    }
    ;(Array.isArray(accounts) ? accounts : []).forEach(function(a) { if (a && a.kind === "public") add(a.account) })
    if (cliBalance && Array.isArray(cliBalance.accounts))
        cliBalance.accounts.forEach(function(a) { if (a && a.kind === "public") add(a.account) })
    if (cliBalance && cliBalance["public"] && cliBalance["public"].account)
        add(cliBalance["public"].account)
    return out
}

/// `first`, then those of `more` it doesn't have, each `Public/<id>` once.
export function mergedAccounts(first, more) {
    var out = (Array.isArray(first) ? first : []).slice()
    ;(Array.isArray(more) ? more : []).forEach(function(a) { if (typeof a === "string" && out.indexOf(a) < 0) out.push(a) })
    return out
}

/// Why `text` isn't a public account ID to watch, or "": base58 of 32 to 44 characters,
/// with an optional `Public/`. The module checks it decodes to exactly 32 bytes.
export function publicIdProblem(text) {
    var id = String(text === null || text === undefined ? "" : text).trim()
    if (id === "") return "Paste a public account ID."
    if (/^Private\//.test(id)) return "A private account can't be read on the bridge test network yet."
    id = id.replace(/^Public\//, "")
    return /^[1-9A-HJ-NP-Za-km-z]{32,44}$/.test(id) ? "" : "That isn't a public account ID: 32 to 44 letters and digits, without 0, O, I or l."
}

/// `digits` base units (decimal text) shown with `decimals` places, trailing zeros cut:
/// `100000000000000000`, 18 → `0.1`; `123456789`, 6 → `123.456789`; whole part grouped.
export function decimalShown(digits, decimals) {
    var text = String(digits === null || digits === undefined ? "" : digits).replace(/^0+(?=\d)/, "")
    var places = Number(decimals) || 0
    if (!/^\d+$/.test(text)) return text
    if (places <= 0) return grouped(text)
    while (text.length <= places) text = "0" + text
    var whole = text.substr(0, text.length - places), part = text.substr(text.length - places).replace(/0+$/, "")
    return grouped(whole) + (part ? "." + part : "")
}

/// The status lines of a `bridge_net_balances` answer: `{line, tone, reset}`. `clock` turns
/// seconds into a time of day. `tone` is "plain", "warn" or "bad"; `reset` is "" or the
/// sentence telling the network was reset.
export function bridgeNetStatus(value, clock) {
    var v = value || {}, read = v.read || null, error = String(v.error || "")
    var out = { line: "", tone: "plain", reset: "" }
    if (read && read.reset_at)
        out.reset = "The bridge test network was reset at " + clock(read.reset_at) + ". What was held there before is gone; balances start again."
    if (!read) {
        out.line = error ? "Can't read the bridge test network: " + error.replace(/^can't reach the bridge test network: /, "") : "Reading the bridge test network…"
        out.tone = error ? "bad" : "plain"
        return out
    }
    if (error) {
        out.line = "Can't reach the bridge test network now. Showing what it said at " + clock(read.read_at) + "."
        out.tone = "warn"
        return out
    }
    out.line = "Updated " + clock(read.read_at) + " · block " + grouped(read.height)
    return out
}

/// The blocks of `bridgeNetRows` the LEZ 0.4 card shows: every account that holds something
/// there, and every watched one; the wallet's accounts the network has never seen are folded
/// into one line (`empty`, how many) unless `showEmpty`.
export function bridgeNetShown(rows, showEmpty) {
    var list = Array.isArray(rows) ? rows : []
    var empty = list.filter(function(r) { return !r.found && !r.watched })
    return { shown: showEmpty ? list : list.filter(function(r) { return r.found || r.watched }), empty: empty.length }
}

/// One block per public account of a `bridge_net_balances` read: `{account, title, found,
/// assets: [{symbol, name, amount}]}`, native LEZ first, then each bridged token with its
/// decimals. An account the network has never seen has no assets (`found` false).
/// What a module call's raw reply means: `{kind, value, error}`. `kind` is "ok", "error" (the
/// module answered with an error), "busy" (no answer in time, or the host's "timeout") or
/// "missing" (the host says the module or the method isn't there, as "Module not connected" or
/// "Module not reachable"). Only "missing" asks for an install:
/// a busy module must never be taken for an absent one.
export function replyKind(reply) {
    if (reply === undefined || reply === null || String(reply) === "")
        return { kind: "busy", value: null, error: "" }
    var text = typeof reply === "string" ? reply : ""
    var parsed = reply
    for (var depth = 0; depth < 2 && typeof parsed === "string"; depth++) {
        try { parsed = JSON.parse(parsed) } catch (error) { break }
    }
    // The module's own busy answer (its call waited its turn too long): busy, not an error.
    if (parsed && typeof parsed === "object" && parsed.busy === true)
        return { kind: "busy", value: null, error: String(parsed.error || "") }
    if (parsed && typeof parsed === "object" && parsed.success !== undefined)
        return parsed.success ? { kind: "ok", value: parsed.value, error: "" } : { kind: "error", value: null, error: String(parsed.error) }
    var said = text !== "" ? text : JSON.stringify(parsed)
    // The host's own words for a module that isn't there (LogosQmlBridge: "Module not connected",
    // "Module not reachable", "Module not reachable yet"); "timeout" and no answer are busy.
    if (/module not connected|module not reachable|no such (module|method)|unknown (module|method)|method not found|module not found|not loaded|is not installed/i.test(said))
        return { kind: "missing", value: null, error: said }
    return { kind: "busy", value: null, error: said }
}

/// A new `cli_status` answer merged with the last one: a wallet seen stays seen unless the module
/// says plainly there is none (`wallet: false`, its store is gone). An answer that has the store
/// but couldn't read which account is the public one (its file was being written) keeps the
/// account last read, so a passing read never sends a person with a wallet back to Welcome.
export function mergeCliStatus(previous, next) {
    if (!next || typeof next !== "object")
        return previous
    var merged = Object.assign({}, next)
    if (previous && previous.wallet && merged.wallet && !merged["public"] && previous["public"])
        merged["public"] = previous["public"]
    if (previous && previous.wallet && merged.wallet && !merged["private"] && previous["private"])
        merged["private"] = previous["private"]
    return merged
}

/// Which public registration to ask about next (`register_name`), or null: the first one whose
/// reveal is due (`ready_at` passed, by `now`), unless an ask is still out and younger than
/// `patienceMs`. An ask that never answered stops holding the rest after that.
export function revealNext(pending, now, askedAt, patienceMs) {
    if (askedAt && now - askedAt < patienceMs)
        return null
    for (var i = 0; i < (pending || []).length; i++) {
        var item = pending[i]
        if (item.ready_at !== null && item.ready_at !== undefined && now < item.ready_at)
            continue
        return item
    }
    return null
}

/// A registration under way as a line: when its reveal goes out and by when it must have.
/// `clock` turns milliseconds into a time of day.
export function revealLine(item, now, clock) {
    var name = String(item.display || item.name)
    if (item.stage === "revealed")
        return name + ": the reveal was sent; it shows as yours once it lands."
    var line = name + ": "
    if (item.ready_at && now < item.ready_at)
        line += "the reveal goes out at " + clock(item.ready_at) + " (in " + Math.ceil((item.ready_at - now) / 60000) + " min)"
    else
        line += "the reveal is due now and goes out while Names is open"
    if (item.reveal_by) {
        var left = Math.max(0, Math.floor((item.reveal_by - now) / 60000))
        line += left > 120 ? "; it can still be sent for " + Math.floor(left / 60) + " h"
                           : "; it must be sent by " + clock(item.reveal_by) + " (" + left + " min left)"
    }
    return line + "."
}

/// What the Welcome says when the names tool for `net` ("0.3" or "0.4") is too old for the
/// module (it has no `module` command). `line` is what the tool says its version is, or "".
/// It names the network, offers no install (none is published that would fix it), and says that
/// the other network is unaffected.
export function cliOutdatedLine(net, line) {
    var tool = line !== "" ? " (" + line + ")" : ""
    if (net === "0.4")
        return "The names tool set for LEZ 0.4" + tool + " is too old for it. Give the path of a LEZ 0.4 names tool in Settings, App settings. LEZ 0.3 is unaffected."
    return "The names tool this app uses for LEZ 0.3" + tool + " is too old for that network as it is now, so creating a wallet there doesn't work with it. LEZ 0.4 is unaffected: switch to it above."
}

export function bridgeNetRows(read) {
    if (!read || !Array.isArray(read.accounts)) return []
    // With more than one bridge, each token row says which bridge listed it.
    var many = Array.isArray(read.bridges) && read.bridges.length > 1
    return read.accounts.map(function(a) {
        var assets = []
        if (a.found) {
            assets.push({ symbol: "LEZ", name: "Native", amount: grouped(a.native) })
            ;(a.tokens || []).forEach(function(t) {
                var name = t.name ? String(t.name) : "Bridged token"
                // "bridge v2" / "bridge v3" from the module (its built-in labels), else the short ID.
                if (many && t.bridge) name += " · " + (t.bridge_label ? String(t.bridge_label) : "bridge " + keyShown(String(t.bridge)))
                assets.push({ symbol: String(t.symbol || ""), name: name, amount: decimalShown(t.amount, t.decimals) })
            })
        }
        var id = keyShown(String(a.account || "").replace(/^Public\//, ""))
        return { account: a.account, title: (a.watched ? "Watched account " : "Public account ") + id, found: !!a.found, watched: !!a.watched, other: Number(a.other_tokens) || 0, assets: assets }
    })
}

/// A token amount in base units shown with its decimals: ("100000000", 6) → "100".
export function withDecimals(base, decimals) {
    var digits = String(base === null || base === undefined ? "" : base)
    var places = Number(decimals) || 0
    if (!/^[0-9]+$/.test(digits) || places <= 0)
        return digits === "" ? digits : grouped(digits)
    while (digits.length <= places)
        digits = "0" + digits
    var whole = digits.substr(0, digits.length - places)
    var fraction = digits.substr(digits.length - places).replace(/0+$/, "")
    return grouped(whole) + (fraction === "" ? "" : "." + fraction)
}

/// The holdings of the token sources the person accepted (`token_source_holdings`), as wallet
/// groups: one per token program, a row per account and token. The names, symbols and
/// decimals are the app's words; the definition ID is always shown with them.
export function bridgedGroups(sources) {
    if (!Array.isArray(sources))
        return []
    var groups = []
    sources.forEach(function(source) {
        var rows = (source.holdings || []).map(function(h) {
            var label = String(h.symbol || h.name || h.chain_name || "Token")
            return { id: "bridged-" + h.account + "-" + h.definition, kind: "token", title: label,
                     sub: withDecimals(h.balance, h.decimals) + " " + label + " · " + (h.account_kind === "private" ? "Private" : "Public") + " · " + (h.origin ? h.origin + " · " : "") + keyShown(h.definition),
                     address: h.definition, balance: String(h.balance), shown: withDecimals(h.balance, h.decimals) + " " + label,
                     fromPrivate: h.account_kind === "private", definition: h.definition, key: "", canSend: false, account: h.account,
                     tokenProgram: source.token_program }
        })
        if (rows.length > 0 || source.error)
            groups.push({ id: "bridged-" + source.token_program, title: "Bridged tokens", letter: "E", total: null, tone: source.error ? "alarm" : "plain",
                          note: (source.app ? "from " + source.app + " · " : "") + keyShown(source.token_program) + (source.error ? " · " + source.error : ""), rows: rows })
    })
    return groups
}


// ── Untrusted text (from lez-program-registry/registry_browser/qml/logic.mjs, tested there) ──

/// Text an app or a chain wrote, safe to show as one plain line: line breaks become ⏎, and
/// control, bidi, zero-width and other invisible characters are dropped.
export function clean(s) {
    return String(s === undefined || s === null ? "" : s).replace(/\r\n|[\n\r\u000B\u000C\u0085\u2028\u2029]/g, "\u23CE").replace(/[\u0000-\u001F\u007F-\u009F\u00AD\u061C\u180E\u200B-\u200F\u202A-\u202E\u2060-\u2064\u2066-\u206F\uFEFF\uFFF9-\uFFFB]|\uDB40[\uDC00-\uDC7F]/g, "")
}

/// The lines of a relayed call's sheet, each cleaned: `{lines, warnings, privacy, after, says, why}`.
export function relayedSheet(sheet) {
    var s = sheet || {}
    var list = function(a) { return (Array.isArray(a) ? a : []).map(clean) }
    return { lines: list(s.lines), warnings: list(s.warnings), privacy: clean(s.privacy), after: clean(s.after),
             says: s.app_says ? clean(s.app_says) : "", why: list(s.why), refused: s.refused === true, hash: clean(s.hash) }
}
