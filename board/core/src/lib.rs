//! The LNS board: posts, threaded replies, likes and follows, signed by names.
//!
//! Like the registry, all logic is here so it can be tested on the host; the
//! guest only wires [`plan`] and [`apply`] to the LEZ runtime.
//!
//! Every change is made as a name. The board never learns which account holds
//! the name: it computes the signer's owner tag for the name and, through
//! [`Plan::inspect`], asks the registry's own record shard whether that tag is
//! the holder's. A holder acting from a private account passes this check
//! inside a proof, so the chain sees "a post by this name" and nothing more.
//!
//! Nothing here touches an account another name writes: a post updates only
//! its author's counter, a like only its own account. So a private change,
//! proved against the state at the start of its proof, never lands stale
//! because someone else posted meanwhile.
//!
//! Accounts, all PDAs of the board program:
//! - `author/<name id>`: [`Author`], the name's post count and last post time.
//! - `post/<name id>/<n>`: [`Post`].
//! - `like/<post>/<liker id>`: [`Like`], present while the post is liked.
//! - `follows/<name id>`: the name's follow list, sealed under a key only the
//!   name's holder has, so nobody else learns who it follows.

use borsh::{BorshDeserialize, BorshSerialize};
use lee_core::{
    account::AccountId,
    program::{AccountMeta, PdaSeed, Plan, PlanInput},
};
use names_core::{NameId, NameRecord, OwnerTag, owner_tag, record_address, validate_name};

/// The names registry the board trusts, registry v5 on the LEZ testnet
/// (`38jEfVV1RCyMzu1FGbYcCn5FqxCxKyY58VgzjruAh2gq`). Pinned, so nobody can
/// pass a look-alike registry that says they hold a name.
pub const REGISTRY: AccountId = AccountId::new([
    31, 178, 50, 8, 70, 127, 182, 98, 58, 220, 183, 147, 107, 133, 219, 64, 244, 34, 187, 144, 254,
    233, 13, 197, 18, 202, 225, 4, 100, 224, 136, 186,
]);

/// Layout version of every account the board writes.
pub const VERSION: u8 = 1;
/// The most text a post holds, in bytes.
pub const MAX_TEXT: usize = 1024;
/// The most Logos Storage CIDs a post carries.
pub const MAX_CIDS: usize = 4;
/// The longest CID accepted.
pub const MAX_CID_LEN: usize = 100;
/// The most a sealed follow list holds, in bytes.
pub const MAX_FOLLOWS: usize = 16 * 1024;
/// The least time between two posts by one name.
pub const MIN_POST_GAP_MS: u64 = 10_000;
/// How far a claimed time may run ahead of the block's: room for a private
/// holder's proof, as in the registry.
pub const TIME_WINDOW_MS: u64 = 10 * 60 * 1000;

const AUTHOR_DOMAIN: &[u8] = b"logos-board/v1/author/";
const POST_DOMAIN: &[u8] = b"logos-board/v1/post/";
const LIKE_DOMAIN: &[u8] = b"logos-board/v1/like/";
const FOLLOWS_DOMAIN: &[u8] = b"logos-board/v1/follows/";

/// A post is named by its author and its number among the author's posts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PostId {
    pub author: NameId,
    pub n: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Author {
    pub version: u8,
    /// How many posts the name has made; the next one is number `count`.
    pub count: u64,
    /// When its last post was made, in milliseconds.
    pub last: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Post {
    pub version: u8,
    pub id: PostId,
    /// The author's name as written, bare. The program can't derive a name's
    /// identifier (Argon2id, 64 MiB), so a reader checks that this name
    /// derives to `id.author` before showing it.
    pub name: String,
    /// The post this one answers; `None` starts a thread.
    pub parent: Option<PostId>,
    pub time: u64,
    pub text: String,
    pub cids: Vec<String>,
    /// Set by the author to take the post down. Its text stays in the chain's
    /// history; clients stop showing it.
    pub hidden: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Like {
    pub version: u8,
    pub post: PostId,
    pub liker: NameId,
    pub time: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Instruction {
    /// Accounts: `[record, author, post, signer]`.
    Post {
        author: NameId,
        name: String,
        n: u64,
        parent: Option<PostId>,
        text: String,
        cids: Vec<String>,
        now: u64,
    },
    /// Accounts: `[record, post, signer]`.
    Hide { post: PostId },
    /// Accounts: `[record, like, post, signer]`; `record` is the liker's.
    Like { liker: NameId, post: PostId, now: u64 },
    /// Accounts: `[record, like, signer]`.
    Unlike { liker: NameId, post: PostId },
    /// Accounts: `[record, follows, signer]`.
    SetFollows { follower: NameId, sealed: Vec<u8> },
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Effect {
    /// On the registry's record shard, read only: the name is held by `tag`.
    HeldBy { tag: OwnerTag },
    /// On the author account: post number `n` at `now` is the next one.
    NextPost { n: u64, now: u64 },
    /// On a post account that must be empty.
    CreatePost(Post),
    /// On a post account: take it down.
    HidePost,
    /// On a post account, read only: it exists and isn't hidden.
    PostLive,
    /// On a like account that must be empty.
    CreateLike(Like),
    /// On a like account: remove it.
    DeleteLike,
    /// On a follows account.
    WriteFollows { sealed: Vec<u8> },
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    use risc0_zkvm::sha::{Impl, Sha256 as _};
    let digest = Impl::hash_bytes(bytes);
    let mut out = [0u8; 32];
    out.copy_from_slice(digest.as_bytes());
    out
}

fn seed(parts: &[&[u8]]) -> PdaSeed {
    PdaSeed::new(sha256(&parts.concat()))
}

#[must_use]
pub fn author_address(board: &AccountId, author: &NameId) -> AccountId {
    AccountId::for_public_pda(board, &seed(&[AUTHOR_DOMAIN, author]))
}

#[must_use]
pub fn post_address(board: &AccountId, post: &PostId) -> AccountId {
    AccountId::for_public_pda(board, &seed(&[POST_DOMAIN, &post.author, &post.n.to_le_bytes()]))
}

#[must_use]
pub fn like_address(board: &AccountId, post: &PostId, liker: &NameId) -> AccountId {
    AccountId::for_public_pda(board, &seed(&[LIKE_DOMAIN, &post.author, &post.n.to_le_bytes(), liker]))
}

#[must_use]
pub fn follows_address(board: &AccountId, follower: &NameId) -> AccountId {
    AccountId::for_public_pda(board, &seed(&[FOLLOWS_DOMAIN, follower]))
}

fn accounts<const N: usize>(input: &PlanInput) -> &[AccountMeta; N] {
    <&[AccountMeta; N]>::try_from(input.accounts.as_slice())
        .unwrap_or_else(|_| panic!("expected {N} input accounts, got {}", input.accounts.len()))
}

fn require(account: &AccountMeta, expected: AccountId, what: &str) {
    assert!(account.account_id == expected, "the {what} account does not match");
}

/// Checks that `record` is the registry's record of `name` and that `signer`,
/// which must sign, holds it: the check runs on the registry's shard.
fn held(plan: &mut Plan, record: &AccountMeta, signer: &AccountMeta, name: &NameId) {
    assert!(signer.is_authorized, "the signer must sign");
    assert!(record.program_account_id == REGISTRY, "the record must be the registry's");
    require(record, record_address(&REGISTRY, name), "record");
    plan.inspect(record, REGISTRY, &Effect::HeldBy { tag: owner_tag(name, &signer.account_id) });
}

fn time_window(plan: &mut Plan, now: u64) {
    let window = lee_core::program::TimestampValidityWindow::try_from((
        Some(now),
        Some(now.saturating_add(TIME_WINDOW_MS)),
    ))
    .expect("a window of positive length");
    plan.timestamp_window(window);
}

fn check_cid(cid: &str) {
    assert!(
        !cid.is_empty() && cid.len() <= MAX_CID_LEN && cid.bytes().all(|b| b.is_ascii_alphanumeric()),
        "a CID is 1-{MAX_CID_LEN} letters and digits"
    );
}

#[must_use]
pub fn plan(input: &PlanInput, instruction: Instruction) -> Plan {
    let mut plan = Plan::new(input);
    let board = &input.self_account_id;
    match instruction {
        Instruction::Post { author, name, n, parent, text, cids, now } => {
            let [record, counter, post, signer] = accounts::<4>(input);
            validate_name(&name).unwrap_or_else(|reason| panic!("{reason}"));
            assert!(!text.trim().is_empty() || !cids.is_empty(), "a post needs text or a file");
            assert!(text.len() <= MAX_TEXT, "a post holds at most {MAX_TEXT} bytes of text");
            assert!(cids.len() <= MAX_CIDS, "a post carries at most {MAX_CIDS} files");
            cids.iter().for_each(|cid| check_cid(cid));
            let id = PostId { author, n };
            require(counter, author_address(board, &author), "author");
            require(post, post_address(board, &id), "post");
            held(&mut plan, record, signer, &author);
            time_window(&mut plan, now);
            plan.effect(counter, &Effect::NextPost { n, now });
            plan.effect(
                post,
                &Effect::CreatePost(Post {
                    version: VERSION,
                    id,
                    name,
                    parent,
                    time: now,
                    text,
                    cids,
                    hidden: false,
                }),
            );
        }
        Instruction::Hide { post: id } => {
            let [record, post, signer] = accounts::<3>(input);
            require(post, post_address(board, &id), "post");
            held(&mut plan, record, signer, &id.author);
            plan.effect(post, &Effect::HidePost);
        }
        Instruction::Like { liker, post: id, now } => {
            let [record, like, post, signer] = accounts::<4>(input);
            require(like, like_address(board, &id, &liker), "like");
            require(post, post_address(board, &id), "post");
            held(&mut plan, record, signer, &liker);
            time_window(&mut plan, now);
            plan.effect(post, &Effect::PostLive);
            plan.effect(
                like,
                &Effect::CreateLike(Like { version: VERSION, post: id, liker, time: now }),
            );
        }
        Instruction::Unlike { liker, post: id } => {
            let [record, like, signer] = accounts::<3>(input);
            require(like, like_address(board, &id, &liker), "like");
            held(&mut plan, record, signer, &liker);
            plan.effect(like, &Effect::DeleteLike);
        }
        Instruction::SetFollows { follower, sealed } => {
            let [record, follows, signer] = accounts::<3>(input);
            assert!(sealed.len() <= MAX_FOLLOWS, "a follow list holds at most {MAX_FOLLOWS} bytes");
            require(follows, follows_address(board, &follower), "follows");
            held(&mut plan, record, signer, &follower);
            plan.effect(follows, &Effect::WriteFollows { sealed });
        }
    }
    plan
}

fn to_bytes<T: BorshSerialize>(value: &T) -> Vec<u8> {
    borsh::to_vec(value).expect("borsh serialization is infallible")
}

/// Applies one effect to the shard it targets; `None` leaves it as it is.
/// Panicking fails the whole transaction.
#[must_use]
pub fn apply(effect: Effect, pre_data: &[u8]) -> Option<Vec<u8>> {
    match effect {
        Effect::HeldBy { tag } => {
            assert!(!pre_data.is_empty(), "that name is not registered");
            let record: NameRecord =
                borsh::from_slice(pre_data).expect("a record shard holds a valid record");
            assert!(record.owner == tag, "the signer does not hold that name");
            None
        }
        Effect::NextPost { n, now } => {
            let author: Author = if pre_data.is_empty() {
                Author::default()
            } else {
                borsh::from_slice(pre_data).expect("an author shard holds a valid author")
            };
            assert!(n == author.count, "the next post of this name is number {}", author.count);
            assert!(
                author.count == 0 || now >= author.last.saturating_add(MIN_POST_GAP_MS),
                "one post every {} seconds",
                MIN_POST_GAP_MS / 1000
            );
            Some(to_bytes(&Author { version: VERSION, count: n + 1, last: now }))
        }
        Effect::CreatePost(post) => {
            assert!(pre_data.is_empty(), "that post already exists");
            Some(to_bytes(&post))
        }
        Effect::HidePost => {
            assert!(!pre_data.is_empty(), "no such post");
            let mut post: Post = borsh::from_slice(pre_data).expect("a post shard holds a valid post");
            post.hidden = true;
            Some(to_bytes(&post))
        }
        Effect::PostLive => {
            assert!(!pre_data.is_empty(), "no such post");
            let post: Post = borsh::from_slice(pre_data).expect("a post shard holds a valid post");
            assert!(!post.hidden, "that post was taken down");
            None
        }
        Effect::CreateLike(like) => {
            assert!(pre_data.is_empty(), "already liked");
            Some(to_bytes(&like))
        }
        Effect::DeleteLike => {
            assert!(!pre_data.is_empty(), "not liked");
            Some(Vec::new())
        }
        Effect::WriteFollows { sealed } => Some(sealed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOARD: AccountId = AccountId::new([7; 32]);
    const JEF: NameId = [1; 32];
    const ADA: NameId = [2; 32];
    const HOLDER: AccountId = AccountId::new([9; 32]);
    const STRANGER: AccountId = AccountId::new([8; 32]);

    fn meta(account_id: AccountId, program: AccountId, signs: bool) -> AccountMeta {
        AccountMeta { account_id, is_authorized: signs, program_account_id: program }
    }

    fn record_of(name: &NameId) -> AccountMeta {
        meta(record_address(&REGISTRY, name), REGISTRY, false)
    }

    fn mine(name: &NameId) -> AccountMeta {
        meta(author_address(&BOARD, name), BOARD, false)
    }

    fn input(accounts: Vec<AccountMeta>, instruction: &Instruction) -> PlanInput {
        PlanInput {
            self_account_id: BOARD,
            caller_account_id: None,
            accounts,
            instruction_data: borsh::to_vec(instruction).expect("the instruction serializes"),
        }
    }

    /// The effects a plan makes, with the shard each names.
    fn run(accounts: Vec<AccountMeta>, instruction: Instruction) -> Vec<(AccountId, AccountId, Effect)> {
        plan(&input(accounts, &instruction), instruction)
            .output()
            .effects
            .iter()
            .map(|e| {
                (
                    e.selector.account_id,
                    e.selector.program_account_id,
                    borsh::from_slice(&e.data).expect("the effect decodes"),
                )
            })
            .collect()
    }

    fn post_instruction(n: u64, parent: Option<PostId>, now: u64) -> Instruction {
        Instruction::Post {
            author: JEF,
            name: "jef".into(),
            n,
            parent,
            text: "hello".into(),
            cids: vec!["zDvZRwzm8K".into()],
            now,
        }
    }

    fn post_accounts(n: u64) -> Vec<AccountMeta> {
        vec![
            record_of(&JEF),
            mine(&JEF),
            meta(post_address(&BOARD, &PostId { author: JEF, n }), BOARD, false),
            meta(HOLDER, AccountId::new([0; 32]), true),
        ]
    }

    fn record_held_by(account: &AccountId, name: &NameId) -> Vec<u8> {
        to_bytes(&NameRecord { version: 5, owner: owner_tag(name, account), expires: u64::MAX, sealed: vec![] })
    }

    #[test]
    fn a_post_checks_the_registry_then_numbers_and_writes_the_post() {
        let effects = run(post_accounts(0), post_instruction(0, None, 1_000));
        assert_eq!(effects.len(), 3);
        // The holder check names the registry's shard, not the board's.
        let (account, shard, check) = &effects[0];
        assert_eq!((*account, *shard), (record_address(&REGISTRY, &JEF), REGISTRY));
        assert_eq!(*check, Effect::HeldBy { tag: owner_tag(&JEF, &HOLDER) });
        assert_eq!(effects[1].2, Effect::NextPost { n: 0, now: 1_000 });
        let Effect::CreatePost(post) = &effects[2].2 else { panic!("a post") };
        assert_eq!((post.id, post.name.as_str(), post.time, post.hidden), (PostId { author: JEF, n: 0 }, "jef", 1_000, false));
    }

    #[test]
    fn the_holder_check_passes_for_the_holder_only() {
        let tag = owner_tag(&JEF, &HOLDER);
        assert_eq!(apply(Effect::HeldBy { tag }, &record_held_by(&HOLDER, &JEF)), None);
        let refused = std::panic::catch_unwind(|| apply(Effect::HeldBy { tag }, &record_held_by(&STRANGER, &JEF)));
        assert!(refused.is_err());
        let unregistered = std::panic::catch_unwind(|| apply(Effect::HeldBy { tag }, &[]));
        assert!(unregistered.is_err());
    }

    #[test]
    #[should_panic(expected = "the record must be the registry's")]
    fn a_look_alike_registry_is_refused() {
        let mut accounts = post_accounts(0);
        accounts[0].program_account_id = AccountId::new([5; 32]);
        let _ = run(accounts, post_instruction(0, None, 1_000));
    }

    #[test]
    #[should_panic(expected = "the record account does not match")]
    fn another_names_record_is_refused() {
        let mut accounts = post_accounts(0);
        accounts[0] = record_of(&ADA);
        let _ = run(accounts, post_instruction(0, None, 1_000));
    }

    #[test]
    #[should_panic(expected = "the signer must sign")]
    fn an_unsigned_post_is_refused() {
        let mut accounts = post_accounts(0);
        accounts[3].is_authorized = false;
        let _ = run(accounts, post_instruction(0, None, 1_000));
    }

    #[test]
    fn posts_are_numbered_in_order_and_spaced() {
        let first = apply(Effect::NextPost { n: 0, now: 1_000 }, &[]).expect("written");
        let author: Author = borsh::from_slice(&first).unwrap();
        assert_eq!((author.count, author.last), (1, 1_000));
        assert!(std::panic::catch_unwind(|| apply(Effect::NextPost { n: 0, now: 50_000 }, &first)).is_err());
        assert!(std::panic::catch_unwind(|| apply(Effect::NextPost { n: 1, now: 5_000 }, &first)).is_err());
        assert!(apply(Effect::NextPost { n: 1, now: 11_000 }, &first).is_some());
    }

    #[test]
    fn a_post_is_written_once_and_hidden_by_its_author() {
        let effects = run(post_accounts(0), post_instruction(0, None, 1_000));
        let Effect::CreatePost(post) = effects[2].2.clone() else { panic!("a post") };
        let written = apply(Effect::CreatePost(post.clone()), &[]).expect("written");
        assert!(std::panic::catch_unwind(|| apply(Effect::CreatePost(post.clone()), &written)).is_err());
        let hidden: Post = borsh::from_slice(&apply(Effect::HidePost, &written).unwrap()).unwrap();
        assert!(hidden.hidden);
        assert_eq!(apply(Effect::PostLive, &written), None);
        let gone = to_bytes(&hidden);
        assert!(std::panic::catch_unwind(|| apply(Effect::PostLive, &gone)).is_err());
    }

    #[test]
    fn a_reply_names_its_parent() {
        let parent = PostId { author: ADA, n: 3 };
        let effects = run(post_accounts(0), post_instruction(0, Some(parent), 1_000));
        let Effect::CreatePost(post) = &effects[2].2 else { panic!("a post") };
        assert_eq!(post.parent, Some(parent));
    }

    #[test]
    fn a_like_is_one_account_per_post_and_liker() {
        let post = PostId { author: ADA, n: 0 };
        let accounts = vec![
            record_of(&JEF),
            meta(like_address(&BOARD, &post, &JEF), BOARD, false),
            meta(post_address(&BOARD, &post), BOARD, false),
            meta(HOLDER, AccountId::new([0; 32]), true),
        ];
        let effects = run(accounts, Instruction::Like { liker: JEF, post, now: 2_000 });
        assert_eq!(effects[0].2, Effect::HeldBy { tag: owner_tag(&JEF, &HOLDER) });
        assert_eq!(effects[1].2, Effect::PostLive);
        let like = apply(effects[2].2.clone(), &[]).expect("written");
        assert!(std::panic::catch_unwind(|| apply(effects[2].2.clone(), &like)).is_err());
        assert_eq!(apply(Effect::DeleteLike, &like), Some(Vec::new()));
        assert_ne!(like_address(&BOARD, &post, &JEF), like_address(&BOARD, &post, &ADA));
    }

    #[test]
    fn bad_posts_are_refused_before_anything_is_checked() {
        for (text, cids) in [
            ("x".repeat(MAX_TEXT + 1), vec![]),
            ("  ".to_owned(), vec![]),
            ("hi".to_owned(), vec!["a".to_owned(); MAX_CIDS + 1]),
            ("hi".to_owned(), vec!["bad cid".to_owned()]),
        ] {
            let instruction = Instruction::Post { author: JEF, name: "jef".into(), n: 0, parent: None, text, cids, now: 1 };
            assert!(std::panic::catch_unwind(|| run(post_accounts(0), instruction)).is_err());
        }
        let bad_name = Instruction::Post { author: JEF, name: "Jef!".into(), n: 0, parent: None, text: "x".into(), cids: vec![], now: 1 };
        assert!(std::panic::catch_unwind(|| run(post_accounts(0), bad_name)).is_err());
    }

    #[test]
    fn the_registry_is_registry_v5() {
        assert_eq!(REGISTRY.to_string(), "38jEfVV1RCyMzu1FGbYcCn5FqxCxKyY58VgzjruAh2gq");
    }
}
