// The Testimonial mini-app's own logic, kept apart from the QML so the Node test
// (../../logic.test.mjs) runs the same code the app does. The QML engine imports this
// as an ECMAScript module: `import "logic.mjs" as Logic`.
//
// It builds the testimonial program's `Post` message and the two PDA seeds exactly as
// `testimonial/core` does (checked against vectors that a Rust test writes), and reads a
// submission's list back from plain `getAccountView` answers, with the same checks as
// `testimonial_core::walk`. Pure JavaScript: the QML engine has no crypto, no BigInt
// and no TextEncoder, so SHA-256, base58, UTF-8 and borsh are written out here.

// ── Constants (testimonial/core/src/lib.rs) ─────────────────────────────────────────

export const SCHEMA = 1
export const MAX_SUBMISSION = 64
export const MAX_USERNAME = 32
export const MAX_TEXT = 1024
export const TIME_WINDOW_MS = 10 * 60 * 1000
export const HEAD_DOMAIN = "testimonial/v1/head"
export const NODE_DOMAIN = "testimonial/v1/node"
export const ADOPTION_TOTAL = 150
export const ADOPTION_MONTHS = 2
export const ADOPTION_PER_MONTH = 30
/// Our λPrize submission (LP-0021, Names Wallet).
export const DEFAULT_SUBMISSION = "lp-0021:names-wallet"
/// `/LEE/v0.2/AccountId/PDA/` zero-padded to 32 bytes (lee_core `for_public_pda`).
const PDA_PREFIX = "/LEE/v0.2/AccountId/PDA/"
/// The clock program and its every-block account (clock_core, LEZ 0.4). Its state is
/// borsh `{block_id: u64, timestamp: u64}`: the last block's time in milliseconds.
/// Both are checked against the Rust values in test/vectors.json.
export const CLOCK_PROGRAM = "2hwaRW8sMnyKLLRbDRw8k68tiv9bnfq98oxcdtcp157Z"
export const CLOCK_ACCOUNT = "4BdcjoXkq786TMWcBGGHqcxeLYMZmn17rL4eM9ZyRWNU"

// ── Bytes ──────────────────────────────────────────────────────────────────────────

/// UTF-8 bytes of a string (lone surrogates become U+FFFD, as in Rust's lossy encoding).
export function utf8(text) {
    var out = []
    for (var i = 0; i < text.length; i++) {
        var c = text.charCodeAt(i)
        if (c >= 0xd800 && c <= 0xdbff && i + 1 < text.length) {
            var d = text.charCodeAt(i + 1)
            if (d >= 0xdc00 && d <= 0xdfff) {
                c = 0x10000 + ((c - 0xd800) << 10) + (d - 0xdc00)
                i++
            } else c = 0xfffd
        } else if (c >= 0xd800 && c <= 0xdfff) c = 0xfffd
        if (c < 0x80) out.push(c)
        else if (c < 0x800) out.push(0xc0 | (c >> 6), 0x80 | (c & 63))
        else if (c < 0x10000) out.push(0xe0 | (c >> 12), 0x80 | ((c >> 6) & 63), 0x80 | (c & 63))
        else out.push(0xf0 | (c >> 18), 0x80 | ((c >> 12) & 63), 0x80 | ((c >> 6) & 63), 0x80 | (c & 63))
    }
    return out
}

/// A string from UTF-8 bytes; throws on malformed input (borsh strings are valid UTF-8).
export function fromUtf8(bytes) {
    var out = ""
    for (var i = 0; i < bytes.length;) {
        var b = bytes[i], c, n
        if (b < 0x80) { c = b; n = 0 }
        else if ((b & 0xe0) === 0xc0) { c = b & 31; n = 1 }
        else if ((b & 0xf0) === 0xe0) { c = b & 15; n = 2 }
        else if ((b & 0xf8) === 0xf0) { c = b & 7; n = 3 }
        else throw new Error("not UTF-8")
        for (var k = 1; k <= n; k++) {
            var x = bytes[i + k]
            if (x === undefined || (x & 0xc0) !== 0x80) throw new Error("not UTF-8")
            c = (c << 6) | (x & 63)
        }
        i += n + 1
        if (c >= 0x10000) {
            c -= 0x10000
            out += String.fromCharCode(0xd800 + (c >> 10), 0xdc00 + (c & 1023))
        } else out += String.fromCharCode(c)
    }
    return out
}

export function hex(bytes) {
    var out = ""
    for (var i = 0; i < bytes.length; i++) out += (bytes[i] < 16 ? "0" : "") + bytes[i].toString(16)
    return out
}

export function unhex(text) {
    var digits = String(text).replace(/^0x/, "")
    if (digits.length % 2 !== 0 || !/^[0-9a-fA-F]*$/.test(digits)) throw new Error("not hexadecimal")
    var out = []
    for (var i = 0; i < digits.length; i += 2) out.push(parseInt(digits.substr(i, 2), 16))
    return out
}

// ── SHA-256 (FIPS 180-4) ─────────────────────────────────────────────────────────────

const K = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2]

/// SHA-256 of a byte array, as a 32-byte array.
export function sha256(bytes) {
    var len = bytes.length
    var padded = Array.prototype.slice.call(bytes)
    padded.push(0x80)
    while (padded.length % 64 !== 56) padded.push(0)
    var bits = len * 8
    var high = Math.floor(bits / 0x100000000), low = bits >>> 0
    padded.push((high >>> 24) & 255, (high >>> 16) & 255, (high >>> 8) & 255, high & 255,
                (low >>> 24) & 255, (low >>> 16) & 255, (low >>> 8) & 255, low & 255)
    var h = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19]
    var w = new Array(64)
    for (var off = 0; off < padded.length; off += 64) {
        for (var t = 0; t < 16; t++)
            w[t] = (padded[off + 4 * t] << 24) | (padded[off + 4 * t + 1] << 16) | (padded[off + 4 * t + 2] << 8) | padded[off + 4 * t + 3]
        for (t = 16; t < 64; t++) {
            var x = w[t - 15], y = w[t - 2]
            var s0 = ((x >>> 7) | (x << 25)) ^ ((x >>> 18) | (x << 14)) ^ (x >>> 3)
            var s1 = ((y >>> 17) | (y << 15)) ^ ((y >>> 19) | (y << 13)) ^ (y >>> 10)
            w[t] = (w[t - 16] + s0 + w[t - 7] + s1) | 0
        }
        var a = h[0], b = h[1], c = h[2], d = h[3], e = h[4], f = h[5], g = h[6], k = h[7]
        for (t = 0; t < 64; t++) {
            var S1 = ((e >>> 6) | (e << 26)) ^ ((e >>> 11) | (e << 21)) ^ ((e >>> 25) | (e << 7))
            var ch = (e & f) ^ (~e & g)
            var t1 = (k + S1 + ch + K[t] + w[t]) | 0
            var S0 = ((a >>> 2) | (a << 30)) ^ ((a >>> 13) | (a << 19)) ^ ((a >>> 22) | (a << 10))
            var maj = (a & b) ^ (a & c) ^ (b & c)
            var t2 = (S0 + maj) | 0
            k = g; g = f; f = e; e = (d + t1) | 0; d = c; c = b; b = a; a = (t1 + t2) | 0
        }
        h[0] = (h[0] + a) | 0; h[1] = (h[1] + b) | 0; h[2] = (h[2] + c) | 0; h[3] = (h[3] + d) | 0
        h[4] = (h[4] + e) | 0; h[5] = (h[5] + f) | 0; h[6] = (h[6] + g) | 0; h[7] = (h[7] + k) | 0
    }
    var out = []
    for (var i = 0; i < 8; i++) out.push((h[i] >>> 24) & 255, (h[i] >>> 16) & 255, (h[i] >>> 8) & 255, h[i] & 255)
    return out
}

// ── Base58 (Bitcoin alphabet), as LEZ writes account IDs ────────────────────────────

const ALPHABET = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"

export function base58(bytes) {
    var digits = [0]
    for (var i = 0; i < bytes.length; i++) {
        var carry = bytes[i]
        for (var j = 0; j < digits.length; j++) {
            carry += digits[j] * 256
            digits[j] = carry % 58
            carry = Math.floor(carry / 58)
        }
        while (carry > 0) { digits.push(carry % 58); carry = Math.floor(carry / 58) }
    }
    var out = ""
    for (var z = 0; z < bytes.length && bytes[z] === 0; z++) out += "1"
    // All zeros: only the leading '1's (the digit loop above left a lone 0).
    if (z === bytes.length) return out
    for (var d = digits.length - 1; d >= 0; d--) out += ALPHABET[digits[d]]
    return out
}

/// Bytes of a base58 string; throws on a character outside the alphabet.
export function unbase58(text) {
    var bytes = [0]
    for (var i = 0; i < text.length; i++) {
        var value = ALPHABET.indexOf(text[i])
        if (value < 0) throw new Error("not base58")
        var carry = value
        for (var j = 0; j < bytes.length; j++) {
            carry += bytes[j] * 58
            bytes[j] = carry & 255
            carry >>= 8
        }
        while (carry > 0) { bytes.push(carry & 255); carry >>= 8 }
    }
    var zeros = 0
    while (zeros < text.length && text[zeros] === "1") zeros++
    while (bytes.length > 1 && bytes[bytes.length - 1] === 0) bytes.pop()
    if (bytes.length === 1 && bytes[0] === 0) bytes = []
    var out = []
    for (var z = 0; z < zeros; z++) out.push(0)
    for (var k = bytes.length - 1; k >= 0; k--) out.push(bytes[k])
    return out
}

/// The 32 bytes of an account ID written in base58, `Public/<id>` or bare; throws otherwise.
export function accountBytes(text) {
    var bare = String(text).trim().replace(/^Public\//, "")
    if (/^Private\//.test(bare)) throw new Error("a testimonial is posted from a public account")
    var bytes = unbase58(bare)
    if (bytes.length !== 32) throw new Error("not an account ID")
    return bytes
}

/// Whether `text` is an account ID (base58 of 32 bytes), with or without `Public/`.
export function isAccount(text) {
    try { accountBytes(text); return true } catch (error) { return false }
}

// ── Seeds and addresses (testimonial_core, SPEL compute_pda) ─────────────────────────

/// A SPEL string seed component: UTF-8, zero-padded to 32 bytes.
export function seedStr(text) {
    var bytes = utf8(text)
    if (bytes.length > 32) throw new Error("a seed string is at most 32 bytes")
    while (bytes.length < 32) bytes.push(0)
    return bytes
}

export function submissionKey(submission) { return sha256(utf8(submission)) }

export function headSeed(submission) {
    return sha256(seedStr(HEAD_DOMAIN).concat(submissionKey(submission)))
}

export function nodeSeed(submission, authorBytes) {
    return sha256(seedStr(NODE_DOMAIN).concat(submissionKey(submission), authorBytes))
}

/// `AccountId::for_public_pda(program, seed)`.
export function pda(programBytes, seed) {
    return sha256(seedStr(PDA_PREFIX).concat(programBytes, seed))
}

export function headAddress(program, submission) { return base58(pda(accountBytes(program), headSeed(submission))) }

export function nodeAddress(program, submission, author) {
    return base58(pda(accountBytes(program), nodeSeed(submission, accountBytes(author))))
}

// ── Validation (the program's own rules, so a post it would refuse is never proposed) ─

/// Rust `char::is_control`: the Unicode Cc category.
function isControl(code) { return code < 0x20 || (code >= 0x7f && code <= 0x9f) }

/// Rust `str::trim`: Unicode White_Space.
const SPACE = "[" + [9, 10, 11, 12, 13, 32, 0x85, 0xa0, 0x1680, 0x2000, 0x2001, 0x2002, 0x2003, 0x2004, 0x2005, 0x2006,
    0x2007, 0x2008, 0x2009, 0x200a, 0x2028, 0x2029, 0x202f, 0x205f, 0x3000].map(function(c) { return String.fromCharCode(c) }).join("") + "]+"
const WHITE = new RegExp("^" + SPACE + "|" + SPACE + "$", "g")
export function rustTrim(text) { return String(text).replace(WHITE, "") }

/// Why the program would refuse this submission identifier, or "" if it is fine.
export function submissionProblem(submission) {
    var s = String(submission)
    var ok = s.length >= 1 && s.length <= MAX_SUBMISSION && /^[a-z0-9][a-z0-9._:\/-]*$/.test(s)
    return ok ? "" : "A submission ID is 1-" + MAX_SUBMISSION + " characters of a-z, 0-9 and . _ : / -, starting with a letter or digit."
}

/// Why the program would refuse this username ("" means none, which is fine), or "".
export function usernameProblem(username) {
    var u = String(username)
    if (u === "") return ""
    var n = utf8(u).length
    if (n > MAX_USERNAME) return "A username is at most " + MAX_USERNAME + " bytes (this one is " + n + ")."
    for (var i = 0; i < u.length; i++)
        if (isControl(u.charCodeAt(i))) return "A username can't hold control characters."
    if (rustTrim(u) !== u) return "A username can't start or end with a space."
    return ""
}

/// Why the program would refuse this text, or "".
export function textProblem(text) {
    var t = String(text)
    if (rustTrim(t) === "") return "Write something first."
    var n = utf8(t).length
    if (n > MAX_TEXT) return "A testimonial holds at most " + MAX_TEXT + " bytes (this one is " + n + ")."
    for (var i = 0; i < t.length; i++) {
        var c = t.charCodeAt(i)
        if (isControl(c) && c !== 10 && c !== 9) return "A testimonial can't hold control characters other than line breaks and tabs."
    }
    return ""
}

// ── Borsh ────────────────────────────────────────────────────────────────────────────

function u32(out, n) { out.push(n & 255, (n >>> 8) & 255, (n >>> 16) & 255, (n >>> 24) & 255) }

/// A whole number below 2^53 as 8 little-endian bytes.
function u64(out, n) {
    if (!(n >= 0) || Math.floor(n) !== n || n > 9007199254740991) throw new Error("not a u64 this app can write")
    var low = n % 0x100000000, high = Math.floor(n / 0x100000000)
    u32(out, low >>> 0)
    u32(out, high >>> 0)
}

function str(out, text) {
    var bytes = utf8(text)
    u32(out, bytes.length)
    for (var i = 0; i < bytes.length; i++) out.push(bytes[i])
}

/// `borsh(Message::Instruction(Instruction::Post { submission, username, text, now }))`:
/// Message variant 0, Instruction variant 0, then the fields in order.
export function postMessage(submission, username, text, now) {
    var out = [0, 0]
    str(out, submission)
    if (username === null || username === undefined || username === "") out.push(0)
    else { out.push(1); str(out, username) }
    str(out, text)
    u64(out, now)
    return out
}

/// Everything `propose_call` needs for a post by `author` (base58, the account the person
/// picked in this app). The signer's own actor of the program comes first and is not
/// listed; then the submission's head and the author's testimonial account, as seeds.
export function postCall(program, submission, username, text, now, author) {
    var authorBytes = accountBytes(author)
    var programBytes = accountBytes(program)
    var head = headSeed(submission)
    var node = nodeSeed(submission, authorBytes)
    return {
        program: base58(programBytes),
        accounts_json: JSON.stringify([{ seed: hex(head) }, { seed: hex(node) }]),
        instruction_hex: hex(postMessage(submission, username, text, now)),
        head: base58(pda(programBytes, head)),
        node: base58(pda(programBytes, node)),
    }
}

/// A borsh reader over a byte array.
export function Reader(bytes) {
    var at = 0
    function take(n) {
        if (at + n > bytes.length) throw new Error("the record is cut short")
        var part = bytes.slice(at, at + n)
        at += n
        return part
    }
    function readU8() { return take(1)[0] }
    function readU32() { var b = take(4); return (b[0] | (b[1] << 8) | (b[2] << 16)) + b[3] * 0x1000000 }
    function readU64() {
        var low = readU32(), high = readU32()
        if (high > 0x1fffff) throw new Error("a number too large to show exactly")
        return high * 0x100000000 + low
    }
    function readString() { return fromUtf8(take(readU32())) }
    function readAccount() { return base58(take(32)) }
    function readOption(read) {
        var tag = readU8()
        if (tag === 0) return null
        if (tag === 1) return read()
        throw new Error("a bad option tag")
    }
    return {
        u8: readU8, u32: readU32, u64: readU64, string: readString, account: readAccount, option: readOption,
        done: function() { return at === bytes.length },
    }
}

/// A `Head` record, or throws.
export function decodeHead(bytes) {
    var r = Reader(bytes)
    var head = {
        schema: r.u8(), submission: r.string(), count: r.u64(),
        newest: r.option(r.account), first_at: r.u64(), last_at: r.u64(),
    }
    if (!r.done()) throw new Error("a head has no trailing bytes")
    return head
}

/// A `Testimonial` record, or throws.
export function decodeTestimonial(bytes) {
    var r = Reader(bytes)
    var t = {
        schema: r.u8(), submission: r.string(), author: r.account(), seq: r.u64(), at: r.u64(),
        username: r.option(r.string), text: r.string(), prev: r.option(r.account),
    }
    if (!r.done()) throw new Error("a testimonial has no trailing bytes")
    return t
}

// ── Reading the chain ────────────────────────────────────────────────────────────────

/// The JSON-RPC body of `getAccountView` for `program`'s state on `account` (both base58).
export function accountViewRequest(account, program) {
    return JSON.stringify({ jsonrpc: "2.0", id: 1, method: "getAccountView",
                            params: [{ account_id: account, program_account_id: program }] })
}

/// `program`'s state on an account, from a `getAccountView` result
/// (`{nonce, data: {actor_states: {"<program>": [bytes…]}}}`, or null): a byte array,
/// empty when it holds none.
export function actorState(result, program) {
    if (result === null || result === undefined) return []
    var states = result.data && result.data.actor_states
    var values = states ? states[program] : undefined
    if (values === undefined || values === null) return []
    if (!Array.isArray(values)) throw new Error("the sequencer sent malformed account data")
    for (var i = 0; i < values.length; i++)
        if (typeof values[i] !== "number" || values[i] < 0 || values[i] > 255 || Math.floor(values[i]) !== values[i])
            throw new Error("the sequencer sent malformed account data")
    return values
}

/// The chain's time from the clock account's state (`{block_id: u64, timestamp: u64}`).
export function clockTime(bytes) {
    var r = Reader(bytes)
    r.u64()
    return r.u64()
}

/// The `now` a post carries: the chain's last block time if it was read, else this
/// computer's time minus a minute (as the `testimonial` client does). The program allows
/// blocks in `[now, now + 10 min)`, so `now` must not run ahead of the chain.
export function postTime(chainTime, localNow) {
    return chainTime > 0 ? chainTime : Math.max(1, localNow - 60000)
}

/// Walks a submission's list from its head, newest first, with the checks of
/// `testimonial_core::walk`: each record is this submission's, at the address derived
/// from its own author (so each account appears once), numbered count-1 down to 0.
/// `read(account, callback(error, bytes))` reads this program's state on `account`.
/// `progress(listing)` is told after each record. Ends with `done(error, listing)`.
export function walk(program, submission, read, progress, done) {
    var problem = submissionProblem(submission)
    if (problem) return done(problem, null)
    var programBytes
    try { programBytes = accountBytes(program) } catch (error) { return done("The program ID is not an account ID.", null) }
    var headAt = base58(pda(programBytes, headSeed(submission)))
    var listing = { program: program, submission: submission, head_address: headAt, head: null, items: [] }
    var authors = {}
    read(headAt, function(error, state) {
        if (error) return done("Reading the list's head: " + error, null)
        if (state.length === 0) return done(null, listing)
        var head
        try { head = decodeHead(state) } catch (e) { return done(headAt + " does not hold a list head.", null) }
        if (head.schema !== SCHEMA || head.submission !== submission) return done(headAt + " is not the head of " + submission + ".", null)
        listing.head = head
        step(head.newest)
    })
    function step(at) {
        if (at === null) {
            if (listing.items.length !== listing.head.count)
                return done("The list has " + listing.items.length + " testimonials but its head counts " + listing.head.count + ".", null)
            return done(null, listing)
        }
        var expected = listing.head.count - 1 - listing.items.length
        if (expected < 0) return done("The list is longer than its count " + listing.head.count + ".", null)
        read(at, function(error, state) {
            if (error) return done("Reading " + at + ": " + error, null)
            if (state.length === 0) return done("The list points at " + at + ", which holds no testimonial.", null)
            var t
            try { t = decodeTestimonial(state) } catch (e) { return done(at + " does not hold a testimonial.", null) }
            if (t.schema !== SCHEMA || t.submission !== submission) return done(at + " is not a testimonial for " + submission + ".", null)
            if (base58(pda(programBytes, nodeSeed(submission, accountBytes(t.author)))) !== at)
                return done(at + " is not the testimonial account of its author " + t.author + ".", null)
            if (authors[t.author]) return done(t.author + " appears twice.", null)
            authors[t.author] = true
            if (t.seq !== expected) return done(at + " is number " + t.seq + ", expected " + expected + ".", null)
            t.address = at
            listing.items.push(t)
            if (progress) progress(listing)
            step(t.prev)
        })
    }
}

// ── Months and the adoption bar ─────────────────────────────────────────────────────

function pad(n, width) { var s = String(n); while (s.length < width) s = "0" + s; return s }

/// `YYYY-MM` (UTC) of a time in milliseconds.
export function monthOf(ms) {
    var d = new Date(ms)
    return pad(d.getUTCFullYear(), 4) + "-" + pad(d.getUTCMonth() + 1, 2)
}

/// `YYYY-MM-DD HH:MM UTC` of a time in milliseconds.
export function formatTime(ms) {
    var d = new Date(ms)
    return pad(d.getUTCFullYear(), 4) + "-" + pad(d.getUTCMonth() + 1, 2) + "-" + pad(d.getUTCDate(), 2) + " " +
        pad(d.getUTCHours(), 2) + ":" + pad(d.getUTCMinutes(), 2) + " UTC"
}

/// The numbers the adoption criteria are judged on (`testimonial_core::summarize`).
/// `months` is [{month, count, qualifies}], oldest first.
export function summarize(items) {
    var perMonth = {}, authors = {}, distinct = 0
    for (var i = 0; i < items.length; i++) {
        var m = monthOf(items[i].at)
        perMonth[m] = (perMonth[m] || 0) + 1
        if (!authors[items[i].author]) { authors[items[i].author] = true; distinct++ }
    }
    var months = Object.keys(perMonth).sort().map(function(month) {
        return { month: month, count: perMonth[month], qualifies: perMonth[month] >= ADOPTION_PER_MONTH }
    })
    var qualifying = months.filter(function(m) { return m.qualifies }).length
    return {
        total: items.length, distinct_authors: distinct, months: months, qualifying_months: qualifying,
        meets_adoption: items.length >= ADOPTION_TOTAL && distinct >= ADOPTION_TOTAL && qualifying >= ADOPTION_MONTHS,
    }
}

// ── Accounts the person gave this app ──────────────────────────────────────────────

/// The granted accounts a testimonial can come from: public ones only (a private
/// sender reaches the program under a fresh alias, which it refuses).
export function postingAccounts(granted) {
    return (granted || []).filter(function(entry) {
        var account = typeof entry === "string" ? entry : entry.account
        return /^Public\//.test(String(account))
    }).map(function(entry) { return typeof entry === "string" ? entry : entry.account })
}

/// `abcdef…wxyz`.
export function short(text) {
    var s = String(text)
    return s.length <= 14 ? s : s.substr(0, 6) + "…" + s.substr(s.length - 4)
}

/// The summary the approval sheet shows as this app's words (at most 280 characters).
export function postSummary(submission, author, text) {
    var quoted = rustTrim(text).replace(/\s+/g, " ")
    if (quoted.length > 120) quoted = quoted.substr(0, 119) + "…"
    return ("Post a testimonial for " + submission + " from " + author + ": “" + quoted + "”").substr(0, 280)
}
