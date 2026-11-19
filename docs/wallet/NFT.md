# Names Wallet: NFTs on LEZ 0.4 (LP-0001, wallet part)

Branch `names-nft` (from `names-next-1118b` 31153da). This page is the plan and the record of
what the wallet does with NFTs. It integrates the reference NFT program of
`logos-blockchain/lez-programs` (branch `refactor/lez-v0.4.x`, dea23c2, `programs/nft`,
`docs/nft/README.md`, `artifacts/nft-idl.json`). It does not write an NFT program. Facts about
the program come from that source and from the observations in
`~/logos/LP0001-JOB-A-FINDINGS.md` ("Job A"), made on a throwaway LEZ v0.4.0-rc3 chain with
development receipts.

## 1. What the prize asks, and what the wallet does

From [LP-0001](https://github.com/logos-co/lambda-prize/blob/master/prizes/LP-0001.md), the
criteria this step covers, word for word:

- **NFT ownership**: "own, send, and receive non-fungible token-program assets on a LEZ zone,
  from both public and private accounts."
- **NFT display**: "the wallet lists the NFTs held by each account, grouped by collection,
  showing at minimum the collection, the token identifier, and any metadata the token program
  exposes. Assets with missing or unresolvable metadata are shown as such rather than hidden or
  silently rendered as valid."
- **Collection view**: "a user can see which collections an account holds assets in, and how
  many from each."
- **Wallet Provider SDK NFT APIs**: "third-party dApps can enumerate an account's NFTs, query
  collection membership, and propose NFT transfers through the SDK, subject to the same approval
  flow as fungible transfers."
- Usability: "Approval prompts for NFT transfers clearly identify the requesting dApp, the target
  account, the collection, and the specific asset leaving the account. Collection-wide
  approvals, if supported, must be distinguishable from single-asset approvals in the prompt";
  "Permissions are legible and revocable … (proof requests, account access, NFT enumeration,
  transaction proposals) and revoke any of them individually"; "The approval prompt must show
  the user the program's source-verification status".
- Reliability: "A dApp cannot … enumerate a private account's NFTs without an explicit user
  approval for that account"; "Private transfers are unlinkable. A private-to-private NFT
  transfer does not reveal a sender→recipient edge to a chain observer"; "Fetching media from
  centralised services is optional and disableable … centralised media services are never
  contacted for a private account without the user turning it on."

LP-0021's criteria apply underneath: "Wallet selection & approval flow: when a dApp requests a
connection or a transaction, the user is shown a selection prompt (which account to use) and an
approval prompt (what is being signed/sent) before anything is signed or submitted."

What the wallet does for them, in this step: lists each account's NFTs per known NFT program,
grouped by collection with a count; shows every field the program stores; initialises a holding
(Receive), sends, burns and prints, from public and private accounts, each behind a sheet that
says what becomes public; gives other apps an NFT list per account they were given for that
purpose, and NFT transfer requests that go through the same sheet.

## 2. What the reference program is (READ, `programs/nft/core/src/lib.rs`)

- A **definition** is one NFT: `name` (1–64 bytes), `printable_supply`, `metadata`. Its
  **master** holding carries the print rights; the master prints **copies** into other holdings.
  `printable_supply: N` allows N − 1 copies. The wallet calls the definition the
  **collection**: its ownership set is the master and the owned copies. There is no grouping
  above a definition.
- An account has **one NFT state per program deployment**: `Definition`, `Master
  {definition_id, print_balance}` or `Copy {definition_id, owned}`. A second NFT needs a second
  account. A holding stays bound to its definition after it sends or burns: it can later receive
  another NFT of the **same** collection only.
- **There is no token serial.** Copies of one definition are identical. The wallet's token
  identifier is the definition's ID, "master" or "copy", and the account that holds it.
- Messages (tags from the IDL): `NewDefinition` 0, `InitializeHolding` 1, `Transfer` 2,
  `PrintNft` 3, `Burn` 4, `ReadState` 5. Pending credits are the program's internal messages
  (`Receive` 3, `ReceivePrint` 4 in the `Internal` enum), which are not in the IDL.
- Every deployment is a different program; state does not carry over
  (`docs/nft/README.md:16-19`).

The wallet's encodings are hand-written in `basecamp-module/rust-lib/src/nft_wire.rs`, one file
that the module and the names CLI both compile, the way `deposit.rs` is shared. They are tested
against bytes cut out of the real transactions of Job A's public runs. No private transaction's
message can be cut out of the chain, and the internal messages are tested against the source.

## 3. The program ID problem

No NFT program runs on the public testnet; the one Job A deployed
(`ErycMYqXUUtducaYmpHZjRgdbLXchrMiV4NZRnmRDNmu`) is a native build on a throwaway chain. Every
deployment is a new program, so the wallet keeps **a list of NFT programs per network**, which
the person adds to (Collectibles, Programs). It ships empty. Each entry shows the Program
Registry's label and source-verification status beside it and on every sheet that uses it.
When Logos deploys the program on its testnet 0.4, that ID is added to the list for that
network, by the person or in a release.

## 4. How the wallet finds an account's NFTs

There is no on-chain list of holders, and no list of definitions (Job A §4). The wallet reads,
for every known program and every account of its own:

- a public account: its actor state under the program (`getAccountView(account, program)`);
- a private account: its own note, after catching up with the chain.

That is one state per account per program, so the read is bounded by accounts × programs.

What can be enumerated: what this wallet's accounts hold; what any public account holds under a
known program; a definition's fields, given its ID. What cannot: which definitions exist under a
program (only by scanning blocks for `NewDefinition` events, which the wallet does not do); who
holds a collection; anything about someone else's private account. A pending NFT credit to one
of this wallet's private accounts is found among the pending messages the wallet's keys open.

## 5. Metadata

The definition stores `standard` (`Simple` or `Expanded`), `uri` (≤ 256 bytes) and `creators`
(≤ 256 bytes) inline. The program gives `Simple` and `Expanded` no meaning; the wallet shows the
word. The wallet shows the name, uri and creators as plain text, cleaned of control and
direction-changing characters. A state that does not decode is listed as "unreadable", never
hidden.

The uri is never opened unless the person allows it **for that collection**. Opening it tells
the host the person's address and that they look at this collection, and its content is remote
and untrusted. When allowed: https only, no redirects, never a loopback or private address, at
most 1 MiB, PNG only, decoded to pixels in the module (as avatars are), no script, nothing
rendered as a page. Off by default; the wallet stays fully usable with it off. For a collection
held only by private accounts, the allow asks a second time. Logos Storage upload and download
are not in this step.

## 6. Receive: a holding first

The program delivers an NFT only to an account that has said beforehand which collection it
takes: `InitializeHolding { definition }`, signed by that account (Job A §8: 37,460 gas). The
app explains it in one line: "An account takes NFTs of one collection, after it says so in a
small transaction. This one will be ready for <collection>." Receive offers a **new account**
(public or private) by default, or an account of this wallet whose state under the program is
empty. A repeat for the same collection is a no-op and the wallet does not send it. A new
account is made only after everything that can refuse was checked, and a new public account's
fee is paid by another public account the person picks (it holds nothing yet). After it, the app
shows the account with Copy, and lists every holding ready to receive. A private account's
receiving keys are not shown yet: for a sender outside the wallet, the app suggests a public
account.

## 7. Send, burn, print: what each reveals

What a chain observer can read, from Job A's table (§8b.3: known account IDs searched in each
transaction's bytes; development receipts, so a check that the IDs are absent, not a proof that
nothing links):

| From → to | Named in the clear | Not named |
|---|---|---|
| public → public | both accounts, the collection, the NFT moved (events `Transferred`/`Printed` name both) | nothing |
| public → private (this wallet's) | the sender, the collection, **the receiving private account's ID** | its other state |
| private → public | the recipient, the collection | the sender |
| private → private (two accounts of one wallet) | only the NFT program | both accounts, the collection |
| private initialise / burn | the collection | the holder |
| public → someone else's private keys | expected: the sender, the collection | expected: the recipient (NOT observed) |
| private → someone else's private keys | expected: only the NFT program | expected: both accounts (NOT observed) |

The sheet shows the row that applies before Approve. Notes:

- A **public recipient**, or one of this wallet's private accounts, is checked before sending:
  it must be a holding of this collection that holds nothing. A wrong recipient makes LEZ refuse
  the whole transaction and nothing moves (Job A §8).
- **Someone else's private account** (given by its receiving keys) cannot be checked. The
  sender's holding empties at once and the NFT waits as a pending message until the recipient's
  wallet receives it. If their account has no holding of this collection, or holds another NFT,
  it cannot receive it and the NFT may stay stuck. This was NOT observed in Job A (all private
  parties were in one wallet). The sheet says so and needs an extra tick.
- **LNS names**: an NFT to a name goes to the name's public account and is checked there. The
  name's private keys are used only when the person picks them, with the warning above.
- **Receive of waiting credits** goes through the same sheet: one private transaction per NFT,
  each paid by the private account chosen there.
- **Who pays a public action is named too.** When another public account pays the fee (a new
  account holds nothing), the transaction names it, so anyone can see the two belong together.
  The sheet says so; Receive says a new public account is tied to its fee payer, and a new
  private account isn't.
- **Burn** (two steps): a copy burns exactly 1; a master burns all its print rights, and the NFT
  with it. The definition is declared, as the program needs (§8: without it, refused).
- **Print** (a master holder only): one copy to a ready holding; the master keeps the rest.

Refusals in plain words (program panics and LEZ errors, mapped in `nft_wire::refusal`):
"recipient has no NFT holding" / "A delivery named Actor … neither a declared public actor nor
private" → "That account isn't ready for this collection: it has to receive a holding first.";
"recipient already holds an NFT" → "That account already holds an NFT of this collection.";
"recipient holds another NFT" / "account already holds another NFT state" → "That account is
tied to another collection under this program; use another account."; "sender holds no NFT" /
"holding holds no NFT" → "This account holds no NFT to send."; "insufficient print balance" →
"The master has no prints left."; "receiver is not an NFT master" → "Only the master can
print."

## 8. Fees, and the private fee payer

- A public action is paid by a public account the person picks on the sheet (by default the
  signer when it can pay). Gas is about 28,000–37,000 (Job A §9); the sheet prices 40,000.
- A **proven** action (any private account, or a public sender to someone's private keys) pays
  inside the proof from a **private** account that holds the reserve; Job A saw about 5.07M
  native reserved per private action on the throwaway chain, refunded later as a pending credit
  whose receipt costs the same reserve. A public payer fails for a proven transaction.
- The wallet never lets the LEZ wallet pick the private payer: its automatic pick was named in
  the clear once (Job A §8c A2.5). The sheet asks which private account pays, shows the reserve
  read from the chain's fee state, and refuses one that can't cover it. A private payer chosen
  explicitly was not named in the clear in Job A's runs (§8c A2.1).

## 9. What is not in this step

- The private ownership proof, its verifiers and its SDK capability (another worker). The seam:
  the provider keeps `nft_list` (enumeration) as its own grant, separate from account access and
  from any future `nft_proof` request, which will need no account access.
- The token-gating mini-app, Logos Storage media, collection creation in the app (the CLI has
  `names nft create` for tests), adoption, and anything on Logos testnet 0.4 (not live).

## 10. Status on branch `names-nft` (2026-11-19)

| Piece | Where | State |
|---|---|---|
| Wire format, readiness, refusals, what each send reveals | `basecamp-module/rust-lib/src/nft_wire.rs` (shared with the CLI) | tested: messages against Job A's real transactions, states against that chain |
| Program list, holdings, collections, sheets, actions, image consent and fetch | `basecamp-module/rust-lib/src/nft.rs`, trait methods `nft_*` in `lib.rs` | tested with a fake CLI (module unit tests) |
| Provider SDK: NFT list grant, membership answer, transfer requests | `basecamp-module/rust-lib/src/provider_nft.rs`, [PROVIDER-SDK.md](PROVIDER-SDK.md#nfts-lp-0001) | tested with a fake CLI |
| `names nft …`, `names module nft …` on the stock wallet crate | `cli/src/nft.rs` | written; not yet compiled (RUN-AFTER-3AM.txt step 2) |
| Collectibles page, the one sheet, apps' NFT requests | `basecamp/qml/Main.qml` (two marked blocks), `basecamp/qml/nft.mjs` | `node basecamp/nft.test.mjs`; 13 offscreen renders (`render.py collectibles-*`) |
| End-to-end on the throwaway chain | `RUN-AFTER-3AM.txt` steps 4-5 | not run |
| The CLI | compiled on the laptop (`RISC0_SKIP_BUILD_KERNELS=1`) | 12 NFT tests pass |
| Sheets bound to what they showed | plan `pin` (state digest) + resolved recipient, `--expect-state` | tested in the module and the CLI |
| Membership proofs, wallet side | `proof.rs` (gate rules ported, checked against the gate's vectors), `provider_proof.rs` | tested with a stand-in gate client; the real proving is UNRUN, fails closed `GATE_NOT_PINNED` |

Known limits, on purpose for this step:

- An app's NFT request is answered on the Collectibles page; the tab says it waits, on every
  page (no poll of its own: it rides on the existing request read).
- Apps get private accounts as per-app handles, never their IDs.
- `nft_membership` is a plain answer about accounts the app was given, not a private proof.
- One NFT per account per program means a wallet makes a new account per NFT received. The
  Receive form offers a new account first.
