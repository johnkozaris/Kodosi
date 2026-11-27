//! The helper on X11, with every desktop there, also one on a server that people reach over RDP or
//! VNC. The X server shows the screen through a pixmap in shared memory, which it fills only where
//! the screen changed, and plays viewers' input as its test devices. X11 asks nobody first, so the
//! switch in Kodosi on the computer is the person's word, as on Windows. One connection captures
//! on a thread of its own; another one plays input, keeps the clipboard and lists the displays.

mod capture;
mod desk;
mod keys;
mod session;

pub(super) use session::serve;

/// A display as the X server places it on its screen, in pixels, which are its points: X11 knows
/// no scale of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Monitor {
    pub(super) x: i16,
    pub(super) y: i16,
    pub(super) width: u16,
    pub(super) height: u16,
}

impl Monitor {
    fn origin(self) -> (f64, f64) {
        (f64::from(self.x), f64::from(self.y))
    }

    fn size(self) -> (f64, f64) {
        (f64::from(self.width), f64::from(self.height))
    }

    fn pixels(self) -> usize {
        usize::from(self.width) * usize::from(self.height)
    }
}

/// What the X server said wrong, for the host.
fn problem(error: impl std::fmt::Display) -> String {
    format!("the X server: {error}")
}
