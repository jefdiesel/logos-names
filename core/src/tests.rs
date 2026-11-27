use std::collections::{BTreeMap, BTreeSet};

use lee_core::program::TimestampValidityWindow;

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

pub(super) fn verify_against_test_list(
    id: &NameId,
    proof: &LengthProof,
) -> Result<u8, &'static str> {
    let list = test_list();
    shortlist::verify_depth(&list.root(), id, proof, list.depth())
}

fn length_of(id: &NameId) -> LengthProof {
    test_list().prove(id).expect("a proof")
}

const PROGRAM: AccountId = AccountId::new([7; 32]);
const NFT_PROGRAM: AccountId = AccountId::new([9; 32]);
const OLD_PROGRAM: AccountId = AccountId::new([8; 32]);
const ALICE: AccountId = AccountId::new([1; 32]);
const BOB: AccountId = AccountId::new([2; 32]);
const CAROL: AccountId = AccountId::new([3; 32]);
const DAVE: AccountId = AccountId::new([4; 32]);
const DEED_PDA: AccountId = AccountId::new([0x0A; 32]);
const JEF: NameId = [0xAA; 32];
const ADA: NameId = [0xBB; 32];
const BLOG_SUB: NameId = [0xC1; 32];
const SHOP_SUB: NameId = [0xC2; 32];
const DEFAULT_TIER: u8 = PRICE_TIER_8_PLUS;
const NOW: u64 = 1_700_000_000_000;
const YEAR: u64 = TERM_LENGTHS_MS[1];
const MONTH: u64 = TERM_LENGTHS_MS[0];
const PRICES: [[u128; TERMS]; PRICE_TIERS] = DEFAULT_PRICES;
const DEPOSITS: [u128; 2] = [100_000_000, 2_000_000_000];
const SECRET: [u8; 32] = [5; 32];
const RECORD_VECTOR: &str = "7bwwvMLtxqqNYiXsd5bYaq8e31E1odYf6jAK5z7tmUdd";
const PAGE_VECTOR: &str = "8M4vmdxCXL4eS8QXiHSdbeUUPt5Vbx1UVjLcv2cPk9Y5";

fn slot(base: &NameId) -> NameId {
    tiered_id(base, DEFAULT_TIER)
}

fn slot_of(base: &NameId, price_tier: u8) -> NameId {
    tiered_id(base, price_tier)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AccountMeta {
    account_id: AccountId,
    is_authorized: bool,
}

impl AccountMeta {
    fn native_balance(account_id: AccountId, is_authorized: bool) -> Self {
        Self {
            account_id,
            is_authorized,
        }
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
    AccountMeta {
        account_id,
        is_authorized,
    }
}

fn native(account_id: AccountId) -> AccountMeta {
    AccountMeta::native_balance(account_id, true)
}

fn input(accounts: Vec<AccountMeta>, instruction: &Instruction) -> PlanInput {
    let signer = accounts.iter().find(|account| account.is_authorized).copied();
    let receiver = signer
        .or(accounts.last().copied())
        .expect("some account");
    let paid = accounts
        .last()
        .filter(|account| !account.is_authorized)
        .map(|account| account.account_id);
    let mut grants = BTreeSet::new();
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
        Instruction::Register {
            treasury,
            deposit: false,
            ..
        }
        | Instruction::Renew { treasury, .. } => {
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
                selector: Target {
                    account_id: message.to.account_id,
                },
                data: borsh::to_vec(&effect).expect("the effect serializes"),
            }
        })
        .collect();
    if let Some(name) = primary {
        effects.push(Sent {
            selector: Target {
                account_id: input.receive.receiver.account_id,
            },
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

fn run_plan_events(accounts: Vec<AccountMeta>, instruction: Instruction) -> Vec<ProgramEvent> {
    plan(&input(accounts, &instruction), instruction)
        .output()
        .events
        .clone()
}

fn only_call(accounts: Vec<AccountMeta>, instruction: Instruction) -> SyncMessage {
    let planned = plan(&input(accounts, &instruction), instruction);
    let [call] = planned.output().chained_calls.as_slice() else {
        panic!("expected one chained call");
    };
    call.clone()
}

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

fn on_registration(base_id: &NameId, payer: AccountId) -> Vec<AccountMeta> {
    on_registration_tier(base_id, DEFAULT_TIER, payer)
}

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

fn paying(mut accounts: Vec<AccountMeta>, treasury: AccountId) -> Vec<AccountMeta> {
    accounts.push(AccountMeta::native_balance(treasury, false));
    accounts
}

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

fn on_record(base_id: &NameId, signer: AccountId) -> Vec<AccountMeta> {
    vec![
        meta(record_address(&PROGRAM, &slot(base_id)), false),
        meta(signer, true),
    ]
}

fn on_record_and_page(base_id: &NameId, signer: AccountId) -> Vec<AccountMeta> {
    let s = slot(base_id);
    vec![
        meta(record_address(&PROGRAM, &s), false),
        meta(page_address(&PROGRAM, &s), false),
        meta(signer, true),
    ]
}

fn on_release(base_id: &NameId, owner: AccountId, refund: u128) -> Vec<AccountMeta> {
    let s = slot(base_id);
    let mut accounts = vec![
        meta(record_address(&PROGRAM, &s), false),
        meta(page_address(&PROGRAM, &s), false),
        AccountMeta::native_balance(owner, true),
    ];
    if refund > 0 {
        accounts.push(AccountMeta::native_balance(
            deposit_address(&PROGRAM, &s),
            false,
        ));
    }
    accounts
}

fn with_deposit_account(id: &NameId, holder: AccountId) -> Vec<AccountMeta> {
    let mut accounts = on_registration(id, holder);
    accounts.push(AccountMeta::native_balance(
        deposit_address(&PROGRAM, &slot(id)),
        false,
    ));
    accounts
}

fn claims(holders: &[(NameId, AccountId)]) -> Vec<[u8; 32]> {
    holders
        .iter()
        .map(|(id, holder)| claim_leaf(&record_address(&OLD_PROGRAM, id), &owner_tag(id, holder)))
        .collect()
}

/// Mock `lez_nft` v3 token state for verifying two-program lockstep invariants.
#[derive(Debug, Clone, PartialEq, Eq)]
struct MockNftRecord {
    token_id: [u8; 32],
    owner: OwnerTag,
    linked: LinkedAuthority,
    content: Vec<u8>,
    shielded_deed: Option<AccountId>,
    expires: u64,
}

/// Mock private LP-0001 deed state in `lez_nft` v3.
#[derive(Debug, Clone, PartialEq, Eq)]
struct MockPrivateDeed {
    token_id: [u8; 32],
    expires: u64,
}

/// In-memory multi-program LEZ 0.4 test harness simulating:
/// - `names_core` (`PROGRAM`) in exact LIFO work-stack order
/// - mock `lez_nft` v3 (`NFT_PROGRAM`) enforcing authority checks & lockstep
/// - native token transfers (`NATIVE_TOKEN_PROGRAM_ID`) tracking balances
#[derive(Clone, Default)]
struct Chain {
    actors: BTreeMap<AccountId, Vec<u8>>,
    nft_tokens: BTreeMap<AccountId, MockNftRecord>,
    nft_deeds: BTreeMap<AccountId, MockPrivateDeed>,
    transfers: Vec<(AccountId, AccountId, u128)>,
}

impl Chain {
    fn run(&mut self, accounts: Vec<AccountMeta>, instruction: Instruction) {
        if let (Instruction::Register { now, .. }, Some(commit_slot)) =
            (&instruction, accounts.get(3))
        {
            self.actors
                .entry(commit_slot.account_id)
                .or_insert_with(|| {
                    to_bytes(&Commitment {
                        version: VERSION,
                        made: now - MIN_COMMIT_AGE_MS,
                        public: false,
                    })
                });
        }
        self.run_exactly(accounts, instruction);
    }

    fn run_exactly(&mut self, accounts: Vec<AccountMeta>, mut instruction: Instruction) {
        let plan_in = input(accounts, &instruction);
        match &mut instruction {
            Instruction::Register {
                treasury,
                deposit: false,
                ..
            }
            | Instruction::Renew { treasury, .. } => {
                *treasury = plan_in.treasury;
            }
            _ => {}
        }
        let root_actor = plan_in.receive.receiver;
        let response = instruct(&plan_in.receive, instruction);
        let mut next = self.clone();
        if let Some(after) = response.post_state {
            next.actors.insert(root_actor.account_id, after.into_inner());
        }
        next.dispatch_messages(root_actor, response.sync_messages);
        *self = next;
    }

    /// Simulates Path B: a holder calling `lez_nft::Transfer` on `NFT_PROGRAM`,
    /// which updates `MockNftRecord.owner` and sends `Message::NftTransferHook`
    /// to `(linked.record_actor, PROGRAM)` atomically in the same transaction.
    fn nft_transfer(
        &mut self,
        token_id: [u8; 32],
        signer: AccountId,
        new_owner: OwnerTag,
        now: u64,
    ) {
        let mut next = self.clone();
        let token_pda = nft_token_address(&NFT_PROGRAM, &token_id);
        let nft = next
            .nft_tokens
            .get_mut(&token_pda)
            .expect("nft token does not exist");
        assert!(nft.shielded_deed.is_none(), "nft is shielded");
        let expected_old_owner = owner_tag(&nft.linked.target, &signer);
        assert_eq!(nft.owner, expected_old_owner, "signer does not own nft");
        nft.owner = new_owner;
        let hook_msg = SyncMessage::new(
            Actor::new(nft.linked.record_actor, nft.linked.program),
            &Message::NftTransferHook {
                id: nft.linked.target,
                token_id,
                expected_old_owner,
                new_owner,
                now,
            },
        );
        next.dispatch_messages(Actor::new(token_pda, NFT_PROGRAM), vec![hook_msg]);
        *self = next;
    }

    fn dispatch_messages(&mut self, initial_from: Actor, initial: Vec<SyncMessage>) {
        let mut stack: Vec<(Actor, SyncMessage)> = initial
            .into_iter()
            .rev()
            .map(|msg| (initial_from, msg))
            .collect();
        while let Some((from, msg)) = stack.pop() {
            let to = msg.to;
            if to.program_account_id == PROGRAM {
                let before = self.actors.get(&to.account_id).cloned().unwrap_or_default();
                let decoded: Message =
                    borsh::from_slice(&msg.message).expect("registry message decodes");
                let receive = ReceiveInput {
                    receiver: to,
                    from: Some(from),
                    grants: msg.grants,
                    pre_state: before.into(),
                    message: msg.message,
                };
                let response = handle(&receive, decoded);
                if let Some(after) = response.post_state {
                    self.actors.insert(to.account_id, after.into_inner());
                }
                stack.extend(
                    response
                        .sync_messages
                        .into_iter()
                        .rev()
                        .map(|child| (to, child)),
                );
            } else if to.program_account_id == NFT_PROGRAM {
                self.handle_mock_nft(from, to, &msg.message);
            } else if to.program_account_id == native_token::NATIVE_TOKEN_PROGRAM_ID {
                let native_msg: native_token::Message =
                    borsh::from_slice(&msg.message).expect("native token message decodes");
                if let native_token::Message::Transfer {
                    to: recipient,
                    amount,
                } = native_msg
                {
                    self.transfers.push((to.account_id, recipient, amount));
                }
            } else {
                panic!("unexpected target program {:?}", to.program_account_id);
            }
        }
    }

    fn handle_mock_nft(&mut self, from: Actor, to: Actor, bytes: &[u8]) {
        assert_eq!(
            from,
            Actor::new(settings_address(&PROGRAM), PROGRAM),
            "AuthoritySync must come from registry settings_address"
        );
        let NftAuthoritySyncMessage(op) =
            borsh::from_slice(bytes).expect("AuthoritySync message decodes");
        assert_eq!(
            to.account_id,
            nft_token_address(&NFT_PROGRAM, op.token_id()),
            "AuthoritySync must target the token PDA"
        );
        match op {
            NftAuthorityOp::Mint {
                token_id,
                owner,
                linked,
                content,
                expires,
            } => {
                assert!(
                    !self.nft_tokens.contains_key(&to.account_id),
                    "mock nft: token already minted"
                );
                self.nft_tokens.insert(
                    to.account_id,
                    MockNftRecord {
                        token_id,
                        owner,
                        linked,
                        content,
                        shielded_deed: None,
                        expires,
                    },
                );
            }
            NftAuthorityOp::Transfer { new_owner, .. } => {
                let nft = self
                    .nft_tokens
                    .get_mut(&to.account_id)
                    .expect("mock nft: token missing on transfer");
                assert!(nft.shielded_deed.is_none(), "mock nft: cannot transfer shielded");
                nft.owner = new_owner;
            }
            NftAuthorityOp::Burn { .. } => {
                let removed = self
                    .nft_tokens
                    .remove(&to.account_id)
                    .expect("mock nft: token missing on burn");
                // Note: if burned after lapse while shielded, the stale private deed
                // remains untouched (its embedded `expires` rejects LP-0001 proofs).
                let _ = removed;
            }
            NftAuthorityOp::SetExpiry { expires, .. } => {
                let nft = self
                    .nft_tokens
                    .get_mut(&to.account_id)
                    .expect("mock nft: token missing on set_expiry");
                nft.expires = expires;
            }
            NftAuthorityOp::Shield {
                token_id,
                deed_account,
                expires,
            } => {
                let nft = self
                    .nft_tokens
                    .get_mut(&to.account_id)
                    .expect("mock nft: token missing on shield");
                assert!(nft.shielded_deed.is_none(), "mock nft: already shielded");
                assert_ne!(
                    deed_account,
                    AccountId::for_public_pda(&NFT_PROGRAM, &nft_deed_seed(&token_id)),
                    "mock nft: deed_account must be a private PDA"
                );
                nft.shielded_deed = Some(deed_account);
                self.nft_deeds
                    .insert(deed_account, MockPrivateDeed { token_id, expires });
            }
            NftAuthorityOp::Unshield { .. } => {
                let nft = self
                    .nft_tokens
                    .get_mut(&to.account_id)
                    .expect("mock nft: token missing on unshield");
                let deed = nft
                    .shielded_deed
                    .take()
                    .expect("mock nft: not shielded");
                self.nft_deeds.remove(&deed);
            }
            NftAuthorityOp::RefreshShielded { expires, .. } => {
                let nft = self
                    .nft_tokens
                    .get_mut(&to.account_id)
                    .expect("mock nft: token missing on refresh");
                let deed_addr = nft
                    .shielded_deed
                    .expect("mock nft: not shielded");
                let deed = self
                    .nft_deeds
                    .get_mut(&deed_addr)
                    .expect("mock nft: private deed missing");
                deed.expires = expires;
            }
        }
    }

    /// Asserts that `NameRecord` and its `MockNftRecord` are in exact lockstep.
    fn assert_nft_lockstep(&self, slot_id: &NameId) {
        let rec = self
            .record_slot(slot_id)
            .expect("record exists for lockstep check");
        let nft = self
            .nft(&rec.nft_token_id)
            .expect("tied NFT must exist for live NameRecord");
        assert_eq!(nft.token_id, rec.nft_token_id);
        assert_eq!(nft.owner, rec.owner, "name and NFT owner must agree");
        assert_eq!(nft.expires, rec.expires, "name and NFT expiry must agree");
        assert_eq!(
            nft.shielded_deed.is_some(),
            rec.nft_shielded,
            "name and NFT shielded state must agree"
        );
        assert_eq!(nft.linked.program, PROGRAM);
        assert_eq!(nft.linked.authority_actor, settings_address(&PROGRAM));
        assert_eq!(nft.linked.target, *slot_id);
        assert_eq!(nft.linked.record_actor, record_address(&PROGRAM, slot_id));
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

    fn nft(&self, token_id: &[u8; 32]) -> Option<&MockNftRecord> {
        self.nft_tokens.get(&nft_token_address(&NFT_PROGRAM, token_id))
    }

    fn market_bucket(&self, bucket: u8) -> Option<MarketBucket> {
        let data = self.data(&market_bucket_address(&PROGRAM, bucket));
        (!data.is_empty()).then(|| borsh::from_slice(data).expect("market bucket decode"))
    }

    fn site_bucket(&self, bucket: u8, page: u16) -> Option<SiteBucket> {
        let data = self.data(&site_bucket_address(&PROGRAM, bucket, page));
        (!data.is_empty()).then(|| borsh::from_slice(data).expect("site bucket decode"))
    }

    fn site_dir_header(&self) -> Option<SiteDirHeader> {
        let data = self.data(&site_dir_header_address(&PROGRAM));
        (!data.is_empty()).then(|| borsh::from_slice(data).expect("site dir header decode"))
    }

    fn commit(&mut self, base_id: &NameId, holder: AccountId, now: u64) {
        let s = slot(base_id);
        let promised = commitment(&s, &owner_tag(&s, &holder), &SECRET);
        self.run_exactly(
            vec![
                meta(commitment_address(&PROGRAM, &promised), false),
                meta(CAROL, true),
            ],
            Instruction::Commit {
                commitment: promised,
                now,
            },
        );
    }

    fn commit_public(&mut self, base_id: &NameId, holder: AccountId, now: u64) {
        let s = slot(base_id);
        let promised = commitment(&s, &owner_tag(&s, &holder), &SECRET);
        self.run_exactly(
            vec![
                meta(commitment_address(&PROGRAM, &promised), false),
                meta(CAROL, true),
            ],
            Instruction::CommitPublic {
                commitment: promised,
                now,
            },
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
            Instruction::CommitPublic {
                commitment: promised,
                now,
            }
        } else {
            Instruction::Commit {
                commitment: promised,
                now,
            }
        };
        self.run_exactly(
            vec![
                meta(commitment_address(&PROGRAM, &promised), false),
                meta(buyer, true),
            ],
            instruction,
        );
    }

    fn configure(
        &mut self,
        signer: AccountId,
        admin: AccountId,
        prices: [[u128; TERMS]; PRICE_TIERS],
    ) {
        self.configure_with_premium(signer, admin, prices, 0);
    }

    fn configure_with_premium(
        &mut self,
        signer: AccountId,
        admin: AccountId,
        prices: [[u128; TERMS]; PRICE_TIERS],
        premium_start: u128,
    ) {
        let signer = if self.settings().is_none() {
            PROGRAM
        } else {
            signer
        };
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
                premium_start,
                nft_program: NFT_PROGRAM,
            },
        );
    }

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
                premium_start: 0,
                nft_program: NFT_PROGRAM,
            },
        );
    }

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
                premium_start: 0,
                nft_program: NFT_PROGRAM,
            },
        );
    }

    fn claim(
        &mut self,
        id: &NameId,
        holder: AccountId,
        leaves: &[[u8; 32]],
        index: usize,
        sealed: &[u8],
    ) {
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

// ── Core registration, profile, page, transfer, release, primary ────────

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
        nft_token_id: [0; 32],
        nft_shielded: false,
        listing: None,
        listing_nonce: 0,
        site_listed: None,
    });
    let asked = |pre_state: Vec<u8>, tag| {
        let input = ReceiveInput {
            receiver: Actor::new(ALICE, PROGRAM),
            from: Some(Actor::new(BOB, AccountId::new([9; 32]))),
            grants: Default::default(),
            pre_state: pre_state.into(),
            message: Vec::new(),
        };
        std::panic::catch_unwind(|| handle(&input, Message::RequireHolder { tag }).post_state)
    };
    assert_eq!(asked(held.clone(), [1; 32]).expect("the holder passes"), None);
    assert!(asked(held, [2; 32]).is_err(), "anyone else is refused");
    assert!(
        asked(Vec::new(), [1; 32]).is_err(),
        "an unregistered name is refused"
    );
}

#[test]
fn a_name_is_registered_with_its_first_profile() {
    let mut chain = Chain::default();
    chain.register(&JEF, ALICE, b"sealed profile");
    let s = slot(&JEF);
    let owner = owner_tag(&s, &ALICE);
    let generation = register_generation(&s, &owner, NOW, &SECRET);
    assert_eq!(
        chain.record(&JEF),
        Some(NameRecord {
            version: VERSION,
            owner,
            expires: NOW + TERM_LENGTHS_MS[1],
            sealed: b"sealed profile".to_vec(),
            claimed: false,
            price_tier: DEFAULT_TIER,
            generation,
            subname_sale: None,
            nft_token_id: nft_token_id(&s, &generation),
            nft_shielded: false,
            listing: None,
            listing_nonce: 0,
            site_listed: None,
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
            .map(|account| AccountMeta {
                is_authorized: false,
                ..account
            })
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
            site_dir: SiteDirOp::Keep,
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
            site_dir: SiteDirOp::Keep,
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
            site_dir: SiteDirOp::Keep,
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
            site_dir: SiteDirOp::Keep,
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
            site_dir: SiteDirOp::Keep,
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
            site_dir: SiteDirOp::Keep,
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
            site_dir: SiteDirOp::Keep,
        },
    );
}

#[test]
fn a_transfer_hands_over_the_name_and_empties_what_was_sealed() {
    let mut chain = Chain::default();
    chain.register(&JEF, ALICE, b"alice's profile");
    let s = slot(&JEF);
    let initial = chain.record(&JEF).expect("registered");
    chain.run(
        on_record_and_page(&JEF, ALICE),
        Instruction::SetPage {
            id: s,
            sealed: b"alice's page".to_vec(),
            site_dir: SiteDirOp::Keep,
        },
    );
    let bob_tag = owner_tag(&s, &BOB);
    chain.run(
        on_record_and_page(&JEF, ALICE),
        Instruction::Transfer {
            id: s,
            new_owner: bob_tag,
            now: NOW,
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
            generation: transfer_generation(&initial.generation, &bob_tag),
            subname_sale: None,
            nft_token_id: initial.nft_token_id,
            nft_shielded: false,
            listing: None,
            listing_nonce: 0,
            site_listed: None,
        })
    );
    assert_eq!(chain.page(&JEF), None);

    chain.run(
        on_record(&JEF, BOB),
        Instruction::SetRecord {
            id: s,
            sealed: b"bob's profile".to_vec(),
            site_dir: SiteDirOp::Keep,
        },
    );
    assert_eq!(
        chain.record(&JEF).expect("registered").sealed,
        b"bob's profile"
    );
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
            now: NOW,
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
            now: NOW,
        },
    );
    chain.run(
        on_record(&JEF, ALICE),
        Instruction::SetRecord {
            id: s,
            sealed: b"still mine".to_vec(),
            site_dir: SiteDirOp::Keep,
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
            site_dir: SiteDirOp::Keep,
        },
    );
    chain.run(
        on_record_and_page(&JEF, ALICE),
        Instruction::Release { id: s, refund: 0 },
    );
    assert_eq!(chain.record(&JEF), None);
    assert_eq!(chain.page(&JEF), None);
    assert!(chain.data(&record_address(&PROGRAM, &s)).is_empty());
    assert!(chain.data(&page_address(&PROGRAM, &s)).is_empty());

    chain.register(&JEF, BOB, b"bob's profile");
    assert_eq!(
        chain.record(&JEF).expect("registered").owner,
        owner_tag(&s, &BOB)
    );
}

#[test]
#[should_panic(expected = "signer does not own this name")]
fn someone_else_cannot_release_the_name() {
    let mut chain = Chain::default();
    chain.register(&JEF, ALICE, b"");
    chain.run(
        on_record_and_page(&JEF, BOB),
        Instruction::Release {
            id: slot(&JEF),
            refund: 0,
        },
    );
}

#[test]
#[should_panic(expected = "name is not registered")]
fn an_unregistered_name_cannot_be_released() {
    let mut chain = Chain::default();
    chain.run(
        on_record_and_page(&JEF, ALICE),
        Instruction::Release {
            id: slot(&JEF),
            refund: 0,
        },
    );
}

#[test]
fn one_names_changes_never_touch_another() {
    let mut chain = Chain::default();
    chain.register(&JEF, ALICE, b"jef");
    chain.register(&ADA, BOB, b"ada");
    chain.run(
        on_record_and_page(&JEF, ALICE),
        Instruction::Release {
            id: slot(&JEF),
            refund: 0,
        },
    );
    assert_eq!(chain.record(&ADA).expect("registered").sealed, b"ada");
    assert_eq!(
        chain.actors.values().filter(|data| !data.is_empty()).count(),
        1
    );
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

    chain.run(
        vec![meta(ALICE, true)],
        Instruction::SetPrimary {
            name: "blog.alice".into(),
        },
    );
    let primary: PrimaryName = borsh::from_slice(chain.data(&ALICE)).expect("a reverse record");
    assert_eq!(primary.name, "blog.alice");

    chain.run(
        vec![meta(ALICE, true)],
        Instruction::SetPrimary {
            name: String::new(),
        },
    );
    assert!(chain.data(&ALICE).is_empty());
}

#[test]
#[should_panic(expected = "name must be 1-32 chars")]
fn a_published_name_must_be_a_valid_name() {
    let mut chain = Chain::default();
    chain.run(
        vec![meta(ALICE, true)],
        Instruction::SetPrimary {
            name: "Not A Name".into(),
        },
    );
}

#[test]
#[should_panic(expected = "name must be 1-32 chars")]
fn double_hyphen_cannot_be_published_as_primary() {
    let mut chain = Chain::default();
    chain.run(
        vec![meta(ALICE, true)],
        Instruction::SetPrimary {
            name: "a--b".into(),
        },
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
    for typed in [
        "jef",
        "jef.lez",
        "Jef.LEZ",
        "  jef.lez  ",
        "lns:jef",
        "lns://jef",
        "@Jef",
    ] {
        assert_eq!(normalize_name(typed), "jef", "{typed}");
    }
    for typed in [
        "blog.alice",
        "Blog.Alice",
        "lns://Blog.Alice",
        "lns:blog.alice",
        "@blog.alice",
        "blog.alice.lez",
    ] {
        assert_eq!(normalize_name(typed), "blog.alice", "{typed}");
    }
    for name in [
        "a",
        "jef",
        "my-name-2",
        &"a".repeat(32),
        "blog.alice",
        "a-b.c-d",
        &format!("{}.{}", "a".repeat(30), "b".repeat(31)),
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
        &format!("{}.{}", "a".repeat(31), "b".repeat(31)),
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
fn a_names_accounts_differ_from_each_other_and_from_other_names() {
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
        market_bucket_address(&PROGRAM, 0),
        market_bucket_address(&PROGRAM, 1),
        site_dir_header_address(&PROGRAM),
        site_bucket_address(&PROGRAM, 0, 0),
        site_bucket_address(&PROGRAM, 0, 1),
        site_bucket_address(&PROGRAM, 1, 0),
    ];
    for (index, account) in all.iter().enumerate() {
        assert!(!all[index + 1..].contains(account));
    }
    assert_ne!(record_address(&PROGRAM, &JEF), record_address(&ALICE, &JEF));
    assert_ne!(
        tiered_id(&JEF, PRICE_TIER_5),
        tiered_id(&JEF, PRICE_TIER_6_7)
    );
    assert_ne!(
        tiered_id(&JEF, PRICE_TIER_6_7),
        tiered_id(&JEF, PRICE_TIER_8_PLUS)
    );
    assert_ne!(tiered_id(&JEF, PRICE_TIER_5), JEF);
}

// ── Borsh specification vectors (v8) ────────────────────────────────────

#[test]
fn addresses_and_instructions_match_the_specification() {
    let id: NameId = core::array::from_fn(|index| index as u8);
    assert_eq!(record_address(&PROGRAM, &id).to_string(), RECORD_VECTOR);
    assert_eq!(page_address(&PROGRAM, &id).to_string(), PAGE_VECTOR);

    let hex = |bytes: Vec<u8>| -> String { bytes.iter().map(|byte| format!("{byte:02x}")).collect() };
    let inst_hex = |instruction: &Instruction| -> String {
        hex(borsh::to_vec(instruction).expect("the instruction serializes"))
    };
    let id_hex = hex(id.to_vec());
    let sub_id = [0x33; 32];
    let sub_hex = "33".repeat(32);

    // 0: Register
    assert_eq!(
        inst_hex(&Instruction::Register {
            id,
            sealed: vec![0xEE, 0xFF],
            term: 1,
            fee: 100_000,
            treasury: None,
            now: NOW,
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
    // 1: Renew
    assert_eq!(
        inst_hex(&Instruction::Renew {
            id,
            term: 2,
            fee: 0,
            treasury: None,
            now: NOW,
        }),
        format!("01{id_hex}02{}0068e5cf8b010000", "00".repeat(17))
    );
    // 2: SetRecord (Keep & Upsert)
    assert_eq!(
        inst_hex(&Instruction::SetRecord {
            id,
            sealed: vec![0xEE],
            site_dir: SiteDirOp::Keep,
        }),
        format!("02{id_hex}01000000ee00")
    );
    assert_eq!(
        inst_hex(&Instruction::SetRecord {
            id,
            sealed: vec![0xEE],
            site_dir: SiteDirOp::Upsert {
                name: "alice".into(),
                title: "hi".into(),
                page: 0,
                now: NOW,
            },
        }),
        format!("02{id_hex}01000000ee0105000000616c69636502000000686900000068e5cf8b010000")
    );
    // 3: SetPage (Remove)
    assert_eq!(
        inst_hex(&Instruction::SetPage {
            id,
            sealed: Vec::new(),
            site_dir: SiteDirOp::Remove,
        }),
        format!("03{id_hex}0000000002")
    );
    // 4: Transfer (now includes `now: u64`)
    assert_eq!(
        inst_hex(&Instruction::Transfer {
            id,
            new_owner: [0x02; 32],
            now: NOW,
        }),
        format!("04{id_hex}{}0068e5cf8b010000", "02".repeat(32))
    );
    // 5: Release
    assert_eq!(
        inst_hex(&Instruction::Release { id, refund: 0 }),
        format!("05{id_hex}{}", "00".repeat(16))
    );
    // 6: SetPrimary
    assert_eq!(
        inst_hex(&Instruction::SetPrimary { name: "jef".into() }),
        "06030000006a6566"
    );
    // 7: Configure (added premium_start: u128, nft_program: AccountId)
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
            premium_start: DEFAULT_PREMIUM_START,
            nft_program: NFT_PROGRAM,
        }),
        format!(
            "07{}{}{table_hex}00{}{}00286bee000000000000000000000000{}",
            "01".repeat(32),
            "02".repeat(32),
            "00".repeat(72),
            "07".repeat(32),
            "09".repeat(32),
        )
    );
    // 8: Commit
    assert_eq!(
        inst_hex(&Instruction::Commit {
            commitment: [0xC0; 32],
            now: NOW,
        }),
        format!("08{}0068e5cf8b010000", "c0".repeat(32))
    );
    // 9: Claim
    assert_eq!(
        inst_hex(&Instruction::Claim {
            id,
            sealed: vec![0xEE],
            now: NOW,
            proof: vec![[0x01; 32], [0x02; 32]],
            price_tier: PRICE_TIER_6_7,
        }),
        format!(
            "09{id_hex}01000000ee0068e5cf8b01000002000000{}{}01",
            "01".repeat(32),
            "02".repeat(32)
        )
    );
    // 10 (0x0a): CommitPublic
    assert_eq!(
        inst_hex(&Instruction::CommitPublic {
            commitment: [0xC0; 32],
            now: NOW,
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
    // 12 (0x0c): SetSubnameSale
    assert_eq!(
        inst_hex(&Instruction::SetSubnameSale {
            id,
            sale: Some(SubnameSale {
                price: 100_000,
                payout: BOB
            }),
        }),
        format!(
            "0c{id_hex}01a0860100000000000000000000000000{}",
            "02".repeat(32)
        )
    );
    assert_eq!(
        inst_hex(&Instruction::SetSubnameSale { id, sale: None }),
        format!("0c{id_hex}00")
    );
    // 13..19 (0x0d..0x13): subnames (unchanged)
    assert_eq!(
        inst_hex(&Instruction::CreateSubname {
            parent_id: id,
            sub_id,
            owner: [0x02; 32],
            revocable: true,
            sealed: vec![0xEE],
            now: NOW,
        }),
        format!(
            "0d{id_hex}{sub_hex}{}0101000000ee0068e5cf8b010000",
            "02".repeat(32)
        )
    );
    assert_eq!(
        inst_hex(&Instruction::RevokeSubname {
            parent_id: id,
            sub_id,
            now: NOW,
        }),
        format!("0e{id_hex}{sub_hex}0068e5cf8b010000")
    );
    assert_eq!(
        inst_hex(&Instruction::BuySubname {
            parent_id: id,
            sub_id,
            sealed: vec![0xEE],
            price: 100_000,
            payout: Some(BOB),
            now: NOW,
            secret: [0x5A; 32],
        }),
        format!(
            "0f{id_hex}{sub_hex}01000000eea086010000000000000000000000000001{}0068e5cf8b010000{}",
            "02".repeat(32),
            "5a".repeat(32),
        )
    );
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
        inst_hex(&Instruction::SetSubnamePage {
            parent_id: id,
            sub_id,
            sealed: Vec::new(),
            now: None,
        }),
        format!("11{id_hex}{sub_hex}0000000000")
    );
    assert_eq!(
        inst_hex(&Instruction::TransferSubname {
            parent_id: id,
            sub_id,
            new_owner: [0x02; 32],
            now: Some(NOW),
        }),
        format!("12{id_hex}{sub_hex}{}010068e5cf8b010000", "02".repeat(32))
    );
    assert_eq!(
        inst_hex(&Instruction::ReleaseSubname {
            parent_id: id,
            sub_id,
            now: None,
        }),
        format!("13{id_hex}{sub_hex}00")
    );
    // 20 (0x14): SetPremium
    assert_eq!(
        inst_hex(&Instruction::SetPremium {
            premium_start: DEFAULT_PREMIUM_START,
        }),
        "1400286bee000000000000000000000000"
    );
    // 21 (0x15): List (public with display_name, and unlisted OTC)
    assert_eq!(
        inst_hex(&Instruction::List {
            id,
            price: 10_000_000,
            pay_to: BOB,
            expires_at: Some(NOW),
            now: NOW,
            display_name: Some("alice".into()),
        }),
        format!(
            "15{id_hex}80969800000000000000000000000000{}010068e5cf8b0100000068e5cf8b0100000105000000616c696365",
            "02".repeat(32)
        )
    );
    assert_eq!(
        inst_hex(&Instruction::List {
            id,
            price: 10_000_000,
            pay_to: BOB,
            expires_at: None,
            now: NOW,
            display_name: None,
        }),
        format!(
            "15{id_hex}80969800000000000000000000000000{}000068e5cf8b01000000",
            "02".repeat(32)
        )
    );
    // 22 (0x16): Cancel
    assert_eq!(inst_hex(&Instruction::Cancel { id }), format!("16{id_hex}"));
    // 23 (0x17): Buy
    assert_eq!(
        inst_hex(&Instruction::Buy {
            id,
            price: 10_000_000,
            pay_to: BOB,
            treasury: CAROL,
            expected_nonce: 1,
            now: NOW,
        }),
        format!(
            "17{id_hex}80969800000000000000000000000000{}{}01000000000000000068e5cf8b010000",
            "02".repeat(32),
            "03".repeat(32),
        )
    );
    // 24 (0x18): ShieldDeed
    assert_eq!(
        inst_hex(&Instruction::ShieldDeed {
            id,
            deed_account: DEED_PDA,
            now: NOW,
        }),
        format!("18{id_hex}{}0068e5cf8b010000", "0a".repeat(32))
    );
    // 25 (0x19): UnshieldDeed
    assert_eq!(
        inst_hex(&Instruction::UnshieldDeed { id }),
        format!("19{id_hex}")
    );
    // 26 (0x1a): RefreshShieldedDeed
    assert_eq!(
        inst_hex(&Instruction::RefreshShieldedDeed { id }),
        format!("1a{id_hex}")
    );

    // Cross-program messages: Message::NftTransferHook (0x03) & NftAuthoritySyncMessage (0x02)
    assert_eq!(
        hex(to_bytes(&Message::NftTransferHook {
            id,
            token_id: [0x44; 32],
            expected_old_owner: [0x01; 32],
            new_owner: [0x02; 32],
            now: NOW,
        })),
        format!(
            "03{id_hex}{}{}{}0068e5cf8b010000",
            "44".repeat(32),
            "01".repeat(32),
            "02".repeat(32),
        )
    );
    assert_eq!(
        hex(to_bytes(&NftAuthoritySyncMessage(NftAuthorityOp::Transfer {
            token_id: [0x44; 32],
            new_owner: [0x02; 32],
        }))),
        format!("0201{}{}", "44".repeat(32), "02".repeat(32))
    );
    assert_eq!(
        hex(to_bytes(&NftAuthoritySyncMessage(NftAuthorityOp::Burn {
            token_id: [0x44; 32],
        }))),
        format!("0202{}", "44".repeat(32))
    );
    assert_eq!(
        hex(to_bytes(&NftAuthoritySyncMessage(
            NftAuthorityOp::SetExpiry {
                token_id: [0x44; 32],
                expires: u64::MAX,
            }
        ))),
        format!("0203{}ffffffffffffffff", "44".repeat(32))
    );
    assert_eq!(
        hex(to_bytes(&NftAuthoritySyncMessage(NftAuthorityOp::Shield {
            token_id: [0x44; 32],
            deed_account: DEED_PDA,
            expires: u64::MAX,
        }))),
        format!(
            "0204{}{}ffffffffffffffff",
            "44".repeat(32),
            "0a".repeat(32)
        )
    );
    assert_eq!(
        hex(to_bytes(&NftAuthoritySyncMessage(
            NftAuthorityOp::Unshield {
                token_id: [0x44; 32],
            }
        ))),
        format!("0205{}", "44".repeat(32))
    );
    assert_eq!(
        hex(to_bytes(&NftAuthoritySyncMessage(
            NftAuthorityOp::RefreshShielded {
                token_id: [0x44; 32],
                expires: u64::MAX,
            }
        ))),
        format!("0206{}ffffffffffffffff", "44".repeat(32))
    );

    // Account layouts (v8): NameRecord, SubnameRecord, MarketBucket, SiteBucket, SiteDirHeader
    let rec = NameRecord {
        version: VERSION,
        owner: [0x01; 32],
        expires: u64::MAX,
        sealed: vec![0xEE],
        claimed: false,
        price_tier: PRICE_TIER_5,
        generation: [0x99; 32],
        subname_sale: Some(SubnameSale {
            price: 100_000,
            payout: BOB,
        }),
        nft_token_id: [0x44; 32],
        nft_shielded: false,
        listing: Some(Listing {
            price: 10_000_000,
            pay_to: CAROL,
            expires_at: None,
            nonce: 1,
            in_market: true,
        }),
        listing_nonce: 1,
        site_listed: Some(0),
    };
    assert_eq!(
        hex(to_bytes(&rec)),
        format!(
            "08{}ffffffffffffffff01000000ee0000{}01a0860100000000000000000000000000{}{}000180969800000000000000000000000000{}000100000000000000010100000000000000010000",
            "01".repeat(32),
            "99".repeat(32),
            "02".repeat(32),
            "44".repeat(32),
            "03".repeat(32),
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
        format!("08{}0001000000ee", "02".repeat(32))
    );
    let sub_rev = SubnameRecord {
        version: VERSION,
        owner: [0x02; 32],
        kind: SubnameKind::Revocable {
            parent_generation: [0x99; 32],
        },
        sealed: Vec::new(),
    };
    assert_eq!(
        hex(to_bytes(&sub_rev)),
        format!("08{}01{}00000000", "02".repeat(32), "99".repeat(32))
    );
    let header = SiteDirHeader {
        version: VERSION,
        active_page: [0; 16],
        total_listed: 1,
    };
    assert_eq!(
        hex(to_bytes(&header)),
        format!("08{}01000000", "00".repeat(32))
    );
    let sb = SiteBucket {
        version: VERSION,
        entries: vec![SiteEntry {
            id,
            name: "alice".into(),
            title: "hi".into(),
            updated_at: NOW,
            name_expires: u64::MAX,
        }],
    };
    assert_eq!(
        hex(to_bytes(&sb)),
        format!(
            "0801000000{id_hex}05000000616c6963650200000068690068e5cf8b010000ffffffffffffffff"
        )
    );
    let mb = MarketBucket {
        version: VERSION,
        entries: vec![MarketEntry {
            id,
            price: 10_000_000,
            pay_to: BOB,
            expires_at: None,
            name_expires: u64::MAX,
            price_tier: PRICE_TIER_5,
            nonce: 1,
            nft_token_id: [0x44; 32],
            display_name: Some("alice".into()),
        }],
    };
    assert_eq!(
        hex(to_bytes(&mb)),
        format!(
            "0801000000{id_hex}80969800000000000000000000000000{}00ffffffffffffffff000100000000000000{}0105000000616c696365",
            "02".repeat(32),
            "44".repeat(32),
        )
    );
}

// ── Events & privacy invariants ─────────────────────────────────────────

#[test]
fn events_carry_the_identifier_and_never_a_name() {
    let s = slot(&JEF);
    let events = run_plan_events(on_registration(&JEF, ALICE), register(JEF, Vec::new()));
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].selector, event_selector(Registered::SELECTOR_NAME));
    assert_eq!(
        borsh::from_slice::<Registered>(&events[0].data).expect("decodes"),
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
            now: NOW,
        },
    );
    assert_eq!(
        events[0].selector,
        event_selector(Transferred::SELECTOR_NAME)
    );

    let events = run_plan_events(
        on_record_and_page(&JEF, ALICE),
        Instruction::Release { id: s, refund: 0 },
    );
    assert_eq!(events[0].selector, event_selector(Released::SELECTOR_NAME));

    assert!(run_plan_events(
        on_record(&JEF, ALICE),
        Instruction::SetRecord {
            id: s,
            sealed: vec![1],
            site_dir: SiteDirOp::Keep,
        },
    )
    .is_empty());
}

#[test]
fn no_effect_or_event_names_a_holder() {
    let holder = ALICE.into_value();
    let carries_holder = |bytes: &[u8]| bytes.windows(32).any(|window| window == holder);
    let s = slot(&JEF);
    let instructions = [
        (on_registration(&JEF, ALICE), register(JEF, vec![9])),
        (
            on_record(&JEF, ALICE),
            Instruction::SetRecord {
                id: s,
                sealed: vec![9],
                site_dir: SiteDirOp::Upsert {
                    name: "alice".into(),
                    title: "site".into(),
                    page: 0,
                    now: NOW,
                },
            },
        ),
        (
            on_record_and_page(&JEF, ALICE),
            Instruction::SetPage {
                id: s,
                sealed: vec![9],
                site_dir: SiteDirOp::Remove,
            },
        ),
        (
            on_record_and_page(&JEF, ALICE),
            Instruction::Transfer {
                id: s,
                new_owner: owner_tag(&s, &BOB),
                now: NOW,
            },
        ),
        (
            on_record_and_page(&JEF, ALICE),
            Instruction::Release { id: s, refund: 0 },
        ),
        (
            vec![meta(ALICE, true)],
            Instruction::List {
                id: s,
                price: 10_000_000,
                pay_to: CAROL, // dedicated public receiving account, not holder
                expires_at: Some(NOW + DAY_MS),
                now: NOW,
                display_name: Some("alice".into()),
            },
        ),
        (vec![meta(ALICE, true)], Instruction::Cancel { id: s }),
        (
            vec![native(ALICE)],
            Instruction::Buy {
                id: s,
                price: 10_000_000,
                pay_to: CAROL,
                treasury: BOB,
                expected_nonce: 1,
                now: NOW,
            },
        ),
        (
            vec![meta(ALICE, true)],
            Instruction::ShieldDeed {
                id: s,
                deed_account: DEED_PDA,
                now: NOW,
            },
        ),
        (vec![meta(ALICE, true)], Instruction::UnshieldDeed { id: s }),
        (
            vec![meta(ALICE, true)],
            Instruction::RefreshShieldedDeed { id: s },
        ),
        (
            vec![meta(ALICE, true)],
            Instruction::SetSubnameSale {
                id: s,
                sale: Some(SubnameSale {
                    price: 10,
                    payout: BOB,
                }),
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
    assert_ne!(owner_tag(&JEF, &ALICE), owner_tag(&ADA, &ALICE));
}

// ── Expiry, renewal, tiered pricing, commit-reveal, deposits, claims ────

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
            site_dir: SiteDirOp::Keep,
        },
    );
    let later = NOW + YEAR + GRACE_MS;
    chain.run(
        on_registration(&JEF, BOB),
        register_at(JEF, b"bob's profile".to_vec(), 0, 0, later),
    );
    let bob_tag = owner_tag(&s, &BOB);
    let generation = register_generation(&s, &bob_tag, later, &SECRET);
    assert_eq!(
        chain.record(&JEF),
        Some(NameRecord {
            version: VERSION,
            owner: bob_tag,
            expires: later + MONTH,
            sealed: b"bob's profile".to_vec(),
            claimed: false,
            price_tier: DEFAULT_TIER,
            generation,
            subname_sale: None,
            nft_token_id: nft_token_id(&s, &generation),
            nft_shielded: false,
            listing: None,
            listing_nonce: 0,
            site_listed: None,
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

    let late = record.expires + GRACE_MS - 1;
    chain.renew(&JEF, BOB, late);
    let record = chain.record(&JEF).expect("registered");
    assert_eq!(
        (record.owner, record.expires),
        (owner_tag(&slot(&JEF), &ALICE), late + MONTH)
    );
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
    chain.assert_nft_lockstep(&slot(&JEF));
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
    chain.assert_nft_lockstep(&slot(&JEF));
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
    // Permanent is 20x the 1-year price across every tier.
    for row in PRICES {
        assert_eq!(row[usize::from(TERM_PERMANENT)], row[usize::from(TERM_1Y)] * 20);
    }

    let id5: NameId = [0x51; 32];
    chain.run(
        paying(on_registration_tier(&id5, PRICE_TIER_5, ALICE), BOB),
        register_tier_at(id5, b"5char".to_vec(), TERM_1Y, 20_000_000, NOW, PRICE_TIER_5),
    );
    chain.assert_nft_lockstep(&slot_of(&id5, PRICE_TIER_5));

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
        chain
            .record_slot(&slot_of(&id5, PRICE_TIER_5))
            .expect("perm")
            .expires,
        u64::MAX
    );
    chain.assert_nft_lockstep(&slot_of(&id5, PRICE_TIER_5));

    let id6: NameId = [0x61; 32];
    chain.run(
        paying(on_registration_tier(&id6, PRICE_TIER_6_7, ALICE), BOB),
        register_tier_at(
            id6,
            b"6char".to_vec(),
            TERM_PERMANENT,
            100_000_000,
            NOW,
            PRICE_TIER_6_7,
        ),
    );
    let rec6 = chain.record_slot(&slot_of(&id6, PRICE_TIER_6_7)).expect("6char");
    assert_eq!((rec6.expires, rec6.price_tier), (u64::MAX, PRICE_TIER_6_7));
    chain.assert_nft_lockstep(&slot_of(&id6, PRICE_TIER_6_7));

    let id8: NameId = [0x81; 32];
    chain.run(
        paying(on_registration_tier(&id8, PRICE_TIER_8_PLUS, ALICE), BOB),
        register_tier_at(
            id8,
            b"8char".to_vec(),
            TERM_30D,
            100_000,
            NOW,
            PRICE_TIER_8_PLUS,
        ),
    );
    assert_eq!(
        chain
            .record_slot(&slot_of(&id8, PRICE_TIER_8_PLUS))
            .expect("8char")
            .expires,
        NOW + MONTH
    );
    chain.assert_nft_lockstep(&slot_of(&id8, PRICE_TIER_8_PLUS));
}

#[test]
fn paying_a_cheaper_tier_cannot_occupy_a_shorter_names_slot() {
    let mut chain = Chain::default();
    chain.configure(ALICE, ALICE, PRICES);
    let five_char_id: NameId = [0x55; 32];

    chain.run(
        paying(
            on_registration_tier(&five_char_id, PRICE_TIER_8_PLUS, CAROL),
            BOB,
        ),
        register_tier_at(
            five_char_id,
            b"spoof".to_vec(),
            TERM_1Y,
            1_000_000,
            NOW,
            PRICE_TIER_8_PLUS,
        ),
    );
    assert_eq!(chain.record_slot(&slot_of(&five_char_id, PRICE_TIER_5)), None);

    chain.run(
        paying(on_registration_tier(&five_char_id, PRICE_TIER_5, ALICE), BOB),
        register_tier_at(
            five_char_id,
            b"real".to_vec(),
            TERM_1Y,
            20_000_000,
            NOW,
            PRICE_TIER_5,
        ),
    );
    assert_eq!(
        chain
            .record_slot(&slot_of(&five_char_id, PRICE_TIER_5))
            .expect("real")
            .sealed,
        b"real"
    );
}

#[test]
fn admin_updates_one_price_cell_affecting_only_new_purchases_and_renewals() {
    let mut chain = Chain::default();
    chain.configure(ALICE, ALICE, PRICES);
    let id5: NameId = [0x52; 32];
    chain.run(
        paying(on_registration_tier(&id5, PRICE_TIER_5, CAROL), BOB),
        register_tier_at(id5, b"old".to_vec(), TERM_1Y, 20_000_000, NOW, PRICE_TIER_5),
    );
    chain.run(
        vec![meta(settings_address(&PROGRAM), false), meta(ALICE, true)],
        Instruction::SetPrice {
            price_tier: PRICE_TIER_5,
            term: TERM_1Y,
            price: 25_000_000,
        },
    );
    assert_eq!(chain.settings().expect("settings").prices[0][1], 25_000_000);
    assert_eq!(chain.settings().expect("settings").prices[0][3], 400_000_000);

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
        chain
            .record_slot(&slot_of(&id5, PRICE_TIER_5))
            .expect("renewed")
            .expires,
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
    assert_eq!(
        chain.record(&JEF).expect("registered").owner,
        owner_tag(&slot(&JEF), &BOB)
    );
    chain.assert_nft_lockstep(&slot(&JEF));
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
            premium_start: 0,
            nft_program: NFT_PROGRAM,
        },
    );
}

#[test]
#[should_panic(expected = "nft_program is immutable once configured")]
fn nft_program_cannot_be_changed_after_initial_configure() {
    let mut chain = Chain::default();
    chain.configure(PROGRAM, ALICE, PRICES);
    chain.run(
        vec![meta(settings_address(&PROGRAM), false), meta(ALICE, true)],
        Instruction::Configure {
            admin: ALICE,
            treasury: BOB,
            prices: PRICES,
            mode: MODE_FEE,
            deposits: [0, 0],
            claims_root: [0; 32],
            claims_until: 0,
            claims_program: PROGRAM,
            premium_start: 0,
            nft_program: AccountId::new([0x55; 32]),
        },
    );
}

#[test]
#[should_panic(expected = "nft_program cannot be the native token program")]
fn nft_program_cannot_be_zero() {
    Chain::default().run(
        vec![meta(settings_address(&PROGRAM), false), meta(PROGRAM, true)],
        Instruction::Configure {
            admin: ALICE,
            treasury: BOB,
            prices: PRICES,
            mode: MODE_FEE,
            deposits: [0, 0],
            claims_root: [0; 32],
            claims_until: 0,
            claims_program: PROGRAM,
            premium_start: 0,
            nft_program: native_token::NATIVE_TOKEN_PROGRAM_ID,
        },
    );
}

#[test]
fn a_name_is_claimed_by_commit_then_reveal() {
    let mut chain = Chain::default();
    chain.commit(&JEF, ALICE, NOW - MIN_COMMIT_AGE_MS);
    chain.reveal(&JEF, ALICE, NOW);
    assert_eq!(
        chain.record(&JEF).map(|r| r.owner),
        Some(owner_tag(&slot(&JEF), &ALICE))
    );
    assert!(chain.data(&committed(&JEF, ALICE)).is_empty());
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

#[test]
#[should_panic(expected = "no commitment to reveal")]
fn a_seen_reveal_cannot_be_replayed_for_another_account() {
    let mut chain = Chain::default();
    chain.commit(&JEF, ALICE, NOW - MIN_COMMIT_AGE_MS);
    chain.run_exactly(
        on_registration(&JEF, BOB),
        register_at(JEF, Vec::new(), 1, 0, NOW),
    );
}

#[test]
#[should_panic(expected = "this commitment was already made")]
fn a_live_commitment_cannot_be_reset() {
    let mut chain = Chain::default();
    chain.commit(&JEF, ALICE, NOW - MIN_COMMIT_AGE_MS);
    chain.commit(&JEF, ALICE, NOW);
}

#[test]
fn a_public_name_is_claimed_after_the_public_wait() {
    let mut chain = Chain::default();
    chain.commit_public(&JEF, ALICE, NOW - PUBLIC_MIN_COMMIT_AGE_MS);
    chain.reveal(&JEF, ALICE, NOW);
    assert_eq!(
        chain.record(&JEF).map(|r| r.owner),
        Some(owner_tag(&slot(&JEF), &ALICE))
    );
    assert!(chain.data(&committed(&JEF, ALICE)).is_empty());
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
    let commit_pub = Instruction::CommitPublic {
        commitment: promised,
        now: NOW,
    };
    let accounts = vec![
        meta(commitment_address(&PROGRAM, &promised), false),
        meta(CAROL, true),
    ];
    let window = plan(&input(accounts, &commit_pub), commit_pub)
        .output()
        .timestamp_validity_window;
    assert_eq!(
        (window.start(), window.end()),
        (Some(NOW), Some(NOW + PUBLIC_TIME_WINDOW_MS))
    );

    let reveal_window = |public: bool, made: u64, now: u64| {
        let receive = ReceiveInput {
            receiver: Actor::new(committed(&JEF, ALICE), PROGRAM),
            from: Some(Actor::new(ALICE, PROGRAM)),
            grants: Default::default(),
            pre_state: to_bytes(&Commitment {
                version: VERSION,
                made,
                public,
            })
            .into(),
            message: Vec::new(),
        };
        handle(&receive, Message::Effect(Effect::Reveal { now }))
            .validity
            .timestamps
    };

    let pub_reveal = NOW + PUBLIC_MIN_COMMIT_AGE_MS;
    let w = reveal_window(true, NOW, pub_reveal);
    assert_eq!(
        (w.start(), w.end()),
        (Some(pub_reveal), Some(pub_reveal + PUBLIC_TIME_WINDOW_MS))
    );

    let priv_reveal = NOW + MIN_COMMIT_AGE_MS;
    let w = reveal_window(false, NOW, priv_reveal);
    assert_eq!(
        (w.start(), w.end()),
        (Some(priv_reveal), Some(priv_reveal + TIME_WINDOW_MS))
    );
}

#[test]
#[should_panic(expected = "the commitment is too recent")]
fn a_private_commit_cannot_use_the_public_wait() {
    let mut chain = Chain::default();
    chain.commit(&JEF, BOB, NOW - TIME_WINDOW_MS + 1);
    chain.reveal(&JEF, BOB, NOW);
}

#[test]
fn a_public_attacker_racing_a_pending_private_reveal_still_waits_four_real_minutes() {
    let mut chain = Chain::default();
    chain.commit(&JEF, ALICE, NOW - MIN_COMMIT_AGE_MS);
    let bob_made = NOW - PUBLIC_TIME_WINDOW_MS + 1;
    chain.commit_public(&JEF, BOB, bob_made);

    const REAL_WAIT_MS: u64 = 4 * 60 * 1000;
    assert_eq!(PUBLIC_MIN_COMMIT_AGE_MS - PUBLIC_TIME_WINDOW_MS, REAL_WAIT_MS);
    assert_eq!(MIN_COMMIT_AGE_MS - TIME_WINDOW_MS, 5 * 60 * 1000);

    let mut bob_early = chain.clone();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        bob_early.reveal(&JEF, BOB, NOW + REAL_WAIT_MS);
    }))
    .is_err());

    chain.reveal(&JEF, ALICE, NOW);
    assert_eq!(
        chain.record(&JEF).map(|r| r.owner),
        Some(owner_tag(&slot(&JEF), &ALICE))
    );
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        chain.reveal(&JEF, BOB, NOW + REAL_WAIT_MS + 1);
    }))
    .is_err());
}

#[test]
fn the_deployer_configures_a_fresh_registry_and_then_only_its_admin() {
    let mut chain = Chain::default();
    assert_eq!(chain.settings(), None);
    chain.configure(CAROL, ALICE, PRICES);
    let custom = [[1, 2, 3, 4], [5, 6, 7, 8], [9, 10, 11, 12]];
    chain.configure_with_premium(ALICE, ALICE, custom, DEFAULT_PREMIUM_START);
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
            premium_start: DEFAULT_PREMIUM_START,
            nft_program: NFT_PROGRAM,
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
        borsh::to_vec(&native_token::Message::Transfer {
            to: BOB,
            amount: fee
        })
        .expect("the transfer serializes")
    );
    let window = output.timestamp_validity_window;
    assert!(window.is_valid_for(NOW) && !window.is_valid_for(NOW - 1));
}

#[test]
fn deposit_mode_and_shortlist_and_claims_keep_working_with_nft_lockstep() {
    let mut chain = Chain::default();
    chain.configure_deposits(CAROL);
    chain.register_with_deposit(&JEF, ALICE, DEPOSITS[0], NOW);
    chain.assert_nft_lockstep(&slot(&JEF));
    let t1 = chain.record(&JEF).expect("reg").nft_token_id;

    // Deposit transfer keeps deposit and syncs NFT.
    let s = slot(&JEF);
    chain.run(
        on_record_and_page(&JEF, ALICE),
        Instruction::Transfer {
            id: s,
            new_owner: owner_tag(&s, &BOB),
            now: NOW,
        },
    );
    chain.assert_nft_lockstep(&s);
    assert_eq!(chain.record(&JEF).expect("rec").nft_token_id, t1);

    // Release refunds deposit and burns NFT.
    let call = only_call(
        on_release(&JEF, BOB, DEPOSITS[0]),
        Instruction::Release {
            id: s,
            refund: DEPOSITS[0],
        },
    );
    assert_eq!(call.to, Actor::native_balance(deposit_address(&PROGRAM, &s)));
    assert!(call.pda_seeds.iter().eq([&seed(DEPOSIT_SEED_DOMAIN, &s)]));

    chain.run(
        on_release(&JEF, BOB, DEPOSITS[0]),
        Instruction::Release {
            id: s,
            refund: DEPOSITS[0],
        },
    );
    assert!(chain.record(&JEF).is_none());
    assert!(chain.nft(&t1).is_none(), "release must burn NFT");

    // Locked names remain admin-only.
    let mut non_admin = chain.clone();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        non_admin.register_with_deposit(&LOCKED_NAME, ALICE, DEPOSITS[0], NOW);
    }))
    .is_err());
    chain.register_with_deposit(&LOCKED_NAME, CAROL, 0, NOW);
    chain.assert_nft_lockstep(&slot(&LOCKED_NAME));

    // Legacy claim overwrites an unclaimed registration, burns old NFT, mints permanent NFT.
    let mut claim_chain = Chain::default();
    let leaves = claims(&[(JEF, ALICE), (ADA, BOB)]);
    claim_chain.open_claims(&leaves, NOW + YEAR);
    claim_chain.register_with_deposit(&JEF, BOB, DEPOSITS[0], NOW);
    let bob_token = claim_chain.record(&JEF).expect("bob reg").nft_token_id;
    claim_chain.assert_nft_lockstep(&s);

    claim_chain.claim(&JEF, ALICE, &leaves, 0, b"claimed");
    let claimed_rec = claim_chain.record(&JEF).expect("claimed");
    assert_eq!(claimed_rec.owner, owner_tag(&s, &ALICE));
    assert_eq!(claimed_rec.expires, u64::MAX);
    assert!(claimed_rec.claimed);
    assert_ne!(claimed_rec.nft_token_id, bob_token);
    assert!(
        claim_chain.nft(&bob_token).is_none(),
        "claim overwrite must burn old NFT"
    );
    claim_chain.assert_nft_lockstep(&s);
}

#[test]
#[should_panic(expected = "the length proof does not hold")]
fn a_short_name_cannot_pass_as_a_normal_one() {
    let around_ada = length_of(&ADA);
    let mut chain = Chain::default();
    chain.configure_deposits(CAROL);
    chain.run(
        with_deposit_account(&THREE_NAME, ALICE),
        deposit_registration_with(THREE_NAME, DEPOSITS[0], around_ada),
    );
}

#[test]
#[should_panic(expected = "only this program may send an effect")]
fn outsiders_cannot_send_effects() {
    let receiver = Actor::new(record_address(&PROGRAM, &JEF), PROGRAM);
    let receive = ReceiveInput {
        receiver,
        from: None,
        grants: BTreeSet::from([receiver]),
        pre_state: Default::default(),
        message: Vec::new(),
    };
    let _ = handle(
        &receive,
        Message::Effect(Effect::Remove {
            id: JEF,
            signer: owner_tag(&JEF, &ALICE),
        }),
    );
}

// ── NFT Lockstep (Path A & Path B, lapse, re-reg, ShieldDeed) ──────────

#[test]
fn nft_lockstep_holds_across_full_lifecycle_including_both_transfer_paths() {
    let mut chain = Chain::default();
    chain.configure(PROGRAM, ALICE, PRICES);
    let row = PRICES[usize::from(DEFAULT_TIER)];
    let s = slot(&JEF);

    // 1. Register -> mints T1
    chain.run(
        paying(on_registration(&JEF, ALICE), BOB),
        register_at(JEF, b"alice".to_vec(), TERM_1Y, row[usize::from(TERM_1Y)], NOW),
    );
    let t1 = chain.record(&JEF).expect("reg").nft_token_id;
    chain.assert_nft_lockstep(&s);
    assert_eq!(chain.nft(&t1).expect("nft").content, DEFAULT_DEED_CONTENT);

    // 2. Path A: Registry Transfer (ALICE -> CAROL), 0 protocol fee, keeps T1
    chain.transfers.clear();
    chain.run(
        on_record_and_page(&JEF, ALICE),
        Instruction::Transfer {
            id: s,
            new_owner: owner_tag(&s, &CAROL),
            now: NOW + 1,
        },
    );
    assert!(chain.transfers.is_empty(), "plain transfer is free");
    assert_eq!(chain.record(&JEF).expect("rec").nft_token_id, t1);
    assert_eq!(chain.record(&JEF).expect("rec").owner, owner_tag(&s, &CAROL));
    chain.assert_nft_lockstep(&s);

    // 3. Carol sets profile, page, site directory, and listing; then Path B: NFT-initiated transfer (CAROL -> DAVE)
    chain.run(
        on_record(&JEF, CAROL),
        Instruction::SetRecord {
            id: s,
            sealed: b"carol site".to_vec(),
            site_dir: SiteDirOp::Upsert {
                name: "jefname".into(),
                title: "Carol".into(),
                page: 0,
                now: NOW + 2,
            },
        },
    );
    chain.run(
        on_record_and_page(&JEF, CAROL),
        Instruction::SetPage {
            id: s,
            sealed: b"carol page".to_vec(),
            site_dir: SiteDirOp::Keep,
        },
    );
    chain.run(
        vec![meta(CAROL, true)],
        Instruction::List {
            id: s,
            price: 5_000_000,
            pay_to: CAROL,
            expires_at: None,
            now: NOW + 3,
            display_name: Some("jefname".into()),
        },
    );
    assert!(chain.market_bucket(bucket_of(&s)).is_some());
    assert!(chain.site_bucket(bucket_of(&s), 0).is_some());

    let gen_before_hook = chain.record(&JEF).expect("rec").generation;
    chain.transfers.clear();
    chain.nft_transfer(t1, CAROL, owner_tag(&s, &DAVE), NOW + 4);
    assert!(chain.transfers.is_empty(), "NFT-side plain transfer is free");
    let after_hook = chain.record(&JEF).expect("rec");
    assert_eq!(after_hook.owner, owner_tag(&s, &DAVE));
    assert_eq!(after_hook.nft_token_id, t1);
    assert!(after_hook.sealed.is_empty(), "profile cleared on NFT transfer");
    assert!(after_hook.listing.is_none(), "listing dies on NFT transfer");
    assert!(after_hook.site_listed.is_none(), "site listing dies on NFT transfer");
    assert_ne!(after_hook.generation, gen_before_hook, "generation advances");
    assert_eq!(chain.page(&JEF), None, "page cleared on NFT transfer");
    assert_eq!(chain.market_bucket(bucket_of(&s)), None, "market bucket cleared");
    assert_eq!(chain.site_bucket(bucket_of(&s), 0), None, "site bucket cleared");
    assert_eq!(chain.site_dir_header().expect("hdr").total_listed, 0);
    chain.assert_nft_lockstep(&s);

    // 4. Lapsed in grace: neither Registry Transfer nor NFT Transfer succeeds
    let in_grace = NOW + YEAR;
    let mut reg_in_grace = chain.clone();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        reg_in_grace.run(
            on_record_and_page(&JEF, DAVE),
            Instruction::Transfer {
                id: s,
                new_owner: owner_tag(&s, &ALICE),
                now: in_grace,
            },
        );
    }))
    .is_err());
    let mut nft_in_grace = chain.clone();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        nft_in_grace.nft_transfer(t1, DAVE, owner_tag(&s, &ALICE), in_grace);
    }))
    .is_err());

    // 5. Past grace: someone else re-registers -> burns T1 and mints T2 atomically!
    let past_grace = NOW + YEAR + GRACE_MS + PREMIUM_DURATION_MS;
    chain.run(
        paying(on_registration(&JEF, ALICE), BOB),
        register_at(
            JEF,
            b"reborn".to_vec(),
            TERM_1Y,
            row[usize::from(TERM_1Y)],
            past_grace,
        ),
    );
    let t2 = chain.record(&JEF).expect("re-reg").nft_token_id;
    assert_ne!(t1, t2);
    assert!(chain.nft(&t1).is_none(), "old lapsed token must be burned");
    chain.assert_nft_lockstep(&s);
}

#[test]
#[should_panic(expected = "transfer hook must come from the configured nft program")]
fn spoofed_nft_transfer_hook_from_wrong_program_is_refused() {
    let mut chain = Chain::default();
    chain.configure(PROGRAM, ALICE, PRICES);
    let row = PRICES[usize::from(DEFAULT_TIER)];
    let s = slot(&JEF);
    chain.run(
        paying(on_registration(&JEF, ALICE), BOB),
        register_at(JEF, Vec::new(), TERM_1Y, row[usize::from(TERM_1Y)], NOW),
    );
    let t1 = chain.record(&JEF).expect("reg").nft_token_id;
    let fake_program = AccountId::new([0x77; 32]);
    let hook = SyncMessage::new(
        Actor::new(record_address(&PROGRAM, &s), PROGRAM),
        &Message::NftTransferHook {
            id: s,
            token_id: t1,
            expected_old_owner: owner_tag(&s, &ALICE),
            new_owner: owner_tag(&s, &CAROL),
            now: NOW,
        },
    );
    chain.dispatch_messages(
        Actor::new(nft_token_address(&fake_program, &t1), fake_program),
        vec![hook],
    );
}

#[test]
#[should_panic(expected = "transfer hook must come from the token's pda")]
fn spoofed_nft_transfer_hook_from_wrong_token_pda_is_refused() {
    let mut chain = Chain::default();
    chain.configure(PROGRAM, ALICE, PRICES);
    let row = PRICES[usize::from(DEFAULT_TIER)];
    let s = slot(&JEF);
    chain.run(
        paying(on_registration(&JEF, ALICE), BOB),
        register_at(JEF, Vec::new(), TERM_1Y, row[usize::from(TERM_1Y)], NOW),
    );
    let t1 = chain.record(&JEF).expect("reg").nft_token_id;
    let hook = SyncMessage::new(
        Actor::new(record_address(&PROGRAM, &s), PROGRAM),
        &Message::NftTransferHook {
            id: s,
            token_id: t1,
            expected_old_owner: owner_tag(&s, &ALICE),
            new_owner: owner_tag(&s, &CAROL),
            now: NOW,
        },
    );
    chain.dispatch_messages(
        Actor::new(nft_token_address(&NFT_PROGRAM, &[0xFF; 32]), NFT_PROGRAM),
        vec![hook],
    );
}

#[test]
fn shield_unshield_and_refresh_shielded_deed_enforce_lockstep_and_blocks() {
    let mut chain = Chain::default();
    chain.configure(PROGRAM, ALICE, PRICES);
    let row = PRICES[usize::from(DEFAULT_TIER)];
    let s = slot(&JEF);
    chain.run(
        paying(on_registration(&JEF, ALICE), BOB),
        register_at(JEF, Vec::new(), TERM_1Y, row[usize::from(TERM_1Y)], NOW),
    );
    let t1 = chain.record(&JEF).expect("reg").nft_token_id;

    // Shield deed into private PDA
    chain.run(
        vec![meta(ALICE, true)],
        Instruction::ShieldDeed {
            id: s,
            deed_account: DEED_PDA,
            now: NOW,
        },
    );
    assert!(chain.record(&JEF).expect("rec").nft_shielded);
    assert_eq!(
        chain.nft_deeds.get(&DEED_PDA),
        Some(&MockPrivateDeed {
            token_id: t1,
            expires: NOW + YEAR
        })
    );
    chain.assert_nft_lockstep(&s);

    // While shielded: Transfer, List, and Release are all refused!
    let mut try_transfer = chain.clone();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        try_transfer.run(
            on_record_and_page(&JEF, ALICE),
            Instruction::Transfer {
                id: s,
                new_owner: owner_tag(&s, &CAROL),
                now: NOW,
            },
        );
    }))
    .is_err());

    let mut try_list = chain.clone();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        try_list.run(
            vec![meta(ALICE, true)],
            Instruction::List {
                id: s,
                price: 1_000_000,
                pay_to: CAROL,
                expires_at: None,
                now: NOW,
                display_name: None,
            },
        );
    }))
    .is_err());

    let mut try_release = chain.clone();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        try_release.run(
            on_record_and_page(&JEF, ALICE),
            Instruction::Release { id: s, refund: 0 },
        );
    }))
    .is_err());

    // Renewing while shielded works publicly; RefreshShieldedDeed updates the private deed's expires.
    chain.run(
        paying(on_renewal(&JEF, CAROL), BOB),
        Instruction::Renew {
            id: s,
            term: TERM_1Y,
            fee: row[usize::from(TERM_1Y)],
            treasury: None,
            now: NOW + 1,
        },
    );
    chain.assert_nft_lockstep(&s);
    assert_eq!(chain.nft_deeds.get(&DEED_PDA).expect("deed").expires, NOW + YEAR);

    chain.run(
        vec![meta(ALICE, true)],
        Instruction::RefreshShieldedDeed { id: s },
    );
    assert_eq!(
        chain.nft_deeds.get(&DEED_PDA).expect("deed").expires,
        NOW + 2 * YEAR
    );

    // Unshield unlocks Transfer & Release
    chain.run(vec![meta(ALICE, true)], Instruction::UnshieldDeed { id: s });
    assert!(!chain.record(&JEF).expect("rec").nft_shielded);
    assert!(chain.nft_deeds.get(&DEED_PDA).is_none());
    chain.assert_nft_lockstep(&s);

    chain.run(
        on_record_and_page(&JEF, ALICE),
        Instruction::Transfer {
            id: s,
            new_owner: owner_tag(&s, &CAROL),
            now: NOW + 2,
        },
    );
    chain.assert_nft_lockstep(&s);
}

#[test]
#[should_panic(expected = "cannot shield an unclaimed name while claims are open")]
fn cannot_shield_unclaimed_name_while_claims_period_is_open() {
    let mut chain = Chain::default();
    let leaves = claims(&[(JEF, ALICE)]);
    chain.open_claims(&leaves, NOW + YEAR);
    chain.register_with_deposit(&JEF, BOB, DEPOSITS[0], NOW);
    chain.run(
        vec![meta(BOB, true)],
        Instruction::ShieldDeed {
            id: slot(&JEF),
            deed_account: DEED_PDA,
            now: NOW,
        },
    );
}

// ── Atomic Secondary Sale: List / Cancel / Buy / Fee Split ──────────────

#[test]
fn sale_split_is_exact_2_5_percent_ceil_and_overflow_free() {
    assert_eq!(sale_split(40), (39, 1));
    assert_eq!(sale_split(41), (39, 2));
    assert_eq!(sale_split(79), (77, 2));
    assert_eq!(sale_split(80), (78, 2));
    assert_eq!(sale_split(10_000_000), (9_750_000, 250_000));
    for price in [40u128, 41, 99, 100, 1_000, 12_345_678, u128::MAX - 1, u128::MAX] {
        let (seller, fee) = sale_split(price);
        assert_eq!(seller + fee, price, "exact sum for {price}");
        assert!(fee >= 1, "fee >= 1 for {price}");
        assert!(seller >= 39, "seller >= 39 for {price}");
    }
}

#[test]
fn atomic_list_and_buy_splits_fee_exactly_and_transfers_name_and_nft() {
    let mut chain = Chain::default();
    chain.configure(PROGRAM, ALICE, PRICES);
    let row = PRICES[usize::from(DEFAULT_TIER)];
    let s = slot(&JEF);

    chain.run(
        paying(on_registration(&JEF, ALICE), BOB),
        register_at(JEF, b"alice".to_vec(), TERM_1Y, row[usize::from(TERM_1Y)], NOW),
    );
    chain.run(
        on_record_and_page(&JEF, ALICE),
        Instruction::SetPage {
            id: s,
            sealed: b"alice page".to_vec(),
            site_dir: SiteDirOp::Upsert {
                name: "jefsite".into(),
                title: "Jef".into(),
                page: 0,
                now: NOW,
            },
        },
    );
    let t1 = chain.record(&JEF).expect("reg").nft_token_id;
    let gen1 = chain.record(&JEF).expect("reg").generation;

    // Alice lists for 10,000,000 paid to DAVE (a fresh public receiving account), with display_name
    let price = 10_000_000u128;
    chain.run(
        vec![meta(ALICE, true)],
        Instruction::List {
            id: s,
            price,
            pay_to: DAVE,
            expires_at: Some(NOW + DAY_MS),
            now: NOW,
            display_name: Some("jefsite".into()),
        },
    );
    let rec = chain.record(&JEF).expect("listed");
    assert_eq!(
        rec.listing,
        Some(Listing {
            price,
            pay_to: DAVE,
            expires_at: Some(NOW + DAY_MS),
            nonce: 1,
            in_market: true,
        })
    );
    let mb = chain.market_bucket(bucket_of(&s)).expect("market bucket");
    assert_eq!(mb.entries.len(), 1);
    assert_eq!(mb.entries[0].display_name.as_deref(), Some("jefsite"));

    // Carol buys atomically -> 9,750,000 to DAVE + 250,000 to BOB (treasury)
    chain.transfers.clear();
    chain.run(
        vec![native(CAROL)],
        Instruction::Buy {
            id: s,
            price,
            pay_to: DAVE,
            treasury: BOB,
            expected_nonce: 1,
            now: NOW + 100,
        },
    );
    assert_eq!(
        chain.transfers,
        vec![(CAROL, DAVE, 9_750_000), (CAROL, BOB, 250_000)]
    );
    let bought = chain.record(&JEF).expect("bought");
    assert_eq!(bought.owner, owner_tag(&s, &CAROL));
    assert_eq!(bought.nft_token_id, t1, "token_id stays fixed across Buy");
    assert!(bought.sealed.is_empty(), "profile cleared on Buy");
    assert!(bought.listing.is_none(), "listing cleared on Buy");
    assert!(bought.site_listed.is_none(), "site directory cleared on Buy");
    assert_ne!(bought.generation, gen1, "generation advances on Buy");
    assert_eq!(chain.page(&JEF), None, "page cleared on Buy");
    assert_eq!(chain.market_bucket(bucket_of(&s)), None, "market bucket cleared");
    assert_eq!(chain.site_bucket(bucket_of(&s), 0), None, "site bucket cleared");
    chain.assert_nft_lockstep(&s);
}

#[test]
fn buy_when_pay_to_equals_treasury_emits_single_transfer_of_full_price() {
    let mut chain = Chain::default();
    chain.configure(PROGRAM, ALICE, PRICES);
    let row = PRICES[usize::from(DEFAULT_TIER)];
    let s = slot(&JEF);
    chain.run(
        paying(on_registration(&JEF, ALICE), BOB),
        register_at(JEF, Vec::new(), TERM_1Y, row[usize::from(TERM_1Y)], NOW),
    );
    // Seller sets pay_to = BOB (same as settings.treasury), OTC (display_name: None)
    chain.run(
        vec![meta(ALICE, true)],
        Instruction::List {
            id: s,
            price: 10_000_000,
            pay_to: BOB,
            expires_at: None,
            now: NOW,
            display_name: None,
        },
    );
    // OTC listing touches no market bucket
    assert_eq!(chain.market_bucket(bucket_of(&s)), None);

    chain.transfers.clear();
    chain.run(
        vec![native(CAROL)],
        Instruction::Buy {
            id: s,
            price: 10_000_000,
            pay_to: BOB,
            treasury: BOB,
            expected_nonce: 1,
            now: NOW + 1,
        },
    );
    assert_eq!(chain.transfers, vec![(CAROL, BOB, 10_000_000)]);
    chain.assert_nft_lockstep(&s);
}

#[test]
fn listing_front_running_expiry_transfer_release_and_grace_renewal_rules_hold() {
    let mut chain = Chain::default();
    chain.configure(PROGRAM, ALICE, PRICES);
    let row = PRICES[usize::from(DEFAULT_TIER)];
    let s = slot(&JEF);
    chain.run(
        paying(on_registration(&JEF, ALICE), BOB),
        register_at(JEF, Vec::new(), TERM_1Y, row[usize::from(TERM_1Y)], NOW),
    );

    // List at 1M (nonce 1), then relist at 2M (nonce 2) -> stale Buy with nonce 1 or old price fails
    chain.run(
        vec![meta(ALICE, true)],
        Instruction::List {
            id: s,
            price: 1_000_000,
            pay_to: DAVE,
            expires_at: Some(NOW + DAY_MS),
            now: NOW,
            display_name: Some("jefname".into()),
        },
    );
    chain.run(
        vec![meta(ALICE, true)],
        Instruction::List {
            id: s,
            price: 2_000_000,
            pay_to: DAVE,
            expires_at: Some(NOW + DAY_MS),
            now: NOW + 1,
            display_name: Some("jefname".into()),
        },
    );
    assert_eq!(chain.record(&JEF).expect("rec").listing_nonce, 2);

    let mut stale_nonce = chain.clone();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        stale_nonce.run(
            vec![native(CAROL)],
            Instruction::Buy {
                id: s,
                price: 2_000_000,
                pay_to: DAVE,
                treasury: BOB,
                expected_nonce: 1,
                now: NOW + 2,
            },
        );
    }))
    .is_err());

    // Self-buy is refused
    let mut self_buy = chain.clone();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        self_buy.run(
            vec![native(ALICE)],
            Instruction::Buy {
                id: s,
                price: 2_000_000,
                pay_to: DAVE,
                treasury: BOB,
                expected_nonce: 2,
                now: NOW + 2,
            },
        );
    }))
    .is_err());

    // Wrong treasury is refused
    let mut bad_treasury = chain.clone();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        bad_treasury.run(
            vec![native(CAROL)],
            Instruction::Buy {
                id: s,
                price: 2_000_000,
                pay_to: DAVE,
                treasury: CAROL,
                expected_nonce: 2,
                now: NOW + 2,
            },
        );
    }))
    .is_err());

    // Listing expiry blocks Buy at `now >= expires_at`
    let mut after_list_exp = chain.clone();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        after_list_exp.run(
            vec![native(CAROL)],
            Instruction::Buy {
                id: s,
                price: 2_000_000,
                pay_to: DAVE,
                treasury: BOB,
                expected_nonce: 2,
                now: NOW + DAY_MS,
            },
        );
    }))
    .is_err());

    // Re-listing as unlisted OTC removes previous MarketBucket entry
    chain.run(
        vec![meta(ALICE, true)],
        Instruction::List {
            id: s,
            price: 2_000_000,
            pay_to: DAVE,
            expires_at: None,
            now: NOW + 2,
            display_name: None,
        },
    );
    assert_eq!(chain.market_bucket(bucket_of(&s)), None);

    // Re-list publicly, then check Transfer kills listing
    chain.run(
        vec![meta(ALICE, true)],
        Instruction::List {
            id: s,
            price: 2_000_000,
            pay_to: DAVE,
            expires_at: None,
            now: NOW + 3,
            display_name: Some("jefname".into()),
        },
    );
    let mut after_transfer = chain.clone();
    after_transfer.run(
        on_record_and_page(&JEF, ALICE),
        Instruction::Transfer {
            id: s,
            new_owner: owner_tag(&s, &CAROL),
            now: NOW + 4,
        },
    );
    assert!(after_transfer.record(&JEF).expect("rec").listing.is_none());
    assert_eq!(after_transfer.market_bucket(bucket_of(&s)), None);
    // If Carol re-lists at the same price/pay_to, nonce increments past Alice's nonce (4 -> 5)
    after_transfer.run(
        vec![meta(CAROL, true)],
        Instruction::List {
            id: s,
            price: 2_000_000,
            pay_to: DAVE,
            expires_at: None,
            now: NOW + 5,
            display_name: None,
        },
    );
    assert_eq!(after_transfer.record(&JEF).expect("rec").listing_nonce, 5);

    // Release kills listing & removes from MarketBucket & burns NFT
    let mut after_release = chain.clone();
    let t1 = after_release.record(&JEF).expect("rec").nft_token_id;
    after_release.run(
        on_record_and_page(&JEF, ALICE),
        Instruction::Release { id: s, refund: 0 },
    );
    assert!(after_release.record(&JEF).is_none());
    assert_eq!(after_release.market_bucket(bucket_of(&s)), None);
    assert!(after_release.nft(&t1).is_none());

    // Name entering grace kills Buy; renewing during grace clears dead listing (never resurrects!)
    let mut in_grace_buy = chain.clone();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        in_grace_buy.run(
            vec![native(CAROL)],
            Instruction::Buy {
                id: s,
                price: 2_000_000,
                pay_to: DAVE,
                treasury: BOB,
                expected_nonce: 4,
                now: NOW + YEAR,
            },
        );
    }))
    .is_err());

    let mut grace_renew = chain.clone();
    grace_renew.run(
        paying(on_renewal(&JEF, ALICE), BOB),
        Instruction::Renew {
            id: s,
            term: TERM_1Y,
            fee: row[usize::from(TERM_1Y)],
            treasury: None,
            now: NOW + YEAR + 100,
        },
    );
    assert!(
        grace_renew.record(&JEF).expect("renewed").listing.is_none(),
        "dead listing must not resurrect on grace renewal"
    );
    assert_eq!(grace_renew.market_bucket(bucket_of(&s)), None);

    // Explicit Cancel works (even during grace) and removes from MarketBucket
    chain.run(vec![meta(ALICE, true)], Instruction::Cancel { id: s });
    assert!(chain.record(&JEF).expect("rec").listing.is_none());
    assert_eq!(chain.market_bucket(bucket_of(&s)), None);
}

// ── 21-Day Falling Re-Release Premium & SetPremium ──────────────────────

#[test]
fn rerelease_premium_halves_daily_over_21_days_and_reaches_zero_smoothly() {
    let start = DEFAULT_PREMIUM_START; // 4_000_000_000
    assert_eq!(rerelease_premium(0, 0), 0);
    assert_eq!(rerelease_premium(start, 0), 4_000_000_000);
    assert_eq!(rerelease_premium(start, DAY_MS / 2), 3_000_000_000);
    assert_eq!(rerelease_premium(start, DAY_MS), 2_000_000_000);
    assert_eq!(rerelease_premium(start, 2 * DAY_MS), 1_000_000_000);
    assert_eq!(rerelease_premium(start, 3 * DAY_MS), 500_000_000);
    assert_eq!(rerelease_premium(start, 7 * DAY_MS), 31_250_000);
    assert_eq!(rerelease_premium(start, 20 * DAY_MS), 3_814);
    assert_eq!(rerelease_premium(start, PREMIUM_DURATION_MS - 1), 1);
    assert_eq!(rerelease_premium(start, PREMIUM_DURATION_MS), 0);
    assert_eq!(rerelease_premium(start, PREMIUM_DURATION_MS + DAY_MS), 0);

    // Monotonically non-increasing across day boundaries and intra-day points
    let mut prev = u128::MAX;
    for step in 0..=(21 * 4) {
        let p = rerelease_premium(start, step * (DAY_MS / 4));
        assert!(p <= prev);
        prev = p;
    }
    // Overflow-free even at u128::MAX
    assert_eq!(rerelease_premium(u128::MAX, 0), u128::MAX);
    assert!(rerelease_premium(u128::MAX, DAY_MS / 2) < u128::MAX);
    assert_eq!(rerelease_premium(u128::MAX, PREMIUM_DURATION_MS), 0);
}

#[test]
fn rerelease_premium_applies_only_to_lapsed_names_and_is_admin_adjustable() {
    let mut chain = Chain::default();
    chain.configure_with_premium(PROGRAM, ALICE, PRICES, DEFAULT_PREMIUM_START);
    let row = PRICES[usize::from(DEFAULT_TIER)];
    let base_1y = row[usize::from(TERM_1Y)];

    // Fresh name pays base price only (no premium)
    chain.run(
        paying(on_registration(&JEF, CAROL), BOB),
        register_at(JEF, Vec::new(), TERM_1Y, base_1y, NOW),
    );

    // Voluntarily released name pays base price only (no premium)
    chain.run(
        on_record_and_page(&JEF, CAROL),
        Instruction::Release {
            id: slot(&JEF),
            refund: 0,
        },
    );
    chain.run(
        paying(on_registration(&JEF, CAROL), BOB),
        register_at(JEF, Vec::new(), TERM_1Y, base_1y, NOW + 100),
    );

    // Let it lapse past 30-day grace: day 0 of premium window requires base + 4,000,000,000
    let grace_end = NOW + 100 + YEAR + GRACE_MS;
    let mut underpay = chain.clone();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        underpay.run(
            paying(on_registration(&JEF, DAVE), BOB),
            register_at(JEF, Vec::new(), TERM_1Y, base_1y + DEFAULT_PREMIUM_START - 1, grace_end),
        );
    }))
    .is_err());

    // Day 1 (elapsed = 1 day): exact premium = 2,000,000,000 -> succeeds and pays 100% to treasury BOB
    let day1 = grace_end + DAY_MS;
    let owed_day1 = base_1y + rerelease_premium(DEFAULT_PREMIUM_START, DAY_MS);
    let mut buy_day1 = chain.clone();
    buy_day1.transfers.clear();
    buy_day1.run(
        paying(on_registration(&JEF, DAVE), BOB),
        register_at(JEF, Vec::new(), TERM_1Y, owed_day1, day1),
    );
    assert_eq!(buy_day1.transfers, vec![(DAVE, BOB, owed_day1)]);
    buy_day1.assert_nft_lockstep(&slot(&JEF));

    // Admin adjusts premium_start via SetPremium; non-admin is refused
    let mut non_admin = chain.clone();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        non_admin.run(
            vec![meta(settings_address(&PROGRAM), false), meta(CAROL, true)],
            Instruction::SetPremium { premium_start: 0 },
        );
    }))
    .is_err());

    // Admin lowers premium_start mid-proof -> a slightly higher pre-computed fee still succeeds (`fee >= owed`)
    chain.run(
        vec![meta(settings_address(&PROGRAM), false), meta(ALICE, true)],
        Instruction::SetPremium {
            premium_start: 1_000_000,
        },
    );
    assert_eq!(chain.settings().expect("settings").premium_start, 1_000_000);
    chain.run(
        paying(on_registration(&JEF, DAVE), BOB),
        register_at(JEF, Vec::new(), TERM_1Y, base_1y + 1_000_000, day1),
    );
    assert_eq!(
        chain.record(&JEF).expect("reg").owner,
        owner_tag(&slot(&JEF), &DAVE)
    );
}

// ── Sites Directory (`SiteBucket` & `SiteDirHeader`, Option A) ──────────

#[test]
fn site_directory_upsert_update_opt_out_renew_and_lifecycle_cleanup() {
    let mut chain = Chain::default();
    chain.configure(PROGRAM, ALICE, PRICES);
    let row = PRICES[usize::from(DEFAULT_TIER)];
    let s = slot(&JEF);
    let b = bucket_of(&s);

    chain.run(
        paying(on_registration(&JEF, ALICE), BOB),
        register_at(JEF, Vec::new(), TERM_1Y, row[usize::from(TERM_1Y)], NOW),
    );

    // First publish with discovery ON (SetRecord + Upsert)
    chain.run(
        on_record(&JEF, ALICE),
        Instruction::SetRecord {
            id: s,
            sealed: b"cid=abc".to_vec(),
            site_dir: SiteDirOp::Upsert {
                name: "jefname".into(),
                title: "Jef's Site".into(),
                page: 0,
                now: NOW + 10,
            },
        },
    );
    assert_eq!(chain.record(&JEF).expect("rec").site_listed, Some(0));
    let sb = chain.site_bucket(b, 0).expect("site bucket");
    assert_eq!(sb.entries.len(), 1);
    assert_eq!(sb.entries[0].name, "jefname");
    assert_eq!(sb.entries[0].title, "Jef's Site");
    assert_eq!(sb.entries[0].name_expires, NOW + YEAR);
    assert_eq!(chain.site_dir_header().expect("hdr").total_listed, 1);

    // Routine content edit with SiteDirOp::Keep does NOT touch SiteBucket
    let planned_keep = plan(
        &input(
            on_record(&JEF, ALICE),
            &Instruction::SetRecord {
                id: s,
                sealed: b"cid=def".to_vec(),
                site_dir: SiteDirOp::Keep,
            },
        ),
        Instruction::SetRecord {
            id: s,
            sealed: b"cid=def".to_vec(),
            site_dir: SiteDirOp::Keep,
        },
    );
    assert_eq!(planned_keep.output().effects.len(), 1);

    // Updating title in place keeps total_listed == 1
    chain.run(
        on_record(&JEF, ALICE),
        Instruction::SetRecord {
            id: s,
            sealed: b"cid=def".to_vec(),
            site_dir: SiteDirOp::Upsert {
                name: "jefname".into(),
                title: "New Title".into(),
                page: 0,
                now: NOW + 20,
            },
        },
    );
    assert_eq!(chain.site_bucket(b, 0).expect("sb").entries[0].title, "New Title");
    assert_eq!(chain.site_dir_header().expect("hdr").total_listed, 1);

    // Renew refreshes SiteEntry.name_expires!
    chain.run(
        paying(on_renewal(&JEF, ALICE), BOB),
        Instruction::Renew {
            id: s,
            term: TERM_1Y,
            fee: row[usize::from(TERM_1Y)],
            treasury: None,
            now: NOW + 30,
        },
    );
    assert_eq!(
        chain.site_bucket(b, 0).expect("sb").entries[0].name_expires,
        NOW + 2 * YEAR
    );

    // Opt-out via SetRecord(Remove) removes from SiteBucket and decrements total_listed
    chain.run(
        on_record(&JEF, ALICE),
        Instruction::SetRecord {
            id: s,
            sealed: b"discover=off".to_vec(),
            site_dir: SiteDirOp::Remove,
        },
    );
    assert_eq!(chain.record(&JEF).expect("rec").site_listed, None);
    assert_eq!(chain.site_bucket(b, 0), None);
    assert_eq!(chain.site_dir_header().expect("hdr").total_listed, 0);

    // Removing again when already unlisted is a safe no-op
    chain.run(
        on_record(&JEF, ALICE),
        Instruction::SetRecord {
            id: s,
            sealed: b"discover=off".to_vec(),
            site_dir: SiteDirOp::Remove,
        },
    );
    assert_eq!(chain.site_dir_header().expect("hdr").total_listed, 0);

    // Re-list via SetPage, then verify Release cleans up SiteBucket atomically
    chain.run(
        on_record_and_page(&JEF, ALICE),
        Instruction::SetPage {
            id: s,
            sealed: b"<html>hi</html>".to_vec(),
            site_dir: SiteDirOp::Upsert {
                name: "jefname".into(),
                title: "".into(),
                page: 0,
                now: NOW + 40,
            },
        },
    );
    assert_eq!(chain.site_dir_header().expect("hdr").total_listed, 1);
    chain.run(
        on_record_and_page(&JEF, ALICE),
        Instruction::Release { id: s, refund: 0 },
    );
    assert_eq!(chain.site_bucket(b, 0), None);
    assert_eq!(chain.site_dir_header().expect("hdr").total_listed, 0);
}

#[test]
fn site_directory_paginates_at_32_entries_blocks_page_griefing_and_prunes_past_grace() {
    let mut chain = Chain::default();
    let target_bucket = 5u8;

    // Collect 33 distinct base IDs whose slot_id lands in `target_bucket`.
    let mut base_ids = Vec::new();
    let mut counter: u32 = 1;
    while base_ids.len() < MAX_SITE_BUCKET_ENTRIES + 1 {
        let mut raw = [0u8; 32];
        raw[..4].copy_from_slice(&counter.to_le_bytes());
        raw[31] = 0x77;
        let s = slot(&raw);
        if bucket_of(&s) == target_bucket {
            base_ids.push(raw);
        }
        counter += 1;
    }

    // Register first 32 names (one with 30-day term, others 1-year) and list all 32 on page 0.
    for (idx, base) in base_ids[..MAX_SITE_BUCKET_ENTRIES].iter().enumerate() {
        let term = if idx == 0 { TERM_30D } else { TERM_1Y };
        chain.run(
            on_registration(base, ALICE),
            register_at(*base, Vec::new(), term, 0, NOW),
        );
        chain.run(
            on_record(base, ALICE),
            Instruction::SetRecord {
                id: slot(base),
                sealed: Vec::new(),
                site_dir: SiteDirOp::Upsert {
                    name: format!("site{idx}name"),
                    title: format!("Site {idx}"),
                    page: 0,
                    now: NOW + idx as u64,
                },
            },
        );
    }

    let hdr = chain.site_dir_header().expect("header");
    assert_eq!(hdr.total_listed, MAX_SITE_BUCKET_ENTRIES as u32);
    assert_eq!(
        hdr.active_page[usize::from(target_bucket)],
        1,
        "page 0 full at 32 -> active_page advances to 1"
    );
    assert_eq!(
        chain
            .site_bucket(target_bucket, 0)
            .expect("page 0")
            .entries
            .len(),
        MAX_SITE_BUCKET_ENTRIES
    );

    // 33rd name: trying page 0 fails (full), trying page 2 fails (exceeds active_page=1),
    // page 1 succeeds!
    let extra = base_ids[MAX_SITE_BUCKET_ENTRIES];
    chain.run(
        on_registration(&extra, ALICE),
        register_at(extra, Vec::new(), TERM_1Y, 0, NOW),
    );
    let mut try_page0 = chain.clone();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        try_page0.run(
            on_record(&extra, ALICE),
            Instruction::SetRecord {
                id: slot(&extra),
                sealed: Vec::new(),
                site_dir: SiteDirOp::Upsert {
                    name: "extrasite".into(),
                    title: "".into(),
                    page: 0,
                    now: NOW + 100,
                },
            },
        );
    }))
    .is_err());

    let mut try_page2 = chain.clone();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        try_page2.run(
            on_record(&extra, ALICE),
            Instruction::SetRecord {
                id: slot(&extra),
                sealed: Vec::new(),
                site_dir: SiteDirOp::Upsert {
                    name: "extrasite".into(),
                    title: "".into(),
                    page: 2,
                    now: NOW + 100,
                },
            },
        );
    }))
    .is_err());

    // After site0's 30d term + 30d grace lapses, inserting `extra` onto page 0 lazily prunes site0!
    let after_site0_grace = NOW + MONTH + GRACE_MS;
    let mut pruned_page0 = chain.clone();
    pruned_page0.run(
        on_record(&extra, ALICE),
        Instruction::SetRecord {
            id: slot(&extra),
            sealed: Vec::new(),
            site_dir: SiteDirOp::Upsert {
                name: "extrasite".into(),
                title: "Reused slot".into(),
                page: 0,
                now: after_site0_grace,
            },
        },
    );
    assert_eq!(
        pruned_page0
            .site_bucket(target_bucket, 0)
            .expect("page 0")
            .entries
            .len(),
        MAX_SITE_BUCKET_ENTRIES
    );
    assert_eq!(
        pruned_page0.site_dir_header().expect("hdr").total_listed,
        MAX_SITE_BUCKET_ENTRIES as u32
    );

    // Or inserting `extra` onto page 1 succeeds immediately while all 32 on page 0 are live!
    chain.run(
        on_record(&extra, ALICE),
        Instruction::SetRecord {
            id: slot(&extra),
            sealed: Vec::new(),
            site_dir: SiteDirOp::Upsert {
                name: "extrasite".into(),
                title: "On page 1".into(),
                page: 1,
                now: NOW + 100,
            },
        },
    );
    assert_eq!(
        chain
            .site_bucket(target_bucket, 1)
            .expect("page 1")
            .entries
            .len(),
        1
    );
    assert_eq!(
        chain.site_dir_header().expect("hdr").total_listed,
        (MAX_SITE_BUCKET_ENTRIES + 1) as u32
    );
}

#[test]
fn site_title_and_name_validation_rejects_control_chars_and_subnames() {
    assert!(validate_site_title("").is_ok());
    assert!(validate_site_title(&"a".repeat(MAX_SITE_TITLE_BYTES)).is_ok());
    assert!(validate_site_title(&"a".repeat(MAX_SITE_TITLE_BYTES + 1)).is_err());
    assert!(validate_site_title("bad\ntitle").is_err());
    assert!(validate_site_title("bad\x00title").is_err());
    assert!(validate_site_title("bad\x7ftitle").is_err());

    // Top-level only: dotted subname is refused in SiteDirOp::Upsert
    let mut chain = Chain::default();
    chain.register(&JEF, ALICE, b"");
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        chain.run(
            on_record(&JEF, ALICE),
            Instruction::SetRecord {
                id: slot(&JEF),
                sealed: Vec::new(),
                site_dir: SiteDirOp::Upsert {
                    name: "blog.alice".into(),
                    title: "".into(),
                    page: 0,
                    now: NOW,
                },
            },
        );
    }))
    .is_err());
}

// ── Subname tests (no subname NFTs, parent sales, follow rules) ─────────

#[test]
fn subnames_create_revoke_follow_rules_and_parent_sales_emit_no_nft() {
    let mut chain = Chain::default();
    chain.configure(PROGRAM, ALICE, PRICES);
    let row = PRICES[usize::from(DEFAULT_TIER)];
    let parent = slot(&JEF);
    chain.run(
        paying(on_registration(&JEF, ALICE), BOB),
        register_at(JEF, b"parent".to_vec(), TERM_1Y, row[usize::from(TERM_1Y)], NOW),
    );
    assert_eq!(chain.nft_tokens.len(), 1, "only top-level name gets an NFT");

    // Create revocable & permanent subnames -> still only 1 NFT (no subname NFTs!)
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
    assert_eq!(chain.nft_tokens.len(), 1, "subnames never mint NFTs");

    // Parent transfer detaches revocable subname while permanent stays; clears subname_sale.
    chain.run(
        vec![meta(ALICE, true)],
        Instruction::SetSubnameSale {
            id: parent,
            sale: Some(SubnameSale {
                price: 500_000,
                payout: ALICE,
            }),
        },
    );
    chain.run(
        on_record_and_page(&JEF, ALICE),
        Instruction::Transfer {
            id: parent,
            new_owner: owner_tag(&parent, &DAVE),
            now: NOW + 1,
        },
    );
    assert_eq!(chain.record(&JEF).expect("parent").subname_sale, None);
    chain.assert_nft_lockstep(&parent);

    // Bob's revocable subname is detached
    let mut bob_try = chain.clone();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        bob_try.run(
            vec![meta(BOB, true)],
            Instruction::SetSubnameRecord {
                parent_id: parent,
                sub_id: BLOG_SUB,
                sealed: b"still mine".to_vec(),
                now: Some(NOW + 2),
            },
        );
    }))
    .is_err());

    // Carol's permanent subname still works with now: None
    chain.run(
        vec![meta(CAROL, true)],
        Instruction::SetSubnameRecord {
            parent_id: parent,
            sub_id: SHOP_SUB,
            sealed: b"carol updated".to_vec(),
            now: None,
        },
    );
    assert_eq!(
        chain.subname(&parent, &SHOP_SUB).expect("perm").sealed,
        b"carol updated"
    );

    // Dave enables parent subname sales; Bob buys a permanent subname via commit-reveal (no NFT)
    chain.run(
        vec![meta(DAVE, true)],
        Instruction::SetSubnameSale {
            id: parent,
            sale: Some(SubnameSale {
                price: 500_000,
                payout: CAROL,
            }),
        },
    );
    chain.commit_subname(&parent, &BLOG_SUB, BOB, NOW + 3, true);
    chain.transfers.clear();
    chain.run_exactly(
        vec![native(BOB)],
        Instruction::BuySubname {
            parent_id: parent,
            sub_id: BLOG_SUB,
            sealed: b"bought".to_vec(),
            price: 500_000,
            payout: Some(CAROL),
            now: NOW + 3 + PUBLIC_MIN_COMMIT_AGE_MS,
            secret: SECRET,
        },
    );
    assert_eq!(chain.transfers, vec![(BOB, CAROL, 500_000)]);
    assert_eq!(
        chain.subname(&parent, &BLOG_SUB).expect("bought").kind,
        SubnameKind::Permanent
    );
    assert_eq!(chain.nft_tokens.len(), 1, "bought subnames do not mint NFTs");

    // Revoking permanent subname is refused
    let mut try_revoke = chain.clone();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        try_revoke.run(
            vec![meta(DAVE, true)],
            Instruction::RevokeSubname {
                parent_id: parent,
                sub_id: BLOG_SUB,
                now: NOW + 3 + PUBLIC_MIN_COMMIT_AGE_MS,
            },
        );
    }))
    .is_err());

    // Permanent subname page, transfer, and release work with now: None and touch no NFTs
    chain.run(
        vec![meta(CAROL, true)],
        Instruction::SetSubnamePage {
            parent_id: parent,
            sub_id: SHOP_SUB,
            sealed: b"shop page".to_vec(),
            now: None,
        },
    );
    assert!(chain.subname_page(&parent, &SHOP_SUB).is_some());
    chain.run(
        vec![meta(CAROL, true)],
        Instruction::TransferSubname {
            parent_id: parent,
            sub_id: SHOP_SUB,
            new_owner: owner_tag(&SHOP_SUB, &BOB),
            now: None,
        },
    );
    assert_eq!(chain.subname_page(&parent, &SHOP_SUB), None);
    chain.run(
        vec![meta(BOB, true)],
        Instruction::ReleaseSubname {
            parent_id: parent,
            sub_id: SHOP_SUB,
            now: None,
        },
    );
    assert_eq!(chain.subname(&parent, &SHOP_SUB), None);

    // Revocable subname can be created and explicitly revoked by the current parent holder
    let temp_sub: NameId = [0xC3; 32];
    chain.run(
        vec![meta(DAVE, true)],
        Instruction::CreateSubname {
            parent_id: parent,
            sub_id: temp_sub,
            owner: owner_tag(&temp_sub, &ALICE),
            revocable: true,
            sealed: b"temp".to_vec(),
            now: NOW + 10,
        },
    );
    chain.run(
        vec![meta(ALICE, true)],
        Instruction::SetSubnamePage {
            parent_id: parent,
            sub_id: temp_sub,
            sealed: b"temp page".to_vec(),
            now: Some(NOW + 11),
        },
    );
    chain.run(
        vec![meta(DAVE, true)],
        Instruction::RevokeSubname {
            parent_id: parent,
            sub_id: temp_sub,
            now: NOW + 12,
        },
    );
    assert_eq!(chain.subname(&parent, &temp_sub), None);
    assert_eq!(chain.subname_page(&parent, &temp_sub), None);
    assert_eq!(chain.nft_tokens.len(), 1);
}

#[test]
fn renew_bounds_time_window_and_refreshes_active_listing_expiry() {
    let mut chain = Chain::default();
    chain.configure(PROGRAM, ALICE, PRICES);
    let row = PRICES[usize::from(DEFAULT_TIER)];
    let s = slot(&JEF);
    chain.run(
        paying(on_registration(&JEF, ALICE), BOB),
        register_at(JEF, Vec::new(), TERM_1Y, row[usize::from(TERM_1Y)], NOW),
    );
    chain.run(
        vec![meta(ALICE, true)],
        Instruction::List {
            id: s,
            price: 1_000_000,
            pay_to: DAVE,
            expires_at: None,
            now: NOW + 1,
            display_name: Some("jefname".into()),
        },
    );

    let renew_inst = Instruction::Renew {
        id: s,
        term: TERM_1Y,
        fee: row[usize::from(TERM_1Y)],
        treasury: None,
        now: NOW + 2,
    };
    let window = plan(
        &input(paying(on_renewal(&JEF, ALICE), BOB), &renew_inst),
        renew_inst.clone(),
    )
    .output()
    .timestamp_validity_window;
    assert_eq!(
        (window.start(), window.end()),
        (Some(NOW + 2), Some(NOW + 2 + TIME_WINDOW_MS))
    );

    chain.run(paying(on_renewal(&JEF, ALICE), BOB), renew_inst);
    assert_eq!(
        chain
            .market_bucket(bucket_of(&s))
            .expect("bucket")
            .entries[0]
            .name_expires,
        NOW + 2 * YEAR
    );
    chain.assert_nft_lockstep(&s);
}
