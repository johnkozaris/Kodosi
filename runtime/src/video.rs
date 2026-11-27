//! The viewer's side of a live video: the shared browser's or the screen's.
//!
//! It knows what the app's decoder holds, which frames it can take, when to ask the stream for a
//! keyframe, and whether the app's picture is the stream's newest. The app gets the pipe's
//! messages and the frames that it can decode as they come, and answers each frame (see `video`
//! in `protocol/desktop-runtime.json`).

use std::time::Duration;

use bytes::Bytes;
pub use stream::{Codec, FrameKind, viewer::Frame};
use tokio::{sync::mpsc, time::Instant};

/// How long the viewer waits for a repair before its app rests the picture.
pub(crate) const MOMENT: Duration = Duration::from_millis(500);
/// How long the stream may say nothing before the app rests its picture. The stream says that
/// nothing changed several times as often, so a lost word does not dim a still picture.
pub(crate) const SILENCE: Duration = Duration::from_secs(1);
/// How long a viewer that waits for a keyframe, while frames keep coming, waits before it asks
/// the stream for one again. The host makes a keyframe at most once a second.
pub(crate) const ASK_AGAIN: Duration = Duration::from_secs(1);
/// The messages that may wait for the app at once. A frame waits for the app's answer before
/// the next message goes, so only JSON messages fill it.
pub(crate) const SHOWN_QUEUE: usize = 8;
/// The app answers at most one frame at a time; the rest is room for restarts.
pub(crate) const TOOK_QUEUE: usize = 4;

/// What the app of a watched video pipe gets, in pipe order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shown {
    /// A JSON message of the pipe, with its tag byte, as the stream sent it.
    Message(Bytes),
    /// A frame that the app's decoder can take, with its header, as the stream sent it.
    Frame(Bytes),
    /// True: the picture is not the stream's newest any more, because a repair takes longer than
    /// a moment or the stream said nothing for a while, so the app rests it. False: it is the
    /// newest again. A frame ends a rest too.
    Stale(bool),
}

/// What the app says about its decoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Took {
    /// The decoder took the last frame.
    Taken,
    /// The decoder cannot go on after the last frame.
    Failed,
    /// The decoder started over on its own, so it needs a keyframe.
    Restart,
}

/// The app's ends of a watched video pipe.
#[derive(Debug)]
pub struct VideoWatch {
    pub shown: mpsc::Receiver<Shown>,
    pub took: mpsc::Sender<Took>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Holds {
    /// The decoder has no picture to build on and waits for a keyframe.
    Nothing,
    /// The decoder has each frame since the last keyframe or anchor.
    Stream,
    /// The decoder missed a frame and waits for a keyframe or an anchor.
    Gap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Arrival {
    /// The decoder can take the frame.
    Hand,
    /// The decoder cannot take it; `ask` says to ask the stream for a keyframe now.
    Drop { ask: bool },
}

#[derive(Debug)]
pub(crate) struct Viewing {
    holds: Holds,
    last: Option<u32>,
    /// Since when the decoder waits for a keyframe or an anchor.
    waiting_since: Option<Instant>,
    /// The last time that someone asked for a keyframe: this viewer, or the host by itself.
    asked_at: Option<Instant>,
    /// When the stream last sent a frame or said that nothing changed, or the app last took a
    /// frame: a silence counts only while the viewer listens.
    heard_at: Option<Instant>,
    /// The app heard that its picture is not the stream's newest, and nothing made it the newest
    /// since.
    stale: bool,
}

impl Viewing {
    pub(crate) const fn new() -> Self {
        Self {
            holds: Holds::Nothing,
            last: None,
            waiting_since: None,
            asked_at: None,
            heard_at: None,
            stale: false,
        }
    }

    /// A new seat at the stream, or none for now: the host asks for a keyframe when the app
    /// watches.
    pub(crate) fn opened(&mut self, now: Instant) {
        self.last = None;
        self.wait(Holds::Nothing, now);
        self.asked_at = Some(now);
    }

    /// After a gap the host knows of the loss from the acknowledgements and repairs it by itself,
    /// so the viewer asks only when the repair is late.
    pub(crate) fn arrived(&mut self, id: u32, kind: FrameKind, now: Instant) -> Arrival {
        self.heard_at = Some(now);
        if self.holds == Holds::Stream && self.last.map(|last| last.wrapping_add(1)) != Some(id) {
            self.wait(Holds::Gap, now);
            self.asked_at = Some(now);
        }
        self.last = Some(id);
        let decodable = match kind {
            FrameKind::Key => true,
            FrameKind::Anchor => self.holds != Holds::Nothing,
            FrameKind::Delta => self.holds == Holds::Stream,
        };
        if decodable {
            return Arrival::Hand;
        }
        let ask = self.asked_at.is_none_or(|at| now >= at + ASK_AGAIN);
        if ask {
            self.asked_at = Some(now);
        }
        Arrival::Drop { ask }
    }

    pub(crate) const fn taken(&mut self, now: Instant) {
        self.holds = Holds::Stream;
        self.waiting_since = None;
        self.asked_at = None;
        self.heard_at = Some(now);
        self.stale = false;
    }

    /// The stream said that nothing changed. Returns true when the app must hear that its picture
    /// is the newest again: it rested only for a silence, and no repair is awaited.
    pub(crate) const fn unchanged(&mut self, now: Instant) -> bool {
        self.heard_at = Some(now);
        let newest = self.stale && matches!(self.holds, Holds::Stream);
        if newest {
            self.stale = false;
        }
        newest
    }

    /// The app may not have got the frame that it had when its watch broke. The host counts that
    /// frame as lost, so the viewer waits for a keyframe or an anchor too.
    pub(crate) fn missed(&mut self, now: Instant) {
        if self.holds == Holds::Stream {
            self.wait(Holds::Gap, now);
            self.asked_at = Some(now);
        }
    }

    /// The decoder failed or started over, so the viewer asks for a keyframe at once.
    pub(crate) fn failed(&mut self, now: Instant) {
        self.wait(Holds::Nothing, now);
        self.asked_at = Some(now);
    }

    /// When the app should hear that its picture is not the stream's newest: a moment into a wait
    /// for a repair, or a silence after the last that the stream said.
    pub(crate) fn stale_due(&self) -> Option<Instant> {
        let repair = self.waiting_since.map(|since| since + MOMENT);
        let silence = self.heard_at.map(|at| at + SILENCE);
        [repair, silence]
            .into_iter()
            .flatten()
            .min()
            .filter(|_| !self.stale)
    }

    pub(crate) const fn went_stale(&mut self) {
        self.stale = true;
    }

    fn wait(&mut self, holds: Holds, now: Instant) {
        self.holds = holds;
        if self.waiting_since.is_none() {
            self.waiting_since = Some(now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seated(now: Instant) -> Viewing {
        let mut viewing = Viewing::new();
        viewing.opened(now);
        assert_eq!(
            viewing.arrived(1, FrameKind::Key, now),
            Arrival::Hand,
            "a new seat starts at a keyframe"
        );
        viewing.taken(now);
        viewing
    }

    #[test]
    fn a_view_that_waits_for_a_keyframe_asks_again_while_frames_keep_coming() {
        let start = Instant::now();
        let mut viewing = seated(start);
        viewing.failed(start);
        let ms = Duration::from_millis;
        let asks = [
            (2, ms(100)),
            (3, ms(600)),
            (4, ms(1000)),
            (5, ms(1500)),
            (6, ms(2100)),
        ]
        .map(|(id, after)| viewing.arrived(id, FrameKind::Delta, start + after));
        assert_eq!(
            asks,
            [false, false, true, false, true].map(|ask| Arrival::Drop { ask }),
            "the view asks again once a second while deltas keep coming, and never gives up"
        );
        assert_eq!(
            viewing.arrived(7, FrameKind::Anchor, start + ms(2200)),
            Arrival::Drop { ask: false },
            "an anchor needs the pictures that a failed decoder lost"
        );
        assert_eq!(
            viewing.arrived(8, FrameKind::Key, start + ms(2300)),
            Arrival::Hand
        );
    }

    #[test]
    fn after_a_gap_the_view_waits_for_a_keyframe_or_an_anchor_and_asks_only_when_it_is_late() {
        let start = Instant::now();
        let ms = Duration::from_millis;
        let mut viewing = seated(start);
        assert_eq!(viewing.arrived(2, FrameKind::Delta, start), Arrival::Hand);
        viewing.taken(start);
        assert_eq!(
            viewing.arrived(4, FrameKind::Delta, start + ms(10)),
            Arrival::Drop { ask: false },
            "the host knows of the loss from the acknowledgements"
        );
        assert_eq!(
            viewing.arrived(5, FrameKind::Delta, start + ms(900)),
            Arrival::Drop { ask: false }
        );
        assert_eq!(
            viewing.arrived(6, FrameKind::Delta, start + ms(1010)),
            Arrival::Drop { ask: true },
            "a repair that does not come in time is asked for"
        );
        assert_eq!(
            viewing.arrived(7, FrameKind::Anchor, start + ms(1100)),
            Arrival::Hand,
            "an anchor builds on what the decoder held before the gap"
        );
        viewing.taken(start + ms(1100));
        assert_eq!(
            viewing.arrived(8, FrameKind::Delta, start + ms(1110)),
            Arrival::Hand
        );
    }

    #[test]
    fn the_app_hears_of_a_repair_once_and_only_when_it_takes_longer_than_a_moment() {
        let start = Instant::now();
        let mut viewing = seated(start);
        viewing.arrived(3, FrameKind::Delta, start);
        assert_eq!(viewing.stale_due(), Some(start + MOMENT));
        viewing.arrived(4, FrameKind::Delta, start + MOMENT / 2);
        viewing.failed(start + MOMENT / 2);
        assert_eq!(
            viewing.stale_due(),
            Some(start + MOMENT),
            "one wait, however it goes on"
        );
        viewing.went_stale();
        assert_eq!(viewing.stale_due(), None);
        viewing.arrived(5, FrameKind::Key, start + MOMENT * 2);
        viewing.taken(start + MOMENT * 2);
        viewing.arrived(7, FrameKind::Delta, start + MOMENT * 3);
        assert_eq!(
            viewing.stale_due(),
            Some(start + MOMENT * 4),
            "the next wait is told again"
        );
    }

    #[test]
    fn a_silent_stream_rests_the_picture_once_and_its_word_that_nothing_changed_ends_the_rest() {
        let start = Instant::now();
        let mut viewing = seated(start);
        assert_eq!(
            viewing.stale_due(),
            Some(start + SILENCE),
            "a live picture rests after a silence"
        );
        viewing.went_stale();
        assert_eq!(viewing.stale_due(), None, "the app hears it once");
        let back = start + SILENCE * 3;
        assert!(viewing.unchanged(back), "the picture is the newest again");
        assert!(!viewing.unchanged(back), "and the app heard so");
        assert_eq!(viewing.stale_due(), Some(back + SILENCE));
    }

    #[test]
    fn a_word_that_nothing_changed_ends_no_rest_while_a_repair_is_awaited() {
        let start = Instant::now();
        let mut viewing = seated(start);
        viewing.arrived(3, FrameKind::Delta, start);
        viewing.went_stale();
        assert!(!viewing.unchanged(start + MOMENT), "after a gap");

        let mut viewing = seated(start);
        viewing.went_stale();
        viewing.failed(start + SILENCE);
        assert!(
            !viewing.unchanged(start + SILENCE),
            "after a silence and then a decoder that failed"
        );
        viewing.arrived(2, FrameKind::Key, start + SILENCE * 2);
        viewing.taken(start + SILENCE * 2);
        assert_eq!(
            viewing.stale_due(),
            Some(start + SILENCE * 3),
            "the frame that repaired it ended the rest"
        );
    }

    #[test]
    fn the_time_that_the_app_takes_for_a_frame_is_no_silence() {
        let start = Instant::now();
        let mut viewing = seated(start);
        assert_eq!(viewing.arrived(2, FrameKind::Delta, start), Arrival::Hand);
        viewing.taken(start + SILENCE);
        assert_eq!(viewing.stale_due(), Some(start + SILENCE * 2));
    }

    #[test]
    fn frames_or_words_that_nothing_changed_keep_a_picture_live_for_minutes() {
        let start = Instant::now();
        let mut viewing = seated(start);
        let period = Duration::from_millis(300);
        let mut id = 1;
        for step in 1..=2000 {
            let now = start + period * step;
            assert!(
                viewing.stale_due().is_some_and(|due| due > now),
                "live after {:?}",
                period * step
            );
            // A minute of frames, then a minute in which the stream only says that nothing changed.
            if step % 400 < 200 {
                id += 1;
                assert_eq!(viewing.arrived(id, FrameKind::Delta, now), Arrival::Hand);
                viewing.taken(now);
            } else {
                assert!(!viewing.unchanged(now));
            }
        }
    }
}
