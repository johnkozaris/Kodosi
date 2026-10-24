#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod console;
pub mod fs;
pub mod ipc;
pub mod process;
pub mod secrets;

#[cfg(target_os = "linux")]
mod session_bus;

#[cfg(windows)]
mod win;
