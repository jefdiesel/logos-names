// Collectibles (NFTs of the reference NFT program, docs/wallet/NFT.md): the rules the view
// follows, kept here so `node basecamp/nft.test.mjs` can check them without a QML engine.
// Words: "LNS names", never "official", never "lookup".

const ID = /^[1-9A-HJ-NP-Za-km-z]{32,44}$/
/// A viewing public key in hex: 1184 bytes.
export const VPK_HEX = 2368
/// Longest on-chain media URI in bytes (`nft_wire::MAX_URI_LEN`).
export const MAX_MEDIA_URI = 256
/// Default loopback REST URL of the person's own Logos Storage node.
export const DEFAULT_STORAGE_URL = "http://127.0.0.1:8080"

/// Plain label for whether an NFT is held by a public or private account.
export function privacyLabel(isPrivate) {
  return isPrivate ? "Private" : "Public"
}

/// An ID shortened in the middle for a row; the full one is copied.
export function idShown(id) {
  const text = String(id || "")
  return text.length > 14 ? text.substr(0, 6) + "…" + text.substr(text.length - 4) : text
}

/// An account as a row shows it: "Public CCVd76…pKnk" / "Private yGrPEb…fe4Z".
export function accountShown(text) {
  const [kind, id] = String(text || "").split("/")
  return id === undefined ? idShown(kind) : kind + " " + idShown(id)
}

/// The token, as the program has it: there is no serial number, only the master or a copy.
export function tokenLine(row) {
  if (!row) return ""
  if (row.state === "master") {
    const rights = Number(row.print_balance || "0")
    const prints = Math.max(0, rights - 1)
    return "Master · " + (prints === 0 ? "no prints left" : prints === 1 ? "can print 1 more copy" : "can print " + prints + " more copies")
  }
  if (row.state === "copy") return "Copy"
  return ""
}

/// A collection's title, never empty: its name, or that it can't be read, with its ID.
export function collectionTitle(collection, definition) {
  if (collection && typeof collection.name === "string" && collection.name.trim() !== "") return collection.name
  return "Unreadable collection " + idShown(definition)
}

/// Every field the program stores for a collection, as rows; a missing one says so.
export function metadataRows(collection) {
  if (!collection) return [{ label: "Metadata", value: "Can't be read: the wallet shows this NFT by its IDs only." }]
  const field = (value) => (value === null || value === undefined || value === "" ? "none given" : String(value))
  return [
    { label: "Supply", value: field(collection.printable_supply) },
    { label: "Standard", value: field(collection.standard) },
    { label: "URI", value: field(collection.uri) },
    { label: "Creators", value: field(collection.creators) },
  ]
}

/// The NFTs to list, by account then collection: only states that hold an NFT, from every
/// listed program, unreadable ones included (shown as such, never hidden).
export function accountGroups(programs) {
  const groups = []
  for (const program of programs || []) {
    for (const row of program.holdings || []) {
      if (!row.holds && row.state !== "unreadable") continue
      let group = groups.find((g) => g.account === row.account)
      if (!group) {
        group = { account: row.account, isPrivate: row.account_kind === "private", nfts: [] }
        groups.push(group)
      }
      group.nfts.push({ program: program.program, programLabel: program.label || idShown(program.program), definition: row.definition, state: row.state, print_balance: row.print_balance, collection: row.collection, unreadable: row.unreadable, isPrivate: row.account_kind === "private" })
    }
  }
  return groups
}

/// The collection view: which collections the wallet's accounts hold, and how many of each.
export function collectionCounts(programs) {
  const out = []
  for (const program of programs || [])
    for (const c of program.collections || [])
      out.push({ program: program.program, definition: c.definition, title: c.readable ? c.name : collectionTitle(null, c.definition), count: c.count, accounts: c.accounts })
  return out
}

/// The collection grid cards: one card per held collection, with art metadata, count held,
/// Public/Private breakdown, and the first holding account so a click opens its item page.
export function collectionCards(programs) {
  const out = []
  for (const program of programs || []) {
    const holdings = program.holdings || []
    for (const c of program.collections || []) {
      const held = holdings.filter((r) => r.holds && r.definition === c.definition)
      const publicCount = held.filter((r) => r.account_kind !== "private").length
      const privateCount = held.filter((r) => r.account_kind === "private").length
      const withMeta = held.find((r) => r.collection) || null
      const collection = withMeta ? withMeta.collection : null
      const firstAccount = held.length > 0 ? held[0].account : (c.accounts && c.accounts[0]) || ""
      const countText = c.count === 1 ? "1 held" : c.count + " held"
      const scope = privateCount > 0 && publicCount > 0
        ? publicCount + " Public, " + privateCount + " Private"
        : privateCount > 0 ? "Private" : publicCount > 0 ? "Public" : ""
      out.push({
        program: program.program,
        programLabel: program.label || idShown(program.program),
        definition: c.definition,
        key: program.program + ":" + c.definition,
        title: c.readable ? c.name : collectionTitle(null, c.definition),
        readable: !!c.readable,
        count: c.count,
        publicCount: publicCount,
        privateCount: privateCount,
        isPrivateOnly: privateCount > 0 && publicCount === 0,
        countLabel: scope !== "" ? countText + " · " + scope : countText,
        collection: collection,
        uri: collection && collection.uri ? collection.uri : "",
        firstAccount: firstAccount,
        accounts: c.accounts || [],
      })
    }
  }
  return out
}

/// Accounts that can be made ready for `definition` under `program` (nothing under it yet), and
/// those already ready (a holding of it that holds nothing).
export function receiveChoices(programs, program, definition) {
  const entry = (programs || []).find((p) => p.program === program)
  const rows = entry ? entry.holdings || [] : []
  return {
    empty: rows.filter((r) => r.state === "empty").map((r) => r.account),
    ready: rows.filter((r) => r.definition === definition && !r.holds && (r.state === "copy" || r.state === "master")).map((r) => r.account),
  }
}

/// A collection link a sender gives: `nft:<program>/<definition>`; or just the definition's ID
/// when the program is chosen. `{program, definition}` or `{problem}`.
export function collectionLink(text, programChosen) {
  const typed = String(text || "").trim()
  if (typed === "") return { problem: "Paste the collection's link or its ID." }
  const link = /^nft:([1-9A-HJ-NP-Za-km-z]{32,44})\/([1-9A-HJ-NP-Za-km-z]{32,44})$/.exec(typed)
  if (link) return { program: link[1], definition: link[2] }
  if (ID.test(typed)) return programChosen ? { program: programChosen, definition: typed } : { problem: "Choose the NFT program first, or paste the whole link (nft:…/…)." }
  return { problem: "That isn't a collection link (nft:<program>/<collection>) or a collection's ID." }
}

export function linkOf(program, definition) {
  return "nft:" + program + "/" + definition
}

/// Why a Send destination can't be used as typed, or "". An LNS name, Public/<id>, one of this
/// wallet's Private/<id>, or someone's receiving keys <npk hex>:<vpk hex>.
export function toProblem(text) {
  const typed = String(text || "").trim()
  if (typed === "") return "Enter an LNS name, an account (Public/…) or receiving keys."
  if (/^(Public|Private)\//.test(typed)) return ID.test(typed.split("/")[1]) ? "" : "After Public/ or Private/ comes the account's ID: letters and digits."
  // An LNS name may be written lns:alice or @alice (SPEC.md §2): a name, never receiving keys.
  const bare = nameBare(typed)
  if (bare !== typed) return /^[a-z0-9][a-z0-9-]{0,62}(\.lez)?$/i.test(bare) ? "" : "That isn't an LNS name, an account or receiving keys."
  if (typed.indexOf(":") >= 0) {
    const [npk, vpk] = typed.split(":")
    // A real vpk is an ML-KEM-768 key: 1184 bytes, 2368 hex characters (the module's VPK_HEX).
    return /^[0-9a-fA-F]{64}$/.test(npk) && new RegExp("^[0-9a-fA-F]{" + VPK_HEX + "}$").test(vpk || "") ? "" : "Receiving keys are written <npk hex>:<vpk hex>: 64 and 2368 hex characters."
  }
  if (ID.test(typed)) return "Write an account as Public/ and then its ID."
  return /^[a-z0-9][a-z0-9-]{0,62}(\.lez)?$/i.test(typed) ? "" : "That isn't an LNS name, an account or receiving keys."
}

/// `typed` without an LNS name's lns: or @ in front (deep review B10); anything else as it is.
function nameBare(typed) {
  return typed.replace(/^lns:/i, "").replace(/^@/, "")
}

/// Whether the destination is someone's receiving keys (can't be checked: an extra tick).
/// `lns:alice` is a name.
export function toTheirKeys(text) {
  const typed = String(text || "").trim()
  return nameBare(typed) === typed && typed.indexOf(":") > 0
}

/// Why the sheet's primary button can't be pressed yet, or "".
export function approveWhy(sheet, ticks) {
  if (!sheet) return "Checking what this does…"
  if (sheet.refusal) return ""
  const t = ticks || {}
  const payers = sheet.payers || []
  if (sheet.proven && !payers.some((p) => p.enough)) return "None of your private accounts holds the fee reserve. Move some LEZ to a private account first."
  if (!t.payer && (sheet.proven || payers.length > 0)) return sheet.proven ? "Choose the private account that pays the fee." : "Choose the account that pays the fee."
  if (t.payer && !payers.some((p) => p.account === t.payer && p.enough)) return "That account can't cover the fee; choose another."
  if ((sheet.warnings || []).length > 0 && !t.warnings) return "Tick the box above to go ahead."
  if (sheet.action === "burn" && !t.burn) return "Tick that you understand it can't be undone."
  return ""
}

/// The first private account the sheet says can pay, or "".
export function defaultPayer(sheet, from) {
  const payers = (sheet && sheet.payers) || []
  const own = payers.find((p) => p.account === from && p.enough)
  if (own) return own.account
  const any = payers.find((p) => p.enough)
  return any ? any.account : ""
}

/// What a running NFT job is doing, honest about time: a public one takes a block, a proven
/// one 7-12 minutes on a fast computer.
export function jobLine(job, now) {
  const seconds = Math.max(0, Math.floor(((now || Date.now()) - job.started) / 1000))
  // A job picked up after the app opened again: whether it proves isn't known.
  if (job.proves === undefined) return job.label + ": under way, " + Math.floor(seconds / 60) + " min so far. You can keep using Names meanwhile."
  if (!job.proves) return job.label + ": sending it publicly, one block (about 30 seconds), " + seconds + "s so far."
  return job.label + ": proving it on this computer, so the chain never learns the private side. This takes 7-12 minutes on a fast computer; " + Math.floor(seconds / 60) + " min so far. You can keep using Names meanwhile."
}

/// The line an ended job leaves: done with its block, or the plain reason.
/// The next step for a credit that waited and couldn't land, by its reason.
export function leftLine(row) {
  const who = accountShown(row.account)
  if (row.code === "not_ready") return "1 is still waiting for " + who + ": that account isn't ready for its collection. Make " + who + " ready for it (Receive an NFT, that account), then Receive again."
  if (row.code === "already_holds") return "1 is still waiting for " + who + ": that account already holds an NFT of its collection. Send or burn that one, then Receive again."
  return "1 is still waiting for " + who + ", which is tied to another collection: it can never take this NFT."
}

export function jobEnded(job, state) {
  const left = state.left || []
  if (state.state === "done" && job.kind === "nft_receive" && left.length > 0) {
    const got = state.received > 0 ? "Received " + (state.received === 1 ? "1 NFT" : state.received + " NFTs") + ". " : ""
    return { kind: "problem", text: got + left.map(leftLine).join(" ") }
  }
  if (state.state === "done" && job.kind === "nft_init" && state.account)
    return { kind: "done", text: "Ready: " + accountShown(state.account) + " takes this collection now" + (state.block !== null && state.block !== undefined ? " (block " + state.block + ")" : "") + ". Give the sender this account.", copy: state.account }
  if (state.state === "done") {
    if (job.kind === "nft_receive") return { kind: "done", text: state.nothing ? "There was nothing to receive." : "Received " + (state.received === 1 ? "1 NFT" : state.received + " NFTs") + "." }
    return { kind: "done", text: job.label + ": done" + (state.block !== null && state.block !== undefined ? ", in block " + state.block : "") + "." }
  }
  const why = state.refusal || state.problem || "It didn't go through."
  return { kind: "problem", text: job.label + " didn't go through: " + why }
}

/// The holdings ready to receive (a holding of a collection that holds nothing): what to give a
/// sender. `[{program, account, definition, title}]`.
export function readyRows(programs) {
  const out = []
  for (const program of programs || [])
    for (const r of program.holdings || [])
      if (!r.holds && (r.state === "copy" || r.state === "master"))
        out.push({ program: program.program, account: r.account, isPrivate: r.account_kind === "private", definition: r.definition, title: collectionTitle(r.collection, r.definition) })
  return out
}

/// Whether only private accounts of this wallet hold `definition` under `program` (loading its
/// image could then link them to the person's address).
export function privateOnly(programs, program, definition) {
  const entry = (programs || []).find((p) => p.program === program)
  const holders = (entry ? entry.holdings || [] : []).filter((r) => r.holds && r.definition === definition)
  return holders.length > 0 && holders.every((r) => r.account_kind === "private")
}

/// UTF-8 byte length of a string (pure JS, no TextEncoder / BigInt, matching Rust `.len()`).
function utf8ByteLen(text) {
  let total = 0
  for (let i = 0; i < text.length; i++) {
    const code = text.charCodeAt(i)
    if (code <= 0x7f) total += 1
    else if (code <= 0x7ff) total += 2
    else if (code >= 0xd800 && code <= 0xdbff && i + 1 < text.length) {
      const next = text.charCodeAt(i + 1)
      if (next >= 0xdc00 && next <= 0xdfff) { total += 4; i += 1 }
      else total += 3
    } else total += 3
  }
  return total
}

/// Classifies a collection's media URI: missing, verified Logos Storage (`logos:<cid>#sha256=<64 hex>`),
/// public `https://`, or unverified/unsupported.
export function parseMediaUri(uri) {
  const trimmed = String(uri || "").trim()
  if (trimmed === "") return { kind: "none", state: "missing" }
  if (utf8ByteLen(trimmed) > MAX_MEDIA_URI)
    return { kind: "unsupported", state: "unverified", reason: "This media URI is longer than 256 bytes, so the wallet doesn't load it." }
  if (/[\s\x00-\x1f\x7f]/.test(trimmed))
    return { kind: "unsupported", state: "unverified", reason: "This media URI has spaces or control characters in it, so the wallet doesn't load it." }
  if (trimmed.indexOf("logos:") === 0) {
    const rest = trimmed.substr(6)
    const hashAt = rest.indexOf("#")
    if (hashAt < 0)
      return { kind: "storage_unverified", state: "unverified", reason: "This Logos Storage URI has no SHA-256 hash (#sha256=<64 hex>), so it can't be verified and isn't loaded." }
    const cid = rest.substr(0, hashAt)
    const frag = rest.substr(hashAt + 1)
    if (!/^[A-Za-z0-9]{8,256}$/.test(cid))
      return { kind: "storage_unverified", state: "unverified", reason: "This Logos Storage URI has an invalid content ID, so it isn't loaded." }
    const hm = /^sha256=([0-9a-fA-F]{64})$/.exec(frag)
    if (!hm)
      return { kind: "storage_unverified", state: "unverified", reason: "This Logos Storage URI doesn't carry a valid 64-character SHA-256 hash, so it can't be verified and isn't loaded." }
    return { kind: "storage", state: "opt_in", cid: cid, sha256: hm[1].toLowerCase() }
  }
  const m = /^https:\/\/([A-Za-z0-9.-]+)(?::\d+)?(?=[\/?#]|$)/.exec(trimmed)
  if (m) return { kind: "https", state: "opt_in", host: m[1] }
  return { kind: "unsupported", state: "unverified", reason: "This collection's URI isn't a verified Logos Storage or https address, so the wallet doesn't load it." }
}

/// The words of the image consent for a collection, before anything is fetched. Both Logos Storage
/// and https are opt-in per collection; a private-only collection always requires a second ask.
export function imageConsent(uri, isPrivateOnly, storageNodeUrl) {
  const parsed = parseMediaUri(uri)
  if (parsed.kind === "storage") {
    const node = String(storageNodeUrl || DEFAULT_STORAGE_URL).trim() || DEFAULT_STORAGE_URL
    return {
      allowed: true,
      kind: "storage",
      secondAsk: !!isPrivateOnly,
      tickText: "Load images of this collection from my Logos Storage node.",
      secondTickText: "I understand only private accounts hold this collection and still want to ask the Logos Storage network for its image.",
      text: "Loading the image asks your Logos Storage node (" + node + ") for " + idShown(parsed.cid) + ". That tells the Logos Storage network what file is wanted. The wallet checks its SHA-256 hash before showing it, and shows it only as a PNG picture (at most 1 MiB)." + (isPrivateOnly ? " Only private accounts hold this collection: asking the network for its media can link them to you." : ""),
    }
  }
  if (parsed.kind === "https") {
    return {
      allowed: true,
      kind: "https",
      secondAsk: !!isPrivateOnly,
      tickText: "Load images of this collection.",
      secondTickText: "I understand only private accounts hold this collection and still want to contact " + parsed.host + ".",
      text: "Loading the image asks " + parsed.host + " for it. That host learns your computer's address and that someone looks at this collection. It is shown only as a picture (PNG, at most 1 MiB), never as a page." + (isPrivateOnly ? " Only private accounts hold this collection: loading it can link them to your address." : ""),
    }
  }
  if (parsed.kind === "none")
    return { allowed: false, kind: "none", secondAsk: false, unverified: false, text: "This collection has no media URI." }
  return { allowed: false, kind: parsed.kind, secondAsk: false, unverified: true, text: parsed.reason }
}

/// Whether a fetch error means the fetched bytes failed hash verification.
export function errorIsUnverified(message) {
  return /sha-?256|unverified|hash/i.test(String(message || ""))
}

/// Visible media state for a collection: missing, unverified, not_loaded (opt-in), loading,
/// failed, or ready. Every state is shown explicitly and never hidden.
export function mediaState(collection, entry) {
  if (!collection)
    return { state: "missing", label: "Missing media", detail: "Collection metadata can't be read.", canLoad: false, canRetry: false, kind: "none", pixels: null }
  const parsed = parseMediaUri(collection.uri)
  if (parsed.kind === "none")
    return { state: "missing", label: "Missing media", detail: "This collection lists no image.", canLoad: false, canRetry: false, kind: "none", pixels: null }
  if (parsed.state === "unverified")
    return { state: "unverified", label: "Unverified media", detail: parsed.reason, canLoad: false, canRetry: false, kind: parsed.kind, pixels: null }
  if (entry && entry.state === "ready" && entry.pixels)
    return { state: "ready", label: parsed.kind === "storage" ? "Verified (Logos Storage)" : "Loaded (https)", detail: "", canLoad: false, canRetry: false, kind: parsed.kind, pixels: entry.pixels }
  if (entry && entry.state === "loading")
    return { state: "loading", label: "Loading media…", detail: parsed.kind === "storage" ? "Fetching from your Logos Storage node and checking SHA-256…" : "Fetching from " + parsed.host + "…", canLoad: false, canRetry: false, kind: parsed.kind, pixels: null }
  if (entry && entry.state === "unverified")
    return { state: "unverified", label: "Unverified media", detail: entry.error || "The image didn't match its SHA-256 hash, so it isn't shown.", canLoad: false, canRetry: true, kind: parsed.kind, pixels: null }
  if (entry && entry.state === "failed")
    return { state: "failed", label: "Media failed to load", detail: entry.error || "The image couldn't be loaded.", canLoad: false, canRetry: true, kind: parsed.kind, pixels: null }
  return {
    state: "not_loaded",
    label: parsed.kind === "storage" ? "Logos Storage · opt-in" : "Web image · opt-in",
    detail: "Not loaded until you allow this collection.",
    canLoad: true,
    canRetry: false,
    kind: parsed.kind,
    pixels: null,
  }
}

/// Validates a loopback-only Logos Storage node URL typed in Collectibles, or "".
export function storageNodeProblem(text) {
  const typed = String(text || "").trim()
  if (typed === "") return ""
  const m = /^http:\/\/(?:127\.(?:25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)\.(?:25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)\.(?:25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)|localhost|\[::1\])(?::(\d{1,5}))?\/?$/i.exec(typed)
  if (!m) return "Enter an http:// address on this computer (127.0.0.1, localhost or [::1]), for example " + DEFAULT_STORAGE_URL + "."
  if (m[1] !== undefined) {
    const port = Number(m[1])
    if (port < 1 || port > 65535) return "The port must be between 1 and 65535."
  }
  return ""
}

/// Burn's two steps: what the second asks.
export function burnConfirm(row) {
  if (!row) return ""
  return row.state === "master"
    ? "This destroys the master and every print right it holds. The collection then has no master: nobody can print it again. This can't be undone."
    : "This destroys this copy. This can't be undone."
}

/// A membership proof's status as one line, with the minutes spent proving.
export function proofLine(status, now) {
  const st = status || {}
  switch (st.state) {
    case "awaiting-approval": return "waiting for your answer."
    case "checking": return "Checking your NFT…"
    case "proving": return "Proving (about 10 minutes): " + Math.floor((st.elapsed_seconds || 0) / 60) + " min " + ((st.elapsed_seconds || 0) % 60) + " s so far."
    case "sending": return "Sending…"
    case "waiting-for-block": return "Waiting for the block…"
    case "ready": return "Ready: the pass is valid until " + (st.proof && st.proof.expires_at ? new Date(st.proof.expires_at).toISOString().replace("T", " ").substr(0, 16) + " UTC" : "its end") + "."
    case "failed": return "Didn't go through: " + (st.plain_message || "") + " (" + (st.code || "") + ")"
    case "cancelled": return "Cancelled. Nothing was sent."
    default: return ""
  }
}

export function proofKind(status) {
  const state = (status || {}).state
  return state === "ready" ? "done" : state === "failed" ? "problem" : state === "cancelled" ? "" : "waiting"
}

/// The sheet's "until [time]" line, its time written out.
export function proofPublicLine(line) {
  const m = /^When the pass starts and ends: until (\d+)\.$/.exec(String(line))
  return m ? "When the pass starts and ends: until " + new Date(Number(m[1])).toISOString().replace("T", " ").substr(0, 16) + " UTC." : String(line)
}

/// "Good to know", with the earlier pass it replaces named when one is live.
export function proofGoodToKnow(sheet) {
  const lines = (sheet.good_to_know || []).slice()
  const live = sheet.live_passes || []
  if (live.length > 0) lines[1] = "It replaces your earlier pass for this NFT, made for " + live[0].app + ", if it was made from the same account."
  return lines
}

export function proofChecksLine(checks) {
  const c = checks || {}
  const word = (label) => (!label || !label.registry ? "unknown (no Program Registry)" : label.verified ? "verified" : "not verified")
  return "Program checks: gate " + (c.gate ? word(c.gate_label) : "none set up on this network yet") + ", NFT program " + word(c.nft_program_label) + "."
}

/// What an app can no longer do once a capability is taken back.
export function capabilityVerb(capability) {
  return capability === "nft_list" ? "see your NFTs" : capability === "nft_transfers" ? "propose NFT transfers" : capability === "membership_proof" ? "ask you for membership proofs" : "use that"
}

/// Running jobs after the app opens again: an NFT job goes to Collectibles, never to the board
/// (review finding 4). `{nft, rest}`.
export function splitRunningJobs(all) {
  const labels = { nft_send: "Your NFT send", nft_burn: "Your NFT burn", nft_print: "Your NFT print", nft_receive: "Receiving your NFTs", nft_init: "Getting the account ready" }
  const jobs = all || []
  const nft = jobs.find((job) => /^nft_/.test(job.kind || ""))
  return {
    nft: nft ? { id: nft.id, kind: nft.kind, label: labels[nft.kind] || "Your NFT action", started: nft.started || Date.now(), proves: undefined, asking: false } : null,
    rest: jobs.filter((job) => !/^nft_/.test(job.kind || "")),
  }
}

/// The line a public sheet adds when another account pays: the payer is named on chain, so
/// anyone can see the two belong together (review finding 5).
export function payerLinkLine(sheet, ticks, from) {
  if (!sheet || sheet.proven || !ticks || !ticks.payer || ticks.payer === from) return ""
  return "The account that pays the fee is named too, so anyone can see the two accounts belong together."
}

/// Every account of the wallet, once, from all listed programs' holdings (an app's NFT list
/// offers these): `[{account, account_kind}]`.
export function listAccounts(programs) {
  const seen = {}
  const out = []
  for (const program of programs || [])
    for (const row of program.holdings || [])
      if (row.account && !seen[row.account]) {
        seen[row.account] = true
        out.push({ account: row.account, account_kind: row.account_kind })
      }
  return out
}

/// The accounts ticked for one request, among those shown: never another request's ticks.
export function tickedFor(ticks, requestId, shown) {
  const mine = (ticks || {})[requestId] || {}
  const allowed = (shown || []).map((row) => row.account)
  return Object.keys(mine).filter((account) => mine[account] && allowed.indexOf(account) >= 0)
}

/// Tick state with only the requests still waiting.
export function pruneTicks(ticks, requests) {
  const ids = (requests || []).map((r) => r.id)
  const out = {}
  for (const id of Object.keys(ticks || {})) if (ids.indexOf(id) >= 0) out[id] = ticks[id]
  return out
}

/// A capability row under Connected apps: what it lets the app learn or do.
export function capabilityTitle(grant) {
  const g = grant || {}
  if (g.capability === "nft_list") return "NFT list: " + (g.accounts || []).map(accountShown).join(", ")
  if (g.capability === "nft_transfers") return "NFT transfers: it may ask; you approve each one"
  if (g.capability === "membership_proof") return "Membership proofs: it learns a pass id, nothing about your accounts"
  return String(g.capability || "")
}

// ── The item page: one NFT, tapped in Collectibles ──────────────────────

/// A name past its grace period counts as not registered (the names tool's `lookup`,
/// registry.rs GRACE_MS: 30 days).
export const GRACE_MS = 30 * 24 * 60 * 60 * 1000

/// One NFT as the item page shows it, read again from the last holdings read on every refresh:
/// `item` is `{program, account, definition}` (an unreadable one has definition ""). null when
/// the account no longer holds it (sent, burned) or the program is no longer listed.
export function itemOf(programs, item) {
  if (!item) return null
  const entry = (programs || []).find((p) => p.program === item.program)
  if (!entry) return null
  const row = (entry.holdings || []).find((r) => r.account === item.account && (r.definition || "") === (item.definition || ""))
  if (!row || (!row.holds && row.state !== "unreadable")) return null
  return {
    program: entry.program, programLabel: entry.label || idShown(entry.program), registry: entry.registry || null,
    account: row.account, isPrivate: row.account_kind === "private", definition: row.definition || "",
    state: row.state, print_balance: row.print_balance, collection: row.collection || null, unreadable: row.unreadable,
  }
}

/// The LNS name whose card (deed) this is, or "": `deeds` is the app's nameDeeds (name ->
/// {deed: {definition, nft_program}}).
export function deedNameOf(deeds, program, definition) {
  if (!definition) return ""
  for (const name of Object.keys(deeds || {})) {
    const d = deeds[name] && deeds[name].deed
    if (d && d.definition === definition && (!d.nft_program || d.nft_program === program)) return name
  }
  return ""
}

/// Whether a holding is an LNS name card: one of the names' cards (deedNameOf). A collection
/// merely called "LNS name deed" is not one: anyone can name a collection so, and a real deed sits
/// in a registry account, never in a wallet account's holdings (deep review B11). A card moves only
/// with its name: the NFT program refuses its holder's own Transfer or Burn. `listed` and
/// `collection` are kept for the callers.
export function isDeed(deeds, listed, program, definition, collection) {
  return deedNameOf(deeds, program, definition) !== ""
}

/// The collection's description, if its record carries one; "" otherwise (the reference
/// program stores none today, so the page leaves the line out rather than invent one).
export function itemDescription(collection) {
  const text = collection && typeof collection.description === "string" ? collection.description.trim() : ""
  return text
}

const KNOWN_FIELDS = ["definition", "name", "printable_supply", "standard", "uri", "creators", "description", "readable"]

/// Every field of the collection, as rows: the four the program stores (as the card shows them),
/// then any other the read carries, by its own name.
export function itemFields(collection) {
  const rows = metadataRows(collection)
  if (!collection) return rows
  for (const key of Object.keys(collection)) {
    if (KNOWN_FIELDS.indexOf(key) >= 0) continue
    const value = collection[key]
    if (value !== null && typeof value === "object") continue
    const label = key.replace(/_/g, " ")
    rows.push({ label: label.charAt(0).toUpperCase() + label.substr(1), value: value === null || value === undefined || value === "" ? "none given" : String(value) })
  }
  return rows
}

/// The edition: master or copy, print rights, and what the supply allows. No serial exists.
export function editionLines(row) {
  if (!row) return []
  if (row.state === "unreadable") return ["Its state can't be read (" + (row.unreadable || "unknown") + ")."]
  const lines = [tokenLine(row)]
  const supply = Number(row.collection && row.collection.printable_supply)
  if (Number.isSafeInteger(supply) && supply >= 1)
    lines.push(supply === 1 ? "The collection has one NFT: the master, and no copies." : "The collection allows the master and up to " + (supply - 1) + (supply === 2 ? " copy." : " copies."))
  lines.push("Copies of one collection are identical: there is no serial number.")
  return lines
}

/// What the Program Registry says about the program, in the words of the programs list.
export function registryLine(registry) {
  if (!registry || !registry.registry) return "The Program Registry isn't installed: its source is unknown."
  if (registry.stale) return "Changed: it runs other code than was registered."
  if (registry.verified) return "Source verified (Program Registry)"
  if (registry.kind === "unregistered") return "Not in the Program Registry: its source is unknown."
  return "Source not verified (Program Registry)"
}

/// The owner as the page shows it: its LNS name, else the shortened account.
export function ownerShown(account, name) {
  return name ? name : accountShown(account)
}

/// The LNS name typed as a Send destination (bare, lower case), or "" when what is typed is an
/// account, receiving keys, or not usable yet.
export function toName(text) {
  const typed = String(text || "").trim()
  if (typed === "" || toProblem(typed) !== "" || /^(Public|Private)\//.test(typed) || toTheirKeys(typed)) return ""
  return nameBare(typed).toLowerCase().replace(/\.lez$/, "")
}

/// One step of checking a name for Send, from `resolve`'s answer `record` for `asked`:
/// `{follow}` (a forward to read next; the names tool follows one) or the outcome
/// `{state: "ok" | "not_registered" | "no_account", name, account, via}`. `typed` is the name
/// first typed; `via` is set when it forwarded to `asked`.
export function nameStep(typed, asked, record, now) {
  const via = typed !== asked ? typed : ""
  if (!record || (!record.never && record.expires !== null && record.expires !== undefined && (now || Date.now()) >= Number(record.expires) + GRACE_MS))
    return { state: "not_registered", name: asked, account: "", via: via }
  if (!via && typeof record.forward === "string" && record.forward !== "") return { follow: record.forward }
  const account = typeof record.account === "string" ? record.account.replace(/^Public\//, "") : ""
  return account !== "" ? { state: "ok", name: asked, account: account, via: via } : { state: "no_account", name: asked, account: "", via: via }
}

/// The line under the Send field for a name: `{text, kind}` (kind "done", "problem" or
/// "working"), or null when no name is typed. `check` is the last outcome, kept with the name
/// it was for (`check.typed`); an outcome for another name is never shown.
export function namePreview(typedName, check) {
  if (!typedName) return null
  if (!check || check.typed !== typedName || check.state === "checking") return { text: "Checking " + typedName + "…", kind: "working" }
  const via = check.via ? " (" + check.via + " points to " + check.name + ")" : ""
  if (check.state === "ok") return { text: "Sends to " + check.name + " · " + idShown(check.account) + via, kind: "done" }
  if (check.state === "not_registered") return { text: check.via ? check.via + " points to " + check.name + ", which nobody holds." : "Nobody holds " + typedName + ". Check the spelling.", kind: "problem" }
  if (check.state === "no_account") return { text: check.name + " has no public account" + via + ", so it can't take an NFT by name. Ask for an account (Public/…) or receiving keys.", kind: "problem" }
  return { text: "Couldn't check " + typedName + ": " + (check.why || "the wallet didn't answer") + ". Try again in a moment.", kind: "problem" }
}

/// Why Send (or Print) on the item page can't be pressed yet, or "".
export function itemSendWhy(text, check, theirKeys) {
  const typed = String(text || "").trim()
  if (typed === "") return "Enter where it goes first."
  const problem = toProblem(typed)
  if (problem !== "") return problem
  if (toTheirKeys(typed) && !theirKeys) return "Tick the box above to send to receiving keys."
  const name = toName(typed)
  if (name === "") return ""
  const preview = namePreview(name, check)
  return preview.kind === "done" ? "" : preview.kind === "working" ? "Checking the name…" : "This name can't take it: see above."
}
