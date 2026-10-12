use std::io;

#[cfg(unix)]
mod unix;
#[cfg(unix)]
use unix as imp;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as imp;

/// Keeps the console in raw mode, passing keys through as VT bytes, until dropped.
pub struct RawMode {
    _restore: imp::RawMode,
}

impl RawMode {
    pub fn enter() -> io::Result<Self> {
        imp::RawMode::enter().map(|restore| Self { _restore: restore })
    }
}

/// The console's visible size as columns and rows.
pub fn size() -> io::Result<(u16, u16)> {
    imp::size()
}

/// Reports changes to the console's size.
pub struct Resizes(imp::Resizes);

impl Resizes {
    pub fn new() -> io::Result<Self> {
        imp::Resizes::new().map(Self)
    }

    /// Waits for the size to change and returns the new size. Cancel-safe.
    pub async fn changed(&mut self) -> io::Result<(u16, u16)> {
        self.0.changed().await
    }
}
