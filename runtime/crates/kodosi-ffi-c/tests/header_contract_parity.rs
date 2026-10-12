#![allow(clippy::expect_used)]
fn header() -> &'static str {
    include_str!(concat!(env!("OUT_DIR"), "/kodosi_runtime.h"))
}

#[test]
fn desktop_contract_states_the_header_abi() {
    let abi: u64 = header()
        .lines()
        .find_map(|line| line.strip_prefix("#define KODOSI_FFI_ABI_VERSION "))
        .and_then(|value| value.trim().parse().ok())
        .expect("ABI version define");
    let contract: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../protocol/desktop-runtime.json"
    )))
    .expect("desktop contract");
    assert_eq!(contract["ffiAbiVersion"], abi);
}
