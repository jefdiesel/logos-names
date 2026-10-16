Thanks for the review, it was useful. Both blockers are fixed in registry v4.1, now live on the LEZ testnet as `2PXg3SSmh5hvECTz4xcfV2d5YvxwyF6yVhpnjTokzCkX` (it replaces `66nUvz…LYad`).

**Front-running on Register:** registration is now commit-then-reveal. `Commit` publishes only `SHA256("logos-names/v4/commitment" ‖ id ‖ owner_tag ‖ secret)`, which hides the name, the holder and a random secret. `Register` reveals them and consumes the commitment. The commitment has to be at least 15 minutes old and at most 24 hours old, and each transaction's claimed time has to fall within a 10-minute window of the chain's clock. The window is that wide because a private holder's proof takes about 4 minutes. Someone who sees the reveal in the mempool can't use it: their own commitment would be too new. The CLI waits out the 15 minutes in the background, so `names register` returns right away.

**Configure deploy race:** the first `Configure` must now be signed by the program's own header account, which only the deployer controls during deployment. A stranger configuring a fresh deploy is refused.

Both were tested on a test chain: a stranger's configure was refused, two racing registrations for one name had the second refused, and a private holder registered through commit and reveal. Code: https://github.com/jefdiesel/logos-names (`core/src/lib.rs`, SPEC "v4 rules").

**Fixed salt and owner-tag enumeration:** the SPEC now says both plainly (section 1). Your numbers were, if anything, generous to us: at hashcat's PBKDF2-SHA256 rate on an RTX 4090, five million common names take about a minute, and the fixed salt means that dictionary works against every registry. The SPEC now says so. Registry v5 will fix it properly: Argon2id (64 MiB), salted per registry, derived in the client (the gateway will stop deriving names server-side, so it never learns them). The owner tag is a plain hash of the name's ID and the holder's account, so anyone who knows a name can test every public account that has appeared on chain and find a public holder. A private-account holder stays hidden, which is why private is the default.

**Websites on Logos Storage:** yes, that's the intent. A name can carry a `cid` text entry and a page, so a viewer can take `vpavlin.lez`, resolve it, and fetch the CID from Logos Storage. Would that work for your viewer as a first step?

Would you be up for a re-review of v4.1?
