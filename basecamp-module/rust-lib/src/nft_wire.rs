//! The reference NFT program's wire format (LP-0001), and what the wallet reads from it.
//!
//! The program is Logos's own, `logos-blockchain/lez-programs` branch `refactor/lez-v0.4.x` at
//! dea23c2 (`programs/nft/core/src/lib.rs`, `artifacts/nft-idl.json`). The wallet does not
//! depend on that unmerged branch: this one file, with no dependency beyond `std`, encodes its
//! messages and decodes its states, and both the Names module (`mod nft_wire`) and the names CLI
//! (included by path, as `deposit.rs` is) compile it, so the two can never send different bytes.
//!
//! Borsh, as the program writes it: an enum is a tag byte (the variant's index), a string a u32
//! little-endian length and its UTF-8 bytes, a u128 16 bytes little-endian, a bool one byte 0 or
//! 1, an account ID its 32 bytes. The tests check every message against bytes cut out of real
//! transactions on Job A's throwaway chain (`~/logos/LP0001-JOB-A-FINDINGS.md` §8), and every
//! state against what that chain holds.
//!
//! Decoding is strict and never panics: unknown tags, trailing bytes, a bool other than 0 or 1
//! and text that isn't UTF-8 are refused, so a state the wallet can't read is shown as unreadable
//! rather than as something it isn't (LP-0001: "shown as such rather than hidden or silently
//! rendered as valid").

/// An account ID, its 32 bytes.
pub type Id = [u8; 32];

/// Longest NFT name, in bytes (`core/src/lib.rs:14`).
pub const MAX_NAME_LEN: usize = 64;
/// Longest metadata `uri`, in bytes (`core/src/lib.rs:16`).
pub const MAX_URI_LEN: usize = 256;
/// Longest metadata `creators`, in bytes (`core/src/lib.rs:18`).
pub const MAX_CREATORS_LEN: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standard {
    Simple,
    Expanded,
}

impl Standard {
    pub fn word(self) -> &'static str {
        match self {
            Standard::Simple => "Simple",
            Standard::Expanded => "Expanded",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    pub standard: Standard,
    pub uri: String,
    pub creators: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Definition {
    pub name: String,
    /// The master's print rights plus the owned copies; only burns lower it.
    pub printable_supply: u128,
    pub metadata: Metadata,
}

/// The program's state on one account. An empty state is "nothing here yet" and is not one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Definition(Definition),
    Master { definition: Id, print_balance: u128 },
    Copy { definition: Id, owned: bool },
}

/// What a credit carries: a master's whole print balance, or one copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Value {
    Master(u128),
    Copy,
}

/// The messages a transaction sends (IDL tags 0-5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    NewDefinition { name: String, printable_supply: u128, metadata: Metadata, master: Id },
    InitializeHolding { definition: Id },
    Transfer { to: Id },
    PrintNft { to: Id },
    Burn { amount: u128 },
    ReadState,
}

/// The program's messages between its own actors (`Internal`, not in the IDL; tags from
/// `core/src/lib.rs:134-160`). A pending credit to a private account is one of these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Internal {
    InitMaster { definition: Id, print_balance: u128 },
    RequestHolding,
    InitHolding { definition: Id },
    Receive { definition: Id, value: Value },
    ReceivePrint { definition: Id },
    DecreaseSupply { amount: u128 },
}

// ── Encoding ────────────────────────────────────────────────────────────

fn put_str(out: &mut Vec<u8>, text: &str) {
    out.extend_from_slice(&(text.len() as u32).to_le_bytes());
    out.extend_from_slice(text.as_bytes());
}

fn put_metadata(out: &mut Vec<u8>, metadata: &Metadata) {
    out.push(match metadata.standard {
        Standard::Simple => 0,
        Standard::Expanded => 1,
    });
    put_str(out, &metadata.uri);
    put_str(out, &metadata.creators);
}

impl Message {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        match self {
            Message::NewDefinition { name, printable_supply, metadata, master } => {
                out.push(0);
                put_str(&mut out, name);
                out.extend_from_slice(&printable_supply.to_le_bytes());
                put_metadata(&mut out, metadata);
                out.extend_from_slice(master);
            }
            Message::InitializeHolding { definition } => {
                out.push(1);
                out.extend_from_slice(definition);
            }
            Message::Transfer { to } => {
                out.push(2);
                out.extend_from_slice(to);
            }
            Message::PrintNft { to } => {
                out.push(3);
                out.extend_from_slice(to);
            }
            Message::Burn { amount } => {
                out.push(4);
                out.extend_from_slice(&amount.to_le_bytes());
            }
            Message::ReadState => out.push(5),
        }
        out
    }
}

impl State {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        match self {
            State::Definition(definition) => {
                out.push(0);
                put_str(&mut out, &definition.name);
                out.extend_from_slice(&definition.printable_supply.to_le_bytes());
                put_metadata(&mut out, &definition.metadata);
            }
            State::Master { definition, print_balance } => {
                out.push(1);
                out.extend_from_slice(definition);
                out.extend_from_slice(&print_balance.to_le_bytes());
            }
            State::Copy { definition, owned } => {
                out.push(2);
                out.extend_from_slice(definition);
                out.push(u8::from(*owned));
            }
        }
        out
    }
}

impl Internal {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        match self {
            Internal::InitMaster { definition, print_balance } => {
                out.push(0);
                out.extend_from_slice(definition);
                out.extend_from_slice(&print_balance.to_le_bytes());
            }
            Internal::RequestHolding => out.push(1),
            Internal::InitHolding { definition } => {
                out.push(2);
                out.extend_from_slice(definition);
            }
            Internal::Receive { definition, value } => {
                out.push(3);
                out.extend_from_slice(definition);
                match value {
                    Value::Master(balance) => {
                        out.push(0);
                        out.extend_from_slice(&balance.to_le_bytes());
                    }
                    Value::Copy => out.push(1),
                }
            }
            Internal::ReceivePrint { definition } => {
                out.push(4);
                out.extend_from_slice(definition);
            }
            Internal::DecreaseSupply { amount } => {
                out.push(5);
                out.extend_from_slice(&amount.to_le_bytes());
            }
        }
        out
    }
}

// ── Decoding ────────────────────────────────────────────────────────────

/// Why bytes are not what the program writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unreadable(pub &'static str);

struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], Unreadable> {
        if self.bytes.len() < n {
            return Err(Unreadable("it ends too early"));
        }
        let (head, rest) = self.bytes.split_at(n);
        self.bytes = rest;
        Ok(head)
    }
    fn byte(&mut self) -> Result<u8, Unreadable> {
        Ok(self.take(1)?[0])
    }
    fn id(&mut self) -> Result<Id, Unreadable> {
        let mut id = [0; 32];
        id.copy_from_slice(self.take(32)?);
        Ok(id)
    }
    fn u128(&mut self) -> Result<u128, Unreadable> {
        let mut le = [0; 16];
        le.copy_from_slice(self.take(16)?);
        Ok(u128::from_le_bytes(le))
    }
    fn bool(&mut self) -> Result<bool, Unreadable> {
        match self.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Unreadable("a yes/no field holds another value")),
        }
    }
    fn text(&mut self, max: usize) -> Result<String, Unreadable> {
        let mut le = [0; 4];
        le.copy_from_slice(self.take(4)?);
        let len = u32::from_le_bytes(le) as usize;
        if len > max {
            return Err(Unreadable("a text field is longer than the program allows"));
        }
        String::from_utf8(self.take(len)?.to_vec()).map_err(|_| Unreadable("a text field isn't UTF-8"))
    }
    fn metadata(&mut self) -> Result<Metadata, Unreadable> {
        let standard = match self.byte()? {
            0 => Standard::Simple,
            1 => Standard::Expanded,
            _ => return Err(Unreadable("an unknown metadata standard")),
        };
        Ok(Metadata { standard, uri: self.text(MAX_URI_LEN)?, creators: self.text(MAX_CREATORS_LEN)? })
    }
    fn done<T>(self, value: T) -> Result<T, Unreadable> {
        if self.bytes.is_empty() {
            Ok(value)
        } else {
            Err(Unreadable("it has bytes left over"))
        }
    }
}

impl State {
    /// The state on an account; `Ok(None)` for an empty one.
    pub fn decode(bytes: &[u8]) -> Result<Option<State>, Unreadable> {
        if bytes.is_empty() {
            return Ok(None);
        }
        let mut r = Reader { bytes };
        let state = match r.byte()? {
            0 => State::Definition(Definition {
                name: r.text(MAX_NAME_LEN)?,
                printable_supply: r.u128()?,
                metadata: r.metadata()?,
            }),
            1 => State::Master { definition: r.id()?, print_balance: r.u128()? },
            2 => State::Copy { definition: r.id()?, owned: r.bool()? },
            _ => return Err(Unreadable("not a state the NFT program writes")),
        };
        r.done(Some(state))
    }
}

impl Message {
    pub fn decode(bytes: &[u8]) -> Result<Message, Unreadable> {
        let mut r = Reader { bytes };
        let message = match r.byte()? {
            0 => Message::NewDefinition {
                name: r.text(MAX_NAME_LEN)?,
                printable_supply: r.u128()?,
                metadata: r.metadata()?,
                master: r.id()?,
            },
            1 => Message::InitializeHolding { definition: r.id()? },
            2 => Message::Transfer { to: r.id()? },
            3 => Message::PrintNft { to: r.id()? },
            4 => Message::Burn { amount: r.u128()? },
            5 => Message::ReadState,
            _ => return Err(Unreadable("not an NFT message")),
        };
        r.done(message)
    }
}

impl Internal {
    pub fn decode(bytes: &[u8]) -> Result<Internal, Unreadable> {
        let mut r = Reader { bytes };
        let message = match r.byte()? {
            0 => Internal::InitMaster { definition: r.id()?, print_balance: r.u128()? },
            1 => Internal::RequestHolding,
            2 => Internal::InitHolding { definition: r.id()? },
            3 => {
                let definition = r.id()?;
                let value = match r.byte()? {
                    0 => Value::Master(r.u128()?),
                    1 => Value::Copy,
                    _ => return Err(Unreadable("an unknown NFT value")),
                };
                Internal::Receive { definition, value }
            }
            4 => Internal::ReceivePrint { definition: r.id()? },
            5 => Internal::DecreaseSupply { amount: r.u128()? },
            _ => return Err(Unreadable("not an NFT program message")),
        };
        r.done(message)
    }
}

/// A pending message that credits an NFT: its collection and what arrives.
pub fn credit_of(message: &[u8]) -> Option<(Id, Value)> {
    match Internal::decode(message).ok()? {
        Internal::Receive { definition, value } => Some((definition, value)),
        Internal::ReceivePrint { definition } => Some((definition, Value::Copy)),
        _ => None,
    }
}

// ── What a state means to the wallet ────────────────────────────────────

/// What an account holds under the program, as the wallet lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Holding {
    /// Nothing yet: it can be made ready for any collection.
    Empty,
    /// The account is a collection's definition.
    Definition(Definition),
    /// The master of `definition`: it holds the NFT while `print_balance` > 0, and can print
    /// while it is above 1.
    Master { definition: Id, print_balance: u128 },
    /// A copy of `definition`, or a holding of it that holds nothing (`owned` false).
    Copy { definition: Id, owned: bool },
    /// Bytes the program would not write.
    Unreadable(&'static str),
}

pub fn holding(state: &[u8]) -> Holding {
    match State::decode(state) {
        Ok(None) => Holding::Empty,
        Ok(Some(State::Definition(definition))) => Holding::Definition(definition),
        Ok(Some(State::Master { definition, print_balance })) => Holding::Master { definition, print_balance },
        Ok(Some(State::Copy { definition, owned })) => Holding::Copy { definition, owned },
        Err(Unreadable(why)) => Holding::Unreadable(why),
    }
}

impl Holding {
    /// The collection this account is tied to, if any.
    pub fn definition(&self) -> Option<Id> {
        match self {
            Holding::Master { definition, .. } | Holding::Copy { definition, .. } => Some(*definition),
            _ => None,
        }
    }
    /// Whether it holds an NFT (a master with print rights, or an owned copy).
    pub fn holds(&self) -> bool {
        matches!(self, Holding::Master { print_balance, .. } if *print_balance > 0) || matches!(self, Holding::Copy { owned: true, .. })
    }
    /// `master` or `copy` for one that holds, as the wallet names the token.
    pub fn kind(&self) -> Option<&'static str> {
        match self {
            Holding::Master { print_balance, .. } if *print_balance > 0 => Some("master"),
            Holding::Copy { owned: true, .. } => Some("copy"),
            _ => None,
        }
    }
}

/// What the wallet must do so `state` can take an NFT of `definition`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Readiness {
    /// It can receive now.
    Ready,
    /// It needs `InitializeHolding` first (an empty account).
    NeedsHolding,
    /// It can't receive one: why, in plain words.
    Refused(&'static str),
}

pub const NOT_READY: &str = "That account isn't ready for this collection: it has to receive a holding first.";
pub const ALREADY_HOLDS: &str = "That account already holds an NFT of this collection.";
pub const OTHER_COLLECTION: &str = "That account is tied to another collection under this NFT program; use another account.";
pub const IS_DEFINITION: &str = "That account is a collection's definition; it can't hold an NFT under this program.";
pub const HOLDS_NOTHING: &str = "This account holds no NFT to send.";
pub const NO_PRINTS_LEFT: &str = "The master has no prints left.";
pub const NOT_MASTER: &str = "Only the master can print copies.";
pub const UNREADABLE: &str = "That account's NFT state can't be read, so the wallet won't use it.";

/// Whether `text` is one of the plain sentences above (safe to tell another app: it names no
/// account and no balance).
pub fn is_plain(text: &str) -> bool {
    [NOT_READY, ALREADY_HOLDS, OTHER_COLLECTION, IS_DEFINITION, HOLDS_NOTHING, NO_PRINTS_LEFT, NOT_MASTER, UNREADABLE].contains(&text)
}

pub fn readiness(state: &[u8], definition: &Id) -> Readiness {
    match holding(state) {
        Holding::Empty => Readiness::NeedsHolding,
        Holding::Definition(_) => Readiness::Refused(IS_DEFINITION),
        Holding::Unreadable(_) => Readiness::Refused(UNREADABLE),
        h if h.definition() != Some(*definition) => Readiness::Refused(OTHER_COLLECTION),
        h if h.holds() => Readiness::Refused(ALREADY_HOLDS),
        _ => Readiness::Ready,
    }
}

/// The amount a burn of everything `state` holds destroys: 1 for a copy, the whole print
/// balance for a master (the NFT with it).
pub fn burn_all(state: &[u8]) -> Result<u128, &'static str> {
    match holding(state) {
        Holding::Copy { owned: true, .. } => Ok(1),
        Holding::Master { print_balance, .. } if print_balance > 0 => Ok(print_balance),
        Holding::Unreadable(_) => Err(UNREADABLE),
        _ => Err(HOLDS_NOTHING),
    }
}

/// `state` can send what it holds.
pub fn can_send(state: &[u8]) -> Result<(Id, Value), &'static str> {
    match holding(state) {
        Holding::Copy { definition, owned: true } => Ok((definition, Value::Copy)),
        Holding::Master { definition, print_balance } if print_balance > 0 => Ok((definition, Value::Master(print_balance))),
        Holding::Unreadable(_) => Err(UNREADABLE),
        _ => Err(HOLDS_NOTHING),
    }
}

/// `state` can print a copy.
pub fn can_print(state: &[u8]) -> Result<Id, &'static str> {
    match holding(state) {
        Holding::Master { definition, print_balance } if print_balance > 1 => Ok(definition),
        Holding::Master { .. } => Err(NO_PRINTS_LEFT),
        Holding::Unreadable(_) => Err(UNREADABLE),
        _ => Err(NOT_MASTER),
    }
}

/// The plain sentence for a refusal the program or LEZ gave, found in its error text
/// (`Guest panicked: <message>` from the program, execution-rule errors from LEZ). `None` when
/// the text is none of the known ones.
pub fn refusal(error: &str) -> Option<&'static str> {
    const KNOWN: [(&str, &str); 12] = [
        ("recipient has no NFT holding", NOT_READY),
        ("which is neither a declared public actor nor private", NOT_READY),
        ("whose account is declared public but not admitted", NOT_READY),
        ("recipient already holds an NFT", ALREADY_HOLDS),
        ("recipient holds another NFT", OTHER_COLLECTION),
        ("account already holds another NFT state", OTHER_COLLECTION),
        ("recipient is an NFT definition", IS_DEFINITION),
        ("sender holds no NFT", HOLDS_NOTHING),
        ("holding holds no NFT", HOLDS_NOTHING),
        ("insufficient print balance", NO_PRINTS_LEFT),
        ("receiver is not an NFT master", NOT_MASTER),
        ("receiver is not an NFT holding", HOLDS_NOTHING),
    ];
    KNOWN.iter().find(|(said, _)| error.contains(said)).map(|(_, plain)| *plain)
}

// ── What a send shows the chain ─────────────────────────────────────────

/// Where an NFT goes, as far as privacy is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum To {
    Public,
    /// One of this wallet's private accounts: declared, checked, credited in the same transaction.
    OwnPrivate,
    /// Someone else's private account by its receiving keys: a pending credit.
    TheirPrivate,
}

/// What a chain observer can read from a transfer, and what it can't, from Job A's table
/// (§8b.3; development receipts, a search for known account IDs). Plain sentences for the sheet.
pub fn reveals(from_private: bool, to: To) -> (Vec<&'static str>, Vec<&'static str>) {
    match (from_private, to) {
        (false, To::Public) => (
            vec!["Both accounts, the collection and the NFT that moved are public."],
            vec![],
        ),
        (false, To::OwnPrivate) => (
            vec!["Your public account, the collection, and the receiving private account's ID are public."],
            vec!["What else the private account holds stays private."],
        ),
        (false, To::TheirPrivate) => (
            vec!["Expected (not yet observed): your public account and the collection are public."],
            vec!["Expected: who receives it is not named; the NFT waits for their wallet to take it."],
        ),
        (true, To::Public) => (
            vec!["The receiving account and the collection are public."],
            vec!["Your private account is not named."],
        ),
        (true, To::OwnPrivate) => (
            vec!["Only that the NFT program ran is public."],
            vec!["Neither account and not the collection are named: no link between sender and recipient."],
        ),
        (true, To::TheirPrivate) => (
            vec!["Expected (observed only between two accounts of one wallet): only that the NFT program ran is public."],
            vec!["Expected: neither account and not the collection are named."],
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b58(text: &str) -> Id {
        const ALPHA: &str = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
        let mut n = [0u32; 32];
        for c in text.chars() {
            let mut carry = ALPHA.find(c).expect("base58") as u32;
            for limb in n.iter_mut().rev() {
                let v = *limb * 58 + carry;
                *limb = v & 0xff;
                carry = v >> 8;
            }
            assert_eq!(carry, 0);
        }
        n.map(|b| b as u8)
    }

    fn hex(text: &str) -> Vec<u8> {
        (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap()).collect()
    }

    // Job A's throwaway chain (96a0a50, port 3310), program ErycMYqX…RmDNmu: account IDs from
    // run/public-flows-96a0a50.log, bytes from getTransaction / getAccountView (cut out by
    // tools/golden.py in /Volumes/exos162/logos/lp0001/wallet-nft/, read-only).
    const ART: &str = "HA47ERcmfSbDChGcYfBcC4SKaPQ8pJJnv4wxCLu1ejL5";
    const ALICE: &str = "7Gt5PNeZhNAGBLXggWM3J3krpgTg24Az1uwnbqu995GD";
    const ALICE2B: &str = "97j6hXueyU4C4CGjKTXDwVH699XLPrjuq8rqEfxYqVpm";
    const ART2B: &str = "73zq8SWWULTuAb5ZNJPLt1jjuDhr65cn5J7GmGfD1QMt";
    const BOB: &str = "HLEwXNBDYx6GS9H5CsawDrMTptD6R4XsJywFEMr6eim2";
    const CAROL: &str = "D4DPXvAqKTFnknHszfFb21f1b6iRT3eVMBKEQLMWg7XR";
    const DAVE: &str = "EGYoFWj7aTnQ3PHJxZ83g6Cxri4jwKh5gWBuyAkp1oPg";

    #[test]
    fn messages_match_the_bytes_of_job_a_transactions() {
        // new-definition --name Art --printable-supply 3 --metadata Simple … --master alice (tx 1e352685…, block 45)
        let new_art = Message::NewDefinition {
            name: "Art".into(),
            printable_supply: 3,
            metadata: Metadata { standard: Standard::Simple, uri: "ipfs://bafyart/meta".into(), creators: "me".into() },
            master: b58(ALICE),
        };
        assert_eq!(new_art.encode(), hex("0003000000417274030000000000000000000000000000000013000000697066733a2f2f626166796172742f6d657461020000006d655d3795c7a70a6d6cbae4a86bcabe0a408e99cfd153fc14c32caf4e79a820f042"));
        // the Expanded one, art2b, master alice2b
        let new_art2 = Message::NewDefinition {
            name: "Art2".into(),
            printable_supply: 1,
            metadata: Metadata { standard: Standard::Expanded, uri: "ipfs://bafyart2/meta".into(), creators: "me, you".into() },
            master: b58(ALICE2B),
        };
        assert_eq!(new_art2.encode(), hex("000400000041727432010000000000000000000000000000000114000000697066733a2f2f62616679617274322f6d657461070000006d652c20796f75789694b6d82b2ba98fdffe05a7cc6bfe799388658ada251e4562da4a41067892"));
        // initialize-holding --definition art (tx 756cc3f3…)
        assert_eq!(Message::InitializeHolding { definition: b58(ART) }.encode(), hex("01f00cbb50684466efb7410aa2097f6bcc3a07d0d2ad8acbe8d655373f619219c2"));
        // print-nft --to dave
        assert_eq!(Message::PrintNft { to: b58(DAVE) }.encode(), hex("03c523987b6cb9fb4148303be076a243fc5844553e468093b3462e360de4ffb1ab"));
        // transfer --to bob (from dave), transfer --to carol (the master, from alice)
        assert_eq!(Message::Transfer { to: b58(BOB) }.encode(), hex("02f2a8c9cc2517b3d14e481fba19a672e1137f4d4f3680624fd5e6b521aa1a48f5"));
        assert_eq!(Message::Transfer { to: b58(CAROL) }.encode(), hex("02b31efe46adca3c92fe629404ca2231a62cb2e195c399cc24c767c6120e180e64"));
        // burn --amount 1
        assert_eq!(Message::Burn { amount: 1 }.encode(), hex("0401000000000000000000000000000000"));
        // The IDL's tag 5, no fields.
        assert_eq!(Message::ReadState.encode(), vec![5]);
        // And each decodes back to itself.
        for message in [new_art, new_art2, Message::Burn { amount: u128::MAX }, Message::ReadState] {
            assert_eq!(Message::decode(&message.encode()), Ok(message));
        }
    }

    #[test]
    fn states_match_what_job_a_chain_holds() {
        let art = hex("0003000000417274010000000000000000000000000000000013000000697066733a2f2f626166796172742f6d657461020000006d65");
        // After the two burns, Art's supply is 1.
        assert_eq!(
            State::decode(&art),
            Ok(Some(State::Definition(Definition {
                name: "Art".into(),
                printable_supply: 1,
                metadata: Metadata { standard: Standard::Simple, uri: "ipfs://bafyart/meta".into(), creators: "me".into() },
            })))
        );
        let art2b = hex("000400000041727432010000000000000000000000000000000114000000697066733a2f2f62616679617274322f6d657461070000006d652c20796f75");
        assert!(matches!(State::decode(&art2b), Ok(Some(State::Definition(Definition { metadata: Metadata { standard: Standard::Expanded, .. }, .. })))));
        // alice: the master after giving its print rights to carol; carol holds them, minus one burnt.
        let alice = hex("01f00cbb50684466efb7410aa2097f6bcc3a07d0d2ad8acbe8d655373f619219c200000000000000000000000000000000");
        assert_eq!(State::decode(&alice), Ok(Some(State::Master { definition: b58(ART), print_balance: 0 })));
        let carol = hex("01f00cbb50684466efb7410aa2097f6bcc3a07d0d2ad8acbe8d655373f619219c201000000000000000000000000000000");
        assert_eq!(holding(&carol), Holding::Master { definition: b58(ART), print_balance: 1 });
        assert_eq!(holding(&hex("0159eaabe9dafd662fed33b2e1c7674c93ffaa083327a6c5e3bbcf09250be6d03701000000000000000000000000000000")).definition(), Some(b58(ART2B)));
        // bob and dave: holdings of Art that hold nothing now.
        let bob = hex("02f00cbb50684466efb7410aa2097f6bcc3a07d0d2ad8acbe8d655373f619219c200");
        assert_eq!(State::decode(&bob), Ok(Some(State::Copy { definition: b58(ART), owned: false })));
        // An owned copy: no account on the chain holds one publicly now; the private read of
        // §8c A2.1 decoded `02 <definition> 01`. Built from the IDL.
        let owned = State::Copy { definition: b58(ART), owned: true }.encode();
        assert_eq!(owned.len(), 34);
        assert_eq!((owned[0], owned[33]), (2, 1));
        // erin: never initialised, no state.
        assert_eq!(State::decode(&[]), Ok(None));
        // Every one encodes back to the same bytes.
        for bytes in [art, art2b, alice, carol, bob, owned] {
            assert_eq!(State::decode(&bytes).unwrap().unwrap().encode(), bytes);
        }
    }

    #[test]
    fn pending_credits_are_internal_messages_from_the_source() {
        // core/src/lib.rs:134-160: Receive is tag 3 then the definition and the value
        // (Master 0 + u128, Copy 1); ReceivePrint tag 4 then the definition.
        let copy = Internal::Receive { definition: b58(ART), value: Value::Copy }.encode();
        assert_eq!(copy.len(), 34);
        assert_eq!((copy[0], copy[33]), (3, 1));
        assert_eq!(credit_of(&copy), Some((b58(ART), Value::Copy)));
        let master = Internal::Receive { definition: b58(ART), value: Value::Master(7) }.encode();
        assert_eq!(master.len(), 50);
        assert_eq!(credit_of(&master), Some((b58(ART), Value::Master(7))));
        let print = Internal::ReceivePrint { definition: b58(ART) }.encode();
        assert_eq!(print.len(), 33);
        assert_eq!(credit_of(&print), Some((b58(ART), Value::Copy)));
        // Job A's forged-Receive probe (§8b.1): the 34-byte credit read as a user Message fails,
        // because tag 3 there is PrintNft with one account.
        assert!(Message::decode(&copy).is_err());
        // A native credit (17 bytes) is not an NFT credit.
        assert_eq!(credit_of(&[0; 17]), None);
        for message in [Internal::InitMaster { definition: [1; 32], print_balance: 9 }, Internal::RequestHolding, Internal::InitHolding { definition: [2; 32] }, Internal::DecreaseSupply { amount: 3 }] {
            assert_eq!(Internal::decode(&message.encode()), Ok(message));
        }
    }

    #[test]
    fn decoding_refuses_what_the_program_never_writes() {
        let bob = hex("02f00cbb50684466efb7410aa2097f6bcc3a07d0d2ad8acbe8d655373f619219c200");
        let mut longer = bob.clone();
        longer.push(0);
        assert_eq!(State::decode(&longer), Err(Unreadable("it has bytes left over")));
        assert_eq!(State::decode(&bob[..20]), Err(Unreadable("it ends too early")));
        let mut two = bob.clone();
        two[33] = 2;
        assert!(State::decode(&two).is_err());
        assert!(State::decode(&[9]).is_err());
        // A name past 64 bytes is refused before its bytes are read.
        let mut long_name = vec![0];
        long_name.extend_from_slice(&65u32.to_le_bytes());
        long_name.extend(vec![b'a'; 65]);
        assert!(State::decode(&long_name).is_err());
        // Not UTF-8.
        let bad = State::Definition(Definition { name: "ab".into(), printable_supply: 1, metadata: Metadata { standard: Standard::Simple, uri: String::new(), creators: String::new() } }).encode();
        let mut bad = bad;
        bad[5] = 0xff;
        assert_eq!(State::decode(&bad), Err(Unreadable("a text field isn't UTF-8")));
        assert!(matches!(holding(&[7, 7]), Holding::Unreadable(_)));
    }

    #[test]
    fn what_an_account_can_do_follows_the_program() {
        let art = b58(ART);
        let other = b58(BOB);
        let empty_copy = State::Copy { definition: art, owned: false }.encode();
        let owned = State::Copy { definition: art, owned: true }.encode();
        let master3 = State::Master { definition: art, print_balance: 3 }.encode();
        let master1 = State::Master { definition: art, print_balance: 1 }.encode();
        let master0 = State::Master { definition: art, print_balance: 0 }.encode();
        assert_eq!(readiness(&[], &art), Readiness::NeedsHolding);
        assert_eq!(readiness(&empty_copy, &art), Readiness::Ready);
        assert_eq!(readiness(&master0, &art), Readiness::Ready);
        assert_eq!(readiness(&owned, &art), Readiness::Refused(ALREADY_HOLDS));
        assert_eq!(readiness(&master1, &art), Readiness::Refused(ALREADY_HOLDS));
        assert_eq!(readiness(&empty_copy, &other), Readiness::Refused(OTHER_COLLECTION));
        assert_eq!(readiness(&[9, 9], &art), Readiness::Refused(UNREADABLE));
        assert_eq!(can_send(&owned), Ok((art, Value::Copy)));
        assert_eq!(can_send(&master3), Ok((art, Value::Master(3))));
        assert_eq!(can_send(&empty_copy), Err(HOLDS_NOTHING));
        assert_eq!(can_send(&master0), Err(HOLDS_NOTHING));
        assert_eq!(burn_all(&owned), Ok(1));
        assert_eq!(burn_all(&master3), Ok(3));
        assert_eq!(burn_all(&[]), Err(HOLDS_NOTHING));
        assert_eq!(can_print(&master3), Ok(art));
        assert_eq!(can_print(&master1), Err(NO_PRINTS_LEFT));
        assert_eq!(can_print(&owned), Err(NOT_MASTER));
        assert_eq!(holding(&owned).kind(), Some("copy"));
        assert_eq!(holding(&master1).kind(), Some("master"));
        assert_eq!(holding(&master0).kind(), None);
    }

    #[test]
    fn every_refusal_job_a_saw_has_a_plain_sentence() {
        // Exact texts from Job A's logs (§8, §8c).
        assert_eq!(refusal("Refused in simulation, so not sent: Failed to execute program: Guest panicked: sender holds no NFT"), Some(HOLDS_NOTHING));
        assert_eq!(refusal("Program violated execution rules: A delivery named Actor { account_id: x, program_account_id: y }, which is neither a declared public actor nor private"), Some(NOT_READY));
        assert_eq!(refusal("A delivery named Actor { account_id: ph1, program_account_id: nft }, whose account is declared public but not admitted"), Some(NOT_READY));
        assert_eq!(refusal("Guest panicked: receiver is not an NFT master"), Some(NOT_MASTER));
        // From the source (src/lib.rs:95-104, initialize_holding.rs:30, print.rs:24).
        assert_eq!(refusal("Guest panicked: recipient has no NFT holding; initialize it first"), Some(NOT_READY));
        assert_eq!(refusal("Guest panicked: recipient already holds an NFT"), Some(ALREADY_HOLDS));
        assert_eq!(refusal("Guest panicked: recipient holds another NFT"), Some(OTHER_COLLECTION));
        assert_eq!(refusal("Guest panicked: account already holds another NFT state"), Some(OTHER_COLLECTION));
        assert_eq!(refusal("Guest panicked: insufficient print balance"), Some(NO_PRINTS_LEFT));
        assert_eq!(refusal("connection refused"), None);
    }

    #[test]
    fn only_a_private_to_private_send_hides_both_sides() {
        let (named, hidden) = reveals(true, To::OwnPrivate);
        assert_eq!(named, vec!["Only that the NFT program ran is public."]);
        assert!(hidden[0].contains("no link"));
        // Someone else's private account was never observed: said as expected, not as fact.
        assert!(reveals(true, To::TheirPrivate).0[0].starts_with("Expected"));
        assert!(reveals(false, To::TheirPrivate).0[0].starts_with("Expected"));
        assert!(reveals(false, To::OwnPrivate).0[0].contains("receiving private account's ID"));
        assert!(reveals(true, To::Public).1[0].contains("not named"));
        assert!(reveals(false, To::Public).1.is_empty());
    }
}
