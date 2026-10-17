# LEZ board: a plan

A threaded comment board on LEZ where `.lez` names are the identities. Plan
only: nothing here is built yet.

## What it is

- **Threads and replies.** A thread has a title and a first post; replies point at
  the post they answer, so a client draws a tree.
- **Every post is signed by a name.** Three ways to post, chosen per post:

| Mode | Who the chain sees | Time to appear | Fee |
|---|---|---|---|
| **As your name, privately** (default) | the name only; the account that holds it stays hidden | about 4 min on an M4 Mac (one proof) | none |
| **As your name, fast** | the name, plus its public posting key | about 10–15 s (one block) | about 0.3M (testnet) |
| **Anonymously** | a generated name, `anon-<word>-<trait>`, held privately by your wallet | about 4 min | none (once the anon name is registered) |

- **Speed setting** (the "switch it on" choice): in the app's settings, proving can
  use the GPU (RISC Zero's Metal prover on Apple silicon) or all CPU cores. Step 1 of
  the spike measures how much faster that is. If it gets a proof under a minute,
  private posting becomes the only mode most people need.

## Scope for the first version

- **Posts and threaded replies**, shown newest first.
- **Two feeds:** **All** (every post on the board) and **Following** (posts from the
  names you follow).
- **Likes** and **follows.**
- **CID links, images included:** a post can carry Logos Storage CIDs; images show
  inline, other files as links.
- Not yet: DMs, private boards.

### The hard part: shared counters and private posts

A private post is proved against the chain's state at the start of its 4 minute proof.
If it also updates an account that someone else changes meanwhile (a global post
counter, a like count, a reply count), it lands stale and is refused. So:

- **Posts are numbered per author**, `post/<name id>/<n>`, and only the author writes
  their own counter. Posting never touches a shared account.
- **All** is built by the reader's app from recent blocks: it reads the board
  program's transactions from the chain's own RPC, newest first, and fetches the posts
  they created. No indexer; the reader's app does the scan. (The spike checks the RPC
  gives what this needs.)
- **Following** is built from the follow list: for each followed name, read its
  counter and its latest posts, then merge by time.
- **Replies** carry their parent's id; a thread is assembled the same way as All,
  filtered to that thread.
- **Likes** are one small account per (post, liker), so a like never conflicts with
  another. Counts come from the same block scan. If that is too slow to read, counts go
  into a few shard counters a liker picks at random, with an automatic retry.
- **Follows** live in the follower's own account, **sealed** under the follower's key
  like a profile: nobody else learns who you follow, and Following still works on any
  of your devices. (So there are no public follower counts in this version.)

### Images and files (CIDs)

- A post holds CIDs next to its text (up to 4). The app fetches them through **your
  own Logos Storage node**, never a public gateway, and shows images inline after a
  size check. Other files show as a link to save.
- Risk: Logos Storage's peer discovery bug (logos-storage #1556) can stop a node
  finding a file it doesn't host. The app then says the file can't be found yet.

## Anonymous names

- First time you post anonymously, the app makes a name like `anon-quiet-otter` (two
  word lists, a few thousand each, plus a short number if taken) and registers it for
  you, privately, with a **fresh private account** in your wallet.
- That account is funded by a private-to-private transfer from your main private
  account, so the chain shows no link between the anon name and your main name. Only
  your wallet knows both.
- It is a real `.lez` name: other people can look it up, reply to it, and see all its
  posts as one pseudonym. You can make a new one at any time.
- Costs: the normal 1M deposit (refunded on release) and the 20 minute registration,
  done once per anon name. The app starts it in the background on first use.
- What it does not hide: what you write, and when you post. Writing style and timing
  can still link two identities.

## How it works on chain

A new LEZ program, `board`, separate from the names registry.

- **Accounts** (all PDAs of the board program, so no list and no indexer):
  - `author/<name id>`: that name's post count (written only by the name).
  - `post/<name id>/<n>`: author name (plaintext), author name id, parent post id
    (none for a new thread), time, text (up to 1 KiB), up to 4 CIDs.
  - `like/<post id>/<liker name id>`: exists if liked.
  - `follows/<name id>`: the name's follow list, sealed.
- **Who may post as a name:** the board program reads the name's record account from
  the registry and checks that the signer's owner tag, `SHA-256("logos-names/v5/owner"
  ‖ id ‖ signer)`, equals the record's owner. That is the check the registry itself
  uses, so a private holder passes it inside a proof without showing the account.
- **The name appears in plaintext in each post.** The program can't run the name
  derivation (Argon2id, 64 MiB), so a reader checks that the name in the post derives
  to the id in it before showing it, and caches the result per author. This means a
  post publishes the name it was signed with. That is the point of posting as a name,
  but it is new: until now the chain never held a name.
- **Fast mode:** the name's profile gets a `post_key` entry (a public account). The
  board accepts that key's signature for that name after checking the profile. That
  key is seen on chain posting, but it holds nothing else.
- **Deleting** hides a post (its author sets a flag); it stays in the chain's history.
- **Spam:** fast posts pay a fee; private posts are throttled by the 4 minute proof.
  A per-board minimum (say one post a minute per name) is enforced by the program.

## Where it shows

- **Basecamp:** a Board app. It reuses the names module for avatars, display names
  and the proving jobs, and shows the same "step N, about 4 min" progress as
  registering. The post box says which mode it will use and how long it takes.
- **CLI:** `names post "<text>" [--cid …]`, `names reply <post> "<text>"`,
  `names like <post>`, `names follow <name>`, `names feed [--following]`,
  `names thread <post>`, with `--fast` and `--anon`.

## Cost (zero marginal cost)

| Path | What serves it | Monthly requests | If a bot hits it 500/s |
|---|---|---|---|
| Reading a board or thread | the LEZ sequencer's public RPC, from the reader's own app | not ours | costs us nothing; the sequencer's own limits apply |
| Posting | a transaction the poster pays for (or proves, privately) | not ours | spam pays fees or waits 4 min per post |
| The website | static files, as today | unchanged | free |

No Worker, no server, no indexer.

## Steps

1. **Spike (about a day).** Measure a proof of a 1 KiB post on CPU and with Metal on
   the Mini. Check that a LEZ program can read another program's account (the name
   record) during a private transaction. If it can't, the board stores the owner tag
   itself and the name holder registers a "board identity" once.
2. **Board program** in `board/` (Rust, `lee_core`, like the registry), with unit
   tests for posting, replying, hiding, the owner-tag check and the rate limit.
3. **CLI commands** and a deployment on testnet with one board, `lez`.
4. **Anonymous names**: name generator, fresh private account, private funding,
   background registration.
5. **Basecamp Board app**, then a `/critique` pass on its copy.
6. **Fast mode** (posting keys), if step 1 shows private posting is too slow.

## Open questions

- Can a LEZ program read an account owned by another program inside a private
  transaction? (Step 1.)
- How much does Metal proving help on the Mini and the M2? (Step 1.)
- Plaintext posts only for now. Private boards (posts sealed to members) can come
  later, the same way names seal their profiles.
