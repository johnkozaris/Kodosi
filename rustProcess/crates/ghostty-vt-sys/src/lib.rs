#![allow(unsafe_code)]

#[allow(
    clippy::doc_markdown,
    clippy::missing_safety_doc,
    clippy::struct_field_names,
    clippy::too_many_arguments,
    clippy::undocumented_unsafe_blocks,
    clippy::unreadable_literal,
    dead_code,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    rustdoc::bare_urls,
    rustdoc::broken_intra_doc_links,
    rustdoc::invalid_html_tags
)]
mod bindings {
    #[cfg(target_os = "linux")]
    include!("bindings_linux.rs");
    #[cfg(target_os = "macos")]
    include!("bindings_macos.rs");
}

#[allow(clippy::wildcard_imports)]
pub(crate) use bindings::*;

pub mod raw;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_bindings_exclude_upstream_snapshot_api() {
        #[cfg(target_os = "linux")]
        let bindings = include_str!("bindings_linux.rs");
        #[cfg(target_os = "macos")]
        let bindings = include_str!("bindings_macos.rs");
        assert!(!bindings.contains("GhosttySnapshot"));
        assert!(!bindings.contains("ghostty_snapshot_"));
    }

    #[test]
    fn checkpoint_abi_layout_matches_64_bit_header() {
        use std::mem::{align_of, offset_of, size_of};

        assert_eq!(size_of::<GhosttyCheckpointLimits>(), 40);
        assert_eq!(align_of::<GhosttyCheckpointLimits>(), 8);
        assert_eq!(offset_of!(GhosttyCheckpointLimits, size), 0);
        assert_eq!(offset_of!(GhosttyCheckpointLimits, max_json_bytes), 8);
        assert_eq!(offset_of!(GhosttyCheckpointLimits, max_string_bytes), 16);
        assert_eq!(
            offset_of!(GhosttyCheckpointLimits, max_continuation_bytes),
            24
        );
        assert_eq!(offset_of!(GhosttyCheckpointLimits, max_cells), 32);

        assert_eq!(size_of::<GhosttyCheckpointEncodeOptions>(), 48);
        assert_eq!(align_of::<GhosttyCheckpointEncodeOptions>(), 8);
        assert_eq!(offset_of!(GhosttyCheckpointEncodeOptions, size), 0);
        assert_eq!(offset_of!(GhosttyCheckpointEncodeOptions, limits), 8);

        assert_eq!(size_of::<GhosttyCheckpointRestoreOptions>(), 48);
        assert_eq!(align_of::<GhosttyCheckpointRestoreOptions>(), 8);
        assert_eq!(offset_of!(GhosttyCheckpointRestoreOptions, size), 0);
        assert_eq!(offset_of!(GhosttyCheckpointRestoreOptions, limits), 8);

        assert_eq!(size_of::<GhosttyCheckpointInfo>(), 32);
        assert_eq!(align_of::<GhosttyCheckpointInfo>(), 8);
        assert_eq!(offset_of!(GhosttyCheckpointInfo, size), 0);
        assert_eq!(offset_of!(GhosttyCheckpointInfo, schema_version), 8);
        assert_eq!(offset_of!(GhosttyCheckpointInfo, json_bytes), 16);
        assert_eq!(offset_of!(GhosttyCheckpointInfo, continuation_bytes), 24);
    }

    #[test]
    fn linked_checkpoint_schema_is_two() {
        assert_eq!(GHOSTTY_CHECKPOINT_SCHEMA_VERSION, 2);

        assert_eq!(unsafe { ghostty_checkpoint_schema() }, 2);
    }

    #[test]
    fn clipboard_write_layout_matches_target_header() {
        assert_eq!(std::mem::size_of::<GhosttyClipboardWrite>(), 72);
        assert_eq!(std::mem::offset_of!(GhosttyClipboardWrite, reply), 64);
        assert_eq!(std::mem::size_of::<GhosttyClipboardWriteReply>(), 16);
    }

    #[test]
    fn terminal_create_write_and_free_links() {
        let mut terminal = std::ptr::null_mut();

        let result = unsafe {
            ghostty_terminal_new(std::ptr::null(), std::ptr::from_mut(&mut terminal), 80, 24)
        };
        assert_eq!(result, GhosttyResult_GHOSTTY_SUCCESS);
        assert!(!terminal.is_null());

        unsafe {
            ghostty_terminal_vt_write(terminal, b"hello".as_ptr(), 5);
            ghostty_terminal_free(terminal);
        }
    }
}
