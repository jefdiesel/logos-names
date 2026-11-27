//! What a Logos Blockchain deposit into LEZ carries, and how LEZ names it once minted.
//!
//! This one file is the deposit encoding for both the Names module (`mod deposit`) and
//! `names bridge` (the CLI includes it by path), so the two can never send different
//! bytes. It needs only `sha2` and `blake2`, which both crates have.
//!
//! A deposit's metadata says which LEZ account it credits, and the sequencer that mints it
//! decodes it with Borsh. The two LEZ versions in use read it differently, and a deposit
//! whose metadata the sequencer can't read is never minted (it stays in the bridge's
//! custody), so the version is asked first (`lez_of`) and the bytes made for it:
//!
//! - LEZ 0.3 (the public testnet until it moves): `DepositMetadata { recipient_id:
//!   AccountId }`, the account's 32 bytes.
//! - LEZ 0.4 (`v0.4.0`, `programs/bridge/core/src/deposit.rs`): `DepositMetadata {
//!   recipient: DepositRecipient::Identified(PublicAccountEvidence) }`, tag 1, then the
//!   evidence: `Key(SigningKeyState)` (0, then the 33-byte Borsh encoding of
//!   `SigningKeyState`: 1-byte `KeyType` tag `0x00` for Schnorr plus 32-byte key commitment)
//!   or `Pda { program, seed }` (1, then both). The sequencer credits the account the evidence
//!   gives, and an unknown `KeyType` tag fails the decode. There is no private recipient:
//!   LEZ 0.4 credits a deposit only to a public account.
//!
//! So on 0.4 an account ID alone is not enough: the sender needs the account's public key,
//! which the wallet has for its own accounts and only the owner can give for another's.

use sha2::{Digest as _, Sha256};

/// Which LEZ decodes the deposit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lez {
    V03,
    V04,
}

/// What shows LEZ 0.4 that a public account is the one credited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Evidence {
    /// The account's initial signing key (`SigningKeyState`, 1-byte key type + 32-byte
    /// commitment): the account is `key_account(key)`.
    Key([u8; 33]),
    /// A program's account: `pda_account(program, seed)`.
    Pda { program: [u8; 32], seed: [u8; 32] },
}

impl Evidence {
    /// The account this evidence names.
    pub fn account(&self) -> [u8; 32] {
        match self {
            Self::Key(key) => key_account(key),
            Self::Pda { program, seed } => pda_account(program, seed),
        }
    }
}

/// Why a deposit's metadata can't be made: nothing was sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// LEZ 0.4 needs the account's public key, and none was given.
    NeedsKey { account: [u8; 32] },
    /// The key given belongs to another account.
    KeyMismatch { account: [u8; 32], belongs_to: [u8; 32] },
    /// The key isn't one LEZ accepts: it would fail the sequencer's decode.
    NotAKey { key: [u8; 33] },
}

impl Refusal {
    /// In plain words, with accounts written by `show` (base58).
    pub fn words(&self, show: &dyn Fn(&[u8; 32]) -> String) -> String {
        match self {
            Self::NeedsKey { account } => format!(
                "LEZ credits a bridge only to an account named by its public key, and this wallet doesn't hold Public/{}. \
                 Ask its owner for that account's public key (66 hex characters, shown with the account in their wallet) \
                 and bridge to the key instead. Nothing was sent.",
                show(account)
            ),
            Self::KeyMismatch { account, belongs_to } => format!(
                "That public key belongs to Public/{}, not Public/{}. Nothing was sent.",
                show(belongs_to),
                show(account)
            ),
            Self::NotAKey { key } => format!(
                "{} isn't a public key LEZ accepts, so LEZ would keep the deposit and credit no one. Check the key. Nothing was sent.",
                hex(key)
            ),
        }
    }
}

/// The deposit's metadata for `lez`, crediting `account`. LEZ 0.3 reads the account's 32
/// bytes; LEZ 0.4 needs `evidence` that names `account` (a key must be a valid one).
pub fn metadata(lez: Lez, account: &[u8; 32], evidence: Option<&Evidence>) -> Result<Vec<u8>, Refusal> {
    if let Some(evidence) = evidence {
        let belongs_to = evidence.account();
        if belongs_to != *account {
            return Err(Refusal::KeyMismatch { account: *account, belongs_to });
        }
    }
    match lez {
        Lez::V03 => Ok(account.to_vec()),
        Lez::V04 => match evidence {
            None => Err(Refusal::NeedsKey { account: *account }),
            Some(Evidence::Key(key)) if !valid_public_key(key) => Err(Refusal::NotAKey { key: *key }),
            Some(Evidence::Key(key)) => Ok([&[1_u8, 0][..], key].concat()),
            Some(Evidence::Pda { program, seed }) => Ok([&[1_u8, 1][..], program, seed].concat()),
        },
    }
}

/// The account a deposit's metadata credits, as `lez` decodes it; `None` for bytes it
/// can't decode (such a deposit is never minted).
pub fn credited(lez: Lez, metadata: &[u8]) -> Option<[u8; 32]> {
    match (lez, metadata) {
        (Lez::V03, bytes) => <[u8; 32]>::try_from(bytes).ok(),
        (Lez::V04, [1, 0, key @ ..]) => {
            let key = <[u8; 33]>::try_from(key).ok()?;
            valid_public_key(&key).then(|| key_account(&key))
        }
        (Lez::V04, [1, 1, rest @ ..]) if rest.len() == 64 => {
            let (program, seed) = rest.split_at(32);
            Some(pda_account(&program.try_into().ok()?, &seed.try_into().ok()?))
        }
        _ => None,
    }
}

/// A public key (`SigningKeyState`) written as 66 hex characters (a leading `0x` is allowed).
pub fn public_key_text(text: &str) -> Option<[u8; 33]> {
    let text = text.trim();
    let digits = text.strip_prefix("0x").unwrap_or(text);
    if digits.len() != 66 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut key = [0_u8; 33];
    for (i, byte) in key.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&digits[2 * i..2 * i + 2], 16).ok()?;
    }
    Some(key)
}

/// The public account of the initial signing key `key` (`SigningKeyState`: 1-byte `KeyType` +
/// 32-byte commitment): two-step SHA-256 as LEZ v0.4.0's `AccountId::for_initial_key` derives it.
pub fn key_account(key: &[u8; 33]) -> [u8; 32] {
    let key_hash: [u8; 32] = Sha256::new()
        .chain_update(b"/LEE/v0.3/AccountId/Public/Schn/")
        .chain_update(&key[1..])
        .finalize()
        .into();
    Sha256::new()
        .chain_update(b"/LEE/v0.3/AccountId/Public/\x00\x00\x00\x00\x00")
        .chain_update(key_hash)
        .finalize()
        .into()
}

/// The account `program` derives from `seed`: `SHA-256("/LEE/v0.2/AccountId/PDA/" ‖ 8 zero
/// bytes ‖ program ‖ seed)`.
pub fn pda_account(program: &[u8; 32], seed: &[u8; 32]) -> [u8; 32] {
    Sha256::new()
        .chain_update(b"/LEE/v0.2/AccountId/PDA/\x00\x00\x00\x00\x00\x00\x00\x00")
        .chain_update(program)
        .chain_update(seed)
        .finalize()
        .into()
}

/// A LEZ v0.4.0 system program account (`AccountId::from_system_program_name`), derived as a
/// PDA under `SYSTEM_UPGRADER_ACCOUNT_ID` (`[0xFD; 32]`).
pub fn system_program(name: &[u8]) -> [u8; 32] {
    let mut padded = [0_u8; 32];
    padded[..name.len()].copy_from_slice(name);
    let seed: [u8; 32] = Sha256::new()
        .chain_update(b"/LEE/SystemUpgrader/v1/Name\x00\x00\x00\x00\x00")
        .chain_update(padded)
        .finalize()
        .into();
    pda_account(&[0xFD; 32], &seed)
}

/// LEZ's bridge program: `system_program(b"bridge")`.
pub fn bridge_program() -> [u8; 32] {
    system_program(b"bridge")
}

/// The account in which LEZ's bridge marks the deposit `op_id` as minted (its receipt):
/// written in the same LEZ transaction that credits it, exactly once per deposit, on 0.3
/// and 0.4 alike.
pub fn receipt(op_id: &[u8; 32]) -> [u8; 32] {
    let seed: [u8; 32] = Sha256::new().chain_update(b"/LEZ/v0.3/BridgeDepositReceipt/0").chain_update(op_id).finalize().into();
    pda_account(&bridge_program(), &seed)
}

/// The identity of a channel deposit on the Logos Blockchain (its `op_id`), which LEZ
/// mints it under: `Blake2b-256("OPERATION_ID_V1" ‖ op)`, the op encoded canonically as
/// the channel's 32 bytes, the input notes (a one-byte count, then each note's 32 bytes),
/// and the metadata (a four-byte little-endian length, then its bytes). Everything in it is
/// known before the deposit is sent; it changes with the metadata, so it is worked out from
/// the very bytes sent. `None` for more notes or metadata than the encoding holds.
pub fn op_id(channel: &[u8; 32], inputs: &[[u8; 32]], metadata: &[u8]) -> Option<[u8; 32]> {
    use blake2::Digest as _;
    let mut hasher = blake2::Blake2b::<blake2::digest::consts::U32>::new();
    hasher.update(b"OPERATION_ID_V1");
    hasher.update(channel);
    hasher.update([u8::try_from(inputs.len()).ok()?]);
    for note in inputs {
        hasher.update(note);
    }
    hasher.update(u32::try_from(metadata.len()).ok()?.to_le_bytes());
    hasher.update(metadata);
    Some(hasher.finalize().into())
}

/// The JSON-RPC call that tells the two LEZ versions apart: a method LEZ 0.4 added (a
/// private account's recovery binding, null for an address nothing was sent to). LEZ 0.3
/// has no such method.
pub const PROBE_METHOD: &str = "getRecoveryBinding";
/// Its one parameter: the all-zero account, in base58.
pub const PROBE_ACCOUNT: &str = "11111111111111111111111111111111";

/// Which LEZ answered `PROBE_METHOD`, from its whole JSON-RPC reply: any result (null
/// included) is 0.4, "method not found" (-32601) is 0.3. Anything else (another error, a
/// reply that isn't JSON-RPC) tells neither, and nothing should be deposited on a guess.
pub fn lez_of(reply: &serde_json::Value) -> Option<Lez> {
    match reply.get("error").filter(|error| !error.is_null()) {
        Some(error) => (error["code"].as_i64() == Some(-32601)).then_some(Lez::V03),
        None => reply.get("result").map(|_| Lez::V04),
    }
}

/// What to say when `lez_of` told neither.
pub const UNKNOWN_LEZ: &str =
    "Couldn't tell which version of LEZ the sequencer runs, and a deposit it can't read is never credited. Try again in a minute. Nothing was sent.";

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

// ── Whether 33 bytes are a SigningKeyState LEZ v0.4.0 accepts ───────────────────
//
// In LEZ v0.4.0 (`lee-core/src/signing_key.rs`), `SigningKeyState` is Borsh-encoded as a
// 1-byte `KeyType` tag (`0x00` = `KeyType::Schnorr`) followed by a 32-byte key commitment
// (arbitrary 32 bytes; no curve check is performed on the commitment). Any tag other than
// `0x00` fails Borsh deserialization and is refused by the sequencer.

/// Whether `key` is a `SigningKeyState` LEZ v0.4.0 accepts (`KeyType::Schnorr` = `0x00`).
pub fn valid_public_key(key: &[u8; 33]) -> bool {
    key[0] == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unhex33(text: &str) -> [u8; 33] {
        public_key_text(text).expect("66 hex")
    }

    /// LEZ 0.4's own vectors: tag 1, then 0 and the 33-byte SigningKeyState, or 1, the
    /// program and the seed.
    #[test]
    fn lez_0_4_metadata_is_tag_one_then_the_evidence() {
        let key = unhex33("00f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9");
        let account = key_account(&key);
        assert_eq!(metadata(Lez::V04, &account, Some(&Evidence::Key(key))).unwrap(), [&[1, 0][..], &key[..]].concat());
        let pda = Evidence::Pda { program: [4; 32], seed: [6; 32] };
        assert_eq!(metadata(Lez::V04, &pda.account(), Some(&pda)).unwrap(), [&[1, 1][..], &[4; 32][..], &[6; 32][..]].concat());
        assert_eq!(credited(Lez::V04, &[&[1, 0][..], &key[..]].concat()), Some(account));
        assert_eq!(credited(Lez::V04, &[&[1, 1][..], &[4; 32][..], &[6; 32][..]].concat()), Some(pda.account()));
        // What 0.4 refuses to decode: the bare account (0.3's form), other tags, misfit lengths.
        assert_eq!(credited(Lez::V04, &account), None);
        assert_eq!(credited(Lez::V04, &[&[2, 0][..], &key[..]].concat()), None);
        assert_eq!(credited(Lez::V04, &[&[1, 0][..], &key[..32]].concat()), None);
        assert_eq!(credited(Lez::V04, &[&[1, 0][..], &key[..], &[0][..]].concat()), None);
    }

    /// LEZ 0.3 reads the account's 32 bytes, as every bridge before 0.4 sent them (and as the
    /// public testnet minted 10,000,000 on 2026-11-06).
    #[test]
    fn lez_0_3_metadata_is_the_accounts_32_bytes() {
        let account = [9_u8; 32];
        assert_eq!(metadata(Lez::V03, &account, None).unwrap(), account.to_vec());
        assert_eq!(credited(Lez::V03, &account), Some(account));
        assert_eq!(credited(Lez::V03, &[1, 0]), None);
    }

    /// On 0.4 an account without its key is refused, as is a key of another account or a
    /// key with an unknown KeyType tag: each would leave the deposit in the bridge's custody.
    #[test]
    fn lez_0_4_refuses_what_it_could_not_credit() {
        let key = unhex33("00f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9");
        let account = key_account(&key);
        assert_eq!(metadata(Lez::V04, &account, None), Err(Refusal::NeedsKey { account }));
        assert_eq!(
            metadata(Lez::V04, &[7; 32], Some(&Evidence::Key(key))),
            Err(Refusal::KeyMismatch { account: [7; 32], belongs_to: account })
        );
        // Also on 0.3: a key given for another account is a mistake either way.
        assert!(matches!(metadata(Lez::V03, &[7; 32], Some(&Evidence::Key(key))), Err(Refusal::KeyMismatch { .. })));
        // Unknown KeyType tag (1 != KeyType::Schnorr (0)).
        let bad_tag = unhex33("01eefdea4cdb677750a420fee807eacf21eb9898ae79b9768766e4faa04a2d4a34");
        assert_eq!(metadata(Lez::V04, &key_account(&bad_tag), Some(&Evidence::Key(bad_tag))), Err(Refusal::NotAKey { key: bad_tag }));
        let show = |account: &[u8; 32]| hex(&account[..2]);
        assert!(Refusal::NeedsKey { account }.words(&show).contains("public key"));
        assert!(Refusal::NotAKey { key: bad_tag }.words(&show).ends_with("Nothing was sent."));
    }

    #[test]
    fn the_key_type_check_knows_valid_from_invalid_tags() {
        assert!(valid_public_key(&unhex33("0079be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798")));
        assert!(valid_public_key(&[0; 33]));
        for tag in [1_u8, 2, 0x7f, 0xff] {
            let mut key = [0xab_u8; 33];
            key[0] = tag;
            assert!(!valid_public_key(&key), "tag {tag:#x}");
        }
    }

    #[test]
    fn a_public_key_is_66_hex() {
        assert_eq!(public_key_text(&"ab".repeat(33)), Some([0xab; 33]));
        assert_eq!(public_key_text(&format!("0x{}", "AB".repeat(33))), Some([0xab; 33]));
        assert_eq!(public_key_text(&"ab".repeat(32)), None);
        assert_eq!(public_key_text("CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk"), None);
        assert_eq!(public_key_text(&"zz".repeat(33)), None);
    }

    /// The probe: a result (null too) is 0.4, "method not found" is 0.3, the rest neither.
    #[test]
    fn the_probe_tells_the_versions_apart_or_says_it_cannot() {
        use serde_json::json;
        assert_eq!(lez_of(&json!({ "jsonrpc": "2.0", "id": 1, "result": null })), Some(Lez::V04));
        assert_eq!(lez_of(&json!({ "jsonrpc": "2.0", "id": 1, "error": { "code": -32601, "message": "Method not found" } })), Some(Lez::V03));
        assert_eq!(lez_of(&json!({ "jsonrpc": "2.0", "id": 1, "error": { "code": -32602, "message": "Invalid params" } })), None);
        assert_eq!(lez_of(&json!({ "error": { "code": -32000, "message": "busy" } })), None);
        assert_eq!(lez_of(&json!({})), None);
        assert_eq!(lez_of(&json!("<html>")), None);
    }

    /// The bridge program as LEZ v0.4.0 names a system program (PDA under
    /// `SYSTEM_UPGRADER_ACCOUNT_ID`); receipts differ by deposit. (The CLI's tests hold both
    /// against LEZ 0.4's own `bridge_core`.)
    #[test]
    fn the_bridge_program_and_a_receipt_are_derived_as_lez_does() {
        let mut padded = [0_u8; 32];
        padded[..6].copy_from_slice(b"bridge");
        let seed: [u8; 32] = Sha256::new()
            .chain_update(b"/LEE/SystemUpgrader/v1/Name\x00\x00\x00\x00\x00")
            .chain_update(padded)
            .finalize()
            .into();
        let program = pda_account(&[0xFD; 32], &seed);
        assert_eq!(bridge_program(), program);
        assert_ne!(receipt(&[1; 32]), receipt(&[2; 32]));
    }
}
