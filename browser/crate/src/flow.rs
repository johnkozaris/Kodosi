use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, Instant},
};

/// Bytes on their way to one viewer before it counts as behind.
const WAITING_MOST: usize = 2 * 1024 * 1024;
/// A viewer that is behind can take a key frame again when this much still waits.
const WAITING_DRAINED: usize = 256 * 1024;
const KEYFRAME_FLOOR: Duration = Duration::from_secs(1);
const HOPELESS: Duration = Duration::from_secs(15);
/// More round trip than the link's own shows that a queue forms.
const QUEUE_LIMIT: Duration = Duration::from_millis(40);
const STEER_EVERY: Duration = Duration::from_millis(250);
const RISE_AFTER: Duration = Duration::from_secs(2);
const ROUND_WINDOW: Duration = Duration::from_secs(10);
pub(crate) const FIRST_BITRATE: u32 = 8_000_000;
const LOWEST_BITRATE: u32 = 1_000_000;
const HIGHEST_BITRATE: u32 = 40_000_000;

struct Sent {
    id: u32,
    bytes: usize,
    at: Instant,
}

struct Seat {
    live: bool,
    sent: VecDeque<Sent>,
    waiting: usize,
    over_since: Option<Instant>,
    round: Option<Duration>,
    least_round: Option<Duration>,
    window_least: Option<Duration>,
    window_at: Instant,
}

impl Seat {
    fn new(now: Instant) -> Self {
        Self {
            live: false,
            sent: VecDeque::new(),
            waiting: 0,
            over_since: None,
            round: None,
            least_round: None,
            window_least: None,
            window_at: now,
        }
    }

    fn queue(&self) -> Duration {
        match (self.round, self.least_round) {
            (Some(round), Some(least)) => round.saturating_sub(least),
            _ => Duration::ZERO,
        }
    }

    fn measure(&mut self, round: Duration, now: Instant) {
        self.round = Some(round);
        self.window_least = Some(self.window_least.map_or(round, |least| least.min(round)));
        self.least_round = Some(self.least_round.map_or(round, |least| least.min(round)));
        if now.duration_since(self.window_at) >= ROUND_WINDOW {
            self.least_round = self.window_least.take();
            self.window_at = now;
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Delivery {
    pub to: Vec<String>,
    pub ask_keyframe: bool,
    pub dropped: Vec<String>,
}

/// One encoder serves all viewers of a window. Viewers report each frame, the bitrate follows
/// the round trip, and a viewer that falls behind gets no new frames until a key frame.
pub(crate) struct Flow {
    seats: HashMap<String, Seat>,
    bitrate: u32,
    steered_at: Option<Instant>,
    cut_at: Option<Instant>,
    keyframe_at: Option<Instant>,
    /// A viewer asked for a key frame within the floor, so one comes when the floor passes.
    keyframe_owed: bool,
}

impl Flow {
    pub(crate) fn new() -> Self {
        Self {
            seats: HashMap::new(),
            bitrate: FIRST_BITRATE,
            steered_at: None,
            cut_at: None,
            keyframe_at: None,
            keyframe_owed: false,
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.seats.is_empty()
    }

    pub(crate) fn seat(&mut self, viewer: &str, now: Instant) {
        self.seats.insert(viewer.to_owned(), Seat::new(now));
    }

    pub(crate) fn leave(&mut self, viewer: &str) {
        self.seats.remove(viewer);
    }

    pub(crate) fn frame(&mut self, id: u32, key: bool, bytes: usize, now: Instant) -> Delivery {
        let mut delivery = Delivery::default();
        let mut wants_key = false;
        if key {
            self.keyframe_owed = false;
        }
        for (viewer, seat) in &mut self.seats {
            if seat.waiting > WAITING_MOST {
                let since = *seat.over_since.get_or_insert(now);
                seat.live = false;
                if now.duration_since(since) >= HOPELESS {
                    delivery.dropped.push(viewer.clone());
                }
                continue;
            }
            seat.over_since = None;
            if !key && !seat.live {
                wants_key |= seat.waiting <= WAITING_DRAINED;
                continue;
            }
            seat.live = true;
            seat.waiting += bytes;
            seat.sent.push_back(Sent { id, bytes, at: now });
            delivery.to.push(viewer.clone());
        }
        for viewer in &delivery.dropped {
            self.seats.remove(viewer);
        }
        delivery.ask_keyframe = wants_key && self.keyframe_allowed(now);
        delivery
    }

    /// A frame that never reached a viewer: the frames after it need it, so the viewer waits
    /// for a key frame again.
    pub(crate) fn lost(&mut self, viewer: &str, id: u32) {
        let Some(seat) = self.seats.get_mut(viewer) else {
            return;
        };
        if seat.sent.back().is_some_and(|sent| sent.id == id)
            && let Some(sent) = seat.sent.pop_back()
        {
            seat.waiting -= sent.bytes;
        }
        seat.live = false;
    }

    /// Whether a viewer may have a key frame now. Each key frame costs every viewer bandwidth.
    pub(crate) fn keyframe_allowed(&mut self, now: Instant) -> bool {
        let allowed = self
            .keyframe_at
            .is_none_or(|at| now.duration_since(at) >= KEYFRAME_FLOOR);
        if allowed {
            self.keyframe_at = Some(now);
        }
        allowed
    }

    pub(crate) fn ask_keyframe(&mut self, now: Instant) -> bool {
        let allowed = self.keyframe_allowed(now);
        self.keyframe_owed |= !allowed;
        allowed
    }

    pub(crate) fn keyframe_sent(&mut self, now: Instant) {
        self.keyframe_at = Some(now);
        self.keyframe_owed = false;
    }

    /// Whether a viewer waits for a key frame that no frame asks for, as on a still page that
    /// draws nothing more: an ask within the floor, or a viewer that lost a frame or caught up.
    pub(crate) fn keyframe_due(&mut self, now: Instant) -> bool {
        let waits = self.keyframe_owed
            || self
                .seats
                .values()
                .any(|seat| !seat.live && seat.waiting <= WAITING_DRAINED);
        if waits && self.keyframe_allowed(now) {
            self.keyframe_owed = false;
            return true;
        }
        false
    }

    /// Frame ids wrap, so a frame is older than the ack when it lies less than half the range
    /// before it.
    pub(crate) fn ack(&mut self, viewer: &str, frame: u32, now: Instant) {
        let Some(seat) = self.seats.get_mut(viewer) else {
            return;
        };
        let mut newest = None;
        while let Some(sent) = seat.sent.front() {
            if frame.wrapping_sub(sent.id) >= u32::MAX / 2 {
                break;
            }
            seat.waiting -= sent.bytes;
            newest = Some(sent.at);
            seat.sent.pop_front();
        }
        if let Some(at) = newest {
            seat.measure(now.duration_since(at), now);
        }
    }

    pub(crate) fn steer(&mut self, now: Instant) -> Option<u32> {
        if self
            .steered_at
            .is_some_and(|at| now.duration_since(at) < STEER_EVERY)
        {
            return None;
        }
        self.steered_at = Some(now);
        let queue = self
            .seats
            .values()
            .filter(|seat| seat.live)
            .map(Seat::queue)
            .max()?;
        let bitrate = if queue > QUEUE_LIMIT {
            self.cut_at = Some(now);
            self.bitrate / 5 * 4
        } else if queue < QUEUE_LIMIT / 2
            && self
                .cut_at
                .is_none_or(|at| now.duration_since(at) >= RISE_AFTER)
        {
            self.bitrate.saturating_add(self.bitrate / 16)
        } else {
            self.bitrate
        }
        .clamp(LOWEST_BITRATE, HIGHEST_BITRATE);
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
            flow.seat(viewer, now);
        }
        flow
    }

    #[test]
    fn a_new_viewer_starts_at_a_key_frame() {
        let now = Instant::now();
        let mut flow = flow_with(&["a"], now);
        let waiting = flow.frame(1, false, 100, now);
        assert_eq!(waiting.to, Vec::<String>::new());
        assert!(waiting.ask_keyframe);
        assert_eq!(flow.frame(2, true, 100, now).to, ["a"]);
        assert_eq!(flow.frame(3, false, 100, now).to, ["a"]);
    }

    #[test]
    fn a_slow_viewer_waits_for_a_key_frame_and_the_others_go_on() {
        let now = Instant::now();
        let mut flow = flow_with(&["slow", "fast"], now);
        flow.frame(1, true, LARGE, now);
        flow.ack("fast", 1, now);
        assert_eq!(flow.frame(2, false, 100, now).to, ["fast"]);
        flow.ack("fast", 2, now);
        flow.ack("slow", 1, now);
        let drained = flow.frame(3, false, 100, now + KEYFRAME_FLOOR);
        assert_eq!(drained.to, ["fast"]);
        assert!(drained.ask_keyframe);
        let mut to = flow.frame(4, true, 100, now + KEYFRAME_FLOOR).to;
        to.sort();
        assert_eq!(to, ["fast", "slow"]);
    }

    #[test]
    fn a_viewer_that_lost_a_frame_waits_for_a_key_frame_and_owes_nothing_for_it() {
        let now = Instant::now();
        let mut flow = flow_with(&["a"], now);
        flow.frame(1, true, 100, now);
        flow.ack("a", 1, now);
        assert_eq!(flow.frame(2, false, 500, now).to, ["a"]);
        flow.lost("a", 2);
        let waiting = flow.frame(3, false, 100, now);
        assert_eq!(waiting.to, Vec::<String>::new());
        assert!(waiting.ask_keyframe);
        assert_eq!(flow.frame(4, true, 100, now + KEYFRAME_FLOOR).to, ["a"]);
        flow.ack("a", 4, now + KEYFRAME_FLOOR);
        assert_eq!(
            flow.frame(5, false, LARGE - 1, now + KEYFRAME_FLOOR).to,
            ["a"]
        );
    }

    #[test]
    fn key_frames_keep_a_floor_between_them() {
        let now = Instant::now();
        let mut flow = flow_with(&["a"], now);
        assert!(flow.frame(1, false, 100, now).ask_keyframe);
        assert!(!flow.frame(2, false, 100, now).ask_keyframe);
        assert!(flow.frame(3, false, 100, now + KEYFRAME_FLOOR).ask_keyframe);
    }

    #[test]
    fn a_viewer_that_lost_a_frame_of_a_still_page_gets_a_key_frame_after_the_floor() {
        let now = Instant::now();
        let mut flow = flow_with(&["a"], now);
        flow.keyframe_sent(now);
        flow.frame(1, true, 100, now);
        flow.ack("a", 1, now);
        flow.frame(2, false, 100, now);
        flow.lost("a", 2);
        assert!(!flow.keyframe_due(now));
        assert!(flow.keyframe_due(now + KEYFRAME_FLOOR));
        assert!(!flow.keyframe_due(now + KEYFRAME_FLOOR));
    }

    #[test]
    fn an_ask_within_the_floor_comes_later_unless_a_key_frame_came_first() {
        let now = Instant::now();
        let mut flow = flow_with(&["a"], now);
        flow.frame(1, true, 100, now);
        assert!(flow.ask_keyframe(now));
        assert!(!flow.ask_keyframe(now));
        assert!(!flow.keyframe_due(now));
        assert!(flow.keyframe_due(now + KEYFRAME_FLOOR));
        assert!(!flow.ask_keyframe(now + KEYFRAME_FLOOR));
        flow.frame(2, true, 100, now + KEYFRAME_FLOOR);
        assert!(!flow.keyframe_due(now + KEYFRAME_FLOOR * 2));
    }

    #[test]
    fn a_viewer_that_stays_behind_goes() {
        let now = Instant::now();
        let mut flow = flow_with(&["gone"], now);
        flow.frame(1, true, LARGE, now);
        assert_eq!(flow.frame(2, true, 100, now).dropped, Vec::<String>::new());
        let later = flow.frame(3, true, 100, now + HOPELESS);
        assert_eq!(later.dropped, ["gone"]);
        assert!(flow.is_empty());
    }

    #[test]
    fn an_ack_after_the_id_wraps_clears_older_frames() {
        let now = Instant::now();
        let mut flow = flow_with(&["a"], now);
        flow.frame(u32::MAX, true, LARGE, now);
        flow.ack("a", 1, now);
        assert_eq!(flow.frame(2, false, 100, now).to, ["a"]);
    }

    #[test]
    fn a_rising_round_trip_cuts_the_bitrate_and_a_calm_one_raises_it() {
        let now = Instant::now();
        let mut flow = flow_with(&["a"], now);
        flow.frame(1, true, 100, now);
        flow.ack("a", 1, now + Duration::from_millis(10));
        flow.frame(2, false, 100, now);
        flow.ack("a", 2, now + Duration::from_millis(100));
        assert_eq!(flow.steer(now), Some(FIRST_BITRATE / 5 * 4));
        let after_cut = now + STEER_EVERY;
        flow.frame(3, false, 100, after_cut);
        flow.ack("a", 3, after_cut + Duration::from_millis(10));
        assert_eq!(flow.steer(after_cut), None);
        let later = now + RISE_AFTER + STEER_EVERY;
        let lower = FIRST_BITRATE / 5 * 4;
        assert_eq!(flow.steer(later), Some(lower + lower / 16));
    }

    #[test]
    fn nobody_steers_a_still_page() {
        let now = Instant::now();
        let mut flow = flow_with(&["a"], now);
        assert_eq!(flow.steer(now), None);
    }
}
