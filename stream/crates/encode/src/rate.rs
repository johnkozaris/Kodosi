//! Rate control for encoders that take a quantizer for each frame. Frames may run a few frame
//! times ahead of the link: a new screen shows coarse within a few frames instead of sharp a
//! second later, passes then sharpen it while the link has room, and typing never waits long
//! behind them.
//!
//! A model tracks the quantizer each share of the picture was last coded at. Coding a frame at a
//! finer quantizer also sharpens everything coded coarser, which the model counts as the cost
//! of reaching that quantizer.

use std::time::{Duration, Instant};

use crate::FRAME_RATE;

/// A picture that stays this long gets passes that sharpen it; motion changes it sooner.
pub(crate) const SETTLED: Duration = Duration::from_millis(50);
/// The quantizer at which text is sharp; finer ones spend bits that nobody sees.
pub(crate) const SHARP: u8 = 22;
/// The coarsest quantizer that H.264 and HEVC allow.
const COARSEST: u8 = 51;
const LEVELS: usize = COARSEST as usize + 1;
/// Quantizer steps that halve a frame. Camera video halves every 6 steps, but text keeps its
/// sharp edges, so screens halve more slowly.
const HALVING: f64 = 8.0;
/// Frame times of link that a new picture may fill.
const BURST: f64 = 4.0;
/// Frame times of link that a pass may leave queued, which is the longest a new picture waits
/// behind one.
const PASS: f64 = 2.0;
/// Changes this close together are motion, which costs less than new detail because the
/// encoder predicts it from the last picture.
const MOTION: Duration = Duration::from_millis(50);
/// A pass that costs more than `PASS` frame times even in its smallest step waits until the
/// picture has stayed this long, so typing rarely waits behind one.
const QUIET: Duration = Duration::from_millis(250);
/// Bytes per pixel at quantizer 0 in the model for detail that the encoder has not seen. Text on
/// a screen measures about half of it, so a new screen errs towards a small frame however cheap
/// the last one was; each frame then corrects the model.
const UNSEEN: f64 = 2.0;
const FIRST_MOTION: f64 = 0.5;
/// Bounds on the model's measures of the picture.
const SIMPLEST: f64 = 0.1;
const MOST_COMPLEX: f64 = 20.0;
/// Frames smaller than this are mostly headers, which tell nothing about the picture.
const LEARNS_FROM: f64 = 2048.0;
/// The most that one frame moves the model, as a factor.
const MOST_CORRECTION: f64 = 8.0;
/// Shares of the picture below this are rounding.
const SPECK: f64 = 1e-4;
/// Bounds, in seconds, on how long the controller waits for the link before it looks again.
const SHORTEST_WAIT: f64 = 0.001;
const LONGEST_WAIT: f64 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Content {
    Detail,
    Motion,
}

/// A frame's quantizer, and what the controller expects the frame to cost.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Choice {
    pub(crate) quantizer: u8,
    expected: f64,
    /// What the cost mostly depends on, and the model's measure of it that the guess used.
    content: Content,
    complexity: f64,
}

pub(crate) struct Rate {
    /// Bytes per second that the link carries.
    pace: f64,
    /// Bytes sent ahead of the link, as of `at`.
    queued: f64,
    at: Instant,
    /// Bytes per pixel at quantizer 0 in the model.
    detail: f64,
    motion: f64,
    /// The share of the picture last coded at each quantizer.
    coded: [f64; LEVELS],
    /// When a picture last brought something new.
    changed_at: Option<Instant>,
}

pub(crate) fn pixels(width: usize, height: usize) -> f64 {
    f64::from(u32::try_from(width.saturating_mul(height)).unwrap_or(u32::MAX))
}

fn scale(quantizer: u8) -> f64 {
    (-f64::from(quantizer) / HALVING).exp2()
}

/// A floor of 8 kbit/s keeps the link's pace above zero.
fn pace(bitrate: u32) -> f64 {
    f64::from(bitrate.max(8_000)) / 8.0
}

impl Rate {
    pub(crate) fn new(bitrate: u32, now: Instant) -> Self {
        Self {
            pace: pace(bitrate),
            queued: 0.0,
            at: now,
            detail: UNSEEN,
            motion: FIRST_MOTION,
            coded: [0.0; LEVELS],
            changed_at: None,
        }
    }

    pub(crate) fn set_bitrate(&mut self, bitrate: u32, now: Instant) {
        self.drain(now);
        self.pace = pace(bitrate);
    }

    /// Whether every share of the picture is sharp.
    pub(crate) fn sharp(&self) -> bool {
        self.coarsest().is_none()
    }

    /// Chooses the quantizer for a picture of which `changed` is new, or for one that codes all
    /// of it again, like a keyframe; or tells when the link will have room for it.
    pub(crate) fn picture(
        &mut self,
        changed: f64,
        whole: bool,
        pixels: f64,
        now: Instant,
    ) -> Result<Choice, Instant> {
        self.drain(now);
        let budget = self.budget();
        let room = BURST.mul_add(budget, -self.queued);
        if room < budget {
            return Err(self.after(budget - room));
        }
        let fresh = if whole { 1.0 } else { changed.clamp(0.0, 1.0) };
        let moving = !whole
            && self
                .changed_at
                .is_some_and(|at| now.saturating_duration_since(at) < MOTION);
        if fresh > 0.0 {
            self.changed_at = Some(now);
        }
        // Motion moves what the picture showed, coarse parts included, so a finer quantizer
        // sharpens all of it; new detail replaces what it covers.
        let (content, complexity, moved) = if moving {
            (Content::Motion, self.motion, 1.0)
        } else if whole {
            (Content::Detail, self.detail, 0.0)
        } else {
            (Content::Detail, self.detail.max(UNSEEN), 1.0 - fresh)
        };
        let costs = |quantizer: u8| {
            (
                pixels * complexity * fresh * scale(quantizer),
                pixels * self.detail * moved * self.refining(quantizer),
            )
        };
        let target = room.min(budget * (BURST - 1.0).mul_add(fresh, 1.0));
        let quantizer = (SHARP..=COARSEST)
            .find(|quantizer| {
                let (new, refined) = costs(*quantizer);
                new + refined <= target
            })
            .unwrap_or(COARSEST);
        let (new, refined) = costs(quantizer);
        self.code(quantizer, 1.0 - fresh, fresh);
        self.queued += new + refined;
        let (content, complexity) = if new >= refined {
            (content, complexity)
        } else {
            (Content::Detail, self.detail)
        };
        Ok(Choice {
            quantizer,
            expected: new + refined,
            content,
            complexity,
        })
    }

    /// Chooses the quantizer for a pass that sharpens what is coarser than it, or tells when the
    /// link will have room for the smallest pass; `None` once the picture is sharp.
    pub(crate) fn pass(&mut self, pixels: f64, now: Instant) -> Option<Result<Choice, Instant>> {
        let coarsest = self.coarsest()?;
        self.drain(now);
        let limit = PASS * self.budget();
        let room = limit - self.queued;
        let cost = |quantizer: u8| pixels * self.detail * self.refining(quantizer);
        let quantizer = (SHARP..coarsest)
            .find(|quantizer| cost(*quantizer) <= room)
            .unwrap_or(coarsest - 1);
        let expected = cost(quantizer);
        let step = expected.min(limit);
        if step > room {
            return Some(Err(self.after(step - room)));
        }
        if let Some(quiet) = self
            .changed_at
            .map(|at| at + QUIET)
            .filter(|quiet| expected > limit && *quiet > now)
        {
            return Some(Err(quiet));
        }
        self.code(quantizer, 1.0, 0.0);
        self.queued += expected;
        Some(Ok(Choice {
            quantizer,
            expected,
            content: Content::Detail,
            complexity: self.detail,
        }))
    }

    /// Settles what a frame really cost, and learns from it how costly the picture is. A frame
    /// that the encoder or the link dropped costs nothing.
    pub(crate) fn coded(&mut self, choice: Choice, bytes: usize) {
        let bytes = f64::from(u32::try_from(bytes).unwrap_or(u32::MAX));
        self.queued = (self.queued + bytes - choice.expected).max(0.0);
        if bytes == 0.0 || choice.expected < LEARNS_FROM {
            return;
        }
        let correction = (bytes / choice.expected)
            .clamp(MOST_CORRECTION.recip(), MOST_CORRECTION)
            .sqrt();
        let learned = (choice.complexity * correction).clamp(SIMPLEST, MOST_COMPLEX);
        match choice.content {
            Content::Detail => self.detail = learned,
            Content::Motion => self.motion = learned,
        }
    }

    fn budget(&self) -> f64 {
        self.pace / f64::from(FRAME_RATE)
    }

    fn drain(&mut self, now: Instant) {
        let carried = self.pace * now.saturating_duration_since(self.at).as_secs_f64();
        self.queued = (self.queued - carried).max(0.0);
        self.at = self.at.max(now);
    }

    /// When the link will have carried `excess` more bytes.
    fn after(&self, excess: f64) -> Instant {
        self.at + Duration::from_secs_f64((excess / self.pace).clamp(SHORTEST_WAIT, LONGEST_WAIT))
    }

    fn coarsest(&self) -> Option<u8> {
        (SHARP + 1..=COARSEST)
            .rev()
            .find(|level| self.coded[usize::from(*level)] > SPECK)
    }

    /// What coding the picture again at `quantizer` costs, per pixel of complexity.
    fn refining(&self, quantizer: u8) -> f64 {
        let target = scale(quantizer);
        (quantizer + 1..=COARSEST)
            .map(|level| self.coded[usize::from(level)] * (target - scale(level)))
            .sum()
    }

    /// Records a frame at `quantizer` that keeps `kept` of the picture and brings `fresh` of it
    /// new; it sharpens whatever it keeps that was coded coarser.
    fn code(&mut self, quantizer: u8, kept: f64, fresh: f64) {
        let at = usize::from(quantizer);
        for share in &mut self.coded {
            *share *= kept;
        }
        let sharpened: f64 = self.coded[at + 1..].iter_mut().map(std::mem::take).sum();
        self.coded[at] += sharpened + fresh;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PIXELS: f64 = 2560.0 * 1664.0;
    /// Terminal text at this size measured 149 KB at quantizer 40 in HEVC on an M4 Pro.
    const TEXT: f64 = 0.65;
    const FRAME: Duration = Duration::from_micros(16_667);

    /// What a frame really costs: the model's shape, with text's slower halving and a
    /// complexity that the first guess misses.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "frame sizes are positive and far below usize::MAX"
    )]
    fn real(coded: &[f64; LEVELS], quantizer: u8, fresh: f64, complexity: f64) -> usize {
        let scale = |quantizer: u8| (-f64::from(quantizer) / 9.5).exp2();
        let refined: f64 = (quantizer + 1..=COARSEST)
            .map(|level| coded[usize::from(level)] * (scale(quantizer) - scale(level)))
            .sum();
        (PIXELS * complexity * fresh.mul_add(scale(quantizer), (1.0 - fresh) * refined)).round()
            as usize
    }

    fn frames(rate: &Rate) -> f64 {
        rate.queued / rate.budget()
    }

    #[test]
    fn a_new_screen_shows_within_a_burst_then_sharpens_in_passes_that_typing_need_not_wait_for() {
        for complexity in [TEXT, 4.0 * TEXT] {
            let start = Instant::now();
            let mut rate = Rate::new(10_000_000, start);
            let before = rate.coded;
            let first = rate.picture(1.0, true, PIXELS, start).unwrap();
            rate.coded(first, real(&before, first.quantizer, 1.0, complexity));
            if complexity == TEXT {
                assert!(frames(&rate) <= BURST, "{}", frames(&rate));
            }
            let mut now = start;
            let mut passes = 0;
            while !rate.sharp() {
                now += FRAME;
                let before = rate.coded;
                match rate.pass(PIXELS, now) {
                    Some(Ok(choice)) => {
                        let bytes = real(&before, choice.quantizer, 0.0, complexity);
                        rate.coded(choice, bytes);
                        passes += 1;
                        let step = f64::from(u32::try_from(bytes).unwrap()) / rate.budget();
                        if passes > 3 {
                            assert!(frames(&rate) <= PASS.max(step) + 0.5, "{}", frames(&rate));
                        }
                    }
                    Some(Err(at)) => assert!(at > now),
                    None => {}
                }
                assert!(
                    now - start < Duration::from_secs(3),
                    "{complexity} never sharp"
                );
            }
            let typed = rate
                .picture(0.001, false, PIXELS, now + 10 * FRAME)
                .unwrap();
            assert_eq!(typed.quantizer, SHARP);
            assert!(typed.expected <= rate.budget());
        }
    }

    #[test]
    fn typing_on_a_coarse_screen_shows_at_once_and_sharpens_it_a_little_on_the_way() {
        let start = Instant::now();
        let mut rate = Rate::new(10_000_000, start);
        rate.code(45, 0.0, 1.0);
        let typed = rate.picture(0.001, false, PIXELS, start).unwrap();
        assert!(typed.quantizer < 45, "{}", typed.quantizer);
        assert!(typed.expected <= rate.budget());
    }

    #[test]
    fn motion_beyond_the_link_skips_pictures_rather_than_queue_them() {
        let start = Instant::now();
        let mut rate = Rate::new(2_000_000, start);
        let (mut sent, mut largest, mut shown) = (0, 0, 0);
        let mut now = start;
        for _ in 0..600 {
            now += FRAME;
            let before = rate.coded;
            if let Ok(choice) = rate.picture(1.0, false, PIXELS, now) {
                let bytes = real(&before, choice.quantizer, 1.0, 0.5);
                rate.coded(choice, bytes);
                (sent, largest, shown) = (sent + bytes, largest.max(bytes), shown + 1);
            }
        }
        let carried = rate.pace * (now - start).as_secs_f64();
        let excess = f64::from(u32::try_from(sent - largest).unwrap()) - carried;
        assert!(excess <= BURST * rate.budget(), "{excess}");
        assert!((10..600).contains(&shown), "{shown}");
    }
}
