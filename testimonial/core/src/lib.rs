//! Testimonials on LEZ 0.4: one on-chain record per (submission, author account).
//!
//! A testimonial is custom text, an optional username and the identifier of the λPrize
//! submission it vouches for (LP-0021: "a standard LEZ program that stores each testimonial
//! on-chain"). The program is built so the adoption criteria can be checked from chain data
//! alone, with plain sequencer reads (`getAccountView`) and no indexer:
//!
//! - **One per account.** A testimonial lives at a PDA derived from the submission and the
//!   author's account, and is written once. A second testimonial from the same account for
//!   the same submission is refused, so a submission's count *is* its number of distinct
//!   author accounts.
//! - **Proven author.** The author is the account whose own actor of this program receives
//!   the instruction, and the instruction must be signed by it and be the transaction's root
//!   message (never relayed by another program). The author is never an argument.
//! - **On-chain time.** The instruction carries `now`, and the program bounds the transaction
//!   to blocks whose timestamp is in `[now, now + 10 min)`; the sequencer refuses it outside
//!   that window. The stored `at` is therefore at most 10 minutes before the block's time.
//! - **Listing without an indexer.** Each submission has a head PDA holding its count and its
//!   newest testimonial; every testimonial points at the one before it. Every address is a
//!   pure function of the arguments, so concurrent posts never race.
//!
//! Accounts (PDAs of this program, SPEL `compute_pda` rule, no integer seeds):
//! - head: `pda[const "testimonial/v1/head", arg sha256(submission)]` → [`Head`]
//! - testimonial: `pda[const "testimonial/v1/node", arg sha256(submission), account author]`
//!   → [`Testimonial`]
//!
//! All logic is here so it can be tested on the host; the guest only wires [`handle`] to the
//! LEZ runtime.

use std::collections::{BTreeMap, BTreeSet};

use borsh::{BorshDeserialize, BorshSerialize};
use lee_core::{
    account::{AccountId, Actor},
    program::{PdaSeed, ProgramEvent, ReceiveInput, Response, SyncMessage},
};

/// Layout version of every record this program writes (first byte).
pub const SCHEMA: u8 = 1;
/// Longest submission identifier, in bytes.
pub const MAX_SUBMISSION: usize = 64;
/// Longest username, in bytes.
pub const MAX_USERNAME: usize = 32;
/// Longest testimonial text, in bytes (UTF-8).
pub const MAX_TEXT: usize = 1024;
/// How far the block's timestamp may run ahead of the `now` an instruction claims.
pub const TIME_WINDOW_MS: u64 = 10 * 60 * 1000;

/// PDA seed domains (each fits one 32-byte SPEL string component).
pub const HEAD_DOMAIN: &str = "testimonial/v1/head";
pub const NODE_DOMAIN: &str = "testimonial/v1/node";
/// Event name; the selector is `sha256(POSTED_EVENT)[..8]`.
pub const POSTED_EVENT: &str = "testimonial::Posted";

/// The LP-0021 adoption bar: testimonials, distinct accounts, months, and per month.
pub const ADOPTION_TOTAL: u64 = 150;
pub const ADOPTION_MONTHS: usize = 2;
pub const ADOPTION_PER_MONTH: u64 = 30;

/// A submission's list: how many testimonials it has (= distinct author accounts) and the
/// newest one.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Head {
    pub schema: u8,
    pub submission: String,
    pub count: u64,
    pub newest: Option<AccountId>,
    /// `at` of the first and of the latest testimonial, in milliseconds.
    pub first_at: u64,
    pub last_at: u64,
}

/// One testimonial. Written once; never changed or removed.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Testimonial {
    pub schema: u8,
    pub submission: String,
    /// The account that signed it.
    pub author: AccountId,
    /// Its position in the submission's list, from 0.
    pub seq: u64,
    /// When it was posted, in milliseconds since the Unix epoch: at most
    /// [`TIME_WINDOW_MS`] before the timestamp of the block that holds it.
    pub at: u64,
    /// A display name the author chose. Not proven.
    pub username: Option<String>,
    pub text: String,
    /// The testimonial before this one in the submission's list.
    pub prev: Option<AccountId>,
}

/// The data of the `testimonial::Posted` event.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Posted {
    pub submission: String,
    pub author: AccountId,
    pub seq: u64,
    pub at: u64,
}

/// What this program receives. A transaction sends an [`Instruction`] to the author's own
/// actor; effects are this program talking to itself.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Message {
    Instruction(Instruction),
    Effect(Effect),
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Instruction {
    /// Root actor: `(author, program)`, signed by the author. Also declare the submission's
    /// head and the author's testimonial account ([`actors_to_declare`]).
    Post { submission: String, username: Option<String>, text: String, now: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Effect {
    /// To the submission's head, from the author's actor: number the testimonial, link it
    /// after the current newest, then write it.
    Index(Testimonial),
    /// To the testimonial's account, from the head: write it, once.
    Write(Testimonial),
}

// --- addresses -----------------------------------------------------------------------------

#[must_use]
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    use risc0_zkvm::sha::{Impl, Sha256 as _};
    let digest = Impl::hash_bytes(bytes);
    let mut out = [0u8; 32];
    out.copy_from_slice(digest.as_bytes());
    out
}

/// A SPEL string seed component: UTF-8, zero-padded to 32 bytes.
#[must_use]
pub fn seed_str(s: &str) -> [u8; 32] {
    let b = s.as_bytes();
    assert!(b.len() <= 32, "a seed string is at most 32 bytes");
    let mut out = [0; 32];
    out[..b.len()].copy_from_slice(b);
    out
}

/// SPEL `compute_pda` combination: one component is the seed, several are sha256(c1‖c2‖…).
#[must_use]
pub fn combine(parts: &[[u8; 32]]) -> PdaSeed {
    assert!(!parts.is_empty(), "a PDA needs a seed component");
    if parts.len() == 1 {
        return PdaSeed::new(parts[0]);
    }
    PdaSeed::new(sha256(&parts.concat()))
}

/// The `[u8; 32]` seed component that stands for a submission identifier.
#[must_use]
pub fn submission_key(submission: &str) -> [u8; 32] {
    sha256(submission.as_bytes())
}

#[must_use]
pub fn head_seed(submission: &str) -> PdaSeed {
    combine(&[seed_str(HEAD_DOMAIN), submission_key(submission)])
}

#[must_use]
pub fn node_seed(submission: &str, author: &AccountId) -> PdaSeed {
    combine(&[seed_str(NODE_DOMAIN), submission_key(submission), *author.value()])
}

#[must_use]
pub fn head_address(program: &AccountId, submission: &str) -> AccountId {
    AccountId::for_public_pda(program, &head_seed(submission))
}

#[must_use]
pub fn node_address(program: &AccountId, submission: &str, author: &AccountId) -> AccountId {
    AccountId::for_public_pda(program, &node_seed(submission, author))
}

/// Every actor a `Post` reaches besides its root `(author, program)`.
#[must_use]
pub fn actors_to_declare(program: &AccountId, author: &AccountId, submission: &str) -> Vec<Actor> {
    vec![
        Actor::new(head_address(program, submission), *program),
        Actor::new(node_address(program, submission, author), *program),
    ]
}

// --- validation ----------------------------------------------------------------------------

/// A submission identifier: 1-64 bytes of `a-z 0-9 . _ : / -`, starting with a letter or
/// digit. Lowercase only, so look-alike identifiers can't split a submission's list.
pub fn validate_submission(submission: &str) -> Result<(), String> {
    let ok_len = (1..=MAX_SUBMISSION).contains(&submission.len());
    let ok_first = submission.bytes().next().is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit());
    let ok_chars = submission
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b".-_:/".contains(&b));
    if ok_len && ok_first && ok_chars {
        Ok(())
    } else {
        Err(format!(
            "a submission id is 1-{MAX_SUBMISSION} characters of a-z, 0-9 and . _ : / -, starting with a letter or digit"
        ))
    }
}

/// A username: 1-32 bytes, no control characters, no space at either end.
pub fn validate_username(username: &str) -> Result<(), String> {
    if username.is_empty() || username.len() > MAX_USERNAME {
        return Err(format!("a username is 1-{MAX_USERNAME} bytes"));
    }
    if username.chars().any(char::is_control) || username.trim() != username {
        return Err("a username has no control characters and no spaces at either end".into());
    }
    Ok(())
}

/// Testimonial text: 1-1024 bytes, not only spaces, no control characters but line breaks
/// and tabs.
pub fn validate_text(text: &str) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("a testimonial needs some text".into());
    }
    if text.len() > MAX_TEXT {
        return Err(format!("a testimonial holds at most {MAX_TEXT} bytes of text"));
    }
    if text.chars().any(|c| c.is_control() && c != '\n' && c != '\t') {
        return Err("a testimonial has no control characters other than line breaks and tabs".into());
    }
    Ok(())
}

pub fn validate_post(submission: &str, username: Option<&str>, text: &str) -> Result<(), String> {
    validate_submission(submission)?;
    if let Some(name) = username {
        validate_username(name)?;
    }
    validate_text(text)
}

// --- the program ---------------------------------------------------------------------------

fn to_bytes<T: BorshSerialize>(value: &T) -> Vec<u8> {
    borsh::to_vec(value).expect("borsh serialization is infallible")
}

fn effect_to(seed: PdaSeed, program: &AccountId, effect: Effect) -> SyncMessage {
    // The seed lets this program create the account itself (self-admission), so clients
    // need no admission evidence.
    SyncMessage::new(Actor::new(AccountId::for_public_pda(program, &seed), *program), &Message::Effect(effect))
        .with_pda_seeds([seed])
}

fn bounded(response: Response, now: u64) -> Response {
    let end = now.checked_add(TIME_WINDOW_MS).expect("now is too large");
    response.try_timestamp_window(now..end).expect("a window of positive length")
}

/// Handles a message to one of this program's actors. The guest's entry point.
#[must_use]
pub fn handle(input: &ReceiveInput, message: Message) -> Response {
    match message {
        Message::Instruction(instruction) => instruct(input, instruction),
        Message::Effect(effect) => {
            assert!(input.from_own_program(), "only this program may send an effect");
            apply(input, effect)
        }
    }
}

/// Turns the author's instruction into the effects it makes.
#[must_use]
pub fn instruct(input: &ReceiveInput, instruction: Instruction) -> Response {
    // In LEZ 0.4 a signature authorizes the signer in every program a transaction reaches, so
    // a program the author called for something else could relay a Post. Only the
    // transaction's own root message may post.
    assert!(input.from.is_none(), "a testimonial must be the transaction's root message");
    assert!(input.is_authorized, "the author must sign");
    let program = input.receiver.program_account_id;
    let author = input.receiver.account_id;
    match instruction {
        Instruction::Post { submission, username, text, now } => {
            validate_post(&submission, username.as_deref(), &text).unwrap_or_else(|reason| panic!("{reason}"));
            assert!(now > 0, "now is the time in milliseconds");
            let seed = head_seed(&submission);
            let draft = Testimonial { schema: SCHEMA, submission, author, seq: 0, at: now, username, text, prev: None };
            bounded(Response::keep_state().send(effect_to(seed, &program, Effect::Index(draft))), now)
        }
    }
}

/// Applies an effect from this program to the actor it targets.
#[must_use]
pub fn apply(input: &ReceiveInput, effect: Effect) -> Response {
    let program = input.receiver.program_account_id;
    let here = input.receiver.account_id;
    let from = input.from.expect("an effect has a sender").account_id;
    match effect {
        Effect::Index(mut testimonial) => {
            assert!(here == head_address(&program, &testimonial.submission), "index: not this submission's head");
            assert!(from == testimonial.author, "index: only the author's own actor indexes a testimonial");
            let mut head = if input.pre_state.is_empty() {
                Head {
                    schema: SCHEMA,
                    submission: testimonial.submission.clone(),
                    count: 0,
                    newest: None,
                    first_at: testimonial.at,
                    last_at: testimonial.at,
                }
            } else {
                borsh::from_slice::<Head>(&input.pre_state).expect("a head")
            };
            assert!(head.schema == SCHEMA && head.submission == testimonial.submission, "index: not this submission's head");
            testimonial.seq = head.count;
            testimonial.prev = head.newest;
            let seed = node_seed(&testimonial.submission, &testimonial.author);
            head.count = head.count.checked_add(1).expect("count overflow");
            head.newest = Some(AccountId::for_public_pda(&program, &seed));
            head.last_at = head.last_at.max(testimonial.at);
            Response::set_state(to_bytes(&head)).send(effect_to(seed, &program, Effect::Write(testimonial)))
        }
        Effect::Write(testimonial) => {
            assert!(
                here == node_address(&program, &testimonial.submission, &testimonial.author),
                "write: not this author's testimonial account"
            );
            assert!(
                from == head_address(&program, &testimonial.submission),
                "write: only the submission's head writes a testimonial"
            );
            assert!(input.pre_state.is_empty(), "this account already posted a testimonial for this submission");
            let event = Posted {
                submission: testimonial.submission.clone(),
                author: testimonial.author,
                seq: testimonial.seq,
                at: testimonial.at,
            };
            Response::set_state(to_bytes(&testimonial)).event(ProgramEvent {
                selector: sha256(POSTED_EVENT.as_bytes())[..8].try_into().expect("8 bytes"),
                data: to_bytes(&event),
            })
        }
    }
}

// --- reading and verifying ------------------------------------------------------------------

/// A submission's testimonials as read from the chain, newest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    pub program: AccountId,
    pub submission: String,
    pub head_address: AccountId,
    pub head: Option<Head>,
    pub items: Vec<(AccountId, Testimonial)>,
}

/// Reads a submission's list from the chain and checks it. `read(account)` returns this
/// program's actor state on `account` (empty if none): one `getAccountView` each.
///
/// The checks are what an evaluator relies on: the list has exactly `head.count` records,
/// each is a testimonial for this submission stored at the address derived from its own
/// author (so no account appears twice), the numbers run down from `count - 1` to 0, and the
/// list ends there.
pub fn walk<E: std::fmt::Display>(
    program: &AccountId,
    submission: &str,
    mut read: impl FnMut(&AccountId) -> Result<Vec<u8>, E>,
) -> Result<Listing, String> {
    validate_submission(submission)?;
    let head_at = head_address(program, submission);
    let state = read(&head_at).map_err(|e| format!("reading the head {head_at}: {e}"))?;
    let mut listing = Listing {
        program: *program,
        submission: submission.to_owned(),
        head_address: head_at,
        head: None,
        items: Vec::new(),
    };
    if state.is_empty() {
        return Ok(listing);
    }
    let head: Head = borsh::from_slice(&state).map_err(|_| format!("{head_at} does not hold a head"))?;
    if head.schema != SCHEMA || head.submission != submission {
        return Err(format!("{head_at} is not the head of {submission}"));
    }
    let mut authors = BTreeSet::new();
    let mut next = head.newest;
    while let Some(at) = next {
        let expected_seq = head
            .count
            .checked_sub(1 + listing.items.len() as u64)
            .ok_or_else(|| format!("the list is longer than its count {}", head.count))?;
        let state = read(&at).map_err(|e| format!("reading {at}: {e}"))?;
        if state.is_empty() {
            return Err(format!("the list points at {at}, which holds no testimonial"));
        }
        let t: Testimonial = borsh::from_slice(&state).map_err(|_| format!("{at} does not hold a testimonial"))?;
        if t.schema != SCHEMA || t.submission != submission {
            return Err(format!("{at} is not a testimonial for {submission}"));
        }
        if node_address(program, submission, &t.author) != at {
            return Err(format!("{at} is not the testimonial account of its author {}", t.author));
        }
        if !authors.insert(t.author) {
            return Err(format!("{} appears twice", t.author));
        }
        if t.seq != expected_seq {
            return Err(format!("{at} is number {}, expected {expected_seq}", t.seq));
        }
        next = t.prev;
        listing.items.push((at, t));
    }
    if listing.items.len() as u64 != head.count {
        return Err(format!("the list has {} testimonials but the head counts {}", listing.items.len(), head.count));
    }
    listing.head = Some(head);
    Ok(listing)
}

/// `YYYY-MM` (UTC) of a time in milliseconds since the Unix epoch.
#[must_use]
pub fn month_of(ms: u64) -> String {
    let (y, m, _) = civil(ms);
    format!("{y:04}-{m:02}")
}

/// `YYYY-MM-DD HH:MM:SS UTC` of a time in milliseconds since the Unix epoch.
#[must_use]
pub fn format_time(ms: u64) -> String {
    let (y, m, d) = civil(ms);
    let s = ms / 1000 % 86_400;
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02} UTC", s / 3600, s / 60 % 60, s % 60)
}

/// Year, month and day (proleptic Gregorian, UTC) of a time in ms; Howard Hinnant's
/// `civil_from_days`.
fn civil(ms: u64) -> (i64, u32, u32) {
    let days = i64::try_from(ms / 86_400_000).expect("a time in range") + 719_468;
    let era = days.div_euclid(146_097);
    let doe = days.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = u32::try_from(doy - (153 * mp + 2) / 5 + 1).expect("a day");
    let m = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).expect("a month");
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

/// The numbers the adoption criteria are judged on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub total: u64,
    pub distinct_authors: u64,
    /// Testimonials per calendar month (UTC), by `at`.
    pub per_month: BTreeMap<String, u64>,
    pub first_at: Option<u64>,
    pub last_at: Option<u64>,
    /// Months with at least [`ADOPTION_PER_MONTH`] testimonials.
    pub qualifying_months: Vec<String>,
    /// At least [`ADOPTION_TOTAL`] testimonials from as many distinct accounts, in at least
    /// [`ADOPTION_MONTHS`] months with [`ADOPTION_PER_MONTH`] or more each.
    pub meets_adoption: bool,
}

#[must_use]
pub fn summarize(listing: &Listing) -> Summary {
    let total = listing.items.len() as u64;
    let distinct_authors = listing.items.iter().map(|(_, t)| t.author).collect::<BTreeSet<_>>().len() as u64;
    let mut per_month = BTreeMap::new();
    for (_, t) in &listing.items {
        *per_month.entry(month_of(t.at)).or_insert(0u64) += 1;
    }
    let qualifying_months: Vec<String> =
        per_month.iter().filter(|(_, n)| **n >= ADOPTION_PER_MONTH).map(|(m, _)| m.clone()).collect();
    Summary {
        total,
        distinct_authors,
        first_at: listing.items.iter().map(|(_, t)| t.at).min(),
        last_at: listing.items.iter().map(|(_, t)| t.at).max(),
        meets_adoption: total >= ADOPTION_TOTAL
            && distinct_authors >= ADOPTION_TOTAL
            && qualifying_months.len() >= ADOPTION_MONTHS,
        qualifying_months,
        per_month,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    const PROGRAM: AccountId = AccountId::new([7; 32]);
    const ALICE: AccountId = AccountId::new([1; 32]);
    const BOB: AccountId = AccountId::new([2; 32]);
    const SUB: &str = "lp-0021/names-wallet";
    // 2027-01-15 00:00:00 UTC.
    const T0: u64 = 1_800_000_000_000;

    fn post(text: &str, now: u64) -> Instruction {
        Instruction::Post { submission: SUB.into(), username: Some("alice".into()), text: text.into(), now }
    }

    fn root_input(signer: AccountId, authorized: bool, instruction: &Instruction) -> ReceiveInput {
        ReceiveInput {
            receiver: Actor::new(signer, PROGRAM),
            from: None,
            is_authorized: authorized,
            pre_state: Default::default(),
            message: to_bytes(&Message::Instruction(instruction.clone())),
        }
    }

    /// A toy engine: runs a root instruction and delivers its sync messages depth-first, as
    /// LEZ does, against in-memory actor states. All or nothing.
    #[derive(Default)]
    struct Chain {
        states: HashMap<AccountId, Vec<u8>>,
        events: Vec<ProgramEvent>,
    }

    impl Chain {
        fn deliver(
            &self,
            states: &mut HashMap<AccountId, Vec<u8>>,
            events: &mut Vec<ProgramEvent>,
            to: Actor,
            from: Option<Actor>,
            authorized: bool,
            message: Vec<u8>,
        ) {
            assert_eq!(to.program_account_id, PROGRAM, "only this program is on the toy chain");
            let input = ReceiveInput {
                receiver: to,
                from,
                is_authorized: authorized,
                pre_state: states.get(&to.account_id).cloned().unwrap_or_default().into(),
                message: message.clone(),
            };
            let response = handle(&input, borsh::from_slice(&message).expect("a message"));
            if let Some(state) = response.post_state {
                states.insert(to.account_id, state.to_vec());
            }
            events.extend(response.events);
            for sent in response.sync_messages {
                // PDA grants make a seeded target authorized, as in LEZ.
                let granted = sent
                    .pda_seeds
                    .iter()
                    .any(|seed| AccountId::for_public_pda(&PROGRAM, seed) == sent.to.account_id);
                self.deliver(states, events, sent.to, Some(to), granted, sent.message);
            }
        }

        fn try_post(&mut self, author: AccountId, instruction: &Instruction) -> Result<(), String> {
            let mut states = self.states.clone();
            let mut events = Vec::new();
            let message = to_bytes(&Message::Instruction(instruction.clone()));
            let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                self.deliver(&mut states, &mut events, Actor::new(author, PROGRAM), None, true, message);
            }));
            match run {
                Ok(()) => {
                    self.states = states;
                    self.events.extend(events);
                    Ok(())
                }
                Err(panic) => Err(panic
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| panic.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                    .unwrap_or_default()),
            }
        }

        fn listing(&self, submission: &str) -> Listing {
            walk(&PROGRAM, submission, |a| Ok::<_, String>(self.states.get(a).cloned().unwrap_or_default()))
                .expect("a consistent list")
        }
    }

    #[test]
    fn domains_fit_one_seed_component() {
        assert!(HEAD_DOMAIN.len() <= 32 && NODE_DOMAIN.len() <= 32);
    }

    #[test]
    fn a_post_sends_one_index_to_the_head_with_its_seed_and_a_time_window() {
        let instruction = post("great wallet", T0);
        let response = instruct(&root_input(ALICE, true, &instruction), instruction);
        assert!(response.post_state.is_none(), "the author's own actor is left alone");
        assert_eq!(response.sync_messages.len(), 1);
        let sent = &response.sync_messages[0];
        assert_eq!(sent.to, Actor::new(head_address(&PROGRAM, SUB), PROGRAM));
        assert!(sent.pda_seeds.contains(&head_seed(SUB)));
        let Ok(Message::Effect(Effect::Index(t))) = borsh::from_slice(&sent.message) else { panic!("an index") };
        assert_eq!((t.author, t.at, t.text.as_str(), t.username.as_deref()), (ALICE, T0, "great wallet", Some("alice")));
        assert!(response.validity.timestamps.is_valid_for(T0));
        assert!(response.validity.timestamps.is_valid_for(T0 + TIME_WINDOW_MS - 1));
        assert!(!response.validity.timestamps.is_valid_for(T0 + TIME_WINDOW_MS));
        assert!(!response.validity.timestamps.is_valid_for(T0 - 1));
    }

    #[test]
    fn posts_are_listed_newest_first_with_distinct_authors() {
        let mut chain = Chain::default();
        chain.try_post(ALICE, &post("first", T0)).unwrap();
        chain.try_post(BOB, &post("second", T0 + 1)).unwrap();
        let listing = chain.listing(SUB);
        let head = listing.head.clone().unwrap();
        assert_eq!((head.count, head.first_at, head.last_at), (2, T0, T0 + 1));
        let authors: Vec<_> = listing.items.iter().map(|(_, t)| (t.author, t.seq)).collect();
        assert_eq!(authors, vec![(BOB, 1), (ALICE, 0)]);
        assert_eq!(listing.items[0].1.prev, Some(node_address(&PROGRAM, SUB, &ALICE)));
        assert_eq!(listing.items[1].1.prev, None);
        assert_eq!(chain.events.len(), 2);
        assert_eq!(chain.events[0].selector, sha256(POSTED_EVENT.as_bytes())[..8]);
    }

    #[test]
    fn one_testimonial_per_account_and_submission() {
        let mut chain = Chain::default();
        chain.try_post(ALICE, &post("first", T0)).unwrap();
        let before = chain.states.clone();
        let error = chain.try_post(ALICE, &post("again", T0 + 1)).unwrap_err();
        assert!(error.contains("already posted"), "{error}");
        assert_eq!(chain.states, before, "a refused post changes nothing");
        // Another submission is another list.
        let other = Instruction::Post { submission: "lp-0021/other".into(), username: None, text: "x".into(), now: T0 };
        chain.try_post(ALICE, &other).unwrap();
        assert_eq!(chain.listing("lp-0021/other").items.len(), 1);
        assert_eq!(chain.listing(SUB).items.len(), 1);
    }

    #[test]
    #[should_panic(expected = "the author must sign")]
    fn an_unsigned_post_is_refused() {
        let instruction = post("x", T0);
        let _ = instruct(&root_input(ALICE, false, &instruction), instruction);
    }

    #[test]
    #[should_panic(expected = "root message")]
    fn a_relayed_post_is_refused() {
        let instruction = post("x", T0);
        let mut input = root_input(ALICE, true, &instruction);
        input.from = Some(Actor::new(ALICE, AccountId::new([9; 32])));
        let _ = handle(&input, Message::Instruction(instruction));
    }

    #[test]
    fn effects_are_only_from_this_program_and_only_along_the_chain() {
        let draft = Testimonial {
            schema: SCHEMA,
            submission: SUB.into(),
            author: ALICE,
            seq: 0,
            at: T0,
            username: None,
            text: "x".into(),
            prev: None,
        };
        let input = |to: AccountId, from: Option<Actor>| ReceiveInput {
            receiver: Actor::new(to, PROGRAM),
            from,
            is_authorized: true,
            pre_state: Default::default(),
            message: Vec::new(),
        };
        let head = head_address(&PROGRAM, SUB);
        let node = node_address(&PROGRAM, SUB, &ALICE);
        let refused = |i: ReceiveInput, e: Effect| std::panic::catch_unwind(|| handle(&i, Message::Effect(e))).is_err();
        // From outside the program, or as a root message.
        assert!(refused(input(head, None), Effect::Index(draft.clone())));
        assert!(refused(input(head, Some(Actor::new(ALICE, AccountId::new([9; 32])))), Effect::Index(draft.clone())));
        // Bob's actor cannot index a testimonial in Alice's name.
        assert!(refused(input(head, Some(Actor::new(BOB, PROGRAM))), Effect::Index(draft.clone())));
        // Only the head writes, and only at the author's address.
        assert!(refused(input(node, Some(Actor::new(ALICE, PROGRAM))), Effect::Write(draft.clone())));
        assert!(refused(input(node_address(&PROGRAM, SUB, &BOB), Some(Actor::new(head, PROGRAM))), Effect::Write(draft.clone())));
        assert!(!refused(input(node, Some(Actor::new(head, PROGRAM))), Effect::Write(draft)));
    }

    #[test]
    fn limits() {
        assert!(validate_submission("lp-0021/names-wallet").is_ok());
        assert!(validate_submission(&"a".repeat(MAX_SUBMISSION)).is_ok());
        for bad in ["", "LP-0021", "-lp", "lp 0021", "lp#1", &"a".repeat(MAX_SUBMISSION + 1)] {
            assert!(validate_submission(bad).is_err(), "{bad}");
        }
        assert!(validate_username("alice").is_ok());
        assert!(validate_username("Ünïcode name").is_ok());
        for bad in ["", " alice", "alice ", "a\u{7}b", &"x".repeat(MAX_USERNAME + 1)] {
            assert!(validate_username(bad).is_err(), "{bad:?}");
        }
        assert!(validate_text("line one\nline two\ttabbed").is_ok());
        assert!(validate_text(&"x".repeat(MAX_TEXT)).is_ok());
        for bad in ["", "   \n", "bell\u{7}", "cr\r", &"x".repeat(MAX_TEXT + 1)] {
            assert!(validate_text(bad).is_err(), "{bad:?}");
        }
        let mut chain = Chain::default();
        let long = Instruction::Post { submission: SUB.into(), username: None, text: "x".repeat(MAX_TEXT + 1), now: T0 };
        assert!(chain.try_post(ALICE, &long).unwrap_err().contains("at most"));
    }

    #[test]
    fn worst_case_record_sizes() {
        let t = Testimonial {
            schema: SCHEMA,
            submission: "a".repeat(MAX_SUBMISSION),
            author: ALICE,
            seq: u64::MAX,
            at: T0,
            username: Some("u".repeat(MAX_USERNAME)),
            text: "x".repeat(MAX_TEXT),
            prev: Some(BOB),
        };
        let h = Head { schema: SCHEMA, submission: "a".repeat(MAX_SUBMISSION), count: 1, newest: Some(BOB), first_at: 1, last_at: 2 };
        assert_eq!(to_bytes(&t).len(), 1 + 68 + 32 + 8 + 8 + 37 + 1028 + 33);
        assert_eq!(to_bytes(&h).len(), 1 + 68 + 8 + 33 + 16);
    }

    #[test]
    fn walk_rejects_a_tampered_list() {
        let mut chain = Chain::default();
        chain.try_post(ALICE, &post("a", T0)).unwrap();
        chain.try_post(BOB, &post("b", T0)).unwrap();
        let read = |states: &HashMap<AccountId, Vec<u8>>| {
            let states = states.clone();
            walk(&PROGRAM, SUB, move |a| Ok::<_, String>(states.get(a).cloned().unwrap_or_default()))
        };
        // A record moved to another address is caught.
        let mut moved = chain.states.clone();
        let bob = node_address(&PROGRAM, SUB, &BOB);
        let alice = node_address(&PROGRAM, SUB, &ALICE);
        let bobs = moved[&bob].clone();
        moved.insert(alice, bobs);
        assert!(read(&moved).unwrap_err().contains("not the testimonial account"));
        // A head that overcounts is caught.
        let mut over = chain.states.clone();
        let head_at = head_address(&PROGRAM, SUB);
        let mut head: Head = borsh::from_slice(&over[&head_at]).unwrap();
        head.count = 3;
        over.insert(head_at, to_bytes(&head));
        assert!(read(&over).is_err());
        assert_eq!(walk(&PROGRAM, "lp-0021/none", |_| Ok::<_, String>(Vec::new())).unwrap().items.len(), 0);
    }

    #[test]
    fn months_and_the_adoption_bar() {
        assert_eq!(month_of(0), "1970-01");
        assert_eq!(format_time(T0), "2027-01-15 08:00:00 UTC");
        assert_eq!(month_of(1_798_761_599_999), "2026-12"); // 2026-12-31 23:59:59.999
        assert_eq!(month_of(1_798_761_600_000), "2027-01");
        assert_eq!(month_of(951_782_400_000), "2000-02"); // 2000-02-29, a leap day
        let mut chain = Chain::default();
        let dec = 1_797_000_000_000; // 2026-12-11
        let jan = T0; // 2027-01-15
        for i in 0..150u8 {
            let author = AccountId::new([i.wrapping_add(10); 32]);
            let now = if i < 75 { dec + u64::from(i) } else { jan + u64::from(i) };
            chain.try_post(author, &post("vouch", now)).unwrap();
        }
        let summary = summarize(&chain.listing(SUB));
        assert_eq!((summary.total, summary.distinct_authors), (150, 150));
        assert_eq!(summary.per_month, BTreeMap::from([("2026-12".into(), 75), ("2027-01".into(), 75)]));
        assert_eq!(summary.qualifying_months, vec!["2026-12".to_string(), "2027-01".to_string()]);
        assert!(summary.meets_adoption);
        // 149 is not enough.
        let mut short = chain.listing(SUB);
        short.items.pop();
        assert!(!summarize(&short).meets_adoption);
    }
}
