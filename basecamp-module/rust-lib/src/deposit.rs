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
//! - LEZ 0.4 (`v0.4.0-rc1`, `programs/bridge/core/src/deposit.rs`): `DepositMetadata {
//!   recipient: DepositRecipient::Identified(PublicAccountEvidence) }`, tag 1, then the
//!   evidence: `Key(PublicKey)` (0, then the account's 32-byte x-only public key) or
//!   `Pda { program, seed }` (1, then both). The sequencer credits the account the evidence
//!   gives, and a key that isn't a point on the curve fails the whole decode. There is no
//!   private recipient: LEZ 0.4 credits a deposit only to a public account.
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
    /// The account's x-only public key: the account is `key_account(key)`.
    Key([u8; 32]),
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
    NotAKey { key: [u8; 32] },
}

impl Refusal {
    /// In plain words, with accounts written by `show` (base58).
    pub fn words(&self, show: &dyn Fn(&[u8; 32]) -> String) -> String {
        match self {
            Self::NeedsKey { account } => format!(
                "LEZ credits a bridge only to an account named by its public key, and this wallet doesn't hold Public/{}. \
                 Ask its owner for that account's public key (64 hex characters, shown with the account in their wallet) \
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
            let key = <[u8; 32]>::try_from(key).ok()?;
            valid_public_key(&key).then(|| key_account(&key))
        }
        (Lez::V04, [1, 1, rest @ ..]) if rest.len() == 64 => {
            let (program, seed) = rest.split_at(32);
            Some(pda_account(&program.try_into().ok()?, &seed.try_into().ok()?))
        }
        _ => None,
    }
}

/// A public key written as 64 hex characters (a leading `0x` is allowed).
pub fn public_key_text(text: &str) -> Option<[u8; 32]> {
    let text = text.trim();
    let digits = text.strip_prefix("0x").unwrap_or(text);
    if digits.len() != 64 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut key = [0_u8; 32];
    for (i, byte) in key.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&digits[2 * i..2 * i + 2], 16).ok()?;
    }
    Some(key)
}

/// The public account of the x-only key `key`: `SHA-256("/LEE/v0.3/AccountId/Public/" ‖ 5
/// zero bytes ‖ key)`, as LEZ derives it.
pub fn key_account(key: &[u8; 32]) -> [u8; 32] {
    Sha256::new().chain_update(b"/LEE/v0.3/AccountId/Public/\x00\x00\x00\x00\x00").chain_update(key).finalize().into()
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

/// LEZ's bridge program: `SHA-256("/LEE-BuiltinProgram/v1/AccountId" ‖ "bridge")`.
pub fn bridge_program() -> [u8; 32] {
    Sha256::new().chain_update(b"/LEE-BuiltinProgram/v1/AccountId").chain_update(b"bridge").finalize().into()
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

// ── Whether 32 bytes are an x-only public key (BIP-340 `lift_x`) ────────────────
//
// LEZ reads a key with `k256::schnorr::VerifyingKey::from_bytes`: the bytes are a field
// element x below p, and x³ + 7 is a square mod p (some y is on the curve). The module has
// no curve library, so the check is done here in a few lines of 256-bit arithmetic; the
// CLI's tests hold it against LEZ's own `PublicKey::try_new` for many keys.

/// A field element, as four 64-bit limbs, least significant first.
type Fe = [u64; 4];

/// secp256k1's field prime, 2²⁵⁶ − 2³² − 977.
const P: Fe = [0xFFFF_FFFE_FFFF_FC2F, u64::MAX, u64::MAX, u64::MAX];
/// 2²⁵⁶ mod p.
const FOLD: u128 = 0x1_0000_03D1;

fn below_p(a: &Fe) -> bool {
    for i in (0..4).rev() {
        if a[i] != P[i] {
            return a[i] < P[i];
        }
    }
    false
}

fn minus_p(a: &Fe) -> Fe {
    let mut out = [0; 4];
    let mut borrow = false;
    for i in 0..4 {
        let (step, under) = a[i].overflowing_sub(P[i]);
        let (step, under_again) = step.overflowing_sub(u64::from(borrow));
        out[i] = step;
        borrow = under || under_again;
    }
    out
}

/// A 512-bit number (eight limbs) mod p.
fn reduce(wide: &[u64; 8]) -> Fe {
    // lo + hi·2²⁵⁶ ≡ lo + hi·FOLD: at most 2²⁵⁶ · 2³⁴.
    let mut folded = [0_u64; 4];
    let mut carry = 0_u128;
    for i in 0..4 {
        let sum = u128::from(wide[i]) + u128::from(wide[i + 4]) * FOLD + carry;
        folded[i] = sum as u64;
        carry = sum >> 64;
    }
    // Once more for what spilled over (below 2³⁴), and once for a carry out of that.
    let mut out = [0_u64; 4];
    let mut add = carry * FOLD;
    for i in 0..4 {
        let sum = u128::from(folded[i]) + add;
        out[i] = sum as u64;
        add = sum >> 64;
    }
    if add != 0 {
        let mut add = FOLD;
        for limb in &mut out {
            let sum = u128::from(*limb) + add;
            *limb = sum as u64;
            add = sum >> 64;
        }
    }
    while !below_p(&out) {
        out = minus_p(&out);
    }
    out
}

fn mul(a: &Fe, b: &Fe) -> Fe {
    let mut wide = [0_u64; 8];
    for i in 0..4 {
        let mut carry = 0_u128;
        for j in 0..4 {
            let sum = u128::from(a[i]) * u128::from(b[j]) + u128::from(wide[i + j]) + carry;
            wide[i + j] = sum as u64;
            carry = sum >> 64;
        }
        wide[i + 4] = carry as u64;
    }
    reduce(&wide)
}

fn pow(base: &Fe, exponent: &Fe) -> Fe {
    let mut acc: Fe = [1, 0, 0, 0];
    for i in (0..4).rev() {
        for bit in (0..64).rev() {
            acc = mul(&acc, &acc);
            if (exponent[i] >> bit) & 1 == 1 {
                acc = mul(&acc, base);
            }
        }
    }
    acc
}

/// Whether `key` is an x-only public key LEZ accepts: x < p and x³ + 7 a square mod p.
pub fn valid_public_key(key: &[u8; 32]) -> bool {
    let mut x: Fe = [0; 4];
    for (i, limb) in x.iter_mut().enumerate() {
        let at = 32 - 8 * (i + 1);
        *limb = u64::from_be_bytes([key[at], key[at + 1], key[at + 2], key[at + 3], key[at + 4], key[at + 5], key[at + 6], key[at + 7]]);
    }
    if !below_p(&x) {
        return false;
    }
    let cube = mul(&mul(&x, &x), &x);
    let mut wide = [0_u64; 8];
    wide[..4].copy_from_slice(&cube);
    let mut carry = 7_u128;
    for limb in &mut wide {
        let sum = u128::from(*limb) + carry;
        *limb = sum as u64;
        carry = sum >> 64;
    }
    let c = reduce(&wide);
    // Euler's criterion: c^((p−1)/2) is 1 for a nonzero square.
    let mut half = minus_one(&P);
    for i in 0..4 {
        half[i] = (half[i] >> 1) | if i < 3 { half[i + 1] << 63 } else { 0 };
    }
    c == [0; 4] || pow(&c, &half) == [1, 0, 0, 0]
}

fn minus_one(a: &Fe) -> Fe {
    let mut out = *a;
    for limb in &mut out {
        let (step, under) = limb.overflowing_sub(1);
        *limb = step;
        if !under {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unhex32(text: &str) -> [u8; 32] {
        public_key_text(text).expect("64 hex")
    }

    /// LEZ 0.4's own vectors (`deposit_metadata_carries_key_or_pda_evidence_under_tag_one`):
    /// tag 1, then 0 and the key, or 1, the program and the seed.
    #[test]
    fn lez_0_4_metadata_is_tag_one_then_the_evidence() {
        // BIP-340's first test key (secret key 3), valid on the curve.
        let key = unhex32("f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9");
        let account = key_account(&key);
        assert_eq!(metadata(Lez::V04, &account, Some(&Evidence::Key(key))).unwrap(), [&[1, 0][..], &key[..]].concat());
        let pda = Evidence::Pda { program: [4; 32], seed: [6; 32] };
        assert_eq!(metadata(Lez::V04, &pda.account(), Some(&pda)).unwrap(), [&[1, 1][..], &[4; 32][..], &[6; 32][..]].concat());
        assert_eq!(credited(Lez::V04, &[&[1, 0][..], &key[..]].concat()), Some(account));
        assert_eq!(credited(Lez::V04, &[&[1, 1][..], &[4; 32][..], &[6; 32][..]].concat()), Some(pda.account()));
        // What 0.4 refuses to decode: the bare account (0.3's form), other tags, misfit lengths.
        assert_eq!(credited(Lez::V04, &account), None);
        assert_eq!(credited(Lez::V04, &[&[2, 0][..], &key[..]].concat()), None);
        assert_eq!(credited(Lez::V04, &[&[1, 0][..], &key[..31]].concat()), None);
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
    /// key off the curve: each would leave the deposit in the bridge's custody.
    #[test]
    fn lez_0_4_refuses_what_it_could_not_credit() {
        let key = unhex32("f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9");
        let account = key_account(&key);
        assert_eq!(metadata(Lez::V04, &account, None), Err(Refusal::NeedsKey { account }));
        assert_eq!(
            metadata(Lez::V04, &[7; 32], Some(&Evidence::Key(key))),
            Err(Refusal::KeyMismatch { account: [7; 32], belongs_to: account })
        );
        // Also on 0.3: a key given for another account is a mistake either way.
        assert!(matches!(metadata(Lez::V03, &[7; 32], Some(&Evidence::Key(key))), Err(Refusal::KeyMismatch { .. })));
        // BIP-340's vector 5: not on the curve.
        let off = unhex32("eefdea4cdb677750a420fee807eacf21eb9898ae79b9768766e4faa04a2d4a34");
        assert_eq!(metadata(Lez::V04, &key_account(&off), Some(&Evidence::Key(off))), Err(Refusal::NotAKey { key: off }));
        let show = |account: &[u8; 32]| hex(&account[..2]);
        assert!(Refusal::NeedsKey { account }.words(&show).contains("public key"));
        assert!(Refusal::NotAKey { key: off }.words(&show).ends_with("Nothing was sent."));
    }

    #[test]
    fn the_curve_check_knows_points_from_non_points() {
        // The generator's x, and BIP-340's test vector keys.
        for valid in [
            "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9",
            "dff1d77f2a671c5f36183726db2341be58feae1da2deced843240f7b502ba659",
            "dd308afec5777e13121fa72b9cc1b7cc0139715309b086c960e18fd969774eb8",
        ] {
            assert!(valid_public_key(&unhex32(valid)), "{valid}");
        }
        // BIP-340 vector 5 (not on the curve), p itself, and above p.
        for invalid in [
            "eefdea4cdb677750a420fee807eacf21eb9898ae79b9768766e4faa04a2d4a34",
            "fffffffffffffffffffffffffffffffffffffffffffffffffffffffefffffc2f",
            "fffffffffffffffffffffffffffffffffffffffffffffffffffffffefffffc30",
            "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        ] {
            assert!(!valid_public_key(&unhex32(invalid)), "{invalid}");
        }
    }

    #[test]
    fn a_public_key_is_64_hex() {
        assert_eq!(public_key_text(&"ab".repeat(32)), Some([0xab; 32]));
        assert_eq!(public_key_text(&format!("0x{}", "AB".repeat(32))), Some([0xab; 32]));
        assert_eq!(public_key_text(&"ab".repeat(31)), None);
        assert_eq!(public_key_text("CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk"), None);
        assert_eq!(public_key_text(&"zz".repeat(32)), None);
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

    /// The bridge program as LEZ names a builtin program; receipts differ by deposit. (The
    /// CLI's tests hold both against LEZ 0.4's own `bridge_core`; registry.rs holds a
    /// receipt against a live rc1 sequencer's genesis deposits.)
    #[test]
    fn the_bridge_program_and_a_receipt_are_derived_as_lez_does() {
        let program: [u8; 32] = Sha256::new().chain_update(b"/LEE-BuiltinProgram/v1/AccountId").chain_update(b"bridge").finalize().into();
        assert_eq!(bridge_program(), program);
        assert_ne!(receipt(&[1; 32]), receipt(&[2; 32]));
    }
}
