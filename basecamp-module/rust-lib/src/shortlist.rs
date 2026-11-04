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
/// Depth of the shipped list (1,845,228 leaves padded to 2,097,152).
pub const SHORTLIST_DEPTH: usize = 21;

/// The root the registry program has built in (`core`'s `SHORTLIST_ROOT`).
pub const SHORTLIST_ROOT: [u8; 32] = [
    0x97, 0x78, 0xbe, 0xd4, 0x90, 0x05, 0xfe, 0x74, 0x01, 0xd2, 0x56, 0x85, 0xa2, 0x7d, 0x9c, 0xf3,
    0xe6, 0x81, 0xfa, 0x05, 0x82, 0xb5, 0x62, 0x33, 0x6c, 0xdb, 0x8e, 0xe9, 0x66, 0x1d, 0x99, 0x0c,
];

const LEAF_DOMAIN: &[u8] = b"logos-names/v5/short-leaf";
const NODE_DOMAIN: &[u8] = b"logos-names/v5/short-node";
const EMPTY_DOMAIN: &[u8] = b"logos-names/v5/short-empty";

pub const SENTINEL_LOW: [u8; 32] = [0x00; 32];
pub const SENTINEL_HIGH: [u8; 32] = [0xff; 32];

/// The names CLI this module asks for proofs: the list (61 MB) is built into the CLI, not
/// into this module. Set before each call that needs a proof.
static PROVER: std::sync::Mutex<Option<crate::cli::Setup>> = std::sync::Mutex::new(None);

/// Has [`length_proof`] ask the names CLI `setup` runs (or nothing, when there is none).
pub fn use_cli(setup: Option<crate::cli::Setup>) {
    if let Ok(mut prover) = PROVER.lock() {
        *prover = setup;
    }
}

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

    /// The repository's list (`shortlist/shortlist.bin`), read once: for tests, which check
    /// the proofs the CLI's list gives against this copy of the format.
    #[cfg(test)]
    pub fn from_repo() -> Result<&'static Self, String> {
        static LIST: std::sync::OnceLock<Result<ShortList, String>> = std::sync::OnceLock::new();
        LIST.get_or_init(|| {
            let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../shortlist/shortlist.bin"))
                .map_err(|e| format!("the repository's shortlist.bin: {e}"))?;
            let leaves = bytes
                .chunks_exact(33)
                .map(|chunk| Leaf { id: chunk[..32].try_into().expect("32 bytes"), tier: chunk[32] })
                .collect();
            Self::from_sorted_leaves(leaves).map_err(str::to_owned)
        })
        .as_ref()
        .map_err(|reason| format!("the repository's list is invalid: {reason}"))
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

/// The reserved names (`reserved/reserved.csv` in the repository, one per line, sorted), which
/// the locked list holds besides every name of one to four characters.
static RESERVED: &str = include_str!("../reserved.txt");

/// The tier the locked list gives `name`, worked out from the name itself: one to four
/// characters or a reserved name is locked, anything else normal. For showing what a name
/// takes without downloading the list; a registration still carries a proof from it.
pub fn tier_of_name(name: &str) -> u8 {
    if name.len() <= 4 || RESERVED.lines().any(|reserved| reserved == name) {
        TIER_LOCKED
    } else {
        TIER_NORMAL
    }
}

/// A proof from the names CLI's JSON (`names module length-proof`, `length_proof_json`).
pub fn proof_from_json(v: &serde_json::Value) -> Result<LengthProof, String> {
    let hash = |h: &serde_json::Value| -> Result<[u8; 32], String> {
        let bytes = crate::registry::unhex(h.as_str().unwrap_or_default())?;
        bytes.try_into().map_err(|_| "a proof hash is 32 bytes".to_owned())
    };
    let path = |p: &serde_json::Value| -> Result<Vec<[u8; 32]>, String> {
        p.as_array().ok_or("a proof path is a list")?.iter().map(hash).collect()
    };
    let leaf = |l: &serde_json::Value| -> Result<Leaf, String> {
        Ok(Leaf { id: hash(&l["id"])?, tier: u8::try_from(l["tier"].as_u64().ok_or("a leaf has a tier")?).map_err(|_| "a tier is a byte")? })
    };
    let index = |i: &serde_json::Value| -> Result<u32, String> {
        u32::try_from(i.as_u64().ok_or("a proof has an index")?).map_err(|_| "an index is 32 bits".to_owned())
    };
    match v["kind"].as_str() {
        Some("member") => Ok(LengthProof::Membership(Membership {
            index: index(&v["index"])?,
            tier: u8::try_from(v["leaf_tier"].as_u64().ok_or("a member has a tier")?).map_err(|_| "a tier is a byte")?,
            path: path(&v["path"])?,
        })),
        Some("non_member") => Ok(LengthProof::NonMembership(NonMembership {
            left: leaf(&v["left"])?,
            left_path: path(&v["left_path"])?,
            right: leaf(&v["right"])?,
            right_path: path(&v["right_path"])?,
            left_index: index(&v["left_index"])?,
        })),
        _ => Err("the names CLI's length proof has no kind".to_owned()),
    }
}

/// The proof of where `id` sits relative to the locked-name list, and its tier: made by the
/// names CLI, which carries the list, and checked here against the program's root before use,
/// so a CLI with another list can't make this module send a proof the program refuses.
#[cfg(not(test))]
pub fn length_proof(id: &[u8; 32]) -> Result<(LengthProof, u8), String> {
    let setup = PROVER.lock().ok().and_then(|p| p.clone()).ok_or("the names CLI isn't installed, and it holds the list of locked names a registration needs: install Names first (Welcome)")?;
    let proof = proof_from_json(&crate::cli::run_length_proof(&setup, id)?)?;
    let tier = verify(&SHORTLIST_ROOT, id, &proof).map_err(|reason| format!("the names CLI's list doesn't match the registry program's ({reason}); update the names CLI"))?;
    Ok((proof, tier))
}

/// In tests: the proof from the repository's list, checked the same way.
#[cfg(test)]
pub fn length_proof(id: &[u8; 32]) -> Result<(LengthProof, u8), String> {
    let proof = ShortList::from_repo()?.prove(id).map_err(|reason| format!("no length proof: {reason}"))?;
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
    fn the_repositorys_list_rebuilds_to_the_programs_root() {
        let list = ShortList::from_repo().expect("the list loads");
        assert_eq!(list.leaves().len(), 1_845_228);
        assert_eq!(list.depth(), SHORTLIST_DEPTH);
        assert_eq!(hex(&list.root()), "9778bed49005fe7401d25685a27d9cf3e681fa0582b562336cdb8ee9661d990c");
        assert_eq!(list.root(), SHORTLIST_ROOT);
    }

    /// Names in each tier, with the sha256 of the proof's borsh bytes as
    /// `names_core` encodes it (generated from `core` itself, through the CLI). One to
    /// four characters and the reserved list (`logos`) are locked.
    #[test]
    fn names_get_their_tier_and_the_proof_core_would_build() {
        for (name, tier, length, sha) in [
            ("jef", TIER_LOCKED, 682, "7e85f58dcf1025560dbcc7322726a9d2eb6ca018938db94f1c65888d14627e8b"),
            ("qkz", TIER_LOCKED, 682, "4170261b91e35ad0b323a2562b48f7ee0354f937e31066918cf53e37377bceb4"),
            ("ab", TIER_LOCKED, 682, "297209b6d49d664524bcb3e189c6e31f3f5da8832911303758bca1a6a127910c"),
            ("logos", TIER_LOCKED, 682, "d75142a3665b16da3b3f88d2f617ab940122d0d1392227f3935f0726750d3823"),
            ("walletci", TIER_NORMAL, 1423, "5db7b882f5f286e9bb99809327955a829da56e3d0776544b28044a81b7fc577a"),
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

    /// The display tier agrees with the list for every reserved name, and the copy of the
    /// reserved names is the repository's.
    #[test]
    fn the_display_tier_agrees_with_the_list() {
        let csv = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../reserved/reserved.csv")).unwrap();
        let mut names: Vec<&str> = csv.lines().skip(1).filter_map(|l| l.trim_end_matches('\r').split(',').next()).collect();
        names.sort_unstable();
        names.dedup();
        let copy: Vec<&str> = RESERVED.lines().collect();
        assert_eq!(copy, names, "copy reserved/reserved.csv's names to basecamp-module/rust-lib/reserved.txt");
        for name in ["jef", "apps", "logos", "names", "nameswallet", "logosappstore", "walletci", "forumer", "a", "abcde"] {
            let (_, tier) = length_proof(&derive(name).id).expect("a proof");
            assert_eq!(tier_of_name(name), tier, "{name}");
        }
    }

    /// The names CLI's JSON (`names module length-proof`) parses back to the same proof.
    #[test]
    fn the_clis_json_proofs_parse_to_the_same_proof() {
        let hexes = |p: &[[u8; 32]]| p.iter().map(|h| hex(h)).collect::<Vec<_>>();
        for name in ["jef", "walletci"] {
            let (proof, tier) = length_proof(&derive(name).id).expect("a proof");
            let json = match &proof {
                LengthProof::Membership(m) => serde_json::json!({
                    "tier": tier, "kind": "member", "index": m.index, "leaf_tier": m.tier, "path": hexes(&m.path),
                }),
                LengthProof::NonMembership(n) => serde_json::json!({
                    "tier": tier, "kind": "non_member",
                    "left": { "id": hex(&n.left.id), "tier": n.left.tier }, "left_index": n.left_index, "left_path": hexes(&n.left_path),
                    "right": { "id": hex(&n.right.id), "tier": n.right.tier }, "right_path": hexes(&n.right_path),
                }),
            };
            assert_eq!(proof_from_json(&json).expect("parses"), proof, "{name}");
        }
        assert!(proof_from_json(&serde_json::json!({ "kind": "member", "index": 1 })).is_err());
        assert!(proof_from_json(&serde_json::json!({})).is_err());
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
