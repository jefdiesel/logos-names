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

/// An entry's value for reading: a long unbroken value is shortened in the middle.
export function entryShown(value) {
    var text = String(value)
    return text.length > 40 && !/\s/.test(text) && !/^https?:/.test(text) ? text.substr(0, 12) + "…" + text.substr(text.length - 8) : text
}

/// Whether an amount (decimal text) is more than nothing.
export function someAmount(digits) {
    return digits !== null && digits !== undefined && !/^0*$/.test(String(digits))
}

/// The largest token amount: token balances are 128-bit numbers.
const TOKEN_MAX = "340282366920938463463374607431768211455"

/// Why `text` isn't a token amount (whole base units, 1 to 2^128 - 1, `,` and
/// `_` may group digits), or "" if it is one.
export function tokenAmountProblem(text) {
    var digits = String(text === null || text === undefined ? "" : text).trim().replace(/[,_]/g, "")
    if (!/^[0-9]+$/.test(digits))
        return "Enter a whole amount of the token's base units, such as 1000."
    digits = digits.replace(/^0+/, "")
    if (digits === "")
        return "Enter an amount above zero."
    if (digits.length > TOKEN_MAX.length || (digits.length === TOKEN_MAX.length && digits > TOKEN_MAX))
        return "That is more than a token amount can be."
    return ""
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

/// An account as listed in Settings: its nickname, else its kind; the default
/// account of each kind is the one the wallet pays and registers from.
export function accountTitle(entry) {
    if (!entry)
        return ""
    var kind = entry.kind === "private" ? "Private account" : "Public account"
    var label = String(entry.label === null || entry.label === undefined ? "" : entry.label).trim()
    return label !== "" ? label : kind
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

/// A Logos Blockchain key cut down for a line of text: `66ecf4bc…a81b`.
export function keyShown(key) {
    var text = String(key || "")
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
export function walletGroups(cliBalance, accounts, holdings, watched, tokenNames, chainAccounts, chainDown, stake) {
    var groups = []
    var lez = []
    if (Array.isArray(accounts) && accounts.length > 0) {
        accountsShown(accounts).forEach(function(a) {
            var priv = a.kind === "private"
            var label = String(a.label === null || a.label === undefined ? "" : a.label).trim()
            lez.push({ id: "lez-" + a.account, kind: "lez", title: label !== "" ? label : a["default"] ? (priv ? "Main" : "Public") : (priv ? "Private account" : "Public account"),
                       sub: (priv ? "Private" : "Public") + (a["default"] ? (priv ? " · pays and registers" : " · bridged tokens land here") : " · " + keyShown(a.account.replace(/^(Public|Private)\//, ""))),
                       address: a.account, balance: a.balance === null || a.balance === undefined ? null : String(a.balance), fromPrivate: priv, definition: "", key: "",
                       canSend: !!a["default"], account: a.account })
        })
    } else if (cliBalance && typeof cliBalance === "object") {
        lez.push({ id: "lez-private", kind: "lez", title: "Main", sub: "Private · pays and registers", address: cliBalance.account ? "Private/" + cliBalance.account : "",
                   balance: cliBalance.balance === undefined ? null : String(cliBalance.balance), fromPrivate: true, definition: "", key: "", canSend: true, account: "" })
        if (cliBalance["public"])
            lez.push({ id: "lez-public", kind: "lez", title: "Public", sub: "Public · bridged tokens land here", address: "Public/" + cliBalance["public"].account,
                       balance: String(cliBalance["public"].balance), fromPrivate: false, definition: "", key: "", canSend: true, account: "" })
    }
    if (lez.length > 0) {
        var lezTotal = lez.reduce(function(sum, r) { return sum === null || r.balance === null ? sum : addDigits(sum, r.balance) }, "0")
        groups.push({ id: "lez", title: "LEZ", letter: "L", total: lezTotal, note: lez.length === 1 ? "1 account" : lez.length + " accounts", tone: "plain", rows: lez })
    }

    var tokens = walletAssets(null, holdings, watched, tokenNames, null, false).map(function(r) {
        return { id: r.id, kind: "token", title: r.title, sub: r.where + " · token on LEZ", address: r.definition, balance: r.balance, fromPrivate: r.fromPrivate,
                 definition: r.definition, key: "", canSend: r.balance !== "0", account: "" }
    })
    if (tokens.length > 0)
        groups.push({ id: "tokens", title: "Tokens on LEZ", letter: "T", total: null, note: tokens.length === 1 ? "1 token" : tokens.length + " tokens", tone: "plain", rows: tokens })

    if (chainAccounts || chainDown) {
        var keys = (chainAccounts || []).map(function(a) {
            return { id: "chain-" + a.key, kind: "chain", title: "Node key", sub: "Public · " + keyShown(a.key), address: a.key,
                     balance: a.balance === null || a.balance === undefined ? null : String(a.balance), fromPrivate: false, definition: "", key: a.key, canSend: true, account: "" }
        })
        var total = chainAccounts ? keys.reduce(function(sum, r) { return sum === null || r.balance === null ? sum : addDigits(sum, r.balance) }, "0") : null
        var waiting = stake && total !== null ? subDigits(total, stake.staked) : null
        var note = chainDown ? "Node not running" : (keys.length === 1 ? "1 key on your node" : keys.length + " keys on your node")
            + (stake === null || stake === undefined || total === null ? "" : waiting === "0" ? " · all staked" : waiting !== null ? " · " + grouped(waiting) + " not counted yet" : " · " + grouped(stake.staked) + " staked")
        groups.push({ id: "chain", title: "Logos Blockchain", letter: "B", total: total, note: note, tone: chainDown ? "alarm" : stake ? "stake" : "plain", rows: keys })
    }
    return groups
}

/// The Staking table: what the node holds, what is staked, what isn't counted yet.
export function stakeTable(stake, chainAccounts) {
    if (!stake) return null
    var held = (chainAccounts || []).reduce(function(sum, a) { return sum === null || a.balance === null ? sum : addDigits(sum, a.balance) }, "0")
    var waiting = held === null ? null : subDigits(held, stake.staked)
    return { held: held, staked: String(stake.staked), waiting: waiting === null ? "0" : waiting, claimable: String(stake.claimable || "0"), vouchers: stake.vouchers || 0 }
}
