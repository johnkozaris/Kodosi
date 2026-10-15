#![allow(
    clippy::expect_used,
    reason = "malformed current protocol authorities or product version must fail the build"
)]

use std::{env, fmt::Write as _, fs, path::PathBuf};

fn main() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest directory"));
    let version_file = root.join("../VERSION");
    println!("cargo:rerun-if-changed={}", version_file.display());
    let version = fs::read_to_string(&version_file).expect("read the product version");
    let version = version.trim();
    assert!(
        version.split('.').count() == 3
            && version.split('.').all(|part| part.parse::<u32>().is_ok()),
        "VERSION must be MAJOR.MINOR.PATCH"
    );
    println!("cargo:rustc-env=KODOSI_VERSION={version}");
    let mut output = String::new();
    for (file, fields) in [
        (
            "backend-api.json",
            &[
                ("/apiContractVersion", "BACKEND_API_VERSION", "u32"),
                ("/authContractVersion", "AUTH_VERSION", "u32"),
                ("/roomContent/limits/people", "ROOM_PEOPLE_MOST", "usize"),
                (
                    "/roomContent/limits/recipients",
                    "ROOM_RECIPIENTS_MOST",
                    "usize",
                ),
            ][..],
        ),
        (
            "terminal-connections.json",
            &[
                (
                    "/terminalConnectionProtocolVersion",
                    "TERMINAL_CONNECTION_VERSION",
                    "u32",
                ),
                ("/channelProtocolVersion", "CHANNEL_VERSION", "u32"),
            ][..],
        ),
    ] {
        let path = root.join("../protocol").join(file);
        println!("cargo:rerun-if-changed={}", path.display());
        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(path).expect("read protocol authority"))
                .expect("parse protocol authority");
        for (field, constant, kind) in fields {
            let number = value
                .pointer(field)
                .and_then(serde_json::Value::as_u64)
                .and_then(|value| u32::try_from(value).ok())
                .expect("protocol number must fit u32");
            writeln!(output, "pub const {constant}: {kind} = {number};")
                .expect("write protocol constant");
        }
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("output directory"));
    fs::write(out.join("protocol_contract.rs"), output).expect("write protocol constants");
}
