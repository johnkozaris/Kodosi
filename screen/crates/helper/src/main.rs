//! "Kodosi Screen": captures this computer's screens for the host and plays its viewers' input.
//! On a Mac it runs as an app of its own so that macOS asks for Screen Recording and
//! Accessibility in its name; on Linux the host runs it as its child, and the desktop's portal
//! asks the person at the computer; on Windows the host runs it as its child, without a console
//! of its own. It quits when the host's link closes.

#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod mac;
#[cfg(windows)]
mod windows;

fn main() {
    #[cfg(target_os = "macos")]
    mac::run();
    #[cfg(target_os = "linux")]
    linux::run();
    #[cfg(windows)]
    windows::run();
}
