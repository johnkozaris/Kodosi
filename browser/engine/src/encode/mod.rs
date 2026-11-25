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
/// Delay before re-encoding after a dropped frame so viewers still get the latest state if the
/// page goes idle.
const REDRAW_AFTER_MS: i64 = 100;

/// After the page stays idle for `STILL_MS`, re-encode the last frame every `REFINE_MS` so
/// rate control can spend extra bits sharpening static detail.
pub(crate) const STILL_MS: i64 = 250;
pub(crate) const REFINE_MS: i64 = 33;
/// Cap on refinement passes: `OpenH264` converges from 40 to 54 dB on static text in ~45
/// frames (plus some frames skipped by rate control), after which extra passes waste bits.
const REFINEMENTS: u32 = 60;
/// Small updates (like a blinking caret or typed character) already get enough bits in a
/// single frame; only cumulative dirty area reaching this fraction triggers refinement.
const LARGE_CHANGE: f64 = 1.0 / 16.0;
/// Stop refining once `CLEAN_AFTER` consecutive frames stay within 1/3 of a frame's bit
/// budget. Require `FIRST_REFINEMENTS` passes first because rate control initially starves
/// refinements right after a keyframe.
const CLEAN_AFTER: usize = 3;
const FIRST_REFINEMENTS: usize = 6;

#[derive(Default)]
pub(crate) struct Cleanup {
    changed: f64,
    left: u32,
    pictures: u64,
}

impl Cleanup {
    pub(crate) fn drawn(&mut self, share: f64) -> Option<u64> {
        self.pictures += 1;
        self.changed += share;
        if self.changed >= LARGE_CHANGE {
            self.changed = 0.0;
            self.left = REFINEMENTS;
        }
        (self.left > 0).then_some(self.pictures)
    }

    pub(crate) const fn due(&self, picture: u64) -> bool {
        picture == self.pictures && self.left > 0
    }

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
