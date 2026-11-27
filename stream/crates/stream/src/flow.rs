use std::{
    collections::{HashMap, HashSet, VecDeque},
    time::{Duration, Instant},
};

use crate::FrameKind;

/// Max unacked bytes in flight per viewer before dropping frames.
const WAITING_MOST: usize = 2 * 1024 * 1024;
/// In-flight byte watermark below which a behind viewer may resume.
const WAITING_DRAINED: usize = 256 * 1024;
/// Excess ack delay (beyond RTT and bitrate serialization) that counts as late.
const LATE: Duration = Duration::from_millis(250);
/// Sustained lateness required before dropping frames, ignoring brief network hiccups.
const LATE_FOR: Duration = Duration::from_millis(500);
/// Max frames buffered in a viewer's outgoing channel before marking it behind.
const QUEUED_MOST: usize = 3;
const KEYFRAME_FLOOR: Duration = Duration::from_secs(1);
const ANCHOR_FLOOR: Duration = Duration::from_millis(250);
/// Minimum steady streaming duration before resetting a viewer's keyframe backoff.
const STEADY: Duration = Duration::from_secs(10);
const MOST_FALLS: u32 = 3;
/// Inactivity timeout before evicting a viewer that stops acking frames.
pub const HOPELESS: Duration = Duration::from_secs(15);
/// How often a viewer with the newest picture hears that nothing changed. A viewer rests its
/// picture after several periods without a frame or this word, so one lost word does not dim it.
const UNCHANGED_EVERY: Duration = Duration::from_millis(250);
/// Queuing delay threshold indicating the link is slower than the current bitrate.
const QUEUE_LIMIT: Duration = Duration::from_millis(40);
const STEER_EVERY: Duration = Duration::from_millis(250);
const RISE_AFTER: Duration = Duration::from_secs(2);
const ROUND_WINDOW: Duration = Duration::from_secs(10);
/// Window over which saturated-link throughput samples are kept.
const CARRIED_FOR: Duration = Duration::from_secs(1);
const FIRST_BITRATE: u32 = 8_000_000;
const LOWEST_BITRATE: u32 = 250_000;
const HIGHEST_BITRATE: u32 = 40_000_000;

/// Decoder reference state for a viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Holds {
    /// No valid reference frames (new or reset decoder); requires a keyframe.
    Nothing,
    /// Continuous stream since the last keyframe or anchor; can decode deltas.
    Stream,
    /// Missed recent frames, but still holds earlier reference marks for anchor recovery.
    Gap,
}

impl Holds {
    const fn after_a_miss(anchors: bool) -> Self {
        if anchors { Self::Gap } else { Self::Nothing }
    }
}

struct Sent {
    id: u32,
    bytes: usize,
    at: Instant,
    kind: FrameKind,
    /// Expected delivery time at the stream bitrate, accounting for prior in-flight frames.
    due: Instant,
    /// Cumulative acked bytes and `heard_at` snapshot when this frame was sent.
    taken: u64,
    heard_at: Instant,
}

struct Seat {
    holds: Holds,
    anchors: bool,
    /// Local viewers are only delayed by app work, not bandwidth, so they never lower bitrate.
    local: bool,
    /// When the viewer last entered `Holds::Stream`.
    going_since: Option<Instant>,
    /// Consecutive rapid drops and the earliest time to request another keyframe.
    falls: u32,
    key_after: Option<Instant>,
    sent: VecDeque<Sent>,
    waiting: usize,
    /// Expected arrival time of the most recently sent frame at the stream bitrate.
    due_at: Instant,
    taken: u64,
    /// Measured throughput samples (bps) collected while the link was saturated.
    carried: VecDeque<(Instant, u64)>,
    late_since: Option<Instant>,
    /// Last ack timestamp, or the send timestamp when the in-flight queue went non-empty.
    heard_at: Instant,
    /// Latest queuing delay sample from frames sent after the last bitrate cut.
    queue: Option<Duration>,
    least_round: Option<Duration>,
    window_least: Option<Duration>,
    window_at: Instant,
    /// The bitrate that the transport's congestion control finds for this viewer, when its frames
    /// go as datagrams.
    transported: Option<u32>,
    /// When the viewer was last sent a frame or told that nothing changed.
    said_at: Instant,
}

impl Seat {
    fn new(anchors: bool, local: bool, now: Instant) -> Self {
        Self {
            holds: Holds::Nothing,
            anchors,
            local,
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
            transported: None,
            said_at: now,
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

    /// Exponentially backs off keyframe requests so a slow viewer cannot trigger a keyframe
    /// every second for all other viewers.
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
        self.said_at = now;
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

    /// Takes the acknowledgement of `frame`. A viewer acknowledges each frame that it gets, in
    /// order, so the frames sent before it and still unacknowledged did not arrive. Returns the
    /// acknowledged frame when it was in flight, and those that did not arrive.
    fn take(&mut self, frame: u32, now: Instant) -> (Option<Sent>, Vec<Sent>) {
        let mut missed = Vec::new();
        while let Some(sent) = self
            .sent
            .pop_front_if(|sent| sent.id != frame && precedes(sent.id, frame))
        {
            self.waiting -= sent.bytes;
            missed.push(sent);
        }
        let acked = self.sent.pop_front_if(|sent| sent.id == frame);
        if let Some(acked) = &acked {
            self.waiting -= acked.bytes;
            self.taken += u64::try_from(acked.bytes).unwrap_or(u64::MAX);
        }
        if acked.is_some() || !missed.is_empty() {
            self.heard_at = now;
        }
        (acked, missed)
    }

    /// The viewer did not get some frames. Unless a later one puts it in step again, its picture
    /// falls, and a missed keyframe leaves it with nothing to decode from.
    fn missed(&mut self, key: bool, in_step: bool, now: Instant) {
        if in_step {
            return;
        }
        if self.holds == Holds::Stream {
            self.fall(now);
        }
        if key {
            self.holds = Holds::Nothing;
        }
    }

    fn measure(&mut self, sent: &Sent, steers: bool, now: Instant) {
        // Exclude keyframes and anchors from RTT tracking; their size inflates transit time.
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
        // Only sample link throughput when queuing indicates the link is saturated.
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

    fn carried(&mut self, now: Instant) -> Option<u64> {
        self.forget_carried(now);
        self.carried.iter().map(|(_, rate)| *rate).max()
    }
}

impl Sent {
    /// Whether the frame puts a viewer in step again after it missed frames: a keyframe always,
    /// an anchor only when no keyframe was missed, since anchors refer to marks on frames after
    /// the last keyframe.
    fn puts_in_step(&self, key_missed: bool) -> bool {
        match self.kind {
            FrameKind::Key => true,
            FrameKind::Anchor => !key_missed,
            FrameKind::Delta => false,
        }
    }
}

/// Whether frame `earlier` comes before or is `later`, past the 32-bit wrap.
fn precedes(earlier: u32, later: u32) -> bool {
    later.wrapping_sub(earlier) < u32::MAX / 2
}

/// Serialization time for `bytes` at `bitrate` bits per second.
fn crossing(bytes: usize, bitrate: u32) -> Duration {
    let bits = u64::try_from(bytes).unwrap_or(u64::MAX).saturating_mul(8);
    Duration::from_micros(bits.saturating_mul(1_000_000) / u64::from(bitrate.max(1)))
}

/// Tracks which viewers have not yet acknowledged a marked reference frame.
struct MarkedFrame {
    id: u32,
    mark: u64,
    waiting: HashSet<String>,
    taken: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask {
    Keyframe,
    Recover,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Delivery {
    pub to: Vec<String>,
    pub ask: Option<Ask>,
}

/// Per-window congestion control and frame gating for a shared encoder.
///
/// Viewers ack received frames; a viewer that falls behind skips frames until the next keyframe
/// or recovery anchor to avoid latency buildup. Bitrate tracks the viewer in control so passive
/// viewers do not degrade quality for the active user.
pub struct Flow {
    seats: HashMap<String, Seat>,
    marks: VecDeque<MarkedFrame>,
    held: Vec<u64>,
    /// True once the encoder emits marked frames, indicating anchor recovery is supported.
    anchors: bool,
    bitrate: u32,
    steered_at: Option<Instant>,
    cut_at: Option<Instant>,
    keyframe_at: Option<Instant>,
    recovered_at: Option<Instant>,
}

impl Default for Flow {
    fn default() -> Self {
        Self::new()
    }
}

impl Flow {
    pub fn new() -> Self {
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

    pub fn is_empty(&self) -> bool {
        self.seats.is_empty()
    }

    pub fn seat(&mut self, viewer: &str, anchors: bool, local: bool, now: Instant) {
        self.forget(viewer);
        if self.seats.is_empty() {
            self.bitrate = if local {
                HIGHEST_BITRATE
            } else {
                FIRST_BITRATE
            };
            self.cut_at = None;
        }
        self.seats
            .insert(viewer.to_owned(), Seat::new(anchors, local, now));
    }

    pub const fn bitrate(&self) -> u32 {
        self.bitrate
    }

    pub fn leave(&mut self, viewer: &str) {
        self.seats.remove(viewer);
        self.forget(viewer);
    }

    pub fn frame(
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

    /// Records a frame sent to `viewer` that will not arrive: dropped before delivery, or given up
    /// by the transport. It may be any frame in flight.
    pub fn lost(&mut self, viewer: &str, id: u32, now: Instant) {
        let Some(seat) = self.seats.get_mut(viewer) else {
            return;
        };
        let Some(at) = seat.sent.iter().position(|sent| sent.id == id) else {
            return;
        };
        let Some(missed) = seat.sent.remove(at) else {
            return;
        };
        seat.waiting -= missed.bytes;
        self.miss(viewer, &[missed], None, at, now);
    }

    /// Acts on frames that `viewer` did not get, oldest first: its picture falls unless `acked`
    /// or a frame in flight from position `later` on puts it in step again, and marks on frames
    /// that it cannot decode go, since anchors may refer only to marks that every viewer holds.
    fn miss(
        &mut self,
        viewer: &str,
        missed: &[Sent],
        acked: Option<&Sent>,
        later: usize,
        now: Instant,
    ) {
        let (Some(oldest), Some(seat)) = (missed.first(), self.seats.get_mut(viewer)) else {
            return;
        };
        let key = missed.iter().any(|sent| sent.kind == FrameKind::Key);
        let in_step = acked
            .into_iter()
            .chain(seat.sent.iter().skip(later))
            .find(|sent| sent.puts_in_step(key))
            .map(|sent| sent.id);
        seat.missed(key, in_step.is_some(), now);
        let from = oldest.id;
        self.marks.retain(|marked| {
            let broken = precedes(from, marked.id)
                && in_step.is_none_or(|until| marked.id != until && precedes(marked.id, until));
            !(broken && marked.waiting.contains(viewer))
        });
        self.settle();
    }

    pub fn reset(
        &mut self,
        viewer: &str,
        now: Instant,
        queued: impl Fn(&str) -> usize,
    ) -> Option<Ask> {
        self.seats.get_mut(viewer)?.holds = Holds::Nothing;
        self.forget(viewer);
        self.ask(now, queued)
    }

    /// Takes the bitrate that the transport carries to a viewer, which then steers the bitrate
    /// instead of the delay of its acknowledgements; 0 returns the viewer to those.
    pub fn rate(&mut self, viewer: &str, bitrate: u64) {
        if let Some(seat) = self.seats.get_mut(viewer) {
            seat.transported = (bitrate > 0).then(|| u32::try_from(bitrate).unwrap_or(u32::MAX));
        }
    }

    pub fn keyframe_asked(&mut self, now: Instant) {
        self.keyframe_at = Some(now);
    }

    /// Decides whether to request a keyframe or recovery anchor for caught-up viewers.
    ///
    /// Also called on timer ticks so recovery proceeds on static pages with no incoming frames.
    pub fn ask(&mut self, now: Instant, queued: impl Fn(&str) -> usize) -> Option<Ask> {
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

    pub fn ack(&mut self, viewer: &str, frame: u32, now: Instant) {
        let Some(seat) = self.seats.get_mut(viewer) else {
            return;
        };
        let (acked, missed) = seat.take(frame, now);
        if let Some(sent) = &acked {
            let steers = self.cut_at.is_none_or(|cut| sent.at > cut);
            seat.measure(sent, steers, now);
        }
        self.miss(viewer, &missed, acked.as_ref(), 0, now);
        for marked in &mut self.marks {
            if precedes(marked.id, frame) && marked.waiting.remove(viewer) {
                marked.taken = true;
            }
        }
        self.settle();
    }

    /// Drains reference marks acknowledged by all viewers since the last call.
    pub fn held(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.held)
    }

    pub fn hopeless(&mut self, now: Instant) -> Vec<String> {
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

    /// Names each viewer to tell now that nothing changed: one that holds the stream, has
    /// acknowledged each frame that it was sent, got neither a frame nor this word for
    /// `UNCHANGED_EVERY`, and has nothing waiting on its way. A viewer with a frame in flight or a
    /// gap hears nothing, since its picture is not the newest.
    pub fn unchanged(&mut self, now: Instant, queued: impl Fn(&str) -> usize) -> Vec<String> {
        self.seats
            .iter_mut()
            .filter(|(viewer, seat)| {
                seat.holds == Holds::Stream
                    && seat.sent.is_empty()
                    && now.duration_since(seat.said_at) >= UNCHANGED_EVERY
                    && queued(viewer) == 0
            })
            .map(|(viewer, seat)| {
                seat.said_at = now;
                viewer.clone()
            })
            .collect()
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

    /// Adjusts bitrate toward the controlling viewer's link (or all remote viewers if none has
    /// control), capping it below measured saturated throughput. Only frames sent after the last
    /// cut count toward the queuing delay, since a cut shows only in those frames.
    pub fn steer(&mut self, control: Option<&str>, now: Instant) -> Option<u32> {
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
        let mut transported: Option<u32> = None;
        let mut linked = false;
        for (viewer, seat) in &mut self.seats {
            let fresh = seat.queue.take();
            if seat.local || followed.is_some_and(|followed| viewer.as_str() != followed) {
                continue;
            }
            if let Some(rate) = seat.transported {
                transported = Some(transported.map_or(rate, |least| least.min(rate)));
                continue;
            }
            linked = true;
            if seat.holds == Holds::Stream {
                queue = queue.max(fresh);
            }
            if let Some(rate) = seat.carried(now) {
                carried = Some(carried.map_or(rate, |least| least.min(rate)));
            }
        }
        let mut bitrate = if !linked {
            transported.unwrap_or(HIGHEST_BITRATE)
        } else if queue.is_some_and(|queue| queue > QUEUE_LIMIT) {
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
        if let Some(transported) = transported {
            bitrate = bitrate.min(transported);
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
            flow.seat(viewer, true, false, now);
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

    struct Link {
        viewer: &'static str,
        rate: u64,
        round: Duration,
    }

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

    fn slow_and_fast(mark: Option<u64>, anchors: bool) -> (Flow, Instant) {
        let now = Instant::now();
        let mut flow = Flow::new();
        flow.seat("slow", anchors, false, now);
        flow.seat("fast", true, false, now);
        flow.frame(1, FrameKind::Key, mark, 100, now, none);
        flow.frame(2, FrameKind::Delta, None, 100, after(now, 10), none);
        for viewer in ["slow", "fast"] {
            flow.ack(viewer, 1, after(now, 10));
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
        for id in 3..=5 {
            flow.ack("slow", id, after(now, 950));
        }
        (flow, now)
    }

    #[test]
    fn the_bitrate_that_the_transport_carries_to_the_viewer_in_control_steers_the_encoder() {
        let now = Instant::now();
        let mut flow = flow_with(&["phone", "mac"], now);
        flow.rate("phone", 3_000_000);
        flow.rate("mac", 20_000_000);
        assert_eq!(flow.steer(Some("phone"), now), Some(3_000_000));
        flow.rate("phone", 6_000_000);
        assert_eq!(flow.steer(Some("phone"), after(now, 250)), Some(6_000_000));
        assert_eq!(flow.steer(None, after(now, 500)), None);
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
        assert_eq!(flow.ask(after(now, 950), none), Some(Ask::Recover));
        assert_eq!(flow.ask(after(now, 960), none), None);
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
        assert_eq!(flow.ask(after(now, 999), none), None);
        assert_eq!(flow.ask(after(now, 1000), none), Some(Ask::Keyframe));
    }

    #[test]
    fn a_viewer_that_keeps_falling_behind_waits_longer_for_each_key_frame() {
        let now = Instant::now();
        let mut flow = Flow::new();
        flow.seat("slow", false, false, now);
        flow.seat("fast", true, false, now);
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
            assert_eq!(flow.ask(after(now, due - 1), none), None);
            assert_eq!(flow.ask(after(now, due), none), Some(Ask::Keyframe));
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
            assert_eq!(flow.ask(after(now, due - 1), none), None);
            assert_eq!(flow.ask(after(now, due), none), Some(Ask::Keyframe));
            key_at = due + 100;
        }
    }

    #[test]
    fn a_late_viewer_goes_on_only_once_its_pipe_took_the_frames_that_wait() {
        let (mut flow, now) = slow_and_fast(Some(1), true);
        let full = |viewer: &str| if viewer == "slow" { QUEUED_MOST } else { 0 };
        assert_eq!(flow.ask(after(now, 950), full), None);
        assert_eq!(flow.ask(after(now, 960), none), Some(Ask::Recover));
    }

    #[test]
    fn without_marks_a_late_viewer_goes_on_at_a_key_frame_after_the_floor() {
        let (mut flow, now) = slow_and_fast(None, true);
        assert_eq!(flow.ask(after(now, 950), none), None);
        assert_eq!(flow.ask(after(now, 1000), none), Some(Ask::Keyframe));
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
        for id in 1..=40 {
            flow.ack("a", id, after(now, 700));
        }
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
    fn a_mark_is_held_once_each_viewer_that_took_its_frame_said_so_and_never_after_a_miss() {
        let now = Instant::now();
        let mut flow = flow_with(&["a", "b"], now);
        send(&mut flow, 1, FrameKind::Key, now);
        flow.frame(2, FrameKind::Delta, Some(20), 100, now, none);
        for viewer in ["a", "b"] {
            flow.ack(viewer, 1, now);
        }
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
        flow.ack("b", 1, now);
        flow.ack("b", 2, now);
        assert_eq!(flow.held(), Vec::<u64>::new());
        assert_eq!(send(&mut flow, 3, FrameKind::Delta, now).to, ["b"]);
        assert_eq!(send(&mut flow, 4, FrameKind::Anchor, now).to, ["a", "b"]);
    }

    #[test]
    fn an_acknowledgement_shows_that_the_frames_before_it_did_not_arrive() {
        let now = Instant::now();
        let mut flow = flow_with(&["phone"], now);
        send(&mut flow, 1, FrameKind::Key, now);
        flow.ack("phone", 1, after(now, 20));
        flow.frame(2, FrameKind::Delta, Some(20), 100, after(now, 30), none);
        for id in [3, 4] {
            send(&mut flow, id, FrameKind::Delta, after(now, 40));
        }
        flow.ack("phone", 3, after(now, 60));
        let next = send(&mut flow, 5, FrameKind::Delta, after(now, 70));
        assert_eq!(
            next.to,
            Vec::<String>::new(),
            "the viewer cannot decode after the lost frame"
        );
        assert_eq!(next.ask, Some(Ask::Recover));
        flow.ack("phone", 4, after(now, 80));
        assert_eq!(flow.held(), Vec::<u64>::new());
    }

    #[test]
    fn a_frame_to_start_from_after_lost_frames_puts_the_viewer_in_step_and_marks_between_go() {
        let now = Instant::now();
        let mut flow = flow_with(&["phone"], now);
        send(&mut flow, 1, FrameKind::Key, now);
        flow.ack("phone", 1, after(now, 20));
        send(&mut flow, 2, FrameKind::Delta, after(now, 30));
        flow.frame(3, FrameKind::Delta, Some(30), 100, after(now, 40), none);
        send(&mut flow, 4, FrameKind::Anchor, after(now, 50));
        flow.lost("phone", 2, after(now, 55));
        for id in [3, 4] {
            flow.ack("phone", id, after(now, 70));
        }
        assert_eq!(
            send(&mut flow, 5, FrameKind::Delta, after(now, 80)).to,
            ["phone"]
        );
        assert_eq!(flow.held(), Vec::<u64>::new(), "frame 3 was not decodable");
    }

    #[test]
    fn a_lost_keyframe_leaves_the_viewer_waiting_for_a_keyframe() {
        let now = Instant::now();
        let mut flow = flow_with(&["phone"], now);
        flow.keyframe_asked(now);
        send(&mut flow, 1, FrameKind::Key, now);
        for (id, at) in [(2, 16), (3, 33)] {
            send(&mut flow, id, FrameKind::Delta, after(now, at));
        }
        flow.lost("phone", 1, after(now, 60));
        for id in [2, 3] {
            flow.ack("phone", id, after(now, 65));
        }
        assert_eq!(
            send(&mut flow, 4, FrameKind::Delta, after(now, 70)).to,
            Vec::<String>::new()
        );
        assert_eq!(flow.ask(after(now, 999), none), None);
        assert_eq!(flow.ask(after(now, 1000), none), Some(Ask::Keyframe));
    }

    /// A viewer whose device is slow to acknowledge frames that arrived stays late, so it skips
    /// frames as before, and gets no repair while it is late.
    #[test]
    fn a_viewer_slow_to_acknowledge_frames_that_arrived_is_skipped_and_gets_no_repair_meanwhile() {
        let now = Instant::now();
        let mut flow = flow_with(&["phone", "mac"], now);
        flow.rate("phone", 3_000_000);
        send(&mut flow, 1, FrameKind::Key, now);
        for viewer in ["phone", "mac"] {
            flow.ack(viewer, 1, after(now, 20));
        }
        let mut skipped_at = None;
        for (id, at) in (2..).zip((40..2000).step_by(40)) {
            let delivery = send(&mut flow, id, FrameKind::Delta, after(now, at));
            flow.ack("mac", id, after(now, at + 10));
            assert_eq!(delivery.ask, None, "at {at} ms");
            if delivery.to == ["mac"] {
                skipped_at.get_or_insert(at);
            }
            assert_eq!(flow.ask(after(now, at + 20), none), None, "at {at} ms");
        }
        let skipped_at = skipped_at.unwrap();
        assert!((700..=840).contains(&skipped_at), "{skipped_at}");
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
        assert_eq!(flow.ask(after(now, 999), none), None);
        assert_eq!(flow.ask(after(now, 1000), none), Some(Ask::Keyframe));
        assert_eq!(flow.ask(after(now, 1000), none), None);
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
    fn only_a_viewer_with_the_newest_picture_hears_once_a_period_that_nothing_changed() {
        let now = Instant::now();
        let mut flow = flow_with(&["current", "in flight", "gap"], now);
        send(&mut flow, 1, FrameKind::Key, now);
        send(&mut flow, 2, FrameKind::Delta, after(now, 100));
        flow.lost("gap", 2, after(now, 100));
        for viewer in ["current", "gap"] {
            flow.ack(viewer, 1, after(now, 110));
        }
        flow.ack("current", 2, after(now, 110));
        let mut told = |at, queued: &dyn Fn(&str) -> usize| flow.unchanged(after(now, at), queued);
        assert_eq!(told(349, &none), Vec::<String>::new());
        assert_eq!(told(350, &none), ["current"]);
        assert_eq!(told(599, &none), Vec::<String>::new(), "once a period");
        assert_eq!(told(600, &none), ["current"]);
        assert_eq!(
            told(850, &|viewer| usize::from(viewer == "current")),
            Vec::<String>::new(),
            "an earlier word still waits on its way"
        );
        assert_eq!(told(900, &none), ["current"]);
        send(&mut flow, 3, FrameKind::Delta, after(now, 1000));
        flow.ack("current", 3, after(now, 1010));
        assert_eq!(
            flow.unchanged(after(now, 1249), none),
            Vec::<String>::new(),
            "a frame starts the period again"
        );
        assert_eq!(flow.unchanged(after(now, 1250), none), ["current"]);
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
    fn a_view_on_the_browsers_computer_keeps_the_highest_bitrate_while_its_app_is_late() {
        let now = Instant::now();
        let mut flow = Flow::new();
        flow.seat("mac", true, true, now);
        assert_eq!(flow.bitrate(), HIGHEST_BITRATE);
        send(&mut flow, 1, FrameKind::Key, now);
        flow.ack("mac", 1, after(now, 20));
        let busy = Link {
            viewer: "mac",
            rate: 7_600_000,
            round: Duration::from_millis(20),
        };
        let every = Duration::from_millis(33);
        let start = after(now, 33);
        let mut acks = stream(&mut flow, &[busy], 2..=61, 33_000, every, start);
        assert_eq!(flow.steer(Some("mac"), start + every * 59), None);
        let end = ack_until(&mut flow, &mut acks, None).unwrap();
        assert_eq!(flow.steer(None, end + STEER_EVERY), None);
        assert_eq!(flow.bitrate(), HIGHEST_BITRATE);
    }

    #[test]
    fn beside_a_view_on_the_browsers_computer_the_link_of_another_device_in_control_sets_the_bitrate()
     {
        let now = Instant::now();
        let mut flow = Flow::new();
        flow.seat("mac", true, true, now);
        flow.seat("phone", true, false, now);
        send(&mut flow, 1, FrameKind::Key, now);
        for viewer in ["mac", "phone"] {
            flow.ack(viewer, 1, after(now, 20));
        }
        let round = Duration::from_millis(20);
        let links = [
            Link {
                viewer: "mac",
                rate: 2_000_000,
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
        assert_eq!(flow.steer(Some("mac"), looked), None);
        let end = ack_until(&mut flow, &mut acks, None).unwrap();
        let fitted = flow.steer(Some("phone"), end).unwrap();
        assert!((450_000..=510_000).contains(&fitted), "{fitted}");
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
        flow.seat("phone", true, false, end);
        assert_eq!(flow.bitrate(), fitted);
        flow.leave("phone");
        flow.seat("mac", true, false, end);
        assert_eq!(flow.bitrate(), FIRST_BITRATE);
    }
}
