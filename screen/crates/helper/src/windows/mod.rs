//! The helper on Windows: a hidden window on the main thread shows the notice and hears the
//! session, the displays and the clipboard, the link to the host has a thread of its own, and
//! the desktop's duplication brings frames on another. The host runs the helper as its child,
//! linked through its standard input and output.

mod app;
mod capture;
mod clipboard;
mod hand;
mod input;
mod keys;
mod pointer;
mod power;
mod session;
mod sources;

use std::ffi::OsString;

use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};

pub(crate) fn run() {
    let Some(token) = token(std::env::args_os().skip(1)) else {
        return;
    };
    // The capture, the pointer and the input all count in the displays' own pixels.
    // SAFETY: the helper has no window yet.
    drop(unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) });
    app::run(token);
}

/// `--token <token>`.
fn token(mut arguments: impl Iterator<Item = OsString>) -> Option<String> {
    let mut token = None;
    while let Some(argument) = arguments.next() {
        if argument == "--token" {
            token = arguments.next().and_then(|token| token.into_string().ok());
        }
    }
    token
}
