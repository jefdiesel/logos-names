//! Deeds: each name of the normal tier has a 1-of-1 NFT of Logos's NFT program
//! (`logos-blockchain/lez-programs`, `programs/nft`, branch `refactor/lez-v0.4.x`
//! at `dea23c22a47c728c9b6f925d0ef1677f99613f09`, used unmodified).
//!
//! - Each deed is its own definition of printable supply 1, so its master is the
//!   deed and no copy can ever be printed. The definition's account is a registry
//!   PDA seeded by the name and a nonce new to each registration
//!   ([`crate::deed_definition_address`]): a released name registered again gets
//!   a new deed, and a used definition is never reused.
//! - The deed sits in a registry PDA per (deed, holder tag)
//!   ([`crate::deed_holding_address`]). A PDA has no key, so only the registry's
//!   seed grant authorizes it: nobody can move or burn a deed through the NFT
//!   program directly. An LNS transfer moves the name and its deed together.
//! - The deed's public metadata says it is an LNS name deed, never which name.
//!
//! The NFT program's messages are encoded here with the same Borsh layout as its
//! `nft_core` crate (variant order and fields), so the registry does not link it.

use borsh::{BorshDeserialize, BorshSerialize};
use lee_core::account::AccountId;

/// The NFT program deployment deeds are minted on: its header account
/// `CjrUpegdA1RjqDN43T8P19kAkwnD1quAkNkER8k3dw9G`. The same header key deploys it
/// on every chain this registry runs on, so this id holds on each.
pub const NFT_PROGRAM: AccountId = AccountId::new(crate::hex_literal(
    b"ae6ae5a379046c0c21b749d116a3e5ca3c6e82d0687742963744cef2e0c13fd7",
));

/// The image id of the NFT program pinned above: `nft.bin` built natively from
/// `dea23c2` (SHA-256 `0e571274…2ab2a4`, 210,704 bytes). Clients check the
/// deployment holds it.
pub const NFT_PROGRAM_IMAGE_ID: [u8; 32] =
    crate::hex_literal(b"4fbb28723a040e0f6da48d8c8c5f839f181086112673cb210fabb6047bbddf58");

/// The public name of every deed. It never names the LNS name.
pub const DEED_NAME: &str = "LNS name deed";
/// What a deed's `uri` points at: a generic description of LNS deeds.
pub const DEED_URI: &str = "https://nameswallet.app/lns-deed";
/// A deed's `creators`: free text with no effect in the NFT program.
pub const DEED_CREATORS: &str = "LNS registry";

/// The NFT program's `MetadataStandard`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum MetadataStandard {
    Simple,
    Expanded,
}

/// The NFT program's `NftMetadata`.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NftMetadata {
    pub standard: MetadataStandard,
    pub uri: String,
    pub creators: String,
}

/// The NFT program's public `Message`, in its variant order.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum NftMessage {
    NewDefinition {
        name: String,
        printable_supply: u128,
        metadata: NftMetadata,
        master: AccountId,
    },
    InitializeHolding { definition: AccountId },
    Transfer { to: AccountId },
    PrintNft { to: AccountId },
    Burn { amount: u128 },
    ReadState,
}

/// The NFT program's actor state, in its variant order.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum NftState {
    Definition(NftDefinition),
    Master(NftMaster),
    Copy(NftCopy),
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NftDefinition {
    pub name: String,
    pub printable_supply: u128,
    pub metadata: NftMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NftMaster {
    pub definition_id: AccountId,
    pub print_balance: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NftCopy {
    pub definition_id: AccountId,
    pub owned: bool,
}

/// The `NewDefinition` that creates a deed with `master` as its holder.
#[must_use]
pub fn new_definition(master: AccountId) -> NftMessage {
    NftMessage::NewDefinition {
        name: DEED_NAME.to_owned(),
        printable_supply: 1,
        metadata: NftMetadata {
            standard: MetadataStandard::Simple,
            uri: DEED_URI.to_owned(),
            creators: DEED_CREATORS.to_owned(),
        },
        master,
    }
}

/// Decodes an NFT actor state; `None` for an empty one or one that is not an NFT state.
#[must_use]
pub fn decode_state(bytes: &[u8]) -> Option<NftState> {
    (!bytes.is_empty()).then(|| borsh::from_slice(bytes).ok()).flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The layouts the NFT program decodes (`nft_core` at dea23c2): a one-byte tag in
    /// declaration order, then the fields; strings as a u32 length and the bytes.
    #[test]
    fn messages_are_laid_out_as_the_nft_program_reads_them() {
        let a = AccountId::new([7; 32]);
        let mut expected = vec![0u8];
        for text in [DEED_NAME] {
            expected.extend(u32::try_from(text.len()).expect("short").to_le_bytes());
            expected.extend(text.as_bytes());
        }
        expected.extend(1u128.to_le_bytes());
        expected.push(0); // Simple
        for text in [DEED_URI, DEED_CREATORS] {
            expected.extend(u32::try_from(text.len()).expect("short").to_le_bytes());
            expected.extend(text.as_bytes());
        }
        expected.extend([7; 32]);
        assert_eq!(borsh::to_vec(&new_definition(a)).expect("encodes"), expected);
        assert_eq!(borsh::to_vec(&NftMessage::InitializeHolding { definition: a }).expect("encodes")[0], 1);
        assert_eq!(borsh::to_vec(&NftMessage::Transfer { to: a }).expect("encodes"), [&[2u8][..], &[7; 32]].concat());
        assert_eq!(borsh::to_vec(&NftMessage::Burn { amount: 1 }).expect("encodes"), [&[4u8][..], &1u128.to_le_bytes()].concat());
        let master = NftState::Master(NftMaster { definition_id: a, print_balance: 1 });
        assert_eq!(decode_state(&borsh::to_vec(&master).expect("encodes")), Some(master));
        assert_eq!(decode_state(&[]), None);
    }

    #[test]
    fn the_public_metadata_never_names_a_name() {
        assert!(DEED_NAME.len() <= 64 && DEED_URI.len() <= 256 && DEED_CREATORS.len() <= 256);
    }
}
