#[cfg(target_os = "macos")]
mod mac;
#[cfg(not(target_os = "macos"))]
mod software;

#[cfg(target_os = "macos")]
pub(crate) use mac::Encoder;
#[cfg(not(target_os = "macos"))]
pub(crate) use software::Encoder;

const FRAME_RATE: u32 = 60;
/// After a picture that the encoder or the link dropped, the page draws again this much later,
/// so the viewers get its newest picture even when the page then stays still.
const REDRAW_AFTER_MS: i64 = 100;

/// A change gets the few bits that one frame allows, so once the page is still the encoder codes
/// its last picture again and again, and the rate control spends the bits on its detail.
pub(crate) const STILL_MS: i64 = 250;
pub(crate) const REFINE_MS: i64 = 33;
/// `OpenH264` goes from 40 to 54 dB on a still page of text in this many frames, and then spends
/// its bits on nothing.
const REFINEMENTS: u32 = 45;
/// A caret or a typed letter gets fine bits in its own frame, so only changes that add up to this
/// much of the page earn a clean-up.
const LARGE_CHANGE: f64 = 1.0 / 16.0;

#[derive(Default)]
pub(crate) struct Cleanup {
    changed: f64,
    left: u32,
    pictures: u64,
}

impl Cleanup {
    /// Takes a picture that the page drew with `share` of it changed, and returns the picture
    /// after which the page must stay still before a clean-up.
    pub(crate) fn drawn(&mut self, share: f64) -> Option<u64> {
        self.pictures += 1;
        self.changed += share;
        if self.changed >= LARGE_CHANGE {
            self.changed = 0.0;
            self.left = REFINEMENTS;
        }
        (self.left > 0).then_some(self.pictures)
    }

    /// Whether the page drew nothing since `picture` and its clean-up has a frame left.
    pub(crate) fn refine(&mut self, picture: u64) -> bool {
        if picture != self.pictures || self.left == 0 {
            return false;
        }
        self.left -= 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_large_change_earns_a_clean_up_that_a_caret_neither_restarts_nor_starts() {
        let mut cleanup = Cleanup::default();
        let page = cleanup.drawn(1.0).unwrap();
        assert!(cleanup.refine(page));
        let caret = cleanup.drawn(0.001).unwrap();
        assert!(!cleanup.refine(page));
        let mut refined = 1;
        while cleanup.refine(caret) {
            refined += 1;
        }
        assert_eq!(refined, REFINEMENTS);
        assert_eq!(cleanup.drawn(0.001), None);
        let typed = (0..70).find_map(|_| cleanup.drawn(0.001));
        assert!(typed.is_some_and(|picture| cleanup.refine(picture)));
    }
}
