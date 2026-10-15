# Suggested RFP: Private naming for LEZ (.lez names and subnames)

LEZ has no name service yet, and the existing designs (ENS, SNS, Handshake) are
public directories. A name points at a public address, so looking a name up
exposes that address's balance and history, and anyone can list every name and
its owner. On a privacy chain that turns every name into a public label on an
account.

I've built a working prototype that does it the other way round, and I'd like
to suggest it as an RFP.

**What it does (live on the LEZ testnet today):**

- **Pay a name without learning its address.** A name carries only private
  receiving keys. `names pay alice.lez 1000` sends a private transfer: the chain
  shows neither recipient nor amount, and the payer never learns an account.
  Paying from a private account hides the payer too.
- **The chain never sees names.** A name becomes an identifier and a sealing
  key on your machine; the chain stores only the identifier and an encrypted
  record. Nobody can list or browse names.
- **Hidden holders.** The chain keeps a per-name tag of the holder, so two names
  of the same person can't be linked, and a private holder never appears on chain.
- **No server in between.** The CLI talks to the LEZ sequencer directly and
  reads the registry's program from the chain to prove private changes.
- Terms, renewals, fees to a treasury, text entries (`chat`, `inbox`, `agent`),
  a page per name served by gateways, `names bridge` from the Logos Blockchain,
  and an MCP server for AI agents.

Try it:

```
curl -fsSL https://logos-names.wrapit.workers.dev/install.sh | sh
names resolve jef.lez
```

Code (MIT/Apache): https://github.com/jefdiesel/logos-names. The spec is in `SPEC.md`.

**What the RFP adds: subnames.** `pay.alice.lez`, `bob.team.lez`, to any
depth, each with its own keys, record and holder. The parent can assign them,
and either keep the right to revoke or give it up for good. They're served as
subdomains by gateways (`pay.alice.<gateway>`). The hard part, and the main
requirement, is that an observer can't link a subname to its parent. The draft
suggests a way: a subname authority secret sealed in the parent's record.

Full draft in Logos RFP format (tier M, 6–10 weeks):
https://github.com/jefdiesel/logos-names/blob/main/docs/RFP-private-naming.md

**Measured, for the LEZ team:** a private transfer on an M4 Mac takes about
3.5 minutes to prove, all on CPU. RISC Zero 3.0.5 has its Metal (Apple GPU)
backend disabled. About 2 min is the circuit's 1.3M cycles, about 47 s a separate
Keccak proof, and about 47 s recursion. Moving to risc0 5.x, where Metal
recursion is back, or replacing Keccak in the circuit would cut most of it.

Feedback welcome. I'd be glad to apply if this becomes an RFP.
