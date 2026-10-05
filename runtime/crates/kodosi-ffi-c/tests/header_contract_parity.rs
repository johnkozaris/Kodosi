#![allow(clippy::expect_used)]
fn header() -> &'static str {
    include_str!(concat!(env!("OUT_DIR"), "/kodosi_runtime.h"))
}

#[test]
fn exported_contract_contains_only_current_entrypoints() {
    let text = header();
    let mut exported = std::collections::BTreeSet::new();
    for (at, _) in text.match_indices("kodosi_") {
        let name = &text[at..];
        let end = name
            .find(|symbol: char| !(symbol.is_ascii_alphanumeric() || symbol == '_'))
            .unwrap_or(name.len());
        if name[end..].starts_with('(') {
            exported.insert(&name[..end]);
        }
    }
    assert_eq!(
        exported.into_iter().collect::<Vec<_>>(),
        [
            "kodosi_abi_version",
            "kodosi_cli_main",
            "kodosi_host_stop",
            "kodosi_last_start_failure",
            "kodosi_protocol_version",
            "kodosi_send_command",
            "kodosi_start",
            "kodosi_stop",
            "kodosi_terminal_connect",
            "kodosi_terminal_disconnect",
            "kodosi_terminal_input",
            "kodosi_terminal_refresh",
        ]
    );
    assert!(text.contains("kodosi_start_failure_t"));
    assert!(text.contains("#define KODOSI_FFI_ABI_VERSION 7"));
}

#[test]
fn checkpoint_callback_remains_synchronous_and_correlated() {
    let text = header();
    assert!(text.contains("typedef int32_t (*kodosi_terminal_checkpoint_cb_t)"));
    for name in [
        "kodosi_callbacks_t",
        "on_event;",
        "on_terminal_data;",
        "on_terminal_control;",
        "on_terminal_connect_result;",
        "on_terminal_checkpoint;",
    ] {
        assert!(text.contains(name), "missing {name}");
    }
}
