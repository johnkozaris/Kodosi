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
    #[cfg(any(target_os = "linux", target_os = "android"))]
    include!("bindings_linux.rs");
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    include!("bindings_macos.rs");
    #[cfg(windows)]
    include!("bindings_windows.rs");
}

#[allow(clippy::wildcard_imports)]
pub(crate) use bindings::*;

pub mod raw;
