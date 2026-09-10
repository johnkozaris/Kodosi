#![allow(
    clippy::expect_used,
    reason = "a malformed or unavailable protocol authority must fail the build"
)]

use std::{env, fs, path::PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let authority_path = manifest_dir.join("../protocol/backend-api-authority.json");
    println!("cargo:rerun-if-changed={}", authority_path.display());

    let authority: serde_json::Value =
        serde_json::from_slice(&fs::read(&authority_path).expect("read backend API authority"))
            .expect("parse backend API authority");
    let api = authority["apiContractVersion"]
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
        .expect("apiContractVersion must be a u32");
    let auth = authority["authContractVersion"]
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
        .expect("authContractVersion must be a u32");
    let output = format!(
        "pub(crate) const EXPECTED_BACKEND_API_CONTRACT: u32 = {api};\nconst EXPECTED_BACKEND_AUTH_CONTRACT: u32 = {auth};\n"
    );
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("out dir"));
    fs::write(out_dir.join("backend_contract_versions.rs"), output)
        .expect("write backend contract versions");
}
