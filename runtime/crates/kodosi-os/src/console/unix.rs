use std::io;

use rustix::termios::{self, OptionalActions, Termios};
use tokio::signal::unix::{Signal, SignalKind, signal};

pub(super) struct RawMode {
    original: Termios,
}

impl RawMode {
    pub(super) fn enter() -> io::Result<Self> {
        let original = termios::tcgetattr(io::stdin())?;
        let mut raw = original.clone();
        raw.make_raw();
        termios::tcsetattr(io::stdin(), OptionalActions::Now, &raw)?;
        Ok(Self { original })
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        let _ = termios::tcsetattr(io::stdin(), OptionalActions::Now, &self.original);
    }
}

pub(super) fn size() -> io::Result<(u16, u16)> {
    let size = termios::tcgetwinsize(io::stdout())?;
    Ok((size.ws_col, size.ws_row))
}

pub(super) struct Resizes(Signal);

impl Resizes {
    pub(super) fn new() -> io::Result<Self> {
        signal(SignalKind::window_change()).map(Self)
    }

    pub(super) async fn changed(&mut self) -> io::Result<(u16, u16)> {
        if self.0.recv().await.is_none() {
            return Err(io::Error::other("resize notifications stopped"));
        }
        size()
    }
}
