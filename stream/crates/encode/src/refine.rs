//! When `OpenH264` sharpens a still picture: its rate control spends spare bits on passes that
//! encode the last picture again.

use std::time::Duration;

use crate::FRAME_RATE;

/// A picture that stays this long gets a pass every `PASS_EVERY`.
pub(crate) const STILL: Duration = Duration::from_millis(250);
pub(crate) const PASS_EVERY: Duration = Duration::from_millis(33);
/// Cap on passes: `OpenH264` converges from 40 to 54 dB on static text in ~45 frames (plus some
/// frames skipped by rate control), after which extra passes waste bits.
const REFINEMENTS: u32 = 60;
/// Small updates (like a blinking caret or typed character) already get enough bits in a
/// single frame; only cumulative dirty area reaching this fraction starts passes.
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
}

impl Cleanup {
    /// Counts a picture of which `share` changed; true while it needs passes once it is still.
    pub(crate) fn drawn(&mut self, share: f64) -> bool {
        self.changed += share;
        if self.changed >= LARGE_CHANGE {
            self.changed = 0.0;
            self.left = REFINEMENTS;
        }
        self.left > 0
    }

    /// Counts a pass; true while more are due.
    pub(crate) fn refined(&mut self, clean: bool) -> bool {
        self.left = if clean {
            0
        } else {
            self.left.saturating_sub(1)
        };
        self.left > 0
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
    fn a_large_change_earns_passes_that_a_caret_neither_restarts_nor_starts() {
        let mut cleanup = Cleanup::default();
        assert!(cleanup.drawn(1.0));
        let mut passes = 0;
        while cleanup.refined(false) {
            passes += 1;
            if passes == 1 {
                assert!(cleanup.drawn(0.001));
            }
        }
        assert_eq!(passes + 1, REFINEMENTS);
        assert!(!cleanup.drawn(0.001));
        assert!((0..70).any(|_| cleanup.drawn(0.001)));
        assert!(!cleanup.refined(true));
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
