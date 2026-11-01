//! Two jobs, both so the hand-written JavaScript encoder is checked by the real
//! thing rather than by reading it:
//!
//! - `gen`  prints vectors.json: bytes made by the real Rust types, which
//!   gateway/public/app/lez-tx.test.mjs reproduces byte for byte.
//! - `run <scenario.json>` takes transactions made in JavaScript (engine-run.mjs),
//!   decodes them with the real borsh types and runs them through the real engine
//!   and the real guest ELF. Prints a JSON report of what happened.

use std::collections::BTreeMap;

use borsh::{BorshDeserialize, to_vec};
use lee::{
    Account, FeeDeclaration, PrivateKey, PublicAccountEvidence, PublicKey, PublicTransaction,
    V03State, ValidatedStateDiff, is_fee_authorized, native_token,
    program::{DEFAULT_PUBLIC_CYCLE_BUDGET, Program},
    public_transaction::{Message as TxMessage, WitnessSet},
};
use lee_core::{
    account::{AccountId, Actor, Nonce},
    program::PdaSeed,
};
use names_core::{
    Instruction, LengthProof, MIN_COMMIT_AGE_MS, MODE_DEPOSIT, Message, NameId, Settings,
    claim_leaf, claim_proof, claim_root, commitment, commitment_address, commitment_seed,
    deposit_address, name_seeds, owner_tag, page_address, record_address, settings_address,
    settings_seed, shortlist,
};
use serde_json::{Value, json};

const T0: u64 = 1_800_000_000_000;
const DEPOSIT: u128 = 1_000_000;
const NOW: u64 = 1_700_000_000_000;
const NAMED_ID: &str = "e04a915482013a12c5ac4884c8a182493bedbbc39cb93fcaaf8216486cb617e7";

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len() / 2)
        .map(|at| u8::from_str_radix(&text[at * 2..at * 2 + 2], 16).expect("hex"))
        .collect()
}

fn key(byte: u8) -> PrivateKey {
    PrivateKey::try_new([byte; 32]).expect("a valid key")
}

fn id_of(byte: u8) -> AccountId {
    AccountId::from(&PublicKey::new_from_private_key(&key(byte)))
}

fn short_list() -> shortlist::ShortList {
    let leaves = include_bytes!("../../../../shortlist/shortlist.bin")
        .chunks_exact(33)
        .map(|chunk| shortlist::Leaf { id: chunk[..32].try_into().expect("32 bytes"), tier: chunk[32] })
        .collect();
    shortlist::ShortList::from_sorted_leaves(leaves).expect("a sorted list")
}

/// A length proof in the JSON the names-proof endpoint serves.
fn proof_json(id: &NameId) -> Value {
    let LengthProof::NonMembership(proof) = short_list().prove(id).expect("a proof") else {
        panic!("test ids are not on the list");
    };
    let path = |path: &[[u8; 32]]| path.iter().map(|hash| hex(hash)).collect::<Vec<_>>();
    json!({
        "left": { "id": hex(&proof.left.id), "tier": proof.left.tier },
        "leftIndex": proof.left_index,
        "leftPath": path(&proof.left_path),
        "right": { "id": hex(&proof.right.id), "tier": proof.right.tier },
        "rightPath": path(&proof.right_path),
    })
}

fn message_json(message: &TxMessage) -> Value {
    json!({ "bytes": hex(&message.to_bytes()), "hash": hex(&message.hash()) })
}

fn generate() -> Value {
    // message.rs a_public_message_has_a_pinned_layout_and_hash
    let to = Actor::new(AccountId::new([42; 32]), AccountId::new([0; 32]));
    let pinned = TxMessage::new_preserialized(
        to,
        vec![0],
        [to],
        BTreeMap::from([(to.account_id, Nonce(1))]),
        None,
    );

    // Everything that sorts, listed out of order, with a fee and both kinds of evidence.
    let (high, low) = (AccountId::new([0xF0; 32]), AccountId::new([0x10; 32]));
    let program = AccountId::new([0x77; 32]);
    let sorted = TxMessage::new(
        Actor::new(high, program),
        vec![1, 2, 3],
        [
            Actor::new(high, program),
            Actor::native_balance(high),
            Actor::new(low, program),
            Actor::native_balance(low),
            Actor::new(low, program),
        ],
        BTreeMap::from([(high, Nonce(9)), (low, Nonce(u128::from(u64::MAX) + 1))]),
        Some(FeeDeclaration::new(high, 250_000, 3, 12_345_678_901_234)),
        vec![
            PublicAccountEvidence::Pda { program, seed: PdaSeed::new([0x5D; 32]) },
            PublicAccountEvidence::Key(PublicKey::new_from_private_key(&key(1))),
        ],
    );

    // What the wallet's NativeTokenTransfer builds.
    let (alice, bob) = (id_of(1), id_of(2));
    let transfer = native_token::Message::Transfer { to: bob, amount: 1337 };
    let wallet_transfer = TxMessage::try_new_with_fees(
        Actor::native_balance(alice),
        [Actor::native_balance(alice), Actor::native_balance(bob)],
        BTreeMap::from([(alice, Nonce(3))]),
        transfer.clone(),
        FeeDeclaration::new(alice, 1_000, 0, 6_464_000),
    )
    .expect("a message");

    let id: NameId = core::array::from_fn(|index| index as u8);
    let registry = AccountId::new([7; 32]);
    let promised = commitment(&id, &owner_tag(&id, &alice), &[0x5A; 32]);
    let [name_seed, page_seed, deposit_seed] = name_seeds(&id);
    let instruction = |value: Instruction| hex(&to_vec(&Message::Instruction(value)).expect("borsh"));
    let leaves: Vec<_> = (0..5_u8)
        .map(|at| claim_leaf(&AccountId::new([at; 32]), &[at + 100; 32]))
        .collect();
    json!({
        "pinned": message_json(&pinned),
        "sorted": message_json(&sorted),
        "walletTransfer": message_json(&wallet_transfer),
        "transferInstruction": hex(&to_vec(&transfer).expect("borsh")),
        "addresses": {
            "alice": alice.to_string(),
            "bob": bob.to_string(),
            "record": record_address(&registry, &id).to_string(),
            "page": page_address(&registry, &id).to_string(),
            "deposit": deposit_address(&registry, &id).to_string(),
            "settings": settings_address(&registry).to_string(),
            "commitment": commitment_address(&registry, &promised).to_string(),
            "commitmentHash": hex(&promised),
            "ownerTag": hex(&owner_tag(&id, &alice)),
        },
        "seeds": {
            "name": hex(name_seed.as_ref()),
            "page": hex(page_seed.as_ref()),
            "deposit": hex(deposit_seed.as_ref()),
            "settings": hex(settings_seed().as_ref()),
            "commitment": hex(commitment_seed(&promised).as_ref()),
        },
        "instructions": {
            "register": instruction(Instruction::Register {
                id,
                sealed: vec![0xEE, 0xFF],
                term: 1,
                fee: 100_000,
                treasury: Some(AccountId::new([2; 32])),
                now: NOW,
                secret: [0x5A; 32],
                deposit: false,
                length: LengthProof::Membership(shortlist::Membership { index: 7, tier: 3, path: vec![[9; 32]] }),
            }),
            "registerDeposit": instruction(Instruction::Register {
                id,
                sealed: vec![0xEE],
                term: 0,
                fee: DEPOSIT,
                treasury: None,
                now: NOW,
                secret: [0x5A; 32],
                deposit: true,
                length: LengthProof::NonMembership(shortlist::NonMembership {
                    left: shortlist::Leaf { id: [1; 32], tier: 3 },
                    left_path: vec![[3; 32]],
                    right: shortlist::Leaf { id: [2; 32], tier: 1 },
                    right_path: vec![[4; 32], [5; 32]],
                    left_index: 6,
                }),
            }),
            "renew": instruction(Instruction::Renew { id, term: 2, fee: 5, treasury: Some(AccountId::new([2; 32])), now: NOW }),
            "setRecord": instruction(Instruction::SetRecord { id, sealed: vec![0xEE] }),
            "setPage": instruction(Instruction::SetPage { id, sealed: Vec::new() }),
            "transfer": instruction(Instruction::Transfer { id, new_owner: [2; 32] }),
            "release": instruction(Instruction::Release { id, refund: DEPOSIT }),
            "setPrimary": instruction(Instruction::SetPrimary { name: "jef".into() }),
            "commit": instruction(Instruction::Commit { commitment: [0xC0; 32], now: NOW }),
            "claim": instruction(Instruction::Claim { id, sealed: vec![0xEE], now: NOW, proof: vec![[1; 32], [2; 32]] }),
        },
        "claims": {
            "leaves": leaves.iter().map(|leaf| hex(leaf)).collect::<Vec<_>>(),
            "root": hex(&claim_root(&leaves)),
            "proof3": claim_proof(&leaves, 3).iter().map(|hash| hex(hash)).collect::<Vec<_>>(),
            "proof4": claim_proof(&leaves, 4).iter().map(|hash| hex(hash)).collect::<Vec<_>>(),
            "leafExample": hex(&claim_leaf(&AccountId::new([7; 32]), &[8; 32])),
        },
        // The other leaf of the two-leaf claims list `run` configures.
        "scenarioClaimSibling": hex(&claim_leaf(&AccountId::new([0x11; 32]), &[0x22; 32])),
        "lengthProofs": {
            "aa": proof_json(&[0xAA; 32]),
            "bb": proof_json(&[0xBB; 32]),
            "cc": proof_json(&[0xCC; 32]),
            // deriveName("alice-lns"), for the run through the app's own register.js.
            "named": proof_json(&unhex(NAMED_ID).try_into().expect("32 bytes")),
        },
    })
}

fn run_scenario(path: &str) -> Value {
    let scenario: Value = serde_json::from_str(&std::fs::read_to_string(path).expect("a scenario")).expect("JSON");
    let program = id_of(9);
    let alice = id_of(2);
    let held: NameId = [0xCC; 32];
    let old_program = AccountId::new([0x38; 32]);
    let leaves = vec![
        claim_leaf(&record_address(&old_program, &held), &owner_tag(&held, &alice)),
        claim_leaf(&AccountId::new([0x11; 32]), &[0x22; 32]),
    ];
    let elf = names_methods::LOGOS_NAMES_ELF;
    let mut state = V03State::new()
        .with_public_account_balances([
            (alice, 10 * DEPOSIT),
            (id_of(3), 10 * DEPOSIT),
        ])
        .with_named_programs([(program, Program::new(elf.into()).expect("the guest loads"))]);

    // The deployer's Configure is the Rust side's; everything after it is JavaScript's.
    let settings = Actor::new(settings_address(&program), program);
    let configure = Message::Instruction(Instruction::Configure {
        admin: id_of(9),
        treasury: id_of(9),
        prices: [0; 3],
        mode: MODE_DEPOSIT,
        deposits: [DEPOSIT, DEPOSIT],
        claims_root: claim_root(&leaves),
        claims_until: T0 + 10 * MIN_COMMIT_AGE_MS,
        claims_program: old_program,
    });
    let signer = Actor::new(program, program);
    let tx = TxMessage::new(
        signer,
        to_vec(&configure).expect("borsh"),
        [settings, signer],
        BTreeMap::from([(program, Nonce(0))]),
        None,
        vec![PublicAccountEvidence::Pda { program, seed: settings_seed() }],
    );
    let witness = WitnessSet::for_message(&tx, &[&key(9)]);
    state
        .transition_from_public_transaction(&PublicTransaction::new(tx, witness), 1, T0)
        .expect("configure");

    let mut block = 1;
    let mut steps = Vec::new();
    for step in scenario["steps"].as_array().expect("steps") {
        block += 1;
        let label = step["label"].as_str().expect("a label");
        let wire = unhex(step["wire"].as_str().expect("wire"));
        assert_eq!(wire[0], 0, "{label}: LeeTransaction::Public is tag 0");
        // try_from_slice refuses trailing bytes, so this is a strict decode.
        let tx = PublicTransaction::try_from_slice(&wire[1..]).unwrap_or_else(|error| panic!("{label}: {error}"));
        assert_eq!(to_vec(&tx).expect("borsh"), wire[1..], "{label}: not canonical");
        assert_eq!(hex(&tx.hash()), step["hash"].as_str().expect("hash"), "{label}: transaction hash");
        assert_eq!(hex(&tx.message().hash()), step["messageHash"].as_str().expect("hash"), "{label}: message hash");
        assert!(tx.witness_set().is_valid_for(tx.message()), "{label}: signature");
        assert!(is_fee_authorized(tx.message(), tx.witness_set()), "{label}: fee authorization");

        let now = step["now"].as_u64().expect("now");
        let metered = ValidatedStateDiff::from_public_transaction_with_cycle_budget(
            &tx,
            &state,
            block,
            now,
            DEFAULT_PUBLIC_CYCLE_BUDGET,
        );
        let (outcome, cycles) = match metered {
            Ok((_, charge)) => ("ok".to_owned(), Some(charge.cycles)),
            Err(error) => (format!("{error:?}"), None),
        };
        let expected = step["expect"].as_str().expect("expect");
        let agreed = if expected == "ok" { outcome == "ok" } else { outcome.contains(expected) };
        assert!(agreed, "{label}: expected {expected}, got {outcome}");
        if outcome == "ok" {
            state.transition_from_public_transaction(&tx, block, now).expect("the same transition");
        }
        println!("{:>9} cycles  {:>5} wire bytes  {label}", cycles.map_or("-".to_owned(), |c| c.to_string()), wire.len());
        steps.push(json!({ "label": label, "cycles": cycles, "wireBytes": wire.len(), "outcome": outcome }));
    }

    // What the RPC would answer, and what the resolver reads.
    let mut views = serde_json::Map::new();
    for (label, account, selected) in scenario["views"].as_array().expect("views").iter().map(|view| {
        (
            view["label"].as_str().expect("label"),
            view["account"].as_str().expect("account").parse::<AccountId>().expect("an account"),
            view["program"].as_str().expect("program").parse::<AccountId>().expect("a program"),
        )
    }) {
        // The executor's GetAccountView: `get_account_by_id_ref(..).map(|a| a.project([program]))`.
        let view: Option<Account> = state.get_account_by_id_ref(account).map(|found| found.project([selected]));
        views.insert(label.to_owned(), serde_json::to_value(&view).expect("JSON"));
    }
    let balances: BTreeMap<String, String> = scenario["balances"]
        .as_array()
        .expect("balances")
        .iter()
        .map(|account| {
            let text = account.as_str().expect("account");
            let id: AccountId = text.parse().expect("an account");
            (text.to_owned(), state.get_account_by_id(id).data.native_balance().expect("a balance").to_string())
        })
        .collect();
    let settings_state = state.get_account_by_id(settings_address(&program));
    let stored: Settings = borsh::from_slice(settings_state.data.actor_state(program)).expect("settings");
    json!({
        "program": scenario["program"],
        "account": scenario["account"],
        "steps": steps,
        "views": views,
        "balances": balances,
        "claimsUntil": stored.claims_until,
    })
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("gen") => println!("{}", serde_json::to_string_pretty(&generate()).expect("JSON")),
        Some("run") => {
            let report = run_scenario(&args[2]);
            std::fs::write(&args[3], serde_json::to_string_pretty(&report).expect("JSON")).expect("a report");
        }
        _ => panic!("usage: lns_js_vectors gen | run <scenario.json> <report.json>"),
    }
}
