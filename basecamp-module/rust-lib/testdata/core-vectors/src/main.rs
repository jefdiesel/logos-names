use lee_core::account::AccountId;
fn h(b: &[u8]) -> String { hex::encode(b) }
fn main() {
    let nft = AccountId::new([0x11; 32]);
    let reg = AccountId::new([0x22; 32]);
    let slot = [0x33u8; 32];
    let owner = [0x44u8; 32];
    let secret = [0x55u8; 32];
    let token = [0xAAu8; 32];
    let now: u64 = 1_700_000_000_000;
    // lez_nft transfer of a linked token
    let ix = lez_nft_core::Message::Instruction(lez_nft_core::Instruction::Transfer {
        token_id: token, new_owner: [0xBB; 32], sealed: None, now: Some(42), linked_target: Some([0xCC; 32]) });
    println!("nft_transfer_hex={}", h(&borsh::to_vec(&ix).unwrap()));
    println!("nft_config_address={}", h(lez_nft_core::config_address(&nft).value()));
    println!("nft_token_address={}", h(lez_nft_core::token_address(&nft, &token).value()));
    println!("core_nft_token_address={}", h(names_core::nft_token_address(&nft, &token).value()));
    let rg = names_core::register_generation(&slot, &owner, now, &secret);
    println!("register_generation={}", h(&rg));
    let cg = names_core::claim_generation(&slot, &owner, now);
    println!("claim_generation={}", h(&cg));
    println!("nft_token_id_register={}", h(&names_core::nft_token_id(&slot, &rg)));
    println!("tiered_id_0={}", h(&names_core::tiered_id(&slot, 0)));
    println!("tiered_id_2={}", h(&names_core::tiered_id(&slot, 2)));
    println!("owner_tag={}", h(&names_core::owner_tag(&slot, &reg)));
    println!("market_bucket_address_3={}", h(names_core::market_bucket_address(&reg, 3).value()));
    println!("site_dir_header_address={}", h(names_core::site_dir_header_address(&reg).value()));
    println!("site_bucket_address_3_7={}", h(names_core::site_bucket_address(&reg, 3, 7).value()));
    println!("subname_record_address={}", h(names_core::subname_record_address(&reg, &slot, &owner).value()));
    println!("subname_page_address={}", h(names_core::subname_page_address(&reg, &slot, &owner).value()));
    println!("subname_commitment={}", h(&names_core::subname_commitment(&slot, &owner, &[0x66;32], &secret)));
    println!("settings_address={}", h(names_core::settings_address(&reg).value()));
    for p in [0u128, 39, 40, 41, 1000, 123_456_789, u128::MAX] { let (a,b)=names_core::sale_split(p); println!("sale_split_{p}={a},{b}"); }
    for e in [0u64, 1, 43_200_000, 86_400_000, 129_600_000, 20*86_400_000+1, 21*86_400_000-1, 21*86_400_000] {
        println!("premium_4e9_{e}={}", names_core::rerelease_premium(4_000_000_000, e)); }
    println!("premium_max_1={}", names_core::rerelease_premium(u128::MAX, 1));
    let out = std::env::args().nth(1).unwrap_or_else(|| "../v8-core-vectors.json".into());
    std::fs::write(&out, vectors()).unwrap();
}

// Writes core-encoded vectors for the module tests (basecamp-module/rust-lib/testdata/v8-core-vectors.json).
#[allow(dead_code)]
pub fn vectors() -> String {
    use names_core::*;
    let a = |b: u8| AccountId::new([b; 32]);
    let id = [0x31u8; 32]; let sub = [0x32u8; 32]; let tag = [0x41u8; 32]; let secret = [0x51u8; 32];
    let r#gen = [0x61u8; 32]; let tok = [0x71u8; 32];
    let ix = |i: Instruction| hex::encode(borsh::to_vec(&Message::Instruction(i)).unwrap());
    let mut v = serde_json_lite::Obj::default();
    let length = LengthProof::Membership(shortlist::Membership { index: 7, tier: TIER_NORMAL, path: vec![[0x81; 32], [0x82; 32]] });
    v.put("register", ix(Instruction::Register { id, sealed: vec![1, 2, 3], term: TERM_PERMANENT, fee: 20_000_000, treasury: Some(a(0x91)), now: 1_700_000_000_000, secret, deposit: false, length, price_tier: PRICE_TIER_6_7 }));
    v.put("renew", ix(Instruction::Renew { id, term: TERM_1Y, fee: 5_000_000, treasury: Some(a(0x91)), now: 1_700_000_000_001 }));
    v.put("set_record_keep", ix(Instruction::SetRecord { id, sealed: vec![9], site_dir: SiteDirOp::Keep }));
    v.put("set_page_upsert", ix(Instruction::SetPage { id, sealed: vec![8, 7], site_dir: SiteDirOp::Upsert { name: "alice".into(), title: "Alice's site".into(), page: 2, now: 5 } }));
    v.put("set_record_remove", ix(Instruction::SetRecord { id, sealed: vec![], site_dir: SiteDirOp::Remove }));
    v.put("transfer", ix(Instruction::Transfer { id, new_owner: tag, now: 77 }));
    v.put("release", ix(Instruction::Release { id, refund: 1_000_000 }));
    v.put("commit", ix(Instruction::Commit { commitment: [0xC1; 32], now: 11 }));
    v.put("commit_public", ix(Instruction::CommitPublic { commitment: [0xC2; 32], now: 12 }));
    v.put("set_subname_sale_on", ix(Instruction::SetSubnameSale { id, sale: Some(SubnameSale { price: 500, payout: a(0x92) }) }));
    v.put("set_subname_sale_off", ix(Instruction::SetSubnameSale { id, sale: None }));
    v.put("create_subname", ix(Instruction::CreateSubname { parent_id: id, sub_id: sub, owner: tag, revocable: true, sealed: vec![4], now: 13 }));
    v.put("revoke_subname", ix(Instruction::RevokeSubname { parent_id: id, sub_id: sub, now: 14 }));
    v.put("buy_subname", ix(Instruction::BuySubname { parent_id: id, sub_id: sub, sealed: vec![5, 6], price: 500, payout: Some(a(0x92)), now: 15, secret }));
    v.put("list", ix(Instruction::List { id, price: 1000, pay_to: a(0x93), expires_at: Some(99), now: 16, display_name: Some("alice".into()) }));
    v.put("list_unlisted", ix(Instruction::List { id, price: 40, pay_to: a(0x93), expires_at: None, now: 17, display_name: None }));
    v.put("cancel", ix(Instruction::Cancel { id }));
    v.put("buy", ix(Instruction::Buy { id, price: 1000, pay_to: a(0x93), treasury: a(0x91), expected_nonce: 3, now: 18 }));
    let rec = NameRecord { version: VERSION, owner: tag, expires: 123, sealed: vec![1, 2], claimed: false, price_tier: 1, generation: r#gen,
        subname_sale: Some(SubnameSale { price: 500, payout: a(0x92) }), nft_token_id: tok, nft_shielded: true,
        listing: Some(Listing { price: 1000, pay_to: a(0x93), expires_at: Some(99), nonce: 3, in_market: true }), listing_nonce: 3, site_listed: Some(2) };
    v.put("name_record", hex::encode(borsh::to_vec(&rec).unwrap()));
    let rec2 = NameRecord { subname_sale: None, listing: None, site_listed: None, nft_shielded: false, expires: u64::MAX, claimed: true, ..rec };
    v.put("name_record_plain", hex::encode(borsh::to_vec(&rec2).unwrap()));
    let set = Settings { version: VERSION, admin: a(0xA1), treasury: a(0x91), prices: DEFAULT_PRICES, mode: MODE_FEE, deposits: [7, 8], claims_root: [0xB1; 32], claims_until: 19, claims_program: a(0xB2), premium_start: DEFAULT_PREMIUM_START, nft_program: a(0x11) };
    v.put("settings", hex::encode(borsh::to_vec(&set).unwrap()));
    let sr = SubnameRecord { version: VERSION, owner: tag, kind: SubnameKind::Revocable { parent_generation: r#gen }, sealed: vec![3] };
    v.put("subname_record", hex::encode(borsh::to_vec(&sr).unwrap()));
    let mb = MarketBucket { version: VERSION, entries: vec![MarketEntry { id, price: 1000, pay_to: a(0x93), expires_at: Some(99), name_expires: 123, price_tier: 1, nonce: 3, nft_token_id: tok, display_name: Some("alice".into()) }] };
    v.put("market_bucket", hex::encode(borsh::to_vec(&mb).unwrap()));
    let sb = SiteBucket { version: VERSION, entries: vec![SiteEntry { id, name: "alice".into(), title: "Alice".into(), updated_at: 5, name_expires: 123 }] };
    v.put("site_bucket", hex::encode(borsh::to_vec(&sb).unwrap()));
    let mut ap = [0u16; 16]; ap[3] = 2;
    v.put("site_dir_header", hex::encode(borsh::to_vec(&SiteDirHeader { version: VERSION, active_page: ap, total_listed: 9 }).unwrap()));
    // derivations
    let reg = a(0x22); let nft = a(0x11);
    v.put("reg", hex::encode(reg.value())); v.put("nft", hex::encode(nft.value()));
    v.put("register_generation", hex::encode(register_generation(&id, &tag, 1_700_000_000_000, &secret)));
    v.put("nft_token_id", hex::encode(nft_token_id(&id, &r#gen)));
    v.put("nft_token_address", hex::encode(nft_token_address(&nft, &tok).value()));
    v.put("nft_config_address", hex::encode(lez_nft_core::config_address(&nft).value()));
    v.put("nft_transfer", hex::encode(borsh::to_vec(&lez_nft_core::Message::Instruction(lez_nft_core::Instruction::Transfer { token_id: tok, new_owner: tag, sealed: None, now: Some(42), linked_target: Some(id) })).unwrap()));
    v.put("tiered_id_1", hex::encode(tiered_id(&id, 1)));
    v.put("market_bucket_address_3", hex::encode(market_bucket_address(&reg, 3).value()));
    v.put("site_dir_header_address", hex::encode(site_dir_header_address(&reg).value()));
    v.put("site_bucket_address_3_7", hex::encode(site_bucket_address(&reg, 3, 7).value()));
    v.put("subname_record_address", hex::encode(subname_record_address(&reg, &id, &sub).value()));
    v.put("subname_page_address", hex::encode(subname_page_address(&reg, &id, &sub).value()));
    v.put("subname_commitment", hex::encode(subname_commitment(&id, &sub, &tag, &secret)));
    v.put("bucket_of", format!("{}", bucket_of(&[0xA7; 32])));
    v.put("sale_split", [0u128, 39, 40, 41, 1000, 123_456_789].iter().map(|p| { let (s, f) = sale_split(*p); format!("{p}:{s}:{f}") }).collect::<Vec<_>>().join(","));
    v.put("premium", [0u64, 1, 43_200_000, 86_400_000, 129_600_000, 20 * 86_400_000 + 1, 21 * 86_400_000 - 1, 21 * 86_400_000].iter().map(|e| format!("{e}:{}", rerelease_premium(DEFAULT_PREMIUM_START, *e))).collect::<Vec<_>>().join(","));
    v.finish()
}

mod serde_json_lite {
    #[derive(Default)] pub struct Obj(Vec<(String, String)>);
    impl Obj {
        pub fn put(&mut self, k: &str, v: String) { self.0.push((k.into(), v)); }
        pub fn finish(&self) -> String {
            let body: Vec<String> = self.0.iter().map(|(k, v)| format!("  \"{k}\": \"{v}\"")).collect();
            format!("{{\n{}\n}}\n", body.join(",\n"))
        }
    }
}
