#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod console;
pub mod fs;
pub mod ipc;
pub mod process;
pub mod secrets;

#[cfg(windows)]
mod win;
