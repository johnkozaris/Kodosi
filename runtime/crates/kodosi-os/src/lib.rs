#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod console;
pub mod fs;
pub mod ipc;
pub mod network;
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
pub mod power;
pub mod process;
#[cfg(target_os = "linux")]
pub mod screen;
pub mod secrets;

#[cfg(target_os = "linux")]
mod bus;

#[cfg(windows)]
mod win;
