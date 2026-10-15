# LNS — LEZ Name Service: private .lez names, payable without seeing the address, with a website at each name

*Posting in Proposals as a suggested RFP. Feedback welcome; I'd be glad to apply if it becomes one.*

## What it is

Human-readable names for the Logos Execution Zone (`alice.lez`), built for privacy rather than as a public directory:

- **Pay a name without learning its address.** A name carries only private receiving keys. Paying `alice.lez` sends a private transfer: the chain shows neither recipient nor amount, and the payer never learns an account, balance or history. Paying from a private account hides the payer too.
- **The chain never sees names.** A name becomes an identifier and a sealing key on the user's machine; the chain stores only the identifier and an encrypted record. Nobody can list or browse names.
- **Hidden holders.** Names are held by private accounts only, which never appear on chain. The chain keeps a per-name tag of the holder, so two names of the same person can't be linked. (A public holder could be found by testing public accounts against the tag, so the client refuses them.)
- **Records:** text entries (`chat`, `inbox`, `agent`) and a page per name.
- **A website at every name:** `alice.lez` serves its page at `alice.<gateway domain>`, so a name works in a browser too. Anyone can run a gateway on their own domain.
- **Logos Chat usernames backed by LNS** (see [ideas #43](https://github.com/logos-co/ideas/issues/43)): a chat username can be an LNS name. LNS already gives what #43 needs for usernames: uniqueness and ordering, enforced by the LEZ registry rather than a separate committee. A name's `chat` entry points at the holder's chat address, so typing `alice.lez` in a chat client opens a conversation with the right person, and a client can show a name as verified when its record points back at the sender's chat address. The holder opts in per name, and other names they hold stay unlinked.

A working prototype is live on the LEZ testnet (registry `66nUvzJtTxA729aUpTcAWDrb7cV9Heb2bDzceHQhLYad`):

```
curl -fsSL https://logos-names.wrapit.workers.dev/install.sh | sh
names resolve jef.lez
```

Code (MIT/Apache-2.0): https://github.com/jefdiesel/logos-names. The spec is in [SPEC.md](https://github.com/jefdiesel/logos-names/blob/main/SPEC.md).

## Why

People can't use 32-byte account IDs, so every chain grows a name service. The existing ones (ENS, SNS, Handshake) are public directories: a name is a permanent public label on an account, so looking it up exposes that account's balance and history, and anyone can list every name and owner. On a privacy chain that undoes the privacy.

LEZ's private accounts make the alternative possible: a name that publishes only private receiving keys, so every payment to it lands as a private note. LNS uses LEZ (registry program, private accounts, privacy-preserving transactions), the Logos Blockchain (`names bridge` deposits from a user's own node), and gives Logos Messaging and Basecamp apps a shared way to find a person or service by name.

## What exists

- **ENS / SNS / Handshake:** public directories. Names, owners and resolved addresses are all public.
- **ENS + stealth addresses (Umbra, EIP-5564):** payments to a name go to one-time addresses, so they can't be linked together, but the name and its owner are still public.
- **This prototype** shows the private version works on LEZ v0.3.1: register, renew, transfer, release, private payments to a name (tested end to end from a fresh install), private holders, the bytecode read from the chain, and no server in the trust path.

Hypothesis: private names that are still payable are a basic primitive that payments, chat and agents on Logos will all want, and a privacy chain should have a name service that doesn't leak.

## Scope (suggested RFP)

Full spec, in Logos RFP format (tier M, 6–10 weeks): https://github.com/jefdiesel/logos-names/blob/main/docs/RFP-private-naming.md

In short:
1. Names never reach the chain (slow client-side derivation; identifier plus sealed record).
2. A name carries only private receiving keys, made at registration; paying is always private.
3. Names are held by private accounts only, behind per-name tags; holders never appear on chain.
4. Register / renew / transfer / release with terms, expiry, grace period and fees to a treasury.
5. A website at every name: `alice.<gateway domain>` serves the name's page; the gateway's JSON never shows an address or keys.
6. Chat integration: a `chat` entry for opening conversations by name; names usable as Logos Chat usernames, with uniqueness from the registry.
7. Clients talk to a LEZ sequencer directly; no gateway, faucet or indexer is required.
8. CLI, SDK (Rust and JS), MCP server for agents, and a spec that independent wallets can implement.

## Risks and deployment

- **Proof time:** a private transfer takes about 3.5 minutes to prove on an M4 Mac, all on CPU (RISC Zero 3.0.5 has its Apple GPU backend disabled). About 2 min is the circuit, about 47 s a Keccak proof and about 47 s recursion. Faster proving in LEZ would help every private app.
- **Names can be checked one at a time:** there's no list, but anyone who types an exact name (say `bob.lez`) sees that it exists, when it was registered and last changed, when it expires, and what its holder chose to publish. Never an address, balance, payment history or holder: payments go to fresh private accounts. Each check costs a little computing work, so trying millions of names is slow. The LEZ sequencer you read from sees which records you read (not the names).
- **Funding:** bridging from the Logos Blockchain takes about 12 hours on today's testnet (finality). A faucet helps onboarding but shouldn't be required.
- **Testnet resets** wipe names; the deployment republishes the registry and documents a migration path.
- **Deployment:** already live on the LEZ testnet. Next: a wildcard domain for the gateway, chat integration with the Logos Chat team, an audit before any mainnet use.
