//! A minimal fungible token program for LEZ 0.4 actors.
//!
//! This is a prototype, not the reference token program: lez-programs has no LEZ 0.4 port
//! (see `docs/wallet/TOKENS.md`). It keeps the reference's semantics where the actor model
//! allows (a definition with a name, a supply and an optional mint authority; fungible
//! transfers; mint; burn) and copies the shape of LEZ's own native token
//! (`lee_core::native_token`): a holder's balances are its own actor state under this
//! program, a transfer debits the signer's actor and credits the recipient's with an async
//! message. That shape is what lets the stock `wallet` crate move the token publicly and
//! privately with the same code it uses for native transfers.
//!
//! State per account under this program (one actor state per account and program):
//! - empty, or [`TokenState::Holdings`]: balance per token definition;
//! - [`TokenState::Definition`]: a token definition. A definition account holds no balances.
//!
//! Messages a transaction sends (root, `from == None`), always to the signer's own actor:
//! [`Message::NewFungibleDefinition`] (to the new definition account), [`Message::Transfer`],
//! [`Message::Mint`] (to the mint authority), [`Message::Burn`], [`Message::SetAuthority`].
//! Messages only this program's own actors may send: [`Message::Credit`],
//! [`Message::MintFrom`], [`Message::BurnFrom`], [`Message::SetAuthorityFrom`].

use std::collections::BTreeMap;

use borsh::{BorshDeserialize, BorshSerialize};
use lee_core::{
    account::{AccountId, Actor, ActorState, Balance},
    program::{ReceiveInput, Response},
};

/// Longest token name, in bytes. Bounds the definition's state.
pub const MAX_NAME_LEN: usize = 64;

/// Most token kinds one account may hold. Bounds the holder's state, which a private account
/// carries in its note (LEZ pads notes to 512 bytes; about seven kinds fit inside the pad).
pub const MAX_HOLDINGS: usize = 32;

/// The wire format. Borsh, variants append-only: the tag byte is the variant index.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Message {
    /// Move `amount` of `definition` from the receiving actor (the signer's) to `to`.
    /// The credit is an async message, so a private recipient receives it later.
    Transfer {
        definition: AccountId,
        to: AccountId,
        amount: Balance,
    },
    /// Create a fungible token at the receiving actor, whose account must sign and hold
    /// nothing under this program yet. `total_supply` is credited to `supply_to`.
    /// `mint_authority`: `Some(id)` lets `id` mint more and rotate or renounce; `None` fixes
    /// the supply.
    NewFungibleDefinition {
        name: String,
        total_supply: Balance,
        mint_authority: Option<AccountId>,
        supply_to: AccountId,
    },
    /// Mint `amount` of `definition` to `to`. Sent to the authority's own actor, which must
    /// sign; it forwards [`Message::MintFrom`] to the definition. When the definition is its
    /// own authority, send it to the definition's actor.
    Mint {
        definition: AccountId,
        to: AccountId,
        amount: Balance,
    },
    /// Burn `amount` of `definition` from the receiving actor (the signer's); the definition's
    /// supply shrinks by the same amount.
    Burn {
        definition: AccountId,
        amount: Balance,
    },
    /// Rotate or renounce (`None`) the mint authority of `definition`. Sent like `Mint`.
    SetAuthority {
        definition: AccountId,
        new_authority: Option<AccountId>,
    },
    /// Add `amount` of `definition` to the receiver's holdings. Own program only.
    Credit {
        definition: AccountId,
        amount: Balance,
    },
    /// To a definition, from the actor of the account that sent `Mint` and signed it.
    MintFrom { to: AccountId, amount: Balance },
    /// To a definition, from the actor that burned.
    BurnFrom { amount: Balance },
    /// To a definition, from the actor of the account that sent `SetAuthority` and signed it.
    SetAuthorityFrom { new_authority: Option<AccountId> },
}

/// A token definition. Same fields as the reference's `TokenDefinition::Fungible`
/// (lez-programs `programs/token/core/src/lib.rs`), so a reader of one reads the other.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum TokenDefinition {
    Fungible {
        name: String,
        total_supply: Balance,
        metadata_id: Option<AccountId>,
        authority: Option<AccountId>,
    },
}

/// What one account holds under this program.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum TokenState {
    /// Balance per token definition. Zero balances are removed; no holdings is empty state.
    Holdings(BTreeMap<AccountId, Balance>),
    Definition(TokenDefinition),
}

impl TokenState {
    /// Decodes an actor state; empty is no holdings. `None` for bytes that are not a state.
    #[must_use]
    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.is_empty() {
            return Some(Self::Holdings(BTreeMap::new()));
        }
        borsh::from_slice(data).ok()
    }

    /// The canonical encoding: no holdings is empty state, so an account that spent all its
    /// tokens carries nothing under this program.
    #[must_use]
    pub fn encode(&self) -> ActorState {
        match self {
            Self::Holdings(holdings) if holdings.is_empty() => ActorState::empty(),
            _ => ActorState::from(borsh::to_vec(self).expect("token state serializes")),
        }
    }
}

/// The balance of `definition` in an account's actor state under this program.
#[must_use]
pub fn balance_of(data: &[u8], definition: &AccountId) -> Balance {
    match TokenState::decode(data) {
        Some(TokenState::Holdings(holdings)) => holdings.get(definition).copied().unwrap_or(0),
        _ => 0,
    }
}

/// The guest's entry point.
pub fn handle(input: &ReceiveInput) -> Response {
    let message: Message = borsh::from_slice(&input.message).expect("message must decode");
    let program = input.receiver.program_account_id;
    let me = input.receiver.account_id;
    match message {
        Message::Transfer {
            definition,
            to,
            amount,
        } => {
            require_root_signer(input, "transfer");
            assert!(amount > 0, "transfer amount must be positive");
            assert_ne!(to, me, "transfer to the sender itself");
            let holdings = debit(input, definition, amount);
            Response::set_state(TokenState::Holdings(holdings).encode())
                .send_async(Actor::new(to, program), &Message::Credit { definition, amount })
        }
        Message::NewFungibleDefinition {
            name,
            total_supply,
            mint_authority,
            supply_to,
        } => {
            require_root_signer(input, "a new definition");
            assert!(
                input.pre_state.is_empty(),
                "the definition account already holds state under this program"
            );
            assert!(!name.is_empty() && name.len() <= MAX_NAME_LEN, "token name length");
            assert_ne!(supply_to, me, "a definition account holds no balances");
            let definition = TokenDefinition::Fungible {
                name,
                total_supply,
                metadata_id: None,
                authority: mint_authority,
            };
            let response = Response::set_state(TokenState::Definition(definition).encode());
            if total_supply == 0 {
                return response;
            }
            response.send_async(
                Actor::new(supply_to, program),
                &Message::Credit {
                    definition: me,
                    amount: total_supply,
                },
            )
        }
        Message::Mint {
            definition,
            to,
            amount,
        } => {
            require_root_signer(input, "a mint");
            if definition == me {
                return mint(input, me, to, amount);
            }
            Response::keep_state().send_sync(
                Actor::new(definition, program),
                &Message::MintFrom { to, amount },
            )
        }
        Message::Burn { definition, amount } => {
            require_root_signer(input, "a burn");
            assert!(amount > 0, "burn amount must be positive");
            let holdings = debit(input, definition, amount);
            Response::set_state(TokenState::Holdings(holdings).encode())
                .send_sync(Actor::new(definition, program), &Message::BurnFrom { amount })
        }
        Message::SetAuthority {
            definition,
            new_authority,
        } => {
            require_root_signer(input, "an authority change");
            if definition == me {
                return set_authority(input, me, new_authority);
            }
            Response::keep_state().send_sync(
                Actor::new(definition, program),
                &Message::SetAuthorityFrom { new_authority },
            )
        }
        Message::Credit { definition, amount } => {
            assert!(
                input.from_own_program(),
                "a credit is only sent by this token program"
            );
            let Some(TokenState::Holdings(mut holdings)) = TokenState::decode(&input.pre_state)
            else {
                panic!("a token definition holds no balances");
            };
            let balance = holdings.entry(definition).or_insert(0);
            *balance = balance.checked_add(amount).expect("balance overflow");
            assert!(holdings.len() <= MAX_HOLDINGS, "the recipient holds too many token kinds");
            Response::set_state(TokenState::Holdings(holdings).encode())
        }
        Message::MintFrom { to, amount } => {
            let authority = own_sender(input, "a mint");
            mint(input, authority, to, amount)
        }
        Message::BurnFrom { amount } => {
            own_sender(input, "a burn");
            let (name, total_supply, authority) = definition_fields(input);
            let total_supply = total_supply
                .checked_sub(amount)
                .expect("burn exceeds the supply");
            Response::set_state(definition_state(name, total_supply, authority))
        }
        Message::SetAuthorityFrom { new_authority } => {
            let authority = own_sender(input, "an authority change");
            set_authority(input, authority, new_authority)
        }
    }
}

/// The transaction itself sent this to the receiver, and the receiver's account authorized it.
/// Root only: an actor reached from another program would be authorized by whatever its
/// signer signed for that program, not for this call.
fn require_root_signer(input: &ReceiveInput, what: &str) {
    assert!(input.from.is_none(), "{what} is sent by the transaction itself");
    assert!(input.is_authorized, "{what} must be signed by the receiving account");
}

/// The account whose actor of this program sent `input`; such an actor only forwards what its
/// own account signed.
fn own_sender(input: &ReceiveInput, what: &str) -> AccountId {
    assert!(
        input.from_own_program(),
        "{what} reaches a definition only from this token program"
    );
    input.from.expect("an own-program message has a sender").account_id
}

/// The receiver's holdings with `amount` of `definition` taken out.
fn debit(input: &ReceiveInput, definition: AccountId, amount: Balance) -> BTreeMap<AccountId, Balance> {
    let Some(TokenState::Holdings(mut holdings)) = TokenState::decode(&input.pre_state) else {
        panic!("a token definition holds no balances");
    };
    let balance = holdings.get(&definition).copied().unwrap_or(0);
    let left = balance.checked_sub(amount).expect("insufficient token balance");
    if left == 0 {
        holdings.remove(&definition);
    } else {
        holdings.insert(definition, left);
    }
    holdings
}

fn definition_fields(input: &ReceiveInput) -> (String, Balance, Option<AccountId>) {
    match TokenState::decode(&input.pre_state) {
        Some(TokenState::Definition(TokenDefinition::Fungible {
            name,
            total_supply,
            authority,
            ..
        })) => (name, total_supply, authority),
        _ => panic!("the receiver is not a token definition"),
    }
}

fn definition_state(name: String, total_supply: Balance, authority: Option<AccountId>) -> ActorState {
    TokenState::Definition(TokenDefinition::Fungible {
        name,
        total_supply,
        metadata_id: None,
        authority,
    })
    .encode()
}

/// Mints at the definition `input` reaches, for the account `by` that authorized it.
fn mint(input: &ReceiveInput, by: AccountId, to: AccountId, amount: Balance) -> Response {
    assert!(amount > 0, "mint amount must be positive");
    let (name, total_supply, authority) = definition_fields(input);
    assert_eq!(
        authority,
        Some(by),
        "only the mint authority may mint (none means a fixed supply)"
    );
    assert_ne!(to, input.receiver.account_id, "a definition account holds no balances");
    let total_supply = total_supply.checked_add(amount).expect("supply overflow");
    Response::set_state(definition_state(name, total_supply, authority)).send_async(
        Actor::new(to, input.receiver.program_account_id),
        &Message::Credit {
            definition: input.receiver.account_id,
            amount,
        },
    )
}

fn set_authority(input: &ReceiveInput, by: AccountId, new_authority: Option<AccountId>) -> Response {
    let (name, total_supply, authority) = definition_fields(input);
    assert_eq!(
        authority,
        Some(by),
        "only the mint authority may change it (none means it was renounced)"
    );
    Response::set_state(definition_state(name, total_supply, new_authority))
}

#[cfg(test)]
mod tests {
    use lee_core::program::{AsyncMessage, SyncMessage};

    use super::*;

    const PROGRAM: AccountId = AccountId::new([9; 32]);
    const ALICE: AccountId = AccountId::new([1; 32]);
    const BOB: AccountId = AccountId::new([2; 32]);
    const DEF: AccountId = AccountId::new([3; 32]);
    const OTHER_DEF: AccountId = AccountId::new([4; 32]);

    fn actor(account: AccountId) -> Actor {
        Actor::new(account, PROGRAM)
    }

    fn input(to: AccountId, from: Option<Actor>, authorized: bool, pre: ActorState, message: &Message) -> ReceiveInput {
        ReceiveInput {
            receiver: actor(to),
            from,
            is_authorized: authorized,
            pre_state: pre,
            message: borsh::to_vec(message).unwrap(),
        }
    }

    fn holdings(entries: &[(AccountId, Balance)]) -> ActorState {
        TokenState::Holdings(entries.iter().copied().collect()).encode()
    }

    fn definition(supply: Balance, authority: Option<AccountId>) -> ActorState {
        definition_state("Test".to_owned(), supply, authority)
    }

    #[test]
    fn a_transfer_debits_the_signer_and_credits_the_recipient_async() {
        let response = handle(&input(
            ALICE,
            None,
            true,
            holdings(&[(DEF, 100), (OTHER_DEF, 5)]),
            &Message::Transfer { definition: DEF, to: BOB, amount: 30 },
        ));
        assert_eq!(response.post_state, Some(holdings(&[(DEF, 70), (OTHER_DEF, 5)])));
        assert_eq!(
            response.async_messages,
            vec![AsyncMessage::new(actor(BOB), &Message::Credit { definition: DEF, amount: 30 })]
        );
    }

    #[test]
    fn spending_everything_leaves_empty_state() {
        let response = handle(&input(
            ALICE,
            None,
            true,
            holdings(&[(DEF, 30)]),
            &Message::Transfer { definition: DEF, to: BOB, amount: 30 },
        ));
        assert_eq!(response.post_state, Some(ActorState::empty()));
    }

    #[test]
    #[should_panic(expected = "must be signed")]
    fn an_unsigned_transfer_is_refused() {
        let _ = handle(&input(ALICE, None, false, holdings(&[(DEF, 100)]), &Message::Transfer {
            definition: DEF,
            to: BOB,
            amount: 1,
        }));
    }

    #[test]
    #[should_panic(expected = "sent by the transaction itself")]
    fn a_transfer_reached_from_another_program_is_refused() {
        let from = Some(Actor::new(BOB, AccountId::new([7; 32])));
        let _ = handle(&input(ALICE, from, true, holdings(&[(DEF, 100)]), &Message::Transfer {
            definition: DEF,
            to: BOB,
            amount: 1,
        }));
    }

    #[test]
    #[should_panic(expected = "insufficient token balance")]
    fn a_transfer_beyond_the_balance_is_refused() {
        let _ = handle(&input(ALICE, None, true, holdings(&[(DEF, 10)]), &Message::Transfer {
            definition: DEF,
            to: BOB,
            amount: 11,
        }));
    }

    #[test]
    fn a_credit_from_this_program_adds_a_holding() {
        let response = handle(&input(
            BOB,
            Some(actor(ALICE)),
            false,
            ActorState::empty(),
            &Message::Credit { definition: DEF, amount: 30 },
        ));
        assert_eq!(response.post_state, Some(holdings(&[(DEF, 30)])));
    }

    #[test]
    #[should_panic(expected = "only sent by this token program")]
    fn a_root_credit_is_refused() {
        let _ = handle(&input(BOB, None, true, ActorState::empty(), &Message::Credit {
            definition: DEF,
            amount: 30,
        }));
    }

    #[test]
    #[should_panic(expected = "only sent by this token program")]
    fn a_credit_from_another_program_is_refused() {
        let from = Some(Actor::new(ALICE, AccountId::new([7; 32])));
        let _ = handle(&input(BOB, from, false, ActorState::empty(), &Message::Credit {
            definition: DEF,
            amount: 30,
        }));
    }

    #[test]
    #[should_panic(expected = "holds no balances")]
    fn a_definition_takes_no_credit() {
        let _ = handle(&input(DEF, Some(actor(ALICE)), false, definition(1, None), &Message::Credit {
            definition: DEF,
            amount: 1,
        }));
    }

    #[test]
    fn a_new_definition_credits_its_supply() {
        let response = handle(&input(
            DEF,
            None,
            true,
            ActorState::empty(),
            &Message::NewFungibleDefinition {
                name: "Test".to_owned(),
                total_supply: 1_000,
                mint_authority: Some(ALICE),
                supply_to: ALICE,
            },
        ));
        assert_eq!(response.post_state, Some(definition(1_000, Some(ALICE))));
        assert_eq!(
            response.async_messages,
            vec![AsyncMessage::new(actor(ALICE), &Message::Credit { definition: DEF, amount: 1_000 })]
        );
    }

    #[test]
    #[should_panic(expected = "already holds state")]
    fn a_definition_is_created_once() {
        let _ = handle(&input(DEF, None, true, definition(1, None), &Message::NewFungibleDefinition {
            name: "Again".to_owned(),
            total_supply: 1,
            mint_authority: None,
            supply_to: ALICE,
        }));
    }

    #[test]
    fn the_authority_mints_through_its_own_actor() {
        let mint = Message::Mint { definition: DEF, to: BOB, amount: 5 };
        let forwarded = handle(&input(ALICE, None, true, ActorState::empty(), &mint));
        assert_eq!(forwarded.post_state, None);
        assert_eq!(
            forwarded.sync_messages,
            vec![SyncMessage::new(actor(DEF), &Message::MintFrom { to: BOB, amount: 5 })]
        );
        let minted = handle(&input(
            DEF,
            Some(actor(ALICE)),
            false,
            definition(100, Some(ALICE)),
            &Message::MintFrom { to: BOB, amount: 5 },
        ));
        assert_eq!(minted.post_state, Some(definition(105, Some(ALICE))));
        assert_eq!(
            minted.async_messages,
            vec![AsyncMessage::new(actor(BOB), &Message::Credit { definition: DEF, amount: 5 })]
        );
    }

    #[test]
    #[should_panic(expected = "only the mint authority may mint")]
    fn a_stranger_cannot_mint() {
        let _ = handle(&input(
            DEF,
            Some(actor(BOB)),
            false,
            definition(100, Some(ALICE)),
            &Message::MintFrom { to: BOB, amount: 5 },
        ));
    }

    #[test]
    #[should_panic(expected = "only the mint authority may mint")]
    fn a_fixed_supply_cannot_grow() {
        let _ = handle(&input(DEF, Some(actor(ALICE)), false, definition(100, None), &Message::MintFrom {
            to: BOB,
            amount: 5,
        }));
    }

    #[test]
    #[should_panic(expected = "only from this token program")]
    fn a_mint_from_another_program_is_refused() {
        let from = Some(Actor::new(ALICE, AccountId::new([7; 32])));
        let _ = handle(&input(DEF, from, false, definition(100, Some(ALICE)), &Message::MintFrom {
            to: BOB,
            amount: 5,
        }));
    }

    #[test]
    fn a_self_authority_mints_at_the_definition() {
        let response = handle(&input(DEF, None, true, definition(1, Some(DEF)), &Message::Mint {
            definition: DEF,
            to: BOB,
            amount: 2,
        }));
        assert_eq!(response.post_state, Some(definition(3, Some(DEF))));
    }

    #[test]
    fn a_burn_shrinks_the_holding_and_the_supply() {
        let response = handle(&input(ALICE, None, true, holdings(&[(DEF, 10)]), &Message::Burn {
            definition: DEF,
            amount: 4,
        }));
        assert_eq!(response.post_state, Some(holdings(&[(DEF, 6)])));
        assert_eq!(
            response.sync_messages,
            vec![SyncMessage::new(actor(DEF), &Message::BurnFrom { amount: 4 })]
        );
        let burned = handle(&input(DEF, Some(actor(ALICE)), false, definition(10, None), &Message::BurnFrom {
            amount: 4,
        }));
        assert_eq!(burned.post_state, Some(definition(6, None)));
    }

    #[test]
    fn the_authority_renounces() {
        let response = handle(&input(
            DEF,
            Some(actor(ALICE)),
            false,
            definition(10, Some(ALICE)),
            &Message::SetAuthorityFrom { new_authority: None },
        ));
        assert_eq!(response.post_state, Some(definition(10, None)));
    }

    #[test]
    fn states_round_trip_and_balances_read() {
        let state = holdings(&[(DEF, 7)]);
        assert_eq!(balance_of(&state, &DEF), 7);
        assert_eq!(balance_of(&state, &OTHER_DEF), 0);
        assert_eq!(balance_of(&[], &DEF), 0);
        assert_eq!(balance_of(&definition(1, None), &DEF), 0);
        assert_eq!(TokenState::decode(&[]), Some(TokenState::Holdings(BTreeMap::new())));
    }

    /// The tag bytes are wire format: clients and other programs encode them.
    #[test]
    fn wire_tags_are_stable() {
        let tag = |message: &Message| borsh::to_vec(message).unwrap()[0];
        assert_eq!(tag(&Message::Transfer { definition: DEF, to: BOB, amount: 1 }), 0);
        assert_eq!(
            tag(&Message::NewFungibleDefinition {
                name: String::new(),
                total_supply: 0,
                mint_authority: None,
                supply_to: BOB
            }),
            1
        );
        assert_eq!(tag(&Message::Mint { definition: DEF, to: BOB, amount: 1 }), 2);
        assert_eq!(tag(&Message::Credit { definition: DEF, amount: 1 }), 5);
    }
}
