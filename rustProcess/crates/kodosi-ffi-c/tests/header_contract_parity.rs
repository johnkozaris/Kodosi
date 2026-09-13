#![allow(clippy::expect_used)]
use std::path::PathBuf;

fn header() -> String {
    let target = std::env::var_os("CARGO_TARGET_DIR").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target"),
        PathBuf::from,
    );
    std::fs::read_to_string(target.join("include/kodosi_runtime.h")).expect("generated header")
}

#[test]
fn exported_contract_contains_only_current_entrypoints() {
    let text = header();
    for name in [
        "kodosi_start(",
        "kodosi_stop(",
        "kodosi_send_command(",
        "kodosi_protocol_version(",
        "kodosi_abi_version(",
        "kodosi_terminal_input(",
        "kodosi_terminal_connect(",
        "kodosi_terminal_refresh(",
        "kodosi_terminal_disconnect(",
    ] {
        assert!(text.contains(name), "missing {name}");
    }
    for removed in [
        "_v2",
        "on_auth_event",
        "on_agent_intel_event",
        "on_agent_global_event",
        "kodosi_send_sessions",
        "kodosi_send_system",
        "kodosi_send_agent_intel",
        "PRESENTATION",
        "_capability",
    ] {
        assert!(
            !text.contains(removed),
            "retired contract survived: {removed}"
        );
    }
    assert!(text.contains("#define KODOSI_FFI_ABI_VERSION 6"));
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
