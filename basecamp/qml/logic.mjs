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
    case "checking":
        return { text: "Checking who publishes it…", tone: "plain" }
    }
    return null
}

// The accounts ticked when a request's sheet opens (or what it knows changes, requestTickKey).
// Nothing while the Program Registry is still being asked (`checking`) or when the app's publisher
// check failed (it may be a fake, M-2). A declaration or a token source is ticked only when
// exactly one account is shared with the app (that one), else nothing; a request naming an account
// ticks that one; anything else ticks nothing.
export function preTicked(request, publisherFailed, hint) {
    if (!request || request.checking === true || publisherFailed)
        return []
    var choices = request.choices || []
    var offered = function(account) { return choices.some(function(choice) { return choice.account === account }) }
    // The account the app asked about, when it named one and it is offered.
    if (typeof request.account === "string" && offered(request.account))
        return [request.account]
    // An access request's hint (`account_hint`, or the claim this app's own screen is running for
    // that app): only that box, when the wallet offers it. The person still decides.
    var hinted = typeof request.account_hint === "string" ? request.account_hint : hint
    if (request.kind === "connect" && typeof hinted === "string" && offered(hinted))
        return [hinted]
    var given = choices.filter(function(choice) { return choice.granted })
    if (request.kind === "programs" || request.kind === "token_source") {
        // The claim this app's own screen is running for: its account, when it is shared with the
        // app (a wallet that shares two accounts with the bridge ticked none, and Approve stayed
        // dim with the choice hidden under Details: the owner's first v4 claim, 15 Nov).
        var bare = function(a) { return String(a || "").replace(/^(Public|Private)\//, "") }
        var hintedGiven = typeof hinted === "string" && hinted !== "" ? given.filter(function(choice) { return bare(choice.account) === bare(hinted) }) : []
        if (hintedGiven.length === 1)
            return [hintedGiven[0].account]
        return given.length === 1 ? [given[0].account] : []
    }
    // Anything else, access, payments and calls included: only an account the app named is
    // ticked; otherwise the person chooses (the wallet's main account is not necessarily the one
    // the app is for: an imported account comes after it). SF-4.
    return []
}

/// Whether the request sheet's Approve can be pressed. `s`: {request, busy, chosen (the ticked
/// accounts), risky (the request needs the warning's tick), understood (that tick)}. An account
/// must be ticked for every kind; when none could be ticked for the person, the short sheet shows
/// the accounts (appNeedsChoice), so Approve is never dim with nothing to do on the screen.
export function canApprove(s) {
    var st = s || {}
    var request = st.request
    if (!request || st.busy || !Array.isArray(st.chosen) || st.chosen.length === 0)
        return false
    // Asked for another zone than the one in use: the same account ID is another account there.
    if (request.zone_in_use === false)
        return false
    // The Program Registry is still being asked about it (the module refuses it too, M-2).
    if (request.checking === true)
        return false
    if (st.risky && !st.understood)
        return false
    // A private call can be approved only once the wallet has simulated it and found nothing it
    // refuses (docs/wallet/BRIDGE-SDK.md).
    if (request.kind === "private_call" && (request.check !== "ready" || !request.sheet || request.sheet.refused))
        return false
    // A call someone else pays for: only what the wallet built and didn't refuse.
    if (request.kind === "relayed_call" && (!request.sheet || request.sheet.refused))
        return false
    return true
}

/// Whether the Add box's failure says the name isn't this wallet's (not registered, or held by
/// someone else): then the box offers Register instead of the CLI's words.
export function addNameNotYours(line) {
    return /doesn't hold|does not hold|is not registered|isn't registered|not registered/i.test(String(line || ""))
}

/// The accounts a request's sheet offers. A program declaration for the claim this app's own
/// screen is running offers only the claim's account (`hint`) when the wallet offers it: another
/// one can't be the claim's (15 Nov, the owner picked another and the claim didn't go on).
export function sheetChoices(request, hint) {
    var choices = request && Array.isArray(request.choices) ? request.choices : []
    if (!request || request.kind !== "programs" || typeof hint !== "string" || hint === "")
        return choices
    var bare = function(a) { return String(a || "").replace(/^(Public|Private)\//, "") }
    var only = choices.filter(function(c) { return bare(c.account) === bare(hint) })
    return only.length > 0 ? only : choices
}

// What the request sheet's ticks were set for: the request, and what the wallet knows about it
// (the publisher check, whether the Program Registry is still being asked, how many warnings).
// When any of it changes the ticks are set again, so an account ticked while "checking" is
// unticked when the answer is a warning (M-2).
export function requestTickKey(request) {
    if (!request)
        return ""
    var publisher = request.publisher && typeof request.publisher === "object" ? String(request.publisher.status || "") : ""
    var warnings = (request.sheet && request.sheet.warnings ? request.sheet.warnings.length : 0) + (request.warnings ? request.warnings.length : 0)
    return [request.id, publisher, request.checking === true ? "checking" : "", warnings].join("|")
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
/// `keep`: accounts shown even with nothing there yet (where a receive arrives), `Public/` or bare.
export function bridgeNetShown(rows, showEmpty, keep) {
    var list = Array.isArray(rows) ? rows : []
    var bare = function(a) { return String(a || "").replace(/^Public\//, "") }
    var kept = (Array.isArray(keep) ? keep : []).map(bare)
    var shows = function(r) { return r.found || r.watched || kept.indexOf(bare(r.account)) >= 0 }
    var empty = list.filter(function(r) { return !shows(r) })
    return { shown: showEmpty ? list : list.filter(shows), empty: empty.length }
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
    // The module's own busy answer: the call came while another was waiting on the node or the
    // Program Registry, and was NOT taken (nothing happened; ask again). `notTaken` tells it from
    // no answer at all (a timeout), after which the call may have been taken.
    if (parsed && typeof parsed === "object" && parsed.busy === true)
        return { kind: "busy", value: null, error: String(parsed.error || ""), notTaken: true }
    if (parsed && typeof parsed === "object" && parsed.success !== undefined)
        return parsed.success ? { kind: "ok", value: parsed.value, error: "" } : { kind: "error", value: null, error: String(parsed.error) }
    var said = text !== "" ? text : JSON.stringify(parsed)
    // After a module was unloaded or reinstalled, this open window's token is no longer
    // recognised: every call is refused until Names is opened again.
    if (/auth token|unauthori[sz]ed/i.test(said))
        return { kind: "stale", value: null, error: said }
    // The host's own words for a module that isn't there (LogosQmlBridge: "Module not connected",
    // "Module not reachable", "Module not reachable yet"); "timeout" and no answer are busy.
    if (/module not connected|module not reachable|no such (module|method)|unknown (module|method)|method not found|module not found|not loaded|is not installed/i.test(said))
        return { kind: "missing", value: null, error: said }
    return { kind: "busy", value: null, error: said }
}

/// What `normalize`'s raw reply means: it answers the name as plain text ("" for text that isn't
/// a name), not an envelope, so it is sorted here: `{kind: "ok", value: name}`, or "busy" (the
/// module's busy answer, or no answer) or "missing" as replyKind says. Never "[object Object]".
export function normalizedReply(reply) {
    // Through callModuleAsync, Basecamp 0.3.2 hands a plain string reply JSON-encoded, once or
    // twice ("\"jef04\""): unwrapped first (the blocking callModule gave it bare; read as "busy"
    // every time on the M2, 15 Nov).
    for (var depth = 0; depth < 2 && typeof reply === "string" && reply.length >= 2 && reply.charAt(0) === '"'; depth++) {
        try { reply = JSON.parse(reply) } catch (error) { break }
    }
    if (reply === "")
        return { kind: "ok", value: "", error: "" }
    if (typeof reply === "string" && /^[a-z0-9-]{1,32}$/.test(reply))
        return { kind: "ok", value: reply, error: "" }
    var answer = replyKind(reply)
    return answer.kind === "ok" && typeof answer.value === "string" ? answer : { kind: answer.kind === "missing" ? "missing" : "busy", value: null, error: answer.error, notTaken: answer.notTaken === true }
}

/// Whether `cli_status` says plainly that there is no wallet on this network (the only thing that
/// opens Welcome): `wallet: false`. Not yet read (null), a busy or failed read, or a wallet with
/// no name never does.
export function noWalletSaid(status) {
    return !!status && typeof status === "object" && status.wallet === false
}

/// Whether an answer to a read asked on network `askedOn` may be shown now that `nowOn` is in
/// use: only on the same network. A late answer to an ask made before a switch is dropped (S-A).
export function sameNetAnswer(askedOn, nowOn) {
    return String(askedOn || "") === String(nowOn || "")
}

/// `status` marked with the network it was read on ("0.3" or "0.4"), for mergeCliStatus.
export function onNet(status, net) {
    return status && typeof status === "object" ? Object.assign({}, status, { net: net }) : status
}

/// A new `cli_status` answer merged with the last one: a wallet seen stays seen unless the module
/// says plainly there is none (`wallet: false`, its store is gone). An answer that has the store
/// but couldn't read which account is the public one (its file was being written) keeps the
/// account last read, so a passing read never sends a person with a wallet back to Welcome.
export function mergeCliStatus(previous, next) {
    if (!next || typeof next !== "object")
        return previous
    var merged = Object.assign({}, next)
    // Only an answer read on the same network carries anything over (S-A): a 0.3 public account
    // is never shown as the 0.4 one.
    if (previous && (previous.net || "") !== (merged.net || ""))
        return merged
    if (previous && previous.wallet && merged.wallet && !merged["public"] && previous["public"])
        merged["public"] = previous["public"]
    if (previous && previous.wallet && merged.wallet && !merged["private"] && previous["private"])
        merged["private"] = previous["private"]
    return merged
}

/// Which public registration to ask about next (`register_name`), or null: the first one whose
/// reveal is due (`ready_at` passed, by `now`), unless an ask is still out and younger than
/// `patienceMs`. An ask that never answered stops holding the rest after that.
export function revealNext(pending, now, askedAt, patienceMs, refused) {
    if (askedAt && now - askedAt < patienceMs)
        return null
    for (var i = 0; i < (pending || []).length; i++) {
        var item = pending[i]
        if (item.ready_at !== null && item.ready_at !== undefined && now < item.ready_at)
            continue
        // One whose reveal was refused (its commitment never landed, or is too old) waits its
        // retry time, so the names behind it are still revealed (SF-A).
        if (refused && refused[item.name] && now < refused[item.name])
            continue
        return item
    }
    return null
}

/// After the module refused `name`'s reveal (not busy, not no answer): when to ask again for it.
export const REVEAL_REFUSED_RETRY_MS = 30 * 60 * 1000

/// Whether a failed read is to be asked again soon: the module was busy or didn't answer.
export function askAgainAfter(message, busy, noAnswer) {
    return message === busy || message === noAnswer
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

export function bridgeNetRows(read, own) {
    if (!read || !Array.isArray(read.accounts)) return []
    // With more than one bridge, each token row says which bridge listed it.
    var many = Array.isArray(read.bridges) && read.bridges.length > 1
    // `own`: the LEZ 0.4 wallet's own public accounts (given on LEZ 0.4). An account read here
    // that isn't one of them and isn't watched is a LEZ 0.3 account this wallet can't sign for:
    // shown as view only, with no Copy, after the wallet's own (it nearly took a deposit once).
    // One the 0.4 wallet holds too (a restored phrase) is shown once, as its own.
    var mine = Array.isArray(own) ? own : null
    var rows = read.accounts.map(function(a) {
        var assets = []
        if (a.found) {
            assets.push({ symbol: "LEZ", name: "Native", amount: grouped(a.native) })
            ;(a.tokens || []).forEach(function(t) {
                var name = t.name ? String(t.name) : "Bridged token"
                // "bridge v2" / "bridge v3" from the module (its built-in labels), else the short ID.
                if (many && t.bridge) name += " · " + (t.bridge_label ? String(t.bridge_label) : "bridge " + keyShown(String(t.bridge)))
                assets.push({ symbol: String(t.symbol || ""), name: name, amount: decimalShown(t.amount, t.decimals),
                              bridge: String(t.bridge || ""), base: String(t.amount || "0"), decimals: Number(t.decimals) || 0, definition: String(t.definition || "") })
            })
        }
        var id = keyShown(String(a.account || "").replace(/^Public\//, ""))
        var written = "Public/" + String(a.account || "").replace(/^Public\//, "")
        var viewOnly = mine !== null && !a.watched && mine.indexOf(written) < 0
        var title = viewOnly ? "LEZ 0.3 account " + id + ", view only: your LEZ 0.4 wallet can't sign for it" : (a.watched ? "Watched account " : "Public account ") + id
        return { account: a.account, title: title, found: !!a.found, watched: !!a.watched, viewOnly: viewOnly, copyable: !viewOnly, other: Number(a.other_tokens) || 0, assets: assets }
    })
    if (mine === null)
        return rows
    var rank = function(row) { return row.viewOnly ? 2 : row.watched ? 1 : 0 }
    return rows.map(function(row, at) { return { row: row, at: at } })
        .sort(function(x, y) { return rank(x.row) - rank(y.row) || x.at - y.at })
        .map(function(entry) { return entry.row })
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


// ── Claim a bridge deposit (the lez_bridge module, called from names_ui) ──
// The module's calls and replies: rfp-022 modules/NAMES-EMBED.md, Part A.

/// A raw lez_bridge reply: `{ok, value}` or `{ok: false, error, kind}`. Basecamp 0.3.2 encodes the
/// module's JSON text twice. `kind`: "missing" (not installed, or an older package), "starting"
/// (the host is (re)starting: a read may be sent again), "busy" (the module is still answering an
/// earlier call: Basecamp's "timeout"), "error" (the module's own answer).
export function bridgeReply(reply) {
    if (reply === undefined || reply === null || String(reply) === "")
        return { ok: false, kind: "busy", error: "The LEZ Bridge module gave no answer yet." }
    if (/auth token|unauthori[sz]ed/i.test(String(reply)))
        return { ok: false, kind: "stale", error: "Close the Names tab and open Names again to reconnect to the bridge." }
    var r = reply
    for (var depth = 0; depth < 2 && typeof r === "string"; depth++) {
        try { r = JSON.parse(r) } catch (e) { return { ok: false, kind: "error", error: "The LEZ Bridge module answered something that isn't JSON." } }
    }
    if (r && typeof r === "object" && "success" in r)
        return r.success ? { ok: true, value: r.value } : { ok: false, kind: "error", error: String(r.error || "The LEZ Bridge module did not say why.") }
    var said = r && typeof r === "object" ? String(r.error || "") : String(r)
    if (/auth token|unauthori[sz]ed/i.test(said))
        return { ok: false, kind: "stale", error: "Close the Names tab and open Names again to reconnect to the bridge." }
    if (/timeout/i.test(said))
        return { ok: false, kind: "busy", error: "The LEZ Bridge module is still answering an earlier call." }
    if (/method not found|unknown_method|no such (module|method)|module not found|not installed/i.test(said))
        return { ok: false, kind: "missing", error: said }
    if (/not connected|not reachable|unavailable|waiting for replica|crashed/i.test(said))
        return { ok: false, kind: "starting", error: said }
    return { ok: false, kind: "error", error: said }
}

/// What the claim screen shows, from what it knows. One of: "missing" (no LEZ Bridge package),
/// "looking" (the bridge module not heard from yet),
/// "start" (choose or paste the file), "checking", "refused" (the check's refusal), "elsewhere"
/// (the account isn't in this LEZ 0.4 wallet), "wrong_bridge" (a bridge the wallet doesn't read),
/// "ready" (Claim), "asking" (relayed_claim on its way), "needs" (a wallet sheet is up),
/// "uncertain" (a proposal got no answer), "running", "done", "failed".
export function claimStage(state) {
    var s = state || {}
    if (s.missing) return "missing"
    // The bridge module not heard from yet (it answers at once when it runs; a module that isn't
    // running takes the host's 10 s to say so).
    if (s.looking && !s.job && !s.check) return "looking"
    if (s.job) {
        if (s.job.state === "done") return "done"
        if (s.job.state === "failed" || s.job.state === "cancelled") return "failed"
        return "running"
    }
    if (s.uncertain) return "uncertain"
    if (s.needs) return "needs"
    if (s.asking) return "asking"
    if (s.checking) return "checking"
    if (!s.check) return "start"
    if (!s.check.ok) return "refused"
    if (s.accountProblem) return "elsewhere"
    if (s.bridgeProblem) return "wrong_bridge"
    return "ready"
}

/// Why the claim's account can't be used here, or "": it must be one of this LEZ 0.4 wallet's own
/// accounts (`accounts`: cli_accounts' list, or `Public/<id>` texts).
export function claimAccountProblem(account, accounts) {
    var want = String(account || "")
    if (want === "") return ""
    var held = (Array.isArray(accounts) ? accounts : []).map(function(a) { return typeof a === "string" ? a : String(a && a.account || "") })
    if (held.indexOf(want) >= 0) return ""
    return "This claim file pays " + want + ", which isn't in your LEZ 0.4 wallet, so the wallet can't sign its claim. "
        + "An account from your LEZ 0.3 wallet gets into the LEZ 0.4 one when you restore its recovery phrase there, or by importing that one account (import-account-04.sh); then check the file again."
}

/// Which bridge a claim is for (its bridge program) and whether this wallet reads it: `{label,
/// problem}`. `labels`: [[program, label]] (bridge_net's names); `bridges`: get_config's list.
export function claimBridge(program, bridges, labels) {
    var p = String(program || "")
    var label = ""
    ;(labels || []).forEach(function(pair) { if (pair[0] === p) label = pair[1] })
    if (label === "") label = p === "" ? "" : "bridge " + p.slice(0, 6) + "…"
    var listed = (Array.isArray(bridges) ? bridges : []).indexOf(p) >= 0
    return { label: label, problem: p !== "" && !listed ? "This claim file is for " + label + " (" + p + "), which your wallet doesn't read on LEZ 0.4. Add it in Settings, then check the file again." : "" }
}

/// The claim job of this check among `relayed_claims` (newest first): a running or finished one for
/// the same account and leaf, so a claim is never started twice.
export function claimJobFor(jobs, check) {
    if (!check || !check.deposit) return null
    var list = Array.isArray(jobs) ? jobs : []
    for (var i = 0; i < list.length; i++) {
        var j = list[i]
        // A cancelled or failed job is history: a new claim may start (the check id is the same
        // for every check of one deposit, so matching it forever looped "Try again", 15 Nov).
        if (j.state === "failed" || j.state === "cancelled") continue
        if (j.check === check.id) return j
        if (j.account === check.deposit.account && j.leaf_index !== undefined && j.leaf_index === check.deposit.leaf_index && j.state !== "failed" && j.state !== "cancelled") return j
    }
    return null
}

/// The running claim job among `relayed_claims`, for reopening the screen.
export function runningClaimJob(jobs) {
    var list = Array.isArray(jobs) ? jobs : []
    for (var i = 0; i < list.length; i++) if (list[i].state === "running") return list[i]
    return null
}

/// A claim job's steps, each with whether it is reached (the module's `phases`, else from `phase`).
export const CLAIM_PHASES = [
    { key: "approve", word: "Approved", hint: "You approve the claim in this app; your wallet signs it" },
    { key: "relay", word: "Relayed", hint: "The bridge's relay checks it, sends it and pays the fee" },
    { key: "confirm", word: "Confirmed", hint: "It waits for its block on the bridge test network" }
]
export function claimPhases(job) {
    var order = ["approve", "relay", "confirm", "done"]
    var at = order.indexOf(job && job.phase ? job.phase : "approve")
    return CLAIM_PHASES.map(function(p, i) {
        return { key: p.key, word: p.word, hint: p.hint, done: job && job.state === "done" ? true : i < at, now: i === at && !(job && job.state === "done") }
    })
}

/// What a failed `relayed_claim` said, as the screen's line and whether pressing Claim again is
/// safe: the wallet busy (nothing was sent) is; no answer (a request may be waiting) is not done
/// blindly: look at the confirmation first.
export function claimFailure(text) {
    var t = String(text || "")
    if (/Names Wallet is busy/i.test(t)) return { line: "Your Names Wallet was busy; nothing was sent. Press Claim again.", again: true }
    if (/didn't answer/i.test(t)) return { line: "Your Names Wallet didn't answer in time, so a claim may be waiting for you. Look at the confirmation in this app first; don't press Claim again until it is answered or gone.", again: false }
    if (/still answering an earlier call/i.test(t)) return { line: "The LEZ Bridge module is still answering an earlier call; nothing new was sent. Try again in a moment.", again: true }
    return { line: t, again: true }
}

/// The claim's refusal title (rfp-022 logic.mjs relayErrorTitle, the codes the check and the job
/// answer with).
export function claimErrorTitle(code) {
    var t = {
        bad_file: "That is not a usable claim file", private_account: "This deposit is for a private account",
        other_chain: "This deposit is on another chain", other_vault: "This deposit went to another vault",
        cfg_mismatch: "This deposit is for another bridge", token_not_registered: "Token not supported",
        bad_denomination: "Not a valid amount", wrong_amount: "Not a valid amount",
        wrong_leaf: "The claim file's leaf does not match", not_in_vault: "No such deposit in the vault",
        not_final: "Not final on Sepolia yet", root_not_attested: "Not recorded on the bridge test network yet",
        already_claimed: "Already claimed", minting_frozen: "Claims are paused", minting_frozen_token: "Claims of this token are paused",
        leaves_mismatch: "The vault's deposits don't add up here", no_bridge: "No bridge on this network",
        wrong_chain: "This bridge belongs to another network", bridge_unreadable: "The bridge's settings can't be read",
        bad_config: "The bridge module's settings are out of date", no_relay: "No claim relay for this bridge",
        relay_cap: "The relay's limit is reached", relay_funds: "The relay is out of funds", relay_once: "The relay already sent this claim",
        relay_refused: "The relay refused the claim", relay_other_bridge: "The relay no longer pays claims on this vault", relay_busy: "The relay is busy", relay_unavailable: "The relay is not answering",
        rate_limited: "Too many requests just now", unreachable: "The bridge test network can't be reached",
        eth_unreachable: "Sepolia can't be read", wrong_account: "Wrong account", busy: "The relay was busy",
        landed_or_used: "Your account was used since you approved", reverted: "Included, then refused",
        wallet_failed: "Your Names Wallet could not finish it", wallet_rejected: "Nothing was sent",
        wallet_no_answer: "Your Names Wallet didn't answer"
    }
    return t[code] || "The claim can't go on"
}

/// What the person gave: a claim file's text, a path, or nothing usable yet.
export function claimInput(text) {
    var t = String(text || "").trim()
    if (t === "") return { kind: "empty", problem: "" }
    if (t.charAt(0) === "{") return t.indexOf("logos-bridge-claim") >= 0 ? { kind: "json", problem: "" } : { kind: "json", problem: "This is not a bridge claim file." }
    if (t.indexOf("file://") === 0 || t.charAt(0) === "/") return { kind: "path", problem: "" }
    return { kind: "other", problem: "Paste the whole claim file (it starts with {), or choose the file." }
}

/// The bridge module's built-in deployments, newest first: v4 (the vault that releases by proof,
/// the bundled bridge's default since 0343cb0), v3, v2.
export const BRIDGE_DEPLOYMENTS = ["bridge-test-network-v4", "bridge-test-network-v3", "bridge-test-network"]
/// The deployment Receive makes new codes on: the bundled bridge's default (v4).
export const RECEIVE_DEPLOYMENT = BRIDGE_DEPLOYMENTS[0]
/// The next built-in deployment to try after `other_vault` (the file is for another vault): the
/// newest one that is neither `name` nor in `tried`; "" when none is left.
export function otherDeployment(name, tried) {
    var done = (Array.isArray(tried) ? tried : []).concat([name])
    return BRIDGE_DEPLOYMENTS.filter(function(d) { return done.indexOf(d) < 0 })[0] || ""
}

/// The v4 bridge program on LEZ 0.4 (rfp-022 deployments.json, bridge-test-network-v4).
export const BRIDGE_V4_PROGRAM = "BKXe3q5JcKeiSrKv9FUBjZw3AowbvYgyHZ3SYPgDxJBf"
/// The bridges the wallet must read for the bundled bridge's claims, from get_config's
/// `bridge_programs`: the list with v4 added when a saved list lacks it (room for 4), else null
/// (nothing to change). Built in here, never taken from the bridge's own answer.
export function bridgesToAdd(programs) {
    if (!Array.isArray(programs) || programs.length === 0 || programs.indexOf(BRIDGE_V4_PROGRAM) >= 0 || programs.length >= 4)
        return null
    return programs.concat([BRIDGE_V4_PROGRAM])
}

/// Why Receive can't make a new code yet with the running bridge, or "". `config`: its config();
/// `programs`: the bridges the wallet reads. An older bridge than the one shipped makes codes for
/// the old vault; a bridge the wallet doesn't read would have its claim refused.
export function receiveHoldLine(config, programs) {
    if (!config || typeof config !== "object")
        return ""
    if (bridgeBehind(config))
        return "New codes come from the new LEZ Bridge. Update it first (above)."
    var program = config.deployment && config.deployment.bridge_program ? String(config.deployment.bridge_program) : ""
    if (program !== "" && Array.isArray(programs) && programs.length > 0 && programs.indexOf(program) < 0)
        return "This wallet doesn't read the LEZ Bridge's network yet. Close the Names tab and open Names again."
    return ""
}

/// Whether a code (a receive_request reply) was made for another vault than Receive's: its
/// `deployment` (a name, or {id, current}) is not RECEIVE_DEPLOYMENT. A fresh code has none: false.
export function codeOlderVault(code) {
    var d = code ? code.deployment : null
    if (d && typeof d === "object")
        return d.current === false || (!!d.id && d.id !== RECEIVE_DEPLOYMENT)
    return typeof d === "string" && d !== "" && d !== RECEIVE_DEPLOYMENT
}

/// The line for a code that can't be shown again (`receive_request` refused): {line, detail}.
/// Never promises the link; the module's own reason goes under Details.
export function receiveCodeRefused(message) {
    var m = String(message || "")
    var older = /older build/i.test(m)
    return { line: older ? "This code can't be shown again: an older LEZ Bridge made it and didn't keep what's needed."
                         : "This code can't be shown again.",
             detail: m }
}

// ── The LEZ Bridge package, installed from inside Names (one click) ──

/// The LEZ Bridge pair this app ships in qml/bundled/, and what Names checks before offering
/// it: each file's SHA-256 (the names module's file_sha256), and each package's manifest root
/// (what package_manager reports for an installed copy, to tell another build apart). Core
/// module first: the app depends on it.
export const BUNDLED_BRIDGE = [
    { name: "lez_bridge", file: "lez_bridge.lgx", label: "LEZ Bridge module",
      sha256: "fd30279b4093acd98a0f141067de9c4aaa24aa9e22cb5aa3ecb0ce48078f99c5",
      root: "307e71a17e7d848329595ea337a479d3ff2ed81acc95b58764e567133664fbde",
      // The commit the shipped bridge says it was built from (its config().build.commit): rfp-022
      // 33fde5c (send-back on the public endpoint: LEZ reads back off on 429; send_back_*, the
      // installed release prover; everything in 0acf483).
      build: "33fde5c" }
]

/// The LEZ 0.4 names tool this app ships in qml/bundled/ (built without in-process proving, so
/// small enough to ship): installed by the names module (`cli_install_bundled`) when LEZ 0.4 has
/// no tool set, after its SHA-256 checks. The build writes the checksum here (build-lgx).
export const BUNDLED_TOOL = { file: "names-lez04", sha256: "@TOOL_SHA256@" }
/// Whether the full LEZ 0.4 tool (with proving, about 100 MB) can still be downloaded from the
/// app: off in builds for people; a build-time switch for testing.
export const FULL_TOOL_DOWNLOAD = false

/// The one line said before a public call when the LEZ 0.4 tool can't try it here first
/// (`cli_status().tool.bare`): `fee`, the most it can be charged (digits), or "".
export function uncheckedLine(tool, kinds) {
    if (!tool || tool.bare !== true)
        return ""
    var caps = tool.unchecked_max_fee || {}
    var total = 0
    for (var i = 0; i < kinds.length; i++)
        total += Number(caps[kinds[i]] || 0)
    return "This computer can't check the call first. If the network refuses it, the fee"
        + (total > 0 ? " (at most " + shortAmount(total) + ")" : "") + " is still paid."
}

/// An amount for one line: 1440624 as "1.44M", 636156 as "636,156".
export function shortAmount(n) {
    var v = Number(n) || 0
    if (v >= 1000000)
        return String(Math.round(v / 10000) / 100) + "M"
    return grouped(String(Math.round(v)))
}

/// Whether the running LEZ Bridge (its config() answer) is another build than the one shipped:
/// its config().build.commit differs, or it has no `build` (every bridge before 00e6f3a).
/// Unknown (no answer yet): false.
export function bridgeBehind(config) {
    if (!config || typeof config !== "object")
        return false
    var build = config.build && typeof config.build === "object" ? String(config.build.commit || "") : ""
    return build === "" || build !== BUNDLED_BRIDGE[0].build
}

/// package_manager.getInstalledPackages' list, out of whatever the host wrapped it in (JSON text,
/// perhaps twice; `{success, value}`). null unless it is surely the list: it must hold this
/// app (names_ui). Nothing is ever offered for install from a list that couldn't be read.
export function installedPackages(reply) {
    var r = reply
    for (var depth = 0; depth < 4; depth++) {
        if (typeof r === "string") {
            try { r = JSON.parse(r) } catch (e) { return null }
        } else if (r && typeof r === "object" && !Array.isArray(r) && "value" in r) {
            r = r.value
        } else
            break
    }
    if (!Array.isArray(r))
        return null
    var named = function(name) { return r.some(function(p) { return p && p.name === name }) }
    return named("names_ui") ? r : null
}

/// The installed entry named `name` in `list`, or null.
export function installedEntry(list, name) {
    var found = (list || []).filter(function(p) { return p && p.name === name })
    return found.length ? found[0] : null
}

/// Where the shipped bridge files are: beside this app's view, `<installDir>/qml/bundled/`.
/// "" when the app's own folder isn't known.
export function bundledPath(list, file) {
    var me = installedEntry(list, "names_ui")
    var dir = me && typeof me.installDir === "string" ? me.installDir.replace(/\/+$/, "") : ""
    return dir && dir.charAt(0) === "/" ? dir + "/qml/bundled/" + file : ""
}

/// What the claim screen does about the LEZ Bridge package. `s`: {list (installedPackages or
/// null), answers (the module answered config: true, false, or null not asked yet), checkWith
/// (it has relayed_claim_check_with), bundle ("ok" | "bad" | "" unchecked)}. One of:
///   "unknown"  the list couldn't be read: the plain manual line, never an install;
///   "absent"   a package is missing: Install (bundle ok) or the manual line;
///   "idle"     installed, the module not running: Start (opens the LEZ Bridge app once);
///   "older"    running, but an older build than the one shipped: the claim works, and a small
///              Update line (bundle ok);
///   "ready"    running.
export function bridgeSetup(s) {
    var st = s || {}
    if (!Array.isArray(st.list))
        return st.answers === true ? "ready" : "unknown"
    // Only the module: Names starts it as its own optional dependency (no bridge app).
    if (!installedEntry(st.list, "lez_bridge"))
        return "absent"
    if (st.answers !== true)
        return st.answers === false ? "idle" : "unknown"
    // An older pair (another build, and one that can't receive: busy-3647c9c, acct-9cbdc72, all
    // "0.1.0"): the claim works, and Update offers the pair shipped inside Names. A pair that
    // says it can receive is never offered a replacement (it may be newer than this one).
    if (bridgeDiffers(st.list) && (Array.isArray(st.features) ? !bridgeCan(st.features, "receives") : st.checkWith === false))
        return "older"
    return "ready"
}

/// Whether an installed bridge package is another build than the shipped one (its manifest root).
export function bridgeDiffers(list) {
    return BUNDLED_BRIDGE.some(function(b) {
        var e = installedEntry(list, b.name)
        var root = e && e.hashes && typeof e.hashes === "object" ? String(e.hashes.root || "") : ""
        return !!e && root !== b.root
    })
}

/// package_manager.installPlugin's reply: `{ok, path, error, timeout}`. A success has `path` and
/// no `error` (what the Package Manager itself counts). No reply is a timeout: the install may
/// still be going, so it is looked at again, never called a failure.
export function installReply(reply) {
    if (reply === undefined || reply === null || String(reply) === "")
        return { ok: false, timeout: true, path: "", error: "" }
    var r = reply
    for (var depth = 0; depth < 3 && typeof r === "string"; depth++) {
        try { r = JSON.parse(r) } catch (e) { return { ok: false, timeout: /timed? ?out/i.test(String(r)), path: "", error: String(r) } }
    }
    if (r && typeof r === "object" && !Array.isArray(r) && "success" in r && "value" in r && typeof r.value === "object")
        r = r.value
    if (!r || typeof r !== "object")
        return { ok: false, timeout: false, path: "", error: "Basecamp's package manager gave an answer that isn't one." }
    var error = r.error ? String(r.error) : ""
    if (/timed? ?out/i.test(error) && !r.path)
        return { ok: false, timeout: true, path: "", error: "" }
    return { ok: !!r.path && error === "", timeout: false, path: r.path ? String(r.path) : "", error: error }
}

/// The answer to an intent (`logos.request`'s `res`: {ok, data, error}): "ok", "cancelled",
/// "not_declared" (Basecamp hasn't read this app's `uses` since it was copied in: it does at its
/// next start or package install), or "other" (refused, unavailable: the plain manual line).
export function intentAnswer(res) {
    if (res && res.ok === true)
        return "ok"
    var error = res && typeof res === "object" ? String(res.error || "") : ""
    return error === "cancelled" || error === "not_declared" ? error : "other"
}

/// The line under "Checking who publishes this app…" on a sheet, counting down from when the
/// sheet first saw the check (`since`, ms) for the host's longest wait (`total`, ms).
export function publisherWaitLine(since, now, total) {
    var left = Math.ceil(((since || now) + (total || 15000) - now) / 1000)
    return left >= 1 ? "Checking who publishes this app… about " + left + " s" : "Checking who publishes this app… a few more seconds"
}

// ── The claim as numbered steps (one action per step; the rest under Details) ──

/// An account ID as its first 6 and last 4 characters ("CCVd76…pKnk"), without its kind.
export function idShort(account) {
    var id = String(account || "").replace(/^(Public|Private)\//, "")
    return id.length > 12 ? id.substr(0, 6) + "…" + id.substr(id.length - 4) : id
}

/// The first-time questions a claim into `account` will ask, from the wallet's app grants
/// (`app_grants`: [{app, accounts}]): none when the bridge already sees the account; "public_account"
/// when it sees others; "connect" then "programs" when it sees none.
export function claimExtras(grants, account, app) {
    var grant = (grants || []).filter(function(g) { return g && g.app === (app || "lez_bridge") })[0]
    if (!grant)
        return ["connect", "programs"]
    var want = String(account || "").replace(/^(Public|Private)\//, "")
    var sees = (grant.accounts || []).some(function(a) {
        var id = typeof a === "string" ? a : a && (a.account || a.id) ? String(a.account || a.id) : ""
        return id.replace(/^(Public|Private)\//, "") === want
    })
    return sees ? [] : ["public_account"]
}

/// A `needs` the module answered, as a step key.
/// A `needs` reply with no wallet request to follow (no request id, so no sheet comes): the
/// module's own words for one line with Try again; "" when there is a request.
export function needsWithoutRequest(value) {
    if (!value || typeof value !== "object" || !value.needs)
        return ""
    var request = value.request && typeof value.request === "object" ? value.request : null
    if (request !== null && String(request.request || "") !== "")
        return ""
    var said = String(value.message || value.error || (request !== null ? request.error || "" : "") || "")
    return said !== "" ? said : "The LEZ Bridge needs an answer from this wallet but didn't ask for one."
}

export function needsStep(needs) {
    var n = String(needs || "")
    return n === "declare" ? "programs" : n
}

/// A step's word in the row of steps at the top of Claim and Receive ("1 Arrived · 2 Checked ·
/// 3 Approve · 4 Claimed"): short, and the past tense once done.
var STEP_SHORT = {
    file: ["File", "File"], arrived: ["Arrived", "Arrived"], check: ["Check", "Checked"], connect: ["Connect", "Connected"],
    programs: ["Programs", "Programs"], public_account: ["Account", "Account"], private_account: ["Account", "Account"],
    secret: ["Keys", "Keys"], approve: ["Approve", "Approved"], done: ["Claim", "Claimed"],
    choose: ["Amount", "Amount"], deposit: ["Deposit", "Deposited"], wait: ["Wait", "Final"], claim: ["Claim", "Claimed"],
    address: ["Address", "Address"], burn: ["Approve", "Approved"], back: ["Ethereum", "Received"]
}
export function stepShort(key, state) {
    var pair = STEP_SHORT[key]
    return pair ? pair[state === "done" ? 1 : 0] : String(key || "")
}

var STEP_TITLES = {
    file: "Choose the claim file", arrived: "Your deposit arrived", check: "Check", connect: "Let the bridge see this account",
    programs: "Allow the bridge's programs", public_account: "Give the bridge this account",
    secret: "Allow the bridge's deposit keys", approve: "Approve in your wallet", done: "Claim"
}
var STEP_DONE = {
    file: "Claim file read", arrived: "", connect: "The bridge sees this account", programs: "The bridge's programs allowed",
    public_account: "Account given to the bridge", secret: "Deposit keys allowed", approve: "Approved"
}

/// The claim's steps: [{key, n, total, title, state ("done" | "now" | "later"), line}]. `s`: {stage
/// (claimStage), extras (claimExtras), extrasDone, needs (the module's needs now), job, summary
/// (the checked deposit in one line)}.
export function claimSteps(s) {
    var st = s || {}
    var extras = (st.extras || []).map(needsStep)
    var needs = needsStep(st.needs)
    if (needs && extras.indexOf(needs) < 0)
        extras = extras.concat([needs])
    var first = st.first === "arrived" ? "arrived" : "file"
    // The deposit keys are asked before a received deposit's check; the rest after the check.
    var before = extras.filter(function(k) { return k === "secret" })
    var after = extras.filter(function(k) { return k !== "secret" })
    var keys = [first].concat(before, ["check"], after, ["approve", "done"])
    var done = (st.extrasDone || []).map(needsStep)
    var job = st.job || null
    var stage = st.stage || "start"
    var now
    if (stage === "start") now = first
    else if (["checking", "refused", "elsewhere", "wrong_bridge", "ready"].indexOf(stage) >= 0) now = "check"
    else if (stage === "needs") now = needs || "approve"
    else if (stage === "asking") now = extras.filter(function(k) { return done.indexOf(k) < 0 })[0] || "approve"
    else if (stage === "uncertain") now = "approve"
    else if (stage === "running" || stage === "failed") now = job && job.phase && job.phase !== "approve" ? "done" : "approve"
    else if (stage === "done") now = "done"
    else now = first
    // A received deposit's own first-time question comes during its check.
    if (first === "arrived" && stage === "needs" && needs) now = needs
    var at = keys.indexOf(now)
    return keys.map(function(k, i) {
        var state = i < at || (stage === "done" && k === "done") ? "done" : i === at ? "now" : "later"
        var title = STEP_TITLES[k]
        if (k === "check" && state === "now")
            title = stage === "checking" ? "Checking…" : stage === "ready" ? "Checked ✓" : "Check"
        if (k === "done")
            title = stage === "done" ? "Done: claimed in block " + (job && job.block !== undefined ? job.block : "?") : state === "now" ? (stage === "failed" ? "Claim stopped" : "Claiming…") : "Claim"
        if (k === "approve" && state === "now" && stage === "failed")
            title = "Not approved"
        var line = state !== "done" ? "" : k === "check" ? String(st.summary || "Checked") : k === "done" ? "" : STEP_DONE[k] || ""
        return { key: k, n: i + 1, total: keys.length, title: title, short: stepShort(k, state), state: state, line: line }
    })
}

/// The checked deposit in one line: "0.1 ETH → your account CCVd76…pKnk (bridge v3)".
export function claimSummary(check, bridgeLabel) {
    if (!check || !check.ok || !check.deposit)
        return ""
    return String(check.deposit.amount_text || "") + " → your account " + idShort(check.deposit.account) + (bridgeLabel ? " (" + bridgeLabel + ")" : "")
}

/// A refusal or failure as ONE line that says what to do (the full message goes under Details).
export function claimErrorLine(code, error) {
    var e = error || {}
    var min = e.eta_s ? Math.max(1, Math.ceil(Number(e.eta_s) / 60)) : e.final_in_min ? Number(e.final_in_min) : 0
    var lines = {
        relay_cap: "The bridge's relay has reached today's limit. Nothing was sent. Try again later.",
        rate_limited: "Too many claims from here today. Nothing was sent. Try again later.",
        relay_busy: "The relay is busy. Try again.", busy: "The relay is busy. Try again.",
        relay_unavailable: "The relay isn't answering. Nothing was sent. Try again later.",
        relay_funds: "The relay is out of funds. Nothing was sent. Try again later.",
        relay_once: "The relay already sent this claim.",
        relay_refused: "The relay refused the claim. Nothing was sent. Try again.",
        reverted: "The claim was refused on the bridge test network. Try again.",
        not_final: min ? "Not final on Ethereum yet: about " + min + " min." : "Not final on Ethereum yet. Try again in a few minutes.",
        root_not_attested: "Not recorded on the bridge yet. Try again in about a minute.",
        already_claimed: "This deposit was already claimed.",
        private_account: "This deposit is for a private account. Private claims aren't in this wallet yet.",
        bad_file: "That isn't a claim file. Choose the file the deposit page gave you.",
        not_in_vault: "No such deposit in the vault yet. Check the file, or try again later.",
        unreachable: "The bridge test network can't be reached. Try again.",
        eth_unreachable: "Sepolia can't be read just now. Try again.",
        // The bridge's own words say which: no, nobody answered within 10 minutes, or cancelled.
        wallet_rejected: /didn't answer|Nobody answered/i.test(String(e.message || "")) ? "The approval wasn't answered within 10 minutes. Nothing was sent. Try again."
            : /^Cancelled/i.test(String(e.message || "")) ? "The approval was cancelled. Nothing was sent. Try again."
            : /rejected/i.test(String(e.message || "")) ? "You said no in your wallet. Nothing was sent. Try again."
            : "The approval ended without a yes. Nothing was sent. Try again.",
        wallet_no_answer: "Your wallet didn't answer. Look for its sheet, then try again.",
        landed_or_used: "Your account changed since you approved. Try again.",
        minting_frozen: "Claims are paused on the bridge. Try again later.",
        minting_frozen_token: "Claims of this token are paused. Try again later."
    }
    return lines[code] || (claimErrorTitle(code) + ".")
}

/// Seconds until a not-ready claim is checked again by itself: the time the refusal gives for
/// finality, about a minute for a root not recorded yet (the bridge records one about every
/// minute); 0 (no automatic check, the person presses Check again) for anything else.
export function claimRetryAfter(code, error) {
    var e = error || {}
    if (code === "not_final")
        return e.eta_s ? Math.max(15, Number(e.eta_s)) : e.final_in_min ? Number(e.final_in_min) * 60 : 0
    if (code === "root_not_attested")
        return 150
    return 0
}

/// Whether a claim refusal is a wait (shown plainly, not as an error): not final or not recorded
/// yet, the relay busy or at its limit, a network not answering just now.
export function claimIsWait(code) {
    return ["not_final", "root_not_attested", "relay_busy", "busy", "relay_cap", "rate_limited", "relay_unavailable", "unreachable", "eth_unreachable"].indexOf(String(code || "")) >= 0
}

// ── Receive from Ethereum (rfp-022 modules/NAMES-RECEIVE.md, "Built on receive-flow") ──

/// The bridge test network's denominations when the bridge's registry can't be read (v3).
export const RECEIVE_FALLBACK = [
    { symbol: "ETH", denoms: [{ index: 0, text: "0.01" }, { index: 1, text: "0.1" }, { index: 2, text: "1" }] },
    { symbol: "dUSDC", denoms: [{ index: 0, text: "100" }, { index: 1, text: "1,000" }, { index: 2, text: "10,000" }] }
]

/// The tokens and denominations to offer, from the bridge's `registry()` value (`tokens[].denoms[]`
/// with `index` and `text`), else RECEIVE_FALLBACK.
export function receiveTokens(registry) {
    var list = registry && Array.isArray(registry.tokens) ? registry.tokens : []
    var out = list.filter(function(t) { return t && t.symbol && Array.isArray(t.denoms) && t.denoms.length > 0 }).map(function(t) {
        return { symbol: String(t.symbol), frozen: !!(t.freeze && t.freeze.eth_deposits === true),
                 denoms: t.denoms.map(function(d, i) { return { index: d.index !== undefined ? Number(d.index) : i, text: String(d.text || d.value || "") } }) }
    })
    return out.length > 0 ? out : RECEIVE_FALLBACK
}

/// A deposit request's reply: {kind: "needs" | "code" | "old", value}. "old": a LEZ Bridge that
/// makes no deposit code yet (no `request`).
export function receiveReply(value) {
    if (value && typeof value === "object" && value.needs)
        return { kind: "needs", value: value }
    if (value && typeof value === "object" && typeof value.request === "string" && value.request.indexOf("logosdep1") === 0)
        return { kind: "code", value: value }
    return { kind: "old", value: value }
}

/// This wallet's receives, from the bridge's `receives()` (no chain read; newest first), not
/// claimed yet: [{id, symbol, amount, account, account_kind, stage, text}]. `stage` is what
/// receive_status last saw ("unknown" before its first call).
export function receivesFrom(value) {
    var rows = value && Array.isArray(value.receives) ? value.receives : Array.isArray(value) ? value : []
    return rows.filter(function(d) { return d && d.id && d.claimed !== true && d.stage !== "claimed" }).map(function(d) {
        var amount = d.denom_text || d.amount_text || (d.denom !== undefined && d.decimals !== undefined ? withDecimals(d.denom, d.decimals) : "")
        return { id: String(d.id), symbol: String(d.symbol || ""), amount: String(amount), account: String(d.account || ""), account_kind: String(d.account_kind || ""),
                 stage: String(d.stage || "unknown"), compare: d.compare && typeof d.compare === "object" ? d.compare : null,
                 deployment: d.deployment && typeof d.deployment === "object" ? d.deployment : null }
    })
}

/// What the installed LEZ Bridge can do, from `config().features` (none: an older pair).
export function bridgeCan(features, what) {
    return Array.isArray(features) && features.indexOf(what) >= 0
}

/// Where one receive stands, from its `receive_status(id)`, in the words the Receive screen, the
/// card's "Arriving from Ethereum" panel and the Receive entry all use: {stage, card (the card
/// row's state), headline (the waiting step's big line), detail (one small line or ""), tone
/// ("wait" | "ready" | "done" | "plain"), claim}. `stage`: "checking" (no answer yet), "waiting"
/// (the code is made, nothing on Ethereum yet), "deposited", "final", "claimable", "claimed".
/// Nothing is promised past what the bridge said: the minutes are its upper bound (whole Sepolia
/// epochs), and once final, "any minute now" (it records a final root about once a minute).
export function receiveProgress(status) {
    var s = status || {}
    var r = function(stage, card, headline, detail, tone, claim) { return { stage: stage, card: card, headline: headline, detail: detail, tone: tone, claim: !!claim } }
    switch (s.stage) {
    case "requested": return r("waiting", "Waiting for your deposit", "Waiting for your deposit…", "", "wait")
    case "deposited": {
        var m = s.final_in_min !== undefined && s.final_in_min !== null && !isNaN(Number(s.final_in_min)) ? Math.ceil(Number(s.final_in_min)) : 20
        var conf = Number(s.confirmations) || 0
        var seen = "Deposit seen on Sepolia" + (conf > 0 ? " · " + conf + (conf === 1 ? " confirmation." : " confirmations.") : ".")
        if (m <= 1)
            return r("deposited", "Deposited · final on Sepolia any minute now", "Any minute now.", seen, "wait")
        return r("deposited", "Deposited · final on Sepolia in up to about " + m + " min", "Final on Sepolia in up to about " + m + " min.", seen, "wait")
    }
    case "final": return r("final", "Final on Sepolia · arriving any minute now", "Any minute now.", "Final on Sepolia; the bridge records it next (about a minute).", "wait")
    case "claimable":
        // Arrived on an older vault whose relay has moved on (`relay: false`): the bridge's words,
        // no Claim (it would be refused).
        if (s.relay === false)
            return r("noclaim", "Arrived · can't be claimed here yet", "Arrived, but it can't be claimed here yet.", String(s.text || ""), "plain")
        return r("claimable", "Arrived · ready to claim", "Arrived: ready to claim.", "", "ready", true)
    case "claimed": {
        var line = "Claimed" + (s.claimed_block ? " in block " + s.claimed_block : "")
        return r("claimed", line, line + ".", "", "done")
    }
    case "unknown":
        // A receive the bridge can't follow (an older build kept no deployment with it): its reason.
        if (s.action === "recover")
            return r("lost", "Can't be followed here", "This receive can't be followed here.", String(s.text || ""), "plain")
    }
    return r("checking", "Checking…", "Checking…", "", "plain")
}

/// Where a receive was made, when that is not the bridge's current vault: "bridge v3", else "".
/// From its receive_status or its receives() row (`deployment: {id, label, current, network}`).
export function receiveWhere(status, row) {
    var d = status && status.deployment && typeof status.deployment === "object" ? status.deployment
        : row && row.deployment && typeof row.deployment === "object" ? row.deployment : null
    return d && d.current === false && d.label ? String(d.label) : ""
}

/// How long a receive claimed while Names was open stays on the card ("Claimed in block N").
export const RECEIVE_CLAIMED_SHOWN_MS = 30 * 60 * 1000

/// The card's "Arriving from Ethereum" rows: {rows: [{id, symbol, amount, account, progress}],
/// hidden}. Arrived first, then those on their way, then the ones claimed in the last 30 min
/// (`claimedAt`: id → ms), then codes nothing was deposited for yet: only the newest two of
/// those unless `showAll` (an unused code stays "requested" for good), `hidden` how many more.
export function receiveRowsShown(list, statuses, claimedAt, now, showAll) {
    var st = statuses || {}, at = claimedAt || {}
    var rank = { claimable: 0, noclaim: 1, final: 1, deposited: 1, claimed: 2, checking: 3, waiting: 3, lost: 3 }
    var rows = (Array.isArray(list) ? list : []).map(function(x, i) { return { row: x, i: i, progress: receiveProgress(st[x.id]) } })
        .filter(function(e) { return e.progress.stage !== "claimed" || (at[e.row.id] && Number(now) - at[e.row.id] < RECEIVE_CLAIMED_SHOWN_MS) })
        .sort(function(a, b) { return rank[a.progress.stage] - rank[b.progress.stage] || a.i - b.i })
    var waiting = 0, hidden = 0
    rows = rows.filter(function(e) {
        if (rank[e.progress.stage] < 3 || showAll) return true
        waiting++
        if (waiting <= 2) return true
        hidden++
        return false
    })
    return { rows: rows.map(function(e) { return { id: e.row.id, symbol: e.row.symbol, amount: e.row.amount, account: e.row.account, progress: e.progress, where: receiveWhere(st[e.row.id], e.row) } }), hidden: hidden }
}

/// The receive that opening "Receive from Ethereum" shows first, or "": the newest one on its way
/// or arrived (deposited, final, claimable), else one whose code was made in this window
/// (`codes`: id → the code) and not deposited yet. Old unused codes never take the screen.
export function receiveInProgress(list, statuses, codes) {
    var rows = Array.isArray(list) ? list : [], st = statuses || {}, made = codes || {}
    var going = rows.filter(function(x) { return ["deposited", "final", "claimable", "noclaim"].indexOf(receiveProgress(st[x.id]).stage) >= 0 })
    if (going.length > 0) return String(going[0].id)
    // The newest code made here (`made_at` kept with it).
    var mine = rows.filter(function(x) { return made[x.id] && ["waiting", "checking"].indexOf(receiveProgress(st[x.id]).stage) >= 0 })
        .sort(function(a, b) { return (Number(made[b.id].made_at) || 0) - (Number(made[a.id].made_at) || 0) })
    return mine.length > 0 ? String(mine[0].id) : ""
}

/// The Receive screen's first line for a receive opened again (its code isn't on the screen):
/// "You have a deposit on its way: 0.1 ETH · final in up to about 14 min".
export function receiveOnItsWay(row, status) {
    if (!row) return ""
    var what = String(row.amount || "") + (row.symbol ? " " + row.symbol : "")
    var p = receiveProgress(status)
    switch (p.stage) {
    case "deposited": return "You have a deposit on its way: " + what + " · " + p.card.replace(/^Deposited · /, "")
    case "final": return "You have a deposit on its way: " + what + " · arriving any minute now"
    case "claimable": case "noclaim": return "Your " + what + " deposit has arrived."
    case "lost": return "Your " + what + " receive can't be followed here."
    case "claimed": return "Your " + what + " deposit is claimed."
    case "waiting": return "Your code for " + what + " is waiting for a deposit."
    }
    return "Your " + what + " deposit: checking…"
}

/// The small line under a receive's state: "Checked just now · next check in 42 s". `polling`: a
/// check is under way; `checkedAt`/`nextAt`: ms (0: never); `failed`: the last check got no answer.
export function receiveCheckLine(polling, checkedAt, nextAt, now, failed) {
    if (polling) return "Checking…"
    // Only a gentle check (minutes away) is announced; a job's quick checks and none at all say nothing.
    var left = Number(nextAt) - Number(now)
    var after = nextAt > 0 && left >= 60000 ? "next check in " + Math.ceil(left / 60000) + " min" : ""
    if (failed) return "Couldn't check just now" + (after ? " · " + after.replace("next check", "next try") : "")
    if (!checkedAt) return after ? after.charAt(0).toUpperCase() + after.slice(1) : ""
    var ago = Math.max(0, Number(now) - Number(checkedAt))
    var when = ago < 15000 ? "just now" : ago < 90000 ? Math.round(ago / 1000) + " s ago" : Math.round(ago / 60000) + " min ago"
    return "Checked " + when + (after ? " · " + after : "")
}

// ── When the bridge things are checked (the owner, 15 Nov: "put a check now button so if they
// ignore it, we ignore it"): when the person asks (Check now), once when a panel or screen comes
// into view, and by itself only in these cases. ──

/// A job that moves only when its status is asked (a burn being approved, sent, a proof being
/// made): every 5 s while it is shown, until it ends.
export const CHECK_JOB_MS = 5000
/// A wait whose answer changes by itself soon (a deposit becoming final): every 2.5 min, only on
/// its own screen, open and in front.
export const CHECK_GENTLE_MS = 150000

/// How a receive is checked by itself, from its receive_status: "gentle" while it becomes final on
/// Sepolia (deposited, final), else "none" (Check now, and once on view).
export function receiveAutoMode(status) {
    var stage = status && status.stage
    return stage === "deposited" || stage === "final" ? "gentle" : "none"
}

/// How a send-back is checked by itself, from its send_back_status: "job" while the burn is being
/// approved or sent, or the proof made (each check moves it: the bridge hands an approved burn to
/// the relay only when asked); "at" with `at` (ms) when its releasable time is known; "gentle"
/// while the endpoint is busy; "none" while it waits on the operator's update, the seal, or the
/// person (Check now).
export function sendBackAutoMode(status) {
    var s = status || {}
    if (s.busy === true) return { mode: "gentle" }
    switch (s.stage) {
    case "approve": return { mode: s.note ? "none" : "job" }
    case "burning": case "proving": return { mode: "job" }
    case "wrapped": return s.releasable_at ? { mode: "at", at: Number(s.releasable_at) * 1000 + 5000 } : { mode: "none" }
    }
    return { mode: "none" }
}

/// When the next check by itself is due (ms), or 0 for none. `s`: {now, visible (a bridge panel or
/// screen shows), frontmost (Basecamp is in front), screen ("card" | "receive" | "sendback"),
/// items: [{mode, at, focused (the one its screen shows)}]}. Jobs go on while shown, in front or
/// not; everything else only in front, and only on the item's own screen.
export function bridgeNextCheck(s) {
    var st = s || {}
    if (!st.visible) return 0
    var now = Number(st.now) || 0
    var due = 0
    var take = function(t) { if (t > 0 && (due === 0 || t < due)) due = t }
    ;(st.items || []).forEach(function(it) {
        var m = it && it.mode
        if (m === "job") take(now + CHECK_JOB_MS)
        if (!st.frontmost || st.screen === "card" || !it.focused) return
        if (m === "gentle") take(now + CHECK_GENTLE_MS)
        if (m === "at" && Number(it.at) > now) take(Number(it.at))
    })
    return due
}

/// Where a claimed receive's tokens are now, from the card's account blocks (bridgeNetRows):
/// "CCVd76…pKnk now holds 0.1 ETH." or, before the network is read again, where to look.
export function receiveBalanceLine(blocks, account, symbol) {
    var id = String(account || "").replace(/^Public\//, "")
    var block = (Array.isArray(blocks) ? blocks : []).filter(function(b) { return String(b.account || "").replace(/^Public\//, "") === id })[0]
    // The wrapped token keeps the symbol or puts a "w" before it (ETH arrives as wETH).
    // With several bridges, one row per bridge: the one this receive's bridge listed (`bridge`,
    // its label such as "bridge v4"), else one that holds something (the first was v2's 0 wETH
    // after a v4 claim on the M2, 15 Nov), else the first.
    var all = block && Array.isArray(block.assets) ? block.assets.filter(function(a) { return a.symbol === symbol || a.symbol === "w" + symbol }) : []
    var bridge = arguments.length > 3 ? String(arguments[3] || "") : ""
    var ofBridge = bridge ? all.filter(function(a) { return String(a.name || "").indexOf("· " + bridge) >= 0 }) : []
    var held = all.filter(function(a) { return !/^0(\.0*)?$/.test(String(a.amount || "0")) })
    var asset = ofBridge[0] || held[0] || all[0] || null
    if (asset) return idShort(id) + " now holds " + asset.amount + " " + asset.symbol + "."
    return "It shows under " + idShort(id) + " below once the network is read again."
}

/// The Receive screen's steps: [{key, n, total, title, short, state, line}]. `s`: {stage ("choose" |
/// "asking" | "needs" | "code", or a receive's progress stage: "waiting" | "checking" |
/// "deposited" | "final" | "claimable" | "claimed"), needs, extras (first-time questions seen),
/// claimedBlock}. Amount, the first-time questions, Deposit, Wait, Claim: once the deposit is seen
/// the deposit step is done by itself ("Deposited") and the wait is the step ("Waiting for
/// Ethereum"); then Claim ("Arrived"); then all done ("Claimed in block N").
export function receiveSteps(s) {
    var st = s || {}
    var extras = (st.extras || []).map(needsStep)
    var needs = needsStep(st.needs)
    if (needs && extras.indexOf(needs) < 0)
        extras = extras.concat([needs])
    var keys = ["choose"].concat(extras, ["deposit", "wait", "claim"])
    var titles = { choose: "Choose the amount and account", connect: "Let the bridge see this account", public_account: "Give the bridge this account",
                   private_account: "Give the bridge this account", secret: "Allow the bridge's deposit keys",
                   deposit: "Deposit on the page", wait: "Waiting for Ethereum",
                   claim: st.stage === "claimable" ? "Arrived: claim it" : st.stage === "noclaim" ? "Arrived" : st.stage === "claimed" ? "Claimed" + (st.claimedBlock ? " in block " + st.claimedBlock : "") : "Claim" }
    var now = st.stage === "code" || st.stage === "waiting" || st.stage === "checking" || st.stage === "lost" ? "deposit" : st.stage === "deposited" || st.stage === "final" ? "wait"
        : st.stage === "claimable" || st.stage === "noclaim" ? "claim" : st.stage === "needs" ? (needs || "choose") : "choose"
    var at = st.stage === "claimed" ? keys.length : keys.indexOf(now)
    return keys.map(function(k, i) {
        var state = i < at ? "done" : i === at ? "now" : "later"
        var title = k === "deposit" && state === "done" ? "Deposited" : titles[k] || k
        return { key: k, n: i + 1, total: keys.length, title: title, short: stepShort(k, state), state: state, line: "" }
    })
}

// ── The LEZ Bridge setup, one state machine for the claim screen, Receive and the LEZ 0.4 card ──

/// What the bridge needs before Claim or Receive can go on, the same on every entry point:
/// {state, line, button ("Install" | "Update" | "Open Settings" | ""), action}. `s`: {list
/// (installedPackages, null unread), bundle ("ok"/"bad"/""), answers (the module answered config:
/// true/false/null), notLoaded (its last config call found it not loaded), config (its config() answer: older than the shipped one by bridgeBehind; without
/// it, `features` lacking "receives"), stuck (it is installed and timed out twice), stale (its last reply was to an
/// old token), features, install (the install/update under way: {phase, line} or null), done (the
/// line shown once after a setup step finished, or "")}.
export function bridgeSetupState(s) {
    var st = s || {}
    var t = function(state, line, button, action) { return { state: state, line: line, button: button || "", action: action || "" } }
    var unload = "Settings → Module Inspector → LEZ Bridge → Unload All. Then close the Names tab and open Names again."
    if (st.install) {
        var p = st.install.phase
        if (p === "asking" || p === "installing" || p === "slow") return t("busy", st.install.line)
        if (p === "reopen") return t("reopen", "Close the Names tab and open Names again to finish.")
        if (p === "unload") return t("unload", "Last step: " + unload, "Open Settings", "settings")
        if (p === "failed") return t("failed", st.install.line, st.bundle === "ok" ? "Install" : "", st.bundle === "ok" ? "install" : "")
    }
    if (!Array.isArray(st.list)) return t("", "")
    var mod = installedEntry(st.list, "lez_bridge")
    if (!mod)
        return st.bundle === "ok" ? t("install", "The LEZ Bridge isn't installed. It comes with Names.", "Install", "install")
            : st.bundle === "bad" ? t("manual", "Install the LEZ Bridge package: Package Manager (the cube icon) → Install Local Package") : t("", "")
    // Installed but not loaded while this Names tab is open (after Unload All): a new tab starts it.
    if (st.answers === false && st.notLoaded)
        return t("reopen", "Close the Names tab and open Names again.")
    // A reply to an old token (the bridge was reloaded under Names): only a reopen reconnects.
    if (st.answers === false && st.stale)
        return t("stale", "Close the Names tab and open Names again to reconnect to the bridge.")
    // A bridge that doesn't answer and is an older build than the one shipped (tonight's frozen
    // one): the new files first, so the one Unload All that follows starts the new bridge.
    if (st.answers === false && st.stuck && st.bundle === "ok" && bridgeDiffers(st.list))
        return t("update", "The LEZ Bridge doesn't answer, and an update is ready.", "Update", "update")
    if (st.answers === false)
        return st.stuck ? t("stuck", "The LEZ Bridge doesn't answer. " + unload, "Open Settings", "settings")
            : t("reopen", "The LEZ Bridge isn't running. Close the Names tab and open Names again to start it.")
    if (st.answers !== true) return t("", "")
    var behind = st.config ? bridgeBehind(st.config) : !bridgeCan(st.features, "receives")
    if (behind && st.bundle === "ok" && bridgeDiffers(st.list)) return t("update", "LEZ Bridge update ready.", "Update", "update")
    if (behind && st.bundle === "ok" && !bridgeDiffers(st.list)) return t("unload", "The new LEZ Bridge is installed; Basecamp still runs the old one. " + unload, "Open Settings", "settings")
    // Once, where the person lands after a setup step finished: "LEZ Bridge updated." (`done`).
    return st.done ? t("done", String(st.done)) : t("", "")
}

// ── What's new: three lines at the top of Settings after an update (change them with each build) ──
export const WHATS_NEW = [
    "Receive from Ethereum uses the new vault (v4); a claim that stopped can be tried again.",
    "Deposits from Ethereum have their own panel on the Wallet card: the minutes left, then Claim.",
    "Settings → Backup: one encrypted file of your wallets, checked after it's made."
]

// ── Settings → Backup ──

/// The one line of backup state: "Last backup: never" / "Last backup: 14 Nov, 19:50 · both wallets".
export function backupLastLine(last, clock) {
    if (!last || !last.at)
        return "Last backup: never"
    var nets = Array.isArray(last.networks) ? last.networks : []
    var what = nets.length >= 2 ? "both wallets" : nets[0] === "lez-0.4" ? "the LEZ 0.4 wallet" : "the LEZ 0.3 wallet"
    return "Last backup: " + clock(Number(last.at)) + " · " + what
}

/// The compare step on Receive. With the bridge's four words (`compare.words`): check them on
/// the page; with an older bridge: the commitment's first and last digits, as before.
export function compareLine(compare) {
    if (!compare)
        return ""
    if (Array.isArray(compare.words) && compare.words.length > 0)
        return "Check these four words match the deposit page. If they differ, stop."
    return "The page must show a commitment starting " + compare.first + " and ending " + compare.last + "."
}
/// The four words, large: "maple · tiger · seven · lamp" (as the module returns them, never made here).
export function compareWords(compare) {
    if (!compare || !Array.isArray(compare.words) || compare.words.length === 0)
        return ""
    return compare.words_text ? String(compare.words_text) : compare.words.map(String).join(" · ")
}
/// Under Details when the words are shown: the commitment's digits too.
export function compareDigitsDetail(compare) {
    return compare && Array.isArray(compare.words) && compare.words.length > 0 && compare.first ? "Commitment starts " + compare.first + " and ends " + compare.last + "." : ""
}

/// Which wallet folders went into a backup, in one line: "From /Users/you/.lee/wallet (LEZ 0.3)".
export function backupFoldersLine(folders) {
    var list = Array.isArray(folders) ? folders : []
    if (list.length === 0)
        return ""
    return "From " + list.map(function(f) { return f.folder + " (" + (f.id === "lez-0.4" ? "LEZ 0.4" : "LEZ 0.3") + ")" }).join(" and ") + "."
}

/// A path from a file dialog (`file:///Users/you/Names%20Wallet%20Backups/x`) as a plain path.
export function dialogPath(url) {
    var text = String(url || "").replace(/^file:\/\//, "")
    try { return decodeURIComponent(text) } catch (e) { return text }
}

/// The line that shows only when true: accounts no recovery phrase brings back.
export function backupUncoveredLine(status) {
    var nets = status && Array.isArray(status.networks) ? status.networks : []
    var n = nets.reduce(function(sum, x) { return sum + (Number(x.uncovered) || 0) }, 0)
    if (n === 0)
        return ""
    return "This wallet has " + n + (n === 1 ? " account" : " accounts") + " your recovery phrase doesn't cover. A backup file is the only way to recover " + (n === 1 ? "it." : "them.")
}

/// The password's hint line: {text, ok}. At least `min` characters; the two must match.
export function backupPasswordHint(first, second, min) {
    var a = String(first || ""), b = String(second || ""), m = Number(min) || 10
    if (a.length === 0) return { text: "At least " + m + " characters. If you forget it, the backup can't be opened.", ok: false }
    if (a.length < m) return { text: "Too short: at least " + m + " characters.", ok: false }
    if (b.length > 0 && a !== b) return { text: "The two don't match.", ok: false }
    if (b.length === 0) return { text: "Type it again below.", ok: false }
    return { text: a.length >= 16 ? "Strong. If you forget it, the backup can't be opened." : "OK; longer is stronger. If you forget it, the backup can't be opened.", ok: true }
}

/// The networks to back up: "this" (the one in use) or "both", as the module takes them.
export function backupNetworks(which, inUse, status) {
    var have = (status && Array.isArray(status.networks) ? status.networks : []).filter(function(n) { return n.has_wallet }).map(function(n) { return n.id })
    var want = which === "both" ? ["lez-0.3", "lez-0.4"] : [inUse]
    return want.filter(function(id) { return have.indexOf(id) >= 0 }).join(",")
}

/// After a backup: "Backup checked: it opens with your password and holds these 5 accounts."
export function backupCheckedLine(check) {
    var nets = check && Array.isArray(check.networks) ? check.networks : []
    var n = nets.reduce(function(sum, x) { return sum + (Array.isArray(x.accounts) ? x.accounts.length : 0) }, 0)
    var same = nets.length > 0 && nets.every(function(x) { return x.matches === true })
    if (!check || check.opens !== true)
        return { text: "The backup didn't open again. Make another one.", ok: false }
    if (nets.length === 0 || nets.some(function(x) { return x.checked === false }))
        return { text: "The backup opens, but it couldn't be compared with the wallet here. Make another one to be sure.", ok: false }
    if (!same)
        return { text: "The backup opens, but its files differ from the wallet now (it keeps syncing). Make another one to be sure.", ok: false }
    return { text: "Backup checked: it opens with your password and holds " + (n === 1 ? "this account." : "these " + n + " accounts."), ok: true }
}

// ── Send back to Ethereum (rfp-022 modules/NAMES-SEND-BACK.md): wrapped tokens in a public
// account on bridge v4 burned, then released on Ethereum to an address the person connects on
// the bridge page. The bridge module keeps everything; these only say it in words. ──

/// The denominations of the bridged tokens on v4, base units, by the symbol the card shows, for
/// when the bridge's registry can't be read.
export const SEND_BACK_FALLBACK = {
    wETH: { symbol: "ETH", decimals: 18, denoms: ["10000000000000000", "100000000000000000", "1000000000000000000"] },
    dUSDC: { symbol: "dUSDC", decimals: 6, denoms: ["100000000", "1000000000", "10000000000"] }
}

/// Whether a card asset row (bridgeNetRows) can be sent back: wrapped on bridge v4 (the vault
/// that releases by proof), at least its smallest denomination held. Never offered otherwise, so
/// no button is dead.
export function sendBackOffer(asset) {
    if (!asset || asset.bridge !== BRIDGE_V4_PROGRAM) return false
    var known = SEND_BACK_FALLBACK[asset.symbol]
    return !!known && atLeast(String(asset.base || "0"), known.denoms[0])
}

/// The amounts to offer for `asset` (a card asset row): one button per denomination the account
/// holds at least once, from the bridge's `registry()` when it lists the token (`value` in base
/// units), else SEND_BACK_FALLBACK. [{index, text, value}] in the registry's order.
export function sendBackAmounts(registry, asset) {
    if (!asset) return []
    var known = SEND_BACK_FALLBACK[asset.symbol]
    if (!known) return []
    var listed = registry && Array.isArray(registry.tokens) ? registry.tokens.filter(function(t) { return t && t.symbol === known.symbol && Array.isArray(t.denoms) })[0] : null
    var denoms = listed ? listed.denoms.map(function(d, i) { return { index: d.index !== undefined ? Number(d.index) : i, value: String(d.value || "") } })
        .filter(function(d) { return /^[0-9]+$/.test(d.value) }) : []
    if (denoms.length === 0)
        denoms = known.denoms.map(function(v, i) { return { index: i, value: v } })
    return denoms.filter(function(d) { return /^[0-9]+$/.test(d.value) && atLeast(String(asset.base || "0"), d.value) })
        .map(function(d) { return { index: d.index, value: d.value, text: withDecimals(d.value, known.decimals) } })
}

/// The bridge module's symbol for a card symbol: wETH → ETH (what `send_back_plan` takes).
export function sendBackSymbol(symbol) {
    var known = SEND_BACK_FALLBACK[String(symbol || "")]
    return known ? known.symbol : String(symbol || "")
}

/// A pasted withdraw-to code, cleaned: the `logoswd1…` part of a bare code, a link or text around
/// it (chat apps add spaces and line breaks), lowercase. "" when there is none.
export function sendBackCode(text) {
    var t = String(text || "").replace(/\s+/g, "")
    var m = /(logoswd1[02-9ac-hj-np-z]+)/i.exec(t)
    return m ? m[1].toLowerCase() : ""
}

/// A refused withdraw-to code, as a title: the bridge's `error.code` (`bad_code:<why>`,
/// `other_chain`, `other_vault`).
export function sendBackCodeTitle(code) {
    var c = String(code || "")
    if (c === "other_chain") return "This code is for another chain"
    if (c === "other_vault") return "This code is for another bridge vault"
    if (c.indexOf("bad_code") === 0) return "That is not a usable code"
    return "The code was refused"
}

/// `HH:MM` of a time in seconds, on this computer's clock.
export function clockShown(seconds) {
    var d = new Date(Number(seconds) * 1000)
    var two = function(n) { return (n < 10 ? "0" : "") + n }
    return two(d.getHours()) + ":" + two(d.getMinutes())
}

/// Where one send-back stands, from its `send_back_status(id)` (or a `send_backs()` row): {stage,
/// headline (the big line; the wait itself while waiting), card (the card row's state), detail,
/// tone ("wait" | "ready" | "done" | "plain"), action ("" | "burn" | "prove" | "seal" |
/// "release")}. `clock`: seconds → "14:32" (clockShown). Times are only the bridge's: the update on
/// Ethereum has none (the operator posts it by hand today), the proof about 20 s, the wrapper about
/// 3 min, the release at the vault's own `releasable_at`.
export function sendBackProgress(status, nowSeconds, clock) {
    var s = status || {}
    var at = clock || clockShown
    var r = function(stage, card, headline, detail, tone, action) { return { stage: stage, card: card, headline: headline, detail: detail || "", tone: tone, action: action || "" } }
    var when = s.releasable_at ? Number(s.releasable_at) : 0
    var later = when > 0 && Number(nowSeconds) < when
    var until = later ? "Releasable at " + at(when) + " at the earliest (the vault's one-hour delay)." : ""
    switch (s.stage) {
    case "planned": return r("planned", "Ready to send back · approve the burn", "Ready: approve the burn.", "", "ready", "burn")
    case "approve":
        // Approved and signed, but the bridge's relay refused or didn't answer (`note`): its words,
        // and the bridge tries again at each check.
        if (s.note && s.busy !== true) return r("approve", "Signed · the bridge's relay hasn't sent it", "Signed, but the bridge's relay hasn't sent it yet.", String(s.note), "plain")
        return r("approve", "Waiting for your approval", "Waiting for your approval.", "", "ready")
    case "burning": return r("burning", "Sending the burn · about 15 s", "Sending the burn: about 15 s.", "", "wait")
    case "burned": return r("burned", "Burned · waiting for Ethereum to record it", "Waiting for Ethereum to record it.",
        "The bridge team posts these updates to Ethereum by hand today, so this can take from minutes to hours.", "wait")
    case "burn_unclear": return r("burn_unclear", "The burn may have been refused", "The burn may have been refused.", String(s.text || ""), "plain")
    case "recorded": return r("recorded", "Recorded on Ethereum · make the proof", "Recorded on Ethereum: make the release proof.", until, "ready", "prove")
    case "proving": return r("proving", "Making the release proof · about 20 s", "Making the release proof on this computer: about 20 s.", until, "wait")
    case "proved": return r("proved", "Waiting for the proof's Ethereum wrapper", "Waiting for the proof's Ethereum wrapper: about 3 min.",
        "The bridge team makes it by hand today and sends you the seal to paste here.", "wait", "seal")
    case "wrapped":
        return r("wrapped", later ? "Releasable at " + at(when) : "Releasable any minute now",
            later ? "Releasable at " + at(when) + "." : "Releasable any minute now.", "The vault waits one hour after Ethereum records it.", "wait")
    case "releasable": return r("releasable", "Ready to collect on Ethereum", "Ready: collect it on the bridge page.", "", "ready", "release")
    case "released": return r("released", "Received on Ethereum", "Received on Ethereum.", "", "done")
    }
    return r("checking", "Checking…", "Checking…", "", "plain")
}

/// This wallet's send-backs, from the bridge's `send_backs()` (no chain read; newest first):
/// [{id, symbol, amount, account, recipient, words, stage}].
export function sendBacksFrom(value) {
    var rows = value && Array.isArray(value.send_backs) ? value.send_backs : Array.isArray(value) ? value : []
    return rows.filter(function(x) { return x && x.id }).map(function(x) {
        return { id: String(x.id), amount: String(x.amount_text || ""), account: String(x.account || ""), recipient: String(x.recipient || ""),
                 words: String(x.words_text || ""), stage: String(x.stage || "") }
    })
}

/// The card's "Bridging to Ethereum" rows, newest first: every one not received yet, and the
/// ones received in the last 30 min (`receivedAt`: id → ms, while Names was open).
export function sendBackRowsShown(list, statuses, receivedAt, nowMs) {
    var st = statuses || {}, at = receivedAt || {}
    return (Array.isArray(list) ? list : []).map(function(x) { var s = st[x.id] || { stage: x.stage }; return { row: x, status: s } })
        .filter(function(e) { return e.status.stage !== "released" || (at[e.row.id] && Number(nowMs) - at[e.row.id] < RECEIVE_CLAIMED_SHOWN_MS) })
        .map(function(e) { return { id: e.row.id, amount: e.row.amount, account: e.row.account, recipient: e.row.recipient, progress: sendBackProgress(e.status, Number(nowMs) / 1000) } })
}

/// The send-back screen's steps: [{key, n, total, title, short, state}]. `s`: {stage ("choose" |
/// "address" (the code being given) | "needs" | a send-back's stage), needs, extras}. Amount, the
/// Ethereum address, the first-time questions the bridge asks, Approve, then the way back.
export function sendBackSteps(s) {
    var st = s || {}
    var extras = (st.extras || []).map(needsStep)
    var needs = needsStep(st.needs)
    if (needs && extras.indexOf(needs) < 0)
        extras = extras.concat([needs])
    var keys = ["choose", "address"].concat(extras, ["burn", "back"])
    var titles = { choose: "Choose the account and the amount", address: "Give the Ethereum address", connect: "Let the bridge see this account",
                   programs: "Allow the bridge's programs", public_account: "Give the bridge this account",
                   secret: "Allow the bridge's exit keys", burn: "Approve the burn", back: st.stage === "released" ? "Received on Ethereum" : "Back to Ethereum" }
    var stage = String(st.stage || "choose")
    var now = stage === "choose" ? "choose" : stage === "address" ? "address" : stage === "needs" ? (needs || "burn")
        : stage === "planned" || stage === "approve" ? "burn" : "back"
    var at = stage === "released" ? keys.length : keys.indexOf(now)
    return keys.map(function(k, i) {
        var state = i < at ? "done" : i === at ? "now" : "later"
        return { key: k, n: i + 1, total: keys.length, title: titles[k] || k, short: stepShort(k, state), state: state, line: "" }
    })
}

/// What the screen says about a `send_back_burn` that can't propose: `needs: "names_update"`
/// (an older Names Wallet), the bridge's own message otherwise.
export function sendBackBurnHeld(value) {
    if (value && value.needs === "names_update")
        return "This Names Wallet can't sign a burn yet. Nothing was signed or sent."
    return ""
}

/// Where the release prover is installed beside the v4 helper (rfp-022 public-demo/prove/
/// release-prover.sh): set in the LEZ Bridge (`set_release_prover`) the first time "Make the
/// proof" finds none set, if it is there; the bridge refuses a path with no program.
export const RELEASE_PROVER = "/Volumes/exos162/logos/rfp-022/v4/release-prover.sh"

/// A check that answered "too many requests" (the endpoint's 429, after the bridge's own retries):
/// a wait, not a failure.
export function rateLimited(message) {
    return /\b429\b|too many requests|rate.?limit|endpoint is busy/i.test(String(message || ""))
}

/// The accounts Send back can start from: each of the wallet's own public accounts on the card
/// (bridgeNetRows: not view-only, not watched) with a token on bridge v4 it holds at least one
/// denomination of. [{key, account, symbol, amount, asset}], in the card's order.
export function sendBackSources(blocks) {
    var out = []
    ;(Array.isArray(blocks) ? blocks : []).forEach(function(b) {
        if (!b || b.viewOnly || b.watched) return
        ;(b.assets || []).forEach(function(a) {
            if (sendBackOffer(a))
                out.push({ key: String(b.account) + "|" + a.symbol, account: String(b.account), symbol: a.symbol, amount: a.amount, asset: a })
        })
    })
    return out
}

/// The Check now button: "Checking…" (and not pressable) from the press until the answer, else
/// "Check now"; `asked`: pressed, waiting for a call under way to end first.
export function checkButton(polling, asked) {
    return polling || asked ? { label: "Checking…", enabled: false } : { label: "Check now", enabled: true }
}
