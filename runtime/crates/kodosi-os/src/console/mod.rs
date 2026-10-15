use std::io;

#[cfg(unix)]
mod unix;
#[cfg(unix)]
use unix as imp;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as imp;

pub struct RawMode {
    _restore: imp::RawMode,
}

impl RawMode {
    pub fn enter() -> io::Result<Self> {
        imp::RawMode::enter().map(|restore| Self { _restore: restore })
    }
}

pub fn size() -> io::Result<(u16, u16)> {
    imp::size()
}

pub struct Resizes(imp::Resizes);

impl Resizes {
    pub fn new() -> io::Result<Self> {
        imp::Resizes::new().map(Self)
    }

    /// Cancel-safe.
    pub async fn changed(&mut self) -> io::Result<(u16, u16)> {
        self.0.changed().await
    }
}
