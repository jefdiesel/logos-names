//! Lockstep NFT coupling between `names_core` (LNS Registry, v8) and `lez_nft` v3.
//!
//! Only top-level LNS names receive tied 1-of-1 NFTs (no subname NFTs).
//! The LNS Registry is the single source of truth; `settings_address(REGISTRY)`
//! is the sole authority actor that emits [`NftAuthoritySyncMessage`]s to
//! `settings.nft_program`, and `lez_nft` v3 notifies `record_address(REGISTRY, &slot_id)`
//! via [`crate::Message::NftTransferHook`] on plain NFT-side transfers.

use borsh::{BorshDeserialize, BorshSerialize};
use lee_core::{
    account::{AccountId, Actor},
    program::{PdaSeed, SyncMessage},
};

use crate::{NameId, OwnerTag, sha256};

const NFT_TOKEN_ID_DOMAIN: &[u8] = b"logos-names/v8/nft-token/";
const NFT_TOKEN_SEED_DOMAIN: &[u8] = b"lez-nft/v3/token/";
const NFT_DEED_SEED_DOMAIN: &[u8] = b"lez-nft/v3/deed/";

/// Wire enum variant index of `lez_nft::Message::AuthoritySync` (`0x02`).
pub const NFT_MESSAGE_TAG_AUTHORITY_SYNC: u8 = 2;

/// Default name-free JSON metadata stored on minted LNS deed NFTs.
/// Clients verify the name locally and render a name card when known; explorers
/// and wallets that do not know the name show this generic deed metadata.
pub const DEFAULT_DEED_CONTENT: &[u8] =
    br#"{"name":"LNS Name Deed","image":"https://nameswallet.app/lns/deed.png"}"#;

/// Metadata telling `lez_nft` v3 which registry and actors control a tied name NFT.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct LinkedAuthority {
    /// The LNS registry program ID.
    pub program: AccountId,
    /// The single registry actor authorized to send `AuthoritySync` operations
    /// (`settings_address(&program)`).
    pub authority_actor: AccountId,
    /// The top-level name's `slot_id`. Both programs compute the identical
    /// `owner_tag(&target, account)` so split ownership is impossible, and
    /// `lez_nft` echoes `target` back as `id` in `Message::NftTransferHook`.
    pub target: NameId,
    /// The name's record PDA (`record_address(&program, &target)`), which
    /// receives `Message::NftTransferHook` on NFT-initiated transfers.
    pub record_actor: AccountId,
}

/// Cross-program operations emitted by `names_core` (`settings_address`) to
/// `lez_nft` v3 (`nft_token_address(nft_program, &token_id)`), wrapped in
/// [`NftAuthoritySyncMessage`] (`lez_nft::Message::AuthoritySync`, tag `0x02`).
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum NftAuthorityOp {
    /// `0x00`: Mint a new tied 1-of-1 name NFT (`Register` or `Claim`).
    Mint {
        token_id: [u8; 32],
        owner: OwnerTag,
        linked: LinkedAuthority,
        content: Vec<u8>,
        expires: u64,
    },
    /// `0x01`: Mirror a registry-initiated ownership change (`Transfer` or `Buy`).
    Transfer {
        token_id: [u8; 32],
        new_owner: OwnerTag,
    },
    /// `0x02`: Burn a tied name NFT (`Release`, `Claim` overwrite, or lapsed re-registration).
    Burn {
        token_id: [u8; 32],
    },
    /// `0x03`: Mirror an updated expiry timestamp (`Renew`).
    SetExpiry {
        token_id: [u8; 32],
        expires: u64,
    },
    /// `0x04`: Shield custody into the holder's private LP-0001 deed PDA (`deed_account`).
    Shield {
        token_id: [u8; 32],
        deed_account: AccountId,
        expires: u64,
    },
    /// `0x05`: Unshield custody from the stored private deed PDA back to the public token record.
    Unshield {
        token_id: [u8; 32],
    },
    /// `0x06`: Refresh `PrivateDeed.expires` after renewing a shielded name.
    RefreshShielded {
        token_id: [u8; 32],
        expires: u64,
    },
}

impl NftAuthorityOp {
    /// The 32-byte token ID targeted by this operation.
    #[must_use]
    pub const fn token_id(&self) -> &[u8; 32] {
        match self {
            Self::Mint { token_id, .. }
            | Self::Transfer { token_id, .. }
            | Self::Burn { token_id }
            | Self::SetExpiry { token_id, .. }
            | Self::Shield { token_id, .. }
            | Self::Unshield { token_id }
            | Self::RefreshShielded { token_id, .. } => token_id,
        }
    }
}

/// Wire envelope matching `lez_nft::Message::AuthoritySync(NftAuthorityOp)`
/// (`0x02` followed by the borsh-encoded [`NftAuthorityOp`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NftAuthoritySyncMessage(pub NftAuthorityOp);

impl BorshSerialize for NftAuthoritySyncMessage {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        NFT_MESSAGE_TAG_AUTHORITY_SYNC.serialize(writer)?;
        self.0.serialize(writer)
    }
}

impl BorshDeserialize for NftAuthoritySyncMessage {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let tag = u8::deserialize_reader(reader)?;
        if tag != NFT_MESSAGE_TAG_AUTHORITY_SYNC {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "expected lez_nft AuthoritySync message tag (2)",
            ));
        }
        Ok(Self(NftAuthorityOp::deserialize_reader(reader)?))
    }
}

/// Derives the unique 1-of-1 NFT token ID for a top-level name registration or
/// claim. Stays fixed across `Transfer`, `Buy`, and `Renew`, and rotates on
/// re-registration after lapse or legacy `Claim`.
#[must_use]
pub fn nft_token_id(slot_id: &NameId, mint_generation: &[u8; 32]) -> [u8; 32] {
    let mut bytes = Vec::with_capacity(NFT_TOKEN_ID_DOMAIN.len() + 64);
    bytes.extend_from_slice(NFT_TOKEN_ID_DOMAIN);
    bytes.extend_from_slice(slot_id);
    bytes.extend_from_slice(mint_generation);
    sha256(&bytes)
}

/// The PDA seed of a token record inside `lez_nft` v3.
#[must_use]
pub fn nft_token_seed(token_id: &[u8; 32]) -> PdaSeed {
    let mut bytes = Vec::with_capacity(NFT_TOKEN_SEED_DOMAIN.len() + 32);
    bytes.extend_from_slice(NFT_TOKEN_SEED_DOMAIN);
    bytes.extend_from_slice(token_id);
    PdaSeed::new(sha256(&bytes))
}

/// The public PDA holding the `NftRecord` for `token_id` under `nft_program`.
#[must_use]
pub fn nft_token_address(nft_program: &AccountId, token_id: &[u8; 32]) -> AccountId {
    AccountId::for_public_pda(nft_program, &nft_token_seed(token_id))
}

/// The private PDA seed used by `lez_nft` v3 for an opt-in LP-0001 `PrivateDeed`.
#[must_use]
pub fn nft_deed_seed(token_id: &[u8; 32]) -> PdaSeed {
    let mut bytes = Vec::with_capacity(NFT_DEED_SEED_DOMAIN.len() + 32);
    bytes.extend_from_slice(NFT_DEED_SEED_DOMAIN);
    bytes.extend_from_slice(token_id);
    PdaSeed::new(sha256(&bytes))
}

/// Builds the synchronous cross-program message from `settings_address` to
/// `(nft_token_address(nft_program, token_id), nft_program)`.
pub(crate) fn authority_sync_message(nft_program: &AccountId, op: NftAuthorityOp) -> SyncMessage {
    let token_pda = nft_token_address(nft_program, op.token_id());
    SyncMessage::new(Actor::new(token_pda, *nft_program), &NftAuthoritySyncMessage(op))
}
