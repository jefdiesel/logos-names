//! `names post`, `names like`, `names unlike`, `names hide`: changes to the LNS
//! board, made as a name. The Basecamp Board app runs these; people rarely type
//! them. Every change is made from the private account that holds the name, so
//! it is proved on this machine and the chain sees the name, never the account.

use super::*;
use board_core::{Instruction as BoardInstruction, Message as BoardMessage, PostId, REGISTRY};

/// The board on the LEZ testnet. `LOGOS_BOARD_PROGRAM` points at another.
const DEFAULT_BOARD: Option<&str> = Some("8PwnCKStKQtoBxJCgopCG6CPPMJCUWba1SDG1SXtXYTf");

pub fn board_program() -> Result<AccountId> {
    let text = std::env::var("LOGOS_BOARD_PROGRAM")
        .ok()
        .or_else(|| DEFAULT_BOARD.map(str::to_owned))
        .ok_or_else(|| anyhow!("no board is deployed yet; set LOGOS_BOARD_PROGRAM"))?;
    text.parse().map_err(|_| anyhow!("LOGOS_BOARD_PROGRAM is not a program ID: {text}"))
}

/// A post, as people pass one: its ID (the address of its account, which is
/// unique and what apps show), or `<name>/<n>`.
#[derive(Clone, Debug)]
pub enum PostRef {
    Id(AccountId),
    Named(String, u64),
}

pub fn parse_post(input: &str) -> Result<PostRef, String> {
    let input = input.trim();
    if let Some((name, n)) = input.rsplit_once('/') {
        let name = parse_name(name)?;
        let n = n.parse().map_err(|_| format!("not a post number: {n}"))?;
        return Ok(PostRef::Named(name, n));
    }
    input
        .parse()
        .map(PostRef::Id)
        .map_err(|_| format!("not a post: {input} (give its ID, or <name>/<number>)"))
}

impl Client {
    /// The post `post` names: its ID on the board, its author's name and its address.
    async fn find_post(&self, board: AccountId, post: &PostRef) -> Result<(PostId, String, AccountId)> {
        let (address, named) = match post {
            PostRef::Id(address) => (*address, None),
            PostRef::Named(name, n) => {
                let id = PostId { author: derive(name).id, n: *n };
                (board_core::post_address(&board, &id), Some(name.clone()))
            }
        };
        let bytes = self.actor_state_of(address, board).await?;
        if bytes.is_empty() {
            bail!("there is no such post on the board");
        }
        let found: board_core::Post = borsh::from_slice(&bytes).context("that account is not a post")?;
        Ok((found.id, named.unwrap_or(found.name), address))
    }

    /// The account in this wallet that holds `name`. A private one posts by a
    /// proof (no gas, about 4 minutes); a public one by a signed transaction
    /// (one block, and it pays LEZ gas).
    async fn private_holder(&self, name: &str) -> Result<(NameId, AccountId)> {
        if self.program != REGISTRY {
            bail!("the board trusts registry {REGISTRY}, not {}; it was built for another chain", self.program);
        }
        let (Derived { id, .. }, record, _) = self.lookup_held(name).await?;
        Ok((id, self.holder(name, &id, &record)?))
    }

    /// The board's actor of `signer`, the transaction's root.
    fn board_root(&self, board: AccountId, signer: AccountId) -> AccountMention {
        self.signer(signer).select_program_actor_state(board)
    }

    /// The board account behind `seed`. The seed lets a transaction create it.
    fn board_actor(&self, board: AccountId, seed: PdaSeed) -> AccountMention {
        AccountIdentity::PublicPda { program: board, seed }.select_program_actor_state(board)
    }

    /// The registry's record of `name`, which only the registry itself reads.
    fn record_actor(&self, name: &NameId) -> AccountMention {
        self.registry_actor(name_seeds(name)[0])
    }

    /// Sends `instruction` to the signer's actor of the board, `mentions[0]`. The
    /// board's holder check runs the registry too, so both programs are named.
    async fn send_board(&mut self, board: AccountId, mentions: Vec<AccountMention>, instruction: &BoardInstruction) -> Result<()> {
        let programs = vec![
            (board, self.program_at(board, "LOGOS_BOARD_PROGRAM_ELF", "logos-board", "the board").await?),
            (self.program, self.program().await?),
        ];
        let message = borsh::to_vec(&BoardMessage::Instruction(instruction.clone()))?;
        // A board call also runs the registry's holder check: about a profile change's cycles.
        self.transact(mentions, message, programs, 0, 65_590).await
    }

    /// Posts `text` as `name`, starting a thread or, with `parent`, replying.
    /// Prints the new post as `<name>/<n>`.
    pub async fn post(&mut self, name: &str, text: String, cids: Vec<String>, parent: Option<PostRef>) -> Result<()> {
        let board = board_program()?;
        let (author, signer) = self.private_holder(name).await?;
        let counter = board_core::author_address(&board, &author);
        let bytes = self.actor_state_of(counter, board).await?;
        let n = if bytes.is_empty() {
            0
        } else {
            borsh::from_slice::<board_core::Author>(&bytes).context("the author account does not decode")?.count
        };
        let parent = match parent {
            Some(parent) => Some(self.find_post(board, &parent).await?.0),
            None => None,
        };
        let id = PostId { author, n };
        let post = board_core::post_address(&board, &id);
        let instruction = BoardInstruction::Post {
            author,
            name: name.to_owned(),
            n,
            parent,
            text,
            cids,
            now: self.now().await,
        };
        let mentions = vec![
            self.board_root(board, signer),
            self.record_actor(&author),
            self.board_actor(board, board_core::author_seed(&author)),
            self.board_actor(board, board_core::post_seed(&id)),
        ];
        self.send_board(board, mentions, &instruction).await?;
        println!("posted {name}/{n}, ID {post}");
        Ok(())
    }

    /// Likes (or, with `undo`, unlikes) the post `post` as `name`.
    pub async fn like(&mut self, name: &str, post: PostRef, undo: bool) -> Result<()> {
        let board = board_program()?;
        let (liker, signer) = self.private_holder(name).await?;
        let (id, _, address) = self.find_post(board, &post).await?;
        let mut mentions = vec![
            self.board_root(board, signer),
            self.record_actor(&liker),
            self.board_actor(board, board_core::like_seed(&id, &liker)),
        ];
        if undo {
            self.send_board(board, mentions, &BoardInstruction::Unlike { liker, post: id }).await?;
        } else {
            mentions.push(self.board_actor(board, board_core::post_seed(&id)));
            let now = self.now().await;
            self.send_board(board, mentions, &BoardInstruction::Like { liker, post: id, now }).await?;
        }
        println!("{} {address}", if undo { "unliked" } else { "liked" });
        Ok(())
    }

    /// Takes down the post `post`, which `post.0` must be held by this wallet.
    pub async fn hide(&mut self, post: PostRef) -> Result<()> {
        let board = board_program()?;
        let (id, name, address) = self.find_post(board, &post).await?;
        let (author, signer) = self.private_holder(&name).await?;
        let id = PostId { author, n: id.n };
        let mentions = vec![
            self.board_root(board, signer),
            self.record_actor(&author),
            self.board_actor(board, board_core::post_seed(&id)),
        ];
        self.send_board(board, mentions, &BoardInstruction::Hide { post: id }).await?;
        println!("hid {address}");
        Ok(())
    }
}

/// The messages a privacy-preserving transaction delivers to public actors.
fn crossings(tx: &lee::PrivacyPreservingTransaction) -> impl Iterator<Item = (&Actor, &Vec<u8>)> {
    tx.message.execution.boundary.iter().filter_map(|step| match step {
        lee_core::execution_state::BoundaryStep::PrivateToPublic(delivery) => {
            Some((&delivery.envelope.to, &delivery.envelope.message))
        }
        _ => None,
    })
}

/// What a message to a program counts as in `names stats`, if anything.
fn kind_of(to: AccountId, message: &[u8], registry: AccountId, board: Option<AccountId>) -> Option<&'static str> {
    if to == registry {
        return match borsh::from_slice::<RegistryMessage>(message).ok()? {
            RegistryMessage::Instruction(Instruction::Register { .. })
            | RegistryMessage::Effect(names_core::Effect::Register { .. }) => Some("register"),
            RegistryMessage::Instruction(Instruction::Renew { .. })
            | RegistryMessage::Effect(names_core::Effect::Renew { .. }) => Some("renew"),
            RegistryMessage::Instruction(Instruction::Claim { .. })
            | RegistryMessage::Effect(names_core::Effect::Claim { .. }) => Some("claim"),
            _ => Some("other registry change"),
        };
    }
    if Some(to) == board {
        return match borsh::from_slice::<BoardMessage>(message).ok()? {
            BoardMessage::Instruction(BoardInstruction::Post { .. })
            | BoardMessage::Effect(board_core::Effect::CreatePost(_)) => Some("post"),
            BoardMessage::Instruction(BoardInstruction::Like { .. })
            | BoardMessage::Effect(board_core::Effect::CreateLike(_)) => Some("like"),
            _ => None,
        };
    }
    None
}

/// A post as the feed shows it.
#[derive(serde::Serialize)]
pub struct FeedPost {
    /// The post's ID: the address of its account on the board.
    pub id: String,
    /// The ID of the post this answers.
    pub parent_id: Option<String>,
    pub post: String,
    pub name: String,
    pub n: u64,
    /// The post this answers, as `<name>/<n>` when its name is known, else
    /// `<author id hex>/<n>`.
    pub parent: Option<String>,
    pub time: u64,
    pub text: String,
    pub cids: Vec<String>,
    pub likes: i64,
    pub hidden: bool,
    pub block: u64,
}

/// The board changes earlier scans found, kept next to the wallet.
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct BoardCache {
    /// The last block read.
    last: u64,
    /// The first block read (0: unknown, from before this was kept).
    #[serde(default)]
    first: u64,
    events: Vec<BoardEvent>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct BoardEvent {
    block: u64,
    /// post, like, unlike or hide.
    kind: String,
    /// Hex of the post, like or post ID it is about.
    data: String,
    /// The like's account, for likes and unlikes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    account: Option<String>,
}

/// What a scan of recent blocks found on the board.
#[derive(Default)]
struct Scan {
    posts: Vec<(board_core::Post, u64)>,
    likes: std::collections::BTreeMap<(NameId, u64), i64>,
    hidden: std::collections::BTreeSet<(NameId, u64)>,
}

impl Client {
    /// Reads the last `blocks` blocks and picks out the board's changes. Private
    /// changes carry their effects on public accounts in the clear, so a post is
    /// read straight from the block that made it.
    async fn scan_board(&self, board: AccountId, blocks: u64) -> Result<Scan> {
        let last = self.wallet.get_last_block_id().await.context("cannot read the chain's height")?;
        let first = last.saturating_sub(blocks.saturating_sub(1)).max(1);
        // Blocks never change once in the chain: what earlier scans found is kept
        // next to the wallet, and only the blocks since are read.
        let path = fetch_persistent_storage_path()?.with_file_name(format!("board-{board}.json"));
        let mut cache: BoardCache = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        // A cache from a narrower or older scan is filled in: blocks before the
        // first it read, then blocks since the last.
        if cache.last < first || cache.first == 0 || cache.first > cache.last {
            cache = BoardCache::default();
        }
        if cache.last == 0 {
            cache.events = self.board_events(board, first, last).await?;
            cache.first = first;
            cache.last = last;
        } else {
            if first < cache.first {
                let found = self.board_events(board, first, cache.first - 1).await?;
                cache.events.extend(found);
                cache.first = first;
            }
            if cache.last < last {
                let found = self.board_events(board, cache.last + 1, last).await?;
                cache.events.extend(found);
                cache.last = last;
            }
        }
        // Keep a week of blocks at most.
        let keep_from = last.saturating_sub(60_000);
        cache.events.retain(|event| event.block >= keep_from);
        cache.first = cache.first.max(keep_from);
        if let Ok(text) = serde_json::to_string(&cache) {
            std::fs::write(&path, text).ok();
        }

        let mut scan = Scan::default();
        let mut liked: std::collections::BTreeMap<AccountId, (NameId, u64)> = std::collections::BTreeMap::new();
        let mut unliked = Vec::new();
        for event in cache.events.iter().filter(|event| event.block >= first) {
            let Ok(data) = hex::decode(&event.data) else { continue };
            match event.kind.as_str() {
                "post" => {
                    if let Ok(post) = borsh::from_slice::<board_core::Post>(&data) {
                        scan.posts.push((post, event.block));
                    }
                }
                "like" => {
                    if let (Ok(like), Some(account)) = (borsh::from_slice::<board_core::Like>(&data), event.account.as_deref().and_then(|a| a.parse().ok())) {
                        *scan.likes.entry((like.post.author, like.post.n)).or_default() += 1;
                        liked.insert(account, (like.post.author, like.post.n));
                    }
                }
                "unlike" => {
                    if let Some(account) = event.account.as_deref().and_then(|a| a.parse().ok()) {
                        unliked.push(account);
                    }
                }
                "hide" => {
                    if let Ok(post) = borsh::from_slice::<PostId>(&data) {
                        scan.hidden.insert((post.author, post.n));
                    }
                }
                _ => {}
            }
        }
        // An unlike names only the like's account; it counts against the post that
        // like was for, when the like was seen.
        for account in unliked {
            if let Some(post) = liked.get(&account) {
                *scan.likes.entry(*post).or_default() -= 1;
            }
        }
        Ok(scan)
    }

    /// Blocks `from..=to`, read 200 a call, several calls at once.
    async fn blocks(&self, from: u64, to: u64) -> Vec<Result<Vec<common::block::Block>>> {
        use futures::StreamExt as _;
        let client = self.wallet.client_owned();
        let ranges: Vec<(u64, u64)> = (from..=to).step_by(200).map(|start| (start, (start + 199).min(to))).collect();
        futures::stream::iter(ranges)
            .map(|(start, end)| {
                let client = client.clone();
                async move { client.get_block_range(start, end).await.map_err(|error| anyhow!("cannot read blocks {start}-{end}: {error}")) }
            })
            .buffered(8)
            .collect()
            .await
    }

    /// The board's changes in blocks `from..=to`.
    async fn board_events(&self, board: AccountId, from: u64, to: u64) -> Result<Vec<BoardEvent>> {
        let fetched = self.blocks(from, to).await;
        let mut events = Vec::new();
        for blocks in fetched {
            for block in blocks? {
                let block_id = block.header.block_id;
                for tx in &block.body.transactions {
                    self.events_of(board, block_id, tx, &mut events).await;
                }
            }
        }
        Ok(events)
    }

    /// Counts the registry's changes in the last `blocks` blocks, private and
    /// public: registrations, renewals, transfers, releases and posts.
    pub async fn stats(&self, blocks: u64) -> Result<()> {
        let last = self.wallet.get_last_block_id().await.context("cannot read the chain's height")?;
        let first = last.saturating_sub(blocks.saturating_sub(1)).max(1);
        let fetched = self.blocks(first, last).await;
        let board = board_program().ok();
        let mut counts: std::collections::BTreeMap<&str, (u64, u64)> = std::collections::BTreeMap::new();
        let mut txs = 0u64;
        for blocks in fetched {
            for block in blocks? {
                for tx in &block.body.transactions {
                    txs += 1;
                    match tx {
                        LeeTransaction::Public(tx) => {
                            let root = &tx.message().execution.root;
                            if let Some(kind) = kind_of(root.to.program_account_id, &root.message, self.program, board) {
                                counts.entry(kind).or_default().1 += 1;
                            }
                        }
                        LeeTransaction::PrivacyPreserving(tx) => {
                            let mut kinds = std::collections::BTreeSet::new();
                            for (to, message) in crossings(tx) {
                                if let Some(kind) = kind_of(to.program_account_id, message, self.program, board) {
                                    kinds.insert(kind);
                                }
                            }
                            for kind in kinds {
                                counts.entry(kind).or_default().0 += 1;
                            }
                        }
                    }
                }
            }
        }
        println!("blocks {first}-{last}: {txs} transactions on the chain");
        if counts.is_empty() {
            println!("no names changes");
        }
        for (kind, (private, public)) in counts {
            println!("{kind}: {} ({private} private, {public} public)", private + public);
        }
        Ok(())
    }

    /// The board changes one transaction made.
    async fn events_of(&self, board: AccountId, block: u64, tx: &LeeTransaction, out: &mut Vec<BoardEvent>) {
        let event = |kind: &str, data: Vec<u8>, account: Option<AccountId>| BoardEvent {
            block,
            kind: kind.to_owned(),
            data: hex::encode(data),
            account: account.map(|account| account.to_string()),
        };
        // A public change carries its instruction, not its effects: read what it
        // wrote from the post's account, which also says it applied.
        let tx = match tx {
            LeeTransaction::Public(tx) => {
                let root = &tx.message().execution.root;
                if root.to.program_account_id != board {
                    return;
                }
                match borsh::from_slice::<BoardMessage>(&root.message) {
                    Ok(BoardMessage::Instruction(BoardInstruction::Post { author, n, .. })) => {
                        let id = PostId { author, n };
                        if let Ok(bytes) = self.actor_state_of(board_core::post_address(&board, &id), board).await {
                            if !bytes.is_empty() {
                                out.push(event("post", bytes, None));
                            }
                        }
                    }
                    Ok(BoardMessage::Instruction(BoardInstruction::Like { liker, post, now })) => {
                        let like = board_core::Like { version: board_core::VERSION, post, liker, time: now };
                        out.push(event("like", borsh::to_vec(&like).unwrap_or_default(), Some(board_core::like_address(&board, &post, &liker))));
                    }
                    Ok(BoardMessage::Instruction(BoardInstruction::Unlike { liker, post })) => {
                        out.push(event("unlike", Vec::new(), Some(board_core::like_address(&board, &post, &liker))));
                    }
                    Ok(BoardMessage::Instruction(BoardInstruction::Hide { post })) => {
                        out.push(event("hide", borsh::to_vec(&post).unwrap_or_default(), None));
                    }
                    _ => {}
                }
                return;
            }
            LeeTransaction::PrivacyPreserving(tx) => tx,
        };
        // A private change's messages to public actors cross the boundary in the clear.
        for (to, message) in crossings(tx) {
            if to.program_account_id != board {
                continue;
            }
            match borsh::from_slice::<BoardMessage>(message) {
                Ok(BoardMessage::Effect(board_core::Effect::CreatePost(post))) => {
                    out.push(event("post", borsh::to_vec(&post).unwrap_or_default(), None));
                }
                Ok(BoardMessage::Effect(board_core::Effect::CreateLike(like))) => {
                    out.push(event("like", borsh::to_vec(&like).unwrap_or_default(), Some(to.account_id)));
                }
                Ok(BoardMessage::Effect(board_core::Effect::DeleteLike)) => {
                    out.push(event("unlike", Vec::new(), Some(to.account_id)));
                }
                Ok(BoardMessage::Effect(board_core::Effect::HidePost)) => {
                    if let Ok(bytes) = self.actor_state_of(to.account_id, board).await {
                        if let Ok(post) = borsh::from_slice::<board_core::Post>(&bytes) {
                            out.push(event("hide", borsh::to_vec(&post.id).unwrap_or_default(), None));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Checks each post's name against its author id (the program can't), and
    /// turns the scan into feed rows, newest first.
    fn feed_rows(&self, board: AccountId, scan: &Scan, keep: impl Fn(&board_core::Post) -> bool) -> Vec<FeedPost> {
        let mut verified: std::collections::BTreeMap<String, NameId> = std::collections::BTreeMap::new();
        let mut names: std::collections::BTreeMap<NameId, String> = std::collections::BTreeMap::new();
        let mut rows = Vec::new();
        for (post, block) in scan.posts.iter().rev() {
            let id = *verified.entry(post.name.clone()).or_insert_with(|| derive(&post.name).id);
            if id != post.id.author || !keep(post) {
                continue;
            }
            names.insert(id, post.name.clone());
            rows.push((post, *block));
        }
        rows.into_iter()
            .map(|(post, block)| {
                let key = (post.id.author, post.id.n);
                FeedPost {
                    id: board_core::post_address(&board, &post.id).to_string(),
                    parent_id: post.parent.map(|parent| board_core::post_address(&board, &parent).to_string()),
                    post: format!("{}/{}", post.name, post.id.n),
                    name: post.name.clone(),
                    n: post.id.n,
                    parent: post.parent.map(|parent| match names.get(&parent.author) {
                        Some(name) => format!("{name}/{}", parent.n),
                        None => format!("{}/{}", hex::encode(parent.author), parent.n),
                    }),
                    time: post.time,
                    text: post.text.clone(),
                    cids: post.cids.clone(),
                    likes: scan.likes.get(&key).copied().unwrap_or(0),
                    hidden: post.hidden || scan.hidden.contains(&key),
                    block,
                }
            })
            .collect()
    }

    /// The board's posts in the last `blocks` blocks, newest first; with
    /// `following`, only those names' posts.
    pub async fn feed(&self, blocks: u64, following: &[String]) -> Result<Vec<FeedPost>> {
        let board = board_program()?;
        let scan = self.scan_board(board, blocks).await?;
        let wanted: std::collections::BTreeSet<&str> = following.iter().map(String::as_str).collect();
        Ok(self.feed_rows(board, &scan, |post| wanted.is_empty() || wanted.contains(post.name.as_str())))
    }

    /// The post `post` and every reply under it found in the last `blocks` blocks,
    /// oldest first, so a thread reads top to bottom.
    pub async fn thread(&self, post: PostRef, blocks: u64) -> Result<Vec<FeedPost>> {
        let board = board_program()?;
        let (root, _, _) = self.find_post(board, &post).await?;
        let scan = self.scan_board(board, blocks).await?;
        let mut members = std::collections::BTreeSet::from([(root.author, root.n)]);
        // Replies come after their parents in chain order, so one pass in order finds them all.
        for (candidate, _) in &scan.posts {
            if let Some(parent) = candidate.parent {
                if members.contains(&(parent.author, parent.n)) {
                    members.insert((candidate.id.author, candidate.id.n));
                }
            }
        }
        let mut rows = self.feed_rows(board, &scan, |p| members.contains(&(p.id.author, p.id.n)));
        rows.reverse();
        Ok(rows)
    }
}

/// Prints posts as JSON for apps, or one line each for people.
pub fn print_posts(posts: &[FeedPost], json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string(posts)?);
        return Ok(());
    }
    if posts.is_empty() {
        println!("no posts found");
    }
    for post in posts.iter().filter(|post| !post.hidden) {
        let reply = post.parent.as_ref().map(|parent| format!(" (reply to {parent})")).unwrap_or_default();
        let likes = if post.likes > 0 { format!(" · {} like{}", post.likes, if post.likes == 1 { "" } else { "s" }) } else { String::new() };
        println!("{}  {}{reply}{likes}  (ID {})", post.name, readable(post.time), post.id);
        for line in post.text.lines() {
            println!("    {line}");
        }
        for cid in &post.cids {
            println!("    file: {cid}");
        }
    }
    Ok(())
}

/// Words for anonymous names: `anon-<word>-<trait>`. About 4,000 pairs; a taken
/// one gets a short number.
const ANON_WORDS: &[&str] = &[
    "quiet", "amber", "brisk", "calm", "dusky", "early", "faint", "gentle", "hollow", "idle", "jade", "keen",
    "lunar", "misty", "noble", "olive", "pale", "quick", "rusty", "silent", "tidal", "umber", "vivid", "wild",
    "young", "zesty", "azure", "bold", "coral", "dawn", "ember", "frost", "golden", "hazel", "ivory", "jolly",
    "kind", "lucky", "mellow", "night", "opal", "plain", "rapid", "sly", "tame", "upper", "velvet", "warm",
    "blue", "crisp", "deep", "far", "grey", "high", "inky", "low", "mild", "north", "odd", "proud", "red", "soft",
];
const ANON_TRAITS: &[&str] = &[
    "otter", "heron", "lynx", "moth", "raven", "fox", "wren", "pike", "hare", "owl", "stag", "newt", "crane",
    "bison", "finch", "gecko", "ibis", "koala", "lemur", "marten", "narwhal", "orca", "panda", "quail", "robin",
    "seal", "tapir", "urchin", "vole", "walrus", "yak", "zebra", "badger", "crow", "dingo", "egret", "ferret",
    "gull", "hyena", "jackal", "kiwi", "lark", "mole", "nightjar", "oriole", "puffin", "rook", "skink", "toad",
    "viper", "weasel", "swift", "tern", "eel", "ant", "bee", "carp", "dove", "elk", "frog", "goat", "hawk", "ram",
];

fn anon_candidate(attempt: u32) -> Result<String> {
    let mut random = [0u8; 4];
    getrandom::fill(&mut random).map_err(|e| anyhow!("no randomness: {e}"))?;
    let word = ANON_WORDS[usize::from(random[0]) % ANON_WORDS.len()];
    let trait_ = ANON_TRAITS[usize::from(random[1]) % ANON_TRAITS.len()];
    Ok(if attempt < 3 {
        format!("anon-{word}-{trait_}")
    } else {
        format!("anon-{word}-{trait_}-{}", u16::from_le_bytes([random[2], random[3]]) % 100)
    })
}

impl Client {
    /// Makes (once) and prints this wallet's anonymous name.
    pub async fn anon(&mut self, public: bool, rpc: Option<&str>) -> Result<()> {
        let defaults = load_defaults();
        if let Some(name) = defaults.anon.clone() {
            if defaults.pending.contains_key(&name) || self.lookup_held(&name).await.is_ok() {
                println!("{name}");
                return Ok(());
            }
        }
        // A free name, not one already held.
        let mut name = String::new();
        for attempt in 0..8 {
            let candidate = anon_candidate(attempt)?;
            if self.record(&derive(&candidate).id).await?.is_none() {
                name = candidate;
                break;
            }
        }
        if name.is_empty() {
            bail!("could not find a free anonymous name; try again");
        }
        let from = default_private()?;
        let id = derive(&name).id;
        let tier = length_proof(&id)?.1;
        let fee = self.registration_cost(tier, from, 1).await?.amount;
        let have = self.spendable(from).await?;
        if have < fee {
            bail!("your private account holds {have}, less than the {fee} deposit an anonymous name takes; add funds first");
        }
        // A fresh private account holds the name, funded privately: the chain shows no
        // link between it and the account that paid.
        let (account, _) = self.wallet.create_new_account_private(None);
        crate::save_wallet(&mut self.wallet)?;
        println!("{name}: making its private account and funding it privately (about 4 minutes)");
        let (hash, _) = NativeTokenTransfer(&self.wallet)
            .transfer(self.signer(from), self.signer(account), fee, CreditDelivery::Automatic)
            .await
            .context("the sequencer did not accept the transfer")?;
        let block = self.finish(hash).await.context("the transfer was not applied")?;
        println!("funded in block {block}");
        let mut defaults = load_defaults();
        defaults.anon = Some(name.clone());
        if public && !defaults.then_public.contains(&name) {
            defaults.then_public.push(name.clone());
        }
        save_defaults(&defaults)?;
        self.register(&name, account, 1, None, false, rpc).await?;
        let mut defaults = load_defaults();
        if !defaults.names.contains(&name) {
            defaults.names.push(name.clone());
        }
        defaults.anon = Some(name.clone());
        save_defaults(&defaults)?;
        println!("{name}");
        Ok(())
    }
}

/// How much a name made public gets for its posting gas: each public change holds
/// about 1.8M while it runs and pays a fraction of that.
pub(crate) const PUBLIC_GAS_FUND: u128 = 10_000_000;

impl Client {
    /// Moves `name` from the private account that holds it to a new public account
    /// in this wallet, so its posts take one block (and pay gas) instead of a
    /// proof. The new account is funded for gas from the private account; the
    /// name keeps its profile, page and private receiving keys.
    /// The private steps of making `name` public: funding a new public account for its gas from
    /// `gas_from`, then `holder` handing the name to it. Each at the fee a worst case allows:
    /// they run after one another, and a stop between them leaves the job half done.
    pub(crate) async fn public_steps(&self, name: &str, holder: AccountId, gas_from: AccountId) -> Result<Vec<plan::Step<AccountId>>> {
        let hand_over = crate::CHANGE_CYCLES;
        Ok(vec![
            plan::Step {
                what: format!("funding a new public account with {} for {name}{NAME_SUFFIX}'s gas", plan::grouped(PUBLIC_GAS_FUND)),
                acts_for: gas_from,
                moves_out: PUBLIC_GAS_FUND,
                credits: 0,
                payer: None,
                fee: self.private_fee(0).await?,
            },
            plan::Step {
                what: format!("handing {name}{NAME_SUFFIX} to that account"),
                acts_for: holder,
                moves_out: 0,
                credits: 0,
                payer: None,
                fee: self.private_fee(crate::gas_limit_for(hand_over)).await?,
            },
        ])
    }

    pub async fn make_public(&mut self, name: &str) -> Result<()> {
        let (Derived { id, .. }, record, profile) = self.lookup_held(name).await?;
        let holder = self.holder(name, &id, &record)?;
        if !self.is_private(holder) {
            bail!("{name}{NAME_SUFFIX} is already held by a public account");
        }
        let page = self.page(name).await?;
        // A public account an earlier try funded, and stopped before handing the name to, is
        // used again: funding another would spend its 10,000,000 and fee a second time.
        let mut funded = None;
        if let Some(public) = load_defaults().gas_accounts.get(name).and_then(|id| id.parse::<AccountId>().ok()) {
            // A read that failed is not an empty account: funding another would spend the
            // 10,000,000 and its fee a second time.
            if self.wallet.get_account_public_signing_key(public).is_some()
                && self
                    .wallet
                    .get_account_balance(public)
                    .await
                    .with_context(|| format!("could not read Public/{public}, which an earlier try funded for {name}{NAME_SUFFIX}'s gas, so nothing was sent"))?
                    >= PUBLIC_GAS_FUND / 2
            {
                funded = Some(public);
            }
        }
        // The gas comes from the holder if it can pay, else from the main private
        // account (an anon's own account holds only its deposit). Either way it
        // leaves the private pool, so the chain can't see which account paid.
        let gas_from = gas_payer(holder, self.spendable(holder).await?, default_private()?);
        let mut steps = self.public_steps(name, holder, gas_from).await?;
        if funded.is_some() {
            steps.remove(0);
        }
        // Both steps checked before the first, as the wallet will pay them: the funding moves
        // 10,000,000 out of its payer besides the fee reserve.
        let costed = self.plan_private(&format!("making {name}{NAME_SUFFIX} public"), &steps).await?;
        let public = match funded {
            Some(public) => {
                println!("step 1 of 3: Public/{public}, funded by an earlier try, already holds its gas");
                public
            }
            None => {
                let (public, _) = self.wallet.create_new_account_public(None);
                crate::save_wallet(&mut self.wallet)?;
                let mut defaults = load_defaults();
                defaults.gas_accounts.insert(name.to_owned(), public.to_string());
                save_defaults(&defaults)?;
                let payer = costed.payers.first().copied().unwrap_or(gas_from);
                println!("{}", crate::fees::estimate_line(&steps[0].fee /* the funding step: kept when nothing was funded before */, &format!("paid inside the proof, from Private/{payer}")));
                println!("step 1 of 3: funding Public/{public} with {PUBLIC_GAS_FUND} for gas, privately (about 4 minutes)");
                let before = self.private_wealth().await;
                let (hash, _) = NativeTokenTransfer(&self.wallet)
                    .transfer(self.signer(gas_from), AccountIdentity::Public(public), PUBLIC_GAS_FUND, CreditDelivery::Automatic)
                    .await
                    .map_err(|error| crate::refused_or(error, "the sequencer did not accept the transfer"))?;
                let (block, block_fee) = self.finish_landed(hash).await.context("the transfer was not applied")?;
                println!("funded in block {block}");
                let after = self.private_wealth().await;
                println!("{}", crate::private_fee_line(block_fee, crate::private_fee_paid(before.as_ref(), after.as_ref(), 0, PUBLIC_GAS_FUND)));
                public
            }
        };
        println!("step 2 of 3: moving {name}{NAME_SUFFIX} to it (about 4 minutes)");
        self.transfer(name, public).await.map_err(|error| {
            error.context(format!(
                "Public/{public} holds {name}{NAME_SUFFIX}'s gas, but the name was not handed to it; `names make-public {name}` tries again and uses that account"
            ))
        })?;
        let mut defaults = load_defaults();
        defaults.gas_accounts.remove(name);
        defaults.then_public.retain(|queued| queued != name);
        defaults.public_failed.remove(name);
        save_defaults(&defaults)?;
        // A transfer empties the profile and page; put them back as they were,
        // receiving keys included, now signed by the public holder (one block each).
        println!("step 3 of 3: putting back its profile and page");
        self.edit(name, |restored| {
            *restored = profile.clone();
            Ok(())
        })
        .await?;
        if let Some(page) = page {
            self.set_page(name, Some(page)).await?;
        }
        println!("{name}{NAME_SUFFIX} is now held by Public/{public}: posts take one block and pay gas");
        Ok(())
    }
}

impl Client {
    /// `count` posts by `name`, newest first, from `before` down (`None` is its
    /// newest), read straight from their accounts: the name's post counter says
    /// how many there are, and each post's address follows from its number.
    /// Likes come from what earlier feeds saw. Returns (total, posts).
    pub async fn posts_of(&self, name: &str, before: Option<u64>, count: u64) -> Result<(u64, Vec<FeedPost>)> {
        use futures::StreamExt as _;
        let board = board_program()?;
        let author = derive(name).id;
        let bytes = self.actor_state_of(board_core::author_address(&board, &author), board).await?;
        let total = if bytes.is_empty() { 0 } else { borsh::from_slice::<board_core::Author>(&bytes)?.count };
        let top = before.map_or(total, |before| before.min(total));
        let numbers: Vec<u64> = (top.saturating_sub(count)..top).rev().collect();
        let read: Vec<Option<board_core::Post>> = futures::stream::iter(numbers)
            .map(|n| async move {
                let id = PostId { author, n };
                let bytes = self.actor_state_of(board_core::post_address(&board, &id), board).await.ok()?;
                borsh::from_slice::<board_core::Post>(&bytes).ok()
            })
            .buffered(10)
            .collect()
            .await;
        // Likes as the local feed cache counted them: no network for that.
        let path = fetch_persistent_storage_path()?.with_file_name(format!("board-{board}.json"));
        let cache: BoardCache = std::fs::read_to_string(&path).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default();
        let mut likes: std::collections::BTreeMap<(NameId, u64), i64> = std::collections::BTreeMap::new();
        let mut liked: std::collections::BTreeMap<String, (NameId, u64)> = std::collections::BTreeMap::new();
        for event in &cache.events {
            match event.kind.as_str() {
                "like" => {
                    if let Ok(like) = hex::decode(&event.data).map_err(|_| ()).and_then(|data| borsh::from_slice::<board_core::Like>(&data).map_err(|_| ())) {
                        *likes.entry((like.post.author, like.post.n)).or_default() += 1;
                        if let Some(account) = &event.account {
                            liked.insert(account.clone(), (like.post.author, like.post.n));
                        }
                    }
                }
                "unlike" => {
                    if let Some(post) = event.account.as_ref().and_then(|account| liked.get(account)) {
                        *likes.entry(*post).or_default() -= 1;
                    }
                }
                _ => {}
            }
        }
        let posts = read
            .into_iter()
            .flatten()
            .filter(|post| post.name == name)
            .map(|post| FeedPost {
                id: board_core::post_address(&board, &post.id).to_string(),
                parent_id: post.parent.map(|parent| board_core::post_address(&board, &parent).to_string()),
                post: format!("{}/{}", post.name, post.id.n),
                name: post.name.clone(),
                n: post.id.n,
                parent: post.parent.map(|parent| format!("{}/{}", hex::encode(parent.author), parent.n)),
                time: post.time,
                text: post.text.clone(),
                cids: post.cids.clone(),
                likes: likes.get(&(post.id.author, post.id.n)).copied().unwrap_or(0),
                hidden: post.hidden,
                block: 0,
            })
            .collect();
        Ok((total, posts))
    }
}
