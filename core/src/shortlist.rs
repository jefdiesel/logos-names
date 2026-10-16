//! The short-name list: a Merkle commitment to the ids of every name of one to
//! three characters, so the program can lock or surcharge them without ever
//! seeing a name.
//!
//! # Format (registry v5)
//!
//! A *leaf* is `(id: [u8; 32], tier: u8)`. The list holds one leaf per short
//! name (tier [`TIER_LOCKED`] for 1-2 characters, [`TIER_THREE`] for 3), plus two
//! sentinel leaves, `id = 00..00` and `id = ff..ff`, both with tier `0xff`
//! ([`TIER_SENTINEL`]). Leaves are sorted by id, strictly ascending, so no id
//! appears twice and the sentinels are first and last.
//!
//! Hashes, all SHA-256:
//!
//! - leaf:  `H("logos-names/v5/short-leaf" || id || tier)`
//! - node:  `H("logos-names/v5/short-node" || left || right)`
//! - empty: `H("logos-names/v5/short-empty")`
//!
//! Tree shape: with `n` leaves, `depth = ceil(log2(n))` (16 for the real list of
//! 49,286 leaves, which pads to 65,536). Leaf `i` sits at position `i` of level
//! 0; positions `n..2^depth` hold the empty hash. Level `k+1` position `j` is the
//! node hash of level `k` positions `2j` and `2j+1`. The root is the single
//! node at level `depth`. A path lists the sibling at each level from the leaf
//! up; at level `k` the sibling is on the right when bit `k` of the index is 0.
//! Every path therefore has exactly `depth` entries.
//!
//! A proof of *membership* names the index and tier of the id's leaf. A proof
//! of *non-membership* gives the two adjacent leaves `left` (index `i`) and
//! `right` (index `i + 1`) with `left.id < id < right.id`; because the list is
//! sorted and the sentinels bound it, no leaf can lie between them.

use borsh::{BorshDeserialize, BorshSerialize};

/// Tier of a 1-2 character name: locked, only the admin can release it.
pub const TIER_LOCKED: u8 = 1;
/// Tier of a 3 character name: higher deposit.
pub const TIER_THREE: u8 = 3;
/// Result of a valid non-membership proof: a normal name.
pub const TIER_NORMAL: u8 = 0;
/// Tier of the two sentinel leaves. Never a real name.
pub const TIER_SENTINEL: u8 = 0xff;
/// Depth of the shipped list (49,286 leaves padded to 65,536).
pub const SHORTLIST_DEPTH: usize = 16;

pub type Tier = u8;

const LEAF_DOMAIN: &[u8] = b"logos-names/v5/short-leaf";
const NODE_DOMAIN: &[u8] = b"logos-names/v5/short-node";
const EMPTY_DOMAIN: &[u8] = b"logos-names/v5/short-empty";

pub const SENTINEL_LOW: [u8; 32] = [0x00; 32];
pub const SENTINEL_HIGH: [u8; 32] = [0xff; 32];

#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Leaf {
    pub id: [u8; 32],
    pub tier: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Membership {
    pub index: u32,
    pub tier: u8,
    pub path: Vec<[u8; 32]>,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NonMembership {
    pub left: Leaf,
    pub left_path: Vec<[u8; 32]>,
    pub right: Leaf,
    pub right_path: Vec<[u8; 32]>,
    /// Index of `left`; `right` is at `left_index + 1`.
    pub left_index: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum LengthProof {
    Membership(Membership),
    NonMembership(NonMembership),
}

fn sha256(parts: &[&[u8]]) -> [u8; 32] {
    use risc0_zkvm::sha::{Impl, Sha256 as _};
    let mut bytes = Vec::with_capacity(parts.iter().map(|p| p.len()).sum());
    for p in parts {
        bytes.extend_from_slice(p);
    }
    Impl::hash_bytes(&bytes)
        .as_bytes()
        .try_into()
        .expect("SHA-256 output is 32 bytes")
}

#[must_use]
pub fn leaf_hash(id: &[u8; 32], tier: u8) -> [u8; 32] {
    sha256(&[LEAF_DOMAIN, id, &[tier]])
}

#[must_use]
pub fn node_hash(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    sha256(&[NODE_DOMAIN, left, right])
}

#[must_use]
pub fn empty_hash() -> [u8; 32] {
    sha256(&[EMPTY_DOMAIN])
}

fn root_from_path(leaf: [u8; 32], index: u32, path: &[[u8; 32]]) -> [u8; 32] {
    let mut acc = leaf;
    for (level, sibling) in path.iter().enumerate() {
        acc = if (index >> level) & 1 == 0 {
            node_hash(&acc, sibling)
        } else {
            node_hash(sibling, &acc)
        };
    }
    acc
}

fn check_leaf_path(
    root: &[u8; 32],
    leaf: &Leaf,
    index: u32,
    path: &[[u8; 32]],
    depth: usize,
) -> Result<(), &'static str> {
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

/// Checks `proof` about `id` against `root` for a list of the shipped depth
/// ([`SHORTLIST_DEPTH`]) and returns the tier: [`TIER_LOCKED`], [`TIER_THREE`],
/// or [`TIER_NORMAL`] for a valid proof that the id is not on the list.
pub fn verify(root: &[u8; 32], id: &[u8; 32], proof: &LengthProof) -> Result<Tier, &'static str> {
    verify_depth(root, id, proof, SHORTLIST_DEPTH)
}

/// [`verify`] for a list of another depth (tests).
pub fn verify_depth(
    root: &[u8; 32],
    id: &[u8; 32],
    proof: &LengthProof,
    depth: usize,
) -> Result<Tier, &'static str> {
    if *id == SENTINEL_LOW || *id == SENTINEL_HIGH {
        return Err("sentinel ids are not names");
    }
    match proof {
        LengthProof::Membership(m) => {
            if m.tier != TIER_LOCKED && m.tier != TIER_THREE {
                return Err("membership tier must be locked or three");
            }
            let leaf = Leaf { id: *id, tier: m.tier };
            check_leaf_path(root, &leaf, m.index, &m.path, depth)?;
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

// ---------------------------------------------------------------- builder

/// A built list: all levels of the tree, for computing the root and proofs.
#[derive(Debug, Clone)]
pub struct ShortList {
    leaves: Vec<Leaf>,
    /// `levels[0]` are the padded leaf hashes, the last level is the root.
    levels: Vec<Vec<[u8; 32]>>,
}

impl ShortList {
    /// Builds the list from real `(id, tier)` pairs; adds the sentinels. Rejects
    /// duplicate ids, a real id equal to a sentinel, and tiers other than
    /// [`TIER_LOCKED`] or [`TIER_THREE`].
    pub fn new(mut entries: Vec<([u8; 32], u8)>) -> Result<Self, &'static str> {
        if entries.iter().any(|(_, t)| *t != TIER_LOCKED && *t != TIER_THREE) {
            return Err("entry tier must be locked or three");
        }
        entries.sort();
        if entries.windows(2).any(|w| w[0].0 == w[1].0) {
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
        if leaves.windows(2).any(|w| w[0].id >= w[1].id) {
            return Err("leaves are not strictly ascending");
        }
        let inner = &leaves[1..leaves.len() - 1];
        if inner.iter().any(|l| l.tier != TIER_LOCKED && l.tier != TIER_THREE) {
            return Err("entry tier must be locked or three");
        }
        let width = leaves.len().next_power_of_two();
        let empty = empty_hash();
        let mut level: Vec<[u8; 32]> = leaves.iter().map(|l| leaf_hash(&l.id, l.tier)).collect();
        level.resize(width, empty);
        let mut levels = vec![level];
        while levels.last().unwrap().len() > 1 {
            let next = levels
                .last()
                .unwrap()
                .chunks(2)
                .map(|p| node_hash(&p[0], &p[1]))
                .collect();
            levels.push(next);
        }
        Ok(Self { leaves, levels })
    }

    /// Sorted leaves, sentinels included.
    #[must_use]
    pub fn leaves(&self) -> &[Leaf] {
        &self.leaves
    }

    #[must_use]
    pub fn depth(&self) -> usize {
        self.levels.len() - 1
    }

    #[must_use]
    pub fn root(&self) -> [u8; 32] {
        self.levels[self.depth()][0]
    }

    fn path(&self, index: usize) -> Vec<[u8; 32]> {
        (0..self.depth()).map(|k| self.levels[k][(index >> k) ^ 1]).collect()
    }

    /// Proof for `id`: membership if on the list, otherwise non-membership.
    pub fn prove(&self, id: &[u8; 32]) -> Result<LengthProof, &'static str> {
        if *id == SENTINEL_LOW || *id == SENTINEL_HIGH {
            return Err("sentinel ids are not names");
        }
        match self.leaves.binary_search_by(|l| l.id.cmp(id)) {
            Ok(i) => Ok(LengthProof::Membership(Membership {
                index: i as u32,
                tier: self.leaves[i].tier,
                path: self.path(i),
            })),
            Err(pos) => {
                // pos is in 1..len because the sentinels bound every other id.
                let l = pos - 1;
                Ok(LengthProof::NonMembership(NonMembership {
                    left: self.leaves[l],
                    left_path: self.path(l),
                    right: self.leaves[l + 1],
                    right_path: self.path(l + 1),
                    left_index: l as u32,
                }))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic pseudo-random ids (xorshift), no extra dependency.
    fn rand_id(state: &mut u64) -> [u8; 32] {
        let mut out = [0u8; 32];
        for chunk in out.chunks_mut(8) {
            *state ^= *state << 13;
            *state ^= *state >> 7;
            *state ^= *state << 17;
            chunk.copy_from_slice(&state.to_le_bytes());
        }
        out
    }

    fn sample(n: usize, seed: u64) -> ShortList {
        let mut s = seed;
        let entries = (0..n)
            .map(|i| (rand_id(&mut s), if i % 3 == 0 { TIER_LOCKED } else { TIER_THREE }))
            .collect();
        ShortList::new(entries).unwrap()
    }

    fn v(l: &ShortList, id: &[u8; 32], p: &LengthProof) -> Result<Tier, &'static str> {
        verify_depth(&l.root(), id, p, l.depth())
    }

    /// An id strictly between two ids (increments the low one), if room.
    fn between(a: &[u8; 32], b: &[u8; 32]) -> Option<[u8; 32]> {
        let mut x = *a;
        for byte in x.iter_mut().rev() {
            let (n, carry) = byte.overflowing_add(1);
            *byte = n;
            if !carry {
                break;
            }
        }
        (x > *a && x < *b).then_some(x)
    }

    #[test]
    fn membership_for_every_leaf() {
        let l = sample(37, 1);
        assert_eq!(l.leaves().len(), 39);
        assert_eq!(l.depth(), 6);
        for leaf in &l.leaves()[1..l.leaves().len() - 1] {
            let p = l.prove(&leaf.id).unwrap();
            assert!(matches!(p, LengthProof::Membership(_)));
            assert_eq!(v(&l, &leaf.id, &p), Ok(leaf.tier));
        }
    }

    #[test]
    fn non_membership_between_every_pair_and_at_the_ends() {
        let l = sample(37, 2);
        let leaves = l.leaves();
        for w in leaves.windows(2) {
            let id = between(&w[0].id, &w[1].id).unwrap();
            let p = l.prove(&id).unwrap();
            assert!(matches!(p, LengthProof::NonMembership(_)));
            assert_eq!(v(&l, &id, &p), Ok(TIER_NORMAL));
        }
        // Just above the low sentinel and just below the high sentinel.
        let mut lowest = [0u8; 32];
        lowest[31] = 1;
        let mut highest = [0xffu8; 32];
        highest[31] = 0xfe;
        for id in [lowest, highest] {
            let p = l.prove(&id).unwrap();
            assert_eq!(v(&l, &id, &p), Ok(TIER_NORMAL));
        }
        // Below the first and above the last real leaf.
        let first = leaves[1].id;
        let last = leaves[leaves.len() - 2].id;
        for id in [between(&SENTINEL_LOW, &first).unwrap(), between(&last, &SENTINEL_HIGH).unwrap()] {
            let p = l.prove(&id).unwrap();
            assert_eq!(v(&l, &id, &p), Ok(TIER_NORMAL));
        }
    }

    #[test]
    fn works_at_every_small_size_including_padding_edges() {
        for n in 0..40 {
            let l = sample(n, 100 + n as u64);
            let mut s = 999u64;
            for _ in 0..20 {
                let id = rand_id(&mut s);
                let p = l.prove(&id).unwrap();
                assert_eq!(v(&l, &id, &p), Ok(TIER_NORMAL));
            }
            for leaf in &l.leaves()[1..l.leaves().len() - 1] {
                let p = l.prove(&leaf.id).unwrap();
                assert_eq!(v(&l, &leaf.id, &p), Ok(leaf.tier));
            }
        }
    }

    #[test]
    fn full_size_list_has_depth_16_and_uses_the_default_verify() {
        let l = sample(49_284, 7);
        assert_eq!(l.leaves().len(), 49_286);
        assert_eq!(l.depth(), SHORTLIST_DEPTH);
        let root = l.root();
        let leaf = l.leaves()[12_345];
        let p = l.prove(&leaf.id).unwrap();
        assert_eq!(verify(&root, &leaf.id, &p), Ok(leaf.tier));
        let mut s = 5u64;
        let id = rand_id(&mut s);
        let p = l.prove(&id).unwrap();
        assert_eq!(verify(&root, &id, &p), Ok(TIER_NORMAL));
        // Proof sizes.
        let mem = borsh::to_vec(&l.prove(&leaf.id).unwrap()).unwrap();
        let non = borsh::to_vec(&p).unwrap();
        assert_eq!(mem.len(), 1 + 4 + 1 + 4 + 16 * 32);
        assert_eq!(non.len(), 1 + 33 + 4 + 16 * 32 + 33 + 4 + 16 * 32 + 4);
        // Wrong depth for the default verifier is rejected.
        let small = sample(5, 3);
        let sl = small.leaves()[2];
        let sp = small.prove(&sl.id).unwrap();
        assert!(verify(&small.root(), &sl.id, &sp).is_err());
    }

    #[test]
    fn borsh_round_trip() {
        let l = sample(20, 4);
        let id = l.leaves()[3].id;
        for p in [l.prove(&id).unwrap(), l.prove(&[7u8; 32]).unwrap()] {
            let bytes = borsh::to_vec(&p).unwrap();
            assert_eq!(borsh::from_slice::<LengthProof>(&bytes).unwrap(), p);
        }
    }

    #[test]
    fn wrong_root_rejected() {
        let l = sample(20, 5);
        let other = sample(20, 6);
        let id = l.leaves()[4].id;
        let p = l.prove(&id).unwrap();
        assert!(verify_depth(&other.root(), &id, &p, l.depth()).is_err());
    }

    #[test]
    fn membership_adversarial() {
        let l = sample(30, 8);
        let leaf = l.leaves()[5];
        let LengthProof::Membership(good) = l.prove(&leaf.id).unwrap() else { panic!() };

        // wrong tier
        let other_tier = if leaf.tier == TIER_LOCKED { TIER_THREE } else { TIER_LOCKED };
        let mut m = good.clone();
        m.tier = other_tier;
        assert!(v(&l, &leaf.id, &LengthProof::Membership(m)).is_err());
        // tier 0 / normal / sentinel tier claimed
        for t in [TIER_NORMAL, 2, TIER_SENTINEL] {
            let mut m = good.clone();
            m.tier = t;
            assert!(v(&l, &leaf.id, &LengthProof::Membership(m)).is_err());
        }
        // tampered path, every position and bit
        for i in 0..good.path.len() {
            let mut m = good.clone();
            m.path[i][0] ^= 1;
            assert!(v(&l, &leaf.id, &LengthProof::Membership(m)).is_err());
        }
        // short, long, empty paths
        let mut m = good.clone();
        m.path.pop();
        assert!(v(&l, &leaf.id, &LengthProof::Membership(m)).is_err());
        let mut m = good.clone();
        m.path.push([0; 32]);
        assert!(v(&l, &leaf.id, &LengthProof::Membership(m)).is_err());
        let mut m = good.clone();
        m.path.clear();
        assert!(v(&l, &leaf.id, &LengthProof::Membership(m)).is_err());
        // wrong index, index out of range, index with high bits aliasing
        for idx in [good.index ^ 1, good.index + 1, 1u32 << l.depth(), good.index + (1 << l.depth()), u32::MAX] {
            let mut m = good.clone();
            m.index = idx;
            assert!(v(&l, &leaf.id, &LengthProof::Membership(m)).is_err());
        }
        // proof for another id
        let other = l.leaves()[6].id;
        assert!(v(&l, &other, &LengthProof::Membership(good.clone())).is_err());
        // a non-listed id with a borrowed membership proof
        assert!(v(&l, &[9u8; 32], &LengthProof::Membership(good)).is_err());
    }

    #[test]
    fn sentinels_are_never_names() {
        let l = sample(30, 9);
        let last = l.leaves().len() - 1;
        for (i, id) in [(0usize, SENTINEL_LOW), (last, SENTINEL_HIGH)] {
            assert_eq!(l.leaves()[i].tier, TIER_SENTINEL);
            assert!(l.prove(&id).is_err());
            // Hand-built membership proof of a sentinel with its real path.
            for tier in [TIER_SENTINEL, TIER_LOCKED, TIER_THREE, TIER_NORMAL] {
                let m = Membership { index: i as u32, tier, path: l.path(i) };
                assert!(v(&l, &id, &LengthProof::Membership(m)).is_err());
            }
            // Non-membership claimed for the sentinel id itself.
            let p = LengthProof::NonMembership(NonMembership {
                left: l.leaves()[0],
                left_path: l.path(0),
                right: l.leaves()[1],
                right_path: l.path(1),
                left_index: 0,
            });
            assert!(v(&l, &id, &p).is_err());
        }
        // Builder refuses sentinels and bad tiers.
        assert!(ShortList::new(vec![(SENTINEL_LOW, TIER_LOCKED)]).is_err());
        assert!(ShortList::new(vec![(SENTINEL_HIGH, TIER_THREE)]).is_err());
        assert!(ShortList::new(vec![([1; 32], TIER_SENTINEL)]).is_err());
        assert!(ShortList::new(vec![([1; 32], 0)]).is_err());
        assert!(ShortList::new(vec![([1; 32], 1), ([1; 32], 3)]).is_err());
    }

    fn non_member(l: &ShortList, id: &[u8; 32]) -> NonMembership {
        match l.prove(id).unwrap() {
            LengthProof::NonMembership(n) => n,
            LengthProof::Membership(_) => panic!("expected non-membership"),
        }
    }

    #[test]
    fn non_membership_adversarial() {
        let l = sample(30, 10);
        let leaves = l.leaves().to_vec();
        let id = between(&leaves[10].id, &leaves[11].id).unwrap();
        let good = non_member(&l, &id);
        assert_eq!(v(&l, &id, &LengthProof::NonMembership(good.clone())), Ok(TIER_NORMAL));

        // non-membership claimed for an id that IS on the list, using each
        // adjacent pair around it, the pair it sits in, and its own leaf twice
        let member = leaves[10].id;
        for (a, b) in [(9, 10), (10, 11), (9, 11), (10, 10), (11, 10)] {
            let n = NonMembership {
                left: leaves[a],
                left_path: l.path(a),
                right: leaves[b],
                right_path: l.path(b),
                left_index: a as u32,
            };
            assert!(v(&l, &member, &LengthProof::NonMembership(n)).is_err(), "{a},{b}");
        }
        // real proof for a nearby absent id, replayed for the member
        assert!(v(&l, &member, &LengthProof::NonMembership(good.clone())).is_err());

        // swapped neighbours
        let mut n = good.clone();
        std::mem::swap(&mut n.left, &mut n.right);
        std::mem::swap(&mut n.left_path, &mut n.right_path);
        assert!(v(&l, &id, &LengthProof::NonMembership(n)).is_err());
        let mut n = good.clone();
        std::mem::swap(&mut n.left_path, &mut n.right_path);
        assert!(v(&l, &id, &LengthProof::NonMembership(n)).is_err());

        // non-adjacent leaves around the id (skip one leaf in between)
        let wide = between(&leaves[10].id, &leaves[11].id).unwrap();
        let n = NonMembership {
            left: leaves[10],
            left_path: l.path(10),
            right: leaves[12],
            right_path: l.path(12),
            left_index: 10,
        };
        assert!(v(&l, &wide, &LengthProof::NonMembership(n.clone())).is_err());
        // ...even when the id is valid between those two and the indices lie
        let mut lie = n.clone();
        lie.left_index = 10;
        let id_wide = between(&leaves[11].id, &leaves[12].id).unwrap();
        assert!(v(&l, &id_wide, &LengthProof::NonMembership(lie)).is_err());
        // far apart: sentinels as the pair
        let n = NonMembership {
            left: leaves[0],
            left_path: l.path(0),
            right: *leaves.last().unwrap(),
            right_path: l.path(leaves.len() - 1),
            left_index: 0,
        };
        assert!(v(&l, &id, &LengthProof::NonMembership(n)).is_err());

        // wrong left_index
        for idx in [good.left_index + 1, good.left_index.wrapping_sub(1), u32::MAX, 1 << l.depth()] {
            let mut n = good.clone();
            n.left_index = idx;
            assert!(v(&l, &id, &LengthProof::NonMembership(n)).is_err());
        }
        // tampered leaf contents / tier / paths
        let mut n = good.clone();
        n.left.tier ^= 0x01;
        assert!(v(&l, &id, &LengthProof::NonMembership(n)).is_err());
        let mut n = good.clone();
        n.right.tier = TIER_SENTINEL;
        assert!(v(&l, &id, &LengthProof::NonMembership(n)).is_err());
        for i in 0..good.left_path.len() {
            let mut n = good.clone();
            n.left_path[i][5] ^= 0x80;
            assert!(v(&l, &id, &LengthProof::NonMembership(n)).is_err());
            let mut n = good.clone();
            n.right_path[i][5] ^= 0x80;
            assert!(v(&l, &id, &LengthProof::NonMembership(n)).is_err());
        }
        // wrong path lengths
        let mut n = good.clone();
        n.left_path.pop();
        assert!(v(&l, &id, &LengthProof::NonMembership(n)).is_err());
        let mut n = good.clone();
        n.right_path.push([0; 32]);
        assert!(v(&l, &id, &LengthProof::NonMembership(n)).is_err());

        // id equal to either bound (not strict)
        let lo = good.left.id;
        let hi = good.right.id;
        assert!(v(&l, &lo, &LengthProof::NonMembership(good.clone())).is_err());
        assert!(v(&l, &hi, &LengthProof::NonMembership(good.clone())).is_err());
        // id outside the bounds
        assert!(v(&l, &leaves[3].id, &LengthProof::NonMembership(good)).is_err());
    }

    #[test]
    fn last_real_leaf_and_padding_cannot_be_forged() {
        // A list whose leaf count is not a power of two: the right bound of the
        // top gap is the high sentinel; padding (empty hashes) must not verify
        // as a leaf at any index.
        let l = sample(26, 11); // 28 leaves -> 32 wide
        let n = l.leaves().len();
        let id = [0xfeu8; 32];
        let mut forged = non_member(&l, &id);
        // Claim the pair (high sentinel, padding slot): no leaf can be there.
        forged.left = l.leaves()[n - 1];
        forged.left_path = l.path(n - 1);
        forged.right = Leaf { id: [0xff; 32], tier: TIER_SENTINEL };
        forged.right_path = l.path(n);
        forged.left_index = (n - 1) as u32;
        assert!(v(&l, &id, &LengthProof::NonMembership(forged)).is_err());
        // Membership at a padding index with an arbitrary id.
        let m = Membership { index: n as u32, tier: TIER_LOCKED, path: l.path(n) };
        assert!(v(&l, &id, &LengthProof::Membership(m)).is_err());
    }

    #[test]
    fn domains_separate_leaf_node_and_empty() {
        let a = [1u8; 32];
        assert_ne!(leaf_hash(&a, 1), node_hash(&a, &a));
        assert_ne!(empty_hash(), leaf_hash(&a, 1));
        assert_ne!(leaf_hash(&a, 1), leaf_hash(&a, 3));
    }

    #[test]
    fn from_sorted_leaves_rejects_malformed_files() {
        let l = sample(5, 12);
        let mut leaves = l.leaves().to_vec();
        assert!(ShortList::from_sorted_leaves(leaves.clone()).is_ok());
        leaves.swap(2, 3);
        assert!(ShortList::from_sorted_leaves(leaves).is_err());
        let mut leaves = l.leaves().to_vec();
        leaves.remove(0);
        assert!(ShortList::from_sorted_leaves(leaves).is_err());
        let mut leaves = l.leaves().to_vec();
        leaves[2].tier = 0;
        assert!(ShortList::from_sorted_leaves(leaves).is_err());
    }
}
