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

/// `<name>/<n>`, as `names post` prints a post.
pub fn parse_post(input: &str) -> Result<(String, u64), String> {
    let (name, n) = input.rsplit_once('/').ok_or("a post is written <name>/<number>, such as jef/0")?;
    let name = parse_name(name)?;
    let n = n.parse().map_err(|_| format!("not a post number: {n}"))?;
    Ok((name, n))
}

/// One account a board change names: which program's shard of it, and whether it signs.
struct Row {
    account: AccountId,
    shard: AccountId,
    signs: bool,
}

impl Client {
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
    pub async fn post(&mut self, name: &str, text: String, cids: Vec<String>, parent: Option<(String, u64)>) -> Result<()> {
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
            Some((parent_name, parent_n)) => {
                let id = PostId { author: derive(&parent_name).id, n: parent_n };
                if self.board_shard(board, board_core::post_address(&board, &id)).await?.is_empty() {
                    bail!("there is no post {parent_name}/{parent_n} to reply to");
                }
                Some(id)
            }
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
        println!("posted {name}/{n}");
        Ok(())
    }

    /// Likes (or, with `undo`, unlikes) the post `post` as `name`.
    pub async fn like(&mut self, name: &str, post: (String, u64), undo: bool) -> Result<()> {
        let board = board_program()?;
        let (liker, signer) = self.private_holder(name).await?;
        let id = PostId { author: derive(&post.0).id, n: post.1 };
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
        println!("{} {}/{}", if undo { "unliked" } else { "liked" }, post.0, post.1);
        Ok(())
    }

    /// Takes down the post `post`, which `post.0` must be held by this wallet.
    pub async fn hide(&mut self, post: (String, u64)) -> Result<()> {
        let board = board_program()?;
        let (author, signer) = self.private_holder(&post.0).await?;
        let id = PostId { author, n: post.1 };
        let rows = vec![
            Row { account: record_address(&REGISTRY, &author), shard: REGISTRY, signs: false },
            Row { account: board_core::post_address(&board, &id), shard: board, signs: false },
            Row { account: signer, shard: board, signs: true },
        ];
        self.send_board(board, rows, &BoardInstruction::Hide { post: id }).await?;
        println!("hid {}/{}", post.0, post.1);
        Ok(())
    }
}

/// A post as the feed shows it.
#[derive(serde::Serialize)]
pub struct FeedPost {
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
        use futures::StreamExt as _;
        let last = self.wallet.get_last_block_id().await.context("cannot read the chain's height")?;
        let first = last.saturating_sub(blocks.saturating_sub(1)).max(1);
        let fetched: Vec<_> = futures::stream::iter(first..=last)
            .map(|id| async move { (id, self.wallet.get_block(id).await) })
            .buffered(24)
            .collect()
            .await;
        let mut scan = Scan::default();
        let mut liked: std::collections::BTreeMap<AccountId, (NameId, u64)> = std::collections::BTreeMap::new();
        let mut unliked = Vec::new();
        for (block_id, block) in fetched {
            let Ok(Some(block)) = block else { continue };
            for tx in &block.body.transactions {
                let common::transaction::LeeTransaction::PrivacyPreserving(tx) = tx else { continue };
                for action in &tx.message.public_actions {
                    for effect in &action.effects {
                        if effect.program_account_id != board || effect.shard_program_account_id != board {
                            continue;
                        }
                        match borsh::from_slice::<board_core::Effect>(&effect.data) {
                            Ok(board_core::Effect::CreatePost(post)) => scan.posts.push((post, block_id)),
                            Ok(board_core::Effect::CreateLike(like)) => {
                                *scan.likes.entry((like.post.author, like.post.n)).or_default() += 1;
                                liked.insert(action.account_id, (like.post.author, like.post.n));
                            }
                            Ok(board_core::Effect::DeleteLike) => unliked.push(action.account_id),
                            Ok(board_core::Effect::HidePost) => {
                                if let Some((post, _)) = scan.posts.iter().find(|(p, _)| board_core::post_address(&board, &p.id) == action.account_id) {
                                    scan.hidden.insert((post.id.author, post.id.n));
                                }
                            }
                            _ => {}
                        }
                    }
                }
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

    /// Checks each post's name against its author id (the program can't), and
    /// turns the scan into feed rows, newest first.
    fn feed_rows(&self, scan: &Scan, keep: impl Fn(&board_core::Post) -> bool) -> Vec<FeedPost> {
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
        Ok(self.feed_rows(&scan, |post| wanted.is_empty() || wanted.contains(post.name.as_str())))
    }

    /// The post `post` and every reply under it found in the last `blocks` blocks,
    /// oldest first, so a thread reads top to bottom.
    pub async fn thread(&self, post: (String, u64), blocks: u64) -> Result<Vec<FeedPost>> {
        let board = board_program()?;
        let scan = self.scan_board(board, blocks).await?;
        let root = PostId { author: derive(&post.0).id, n: post.1 };
        let mut members = std::collections::BTreeSet::from([(root.author, root.n)]);
        // Replies come after their parents in chain order, so one pass in order finds them all.
        for (candidate, _) in &scan.posts {
            if let Some(parent) = candidate.parent {
                if members.contains(&(parent.author, parent.n)) {
                    members.insert((candidate.id.author, candidate.id.n));
                }
            }
        }
        let mut rows = self.feed_rows(&scan, |p| members.contains(&(p.id.author, p.id.n)));
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
        println!("{}  {}{reply}{likes}", post.post, readable(post.time));
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
    pub async fn anon(&mut self, rpc: Option<&str>) -> Result<()> {
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
        let (public, _) = self.wallet.create_new_account_public(None);
        quietly(|| self.wallet.store_persistent_data())?;
        println!("step 1 of 3: funding Public/{public} with {PUBLIC_GAS_FUND} for gas, privately (about 4 minutes)");
        let (hash, secret) = NativeTokenTransfer(&self.wallet)
            .send_deshielded_transfer(holder, public, PUBLIC_GAS_FUND)
            .await
            .context("the sequencer did not accept the transfer")?;
        let (tx, block) = self.wallet.poll_transaction(hash).await.context("the transfer was not included in a block")?;
        if let common::transaction::LeeTransaction::PrivacyPreserving(tx) = &tx {
            quietly(|| self.wallet.decode_insert_privacy_preserving_transaction_results(tx, &[AccDecodeData::Decode(secret, holder)]))?;
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
