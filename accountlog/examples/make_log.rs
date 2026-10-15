//! Makes a throwaway Logos account and a signed AccountLog endorsing an LNS name,
//! for testing `names verify-account`:
//!
//!     cargo run -p names_accountlog --example make_log -- jef.lez <dir>
//!
//! Writes `<dir>/account.key` (the 32-byte seed), `<dir>/address` and `<dir>/log.hex`.
use ed25519_dalek::{Signer as _, SigningKey};
use std::io::Read as _;

fn main() {
    let mut args = std::env::args().skip(1);
    let name = args.next().expect("usage: make_log <name.lez> <dir>");
    let dir = std::path::PathBuf::from(args.next().expect("usage: make_log <name.lez> <dir>"));
    let mut seed = [0; 32];
    std::fs::File::open("/dev/urandom").unwrap().read_exact(&mut seed).unwrap();
    let key = SigningKey::from_bytes(&seed);
    let mut payload = names_accountlog::DOMAIN.to_vec();
    payload.extend(names_accountlog::name_entry(&name));
    let mut signed = key.sign(&payload).to_bytes().to_vec();
    signed.extend(&payload);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("account.key"), seed).unwrap();
    let address: String = key.verifying_key().to_bytes().iter().map(|b| format!("{b:02x}")).collect();
    std::fs::write(dir.join("address"), &address).unwrap();
    let log: String = signed.iter().map(|b| format!("{b:02x}")).collect();
    std::fs::write(dir.join("log.hex"), log).unwrap();
    println!("{address}");
}
