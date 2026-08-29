#![allow(clippy::panic)]

use std::{collections::BTreeSet, path::PathBuf};

fn header_text() -> String {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let header_path = std::env::var("CARGO_TARGET_DIR")
        .map_or_else(
            |_| {
                PathBuf::from(manifest_dir)
                    .join("..")
                    .join("..")
                    .join("target")
            },
            PathBuf::from,
        )
        .join("include")
        .join("kodosi_runtime.h");
    std::fs::read_to_string(&header_path).unwrap_or_else(|e| {
        panic!(
            "kodosi_runtime.h not readable at {}: {e}\n\
             Run `cargo build -p kodosi-ffi-c` first to regenerate.",
            header_path.display()
        )
    })
}

#[test]
fn contract_symbols_in_header() {
    let header = header_text();
    let expected: &[&str] = &[
        "kodosi_start_v2",
        "kodosi_stop",
        "kodosi_protocol_version",
        "kodosi_terminal_semantic_checkpoint_v2_capability",
        "kodosi_terminal_connect_result_v2_capability",
        "kodosi_send_terminal",
        "kodosi_send_system",
        "kodosi_send_auth",
        "kodosi_send_friends",
        "kodosi_send_devices",
        "kodosi_send_sessions",
        "kodosi_send_trust",
        "kodosi_send_room",
        "kodosi_send_agent_intel",
        "kodosi_terminal_input",
        "kodosi_terminal_connect_v2",
        "kodosi_terminal_refresh_v2",
        "kodosi_terminal_disconnect_v2",
    ];
    for sym in expected {
        assert!(
            header.contains(sym),
            "symbol `{sym}` missing from generated header"
        );
    }
    assert!(
        !header.contains("kodosi_protocol_version_11_capability"),
        "generated header retains obsolete desktop/runtime protocol-v11 authority"
    );
}

#[test]
fn contract_typedefs_in_header() {
    let header = header_text();
    let expected: &[&str] = &[
        "kodosi_callbacks_v2_t",
        "kodosi_event_cb_t",
        "kodosi_terminal_data_v2_cb_t",
        "kodosi_terminal_control_v2_cb_t",
        "kodosi_terminal_connect_result_v2_cb_t",
        "kodosi_terminal_semantic_checkpoint_v2_cb_t",
    ];
    for typ in expected {
        assert!(
            header.contains(typ),
            "typedef `{typ}` missing from generated header"
        );
    }
}

#[test]
fn contract_constants_in_header() {
    let header = header_text();
    let expected: &[&str] = &[
        "KODOSI_FFI_ABI_VERSION",
        "KODOSI_MAX_FRAME_BYTES",
        "KODOSI_TERMINAL_SEMANTIC_CHECKPOINT_MAX_BYTES",
        "KODOSI_TERMINAL_PRESENTATION_MAX_SERIALIZED_BYTES",
        "KODOSI_TERMINAL_CONTROL_MAX_BYTES",
        "KODOSI_FFI_OK",
        "KODOSI_FFI_NULL_HANDLE",
        "KODOSI_FFI_DESER_FAILED",
        "KODOSI_FFI_PAYLOAD_TOO_LARGE",
        "KODOSI_FFI_RUNTIME_STOPPED",
        "KODOSI_FFI_BUSY",
        "KODOSI_FFI_SESSION_NOT_FOUND",
        "KODOSI_FFI_STALE_SUBSCRIPTION",
        "KODOSI_FFI_TERMINAL_CHECKPOINT_REJECTED",
        "KODOSI_FFI_REQUIRED_CALLBACK_MISSING",
        "KODOSI_FFI_PANIC",
    ];
    for constant in expected {
        assert!(
            header.contains(constant),
            "generated header is missing `{constant}`"
        );
    }
    let expected = expected
        .iter()
        .copied()
        .filter(|name| name.starts_with("KODOSI_FFI_"))
        .collect::<BTreeSet<_>>();
    let actual = header
        .lines()
        .filter_map(|line| {
            let definition = line.strip_prefix("#define ")?;
            let name = definition.split_ascii_whitespace().next()?;
            name.starts_with("KODOSI_FFI_").then_some(name)
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        actual, expected,
        "generated FFI error/ABI constant set drifted"
    );
    assert!(header.contains("#define KODOSI_FFI_ABI_VERSION 5"));
    assert!(header.contains("const char *subscription_id"));
    assert!(header.contains("uint64_t subscription_generation"));
    assert!(header.contains("int32_t kodosi_terminal_disconnect_v2"));
    assert!(header.contains("int32_t kodosi_terminal_refresh_v2"));
}

#[test]
fn callback_table_contains_only_correlated_terminal_callbacks() {
    let header = header_text();
    assert!(!header.contains("on_terminal_data;"));
    assert!(!header.contains("on_terminal_control;"));
    assert!(!header.contains("kodosi_terminal_connect("));
    assert!(!header.contains("kodosi_terminal_disconnect("));
    assert!(header.contains("on_terminal_data_v2;"));
    assert!(header.contains("on_terminal_control_v2;"));
    assert!(header.contains("on_terminal_connect_result_v2;"));
    assert!(header.contains("on_terminal_semantic_checkpoint_v2;"));
}
