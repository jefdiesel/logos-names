//! End-to-end run of the LNS registry together with lez_nft v3 on the LEZ 0.4 engine:
//! both real guest ELFs, real signatures and the real state machine, with no chain,
//! network or proving.
//!
//! Flow: register -> NFT linked -> list -> buy with the 2.5% split -> transfer through
//! the NFT moves the name -> sites directory on publish and off on `discover=off` ->
//! subname create/revoke -> lapse, grace, re-release premium -> re-register (old NFT
//! burned, new one minted).

mod crosscheck;

use std::collections::{BTreeMap, BTreeSet};

use lee::{
    Authority, GenesisBuilder, PublicAccountEvidence, PublicTransaction, V03State,
    program::Program,
    public_transaction::{Message as TxMessage, WitnessSet},
    signature::{INITIAL_KEY_TYPE, PublicAccountSecret, Signer, public_account_id},
};
use lee_core::{
    account::{Account, AccountId, Actor, Nonce},
    program::PdaSeed,
};
use lez_nft_core as nft;
use names_core::{
    GRACE_MS, Instruction, LengthProof, MIN_COMMIT_AGE_MS, MODE_DEPOSIT, MODE_FEE,
    PUBLIC_MIN_COMMIT_AGE_MS, PUBLIC_TIME_WINDOW_MS, claim_generation, claim_leaf, claim_proof,
    claim_root, deposit_address, MarketBucket, Message,
    NameId, NameRecord, PRICE_TIER_8_PLUS, PRICE_TIERS, SiteBucket,
    SiteDirHeader, SiteDirOp, SubnameRecord, TERM_1Y, TERM_LENGTHS_MS, TERMS, bucket_of,
    commitment, commitment_address, commitment_seed, market_bucket_address, market_bucket_seed,
    name_seeds, nft_token_address, nft_token_id, nft_token_seed, owner_tag, page_address,
    record_address, register_generation, rerelease_premium, sale_split, settings_address,
    settings_seed, site_bucket_address, site_bucket_seed, site_dir_header_address,
    site_dir_header_seed, subname_page_address, subname_record_address, subname_seeds, tiered_id,
};

const T0: u64 = 1_800_000_000_000;
const DAY: u64 = 24 * 60 * 60 * 1000;
const FUNDS: u128 = 1_000_000_000;
/// Placeholder LEZ prices, `[tier][30d, 1y, 5y, permanent]`.
const PRICES: [[u128; TERMS]; PRICE_TIERS] = [
    [2_000_000, 20_000_000, 80_000_000, 400_000_000],
    [500_000, 5_000_000, 20_000_000, 100_000_000],
    [100_000, 1_000_000, 4_000_000, 20_000_000],
];
const PREMIUM_START: u128 = 64_000_000;
const DEPOSIT: u128 = 1_000_000;
/// Not a multiple of 40, so the 2.5% fee rounds up.
const SALE_PRICE: u128 = 3_000_017;

struct Who {
    secret: PublicAccountSecret,
    id: AccountId,
    nonce: u128,
}

fn who(byte: u8) -> Who {
    let secret = PublicAccountSecret::new([byte; 32]);
    let id = public_account_id(&secret);
    Who { secret, id, nonce: 0 }
}

struct Run {
    state: V03State,
    registry: AccountId,
    nft: AccountId,
    block: u64,
    /// Every program account a transaction here could create, as `(program, seed)`.
    seeds: Vec<(AccountId, PdaSeed)>,
}

impl Run {
    fn reg(&self, account: AccountId) -> Actor {
        Actor::new(account, self.registry)
    }

    fn nft_actor(&self, account: AccountId) -> Actor {
        Actor::new(account, self.nft)
    }

    fn try_send(
        &mut self,
        from: &mut Who,
        program: AccountId,
        bytes: Vec<u8>,
        actors: Vec<Actor>,
        now: u64,
    ) -> Result<(), String> {
        let root = Actor::new(from.id, program);
        self.try_send_as(from, root, bytes, actors, now)
    }

    /// Sends `bytes` to `root`, signed by `from`, declaring `actors` as the public actors.
    fn try_send_as(
        &mut self,
        from: &mut Who,
        root: Actor,
        bytes: Vec<u8>,
        actors: Vec<Actor>,
        now: u64,
    ) -> Result<(), String> {
        let mut public = actors;
        public.push(root);
        public.push(Actor::new(from.id, root.program_account_id));
        let mut seen = BTreeSet::new();
        public.retain(|actor| seen.insert(*actor));
        let nonces = BTreeMap::from([(from.id, Nonce(from.nonce))]);
        let wanted: BTreeSet<AccountId> = public.iter().map(|actor| actor.account_id).collect();
        let evidence = self
            .seeds
            .iter()
            .map(|(program, seed)| PublicAccountEvidence::Pda { program: *program, seed: *seed })
            .filter(|evidence| evidence.account_id().is_some_and(|id| wanted.contains(&id)))
            .collect();
        // The signer authorizes every actor on its own account (its program actors and its
        // native balance); the rest are plain.
        let signer = from.id;
        let public = public.into_iter().map(|actor| {
            let authority = if actor.account_id == signer { Authority::Signer } else { Authority::None };
            (actor, authority)
        });
        let tx = TxMessage::new(root, bytes, public, nonces, None, evidence);
        let key = Signer::new(&from.secret, Nonce(from.nonce), INITIAL_KEY_TYPE, INITIAL_KEY_TYPE);
        let witness = WitnessSet::sign(&tx, &[key]);
        let tx = PublicTransaction::new(tx, witness);
        self.block += 1;
        self.state
            .transition_from_public_transaction(&tx, self.block, now)
            .map(|_| from.nonce += 1)
            .map_err(|error| format!("{error:?}"))
    }

    fn send(&mut self, what: &str, from: &mut Who, instruction: Instruction, actors: Vec<Actor>, now: u64) {
        let bytes = borsh::to_vec(&Message::Instruction(instruction)).expect("a message");
        self.try_send(from, self.registry, bytes, actors, now)
            .unwrap_or_else(|error| panic!("{what}: {error}"));
        println!("ok   {what}");
    }

    fn send_nft(&mut self, what: &str, from: &mut Who, message: &nft::Message, actors: Vec<Actor>, now: u64) {
        let bytes = borsh::to_vec(message).expect("a message");
        self.try_send(from, self.nft, bytes, actors, now)
            .unwrap_or_else(|error| panic!("{what}: {error}"));
        println!("ok   {what}");
    }

    fn refuse(&mut self, what: &str, expected: &str, from: &mut Who, message: Message, actors: Vec<Actor>, now: u64) {
        let bytes = borsh::to_vec(&message).expect("a message");
        let error = self.try_send(from, self.registry, bytes, actors, now).expect_err(what);
        assert!(error.contains(expected), "{what}: refused for the wrong reason: {error}");
        println!("ok   {what} (refused: {expected})");
    }

    fn refuse_bytes(&mut self, what: &str, expected: &str, from: &mut Who, root: Actor, bytes: Vec<u8>, actors: Vec<Actor>, now: u64) {
        let error = self.try_send_as(from, root, bytes, actors, now).expect_err(what);
        assert!(error.contains(expected), "{what}: refused for the wrong reason: {error}");
        println!("ok   {what} (refused: {expected})");
    }

    fn data(&self, account: AccountId, program: AccountId) -> Vec<u8> {
        self.state.get_account_by_id(account).data.actor_state(program).to_vec()
    }

    fn decode<T: borsh::BorshDeserialize>(&self, account: AccountId, program: AccountId) -> Option<T> {
        let data = self.data(account, program);
        (!data.is_empty()).then(|| borsh::from_slice(&data).expect("valid state"))
    }

    fn record(&self, slot: &NameId) -> Option<NameRecord> {
        self.decode(record_address(&self.registry, slot), self.registry)
    }

    fn token(&self, token_id: &[u8; 32]) -> Option<nft::NftRecord> {
        self.decode(nft_token_address(&self.nft, token_id), self.nft)
    }

    fn balance(&self, account: AccountId) -> u128 {
        self.state.get_account_by_id(account).data.native_balance().expect("a balance")
    }

    fn name_actors(&self, slot: &NameId) -> Vec<Actor> {
        vec![self.reg(record_address(&self.registry, slot)), self.reg(page_address(&self.registry, slot))]
    }

    fn token_actor(&self, token_id: &[u8; 32]) -> Actor {
        self.nft_actor(nft_token_address(&self.nft, token_id))
    }

    /// The registry and NFT state must agree on owner, expiry and link.
    fn assert_lockstep(&self, slot: &NameId) {
        let record = self.record(slot).expect("a record");
        let token = self.token(&record.nft_token_id).expect("its NFT");
        assert_eq!(token.owner, record.owner, "NFT owner follows the name");
        assert_eq!(token.expires, record.expires, "NFT expiry follows the name");
        let linked = token.linked.expect("a linked NFT");
        assert_eq!(linked.program, self.registry);
        assert_eq!(linked.authority_actor, settings_address(&self.registry));
        assert_eq!(linked.target, *slot);
        assert_eq!(linked.record_actor, record_address(&self.registry, slot));
        assert_eq!(token.collection, Some((nft::lns_collection_id(&self.registry), 0)));
    }
}

/// The short-name list the registry guest's root was built from.
fn short_list() -> names_core::shortlist::ShortList {
    let leaves = include_bytes!("../../../shortlist/shortlist.bin")
        .chunks_exact(33)
        .map(|chunk| names_core::shortlist::Leaf { id: chunk[..32].try_into().expect("32 bytes"), tier: chunk[32] })
        .collect();
    names_core::shortlist::ShortList::from_sorted_leaves(leaves).expect("a sorted list")
}

struct Registration {
    base: NameId,
    slot: NameId,
    secret: [u8; 32],
    token_id: [u8; 32],
    length: LengthProof,
}

impl Run {
    /// Commits privately at `at`, so a reveal is valid from `at + MIN_COMMIT_AGE_MS`.
    fn commit(&mut self, who: &mut Who, base: NameId, secret: [u8; 32], at: u64, reveal_at: u64) -> Registration {
        let slot = tiered_id(&base, PRICE_TIER_8_PLUS);
        let owner = owner_tag(&slot, &who.id);
        let promised = commitment(&slot, &owner, &secret);
        let token_id = nft_token_id(&slot, &register_generation(&slot, &owner, reveal_at, &secret));
        self.seeds.extend(name_seeds(&slot).map(|seed| (self.registry, seed)));
        self.seeds.push((self.registry, commitment_seed(&promised)));
        self.seeds.push((self.nft, nft_token_seed(&token_id)));
        let actors = vec![self.reg(commitment_address(&self.registry, &promised))];
        self.send("commit to a name", who, Instruction::Commit { commitment: promised, now: at }, actors, at);
        let length = short_list().prove(&base).expect("a proof");
        Registration { base, slot, secret, token_id, length }
    }

    fn register_instruction(&self, reg: &Registration, fee: u128, treasury: AccountId, now: u64) -> Instruction {
        Instruction::Register {
            id: reg.base,
            sealed: b"profile".to_vec(),
            term: TERM_1Y,
            fee,
            treasury: Some(treasury),
            now,
            secret: reg.secret,
            deposit: false,
            length: reg.length.clone(),
            price_tier: PRICE_TIER_8_PLUS,
        }
    }

    /// Everything a paid registration touches: the name, settings, the commitment, the
    /// payer's and treasury's balances, the new NFT and the NFT program's config (and the
    /// lapsed predecessor's NFT, if any).
    fn registering(&self, reg: &Registration, payer: AccountId, treasury: AccountId, old_token: Option<[u8; 32]>) -> Vec<Actor> {
        let promised = commitment(&reg.slot, &owner_tag(&reg.slot, &payer), &reg.secret);
        let mut actors = self.name_actors(&reg.slot);
        actors.extend([
            self.reg(settings_address(&self.registry)),
            self.reg(commitment_address(&self.registry, &promised)),
            Actor::native_balance(payer),
            Actor::native_balance(treasury),
            self.token_actor(&reg.token_id),
            self.nft_actor(nft::config_address(&self.nft)),
        ]);
        if let Some(old) = old_token {
            actors.push(self.token_actor(&old));
        }
        actors
    }
}

fn main() {
    crosscheck::run();

    let mut deployer = who(9);
    let mut admin = who(1);
    let mut alice = who(2);
    let mut bob = who(3);
    let mut carol = who(4);
    let mut dave = who(5);
    let mut nft_admin = who(6);
    let (alice_id, bob_id, carol_id, dave_id, admin_id) = (alice.id, bob.id, carol.id, dave.id, admin.id);
    let treasury = who(7).id;
    let alice_payout = who(8).id;
    let registry = deployer.id;
    // The NFT program's own address on this test chain.
    let nft_program = AccountId::new([0x4E; 32]);

    let state = GenesisBuilder::new()
        .with_public_accounts(
            [
                (alice_id, FUNDS),
                (bob_id, FUNDS),
                (carol_id, FUNDS),
                (dave_id, FUNDS),
                (admin_id, FUNDS),
                (nft_admin.id, FUNDS),
                (treasury, 1),
                (alice_payout, 1),
            ]
            .map(|(id, balance)| (id, Account::funded(balance))),
        )
        .with_programs_at([
            (registry, Program::new(names_methods::LOGOS_NAMES_ELF.into()).expect("the registry guest loads"), false),
            (nft_program, Program::new(lez_nft_methods::LEZ_NFT_GUEST_ELF.into()).expect("the NFT guest loads"), false),
        ])
        .build();
    let mut run = Run {
        state,
        registry,
        nft: nft_program,
        block: 0,
        seeds: vec![
            (registry, settings_seed()),
            (nft_program, nft::config_seed()),
            (registry, site_dir_header_seed()),
        ],
    };
    let settings = run.reg(settings_address(&registry));
    let config = run.nft_actor(nft::config_address(&nft_program));

    // ── 1. Deploy order: NFT config (reserving the LNS collection), then the registry.
    let nft_admin_id = nft_admin.id;
    run.send_nft(
        "the NFT admin reserves the LNS collection for the registry",
        &mut nft_admin,
        &nft::Message::Instruction(nft::Instruction::Configure {
            admin: nft_admin_id,
            authorities: vec![nft::AuthorizedProgram {
                program: registry,
                collection_id: Some(nft::lns_collection_id(&registry)),
            }],
        }),
        vec![config],
        T0,
    );

    let jef: NameId = [0xAA; 32];
    let reg = run.commit(&mut alice, jef, [5; 32], T0, T0 + MIN_COMMIT_AGE_MS);
    let at = T0 + MIN_COMMIT_AGE_MS;
    let year_price = PRICES[usize::from(PRICE_TIER_8_PLUS)][usize::from(TERM_1Y)];
    let actors = run.registering(&reg, alice_id, treasury, None);
    let early = run.register_instruction(&reg, year_price, treasury, at);
    run.refuse(
        "nobody registers before the registry is configured",
        "the registry has no settings",
        &mut alice,
        Message::Instruction(early),
        actors,
        at,
    );

    // Claims carried over from the registry on an earlier chain.
    let old_program = AccountId::new([0x38; 32]);
    let held: NameId = [0xCC; 32];
    let leaves = vec![claim_leaf(&record_address(&old_program, &held), &owner_tag(&held, &carol_id))];
    let configure = |mode, deposits| Instruction::Configure {
        admin: admin_id,
        treasury,
        prices: PRICES,
        mode,
        deposits,
        claims_root: claim_root(&leaves),
        claims_until: T0 + 3650 * DAY,
        claims_program: old_program,
        premium_start: PREMIUM_START,
        nft_program,
    };
    run.refuse(
        "a stranger cannot configure a fresh registry",
        "the first settings must be signed",
        &mut bob,
        Message::Instruction(configure(MODE_FEE, [0, 0])),
        vec![settings],
        at,
    );
    run.send("the deployer configures the registry with the NFT program", &mut deployer, configure(MODE_FEE, [0, 0]), vec![settings], at);

    // ── 2. Register: the fee goes to the treasury and a linked NFT is minted.
    let before = run.balance(treasury);
    let actors = run.registering(&reg, alice_id, treasury, None);
    run.send(
        "alice registers a name for a year",
        &mut alice,
        run.register_instruction(&reg, year_price, treasury, at),
        actors,
        at,
    );
    let slot = reg.slot;
    let record = run.record(&slot).expect("registered");
    assert_eq!(record.owner, owner_tag(&slot, &alice_id));
    assert_eq!(record.nft_token_id, reg.token_id, "the client can predict the token id");
    assert_eq!(record.expires, at + TERM_LENGTHS_MS[usize::from(TERM_1Y)]);
    assert_eq!(run.balance(treasury), before + year_price);
    run.assert_lockstep(&slot);
    println!("ok   the NFT is minted, linked to the name, in the LNS collection");
    let t1 = reg.token_id;
    let token = run.token_actor(&t1);

    // Refusals on the NFT side.
    let forged = nft::Message::AuthoritySync(nft::AuthorityOp::Transfer { token_id: t1, new_owner: owner_tag(&slot, &bob_id) });
    run.refuse_bytes(
        "a stranger cannot send AuthoritySync to the token",
        "sender is not the token's linked authority",
        &mut bob,
        token,
        borsh::to_vec(&forged).expect("bytes"),
        vec![],
        at,
    );
    let burn = nft::Message::Instruction(nft::Instruction::Burn { token_id: t1 });
    let error = run.try_send(&mut alice, nft_program, borsh::to_vec(&burn).expect("bytes"), vec![token], at).expect_err("burn");
    assert!(error.contains("linked token can only be burned by its linked authority"), "{error}");
    println!("ok   the holder cannot burn a linked NFT directly (refused)");

    // ── 3. List, then buy: 97.5% to the seller's payout account, 2.5% (rounded up) to the treasury.
    let t = at + 60_000;
    run.seeds.push((registry, market_bucket_seed(bucket_of(&slot))));
    let market = run.reg(market_bucket_address(&registry, bucket_of(&slot)));
    run.send(
        "alice lists the name in the market",
        &mut alice,
        Instruction::List {
            id: slot,
            price: SALE_PRICE,
            pay_to: alice_payout,
            expires_at: None,
            now: t,
            display_name: Some("jefdiesel".into()),
        },
        vec![run.reg(record_address(&registry, &slot)), market],
        t,
    );
    let bucket: MarketBucket = run.decode(market.account_id, registry).expect("a bucket");
    assert!(bucket.entries.iter().any(|entry| entry.id == slot && entry.price == SALE_PRICE));
    let nonce = run.record(&slot).expect("held").listing.expect("listed").nonce;
    println!("ok   the listing is in the market bucket");

    let mut buying = run.name_actors(&slot);
    buying.extend([
        settings,
        market,
        token,
        Actor::native_balance(bob_id),
        Actor::native_balance(alice_payout),
        Actor::native_balance(treasury),
    ]);
    let buy = |nonce| Instruction::Buy { id: slot, price: SALE_PRICE, pay_to: alice_payout, treasury, expected_nonce: nonce, now: t };
    run.refuse("a buy against a stale listing nonce is refused", "listing has changed", &mut bob, Message::Instruction(buy(nonce + 1)), buying.clone(), t);
    let (seller, fee) = sale_split(SALE_PRICE);
    assert_eq!(seller + fee, SALE_PRICE);
    assert_eq!(fee, SALE_PRICE.div_ceil(40));
    let (b0, p0, tr0) = (run.balance(bob_id), run.balance(alice_payout), run.balance(treasury));
    run.send("bob buys the name", &mut bob, buy(nonce), buying, t);
    assert_eq!(run.balance(bob_id), b0 - SALE_PRICE);
    assert_eq!(run.balance(alice_payout), p0 + seller);
    assert_eq!(run.balance(treasury), tr0 + fee);
    let record = run.record(&slot).expect("held");
    assert_eq!(record.owner, owner_tag(&slot, &bob_id));
    assert!(record.listing.is_none());
    let bucket: MarketBucket = run.decode(market.account_id, registry).unwrap_or(MarketBucket { version: 8, entries: vec![] });
    assert!(bucket.entries.iter().all(|entry| entry.id != slot), "the sale left the market");
    run.assert_lockstep(&slot);
    println!("ok   the sale paid {seller} to the seller and {fee} (2.5%, rounded up) to the treasury, and the NFT moved");

    // ── 4. A plain transfer through the NFT program moves the name (no fee).
    let t = t + 60_000;
    let (b0, tr0) = (run.balance(bob_id), run.balance(treasury));
    run.send_nft(
        "bob gives the name to carol by transferring the NFT",
        &mut bob,
        &nft::Message::Instruction(nft::Instruction::Transfer {
            token_id: t1,
            new_owner: owner_tag(&slot, &carol_id),
            sealed: None,
            now: Some(t),
            linked_target: Some(slot),
        }),
        vec![token, run.reg(record_address(&registry, &slot)), run.reg(page_address(&registry, &slot)), settings],
        t,
    );
    assert_eq!(run.record(&slot).expect("held").owner, owner_tag(&slot, &carol_id));
    assert_eq!((run.balance(bob_id), run.balance(treasury)), (b0, tr0), "plain transfers are free");
    run.assert_lockstep(&slot);
    println!("ok   the registry followed the NFT transfer");

    // ── 5. Sites directory: publishing a site lists it, discover=off takes it out.
    let t = t + 60_000;
    run.seeds.push((registry, site_bucket_seed(bucket_of(&slot), 0)));
    let sites = run.reg(site_bucket_address(&registry, bucket_of(&slot), 0));
    let header = run.reg(site_dir_header_address(&registry));
    let mut publishing = run.name_actors(&slot);
    publishing.extend([sites, header]);
    run.send(
        "carol publishes a site page (discovery on by default)",
        &mut carol,
        Instruction::SetPage {
            id: slot,
            sealed: b"<html>".to_vec(),
            site_dir: SiteDirOp::Upsert { name: "jefdiesel".into(), title: "Carol's site".into(), page: 0, now: t },
        },
        publishing.clone(),
        t,
    );
    let listed: SiteBucket = run.decode(sites.account_id, registry).expect("a site bucket");
    assert!(listed.entries.iter().any(|entry| entry.id == slot && entry.name == "jefdiesel"));
    let count: SiteDirHeader = run.decode(header.account_id, registry).expect("a header");
    assert_eq!(count.total_listed, 1);
    println!("ok   the site is in the directory");
    run.send(
        "carol sets discover=off",
        &mut carol,
        Instruction::SetRecord { id: slot, sealed: b"discover=off".to_vec(), site_dir: SiteDirOp::Remove },
        publishing,
        t,
    );
    let listed: Option<SiteBucket> = run.decode(sites.account_id, registry);
    assert!(listed.is_none_or(|bucket| bucket.entries.iter().all(|entry| entry.id != slot)));
    let count: SiteDirHeader = run.decode(header.account_id, registry).expect("a header");
    assert_eq!(count.total_listed, 0);
    assert_eq!(run.record(&slot).expect("held").site_listed, None);
    println!("ok   the site left the directory");

    // ── 6. Subnames: carol creates a revocable `blog.<name>` for dave, then revokes it.
    let blog: NameId = [0xB1; 32];
    run.seeds.extend(subname_seeds(&slot, &blog).map(|seed| (registry, seed)));
    let sub_actors = vec![
        run.reg(record_address(&registry, &slot)),
        run.reg(subname_record_address(&registry, &slot, &blog)),
        run.reg(subname_page_address(&registry, &slot, &blog)),
    ];
    run.send(
        "carol creates a revocable subname for dave",
        &mut carol,
        Instruction::CreateSubname {
            parent_id: slot,
            sub_id: blog,
            owner: owner_tag(&blog, &dave_id),
            revocable: true,
            sealed: b"blog".to_vec(),
            now: t,
        },
        sub_actors.clone(),
        t,
    );
    let sub: SubnameRecord = run.decode(subname_record_address(&registry, &slot, &blog), registry).expect("a subname");
    assert_eq!(sub.owner, owner_tag(&blog, &dave_id));
    run.send("carol revokes it", &mut carol, Instruction::RevokeSubname { parent_id: slot, sub_id: blog, now: t }, sub_actors, t);
    assert!(run.decode::<SubnameRecord>(subname_record_address(&registry, &slot, &blog), registry).is_none());
    println!("ok   the subname is gone");

    // ── 7. Lapse: in grace the name is frozen on both paths.
    let expires = run.record(&slot).expect("held").expires;
    let in_grace = expires + DAY;
    run.refuse(
        "in grace, a registry transfer is refused",
        "lapsed name cannot transfer",
        &mut carol,
        Message::Instruction(Instruction::Transfer { id: slot, new_owner: owner_tag(&slot, &bob_id), now: in_grace }),
        {
            let mut actors = run.name_actors(&slot);
            actors.extend([settings, token]);
            actors
        },
        in_grace,
    );
    let frozen = nft::Message::Instruction(nft::Instruction::Transfer {
        token_id: t1,
        new_owner: owner_tag(&slot, &bob_id),
        sealed: None,
        now: Some(in_grace),
        linked_target: Some(slot),
    });
    let error = run
        .try_send(&mut carol, nft_program, borsh::to_vec(&frozen).expect("bytes"), vec![token, run.reg(record_address(&registry, &slot)), run.reg(page_address(&registry, &slot)), settings], in_grace)
        .expect_err("frozen");
    assert!(error.contains("linked token has expired"), "{error}");
    println!("ok   in grace, an NFT transfer is refused (refused: linked token has expired)");

    // ── 8. After grace, the re-release premium: halving daily over 21 days, paid to the treasury.
    let grace_end = expires + GRACE_MS;
    let reveal = grace_end + 5 * DAY + DAY / 2;
    let commit_at = reveal - MIN_COMMIT_AGE_MS;
    let again = run.commit(&mut dave, jef, [6; 32], commit_at, reveal);
    let premium = rerelease_premium(PREMIUM_START, reveal - grace_end);
    assert!(premium > 0 && premium < PREMIUM_START / 32 && premium > PREMIUM_START / 64, "5.5 days in: between 1/32 and 1/64 of the start");
    let owed = year_price + premium;
    let actors = run.registering(&again, dave_id, treasury, Some(t1));
    run.refuse(
        "re-registering without the premium is refused",
        "the fee is not the price of this term",
        &mut dave,
        Message::Instruction(run.register_instruction(&again, owed - 1, treasury, reveal)),
        actors.clone(),
        reveal,
    );
    let tr0 = run.balance(treasury);
    run.send(
        "dave re-registers the lapsed name, paying the premium",
        &mut dave,
        run.register_instruction(&again, owed, treasury, reveal),
        actors,
        reveal,
    );
    assert_eq!(run.balance(treasury), tr0 + owed);
    let record = run.record(&slot).expect("re-registered");
    assert_eq!(record.owner, owner_tag(&slot, &dave_id));
    assert_eq!(record.nft_token_id, again.token_id);
    assert_ne!(again.token_id, t1);
    assert!(run.token(&t1).is_none(), "the old NFT is burned");
    run.assert_lockstep(&slot);
    println!("ok   premium {premium} on top of {year_price}; old NFT burned, new NFT minted to dave");

    // ── 9. The rest of the tested port's flow, on the v8 registry with its NFTs.
    let t = reveal + MIN_COMMIT_AGE_MS;
    run.refuse(
        "outsiders cannot send an effect",
        "only this program may send an effect",
        &mut bob,
        Message::Effect(names_core::Effect::Remove { id: slot, signer: owner_tag(&slot, &bob_id) }),
        run.name_actors(&slot),
        t,
    );
    let t2 = again.token_id;
    let token2 = run.token_actor(&t2);
    let mut actors = run.name_actors(&slot);
    actors.extend([settings, token2]);
    run.refuse(
        "bob cannot take dave's name",
        "signer does not own this name",
        &mut bob,
        Message::Instruction(Instruction::Transfer { id: slot, new_owner: owner_tag(&slot, &bob_id), now: t }),
        actors.clone(),
        t,
    );
    run.send(
        "dave updates his profile (no directory change)",
        &mut dave,
        Instruction::SetRecord { id: slot, sealed: b"new".to_vec(), site_dir: SiteDirOp::Keep },
        run.name_actors(&slot),
        t,
    );
    run.send(
        "dave transfers the name to alice through the registry",
        &mut dave,
        Instruction::Transfer { id: slot, new_owner: owner_tag(&slot, &alice_id), now: t },
        actors.clone(),
        t,
    );
    run.assert_lockstep(&slot);
    run.send("alice releases the name", &mut alice, Instruction::Release { id: slot, refund: 0 }, actors, t);
    assert!(run.record(&slot).is_none());
    assert!(run.token(&t2).is_none(), "release burns the NFT");
    println!("ok   release emptied the record and burned the NFT");

    // Claims: free, permanent, once, and only by the old holder; a claim mints the NFT too.
    let claim_slot = tiered_id(&held, PRICE_TIER_8_PLUS);
    let claim_token = nft_token_id(&claim_slot, &claim_generation(&claim_slot, &owner_tag(&claim_slot, &carol_id), t));
    run.seeds.extend(name_seeds(&claim_slot).map(|seed| (registry, seed)));
    run.seeds.push((nft_program, nft_token_seed(&claim_token)));
    let mut actors = run.name_actors(&claim_slot);
    actors.extend([settings, run.token_actor(&claim_token), config]);
    let claim = || Instruction::Claim {
        id: held,
        sealed: b"carried".to_vec(),
        now: t,
        proof: claim_proof(&leaves, 0),
        price_tier: PRICE_TIER_8_PLUS,
    };
    run.refuse("bob cannot claim carol's old name", "this account did not hold this name", &mut bob, Message::Instruction(claim()), actors.clone(), t);
    let before = run.balance(carol_id);
    run.send("carol claims her old name for free", &mut carol, claim(), actors.clone(), t);
    let record = run.record(&claim_slot).expect("claimed");
    assert!(record.claimed && record.expires == u64::MAX && record.sealed == b"carried");
    assert_eq!(record.nft_token_id, claim_token);
    assert_eq!(run.balance(carol_id), before, "a claim costs nothing");
    run.assert_lockstep(&claim_slot);
    run.refuse("a name is claimed once", "this name was already claimed", &mut carol, Message::Instruction(claim()), actors, t);

    // A public registration: CommitPublic is valid only within a minute of its `now`, and the
    // reveal waits 5 minutes, not 15.
    let quick: NameId = [0xDD; 32];
    let quick_slot = tiered_id(&quick, PRICE_TIER_8_PLUS);
    let secret = [7; 32];
    let promise = commitment(&quick_slot, &owner_tag(&quick_slot, &bob_id), &secret);
    run.seeds.push((registry, commitment_seed(&promise)));
    let commit_actors = vec![run.reg(commitment_address(&registry, &promise))];
    let commit_public = |now| Instruction::CommitPublic { commitment: promise, now };
    run.refuse(
        "a public commit outside its one-minute window is refused",
        "OutOfValidityWindow",
        &mut bob,
        Message::Instruction(commit_public(t)),
        commit_actors.clone(),
        t + PUBLIC_TIME_WINDOW_MS,
    );
    run.send("bob commits to a name in public", &mut bob, commit_public(t), commit_actors, t);
    let early = t + PUBLIC_MIN_COMMIT_AGE_MS - 1;
    let ready = t + PUBLIC_MIN_COMMIT_AGE_MS;
    let quick_reg = |now| Registration {
        base: quick,
        slot: quick_slot,
        secret,
        token_id: nft_token_id(&quick_slot, &register_generation(&quick_slot, &owner_tag(&quick_slot, &bob_id), now, &secret)),
        length: short_list().prove(&quick).expect("a proof"),
    };
    let (soon, later) = (quick_reg(early), quick_reg(ready));
    run.seeds.extend(name_seeds(&quick_slot).map(|seed| (registry, seed)));
    run.seeds.extend([(nft_program, nft_token_seed(&soon.token_id)), (nft_program, nft_token_seed(&later.token_id))]);
    run.refuse(
        "a public reveal before the 5-minute wait is refused",
        "the commitment is too recent",
        &mut bob,
        Message::Instruction(run.register_instruction(&soon, year_price, treasury, early)),
        run.registering(&soon, bob_id, treasury, None),
        early,
    );
    run.send(
        "bob registers it after the 5-minute public wait",
        &mut bob,
        run.register_instruction(&later, year_price, treasury, ready),
        run.registering(&later, bob_id, treasury, None),
        ready,
    );
    run.assert_lockstep(&quick_slot);
    println!("ok   the public wait is 5 minutes");

    // Deposit mode: the admin switches modes (the NFT program stays bound); a deposit moves
    // into the name's account and comes back on release.
    let t = ready + MIN_COMMIT_AGE_MS;
    run.refuse(
        "the NFT program cannot be changed",
        "nft_program is immutable once configured",
        &mut admin,
        Message::Instruction(Instruction::Configure {
            admin: admin_id,
            treasury,
            prices: PRICES,
            mode: MODE_FEE,
            deposits: [0, 0],
            claims_root: claim_root(&leaves),
            claims_until: T0 + 3650 * DAY,
            claims_program: old_program,
            premium_start: PREMIUM_START,
            nft_program: AccountId::new([0x99; 32]),
        }),
        vec![settings],
        t,
    );
    run.send("the admin switches the registry to deposits", &mut admin, configure(MODE_DEPOSIT, [DEPOSIT, 2 * DEPOSIT]), vec![settings], t);
    let dep: NameId = [0xEE; 32];
    let dep_reg = run.commit(&mut bob, dep, [8; 32], t, t + MIN_COMMIT_AGE_MS);
    let t = t + MIN_COMMIT_AGE_MS;
    let mut actors = run.registering(&dep_reg, bob_id, treasury, None);
    actors.push(Actor::native_balance(deposit_address(&registry, &dep_reg.slot)));
    let before = run.balance(bob_id);
    run.send(
        "bob registers with a deposit",
        &mut bob,
        Instruction::Register {
            id: dep,
            sealed: Vec::new(),
            term: 0,
            fee: DEPOSIT,
            treasury: None,
            now: t,
            secret: dep_reg.secret,
            deposit: true,
            length: dep_reg.length.clone(),
            price_tier: PRICE_TIER_8_PLUS,
        },
        actors,
        t,
    );
    assert_eq!(run.balance(bob_id), before - DEPOSIT);
    assert_eq!(run.balance(deposit_address(&registry, &dep_reg.slot)), DEPOSIT);
    assert_eq!(run.record(&dep_reg.slot).expect("held").expires, u64::MAX);
    run.assert_lockstep(&dep_reg.slot);
    let mut actors = run.name_actors(&dep_reg.slot);
    actors.extend([
        settings,
        run.token_actor(&dep_reg.token_id),
        Actor::native_balance(deposit_address(&registry, &dep_reg.slot)),
        Actor::native_balance(bob_id),
    ]);
    run.send("bob releases it and collects the deposit", &mut bob, Instruction::Release { id: dep_reg.slot, refund: DEPOSIT }, actors, t);
    assert_eq!(run.balance(bob_id), before);
    assert!(run.token(&dep_reg.token_id).is_none());
    println!("ok   the deposit moved in and back atomically, and the NFT was burned");

    println!("\nall scenarios passed");
}
