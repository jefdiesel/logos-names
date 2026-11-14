//! The real testimonial guest on LEZ v0.4.0-rc3's state machine. Run with
//! `cargo test --release -p testimonial_engine_test -- --nocapture`.

use lee_core::account::{AccountId, Actor};
use testimonial_core::{
    ADOPTION_TOTAL, Effect, Head, MAX_TEXT, Message, POSTED_EVENT, Posted, SCHEMA, TIME_WINDOW_MS, Testimonial,
    head_address, head_seed, node_address, sha256, summarize,
};
use testimonial_engine_test::{Engine, Key, Send, T0, key};

const SUB: &str = "lp-0021/names-wallet";

fn engine() -> Engine {
    Engine::new(AccountId::new([0x7e; 32]), AccountId::new([0x7f; 32]))
}

#[track_caller]
fn refused(e: &mut Engine, what: &str, expected: &str, run: impl FnOnce(&mut Engine) -> Result<testimonial_engine_test::Applied, String>) {
    let before = e.state.clone();
    let error = match run(e) {
        Ok(_) => panic!("{what}: was accepted, expected a refusal containing {expected:?}"),
        Err(error) => error,
    };
    assert!(error.contains(expected), "{what}: refused for the wrong reason:\n  {error}\n  expected {expected:?}");
    assert!(e.state == before, "{what}: a refused transaction changed state");
    println!("refused {what}  ({expected})");
}

#[test]
fn t01_post_then_list_from_chain_state() {
    let mut e = engine();
    let alice = key(2);
    let applied = e.try_post(&alice, SUB, Some("alice"), "Names Wallet just works.", T0, T0, false).expect("alice posts");
    println!("ok      post  ({} cycles, {} tx bytes)", applied.cycles, applied.tx_bytes);

    let listing = e.listing(SUB).expect("a consistent list");
    let head = listing.head.clone().expect("a head");
    assert_eq!((head.count, head.first_at, head.last_at), (1, T0, T0));
    let (at, t) = &listing.items[0];
    assert_eq!(*at, node_address(&e.program, SUB, &alice.id));
    assert_eq!(
        t,
        &Testimonial {
            schema: SCHEMA,
            submission: SUB.into(),
            author: alice.id,
            seq: 0,
            at: T0,
            username: Some("alice".into()),
            text: "Names Wallet just works.".into(),
            prev: None,
        }
    );
    // The Posted event.
    let selector: [u8; 8] = sha256(POSTED_EVENT.as_bytes())[..8].try_into().unwrap();
    let posted: Vec<Posted> = applied
        .events
        .iter()
        .filter(|ev| ev.event.selector == selector)
        .map(|ev| borsh::from_slice(&ev.event.data).unwrap())
        .collect();
    assert_eq!(posted, vec![Posted { submission: SUB.into(), author: alice.id, seq: 0, at: T0 }]);
    // The author's own actor is untouched; the head lives where the client derives it.
    assert!(e.read(&alice.id).is_empty());
    let stored: Head = borsh::from_slice(&e.read(&head_address(&e.program, SUB))).unwrap();
    assert_eq!(stored, head);
}

#[test]
fn t02_many_authors_in_one_block_form_one_list() {
    let mut e = engine();
    let keys: Vec<Key> = (2..=6).map(key).collect();
    for (i, k) in keys.iter().enumerate() {
        e.try_post(k, SUB, None, &format!("testimonial {i}"), T0, T0 + 5, true).expect("posts");
    }
    assert_eq!(e.block, 1, "all in one block");
    let listing = e.listing(SUB).unwrap();
    let order: Vec<(AccountId, u64)> = listing.items.iter().map(|(_, t)| (t.author, t.seq)).collect();
    let expected: Vec<(AccountId, u64)> = keys.iter().enumerate().rev().map(|(i, k)| (k.id, i as u64)).collect();
    assert_eq!(order, expected, "newest first, numbered in order");
    println!("ok      5 posts in one block, listed newest first");
}

#[test]
fn t03_one_testimonial_per_account_and_submission() {
    let mut e = engine();
    let alice = key(2);
    e.try_post(&alice, SUB, None, "first", T0, T0, false).unwrap();
    refused(&mut e, "a second testimonial from the same account", "already posted a testimonial for this submission", |e| {
        e.try_post(&alice, SUB, Some("other name"), "second", T0 + 1, T0 + 1, false)
    });
    // Another submission is another list.
    e.try_post(&alice, "lp-0099/someone-else", None, "also good", T0 + 2, T0 + 2, false).unwrap();
    assert_eq!(e.listing(SUB).unwrap().items.len(), 1);
    assert_eq!(e.listing("lp-0099/someone-else").unwrap().items.len(), 1);
    println!("ok      another submission is another list");
}

#[test]
fn t04_only_the_author_signs_its_testimonial() {
    let mut e = engine();
    let alice = key(2);
    let bob = key(3);
    // Alice's account exists (else the engine refuses it as unadmitted before the program
    // runs). The post is rooted at her actor but signed by bob.
    e.fund(&[alice.id]);
    refused(&mut e, "a post in alice's name signed by bob", "the author must sign", |e| {
        let message = borsh::to_vec(&Message::Instruction(testimonial_core::Instruction::Post {
            submission: SUB.into(),
            username: None,
            text: "forged".into(),
            now: T0,
        }))
        .unwrap();
        let actors = testimonial_core::actors_to_declare(&e.program, &alice.id, SUB);
        e.try_tx(Actor::new(alice.id, e.program), message, actors, &[&bob], false, T0)
    });
}

#[test]
fn t05_a_program_cannot_relay_a_post_or_an_effect() {
    let mut e = engine();
    let alice = key(2);
    let (program, forwarder) = (e.program, e.forwarder);
    let post = borsh::to_vec(&Message::Instruction(testimonial_core::Instruction::Post {
        submission: SUB.into(),
        username: None,
        text: "relayed".into(),
        now: T0,
    }))
    .unwrap();
    // Alice signs a transaction to another program, which relays a Post to her actor: her
    // signature authorizes her actor there too, but the post is not the root message.
    let relay = |data: Vec<u8>, to: AccountId, seeds: Vec<[u8; 32]>| {
        borsh::to_vec(&vec![Send { to_account: *to.value(), to_program: *program.value(), data, pda_seeds: seeds }]).unwrap()
    };
    let mut actors = testimonial_core::actors_to_declare(&program, &alice.id, SUB);
    actors.push(Actor::new(alice.id, program));
    refused(&mut e, "a Post relayed by another program", "root message", |e| {
        e.try_tx(Actor::new(alice.id, forwarder), relay(post.clone(), alice.id, vec![]), actors.clone(), &[&alice], false, T0)
    });
    // Effects straight from a transaction, or from another program, are refused too. Bob posts
    // first so the head exists (an effect to a missing account is refused earlier, as
    // unadmitted); alice's testimonial account stays empty, so a Write there would succeed if
    // it got through.
    let bob = key(3);
    e.try_post(&bob, SUB, None, "real", T0, T0, false).expect("bob posts");
    e.fund(&[node_address(&program, SUB, &alice.id)]);
    let draft = Testimonial {
        schema: SCHEMA,
        submission: SUB.into(),
        author: alice.id,
        seq: 0,
        at: T0,
        username: None,
        text: "forged".into(),
        prev: None,
    };
    let write = borsh::to_vec(&Message::Effect(Effect::Write(draft.clone()))).unwrap();
    let node = node_address(&program, SUB, &alice.id);
    refused(&mut e, "an effect as a transaction's root message", "only this program may send an effect", |e| {
        e.try_tx(Actor::new(node, program), write.clone(), vec![], &[&alice], false, T0)
    });
    let index = borsh::to_vec(&Message::Effect(Effect::Index(draft))).unwrap();
    let head = head_address(&program, SUB);
    refused(&mut e, "an effect relayed by another program", "only this program may send an effect", |e| {
        e.try_tx(
            Actor::new(alice.id, forwarder),
            relay(index.clone(), head, vec![*head_seed(SUB).as_bytes()]),
            vec![Actor::new(head, program), Actor::new(node, program)],
            &[&alice],
            false,
            T0,
        )
    });
}

#[test]
fn t06_the_block_time_bounds_the_stored_time() {
    let mut e = engine();
    let alice = key(2);
    refused(&mut e, "a block before the claimed time", "OutOfValidityWindow", |e| {
        e.try_post(&alice, SUB, None, "early", T0, T0 - 1, false)
    });
    refused(&mut e, "a block 10 minutes after the claimed time", "OutOfValidityWindow", |e| {
        e.try_post(&alice, SUB, None, "late", T0, T0 + TIME_WINDOW_MS, false)
    });
    e.try_post(&alice, SUB, None, "in time", T0, T0 + TIME_WINDOW_MS - 1, false).expect("last millisecond of the window");
    println!("ok      the last millisecond of the window");
}

#[test]
fn t07_limits_are_enforced_on_chain() {
    let mut e = engine();
    let alice = key(2);
    refused(&mut e, "text over the limit", "at most", |e| {
        e.try_post(&alice, SUB, None, &"x".repeat(MAX_TEXT + 1), T0, T0, false)
    });
    refused(&mut e, "an uppercase submission id", "a submission id is", |e| {
        e.try_post(&alice, "LP-0021", None, "x", T0, T0, false)
    });
    refused(&mut e, "a username with a control character", "a username", |e| {
        e.try_post(&alice, SUB, Some("al\u{7}ice"), "x", T0, T0, false)
    });
    let applied = e
        .try_post(&alice, &"a".repeat(64), Some(&"u".repeat(32)), &"x".repeat(MAX_TEXT), T0, T0, false)
        .expect("everything at its limit");
    println!("ok      worst-case post  ({} cycles, {} tx bytes)", applied.cycles, applied.tx_bytes);
}

#[test]
fn t08_adoption_counts_from_chain_data() {
    let mut e = engine();
    let dec = 1_797_000_000_000; // 2026-12-11
    let mut cycles = 0;
    for i in 0..ADOPTION_TOTAL {
        let k = key(u8::try_from(10 + i).unwrap());
        let now = if i < 70 { dec + i * 60_000 } else { T0 + i * 60_000 };
        cycles = cycles.max(e.try_post(&k, SUB, None, "vouching", now, now + 2_000, false).expect("posts").cycles);
    }
    let summary = summarize(&e.listing(SUB).unwrap());
    println!("ok      {} testimonials, {} distinct accounts, per month {:?}, max {cycles} cycles", summary.total, summary.distinct_authors, summary.per_month);
    assert_eq!((summary.total, summary.distinct_authors), (150, 150));
    assert_eq!(summary.per_month.get("2026-12"), Some(&70));
    assert_eq!(summary.per_month.get("2027-01"), Some(&80));
    assert!(summary.meets_adoption);
}
