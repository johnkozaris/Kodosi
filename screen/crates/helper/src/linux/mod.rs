//! The helper on Linux: the desktop's portal shares the displays and takes the input, `PipeWire`
//! brings the frames on a thread of their own, and libei plays viewers' input. The host runs the
//! helper as its child, linked through its standard input and output.

mod away;
mod capture;
mod clipboard;
mod hand;
mod input;
mod keymap;
mod keys;
mod notice;
mod portal;
mod session;

use std::{ffi::OsString, path::PathBuf, time::Duration};

pub(crate) fn run() {
    let Some(arguments) = Arguments::read(std::env::args_os().skip(1)) else {
        return;
    };
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return;
    };
    // libei's connection stays on the helper's thread.
    tokio::task::LocalSet::new().block_on(&runtime, session::serve(arguments));
    // Reading the host's link blocks a thread, which would keep the helper from quitting.
    runtime.shutdown_timeout(Duration::ZERO);
}

/// `--token <token> --grant <file>`.
struct Arguments {
    token: String,
    /// Where the helper keeps what the desktop granted it, to share the screen again without
    /// asking the person at the computer.
    grant: PathBuf,
}

impl Arguments {
    fn read(mut arguments: impl Iterator<Item = OsString>) -> Option<Self> {
        let (mut token, mut grant) = (None, None);
        while let Some(argument) = arguments.next() {
            if argument == "--token" {
                token = arguments.next().and_then(|token| token.into_string().ok());
            } else if argument == "--grant" {
                grant = arguments.next().map(PathBuf::from);
            }
        }
        Some(Self {
            token: token?,
            grant: grant?,
        })
    }
}
