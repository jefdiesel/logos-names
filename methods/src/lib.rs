//! Builds the guest program with the local RISC Zero toolchain. This build is
//! not reproducible; use `cargo risczero build` (Docker) for a binary whose
//! image ID others can re-derive from source.

include!(concat!(env!("OUT_DIR"), "/methods.rs"));
