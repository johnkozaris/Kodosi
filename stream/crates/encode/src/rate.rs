//! Rate control for encoders that take a quantizer for each frame. New pictures come at the
//! stream's pace, and frames may run a few hundredths of a second ahead of the link: a new screen
//! shows coarse at once instead of sharp a second later, passes then sharpen it while the link has
//! room, and typing never waits long behind them. A frame's own share of the link is its frame
//! time, so at a slower pace each frame gets more bits and nothing waits longer.
//!
//! A model tracks the quantizer each share of the picture was last coded at. Coding a frame at a
//! finer quantizer also sharpens everything coded coarser, which the model counts as the cost
//! of reaching that quantizer.

use std::time::{Duration, Instant};

use crate::{Pace, due};

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
/// The link's time that a new picture may fill.
const BURST: Duration = Duration::from_millis(67);
/// The link's time that a pass may leave queued, which is the longest a new picture waits behind
/// one.
const PASS: Duration = Duration::from_millis(33);
/// Changes this close together are motion, which costs less than new detail because the
/// encoder predicts it from the last picture.
const MOTION: Duration = Duration::from_millis(50);
/// A pass that costs more than `PASS` of link even in its smallest step waits until the picture
/// has stayed this long, so typing rarely waits behind one.
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
    speed: f64,
    pace: Pace,
    /// When the last new picture was coded.
    pictured_at: Option<Instant>,
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

/// A floor of 8 kbit/s keeps the link's speed above zero.
fn speed(bitrate: u32) -> f64 {
    f64::from(bitrate.max(8_000)) / 8.0
}

impl Rate {
    pub(crate) fn new(bitrate: u32, pace: Pace, now: Instant) -> Self {
        Self {
            speed: speed(bitrate),
            pace,
            pictured_at: None,
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
        self.speed = speed(bitrate);
    }

    pub(crate) fn set_pace(&mut self, pace: Pace) {
        self.pace = pace;
    }

    /// Whether every share of the picture is sharp.
    pub(crate) fn sharp(&self) -> bool {
        self.coarsest().is_none()
    }

    /// Chooses the quantizer for a new picture of which `changed` is new, or which `whole` codes
    /// all of again, like a keyframe; or tells when its frame time comes or when the link will
    /// have room for it. A whole picture repairs the stream, so it does not wait for its frame
    /// time.
    pub(crate) fn picture(
        &mut self,
        changed: f64,
        whole: bool,
        pixels: f64,
        now: Instant,
    ) -> Result<Choice, Instant> {
        if !whole {
            due(self.pace, self.pictured_at, now)?;
        }
        let choice = self.choose(changed, whole, pixels, now)?;
        self.pictured_at = Some(now);
        Ok(choice)
    }

    /// Chooses the quantizer for the last picture again, all of it new, to repair the stream;
    /// or tells when the link will have room for it.
    pub(crate) fn repair(
        &mut self,
        whole: bool,
        pixels: f64,
        now: Instant,
    ) -> Result<Choice, Instant> {
        self.choose(1.0, whole, pixels, now)
    }

    fn choose(
        &mut self,
        changed: f64,
        whole: bool,
        pixels: f64,
        now: Instant,
    ) -> Result<Choice, Instant> {
        self.drain(now);
        let share = self.carried(self.pace.frame_time());
        let burst = self.carried(BURST);
        let room = burst - self.queued;
        if room < share {
            return Err(self.after(share - room));
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
        let target = room.min((burst - share).mul_add(fresh, share));
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
        let limit = self.carried(PASS);
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

    /// The bytes that the link carries in `time`.
    fn carried(&self, time: Duration) -> f64 {
        self.speed * time.as_secs_f64()
    }

    fn drain(&mut self, now: Instant) {
        let carried = self.carried(now.saturating_duration_since(self.at));
        self.queued = (self.queued - carried).max(0.0);
        self.at = self.at.max(now);
    }

    /// When the link will have carried `excess` more bytes.
    fn after(&self, excess: f64) -> Instant {
        self.at + Duration::from_secs_f64((excess / self.speed).clamp(SHORTEST_WAIT, LONGEST_WAIT))
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

    /// The link's time that the bytes sent ahead of it take.
    fn queued(rate: &Rate) -> Duration {
        Duration::from_secs_f64(rate.queued / rate.speed)
    }

    fn share(rate: &Rate) -> f64 {
        rate.carried(rate.pace.frame_time())
    }

    #[test]
    fn a_new_screen_shows_within_a_burst_then_sharpens_in_passes_that_typing_need_not_wait_for() {
        for pace in [Pace::Acting, Pace::Watching] {
            for complexity in [TEXT, 4.0 * TEXT] {
                let start = Instant::now();
                let mut rate = Rate::new(10_000_000, pace, start);
                let before = rate.coded;
                let first = rate.picture(1.0, true, PIXELS, start).unwrap();
                rate.coded(first, real(&before, first.quantizer, 1.0, complexity));
                if complexity == TEXT {
                    assert!(queued(&rate) <= BURST, "{pace:?}: {:?}", queued(&rate));
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
                            let step = Duration::from_secs_f64(
                                f64::from(u32::try_from(bytes).unwrap()) / rate.speed,
                            );
                            if passes > 3 {
                                assert!(
                                    queued(&rate) <= PASS.max(step) + FRAME / 2,
                                    "{pace:?}: {:?}",
                                    queued(&rate)
                                );
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
                assert!(typed.expected <= share(&rate));
            }
        }
    }

    #[test]
    fn typing_on_a_coarse_screen_shows_at_once_and_sharpens_it_a_little_on_the_way() {
        let start = Instant::now();
        let mut rate = Rate::new(10_000_000, Pace::Acting, start);
        rate.code(45, 0.0, 1.0);
        let typed = rate.picture(0.001, false, PIXELS, start).unwrap();
        assert!(typed.quantizer < 45, "{}", typed.quantizer);
        assert!(typed.expected <= share(&rate));
    }

    #[test]
    fn a_small_change_while_people_watch_may_cost_twice_what_it_may_while_a_viewer_acts() {
        let typed = |pace| {
            let start = Instant::now();
            let mut rate = Rate::new(10_000_000, pace, start);
            (
                rate.picture(0.031, false, PIXELS, start).unwrap(),
                share(&rate),
            )
        };
        let (acting, share) = typed(Pace::Acting);
        let (watching, _) = typed(Pace::Watching);
        assert!(acting.quantizer > SHARP, "{}", acting.quantizer);
        assert_eq!(watching.quantizer, SHARP);
        assert!(
            (share..=2.0 * share).contains(&watching.expected),
            "{} of {share}",
            watching.expected
        );
    }

    #[test]
    fn a_repair_with_a_new_picture_comes_at_once_and_the_next_picture_waits_its_frame_time_after_it()
     {
        let start = Instant::now();
        let mut rate = Rate::new(100_000_000, Pace::Watching, start);
        let first = rate.picture(0.01, false, PIXELS, start).unwrap();
        rate.coded(first, 0);
        let soon = start + FRAME;
        assert!(rate.picture(0.01, false, PIXELS, soon).is_err());
        let key = rate.picture(0.01, true, PIXELS, soon).unwrap();
        rate.coded(key, 0);
        let due = rate
            .picture(0.01, false, PIXELS, start + FRAME * 2)
            .unwrap_err();
        assert!(
            due >= soon + Pace::Watching.frame_time(),
            "{:?}",
            due - soon
        );
    }

    #[test]
    fn motion_beyond_the_link_skips_pictures_rather_than_queue_them() {
        let start = Instant::now();
        let mut rate = Rate::new(2_000_000, Pace::Acting, start);
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
        let carried = rate.carried(now - start);
        let excess = f64::from(u32::try_from(sent - largest).unwrap()) - carried;
        assert!(excess <= rate.carried(BURST), "{excess}");
        assert!((10..600).contains(&shown), "{shown}");
    }

    /// Offers each picture when `offers` says, as a capture does, and leaves it waiting in the
    /// slot until the controller takes it or a newer one replaces it, as an encoder's worker
    /// does. Returns which pictures the controller took, and when.
    fn worker(rate: &mut Rate, offers: &[Instant]) -> Vec<(usize, Instant)> {
        let mut taken = Vec::new();
        let mut waiting: Option<(usize, Instant)> = None;
        let mut next = 0;
        loop {
            let offer = offers.get(next).copied();
            let (picture, now) = match waiting {
                Some((picture, due)) if offer.is_none_or(|at| due < at) => (picture, due),
                _ => {
                    let Some(at) = offer else {
                        return taken;
                    };
                    next += 1;
                    (next - 1, at)
                }
            };
            waiting = match rate.picture(0.01, false, PIXELS, now) {
                Ok(choice) => {
                    rate.coded(choice, 0);
                    taken.push((picture, now));
                    None
                }
                Err(due) => Some((picture, due)),
            };
        }
    }

    #[test]
    fn while_people_watch_every_second_picture_of_a_capture_passes_as_it_comes_and_the_last_one_after_it()
     {
        let start = Instant::now();
        let offers: Vec<Instant> = (0..60_u64)
            .map(|index| {
                let late = [12_000, 10_000, 8_000, 10_000][usize::try_from(index % 4).unwrap()];
                start + Duration::from_micros(late + 16_667 * index)
            })
            .collect();

        let mut rate = Rate::new(100_000_000, Pace::Acting, start);
        let all: Vec<(usize, Instant)> = offers.iter().copied().enumerate().collect();
        assert_eq!(worker(&mut rate, &offers), all);

        let mut rate = Rate::new(100_000_000, Pace::Watching, start);
        let taken = worker(&mut rate, &offers);
        let (last, shown) = *taken.last().unwrap();
        let second: Vec<(usize, Instant)> = all.iter().copied().step_by(2).collect();
        assert_eq!(taken[..taken.len() - 1], second);
        assert_eq!(last, offers.len() - 1);
        let before = taken[taken.len() - 2].1;
        assert!(
            shown >= before + Pace::Watching.frame_time(),
            "{:?}",
            shown - before
        );
        assert!(
            shown < offers[last] + FRAME * 2,
            "{:?}",
            shown - offers[last]
        );
    }
}
