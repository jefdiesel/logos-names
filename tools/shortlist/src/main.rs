//! Generates and checks the registry's short-name list (see SPEC.md and
//! `names_core::shortlist`).
//!
//! `shortlist generate <out-dir> [--threads N] [--limit N]`
//! `shortlist verify <out-dir>`

use names_core::shortlist::{
    Leaf, LengthProof, ShortList, TIER_LOCKED, TIER_NORMAL, TIER_THREE, verify_depth,
};
use std::{
    fs,
    path::Path,
    process::ExitCode,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

const ALNUM: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
const MID: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789-";

/// Every valid name of length 1 to 3, with its tier.
fn enumerate() -> Vec<(String, u8)> {
    let mut out = Vec::with_capacity(49_284);
    for &a in ALNUM {
        out.push(((a as char).to_string(), TIER_LOCKED));
    }
    for &a in ALNUM {
        for &b in ALNUM {
            out.push((format!("{}{}", a as char, b as char), TIER_LOCKED));
        }
    }
    for &a in ALNUM {
        for &b in MID {
            for &c in ALNUM {
                out.push((format!("{}{}{}", a as char, b as char, c as char), TIER_THREE));
            }
        }
    }
    out
}

fn derive_all(names: Vec<(String, u8)>, threads: usize) -> Vec<([u8; 32], u8)> {
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let out = Mutex::new(Vec::with_capacity(names.len()));
    let total = names.len();
    std::thread::scope(|s| {
        for _ in 0..threads.max(1) {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some((name, tier)) = names.get(i) else { break };
                    // Each derivation may use 64 MiB (Argon2id), hence the worker cap.
                    let id = names_seal::derive(name).id;
                    out.lock().unwrap().push((id, *tier));
                    let d = done.fetch_add(1, Ordering::Relaxed) + 1;
                    if d % 1000 == 0 {
                        eprintln!("derived {d}/{total}");
                    }
                }
            });
        }
    });
    out.into_inner().unwrap()
}

fn generate(dir: &Path, threads: usize, limit: Option<usize>) -> Result<(), String> {
    let mut names = enumerate();
    if let Some(n) = limit {
        // Spread the subset over all lengths instead of taking only 1-char names.
        let total = names.len();
        let stride = (total / n.max(1)).max(1);
        names = names.into_iter().step_by(stride).take(n).collect();
    } else {
        assert_eq!(names.len(), 49_284);
    }
    eprintln!("deriving {} ids with {threads} threads", names.len());
    let entries = derive_all(names, threads);
    let list = ShortList::new(entries).map_err(|e| e.to_string())?;
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut bin = Vec::with_capacity(list.leaves().len() * 33);
    for l in list.leaves() {
        bin.extend_from_slice(&l.id);
        bin.push(l.tier);
    }
    fs::write(dir.join("shortlist.bin"), bin).map_err(|e| e.to_string())?;
    let root = hex::encode(list.root());
    fs::write(dir.join("root.hex"), format!("{root}\n")).map_err(|e| e.to_string())?;
    println!("leaves {} depth {} root {root}", list.leaves().len(), list.depth());
    Ok(())
}

fn verify(dir: &Path) -> Result<(), String> {
    let bin = fs::read(dir.join("shortlist.bin")).map_err(|e| e.to_string())?;
    if bin.len() % 33 != 0 {
        return Err("shortlist.bin is not a multiple of 33 bytes".into());
    }
    let leaves: Vec<Leaf> = bin
        .chunks_exact(33)
        .map(|c| Leaf { id: c[..32].try_into().unwrap(), tier: c[32] })
        .collect();
    let list = ShortList::from_sorted_leaves(leaves).map_err(|e| e.to_string())?;
    let want = fs::read_to_string(dir.join("root.hex")).map_err(|e| e.to_string())?;
    let got = hex::encode(list.root());
    if want.trim() != got {
        return Err(format!("root mismatch: file {} rebuilt {got}", want.trim()));
    }
    let root = list.root();
    let n = list.leaves().len();
    let mut checked = 0;
    // Membership at a spread of positions, and its tampered versions must fail.
    for i in (1..n - 1).step_by(((n - 2) / 64).max(1)) {
        let leaf = list.leaves()[i];
        let proof = list.prove(&leaf.id).map_err(|e| e.to_string())?;
        let tier = verify_depth(&root, &leaf.id, &proof, list.depth())?;
        if tier != leaf.tier || !(tier == TIER_LOCKED || tier == TIER_THREE) {
            return Err(format!("membership tier mismatch at {i}"));
        }
        if let LengthProof::Membership(mut m) = proof {
            m.path[0][0] ^= 1;
            if verify_depth(&root, &leaf.id, &LengthProof::Membership(m), list.depth()).is_ok() {
                return Err(format!("tampered proof accepted at {i}"));
            }
        }
        checked += 1;
    }
    // Non-membership for ids not on the list (hashes of a counter), and bounds.
    let mut probes: Vec<[u8; 32]> = vec![[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1], {
        let mut h = [0xff; 32];
        h[31] = 0xfe;
        h
    }];
    for k in 0u32..64 {
        let mut h = [0u8; 32];
        let mut x = u64::from(k).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        for c in h.chunks_mut(8) {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            c.copy_from_slice(&x.to_le_bytes());
        }
        probes.push(h);
    }
    for id in probes {
        if list.leaves().binary_search_by(|l| l.id.cmp(&id)).is_ok() {
            continue;
        }
        let proof = list.prove(&id).map_err(|e| e.to_string())?;
        if verify_depth(&root, &id, &proof, list.depth())? != TIER_NORMAL {
            return Err("non-membership returned a tier".into());
        }
        checked += 1;
    }
    println!("ok: {n} leaves, depth {}, root {got}, {checked} proofs checked", list.depth());
    Ok(())
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let usage = "usage: shortlist generate <out-dir> [--threads N] [--limit N] | shortlist verify <out-dir>";
    match args.first().map(String::as_str) {
        Some("generate") => {
            let dir = args.get(1).ok_or(usage)?;
            let (mut threads, mut limit) = (6usize, None);
            let mut it = args[2..].iter();
            while let Some(flag) = it.next() {
                let val = it.next().ok_or(usage)?.parse::<usize>().map_err(|e| e.to_string())?;
                match flag.as_str() {
                    "--threads" => threads = val,
                    "--limit" => limit = Some(val),
                    _ => return Err(usage.into()),
                }
            }
            generate(Path::new(dir), threads, limit)
        }
        Some("verify") if args.len() == 2 => verify(Path::new(&args[1])),
        _ => Err(usage.into()),
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_and_validity() {
        let names = enumerate();
        assert_eq!(names.len(), 49_284);
        assert_eq!(names.iter().filter(|(_, t)| *t == TIER_LOCKED).count(), 1_332);
        assert_eq!(names.iter().filter(|(_, t)| *t == TIER_THREE).count(), 47_952);
        let mut seen = std::collections::HashSet::new();
        for (n, t) in &names {
            names_core::validate_name(n).unwrap();
            assert_eq!(*t == TIER_LOCKED, n.len() <= 2);
            assert!(seen.insert(n.clone()));
        }
    }

    #[test]
    fn pipeline_on_a_subset() {
        let dir = std::env::temp_dir().join(format!("shortlist-test-{}", std::process::id()));
        // Fake derivation is not available here; exercise file round trip instead.
        let entries: Vec<([u8; 32], u8)> = (1u8..=20).map(|i| ([i.wrapping_mul(37); 32], TIER_THREE)).collect();
        let list = ShortList::new(entries).unwrap();
        fs::create_dir_all(&dir).unwrap();
        let mut bin = Vec::new();
        for l in list.leaves() {
            bin.extend_from_slice(&l.id);
            bin.push(l.tier);
        }
        fs::write(dir.join("shortlist.bin"), bin).unwrap();
        fs::write(dir.join("root.hex"), hex::encode(list.root())).unwrap();
        verify(&dir).unwrap();
        fs::remove_dir_all(&dir).unwrap();
    }
}
