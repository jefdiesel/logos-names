---
id: RFP-0XX
title: LNS — LEZ Name Service (.lez names, private by default)
tier: S
status: suggested
category: Developer Tooling & Infrastructure
dependencies: []
---

# RFP-0XX — LNS: LEZ Name Service (.lez names, private by default)

> **Suggested RFP.** Written by the author of [Logos Names](https://github.com/jefdiesel/logos-names),
> a working prototype on the LEZ testnet, as a proposal for the Logos RFP
> program. The standard Logos RFP notices (terms, regulatory responsibility)
> apply as in [RFP-000](https://github.com/logos-co/rfp/blob/master/RFPs/RFP-000-template.md).

## 🧭 Overview

LEZ needs human-readable names, so people can pay `alice.lez` instead of a
32-byte account ID, and so apps, chat and agents can find each other by name.
Every existing name system (ENS, SNS, Handshake) is a public directory: a
name points at a public address, so resolving a name exposes that address's
balance and history, and anyone can list every name and who owns it. This RFP
asks for a **naming standard and program for LEZ that is private by default
and public by choice**: by default a name can be paid without anyone, the
payer included, learning the account behind it; a holder who wants a public
address (a shop, a project, donations) can publish one. Either way the chain
never sees names, nobody can list or browse them, and who holds a name is
hidden. A team with LEZ program, zero-knowledge and wallet experience is a
good fit.

## 🔥 Why This Matters

Readable names are one of the main things that make a chain usable: people
remember `alice.lez`, not account IDs. But a name service that only maps names
to public addresses undoes a privacy chain's privacy, because every name
becomes a permanent public label on an account. LEZ's private accounts make a
better default possible: a name carries private receiving keys, so every
payment to it lands as a private note that observers can't attribute. Some
names should be public (a shop, a DAO, a donations page), so the holder can
also publish a public address. Names also give Logos Messaging, Storage and
apps a shared way to find a person or service (`chat`, `inbox`, `agent`
entries).

## ✅ Scope of Work

### Hard Requirements

#### Functionality

1. **Names never reach the chain.** A name is turned into an identifier and a
   sealing key on the client (a slow derivation, so guessing many names is
   costly). The chain stores only the identifier and a record sealed under the
   key. An observer who doesn't know a name cannot learn it, list names, or
   link two records, unless the holder lists it (requirement 4).
2. **Private by default.** A name carries private receiving keys (LEZ
   nullifier and viewing public keys). Paying it sends a private transfer to
   those keys: the chain shows neither the recipient nor the amount, and the
   payer learns no account, balance or history of the recipient. Paying from
   a private account also hides the payer. Registering a name gives it keys of
   a fresh private account, so it is payable at once; the holder can rotate
   them.
3. **Public by choice.** A holder can also publish a public account on a name,
   for names that should be openly payable (a shop, a project, donations).
   Wallets then let the payer choose public or private, and say clearly that a
   public address shows its balance and history to anyone who resolves the
   name.
4. **Optional public listing.** A holder can list a name in an on-chain
   directory, in the clear, so people and apps can find and browse it. Listing
   is per name and can be undone. It reveals the name, not the holder, and a
   listed name can still be paid only privately.
5. **Hidden holders.** The chain stores only a tag of the holder, unique per
   name, so two names held by the same account cannot be linked. A holder
   acting from a private account never appears on chain.
6. **Register, renew, transfer, release** with a term and an expiry, and a
   grace period after expiry. A released or lapsed name leaves nothing behind
   on chain.
7. **Registration fees** to a configurable treasury, with one price per term.
   The chain can't see names, so it can't price them by length.
8. **Records:** receiving keys, free-form text entries (such as `chat`,
   `inbox`, `agent`), and a page of at least 64 KiB, all sealed under the
   name's key.
9. **Logos Chat usernames.** A name's `chat` entry points at its holder's chat
    address, so a chat client can open a conversation from `alice.lez`, and can
    show a name as verified for a sender whose chat address the name's record
    points back to. Chat usernames can be LNS names, which gives them
    uniqueness and ordering from the registry. The holder opts in per name, and
    their other names stay unlinked.
10. **No server in the trust path.** Clients talk to a LEZ sequencer directly,
    read the program's bytecode from the chain when they need it to prove, and
    work without any gateway, faucet or indexer run by the team.

#### Usability

1. Provide an SDK (Rust, and JavaScript for resolvers) that wallets, apps and
   Logos modules can use to resolve and pay names.
2. Provide a CLI covering all operations, installable with one command on
   macOS (Apple silicon) and Linux (x86_64), including RISC Zero's prover for
   private actions.
3. Provide an IDL for the LEZ program, preferably with SPEL once it supports
   LEZ v0.3.
4. Provide an MCP server so AI agents can resolve names, read-only.
5. Paying a name is one command or one button: the user never handles keys or
   account IDs.

#### Reliability

1. Clients run the program's logic locally before sending, so a refused
   transaction fails with the program's reason and costs no fee.
2. Clients confirm each change by reading state back, since LEZ includes failed
   transactions in blocks.
3. A private holder's client records each new private note after a change, so
   consecutive private changes don't collide.

#### Performance

1. Document the cycle count and proving time of every operation on consumer
   hardware (an Apple M-series Mac and a Linux machine with and without an
   NVIDIA GPU).
2. Resolving a name takes at most two account reads.
3. Report which parts of proving time come from LEZ's privacy circuit versus
   the naming program, so the LEZ team can see where to optimise.

#### Supportability

1. Deployed and tested on the LEZ testnet, with program addresses published.
2. End-to-end tests against a standalone LEZ sequencer in CI, including the
   private paths (proof generation).
3. A specification precise enough for an independent wallet to implement
   resolution and payment without this code.
4. A privacy properties document: what observers see for each operation, what
   private paths protect, trust assumptions, and what leaks if a client misbehaves
   (for example, anyone who types an exact name can see that it exists, when
   it was last changed and what its holder published; a private name shows no
   address, balance or payment history, while a name with a public account
   shows that account and so its balance and history).
5. A design decision log.
6. CI green on the default branch; MIT + Apache-2.0 licence.

### Soft Requirements

1. A Logos mini-app (Basecamp) for looking up, paying and managing names.
2. Private funding of a holder or payer account in one step (faucet or bridge
   deposit straight into a private account).
3. Payments that don't block the user: proving in the background with a
   notification when it lands.
4. Reverse resolution that an account holder opts into without linking their
   other names.
5. Integration with Logos Messaging (`chat` entry opens a conversation) and
   Logos Storage (pages larger than one account).
6. A migration path when LEZ or its account model changes between testnets.

### Future ideas (not in scope)

- **A website at every name:** a gateway serving `alice.lez`'s page at
  `alice.<gateway domain>`, through a wildcard subdomain.
- **Subnames** such as `pay.alice.lez`: names under a name, created by its
  holder, that observers can't link to the parent.

## 👤 Recommended Team Profile

- LEZ program development (plan/apply execution, shards, program loader)
- Zero-knowledge proving with RISC Zero; LEZ private accounts and notes
- Wallet and key-management UX
- Applied cryptography (key derivation, authenticated encryption)

## ⏱ Timeline Expectations

Estimated duration: **2–3 weeks**. The prototype linked below, which already
meets most hard requirements, was built in about two days. What remains is
the opt-in public listing, Logos Chat integration, CI tests for the private
paths, the privacy properties document and the design log.

## 🌍 Open Source Requirement

All code must be released under the **MIT + Apache-2.0 License**.

## Resources

- Prototype: [jefdiesel/logos-names](https://github.com/jefdiesel/logos-names),
  running on the LEZ testnet as registry `66nUvzJtTxA729aUpTcAWDrb7cV9Heb2bDzceHQhLYad`.
  It already does requirements 1–3, 5–8 and 10, and the `chat` entry of 9 (not
  the public listing), with a
  [specification](https://github.com/jefdiesel/logos-names/blob/main/SPEC.md),
  a CLI, a resolver library, a gateway and an MCP server. Guide:
  https://logos-names.wrapit.workers.dev/
- Measured on an M4 Mac, LEZ v0.3.1: a private transfer takes about 3.5 minutes
  to prove, all on CPU, because RISC Zero 3.0.5 has its Metal (Apple GPU) backend
  disabled. About 2 minutes are the circuit's 1.3M cycles, about 47 s a separate
  Keccak proof, about 47 s recursion.
- [LEZ](https://github.com/logos-blockchain/logos-execution-zone),
  [SPEL](https://github.com/logos-co/spel).
