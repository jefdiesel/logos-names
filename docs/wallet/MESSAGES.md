# Messages in the Basecamp wallet

The wallet's **Messages** tab: chats between LNS names that work with the Names phone app (logos-node-android) and
the Names web app. **Logos Chat** is the other tab, Basecamp's Chat (`chat_module`), unchanged.

## How it works

- **Format:** the names message format, version 1 (`gateway/app-src/msg-crypto.js`), with the phone's padding
  (plaintext made up to 512, 2048, 8192 or 15872 bytes) and its safety codes. Ported to Rust in
  `basecamp-module/rust-lib/src/messages/crypto.rs`. Checked byte for byte against the phone's own code:
  `basecamp-module/tools/msg-vectors.mjs` runs the phone's `app/messages` files with fixed randomness and writes
  `rust-lib/testdata/msg-v1-vectors.json`; the tests seal every vector again and compare.
- **Network:** Logos Messaging on **logos.test** (cluster 2). Every `/names/1/msg-<16 hex>/proto` inbox topic is on
  shard 7, the same as the phone. Messages are not on a chain. Name lookups (a name's `msg` entry) are LNS reads on
  the public LEZ testnet.
- **Transport:** Basecamp's own `delivery_module`, called by name through the Logos module SDK
  (`messages/service.rs`). It is one node shared with Chat: Messages uses it when Chat has started it, and starts
  it (`{"mode":"Core","preset":"logos.test"}`) only if nobody has.
- **The messaging key:** this computer's own X25519 key, made by "Turn on Messages", in
  `~/.config/logos-names/messages/msgkey.json` (0600, folder 0700). Its public half goes in the main name's `msg`
  entry (a `set_text` change on LEZ, with its fee).
- **Saved chats:** `messages/store.bin`, sealed with a key derived from the messaging key. The phone's loss-safety
  rules apply. Each write goes to a temporary file, is synced, then renamed; the previous copy is kept as
  `store.bin.prev`. A file that is there but can't be opened is never written over. Nothing is saved until the
  person picks "Use the last saved copy" or "Start fresh", and each of those first sets the bad file aside.
- **Receiving:** a listener thread hears `delivery_module`'s `messageReceived` and drops repeats by message hash.
  The last 4,096 hashes are kept, which covers the node's history replay after a restart or a gap. It opens each
  message with the key and files it:
  - from someone in the chats: into their chat;
  - from anyone else: a request (at most 50 senders and 3 messages each);
  - from a blocked address, a message that doesn't prove its sender, or past those limits: deleted unread, and
    counted.
- **Sending:** sealed (padded), put in the chat and in an outbox, sent by the outbox thread. A send the node refuses
  is tried again after 15 s, 1, 4, 15 and 60 minutes, then shown as "not sent". "Propagated" or "sent" ends it.
- **Requests and names:** a request shows the name it *says* it is from. Accepting checks that name's `msg` entry
  on chain. If it is the sender's address, the chat gets the name; if not, the chat stays an address.

## Known limits

- **Sending from Basecamp doesn't work today; receiving does.** Measured on 2026-11-14 with delivery_module
  **0.3.1**, the version in the Mini's Basecamp:
  - The node accepted a send, then raised `messageError` "Unable to send within retry time window".
  - Its RLN module could not even read the registry it uses (`logos:testnet:841312e9…c893`). The LEZ sequencer
    gave an empty answer for that registry's config account.
  - delivery_module 0.3.3 (2026-10-14) moved to a new RLN registry, and memberships don't carry over.
  - So sending needs **delivery_module ≥ 0.3.3 plus an RLN membership on the new registry**. Registering one is a
    funded LEZ transaction through `liblogos_rln_module.register_membership`. The 0.3.0 release notes require a
    payer holding at least 2×10⁸ native LEZ. That LEZ comes from a Logos Blockchain deposit, which only the
    wallet's owner can make.
  - The screen says so instead of failing silently. After the node refuses a send for RLN, the composer shows:
    "Receiving works. Sending from Basecamp needs a newer messaging module and a membership". The next send that
    goes out clears the message.
- **RLN validation would stop the phone too.** The phone sends without RLN proofs; today's nodes generate proofs
  but don't check them. If logos.test turns validation on, the phone can't send until it has a membership as well.
- **One `msg` key per name: two devices on one name don't work.** A name's `msg` entry holds one address, and its
  inbox is derived from it, so only that device receives. Moving `msg` to this computer means:
  - the phone stops receiving for the name;
  - every contact sees "messaging address changed";
  - contacts who had marked it checked stop sending until they compare codes again.

  Sharing one key between devices, or a signed device list, is future work (MESSAGES-V2 step 5).
- **delivery_module must be loaded.** It is called by name, so Basecamp has to have it loaded, which happens once
  the Chat app has run. Its API is still changing (0.3.x since 2026-09-30); a change can break Messages until the
  module is updated.
- **The screen polls** `msg_status` every 3 s while it shows, and reloads when its `changes` counter moves. The
  module doesn't send an event yet.
- **Not ported from the phone yet:**
  - "Only people I've written to";
  - the lazy re-check of claimed names in Requests;
  - the safety-code QR scan;
  - backups of the chats;
  - the key-change send guard (a contact whose address changed after it was checked).

## Testing it without Basecamp

- **Unit tests:** `cargo test --lib messages` in `basecamp-module/rust-lib`, on the Mini through `mini-lock.sh`.
- **The harness:** a logosctl 0.3.1 session with its own folder, loading Basecamp's `delivery_module` 0.3.1. It
  received a message from the Mini's test partner on logos.test, which the Rust port opened (`authentic: true`):
  - harness: `/Volumes/exos162/logos/dev/wallet-messages`;
  - probes: `messages/probe.rs` (`open_what_the_harness_received`, `seal_a_reply`, both ignored by default).
- **Screens:** `basecamp/tools/render.py` scenarios `nmessages`, `nmessages-off`, `nmessages-chat`,
  `nmessages-cantsend` and `nmessages-requests`.
