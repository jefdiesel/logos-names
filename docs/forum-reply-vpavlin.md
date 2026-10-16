Thanks for the review, it was useful. Both blockers are fixed in registry v4.1, now live on the LEZ testnet as `2PXg3SSmh5hvECTz4xcfV2d5YvxwyF6yVhpnjTokzCkX` (it replaces `66nUvz…LYad`).

**Front-running on Register (1.1):** registration is now commit-then-reveal. `Commit` publishes only `SHA256("logos-names/v4/commitment" ‖ id ‖ owner_tag ‖ secret)`, so the name, the holder and a random secret stay hidden. `Register` reveals them and consumes the commitment. A commitment must be at least 15 minutes old and at most 24 hours old before it can be revealed. A reveal someone sees in the mempool is useless to them: it is bound to the committer's owner tag, and their own commitment would be too new. Unit tests in `core/src/lib.rs` cover this: `a_seen_reveal_cannot_be_replayed_for_another_account`, `a_reveal_waits_for_its_commitment_to_age`, `a_reveal_without_a_commitment_is_refused`, `an_old_commitment_cannot_be_revealed`, `a_live_commitment_cannot_be_reset`.

**Configure deploy race (1.2):** the first `Configure` must be signed by the program's own header account, which only the deployer holds during deployment. Tests: `nobody_but_the_deployer_configures_a_fresh_registry` and `the_deployer_configures_a_fresh_registry_and_then_only_its_admin`. On a test chain (v4, same code paths; v4.1 only widened the time windows so private proofs fit), a stranger's `Configure` on a fresh deploy was refused.

**Write flows end to end (4.1):** all done today on the official testnet, by fresh wallets made with the released CLI, every action from a private account:
- register by commit, then reveal (`jef.lez`, `logos.lez` and a new user's name);
- transfer to another wallet's private account (block 23088);
- the old holder's release refused after the transfer;
- the new holder taking over with fresh receiving keys (block 23131);
- release, after which the name resolves as not registered (block 23159).

**Fixed salt (1.3):** your numbers were, if anything, generous to us. At hashcat's PBKDF2-SHA256 rate on an RTX 4090, five million common names take about a minute, and the fixed salt means that dictionary works against every registry. SPEC §1 now says so plainly. A memory-hard KDF with a per-registry salt is the real fix, and we're looking at it for the next registry version.

**Owner tags (1.4):** SPEC §1 now says plainly that the holder of a name held from a public account can be found after the fact, by testing public accounts against the tag. A private-account holder stays hidden, which is why names are held privately by default.

**The gateway (1.5, 1.7):** the website no longer resolves names at all; it is static files. Pages are viewed with `names open <name>`, which reads the page from the chain on the viewer's machine and serves it on 127.0.0.1, under a policy that loads nothing from the network (SPEC §12.2). §12.3 specifies resolving in the browser once sequencers allow cross-origin requests: the name goes after `#`, and nothing relays.

**Installer (1.8):** releases are now signed (ed25519, `ssh-keygen -Y`), and `install.sh` refuses a missing or bad signature.

**Websites on Logos Storage:** yes. A name can carry a `cid` text entry, so your viewer can take `vpavlin.lez`, resolve it in the client (JS resolver in `resolver/`; in a browser that needs the testnet RPC to allow CORS, which I'm asking for), and fetch that CID from Logos Storage. Would that work for you?

Would you be up for a re-review of v4.1?
