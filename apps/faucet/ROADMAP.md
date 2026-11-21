# Roadmap: open faucets in the Faucet app

Today the app asks one service for test LEZ. The person connects Names Wallet, picks a public
account, and the app's small backend sends that account ID to the Names faucet. The service holds
the LEZ and sets the rules: 2,000,000, once a day per account and per internet address. The faucet
address is a setting, so another service that speaks the same protocol works too. The app only
watches the account balance and shows the result.

The idea is to let anyone run a faucet, with its LEZ and its rules on the Logos test network.

## 1. What changes for people

- **A list of open faucets.** Each row says who made it, what LEZ is left, and its rules. Pick one
  to see its details.
- **Fund this faucet.** Anyone can add LEZ to a faucet they like.
- **Ask for test LEZ.** The faucet's rules decide who gets how much and how often. The app shows
  the decision in plain words, as it does today.
- **Make a faucet.** Anyone can create one and set its rules at creation.
- **Withdraw and Change rules.** The creator sees these for their own faucet, when its rules allow
  a change.

The connect step stays the same: the person picks a public account and approves in Names Wallet.
The screens keep the shared kit — one column, numbered steps, one strong button, a plain result
line.

## 2. Rules a faucet can have

- **Amount per claim** — how much LEZ one request pays, so a faucet can serve more people.
- **Cooldown for one account** — how long an account must wait before claiming again, so one
  account can't take everything at once.
- **Total cap or end date** — a faucet can stop after a set amount or on a set date, so a creator
  knows what they are giving.
- **Who may claim** — anyone, holders of an LNS name, members of an NFT collection (proved with
  the LP-0001 membership proof), or a named list. This lets a faucet serve a group without asking
  people for anything more than a proof they already have.
- **Pause** — the creator can stop claims for a while, so a mistake or an attack does less harm.

Which rules can change after creation, and which must be fixed when the faucet is made, is Jef's
decision. They are listed as an open question below. Until then, each rule should say which kind
it is when the faucet is created.

## 3. How it works underneath

- An on-chain faucet program holds each faucet's LEZ in its own account.
- The program enforces the rules. The app never decides who gets LEZ; it only shows what the chain
  says happened.
- Every action is signed by the person through Names Wallet SDK: connect, then `propose_call` for
  funding, claiming, creating, withdrawing and changing rules. The app never sees a key.
- The app reads the list of faucets and each faucet's state from the chain. No server is needed.
- The current faucet service stays as one option in the list until the program exists, so people
  can still get test LEZ during the change.

## 4. Safety

- Only the creator can withdraw from their faucet. Nobody else can take its LEZ.
- The program checks the rules when a claim is made, so nobody can drain a faucet faster than its
  rules allow — not even with a different app or many requests at once.
- If nobody tops up a faucet, it runs out. Claims are refused with a plain message; the LEZ still
  in it stays there until the creator withdraws it.
- Many accounts are the hard case. A cooldown and an amount limit slow one account, but cannot
  tell whether two accounts belong to the same person. An LNS name rule, an NFT membership rule or
  a named list narrows who may claim; those rules need more setup from the creator. The program
  cannot do more than the rules say.

## 5. Phases

1. **The program and its tests on the bridge test network.** This part holds people's LEZ, so a
   stronger worker should write it and test it first.
2. **The module calls through Names Wallet SDK.** Same reason: these calls move LEZ and must be
   right before an app uses them. The stronger worker does this too.
3. **The app screens on the shared design kit.** These can follow once the calls exist: the list,
   a faucet's detail card, Fund, Ask, Make, Withdraw and Change rules.
4. **Listing faucets in the Apps registry.** Last, so people can find the faucets in Basecamp.

## 6. Open questions for Jef

- Which rules are fixed at creation, and which can the creator change later?
- Who pays network fees — the faucet, its creator, or the person claiming?
- Can a faucet be closed for good, and what happens to the LEZ left in it?
- What limits should each faucet have: most LEZ it can hold, most claims, or a lifetime?
