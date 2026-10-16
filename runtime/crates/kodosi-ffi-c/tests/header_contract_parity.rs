#![allow(clippy::expect_used)]
fn header() -> &'static str {
    include_str!(concat!(env!("OUT_DIR"), "/kodosi_runtime.h"))
}

fn define(name: &str) -> u64 {
    header()
        .lines()
        .find_map(|line| line.strip_prefix(&format!("#define {name} ")))
        .and_then(|value| value.trim().parse().ok())
        .expect("define")
}

fn contract() -> serde_json::Value {
    serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../protocol/desktop-runtime.json"
    )))
    .expect("desktop contract")
}

#[test]
fn desktop_contract_states_the_header_abi() {
    assert_eq!(
        contract()["ffiAbiVersion"],
        define("KODOSI_FFI_ABI_VERSION")
    );
}

#[test]
fn the_header_states_the_terminal_limits_of_the_contract() {
    let terminal = &contract()["terminal"];
    assert_eq!(
        terminal["checkpointMaxBytes"],
        define("KODOSI_TERMINAL_SNAPSHOT_MAX_BYTES")
    );
    assert_eq!(
        terminal["imageStorageMaxBytes"],
        define("KODOSI_TERMINAL_IMAGE_STORAGE_MAX_BYTES")
    );
}
