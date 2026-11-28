// Native build by default. With RISC0_USE_DOCKER=1 the guest builds in RISC Zero's pinned
// Docker builder (the registry repo's deploy/repro-build.sh), with the workspace root as the Docker context, so
// anyone gets the same ELF and image id from this commit.
use std::{collections::HashMap, env, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=RISC0_USE_DOCKER");
    println!("cargo:rerun-if-env-changed=RISC0_DOCKER_CONTAINER_TAG");
    let docker = env::var("RISC0_USE_DOCKER").map(|v| !v.is_empty() && v != "0").unwrap_or(false);
    if !docker {
        risc0_build::embed_methods();
        return;
    }
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"))
        .join("..")
        .canonicalize()
        .expect("workspace root");
    let docker = risc0_build::DockerOptionsBuilder::default().root_dir(root).build().expect("DockerOptions");
    let guest = risc0_build::GuestOptionsBuilder::default().use_docker(docker).build().expect("GuestOptions");
    risc0_build::embed_methods_with_options(HashMap::from([("lez_nft_guest", guest)]));
}
