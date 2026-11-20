# LNS name NFTs (deeds)

A plain guide to the optional NFT an LNS name can have. SPEC.md §8.6 is the source of truth.

## What it is

An LNS name can have an NFT, called a deed. It is optional: a name works fully without one.
Claiming is free; the only cost is gas. There is one deed per name. Only names of 5-32 characters
that are not locked or reserved can have one. Registering a name never mints a deed.

You claim one in the Names Wallet: **My names → Claim NFT (free)**. The claim is refused for anyone
but the holder, for a name that already has a deed, and for locked, reserved and short names.

## Transfer, release, lapse

Transfer of a name with a deed moves the deed to the new holder's deed account in the same
transaction, including when the recipient is private. A name without a deed transfers as before.

Release burns the deed: the definition's printable supply goes to 0. A registration over a lapsed
or claimed-over record burns the old deed. The next holder may claim a fresh one, with a new nonce,
a new definition and a new deed account. Used accounts are never reused.

The registry code settles what happens while a name is lapsed. The holder can still transfer it,
and its deed goes with it: `Effect::Transfer` uses `load_owned`, which checks only that a record
exists and that the signer owns it, with no expiry or grace check
(`core/src/lib.rs:1391-1396`, `:1217-1221`); the deed move is sent with the transfer
(`core/src/lib.rs:1190-1193`). Only the holder can claim a deed on such a name, because
`Effect::ClaimDeed` uses the same `load_owned` check (`core/src/lib.rs:1103-1111`, `:1439-1445`).
The old deed is burned when a registration overwrites the lapsed record
(`core/src/lib.rs:400`, `:1183-1185`), when a claim overwrites a record (`:1186-1188`), and on
release (`:1194-1196`, `:1010-1017`). The test
`a_lapsed_names_deed_is_burned_when_it_is_registered_again` (`core/src/lib.rs:3415`) checks the
burn on re-registration.

## Privacy

The image on the chain is the same shared card for every deed:
https://nameswallet.app/lns/deed.png. It reads "Names", never a name. The definition's metadata is
fixed too: name `LNS name deed`, standard `Simple`, creators `LNS registry`.

The wallet, or any SDK app, draws the holder's own name on the card locally. That is why the chain
never prints which names an account holds.

## For developers

The JS resolver lives in `resolver/` on branch `hy4-deeds-js` and is packaged as `@lns/resolver`
(a private placeholder name for now). `resolveName(rpcUrl, program, name)` reads the version 7
record and returns `deed: { definition, holding, nonce }`, or `deed: null` for version 6 and older.
`deedAccounts(program, id, ownerTag, nonce)` derives the same pair from raw record bytes.
`nameCard(name)` returns a 480x480 SVG string; fonts are named, never embedded.

```js
import { resolveName } from "@lns/resolver";
import { nameCard } from "@lns/resolver/card";
const record = await resolveName(RPC, PROGRAM, "jef05");
if (record?.deed) console.log(nameCard("jef05"));
```

The accounts come from SPEC.md §8.6. The definition is
`D = PDA(registry, SHA-256("lns-deed/v1/definition/" ‖ id ‖ nonce))`, with printable supply 1. The
deed is its master, held at
`PDA(registry, SHA-256("lns-deed/v1/holding/" ‖ D ‖ owner tag))`. A PDA has no key, so only the
registry's seed grant authorizes it: the NFT program refuses a holder's own `Transfer` or `Burn` of
a deed. A transfer or release declares these accounts, or the change is refused.

## Known limits

From the wallet-1121 install review, §5 and §9:

- The Claim NFT (free) button appears only once `deed_tiers` says the name is of the normal tier.
  That tier comes from the names tool's locked-name list; with no tool there is no button. A failed
  check now shows a plain line under the name instead of a silent wait.
- The card's footer uses the app's mono font (Menlo), not Fira Code (not bundled).
- The module's own transfer/release path (`changes.rs`, signed through `module send`) declares the
  deed accounts and is unit-tested, but was not run against the chain in that review. The app's
  transfers use the names tool's path, which the lns-deeds rehearsal ran.
- One Collectibles read of the built-in NFT program returned HTTP 429 from `lez.nameswallet.app`.
  The names tool's pacing and the module fix were installed at 02:20 UTC; the later read showed the
  card with no 429 line.
