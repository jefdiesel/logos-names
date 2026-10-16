//! The short-name list (SPEC.md section 8.1): a Merkle commitment to the ids of
//! every name of one to three characters, which lets the registry lock or
//! surcharge them without ever seeing a name. A registration carries a proof of
//! where its identifier sits relative to the list.
//!
//! A copy of `core/src/shortlist.rs`, hashing with the `sha2` crate instead of
//! risc0's, and encoding proofs by hand instead of with borsh. The list itself
//! (`shortlist.bin`, 33 bytes per leaf: `id ‖ tier`, sentinels included) is a
//! copy of the repository's `shortlist/shortlist.bin`, built in.
//!
//! - leaf:  `H("logos-names/v5/short-leaf" || id || tier)`
//! - node:  `H("logos-names/v5/short-node" || left || right)`
//! - empty: `H("logos-names/v5/short-empty")`
//!
//! With `n` leaves the tree has `depth = ceil(log2(n))` levels (16 for the real
//! list of 49,286 leaves, padded with the empty hash to 65,536). A path lists the
//! sibling at each level from the leaf up; at level `k` the sibling is on the
//! right when bit `k` of the index is 0.

use crate::registry::Writer;
use sha2::{Digest, Sha256};

/// Tier of a 1-2 character name: locked, only the admin can register it.
pub const TIER_LOCKED: u8 = 1;
/// Tier of a 3 character name: the higher deposit.
pub const TIER_THREE: u8 = 3;
/// Result of a valid non-membership proof: a normal name.
pub const TIER_NORMAL: u8 = 0;
/// Tier of the two sentinel leaves. Never a real name.
pub const TIER_SENTINEL: u8 = 0xff;
/// Depth of the shipped list (49,286 leaves padded to 65,536).
pub const SHORTLIST_DEPTH: usize = 16;

/// The root the registry program has built in (`core`'s `SHORTLIST_ROOT`).
pub const SHORTLIST_ROOT: [u8; 32] = [
    0x66, 0xc7, 0x5f, 0x76, 0xe4, 0x2f, 0x1e, 0x87, 0xc8, 0x0b, 0x87, 0x83, 0xc8, 0xa3, 0x15, 0xdd,
    0x84, 0x47, 0x10, 0x41, 0xb0, 0x29, 0x06, 0x6b, 0xd5, 0xf8, 0x91, 0x1d, 0x6c, 0x5a, 0x5e, 0x3c,
];

const LEAF_DOMAIN: &[u8] = b"logos-names/v5/short-leaf";
const NODE_DOMAIN: &[u8] = b"logos-names/v5/short-node";
const EMPTY_DOMAIN: &[u8] = b"logos-names/v5/short-empty";

pub const SENTINEL_LOW: [u8; 32] = [0x00; 32];
pub const SENTINEL_HIGH: [u8; 32] = [0xff; 32];

/// The list this module was built with.
static SHORTLIST_BIN: &[u8] = include_bytes!("../shortlist.bin");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Leaf {
    pub id: [u8; 32],
    pub tier: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Membership {
    pub index: u32,
    pub tier: u8,
    pub path: Vec<[u8; 32]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NonMembership {
    pub left: Leaf,
    pub left_path: Vec<[u8; 32]>,
    pub right: Leaf,
    pub right_path: Vec<[u8; 32]>,
    /// Index of `left`; `right` is at `left_index + 1`.
    pub left_index: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LengthProof {
    Membership(Membership),
    NonMembership(NonMembership),
}

impl LengthProof {
    /// Appends the proof's borsh encoding, as `core`'s `LengthProof` derives it:
    /// the variant index as one byte, then the fields in order, each path a
    /// `u32` count followed by its 32-byte hashes.
    pub fn encode(&self, writer: &mut Writer) {
        fn path(writer: &mut Writer, path: &[[u8; 32]]) {
            writer.u32(path.len() as u32);
            for hash in path {
                writer.0.extend(hash);
            }
        }
        fn leaf(writer: &mut Writer, leaf: &Leaf) {
            writer.0.extend(leaf.id);
            writer.u8(leaf.tier);
        }
        match self {
            Self::Membership(proof) => {
                writer.u8(0);
                writer.u32(proof.index);
                writer.u8(proof.tier);
                path(writer, &proof.path);
            }
            Self::NonMembership(proof) => {
                writer.u8(1);
                leaf(writer, &proof.left);
                path(writer, &proof.left_path);
                leaf(writer, &proof.right);
                path(writer, &proof.right_path);
                writer.u32(proof.left_index);
            }
        }
    }
}

fn sha256(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize().into()
}

pub fn leaf_hash(id: &[u8; 32], tier: u8) -> [u8; 32] {
    sha256(&[LEAF_DOMAIN, id, &[tier]])
}

pub fn node_hash(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    sha256(&[NODE_DOMAIN, left, right])
}

pub fn empty_hash() -> [u8; 32] {
    sha256(&[EMPTY_DOMAIN])
}

fn root_from_path(leaf: [u8; 32], index: u32, path: &[[u8; 32]]) -> [u8; 32] {
    let mut acc = leaf;
    for (level, sibling) in path.iter().enumerate() {
        acc = if (index >> level) & 1 == 0 { node_hash(&acc, sibling) } else { node_hash(sibling, &acc) };
    }
    acc
}

fn check_leaf_path(root: &[u8; 32], leaf: &Leaf, index: u32, path: &[[u8; 32]], depth: usize) -> Result<(), &'static str> {
    if path.len() != depth {
        return Err("wrong proof path length");
    }
    if depth < 32 && u64::from(index) >= (1u64 << depth) {
        return Err("leaf index out of range");
    }
    if &root_from_path(leaf_hash(&leaf.id, leaf.tier), index, path) != root {
        return Err("proof does not match the list root");
    }
    Ok(())
}

/// Checks `proof` about `id` against `root` for a list of the shipped depth and
/// returns the tier, exactly as the program does.
pub fn verify(root: &[u8; 32], id: &[u8; 32], proof: &LengthProof) -> Result<u8, &'static str> {
    verify_depth(root, id, proof, SHORTLIST_DEPTH)
}

pub fn verify_depth(root: &[u8; 32], id: &[u8; 32], proof: &LengthProof, depth: usize) -> Result<u8, &'static str> {
    if *id == SENTINEL_LOW || *id == SENTINEL_HIGH {
        return Err("sentinel ids are not names");
    }
    match proof {
        LengthProof::Membership(m) => {
            if m.tier != TIER_LOCKED && m.tier != TIER_THREE {
                return Err("membership tier must be locked or three");
            }
            check_leaf_path(root, &Leaf { id: *id, tier: m.tier }, m.index, &m.path, depth)?;
            Ok(m.tier)
        }
        LengthProof::NonMembership(n) => {
            if !(n.left.id < *id && *id < n.right.id) {
                return Err("id is not strictly between the proof leaves");
            }
            let right_index = n.left_index.checked_add(1).ok_or("leaf index out of range")?;
            check_leaf_path(root, &n.left, n.left_index, &n.left_path, depth)?;
            check_leaf_path(root, &n.right, right_index, &n.right_path, depth)?;
            Ok(TIER_NORMAL)
        }
    }
}

/// A built list: all levels of the tree, for computing the root and proofs.
#[derive(Debug, Clone)]
pub struct ShortList {
    leaves: Vec<Leaf>,
    /// `levels[0]` are the padded leaf hashes, the last level is the root.
    levels: Vec<Vec<[u8; 32]>>,
}

impl ShortList {
    /// Builds from real `(id, tier)` pairs, adding the sentinels (tests).
    #[allow(dead_code)]
    pub fn new(mut entries: Vec<([u8; 32], u8)>) -> Result<Self, &'static str> {
        if entries.iter().any(|(_, tier)| *tier != TIER_LOCKED && *tier != TIER_THREE) {
            return Err("entry tier must be locked or three");
        }
        entries.sort();
        if entries.windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return Err("duplicate id in the list");
        }
        if entries.iter().any(|(id, _)| *id == SENTINEL_LOW || *id == SENTINEL_HIGH) {
            return Err("a real id equals a sentinel");
        }
        let mut leaves = Vec::with_capacity(entries.len() + 2);
        leaves.push(Leaf { id: SENTINEL_LOW, tier: TIER_SENTINEL });
        leaves.extend(entries.into_iter().map(|(id, tier)| Leaf { id, tier }));
        leaves.push(Leaf { id: SENTINEL_HIGH, tier: TIER_SENTINEL });
        Self::from_sorted_leaves(leaves)
    }

    /// Rebuilds from the full sorted leaves, sentinels included (the file format).
    pub fn from_sorted_leaves(leaves: Vec<Leaf>) -> Result<Self, &'static str> {
        if leaves.len() < 2
            || leaves[0] != (Leaf { id: SENTINEL_LOW, tier: TIER_SENTINEL })
            || leaves[leaves.len() - 1] != (Leaf { id: SENTINEL_HIGH, tier: TIER_SENTINEL })
        {
            return Err("list must start and end with the sentinels");
        }
        if leaves.windows(2).any(|pair| pair[0].id >= pair[1].id) {
            return Err("leaves are not strictly ascending");
        }
        if leaves[1..leaves.len() - 1].iter().any(|leaf| leaf.tier != TIER_LOCKED && leaf.tier != TIER_THREE) {
            return Err("entry tier must be locked or three");
        }
        let width = leaves.len().next_power_of_two();
        let mut level: Vec<[u8; 32]> = leaves.iter().map(|leaf| leaf_hash(&leaf.id, leaf.tier)).collect();
        level.resize(width, empty_hash());
        let mut levels = vec![level];
        while levels.last().expect("at least one level").len() > 1 {
            let next = levels
                .last()
                .expect("at least one level")
                .chunks(2)
                .map(|pair| node_hash(&pair[0], &pair[1]))
                .collect();
            levels.push(next);
        }
        Ok(Self { leaves, levels })
    }

    /// The list built into this module, read once.
    pub fn built_in() -> Result<&'static Self, String> {
        static LIST: std::sync::OnceLock<Result<ShortList, String>> = std::sync::OnceLock::new();
        LIST.get_or_init(|| {
            let leaves = SHORTLIST_BIN
                .chunks_exact(33)
                .map(|chunk| Leaf { id: chunk[..32].try_into().expect("32 bytes"), tier: chunk[32] })
                .collect();
            Self::from_sorted_leaves(leaves).map_err(str::to_owned)
        })
        .as_ref()
        .map_err(|reason| format!("the built-in short-name list is invalid: {reason}"))
    }

    #[allow(dead_code)]
    pub fn leaves(&self) -> &[Leaf] {
        &self.leaves
    }

    pub fn depth(&self) -> usize {
        self.levels.len() - 1
    }

    pub fn root(&self) -> [u8; 32] {
        self.levels[self.depth()][0]
    }

    fn path(&self, index: usize) -> Vec<[u8; 32]> {
        (0..self.depth()).map(|level| self.levels[level][(index >> level) ^ 1]).collect()
    }

    /// Proof for `id`: membership if on the list, otherwise non-membership.
    pub fn prove(&self, id: &[u8; 32]) -> Result<LengthProof, &'static str> {
        if *id == SENTINEL_LOW || *id == SENTINEL_HIGH {
            return Err("sentinel ids are not names");
        }
        match self.leaves.binary_search_by(|leaf| leaf.id.cmp(id)) {
            Ok(index) => Ok(LengthProof::Membership(Membership {
                index: index as u32,
                tier: self.leaves[index].tier,
                path: self.path(index),
            })),
            Err(position) => {
                // `position` is in 1..len because the sentinels bound every other id.
                let left = position - 1;
                Ok(LengthProof::NonMembership(NonMembership {
                    left: self.leaves[left],
                    left_path: self.path(left),
                    right: self.leaves[left + 1],
                    right_path: self.path(left + 1),
                    left_index: left as u32,
                }))
            }
        }
    }
}

/// The proof of where `id` sits relative to the short-name list, and its tier,
/// as the CLI's `length_proof` makes it: from the built-in list, refused if that
/// list is not the one the program's root commits to, and checked before use.
pub fn length_proof(id: &[u8; 32]) -> Result<(LengthProof, u8), String> {
    let list = ShortList::built_in()?;
    if list.root() != SHORTLIST_ROOT {
        return Err("this module's short-name list doesn't match the registry program's; use a matching release".to_owned());
    }
    let proof = list.prove(id).map_err(|reason| format!("no length proof: {reason}"))?;
    let tier = verify(&SHORTLIST_ROOT, id, &proof).map_err(|reason| format!("the length proof does not hold: {reason}"))?;
    Ok((proof, tier))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{derive, hex};

    fn digest(bytes: &[u8]) -> String {
        hex(&sha256(&[bytes]))
    }

    fn encoded(proof: &LengthProof) -> Vec<u8> {
        let mut writer = Writer::default();
        proof.encode(&mut writer);
        writer.0
    }

    #[test]
    fn the_built_in_list_rebuilds_to_the_programs_root() {
        let list = ShortList::built_in().expect("the list loads");
        assert_eq!(list.leaves().len(), 49_286);
        assert_eq!(list.depth(), SHORTLIST_DEPTH);
        assert_eq!(hex(&list.root()), "66c75f76e42f1e87c80b8783c8a315dd84471041b029066bd5f8911d6c5a5e3c");
        assert_eq!(list.root(), SHORTLIST_ROOT);
    }

    /// Names in each tier, with the sha256 of the proof's borsh bytes as
    /// `names_core` encodes it (generated from `core` itself). `jef` is three
    /// characters, so it is on the list, in the three-character tier, as the
    /// CLI's own test says.
    #[test]
    fn names_get_their_tier_and_the_proof_core_would_build() {
        for (name, tier, length, sha) in [
            ("jef", TIER_THREE, 522, "a7a0e61180ebb015296c7aea39f3a2b1cf86264192942ff0c6141595244c0523"),
            ("qkz", TIER_THREE, 522, "8120b1a6e2833c13dca673a304330770a3a5f3e9de770150d7195db2be56b527"),
            ("ab", TIER_LOCKED, 522, "ebe30415ddb61fdc2092903aa77b43924701e8625c9acca432d3b80cf6433042"),
            ("logos", TIER_NORMAL, 1103, "51fca46480d62d77d4756d18cdda0f4a121b794d434d088bb65f08f97e9b7c67"),
        ] {
            let (proof, got) = length_proof(&derive(name).id).expect("a proof");
            assert_eq!(got, tier, "{name}");
            let is_member = matches!(proof, LengthProof::Membership(_));
            assert_eq!(is_member, tier != TIER_NORMAL, "{name}");
            let bytes = encoded(&proof);
            assert_eq!(bytes.len(), length, "{name}");
            assert_eq!(digest(&bytes), sha, "{name}");
        }
    }

    #[test]
    fn a_hand_built_non_membership_proof_encodes_as_borsh() {
        let proof = LengthProof::NonMembership(NonMembership {
            left: Leaf { id: SENTINEL_LOW, tier: TIER_SENTINEL },
            left_path: vec![[0xAA; 32]],
            right: Leaf { id: SENTINEL_HIGH, tier: TIER_SENTINEL },
            right_path: vec![[0xBB; 32]],
            left_index: 9,
        });
        assert_eq!(
            hex(&encoded(&proof)),
            format!(
                "01{}ff01000000{}{}ff01000000{}09000000",
                "00".repeat(32),
                "aa".repeat(32),
                "ff".repeat(32),
                "bb".repeat(32)
            )
        );
        let member = LengthProof::Membership(Membership { index: 7, tier: TIER_THREE, path: Vec::new() });
        assert_eq!(hex(&encoded(&member)), "00070000000300000000");
    }

    #[test]
    fn proofs_verify_and_tampering_is_caught() {
        let list = ShortList::new(vec![([0x11; 32], TIER_LOCKED), ([0x33; 32], TIER_THREE), ([0x55; 32], TIER_THREE)])
            .expect("a list");
        let check = |id: &[u8; 32], proof: &LengthProof| verify_depth(&list.root(), id, proof, list.depth());
        assert_eq!(check(&[0x11; 32], &list.prove(&[0x11; 32]).unwrap()), Ok(TIER_LOCKED));
        assert_eq!(check(&[0x33; 32], &list.prove(&[0x33; 32]).unwrap()), Ok(TIER_THREE));
        assert_eq!(check(&[0x44; 32], &list.prove(&[0x44; 32]).unwrap()), Ok(TIER_NORMAL));
        // A short name can't pass as a normal one with a neighbour's proof.
        assert!(check(&[0x33; 32], &list.prove(&[0x44; 32]).unwrap()).is_err());
        let LengthProof::Membership(mut forged) = list.prove(&[0x33; 32]).unwrap() else { panic!("membership") };
        forged.tier = TIER_NORMAL;
        assert!(check(&[0x33; 32], &LengthProof::Membership(forged)).is_err());
        assert!(list.prove(&SENTINEL_LOW).is_err());
    }
}
