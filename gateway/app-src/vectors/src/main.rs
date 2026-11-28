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

#![recursion_limit = "512"]

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
use lez_nft_core as nft;
use names_core::{
    Commitment, DEFAULT_DEED_CONTENT, DEFAULT_PREMIUM_START, DEFAULT_PRICES, Instruction, LengthProof, Listing,
    MARKET_BUCKETS, MIN_COMMIT_AGE_MS, MODE_DEPOSIT, MODE_FEE, MarketBucket, MarketEntry, Message, NameId, NameRecord,
    PRICE_TIER_5, PRICE_TIER_6_7, PRICE_TIER_8_PLUS, PRICE_TIERS, SITE_BUCKETS, Settings, SiteBucket, SiteDirHeader,
    SiteDirOp, SiteEntry, SubnameKind, SubnameRecord, SubnameSale, TERM_1Y, TERM_30D, TERM_5Y, TERM_PERMANENT, TERMS,
    VERSION, bucket_of, claim_generation, claim_leaf, claim_proof, claim_root, commitment, commitment_address,
    commitment_seed, deposit_address, market_bucket_address, market_bucket_seed, name_seeds, nft_deed_seed,
    nft_token_address, nft_token_id, nft_token_seed, owner_tag, page_address, price_tier_for_len, record_address,
    register_generation, rerelease_premium, sale_split, settings_address, settings_seed, shortlist,
    site_bucket_address, site_bucket_seed, site_dir_header_address, site_dir_header_seed, subname_commitment,
    subname_page_address, subname_record_address, subname_seeds, tiered_id,
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
    let nft_program = AccountId::new([8; 32]);
    let slot = tiered_id(&id, PRICE_TIER_8_PLUS);
    let holder = owner_tag(&id, &alice);
    let promised = commitment(&id, &holder, &[0x5A; 32]);
    let [name_seed, page_seed, deposit_seed] = name_seeds(&id);
    let instruction = |value: Instruction| hex(&to_vec(&Message::Instruction(value)).expect("borsh"));
    let leaves: Vec<_> = (0..5_u8)
        .map(|at| claim_leaf(&AccountId::new([at; 32]), &[at + 100; 32]))
        .collect();
    let commitment_state = |public| hex(&to_vec(&Commitment { version: VERSION, made: NOW, public }).expect("borsh"));
    let membership = || LengthProof::Membership(shortlist::Membership { index: 7, tier: 3, path: vec![[9; 32]] });
    let non_membership = || {
        LengthProof::NonMembership(shortlist::NonMembership {
            left: shortlist::Leaf { id: [1; 32], tier: 3 },
            left_path: vec![[3; 32]],
            right: shortlist::Leaf { id: [2; 32], tier: 1 },
            right_path: vec![[4; 32], [5; 32]],
            left_index: 6,
        })
    };
    let sub: NameId = [0x33; 32];
    let upsert = SiteDirOp::Upsert { name: "alice".into(), title: "Hi".into(), page: 2, now: NOW };
    let prices: [[u128; TERMS]; PRICE_TIERS] = [[1, 2, 3, 4], [5, 6, 7, 8], [9, 10, 11, 12]];

    // v8 derivations, on the 8+ tier slot of `id` (and its two other tiers).
    let slot_holder = owner_tag(&slot, &alice);
    let reg_gen = register_generation(&slot, &slot_holder, NOW, &[0x5A; 32]);
    let claim_gen = claim_generation(&slot, &slot_holder, NOW);
    let token = nft_token_id(&slot, &reg_gen);
    let [sub_seed, subpage_seed] = subname_seeds(&slot, &sub);
    let bucket = bucket_of(&slot);
    let sale_prices: [u128; 7] = [40, 41, 79, 80, 1_000_000, 3_000_017, u128::MAX];
    let day = 86_400_000_u64;
    let elapsed: [u64; 10] = [0, 1, day / 2, day, day + 12_345_678, 2 * day, 10 * day + 7, 20 * day, 21 * day - 1, 21 * day];

    // A full v8 record, and the states the resolver and the market/sites readers decode.
    let listing = Listing { price: 3_000_017, pay_to: id_of(8), expires_at: Some(NOW + 1000), nonce: 4, in_market: true };
    let full_record = NameRecord {
        version: VERSION,
        owner: slot_holder,
        expires: NOW + 365 * day,
        sealed: vec![0xEE, 0xFF],
        claimed: false,
        price_tier: PRICE_TIER_8_PLUS,
        generation: reg_gen,
        subname_sale: Some(SubnameSale { price: 50_000, payout: id_of(8) }),
        nft_token_id: token,
        nft_shielded: false,
        listing: Some(listing),
        listing_nonce: 4,
        site_listed: Some(2),
    };
    let bare_record = NameRecord {
        version: VERSION,
        owner: slot_holder,
        expires: u64::MAX,
        sealed: Vec::new(),
        claimed: true,
        price_tier: PRICE_TIER_5,
        generation: claim_gen,
        subname_sale: None,
        nft_token_id: [0; 32],
        nft_shielded: true,
        listing: None,
        listing_nonce: 0,
        site_listed: None,
    };
    let settings_state = Settings {
        version: VERSION,
        admin: id_of(1),
        treasury: id_of(7),
        prices: DEFAULT_PRICES,
        mode: MODE_FEE,
        deposits: [0, 0],
        claims_root: [0; 32],
        claims_until: 0,
        claims_program: AccountId::new([0x38; 32]),
        premium_start: DEFAULT_PREMIUM_START,
        nft_program,
    };
    let permanent_sub = SubnameRecord { version: VERSION, owner: owner_tag(&sub, &alice), kind: SubnameKind::Permanent, sealed: vec![0xEE] };
    let revocable_sub = SubnameRecord {
        version: VERSION,
        owner: owner_tag(&sub, &alice),
        kind: SubnameKind::Revocable { parent_generation: reg_gen },
        sealed: Vec::new(),
    };
    let market_bucket = MarketBucket {
        version: VERSION,
        entries: vec![
            MarketEntry {
                id: slot,
                price: 3_000_017,
                pay_to: id_of(8),
                expires_at: None,
                name_expires: NOW + 365 * day,
                price_tier: PRICE_TIER_8_PLUS,
                nonce: 4,
                nft_token_id: token,
                display_name: Some("alice-lns".into()),
            },
            MarketEntry {
                id: [0xA0; 32],
                price: 40,
                pay_to: id_of(2),
                expires_at: Some(NOW + 5),
                name_expires: u64::MAX,
                price_tier: PRICE_TIER_5,
                nonce: 1,
                nft_token_id: [0xA1; 32],
                display_name: None,
            },
        ],
    };
    let site_bucket = SiteBucket {
        version: VERSION,
        entries: vec![SiteEntry { id: slot, name: "alice-lns".into(), title: "Alice's site".into(), updated_at: NOW, name_expires: NOW + 365 * day }],
    };
    let mut active_page = [0_u16; SITE_BUCKETS as usize];
    active_page[usize::from(bucket)] = 2;
    active_page[15] = 513;
    let site_header = SiteDirHeader { version: VERSION, active_page, total_listed: 70_000 };
    let state_hex = |value: &dyn Fn() -> Vec<u8>| hex(&value());

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
            "ownerTag": hex(&holder),
        },
        "seeds": {
            "name": hex(name_seed.as_ref()),
            "page": hex(page_seed.as_ref()),
            "deposit": hex(deposit_seed.as_ref()),
            "settings": hex(settings_seed().as_ref()),
            "commitment": hex(commitment_seed(&promised).as_ref()),
        },
        // borsh(Message::Instruction(..)) for every v8 instruction, with the inputs
        // gateway/public/app/lez-tx.test.mjs uses.
        "instructions": {
            "register": instruction(Instruction::Register {
                id, sealed: vec![0xEE, 0xFF], term: TERM_1Y, fee: 100_000, treasury: Some(AccountId::new([2; 32])),
                now: NOW, secret: [0x5A; 32], deposit: false, length: membership(), price_tier: PRICE_TIER_8_PLUS,
            }),
            "registerDeposit": instruction(Instruction::Register {
                id, sealed: vec![0xEE], term: TERM_30D, fee: DEPOSIT, treasury: None, now: NOW, secret: [0x5A; 32],
                deposit: true, length: non_membership(), price_tier: PRICE_TIER_8_PLUS,
            }),
            "registerPermanentTier5": instruction(Instruction::Register {
                id, sealed: Vec::new(), term: TERM_PERMANENT, fee: 400_000_000, treasury: Some(AccountId::new([2; 32])),
                now: NOW, secret: [0x5A; 32], deposit: false, length: non_membership(), price_tier: PRICE_TIER_5,
            }),
            "renew": instruction(Instruction::Renew { id, term: TERM_5Y, fee: 5, treasury: Some(AccountId::new([2; 32])), now: NOW }),
            "renewPermanent": instruction(Instruction::Renew { id, term: TERM_PERMANENT, fee: 20_000_000, treasury: None, now: NOW }),
            "setRecord": instruction(Instruction::SetRecord { id, sealed: vec![0xEE], site_dir: SiteDirOp::Keep }),
            "setRecordRemove": instruction(Instruction::SetRecord { id, sealed: vec![0xEE], site_dir: SiteDirOp::Remove }),
            "setRecordUpsert": instruction(Instruction::SetRecord { id, sealed: vec![0xEE], site_dir: upsert.clone() }),
            "setPage": instruction(Instruction::SetPage { id, sealed: Vec::new(), site_dir: SiteDirOp::Keep }),
            "setPageUpsert": instruction(Instruction::SetPage { id, sealed: b"<p>".to_vec(), site_dir: upsert.clone() }),
            "transfer": instruction(Instruction::Transfer { id, new_owner: [2; 32], now: NOW }),
            "release": instruction(Instruction::Release { id, refund: DEPOSIT }),
            "setPrimary": instruction(Instruction::SetPrimary { name: "jef".into() }),
            "setPrimarySubname": instruction(Instruction::SetPrimary { name: "blog.alice".into() }),
            "configure": instruction(Instruction::Configure {
                admin: AccountId::new([1; 32]), treasury: AccountId::new([2; 32]), prices, mode: MODE_FEE, deposits: [0, 0],
                claims_root: [0; 32], claims_until: 0, claims_program: AccountId::new([7; 32]),
                premium_start: DEFAULT_PREMIUM_START, nft_program: AccountId::new([8; 32]),
            }),
            "configureDeposit": instruction(Instruction::Configure {
                admin: AccountId::new([1; 32]), treasury: AccountId::new([2; 32]), prices: DEFAULT_PRICES, mode: MODE_DEPOSIT,
                deposits: [DEPOSIT, 3 * DEPOSIT], claims_root: [0xC1; 32], claims_until: NOW, claims_program: AccountId::new([0x38; 32]),
                premium_start: 0, nft_program: AccountId::new([8; 32]),
            }),
            "commit": instruction(Instruction::Commit { commitment: [0xC0; 32], now: NOW }),
            "claim": instruction(Instruction::Claim { id, sealed: vec![0xEE], now: NOW, proof: vec![[1; 32], [2; 32]], price_tier: PRICE_TIER_6_7 }),
            "commitPublic": instruction(Instruction::CommitPublic { commitment: [0xC0; 32], now: NOW }),
            "setPrice": instruction(Instruction::SetPrice { price_tier: PRICE_TIER_5, term: TERM_PERMANENT, price: 400_000_000 }),
            "setSubnameSaleOn": instruction(Instruction::SetSubnameSale { id, sale: Some(SubnameSale { price: 100_000, payout: AccountId::new([2; 32]) }) }),
            "setSubnameSaleOff": instruction(Instruction::SetSubnameSale { id, sale: None }),
            "createSubname": instruction(Instruction::CreateSubname { parent_id: id, sub_id: sub, owner: [2; 32], revocable: true, sealed: vec![0xEE], now: NOW }),
            "revokeSubname": instruction(Instruction::RevokeSubname { parent_id: id, sub_id: sub, now: NOW }),
            "buySubname": instruction(Instruction::BuySubname {
                parent_id: id, sub_id: sub, sealed: vec![0xEE], price: 100_000, payout: Some(AccountId::new([2; 32])), now: NOW, secret: [0x5A; 32],
            }),
            "buySubnameFree": instruction(Instruction::BuySubname { parent_id: id, sub_id: sub, sealed: Vec::new(), price: 0, payout: None, now: NOW, secret: [0x5A; 32] }),
            "setSubnameRecordNone": instruction(Instruction::SetSubnameRecord { parent_id: id, sub_id: sub, sealed: vec![0xEE], now: None }),
            "setSubnameRecordSome": instruction(Instruction::SetSubnameRecord { parent_id: id, sub_id: sub, sealed: vec![0xEE], now: Some(NOW) }),
            "setSubnamePage": instruction(Instruction::SetSubnamePage { parent_id: id, sub_id: sub, sealed: Vec::new(), now: None }),
            "transferSubname": instruction(Instruction::TransferSubname { parent_id: id, sub_id: sub, new_owner: [2; 32], now: Some(NOW) }),
            "releaseSubname": instruction(Instruction::ReleaseSubname { parent_id: id, sub_id: sub, now: None }),
            "setPremium": instruction(Instruction::SetPremium { premium_start: DEFAULT_PREMIUM_START }),
            "listPublic": instruction(Instruction::List {
                id, price: 100_000, pay_to: AccountId::new([2; 32]), expires_at: Some(NOW + 1000), now: NOW, display_name: Some("alice".into()),
            }),
            "listOtc": instruction(Instruction::List { id, price: 100_000, pay_to: AccountId::new([2; 32]), expires_at: None, now: NOW, display_name: None }),
            "cancel": instruction(Instruction::Cancel { id }),
            "buy": instruction(Instruction::Buy {
                id, price: 100_000, pay_to: AccountId::new([2; 32]), treasury: AccountId::new([3; 32]), expected_nonce: 7, now: NOW,
            }),
            "shieldDeed": instruction(Instruction::ShieldDeed { id, deed_account: AccountId::new([4; 32]), now: NOW }),
            "unshieldDeed": instruction(Instruction::UnshieldDeed { id }),
            "refreshShieldedDeed": instruction(Instruction::RefreshShieldedDeed { id }),
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
        // The root of that list, which `run` configures (alice held [0xCC; 32] on [0x38; 32]).
        "scenarioClaimRoot": hex(&claim_root(&[
            claim_leaf(&record_address(&AccountId::new([0x38; 32]), &[0xCC; 32]), &owner_tag(&[0xCC; 32], &id_of(2))),
            claim_leaf(&AccountId::new([0x11; 32]), &[0x22; 32]),
        ])),
        "lengthProofs": {
            "aa": proof_json(&[0xAA; 32]),
            "bb": proof_json(&[0xBB; 32]),
            "cc": proof_json(&[0xCC; 32]),
            // deriveName("alice-lns"), for the run through the app's own register.js.
            "named": proof_json(&unhex(NAMED_ID).try_into().expect("32 bytes")),
        },
        // The v8 derivations, on `id` (0..31), registry [7; 32], NFT program [8; 32],
        // account `alice`, secret [0x5A; 32], now NOW.
        "v8": {
            "id": hex(&id),
            "priceTierForLen": (0..=12).map(price_tier_for_len).collect::<Vec<_>>(),
            "tieredIds": ([PRICE_TIER_5, PRICE_TIER_6_7, PRICE_TIER_8_PLUS].map(|tier| hex(&tiered_id(&id, tier)))),
            "slot": hex(&slot),
            "slotOwnerTag": hex(&slot_holder),
            "slotCommitment": hex(&commitment(&slot, &slot_holder, &[0x5A; 32])),
            "slotRecord": record_address(&registry, &slot).to_string(),
            "slotPage": page_address(&registry, &slot).to_string(),
            "slotDeposit": deposit_address(&registry, &slot).to_string(),
            "registerGeneration": hex(&reg_gen),
            "claimGeneration": hex(&claim_gen),
            "subname": {
                "sub": hex(&sub),
                "recordSeed": hex(sub_seed.as_ref()),
                "pageSeed": hex(subpage_seed.as_ref()),
                "record": subname_record_address(&registry, &slot, &sub).to_string(),
                "page": subname_page_address(&registry, &slot, &sub).to_string(),
                "ownerTag": hex(&owner_tag(&sub, &alice)),
                "commitment": hex(&subname_commitment(&slot, &sub, &owner_tag(&sub, &alice), &[0x5A; 32])),
            },
            "market": {
                "bucket": bucket,
                "seeds": (0..MARKET_BUCKETS).map(|b| hex(market_bucket_seed(b).as_ref())).collect::<Vec<_>>(),
                "addresses": (0..MARKET_BUCKETS).map(|b| market_bucket_address(&registry, b).to_string()).collect::<Vec<_>>(),
            },
            "sites": {
                "headerSeed": hex(site_dir_header_seed().as_ref()),
                "header": site_dir_header_address(&registry).to_string(),
                "bucketSeeds": ([(0_u8, 0_u16), (bucket, 0), (bucket, 2), (15, 513)].map(|(b, page)| json!({
                    "bucket": b, "page": page, "seed": hex(site_bucket_seed(b, page).as_ref()),
                    "address": site_bucket_address(&registry, b, page).to_string(),
                }))),
            },
            "nft": {
                "program": nft_program.to_string(),
                "tokenId": hex(&token),
                "claimTokenId": hex(&nft_token_id(&slot, &claim_gen)),
                "tokenSeed": hex(nft_token_seed(&token).as_ref()),
                "tokenAddress": nft_token_address(&nft_program, &token).to_string(),
                "deedSeed": hex(nft_deed_seed(&token).as_ref()),
                "configSeed": hex(nft::config_seed().as_ref()),
                "configAddress": nft::config_address(&nft_program).to_string(),
                "lnsCollectionId": hex(&nft::lns_collection_id(&registry)),
                "deedContent": String::from_utf8(DEFAULT_DEED_CONTENT.to_vec()).expect("utf-8"),
            },
            "saleSplit": (sale_prices.map(|price| { let (seller, fee) = sale_split(price); [price.to_string(), seller.to_string(), fee.to_string()] })),
            "premium": ([DEFAULT_PREMIUM_START, 64_000_000, 1, u128::MAX].iter().flat_map(|start| {
                elapsed.iter().map(move |at| [start.to_string(), at.to_string(), rerelease_premium(*start, *at).to_string()])
            }).collect::<Vec<_>>()),
            "states": {
                "fullRecord": state_hex(&|| to_vec(&full_record).expect("borsh")),
                "bareRecord": state_hex(&|| to_vec(&bare_record).expect("borsh")),
                "settings": state_hex(&|| to_vec(&settings_state).expect("borsh")),
                "permanentSubname": state_hex(&|| to_vec(&permanent_sub).expect("borsh")),
                "revocableSubname": state_hex(&|| to_vec(&revocable_sub).expect("borsh")),
                "marketBucket": state_hex(&|| to_vec(&market_bucket).expect("borsh")),
                "siteBucket": state_hex(&|| to_vec(&site_bucket).expect("borsh")),
                "siteHeader": state_hex(&|| to_vec(&site_header).expect("borsh")),
            },
            "accounts": { "payout": id_of(8).to_string(), "treasury": id_of(7).to_string(), "admin": id_of(1).to_string() },
        },
    })
}

/// The NFT program's address in `run` (as in tools/engine-test).
const NFT_PROGRAM: [u8; 32] = [0x4E; 32];
const FUNDS: u128 = 1_000_000_000;
/// The placeholder price table `run` configures.
const RUN_PRICES: [[u128; TERMS]; PRICE_TIERS] = DEFAULT_PRICES;
const RUN_PREMIUM_START: u128 = 64_000_000;

fn sign_and_apply(state: &mut V03State, signer_byte: u8, nonce: u128, root: Actor, bytes: Vec<u8>, actors: Vec<Actor>, evidence: Vec<PublicAccountEvidence>, block: u64, now: u64, what: &str) {
    let signer_id = id_of(signer_byte);
    let mut public: Vec<(Actor, Authority)> = actors.into_iter().map(|actor| (actor, Authority::None)).collect();
    public.push((root, Authority::Signer));
    let tx = TxMessage::new(root, bytes, public, BTreeMap::from([(signer_id, Nonce(nonce))]), None, evidence);
    let witness = WitnessSet::sign(&tx, &[signer(signer_byte, nonce)]);
    state
        .transition_from_public_transaction(&PublicTransaction::new(tx, witness), block, now)
        .unwrap_or_else(|error| panic!("{what}: {error:?}"));
}

fn run_scenario(path: &str) -> Value {
    let scenario: Value = serde_json::from_str(&std::fs::read_to_string(path).expect("a scenario")).expect("JSON");
    let program = id_of(9);
    let nft_program = AccountId::new(NFT_PROGRAM);
    let alice = id_of(2);
    let held: NameId = [0xCC; 32];
    let old_program = AccountId::new([0x38; 32]);
    let leaves = vec![
        claim_leaf(&record_address(&old_program, &held), &owner_tag(&held, &alice)),
        claim_leaf(&AccountId::new([0x11; 32]), &[0x22; 32]),
    ];
    let mut state = GenesisBuilder::new()
        .with_public_accounts(
            [(alice, FUNDS), (id_of(3), FUNDS), (id_of(4), FUNDS), (id_of(6), FUNDS), (id_of(7), 1), (id_of(8), 1)]
                .map(|(id, balance)| (id, Account::funded(balance))),
        )
        .with_programs_at([
            (program, Program::new(names_methods::LOGOS_NAMES_ELF.into()).expect("the guest loads"), false),
            (nft_program, Program::new(lez_nft_methods::LEZ_NFT_GUEST_ELF.into()).expect("the NFT guest loads"), false),
        ])
        .build();

    // Deploy order (ENCODING-CHANGES.md): the NFT admin reserves the LNS collection for the
    // registry, then the registry's own account configures it with the NFT program. Both are
    // the Rust side's; everything after them is JavaScript's.
    let nft_admin = Actor::new(id_of(6), nft_program);
    let nft_configure = nft::Message::Instruction(nft::Instruction::Configure {
        admin: id_of(6),
        authorities: vec![nft::AuthorizedProgram { program, collection_id: Some(nft::lns_collection_id(&program)) }],
    });
    sign_and_apply(
        &mut state,
        6,
        0,
        nft_admin,
        to_vec(&nft_configure).expect("borsh"),
        vec![Actor::new(nft::config_address(&nft_program), nft_program)],
        vec![PublicAccountEvidence::Pda { program: nft_program, seed: nft::config_seed() }],
        1,
        T0,
        "NFT configure",
    );
    let configure = Message::Instruction(Instruction::Configure {
        admin: id_of(9),
        treasury: id_of(7),
        prices: RUN_PRICES,
        mode: MODE_FEE,
        deposits: [0, 0],
        claims_root: claim_root(&leaves),
        claims_until: T0 + 10 * MIN_COMMIT_AGE_MS,
        claims_program: old_program,
        premium_start: RUN_PREMIUM_START,
        nft_program,
    });
    sign_and_apply(
        &mut state,
        9,
        0,
        Actor::new(program, program),
        to_vec(&configure).expect("borsh"),
        vec![Actor::new(settings_address(&program), program)],
        vec![PublicAccountEvidence::Pda { program, seed: settings_seed() }],
        2,
        T0,
        "registry configure",
    );

    let mut block = 2;
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
        "nftProgram": scenario["nftProgram"],
        "account": scenario["account"],
        "salePrice": scenario["salePrice"],
        "yearPrice": scenario["yearPrice"],
        "premium": scenario["premium"],
        "thirtyDays": scenario["thirtyDays"],
        "steps": steps,
        "views": views,
        "balances": balances,
        "signingKeys": signing_keys,
        "expectedKeys": scenario["expectedKeys"],
        "claimsUntil": stored.claims_until,
    })
}

/// Verifies, with names_core's own verifier and root, the proofs shard-proofs.mjs made the
/// way the web app does, and that every name the app refused as locked is locked on the list.
fn check_proofs(path: &str) -> Value {
    let input: Value = serde_json::from_str(&std::fs::read_to_string(path).expect("proofs")).expect("JSON");
    let list = short_list();
    assert_eq!(list.root(), names_core::SHORTLIST_ROOT, "the repo's list builds to the compiled root");
    let mut out = serde_json::Map::new();
    for (name, entry) in input["proofs"].as_object().expect("proofs") {
        let id: NameId = unhex(entry["id"].as_str().expect("id")).try_into().expect("32 bytes");
        let proof = LengthProof::try_from_slice(&unhex(entry["proof"].as_str().expect("proof"))).expect("a LengthProof");
        let tier = shortlist::verify(&names_core::SHORTLIST_ROOT, &id, &proof).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(tier, names_core::TIER_NORMAL, "{name} is open");
        assert_eq!(list.prove(&id).expect("a proof"), proof, "{name}: the shard proof is the full list's");
        out.insert(name.clone(), json!("open: the shard proof verifies against SHORTLIST_ROOT"));
    }
    for (name, entry) in input["locked"].as_object().expect("locked") {
        let id: NameId = unhex(entry["id"].as_str().expect("id")).try_into().expect("32 bytes");
        let proof = list.prove(&id).expect("a proof");
        assert!(matches!(proof, LengthProof::Membership(_)), "{name} is on the list");
        let tier = shortlist::verify(&names_core::SHORTLIST_ROOT, &id, &proof).expect("verifies");
        assert_eq!(tier, names_core::TIER_LOCKED, "{name} is locked");
        out.insert(name.clone(), json!("locked: on the list with TIER_LOCKED, refused by the app"));
    }
    Value::Object(out)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("gen") => println!("{}", serde_json::to_string_pretty(&generate()).expect("JSON")),
        Some("run") => {
            let report = run_scenario(&args[2]);
            std::fs::write(&args[3], serde_json::to_string_pretty(&report).expect("JSON")).expect("a report");
        }
        Some("check-proofs") => println!("{}", serde_json::to_string_pretty(&check_proofs(&args[2])).expect("JSON")),
        _ => panic!("usage: lns_js_vectors gen | run <scenario.json> <report.json> | check-proofs <proofs.json>"),
    }
}
