//! Writes the testimonial guest ELF to the path given (if any) and prints its image ID: the
//! binary to deploy, and the ID the deployed header must show.

fn main() {
    let elf = testimonial_engine_test::testimonial_elf();
    if let Some(path) = std::env::args().nth(1) {
        std::fs::write(&path, elf).expect("write the ELF");
        eprintln!("wrote {} bytes to {path}", elf.len());
    }
    let id = testimonial_engine_test::image_id_of(elf);
    let hex: String = testimonial_engine_test::image_bytes(&id).iter().map(|b| format!("{b:02x}")).collect();
    println!("{hex}");
}
