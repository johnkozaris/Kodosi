#[cfg(target_os = "macos")]
mod mac;
#[cfg(not(target_os = "macos"))]
mod software;
#[cfg(target_os = "macos")]
mod sps;

#[cfg(target_os = "macos")]
pub(crate) use mac::Encoder;
#[cfg(not(target_os = "macos"))]
pub(crate) use software::Encoder;

const FRAME_RATE: u32 = 60;
/// After a picture that the encoder or the link dropped, it goes again this much later, so the
/// viewers get the newest picture even when the page then stays still.
const REDRAW_AFTER_MS: i64 = 100;

/// A change gets the few bits that one frame allows, so once the page is still the encoder codes
/// its last picture again and again, and the rate control spends the bits on its detail.
pub(crate) const STILL_MS: i64 = 250;
pub(crate) const REFINE_MS: i64 = 33;
/// `OpenH264` goes from 40 to 54 dB on a still page of text in 45 frames, and then spends its bits
/// on nothing; its rate control skips some of them.
const REFINEMENTS: u32 = 60;
/// A caret or a typed letter gets fine bits in its own frame, so only changes that add up to this
/// much of the page earn a clean-up.
const LARGE_CHANGE: f64 = 1.0 / 16.0;
/// A refinement of a clean picture takes a third of a frame's bits at most, as the encoder has
/// little left to code. Right after a key frame the rate control gives refinements next to
/// nothing for a while, and a refinement that spends all its bits is still at work.
const CLEAN_AFTER: usize = 3;
const FIRST_REFINEMENTS: usize = 6;

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
    pub(crate) const fn due(&self, picture: u64) -> bool {
        picture == self.pictures && self.left > 0
    }

    /// A busy encoder takes no refinement, so only those that it took count.
    pub(crate) const fn refined(&mut self) {
        self.left = self.left.saturating_sub(1);
    }
}

#[derive(Default)]
pub(crate) struct Convergence {
    sizes: [usize; CLEAN_AFTER],
    refined: usize,
}

impl Convergence {
    /// Takes the size of a coded refinement and the bytes that one frame may have, and returns
    /// whether the picture is clean.
    pub(crate) fn refined(&mut self, bytes: usize, budget: usize) -> bool {
        self.sizes[self.refined % CLEAN_AFTER] = bytes;
        self.refined += 1;
        self.refined >= FIRST_REFINEMENTS && self.sizes.iter().all(|size| size * 3 <= budget)
    }
}

pub(crate) fn frame_budget(bitrate: u32) -> usize {
    usize::try_from(bitrate / 8 / FRAME_RATE).unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_large_change_earns_a_clean_up_that_a_caret_neither_restarts_nor_starts() {
        let mut cleanup = Cleanup::default();
        let page = cleanup.drawn(1.0).unwrap();
        assert!(cleanup.due(page));
        cleanup.refined();
        let caret = cleanup.drawn(0.001).unwrap();
        assert!(!cleanup.due(page));
        let mut refined = 1;
        while cleanup.due(caret) {
            cleanup.refined();
            refined += 1;
        }
        assert_eq!(refined, REFINEMENTS);
        assert_eq!(cleanup.drawn(0.001), None);
        let typed = (0..70).find_map(|_| cleanup.drawn(0.001));
        assert!(typed.is_some_and(|picture| cleanup.due(picture)));
    }

    #[test]
    fn a_picture_is_clean_once_its_refinements_take_a_third_of_a_frame_at_most() {
        let budget = frame_budget(8_000_000);
        let mut sharpening = Convergence::default();
        let sizes = [70_874, 31_974, 6_676, 5_345, 5_157, 5_154];
        let clean = sizes.map(|bytes| sharpening.refined(bytes, budget));
        assert_eq!(clean, [false, false, false, false, false, true]);
        let mut spending = Convergence::default();
        let sizes = [9_659, 27_355, 27_504, 28_981, 29_726, 29_195, 29_000];
        assert!(
            !sizes
                .map(|bytes| spending.refined(bytes, budget))
                .contains(&true)
        );
        let mut after_key = Convergence::default();
        let sizes = [28, 28, 45_585, 55_967, 11_353, 28, 28, 28];
        let clean = sizes.map(|bytes| after_key.refined(bytes, budget));
        assert_eq!(
            clean,
            [false, false, false, false, false, false, false, true]
        );
    }
}
