# LNS — LEZ Name Service: .lez names, private by default

*Posting as a suggested RFP. Feedback welcome; I'd be glad to apply if it becomes one.*

## What it is

Human-readable names for the Logos Execution Zone: pay `alice.lez` instead of a 32-byte account ID.

- **Private by default.** A name carries private receiving keys. Paying `alice.lez` sends a private transfer: the chain shows neither recipient nor amount, and the payer never learns an account, balance or history. Paying from a private account hides the payer too.
- **Public by choice.** A holder who wants an openly payable name (a shop, a project, donations) can publish a public address on it instead, or as well.
- **The chain never sees names.** A name becomes an identifier and a key on your machine; the chain stores only the identifier and an encrypted record. Nobody can list or browse names, unless a holder opts in.
- **Optional public listing.** A holder can list a name in an on-chain directory so people can find and browse it, even if payments to it stay private.
- **Hidden holders.** The chain keeps a per-name tag of the holder, so two names of the same person can't be linked, and a holder using a private account never appears on chain.
- **Records:** text entries (`chat`, `inbox`, `agent`) and a page per name.
- **Logos Chat usernames backed by LNS** (see [ideas #43](https://github.com/logos-co/ideas/issues/43)): a chat username can be an LNS name, which gives it uniqueness from the LEZ registry. A name's `chat` entry points at the holder's chat address, so typing `alice.lez` in a chat app opens a conversation with the right person, and the app can show the name as verified when the record points back at the sender. Opt-in per name.
- **No server in between.** The client talks to the LEZ sequencer directly and reads the registry's program from the chain to prove private changes.

A working prototype is live on the LEZ testnet (registry `2PXg3SSmh5hvECTz4xcfV2d5YvxwyF6yVhpnjTokzCkX`):

```
curl -fsSL https://logos-names.wrapit.workers.dev/install.sh | sh
names resolve jef.lez
```

Code (MIT/Apache-2.0): https://github.com/jefdiesel/logos-names. The spec is in [SPEC.md](https://github.com/jefdiesel/logos-names/blob/main/SPEC.md).

## Why

People can't use 32-byte account IDs, so every chain grows a name service. The existing ones (ENS, SNS, Handshake) only map names to public addresses, so a name becomes a permanent public label on an account, and anyone can list every name and owner. On a privacy chain that should be a choice, not the only option. LEZ's private accounts make private names possible, and LNS keeps public names for the cases that want them.

## What exists

- **ENS / SNS / Handshake:** public directories. Names, owners and addresses are all public.
- **ENS + stealth addresses (Umbra, EIP-5564):** payments to a name go to one-time addresses, but the name and its owner are still public.
- **This prototype:** register, renew, transfer, release, private payments to a name (tested end to end from a fresh install), private holders, `names bridge` from the Logos Blockchain, and an MCP server for AI agents. Built in about two days.

## Scope (suggested RFP)

Full spec, in Logos RFP format (tier S, 2–3 weeks): https://github.com/jefdiesel/logos-names/blob/main/docs/RFP-private-naming.md

In short:
1. Names never reach the chain (slow client-side derivation; identifier plus encrypted record).
2. Private by default: receiving keys made at registration; payments to them are private.
3. Public by choice: a holder can publish a public address on a name, and can list a name in an opt-in directory (payments can stay private).
4. Hidden holders behind per-name tags.
5. Register / renew / transfer / release with terms, expiry, grace period and fees to a treasury.
6. Logos Chat usernames backed by LNS names.
7. Clients talk to a LEZ sequencer directly; no gateway, faucet or indexer required.
8. CLI, SDK (Rust and JS), MCP server, and a spec that other wallets can implement.

**Future ideas:** a website at every name (`alice.<gateway domain>`), and subnames such as `pay.alice.lez`.

## Risks and deployment

- **Proof time:** a private transfer takes about 3.5 minutes to prove on an M4 Mac, all on CPU (RISC Zero 3.0.5 has its Apple GPU backend disabled). About 2 min is the circuit, about 47 s a Keccak proof and about 47 s recursion. Faster proving in LEZ would help every private app.
- **Unlisted names can be checked one at a time:** anyone who types an exact name sees that it exists, when it was last changed and what its holder published. A private name shows no address, balance or payment history. A name with a public address shows that address, and so its balance and history, to anyone who looks it up; that's the holder's choice.
- **Funding:** bridging from the Logos Blockchain takes about 12 hours on today's testnet (finality).
- **Testnet resets** wipe names; the registry gets redeployed with a migration path.
