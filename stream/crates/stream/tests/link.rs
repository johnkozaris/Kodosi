//! The video path end to end on a virtual clock: the real packer and its send rules, a link with
//! a rate limit, a queue and loss, the real unpacker, a viewer app that acknowledges each frame
//! it gets and decodes what it can, and the real flow control, which alone repairs the picture.
//! The viewer app never asks for a keyframe here.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    time::{Duration, Instant},
};

use bytes::Bytes;
use stream::{
    Ask, Codec, Flow, FrameKind,
    media::{self, Link, Packer, Unpacker},
    viewer::Frame,
};

const STEP: Duration = Duration::from_micros(250);
const LENGTH: Duration = Duration::from_secs(15);
const DATAGRAM: usize = 1162;
/// What QUIC adds to each datagram.
const PACKET_BYTES: usize = 40;
const BURST: f64 = 4.0 * 1200.0;
const RECEIVE_EVERY: u32 = 20;
const LIVE_EVERY: u32 = 400;
/// The bottleneck drops what would wait longer than this.
const QUEUE: Duration = Duration::from_millis(200);
const VIEWER: &str = "viewer";
/// The frames that wait for the viewer's app, and the shards of a flow that wait for the viewer's
/// runtime while it waits for room there.
const PIPE: usize = 64;
const FLOW_SHARDS: usize = 4096;
const STALL: Duration = Duration::from_secs(2);
/// The longest the viewer may be without a picture it can decode, and the longest a whole new
/// screen may take to show.
const BOUND: Duration = Duration::from_secs(1);

struct Random(u64);

impl Random {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        #[expect(clippy::cast_precision_loss, reason = "53 random bits")]
        let share = (self.0 >> 11) as f64 / (1_u64 << 53) as f64;
        share
    }
}

#[derive(Clone, Copy)]
struct Case {
    name: &'static str,
    capacity: f64,
    rtt: Duration,
    screen: usize,
}

#[derive(Clone, Copy)]
struct Made {
    kind: FrameKind,
    mark: Option<u64>,
    refers: Option<u64>,
    bytes: usize,
}

/// What a decoder holds, from the frames that it got in order.
#[derive(Default, PartialEq, Eq, Clone, Copy, Debug)]
enum Holds {
    #[default]
    Nothing,
    Stream,
    Gap,
}

#[derive(Default)]
struct Picture {
    holds: Holds,
    last: Option<u32>,
    marks: HashSet<u64>,
}

impl Picture {
    fn take(&mut self, id: u32, made: Made) {
        if self.holds == Holds::Stream && self.last.is_some_and(|last| last + 1 != id) {
            self.holds = Holds::Gap;
        }
        self.last = Some(id);
        match made.kind {
            FrameKind::Key => {
                self.holds = Holds::Stream;
                self.marks.clear();
            }
            FrameKind::Anchor => {
                let held = made.refers.is_some_and(|mark| self.marks.contains(&mark));
                if self.holds == Holds::Stream || (self.holds == Holds::Gap && held) {
                    self.holds = Holds::Stream;
                }
            }
            FrameKind::Delta => {}
        }
        if self.holds == Holds::Stream
            && let Some(mark) = made.mark
        {
            self.marks.insert(mark);
        }
    }
}

/// An encoder that makes a frame for each change of the screen, and a keyframe or an anchor on
/// a long-term reference when the flow control asks.
#[derive(Default)]
struct Encoder {
    newest: u32,
    repair: Option<Ask>,
    held: Vec<u64>,
    marks: u64,
    deltas: u32,
}

impl Encoder {
    fn frame(&mut self, change: Option<usize>, screen: usize) -> Option<(u32, Made)> {
        let (kind, refers, bytes) = match self.repair.take() {
            Some(Ask::Keyframe) => (FrameKind::Key, None, screen),
            Some(Ask::Recover) => self
                .held
                .last()
                .map_or((FrameKind::Key, None, screen), |mark| {
                    (
                        FrameKind::Anchor,
                        Some(*mark),
                        change.unwrap_or(0).max(screen / 2),
                    )
                }),
            None => (FrameKind::Delta, None, change?),
        };
        if kind == FrameKind::Key {
            self.held.clear();
        }
        self.newest += 1;
        let marked = kind != FrameKind::Delta || {
            self.deltas += 1;
            self.deltas.is_multiple_of(8)
        };
        let mark = marked.then(|| {
            self.marks += 1;
            self.marks
        });
        Some((
            self.newest,
            Made {
                kind,
                mark,
                refers,
                bytes,
            },
        ))
    }
}

/// Typing, two whole new screens, a scroll at half the link and 60 frames a second, then a still
/// picture.
fn script(case: Case) -> Vec<(Duration, usize, bool)> {
    let ms = Duration::from_millis;
    let mut events = Vec::new();
    for (from, to) in [(500, 2500), (5000, 6500), (9000, 10_000)] {
        events.extend((from..to).step_by(100).map(|at| (ms(at), 1500, false)));
    }
    events.extend([(ms(3000), case.screen, true), (ms(7000), case.screen, true)]);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a frame's bytes"
    )]
    let scrolled = (case.capacity * 0.5 / 60.0 / 8.0) as usize;
    events.extend(
        (10_500..12_500)
            .step_by(16)
            .map(|at| (ms(at), scrolled, false)),
    );
    events.sort_by_key(|(at, ..)| *at);
    events
}

#[derive(Debug)]
struct Outcome {
    shown: Vec<Option<Duration>>,
    /// From the end of the app's stall until it shows the newest frame.
    back: Option<Duration>,
    longest_broken: Duration,
    in_step: bool,
    sent_again: usize,
}

#[expect(clippy::too_many_lines, reason = "one simulation loop")]
fn run(case: Case, loss: f64, seed: u64, stall: Option<Duration>) -> Outcome {
    let start = Instant::now();
    let one_way = case.rtt / 2;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "bitrates"
    )]
    let (pacing, transported) = ((case.capacity * 1.25) as u64, (case.capacity * 0.7) as u64);
    let mut random = Random(0x9e37_79b9_7f4a_7c15 ^ seed.wrapping_mul(0x2545_f491_4f6c_dd1d));
    let mut flow = Flow::new();
    flow.seat(VIEWER, true, false, start);
    flow.rate(VIEWER, transported);
    flow.keyframe_asked(start);
    let mut encoder = Encoder {
        repair: Some(Ask::Keyframe),
        ..Encoder::default()
    };
    let mut packer = Packer::new(1);
    let mut unpacker = Unpacker::new(1);
    let mut made: HashMap<u32, Made> = HashMap::new();
    let mut picture = Picture::default();
    let mut events: VecDeque<_> = script(case).into();
    let mut tokens = BURST;
    let mut free_at = start;
    let mut wire: VecDeque<(Instant, Bytes)> = VecDeque::new();
    let mut queued: VecDeque<Bytes> = VecDeque::new();
    let mut pending: VecDeque<Bytes> = VecDeque::new();
    let mut pipe: VecDeque<Bytes> = VecDeque::new();
    let resumed = stall.map(|from| start + from + STALL);
    let mut back = None;
    let mut reports: VecDeque<(Instant, Bytes)> = VecDeque::new();
    let mut acks: VecDeque<(Instant, u32)> = VecDeque::new();
    let mut sent: HashSet<(u16, u16)> = HashSet::new();
    let mut sent_again = 0;
    let mut changes: Vec<(Instant, u32)> = Vec::new();
    let mut shown: Vec<Option<Duration>> = Vec::new();
    let mut broken_since: Option<Instant> = None;
    let mut longest_broken = Duration::ZERO;
    let mut ever_shown = false;
    let steps = u32::try_from(LENGTH.as_micros() / STEP.as_micros()).unwrap();
    for step in 0..steps {
        let now = start + STEP * step;
        let rtt = case.rtt + free_at.saturating_duration_since(now);
        let mut told = Vec::new();

        // The stream: its live tick, the screen, the encoder.
        if step % LIVE_EVERY == 0 {
            flow.steer(Some(VIEWER), now);
            assert_eq!(flow.hopeless(now), Vec::<String>::new(), "{}", case.name);
            encoder.repair = flow.ask(now, |_| 0).or(encoder.repair);
            encoder.held.extend(flow.held());
        }
        let mut change = None;
        let mut whole = false;
        while let Some((_, bytes, all)) = events.pop_front_if(|(at, ..)| start + *at <= now) {
            *change.get_or_insert(0) += bytes;
            whole |= all;
        }
        if let Some((id, frame)) = encoder.frame(change, case.screen) {
            if whole {
                changes.push((now, id));
                shown.push(None);
            }
            let delivery = flow.frame(id, frame.kind, frame.mark, frame.bytes, now, |_| 0);
            encoder.repair = delivery.ask.or(encoder.repair);
            if delivery.to.iter().any(|viewer| viewer == VIEWER) {
                let message = Frame {
                    id,
                    codec: Codec::H264,
                    kind: frame.kind,
                    region: None,
                    data: Bytes::from(vec![0; frame.bytes]),
                }
                .encode();
                let link = Link {
                    rtt,
                    pacing,
                    ahead: 0,
                };
                packer.pack(&message, DATAGRAM, link, now);
            }
            made.insert(id, frame);
        }

        // The host's send task: the pacer takes shards as its rate allows, into the bottleneck.
        #[expect(clippy::cast_precision_loss, reason = "bitrates")]
        let earned = pacing as f64 * STEP.as_secs_f64() / 8.0;
        tokens = (tokens + earned).min(BURST);
        while let Some(shard) = packer.next(now) {
            #[expect(clippy::cast_precision_loss, reason = "packet sizes")]
            let bytes = (shard.len() + PACKET_BYTES) as f64;
            if tokens < bytes {
                break;
            }
            tokens -= bytes;
            packer.sent(now);
            let key = (
                u16::from_be_bytes([shard[3], shard[4]]),
                u16::from_be_bytes([shard[5], shard[6]]),
            );
            if !sent.insert(key) {
                sent_again += 1;
            }
            if random.next() < loss || free_at.saturating_duration_since(now) > QUEUE {
                continue;
            }
            free_at = free_at.max(now) + Duration::from_secs_f64(bytes * 8.0 / case.capacity);
            wire.push_back((free_at + one_way, shard));
        }
        while let Some((_, report)) = reports.pop_front_if(|(due, _)| *due <= now) {
            packer.report(&report, now, rtt);
        }
        while let Some((_, id)) = acks.pop_front_if(|(due, _)| *due <= now) {
            flow.ack(VIEWER, id, now);
        }
        // Each frame that the viewer cannot have, told to the stream, which repairs at once.
        for id in packer.lost() {
            flow.lost(VIEWER, id, now);
            encoder.repair = flow.ask(now, |_| 0).or(encoder.repair);
        }

        // The viewer: its runtime puts frames together and waits for room in the pipe to the app,
        // which acknowledges and decodes each frame that it takes.
        while let Some((_, shard)) = wire.pop_front_if(|(due, _)| *due <= now) {
            if queued.len() < FLOW_SHARDS {
                queued.push_back(shard);
            }
        }
        let room = |pending: &mut VecDeque<Bytes>, pipe: &mut VecDeque<Bytes>| {
            while pipe.len() < PIPE
                && let Some(frame) = pending.pop_front()
            {
                pipe.push_back(frame);
            }
        };
        room(&mut pending, &mut pipe);
        while pending.is_empty()
            && let Some(shard) = queued.pop_front()
        {
            pending.extend(unpacker.shard(&shard, now));
            room(&mut pending, &mut pipe);
        }
        if pending.is_empty() && step % RECEIVE_EVERY == 0 {
            let (handed, report) = unpacker.tick(now, rtt, DATAGRAM);
            pending.extend(handed);
            room(&mut pending, &mut pipe);
            if let Some(report) = report
                && random.next() >= loss
            {
                reports.push_back((now + one_way, report));
            }
        }
        let stalled = stall.is_some_and(|from| (start + from..start + from + STALL).contains(&now));
        if !stalled {
            told.extend(std::mem::take(&mut pipe));
        }
        for message in told {
            let id = media::frame_id(&message).unwrap();
            picture.take(id, made[&id]);
            acks.push_back((now + one_way, id));
        }
        if picture.holds == Holds::Stream {
            ever_shown = true;
            if let Some(since) = broken_since.take() {
                longest_broken = longest_broken.max(now - since);
            }
            for ((at, id), shown) in changes.iter().zip(&mut shown) {
                if shown.is_none() && picture.last.is_some_and(|last| last >= *id) {
                    *shown = Some(now - *at);
                }
            }
        } else if ever_shown {
            broken_since.get_or_insert(now);
        }
        if resumed.is_some_and(|resumed| now >= resumed)
            && back.is_none()
            && picture.holds == Holds::Stream
            && picture.last == Some(encoder.newest)
        {
            back = resumed.map(|resumed| now - resumed);
        }
    }
    if let Some(since) = broken_since {
        longest_broken = longest_broken.max(start + LENGTH - since);
    }
    Outcome {
        shown,
        back,
        longest_broken,
        in_step: picture.holds == Holds::Stream && picture.last == Some(encoder.newest),
        sent_again,
    }
}

/// Without repairs that follow the transport, whole new screens on these links stopped the
/// picture for good, and lost frames left it stopped. A viewer app that stops taking frames for a
/// while has the picture back soon after it goes on.
#[test]
fn the_viewer_has_a_picture_again_soon_after_whole_new_screens_lost_frames_and_a_stall() {
    let ms = Duration::from_millis;
    let cases = [
        Case {
            name: "home Wi-Fi, 100 Mbit/s, 4 ms",
            capacity: 100e6,
            rtt: ms(4),
            screen: 150_000,
        },
        Case {
            name: "fibre, 20 Mbit/s, 15 ms",
            capacity: 20e6,
            rtt: ms(15),
            screen: 150_000,
        },
        Case {
            name: "weak Wi-Fi, 8 Mbit/s, 8 ms",
            capacity: 8e6,
            rtt: ms(8),
            screen: 60_000,
        },
        Case {
            name: "far relay, 10 Mbit/s, 120 ms",
            capacity: 10e6,
            rtt: ms(120),
            screen: 150_000,
        },
        Case {
            name: "slow link, 1 Mbit/s, 50 ms",
            capacity: 1e6,
            rtt: ms(50),
            screen: 30_000,
        },
    ];
    // A long round trip lets the most frames reach an app before the host takes it as late.
    let satellite = Case {
        name: "satellite, 10 Mbit/s, 300 ms",
        capacity: 10e6,
        rtt: ms(300),
        screen: 150_000,
    };
    let mut faults = Vec::new();
    for case in cases.into_iter().chain([satellite]) {
        for loss in [0.0, 0.02] {
            let outcome = run(case, loss, 1, Some(Duration::from_millis(10_600)));
            let back = outcome.back.is_some_and(|back| back <= BOUND);
            if !back || outcome.longest_broken > BOUND || !outcome.in_step {
                faults.push(format!(
                    "{}, loss {loss}, app still for {STALL:?}: {outcome:?}",
                    case.name
                ));
            }
        }
    }
    for case in cases {
        for loss in [0.0, 0.02, 0.05] {
            for seed in 1..=2 {
                let outcome = run(case, loss, seed, None);
                let late = outcome
                    .shown
                    .iter()
                    .any(|shown| shown.is_none_or(|shown| shown > BOUND));
                let wasteful = loss == 0.0 && outcome.sent_again > 0;
                if late || outcome.longest_broken > BOUND || !outcome.in_step || wasteful {
                    faults.push(format!(
                        "{}, loss {loss}, seed {seed}: {outcome:?}",
                        case.name
                    ));
                }
            }
        }
    }
    assert!(faults.is_empty(), "{}", faults.join("\n"));
}
