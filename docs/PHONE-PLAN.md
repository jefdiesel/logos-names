# Names on a phone: the plan

Decided 2026-10-18. Change it only when a test shows a step can't work, and say
which step and why.

## The design

1. **Keys stay on your computer.** The phone holds no chain keys.
2. **The phone is a remote for your Basecamp.** It reads the chain directly (names,
   profiles, posts, balances) and sends requests (post, pay, register, message) to
   your computer, which does them with your keys, proofs included. Small things go
   through on their own; payments ask, or follow a limit you set.
3. **Phone and computer talk over Logos messaging (Waku).** Names has its own
   end-to-end encrypted messages, keyed by a messaging key published on the name,
   so they don't depend on Basecamp's chat (whose address changes on every launch:
   logos-chat-module sets `LIBCHAT_PERSISTENCE_ENABLED = false`). No official Logos
   node accepts browser connections, so we run one node on cluster 2 (logos.test)
   with secure WebSockets and Store, as the phones' door to the network. The Names
   module sends and receives through Basecamp's `delivery_module`; the phone uses
   js-waku as a light client of our node.
4. **The app** is static files on its own subdomain, installed on the phone, which
   then only accepts updates signed with the release key. A passcode locks it.

The cost: your computer has to be on, with Basecamp open, for the phone to act.
Reading works without it.

## Order

1. **Checks (about a day).** Can the Mini's Basecamp send on logos.test (RLN
   membership)? Does a phone page round-trip messages with `delivery_module` on the
   Mini through our own WSS node (lightpush, filter, Store)?
2. **Names messaging (about 2 weeks).** Our node; the module's send, subscribe and
   messaging key; the phone's side.
3. **Pairing and requests.** QR pairing from Basecamp; the request types; approvals
   and limits on the computer.
4. **The signed app** on its subdomain.

## Not this plan

- A wallet with chain keys in the phone (parked on branch `phone-wallet-wip`).
- Delegated device keys on names.
- Porting Basecamp's chat (libchat: OpenMLS, de-MLS) to the browser.

## Risks

- js-waku (logos-delivery-js) has had no release since 2026-02; an issue asking if
  it is maintained is unanswered.
- Sending on logos.test may need an RLN membership; not yet confirmed on the Mini.
- A per-name content topic shows who is being messaged, though not what.
