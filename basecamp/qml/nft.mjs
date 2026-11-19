// Collectibles (NFTs of the reference NFT program, docs/wallet/NFT.md): the rules the view
// follows, kept here so `node basecamp/nft.test.mjs` can check them without a QML engine.
// Words: "LNS names", never "official", never "lookup".

const ID = /^[1-9A-HJ-NP-Za-km-z]{32,44}$/

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
      group.nfts.push({ program: program.program, programLabel: program.label || idShown(program.program), definition: row.definition, state: row.state, print_balance: row.print_balance, collection: row.collection, unreadable: row.unreadable })
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
  if (typed.indexOf(":") >= 0) {
    const [npk, vpk] = typed.split(":")
    return /^[0-9a-fA-F]{64}$/.test(npk) && /^([0-9a-fA-F]{2})+$/.test(vpk || "") ? "" : "Receiving keys are written <npk hex>:<vpk hex>."
  }
  if (ID.test(typed)) return "Write an account as Public/ and then its ID."
  return /^[a-z0-9][a-z0-9-]{0,62}(\.lez)?$/i.test(typed) ? "" : "That isn't an LNS name, an account or receiving keys."
}

/// Whether the destination is someone's receiving keys (can't be checked: an extra tick).
export function toTheirKeys(text) {
  return String(text || "").indexOf(":") > 0
}

/// Why the sheet's primary button can't be pressed yet, or "".
export function approveWhy(sheet, ticks) {
  if (!sheet) return "Checking what this does…"
  if (sheet.refusal) return ""
  const t = ticks || {}
  if (sheet.proven && !t.payer) return "Choose the private account that pays the fee."
  if (sheet.proven && !(sheet.payers || []).some((p) => p.account === t.payer && p.enough)) return "That account can't cover the fee reserve; choose another."
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
  if (!job.proves) return job.label + ": sending it publicly, one block (about 30 seconds), " + seconds + "s so far."
  return job.label + ": proving it on this computer, so the chain never learns the private side. This takes 7-12 minutes on a fast computer; " + Math.floor(seconds / 60) + " min so far. You can keep using Names meanwhile."
}

/// The line an ended job leaves: done with its block, or the plain reason.
export function jobEnded(job, state) {
  if (state.state === "done") {
    if (job.kind === "nft_receive") return { kind: "done", text: state.nothing ? "There was nothing to receive." : "Received " + (state.received === 1 ? "1 NFT" : state.received + " NFTs") + "." }
    return { kind: "done", text: job.label + ": done" + (state.block !== null && state.block !== undefined ? ", in block " + state.block : "") + "." }
  }
  const why = state.refusal || state.problem || "It didn't go through."
  return { kind: "problem", text: job.label + " didn't go through: " + why }
}

/// Whether only private accounts of this wallet hold `definition` under `program` (loading its
/// image could then link them to the person's address).
export function privateOnly(programs, program, definition) {
  const entry = (programs || []).find((p) => p.program === program)
  const holders = (entry ? entry.holdings || [] : []).filter((r) => r.holds && r.definition === definition)
  return holders.length > 0 && holders.every((r) => r.account_kind === "private")
}

/// The words of the image consent for a collection, before anything is fetched.
export function imageConsent(uri, privateOnly) {
  let host = ""
  const m = /^https:\/\/([^/?#@]+)/.exec(String(uri || ""))
  if (m) host = m[1]
  if (host === "") return { allowed: false, text: "This collection's URI isn't an https address, so the wallet doesn't load it." }
  return {
    allowed: true,
    text: "Loading the image asks " + host + " for it. That host learns your computer's address and that someone looks at this collection. It is shown only as a picture (PNG, at most 1 MiB), never as a page." + (privateOnly ? " Only private accounts hold this collection: loading it can link them to your address." : ""),
  }
}

/// Burn's two steps: what the second asks.
export function burnConfirm(row) {
  if (!row) return ""
  return row.state === "master"
    ? "This destroys the master and every print right it holds. The collection then has no master: nobody can print it again. This can't be undone."
    : "This destroys this copy. This can't be undone."
}
