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
