//! `lns_imageid <elf>...` prints `<image id hex> <sha256 hex> <path>` for each guest ELF.
use sha2::{Digest, Sha256};

fn main() {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        eprintln!("usage: lns_imageid <elf>...");
        std::process::exit(2);
    }
    for p in paths {
        let elf = std::fs::read(&p).unwrap_or_else(|e| panic!("read {p}: {e}"));
        let id = risc0_binfmt::compute_image_id(&elf).unwrap_or_else(|e| panic!("{p}: {e}"));
        let sha = Sha256::digest(&elf);
        let hex: String = sha.iter().map(|b| format!("{b:02x}")).collect();
        println!("{} {} {}", id, hex, p);
    }
}
