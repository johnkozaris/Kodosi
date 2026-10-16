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
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    include!("bindings_macos.rs");
    #[cfg(windows)]
    include!("bindings_windows.rs");
}

#[allow(clippy::wildcard_imports)]
pub(crate) use bindings::*;

pub mod raw;

#[cfg(test)]
mod tests {
    use super::*;

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
