//! The token guest, built with the local RISC Zero toolchain. Not reproducible: use
//! `cargo risczero build` (Docker) for a binary whose image ID others can re-derive.

include!(concat!(env!("OUT_DIR"), "/methods.rs"));
