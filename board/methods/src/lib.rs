//! Builds the board guest program with the local RISC Zero toolchain (not
//! reproducible; `cargo risczero build` gives a binary others can re-derive).

include!(concat!(env!("OUT_DIR"), "/methods.rs"));
