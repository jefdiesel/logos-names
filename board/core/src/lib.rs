//! The LNS board: posts, threaded replies, likes and follows, signed by names.
//!
//! Like the registry, all logic is here so it can be tested on the host; the
//! guest only wires [`handle`] to the LEZ runtime.
//!
//! Every change is made as a name. The board never learns which account holds
//! the name: it computes the signer's owner tag for the name and asks the
//! registry's record actor to refuse unless that tag is the holder's
//! ([`require_holder`]). A holder acting from a private account passes this
//! check inside a proof, so the chain sees "a post by this name" and nothing more.
//!
//! A transaction is one [`Message::Instruction`] to the signer's own actor of
//! the board. [`instruct`] derives every address itself and sends each change
//! as a [`Message::Effect`] to the actor it concerns, as the registry does.
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
    account::{AccountId, Actor},
    program::{PdaSeed, ReceiveInput, Response, SyncMessage},
};
use names_core::{NameId, owner_tag, record_address, validate_name};

/// The names registry the board trusts, pinned at build time so nobody can pass
/// a look-alike registry that says they hold a name. Without
/// `LOGOS_BOARD_REGISTRY` (64 hex characters) it is registry v5 of the LEZ 0.3
/// testnet (`38jEfVV1RCyMzu1FGbYcCn5FqxCxKyY58VgzjruAh2gq`), which does not
/// exist on a 0.4 chain: the guest and the CLI must both be built with the 0.4
/// registry's ID once it is deployed. Changing it changes the guest's image ID.
pub const REGISTRY: AccountId = match option_env!("LOGOS_BOARD_REGISTRY") {
    Some(hex) => AccountId::new(hex_id(hex.as_bytes())),
    None => AccountId::new([
        31, 178, 50, 8, 70, 127, 182, 98, 58, 220, 183, 147, 107, 133, 219, 64, 244, 34, 187, 144,
        254, 233, 13, 197, 18, 202, 225, 4, 100, 224, 136, 186,
    ]),
};

const fn hex_id(hex: &[u8]) -> [u8; 32] {
    const fn nibble(c: u8) -> u8 {
        match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            _ => panic!("LOGOS_BOARD_REGISTRY must be lowercase hex"),
        }
    }
    assert!(hex.len() == 64, "LOGOS_BOARD_REGISTRY must be 64 hex characters");
    let mut out = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        out[i] = nibble(hex[2 * i]) << 4 | nibble(hex[2 * i + 1]);
        i += 1;
    }
    out
}

/// The tag of the registry's `Message::RequireHolder`: an actor of another
/// program can read the registry only by asking it, so the registry refuses
/// unless the record is held by the tag that follows. See
/// `board/registry-require-holder.patch`.
const REGISTRY_REQUIRE_HOLDER: u8 = 2;

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

/// What this program receives. A transaction sends an [`Instruction`] to the
/// signer's own actor; everything else is this program talking to itself.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Message {
    Instruction(Instruction),
    Effect(Effect),
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Instruction {
    /// Actors: the name's record, author counter and post, and the signer's own.
    Post {
        author: NameId,
        name: String,
        n: u64,
        parent: Option<PostId>,
        text: String,
        cids: Vec<String>,
        now: u64,
    },
    /// Actors: the author's record, the post, and the signer's own.
    Hide { post: PostId },
    /// Actors: the liker's record, the like, the post, and the signer's own.
    Like { liker: NameId, post: PostId, now: u64 },
    /// Actors: the liker's record, the like, and the signer's own.
    Unlike { liker: NameId, post: PostId },
    /// Actors: the follower's record, the follows account, and the signer's own.
    SetFollows { follower: NameId, sealed: Vec<u8> },
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Effect {
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

// LEZ 0.4 admits a new public account only with evidence of where it comes from,
// so a client lists the seed of every account a transaction may create.

#[must_use]
pub fn author_seed(author: &NameId) -> PdaSeed {
    seed(&[AUTHOR_DOMAIN, author])
}

#[must_use]
pub fn post_seed(post: &PostId) -> PdaSeed {
    seed(&[POST_DOMAIN, &post.author, &post.n.to_le_bytes()])
}

#[must_use]
pub fn like_seed(post: &PostId, liker: &NameId) -> PdaSeed {
    seed(&[LIKE_DOMAIN, &post.author, &post.n.to_le_bytes(), liker])
}

#[must_use]
pub fn follows_seed(follower: &NameId) -> PdaSeed {
    seed(&[FOLLOWS_DOMAIN, follower])
}

#[must_use]
pub fn author_address(board: &AccountId, author: &NameId) -> AccountId {
    AccountId::for_public_pda(board, &author_seed(author))
}

#[must_use]
pub fn post_address(board: &AccountId, post: &PostId) -> AccountId {
    AccountId::for_public_pda(board, &post_seed(post))
}

#[must_use]
pub fn like_address(board: &AccountId, post: &PostId, liker: &NameId) -> AccountId {
    AccountId::for_public_pda(board, &like_seed(post, liker))
}

#[must_use]
pub fn follows_address(board: &AccountId, follower: &NameId) -> AccountId {
    AccountId::for_public_pda(board, &follows_seed(follower))
}

/// Asks the registry to refuse unless `signer` holds the name `name`.
#[must_use]
pub fn require_holder(name: &NameId, signer: &AccountId) -> SyncMessage {
    let mut message = vec![REGISTRY_REQUIRE_HOLDER];
    message.extend_from_slice(&owner_tag(name, signer));
    SyncMessage {
        to: Actor::new(record_address(&REGISTRY, name), REGISTRY),
        message,
        pda_seeds: Default::default(),
    }
}

fn effect(to: AccountId, board: &AccountId, effect: &Effect) -> SyncMessage {
    SyncMessage::new(Actor::new(to, *board), &Message::Effect(effect.clone()))
}

fn time_window(response: Response, now: u64) -> Response {
    let window = lee_core::program::TimestampValidityWindow::try_from((
        Some(now),
        Some(now.saturating_add(TIME_WINDOW_MS)),
    ))
    .expect("a window of positive length");
    response.timestamp_window(window)
}

fn check_cid(cid: &str) {
    assert!(
        !cid.is_empty() && cid.len() <= MAX_CID_LEN && cid.bytes().all(|b| b.is_ascii_alphanumeric()),
        "a CID is 1-{MAX_CID_LEN} letters and digits"
    );
}

/// Handles a message to one of this program's actors. The guest's entry point.
#[must_use]
pub fn handle(input: &ReceiveInput, message: Message) -> Response {
    match message {
        Message::Instruction(instruction) => instruct(input, instruction),
        Message::Effect(effect) => {
            assert!(input.from_own_program(), "only this program may send an effect");
            match apply(effect, &input.pre_state) {
                Some(state) => Response::set_state(state),
                None => Response::keep_state(),
            }
        }
    }
}

/// Turns an instruction from the signer into the checks and changes it makes.
#[must_use]
pub fn instruct(input: &ReceiveInput, instruction: Instruction) -> Response {
    assert!(input.is_authorized, "the signer must sign");
    let board = &input.receiver.program_account_id;
    let signer = &input.receiver.account_id;
    let response = Response::keep_state();
    match instruction {
        Instruction::Post { author, name, n, parent, text, cids, now } => {
            validate_name(&name).unwrap_or_else(|reason| panic!("{reason}"));
            assert!(!text.trim().is_empty() || !cids.is_empty(), "a post needs text or a file");
            assert!(text.len() <= MAX_TEXT, "a post holds at most {MAX_TEXT} bytes of text");
            assert!(cids.len() <= MAX_CIDS, "a post carries at most {MAX_CIDS} files");
            cids.iter().for_each(|cid| check_cid(cid));
            let id = PostId { author, n };
            let post = Post { version: VERSION, id, name, parent, time: now, text, cids, hidden: false };
            time_window(response.send(require_holder(&author, signer)), now)
                .send(effect(author_address(board, &author), board, &Effect::NextPost { n, now }))
                .send(effect(post_address(board, &id), board, &Effect::CreatePost(post)))
        }
        Instruction::Hide { post: id } => response
            .send(require_holder(&id.author, signer))
            .send(effect(post_address(board, &id), board, &Effect::HidePost)),
        Instruction::Like { liker, post: id, now } => {
            let like = Like { version: VERSION, post: id, liker, time: now };
            time_window(response.send(require_holder(&liker, signer)), now)
                .send(effect(post_address(board, &id), board, &Effect::PostLive))
                .send(effect(like_address(board, &id, &liker), board, &Effect::CreateLike(like)))
        }
        Instruction::Unlike { liker, post: id } => response
            .send(require_holder(&liker, signer))
            .send(effect(like_address(board, &id, &liker), board, &Effect::DeleteLike)),
        Instruction::SetFollows { follower, sealed } => {
            assert!(sealed.len() <= MAX_FOLLOWS, "a follow list holds at most {MAX_FOLLOWS} bytes");
            response
                .send(require_holder(&follower, signer))
                .send(effect(follows_address(board, &follower), board, &Effect::WriteFollows { sealed }))
        }
    }
}

fn to_bytes<T: BorshSerialize>(value: &T) -> Vec<u8> {
    borsh::to_vec(value).expect("borsh serialization is infallible")
}

/// Applies one effect to the actor it targets; `None` leaves it as it is.
/// Panicking fails the whole transaction.
#[must_use]
pub fn apply(effect: Effect, pre_data: &[u8]) -> Option<Vec<u8>> {
    match effect {
        Effect::NextPost { n, now } => {
            let author: Author = if pre_data.is_empty() {
                Author::default()
            } else {
                borsh::from_slice(pre_data).expect("an author actor holds a valid author")
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
            let mut post: Post = borsh::from_slice(pre_data).expect("a post actor holds a valid post");
            post.hidden = true;
            Some(to_bytes(&post))
        }
        Effect::PostLive => {
            assert!(!pre_data.is_empty(), "no such post");
            let post: Post = borsh::from_slice(pre_data).expect("a post actor holds a valid post");
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

    fn input(signer: AccountId, authorized: bool, instruction: &Instruction) -> ReceiveInput {
        ReceiveInput {
            receiver: Actor::new(signer, BOARD),
            from: None,
            is_authorized: authorized,
            pre_state: Default::default(),
            message: borsh::to_vec(&Message::Instruction(instruction.clone())).expect("the instruction serializes"),
        }
    }

    /// The registry check an instruction starts with, then its effects on the board.
    fn run(signer: AccountId, instruction: Instruction) -> (SyncMessage, Vec<(AccountId, Effect)>) {
        let sent = instruct(&input(signer, true, &instruction), instruction).sync_messages;
        let check = sent[0].clone();
        let effects = sent[1..]
            .iter()
            .map(|message| {
                assert_eq!(message.to.program_account_id, BOARD);
                let Ok(Message::Effect(effect)) = borsh::from_slice(&message.message) else {
                    panic!("the board only sends itself effects");
                };
                (message.to.account_id, effect)
            })
            .collect();
        (check, effects)
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

    #[test]
    fn a_post_checks_the_registry_then_numbers_and_writes_the_post() {
        let (check, effects) = run(HOLDER, post_instruction(0, None, 1_000));
        assert_eq!(effects.len(), 2);
        // The holder check goes to the registry's record, not the board.
        assert_eq!(check, require_holder(&JEF, &HOLDER));
        assert_eq!(check.to, Actor::new(record_address(&REGISTRY, &JEF), REGISTRY));
        assert_eq!(check.message[0], REGISTRY_REQUIRE_HOLDER);
        assert_eq!(check.message[1..], owner_tag(&JEF, &HOLDER));
        assert_eq!(effects[0], (author_address(&BOARD, &JEF), Effect::NextPost { n: 0, now: 1_000 }));
        assert_eq!(effects[1].0, post_address(&BOARD, &PostId { author: JEF, n: 0 }));
        let Effect::CreatePost(post) = &effects[1].1 else { panic!("a post") };
        assert_eq!((post.id, post.name.as_str(), post.time, post.hidden), (PostId { author: JEF, n: 0 }, "jef", 1_000, false));
    }

    #[test]
    fn every_change_is_checked_against_the_signers_tag() {
        let post = PostId { author: JEF, n: 0 };
        for instruction in [
            post_instruction(0, None, 1_000),
            Instruction::Hide { post },
            Instruction::Like { liker: JEF, post, now: 1 },
            Instruction::Unlike { liker: JEF, post },
            Instruction::SetFollows { follower: JEF, sealed: vec![1] },
        ] {
            assert_eq!(run(STRANGER, instruction).0, require_holder(&JEF, &STRANGER));
        }
    }

    #[test]
    #[should_panic(expected = "the signer must sign")]
    fn an_unsigned_post_is_refused() {
        let instruction = post_instruction(0, None, 1_000);
        let _ = instruct(&input(HOLDER, false, &instruction), instruction);
    }

    #[test]
    #[should_panic(expected = "only this program may send an effect")]
    fn outsiders_cannot_send_an_effect() {
        let mut received = input(HOLDER, true, &post_instruction(0, None, 1));
        received.from = Some(Actor::new(HOLDER, AccountId::new([5; 32])));
        let _ = handle(&received, Message::Effect(Effect::HidePost));
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
        let (_, effects) = run(HOLDER, post_instruction(0, None, 1_000));
        let Effect::CreatePost(post) = effects[1].1.clone() else { panic!("a post") };
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
        let (_, effects) = run(HOLDER, post_instruction(0, Some(parent), 1_000));
        let Effect::CreatePost(post) = &effects[1].1 else { panic!("a post") };
        assert_eq!(post.parent, Some(parent));
    }

    #[test]
    fn a_like_is_one_account_per_post_and_liker() {
        let post = PostId { author: ADA, n: 0 };
        let (check, effects) = run(HOLDER, Instruction::Like { liker: JEF, post, now: 2_000 });
        assert_eq!(check, require_holder(&JEF, &HOLDER));
        assert_eq!(effects[0].1, Effect::PostLive);
        assert_eq!(effects[1].0, like_address(&BOARD, &post, &JEF));
        let like = apply(effects[1].1.clone(), &[]).expect("written");
        assert!(std::panic::catch_unwind(|| apply(effects[1].1.clone(), &like)).is_err());
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
            assert!(std::panic::catch_unwind(|| run(HOLDER, instruction)).is_err());
        }
        let bad_name = Instruction::Post { author: JEF, name: "Jef!".into(), n: 0, parent: None, text: "x".into(), cids: vec![], now: 1 };
        assert!(std::panic::catch_unwind(|| run(HOLDER, bad_name)).is_err());
    }

    #[test]
    fn the_pinned_registry_builds_from_hex() {
        assert_eq!(hex_id(b"0102030405060708091011121314151617181920212223242526272829303132")[..2], [1, 2]);
    }
}
