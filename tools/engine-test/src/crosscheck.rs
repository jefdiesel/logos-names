//! The registry and lez_nft v3 must agree on every byte they send each other: encode with
//! one crate, decode with the other.

use lee_core::account::AccountId;
use lez_nft_core as nft;
use names_core::{
    LinkedAuthority, Message, NFT_MESSAGE_TAG_AUTHORITY_SYNC, NftAuthorityOp, NftAuthoritySyncMessage,
    nft_deed_seed, nft_token_address, nft_token_seed, owner_tag, record_address, settings_address,
};

fn to_nft(op: NftAuthorityOp) -> nft::AuthorityOp {
    let bytes = borsh::to_vec(&NftAuthoritySyncMessage(op)).expect("bytes");
    match borsh::from_slice::<nft::Message>(&bytes).expect("lez_nft decodes what the registry sends") {
        nft::Message::AuthoritySync(op) => op,
        other => panic!("not an AuthoritySync: {other:?}"),
    }
}

pub fn run() {
    let registry = AccountId::new([0x07; 32]);
    let nft_program = AccountId::new([0x4E; 32]);
    let slot = [0x11; 32];
    let token_id = [0xAA; 32];
    let account = AccountId::new([0x22; 32]);
    let deed = AccountId::new([0xDD; 32]);

    // Tags and derivations.
    assert_eq!(NFT_MESSAGE_TAG_AUTHORITY_SYNC, 2);
    assert_eq!(nft::REGISTRY_NFT_TRANSFER_HOOK_TAG, 3);
    assert_eq!(owner_tag(&slot, &account), nft::linked_owner_tag(&slot, &account), "one owner tag in both programs");
    assert_eq!(nft_token_seed(&token_id), nft::token_seed(&token_id));
    assert_eq!(nft_token_address(&nft_program, &token_id), nft::token_address(&nft_program, &token_id));
    assert_eq!(nft_deed_seed(&token_id), nft::deed_seed(&token_id));

    // Registry -> NFT, every operation the registry can send.
    let linked = LinkedAuthority {
        program: registry,
        authority_actor: settings_address(&registry),
        target: slot,
        record_actor: record_address(&registry, &slot),
    };
    let nft_linked = nft::LinkedAuthority {
        program: registry,
        authority_actor: settings_address(&registry),
        target: slot,
        record_actor: record_address(&registry, &slot),
    };
    assert_eq!(borsh::to_vec(&linked).expect("bytes"), borsh::to_vec(&nft_linked).expect("bytes"));
    let owner = owner_tag(&slot, &account);
    let content = names_core::DEFAULT_DEED_CONTENT.to_vec();
    let cases = [
        (
            NftAuthorityOp::Mint { token_id, owner, linked: linked.clone(), content: content.clone(), expires: 42 },
            nft::AuthorityOp::Mint { token_id, owner, linked: nft_linked, content, expires: 42 },
        ),
        (
            NftAuthorityOp::Transfer { token_id, new_owner: owner },
            nft::AuthorityOp::Transfer { token_id, new_owner: owner },
        ),
        (NftAuthorityOp::Burn { token_id }, nft::AuthorityOp::Burn { token_id }),
        (
            NftAuthorityOp::SetExpiry { token_id, expires: u64::MAX },
            nft::AuthorityOp::SetExpiry { token_id, expires: u64::MAX },
        ),
        (
            NftAuthorityOp::Shield { token_id, deed_account: deed, expires: 7 },
            nft::AuthorityOp::Shield { token_id, deed_account: deed, expires: 7 },
        ),
        (NftAuthorityOp::Unshield { token_id }, nft::AuthorityOp::Unshield { token_id }),
        (
            NftAuthorityOp::RefreshShielded { token_id, expires: 9 },
            nft::AuthorityOp::RefreshShielded { token_id, expires: 9 },
        ),
    ];
    let count = cases.len();
    for (sent, expected) in cases {
        assert_eq!(to_nft(sent), expected);
    }

    // NFT -> registry: the transfer hook.
    let hook = nft::RegistryTransferHook {
        id: slot,
        token_id,
        expected_old_owner: [0x01; 32],
        new_owner: [0x02; 32],
        now: 1_234,
    };
    let bytes = borsh::to_vec(&hook).expect("bytes");
    assert_eq!(bytes.len(), 137);
    assert_eq!(
        borsh::from_slice::<Message>(&bytes).expect("the registry decodes the NFT's hook"),
        Message::NftTransferHook {
            id: slot,
            token_id,
            expected_old_owner: [0x01; 32],
            new_owner: [0x02; 32],
            now: 1_234,
        }
    );
    println!("ok   cross-check: {count} AuthoritySync ops, the transfer hook, owner tags and PDAs agree byte for byte");
}
