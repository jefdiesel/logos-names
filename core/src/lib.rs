//! Logos Names: a private name registry for the Logos Execution Zone (LEZ v0.4).
//!
//! A name such as `jef.lez` resolves to where it can be paid, some free-form
//! entries and an optional page. The registry is built so that knowing a name
//! is the only way to find it:
//!
//! - The chain never sees a name. Clients derive a 32-byte identifier from the
//!   name and every instruction, account address and event uses only that.
//! - What a name resolves to is stored sealed: encrypted by the client under a
//!   key that is also derived from the name. This program stores the bytes and
//!   cannot read them.
//! - There is no list of names. Nothing links one record to another.
//!
//! - No holder is named. A record keeps a tag of its holder's account bound to
//!   that one name ([`owner_tag`]), and no effect or event carries an account.
//!   A holder acting from a LEZ private account, through a privacy-preserving
//!   transaction, never appears on chain; one acting from a public account is
//!   seen signing, but its names stay unlinked to each other.
//!
//! What stays public is that some identifier is registered, until when, and
//! the fee paid into the treasury.
//!
//! Each name owns this program's actor on two accounts, both derived from its
//! identifier: the record ([`record_address`]) and the page ([`page_address`]).
//! A reverse record, the one thing a holder may choose to publish in the
//! clear, lives on the holder's own account.
//!
//! A name is held for a term, paid for in the native token to the registry's
//! treasury, and lapses unless renewed. The prices and the treasury are the
//! registry's settings ([`settings_address`]).
//!
//! The registry keeps no history. Releasing a name empties its record and page;
//! transferring a name empties everything its last holder sealed.
//!
//! A name is claimed in two steps, commit then reveal ([`Instruction::Commit`]
//! or [`Instruction::CommitPublic`], then [`Instruction::Register`]), so that watching pending transactions doesn't let
//! anyone register an identifier before the person who knows its name: the
//! commitment hides the identifier, and the reveal is bound to the committed
//! holder and only valid a while after the commitment.
//!
//! A transaction is one [`Message::Instruction`] to the signer's own actor of
//! this program. [`instruct`] sees no other actor's state, so it derives every
//! address itself and sends each check or change as a [`Message::Effect`] to the
//! actor it concerns, where [`apply`] sees that actor's state. Only this program's
//! own actors may send an effect, and any failure undoes the whole transaction.
//! [`apply`] sees no clock either: the time is the sender's claim, which
//! [`instruct`] binds to the block's timestamp and passes on inside the effects.

pub mod shortlist;
use borsh::{BorshDeserialize, BorshSerialize};
use lee_core::{
    account::{AccountId, Actor},
    native_token,
    program::{PdaSeed, ProgramEvent, ReceiveInput, Response, SyncMessage},
};

/// Record layout version written by this program. Version 3 holds an owner
/// tag where version 2 held the owner's account; version 4 records are laid out
/// as version 3's, and are only written after a commitment ([`Instruction::Commit`]).
/// Version 5 records are laid out as version 3's; a name registered in deposit
/// mode never lapses (`expires` is `u64::MAX`).
/// Version 6 records add [`NameRecord::claimed`] and run on LEZ 0.4.
/// Version 7 adds length-tier × term pricing (including permanent), parent
/// `generation` tracking, subname sales (`subname_sale`), and one-level
/// subnames ([`SubnameRecord`]).
pub const VERSION: u8 = 7;
/// Maximum byte length of a single name label (top-level name, or either side
/// of a subname `label.parent`).
pub const MAX_NAME_LEN: usize = 32;
/// Maximum byte length of a full subname `label.parent`. Chosen as 62 so that
/// the gateway's single-label encoding `label--parent` is at most 63 bytes,
/// the RFC 1035 DNS label limit.
pub const MAX_SUBNAME_LEN: usize = 62;
/// How names are written for people: bare, `jef` (SPEC 2). Kept as a constant
/// so every message that shows a name shows it the same way.
pub const NAME_SUFFIX: &str = "";
/// The suffix names used to be written with. Typed or pasted, it is still
/// accepted and removed, as are a leading `@` and an `lns:` / `lns://` prefix.
pub const LEGACY_SUFFIX: &str = ".lez";
/// Room for a sealed profile with private receiving keys and sixteen entries.
pub const MAX_SEALED_RECORD_LEN: usize = 12 * 1024;
/// LEZ caps actor data at 100 KiB, but a page that large does not fit one
/// transaction under the wallet's default gas limit: 80 KiB applies, 96 KiB fails.
pub const MAX_SEALED_PAGE_LEN: usize = 80 * 1024;

const NAME_SEED_DOMAIN: &[u8] = b"logos-names/v5/name/";
const PAGE_SEED_DOMAIN: &[u8] = b"logos-names/v5/page/";
const SETTINGS_SEED_DOMAIN: &[u8] = b"logos-names/v5/config";
const OWNER_TAG_DOMAIN: &[u8] = b"logos-names/v5/owner";
const COMMITMENT_DOMAIN: &[u8] = b"logos-names/v5/commitment";
const COMMITMENT_SEED_DOMAIN: &[u8] = b"logos-names/v5/commit/";
const DEPOSIT_SEED_DOMAIN: &[u8] = b"logos-names/v5/deposit/";
const CLAIM_LEAF_DOMAIN: &[u8] = b"logos-names/v6/claim-leaf";
const CLAIM_NODE_DOMAIN: &[u8] = b"logos-names/v6/claim-node";
const TIER_ID_DOMAIN: &[u8] = b"logos-names/v7/tier/";
const SUBNAME_SEED_DOMAIN: &[u8] = b"logos-names/v7/subname/";
const SUBPAGE_SEED_DOMAIN: &[u8] = b"logos-names/v7/subpage/";
const SUBNAME_COMMITMENT_DOMAIN: &[u8] = b"logos-names/v7/sub-commitment";
const GEN_REGISTER_DOMAIN: &[u8] = b"logos-names/v7/gen/register";
const GEN_CLAIM_DOMAIN: &[u8] = b"logos-names/v7/gen/claim";
const GEN_TRANSFER_DOMAIN: &[u8] = b"logos-names/v7/gen/transfer";

/// The registry charges a fee per term, paid to its treasury; names lapse
/// (unless registered or renewed with [`TERM_PERMANENT`]).
pub const MODE_FEE: u8 = 0;
/// The registry takes a deposit, held in the name's deposit account until the
/// name is released; names never lapse.
pub const MODE_DEPOSIT: u8 = 1;

/// How far a private commit, a private reveal or a claim may be from the time it
/// claims: it is valid only in a block whose timestamp is in
/// `[now, now + TIME_WINDOW_MS)`. So a claimed time is at most this much earlier
/// than the real one. Long enough for a private holder's proof (minutes on a
/// laptop) to be made and included.
pub const TIME_WINDOW_MS: u64 = 10 * 60 * 1000;
/// How long a private commitment must wait before its reveal. With a claimed
/// time off by at most [`TIME_WINDOW_MS`], at least
/// `MIN_COMMIT_AGE_MS - TIME_WINDOW_MS` (five minutes) really passes between a
/// commitment and its reveal, so a reveal seen pending can't be beaten by a
/// commitment made after seeing it.
pub const MIN_COMMIT_AGE_MS: u64 = 15 * 60 * 1000;
/// How far a public commit ([`Instruction::CommitPublic`]) or the reveal of a
/// public commitment may be from the time it claims: it is valid only in a block
/// whose timestamp is in `[now, now + PUBLIC_TIME_WINDOW_MS)`. A public
/// transaction has no slow proof to wait for, so it only needs room to reach the
/// next block; a private holder, whose proof takes minutes, normally uses
/// [`Instruction::Commit`] instead.
pub const PUBLIC_TIME_WINDOW_MS: u64 = 60 * 1000;
/// How long a public commitment ([`Instruction::CommitPublic`]) must wait before
/// its reveal. A public commit can claim a time up to one minute old
/// ([`PUBLIC_TIME_WINDOW_MS`]), so a five-minute wait means at least four real
/// minutes between a commitment and its reveal.
pub const PUBLIC_MIN_COMMIT_AGE_MS: u64 = 5 * 60 * 1000;
/// How long a commitment can still be revealed.
pub const MAX_COMMIT_AGE_MS: u64 = 24 * 60 * 60 * 1000;

const DAY_MS: u64 = 24 * 60 * 60 * 1000;
/// How long each finite term holds a name, in milliseconds as LEZ block
/// timestamps are: 30 days, one year, five years. Term index 3 is
/// [`TERM_PERMANENT`] (`expires = u64::MAX`).
pub const TERM_LENGTHS_MS: [u64; 3] = [30 * DAY_MS, 365 * DAY_MS, 5 * 365 * DAY_MS];
/// Number of pricing terms: 30 days, 1 year, 5 years, permanent.
pub const TERMS: usize = 4;
pub const TERM_30D: u8 = 0;
pub const TERM_1Y: u8 = 1;
pub const TERM_5Y: u8 = 2;
pub const TERM_PERMANENT: u8 = 3;
/// Number of length-based pricing tiers: 5 chars (and admin-placed 1–4), 6–7 chars, 8+ chars.
pub const PRICE_TIERS: usize = 3;
pub const PRICE_TIER_5: u8 = 0;
pub const PRICE_TIER_6_7: u8 = 1;
pub const PRICE_TIER_8_PLUS: u8 = 2;
/// Starting placeholder price table `[price_tier][term]` in native token base units:
/// 1y = 20M / 5M / 1M (5 / 6–7 / 8+ chars); 30d = 1/10×, 5y = 4×, permanent = 20×.
/// Admin-changeable on chain via [`Instruction::Configure`] or [`Instruction::SetPrice`].
pub const DEFAULT_PRICES: [[u128; TERMS]; PRICE_TIERS] = [
    [2_000_000, 20_000_000, 80_000_000, 400_000_000],
    [500_000, 5_000_000, 20_000_000, 100_000_000],
    [100_000, 1_000_000, 4_000_000, 20_000_000],
];
/// How long after its expiry a name can still be renewed, and by nobody else
/// registered.
pub const GRACE_MS: u64 = 30 * DAY_MS;

/// What the chain knows a name by: 32 bytes a client derives from the name.
/// The derivation is deliberately slow and is not this program's business.
pub type NameId = [u8; 32];

/// What a record keeps in place of its holder's account: a tag binding the
/// account to this one name ([`owner_tag`]). Holding several names, or
/// changing one, publishes nothing that links them, and a holder acting from
/// a private account never appears on chain at all.
pub type OwnerTag = [u8; 32];

/// Subname sales settings stored on a parent name's record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SubnameSale {
    /// Price in native token base units to buy any free subname of this parent.
    /// `0` allows anyone to claim free permanent subnames (paying only tx gas).
    pub price: u128,
    /// Public account receiving the native-token payment in the same transaction.
    /// Ignored when `price == 0`.
    pub payout: AccountId,
}

/// A registered top-level name. `sealed` is opaque here: only someone who knows
/// the name can open it.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NameRecord {
    pub version: u8,
    /// The tag of the account that may change, transfer or release the name.
    pub owner: OwnerTag,
    /// When the name lapses, in milliseconds since the Unix epoch (`u64::MAX`
    /// for permanent or deposit-held names).
    pub expires: u64,
    pub sealed: Vec<u8>,
    /// Carried over from an earlier chain by [`Instruction::Claim`]. It holds
    /// the name against a second claim, including after a transfer.
    pub claimed: bool,
    /// Length-based pricing tier (`PRICE_TIER_5`, `PRICE_TIER_6_7`, or
    /// `PRICE_TIER_8_PLUS`), bound into this record's address via [`tiered_id`].
    pub price_tier: u8,
    /// Unique generation token for this hold. Advances on every registration,
    /// claim, and transfer; revocable subnames bind to this value so they
    /// follow the parent without enumerating subnames on chain.
    pub generation: [u8; 32],
    /// Public subname sale settings for this parent; `None` when sales are off.
    /// Automatically cleared when the parent is transferred or released.
    pub subname_sale: Option<SubnameSale>,
}

/// Whether a subname is permanent or revocable by its parent's holder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum SubnameKind {
    /// Independent of the parent once created or bought; never expires and
    /// cannot be revoked.
    Permanent,
    /// Controlled by the parent's holder and bound to `parent_generation`.
    /// Detaches automatically if the parent is transferred, released, or lapses
    /// past its grace period.
    Revocable { parent_generation: [u8; 32] },
}

/// A registered one-level subname (`label.parent`), stored at
/// [`subname_record_address`].
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SubnameRecord {
    pub version: u8,
    /// The tag of the account that may change, transfer or release this subname
    /// (`owner_tag(&sub_id, holder)`).
    pub owner: OwnerTag,
    /// Permanent or revocable (carrying the parent generation it was issued under).
    pub kind: SubnameKind,
    /// Sealed profile encrypted under the subname's key.
    pub sealed: Vec<u8>,
}

/// A commitment to claim some name: when it was made and which clock-window path
/// it was made under. The name, its identifier and the holder are hidden in the
/// commitment's address.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Commitment {
    pub version: u8,
    /// The committer's claimed time, at most [`PUBLIC_TIME_WINDOW_MS`] early if
    /// `public`, or [`TIME_WINDOW_MS`] early otherwise.
    pub made: u64,
    /// True when made by [`Instruction::CommitPublic`] under the shorter
    /// [`PUBLIC_TIME_WINDOW_MS`] window; false when made by [`Instruction::Commit`]
    /// under [`TIME_WINDOW_MS`]. Bound at commit time and checked at reveal time,
    /// so a commitment only counts for the path it was made for.
    pub public: bool,
}

/// A name's page, sealed like its record.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Page {
    pub version: u8,
    pub sealed: Vec<u8>,
}

/// The reverse record on an account: the name it chooses to be shown as. This
/// is the one place a name appears in the clear, and only if its holder asks.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PrimaryName {
    pub version: u8,
    pub name: String,
}

/// The registry's settings. While the settings account is empty the registry
/// charges nothing.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Settings {
    pub version: u8,
    /// The account that may change the settings.
    pub admin: AccountId,
    /// The account fees are paid to. It pays nothing itself.
    pub treasury: AccountId,
    /// Price table `[price_tier][term]` in the native token's base units, used
    /// in [`MODE_FEE`]. Rows: `0` = 5 chars (and admin-placed 1–4), `1` = 6–7
    /// chars, `2` = 8+ chars. Columns: `0` = 30d, `1` = 1y, `2` = 5y, `3` = permanent.
    pub prices: [[u128; TERMS]; PRICE_TIERS],
    /// [`MODE_FEE`] or [`MODE_DEPOSIT`].
    pub mode: u8,
    /// The deposit for a normal name and for a three-character name, in
    /// [`MODE_DEPOSIT`]. A change applies to new registrations only: each name's
    /// deposit stays in its own account.
    pub deposits: [u128; 2],
    /// The root of the names carried over from an earlier chain
    /// ([`claim_root`]); all zeros while there are none.
    pub claims_root: [u8; 32],
    /// Claims are accepted before this time, in milliseconds since the Unix epoch.
    pub claims_until: u64,
    /// The registry on the earlier chain that the claims list was taken from.
    pub claims_program: AccountId,
}

/// What this program receives. A transaction sends an [`Instruction`] to the
/// signer's own actor; everything else is this program talking to itself.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Message {
    Instruction(Instruction),
    Effect(Effect),
    /// To a record actor, from anyone: refused unless `tag` holds the name. It
    /// changes nothing and tells nothing the record does not already show, and it
    /// is how another program, such as the board, checks who holds a name: an
    /// actor sees only its own state, so it has to ask.
    RequireHolder { tag: OwnerTag },
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Instruction {
    /// Reveal a commitment and claim a top-level name that is unregistered, or
    /// lapsed past its grace period, for the payer, with its first sealed profile
    /// (which may be empty). `id` is the name's base identifier (checked against
    /// `SHORTLIST_ROOT`); the record, page, deposit and commitment live under
    /// `slot_id = tiered_id(&id, price_tier)` so a lower pricing tier can never
    /// occupy or spoof a higher-tier name's slot. The commitment must be
    /// [`commitment(slot_id, owner_tag(slot_id, payer), secret)`](commitment),
    /// made at least [`PUBLIC_MIN_COMMIT_AGE_MS`] (for [`Instruction::CommitPublic`])
    /// or [`MIN_COMMIT_AGE_MS`] (for [`Instruction::Commit`]) and less than
    /// [`MAX_COMMIT_AGE_MS`] before `now`; revealing uses it up. `now` is the
    /// sender's claim of the time; the transaction is only valid in a block
    /// whose timestamp is in `[now, now + PUBLIC_TIME_WINDOW_MS)` for a public
    /// commitment or `[now, now + TIME_WINDOW_MS)` for a private one. With
    /// `deposit` false (fee mode), `fee` is `settings.prices[price_tier][term]`,
    /// paid to the treasury, and `term == TERM_PERMANENT` sets `expires = u64::MAX`;
    /// with `deposit` true (deposit mode), `fee` is the settings' deposit, paid
    /// into the name's deposit account, and `term` is ignored. The signer pays.
    Register {
        id: NameId,
        sealed: Vec<u8>,
        term: u8,
        fee: u128,
        treasury: Option<AccountId>,
        now: u64,
        secret: [u8; 32],
        deposit: bool,
        /// Where the name's base identifier sits relative to the short-name list,
        /// which locks 1–4 character and reserved names ([`shortlist`]).
        length: LengthProof,
        /// Length-based pricing tier (`PRICE_TIER_5`, `PRICE_TIER_6_7`, or
        /// `PRICE_TIER_8_PLUS`), bound into the record's slot via [`tiered_id`].
        price_tier: u8,
    },
    /// Extend a top-level name (`id` = `slot_id`) that is active or in grace by
    /// `term` (or upgrade to permanent with [`TERM_PERMANENT`]); anyone may pay.
    /// Uses the `price_tier` stored on the name's record.
    Renew {
        id: NameId,
        term: u8,
        fee: u128,
        treasury: Option<AccountId>,
        now: u64,
    },
    /// Replace the top-level name's sealed profile (`id` = `slot_id`).
    SetRecord { id: NameId, sealed: Vec<u8> },
    /// Write the top-level name's sealed page (`id` = `slot_id`); empty removes it.
    SetPage { id: NameId, sealed: Vec<u8> },
    /// Hand the top-level name (`id` = `slot_id`) to a new owner. Everything
    /// the last owner sealed is emptied, profile and page, subname sales are
    /// switched off, and the parent's `generation` advances so revocable
    /// subnames follow the parent.
    Transfer { id: NameId, new_owner: OwnerTag },
    /// Give the top-level name (`id` = `slot_id`) up. Its record and page are
    /// emptied, releasing any revocable subnames; `refund` moves from the
    /// name's deposit account to the owner (0 for a name with no deposit).
    Release { id: NameId, refund: u128 },
    /// Publish `name` (top-level or `label.parent` subname) as what the signer's
    /// account is shown as; an empty name removes it. Unlike everything else
    /// here, this puts a name on the chain in the clear.
    SetPrimary { name: String },
    /// Set the registry's settings. The first settings must be signed by the
    /// program's own account, whose key only its deployer holds; afterwards
    /// only by their admin.
    Configure {
        admin: AccountId,
        treasury: AccountId,
        prices: [[u128; TERMS]; PRICE_TIERS],
        mode: u8,
        deposits: [u128; 2],
        claims_root: [u8; 32],
        claims_until: u64,
        claims_program: AccountId,
    },
    /// Commit to claiming a name (or buying a subname) on the private path: the
    /// first step of a private [`Instruction::Register`] or [`Instruction::BuySubname`].
    /// Valid only in a block whose timestamp is in `[now, now + TIME_WINDOW_MS)`,
    /// and revealable after [`MIN_COMMIT_AGE_MS`]. Anyone may commit; whoever
    /// signs pays.
    Commit { commitment: [u8; 32], now: u64 },
    /// Take back a name held on an earlier chain, with no fee or deposit: the
    /// signer must be the account that held `id` (base identifier) there, shown
    /// by `proof` against the settings' `claims_root`. Written into the
    /// `tiered_id(&id, price_tier)` slot. Valid only in a block whose timestamp
    /// is in `[now, now + TIME_WINDOW_MS)`.
    Claim {
        id: NameId,
        sealed: Vec<u8>,
        now: u64,
        proof: ClaimProof,
        price_tier: u8,
    },
    /// Commit to claiming a name (or buying a subname) on the public (fast) path:
    /// valid only in a block whose timestamp is in `[now, now + PUBLIC_TIME_WINDOW_MS)`
    /// and revealable after [`PUBLIC_MIN_COMMIT_AGE_MS`].
    CommitPublic { commitment: [u8; 32], now: u64 },
    /// Update one cell of the fee-mode price table (`settings.prices[price_tier][term]`).
    /// Admin-only; applies to new purchases and renewals only.
    SetPrice {
        price_tier: u8,
        term: u8,
        price: u128,
    },
    /// Enable or disable public subname sales on a parent name (`id` = parent's
    /// `slot_id`). Only the parent's holder may call this. Passing `None` turns
    /// sales off.
    SetSubnameSale {
        id: NameId,
        sale: Option<SubnameSale>,
    },
    /// Create a one-level subname (`label.parent`) by the parent's holder (public
    /// or private). Costs only the transaction fee; no commit-reveal needed.
    /// `parent_id` is the parent's `slot_id`; `sub_id` is derived from
    /// `"label.parent"`; `owner` is `owner_tag(&sub_id, holder)`. Refused if the
    /// subname is already permanent or actively held in the parent's current
    /// generation; a stale revocable subname from an earlier generation of the
    /// parent is overwritten and its old page cleared.
    CreateSubname {
        parent_id: NameId,
        sub_id: NameId,
        owner: OwnerTag,
        revocable: bool,
        sealed: Vec<u8>,
        now: u64,
    },
    /// Revoke a revocable subname (`label.parent`) by the parent's holder (public
    /// or private), emptying its record and page. Refused for permanent subnames.
    RevokeSubname {
        parent_id: NameId,
        sub_id: NameId,
        now: u64,
    },
    /// Buy a free subname (`label.parent`) from a parent with subname sales
    /// enabled, paying `price` to `payout` in the same transaction. Always
    /// produces a permanent subname. Uses commit-reveal via [`Instruction::Commit`]
    /// or [`Instruction::CommitPublic`] over
    /// [`subname_commitment(parent_id, sub_id, owner_tag(sub_id, buyer), secret)`](subname_commitment)
    /// to prevent mempool front-running.
    BuySubname {
        parent_id: NameId,
        sub_id: NameId,
        sealed: Vec<u8>,
        price: u128,
        payout: Option<AccountId>,
        now: u64,
        secret: [u8; 32],
    },
    /// Replace a subname's sealed profile. For permanent subnames `now` may be
    /// `None` (the parent record is not touched); for revocable subnames `now`
    /// MUST be `Some(now)` so the program checks the parent's liveness and
    /// generation.
    SetSubnameRecord {
        parent_id: NameId,
        sub_id: NameId,
        sealed: Vec<u8>,
        now: Option<u64>,
    },
    /// Write a subname's sealed page; empty removes it. Same `now` rule as
    /// [`Instruction::SetSubnameRecord`].
    SetSubnamePage {
        parent_id: NameId,
        sub_id: NameId,
        sealed: Vec<u8>,
        now: Option<u64>,
    },
    /// Hand a subname to a new owner, emptying its sealed profile and page.
    /// Keeps its `SubnameKind` (permanent stays permanent; revocable stays
    /// revocable under the same parent generation). Same `now` rule as
    /// [`Instruction::SetSubnameRecord`].
    TransferSubname {
        parent_id: NameId,
        sub_id: NameId,
        new_owner: OwnerTag,
        now: Option<u64>,
    },
    /// Give a subname up, emptying its record and page. Same `now` rule as
    /// [`Instruction::SetSubnameRecord`].
    ReleaseSubname {
        parent_id: NameId,
        sub_id: NameId,
        now: Option<u64>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Effect {
    /// On the record: refused unless the actor is empty or holds a record
    /// whose grace period ended by `now`.
    Register {
        owner: OwnerTag,
        now: u64,
        expires: u64,
        sealed: Vec<u8>,
        price_tier: u8,
        generation: [u8; 32],
    },
    /// On the record: extend expiry by `term` (or set `u64::MAX` for
    /// [`TERM_PERMANENT`]), refused once its grace period has ended or if
    /// already permanent/deposit-held; forwards [`Effect::RequirePrice`] with
    /// the record's stored `price_tier` to the settings actor.
    Renew {
        id: NameId,
        payer: OwnerTag,
        treasury: Option<AccountId>,
        term: u8,
        fee: u128,
        now: u64,
    },
    /// On the settings: changes nothing, refused unless `fee` is what the payer
    /// owes for `(price_tier, term)` (or deposit for `shortlist_tier`) and
    /// `treasury`, when a fee is paid, is where it is owed. `payer` is the
    /// payer's tag for `id` (`slot_id`), so the treasury/admin can be told apart
    /// without the payer being named.
    RequirePrice {
        id: NameId,
        payer: OwnerTag,
        treasury: Option<AccountId>,
        term: u8,
        fee: u128,
        /// Paid as a deposit (deposit mode) rather than a fee (fee mode).
        deposit: bool,
        /// The name's short-list tier, as proven against `SHORTLIST_ROOT`.
        shortlist_tier: u8,
        /// The name's length-based pricing tier (`0`, `1`, or `2`).
        price_tier: u8,
    },
    /// On the settings: replace them, refused unless `signer` is their admin,
    /// or, while there are none, unless the program's own account signed.
    WriteSettings {
        signer: AccountId,
        by_program: bool,
        admin: AccountId,
        treasury: AccountId,
        prices: [[u128; TERMS]; PRICE_TIERS],
        mode: u8,
        deposits: [u128; 2],
        claims_root: [u8; 32],
        claims_until: u64,
        claims_program: AccountId,
    },
    /// On the settings: update a single cell of `prices`, refused unless
    /// `signer` is the admin.
    SetPrice {
        signer: AccountId,
        price_tier: u8,
        term: u8,
        price: u128,
    },
    /// On the settings: changes nothing, refused unless `proof` shows `holder`
    /// held `id` (base identifier) on the earlier chain and claims are open at `now`.
    RequireClaim {
        id: NameId,
        holder: OwnerTag,
        now: u64,
        proof: ClaimProof,
    },
    /// On the record: hold it for `owner` for good, refused if it was claimed.
    Claim {
        owner: OwnerTag,
        sealed: Vec<u8>,
        price_tier: u8,
        generation: [u8; 32],
    },
    /// On the record: replace the sealed profile, refused unless `signer` owns it.
    Seal { signer: OwnerTag, sealed: Vec<u8> },
    /// On the record: give it to `new_owner` with an empty profile, clear
    /// `subname_sale`, and advance `generation`; refused unless `signer` owns it.
    Transfer {
        signer: OwnerTag,
        new_owner: OwnerTag,
    },
    /// On the record: empty it, refused unless `signer` owns it.
    Remove { signer: OwnerTag },
    /// On the record: changes nothing, refused unless `signer` owns it.
    RequireOwner { signer: OwnerTag },
    /// On the page account (top-level or subname); an empty value empties it.
    WritePage { sealed: Vec<u8> },
    /// On the signer's own account.
    WritePrimary { name: String },
    /// On a commitment account: record that it was made at `now` on the public
    /// (`public = true`) or private (`public = false`) path, refused if one is
    /// there that can still be revealed.
    Commit { now: u64, public: bool },
    /// On a commitment account: use it up, refused unless it was made at least
    /// [`PUBLIC_MIN_COMMIT_AGE_MS`] (if public) or [`MIN_COMMIT_AGE_MS`] (if
    /// private) and less than [`MAX_COMMIT_AGE_MS`] before `now`.
    Reveal { now: u64 },
    /// On the parent record: update `subname_sale`, refused unless `signer` owns it.
    SetSubnameSale {
        signer: OwnerTag,
        sale: Option<SubnameSale>,
    },
    /// On the parent record: verify `signer` owns the active parent, then emit
    /// [`Effect::WriteSubname`] and [`Effect::WritePage`] (empty) to the subname.
    AuthorizeCreateSubname {
        signer: OwnerTag,
        parent_id: NameId,
        sub_id: NameId,
        owner: OwnerTag,
        revocable: bool,
        sealed: Vec<u8>,
        now: u64,
    },
    /// On the parent record: verify the parent is active and has sales enabled
    /// matching `(price, payout)`, then emit permanent [`Effect::WriteSubname`]
    /// and [`Effect::WritePage`] (empty) to the subname.
    AuthorizeBuySubname {
        parent_id: NameId,
        sub_id: NameId,
        owner: OwnerTag,
        sealed: Vec<u8>,
        price: u128,
        payout: Option<AccountId>,
        now: u64,
    },
    /// On the parent record: verify `signer` owns the parent and it has not
    /// lapsed past grace.
    RequireParentOwner { signer: OwnerTag, now: u64 },
    /// On the parent record: verify the parent exists and has not lapsed past
    /// grace, then forward [`Effect::RequireSubnameParentGeneration`] with the
    /// parent's current `generation` to the subname record.
    VerifyParentForSubname {
        parent_id: NameId,
        sub_id: NameId,
        now: u64,
    },
    /// On the subname record: write a new subname; refused if permanent or if an
    /// active revocable subname matching `current_parent_generation` is already there.
    WriteSubname {
        owner: OwnerTag,
        kind: SubnameKind,
        current_parent_generation: [u8; 32],
        sealed: Vec<u8>,
    },
    /// On the subname record: empty it; refused unless it exists and is revocable.
    RevokeSubname,
    /// On the subname record: verify a revocable subname's `parent_generation`
    /// matches `parent_generation` (always passes for permanent subnames).
    RequireSubnameParentGeneration { parent_generation: [u8; 32] },
    /// On the subname record: replace `sealed`; refused unless `signer` owns it
    /// (and, if `require_permanent`, unless it is permanent).
    SubnameSeal {
        signer: OwnerTag,
        sealed: Vec<u8>,
        require_permanent: bool,
    },
    /// On the subname record: transfer ownership and clear `sealed`.
    SubnameTransfer {
        signer: OwnerTag,
        new_owner: OwnerTag,
        require_permanent: bool,
    },
    /// On the subname record: empty it; refused unless `signer` owns it.
    SubnameRemove {
        signer: OwnerTag,
        require_permanent: bool,
    },
    /// On the subname record: verify `signer` owns it (for [`Instruction::SetSubnamePage`]).
    SubnameRequireOwner {
        signer: OwnerTag,
        require_permanent: bool,
    },
}

/// Emitted when a name is claimed. Events are emitted while planning, before
/// the program has checked the record, so they are hints: confirm each against
/// the record. They carry the identifier, never the name, and never a holder.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Registered {
    pub id: NameId,
    pub expires: u64,
}

/// Emitted when a name is carried over. A hint, like [`Registered`].
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Claimed {
    pub id: NameId,
}

/// Emitted when a name's term is extended. A hint, like [`Registered`].
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Renewed {
    pub id: NameId,
    pub expires: u64,
}

/// Emitted when a name is handed to a new owner. A hint, like [`Registered`].
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Transferred {
    pub id: NameId,
}

/// Emitted when a name is released. A hint, like [`Registered`].
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Released {
    pub id: NameId,
}

/// Emitted when a parent holder creates a subname. A hint, like [`Registered`].
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SubnameCreated {
    pub sub_id: NameId,
    pub revocable: bool,
}

/// Emitted when a permanent subname is bought from a parent. A hint, like [`Registered`].
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SubnameBought {
    pub sub_id: NameId,
}

/// Emitted when a parent holder revokes a revocable subname. A hint, like [`Registered`].
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SubnameRevoked {
    pub sub_id: NameId,
}

/// Emitted when a subname is handed to a new owner. A hint, like [`Registered`].
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SubnameTransferred {
    pub sub_id: NameId,
}

/// Emitted when a subname is released by its holder. A hint, like [`Registered`].
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SubnameReleased {
    pub sub_id: NameId,
}

impl Registered {
    pub const SELECTOR_NAME: &str = "logos_names::Registered";
}

impl Claimed {
    pub const SELECTOR_NAME: &str = "logos_names::Claimed";
}

impl Renewed {
    pub const SELECTOR_NAME: &str = "logos_names::Renewed";
}

impl Released {
    pub const SELECTOR_NAME: &str = "logos_names::Released";
}

impl Transferred {
    pub const SELECTOR_NAME: &str = "logos_names::Transferred";
}

impl SubnameCreated {
    pub const SELECTOR_NAME: &str = "logos_names::SubnameCreated";
}

impl SubnameBought {
    pub const SELECTOR_NAME: &str = "logos_names::SubnameBought";
}

impl SubnameRevoked {
    pub const SELECTOR_NAME: &str = "logos_names::SubnameRevoked";
}

impl SubnameTransferred {
    pub const SELECTOR_NAME: &str = "logos_names::SubnameTransferred";
}

impl SubnameReleased {
    pub const SELECTOR_NAME: &str = "logos_names::SubnameReleased";
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    use risc0_zkvm::sha::{Impl, Sha256 as _};

    Impl::hash_bytes(bytes)
        .as_bytes()
        .try_into()
        .expect("SHA-256 output is 32 bytes")
}

/// LEZ's convention for event selectors: the first 8 bytes of
/// `sha256("<program>::<EventName>")`.
#[must_use]
pub fn event_selector(selector_name: &str) -> [u8; 8] {
    sha256(selector_name.as_bytes())[..8]
        .try_into()
        .expect("a slice of 8 bytes")
}

fn event<E: BorshSerialize>(selector_name: &str, data: &E) -> ProgramEvent {
    ProgramEvent {
        selector: event_selector(selector_name),
        data: borsh::to_vec(data).expect("borsh serialization is infallible"),
    }
}

fn seed(domain: &[u8], id: &NameId) -> PdaSeed {
    let mut bytes = Vec::with_capacity(domain.len() + id.len());
    bytes.extend_from_slice(domain);
    bytes.extend_from_slice(id);
    PdaSeed::new(sha256(&bytes))
}

fn subname_seed(domain: &[u8], parent_id: &NameId, sub_id: &NameId) -> PdaSeed {
    let mut bytes = Vec::with_capacity(domain.len() + 64);
    bytes.extend_from_slice(domain);
    bytes.extend_from_slice(parent_id);
    bytes.extend_from_slice(sub_id);
    PdaSeed::new(sha256(&bytes))
}

/// The seeds behind a top-level name's accounts (`id` = `slot_id`). LEZ 0.4
/// admits a new public account only with evidence of where it comes from, so a
/// client lists the seed of every account a transaction may create.
#[must_use]
pub fn name_seeds(id: &NameId) -> [PdaSeed; 3] {
    [seed(NAME_SEED_DOMAIN, id), seed(PAGE_SEED_DOMAIN, id), seed(DEPOSIT_SEED_DOMAIN, id)]
}

/// The seeds behind a subname's record and page accounts.
#[must_use]
pub fn subname_seeds(parent_id: &NameId, sub_id: &NameId) -> [PdaSeed; 2] {
    [
        subname_seed(SUBNAME_SEED_DOMAIN, parent_id, sub_id),
        subname_seed(SUBPAGE_SEED_DOMAIN, parent_id, sub_id),
    ]
}

/// The seed behind [`commitment_address`].
#[must_use]
pub fn commitment_seed(commitment: &[u8; 32]) -> PdaSeed {
    seed(COMMITMENT_SEED_DOMAIN, commitment)
}

/// The seed behind [`settings_address`].
#[must_use]
pub fn settings_seed() -> PdaSeed {
    PdaSeed::new(sha256(SETTINGS_SEED_DOMAIN))
}

/// The pricing tier for a top-level name of byte length `len`:
/// `0` (`PRICE_TIER_5`) for 5 chars (and admin-placed 1–4 char names),
/// `1` (`PRICE_TIER_6_7`) for 6–7 chars, `2` (`PRICE_TIER_8_PLUS`) for 8+ chars.
#[must_use]
pub const fn price_tier_for_len(len: usize) -> u8 {
    if len <= 5 {
        PRICE_TIER_5
    } else if len <= 7 {
        PRICE_TIER_6_7
    } else {
        PRICE_TIER_8_PLUS
    }
}

/// Derives the on-chain slot identifier of a top-level name from its base
/// identifier (`derive(name).id`) and length-based `price_tier`.
///
/// Binding `price_tier` into the slot identifier makes it cryptographically
/// impossible to occupy or spoof a higher-priced tier's record by paying a
/// cheaper tier's fee, without revealing the name on chain or enumerating 5–7
/// character names into the shortlist.
#[must_use]
pub fn tiered_id(base_id: &NameId, price_tier: u8) -> NameId {
    assert!(
        usize::from(price_tier) < PRICE_TIERS,
        "price tier must be 0 (<=5 chars), 1 (6-7 chars), or 2 (8+ chars)"
    );
    let mut bytes = Vec::with_capacity(TIER_ID_DOMAIN.len() + 1 + 32);
    bytes.extend_from_slice(TIER_ID_DOMAIN);
    bytes.push(price_tier);
    bytes.extend_from_slice(base_id);
    sha256(&bytes)
}

/// The tag that stands for `account` as the holder of the name `id`. Anyone who
/// knows both can compute it; from the tag alone neither can be recovered, and
/// the same account holding two names gets two unrelated tags.
#[must_use]
pub fn owner_tag(id: &NameId, account: &AccountId) -> OwnerTag {
    let mut bytes = Vec::with_capacity(OWNER_TAG_DOMAIN.len() + 64);
    bytes.extend_from_slice(OWNER_TAG_DOMAIN);
    bytes.extend_from_slice(id);
    bytes.extend_from_slice(account.value());
    sha256(&bytes)
}

/// What a top-level registration commits to: the name's slot identifier, the
/// tag of the account that will hold it, and a secret the committer picks at random.
#[must_use]
pub fn commitment(id: &NameId, holder: &OwnerTag, secret: &[u8; 32]) -> [u8; 32] {
    let mut bytes = Vec::with_capacity(COMMITMENT_DOMAIN.len() + 96);
    bytes.extend_from_slice(COMMITMENT_DOMAIN);
    bytes.extend_from_slice(id);
    bytes.extend_from_slice(holder);
    bytes.extend_from_slice(secret);
    sha256(&bytes)
}

/// What a public subname purchase ([`Instruction::BuySubname`]) commits to,
/// used with [`Instruction::Commit`] or [`Instruction::CommitPublic`].
#[must_use]
pub fn subname_commitment(
    parent_id: &NameId,
    sub_id: &NameId,
    holder: &OwnerTag,
    secret: &[u8; 32],
) -> [u8; 32] {
    let mut bytes = Vec::with_capacity(SUBNAME_COMMITMENT_DOMAIN.len() + 128);
    bytes.extend_from_slice(SUBNAME_COMMITMENT_DOMAIN);
    bytes.extend_from_slice(parent_id);
    bytes.extend_from_slice(sub_id);
    bytes.extend_from_slice(holder);
    bytes.extend_from_slice(secret);
    sha256(&bytes)
}

fn register_generation(slot_id: &NameId, owner: &OwnerTag, now: u64, secret: &[u8; 32]) -> [u8; 32] {
    let mut bytes = Vec::with_capacity(GEN_REGISTER_DOMAIN.len() + 104);
    bytes.extend_from_slice(GEN_REGISTER_DOMAIN);
    bytes.extend_from_slice(slot_id);
    bytes.extend_from_slice(owner);
    bytes.extend_from_slice(&now.to_le_bytes());
    bytes.extend_from_slice(secret);
    sha256(&bytes)
}

fn claim_generation(slot_id: &NameId, owner: &OwnerTag, now: u64) -> [u8; 32] {
    let mut bytes = Vec::with_capacity(GEN_CLAIM_DOMAIN.len() + 72);
    bytes.extend_from_slice(GEN_CLAIM_DOMAIN);
    bytes.extend_from_slice(slot_id);
    bytes.extend_from_slice(owner);
    bytes.extend_from_slice(&now.to_le_bytes());
    sha256(&bytes)
}

fn transfer_generation(prev: &[u8; 32], new_owner: &OwnerTag) -> [u8; 32] {
    let mut bytes = Vec::with_capacity(GEN_TRANSFER_DOMAIN.len() + 64);
    bytes.extend_from_slice(GEN_TRANSFER_DOMAIN);
    bytes.extend_from_slice(prev);
    bytes.extend_from_slice(new_owner);
    sha256(&bytes)
}

/// The account holding the commitment `commitment` under the registry `program`.
#[must_use]
pub fn commitment_address(program: &AccountId, commitment: &[u8; 32]) -> AccountId {
    AccountId::for_public_pda(program, &seed(COMMITMENT_SEED_DOMAIN, commitment))
}

const fn commit_window_ms(public: bool) -> u64 {
    if public { PUBLIC_TIME_WINDOW_MS } else { TIME_WINDOW_MS }
}

const fn min_commit_age_ms(public: bool) -> u64 {
    if public { PUBLIC_MIN_COMMIT_AGE_MS } else { MIN_COMMIT_AGE_MS }
}

/// Valid only in a block whose timestamp is in `[now, now + window_ms)`.
fn bounded_window_of(response: Response, now: u64, window_ms: u64) -> Response {
    let window = lee_core::program::TimestampValidityWindow::try_from((
        Some(now),
        Some(after(now, window_ms)),
    ))
    .expect("a window of positive length");
    response.timestamp_window(window)
}

/// Valid only in a block whose timestamp is in `[now, now + TIME_WINDOW_MS)`.
fn bounded_window(response: Response, now: u64) -> Response {
    bounded_window_of(response, now, TIME_WINDOW_MS)
}

/// The leaf of a name on the claims list: the address of its record on the
/// earlier chain and its holder's tag. Addresses, not identifiers, because a
/// snapshot can see which accounts hold records but not, for a name registered
/// privately, which identifier made them. So the list publishes no identifier.
#[must_use]
pub fn claim_leaf(old_record: &AccountId, holder: &OwnerTag) -> [u8; 32] {
    let mut bytes = Vec::with_capacity(CLAIM_LEAF_DOMAIN.len() + 64);
    bytes.extend_from_slice(CLAIM_LEAF_DOMAIN);
    bytes.extend_from_slice(old_record.value());
    bytes.extend_from_slice(holder);
    sha256(&bytes)
}

fn claim_node(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let (low, high) = if left <= right { (left, right) } else { (right, left) };
    let mut bytes = Vec::with_capacity(CLAIM_NODE_DOMAIN.len() + 64);
    bytes.extend_from_slice(CLAIM_NODE_DOMAIN);
    bytes.extend_from_slice(low);
    bytes.extend_from_slice(high);
    sha256(&bytes)
}

/// A path from a leaf to the claims root. Pairs are hashed in sorted order, so
/// a path needs no directions.
pub type ClaimProof = Vec<[u8; 32]>;

/// One level up: pairs hashed, an odd last node carried as is.
fn claim_level(level: &[[u8; 32]]) -> Vec<[u8; 32]> {
    level
        .chunks(2)
        .map(|pair| match pair {
            [left, right] => claim_node(left, right),
            [only] => *only,
            _ => unreachable!("chunks of two"),
        })
        .collect()
}

/// The root of the claims list `leaves`; all zeros for an empty list.
#[must_use]
pub fn claim_root(leaves: &[[u8; 32]]) -> [u8; 32] {
    let Some(first) = leaves.first() else { return [0; 32] };
    let mut level = leaves.to_vec();
    while level.len() > 1 {
        level = claim_level(&level);
    }
    level.first().copied().unwrap_or(*first)
}

/// The proof for the leaf at `index` of `leaves`.
#[must_use]
pub fn claim_proof(leaves: &[[u8; 32]], mut index: usize) -> ClaimProof {
    let mut proof = Vec::new();
    let mut level = leaves.to_vec();
    while level.len() > 1 {
        if let Some(sibling) = level.get(index ^ 1) {
            proof.push(*sibling);
        }
        level = claim_level(&level);
        index /= 2;
    }
    proof
}

fn claim_holds(root: &[u8; 32], leaf: [u8; 32], proof: &[[u8; 32]]) -> bool {
    proof.iter().fold(leaf, |node, sibling| claim_node(&node, sibling)) == *root
}

/// The account holding the record of the top-level name `id` (`slot_id`) under
/// the registry `program`.
#[must_use]
pub fn record_address(program: &AccountId, id: &NameId) -> AccountId {
    AccountId::for_public_pda(program, &seed(NAME_SEED_DOMAIN, id))
}

/// The account holding the page of the top-level name `id` (`slot_id`) under
/// the registry `program`.
#[must_use]
pub fn page_address(program: &AccountId, id: &NameId) -> AccountId {
    AccountId::for_public_pda(program, &seed(PAGE_SEED_DOMAIN, id))
}

/// The account holding the deposit of the top-level name `id` (`slot_id`) under
/// the registry `program`, as its native balance. Only the registry can move it,
/// on release.
#[must_use]
pub fn deposit_address(program: &AccountId, id: &NameId) -> AccountId {
    AccountId::for_public_pda(program, &seed(DEPOSIT_SEED_DOMAIN, id))
}

/// The account holding the record of a subname (`sub_id`) under its parent
/// (`parent_id`, the parent's `slot_id`). Keying the PDA by both `parent_id`
/// and `sub_id` isolates every parent's subname namespace on chain.
#[must_use]
pub fn subname_record_address(
    program: &AccountId,
    parent_id: &NameId,
    sub_id: &NameId,
) -> AccountId {
    AccountId::for_public_pda(program, &subname_seed(SUBNAME_SEED_DOMAIN, parent_id, sub_id))
}

/// The account holding the page of a subname (`sub_id`) under its parent
/// (`parent_id`, the parent's `slot_id`).
#[must_use]
pub fn subname_page_address(
    program: &AccountId,
    parent_id: &NameId,
    sub_id: &NameId,
) -> AccountId {
    AccountId::for_public_pda(program, &subname_seed(SUBPAGE_SEED_DOMAIN, parent_id, sub_id))
}

/// The account holding the settings of the registry `program`.
#[must_use]
pub fn settings_address(program: &AccountId) -> AccountId {
    AccountId::for_public_pda(program, &PdaSeed::new(sha256(SETTINGS_SEED_DOMAIN)))
}

/// Checks a single label (a top-level name, or either side of `label.parent`):
/// 1–32 ASCII chars of `a-z`, `0-9`, `-`, not starting or ending with `-`, and
/// never containing `--` (reserved for the gateway's `label--parent` encoding).
pub fn validate_label(label: &str) -> Result<(), &'static str> {
    let bytes = label.as_bytes();
    let charset_ok = bytes
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-');
    if bytes.is_empty()
        || bytes.len() > MAX_NAME_LEN
        || !charset_ok
        || bytes[0] == b'-'
        || bytes[bytes.len() - 1] == b'-'
        || bytes.windows(2).any(|w| w == b"--")
    {
        return Err(
            "name must be 1-32 chars of a-z, 0-9 or '-', not starting or ending with '-', and without '--'",
        );
    }
    Ok(())
}

/// Splits a valid one-level subname `label.parent` into `(label, parent)`.
/// Returns `None` for top-level names or invalid subnames.
#[must_use]
pub fn split_subname(name: &str) -> Option<(&str, &str)> {
    if name.len() > MAX_SUBNAME_LEN {
        return None;
    }
    let (label, parent) = name.split_once('.')?;
    validate_label(label).ok()?;
    validate_label(parent).ok()?;
    Some((label, parent))
}

/// Checks a top-level name (`alice`) or a one-level subname (`blog.alice`).
pub fn validate_name(name: &str) -> Result<(), &'static str> {
    if let Some((label, parent)) = name.split_once('.') {
        if name.len() > MAX_SUBNAME_LEN {
            return Err("subname must be at most 62 chars (so gateway label--parent fits in 63 bytes)");
        }
        validate_label(label)?;
        validate_label(parent)?;
        return Ok(());
    }
    validate_label(name)
}

/// Turns what a person typed, such as `Jef`, `@jef`, `lns:jef`, `lns://blog.alice`
/// or `jef.lez`, into the bare name. The result still has to pass [`validate_name`].
#[must_use]
pub fn normalize_name(input: &str) -> String {
    let lowered = input.trim().to_ascii_lowercase();
    let bare = lowered
        .strip_prefix("lns://")
        .or_else(|| lowered.strip_prefix("lns:"))
        .unwrap_or(&lowered);
    let bare = bare.strip_prefix('@').unwrap_or(bare);
    bare.strip_suffix(LEGACY_SUFFIX).unwrap_or(bare).to_owned()
}

fn checked<T>(result: Result<T, &'static str>) -> T {
    result.unwrap_or_else(|reason| panic!("{reason}"))
}

fn term_length(term: u8) -> u64 {
    *TERM_LENGTHS_MS
        .get(usize::from(term))
        .unwrap_or_else(|| panic!("term must be 0, 1, 2 or 3"))
}

fn check_term(term: u8) {
    assert!(usize::from(term) < TERMS, "term must be 0, 1, 2 or 3");
}

fn after(time: u64, length: u64) -> u64 {
    time.checked_add(length).expect("the time is out of range")
}

pub use shortlist::{LengthProof, TIER_LOCKED, TIER_NORMAL, TIER_THREE};

/// The root of the locked-name list ([`shortlist`]): the identifiers of every name
/// of one to four characters and of the reserved list (`reserved/reserved.csv`:
/// nouns, first names, web3 projects, brands, and the app-store and wallet names),
/// all locked. Set from `tools/shortlist`'s output (`shortlist/root.hex`) for the v5
/// derivation: 1,845,226 names and two sentinels, depth 21. Regenerate it if the
/// derivation or the reserved list changes.
pub const SHORTLIST_ROOT: [u8; 32] = hex_literal(b"9778bed49005fe7401d25685a27d9cf3e681fa0582b562336cdb8ee9661d990c");

/// 64 hex characters as 32 bytes, at compile time.
const fn hex_literal(hex: &[u8; 64]) -> [u8; 32] {
    const fn nibble(c: u8) -> u8 {
        match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            _ => panic!("not a lowercase hex digit"),
        }
    }
    let mut out = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        out[i] = nibble(hex[2 * i]) << 4 | nibble(hex[2 * i + 1]);
        i += 1;
    }
    out
}

/// The tier `proof` shows for the name `id`; refuses a proof that does not hold.
fn length_tier(id: &NameId, proof: &LengthProof) -> u8 {
    #[cfg(not(test))]
    let checked = shortlist::verify(&SHORTLIST_ROOT, id, proof);
    #[cfg(test)]
    let checked = tests::verify_against_test_list(id, proof);
    checked.unwrap_or_else(|reason| panic!("the length proof does not hold: {reason}"))
}

fn self_actor(account_id: AccountId, program: &AccountId) -> Actor {
    Actor::new(account_id, *program)
}

fn effect(to: AccountId, program: &AccountId, effect: Effect) -> SyncMessage {
    SyncMessage::new(self_actor(to, program), &Message::Effect(effect))
}

/// Checks and charges a registration for the signing `payer`. The settings
/// decide in `apply` whether the amount is right; the money moves by a native
/// transfer, which the payer's signature authorizes. It is credited in this
/// transaction as long as the client declares the account paid.
#[allow(clippy::too_many_arguments)]
fn charge(
    input: &ReceiveInput,
    response: Response,
    program: &AccountId,
    payer: &AccountId,
    id: &NameId,
    treasury: Option<AccountId>,
    term: u8,
    fee: u128,
    deposit: bool,
    shortlist_tier: u8,
    price_tier: u8,
) -> Response {
    let to = match (fee, deposit) {
        (0, _) => None,
        (1.., true) => Some(deposit_address(program, id)),
        (1.., false) => Some(treasury.expect("a fee is paid to the treasury")),
    };
    let response = response.send(self::effect(
        settings_address(program),
        program,
        Effect::RequirePrice {
            id: *id,
            payer: owner_tag(id, payer),
            treasury: if deposit { None } else { to },
            term,
            fee,
            deposit,
            shortlist_tier,
            price_tier,
        },
    ));
    match to {
        Some(to) => response.send(
            SyncMessage::new(
                Actor::native_balance(*payer),
                &native_token::Message::Transfer { to, amount: fee },
            )
            .with_lent_grant(input),
        ),
        None => response,
    }
}

/// Handles a message to one of this program's actors. The guest's entry point.
#[must_use]
pub fn handle(input: &ReceiveInput, message: Message) -> Response {
    match message {
        Message::Instruction(instruction) => instruct(input, instruction),
        Message::Effect(effect) => {
            assert!(input.from_own_program(), "only this program may send an effect");
            // For a reveal, the commitment actor itself knows which path the
            // commitment was made under and enforces the matching clock window
            // on the reveal transaction.
            let reveal_window = match &effect {
                Effect::Reveal { now } => load_commitment(&input.pre_state)
                    .map(|held| (*now, commit_window_ms(held.public))),
                _ => None,
            };
            let response = apply(&input.receiver.program_account_id, effect, &input.pre_state);
            match reveal_window {
                Some((now, window_ms)) => bounded_window_of(response, now, window_ms),
                None => response,
            }
        }
        Message::RequireHolder { tag } => {
            load_owned(&input.pre_state, &tag);
            Response::keep_state()
        }
    }
}

/// Turns an instruction from the signer into the effects it has on other actors.
#[must_use]
pub fn instruct(input: &ReceiveInput, instruction: Instruction) -> Response {
    assert!(input.is_authorized(), "the signer's account must sign");
    let program = &input.receiver.program_account_id;
    let signer = &input.receiver.account_id;
    let response = Response::keep_state();

    match instruction {
        Instruction::Register {
            id,
            sealed,
            term,
            fee,
            treasury,
            now,
            secret,
            deposit,
            length,
            price_tier,
        } => {
            assert!(
                sealed.len() <= MAX_SEALED_RECORD_LEN,
                "sealed profile exceeds {MAX_SEALED_RECORD_LEN} bytes"
            );
            let expires = if deposit {
                u64::MAX
            } else {
                check_term(term);
                if term == TERM_PERMANENT {
                    u64::MAX
                } else {
                    after(now, term_length(term))
                }
            };
            let shortlist_tier = length_tier(&id, &length);
            let slot_id = tiered_id(&id, price_tier);
            let owner = owner_tag(&slot_id, signer);
            // Bound to the holder and slot: a reveal seen pending can't be
            // replayed for another account or pricing tier.
            let promised = commitment(&slot_id, &owner, &secret);
            let generation = register_generation(&slot_id, &owner, now, &secret);
            let response = charge(
                input,
                response,
                program,
                signer,
                &slot_id,
                treasury,
                term,
                fee,
                deposit,
                shortlist_tier,
                price_tier,
            );
            bounded_window(response, now)
                .send(self::effect(
                    commitment_address(program, &promised),
                    program,
                    Effect::Reveal { now },
                ))
                .event(event(Registered::SELECTOR_NAME, &Registered { id: slot_id, expires }))
                .send(self::effect(
                    record_address(program, &slot_id),
                    program,
                    Effect::Register {
                        owner,
                        now,
                        expires,
                        sealed,
                        price_tier,
                        generation,
                    },
                ))
                // A lapsed holder's page must not outlive their hold on the name.
                .send(self::effect(
                    page_address(program, &slot_id),
                    program,
                    Effect::WritePage { sealed: Vec::new() },
                ))
        }
        Instruction::Renew { id, term, fee, treasury, now } => {
            check_term(term);
            let to = if fee == 0 {
                None
            } else {
                Some(treasury.expect("a fee is paid to the treasury"))
            };
            let event_expires = if term == TERM_PERMANENT {
                u64::MAX
            } else {
                after(now, term_length(term))
            };
            let response = response
                // Valid only in a block at least as late as `now`, so `now` is
                // never in the future and `apply` can use it as the time.
                .timestamp_window(now..)
                // The new expiry depends on the record, which only `apply` sees;
                // the event carries the least it can be.
                .event(event(
                    Renewed::SELECTOR_NAME,
                    &Renewed { id, expires: event_expires },
                ))
                .send(self::effect(
                    record_address(program, &id),
                    program,
                    Effect::Renew {
                        id,
                        payer: owner_tag(&id, signer),
                        treasury: to,
                        term,
                        fee,
                        now,
                    },
                ));
            match to {
                Some(to) => response.send(
                    SyncMessage::new(
                        Actor::native_balance(*signer),
                        &native_token::Message::Transfer { to, amount: fee },
                    )
                    .with_lent_grant(input),
                ),
                None => response,
            }
        }
        Instruction::SetRecord { id, sealed } => {
            assert!(
                sealed.len() <= MAX_SEALED_RECORD_LEN,
                "sealed profile exceeds {MAX_SEALED_RECORD_LEN} bytes"
            );
            response.send(self::effect(
                record_address(program, &id),
                program,
                Effect::Seal { signer: owner_tag(&id, signer), sealed },
            ))
        }
        Instruction::SetPage { id, sealed } => {
            assert!(
                sealed.len() <= MAX_SEALED_PAGE_LEN,
                "sealed page exceeds {MAX_SEALED_PAGE_LEN} bytes"
            );
            response
                .send(self::effect(
                    record_address(program, &id),
                    program,
                    Effect::RequireOwner { signer: owner_tag(&id, signer) },
                ))
                .send(self::effect(
                    page_address(program, &id),
                    program,
                    Effect::WritePage { sealed },
                ))
        }
        Instruction::Transfer { id, new_owner } => response
            .event(event(Transferred::SELECTOR_NAME, &Transferred { id }))
            .send(self::effect(
                record_address(program, &id),
                program,
                Effect::Transfer { signer: owner_tag(&id, signer), new_owner },
            ))
            .send(self::effect(
                page_address(program, &id),
                program,
                Effect::WritePage { sealed: Vec::new() },
            )),
        Instruction::Release { id, refund } => {
            let response = response
                .event(event(Released::SELECTOR_NAME, &Released { id }))
                .send(self::effect(
                    record_address(program, &id),
                    program,
                    Effect::Remove { signer: owner_tag(&id, signer) },
                ))
                .send(self::effect(
                    page_address(program, &id),
                    program,
                    Effect::WritePage { sealed: Vec::new() },
                ));
            if refund == 0 {
                return response;
            }
            // Whoever holds the name gets its deposit: only the owner passes
            // `Remove`, and only this program can sign for the deposit account.
            response.send(native_token::custody_transfer(
                deposit_address(program, &id),
                seed(DEPOSIT_SEED_DOMAIN, &id),
                *signer,
                refund,
            ))
        }
        Instruction::SetPrimary { name } => {
            if !name.is_empty() {
                checked(validate_name(&name));
            }
            // The one instruction on the signer's own actor.
            apply(program, Effect::WritePrimary { name }, &input.pre_state)
        }
        Instruction::Configure {
            admin,
            treasury,
            prices,
            mode,
            deposits,
            claims_root,
            claims_until,
            claims_program,
        } => {
            assert!(mode == MODE_FEE || mode == MODE_DEPOSIT, "mode must be fee (0) or deposit (1)");
            response.send(self::effect(
                settings_address(program),
                program,
                Effect::WriteSettings {
                    signer: *signer,
                    by_program: signer == program,
                    admin,
                    treasury,
                    prices,
                    mode,
                    deposits,
                    claims_root,
                    claims_until,
                    claims_program,
                },
            ))
        }
        Instruction::SetPrice { price_tier, term, price } => {
            assert!(
                usize::from(price_tier) < PRICE_TIERS,
                "price tier must be 0 (<=5 chars), 1 (6-7 chars), or 2 (8+ chars)"
            );
            check_term(term);
            response.send(self::effect(
                settings_address(program),
                program,
                Effect::SetPrice {
                    signer: *signer,
                    price_tier,
                    term,
                    price,
                },
            ))
        }
        Instruction::Commit { commitment, now } => bounded_window(response, now).send(self::effect(
            commitment_address(program, &commitment),
            program,
            Effect::Commit { now, public: false },
        )),
        Instruction::CommitPublic { commitment, now } => {
            bounded_window_of(response, now, PUBLIC_TIME_WINDOW_MS).send(self::effect(
                commitment_address(program, &commitment),
                program,
                Effect::Commit { now, public: true },
            ))
        }
        Instruction::Claim {
            id,
            sealed,
            now,
            proof,
            price_tier,
        } => {
            assert!(
                sealed.len() <= MAX_SEALED_RECORD_LEN,
                "sealed profile exceeds {MAX_SEALED_RECORD_LEN} bytes"
            );
            let slot_id = tiered_id(&id, price_tier);
            // The snapshot on the earlier chain recorded `owner_tag(&base_id, signer)`.
            let old_holder = owner_tag(&id, signer);
            let owner = owner_tag(&slot_id, signer);
            let generation = claim_generation(&slot_id, &owner, now);
            bounded_window(response, now)
                .event(event(Claimed::SELECTOR_NAME, &Claimed { id: slot_id }))
                .send(self::effect(
                    settings_address(program),
                    program,
                    Effect::RequireClaim { id, holder: old_holder, now, proof },
                ))
                .send(self::effect(
                    record_address(program, &slot_id),
                    program,
                    Effect::Claim {
                        owner,
                        sealed,
                        price_tier,
                        generation,
                    },
                ))
                // Whatever a registrant wrote before this claim goes with them.
                .send(self::effect(
                    page_address(program, &slot_id),
                    program,
                    Effect::WritePage { sealed: Vec::new() },
                ))
        }
        Instruction::SetSubnameSale { id, sale } => response.send(self::effect(
            record_address(program, &id),
            program,
            Effect::SetSubnameSale {
                signer: owner_tag(&id, signer),
                sale,
            },
        )),
        Instruction::CreateSubname {
            parent_id,
            sub_id,
            owner,
            revocable,
            sealed,
            now,
        } => {
            assert!(
                sealed.len() <= MAX_SEALED_RECORD_LEN,
                "sealed profile exceeds {MAX_SEALED_RECORD_LEN} bytes"
            );
            bounded_window(response, now)
                .event(event(
                    SubnameCreated::SELECTOR_NAME,
                    &SubnameCreated { sub_id, revocable },
                ))
                .send(self::effect(
                    record_address(program, &parent_id),
                    program,
                    Effect::AuthorizeCreateSubname {
                        signer: owner_tag(&parent_id, signer),
                        parent_id,
                        sub_id,
                        owner,
                        revocable,
                        sealed,
                        now,
                    },
                ))
        }
        Instruction::RevokeSubname { parent_id, sub_id, now } => bounded_window(response, now)
            .event(event(
                SubnameRevoked::SELECTOR_NAME,
                &SubnameRevoked { sub_id },
            ))
            .send(self::effect(
                record_address(program, &parent_id),
                program,
                Effect::RequireParentOwner {
                    signer: owner_tag(&parent_id, signer),
                    now,
                },
            ))
            .send(self::effect(
                subname_record_address(program, &parent_id, &sub_id),
                program,
                Effect::RevokeSubname,
            ))
            .send(self::effect(
                subname_page_address(program, &parent_id, &sub_id),
                program,
                Effect::WritePage { sealed: Vec::new() },
            )),
        Instruction::BuySubname {
            parent_id,
            sub_id,
            sealed,
            price,
            payout,
            now,
            secret,
        } => {
            assert!(
                sealed.len() <= MAX_SEALED_RECORD_LEN,
                "sealed profile exceeds {MAX_SEALED_RECORD_LEN} bytes"
            );
            let to = if price == 0 {
                None
            } else {
                Some(payout.expect("a subname purchase payment needs the parent's payout account"))
            };
            let owner = owner_tag(&sub_id, signer);
            let promised = subname_commitment(&parent_id, &sub_id, &owner, &secret);
            let response = bounded_window(response, now)
                .send(self::effect(
                    commitment_address(program, &promised),
                    program,
                    Effect::Reveal { now },
                ))
                .event(event(
                    SubnameBought::SELECTOR_NAME,
                    &SubnameBought { sub_id },
                ))
                .send(self::effect(
                    record_address(program, &parent_id),
                    program,
                    Effect::AuthorizeBuySubname {
                        parent_id,
                        sub_id,
                        owner,
                        sealed,
                        price,
                        payout: to,
                        now,
                    },
                ));
            match to {
                Some(to) => response.send(
                    SyncMessage::new(
                        Actor::native_balance(*signer),
                        &native_token::Message::Transfer { to, amount: price },
                    )
                    .with_lent_grant(input),
                ),
                None => response,
            }
        }
        Instruction::SetSubnameRecord {
            parent_id,
            sub_id,
            sealed,
            now,
        } => {
            assert!(
                sealed.len() <= MAX_SEALED_RECORD_LEN,
                "sealed profile exceeds {MAX_SEALED_RECORD_LEN} bytes"
            );
            let (response, require_permanent) =
                with_optional_parent_check(response, program, &parent_id, &sub_id, now);
            response.send(self::effect(
                subname_record_address(program, &parent_id, &sub_id),
                program,
                Effect::SubnameSeal {
                    signer: owner_tag(&sub_id, signer),
                    sealed,
                    require_permanent,
                },
            ))
        }
        Instruction::SetSubnamePage {
            parent_id,
            sub_id,
            sealed,
            now,
        } => {
            assert!(
                sealed.len() <= MAX_SEALED_PAGE_LEN,
                "sealed page exceeds {MAX_SEALED_PAGE_LEN} bytes"
            );
            let (response, require_permanent) =
                with_optional_parent_check(response, program, &parent_id, &sub_id, now);
            response
                .send(self::effect(
                    subname_record_address(program, &parent_id, &sub_id),
                    program,
                    Effect::SubnameRequireOwner {
                        signer: owner_tag(&sub_id, signer),
                        require_permanent,
                    },
                ))
                .send(self::effect(
                    subname_page_address(program, &parent_id, &sub_id),
                    program,
                    Effect::WritePage { sealed },
                ))
        }
        Instruction::TransferSubname {
            parent_id,
            sub_id,
            new_owner,
            now,
        } => {
            let (response, require_permanent) =
                with_optional_parent_check(response, program, &parent_id, &sub_id, now);
            response
                .event(event(
                    SubnameTransferred::SELECTOR_NAME,
                    &SubnameTransferred { sub_id },
                ))
                .send(self::effect(
                    subname_record_address(program, &parent_id, &sub_id),
                    program,
                    Effect::SubnameTransfer {
                        signer: owner_tag(&sub_id, signer),
                        new_owner,
                        require_permanent,
                    },
                ))
                .send(self::effect(
                    subname_page_address(program, &parent_id, &sub_id),
                    program,
                    Effect::WritePage { sealed: Vec::new() },
                ))
        }
        Instruction::ReleaseSubname { parent_id, sub_id, now } => {
            let (response, require_permanent) =
                with_optional_parent_check(response, program, &parent_id, &sub_id, now);
            response
                .event(event(
                    SubnameReleased::SELECTOR_NAME,
                    &SubnameReleased { sub_id },
                ))
                .send(self::effect(
                    subname_record_address(program, &parent_id, &sub_id),
                    program,
                    Effect::SubnameRemove {
                        signer: owner_tag(&sub_id, signer),
                        require_permanent,
                    },
                ))
                .send(self::effect(
                    subname_page_address(program, &parent_id, &sub_id),
                    program,
                    Effect::WritePage { sealed: Vec::new() },
                ))
        }
    }
}

/// If `now` is `Some`, binds the transaction to `[now, now + TIME_WINDOW_MS)`
/// and verifies the parent record's liveness and generation before the subname
/// effect executes. If `None`, the subname effect enforces `SubnameKind::Permanent`.
fn with_optional_parent_check(
    response: Response,
    program: &AccountId,
    parent_id: &NameId,
    sub_id: &NameId,
    now: Option<u64>,
) -> (Response, bool) {
    match now {
        Some(now) => (
            bounded_window(response, now).send(self::effect(
                record_address(program, parent_id),
                program,
                Effect::VerifyParentForSubname {
                    parent_id: *parent_id,
                    sub_id: *sub_id,
                    now,
                },
            )),
            false,
        ),
        None => (response, true),
    }
}

fn to_bytes<T: BorshSerialize>(value: &T) -> Vec<u8> {
    borsh::to_vec(value).expect("borsh serialization is infallible")
}

/// Decodes the record in `pre_data`, refusing unless `signer` is its owner's tag.
fn load_owned(pre_data: &[u8], signer: &OwnerTag) -> NameRecord {
    assert!(!pre_data.is_empty(), "name is not registered");
    let record: NameRecord =
        borsh::from_slice(pre_data).expect("a record actor holds a valid record");
    assert!(record.owner == *signer, "signer does not own this name");
    record
}

/// Decodes the subname record in `pre_data`, refusing unless `signer` owns it
/// (and, when `require_permanent` is true, unless it is permanent).
fn load_owned_subname(
    pre_data: &[u8],
    signer: &OwnerTag,
    require_permanent: bool,
) -> SubnameRecord {
    assert!(!pre_data.is_empty(), "subname is not registered");
    let record: SubnameRecord =
        borsh::from_slice(pre_data).expect("a subname record actor holds a valid subname record");
    if require_permanent {
        assert!(
            matches!(record.kind, SubnameKind::Permanent),
            "revocable subnames must be checked against the parent"
        );
    }
    assert!(record.owner == *signer, "signer does not own this subname");
    record
}

fn load_settings(pre_data: &[u8]) -> Option<Settings> {
    (!pre_data.is_empty())
        .then(|| borsh::from_slice(pre_data).expect("the settings actor holds valid settings"))
}

/// Applies one effect to the actor it targets. `pre_data` is the actor's
/// current data (empty if unset). Returns a [`Response`] with any new actor
/// state and any follow-up effects. Panicking fails the whole transaction.
#[must_use]
pub fn apply(program: &AccountId, effect: Effect, pre_data: &[u8]) -> Response {
    match effect {
        Effect::Register {
            owner,
            now,
            expires,
            sealed,
            price_tier,
            generation,
        } => {
            if !pre_data.is_empty() {
                let held: NameRecord =
                    borsh::from_slice(pre_data).expect("a record actor holds a valid record");
                assert!(
                    held.expires.saturating_add(GRACE_MS) <= now,
                    "name is already registered"
                );
            }
            Response::set_state(to_bytes(&NameRecord {
                version: VERSION,
                owner,
                expires,
                sealed,
                claimed: false,
                price_tier,
                generation,
                subname_sale: None,
            }))
        }
        Effect::Renew {
            id,
            payer,
            treasury,
            term,
            fee,
            now,
        } => {
            assert!(!pre_data.is_empty(), "name is not registered");
            let mut record: NameRecord =
                borsh::from_slice(pre_data).expect("a record actor holds a valid record");
            assert!(
                record.expires != u64::MAX,
                "a permanent or deposit-held name does not lapse or renew"
            );
            assert!(
                now < after(record.expires, GRACE_MS),
                "name has lapsed and must be registered again"
            );
            record.expires = if term == TERM_PERMANENT {
                u64::MAX
            } else {
                after(record.expires.max(now), term_length(term))
            };
            let price_tier = record.price_tier;
            Response::set_state(to_bytes(&record)).send(self::effect(
                settings_address(program),
                program,
                Effect::RequirePrice {
                    id,
                    payer,
                    treasury,
                    term,
                    fee,
                    deposit: false,
                    shortlist_tier: TIER_NORMAL,
                    price_tier,
                },
            ))
        }
        Effect::RequirePrice {
            id,
            payer,
            treasury,
            term,
            fee,
            deposit,
            shortlist_tier,
            price_tier,
        } => {
            let settings = load_settings(pre_data);
            if shortlist_tier == TIER_LOCKED {
                // Locked names are released by the admin registering them, then
                // transferring them to whoever they are for.
                assert!(
                    settings.as_ref().is_some_and(|settings| payer == owner_tag(&id, &settings.admin)),
                    "this name is locked (one to four characters, or on the reserved list); only the registry's admin may register it"
                );
            }
            let owed = match settings {
                Some(settings) => {
                    let deposit_mode = settings.mode == MODE_DEPOSIT;
                    assert!(
                        deposit == deposit_mode,
                        "{}",
                        if deposit_mode {
                            "the registry takes deposits: register with a deposit; names don't renew"
                        } else {
                            "the registry charges fees, not deposits"
                        }
                    );
                    if deposit_mode && payer == owner_tag(&id, &settings.admin) {
                        // The admin places names (a locked one it releases, a
                        // built-in one) without a deposit; there is nothing to
                        // refund when they are released.
                        0
                    } else if deposit_mode {
                        settings.deposits[usize::from(shortlist_tier == TIER_THREE)]
                    } else if payer == owner_tag(&id, &settings.treasury) {
                        // A treasury paying itself would move nothing.
                        0
                    } else {
                        assert!(
                            treasury.is_none_or(|treasury| treasury == settings.treasury),
                            "the treasury account is not the registry's treasury"
                        );
                        settings.prices[usize::from(price_tier)][usize::from(term)]
                    }
                }
                // An unconfigured registry charges nothing.
                None => 0,
            };
            assert!(
                fee == owed,
                "{}",
                if deposit {
                    "the deposit is not this name's deposit"
                } else {
                    "the fee is not the price of this term"
                }
            );
            Response::keep_state()
        }
        Effect::WriteSettings {
            signer,
            by_program,
            admin,
            treasury,
            prices,
            mode,
            deposits,
            claims_root,
            claims_until,
            claims_program,
        } => {
            match load_settings(pre_data) {
                Some(settings) => {
                    assert!(signer == settings.admin, "signer is not the registry's admin");
                }
                // Nobody but the deployer can configure a fresh registry.
                None => assert!(by_program, "the first settings must be signed by the program's own account"),
            }
            Response::set_state(to_bytes(&Settings {
                version: VERSION,
                admin,
                treasury,
                prices,
                mode,
                deposits,
                claims_root,
                claims_until,
                claims_program,
            }))
        }
        Effect::SetPrice {
            signer,
            price_tier,
            term,
            price,
        } => {
            let mut settings = load_settings(pre_data).expect("the registry has no settings");
            assert!(signer == settings.admin, "signer is not the registry's admin");
            settings.prices[usize::from(price_tier)][usize::from(term)] = price;
            Response::set_state(to_bytes(&settings))
        }
        Effect::RequireClaim { id, holder, now, proof } => {
            let settings = load_settings(pre_data).expect("the registry has no settings");
            assert!(settings.claims_root != [0; 32], "this registry takes no claims");
            assert!(now < settings.claims_until, "the claim period has ended");
            let leaf = claim_leaf(&record_address(&settings.claims_program, &id), &holder);
            assert!(
                claim_holds(&settings.claims_root, leaf, &proof),
                "this account did not hold this name"
            );
            Response::keep_state()
        }
        Effect::Claim {
            owner,
            sealed,
            price_tier,
            generation,
        } => {
            if !pre_data.is_empty() {
                let held: NameRecord =
                    borsh::from_slice(pre_data).expect("a record state holds a valid record");
                assert!(!held.claimed, "this name was already claimed");
            }
            Response::set_state(to_bytes(&NameRecord {
                version: VERSION,
                owner,
                expires: u64::MAX,
                sealed,
                claimed: true,
                price_tier,
                generation,
                subname_sale: None,
            }))
        }
        Effect::Seal { signer, sealed } => {
            let mut record = load_owned(pre_data, &signer);
            record.sealed = sealed;
            Response::set_state(to_bytes(&record))
        }
        Effect::Transfer { signer, new_owner } => {
            let mut record = load_owned(pre_data, &signer);
            record.owner = new_owner;
            record.sealed = Vec::new();
            record.generation = transfer_generation(&record.generation, &new_owner);
            record.subname_sale = None;
            Response::set_state(to_bytes(&record))
        }
        Effect::Remove { signer } => {
            load_owned(pre_data, &signer);
            Response::set_state(Vec::new())
        }
        Effect::RequireOwner { signer } => {
            load_owned(pre_data, &signer);
            Response::keep_state()
        }
        Effect::WritePage { sealed } => Response::set_state(if sealed.is_empty() {
            Vec::new()
        } else {
            to_bytes(&Page {
                version: VERSION,
                sealed,
            })
        }),
        Effect::WritePrimary { name } => Response::set_state(if name.is_empty() {
            Vec::new()
        } else {
            to_bytes(&PrimaryName {
                version: VERSION,
                name,
            })
        }),
        Effect::Commit { now, public } => {
            if let Some(held) = load_commitment(pre_data) {
                assert!(
                    after(held.made, MAX_COMMIT_AGE_MS) <= now,
                    "this commitment was already made"
                );
            }
            Response::set_state(to_bytes(&Commitment { version: VERSION, made: now, public }))
        }
        Effect::Reveal { now } => {
            let held = load_commitment(pre_data).expect("no commitment to reveal");
            assert!(
                after(held.made, min_commit_age_ms(held.public)) <= now,
                "the commitment is too recent; reveal it later"
            );
            assert!(now < after(held.made, MAX_COMMIT_AGE_MS), "the commitment has expired");
            Response::set_state(Vec::new())
        }
        Effect::SetSubnameSale { signer, sale } => {
            let mut record = load_owned(pre_data, &signer);
            record.subname_sale = sale;
            Response::set_state(to_bytes(&record))
        }
        Effect::AuthorizeCreateSubname {
            signer,
            parent_id,
            sub_id,
            owner,
            revocable,
            sealed,
            now,
        } => {
            let parent = load_owned(pre_data, &signer);
            assert!(now < parent.expires, "parent name has lapsed");
            let kind = if revocable {
                SubnameKind::Revocable {
                    parent_generation: parent.generation,
                }
            } else {
                SubnameKind::Permanent
            };
            Response::keep_state()
                .send(self::effect(
                    subname_record_address(program, &parent_id, &sub_id),
                    program,
                    Effect::WriteSubname {
                        owner,
                        kind,
                        current_parent_generation: parent.generation,
                        sealed,
                    },
                ))
                .send(self::effect(
                    subname_page_address(program, &parent_id, &sub_id),
                    program,
                    Effect::WritePage { sealed: Vec::new() },
                ))
        }
        Effect::AuthorizeBuySubname {
            parent_id,
            sub_id,
            owner,
            sealed,
            price,
            payout,
            now,
        } => {
            assert!(!pre_data.is_empty(), "parent name is not registered");
            let parent: NameRecord =
                borsh::from_slice(pre_data).expect("a record actor holds a valid record");
            assert!(now < parent.expires, "parent name has lapsed");
            let sale = parent
                .subname_sale
                .expect("subname sales are not enabled for this parent");
            assert!(
                price == sale.price,
                "the payment does not match the parent's subname price"
            );
            if sale.price > 0 {
                assert!(
                    payout == Some(sale.payout),
                    "the payout account is not the parent's payout account"
                );
            }
            Response::keep_state()
                .send(self::effect(
                    subname_record_address(program, &parent_id, &sub_id),
                    program,
                    Effect::WriteSubname {
                        owner,
                        kind: SubnameKind::Permanent,
                        current_parent_generation: parent.generation,
                        sealed,
                    },
                ))
                .send(self::effect(
                    subname_page_address(program, &parent_id, &sub_id),
                    program,
                    Effect::WritePage { sealed: Vec::new() },
                ))
        }
        Effect::RequireParentOwner { signer, now } => {
            let parent = load_owned(pre_data, &signer);
            assert!(
                now < parent.expires.saturating_add(GRACE_MS),
                "parent name has lapsed"
            );
            Response::keep_state()
        }
        Effect::VerifyParentForSubname {
            parent_id,
            sub_id,
            now,
        } => {
            assert!(!pre_data.is_empty(), "parent name is not registered");
            let parent: NameRecord =
                borsh::from_slice(pre_data).expect("a record actor holds a valid record");
            assert!(
                now < parent.expires.saturating_add(GRACE_MS),
                "parent name has lapsed"
            );
            Response::keep_state().send(self::effect(
                subname_record_address(program, &parent_id, &sub_id),
                program,
                Effect::RequireSubnameParentGeneration {
                    parent_generation: parent.generation,
                },
            ))
        }
        Effect::WriteSubname {
            owner,
            kind,
            current_parent_generation,
            sealed,
        } => {
            if !pre_data.is_empty() {
                let held: SubnameRecord = borsh::from_slice(pre_data)
                    .expect("a subname record actor holds a valid subname record");
                match held.kind {
                    SubnameKind::Permanent => panic!("subname is already registered"),
                    SubnameKind::Revocable { parent_generation } => {
                        assert!(
                            parent_generation != current_parent_generation,
                            "subname is already registered"
                        );
                    }
                }
            }
            Response::set_state(to_bytes(&SubnameRecord {
                version: VERSION,
                owner,
                kind,
                sealed,
            }))
        }
        Effect::RevokeSubname => {
            assert!(!pre_data.is_empty(), "subname is not registered");
            let held: SubnameRecord = borsh::from_slice(pre_data)
                .expect("a subname record actor holds a valid subname record");
            assert!(
                matches!(held.kind, SubnameKind::Revocable { .. }),
                "permanent subnames cannot be revoked"
            );
            Response::set_state(Vec::new())
        }
        Effect::RequireSubnameParentGeneration { parent_generation } => {
            assert!(!pre_data.is_empty(), "subname is not registered");
            let held: SubnameRecord = borsh::from_slice(pre_data)
                .expect("a subname record actor holds a valid subname record");
            if let SubnameKind::Revocable {
                parent_generation: expected,
            } = held.kind
            {
                assert!(
                    expected == parent_generation,
                    "subname is no longer valid under the parent's current holder"
                );
            }
            Response::keep_state()
        }
        Effect::SubnameSeal {
            signer,
            sealed,
            require_permanent,
        } => {
            let mut record = load_owned_subname(pre_data, &signer, require_permanent);
            record.sealed = sealed;
            Response::set_state(to_bytes(&record))
        }
        Effect::SubnameTransfer {
            signer,
            new_owner,
            require_permanent,
        } => {
            let mut record = load_owned_subname(pre_data, &signer, require_permanent);
            record.owner = new_owner;
            record.sealed = Vec::new();
            Response::set_state(to_bytes(&record))
        }
        Effect::SubnameRemove {
            signer,
            require_permanent,
        } => {
            load_owned_subname(pre_data, &signer, require_permanent);
            Response::set_state(Vec::new())
        }
        Effect::SubnameRequireOwner {
            signer,
            require_permanent,
        } => {
            load_owned_subname(pre_data, &signer, require_permanent);
            Response::keep_state()
        }
    }
}

fn load_commitment(pre_data: &[u8]) -> Option<Commitment> {
    (!pre_data.is_empty())
        .then(|| borsh::from_slice(pre_data).expect("a commitment actor holds a commitment"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tests' stand-in for the short-name list: two locked names and two
    /// three-character names, among ids the other tests don't use.
    pub(super) const LOCKED_NAME: NameId = [0x11; 32];
    pub(super) const LOCKED_NAME_2: NameId = [0x12; 32];
    pub(super) const THREE_NAME: NameId = [0x33; 32];
    pub(super) const THREE_NAME_2: NameId = [0x34; 32];

    fn test_list() -> &'static shortlist::ShortList {
        static LIST: std::sync::OnceLock<shortlist::ShortList> = std::sync::OnceLock::new();
        LIST.get_or_init(|| {
            shortlist::ShortList::new(vec![
                (LOCKED_NAME, TIER_LOCKED),
                (LOCKED_NAME_2, TIER_LOCKED),
                (THREE_NAME, TIER_THREE),
                (THREE_NAME_2, TIER_THREE),
            ])
            .expect("a valid test list")
        })
    }

    pub(super) fn verify_against_test_list(id: &NameId, proof: &LengthProof) -> Result<u8, &'static str> {
        let list = test_list();
        shortlist::verify_depth(&list.root(), id, proof, list.depth())
    }

    /// The true length proof for `id` under the test list.
    fn length_of(id: &NameId) -> LengthProof {
        test_list().prove(id).expect("a proof")
    }

    #[test]
    fn anyone_can_ask_who_holds_a_name_and_only_the_holder_passes() {
        let held = to_bytes(&NameRecord {
            version: VERSION,
            owner: [1; 32],
            expires: u64::MAX,
            sealed: Vec::new(),
            claimed: false,
            price_tier: PRICE_TIER_8_PLUS,
            generation: [0; 32],
            subname_sale: None,
        });
        let asked = |pre_state: Vec<u8>, tag| {
            let input = ReceiveInput {
                receiver: Actor::new(ALICE, PROGRAM),
                from: Some(Actor::new(BOB, AccountId::new([9; 32]))),
                grants: Default::default(),
                pre_state: pre_state.into(),
                message: Vec::new().into(),
            };
            std::panic::catch_unwind(|| handle(&input, Message::RequireHolder { tag }).post_state)
        };
        assert_eq!(asked(held.clone(), [1; 32]).expect("the holder passes"), None);
        assert!(asked(held, [2; 32]).is_err(), "anyone else is refused");
        assert!(asked(Vec::new(), [1; 32]).is_err(), "an unregistered name is refused");
    }

    const PROGRAM: AccountId = AccountId::new([7; 32]);
    const ALICE: AccountId = AccountId::new([1; 32]);
    const BOB: AccountId = AccountId::new([2; 32]);
    const CAROL: AccountId = AccountId::new([3; 32]);
    const DAVE: AccountId = AccountId::new([4; 32]);
    const JEF: NameId = [0xAA; 32];
    const ADA: NameId = [0xBB; 32];
    const BLOG_SUB: NameId = [0xC1; 32];
    const SHOP_SUB: NameId = [0xC2; 32];
    const DEFAULT_TIER: u8 = PRICE_TIER_8_PLUS;
    const NOW: u64 = 1_700_000_000_000;
    const YEAR: u64 = TERM_LENGTHS_MS[1];
    const MONTH: u64 = TERM_LENGTHS_MS[0];
    const PRICES: [[u128; TERMS]; PRICE_TIERS] = DEFAULT_PRICES;

    fn slot(base: &NameId) -> NameId {
        tiered_id(base, DEFAULT_TIER)
    }

    fn slot_of(base: &NameId, price_tier: u8) -> NameId {
        tiered_id(base, price_tier)
    }

    use lee_core::program::TimestampValidityWindow;

    // The tests were written against explicit account lists. They keep them: the
    // signer is the list's authorized account, and everything else the actor
    // model now derives for itself is checked by asserting on the messages.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct AccountMeta {
        account_id: AccountId,
        is_authorized: bool,
    }

    impl AccountMeta {
        fn native_balance(account_id: AccountId, is_authorized: bool) -> Self {
            Self { account_id, is_authorized }
        }
    }

    struct PlanInput {
        receive: ReceiveInput,
        treasury: Option<AccountId>,
    }

    struct Target {
        account_id: AccountId,
    }

    struct Sent {
        selector: Target,
        data: Vec<u8>,
    }

    struct Output {
        effects: Vec<Sent>,
        events: Vec<ProgramEvent>,
        chained_calls: Vec<SyncMessage>,
        timestamp_validity_window: TimestampValidityWindow,
    }

    struct Planned(Output);

    impl Planned {
        fn output(&self) -> &Output {
            &self.0
        }
    }

    fn meta(account_id: AccountId, is_authorized: bool) -> AccountMeta {
        AccountMeta { account_id, is_authorized }
    }

    fn input(accounts: Vec<AccountMeta>, instruction: &Instruction) -> PlanInput {
        let signer = accounts.iter().find(|account| account.is_authorized).copied();
        let receiver = signer.or(accounts.last().copied()).expect("some account");
        let paid = accounts.last().filter(|account| !account.is_authorized).map(|account| account.account_id);
        let mut grants = std::collections::BTreeSet::new();
        if let Some(s) = signer {
            grants.insert(Actor::new(s.account_id, PROGRAM));
            grants.insert(Actor::native_balance(s.account_id));
        }
        PlanInput {
            receive: ReceiveInput {
                receiver: Actor::new(receiver.account_id, PROGRAM),
                from: None,
                grants,
                pre_state: Default::default(),
                message: borsh::to_vec(instruction).expect("the instruction serializes"),
            },
            treasury: paid,
        }
    }

    fn plan(input: &PlanInput, mut instruction: Instruction) -> Planned {
        match &mut instruction {
            Instruction::Register { treasury, deposit: false, .. } | Instruction::Renew { treasury, .. } => {
                *treasury = input.treasury;
            }
            _ => {}
        }
        let primary = match &instruction {
            Instruction::SetPrimary { name } => Some(name.clone()),
            _ => None,
        };
        let response = instruct(&input.receive, instruction);
        let (mine, chained_calls): (Vec<_>, Vec<_>) = response
            .sync_messages
            .into_iter()
            .partition(|message| message.to.program_account_id == PROGRAM);
        let mut effects: Vec<Sent> = mine
            .into_iter()
            .map(|message| {
                let Ok(Message::Effect(effect)) = borsh::from_slice(&message.message) else {
                    panic!("this program only sends itself effects");
                };
                Sent {
                    selector: Target { account_id: message.to.account_id },
                    data: borsh::to_vec(&effect).expect("the effect serializes"),
                }
            })
            .collect();
        if let Some(name) = primary {
            effects.push(Sent {
                selector: Target { account_id: input.receive.receiver.account_id },
                data: borsh::to_vec(&Effect::WritePrimary { name }).expect("the effect serializes"),
            });
        }
        Planned(Output {
            effects,
            events: response.events,
            chained_calls,
            timestamp_validity_window: response.validity.timestamps,
        })
    }

    fn run_plan(accounts: Vec<AccountMeta>, instruction: Instruction) -> Vec<(AccountId, Effect)> {
        plan(&input(accounts, &instruction), instruction)
            .output()
            .effects
            .iter()
            .map(|effect| {
                (
                    effect.selector.account_id,
                    borsh::from_slice(&effect.data).expect("the effect decodes"),
                )
            })
            .collect()
    }

    fn run_plan_events(accounts: Vec<AccountMeta>, instruction: Instruction) -> Vec<ProgramEvent> {
        plan(&input(accounts, &instruction), instruction)
            .output()
            .events
            .clone()
    }

    fn native(account_id: AccountId) -> AccountMeta {
        AccountMeta::native_balance(account_id, true)
    }

    /// The secret the test registrations commit with.
    const SECRET: [u8; 32] = [5; 32];

    fn committed_slot(slot_id: &NameId, holder: AccountId) -> AccountId {
        commitment_address(&PROGRAM, &commitment(slot_id, &owner_tag(slot_id, &holder), &SECRET))
    }

    fn committed(base_id: &NameId, holder: AccountId) -> AccountId {
        committed_slot(&slot(base_id), holder)
    }

    fn on_registration_tier(base_id: &NameId, price_tier: u8, payer: AccountId) -> Vec<AccountMeta> {
        let s = slot_of(base_id, price_tier);
        vec![
            meta(record_address(&PROGRAM, &s), false),
            meta(page_address(&PROGRAM, &s), false),
            meta(settings_address(&PROGRAM), false),
            meta(committed_slot(&s, payer), false),
            native(payer),
        ]
    }

    /// The accounts of a `Register` that pays no fee (default tier).
    fn on_registration(base_id: &NameId, payer: AccountId) -> Vec<AccountMeta> {
        on_registration_tier(base_id, DEFAULT_TIER, payer)
    }

    /// The accounts of a `Renew` that pays no fee (`slot_id`).
    fn on_renewal_slot(slot_id: &NameId, payer: AccountId) -> Vec<AccountMeta> {
        vec![
            meta(record_address(&PROGRAM, slot_id), false),
            meta(settings_address(&PROGRAM), false),
            native(payer),
        ]
    }

    fn on_renewal(base_id: &NameId, payer: AccountId) -> Vec<AccountMeta> {
        on_renewal_slot(&slot(base_id), payer)
    }

    /// The accounts with the treasury a fee is paid to added.
    fn paying(mut accounts: Vec<AccountMeta>, treasury: AccountId) -> Vec<AccountMeta> {
        accounts.push(AccountMeta::native_balance(treasury, false));
        accounts
    }

    /// A free one-year registration at `NOW` (default tier).
    fn register(id: NameId, sealed: Vec<u8>) -> Instruction {
        register_at(id, sealed, 1, 0, NOW)
    }

    fn register_at(id: NameId, sealed: Vec<u8>, term: u8, fee: u128, now: u64) -> Instruction {
        register_tier_at(id, sealed, term, fee, now, DEFAULT_TIER)
    }

    fn register_tier_at(
        id: NameId,
        sealed: Vec<u8>,
        term: u8,
        fee: u128,
        now: u64,
        price_tier: u8,
    ) -> Instruction {
        Instruction::Register {
            id,
            sealed,
            term,
            fee,
            treasury: None,
            now,
            secret: SECRET,
            deposit: false,
            length: length_of(&id),
            price_tier,
        }
    }

    fn on_record(base_id: &NameId, signer: AccountId) -> Vec<AccountMeta> {
        vec![meta(record_address(&PROGRAM, &slot(base_id)), false), meta(signer, true)]
    }

    fn on_record_and_page(base_id: &NameId, signer: AccountId) -> Vec<AccountMeta> {
        let s = slot(base_id);
        vec![
            meta(record_address(&PROGRAM, &s), false),
            meta(page_address(&PROGRAM, &s), false),
            meta(signer, true),
        ]
    }

    /// Runs an instruction against a tiny in-memory chain: plans it, then
    /// dispatches every effect (and follow-up sync effects) in LEZ's exact
    /// depth-first LIFO order, all or nothing.
    #[derive(Default)]
    struct Chain {
        actors: std::collections::BTreeMap<AccountId, Vec<u8>>,
    }

    impl Chain {
        fn run(&mut self, accounts: Vec<AccountMeta>, instruction: Instruction) {
            // Tests that are not about commitments register as if one had been
            // made long enough before.
            if let (Instruction::Register { now, .. }, Some(commit_slot)) =
                (&instruction, accounts.get(3))
            {
                self.actors.entry(commit_slot.account_id).or_insert_with(|| {
                    to_bytes(&Commitment {
                        version: VERSION,
                        made: now - MIN_COMMIT_AGE_MS,
                        public: false,
                    })
                });
            }
            self.run_exactly(accounts, instruction);
        }

        fn run_exactly(&mut self, accounts: Vec<AccountMeta>, instruction: Instruction) {
            let mut next = self.actors.clone();
            let mut stack: Vec<(AccountId, Effect)> =
                run_plan(accounts, instruction).into_iter().rev().collect();
            while let Some((account, effect)) = stack.pop() {
                let before = next.get(&account).cloned().unwrap_or_default();
                let response = apply(&PROGRAM, effect, &before);
                if let Some(after) = response.post_state {
                    next.insert(account, after.into_inner());
                }
                let follow_ups: Vec<(AccountId, Effect)> = response
                    .sync_messages
                    .into_iter()
                    .filter(|message| message.to.program_account_id == PROGRAM)
                    .map(|message| {
                        let Ok(Message::Effect(effect)) = borsh::from_slice(&message.message) else {
                            panic!("this program only sends itself effects");
                        };
                        (message.to.account_id, effect)
                    })
                    .collect();
                stack.extend(follow_ups.into_iter().rev());
            }
            self.actors = next;
        }

        fn data(&self, account: &AccountId) -> &[u8] {
            self.actors.get(account).map_or(&[], Vec::as_slice)
        }

        fn record_slot(&self, slot_id: &NameId) -> Option<NameRecord> {
            let data = self.data(&record_address(&PROGRAM, slot_id));
            (!data.is_empty()).then(|| borsh::from_slice(data).expect("a record"))
        }

        fn record(&self, base_id: &NameId) -> Option<NameRecord> {
            self.record_slot(&slot(base_id))
        }

        fn page_slot(&self, slot_id: &NameId) -> Option<Page> {
            let data = self.data(&page_address(&PROGRAM, slot_id));
            (!data.is_empty()).then(|| borsh::from_slice(data).expect("a page"))
        }

        fn page(&self, base_id: &NameId) -> Option<Page> {
            self.page_slot(&slot(base_id))
        }

        fn subname(&self, parent_slot: &NameId, sub_id: &NameId) -> Option<SubnameRecord> {
            let data = self.data(&subname_record_address(&PROGRAM, parent_slot, sub_id));
            (!data.is_empty()).then(|| borsh::from_slice(data).expect("a subname record"))
        }

        fn subname_page(&self, parent_slot: &NameId, sub_id: &NameId) -> Option<Page> {
            let data = self.data(&subname_page_address(&PROGRAM, parent_slot, sub_id));
            (!data.is_empty()).then(|| borsh::from_slice(data).expect("a subname page"))
        }

        fn settings(&self) -> Option<Settings> {
            let data = self.data(&settings_address(&PROGRAM));
            (!data.is_empty()).then(|| borsh::from_slice(data).expect("settings decode"))
        }

        fn commit(&mut self, base_id: &NameId, holder: AccountId, now: u64) {
            let s = slot(base_id);
            let promised = commitment(&s, &owner_tag(&s, &holder), &SECRET);
            self.run_exactly(
                vec![meta(commitment_address(&PROGRAM, &promised), false), meta(CAROL, true)],
                Instruction::Commit { commitment: promised, now },
            );
        }

        fn commit_public(&mut self, base_id: &NameId, holder: AccountId, now: u64) {
            let s = slot(base_id);
            let promised = commitment(&s, &owner_tag(&s, &holder), &SECRET);
            self.run_exactly(
                vec![meta(commitment_address(&PROGRAM, &promised), false), meta(CAROL, true)],
                Instruction::CommitPublic { commitment: promised, now },
            );
        }

        fn reveal(&mut self, base_id: &NameId, holder: AccountId, now: u64) {
            self.run_exactly(
                on_registration(base_id, holder),
                register_at(*base_id, Vec::new(), 1, 0, now),
            );
        }

        fn commit_subname(
            &mut self,
            parent_slot: &NameId,
            sub_id: &NameId,
            buyer: AccountId,
            now: u64,
            public: bool,
        ) {
            let promised =
                subname_commitment(parent_slot, sub_id, &owner_tag(sub_id, &buyer), &SECRET);
            let instruction = if public {
                Instruction::CommitPublic { commitment: promised, now }
            } else {
                Instruction::Commit { commitment: promised, now }
            };
            self.run_exactly(
                vec![meta(commitment_address(&PROGRAM, &promised), false), meta(buyer, true)],
                instruction,
            );
        }

        /// The first settings are signed by the program's own account, as its
        /// deployer does; later ones by `signer`.
        fn configure(
            &mut self,
            signer: AccountId,
            admin: AccountId,
            prices: [[u128; TERMS]; PRICE_TIERS],
        ) {
            let signer = if self.settings().is_none() { PROGRAM } else { signer };
            self.run(
                vec![meta(settings_address(&PROGRAM), false), meta(signer, true)],
                Instruction::Configure {
                    admin,
                    treasury: BOB,
                    prices,
                    mode: MODE_FEE,
                    deposits: [0, 0],
                    claims_root: [0; 32],
                    claims_until: 0,
                    claims_program: PROGRAM,
                },
            );
        }

        fn renew(&mut self, base_id: &NameId, payer: AccountId, now: u64) {
            self.run(
                on_renewal(base_id, payer),
                Instruction::Renew {
                    id: slot(base_id),
                    term: 0,
                    fee: 0,
                    treasury: None,
                    now,
                },
            );
        }

        fn register(&mut self, base_id: &NameId, owner: AccountId, sealed: &[u8]) {
            self.run(
                on_registration(base_id, owner),
                register(*base_id, sealed.to_vec()),
            );
        }
    }

    #[test]
    fn a_name_is_registered_with_its_first_profile() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"sealed profile");
        let s = slot(&JEF);
        let owner = owner_tag(&s, &ALICE);
        assert_eq!(
            chain.record(&JEF),
            Some(NameRecord {
                version: VERSION,
                owner,
                expires: NOW + TERM_LENGTHS_MS[1],
                sealed: b"sealed profile".to_vec(),
                claimed: false,
                price_tier: DEFAULT_TIER,
                generation: register_generation(&s, &owner, NOW, &SECRET),
                subname_sale: None,
            })
        );
    }

    #[test]
    #[should_panic(expected = "name is already registered")]
    fn a_registered_name_cannot_be_registered_again() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"");
        chain.register(&JEF, BOB, b"");
    }

    #[test]
    #[should_panic(expected = "the signer's account must sign")]
    fn registering_needs_the_owners_signature() {
        let mut chain = Chain::default();
        chain.run(
            on_registration(&JEF, ALICE)
                .into_iter()
                .map(|account| AccountMeta { is_authorized: false, ..account })
                .collect(),
            register(JEF, Vec::new()),
        );
    }

    #[test]
    fn the_owner_replaces_the_profile() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"first");
        chain.run(
            on_record(&JEF, ALICE),
            Instruction::SetRecord {
                id: slot(&JEF),
                sealed: b"second".to_vec(),
            },
        );
        let record = chain.record(&JEF).expect("registered");
        assert_eq!(record.sealed, b"second");
        assert_eq!(record.owner, owner_tag(&slot(&JEF), &ALICE));
    }

    #[test]
    #[should_panic(expected = "signer does not own this name")]
    fn someone_else_cannot_replace_the_profile() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"first");
        chain.run(
            on_record(&JEF, BOB),
            Instruction::SetRecord {
                id: slot(&JEF),
                sealed: b"hijacked".to_vec(),
            },
        );
    }

    #[test]
    #[should_panic(expected = "name is not registered")]
    fn an_unregistered_name_has_no_profile_to_replace() {
        let mut chain = Chain::default();
        chain.run(
            on_record(&JEF, ALICE),
            Instruction::SetRecord {
                id: slot(&JEF),
                sealed: b"x".to_vec(),
            },
        );
    }

    #[test]
    #[should_panic(expected = "sealed profile exceeds")]
    fn an_oversized_profile_is_refused() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, &vec![0; MAX_SEALED_RECORD_LEN + 1]);
    }

    #[test]
    fn the_owner_writes_and_removes_the_page() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"");
        chain.run(
            on_record_and_page(&JEF, ALICE),
            Instruction::SetPage {
                id: slot(&JEF),
                sealed: b"sealed page".to_vec(),
            },
        );
        assert_eq!(
            chain.page(&JEF),
            Some(Page {
                version: VERSION,
                sealed: b"sealed page".to_vec(),
            })
        );
        chain.run(
            on_record_and_page(&JEF, ALICE),
            Instruction::SetPage {
                id: slot(&JEF),
                sealed: Vec::new(),
            },
        );
        assert_eq!(chain.page(&JEF), None);
    }

    #[test]
    #[should_panic(expected = "signer does not own this name")]
    fn someone_else_cannot_write_the_page() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"");
        chain.run(
            on_record_and_page(&JEF, BOB),
            Instruction::SetPage {
                id: slot(&JEF),
                sealed: b"defaced".to_vec(),
            },
        );
    }

    #[test]
    #[should_panic(expected = "sealed page exceeds")]
    fn an_oversized_page_is_refused() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"");
        chain.run(
            on_record_and_page(&JEF, ALICE),
            Instruction::SetPage {
                id: slot(&JEF),
                sealed: vec![0; MAX_SEALED_PAGE_LEN + 1],
            },
        );
    }

    #[test]
    fn a_transfer_hands_over_the_name_and_empties_what_was_sealed() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"alice's profile");
        let s = slot(&JEF);
        let initial_gen = chain.record(&JEF).expect("registered").generation;
        chain.run(
            on_record_and_page(&JEF, ALICE),
            Instruction::SetPage {
                id: s,
                sealed: b"alice's page".to_vec(),
            },
        );
        let bob_tag = owner_tag(&s, &BOB);
        chain.run(
            on_record_and_page(&JEF, ALICE),
            Instruction::Transfer {
                id: s,
                new_owner: bob_tag,
            },
        );
        assert_eq!(
            chain.record(&JEF),
            Some(NameRecord {
                version: VERSION,
                owner: bob_tag,
                expires: NOW + TERM_LENGTHS_MS[1],
                sealed: Vec::new(),
                claimed: false,
                price_tier: DEFAULT_TIER,
                generation: transfer_generation(&initial_gen, &bob_tag),
                subname_sale: None,
            })
        );
        assert_eq!(chain.page(&JEF), None);

        // The new owner can use it; the old one no longer can.
        chain.run(
            on_record(&JEF, BOB),
            Instruction::SetRecord {
                id: s,
                sealed: b"bob's profile".to_vec(),
            },
        );
        assert_eq!(chain.record(&JEF).expect("registered").sealed, b"bob's profile");
    }

    #[test]
    #[should_panic(expected = "signer does not own this name")]
    fn someone_else_cannot_transfer_the_name() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"");
        let s = slot(&JEF);
        chain.run(
            on_record_and_page(&JEF, BOB),
            Instruction::Transfer {
                id: s,
                new_owner: owner_tag(&s, &BOB),
            },
        );
    }

    #[test]
    #[should_panic(expected = "signer does not own this name")]
    fn the_last_owner_cannot_change_a_transferred_name() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"");
        let s = slot(&JEF);
        chain.run(
            on_record_and_page(&JEF, ALICE),
            Instruction::Transfer {
                id: s,
                new_owner: owner_tag(&s, &BOB),
            },
        );
        chain.run(
            on_record(&JEF, ALICE),
            Instruction::SetRecord {
                id: s,
                sealed: b"still mine".to_vec(),
            },
        );
    }

    #[test]
    fn a_released_name_leaves_nothing_and_can_be_registered_again() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"alice's profile");
        let s = slot(&JEF);
        chain.run(
            on_record_and_page(&JEF, ALICE),
            Instruction::SetPage {
                id: s,
                sealed: b"alice's page".to_vec(),
            },
        );
        chain.run(on_record_and_page(&JEF, ALICE), Instruction::Release { id: s, refund: 0 });
        assert_eq!(chain.record(&JEF), None);
        assert_eq!(chain.page(&JEF), None);
        assert!(chain.data(&record_address(&PROGRAM, &s)).is_empty());
        assert!(chain.data(&page_address(&PROGRAM, &s)).is_empty());

        chain.register(&JEF, BOB, b"bob's profile");
        assert_eq!(chain.record(&JEF).expect("registered").owner, owner_tag(&s, &BOB));
    }

    #[test]
    #[should_panic(expected = "signer does not own this name")]
    fn someone_else_cannot_release_the_name() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"");
        chain.run(on_record_and_page(&JEF, BOB), Instruction::Release { id: slot(&JEF), refund: 0 });
    }

    #[test]
    #[should_panic(expected = "name is not registered")]
    fn an_unregistered_name_cannot_be_released() {
        let mut chain = Chain::default();
        chain.run(on_record_and_page(&JEF, ALICE), Instruction::Release { id: slot(&JEF), refund: 0 });
    }

    #[test]
    fn one_names_changes_never_touch_another() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"jef");
        chain.register(&ADA, BOB, b"ada");
        chain.run(on_record_and_page(&JEF, ALICE), Instruction::Release { id: slot(&JEF), refund: 0 });
        assert_eq!(chain.record(&ADA).expect("registered").sealed, b"ada");
        assert_eq!(chain.actors.values().filter(|data| !data.is_empty()).count(), 1);
    }

    #[test]
    fn an_account_publishes_and_withdraws_the_name_it_is_shown_as() {
        let mut chain = Chain::default();
        chain.run(
            vec![meta(ALICE, true)],
            Instruction::SetPrimary { name: "jef".into() },
        );
        let primary: PrimaryName = borsh::from_slice(chain.data(&ALICE)).expect("a reverse record");
        assert_eq!(primary.name, "jef");

        // Subnames are valid primary names too.
        chain.run(
            vec![meta(ALICE, true)],
            Instruction::SetPrimary { name: "blog.alice".into() },
        );
        let primary: PrimaryName = borsh::from_slice(chain.data(&ALICE)).expect("a reverse record");
        assert_eq!(primary.name, "blog.alice");

        chain.run(
            vec![meta(ALICE, true)],
            Instruction::SetPrimary { name: String::new() },
        );
        assert!(chain.data(&ALICE).is_empty());
    }

    #[test]
    #[should_panic(expected = "name must be 1-32 chars")]
    fn a_published_name_must_be_a_valid_name() {
        let mut chain = Chain::default();
        chain.run(
            vec![meta(ALICE, true)],
            Instruction::SetPrimary { name: "Not A Name".into() },
        );
    }

    #[test]
    #[should_panic(expected = "name must be 1-32 chars")]
    fn double_hyphen_cannot_be_published_as_primary() {
        let mut chain = Chain::default();
        chain.run(
            vec![meta(ALICE, true)],
            Instruction::SetPrimary { name: "a--b".into() },
        );
    }

    #[test]
    #[should_panic(expected = "the signer's account must sign")]
    fn nobody_can_publish_a_name_for_someone_elses_account() {
        let mut chain = Chain::default();
        chain.run(
            vec![meta(ALICE, false)],
            Instruction::SetPrimary { name: "jef".into() },
        );
    }

    #[test]
    fn names_are_normalised_and_checked() {
        for typed in ["jef", "jef.lez", "Jef.LEZ", "  jef.lez  ", "lns:jef", "lns://jef", "@Jef"] {
            assert_eq!(normalize_name(typed), "jef", "{typed}");
        }
        for typed in ["blog.alice", "Blog.Alice", "lns://Blog.Alice", "lns:blog.alice", "@blog.alice", "blog.alice.lez"] {
            assert_eq!(normalize_name(typed), "blog.alice", "{typed}");
        }
        for name in [
            "a",
            "jef",
            "my-name-2",
            &"a".repeat(32),
            "blog.alice",
            "a-b.c-d",
            &format!("{}.{}", "a".repeat(30), "b".repeat(31)), // 62 chars (gateway 63)
        ] {
            assert!(validate_name(name).is_ok(), "{name}");
        }
        assert_eq!(split_subname("blog.alice"), Some(("blog", "alice")));
        assert_eq!(split_subname("alice"), None);
        assert_eq!(split_subname("a.b.c"), None);

        for name in [
            "",
            "-jef",
            "jef-",
            "a--b",
            "xn--abc",
            "Jef",
            "j f",
            &"a".repeat(33),
            ".alice",
            "blog.",
            "a..b",
            "a.b.c",
            "a--b.alice",
            "blog.a--b",
            "-blog.alice",
            "blog-.alice",
            "blog.-alice",
            "blog.alice-",
            &format!("{}.{}", "a".repeat(33), "b"),
            &format!("{}.{}", "a", "b".repeat(33)),
            &format!("{}.{}", "a".repeat(31), "b".repeat(31)), // 63 chars -> gateway 64 > 63
        ] {
            assert!(validate_name(name).is_err(), "{name}");
        }
        assert_eq!(price_tier_for_len(1), PRICE_TIER_5);
        assert_eq!(price_tier_for_len(4), PRICE_TIER_5);
        assert_eq!(price_tier_for_len(5), PRICE_TIER_5);
        assert_eq!(price_tier_for_len(6), PRICE_TIER_6_7);
        assert_eq!(price_tier_for_len(7), PRICE_TIER_6_7);
        assert_eq!(price_tier_for_len(8), PRICE_TIER_8_PLUS);
        assert_eq!(price_tier_for_len(32), PRICE_TIER_8_PLUS);
    }

    #[test]
    fn a_names_two_accounts_differ_from_each_other_and_from_other_names() {
        let all = [
            record_address(&PROGRAM, &JEF),
            page_address(&PROGRAM, &JEF),
            record_address(&PROGRAM, &ADA),
            page_address(&PROGRAM, &ADA),
            subname_record_address(&PROGRAM, &JEF, &BLOG_SUB),
            subname_page_address(&PROGRAM, &JEF, &BLOG_SUB),
            subname_record_address(&PROGRAM, &ADA, &BLOG_SUB),
            subname_record_address(&PROGRAM, &JEF, &SHOP_SUB),
            record_address(&PROGRAM, &BLOG_SUB),
        ];
        for (index, account) in all.iter().enumerate() {
            assert!(!all[index + 1..].contains(account));
        }
        assert_ne!(record_address(&PROGRAM, &JEF), record_address(&ALICE, &JEF));
        // Distinct tiers map the same base id to distinct slot ids.
        assert_ne!(tiered_id(&JEF, PRICE_TIER_5), tiered_id(&JEF, PRICE_TIER_6_7));
        assert_ne!(tiered_id(&JEF, PRICE_TIER_6_7), tiered_id(&JEF, PRICE_TIER_8_PLUS));
        assert_ne!(tiered_id(&JEF, PRICE_TIER_5), JEF);
    }

    /// The vectors in SPEC.md: other implementations must produce the same bytes.
    #[test]
    fn addresses_and_instructions_match_the_specification() {
        let id: NameId = core::array::from_fn(|index| index as u8);
        assert_eq!(record_address(&PROGRAM, &id).to_string(), RECORD_VECTOR);
        assert_eq!(page_address(&PROGRAM, &id).to_string(), PAGE_VECTOR);

        let hex = |bytes: Vec<u8>| -> String {
            bytes.iter().map(|byte| format!("{byte:02x}")).collect()
        };
        let inst_hex = |instruction: &Instruction| -> String {
            hex(borsh::to_vec(instruction).expect("the instruction serializes"))
        };
        let id_hex = hex(id.to_vec());
        let sub_id = [0x33; 32];
        let sub_hex = "33".repeat(32);

        // 0: Register (added trailing price_tier: u8)
        assert_eq!(
            inst_hex(&Instruction::Register {
                id,
                sealed: vec![0xEE, 0xFF],
                term: 1,
                fee: 100_000,
                treasury: None,
                now: 1_700_000_000_000,
                secret: [0x5A; 32],
                deposit: false,
                length: LengthProof::Membership(shortlist::Membership {
                    index: 7,
                    tier: TIER_THREE,
                    path: Vec::new(),
                }),
                price_tier: PRICE_TIER_8_PLUS,
            }),
            format!(
                "00{id_hex}02000000eeff01a0860100000000000000000000000000000068e5cf8b010000{}00000700000003{}02",
                "5a".repeat(32),
                "00".repeat(4)
            )
        );
        // 1..6, 8, 10: unchanged
        assert_eq!(
            inst_hex(&Instruction::Renew {
                id,
                term: 2,
                fee: 0,
                treasury: None,
                now: 1_700_000_000_000,
            }),
            format!("01{id_hex}02{}0068e5cf8b010000", "00".repeat(17))
        );
        assert_eq!(
            inst_hex(&Instruction::SetRecord { id, sealed: vec![0xEE] }),
            format!("02{id_hex}01000000ee")
        );
        assert_eq!(
            inst_hex(&Instruction::SetPage { id, sealed: Vec::new() }),
            format!("03{id_hex}00000000")
        );
        assert_eq!(
            inst_hex(&Instruction::Transfer { id, new_owner: [0x02; 32] }),
            format!("04{id_hex}{}", "02".repeat(32))
        );
        assert_eq!(
            inst_hex(&Instruction::Release { id, refund: 0 }),
            format!("05{id_hex}{}", "00".repeat(16))
        );
        assert_eq!(
            inst_hex(&Instruction::SetPrimary { name: "jef".into() }),
            "06030000006a6566"
        );
        // 7: Configure (prices is now [[u128; 4]; 3])
        let table_hex: String = (1u8..=12)
            .map(|n| format!("{n:02x}{}", "00".repeat(15)))
            .collect();
        assert_eq!(
            inst_hex(&Instruction::Configure {
                admin: ALICE,
                treasury: BOB,
                prices: [[1, 2, 3, 4], [5, 6, 7, 8], [9, 10, 11, 12]],
                mode: MODE_FEE,
                deposits: [0, 0],
                claims_root: [0; 32],
                claims_until: 0,
                claims_program: PROGRAM,
            }),
            format!(
                "07{}{}{table_hex}00{}{}",
                "01".repeat(32),
                "02".repeat(32),
                "00".repeat(72),
                "07".repeat(32),
            )
        );
        assert_eq!(
            inst_hex(&Instruction::Commit {
                commitment: [0xC0; 32],
                now: 1_700_000_000_000,
            }),
            format!("08{}0068e5cf8b010000", "c0".repeat(32))
        );
        // 9: Claim (added trailing price_tier: u8)
        assert_eq!(
            inst_hex(&Instruction::Claim {
                id,
                sealed: vec![0xEE],
                now: 1_700_000_000_000,
                proof: vec![[0x01; 32], [0x02; 32]],
                price_tier: PRICE_TIER_6_7,
            }),
            format!(
                "09{id_hex}01000000ee0068e5cf8b01000002000000{}{}01",
                "01".repeat(32),
                "02".repeat(32)
            )
        );
        assert_eq!(
            inst_hex(&Instruction::CommitPublic {
                commitment: [0xC0; 32],
                now: 1_700_000_000_000,
            }),
            format!("0a{}0068e5cf8b010000", "c0".repeat(32))
        );
        // 11 (0x0b): SetPrice
        assert_eq!(
            inst_hex(&Instruction::SetPrice {
                price_tier: PRICE_TIER_5,
                term: TERM_PERMANENT,
                price: 400_000_000,
            }),
            "0b00030084d717000000000000000000000000"
        );
        // 12 (0x0c): SetSubnameSale (on & off)
        assert_eq!(
            inst_hex(&Instruction::SetSubnameSale {
                id,
                sale: Some(SubnameSale { price: 100_000, payout: BOB }),
            }),
            format!("0c{id_hex}01a0860100000000000000000000000000{}", "02".repeat(32))
        );
        assert_eq!(
            inst_hex(&Instruction::SetSubnameSale { id, sale: None }),
            format!("0c{id_hex}00")
        );
        // 13 (0x0d): CreateSubname
        assert_eq!(
            inst_hex(&Instruction::CreateSubname {
                parent_id: id,
                sub_id,
                owner: [0x02; 32],
                revocable: true,
                sealed: vec![0xEE],
                now: 1_700_000_000_000,
            }),
            format!(
                "0d{id_hex}{sub_hex}{}0101000000ee0068e5cf8b010000",
                "02".repeat(32)
            )
        );
        // 14 (0x0e): RevokeSubname
        assert_eq!(
            inst_hex(&Instruction::RevokeSubname {
                parent_id: id,
                sub_id,
                now: 1_700_000_000_000,
            }),
            format!("0e{id_hex}{sub_hex}0068e5cf8b010000")
        );
        // 15 (0x0f): BuySubname
        assert_eq!(
            inst_hex(&Instruction::BuySubname {
                parent_id: id,
                sub_id,
                sealed: vec![0xEE],
                price: 100_000,
                payout: Some(BOB),
                now: 1_700_000_000_000,
                secret: [0x5A; 32],
            }),
            format!(
                "0f{id_hex}{sub_hex}01000000eea086010000000000000000000000000001{}0068e5cf8b010000{}",
                "02".repeat(32),
                "5a".repeat(32),
            )
        );
        // 16 (0x10): SetSubnameRecord (None & Some)
        assert_eq!(
            inst_hex(&Instruction::SetSubnameRecord {
                parent_id: id,
                sub_id,
                sealed: vec![0xEE],
                now: None,
            }),
            format!("10{id_hex}{sub_hex}01000000ee00")
        );
        assert_eq!(
            inst_hex(&Instruction::SetSubnameRecord {
                parent_id: id,
                sub_id,
                sealed: vec![0xEE],
                now: Some(1_700_000_000_000),
            }),
            format!("10{id_hex}{sub_hex}01000000ee010068e5cf8b010000")
        );
        // 17 (0x11): SetSubnamePage
        assert_eq!(
            inst_hex(&Instruction::SetSubnamePage {
                parent_id: id,
                sub_id,
                sealed: Vec::new(),
                now: None,
            }),
            format!("11{id_hex}{sub_hex}0000000000")
        );
        // 18 (0x12): TransferSubname
        assert_eq!(
            inst_hex(&Instruction::TransferSubname {
                parent_id: id,
                sub_id,
                new_owner: [0x02; 32],
                now: Some(1_700_000_000_000),
            }),
            format!(
                "12{id_hex}{sub_hex}{}010068e5cf8b010000",
                "02".repeat(32)
            )
        );
        // 19 (0x13): ReleaseSubname
        assert_eq!(
            inst_hex(&Instruction::ReleaseSubname {
                parent_id: id,
                sub_id,
                now: None,
            }),
            format!("13{id_hex}{sub_hex}00")
        );

        // Account layouts (v7): NameRecord & SubnameRecord
        let rec = NameRecord {
            version: VERSION,
            owner: [0x01; 32],
            expires: u64::MAX,
            sealed: vec![0xEE],
            claimed: false,
            price_tier: PRICE_TIER_5,
            generation: [0x99; 32],
            subname_sale: Some(SubnameSale { price: 100_000, payout: BOB }),
        };
        assert_eq!(
            hex(to_bytes(&rec)),
            format!(
                "07{}ffffffffffffffff01000000ee0000{}01a0860100000000000000000000000000{}",
                "01".repeat(32),
                "99".repeat(32),
                "02".repeat(32),
            )
        );
        let sub_perm = SubnameRecord {
            version: VERSION,
            owner: [0x02; 32],
            kind: SubnameKind::Permanent,
            sealed: vec![0xEE],
        };
        assert_eq!(
            hex(to_bytes(&sub_perm)),
            format!("07{}0001000000ee", "02".repeat(32))
        );
        let sub_rev = SubnameRecord {
            version: VERSION,
            owner: [0x02; 32],
            kind: SubnameKind::Revocable { parent_generation: [0x99; 32] },
            sealed: Vec::new(),
        };
        assert_eq!(
            hex(to_bytes(&sub_rev)),
            format!("07{}01{}00000000", "02".repeat(32), "99".repeat(32))
        );
    }

    const RECORD_VECTOR: &str = "7bwwvMLtxqqNYiXsd5bYaq8e31E1odYf6jAK5z7tmUdd";
    const PAGE_VECTOR: &str = "8M4vmdxCXL4eS8QXiHSdbeUUPt5Vbx1UVjLcv2cPk9Y5";

    #[test]
    fn events_carry_the_identifier_and_never_a_name() {
        let s = slot(&JEF);
        let events = run_plan_events(
            on_registration(&JEF, ALICE),
            register(JEF, Vec::new()),
        );
        assert_eq!(events.len(), 1);
        let registered = &events[0];
        assert_eq!(registered.selector, event_selector(Registered::SELECTOR_NAME));
        assert_eq!(
            borsh::from_slice::<Registered>(&registered.data).expect("decodes"),
            Registered {
                id: s,
                expires: NOW + TERM_LENGTHS_MS[1],
            }
        );

        let events = run_plan_events(
            on_record_and_page(&JEF, ALICE),
            Instruction::Transfer {
                id: s,
                new_owner: owner_tag(&s, &BOB),
            },
        );
        assert_eq!(events[0].selector, event_selector(Transferred::SELECTOR_NAME));

        let events = run_plan_events(
            on_record_and_page(&JEF, ALICE),
            Instruction::Release { id: s, refund: 0 },
        );
        assert_eq!(events[0].selector, event_selector(Released::SELECTOR_NAME));

        // Changing a profile or a page says nothing at all.
        assert!(run_plan_events(
            on_record(&JEF, ALICE),
            Instruction::SetRecord { id: s, sealed: vec![1] },
        )
        .is_empty());
    }

    #[test]
    fn no_effect_or_event_names_a_holder() {
        // What a private transaction publishes is its effects on public
        // accounts and its events: neither may carry the holder's account.
        let holder = ALICE.into_value();
        let carries_holder = |bytes: &[u8]| bytes.windows(32).any(|window| window == holder);
        let s = slot(&JEF);
        let instructions = [
            (on_registration(&JEF, ALICE), register(JEF, vec![9])),
            (on_record(&JEF, ALICE), Instruction::SetRecord { id: s, sealed: vec![9] }),
            (on_record_and_page(&JEF, ALICE), Instruction::SetPage { id: s, sealed: vec![9] }),
            (
                on_record_and_page(&JEF, ALICE),
                Instruction::Transfer { id: s, new_owner: owner_tag(&s, &BOB) },
            ),
            (on_record_and_page(&JEF, ALICE), Instruction::Release { id: s, refund: 0 }),
            (
                vec![meta(ALICE, true)],
                Instruction::SetSubnameSale {
                    id: s,
                    sale: Some(SubnameSale { price: 10, payout: BOB }),
                },
            ),
            (
                vec![meta(ALICE, true)],
                Instruction::CreateSubname {
                    parent_id: s,
                    sub_id: BLOG_SUB,
                    owner: owner_tag(&BLOG_SUB, &BOB),
                    revocable: true,
                    sealed: vec![9],
                    now: NOW,
                },
            ),
            (
                vec![meta(ALICE, true)],
                Instruction::RevokeSubname {
                    parent_id: s,
                    sub_id: BLOG_SUB,
                    now: NOW,
                },
            ),
            (
                vec![native(ALICE)],
                Instruction::BuySubname {
                    parent_id: s,
                    sub_id: BLOG_SUB,
                    sealed: vec![9],
                    price: 10,
                    payout: Some(BOB),
                    now: NOW,
                    secret: SECRET,
                },
            ),
            (
                vec![meta(ALICE, true)],
                Instruction::SetSubnameRecord {
                    parent_id: s,
                    sub_id: BLOG_SUB,
                    sealed: vec![9],
                    now: Some(NOW),
                },
            ),
            (
                vec![meta(ALICE, true)],
                Instruction::SetSubnamePage {
                    parent_id: s,
                    sub_id: BLOG_SUB,
                    sealed: vec![9],
                    now: None,
                },
            ),
            (
                vec![meta(ALICE, true)],
                Instruction::TransferSubname {
                    parent_id: s,
                    sub_id: BLOG_SUB,
                    new_owner: owner_tag(&BLOG_SUB, &BOB),
                    now: Some(NOW),
                },
            ),
            (
                vec![meta(ALICE, true)],
                Instruction::ReleaseSubname {
                    parent_id: s,
                    sub_id: BLOG_SUB,
                    now: Some(NOW),
                },
            ),
        ];
        for (accounts, instruction) in instructions {
            let output = plan(&input(accounts, &instruction), instruction);
            let output = output.output();
            for effect in &output.effects {
                assert!(!carries_holder(&effect.data), "an effect carries the holder");
            }
            for event in &output.events {
                assert!(!carries_holder(&event.data), "an event carries the holder");
            }
        }
        // The same holder gets an unrelated tag for each name.
        assert_ne!(owner_tag(&JEF, &ALICE), owner_tag(&ADA, &ALICE));
    }

    #[test]
    fn a_name_past_its_grace_period_can_be_registered_by_anyone() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"alice's profile");
        let s = slot(&JEF);
        chain.run(
            on_record_and_page(&JEF, ALICE),
            Instruction::SetPage {
                id: s,
                sealed: b"alice's page".to_vec(),
            },
        );
        let later = NOW + YEAR + GRACE_MS;
        chain.run(
            on_registration(&JEF, BOB),
            register_at(JEF, b"bob's profile".to_vec(), 0, 0, later),
        );
        let bob_tag = owner_tag(&s, &BOB);
        assert_eq!(
            chain.record(&JEF),
            Some(NameRecord {
                version: VERSION,
                owner: bob_tag,
                expires: later + MONTH,
                sealed: b"bob's profile".to_vec(),
                claimed: false,
                price_tier: DEFAULT_TIER,
                generation: register_generation(&s, &bob_tag, later, &SECRET),
                subname_sale: None,
            })
        );
        assert_eq!(chain.page(&JEF), None);
    }

    #[test]
    #[should_panic(expected = "name is already registered")]
    fn a_name_in_its_grace_period_cannot_be_registered() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"");
        chain.run(
            on_registration(&JEF, BOB),
            register_at(JEF, Vec::new(), 1, 0, NOW + YEAR + GRACE_MS - 1),
        );
    }

    #[test]
    fn a_renewal_extends_from_the_later_of_the_expiry_and_now() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"profile");
        chain.renew(&JEF, ALICE, NOW + 1);
        let record = chain.record(&JEF).expect("registered");
        assert_eq!(record.expires, NOW + YEAR + MONTH);
        assert_eq!(record.sealed, b"profile");

        // In grace, and paid for by someone else: the name stays its owner's.
        let late = record.expires + GRACE_MS - 1;
        chain.renew(&JEF, BOB, late);
        let record = chain.record(&JEF).expect("registered");
        assert_eq!((record.owner, record.expires), (owner_tag(&slot(&JEF), &ALICE), late + MONTH));
    }

    #[test]
    #[should_panic(expected = "name has lapsed")]
    fn a_name_past_its_grace_period_cannot_be_renewed() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"");
        chain.renew(&JEF, ALICE, NOW + YEAR + GRACE_MS);
    }

    #[test]
    #[should_panic(expected = "name is not registered")]
    fn an_unregistered_name_cannot_be_renewed() {
        Chain::default().renew(&JEF, ALICE, NOW);
    }

    #[test]
    #[should_panic(expected = "term must be 0, 1, 2 or 3")]
    fn an_unknown_term_is_refused() {
        Chain::default().run(
            on_registration(&JEF, ALICE),
            register_at(JEF, Vec::new(), 4, 0, NOW),
        );
    }

    #[test]
    #[should_panic(expected = "the fee is not the price of this term")]
    fn an_unconfigured_registry_takes_no_fee() {
        Chain::default().run(
            paying(on_registration(&JEF, ALICE), BOB),
            register_at(JEF, Vec::new(), 1, 5, NOW),
        );
    }

    #[test]
    fn a_configured_registry_charges_the_price_of_the_term() {
        let mut chain = Chain::default();
        chain.configure(ALICE, ALICE, PRICES);
        let row = PRICES[usize::from(DEFAULT_TIER)];
        chain.run(
            paying(on_registration(&JEF, ALICE), BOB),
            register_at(JEF, Vec::new(), TERM_1Y, row[usize::from(TERM_1Y)], NOW),
        );
        chain.run(
            paying(on_renewal(&JEF, CAROL), BOB),
            Instruction::Renew {
                id: slot(&JEF),
                term: TERM_5Y,
                fee: row[usize::from(TERM_5Y)],
                treasury: None,
                now: NOW,
            },
        );
        let record = chain.record(&JEF).expect("registered");
        assert_eq!(
            (record.owner, record.expires),
            (owner_tag(&slot(&JEF), &ALICE), NOW + YEAR + 5 * YEAR)
        );
    }

    #[test]
    fn length_tiers_and_permanent_term_are_priced_from_the_table() {
        let mut chain = Chain::default();
        chain.configure(ALICE, ALICE, PRICES);

        // 5-char tier (row 0), 1 year: 20M
        let id5: NameId = [0x51; 32];
        chain.run(
            paying(on_registration_tier(&id5, PRICE_TIER_5, ALICE), BOB),
            register_tier_at(id5, b"5char".to_vec(), TERM_1Y, 20_000_000, NOW, PRICE_TIER_5),
        );
        assert_eq!(
            chain.record_slot(&slot_of(&id5, PRICE_TIER_5)).expect("5char").expires,
            NOW + YEAR
        );

        // Upgrading that 5-char name to permanent via Renew charges row 0 col 3 (400M)
        // and sets expires = u64::MAX.
        chain.run(
            paying(on_renewal_slot(&slot_of(&id5, PRICE_TIER_5), ALICE), BOB),
            Instruction::Renew {
                id: slot_of(&id5, PRICE_TIER_5),
                term: TERM_PERMANENT,
                fee: 400_000_000,
                treasury: None,
                now: NOW,
            },
        );
        assert_eq!(
            chain.record_slot(&slot_of(&id5, PRICE_TIER_5)).expect("perm").expires,
            u64::MAX
        );

        // 6-7-char tier (row 1), permanent directly on Register: 100M
        let id6: NameId = [0x61; 32];
        chain.run(
            paying(on_registration_tier(&id6, PRICE_TIER_6_7, ALICE), BOB),
            register_tier_at(id6, b"6char".to_vec(), TERM_PERMANENT, 100_000_000, NOW, PRICE_TIER_6_7),
        );
        let rec6 = chain.record_slot(&slot_of(&id6, PRICE_TIER_6_7)).expect("6char");
        assert_eq!((rec6.expires, rec6.price_tier), (u64::MAX, PRICE_TIER_6_7));

        // 8+-char tier (row 2), 30 days: 100k
        let id8: NameId = [0x81; 32];
        chain.run(
            paying(on_registration_tier(&id8, PRICE_TIER_8_PLUS, ALICE), BOB),
            register_tier_at(id8, b"8char".to_vec(), TERM_30D, 100_000, NOW, PRICE_TIER_8_PLUS),
        );
        assert_eq!(
            chain.record_slot(&slot_of(&id8, PRICE_TIER_8_PLUS)).expect("8char").expires,
            NOW + MONTH
        );
    }

    #[test]
    fn paying_a_cheaper_tier_cannot_occupy_a_shorter_names_slot() {
        let mut chain = Chain::default();
        chain.configure(ALICE, ALICE, PRICES);
        let five_char_id: NameId = [0x55; 32];

        // Attacker registers `five_char_id` claiming 8+ tier for 1M.
        chain.run(
            paying(on_registration_tier(&five_char_id, PRICE_TIER_8_PLUS, CAROL), BOB),
            register_tier_at(five_char_id, b"spoof".to_vec(), TERM_1Y, 1_000_000, NOW, PRICE_TIER_8_PLUS),
        );
        // The real 5-char slot is untouched and still unregistered.
        assert_eq!(chain.record_slot(&slot_of(&five_char_id, PRICE_TIER_5)), None);

        // Legitimate buyer registers the real 5-char slot for 20M without interference.
        chain.run(
            paying(on_registration_tier(&five_char_id, PRICE_TIER_5, ALICE), BOB),
            register_tier_at(five_char_id, b"real".to_vec(), TERM_1Y, 20_000_000, NOW, PRICE_TIER_5),
        );
        assert_eq!(
            chain.record_slot(&slot_of(&five_char_id, PRICE_TIER_5)).expect("real").sealed,
            b"real"
        );
    }

    #[test]
    fn admin_updates_one_price_cell_affecting_only_new_purchases_and_renewals() {
        let mut chain = Chain::default();
        chain.configure(ALICE, ALICE, PRICES);

        // Register 5-char name at old 1y price (20M).
        let id5: NameId = [0x52; 32];
        chain.run(
            paying(on_registration_tier(&id5, PRICE_TIER_5, CAROL), BOB),
            register_tier_at(id5, b"old".to_vec(), TERM_1Y, 20_000_000, NOW, PRICE_TIER_5),
        );

        // Admin raises 5-char 1y price to 25M via SetPrice.
        chain.run(
            vec![meta(settings_address(&PROGRAM), false), meta(ALICE, true)],
            Instruction::SetPrice {
                price_tier: PRICE_TIER_5,
                term: TERM_1Y,
                price: 25_000_000,
            },
        );
        assert_eq!(chain.settings().expect("settings").prices[0][1], 25_000_000);
        // Other cells are unchanged.
        assert_eq!(chain.settings().expect("settings").prices[0][3], 400_000_000);
        // Existing name keeps its expiry untouched.
        assert_eq!(
            chain.record_slot(&slot_of(&id5, PRICE_TIER_5)).expect("held").expires,
            NOW + YEAR
        );
        // Renewing it now charges the new 25M price.
        chain.run(
            paying(on_renewal_slot(&slot_of(&id5, PRICE_TIER_5), CAROL), BOB),
            Instruction::Renew {
                id: slot_of(&id5, PRICE_TIER_5),
                term: TERM_1Y,
                fee: 25_000_000,
                treasury: None,
                now: NOW,
            },
        );
        assert_eq!(
            chain.record_slot(&slot_of(&id5, PRICE_TIER_5)).expect("renewed").expires,
            NOW + 2 * YEAR
        );
    }

    #[test]
    #[should_panic(expected = "signer is not the registry's admin")]
    fn non_admin_cannot_set_price() {
        let mut chain = Chain::default();
        chain.configure(ALICE, ALICE, PRICES);
        chain.run(
            vec![meta(settings_address(&PROGRAM), false), meta(CAROL, true)],
            Instruction::SetPrice {
                price_tier: PRICE_TIER_5,
                term: TERM_1Y,
                price: 1,
            },
        );
    }

    #[test]
    #[should_panic(expected = "a permanent or deposit-held name does not lapse or renew")]
    fn a_permanent_fee_name_cannot_be_renewed() {
        let mut chain = Chain::default();
        chain.configure(ALICE, ALICE, PRICES);
        let perm_price = PRICES[usize::from(DEFAULT_TIER)][usize::from(TERM_PERMANENT)];
        chain.run(
            paying(on_registration(&JEF, ALICE), BOB),
            register_at(JEF, Vec::new(), TERM_PERMANENT, perm_price, NOW),
        );
        chain.run(
            paying(on_renewal(&JEF, ALICE), BOB),
            Instruction::Renew {
                id: slot(&JEF),
                term: TERM_1Y,
                fee: PRICES[usize::from(DEFAULT_TIER)][usize::from(TERM_1Y)],
                treasury: None,
                now: NOW,
            },
        );
    }

    #[test]
    #[should_panic(expected = "the fee is not the price of this term")]
    fn a_wrong_fee_is_refused() {
        let mut chain = Chain::default();
        chain.configure(ALICE, ALICE, PRICES);
        let row = PRICES[usize::from(DEFAULT_TIER)];
        chain.run(
            paying(on_registration(&JEF, ALICE), BOB),
            register_at(JEF, Vec::new(), TERM_1Y, row[usize::from(TERM_30D)], NOW),
        );
    }

    #[test]
    #[should_panic(expected = "the fee is not the price of this term")]
    fn a_configured_registry_is_not_free() {
        let mut chain = Chain::default();
        chain.configure(ALICE, ALICE, PRICES);
        chain.register(&JEF, ALICE, b"");
    }

    #[test]
    #[should_panic(expected = "the treasury account is not the registry's treasury")]
    fn a_fee_paid_to_another_account_is_refused() {
        let mut chain = Chain::default();
        chain.configure(ALICE, ALICE, PRICES);
        let row = PRICES[usize::from(DEFAULT_TIER)];
        chain.run(
            paying(on_registration(&JEF, ALICE), CAROL),
            register_at(JEF, Vec::new(), TERM_1Y, row[usize::from(TERM_1Y)], NOW),
        );
    }

    #[test]
    fn the_treasury_pays_nothing() {
        let mut chain = Chain::default();
        chain.configure(ALICE, ALICE, PRICES);
        chain.register(&JEF, BOB, b"");
        assert_eq!(chain.record(&JEF).expect("registered").owner, owner_tag(&slot(&JEF), &BOB));
    }

    #[test]
    #[should_panic(expected = "the fee is not the price of this term")]
    fn the_treasury_cannot_pay_itself_a_fee() {
        let mut chain = Chain::default();
        chain.configure(ALICE, ALICE, PRICES);
        let row = PRICES[usize::from(DEFAULT_TIER)];
        chain.run(
            paying(on_registration(&JEF, BOB), BOB),
            register_at(JEF, Vec::new(), TERM_1Y, row[usize::from(TERM_1Y)], NOW),
        );
    }

    #[test]
    #[should_panic(expected = "the first settings must be signed by the program's own account")]
    fn nobody_but_the_deployer_configures_a_fresh_registry() {
        Chain::default().run(
            vec![meta(settings_address(&PROGRAM), false), meta(CAROL, true)],
            Instruction::Configure {
                admin: CAROL,
                treasury: CAROL,
                prices: PRICES,
                mode: MODE_FEE,
                deposits: [0, 0],
                claims_root: [0; 32],
                claims_until: 0,
                claims_program: PROGRAM,
            },
        );
    }

    #[test]
    fn a_name_is_claimed_by_commit_then_reveal() {
        let mut chain = Chain::default();
        chain.commit(&JEF, ALICE, NOW - MIN_COMMIT_AGE_MS);
        chain.reveal(&JEF, ALICE, NOW);
        assert_eq!(chain.record(&JEF).map(|r| r.owner), Some(owner_tag(&slot(&JEF), &ALICE)));
        assert!(chain.data(&committed(&JEF, ALICE)).is_empty(), "revealing uses the commitment up");
    }

    #[test]
    #[should_panic(expected = "the commitment is too recent")]
    fn a_reveal_waits_for_its_commitment_to_age() {
        let mut chain = Chain::default();
        chain.commit(&JEF, ALICE, NOW);
        chain.reveal(&JEF, ALICE, NOW + MIN_COMMIT_AGE_MS - 1);
    }

    #[test]
    #[should_panic(expected = "the commitment has expired")]
    fn an_old_commitment_cannot_be_revealed() {
        let mut chain = Chain::default();
        chain.commit(&JEF, ALICE, NOW);
        chain.reveal(&JEF, ALICE, NOW + MAX_COMMIT_AGE_MS);
    }

    #[test]
    #[should_panic(expected = "no commitment to reveal")]
    fn a_reveal_without_a_commitment_is_refused() {
        Chain::default().reveal(&JEF, ALICE, NOW);
    }

    /// A front-runner who sees Alice's reveal pending knows `id` and `secret`, but
    /// the commitment is bound to Alice, so it can't claim the name with it.
    #[test]
    #[should_panic(expected = "no commitment to reveal")]
    fn a_seen_reveal_cannot_be_replayed_for_another_account() {
        let mut chain = Chain::default();
        chain.commit(&JEF, ALICE, NOW - MIN_COMMIT_AGE_MS);
        chain.run_exactly(on_registration(&JEF, BOB), register_at(JEF, Vec::new(), 1, 0, NOW));
    }

    #[test]
    #[should_panic(expected = "this commitment was already made")]
    fn a_live_commitment_cannot_be_reset() {
        let mut chain = Chain::default();
        chain.commit(&JEF, ALICE, NOW - MIN_COMMIT_AGE_MS);
        chain.commit(&JEF, ALICE, NOW);
    }

    #[test]
    fn commit_and_reveal_are_valid_only_near_their_claimed_time() {
        let s = slot(&JEF);
        let promised = commitment(&s, &owner_tag(&s, &ALICE), &SECRET);
        for instruction in [
            Instruction::Commit { commitment: promised, now: NOW },
            register_at(JEF, Vec::new(), 1, 0, NOW),
        ] {
            let accounts = match &instruction {
                Instruction::Commit { .. } => {
                    vec![meta(commitment_address(&PROGRAM, &promised), false), meta(CAROL, true)]
                }
                _ => on_registration(&JEF, ALICE),
            };
            let window = plan(&input(accounts, &instruction), instruction).output().timestamp_validity_window;
            assert_eq!((window.start(), window.end()), (Some(NOW), Some(NOW + TIME_WINDOW_MS)));
        }
    }

    #[test]
    fn a_public_name_is_claimed_after_the_public_wait() {
        let mut chain = Chain::default();
        chain.commit_public(&JEF, ALICE, NOW - PUBLIC_MIN_COMMIT_AGE_MS);
        chain.reveal(&JEF, ALICE, NOW);
        assert_eq!(chain.record(&JEF).map(|r| r.owner), Some(owner_tag(&slot(&JEF), &ALICE)));
        assert!(chain.data(&committed(&JEF, ALICE)).is_empty(), "revealing uses the commitment up");
    }

    #[test]
    #[should_panic(expected = "the commitment is too recent")]
    fn a_public_reveal_before_the_public_wait_is_refused() {
        let mut chain = Chain::default();
        chain.commit_public(&JEF, ALICE, NOW);
        chain.reveal(&JEF, ALICE, NOW + PUBLIC_MIN_COMMIT_AGE_MS - 1);
    }

    #[test]
    fn a_public_commit_and_its_reveal_use_the_short_window() {
        let s = slot(&JEF);
        let promised = commitment(&s, &owner_tag(&s, &ALICE), &SECRET);
        let commit_pub = Instruction::CommitPublic { commitment: promised, now: NOW };
        let accounts = vec![meta(commitment_address(&PROGRAM, &promised), false), meta(CAROL, true)];
        let window = plan(&input(accounts, &commit_pub), commit_pub).output().timestamp_validity_window;
        assert_eq!((window.start(), window.end()), (Some(NOW), Some(NOW + PUBLIC_TIME_WINDOW_MS)));

        let reveal_window = |public: bool, made: u64, now: u64| {
            let receive = ReceiveInput {
                receiver: Actor::new(committed(&JEF, ALICE), PROGRAM),
                from: Some(Actor::new(ALICE, PROGRAM)),
                grants: Default::default(),
                pre_state: to_bytes(&Commitment { version: VERSION, made, public }).into(),
                message: Vec::new(),
            };
            handle(&receive, Message::Effect(Effect::Reveal { now })).validity.timestamps
        };

        let pub_reveal = NOW + PUBLIC_MIN_COMMIT_AGE_MS;
        let w = reveal_window(true, NOW, pub_reveal);
        assert_eq!((w.start(), w.end()), (Some(pub_reveal), Some(pub_reveal + PUBLIC_TIME_WINDOW_MS)));

        let priv_reveal = NOW + MIN_COMMIT_AGE_MS;
        let w = reveal_window(false, NOW, priv_reveal);
        assert_eq!((w.start(), w.end()), (Some(priv_reveal), Some(priv_reveal + TIME_WINDOW_MS)));
    }

    /// A commitment made under the 10-minute private clock window cannot be
    /// revealed after the shorter public wait: otherwise an attacker could
    /// backdate a private `Commit` by 10 minutes and reveal immediately.
    #[test]
    #[should_panic(expected = "the commitment is too recent")]
    fn a_private_commit_cannot_use_the_public_wait() {
        let mut chain = Chain::default();
        chain.commit(&JEF, BOB, NOW - TIME_WINDOW_MS + 1);
        chain.reveal(&JEF, BOB, NOW);
    }

    /// Alice commits privately and broadcasts her reveal at real time `NOW`.
    /// Bob sees Alice's pending reveal at `NOW` and submits a `CommitPublic`
    /// maximally backdated within `PUBLIC_TIME_WINDOW_MS`. Even so, Bob cannot
    /// reveal before four real minutes have passed, and Alice's reveal wins.
    #[test]
    fn a_public_attacker_racing_a_pending_private_reveal_still_waits_four_real_minutes() {
        let mut chain = Chain::default();
        chain.commit(&JEF, ALICE, NOW - MIN_COMMIT_AGE_MS);

        let bob_made = NOW - PUBLIC_TIME_WINDOW_MS + 1;
        chain.commit_public(&JEF, BOB, bob_made);

        // A public commit's claimed time is at most one minute early, so its
        // five-minute wait is at least four real minutes; a private commit's is
        // at most ten minutes early, so its fifteen-minute wait is at least five.
        const REAL_WAIT_MS: u64 = 4 * 60 * 1000;
        assert_eq!(PUBLIC_MIN_COMMIT_AGE_MS - PUBLIC_TIME_WINDOW_MS, REAL_WAIT_MS);
        assert_eq!(MIN_COMMIT_AGE_MS - TIME_WINDOW_MS, 5 * 60 * 1000);

        let mut bob_early = Chain { actors: chain.actors.clone() };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            bob_early.reveal(&JEF, BOB, NOW + REAL_WAIT_MS);
        }));
        assert!(result.is_err(), "a public attacker cannot beat the four-minute real wait");

        chain.reveal(&JEF, ALICE, NOW);
        assert_eq!(chain.record(&JEF).map(|r| r.owner), Some(owner_tag(&slot(&JEF), &ALICE)));

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            chain.reveal(&JEF, BOB, NOW + REAL_WAIT_MS + 1);
        }));
        assert!(result.is_err(), "Bob's later reveal cannot overwrite Alice's registration");
    }

    #[test]
    #[should_panic(expected = "this commitment was already made")]
    fn a_live_private_commitment_cannot_be_overwritten_as_public() {
        let mut chain = Chain::default();
        chain.commit(&JEF, ALICE, NOW - MIN_COMMIT_AGE_MS);
        chain.commit_public(&JEF, ALICE, NOW);
    }

    #[test]
    fn the_deployer_configures_a_fresh_registry_and_then_only_its_admin() {
        let mut chain = Chain::default();
        assert_eq!(chain.settings(), None);
        chain.configure(CAROL, ALICE, PRICES);
        let custom = [[1, 2, 3, 4], [5, 6, 7, 8], [9, 10, 11, 12]];
        chain.configure(ALICE, ALICE, custom);
        assert_eq!(
            chain.settings(),
            Some(Settings {
                version: VERSION,
                admin: ALICE,
                treasury: BOB,
                prices: custom,
                mode: MODE_FEE,
                deposits: [0, 0],
                claims_root: [0; 32],
                claims_until: 0,
                claims_program: PROGRAM,
            })
        );
    }

    #[test]
    #[should_panic(expected = "signer is not the registry's admin")]
    fn someone_else_cannot_configure_a_configured_registry() {
        let mut chain = Chain::default();
        chain.configure(CAROL, ALICE, PRICES);
        chain.configure(CAROL, CAROL, PRICES);
    }

    #[test]
    #[should_panic(expected = "the signer's account must sign")]
    fn configuring_needs_a_signature() {
        Chain::default().run(
            vec![meta(settings_address(&PROGRAM), false), meta(ALICE, false)],
            Instruction::Configure {
                admin: ALICE,
                treasury: BOB,
                prices: PRICES,
                mode: MODE_FEE,
                deposits: [0, 0],
                claims_root: [0; 32],
                claims_until: 0,
                claims_program: PROGRAM,
            },
        );
    }

    #[test]
    fn a_fee_is_one_native_transfer_from_the_payer_to_the_treasury() {
        let fee = PRICES[usize::from(DEFAULT_TIER)][usize::from(TERM_1Y)];
        let instruction = register_at(JEF, Vec::new(), TERM_1Y, fee, NOW);
        let planned = plan(
            &input(paying(on_registration(&JEF, ALICE), BOB), &instruction),
            instruction,
        );
        let output = planned.output();
        let [call] = output.chained_calls.as_slice() else {
            panic!("expected one transfer");
        };
        assert!(call.to == Actor::native_balance(ALICE));
        assert!(call.grants.contains(&Actor::native_balance(ALICE)));
        assert_eq!(
            call.message,
            borsh::to_vec(&native_token::Message::Transfer { to: BOB, amount: fee })
                .expect("the transfer serializes")
        );
        let window = output.timestamp_validity_window;
        assert!(window.is_valid_for(NOW) && !window.is_valid_for(NOW - 1));

        let instruction = register(JEF, Vec::new());
        let planned = plan(&input(on_registration(&JEF, ALICE), &instruction), instruction);
        assert!(planned.output().chained_calls.is_empty());
    }

    const DEPOSITS: [u128; 2] = [100_000_000, 2_000_000_000];

    impl Chain {
        /// A registry in deposit mode, administered by `admin`.
        fn configure_deposits(&mut self, admin: AccountId) {
            self.run(
                vec![meta(settings_address(&PROGRAM), false), meta(PROGRAM, true)],
                Instruction::Configure {
                    admin,
                    treasury: BOB,
                    prices: PRICES,
                    mode: MODE_DEPOSIT,
                    deposits: DEPOSITS,
                    claims_root: [0; 32],
                    claims_until: 0,
                    claims_program: PROGRAM,
                },
            );
        }

        /// A deposit-mode registration of `id` for `holder`, paying `deposit`.
        fn register_with_deposit(&mut self, id: &NameId, holder: AccountId, deposit: u128, now: u64) {
            let mut accounts = on_registration(id, holder);
            if deposit > 0 {
                accounts.push(AccountMeta::native_balance(
                    deposit_address(&PROGRAM, &slot(id)),
                    false,
                ));
            }
            self.run(accounts, deposit_registration(*id, deposit, now));
        }
    }

    fn deposit_registration(id: NameId, deposit: u128, now: u64) -> Instruction {
        Instruction::Register {
            id,
            sealed: Vec::new(),
            term: 0,
            fee: deposit,
            treasury: None,
            now,
            secret: SECRET,
            deposit: true,
            length: length_of(&id),
            price_tier: DEFAULT_TIER,
        }
    }

    /// The `[record, page, owner]` accounts of a release, with the owner's native
    /// actor, and the deposit account when something is refunded.
    fn on_release(base_id: &NameId, owner: AccountId, refund: u128) -> Vec<AccountMeta> {
        let s = slot(base_id);
        let mut accounts = vec![
            meta(record_address(&PROGRAM, &s), false),
            meta(page_address(&PROGRAM, &s), false),
            AccountMeta::native_balance(owner, true),
        ];
        if refund > 0 {
            accounts.push(AccountMeta::native_balance(deposit_address(&PROGRAM, &s), false));
        }
        accounts
    }

    fn only_call(accounts: Vec<AccountMeta>, instruction: Instruction) -> SyncMessage {
        let planned = plan(&input(accounts, &instruction), instruction);
        let [call] = planned.output().chained_calls.as_slice() else {
            panic!("expected one chained call");
        };
        call.clone()
    }

    #[test]
    fn a_deposit_moves_from_the_payer_into_the_names_deposit_account() {
        let s = slot(&JEF);
        let mut accounts = on_registration(&JEF, ALICE);
        accounts.push(AccountMeta::native_balance(deposit_address(&PROGRAM, &s), false));
        let call = only_call(accounts, deposit_registration(JEF, DEPOSITS[0], NOW));
        assert!(call.to == Actor::native_balance(ALICE));
        let to = deposit_address(&PROGRAM, &s);
        assert_eq!(
            call.message,
            borsh::to_vec(&native_token::Message::Transfer { to, amount: DEPOSITS[0] }).expect("serializes")
        );
        assert!(call.pda_seeds.is_empty(), "the payer's own signature authorizes a deposit");
    }

    #[test]
    fn a_name_held_by_a_deposit_never_lapses() {
        let mut chain = Chain::default();
        chain.configure_deposits(CAROL);
        chain.register_with_deposit(&JEF, ALICE, DEPOSITS[0], NOW);
        let record = chain.record(&JEF).expect("registered");
        assert_eq!(record.expires, u64::MAX);
        assert_eq!(record.owner, owner_tag(&slot(&JEF), &ALICE));
        assert_eq!(record.version, VERSION);
    }

    #[test]
    #[should_panic(expected = "name is already registered")]
    fn nobody_takes_a_deposit_held_name_even_decades_later() {
        let mut chain = Chain::default();
        chain.configure_deposits(CAROL);
        chain.register_with_deposit(&JEF, ALICE, DEPOSITS[0], NOW);
        chain.register_with_deposit(&JEF, BOB, DEPOSITS[0], NOW + 50 * YEAR);
    }

    #[test]
    #[should_panic(expected = "the deposit is not this name's deposit")]
    fn a_short_deposit_is_refused() {
        let mut chain = Chain::default();
        chain.configure_deposits(CAROL);
        chain.register_with_deposit(&JEF, ALICE, DEPOSITS[0] - 1, NOW);
    }

    #[test]
    #[should_panic(expected = "the deposit is not this name's deposit")]
    fn no_deposit_is_refused_in_deposit_mode() {
        let mut chain = Chain::default();
        chain.configure_deposits(CAROL);
        chain.register_with_deposit(&JEF, ALICE, 0, NOW);
    }

    #[test]
    #[should_panic(expected = "the registry takes deposits")]
    fn a_fee_registration_is_refused_in_deposit_mode() {
        let mut chain = Chain::default();
        chain.configure_deposits(CAROL);
        let fee = PRICES[usize::from(DEFAULT_TIER)][usize::from(TERM_1Y)];
        chain.run(
            paying(on_registration(&JEF, ALICE), BOB),
            register_at(JEF, Vec::new(), TERM_1Y, fee, NOW),
        );
    }

    #[test]
    #[should_panic(expected = "the registry charges fees, not deposits")]
    fn a_deposit_registration_is_refused_in_fee_mode() {
        let mut chain = Chain::default();
        chain.configure(PROGRAM, ALICE, PRICES);
        chain.register_with_deposit(&JEF, ALICE, DEPOSITS[0], NOW);
    }

    #[test]
    #[should_panic(expected = "a permanent or deposit-held name does not lapse or renew")]
    fn names_do_not_renew_in_deposit_mode() {
        let mut chain = Chain::default();
        chain.configure_deposits(CAROL);
        chain.register_with_deposit(&JEF, ALICE, DEPOSITS[0], NOW);
        let fee = PRICES[usize::from(DEFAULT_TIER)][usize::from(TERM_1Y)];
        chain.run(
            paying(on_renewal(&JEF, ALICE), BOB),
            Instruction::Renew { id: slot(&JEF), term: TERM_1Y, fee, treasury: None, now: NOW },
        );
    }

    #[test]
    #[should_panic(expected = "the registry takes deposits")]
    fn a_fee_name_cannot_be_renewed_after_switching_to_deposit_mode() {
        let mut chain = Chain::default();
        chain.configure(PROGRAM, CAROL, PRICES);
        let fee = PRICES[usize::from(DEFAULT_TIER)][usize::from(TERM_1Y)];
        chain.run(
            paying(on_registration(&JEF, ALICE), BOB),
            register_at(JEF, Vec::new(), TERM_1Y, fee, NOW),
        );
        chain.run(
            vec![meta(settings_address(&PROGRAM), false), meta(CAROL, true)],
            Instruction::Configure {
                admin: CAROL,
                treasury: BOB,
                prices: PRICES,
                mode: MODE_DEPOSIT,
                deposits: DEPOSITS,
                claims_root: [0; 32],
                claims_until: 0,
                claims_program: PROGRAM,
            },
        );
        chain.run(
            paying(on_renewal(&JEF, ALICE), BOB),
            Instruction::Renew { id: slot(&JEF), term: TERM_1Y, fee, treasury: None, now: NOW },
        );
    }

    #[test]
    #[should_panic(expected = "a permanent or deposit-held name does not lapse or renew")]
    fn a_deposit_name_cannot_be_renewed_even_after_a_switch_to_fees() {
        let mut chain = Chain::default();
        chain.configure_deposits(CAROL);
        chain.register_with_deposit(&JEF, ALICE, DEPOSITS[0], NOW);
        chain.configure(CAROL, CAROL, PRICES);
        let fee = PRICES[usize::from(DEFAULT_TIER)][usize::from(TERM_1Y)];
        chain.run(
            paying(on_renewal(&JEF, ALICE), BOB),
            Instruction::Renew { id: slot(&JEF), term: TERM_1Y, fee, treasury: None, now: NOW },
        );
    }

    #[test]
    fn a_release_refunds_the_deposit_to_the_holder_from_the_names_account() {
        let s = slot(&JEF);
        let call = only_call(
            on_release(&JEF, ALICE, DEPOSITS[0]),
            Instruction::Release { id: s, refund: DEPOSITS[0] },
        );
        assert!(call.to == Actor::native_balance(deposit_address(&PROGRAM, &s)));
        assert_eq!(
            call.message,
            borsh::to_vec(&native_token::Message::Transfer { to: ALICE, amount: DEPOSITS[0] }).expect("serializes")
        );
        // Only the registry can sign for the deposit account.
        assert!(call.pda_seeds.iter().eq([&seed(DEPOSIT_SEED_DOMAIN, &s)]));
    }

    #[test]
    #[should_panic(expected = "signer does not own this name")]
    fn only_the_holder_collects_the_deposit() {
        let mut chain = Chain::default();
        chain.configure_deposits(CAROL);
        chain.register_with_deposit(&JEF, ALICE, DEPOSITS[0], NOW);
        chain.run(
            on_release(&JEF, BOB, DEPOSITS[0]),
            Instruction::Release { id: slot(&JEF), refund: DEPOSITS[0] },
        );
    }

    #[test]
    fn a_transferred_name_keeps_its_deposit_for_the_new_holder() {
        let mut chain = Chain::default();
        chain.configure_deposits(CAROL);
        chain.register_with_deposit(&JEF, ALICE, DEPOSITS[0], NOW);
        let s = slot(&JEF);
        chain.run(
            on_record_and_page(&JEF, ALICE),
            Instruction::Transfer { id: s, new_owner: owner_tag(&s, &BOB) },
        );
        // The new holder releases it and is refunded; the old one no longer can.
        chain.run(
            on_release(&JEF, BOB, DEPOSITS[0]),
            Instruction::Release { id: s, refund: DEPOSITS[0] },
        );
        assert!(chain.record(&JEF).is_none());
    }

    #[test]
    fn a_released_deposit_name_can_be_registered_again() {
        let mut chain = Chain::default();
        chain.configure_deposits(CAROL);
        chain.register_with_deposit(&JEF, ALICE, DEPOSITS[0], NOW);
        let s = slot(&JEF);
        chain.run(
            on_release(&JEF, ALICE, DEPOSITS[0]),
            Instruction::Release { id: s, refund: DEPOSITS[0] },
        );
        chain.register_with_deposit(&JEF, BOB, DEPOSITS[0], NOW + MONTH);
        assert_eq!(chain.record(&JEF).expect("registered").owner, owner_tag(&s, &BOB));
    }

    #[test]
    fn the_admin_switches_modes_and_changes_deposits() {
        let mut chain = Chain::default();
        chain.configure_deposits(CAROL);
        chain.run(
            vec![meta(settings_address(&PROGRAM), false), meta(CAROL, true)],
            Instruction::Configure {
                admin: CAROL,
                treasury: BOB,
                prices: PRICES,
                mode: MODE_DEPOSIT,
                deposits: [5, 50],
                claims_root: [0; 32],
                claims_until: 0,
                claims_program: PROGRAM,
            },
        );
        assert_eq!(chain.settings().expect("configured").deposits, [5, 50]);
        chain.register_with_deposit(&JEF, ALICE, 5, NOW);
    }

    #[test]
    #[should_panic(expected = "mode must be fee (0) or deposit (1)")]
    fn an_unknown_mode_is_refused() {
        let mut chain = Chain::default();
        chain.run(
            vec![meta(settings_address(&PROGRAM), false), meta(PROGRAM, true)],
            Instruction::Configure {
                admin: CAROL,
                treasury: BOB,
                prices: PRICES,
                mode: 2,
                deposits: DEPOSITS,
                claims_root: [0; 32],
                claims_until: 0,
                claims_program: PROGRAM,
            },
        );
    }

    fn deposit_registration_with(id: NameId, deposit: u128, length: LengthProof) -> Instruction {
        Instruction::Register {
            id,
            sealed: Vec::new(),
            term: 0,
            fee: deposit,
            treasury: None,
            now: NOW,
            secret: SECRET,
            deposit: true,
            length,
            price_tier: DEFAULT_TIER,
        }
    }

    fn with_deposit_account(id: &NameId, holder: AccountId) -> Vec<AccountMeta> {
        let mut accounts = on_registration(id, holder);
        accounts.push(AccountMeta::native_balance(deposit_address(&PROGRAM, &slot(id)), false));
        accounts
    }

    #[test]
    fn a_three_character_name_takes_the_higher_deposit() {
        let mut chain = Chain::default();
        chain.configure_deposits(CAROL);
        chain.register_with_deposit(&THREE_NAME, ALICE, DEPOSITS[1], NOW);
        assert!(chain.record(&THREE_NAME).is_some());
    }

    #[test]
    #[should_panic(expected = "the deposit is not this name's deposit")]
    fn a_three_character_name_is_refused_at_the_normal_deposit() {
        let mut chain = Chain::default();
        chain.configure_deposits(CAROL);
        chain.register_with_deposit(&THREE_NAME, ALICE, DEPOSITS[0], NOW);
    }

    #[test]
    #[should_panic(expected = "the length proof does not hold")]
    fn a_short_name_cannot_pass_as_a_normal_one() {
        // A non-membership proof built around the short name's neighbours.
        let around_ada = length_of(&ADA);
        let mut chain = Chain::default();
        chain.configure_deposits(CAROL);
        chain.run(
            with_deposit_account(&THREE_NAME, ALICE),
            deposit_registration_with(THREE_NAME, DEPOSITS[0], around_ada),
        );
    }

    #[test]
    #[should_panic(expected = "the length proof does not hold")]
    fn a_short_name_cannot_borrow_another_short_names_proof() {
        let mut chain = Chain::default();
        chain.configure_deposits(CAROL);
        chain.run(
            with_deposit_account(&LOCKED_NAME, ALICE),
            deposit_registration_with(LOCKED_NAME, DEPOSITS[1], length_of(&THREE_NAME)),
        );
    }

    #[test]
    #[should_panic(expected = "only the registry's admin may register it")]
    fn a_locked_name_is_refused_to_everyone_but_the_admin() {
        let mut chain = Chain::default();
        chain.configure_deposits(CAROL);
        chain.register_with_deposit(&LOCKED_NAME, ALICE, DEPOSITS[0], NOW);
    }

    #[test]
    fn the_admin_releases_a_locked_name_by_registering_and_transferring_it() {
        let mut chain = Chain::default();
        chain.configure_deposits(CAROL);
        chain.register_with_deposit(&LOCKED_NAME, CAROL, 0, NOW);
        let s = slot(&LOCKED_NAME);
        chain.run(
            on_record_and_page(&LOCKED_NAME, CAROL),
            Instruction::Transfer { id: s, new_owner: owner_tag(&s, &ALICE) },
        );
        assert_eq!(chain.record(&LOCKED_NAME).expect("held").owner, owner_tag(&s, &ALICE));
    }

    #[test]
    #[should_panic(expected = "only the registry's admin may register it")]
    fn locked_names_are_locked_in_fee_mode_too() {
        let mut chain = Chain::default();
        chain.configure(PROGRAM, CAROL, PRICES);
        chain.run(
            on_registration(&LOCKED_NAME_2, ALICE),
            register_at(LOCKED_NAME_2, Vec::new(), 1, 0, NOW),
        );
    }

    #[test]
    #[should_panic(expected = "only the registry's admin may register it")]
    fn an_unconfigured_registry_locks_short_names() {
        let mut chain = Chain::default();
        chain.run(
            on_registration(&LOCKED_NAME, ALICE),
            register_at(LOCKED_NAME, Vec::new(), 1, 0, NOW),
        );
    }

    #[test]
    fn the_admin_places_a_name_without_a_deposit() {
        let mut chain = Chain::default();
        chain.configure_deposits(CAROL);
        chain.register_with_deposit(&THREE_NAME, CAROL, 0, NOW);
        let s = slot(&THREE_NAME);
        chain.run(
            on_record_and_page(&THREE_NAME, CAROL),
            Instruction::Transfer { id: s, new_owner: owner_tag(&s, &ALICE) },
        );
        assert_eq!(chain.record(&THREE_NAME).expect("held").owner, owner_tag(&s, &ALICE));
    }

    #[test]
    #[should_panic(expected = "the deposit is not this name's deposit")]
    fn only_the_admin_skips_the_deposit() {
        let mut chain = Chain::default();
        chain.configure_deposits(CAROL);
        chain.register_with_deposit(&THREE_NAME, ALICE, 0, NOW);
    }

    const OLD_PROGRAM: AccountId = AccountId::new([8; 32]);

    fn claims(holders: &[(NameId, AccountId)]) -> Vec<[u8; 32]> {
        holders
            .iter()
            .map(|(id, holder)| claim_leaf(&record_address(&OLD_PROGRAM, id), &owner_tag(id, holder)))
            .collect()
    }

    impl Chain {
        fn open_claims(&mut self, leaves: &[[u8; 32]], until: u64) {
            self.run(
                vec![meta(settings_address(&PROGRAM), false), meta(PROGRAM, true)],
                Instruction::Configure {
                    admin: CAROL,
                    treasury: BOB,
                    prices: [[0; TERMS]; PRICE_TIERS],
                    mode: MODE_DEPOSIT,
                    deposits: [DEPOSITS[0], DEPOSITS[1]],
                    claims_root: claim_root(leaves),
                    claims_until: until,
                    claims_program: OLD_PROGRAM,
                },
            );
        }

        fn claim(&mut self, id: &NameId, holder: AccountId, leaves: &[[u8; 32]], index: usize, sealed: &[u8]) {
            self.run(
                vec![meta(holder, true)],
                Instruction::Claim {
                    id: *id,
                    sealed: sealed.to_vec(),
                    now: NOW,
                    proof: claim_proof(leaves, index),
                    price_tier: DEFAULT_TIER,
                },
            );
        }
    }

    #[test]
    fn claim_proofs_hold_for_every_list_size() {
        for size in 1_u8..40 {
            let leaves: Vec<[u8; 32]> = (0..size).map(|i| [i; 32]).collect();
            let root = claim_root(&leaves);
            for (index, leaf) in leaves.iter().enumerate() {
                assert!(claim_holds(&root, *leaf, &claim_proof(&leaves, index)), "{size}/{index}");
                assert!(!claim_holds(&root, [0xEE; 32], &claim_proof(&leaves, index)));
            }
        }
    }

    #[test]
    fn a_holder_claims_their_name_for_free_and_for_good() {
        let mut chain = Chain::default();
        let leaves = claims(&[(JEF, ALICE), (ADA, BOB)]);
        chain.open_claims(&leaves, NOW + YEAR);
        chain.claim(&JEF, ALICE, &leaves, 0, b"sealed");
        let record = chain.record(&JEF).expect("claimed");
        assert_eq!(record.owner, owner_tag(&slot(&JEF), &ALICE));
        assert_eq!(record.expires, u64::MAX);
        assert_eq!(record.sealed, b"sealed");
        assert!(record.claimed);
        assert_eq!(record.price_tier, DEFAULT_TIER);
    }

    #[test]
    fn a_claim_moves_no_money() {
        let leaves = claims(&[(JEF, ALICE)]);
        let instruction = Instruction::Claim {
            id: JEF,
            sealed: Vec::new(),
            now: NOW,
            proof: claim_proof(&leaves, 0),
            price_tier: DEFAULT_TIER,
        };
        let planned = plan(&input(vec![meta(ALICE, true)], &instruction), instruction);
        assert!(planned.output().chained_calls.is_empty());
    }

    #[test]
    #[should_panic(expected = "this account did not hold this name")]
    fn someone_else_cannot_use_a_holders_proof() {
        let mut chain = Chain::default();
        let leaves = claims(&[(JEF, ALICE), (ADA, BOB)]);
        chain.open_claims(&leaves, NOW + YEAR);
        chain.claim(&JEF, BOB, &leaves, 0, b"");
    }

    #[test]
    #[should_panic(expected = "this name was already claimed")]
    fn a_name_is_claimed_once_even_after_a_transfer() {
        let mut chain = Chain::default();
        let leaves = claims(&[(JEF, ALICE)]);
        chain.open_claims(&leaves, NOW + YEAR);
        chain.claim(&JEF, ALICE, &leaves, 0, b"");
        let s = slot(&JEF);
        chain.run(
            on_record_and_page(&JEF, ALICE),
            Instruction::Transfer { id: s, new_owner: owner_tag(&s, &BOB) },
        );
        chain.claim(&JEF, ALICE, &leaves, 0, b"");
    }

    #[test]
    fn a_claim_outranks_a_registration_of_the_same_name() {
        let mut chain = Chain::default();
        let leaves = claims(&[(JEF, ALICE)]);
        chain.open_claims(&leaves, NOW + YEAR);
        chain.register_with_deposit(&JEF, BOB, DEPOSITS[0], NOW);
        chain.claim(&JEF, ALICE, &leaves, 0, b"");
        assert_eq!(chain.record(&JEF).expect("claimed").owner, owner_tag(&slot(&JEF), &ALICE));
    }

    #[test]
    #[should_panic(expected = "name is already registered")]
    fn a_claimed_name_cannot_be_registered_over() {
        let mut chain = Chain::default();
        let leaves = claims(&[(JEF, ALICE)]);
        chain.open_claims(&leaves, NOW + YEAR);
        chain.claim(&JEF, ALICE, &leaves, 0, b"");
        chain.register_with_deposit(&JEF, BOB, DEPOSITS[0], NOW);
    }

    #[test]
    #[should_panic(expected = "the claim period has ended")]
    fn claims_stop_at_the_end_of_the_period() {
        let mut chain = Chain::default();
        let leaves = claims(&[(JEF, ALICE)]);
        chain.open_claims(&leaves, NOW);
        chain.claim(&JEF, ALICE, &leaves, 0, b"");
    }

    #[test]
    #[should_panic(expected = "this registry takes no claims")]
    fn a_registry_without_a_root_takes_no_claims() {
        let mut chain = Chain::default();
        chain.open_claims(&[], NOW + YEAR);
        chain.claim(&JEF, ALICE, &[[0; 32]], 0, b"");
    }

    #[test]
    #[should_panic(expected = "only this program may send an effect")]
    fn outsiders_cannot_send_effects() {
        let receiver = Actor::new(record_address(&PROGRAM, &JEF), PROGRAM);
        let receive = ReceiveInput {
            receiver,
            from: None,
            grants: std::collections::BTreeSet::from([receiver]),
            pre_state: Default::default(),
            message: Vec::new(),
        };
        let _ = handle(&receive, Message::Effect(Effect::Remove { signer: owner_tag(&JEF, &ALICE) }));
    }

    #[test]
    fn another_programs_effect_is_refused_too() {
        let receive = ReceiveInput {
            receiver: Actor::new(record_address(&PROGRAM, &JEF), PROGRAM),
            from: Some(Actor::new(ALICE, BOB)),
            grants: Default::default(),
            pre_state: Default::default(),
            message: Vec::new(),
        };
        assert!(!receive.from_own_program());
    }

    // ── Subname tests ───────────────────────────────────────────────────

    #[test]
    fn parent_holder_creates_permanent_and_revocable_subnames_and_moves_no_money() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"parent");
        let parent = slot(&JEF);
        let parent_gen = chain.record(&JEF).expect("parent").generation;

        // Creating a subname costs only transaction gas (no native token transfer).
        let create_rev = Instruction::CreateSubname {
            parent_id: parent,
            sub_id: BLOG_SUB,
            owner: owner_tag(&BLOG_SUB, &BOB),
            revocable: true,
            sealed: b"bob's blog".to_vec(),
            now: NOW,
        };
        let planned = plan(&input(vec![meta(ALICE, true)], &create_rev), create_rev.clone());
        assert!(planned.output().chained_calls.is_empty());
        let window = planned.output().timestamp_validity_window;
        assert_eq!((window.start(), window.end()), (Some(NOW), Some(NOW + TIME_WINDOW_MS)));

        chain.run(vec![meta(ALICE, true)], create_rev);
        assert_eq!(
            chain.subname(&parent, &BLOG_SUB),
            Some(SubnameRecord {
                version: VERSION,
                owner: owner_tag(&BLOG_SUB, &BOB),
                kind: SubnameKind::Revocable { parent_generation: parent_gen },
                sealed: b"bob's blog".to_vec(),
            })
        );

        chain.run(
            vec![meta(ALICE, true)],
            Instruction::CreateSubname {
                parent_id: parent,
                sub_id: SHOP_SUB,
                owner: owner_tag(&SHOP_SUB, &CAROL),
                revocable: false,
                sealed: b"carol's shop".to_vec(),
                now: NOW,
            },
        );
        assert_eq!(
            chain.subname(&parent, &SHOP_SUB),
            Some(SubnameRecord {
                version: VERSION,
                owner: owner_tag(&SHOP_SUB, &CAROL),
                kind: SubnameKind::Permanent,
                sealed: b"carol's shop".to_vec(),
            })
        );
    }

    #[test]
    #[should_panic(expected = "signer does not own this name")]
    fn non_parent_cannot_create_a_subname() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"");
        chain.run(
            vec![meta(BOB, true)],
            Instruction::CreateSubname {
                parent_id: slot(&JEF),
                sub_id: BLOG_SUB,
                owner: owner_tag(&BLOG_SUB, &BOB),
                revocable: false,
                sealed: Vec::new(),
                now: NOW,
            },
        );
    }

    #[test]
    #[should_panic(expected = "parent name has lapsed")]
    fn lapsed_parent_cannot_create_a_subname() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"");
        chain.run(
            vec![meta(ALICE, true)],
            Instruction::CreateSubname {
                parent_id: slot(&JEF),
                sub_id: BLOG_SUB,
                owner: owner_tag(&BLOG_SUB, &BOB),
                revocable: true,
                sealed: Vec::new(),
                now: NOW + YEAR,
            },
        );
    }

    #[test]
    fn parent_holder_revokes_a_revocable_subname_and_clears_its_page() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"");
        let parent = slot(&JEF);
        chain.run(
            vec![meta(ALICE, true)],
            Instruction::CreateSubname {
                parent_id: parent,
                sub_id: BLOG_SUB,
                owner: owner_tag(&BLOG_SUB, &BOB),
                revocable: true,
                sealed: b"blog".to_vec(),
                now: NOW,
            },
        );
        chain.run(
            vec![meta(BOB, true)],
            Instruction::SetSubnamePage {
                parent_id: parent,
                sub_id: BLOG_SUB,
                sealed: b"blog page".to_vec(),
                now: Some(NOW),
            },
        );
        assert!(chain.subname_page(&parent, &BLOG_SUB).is_some());

        chain.run(
            vec![meta(ALICE, true)],
            Instruction::RevokeSubname {
                parent_id: parent,
                sub_id: BLOG_SUB,
                now: NOW,
            },
        );
        assert_eq!(chain.subname(&parent, &BLOG_SUB), None);
        assert_eq!(chain.subname_page(&parent, &BLOG_SUB), None);
    }

    #[test]
    #[should_panic(expected = "permanent subnames cannot be revoked")]
    fn parent_holder_cannot_revoke_a_permanent_subname() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"");
        let parent = slot(&JEF);
        chain.run(
            vec![meta(ALICE, true)],
            Instruction::CreateSubname {
                parent_id: parent,
                sub_id: SHOP_SUB,
                owner: owner_tag(&SHOP_SUB, &BOB),
                revocable: false,
                sealed: Vec::new(),
                now: NOW,
            },
        );
        chain.run(
            vec![meta(ALICE, true)],
            Instruction::RevokeSubname {
                parent_id: parent,
                sub_id: SHOP_SUB,
                now: NOW,
            },
        );
    }

    #[test]
    #[should_panic(expected = "signer does not own this name")]
    fn subname_holder_cannot_call_revoke() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"");
        let parent = slot(&JEF);
        chain.run(
            vec![meta(ALICE, true)],
            Instruction::CreateSubname {
                parent_id: parent,
                sub_id: BLOG_SUB,
                owner: owner_tag(&BLOG_SUB, &BOB),
                revocable: true,
                sealed: Vec::new(),
                now: NOW,
            },
        );
        chain.run(
            vec![meta(BOB, true)],
            Instruction::RevokeSubname {
                parent_id: parent,
                sub_id: BLOG_SUB,
                now: NOW,
            },
        );
    }

    #[test]
    #[should_panic(expected = "revocable subnames must be checked against the parent")]
    fn revocable_subname_cannot_bypass_parent_check() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"");
        let parent = slot(&JEF);
        chain.run(
            vec![meta(ALICE, true)],
            Instruction::CreateSubname {
                parent_id: parent,
                sub_id: BLOG_SUB,
                owner: owner_tag(&BLOG_SUB, &BOB),
                revocable: true,
                sealed: Vec::new(),
                now: NOW,
            },
        );
        chain.run(
            vec![meta(BOB, true)],
            Instruction::SetSubnameRecord {
                parent_id: parent,
                sub_id: BLOG_SUB,
                sealed: b"bypass".to_vec(),
                now: None,
            },
        );
    }

    #[test]
    fn parent_transfer_detaches_revocable_subnames_while_permanent_stay() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"");
        let parent = slot(&JEF);

        // Create one revocable (for Bob) and one permanent (for Carol).
        chain.run(
            vec![meta(ALICE, true)],
            Instruction::CreateSubname {
                parent_id: parent,
                sub_id: BLOG_SUB,
                owner: owner_tag(&BLOG_SUB, &BOB),
                revocable: true,
                sealed: b"bob's blog".to_vec(),
                now: NOW,
            },
        );
        chain.run(
            vec![meta(BOB, true)],
            Instruction::SetSubnamePage {
                parent_id: parent,
                sub_id: BLOG_SUB,
                sealed: b"bob's page".to_vec(),
                now: Some(NOW),
            },
        );
        chain.run(
            vec![meta(ALICE, true)],
            Instruction::CreateSubname {
                parent_id: parent,
                sub_id: SHOP_SUB,
                owner: owner_tag(&SHOP_SUB, &CAROL),
                revocable: false,
                sealed: b"carol's shop".to_vec(),
                now: NOW,
            },
        );

        // Alice enables subname sales, then transfers parent to Dave.
        chain.run(
            vec![meta(ALICE, true)],
            Instruction::SetSubnameSale {
                id: parent,
                sale: Some(SubnameSale { price: 500, payout: ALICE }),
            },
        );
        chain.run(
            on_record_and_page(&JEF, ALICE),
            Instruction::Transfer { id: parent, new_owner: owner_tag(&parent, &DAVE) },
        );
        // Sales were cleared on transfer.
        assert_eq!(chain.record(&JEF).expect("parent").subname_sale, None);

        // Bob can no longer update his revocable subname.
        let mut bob_try = Chain { actors: chain.actors.clone() };
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            bob_try.run(
                vec![meta(BOB, true)],
                Instruction::SetSubnameRecord {
                    parent_id: parent,
                    sub_id: BLOG_SUB,
                    sealed: b"still mine".to_vec(),
                    now: Some(NOW),
                },
            );
        }))
        .is_err());

        // Carol's permanent subname is completely unaffected (even with now: None).
        chain.run(
            vec![meta(CAROL, true)],
            Instruction::SetSubnameRecord {
                parent_id: parent,
                sub_id: SHOP_SUB,
                sealed: b"carol updated".to_vec(),
                now: None,
            },
        );
        assert_eq!(chain.subname(&parent, &SHOP_SUB).expect("perm").sealed, b"carol updated");

        // Dave (new parent holder) cannot overwrite Carol's permanent subname.
        let mut dave_try = Chain { actors: chain.actors.clone() };
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            dave_try.run(
                vec![meta(DAVE, true)],
                Instruction::CreateSubname {
                    parent_id: parent,
                    sub_id: SHOP_SUB,
                    owner: owner_tag(&SHOP_SUB, &DAVE),
                    revocable: false,
                    sealed: Vec::new(),
                    now: NOW,
                },
            );
        }))
        .is_err());

        // Dave CAN directly reclaim Bob's stale revocable subname in one transaction
        // (overwriting it with Dave's new generation and clearing Bob's old page).
        chain.run(
            vec![meta(DAVE, true)],
            Instruction::CreateSubname {
                parent_id: parent,
                sub_id: BLOG_SUB,
                owner: owner_tag(&BLOG_SUB, &DAVE),
                revocable: true,
                sealed: b"dave's blog".to_vec(),
                now: NOW,
            },
        );
        let reclaimed = chain.subname(&parent, &BLOG_SUB).expect("reclaimed");
        assert_eq!(reclaimed.owner, owner_tag(&BLOG_SUB, &DAVE));
        assert_eq!(reclaimed.sealed, b"dave's blog");
        assert_eq!(
            reclaimed.kind,
            SubnameKind::Revocable {
                parent_generation: chain.record(&JEF).expect("parent").generation,
            }
        );
        assert_eq!(chain.subname_page(&parent, &BLOG_SUB), None);
    }

    #[test]
    fn parent_release_releases_revocable_subnames_even_after_reregistration_while_permanent_stay() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"");
        let parent = slot(&JEF);

        chain.run(
            vec![meta(ALICE, true)],
            Instruction::CreateSubname {
                parent_id: parent,
                sub_id: BLOG_SUB,
                owner: owner_tag(&BLOG_SUB, &BOB),
                revocable: true,
                sealed: b"rev".to_vec(),
                now: NOW,
            },
        );
        chain.run(
            vec![meta(ALICE, true)],
            Instruction::CreateSubname {
                parent_id: parent,
                sub_id: SHOP_SUB,
                owner: owner_tag(&SHOP_SUB, &CAROL),
                revocable: false,
                sealed: b"perm".to_vec(),
                now: NOW,
            },
        );

        // Alice releases the parent.
        chain.run(
            on_record_and_page(&JEF, ALICE),
            Instruction::Release { id: parent, refund: 0 },
        );

        // Bob's revocable subname is refused because parent is unregistered.
        let mut bob_try = Chain { actors: chain.actors.clone() };
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            bob_try.run(
                vec![meta(BOB, true)],
                Instruction::SetSubnameRecord {
                    parent_id: parent,
                    sub_id: BLOG_SUB,
                    sealed: b"dead".to_vec(),
                    now: Some(NOW),
                },
            );
        }))
        .is_err());

        // Carol's permanent subname still works, transfers, and releases with no parent!
        chain.run(
            vec![meta(CAROL, true)],
            Instruction::TransferSubname {
                parent_id: parent,
                sub_id: SHOP_SUB,
                new_owner: owner_tag(&SHOP_SUB, &DAVE),
                now: None,
            },
        );
        assert_eq!(
            chain.subname(&parent, &SHOP_SUB).expect("perm").owner,
            owner_tag(&SHOP_SUB, &DAVE)
        );

        // Even if Alice re-registers the parent later, its generation is fresh,
        // so Bob's old revocable subname stays dead.
        let later = NOW + MIN_COMMIT_AGE_MS;
        chain.run(
            on_registration(&JEF, ALICE),
            register_at(JEF, b"new".to_vec(), TERM_1Y, 0, later),
        );
        let mut bob_try2 = Chain { actors: chain.actors.clone() };
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            bob_try2.run(
                vec![meta(BOB, true)],
                Instruction::SetSubnameRecord {
                    parent_id: parent,
                    sub_id: BLOG_SUB,
                    sealed: b"zombie".to_vec(),
                    now: Some(later),
                },
            );
        }))
        .is_err());

        // Dave releases the permanent subname; now Alice can create it again.
        chain.run(
            vec![meta(DAVE, true)],
            Instruction::ReleaseSubname {
                parent_id: parent,
                sub_id: SHOP_SUB,
                now: None,
            },
        );
        assert_eq!(chain.subname(&parent, &SHOP_SUB), None);
        chain.run(
            vec![meta(ALICE, true)],
            Instruction::CreateSubname {
                parent_id: parent,
                sub_id: SHOP_SUB,
                owner: owner_tag(&SHOP_SUB, &ALICE),
                revocable: false,
                sealed: b"reborn".to_vec(),
                now: later,
            },
        );
        assert_eq!(chain.subname(&parent, &SHOP_SUB).expect("reborn").sealed, b"reborn");
    }

    #[test]
    fn revocable_subname_survives_parent_renewal_and_dies_after_parent_grace() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"");
        let parent = slot(&JEF);
        chain.run(
            vec![meta(ALICE, true)],
            Instruction::CreateSubname {
                parent_id: parent,
                sub_id: BLOG_SUB,
                owner: owner_tag(&BLOG_SUB, &BOB),
                revocable: true,
                sealed: b"v1".to_vec(),
                now: NOW,
            },
        );
        // Renewing the parent does NOT change its generation.
        chain.renew(&JEF, ALICE, NOW + YEAR - 1);
        chain.run(
            vec![meta(BOB, true)],
            Instruction::SetSubnameRecord {
                parent_id: parent,
                sub_id: BLOG_SUB,
                sealed: b"v2".to_vec(),
                now: Some(NOW + YEAR),
            },
        );
        assert_eq!(chain.subname(&parent, &BLOG_SUB).expect("alive").sealed, b"v2");

        // Past the parent's grace period, Bob's update is refused.
        let past_grace = NOW + YEAR + MONTH + GRACE_MS;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            chain.run(
                vec![meta(BOB, true)],
                Instruction::SetSubnameRecord {
                    parent_id: parent,
                    sub_id: BLOG_SUB,
                    sealed: b"too late".to_vec(),
                    now: Some(past_grace),
                },
            );
        }));
        assert!(result.is_err());
    }

    #[test]
    fn subname_sales_pay_parent_atomically_and_always_create_permanent_subnames() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"");
        let parent = slot(&JEF);

        // Alice enables sales at 500_000 paid to Alice's chosen public payout account (CAROL).
        let price = 500_000;
        chain.run(
            vec![meta(ALICE, true)],
            Instruction::SetSubnameSale {
                id: parent,
                sale: Some(SubnameSale { price, payout: CAROL }),
            },
        );

        // Bob commits publicly and waits PUBLIC_MIN_COMMIT_AGE_MS.
        chain.commit_subname(&parent, &BLOG_SUB, BOB, NOW, true);
        let buy_time = NOW + PUBLIC_MIN_COMMIT_AGE_MS;
        let buy = Instruction::BuySubname {
            parent_id: parent,
            sub_id: BLOG_SUB,
            sealed: b"bought".to_vec(),
            price,
            payout: Some(CAROL),
            now: buy_time,
            secret: SECRET,
        };

        // Verify the native transfer pays CAROL exactly `price` in the same tx.
        let call = only_call(vec![native(BOB)], buy.clone());
        assert_eq!(call.to, Actor::native_balance(BOB));
        assert!(call.grants.contains(&Actor::native_balance(BOB)));
        assert_eq!(
            call.message,
            borsh::to_vec(&native_token::Message::Transfer { to: CAROL, amount: price })
                .expect("serializes")
        );

        chain.run_exactly(vec![native(BOB)], buy);
        assert_eq!(
            chain.subname(&parent, &BLOG_SUB),
            Some(SubnameRecord {
                version: VERSION,
                owner: owner_tag(&BLOG_SUB, &BOB),
                kind: SubnameKind::Permanent,
                sealed: b"bought".to_vec(),
            })
        );
        // Commitment was consumed.
        let promised = subname_commitment(&parent, &BLOG_SUB, &owner_tag(&BLOG_SUB, &BOB), &SECRET);
        assert!(chain.data(&commitment_address(&PROGRAM, &promised)).is_empty());

        // Switching sales off stops further purchases.
        chain.run(
            vec![meta(ALICE, true)],
            Instruction::SetSubnameSale { id: parent, sale: None },
        );
        chain.commit_subname(&parent, &SHOP_SUB, BOB, buy_time, true);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            chain.run_exactly(
                vec![native(BOB)],
                Instruction::BuySubname {
                    parent_id: parent,
                    sub_id: SHOP_SUB,
                    sealed: Vec::new(),
                    price,
                    payout: Some(CAROL),
                    now: buy_time + PUBLIC_MIN_COMMIT_AGE_MS,
                    secret: SECRET,
                },
            );
        }));
        assert!(result.is_err());
    }

    #[test]
    fn subname_sales_support_free_claims_and_block_wrong_price_payout_or_frontrunning() {
        let mut chain = Chain::default();
        chain.register(&JEF, ALICE, b"");
        let parent = slot(&JEF);

        // Free open subnames (price = 0): no native transfer emitted.
        chain.run(
            vec![meta(ALICE, true)],
            Instruction::SetSubnameSale {
                id: parent,
                sale: Some(SubnameSale { price: 0, payout: ALICE }),
            },
        );
        chain.commit_subname(&parent, &BLOG_SUB, BOB, NOW, true);
        let buy_free = Instruction::BuySubname {
            parent_id: parent,
            sub_id: BLOG_SUB,
            sealed: b"free".to_vec(),
            price: 0,
            payout: None,
            now: NOW + PUBLIC_MIN_COMMIT_AGE_MS,
            secret: SECRET,
        };
        let planned = plan(&input(vec![native(BOB)], &buy_free), buy_free.clone());
        assert!(planned.output().chained_calls.is_empty());

        // A front-runner (DAVE) seeing Bob's reveal cannot replay Bob's commitment.
        let mut front = Chain { actors: chain.actors.clone() };
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            front.run_exactly(vec![native(DAVE)], buy_free.clone());
        }))
        .is_err());

        chain.run_exactly(vec![native(BOB)], buy_free);
        assert_eq!(chain.subname(&parent, &BLOG_SUB).expect("bought").kind, SubnameKind::Permanent);

        // Paid sales reject wrong price and wrong payout account.
        chain.run(
            vec![meta(ALICE, true)],
            Instruction::SetSubnameSale {
                id: parent,
                sale: Some(SubnameSale { price: 1000, payout: CAROL }),
            },
        );
        chain.commit_subname(&parent, &SHOP_SUB, BOB, NOW, true);
        let t = NOW + PUBLIC_MIN_COMMIT_AGE_MS;

        let mut wrong_price = Chain { actors: chain.actors.clone() };
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            wrong_price.run_exactly(
                vec![native(BOB)],
                Instruction::BuySubname {
                    parent_id: parent,
                    sub_id: SHOP_SUB,
                    sealed: Vec::new(),
                    price: 999,
                    payout: Some(CAROL),
                    now: t,
                    secret: SECRET,
                },
            );
        }))
        .is_err());

        let mut wrong_payout = Chain { actors: chain.actors.clone() };
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            wrong_payout.run_exactly(
                vec![native(BOB)],
                Instruction::BuySubname {
                    parent_id: parent,
                    sub_id: SHOP_SUB,
                    sealed: Vec::new(),
                    price: 1000,
                    payout: Some(DAVE),
                    now: t,
                    secret: SECRET,
                },
            );
        }))
        .is_err());
    }
}
