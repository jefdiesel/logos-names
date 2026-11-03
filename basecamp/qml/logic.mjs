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
