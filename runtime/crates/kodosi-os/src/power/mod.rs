use std::io;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as imp;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as imp;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as imp;

/// Holds off only the sleep that the computer starts by itself when nobody uses it. A person can
/// still put the computer to sleep, and so can `sleep`.
pub struct Awake {
    _held: imp::Awake,
}

impl Awake {
    pub fn hold(reason: &str) -> io::Result<Self> {
        imp::Awake::hold(reason).map(|held| Self { _held: held })
    }
}

pub enum Sleep {
    /// The computer goes to sleep in a moment.
    Starts,
    /// The computer slept and is awake again.
    Ended,
}

pub fn can_sleep() -> io::Result<()> {
    imp::can_sleep()
}

pub fn sleep() -> io::Result<Sleep> {
    imp::sleep()
}
