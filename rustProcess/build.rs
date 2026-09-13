#![allow(
    clippy::expect_used,
    reason = "malformed current protocol authorities must fail the build"
)]

use std::{env, fmt::Write as _, fs, path::PathBuf};

fn main() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest directory"));
    let mut output = String::new();
    for (file, fields) in [
        (
            "backend-api-authority.json",
            &[
                ("apiContractVersion", "BACKEND_API_VERSION"),
                ("authContractVersion", "AUTH_VERSION"),
            ][..],
        ),
        (
            "session-relay-authority.json",
            &[("relayProtocolVersion", "RELAY_VERSION")][..],
        ),
    ] {
        let path = root.join("../protocol").join(file);
        println!("cargo:rerun-if-changed={}", path.display());
        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(path).expect("read protocol authority"))
                .expect("parse protocol authority");
        for (field, constant) in fields {
            let version = value[field]
                .as_u64()
                .and_then(|value| u32::try_from(value).ok())
                .expect("protocol version must fit u32");
            writeln!(output, "pub const {constant}: u32 = {version};")
                .expect("write protocol constant");
        }
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("output directory"));
    fs::write(out.join("network_versions.rs"), output).expect("write current protocol versions");
}
