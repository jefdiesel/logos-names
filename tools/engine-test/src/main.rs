//! End-to-end run of the registry on the LEZ 0.4 engine: the real guest ELF,
//! real signatures and the real state machine, with no chain or network.

use std::collections::{BTreeMap, BTreeSet};

use lee::{
    PrivateKey, PublicAccountEvidence, PublicKey, PublicTransaction, V03State,
    program::Program,
    public_transaction::{Message as TxMessage, WitnessSet},
};
use lee_core::{
    account::{AccountId, Actor, Nonce},
    program::PdaSeed,
};
use names_core::{
    Instruction, Message, NameId, NameRecord, Settings, LengthProof, MIN_COMMIT_AGE_MS,
    MODE_DEPOSIT, claim_leaf, claim_proof, claim_root, commitment, commitment_address, deposit_address,
    name_seeds, commitment_seed, owner_tag, page_address, record_address, settings_address, settings_seed,
    OwnerTag, decode_record, deed, deed_definition_address, deed_definition_seed, deed_holding_address,
    deed_holding_seed, registration_nonce,
};
use deed::{NftMessage, NftState, NFT_PROGRAM};

const T0: u64 = 1_800_000_000_000;
const DEPOSIT: u128 = 1_000_000;

struct Who {
    key: PrivateKey,
    id: AccountId,
    nonce: u128,
}

fn who(byte: u8) -> Who {
    let key = PrivateKey::try_new([byte; 32]).expect("a valid key");
    let id = AccountId::from(&PublicKey::new_from_private_key(&key));
    Who { key, id, nonce: 0 }
}

struct Run {
    state: V03State,
    program: AccountId,
    block: u64,
    seeds: Vec<PdaSeed>,
}

impl Run {
    fn actor(&self, account: AccountId) -> Actor {
        Actor::new(account, self.program)
    }

    /// Evidence for every program account these actors could create.
    fn evidence(&self, seeds: &[PdaSeed]) -> Vec<PublicAccountEvidence> {
        seeds.iter().map(|seed| PublicAccountEvidence::Pda { program: self.program, seed: *seed }).collect()
    }

    fn try_send(&mut self, from: &mut Who, message: &Message, actors: Vec<Actor>, now: u64) -> Result<(), String> {
        let root = self.actor(from.id);
        let bytes = borsh::to_vec(message).expect("a message");
        self.try_send_to(from, root, bytes, actors, now)
    }

    /// Sends `bytes` to the actor `root`, signed by `from`.
    fn try_send_to(&mut self, from: &mut Who, root: Actor, bytes: Vec<u8>, actors: Vec<Actor>, now: u64) -> Result<(), String> {
        let mut public = actors;
        public.push(root);
        public.push(self.actor(from.id));
        let nonces = BTreeMap::from([(from.id, Nonce(from.nonce))]);
        let wanted: BTreeSet<AccountId> = public.iter().map(|actor| actor.account_id).collect();
        let evidence = self
            .evidence(&self.seeds)
            .into_iter()
            .filter(|evidence| wanted.contains(&evidence.account_id()))
            .collect();
        let mut seen = BTreeSet::new();
        public.retain(|actor| seen.insert((actor.account_id, actor.program_account_id)));
        let tx = TxMessage::new(root, bytes, public, nonces, None, evidence);
        let witness = WitnessSet::for_message(&tx, &[&from.key]);
        let tx = PublicTransaction::new(tx, witness);
        self.block += 1;
        self.state
            .transition_from_public_transaction(&tx, self.block, now)
            .map(|_| from.nonce += 1)
            .map_err(|error| format!("{error:?}"))
    }

    fn send(&mut self, what: &str, from: &mut Who, instruction: Instruction, actors: Vec<Actor>, now: u64) {
        self.try_send(from, &Message::Instruction(instruction), actors, now)
            .unwrap_or_else(|error| panic!("{what}: {error}"));
        println!("ok   {what}");
    }

    fn refuse(&mut self, what: &str, expected: &str, from: &mut Who, message: Message, actors: Vec<Actor>, now: u64) {
        let error = self.try_send(from, &message, actors, now).expect_err(what);
        assert!(error.contains(expected), "{what}: refused for the wrong reason: {error}");
        if expected.is_empty() {
            println!("ok   {what} (refused: {})", error.chars().take(220).collect::<String>());
        } else {
            println!("ok   {what} (refused: {expected})");
        }
    }

    fn record(&self, id: &NameId) -> Option<NameRecord> {
        let account = self.state.get_account_by_id(record_address(&self.program, id));
        let state = account.data.actor_state(self.program);
        (!state.is_empty()).then(|| decode_record(state).expect("a record"))
    }

    /// The NFT program's state on `account`.
    fn nft(&self, account: AccountId) -> Option<NftState> {
        deed::decode_state(self.state.get_account_by_id(account).data.actor_state(NFT_PROGRAM))
    }

    /// The deed account of the holder tagged `owner` for the deed `definition`.
    fn holding(&self, definition: &AccountId, owner: &OwnerTag) -> AccountId {
        deed_holding_address(&self.program, definition, owner)
    }

    /// The print balance the deed account of `owner` holds of `definition` (None: no holding).
    fn deed_balance(&self, definition: &AccountId, owner: &OwnerTag) -> Option<u128> {
        match self.nft(self.holding(definition, owner)) {
            Some(NftState::Master(master)) => {
                assert_eq!(master.definition_id, *definition);
                Some(master.print_balance)
            }
            None => None,
            other => panic!("a deed account holds a master, not {other:?}"),
        }
    }

    fn supply(&self, definition: &AccountId) -> u128 {
        match self.nft(*definition) {
            Some(NftState::Definition(found)) => found.printable_supply,
            other => panic!("not a definition: {other:?}"),
        }
    }

    /// The actors a change to `id`'s deed reaches, for the holders `tags`: the
    /// definition and each holder's deed account, under the NFT program and the
    /// registry, with the seeds that admit them when new.
    fn deed_actors(&mut self, definition: &AccountId, nonce: Option<(&NameId, &[u8; 32])>, tags: &[OwnerTag]) -> Vec<Actor> {
        let mut actors = vec![Actor::new(*definition, NFT_PROGRAM)];
        if let Some((id, nonce)) = nonce {
            self.seeds.push(deed_definition_seed(id, nonce));
        }
        for tag in tags {
            let holding = self.holding(definition, tag);
            self.seeds.push(deed_holding_seed(definition, tag));
            actors.extend([Actor::new(holding, NFT_PROGRAM), self.actor(holding)]);
        }
        actors
    }

    fn balance(&self, account: AccountId) -> u128 {
        self.state.get_account_by_id(account).data.native_balance().expect("a balance")
    }

    fn name_actors(&self, id: &NameId) -> Vec<Actor> {
        vec![
            self.actor(record_address(&self.program, id)),
            self.actor(page_address(&self.program, id)),
        ]
    }

    fn registering(&self, id: &NameId, payer: AccountId, secret: &[u8; 32]) -> Vec<Actor> {
        let promised = commitment(id, &owner_tag(id, &payer), secret);
        let mut actors = self.name_actors(id);
        actors.extend([
            self.actor(settings_address(&self.program)),
            self.actor(commitment_address(&self.program, &promised)),
            Actor::native_balance(payer),
            Actor::native_balance(deposit_address(&self.program, id)),
        ]);
        actors
    }
}

/// The short-name list the guest's root was built from.
fn short_list() -> names_core::shortlist::ShortList {
    let leaves = include_bytes!("../../../shortlist/shortlist.bin")
        .chunks_exact(33)
        .map(|chunk| names_core::shortlist::Leaf { id: chunk[..32].try_into().expect("32 bytes"), tier: chunk[32] })
        .collect();
    names_core::shortlist::ShortList::from_sorted_leaves(leaves).expect("a sorted list")
}

fn register(id: NameId, secret: [u8; 32], now: u64, length: LengthProof) -> Instruction {
    Instruction::Register {
        id,
        sealed: b"profile".to_vec(),
        term: 0,
        fee: DEPOSIT,
        treasury: None,
        now,
        secret,
        deposit: true,
        length,
    }
}

/// [`register`], minting the name's deed.
fn register_deeded(id: NameId, secret: [u8; 32], now: u64, length: LengthProof) -> Instruction {
    let Instruction::Register { id, sealed, term, fee, treasury, now, secret, deposit, length } = register(id, secret, now, length) else {
        unreachable!()
    };
    Instruction::RegisterWithDeed { id, sealed, term, fee, treasury, now, secret, deposit, length }
}

fn main() {
    let elf = names_methods::LOGOS_NAMES_ELF;
    let mut deployer = who(9);
    let admin = who(1);
    let mut alice = who(2);
    let mut bob = who(3);
    let carol = who(4);
    let program = deployer.id;

    // Logos's NFT program, unmodified: lez-programs dea23c2, the native nft.bin of Job A.
    let nft_bin = std::fs::read(std::env::var("LNS_NFT_BIN").expect("LNS_NFT_BIN: the NFT program's nft.bin"))
        .expect("nft.bin reads");
    let nft = Program::new(nft_bin.into()).expect("the NFT program loads");
    let image: Vec<u8> = nft.id().iter().flat_map(|word| word.to_le_bytes()).collect();
    assert_eq!(image, deed::NFT_PROGRAM_IMAGE_ID, "the pinned NFT program image");
    println!("ok   the NFT program loaded is the pinned image {}", image.iter().map(|b| format!("{b:02x}")).collect::<String>());
    let state = V03State::new()
        .with_public_account_balances([(alice.id, 10 * DEPOSIT), (bob.id, 10 * DEPOSIT), (admin.id, DEPOSIT)])
        .with_named_programs([
            (program, Program::new(elf.into()).expect("the guest loads")),
            (NFT_PROGRAM, nft),
        ]);
    let mut seeds = vec![settings_seed()];
    let mut run = Run { state, program, block: 0, seeds: Vec::new() };

    // Names: not on the short list, in any tier, so use an unlisted proof.
    let jef: NameId = [0xAA; 32];
    let ada: NameId = [0xBB; 32];
    let held: NameId = [0xCC; 32];
    // The registry the claims were snapshotted from, on the earlier chain.
    let old_program = AccountId::new([0x38; 32]);
    let leaves = vec![
        claim_leaf(&record_address(&old_program, &held), &owner_tag(&held, &alice.id)),
        claim_leaf(&record_address(&old_program, &ada), &owner_tag(&ada, &carol.id)),
    ];
    let settings = run.actor(settings_address(&program));
    let secret = [5; 32];
    let promised = commitment(&jef, &owner_tag(&jef, &alice.id), &secret);
    for id in [&jef, &ada, &held] {
        seeds.extend(name_seeds(id));
    }
    seeds.push(commitment_seed(&promised));
    run.seeds = seeds;

    let configure = Instruction::Configure {
        admin: admin.id,
        treasury: admin.id,
        prices: [0; 3],
        mode: MODE_DEPOSIT,
        deposits: [DEPOSIT, DEPOSIT],
        claims_root: claim_root(&leaves),
        claims_until: T0 + 10 * MIN_COMMIT_AGE_MS,
        claims_program: old_program,
    };
    run.refuse(
        "a stranger cannot configure a fresh registry",
        "the first settings must be signed",
        &mut bob,
        Message::Instruction(configure.clone()),
        vec![settings],
        T0,
    );
    run.send("the deployer configures the registry", &mut deployer, configure, vec![settings], T0);
    let stored: Settings = borsh::from_slice(
        run.state.get_account_by_id(settings_address(&program)).data.actor_state(program),
    )
    .expect("settings");
    assert_eq!(stored.admin, admin.id);

    // Register, with the real proof for an unlisted id.
    run.send(
        "alice commits to a name",
        &mut alice,
        Instruction::Commit { commitment: promised, now: T0 },
        vec![run.actor(commitment_address(&program, &promised))],
        T0,
    );
    let later = T0 + MIN_COMMIT_AGE_MS;
    let length = short_list().prove(&jef).expect("a proof");
    let before = run.balance(alice.id);
    let mut actors = run.registering(&jef, alice.id, &secret);
    let (alices, bobs) = (owner_tag(&jef, &alice.id), owner_tag(&jef, &bob.id));
    let nonce = registration_nonce(&promised, later);
    let definition = deed_definition_address(&program, &jef, &nonce);
    actors.extend(run.deed_actors(&definition, Some((&jef, &nonce)), &[alices]));
    run.send("alice registers with a deposit", &mut alice, register_deeded(jef, secret, later, length), actors, later);
    let record = run.record(&jef).expect("registered");
    assert_eq!(record.owner, alices);
    assert_eq!(run.balance(alice.id), before - DEPOSIT, "the deposit left alice in the same transaction");
    assert_eq!(run.balance(deposit_address(&program, &jef)), DEPOSIT, "and reached the name's account");
    println!("ok   the deposit moved atomically");
    // N1: the nested grant mints the deed in the registration's own transaction.
    assert_eq!(record.deed.map(|deed| deed.definition), Some(definition), "the record names its deed");
    match run.nft(definition) {
        Some(NftState::Definition(found)) => {
            assert_eq!((found.name.as_str(), found.printable_supply), (deed::DEED_NAME, 1));
            assert_eq!(found.metadata.uri, deed::DEED_URI);
        }
        other => panic!("no deed definition: {other:?}"),
    }
    assert_eq!(run.deed_balance(&definition, &alices), Some(1), "alice's deed account holds the deed");
    println!("ok   N1: the registration minted a 1-of-1 deed into alice's deed account");

    let bobs_tag = owner_tag(&jef, &bob.id);
    let actors = run.name_actors(&jef);
    run.refuse(
        "outsiders cannot send an effect",
        "only this program may send an effect",
        &mut bob,
        Message::Effect(names_core::Effect::Remove { signer: bobs_tag }),
        actors,
        later,
    );
    let stolen = Instruction::Transfer { id: jef, new_owner: owner_tag(&jef, &bob.id) };
    let actors = run.name_actors(&jef);
    run.refuse(
        "bob cannot take alice's name",
        "signer does not own this name",
        &mut bob,
        Message::Instruction(stolen),
        actors,
        later,
    );

    let actors = run.name_actors(&jef);
    run.send(
        "alice updates her profile",
        &mut alice,
        Instruction::SetRecord { id: jef, sealed: b"new".to_vec() },
        actors.clone(),
        later,
    );
    // A client that doesn't declare the deed's actors can't transfer a deeded name.
    run.refuse(
        "a transfer that leaves the deed out is refused",
        "",
        &mut alice,
        Message::Instruction(Instruction::Transfer { id: jef, new_owner: bobs }),
        actors.clone(),
        later,
    );
    assert_eq!(run.record(&jef).expect("held").owner, alices, "and nothing moved");
    let mut moving = actors.clone();
    moving.extend(run.deed_actors(&definition, None, &[alices, bobs]));
    run.send(
        "alice transfers the name to bob",
        &mut alice,
        Instruction::Transfer { id: jef, new_owner: bobs },
        moving.clone(),
        later,
    );
    assert_eq!(run.record(&jef).expect("held").owner, bobs);
    // N3: the deed moved with it, in the same transaction.
    assert_eq!(run.deed_balance(&definition, &alices), Some(0), "alice's deed account is empty");
    assert_eq!(run.deed_balance(&definition, &bobs), Some(1), "bob's holds the deed");
    println!("ok   N3: the transfer moved the deed alice -> bob in one transaction");

    // N2: nobody moves or burns a deed through the NFT program directly, not even its holder.
    let bobs_deed = run.holding(&definition, &bobs);
    let bobs_own = Actor::new(bob.id, NFT_PROGRAM);
    for (what, message) in [
        ("bob cannot move his deed through the NFT program", NftMessage::Transfer { to: bob.id }),
        ("bob cannot burn his deed through the NFT program", NftMessage::Burn { amount: 1 }),
    ] {
        let expected = if matches!(message, NftMessage::Burn { .. }) { "holding must authorize the burn" } else { "sender holding must authorize the transfer" };
        let error = run
            .try_send_to(
                &mut bob,
                Actor::new(bobs_deed, NFT_PROGRAM),
                borsh::to_vec(&message).expect("encodes"),
                vec![bobs_own, Actor::new(definition, NFT_PROGRAM)],
                later,
            )
            .expect_err(what);
        assert!(error.contains(expected), "{what}: refused for the wrong reason: {error}");
        println!("ok   N2: {what} (refused by the NFT program: {expected})");
    }
    let error = run
        .try_send_to(
            &mut alice,
            Actor::new(run.holding(&definition, &alices), NFT_PROGRAM),
            borsh::to_vec(&NftMessage::InitializeHolding { definition }).expect("encodes"),
            vec![Actor::new(definition, NFT_PROGRAM)],
            later,
        )
        .expect_err("alice cannot touch her old deed account");
    assert!(error.contains("must authorize"), "refused for the wrong reason: {error}");
    println!("ok   N2: alice cannot use her old deed account through the NFT program either");
    assert_eq!(run.deed_balance(&definition, &bobs), Some(1), "the deed stayed with bob");

    let before = run.balance(bob.id);
    let mut actors = actors;
    actors.extend(run.deed_actors(&definition, None, &[bobs]));
    actors.extend([
        Actor::native_balance(deposit_address(&program, &jef)),
        Actor::native_balance(bob.id),
    ]);
    run.send(
        "bob releases the name and collects the deposit",
        &mut bob,
        Instruction::Release { id: jef, refund: DEPOSIT },
        actors,
        later,
    );
    assert_eq!(run.record(&jef), None);
    assert_eq!(run.balance(bob.id), before + DEPOSIT);
    assert_eq!(run.balance(deposit_address(&program, &jef)), 0);
    println!("ok   the refund moved atomically");
    assert_eq!(run.deed_balance(&definition, &bobs), Some(0), "the deed is burned");
    assert_eq!(run.supply(&definition), 0, "its definition has no supply left");
    println!("ok   the release burned the deed (supply 0)");

    // Claims.
    let claim = |id, proof, now| Instruction::Claim { id, sealed: b"carried".to_vec(), now, proof };
    let mut actors = run.name_actors(&held);
    actors.push(settings);
    run.refuse(
        "bob cannot claim alice's name",
        "this account did not hold this name",
        &mut bob,
        Message::Instruction(claim(held, claim_proof(&leaves, 0), later)),
        actors.clone(),
        later,
    );
    let balance = run.balance(alice.id);
    run.send(
        "alice claims her old name for free",
        &mut alice,
        claim(held, claim_proof(&leaves, 0), later),
        actors.clone(),
        later,
    );
    let record = run.record(&held).expect("claimed");
    assert!(record.claimed && record.expires == u64::MAX && record.sealed == b"carried");
    assert_eq!(run.balance(alice.id), balance, "a claim costs no deposit");
    run.refuse(
        "a name is claimed once",
        "this name was already claimed",
        &mut alice,
        Message::Instruction(claim(held, claim_proof(&leaves, 0), later)),
        actors,
        later,
    );
    assert_eq!(record.deed, None, "a claimed name has no deed until its holder mints one");

    // A name without a deed (here a claimed one; on the live chain, every name registered
    // before deeds) gets one from its holder.
    let helds = owner_tag(&held, &alice.id);
    let nonce = [0x4d; 32];
    let held_definition = deed_definition_address(&program, &held, &nonce);
    let mut actors = vec![run.actor(record_address(&program, &held))];
    actors.extend(run.deed_actors(&held_definition, Some((&held, &nonce)), &[helds]));
    let mint = Instruction::MintDeed { id: held, nonce, length: short_list().prove(&held).expect("a proof") };
    run.refuse(
        "bob cannot mint the deed of alice's name",
        "signer does not own this name",
        &mut bob,
        Message::Instruction(mint.clone()),
        actors.clone(),
        later,
    );
    run.send("alice mints the deed of a name that had none", &mut alice, mint.clone(), actors.clone(), later);
    assert_eq!(run.record(&held).expect("held").deed.map(|deed| deed.definition), Some(held_definition));
    assert_eq!(run.deed_balance(&held_definition, &helds), Some(1));
    println!("ok   the opt-in mint gave the name its deed");
    let again = Instruction::MintDeed { id: held, nonce: [0x4e; 32], length: short_list().prove(&held).expect("a proof") };
    let mut again_actors = vec![run.actor(record_address(&program, &held))];
    let other = deed_definition_address(&program, &held, &[0x4e; 32]);
    again_actors.extend(run.deed_actors(&other, Some((&held, &[0x4e; 32])), &[helds]));
    run.refuse("a name has one deed", "this name already has a deed", &mut alice, Message::Instruction(again), again_actors, later);
    let locked = short_list()
        .leaves()
        .iter()
        .find(|leaf| leaf.tier == names_core::TIER_LOCKED)
        .expect("a locked name")
        .id;
    let locked_mint = Instruction::MintDeed { id: locked, nonce, length: short_list().prove(&locked).expect("a proof") };
    run.refuse(
        "a locked name has no deed",
        "so it has no deed",
        &mut alice,
        Message::Instruction(locked_mint),
        vec![run.actor(record_address(&program, &locked))],
        later,
    );

    // A transfer to a private recipient: the instruction carries the private account's tag,
    // and the deed goes to that tag's deed account in the same transaction.
    let vpk = lee_core::encryption::shared_key_derivation::MlKem768EncapsulationKey::from_seed(&[0x51; 32], &[0x52; 32]);
    let private = AccountId::for_regular_private_account(&lee_core::NullifierPublicKey([0x53; 32]), &vpk);
    let privates = owner_tag(&held, &private);
    let mut actors = run.name_actors(&held);
    actors.extend(run.deed_actors(&held_definition, None, &[helds, privates]));
    run.send(
        "alice transfers a deeded name to a private account",
        &mut alice,
        Instruction::Transfer { id: held, new_owner: privates },
        actors,
        later,
    );
    assert_eq!(run.record(&held).expect("held").owner, privates);
    assert_eq!(run.deed_balance(&held_definition, &helds), Some(0));
    assert_eq!(run.deed_balance(&held_definition, &privates), Some(1), "the private holder's deed account holds it");
    println!("ok   the deed moved to the private recipient's deed account in one transaction");

    // Released and registered again, a name gets a new deed: the old definition is never reused.
    let secret2 = [6; 32];
    let promised2 = commitment(&jef, &alices, &secret2);
    run.seeds.push(commitment_seed(&promised2));
    let again_at = later + 1;
    run.send(
        "alice commits to jef again",
        &mut alice,
        Instruction::Commit { commitment: promised2, now: again_at },
        vec![run.actor(commitment_address(&program, &promised2))],
        again_at,
    );
    let reveal_at = again_at + MIN_COMMIT_AGE_MS;
    let nonce2 = registration_nonce(&promised2, reveal_at);
    let definition2 = deed_definition_address(&program, &jef, &nonce2);
    let mut actors = run.registering(&jef, alice.id, &secret2);
    actors.extend(run.deed_actors(&definition2, Some((&jef, &nonce2)), &[alices]));
    let length = short_list().prove(&jef).expect("a proof");
    run.send("alice registers jef again", &mut alice, register_deeded(jef, secret2, reveal_at, length), actors, reveal_at);
    assert_ne!(definition2, definition, "a new registration, a new deed");
    assert_eq!(run.deed_balance(&definition2, &alices), Some(1));
    assert_eq!(run.supply(&definition), 0, "the old deed stays burned");
    println!("ok   a name registered again gets a new deed definition and deed account");

    // A client built before deeds sends plain Register with the old actor list: it still
    // works, the name has no deed, and its record is the version 6 layout older readers read.
    let plain: NameId = [0xEE; 32];
    let secret3 = [7; 32];
    let promised3 = commitment(&plain, &owner_tag(&plain, &bob.id), &secret3);
    run.seeds.extend(name_seeds(&plain));
    run.seeds.push(commitment_seed(&promised3));
    run.send(
        "bob commits as an older client would",
        &mut bob,
        Instruction::Commit { commitment: promised3, now: again_at },
        vec![run.actor(commitment_address(&program, &promised3))],
        again_at,
    );
    let actors = run.registering(&plain, bob.id, &secret3);
    let length = short_list().prove(&plain).expect("a proof");
    run.send("bob registers with plain Register and no deed actors", &mut bob, register(plain, secret3, reveal_at, length), actors, reveal_at);
    let record = run.record(&plain).expect("registered");
    assert_eq!(record.deed, None);
    let bytes = run.state.get_account_by_id(record_address(&program, &plain)).data.actor_state(program).to_vec();
    assert_eq!(bytes.first(), Some(&6), "the version 6 layout");
    let old: (u8, [u8; 32], u64, Vec<u8>, bool) = borsh::from_slice(&bytes).expect("a strict version 6 reader reads it");
    assert_eq!(old.1, owner_tag(&plain, &bob.id));
    println!("ok   an older client's plain Register still registers (no deed, version 6 record)");
    println!("\nall scenarios passed");
}
