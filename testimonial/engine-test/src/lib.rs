//! Engine-test harness: the real testimonial guest ELF on LEZ v0.4.0-rc1's real state machine
//! (`V03State`, `ValidatedStateDiff`), real signatures and the real timestamp check. No chain,
//! no network, no fees, and no admission evidence ever: the program admits its own PDAs.

use std::collections::BTreeMap;

use borsh::BorshSerialize;
use lee::{
    PrivateKey, PublicKey, PublicTransaction, V03State, ValidatedStateDiff,
    program::{DEFAULT_PUBLIC_CYCLE_BUDGET, Program},
    public_transaction::{Message as TxMessage, WitnessSet},
};
use lee_core::account::{AccountId, Actor, Nonce};
use testimonial_core::{Instruction, Listing, Message, actors_to_declare, walk};

/// 2027-01-15 08:00:00 UTC.
pub const T0: u64 = 1_800_000_000_000;

pub struct Key {
    pub key: PrivateKey,
    pub id: AccountId,
}

#[must_use]
pub fn key(seed: u8) -> Key {
    let key = PrivateKey::try_new([seed; 32]).expect("a valid key");
    let id = AccountId::from(&PublicKey::new_from_private_key(&key));
    Key { key, id }
}

#[must_use]
pub fn testimonial_elf() -> &'static [u8] {
    testimonial_methods::LOGOS_TESTIMONIAL_ELF
}

#[must_use]
pub fn forwarder_elf() -> &'static [u8] {
    testimonial_forwarder_methods::TESTIMONIAL_FORWARDER_ELF
}

#[must_use]
pub fn image_id_of(elf: &[u8]) -> [u32; 8] {
    Program::new(elf.to_vec().into()).expect("a guest").id()
}

#[must_use]
pub fn image_bytes(id: &[u32; 8]) -> [u8; 32] {
    let mut out = [0; 32];
    for (i, w) in id.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&w.to_le_bytes());
    }
    out
}

/// The forwarder guest's root message: one sync message to send.
#[derive(BorshSerialize)]
pub struct Send {
    pub to_account: [u8; 32],
    pub to_program: [u8; 32],
    pub data: Vec<u8>,
    pub pda_seeds: Vec<[u8; 32]>,
}

/// One applied transaction's metering and events.
#[derive(Debug, Clone)]
pub struct Applied {
    pub cycles: u64,
    pub tx_bytes: usize,
    pub events: Vec<lee_core::program::TransactionEvent>,
}

pub struct Engine {
    pub state: V03State,
    pub program: AccountId,
    pub forwarder: AccountId,
    pub block: u64,
}

impl Engine {
    /// The testimonial program at `program` and the forwarder at `forwarder`, as genesis
    /// programs.
    #[must_use]
    pub fn new(program: AccountId, forwarder: AccountId) -> Self {
        let state = V03State::new().with_named_programs([
            (program, Program::new(testimonial_elf().to_vec().into()).expect("the testimonial guest loads")),
            (forwarder, Program::new(forwarder_elf().to_vec().into()).expect("the forwarder loads")),
        ]);
        Self { state, program, forwarder, block: 0 }
    }

    /// Makes accounts exist (with a balance), as if funded earlier. An account a transaction
    /// declares but that neither signs nor exists nor is derived by a seed is refused as
    /// `UnadmittedPublicActor` before any program runs.
    pub fn fund(&mut self, accounts: &[AccountId]) {
        let state = std::mem::take(&mut self.state);
        self.state = state.with_public_account_balances(accounts.iter().map(|a| (*a, 1_000_000)));
    }

    fn nonce(&self, id: AccountId) -> Nonce {
        self.state.get_account_by_id_ref(id).map_or(Nonce(0), |a| a.nonce)
    }

    /// Builds, signs and applies one public transaction in a new block (or the same block)
    /// whose timestamp is `block_time`. On an error the state is unchanged.
    pub fn try_tx(
        &mut self,
        root: Actor,
        message: Vec<u8>,
        actors: Vec<Actor>,
        signers: &[&Key],
        same_block: bool,
        block_time: u64,
    ) -> Result<Applied, String> {
        let mut declared: Vec<Actor> = Vec::new();
        for a in std::iter::once(root).chain(actors) {
            if !declared.contains(&a) {
                declared.push(a);
            }
        }
        let nonces: BTreeMap<AccountId, Nonce> = signers.iter().map(|k| (k.id, self.nonce(k.id))).collect();
        // No admission evidence, ever.
        let tx = TxMessage::new(root, message, declared, nonces, None, Vec::new());
        let keys: Vec<&PrivateKey> = signers.iter().map(|k| &k.key).collect();
        let witness = WitnessSet::for_message(&tx, &keys);
        let tx = PublicTransaction::new(tx, witness);
        let tx_bytes = borsh::to_vec(&tx).expect("borsh").len();
        if !same_block || self.block == 0 {
            self.block += 1;
        }
        let (diff, charge) = ValidatedStateDiff::from_public_transaction_with_cycle_budget(
            &tx,
            &self.state,
            self.block,
            block_time,
            DEFAULT_PUBLIC_CYCLE_BUDGET,
        )
        .map_err(|e| format!("{e:?} / {e}"))?;
        let events = self.state.apply_state_diff(diff).map_err(|e| format!("{e:?}"))?;
        Ok(Applied { cycles: charge.cycles, tx_bytes, events })
    }

    /// A `Post` from `author`, rooted at its own actor and declaring exactly
    /// `actors_to_declare`.
    pub fn try_post(
        &mut self,
        author: &Key,
        submission: &str,
        username: Option<&str>,
        text: &str,
        now: u64,
        block_time: u64,
        same_block: bool,
    ) -> Result<Applied, String> {
        let instruction = Instruction::Post {
            submission: submission.into(),
            username: username.map(Into::into),
            text: text.into(),
            now,
        };
        let message = borsh::to_vec(&Message::Instruction(instruction)).expect("borsh");
        let actors = actors_to_declare(&self.program, &author.id, submission);
        self.try_tx(Actor::new(author.id, self.program), message, actors, &[author], same_block, block_time)
    }

    /// This program's actor state on `account` (empty if none).
    #[must_use]
    pub fn read(&self, account: &AccountId) -> Vec<u8> {
        self.state
            .get_account_by_id_ref(*account)
            .map(|a| a.data.actor_state(self.program).to_vec())
            .unwrap_or_default()
    }

    /// A submission's list as the client reads it, checked.
    pub fn listing(&self, submission: &str) -> Result<Listing, String> {
        walk(&self.program, submission, |a| Ok::<_, String>(self.read(a)))
    }
}
