//! "Kodosi Screen": captures this computer's screens and windows for the host and plays its
//! viewers' input. It runs as an app of its own so that macOS asks for Screen Recording and
//! Accessibility in its name, and it quits when the host's link closes.

#[cfg(target_os = "macos")]
mod mac;

fn main() {
    #[cfg(target_os = "macos")]
    mac::run();
}
