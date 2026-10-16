# Roadmap

Ideas the maintainer has agreed belong here. Nothing below is built unless it says so.

## Registry v5

See [v5-plan.md](v5-plan.md): Argon2id with a salt shared by testnet and mainnet,
refundable deposits (fee mode kept behind a setting), locked 1–2 character names,
higher deposits for 3 and 4 characters, and testnet-to-mainnet claims.

## Private notes with a payment

`names pay jef.lez 1000 --note "thanks"`: a message only the recipient can read,
sent in the same private transaction as the payment, or on its own.

- LEZ transfers have no memo field today. A private payment creates a new private
  account for the recipient, encrypted to their keys; a small "notes" program would
  write the message into that account's data in the same transaction.
- The chain sees neither the message, the sender, the recipient nor the amount.
- Reading: the recipient's wallet shows notes when it syncs (for example in
  `names balance`, or a `names notes` command).
- **Size** (LEZ source, 2026-10-16): a transaction may be about 1 MiB; a private
  one carries a ~224 KB proof, leaving roughly 700 KB. There is no separate cap on
  a note. Private data is padded only to 512 bytes, so a longer note reveals its
  approximate length: pad notes to fixed sizes (1, 4 or 16 KiB).
- **Next LEZ already has most of this:** after v0.3.1, LEZ moved to an actor model
  with sealed async messages. Any program can send bytes sealed to a recipient's
  viewing key, and the recipient claims them from a pending mailbox in their own
  private transaction. A notes feature is mostly built on that.
- **As an NFT:** LEZ has no token or NFT program (v0.3.1 removed them). A note held
  in an account its holder can transfer is one; that takes a small program.
- **Forwarding:** the holder spends the note (its nullifier stops reuse) and sends
  it on to someone else's keys, privately. The program has to allow it.
- Also suggest a memo on transfers to LEZ upstream, so every wallet gets it.

## Next LEZ release (actor model)

LEZ `dev` (after v0.3.1) replaced program shards with actor states and messages, so
the v0.3.1 registry program will not run on the next testnet. Registry v5 should
target it. A `dev` wallet also cannot read today's testnet (`unknown field
shards`), so the LEZ wallet PR (`--to-name`) can be tried live only once the
testnet runs the new release.

## Later

- Websites per name on Logos Storage (a `cid` text entry; vpavlin's viewer).
- Subnames.
- An opt-in public list of names, for holders who want to be found.
- A browser resolver, once sequencers allow cross-origin requests (SPEC 12.3).
- Make the Linux binary need no PC/SC library: Keycard support behind a feature
  (Logos Kit's patch `0005-keycard-feature` does this), or suggest it upstream.
