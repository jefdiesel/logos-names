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
};

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
        let mut public = actors;
        public.push(self.actor(from.id));
        let nonces = BTreeMap::from([(from.id, Nonce(from.nonce))]);
        let wanted: BTreeSet<AccountId> = public.iter().map(|actor| actor.account_id).collect();
        let evidence = self
            .evidence(&self.seeds)
            .into_iter()
            .filter(|evidence| wanted.contains(&evidence.account_id()))
            .collect();
        let bytes = borsh::to_vec(message).expect("a message");
        let tx = TxMessage::new(self.actor(from.id), bytes, public, nonces, None, evidence);
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
        println!("ok   {what} (refused: {expected})");
    }

    fn record(&self, id: &NameId) -> Option<NameRecord> {
        let account = self.state.get_account_by_id(record_address(&self.program, id));
        let state = account.data.actor_state(self.program);
        (!state.is_empty()).then(|| borsh::from_slice(state).expect("a record"))
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

fn main() {
    let elf = names_methods::LOGOS_NAMES_ELF;
    let mut deployer = who(9);
    let admin = who(1);
    let mut alice = who(2);
    let mut bob = who(3);
    let carol = who(4);
    let program = deployer.id;

    let state = V03State::new()
        .with_public_account_balances([(alice.id, 10 * DEPOSIT), (bob.id, 10 * DEPOSIT), (admin.id, DEPOSIT)])
        .with_named_programs([(program, Program::new(elf.into()).expect("the guest loads"))]);
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
    let actors = run.registering(&jef, alice.id, &secret);
    run.send("alice registers with a deposit", &mut alice, register(jef, secret, later, length), actors, later);
    assert_eq!(run.record(&jef).expect("registered").owner, owner_tag(&jef, &alice.id));
    assert_eq!(run.balance(alice.id), before - DEPOSIT, "the deposit left alice in the same transaction");
    assert_eq!(run.balance(deposit_address(&program, &jef)), DEPOSIT, "and reached the name's account");
    println!("ok   the deposit moved atomically");

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
    run.send(
        "alice transfers the name to bob",
        &mut alice,
        Instruction::Transfer { id: jef, new_owner: owner_tag(&jef, &bob.id) },
        actors.clone(),
        later,
    );
    assert_eq!(run.record(&jef).expect("held").owner, owner_tag(&jef, &bob.id));

    let before = run.balance(bob.id);
    let mut actors = actors;
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
    println!("\nall scenarios passed");
}
