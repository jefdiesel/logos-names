---
id: RFP-0XX
title: LNS — LEZ Name Service (private .lez names, with a website at each name)
tier: M
status: suggested
category: Developer Tooling & Infrastructure
dependencies: []
---

# RFP-0XX — LNS: LEZ Name Service (private .lez names, with a website at each name)

> **Suggested RFP.** Written by the author of [Logos Names](https://github.com/jefdiesel/logos-names),
> a working prototype on the LEZ testnet, as a proposal for the Logos RFP
> program. The standard Logos RFP notices (terms, regulatory responsibility)
> apply as in [RFP-000](https://github.com/logos-co/rfp/blob/master/RFPs/RFP-000-template.md).

## 🧭 Overview

LEZ needs human-readable names, so people can pay `alice.lez` instead of a
32-byte account ID, and so apps, chat and agents can find each other by name.
Every existing name system (ENS, SNS, Handshake) is a public directory: a
name points at a public address, so resolving a name exposes that address's
balance and history, and anyone can list every name and who owns it. That is
the opposite of what Logos stands for. This RFP asks for a **private naming
standard and program for LEZ**: a name can be paid without anyone, the payer
included, learning the account behind it; the chain never sees names; nobody
can list or browse them; and who holds a name is hidden. Every name is also
a **website address**: `alice.lez` serves its page at `alice.<gateway domain>`,
so a name works in a browser as well as in a wallet. A team with LEZ program, zero-knowledge and
wallet experience is a good fit.

## 🔥 Why This Matters

Readable names are one of the main things that make a chain usable: people
remember `alice.lez`, not account IDs. But a public name service on a privacy
chain undoes its privacy, because the name becomes a permanent public label on
an account. LEZ's private accounts make a better design possible: a name can
carry only private receiving keys, so every payment to it lands as a private
note that observers can't attribute. Names also give Logos Messaging, Storage
and Basecamp apps a shared way to find a person or service (`chat`, `inbox`,
`agent` entries). And a name that is also a URL gives every person or project
on Logos a website they control from their wallet, with no hosting account,
no DNS registrar and no record of who owns it.

## ✅ Scope of Work

### Hard Requirements

#### Functionality

1. **Names never reach the chain.** A name is turned into an identifier and a
   sealing key on the client (a slow derivation, so guessing many names is
   costly). The chain stores only the identifier and a record sealed under the
   key. An observer who doesn't know a name cannot learn it, list names, or
   link two records.
2. **Pay a name without learning its address.** A name carries private
   receiving keys (LEZ nullifier and viewing public keys), never a public
   account ID. Paying a name sends a private transfer to those keys; the
   chain shows neither the recipient nor the amount, and the payer learns no
   account, balance or history of the recipient. Paying from a private account
   also hides the payer.
3. **Receiving keys from the start.** Registering a name gives it receiving
   keys of a fresh private account, so it is payable immediately. The holder
   can rotate them.
4. **Hidden holders.** Names are held by private accounts only. The chain
   stores only a tag of the holder, unique per name, so two names of one holder
   can't be linked, and the holder never appears on chain. (A public holder
   would be findable: anyone who knows the name could test every public
   account against the tag. Clients must refuse public holders.)
5. **Register, renew, transfer, release** with a term and an expiry, and a
   grace period after expiry. A released or lapsed name leaves nothing behind
   on chain.
6. **Registration fees** to a configurable treasury, with one price per term.
   The chain can't see names, so it can't price them by length.
7. **Records:** receiving keys, free-form text entries (such as `chat`,
   `inbox`, `agent`), and a page of at least 64 KiB, all sealed under the
   name's key.
8. **A website at every name.** A gateway serves a name's page at
   `alice.<gateway domain>` (one subdomain per name, through a wildcard), and
   the name's public record as JSON at a well-known path. The JSON shows only
   when the name expires, whether it can be paid, and the entries the holder
   chose to publish; never an address or keys. Anyone can run a gateway on
   their own domain; a gateway only learns the names it is asked for, and
   serves pages so one name's page can't read another's data.
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
3. Provide a Logos mini-app (Basecamp) for looking up, paying and managing
   names.
4. Provide an IDL for the LEZ program, preferably with SPEL once it supports
   LEZ v0.3.
5. Provide an MCP server so AI agents can resolve names, read-only.
6. Paying a name is one command or one button: the user never handles keys or
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
   private paths (proof generation) and the gateway.
3. A specification precise enough for an independent wallet to implement
   resolution and payment without this code.
4. A privacy properties document: what observers see for each operation, what
   private paths protect, trust assumptions, and what leaks if a client misbehaves
   (for example, anyone who types an exact name can see that it exists, when
   it was registered and last changed, and the entries its holder published,
   but never an address, balance, payment history or holder; and the
   sequencer a client reads from sees which records it reads, though not the
   names).
5. A design decision log.
6. CI green on the default branch; MIT + Apache-2.0 licence.

### Soft Requirements

1. Private funding of a holder or payer account in one step (faucet or bridge
   deposit straight into a private account).
2. Payments that don't block the user: proving in the background with a
   notification when it lands.
3. Reverse resolution that an account holder opts into without linking their
   other names.
4. Integration with Logos Messaging (`chat` entry opens a conversation) and
   Logos Storage (pages larger than one account).
5. A migration path when LEZ or its account model changes between testnets.

## 👤 Recommended Team Profile

- LEZ program development (plan/apply execution, shards, program loader)
- Zero-knowledge proving with RISC Zero; LEZ private accounts and notes
- Wallet and key-management UX
- Applied cryptography (key derivation, authenticated encryption)

## ⏱ Timeline Expectations

Estimated duration: **6–10 weeks**.

## 🌍 Open Source Requirement

All code must be released under the **MIT + Apache-2.0 License**.

## Resources

- Prototype: [jefdiesel/logos-names](https://github.com/jefdiesel/logos-names),
  running on the LEZ testnet as registry `66nUvzJtTxA729aUpTcAWDrb7cV9Heb2bDzceHQhLYad`.
  It already does requirements 1–8 and 10, and the `chat` entry of 9 (not chat-client integration; its gateway runs at `/<name>` paths until it has a domain with a wildcard), with a
  [specification](https://github.com/jefdiesel/logos-names/blob/main/SPEC.md),
  a CLI, a resolver library, a gateway and an MCP server. Guide:
  https://logos-names.wrapit.workers.dev/
- Measured on an M4 Mac, LEZ v0.3.1: a private transfer takes about 3.5 minutes
  to prove, all on CPU, because RISC Zero 3.0.5 has its Metal (Apple GPU) backend
  disabled. About 2 minutes are the circuit's 1.3M cycles, about 47 s a separate
  Keccak proof, about 47 s recursion.
- [LEZ](https://github.com/logos-blockchain/logos-execution-zone),
  [SPEL](https://github.com/logos-co/spel).
