use std::{
    collections::{HashMap, HashSet, VecDeque},
    time::{Duration, Instant},
};

use crate::link::FrameKind;

/// Bytes on their way to one viewer before it counts as behind.
const WAITING_MOST: usize = 2 * 1024 * 1024;
/// A viewer that is behind can go on when this much still waits.
const WAITING_DRAINED: usize = 256 * 1024;
/// A frame is late when it comes this much later than the link's own round trip and the
/// stream's bitrate allow.
const LATE: Duration = Duration::from_millis(250);
/// A viewer that stays late this long cannot take the stream. A link that stalls for a moment
/// catches up sooner.
const LATE_FOR: Duration = Duration::from_millis(500);
/// Frames for one viewer that its pipe has not taken.
const QUEUED_MOST: usize = 3;
const KEYFRAME_FLOOR: Duration = Duration::from_secs(1);
const ANCHOR_FLOOR: Duration = Duration::from_millis(250);
/// A viewer that falls behind again within this time after it went on waits longer for each next
/// key frame while another viewer takes the stream: one, three, then seven seconds more than the
/// floor.
const STEADY: Duration = Duration::from_secs(10);
const MOST_FALLS: u32 = 3;
/// A viewer that takes no frame for this long is gone.
const HOPELESS: Duration = Duration::from_secs(15);
/// A frame that comes this much later than the link's own round trip and the stream's bitrate
/// allow shows that the link is slower than the stream.
const QUEUE_LIMIT: Duration = Duration::from_millis(40);
const STEER_EVERY: Duration = Duration::from_millis(250);
const RISE_AFTER: Duration = Duration::from_secs(2);
const ROUND_WINDOW: Duration = Duration::from_secs(10);
/// What a link that is slower than the stream carried in this time is what it can take.
const CARRIED_FOR: Duration = Duration::from_secs(1);
const FIRST_BITRATE: u32 = 8_000_000;
const LOWEST_BITRATE: u32 = 250_000;
const HIGHEST_BITRATE: u32 = 40_000_000;

/// What a viewer's decoder holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Holds {
    /// Nothing that a frame after a key frame needs, as a new decoder or one that broke.
    Nothing,
    /// Each frame since its key frame.
    Stream,
    /// The marks that every viewer held before it missed a frame, but not the frames since.
    Gap,
}

impl Holds {
    /// A decoder other than `VideoToolbox`'s can lose its references some frames after an
    /// anchor, so a viewer that takes no anchor holds nothing once it misses a frame.
    const fn after_a_miss(anchors: bool) -> Self {
        if anchors { Self::Gap } else { Self::Nothing }
    }
}

struct Sent {
    id: u32,
    bytes: usize,
    at: Instant,
    kind: FrameKind,
    /// When a link at the stream's bitrate would have brought the frame, after those before it.
    due: Instant,
    /// The bytes that the viewer had taken when the frame went, and its seat's `heard_at` then.
    taken: u64,
    heard_at: Instant,
}

struct Seat {
    holds: Holds,
    anchors: bool,
    /// Since when the viewer takes each frame.
    going_since: Option<Instant>,
    /// The quick falls in a row, and the time before which the viewer gets no key frame.
    falls: u32,
    key_after: Option<Instant>,
    sent: VecDeque<Sent>,
    waiting: usize,
    /// When a link at the stream's bitrate would have brought the last frame that went.
    due_at: Instant,
    taken: u64,
    /// What the link carried to the viewer while it was slower than the stream, in bits per
    /// second.
    carried: VecDeque<(Instant, u64)>,
    late_since: Option<Instant>,
    /// When the viewer last took a frame, or when a frame went to it while none waited.
    heard_at: Instant,
    /// How late the newest frame came that went after the last cut, until the next steer.
    queue: Option<Duration>,
    least_round: Option<Duration>,
    window_least: Option<Duration>,
    window_at: Instant,
}

impl Seat {
    fn new(anchors: bool, now: Instant) -> Self {
        Self {
            holds: Holds::Nothing,
            anchors,
            going_since: None,
            falls: 0,
            key_after: None,
            sent: VecDeque::new(),
            waiting: 0,
            due_at: now,
            taken: 0,
            carried: VecDeque::new(),
            late_since: None,
            heard_at: now,
            queue: None,
            least_round: None,
            window_least: None,
            window_at: now,
        }
    }

    fn lateness(&self, sent: &Sent, now: Instant) -> Duration {
        now.saturating_duration_since(sent.due)
            .saturating_sub(self.least_round.unwrap_or_default())
    }

    fn late(&self, now: Instant) -> bool {
        self.sent
            .front()
            .is_some_and(|oldest| self.lateness(oldest, now) > LATE)
    }

    fn behind(&mut self, queued: usize, now: Instant) -> bool {
        let late = self.late(now);
        if !late {
            self.late_since = None;
        }
        self.waiting > WAITING_MOST
            || queued >= QUEUED_MOST
            || late && now.duration_since(*self.late_since.get_or_insert(now)) >= LATE_FOR
    }

    fn caught_up(&self, queued: usize, now: Instant) -> bool {
        self.waiting <= WAITING_DRAINED && queued < QUEUED_MOST && !self.late(now)
    }

    /// A viewer whose link cannot take the stream would otherwise ask for a key frame each
    /// second, and each one goes to every viewer.
    fn fall(&mut self, now: Instant) {
        self.holds = Holds::after_a_miss(self.anchors);
        self.key_after = Some(now + KEYFRAME_FLOOR * (2_u32.pow(self.falls) - 1));
        let quick = self
            .going_since
            .is_some_and(|since| now.duration_since(since) < STEADY);
        self.falls = if quick {
            (self.falls + 1).min(MOST_FALLS)
        } else {
            0
        };
        self.going_since = None;
    }

    fn wants_key(&self, anchors: bool, costs_others: bool, queued: usize, now: Instant) -> bool {
        let waits = self.holds == Holds::Nothing || (!anchors && self.holds == Holds::Gap);
        let backs_off = costs_others && self.key_after.is_some_and(|after| now < after);
        waits && !backs_off && self.caught_up(queued, now)
    }

    fn send(&mut self, id: u32, kind: FrameKind, bytes: usize, bitrate: u32, now: Instant) {
        if self.sent.is_empty() {
            self.heard_at = now;
        }
        self.due_at = self.due_at.max(now) + crossing(bytes, bitrate);
        self.waiting += bytes;
        self.sent.push_back(Sent {
            id,
            bytes,
            at: now,
            kind,
            due: self.due_at,
            taken: self.taken,
            heard_at: self.heard_at,
        });
    }

    /// Frame ids wrap, so a frame is older than the ack when it lies less than half the range
    /// before it. Returns the newest frame that the ack took.
    fn take(&mut self, frame: u32, now: Instant) -> Option<Sent> {
        let mut newest = None;
        while let Some(sent) = self
            .sent
            .pop_front_if(|sent| frame.wrapping_sub(sent.id) < u32::MAX / 2)
        {
            self.waiting -= sent.bytes;
            self.taken += u64::try_from(sent.bytes).unwrap_or(u64::MAX);
            self.heard_at = now;
            newest = Some(sent);
        }
        newest
    }

    fn measure(&mut self, sent: &Sent, steers: bool, now: Instant) {
        // A key frame or an anchor takes longer to cross than the link's round trip.
        if sent.kind == FrameKind::Delta {
            let round = now.duration_since(sent.at);
            self.window_least = Some(self.window_least.map_or(round, |least| least.min(round)));
            self.least_round = Some(self.least_round.map_or(round, |least| least.min(round)));
            if now.duration_since(self.window_at) >= ROUND_WINDOW {
                self.least_round = self.window_least.take();
                self.window_at = now;
            }
        }
        let lateness = self.lateness(sent, now);
        if steers {
            self.queue = Some(lateness);
        }
        // A link that keeps up with the stream carries only what the stream gives it.
        if lateness > QUEUE_LIMIT {
            let micros = now.duration_since(sent.heard_at).as_micros();
            let bits = u128::from(self.taken - sent.taken) * 8;
            if let Some(rate) = (bits * 1_000_000).checked_div(micros) {
                self.carried
                    .push_back((now, u64::try_from(rate).unwrap_or(u64::MAX)));
            }
        }
        self.forget_carried(now);
    }

    fn forget_carried(&mut self, now: Instant) {
        while self
            .carried
            .front()
            .is_some_and(|(at, _)| now.duration_since(*at) > CARRIED_FOR)
        {
            self.carried.pop_front();
        }
    }

    /// The most that the link carried while it was slower than the stream.
    fn carried(&mut self, now: Instant) -> Option<u64> {
        self.forget_carried(now);
        self.carried.iter().map(|(_, rate)| *rate).max()
    }
}

/// The time that a frame takes at the stream's bitrate.
fn crossing(bytes: usize, bitrate: u32) -> Duration {
    let bits = u64::try_from(bytes).unwrap_or(u64::MAX).saturating_mul(8);
    Duration::from_micros(bits.saturating_mul(1_000_000) / u64::from(bitrate.max(1)))
}

/// A frame's mark, until each viewer that took the frame said so.
struct MarkedFrame {
    id: u32,
    mark: u64,
    waiting: HashSet<String>,
    taken: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ask {
    Keyframe,
    Recover,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Delivery {
    pub to: Vec<String>,
    pub ask: Option<Ask>,
}

/// One encoder serves all viewers of a window. Viewers report each frame, and a viewer that
/// falls behind misses frames until a key frame or an anchor, so it never sees an old page. The
/// bitrate follows the link of the viewer in control, so one that only looks never lowers it for
/// the others.
pub(crate) struct Flow {
    seats: HashMap<String, Seat>,
    marks: VecDeque<MarkedFrame>,
    held: Vec<u64>,
    /// The encoder marks frames, so it can make anchors.
    anchors: bool,
    bitrate: u32,
    steered_at: Option<Instant>,
    cut_at: Option<Instant>,
    keyframe_at: Option<Instant>,
    recovered_at: Option<Instant>,
}

impl Flow {
    pub(crate) fn new() -> Self {
        Self {
            seats: HashMap::new(),
            marks: VecDeque::new(),
            held: Vec::new(),
            anchors: false,
            bitrate: FIRST_BITRATE,
            steered_at: None,
            cut_at: None,
            keyframe_at: None,
            recovered_at: None,
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.seats.is_empty()
    }

    pub(crate) fn seat(&mut self, viewer: &str, anchors: bool, now: Instant) {
        self.forget(viewer);
        if self.seats.is_empty() {
            self.bitrate = FIRST_BITRATE;
            self.cut_at = None;
        }
        self.seats
            .insert(viewer.to_owned(), Seat::new(anchors, now));
    }

    pub(crate) const fn bitrate(&self) -> u32 {
        self.bitrate
    }

    pub(crate) fn leave(&mut self, viewer: &str) {
        self.seats.remove(viewer);
        self.forget(viewer);
    }

    /// Takes how many frames wait in each viewer's pipe, and returns the viewers that take the
    /// frame.
    pub(crate) fn frame(
        &mut self,
        id: u32,
        kind: FrameKind,
        mark: Option<u64>,
        bytes: usize,
        now: Instant,
        queued: impl Fn(&str) -> usize,
    ) -> Delivery {
        let mut delivery = Delivery::default();
        if kind == FrameKind::Key {
            self.marks.clear();
            self.keyframe_at = Some(now);
        }
        for (viewer, seat) in &mut self.seats {
            let behind = seat.behind(queued(viewer), now);
            let takes = match kind {
                FrameKind::Key => seat.waiting <= WAITING_MOST,
                FrameKind::Anchor => seat.holds != Holds::Nothing && !behind,
                FrameKind::Delta => seat.holds == Holds::Stream && !behind,
            };
            if takes {
                if seat.holds != Holds::Stream {
                    seat.going_since = Some(now);
                }
                seat.holds = Holds::Stream;
                seat.send(id, kind, bytes, self.bitrate, now);
                delivery.to.push(viewer.clone());
            } else if kind == FrameKind::Key {
                seat.holds = Holds::Nothing;
            } else if seat.holds == Holds::Stream {
                seat.fall(now);
            }
        }
        if let Some(mark) = mark {
            self.anchors = true;
            let missed = self.seats.values().any(|seat| seat.holds == Holds::Gap);
            if !missed && !delivery.to.is_empty() {
                self.marks.push_back(MarkedFrame {
                    id,
                    mark,
                    waiting: delivery.to.iter().cloned().collect(),
                    taken: false,
                });
            }
        }
        delivery.ask = self.ask(now, queued);
        delivery
    }

    /// A frame that never reached a viewer: the frames after it need it, and the viewer does
    /// not hold its mark.
    pub(crate) fn lost(&mut self, viewer: &str, id: u32, now: Instant) {
        let Some(seat) = self.seats.get_mut(viewer) else {
            return;
        };
        if let Some(sent) = seat.sent.pop_back_if(|sent| sent.id == id) {
            seat.waiting -= sent.bytes;
            seat.fall(now);
            if sent.kind == FrameKind::Key {
                seat.holds = Holds::Nothing;
            }
        }
        self.marks
            .retain(|marked| marked.id != id || !marked.waiting.contains(viewer));
    }

    /// A viewer whose decoder starts again holds nothing, so it gets no frame before a key frame.
    pub(crate) fn reset(
        &mut self,
        viewer: &str,
        now: Instant,
        queued: impl Fn(&str) -> usize,
    ) -> Option<Ask> {
        self.seats.get_mut(viewer)?.holds = Holds::Nothing;
        self.forget(viewer);
        self.ask(now, queued)
    }

    /// The window asked for a key frame itself.
    pub(crate) fn keyframe_asked(&mut self, now: Instant) {
        self.keyframe_at = Some(now);
    }

    /// What a viewer that waits needs while no frame comes, as on a still page.
    pub(crate) fn due(&mut self, now: Instant, queued: impl Fn(&str) -> usize) -> Option<Ask> {
        self.ask(now, queued)
    }

    /// A viewer that holds nothing gets a key frame once it takes frames again, and one that
    /// missed frames an anchor. Each key frame costs every viewer.
    fn ask(&mut self, now: Instant, queued: impl Fn(&str) -> usize) -> Option<Ask> {
        let anchors = self.anchors;
        let costs_others = self.seats.values().any(|seat| seat.holds == Holds::Stream);
        let wants_key = self
            .seats
            .iter()
            .any(|(viewer, seat)| seat.wants_key(anchors, costs_others, queued(viewer), now));
        if wants_key {
            let allowed = self
                .keyframe_at
                .is_none_or(|at| now.duration_since(at) >= KEYFRAME_FLOOR);
            if allowed {
                self.keyframe_at = Some(now);
            }
            return allowed.then_some(Ask::Keyframe);
        }
        let wants_anchor = anchors
            && self.seats.iter().any(|(viewer, seat)| {
                seat.holds == Holds::Gap && seat.caught_up(queued(viewer), now)
            });
        let allowed = wants_anchor
            && self
                .recovered_at
                .is_none_or(|at| now.duration_since(at) >= ANCHOR_FLOOR);
        if allowed {
            self.recovered_at = Some(now);
        }
        allowed.then_some(Ask::Recover)
    }

    pub(crate) fn ack(&mut self, viewer: &str, frame: u32, now: Instant) {
        let Some(seat) = self.seats.get_mut(viewer) else {
            return;
        };
        if let Some(sent) = seat.take(frame, now) {
            let steers = self.cut_at.is_none_or(|cut| sent.at > cut);
            seat.measure(&sent, steers, now);
        }
        for marked in &mut self.marks {
            if frame.wrapping_sub(marked.id) < u32::MAX / 2 && marked.waiting.remove(viewer) {
                marked.taken = true;
            }
        }
        self.settle();
    }

    /// The marks that every viewer holds since the last call.
    pub(crate) fn held(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.held)
    }

    /// The viewers that took no frame for too long, which leave.
    pub(crate) fn hopeless(&mut self, now: Instant) -> Vec<String> {
        let gone: Vec<String> = self
            .seats
            .iter()
            .filter(|(_, seat)| {
                !seat.sent.is_empty() && now.duration_since(seat.heard_at) >= HOPELESS
            })
            .map(|(viewer, _)| viewer.clone())
            .collect();
        for viewer in &gone {
            self.leave(viewer);
        }
        gone
    }

    fn forget(&mut self, viewer: &str) {
        for marked in &mut self.marks {
            marked.waiting.remove(viewer);
        }
        self.settle();
    }

    fn settle(&mut self) {
        let held = &mut self.held;
        self.marks.retain(|marked| {
            if !marked.waiting.is_empty() {
                return true;
            }
            if marked.taken {
                held.push(marked.mark);
            }
            false
        });
    }

    /// The bitrate follows the viewer in control, or every viewer while none is, and stays within
    /// what its link carried while it was slower than the stream. A cut shows only in the frames
    /// that go after it.
    pub(crate) fn steer(&mut self, control: Option<&str>, now: Instant) -> Option<u32> {
        if self
            .steered_at
            .is_some_and(|at| now.duration_since(at) < STEER_EVERY)
        {
            return None;
        }
        self.steered_at = Some(now);
        let followed = control.filter(|viewer| self.seats.contains_key(*viewer));
        let mut queue = None;
        let mut carried: Option<u64> = None;
        for (viewer, seat) in &mut self.seats {
            let fresh = seat.queue.take();
            if followed.is_some_and(|followed| viewer.as_str() != followed) {
                continue;
            }
            if seat.holds == Holds::Stream {
                queue = queue.max(fresh);
            }
            if let Some(rate) = seat.carried(now) {
                carried = Some(carried.map_or(rate, |least| least.min(rate)));
            }
        }
        let mut bitrate = if queue.is_some_and(|queue| queue > QUEUE_LIMIT) {
            self.bitrate / 5 * 4
        } else if queue.is_some_and(|queue| queue < QUEUE_LIMIT / 2)
            && self
                .cut_at
                .is_none_or(|at| now.duration_since(at) >= RISE_AFTER)
        {
            self.bitrate.saturating_add(self.bitrate / 16)
        } else {
            self.bitrate
        };
        if let Some(carried) = carried {
            bitrate = bitrate.min(u32::try_from(carried / 20 * 17).unwrap_or(u32::MAX));
        }
        let bitrate = bitrate.clamp(LOWEST_BITRATE, HIGHEST_BITRATE);
        if bitrate < self.bitrate {
            self.cut_at = Some(now);
        }
        (bitrate != self.bitrate).then(|| {
            self.bitrate = bitrate;
            bitrate
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LARGE: usize = WAITING_MOST + 1;

    fn flow_with(viewers: &[&str], now: Instant) -> Flow {
        let mut flow = Flow::new();
        for viewer in viewers {
            flow.seat(viewer, true, now);
        }
        flow
    }

    fn none(_: &str) -> usize {
        0
    }

    fn after(now: Instant, millis: u64) -> Instant {
        now + Duration::from_millis(millis)
    }

    fn send(flow: &mut Flow, id: u32, kind: FrameKind, now: Instant) -> Delivery {
        let mut delivery = flow.frame(id, kind, None, 100, now, none);
        delivery.to.sort();
        delivery
    }

    /// A viewer's link, which brings each frame at its rate in bits per second, one after
    /// another, and the ack after the round trip.
    struct Link {
        viewer: &'static str,
        rate: u64,
        round: Duration,
    }

    /// Sends a frame of `bytes` every `every` from `start`, takes the acks that come meanwhile,
    /// and returns those still on their way.
    fn stream(
        flow: &mut Flow,
        links: &[Link],
        ids: std::ops::RangeInclusive<u32>,
        bytes: usize,
        every: Duration,
        start: Instant,
    ) -> Vec<(Instant, &'static str, u32)> {
        let mut free_at = vec![start; links.len()];
        let mut acks = Vec::new();
        for (frame, id) in ids.enumerate() {
            let at = start + every * u32::try_from(frame).unwrap();
            ack_until(flow, &mut acks, Some(at));
            let delivery = flow.frame(id, FrameKind::Delta, None, bytes, at, none);
            for (link, free_at) in links.iter().zip(&mut free_at) {
                if delivery.to.iter().any(|viewer| viewer == link.viewer) {
                    let bits = u64::try_from(bytes).unwrap() * 8;
                    *free_at =
                        (*free_at).max(at) + Duration::from_micros(bits * 1_000_000 / link.rate);
                    acks.push((*free_at + link.round, link.viewer, id));
                }
            }
        }
        acks
    }

    fn ack_until(
        flow: &mut Flow,
        acks: &mut Vec<(Instant, &'static str, u32)>,
        until: Option<Instant>,
    ) -> Option<Instant> {
        acks.sort_by_key(|(at, ..)| *at);
        let due = acks
            .iter()
            .take_while(|(at, ..)| until.is_none_or(|until| *at <= until))
            .count();
        let mut last = None;
        for (at, viewer, id) in acks.drain(..due) {
            flow.ack(viewer, id, at);
            last = Some(at);
        }
        last
    }

    /// The slow viewer takes frames 1 to 4 and then stays late.
    fn slow_and_fast(mark: Option<u64>, anchors: bool) -> (Flow, Instant) {
        let now = Instant::now();
        let mut flow = Flow::new();
        flow.seat("slow", anchors, now);
        flow.seat("fast", true, now);
        flow.frame(1, FrameKind::Key, mark, 100, now, none);
        flow.frame(2, FrameKind::Delta, None, 100, after(now, 10), none);
        for viewer in ["slow", "fast"] {
            flow.ack(viewer, 2, after(now, 10));
        }
        for (id, at) in [(3, 100), (4, 400), (5, 899)] {
            assert_eq!(
                send(&mut flow, id, FrameKind::Delta, after(now, at)).to,
                ["fast", "slow"]
            );
            flow.ack("fast", id, after(now, at));
        }
        let late = send(&mut flow, 6, FrameKind::Delta, after(now, 900));
        assert_eq!(late.to, ["fast"]);
        assert_eq!(late.ask, None);
        flow.ack("fast", 6, after(now, 900));
        flow.ack("slow", 5, after(now, 950));
        (flow, now)
    }

    #[test]
    fn a_new_viewer_starts_at_a_key_frame() {
        let now = Instant::now();
        let mut flow = flow_with(&["a"], now);
        let waiting = send(&mut flow, 1, FrameKind::Delta, now);
        assert_eq!(waiting.to, Vec::<String>::new());
        assert_eq!(waiting.ask, Some(Ask::Keyframe));
        assert_eq!(
            send(&mut flow, 2, FrameKind::Anchor, now).to,
            Vec::<String>::new()
        );
        assert_eq!(send(&mut flow, 3, FrameKind::Key, now).to, ["a"]);
        assert_eq!(send(&mut flow, 4, FrameKind::Delta, now).to, ["a"]);
    }

    #[test]
    fn a_late_viewer_misses_frames_and_goes_on_at_an_anchor_while_the_other_takes_each() {
        let (mut flow, now) = slow_and_fast(Some(1), true);
        assert_eq!(flow.due(after(now, 950), none), Some(Ask::Recover));
        assert_eq!(flow.due(after(now, 960), none), None);
        assert_eq!(
            send(&mut flow, 7, FrameKind::Delta, after(now, 960)).to,
            ["fast"]
        );
        assert_eq!(
            send(&mut flow, 8, FrameKind::Anchor, after(now, 970)).to,
            ["fast", "slow"]
        );
        assert_eq!(
            send(&mut flow, 9, FrameKind::Delta, after(now, 980)).to,
            ["fast", "slow"]
        );
    }

    #[test]
    fn a_late_viewer_that_takes_no_anchor_goes_on_at_a_key_frame_and_holds_up_no_mark() {
        let (mut flow, now) = slow_and_fast(Some(1), false);
        flow.frame(7, FrameKind::Delta, Some(70), 100, after(now, 960), none);
        flow.ack("fast", 7, after(now, 960));
        assert_eq!(flow.held(), [1, 70]);
        assert_eq!(
            send(&mut flow, 8, FrameKind::Anchor, after(now, 970)).to,
            ["fast"]
        );
        assert_eq!(flow.due(after(now, 999), none), None);
        assert_eq!(flow.due(after(now, 1000), none), Some(Ask::Keyframe));
    }

    #[test]
    fn a_viewer_that_keeps_falling_behind_waits_longer_for_each_key_frame() {
        let now = Instant::now();
        let mut flow = Flow::new();
        flow.seat("slow", false, now);
        flow.seat("fast", true, now);
        let full = |viewer: &str| if viewer == "slow" { QUEUED_MOST } else { 0 };
        let mut key_at = 0;
        for (id, extra) in [(1, 0), (3, 1000), (5, 3000)] {
            send(&mut flow, id, FrameKind::Key, after(now, key_at));
            for viewer in ["slow", "fast"] {
                flow.ack(viewer, id, after(now, key_at + 10));
            }
            let behind_at = key_at + 100;
            flow.frame(
                id + 1,
                FrameKind::Delta,
                None,
                100,
                after(now, behind_at),
                full,
            );
            flow.ack("fast", id + 1, after(now, behind_at + 10));
            let due = (behind_at + extra).max(key_at + 1000);
            assert_eq!(flow.due(after(now, due - 1), none), None);
            assert_eq!(flow.due(after(now, due), none), Some(Ask::Keyframe));
            key_at = due + 100;
        }
    }

    #[test]
    fn a_viewer_alone_that_keeps_falling_behind_gets_each_key_frame_after_the_floor() {
        let now = Instant::now();
        let mut flow = flow_with(&["slow"], now);
        let full = |_: &str| QUEUED_MOST;
        let mut key_at = 0;
        for id in [1, 3, 5] {
            send(&mut flow, id, FrameKind::Key, after(now, key_at));
            flow.ack("slow", id, after(now, key_at + 10));
            flow.frame(
                id + 1,
                FrameKind::Delta,
                None,
                100,
                after(now, key_at + 100),
                full,
            );
            let due = key_at + 1000;
            assert_eq!(flow.due(after(now, due - 1), none), None);
            assert_eq!(flow.due(after(now, due), none), Some(Ask::Keyframe));
            key_at = due + 100;
        }
    }

    #[test]
    fn a_late_viewer_goes_on_only_once_its_pipe_took_the_frames_that_wait() {
        let (mut flow, now) = slow_and_fast(Some(1), true);
        let full = |viewer: &str| if viewer == "slow" { QUEUED_MOST } else { 0 };
        assert_eq!(flow.due(after(now, 950), full), None);
        assert_eq!(flow.due(after(now, 960), none), Some(Ask::Recover));
    }

    #[test]
    fn without_marks_a_late_viewer_goes_on_at_a_key_frame_after_the_floor() {
        let (mut flow, now) = slow_and_fast(None, true);
        assert_eq!(flow.due(after(now, 950), none), None);
        assert_eq!(flow.due(after(now, 1000), none), Some(Ask::Keyframe));
        assert_eq!(
            send(&mut flow, 7, FrameKind::Key, after(now, 1010)).to,
            ["fast", "slow"]
        );
        assert_eq!(
            send(&mut flow, 8, FrameKind::Delta, after(now, 1020)).to,
            ["fast", "slow"]
        );
    }

    #[test]
    fn the_frames_behind_a_key_frame_are_late_only_once_it_crosses_slower_than_the_bitrate() {
        let now = Instant::now();
        let mut flow = flow_with(&["a"], now);
        let half_a_second = usize::try_from(FIRST_BITRATE / 16).unwrap();
        flow.frame(1, FrameKind::Key, None, half_a_second, now, none);
        for id in 2..=40 {
            let at = after(now, u64::from(id) * 16);
            assert_eq!(send(&mut flow, id, FrameKind::Delta, at).to, ["a"]);
        }
        flow.ack("a", 40, after(now, 700));
        flow.frame(
            41,
            FrameKind::Key,
            None,
            half_a_second,
            after(now, 700),
            none,
        );
        for (id, at) in (42..).zip((800..=2000).step_by(100)) {
            assert_eq!(
                send(&mut flow, id, FrameKind::Delta, after(now, at)).to,
                ["a"]
            );
        }
        assert_eq!(
            send(&mut flow, 60, FrameKind::Delta, after(now, 2100)).to,
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_viewer_whose_pipe_takes_no_frames_misses_them() {
        let now = Instant::now();
        let mut flow = flow_with(&["a", "b"], now);
        send(&mut flow, 1, FrameKind::Key, now);
        let full = |viewer: &str| if viewer == "a" { QUEUED_MOST } else { 0 };
        assert_eq!(
            flow.frame(2, FrameKind::Delta, None, 100, now, full).to,
            ["b"]
        );
        assert_eq!(send(&mut flow, 3, FrameKind::Delta, now).to, ["b"]);
    }

    #[test]
    fn a_mark_is_held_once_each_viewer_that_took_its_frame_said_so_and_never_after_a_miss() {
        let now = Instant::now();
        let mut flow = flow_with(&["a", "b"], now);
        send(&mut flow, 1, FrameKind::Key, now);
        flow.frame(2, FrameKind::Delta, Some(20), 100, now, none);
        flow.ack("a", 2, now);
        assert_eq!(flow.held(), Vec::<u64>::new());
        flow.ack("b", 2, now);
        assert_eq!(flow.held(), [20]);
        let full = |viewer: &str| if viewer == "b" { QUEUED_MOST } else { 0 };
        flow.frame(3, FrameKind::Delta, Some(30), 100, now, full);
        flow.ack("a", 3, now);
        flow.frame(4, FrameKind::Delta, Some(40), 100, now, none);
        flow.ack("a", 4, now);
        flow.ack("b", 4, now);
        assert_eq!(flow.held(), Vec::<u64>::new());
    }

    #[test]
    fn a_lost_frame_breaks_its_viewers_stream_and_mark() {
        let now = Instant::now();
        let mut flow = flow_with(&["a", "b"], now);
        send(&mut flow, 1, FrameKind::Key, now);
        flow.frame(2, FrameKind::Delta, Some(20), 100, now, none);
        flow.lost("a", 2, now);
        flow.ack("b", 2, now);
        assert_eq!(flow.held(), Vec::<u64>::new());
        assert_eq!(send(&mut flow, 3, FrameKind::Delta, now).to, ["b"]);
        assert_eq!(send(&mut flow, 4, FrameKind::Anchor, now).to, ["a", "b"]);
    }

    #[test]
    fn a_viewer_that_starts_its_decoder_again_gets_no_frame_before_a_key_frame() {
        let now = Instant::now();
        let mut flow = flow_with(&["a"], now);
        send(&mut flow, 1, FrameKind::Key, now);
        flow.ack("a", 1, now);
        assert_eq!(flow.reset("a", now, none), None);
        assert_eq!(
            send(&mut flow, 2, FrameKind::Anchor, now).to,
            Vec::<String>::new()
        );
        assert_eq!(flow.due(after(now, 999), none), None);
        assert_eq!(flow.due(after(now, 1000), none), Some(Ask::Keyframe));
        assert_eq!(flow.due(after(now, 1000), none), None);
    }

    #[test]
    fn a_viewer_that_takes_nothing_for_long_goes() {
        let now = Instant::now();
        let mut flow = flow_with(&["gone", "here"], now);
        send(&mut flow, 1, FrameKind::Key, now);
        flow.ack("here", 1, after(now, 10));
        assert_eq!(flow.hopeless(now + HOPELESS), ["gone"]);
        assert_eq!(
            send(&mut flow, 2, FrameKind::Delta, now + HOPELESS).to,
            ["here"]
        );
    }

    #[test]
    fn a_viewer_behind_by_many_bytes_gets_no_key_frame_until_they_drain() {
        let now = Instant::now();
        let mut flow = flow_with(&["a"], now);
        flow.frame(1, FrameKind::Key, None, LARGE, now, none);
        assert_eq!(
            send(&mut flow, 2, FrameKind::Key, now).to,
            Vec::<String>::new()
        );
        flow.ack("a", 1, now);
        assert_eq!(send(&mut flow, 3, FrameKind::Key, now).to, ["a"]);
    }

    #[test]
    fn an_ack_after_the_id_wraps_clears_older_frames() {
        let now = Instant::now();
        let mut flow = flow_with(&["a"], now);
        flow.frame(u32::MAX, FrameKind::Key, None, LARGE, now, none);
        flow.ack("a", 1, now);
        assert_eq!(send(&mut flow, 2, FrameKind::Delta, now).to, ["a"]);
    }

    #[test]
    fn a_link_slower_than_the_stream_cuts_the_bitrate_once_and_a_faster_one_raises_it() {
        let now = Instant::now();
        let mut flow = flow_with(&["a"], now);
        send(&mut flow, 1, FrameKind::Key, now);
        flow.ack("a", 1, after(now, 20));
        let every = Duration::from_millis(33);
        let round = Duration::from_millis(20);
        let slower = Link {
            viewer: "a",
            rate: 7_600_000,
            round,
        };
        let start = after(now, 33);
        let mut acks = stream(&mut flow, &[slower], 2..=61, 33_000, every, start);
        let sent = start + every * 59;
        let cut = FIRST_BITRATE / 5 * 4;
        assert_eq!(flow.steer(Some("a"), sent), Some(cut));
        let end = ack_until(&mut flow, &mut acks, None).unwrap();
        assert_eq!(flow.steer(Some("a"), end.max(sent + STEER_EVERY)), None);
        let faster = Link {
            viewer: "a",
            rate: 50_000_000,
            round,
        };
        let mut acks = stream(
            &mut flow,
            &[faster],
            62..=63,
            33_000,
            every,
            end + RISE_AFTER,
        );
        let calm = ack_until(&mut flow, &mut acks, None).unwrap();
        assert_eq!(flow.steer(Some("a"), calm), Some(cut + cut / 16));
    }

    #[test]
    fn the_bitrate_fits_what_the_link_of_the_viewer_in_control_carried() {
        let now = Instant::now();
        let mut flow = flow_with(&["mac", "phone"], now);
        send(&mut flow, 1, FrameKind::Key, now);
        for viewer in ["mac", "phone"] {
            flow.ack(viewer, 1, after(now, 20));
        }
        let round = Duration::from_millis(20);
        let links = [
            Link {
                viewer: "mac",
                rate: 50_000_000,
                round,
            },
            Link {
                viewer: "phone",
                rate: 600_000,
                round,
            },
        ];
        let every = Duration::from_millis(25);
        let start = after(now, 25);
        let mut acks = stream(&mut flow, &links, 2..=13, 25_000, every, start);
        let looked = start + every * 12;
        ack_until(&mut flow, &mut acks, Some(looked));
        assert_eq!(
            flow.steer(Some("mac"), looked),
            Some(FIRST_BITRATE + FIRST_BITRATE / 16)
        );
        let end = ack_until(&mut flow, &mut acks, None).unwrap();
        let fitted = flow.steer(Some("phone"), end).unwrap();
        assert!((450_000..=510_000).contains(&fitted), "{fitted}");
    }

    #[test]
    fn nobody_steers_a_still_page() {
        let now = Instant::now();
        let mut flow = flow_with(&["a"], now);
        assert_eq!(flow.steer(None, now), None);
    }

    #[test]
    fn viewers_that_come_after_all_others_left_start_at_the_first_bitrate() {
        let now = Instant::now();
        let mut flow = flow_with(&["phone"], now);
        send(&mut flow, 1, FrameKind::Key, now);
        flow.ack("phone", 1, after(now, 20));
        let slow = Link {
            viewer: "phone",
            rate: 600_000,
            round: Duration::from_millis(20),
        };
        let every = Duration::from_millis(25);
        let mut acks = stream(&mut flow, &[slow], 2..=13, 25_000, every, after(now, 25));
        let end = ack_until(&mut flow, &mut acks, None).unwrap();
        let fitted = flow.steer(Some("phone"), end).unwrap();
        assert_eq!(flow.bitrate(), fitted);
        flow.seat("phone", true, end);
        assert_eq!(flow.bitrate(), fitted);
        flow.leave("phone");
        flow.seat("mac", true, end);
        assert_eq!(flow.bitrate(), FIRST_BITRATE);
    }
}
