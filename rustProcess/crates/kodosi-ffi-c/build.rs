#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

use std::env;
use std::path::PathBuf;

fn main() {
    let crate_dir =
        env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set by cargo");

    let target_dir = env::var("CARGO_TARGET_DIR").map_or_else(
        |_| {
            PathBuf::from(&crate_dir)
                .join("..")
                .join("..")
                .join("target")
        },
        PathBuf::from,
    );
    let out_dir = target_dir.join("include");
    std::fs::create_dir_all(&out_dir)
        .unwrap_or_else(|e| panic!("cbindgen: cannot create {}: {e}", out_dir.display()));
    let out_path = out_dir.join("kodosi_runtime.h");

    let config = cbindgen::Config::from_file(PathBuf::from(&crate_dir).join("cbindgen.toml"))
        .unwrap_or_else(|e| panic!("cbindgen: failed to load cbindgen.toml: {e}"));

    let bindings = cbindgen::Builder::new()
        .with_crate(&crate_dir)
        .with_config(config)
        .generate()
        .unwrap_or_else(|e| panic!("cbindgen: generation failed: {e}"));

    bindings.write_to_file(&out_path);
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=cbindgen.toml");
}
