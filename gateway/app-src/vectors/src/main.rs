//! Two jobs, both so the hand-written JavaScript encoder is checked by the real
//! thing rather than by reading it:
//!
//! - `gen`  prints vectors.json: bytes made by the real Rust types, which
//!   gateway/public/app/lez-tx.test.mjs reproduces byte for byte.
//! - `run <scenario.json>` takes transactions made in JavaScript (engine-run.mjs),
//!   decodes them with the real borsh types and runs them through the real engine
//!   and the real guest ELF. Prints a JSON report of what happened.
//!
//! LEZ v0.4.0: public accounts sign with a per-nonce key derived from a
//! `PublicAccountSecret`, and each witness entry commits to the signer's next key.

use std::collections::BTreeMap;

use borsh::{BorshDeserialize, to_vec};
use lee::{
    Account, Authority, FeeDeclaration, GenesisBuilder, PublicAccountEvidence, PublicTransaction,
    SignedTransaction as _, V03State, ValidatedStateDiff, is_fee_authorized, native_token,
    program::{DEFAULT_PUBLIC_CYCLE_BUDGET, Program},
    public_transaction::{Message as TxMessage, WitnessSet},
    signature::{
        INITIAL_KEY_TYPE, PublicAccountSecret, Schnorr, Signer, initial_key, key_state,
        public_account_id, signing_key,
    },
};
use lee_core::{
    account::{AccountId, Actor, Nonce},
    program::{PdaSeed, SystemProgramName},
    signing_key::{KeyType, SigningKeyState, signature_program_id},
};
use names_core::{
    Commitment, Instruction, LengthProof, MIN_COMMIT_AGE_MS, MODE_DEPOSIT, Message, NameId, Settings,
    VERSION, claim_leaf, claim_proof, claim_root, commitment, commitment_address, commitment_seed,
    deposit_address, name_seeds, owner_tag, page_address, record_address, settings_address,
    settings_seed, shortlist,
};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

const T0: u64 = 1_800_000_000_000;
const DEPOSIT: u128 = 1_000_000;
const NOW: u64 = 1_700_000_000_000;
const NAMED_ID: &str = "e04a915482013a12c5ac4884c8a182493bedbbc39cb93fcaaf8216486cb617e7";
/// `public_transaction/message.rs` PREFIX (private there; `Message::hash` is crate-private).
const MESSAGE_PREFIX: &[u8; 32] = b"/LEE/v0.3/Message/Public/\x00\x00\x00\x00\x00\x00\x00";
/// `lez/wallet/src/lib.rs` `max_fee_for(1_000)`: (1_000 + 100_000) × 64.
const TRANSFER_MAX_FEE: u128 = 6_464_000;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len() / 2)
        .map(|at| u8::from_str_radix(&text[at * 2..at * 2 + 2], 16).expect("hex"))
        .collect()
}

fn account_secret(byte: u8) -> PublicAccountSecret {
    PublicAccountSecret::new([byte; 32])
}

fn id_of(byte: u8) -> AccountId {
    public_account_id(&account_secret(byte))
}

fn signer(byte: u8, nonce: u128) -> Signer {
    Signer::new(&account_secret(byte), Nonce(nonce), INITIAL_KEY_TYPE, INITIAL_KEY_TYPE)
}

fn key_hex(state: &SigningKeyState) -> String {
    hex(&to_vec(state).expect("33 bytes"))
}

fn message_hash(message: &TxMessage) -> [u8; 32] {
    Sha256::new().chain_update(MESSAGE_PREFIX).chain_update(message.to_bytes()).finalize().into()
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
    json!({ "bytes": hex(&message.to_bytes()), "hash": hex(&message_hash(message)) })
}

/// What the account of secret `[byte; 32]` signs with: its id, its nonce-0 key, and its key
/// states and x-only public keys at nonces 0 to 4.
fn keys_json(byte: u8) -> Value {
    let secret = account_secret(byte);
    let nonces = 0..5_u128;
    json!({
        "accountId": public_account_id(&secret).to_string(),
        "initialKey": key_hex(&initial_key(&secret)),
        "keyStates": nonces.clone().map(|n| key_hex(&key_state(&secret, KeyType::Schnorr, Nonce(n)))).collect::<Vec<_>>(),
        "publicKeys": nonces.map(|n| hex(signing_key::<Schnorr>(&secret, Nonce(n)).1.value())).collect::<Vec<_>>(),
    })
}

/// A transaction Rust signed: its bytes, and what each signer signed.
fn signed_json(message: TxMessage, signers: &[Signer]) -> Value {
    let next: Vec<SigningKeyState> = {
        let mut sorted: Vec<&Signer> = signers.iter().collect();
        sorted.sort_by_key(|signer| signer.account_id());
        sorted.iter().map(|signer| signer.next()).collect()
    };
    let keyed = message.message_with_keys(&next);
    let witness = WitnessSet::sign(&message, signers);
    let tx = PublicTransaction::new(message, witness);
    json!({
        "tx": hex(&to_vec(&tx).expect("borsh")),
        "hash": hex(&tx.hash()),
        "messageWithKeys": hex(&keyed),
        "next": next.iter().map(key_hex).collect::<Vec<_>>(),
        "authDataLengths": tx.witness_set().entries().iter().map(|entry| entry.auth_data.len()).collect::<Vec<_>>(),
    })
}

fn wallet_transfer(alice: AccountId, bob: AccountId, nonce: u128, transfer: &native_token::Message) -> TxMessage {
    TxMessage::try_new_with_fees(
        Actor::native_balance(alice),
        [(Actor::native_balance(alice), Authority::Signer), (Actor::native_balance(bob), Authority::None)],
        BTreeMap::from([(alice, Nonce(nonce))]),
        transfer.clone(),
        FeeDeclaration::new(alice, 1_000, 0, TRANSFER_MAX_FEE),
    )
    .expect("a message")
}

fn generate() -> Value {
    // message.rs a_public_message_has_a_pinned_layout_and_hash / message_with_keys_has_known_answers
    let to = Actor::new(AccountId::new([42; 32]), AccountId::new([0; 32]));
    let pinned = TxMessage::new_preserialized(
        to,
        vec![0],
        [(to, Authority::Signer)],
        BTreeMap::from([(to.account_id, Nonce(1))]),
        None,
    );
    let state = |commitment: u8| SigningKeyState { key_type: KeyType::Schnorr, key_commitment: [commitment; 32] };

    // Everything that sorts, listed out of order, with a fee and both kinds of evidence. The
    // actors carry the authority the JavaScript gives them after merging its duplicates.
    let (high, low) = (AccountId::new([0xF0; 32]), AccountId::new([0x10; 32]));
    let program = AccountId::new([0x77; 32]);
    let sorted = TxMessage::new(
        Actor::new(high, program),
        vec![1, 2, 3],
        [
            (Actor::new(high, program), Authority::Signer),
            (Actor::native_balance(high), Authority::Signer),
            (Actor::new(low, program), Authority::None),
            (Actor::native_balance(low), Authority::None),
        ],
        BTreeMap::from([(high, Nonce(9)), (low, Nonce(u128::from(u64::MAX) + 1))]),
        Some(FeeDeclaration::new(high, 250_000, 3, 12_345_678_901_234)),
        vec![
            PublicAccountEvidence::Pda { program, seed: PdaSeed::new([0x5D; 32]) },
            PublicAccountEvidence::Key(initial_key(&account_secret(1))),
        ],
    );

    // What the wallet's NativeTokenTransfer builds: the sender's balance authorized, the
    // recipient's not, the sender paying a fee capped at max_fee_for(gas_limit).
    let (alice, bob) = (id_of(1), id_of(2));
    let transfer = native_token::Message::Transfer { to: bob, amount: 1337 };
    let transfer_message = wallet_transfer(alice, bob, 3, &transfer);
    // Two signers, so the witness entries sort and both sign every next key.
    let both = TxMessage::try_new_with_fees(
        Actor::native_balance(alice),
        [(Actor::native_balance(alice), Authority::Signer), (Actor::native_balance(bob), Authority::Signer)],
        BTreeMap::from([(alice, Nonce(1)), (bob, Nonce(0))]),
        transfer.clone(),
        FeeDeclaration::new(alice, 1_000, 0, TRANSFER_MAX_FEE),
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
    let commitment_state = |public| hex(&to_vec(&Commitment { version: VERSION, made: NOW, public }).expect("borsh"));
    json!({
        "pinned": message_json(&pinned),
        "pinnedWithKeys": {
            "seven": hex(&pinned.message_with_keys(&[state(7)])),
            "oneTwoThree": hex(&pinned.message_with_keys(&[state(1), state(2), state(3)])),
        },
        "sorted": message_json(&sorted),
        "walletTransfer": message_json(&transfer_message),
        "walletTransferFirst": message_json(&wallet_transfer(alice, bob, 0, &transfer)),
        "transferInstruction": hex(&to_vec(&transfer).expect("borsh")),
        "keys": { "one": keys_json(1), "two": keys_json(2), "seven": keys_json(7) },
        "signed": {
            "transfer": signed_json(transfer_message.clone(), &[signer(1, 3)]),
            "transferFirst": signed_json(wallet_transfer(alice, bob, 0, &transfer), &[signer(1, 0)]),
            "twoSigners": signed_json(both, &[signer(2, 0), signer(1, 1)]),
        },
        "clock": {
            // clock_core: CLOCK_PROGRAM_ACCOUNT_ID holds the state of clock_account_id().
            "account": AccountId::new(*b"/LEZ/ClockProgramAccount/0000000").to_string(),
            "program": AccountId::from_system_program_name(&SystemProgramName::new(b"clock")).to_string(),
            "signatureProgram": signature_program_id().to_string(),
        },
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
            "commitPublic": instruction(Instruction::CommitPublic { commitment: [0xC0; 32], now: NOW }),
            "claim": instruction(Instruction::Claim { id, sealed: vec![0xEE], now: NOW, proof: vec![[1; 32], [2; 32]] }),
        },
        "commitmentState": { "made": NOW, "public": commitment_state(true), "private": commitment_state(false) },
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
    let mut state = GenesisBuilder::new()
        .with_public_accounts(
            [(alice, 10 * DEPOSIT), (id_of(3), 10 * DEPOSIT)].map(|(id, balance)| (id, Account::funded(balance))),
        )
        .with_programs_at([(program, Program::new(elf.into()).expect("the guest loads"), false)])
        .build();

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
    let deployer = Actor::new(program, program);
    let tx = TxMessage::new(
        deployer,
        to_vec(&configure).expect("borsh"),
        [(settings, Authority::None), (deployer, Authority::Signer)],
        BTreeMap::from([(program, Nonce(0))]),
        None,
        vec![PublicAccountEvidence::Pda { program, seed: settings_seed() }],
    );
    let witness = WitnessSet::sign(&tx, &[signer(9, 0)]);
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
        assert_eq!(hex(&message_hash(tx.message())), step["messageHash"].as_str().expect("hash"), "{label}: message hash");
        // Nonces name exactly the signers, authority only where they sign, auth data well formed.
        tx.check_stateless().unwrap_or_else(|error| panic!("{label}: {error}"));
        assert!(is_fee_authorized(tx.message()), "{label}: fee authorization");

        // The sequencer runs a public transaction's action at its declared gas limit
        // (lez/chain_state/src/apply.rs), so a limit set too low shows up here as OutOfGas.
        let budget = tx.message().fee.map_or(DEFAULT_PUBLIC_CYCLE_BUDGET, |fee| fee.gas_limit);
        let now = step["now"].as_u64().expect("now");
        // Authenticates each signer against the key its account holds now (the next key the
        // last transaction committed to) before running anything.
        let metered = ValidatedStateDiff::from_public_transaction_with_cycle_budget(&tx, &state, block, now, budget);
        let (outcome, cycles) = match metered {
            Ok((diff, charge)) => {
                state.apply_state_diff(diff).unwrap_or_else(|error| panic!("{label}: applying: {error:?}"));
                ("ok".to_owned(), Some(charge.cycles))
            }
            Err(error) => (format!("{error:?}"), None),
        };
        let expected = step["expect"].as_str().expect("expect");
        let agreed = if expected == "ok" { outcome == "ok" } else { outcome.contains(expected) };
        assert!(agreed, "{label}: expected {expected}, got {outcome}");
        println!(
            "{:>9} cycles  {:>9} limit  {:>5} wire bytes  {label}",
            cycles.map_or("-".to_owned(), |c| c.to_string()),
            budget,
            wire.len()
        );
        steps.push(json!({
            "label": label,
            "cycles": cycles,
            "gasLimit": tx.message().fee.map(|fee| fee.gas_limit),
            "wireBytes": wire.len(),
            "outcome": outcome,
        }));
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
    // Each signer's current key, as the signature program holds it: the `next` its last
    // transaction committed to.
    let signing_keys: BTreeMap<String, Value> = scenario["signers"]
        .as_array()
        .expect("signers")
        .iter()
        .map(|account| {
            let text = account.as_str().expect("account");
            let found = state.get_account_by_id(text.parse().expect("an account"));
            let key = found.data.actor_state(signature_program_id());
            (text.to_owned(), json!({ "nonce": found.nonce.0.to_string(), "key": hex(key) }))
        })
        .collect();
    assert_eq!(
        serde_json::to_value(&signing_keys).expect("JSON"),
        scenario["expectedKeys"],
        "the engine's current keys are the next keys the JavaScript committed to"
    );
    let settings_state = state.get_account_by_id(settings_address(&program));
    let stored: Settings = borsh::from_slice(settings_state.data.actor_state(program)).expect("settings");
    json!({
        "program": scenario["program"],
        "account": scenario["account"],
        "steps": steps,
        "views": views,
        "balances": balances,
        "signingKeys": signing_keys,
        "expectedKeys": scenario["expectedKeys"],
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
