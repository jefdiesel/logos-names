//! `names post`, `names like`, `names unlike`, `names hide`: changes to the LNS
//! board, made as a name. The Basecamp Board app runs these; people rarely type
//! them. Every change is made from the private account that holds the name, so
//! it is proved on this machine and the chain sees the name, never the account.

use super::*;
use board_core::{Instruction as BoardInstruction, PostId, REGISTRY};

/// The gas limit of a public board call.
const BOARD_GAS_LIMIT: u64 = 200_000;

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

/// One account a board change names: which program's shard of it, and whether it signs.
struct Row {
    account: AccountId,
    shard: AccountId,
    signs: bool,
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
        let bytes = self.board_shard(board, address).await?;
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
        let (Derived { id, .. }, record, _) = self.lookup_held(name).await?;
        Ok((id, self.holder(name, &id, &record)?))
    }

    async fn board_shard(&self, board: AccountId, account: AccountId) -> Result<Vec<u8>> {
        let view = self
            .wallet
            .get_account_view(ProgramShardSelector::new(account, board))
            .await
            .context("failed to read the board from the sequencer")?;
        Ok(view.data.shard(board).to_vec())
    }

    /// Sends `instruction` to the board as a privacy-preserving transaction signed
    /// by the private account in `rows`, and waits for it to be applied.
    async fn send_board(&mut self, board: AccountId, rows: Vec<Row>, instruction: &BoardInstruction) -> Result<()> {
        let program = self
            .program_at(board, "LOGOS_BOARD_PROGRAM_ELF", "logos-board", "the board")
            .await?;
        let public = rows.iter().any(|row| row.signs && !self.is_private(row.account));
        let mut signers = Vec::new();
        let mut mentions = Vec::new();
        for row in &rows {
            let identity = if !row.signs {
                AccountIdentity::PublicNoSign(row.account)
            } else if public {
                AccountIdentity::Public(row.account)
            } else {
                signers.push(row.account);
                self.wallet
                    .resolve_private_account(row.account)
                    .ok_or_else(|| anyhow!("{} is not a private account in this wallet", row.account))?
            };
            mentions.push(identity.select_program_shard(row.shard));
        }
        if std::env::var_os("LOGOS_NAMES_DRY_RUN").is_some() {
            bail!("LOGOS_NAMES_DRY_RUN is set, so nothing was sent");
        }
        if public {
            // A board call runs about as many cycles as a registry edit; this limit
            // covers it with room, and a failed call is billed the whole limit.
            let saved = self.wallet.config().clone();
            let mut config = saved.clone();
            config.gas_limit = BOARD_GAS_LIMIT;
            self.wallet.set_config(config);
            println!("posting publicly; one block, paid in LEZ gas");
            let before = self.wallet.get_account_balance(rows.iter().find(|row| row.signs).map(|row| row.account).unwrap_or(board)).await.unwrap_or(0);
            let sent = self.wallet.send_pub_tx(mentions, borsh::to_vec(instruction)?, board).await;
            self.wallet.set_config(saved);
            let hash = sent.context("the sequencer did not accept the transaction")?;
            println!("submitted {hash}");
            let (_, block) = self.wallet.poll_transaction(hash).await.context("the transaction was not included in a block")?;
            let after = self.wallet.get_account_balance(rows.iter().find(|row| row.signs).map(|row| row.account).unwrap_or(board)).await.unwrap_or(0);
            println!("included in block {block}; gas paid {}", before.saturating_sub(after));
            return Ok(());
        }
        println!("acting privately; proving it on this computer (about 4 minutes), so the chain never learns your account");
        let program = ProgramWithDependencies::new(program, board, Default::default());
        let (hash, secrets) = self
            .wallet
            .send_privacy_preserving_tx(mentions, borsh::to_vec(instruction)?, &program)
            .await
            .context("the sequencer did not accept the transaction")?;
        println!("submitted {hash}");
        let (tx, block) = self
            .wallet
            .poll_transaction(hash)
            .await
            .context("the transaction was not included in a block")?;
        if let common::transaction::LeeTransaction::PrivacyPreserving(tx) = &tx {
            let decode: Vec<AccDecodeData> = secrets
                .into_iter()
                .zip(signers)
                .map(|(secret, account)| AccDecodeData::Decode(secret, account))
                .collect();
            quietly(|| self.wallet.decode_insert_privacy_preserving_transaction_results(tx, &decode))?;
            quietly(|| self.wallet.store_persistent_data())?;
        }
        println!("included in block {block}");
        Ok(())
    }

    /// Posts `text` as `name`, starting a thread or, with `parent`, replying.
    /// Prints the new post as `<name>/<n>`.
    pub async fn post(&mut self, name: &str, text: String, cids: Vec<String>, parent: Option<PostRef>) -> Result<()> {
        let board = board_program()?;
        let (author, signer) = self.private_holder(name).await?;
        let counter = board_core::author_address(&board, &author);
        let bytes = self.board_shard(board, counter).await?;
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
        let rows = vec![
            Row { account: record_address(&REGISTRY, &author), shard: REGISTRY, signs: false },
            Row { account: counter, shard: board, signs: false },
            Row { account: post, shard: board, signs: false },
            Row { account: signer, shard: board, signs: true },
        ];
        self.send_board(board, rows, &instruction).await?;
        if self.board_shard(board, post).await?.is_empty() {
            bail!("the post was included but not written; it failed on chain");
        }
        println!("posted {name}/{n}, ID {post}");
        Ok(())
    }

    /// Likes (or, with `undo`, unlikes) the post `post` as `name`.
    pub async fn like(&mut self, name: &str, post: PostRef, undo: bool) -> Result<()> {
        let board = board_program()?;
        let (liker, signer) = self.private_holder(name).await?;
        let (id, _, address) = self.find_post(board, &post).await?;
        let like = board_core::like_address(&board, &id, &liker);
        let record = Row { account: record_address(&REGISTRY, &liker), shard: REGISTRY, signs: false };
        let signer = Row { account: signer, shard: board, signs: true };
        let like_row = Row { account: like, shard: board, signs: false };
        if undo {
            self.send_board(board, vec![record, like_row, signer], &BoardInstruction::Unlike { liker, post: id }).await?;
        } else {
            let post_row = Row { account: board_core::post_address(&board, &id), shard: board, signs: false };
            let now = self.now().await;
            self.send_board(board, vec![record, like_row, post_row, signer], &BoardInstruction::Like { liker, post: id, now })
                .await?;
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
        let rows = vec![
            Row { account: record_address(&REGISTRY, &author), shard: REGISTRY, signs: false },
            Row { account: board_core::post_address(&board, &id), shard: board, signs: false },
            Row { account: signer, shard: board, signs: true },
        ];
        self.send_board(board, rows, &BoardInstruction::Hide { post: id }).await?;
        println!("hid {address}");
        Ok(())
    }
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

/// Blocks `start..=end` from the sequencer's `getBlockRange`.
fn block_range(url: &str, start: u64, end: u64) -> Result<Vec<common::block::Block>> {
    use base64::Engine as _;
    let reply = super::rpc_call(url, "getBlockRange", serde_json::json!([start, end]))?;
    let list = reply.as_array().ok_or_else(|| anyhow!("blocks {start}-{end} did not come back as a list"))?;
    list.iter()
        .map(|item| {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(item.as_str().unwrap_or_default())
                .map_err(|error| anyhow!("a block is not base64: {error}"))?;
            borsh::from_slice::<common::block::Block>(&bytes).map_err(|error| anyhow!("a block does not decode: {error}"))
        })
        .collect()
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
        let from = if cache.last >= first { cache.last + 1 } else { first };
        if from <= last {
            let found = self.board_events(board, from, last).await?;
            cache.events.extend(found);
            cache.last = last;
        }
        // Keep a week of blocks at most.
        let keep_from = last.saturating_sub(60_000);
        cache.events.retain(|event| event.block >= keep_from);
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

    /// The board's changes in blocks `from..=to`, read 200 blocks a call, several
    /// calls at once.
    async fn board_events(&self, board: AccountId, from: u64, to: u64) -> Result<Vec<BoardEvent>> {
        use futures::StreamExt as _;
        let url = self.wallet.config().sequencer.sequencer_addr.to_string();
        let ranges: Vec<(u64, u64)> = (from..=to).step_by(200).map(|start| (start, (start + 199).min(to))).collect();
        let fetched: Vec<Result<Vec<common::block::Block>>> = futures::stream::iter(ranges)
            .map(|(start, end)| {
                let url = url.clone();
                async move {
                    tokio::task::spawn_blocking(move || block_range(&url, start, end))
                        .await
                        .map_err(|error| anyhow!("{error}"))?
                }
            })
            .buffered(8)
            .collect()
            .await;
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

    /// The board changes one transaction made.
    async fn events_of(&self, board: AccountId, block: u64, tx: &common::transaction::LeeTransaction, out: &mut Vec<BoardEvent>) {
        let event = |kind: &str, data: Vec<u8>, account: Option<AccountId>| BoardEvent {
            block,
            kind: kind.to_owned(),
            data: hex::encode(data),
            account: account.map(|account| account.to_string()),
        };
        // A public change carries its instruction, not its effects: read what it
        // wrote from the post's account, which also says it applied.
        if let common::transaction::LeeTransaction::Public(tx) = tx {
            if tx.message.program_account_id != board {
                return;
            }
            match borsh::from_slice::<BoardInstruction>(&tx.message.instruction_data) {
                Ok(BoardInstruction::Post { author, n, .. }) => {
                    let id = PostId { author, n };
                    if let Ok(bytes) = self.board_shard(board, board_core::post_address(&board, &id)).await {
                        if !bytes.is_empty() {
                            out.push(event("post", bytes, None));
                        }
                    }
                }
                Ok(BoardInstruction::Like { liker, post, now }) => {
                    let like = board_core::Like { version: board_core::VERSION, post, liker, time: now };
                    out.push(event("like", borsh::to_vec(&like).unwrap_or_default(), Some(board_core::like_address(&board, &post, &liker))));
                }
                Ok(BoardInstruction::Unlike { liker, post }) => {
                    out.push(event("unlike", Vec::new(), Some(board_core::like_address(&board, &post, &liker))));
                }
                Ok(BoardInstruction::Hide { post }) => out.push(event("hide", borsh::to_vec(&post).unwrap_or_default(), None)),
                _ => {}
            }
            return;
        }
        let common::transaction::LeeTransaction::PrivacyPreserving(tx) = tx else { return };
        for action in &tx.message.public_actions {
            for effect in &action.effects {
                if effect.program_account_id != board || effect.shard_program_account_id != board {
                    continue;
                }
                match borsh::from_slice::<board_core::Effect>(&effect.data) {
                    Ok(board_core::Effect::CreatePost(post)) => out.push(event("post", borsh::to_vec(&post).unwrap_or_default(), None)),
                    Ok(board_core::Effect::CreateLike(like)) => {
                        out.push(event("like", borsh::to_vec(&like).unwrap_or_default(), Some(action.account_id)));
                    }
                    Ok(board_core::Effect::DeleteLike) => out.push(event("unlike", Vec::new(), Some(action.account_id))),
                    Ok(board_core::Effect::HidePost) => {
                        if let Ok(bytes) = self.board_shard(board, action.account_id).await {
                            if let Ok(post) = borsh::from_slice::<board_core::Post>(&bytes) {
                                out.push(event("hide", borsh::to_vec(&post.id).unwrap_or_default(), None));
                            }
                        }
                    }
                    _ => {}
                }
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
        let (fee, _, _) = self.registration_cost(&id, tier, from, 1).await?;
        let have = self.spendable(from).await;
        if have < fee {
            bail!("your private account holds {have}, less than the {fee} deposit an anonymous name takes; add funds first");
        }
        // A fresh private account holds the name, funded privately: the chain shows no
        // link between it and the account that paid.
        let (account, _) = self.wallet.create_new_account_private(None);
        quietly(|| self.wallet.store_persistent_data())?;
        println!("{name}: making its private account and funding it privately (about 4 minutes)");
        let (hash, [from_secret, to_secret]) = NativeTokenTransfer(&self.wallet)
            .send_private_transfer_to_owned_account(from, account, fee)
            .await
            .context("the sequencer did not accept the transfer")?;
        let (tx, block) = self.wallet.poll_transaction(hash).await.context("the transfer was not included in a block")?;
        if let common::transaction::LeeTransaction::PrivacyPreserving(tx) = &tx {
            quietly(|| {
                self.wallet.decode_insert_privacy_preserving_transaction_results(
                    tx,
                    &[AccDecodeData::Decode(from_secret, from), AccDecodeData::Decode(to_secret, account)],
                )
            })?;
            quietly(|| self.wallet.store_persistent_data())?;
        }
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
const PUBLIC_GAS_FUND: u128 = 10_000_000;

impl Client {
    /// Moves `name` from the private account that holds it to a new public account
    /// in this wallet, so its posts take one block (and pay gas) instead of a
    /// proof. The new account is funded for gas from the private account; the
    /// name keeps its profile, page and private receiving keys.
    pub async fn make_public(&mut self, name: &str) -> Result<()> {
        let (Derived { id, .. }, record, profile) = self.lookup_held(name).await?;
        let holder = self.holder(name, &id, &record)?;
        if !self.is_private(holder) {
            bail!("{name}{NAME_SUFFIX} is already held by a public account");
        }
        let page = self.page(name).await?;
        // The gas comes from the holder if it can pay, else from the main private
        // account (an anon's own account holds only its deposit). Either way it
        // leaves the private pool, so the chain can't see which account paid.
        let payer = if self.spendable(holder).await >= PUBLIC_GAS_FUND { holder } else { default_private()? };
        if self.spendable(payer).await < PUBLIC_GAS_FUND {
            bail!("your private account holds less than the {PUBLIC_GAS_FUND} a public name needs for gas; add funds first");
        }
        let (public, _) = self.wallet.create_new_account_public(None);
        quietly(|| self.wallet.store_persistent_data())?;
        println!("step 1 of 3: funding Public/{public} with {PUBLIC_GAS_FUND} for gas, privately (about 4 minutes)");
        let (hash, secret) = NativeTokenTransfer(&self.wallet)
            .send_deshielded_transfer(payer, public, PUBLIC_GAS_FUND)
            .await
            .context("the sequencer did not accept the transfer")?;
        let (tx, block) = self.wallet.poll_transaction(hash).await.context("the transfer was not included in a block")?;
        if let common::transaction::LeeTransaction::PrivacyPreserving(tx) = &tx {
            quietly(|| self.wallet.decode_insert_privacy_preserving_transaction_results(tx, &[AccDecodeData::Decode(secret, payer)]))?;
            quietly(|| self.wallet.store_persistent_data())?;
        }
        println!("funded in block {block}");
        println!("step 2 of 3: moving {name}{NAME_SUFFIX} to it (about 4 minutes)");
        self.transfer(name, public).await?;
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
