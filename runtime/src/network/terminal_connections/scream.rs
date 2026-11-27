//! `SCReAMv2` (draft-ietf-ccwg-rfc8298bis-screamv2-03) as the congestion controller of the QUIC
//! connections between devices. It keeps the queue it builds short, which is what interactive video
//! needs, and tells the video senders the bitrate that the path carries.
//!
//! Queue delay comes from round trips: each acknowledged packet's round trip minus the base round
//! trip, the least one in the last half minute. Round trips need no clock in common, so the draft's
//! clock drift remedy (section 4.5.1) does not apply: it would take a standing queue for a longer
//! route. Classic ECN marks count as congestion; L4S marking needs per-acknowledgement mark counts
//! that noq does not report.
//!
//! The draft also counts a loss as congestion once the loss rate passes a threshold, with or
//! without a queue. A radio that loses packets by itself, or a policer that drops what exceeds its
//! rate, would then hold the window at its floor, although the video's parity and resends repair
//! such loss. Here only a loss while a queue builds takes the draft's cut. Without a queue, the
//! window gives up only the share that the path loses beyond what repair carries: a radio that
//! loses little costs nothing, and behind a policer, where that share is the overshoot, the window
//! settles just above the policer's rate. Such a cut does not slow the growth that follows, as a
//! cut for congestion does: it says what repair costs, not that the window reached what the path
//! carries, and growth that slows after each one would hold a long path far below what it carries.
//!
//! A screen sends a burst and is then still, and even a video source that sends at its bitrate
//! runs dry after each frame, as the pacer sends faster than that bitrate: noq then tells the
//! controller that it has nothing to send at most acknowledgements. So the window grows by every
//! acknowledged byte, as in the draft, and only the draft's bound on bytes in flight keeps it near
//! what the source showed: a still screen sends nothing and grows nothing. While noq has nothing to
//! send, the round trip that the bitrate is published with falls only slowly, as in libwebrtc's
//! `SCReAM` and the draft's section 6: a burst's acknowledgements come in as the queue that it
//! built drains, and the window was measured against that queue, so a round trip that falls back
//! at once would publish more than the path carried.

use std::{
    any::Any,
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use noq::congestion::{Controller, ControllerFactory, ControllerMetrics};
use noq_proto::RttEstimator;

const QDELAY_TARGET_LO: f64 = 0.03;
const QDELAY_TARGET_HI: f64 = 0.4;
const QDELAY_AVG_G: f64 = 1.0 / 4.0;
const QDELAY_SHORT_AVG_G: f64 = 1.0 / 5.0;
const QDELAY_LONG_AVG_G: f64 = 1.0 / 50.0;
const LATENCY_DIFF_MARGIN_1: f64 = 0.001;
const LATENCY_DIFF_MARGIN_2: f64 = 0.01;
const LATENCY_DIFF_UP_GAIN: f64 = 20.0;
const LATENCY_DIFF_DOWN_GAIN: f64 = 2.0;
const MIN_REF_WND: f64 = 3000.0;
const BYTES_IN_FLIGHT_HEAD_ROOM: f64 = 1.5;
const BETA_LOSS: f64 = 0.7;
const BETA_ECN: f64 = 0.8;
const POST_CONGESTION_DELAY_RTTS: f64 = 100.0;
const MUL_INCREASE_FACTOR: f64 = 0.02;
const VIRTUAL_RTT: f64 = 0.025;
const REF_WND_I_UPDATE_MAX_HOLD_RTTS: f64 = 50.0;
const REF_WND_OVERHEAD_MIN: f64 = 1.5;
const REF_WND_OVERHEAD_MAX: f64 = 3.0;
const RATE_PACE_MIN: f64 = 50_000.0;
const PACKET_PACING_HEADROOM: f64 = 1.5;
const MAX_RELAXED_PACING_FACTOR: f64 = 4.0;
const RELAXED_PACING_LIMIT_LOW: f64 = 0.8;
const PACKET_OVERHEAD: f64 = 20.0;
const LOSS_RATE_GAIN: f64 = 0.0025;
/// The share of packets that a path may lose without a queue at no cost to the window: the
/// video's parity and resends repair it.
const LOSS_TOLERATED: f64 = 0.02;
const ACKED_BITRATE_MARGIN: f64 = 0.8;
const BACKOFF_SCALE_LOW_TARGET_RATE: f64 = 0.25;
/// The bitrate that all video on one connection may reach, which bounds how far the window grows
/// while the encoders send less than the path carries.
const TARGET_BITRATE_MAX: f64 = 100_000_000.0;
/// The first reference window: QUIC's usual initial window of ten packets.
const FIRST_PACKETS: f64 = 10.0;
/// Packets that the pacer may send at once.
const QUANTUM_PACKETS: u64 = 4;
/// Base round trips are the least of each ten seconds, for half a minute, so a longer route takes
/// over within half a minute.
const BASE_SPAN: Duration = Duration::from_secs(10);
const BASE_SPANS: usize = 3;
/// Queue delay samples for the competing-flows estimate (draft section 4.5.4).
const NORM_EVERY: Duration = Duration::from_millis(50);
const NORM_KEPT: usize = 200;
const NORM_AVERAGED: usize = 50;
const SMALLEST_RTT: f64 = 0.001;
/// The share of the way to the path's smoothed round trip that the published one moves down at
/// each acknowledgement while the connection has nothing to send.
const IDLE_RTT_GAIN: f64 = 1.0 / 128.0;

/// Builds one controller for each path of each connection.
#[derive(Debug, Default)]
pub(crate) struct Scream;

impl ControllerFactory for Scream {
    fn build(self: Arc<Self>, now: Instant, current_mtu: u16) -> Box<dyn Controller> {
        Box::new(Window::new(now, current_mtu))
    }
}

/// What a path's controller tells the video senders, in bits per second: the bitrate that the
/// path carries for video before each sender takes off what its own queue and frame sizes need
/// (draft section 4.4), and the rate at which the pacer sends. Shared by a controller and its
/// clones, so a sender reads the live values from the clone that `Connection::congestion_state`
/// returns.
#[derive(Debug, Default)]
pub(crate) struct Rates {
    carried: AtomicU64,
    pacing: AtomicU64,
}

impl Rates {
    #[cfg(any(feature = "host", test))]
    pub(crate) fn carried(&self) -> u64 {
        self.carried.load(Ordering::Relaxed)
    }

    #[cfg(feature = "host")]
    pub(crate) fn pacing(&self) -> u64 {
        self.pacing.load(Ordering::Relaxed)
    }
}

/// Finds the rates that a controller built by [`Scream`] publishes.
#[cfg(feature = "host")]
pub(crate) fn rates(controller: Box<dyn Controller>) -> Option<Arc<Rates>> {
    controller
        .into_any()
        .downcast::<Window>()
        .ok()
        .map(|window| window.rates)
}

/// Congestion signals since the window last reacted.
#[derive(Debug, Clone, Copy, Default)]
struct Seen {
    loss: bool,
    ecn: bool,
}

/// How the window answers the congestion signals of a round trip.
#[derive(Debug, Clone, Copy)]
enum Reaction {
    /// A loss while a queue builds: the queue overflowed.
    Overflow,
    /// Losses without a queue, more than the tolerated share: the window gives up the share
    /// beyond it.
    PathLoss(f64),
    Ecn,
    /// A queue that grows beyond half its target: the share of the way to halving the window.
    Queue(f64),
}

#[derive(Debug, Clone)]
struct Window {
    rates: Arc<Rates>,
    mss: f64,
    ref_wnd: f64,
    ref_wnd_i: f64,
    ref_wnd_i_update_allowed: bool,
    ref_wnd_i_update_time: Option<Instant>,
    ref_wnd_lowest: f64,
    ref_wnd_overhead: f64,
    /// The round trip for the published bitrate and the reactions, from `path_rtt`, noq's
    /// smoothed one, from its first sample on.
    s_rtt: f64,
    path_rtt: Option<f64>,
    /// noq had nothing to send when it last could.
    idle: bool,
    base: VecDeque<(Instant, f64)>,
    qdelay: f64,
    qdelay_avg: f64,
    qdelay_avg_prev: f64,
    qdelay_avg_at: Option<Instant>,
    qdelay_target: f64,
    qdelay_short_avg: f64,
    qdelay_long_avg: f64,
    latency_diff_avg: f64,
    norms: VecDeque<f64>,
    norm_at: Option<Instant>,
    loss_rate: f64,
    loss_event_rate: f64,
    seen: Seen,
    congested: bool,
    last_congestion_detected: Option<Instant>,
    last_reaction: Option<Instant>,
    bytes_newly_acked: f64,
    bytes_in_flight: f64,
    bytes_in_flight_prev: f64,
    max_bytes_in_flight: f64,
    max_bytes_in_flight_prev: f64,
    round_at: Instant,
    acked: VecDeque<(Instant, f64)>,
    target_bitrate: f64,
}

impl Window {
    fn new(now: Instant, mtu: u16) -> Self {
        let mss = f64::from(mtu);
        let ref_wnd = (FIRST_PACKETS * mss).max(MIN_REF_WND);
        Self {
            rates: Arc::default(),
            mss,
            ref_wnd,
            ref_wnd_i: 1.0,
            ref_wnd_i_update_allowed: true,
            ref_wnd_i_update_time: None,
            ref_wnd_lowest: ref_wnd,
            ref_wnd_overhead: REF_WND_OVERHEAD_MAX,
            s_rtt: 0.1,
            path_rtt: None,
            idle: false,
            base: VecDeque::new(),
            qdelay: 0.0,
            qdelay_avg: 0.0,
            qdelay_avg_prev: 0.0,
            qdelay_avg_at: None,
            qdelay_target: QDELAY_TARGET_LO,
            qdelay_short_avg: 0.0,
            qdelay_long_avg: 0.0,
            latency_diff_avg: 0.0,
            norms: VecDeque::new(),
            norm_at: None,
            loss_rate: 0.0,
            loss_event_rate: 0.0,
            seen: Seen::default(),
            congested: false,
            last_congestion_detected: None,
            last_reaction: None,
            bytes_newly_acked: 0.0,
            bytes_in_flight: 0.0,
            bytes_in_flight_prev: 0.0,
            max_bytes_in_flight: 0.0,
            max_bytes_in_flight_prev: 0.0,
            round_at: now,
            acked: VecDeque::new(),
            target_bitrate: 0.0,
        }
    }

    fn since(earlier: Option<Instant>, now: Instant) -> f64 {
        earlier.map_or(f64::MAX, |earlier| {
            now.saturating_duration_since(earlier).as_secs_f64()
        })
    }

    fn base_rtt(&self) -> Option<f64> {
        self.base.iter().map(|(_, rtt)| *rtt).reduce(f64::min)
    }

    fn sample_base(&mut self, now: Instant, rtt: f64) {
        match self.base.back_mut() {
            Some((started, least)) if now.saturating_duration_since(*started) < BASE_SPAN => {
                *least = least.min(rtt);
            }
            _ => {
                self.base.push_back((now, rtt));
                if self.base.len() > BASE_SPANS {
                    self.base.pop_front();
                }
            }
        }
    }

    fn ref_wnd_ratio(&self) -> f64 {
        (self.mss / self.ref_wnd).min(1.0)
    }

    fn loss_rate_gain(&self) -> f64 {
        LOSS_RATE_GAIN.min(self.mss / self.ref_wnd / 2.0)
    }

    /// The acknowledged bitrate over the last two round trips.
    fn acked_bitrate(&mut self, now: Instant) -> f64 {
        let span = Duration::from_secs_f64((2.0 * self.s_rtt).max(0.04));
        while self
            .acked
            .front()
            .is_some_and(|(at, _)| now.saturating_duration_since(*at) > span)
        {
            self.acked.pop_front();
        }
        let bytes: f64 = self.acked.iter().map(|(_, bytes)| bytes).sum();
        8.0 * bytes / span.as_secs_f64()
    }

    fn update_qdelay_avg(&mut self, now: Instant) {
        if Self::since(self.qdelay_avg_at, now) < VIRTUAL_RTT.min(self.s_rtt) {
            return;
        }
        self.qdelay_avg = if self.qdelay < self.qdelay_avg {
            self.qdelay
        } else {
            QDELAY_AVG_G.mul_add(self.qdelay, (1.0 - QDELAY_AVG_G) * self.qdelay_avg)
        };
        self.update_latency_diff_avg();
        self.qdelay_avg_at = Some(now);
    }

    /// How much the queue delay varies, so that jitter from scheduling and link retransmissions
    /// slows growth instead of cutting the window (draft section 4.2.1.4).
    fn update_latency_diff_avg(&mut self) {
        self.qdelay_short_avg = self.qdelay.mul_add(
            QDELAY_SHORT_AVG_G,
            self.qdelay_short_avg * (1.0 - QDELAY_SHORT_AVG_G),
        );
        self.qdelay_long_avg = self.qdelay.mul_add(
            QDELAY_LONG_AVG_G,
            self.qdelay_long_avg * (1.0 - QDELAY_LONG_AVG_G),
        );
        if self.congested {
            let diff = (self.qdelay_short_avg - self.qdelay_long_avg - LATENCY_DIFF_MARGIN_1)
                .clamp(-LATENCY_DIFF_MARGIN_1, LATENCY_DIFF_MARGIN_1);
            if diff > 0.0 {
                let low = (self.qdelay_avg / LATENCY_DIFF_MARGIN_2).min(1.0);
                self.latency_diff_avg =
                    (LATENCY_DIFF_UP_GAIN * diff).mul_add(low, self.latency_diff_avg);
            } else {
                self.latency_diff_avg = LATENCY_DIFF_DOWN_GAIN.mul_add(diff, self.latency_diff_avg);
            }
            self.latency_diff_avg = self.latency_diff_avg.clamp(0.0, 1.0);
        } else {
            self.qdelay_long_avg = self.qdelay_short_avg;
        }
    }

    /// Raises the queue delay target while loss-based flows share the bottleneck, so video keeps a
    /// share of it (draft section 4.5.4).
    fn adjust_qdelay_target(&mut self, now: Instant) {
        if self
            .norm_at
            .is_some_and(|at| now.saturating_duration_since(at) < NORM_EVERY)
        {
            return;
        }
        self.norm_at = Some(now);
        self.norms.push_back(self.qdelay / QDELAY_TARGET_LO);
        if self.norms.len() > NORM_KEPT {
            self.norms.pop_front();
        }
        let count = f64::from(u32::try_from(self.norms.len()).unwrap_or(u32::MAX));
        let mean = self.norms.iter().sum::<f64>() / count;
        let variance = self
            .norms
            .iter()
            .map(|norm| (norm - mean).powi(2))
            .sum::<f64>()
            / count;
        let recent = self.norms.iter().rev().take(NORM_AVERAGED);
        let recent_count =
            f64::from(u32::try_from(self.norms.len().min(NORM_AVERAGED)).unwrap_or(u32::MAX));
        let average = recent.sum::<f64>() / recent_count;
        let new_target = (average + variance.sqrt()) * QDELAY_TARGET_LO;
        if self.loss_event_rate > 0.002 {
            self.qdelay_target = 1.5 * new_target;
        } else if variance < 0.2 {
            self.qdelay_target = new_target;
        } else if new_target < QDELAY_TARGET_LO {
            self.qdelay_target = (self.qdelay_target * 0.5).max(new_target);
        } else {
            self.qdelay_target *= 0.9;
        }
        self.qdelay_target = self.qdelay_target.clamp(QDELAY_TARGET_LO, QDELAY_TARGET_HI);
    }

    fn react(&mut self, now: Instant) {
        let latency_scale = 1.0 - self.latency_diff_avg;
        let mut scl = ((self.ref_wnd - self.ref_wnd_i) / self.ref_wnd_i * 8.0).powi(2);
        scl = scl.clamp(0.2, 1.0);
        if self.ref_wnd > self.ref_wnd_i && scl < 1.0 {
            scl = (scl * latency_scale).max(0.2);
        }
        let mut reaction = None;
        if Self::since(self.last_reaction, now) >= self.s_rtt {
            let loss_beyond = self.loss_rate - LOSS_TOLERATED;
            reaction = if self.seen.loss && self.qdelay_avg > self.qdelay_target / 4.0 {
                Some(Reaction::Overflow)
            } else if self.seen.loss && loss_beyond > 0.0 {
                Some(Reaction::PathLoss(loss_beyond.min(1.0 - BETA_LOSS)))
            } else if self.seen.ecn {
                Some(Reaction::Ecn)
            } else if self.qdelay_avg > self.qdelay_target / 2.0
                && self.qdelay_avg >= self.qdelay_avg_prev
            {
                Some(Reaction::Queue(
                    ((self.qdelay_avg - self.qdelay_target / 2.0) / (self.qdelay_target / 2.0))
                        .clamp(0.0, 1.0),
                ))
            } else {
                None
            };
            let lost = matches!(reaction, Some(Reaction::Overflow | Reaction::PathLoss(_)));
            self.loss_event_rate =
                0.99f64.mul_add(self.loss_event_rate, if lost { 0.01 } else { 0.0 });
            self.seen = Seen::default();
        }
        self.qdelay_avg_prev = self.qdelay_avg;
        if reaction.is_some() {
            self.congested = true;
            if !matches!(reaction, Some(Reaction::PathLoss(_))) {
                self.last_congestion_detected = Some(now);
            }
            if self.ref_wnd > (self.ref_wnd_lowest + self.mss).max(self.ref_wnd_lowest * 1.05)
                || Self::since(self.ref_wnd_i_update_time, now)
                    > REF_WND_I_UPDATE_MAX_HOLD_RTTS * self.s_rtt
            {
                self.ref_wnd_i_update_allowed = true;
            }
            if self.ref_wnd_i_update_allowed {
                self.ref_wnd_i = self.ref_wnd;
                self.ref_wnd_i_update_allowed = false;
                self.ref_wnd_i_update_time = Some(now);
            }
        }
        let acked_bitrate = self.acked_bitrate(now);
        let soften = |backoff: f64, window: &Self| {
            if window.target_bitrate < acked_bitrate * ACKED_BITRATE_MARGIN {
                if window.bytes_in_flight < window.bytes_in_flight_prev {
                    0.0
                } else {
                    backoff * BACKOFF_SCALE_LOW_TARGET_RATE
                }
            } else {
                backoff
            }
        };
        match reaction {
            Some(Reaction::Overflow) => self.ref_wnd *= BETA_LOSS,
            Some(Reaction::PathLoss(share)) => self.ref_wnd *= 1.0 - share,
            Some(Reaction::Ecn) => self.ref_wnd *= BETA_ECN,
            Some(Reaction::Queue(alpha)) => self.ref_wnd *= 1.0 - soften(alpha / 2.0, self),
            None => {}
        }
        self.ref_wnd = self.ref_wnd.max(MIN_REF_WND);
        self.ref_wnd_lowest = self.ref_wnd;
        if reaction.is_some() {
            self.last_reaction = Some(now);
        }
        self.grow(now, scl, latency_scale);
    }

    /// Grows the window by the bytes acknowledged without marks, more while no congestion has been
    /// seen for a while and less near the last window that met congestion (draft section 4.2.2.2).
    fn grow(&mut self, now: Instant, scl: f64, latency_scale: f64) {
        let post_congestion = (Self::since(self.last_congestion_detected, now)
            / (POST_CONGESTION_DELAY_RTTS * VIRTUAL_RTT.max(self.s_rtt)))
        .clamp(0.0, 1.0)
            * latency_scale;
        let scale_factor = 1.0 + MUL_INCREASE_FACTOR * self.ref_wnd / self.mss;
        let increment_scale = (-3.0f64).mul_add(latency_scale, 4.0);
        let mut increment = self.bytes_newly_acked * self.ref_wnd_ratio() / increment_scale;
        increment *= (self.s_rtt / VIRTUAL_RTT).min(1.0);
        increment *= scl;
        let multiplier = if scale_factor > 1.0 {
            (scale_factor - 1.0).mul_add(post_congestion, 1.0)
        } else {
            scale_factor
        };
        increment *= multiplier;
        let allowed = self
            .max_bytes_in_flight
            .max(self.max_bytes_in_flight_prev)
            .mul_add(BYTES_IN_FLIGHT_HEAD_ROOM, self.mss);
        let grown = self.ref_wnd + increment;
        if grown <= allowed && self.target_bitrate < TARGET_BITRATE_MAX {
            self.ref_wnd = grown;
        }
        self.bytes_newly_acked = 0.0;
        self.ref_wnd_overhead = (REF_WND_OVERHEAD_MAX - REF_WND_OVERHEAD_MIN)
            .mul_add(latency_scale, REF_WND_OVERHEAD_MIN);
    }

    fn publish(&mut self) {
        let small_window = 1.0 - (self.ref_wnd_ratio() - 0.1).clamp(0.0, 0.2);
        let packets = self.mss / (self.mss + PACKET_OVERHEAD);
        let carried = small_window * packets * 8.0 * self.ref_wnd / self.s_rtt;
        self.target_bitrate = (carried / 1.2).min(TARGET_BITRATE_MAX);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "finite positive bitrates"
        )]
        let (carried, pacing) = (
            carried.min(TARGET_BITRATE_MAX * 1.2) as u64,
            self.pace_bitrate() as u64,
        );
        self.rates.carried.store(carried, Ordering::Relaxed);
        self.rates.pacing.store(pacing, Ordering::Relaxed);
    }

    fn pace_bitrate(&self) -> f64 {
        let pace = self.target_bitrate.max(RATE_PACE_MIN) * PACKET_PACING_HEADROOM;
        let nominal = self.target_bitrate / TARGET_BITRATE_MAX;
        let relaxed =
            ((nominal - RELAXED_PACING_LIMIT_LOW) / (1.0 - RELAXED_PACING_LIMIT_LOW)).min(1.0);
        let scale = (1.0 - relaxed).clamp(1.0 / MAX_RELAXED_PACING_FACTOR, 1.0);
        pace / scale
    }

    fn acked_packet(&mut self, now: Instant, sent: Instant, bytes: u64, smoothed: Duration) {
        #[expect(clippy::cast_precision_loss, reason = "packet sizes are small")]
        let bytes = bytes as f64;
        let round = now.saturating_duration_since(sent).as_secs_f64();
        let path_rtt = smoothed.as_secs_f64().max(SMALLEST_RTT);
        if self.path_rtt.replace(path_rtt).is_none() {
            self.s_rtt = path_rtt;
        }
        self.sample_base(now, round);
        self.qdelay = (round - self.base_rtt().unwrap_or(round)).max(0.0);
        self.bytes_newly_acked += bytes;
        self.acked.push_back((now, bytes));
        self.loss_rate *= 1.0 - self.loss_rate_gain();
    }

    /// Follows the path's round trip, except that it falls only slowly while the connection has
    /// nothing to send.
    fn follow_rtt(&mut self) {
        let Some(path) = self.path_rtt else {
            return;
        };
        let gain = if self.idle && path < self.s_rtt {
            IDLE_RTT_GAIN
        } else {
            1.0
        };
        self.s_rtt = (path - self.s_rtt).mul_add(gain, self.s_rtt);
    }

    fn turn_round(&mut self, now: Instant) {
        if now.saturating_duration_since(self.round_at).as_secs_f64() < self.s_rtt {
            return;
        }
        self.round_at = now;
        self.max_bytes_in_flight_prev = self.max_bytes_in_flight;
        self.max_bytes_in_flight = self.bytes_in_flight;
    }
}

impl Controller for Window {
    fn on_packet_sent(&mut self, _now: Instant, bytes: u16, _pn: u64) {
        self.bytes_in_flight += f64::from(bytes);
        self.max_bytes_in_flight = self.max_bytes_in_flight.max(self.bytes_in_flight);
    }

    fn on_ack(
        &mut self,
        now: Instant,
        sent: Instant,
        bytes: u64,
        _pn: u64,
        _app_limited: bool,
        rtt: &RttEstimator,
    ) {
        self.acked_packet(now, sent, bytes, rtt.get());
    }

    fn on_end_acks(
        &mut self,
        now: Instant,
        in_flight: u64,
        app_limited: bool,
        _largest_packet_num_acked: Option<u64>,
    ) {
        self.idle = app_limited;
        self.follow_rtt();
        self.bytes_in_flight_prev = self.bytes_in_flight;
        #[expect(clippy::cast_precision_loss, reason = "bytes in flight are small")]
        let in_flight = in_flight as f64;
        self.bytes_in_flight = in_flight;
        self.update_qdelay_avg(now);
        self.adjust_qdelay_target(now);
        self.react(now);
        self.publish();
        self.turn_round(now);
    }

    fn on_congestion_event(
        &mut self,
        _now: Instant,
        _sent: Instant,
        is_persistent_congestion: bool,
        is_ecn: bool,
        _lost_bytes: u64,
        _largest_lost_pn: u64,
    ) {
        if is_ecn {
            self.seen.ecn = true;
        } else {
            self.seen.loss = true;
        }
        if is_persistent_congestion {
            self.ref_wnd = MIN_REF_WND;
            self.publish();
        }
    }

    fn on_packet_lost(&mut self, lost_bytes: u16, _pn: u64, _now: Instant) {
        let gain = self.loss_rate_gain();
        self.loss_rate = (1.0 - gain).mul_add(self.loss_rate, gain);
        self.bytes_in_flight = (self.bytes_in_flight - f64::from(lost_bytes)).max(0.0);
    }

    fn on_mtu_update(&mut self, new_mtu: u16) {
        self.mss = f64::from(new_mtu);
    }

    fn window(&self) -> u64 {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a finite positive window"
        )]
        let window = (self.ref_wnd * self.ref_wnd_overhead).max(2.0 * self.mss) as u64;
        window
    }

    fn metrics(&self) -> ControllerMetrics {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "finite positive rates and sizes"
        )]
        let (pacing, bandwidth, mss) = (
            (self.pace_bitrate() / 8.0) as u64,
            self.target_bitrate as u64,
            self.mss as u64,
        );
        let mut metrics = ControllerMetrics::default();
        metrics.congestion_window = self.window();
        metrics.pacing_rate = Some(pacing);
        metrics.bandwidth_estimate = Some(bandwidth);
        metrics.send_quantum = Some(QUANTUM_PACKETS * mss);
        metrics
    }

    fn clone_box(&self) -> Box<dyn Controller> {
        Box::new(self.clone())
    }

    fn initial_window(&self) -> u64 {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a finite positive window"
        )]
        let window = ((FIRST_PACKETS * self.mss).max(MIN_REF_WND) * REF_WND_OVERHEAD_MAX) as u64;
        window
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MTU: u16 = 1200;
    const STEP: Duration = Duration::from_millis(1);

    fn bits(rate: u64) -> f64 {
        f64::from(u32::try_from(rate).unwrap())
    }

    /// A video sender behind noq, through a bottleneck that serves `capacity` bits per second
    /// from one queue, on a path with a round trip without queue of 20 ms or the one that a test
    /// sets. The source makes a frame each sixtieth of a second at the published bitrate, or a
    /// burst of several frame times at once. noq sends what waits at the published pacing rate
    /// with its own pacer and tells the controller that it had nothing to send whenever nothing
    /// waited and neither the window nor the pacer held a packet back.
    ///
    /// A test can also add a radio after the bottleneck that loses packets at random, a policer
    /// before it that drops what exceeds its rate, and a limit on the time that a packet may wait
    /// in the queue. As in noq, the controller hears of a lost packet once a packet sent three
    /// later is acknowledged, or a later one is and the lost one left 9/8 of a round trip ago.
    struct Path {
        window: Window,
        start: Instant,
        capacity: f64,
        round_trip: Duration,
        radio_loss: f64,
        policer: Option<Policer>,
        queue_limit: Option<Duration>,
        random: u64,
        waiting: VecDeque<u16>,
        next_frame: Duration,
        pacer: Pacer,
        app_limited: bool,
        served_until: Instant,
        flying: VecDeque<Flight>,
        in_flight: u64,
        sent_packets: u64,
        largest_acked: Option<u64>,
        smoothed: f64,
        sent_bits: f64,
        measured: Measured,
    }

    struct Flight {
        number: u64,
        sent: Instant,
        bytes: u16,
        /// `None` for a packet that the path dropped.
        acked: Option<Instant>,
    }

    /// noq's pacer for a controller that sets a pacing rate: a bucket filled at that rate, which
    /// holds 2 ms of it, at least 10 and at most 256 packets, and never more than 10 ms of it.
    struct Pacer {
        tokens: f64,
        at: Option<Instant>,
    }

    impl Pacer {
        /// Takes a packet of `bytes` when the bucket has room for a whole one, at `rate` bytes
        /// per second.
        fn take(&mut self, rate: f64, bytes: u16, now: Instant) -> bool {
            let packet = f64::from(MTU);
            let capacity = (rate * 0.002)
                .clamp(10.0 * packet, 256.0 * packet)
                .min((rate * 0.01).max(packet));
            let filled = self.at.map_or(f64::MAX, |at| {
                rate * now.saturating_duration_since(at).as_secs_f64()
            });
            self.tokens = (self.tokens + filled).min(capacity);
            self.at = Some(now);
            let takes = self.tokens >= packet.min(capacity);
            if takes {
                self.tokens = (self.tokens - f64::from(bytes)).max(0.0);
            }
            takes
        }
    }

    /// Drops each packet for which its bucket, filled at `rate` bits per second up to 20 ms of
    /// that rate, has no room.
    struct Policer {
        rate: f64,
        tokens: f64,
        at: Option<Instant>,
    }

    impl Policer {
        fn passes(&mut self, bytes: u16, now: Instant) -> bool {
            let filled = self.at.map_or(f64::MAX, |at| {
                self.rate / 8.0 * now.saturating_duration_since(at).as_secs_f64()
            });
            self.tokens = (self.tokens + filled).min(self.rate / 8.0 * 0.02);
            self.at = Some(now);
            let passes = self.tokens >= f64::from(bytes);
            if passes {
                self.tokens -= f64::from(bytes);
            }
            passes
        }
    }

    /// What the path did from `from` on.
    #[derive(Default)]
    struct Measured {
        from: Duration,
        published_sum: f64,
        samples: u32,
        published_least: Option<f64>,
        sent: u32,
        lost: u32,
        queued_most: Duration,
    }

    impl Measured {
        fn published_mean(&self) -> f64 {
            self.published_sum / f64::from(self.samples)
        }

        fn published_least(&self) -> f64 {
            self.published_least.unwrap()
        }

        fn lost_share(&self) -> f64 {
            f64::from(self.lost) / f64::from(self.sent)
        }
    }

    impl Path {
        fn new(capacity: f64) -> Self {
            let start = Instant::now();
            Self {
                window: Window::new(start, MTU),
                start,
                capacity,
                round_trip: Duration::ZERO,
                radio_loss: 0.0,
                policer: None,
                queue_limit: None,
                random: 0x9e37_79b9_7f4a_7c15,
                waiting: VecDeque::new(),
                next_frame: Duration::ZERO,
                pacer: Pacer {
                    tokens: 0.0,
                    at: None,
                },
                app_limited: true,
                served_until: start,
                flying: VecDeque::new(),
                in_flight: 0,
                sent_packets: 0,
                largest_acked: None,
                smoothed: 0.0,
                sent_bits: 0.0,
                measured: Measured::default(),
            }
            .with_round_trip(Duration::from_millis(20))
        }

        /// The handshake has measured the round trip before the source starts.
        fn with_round_trip(mut self, round_trip: Duration) -> Self {
            self.round_trip = round_trip;
            self.smoothed = round_trip.as_secs_f64();
            self.window.path_rtt = Some(self.smoothed);
            self.window.s_rtt = self.smoothed;
            self.window.publish();
            self
        }

        fn with_radio_loss(mut self, share: f64) -> Self {
            self.radio_loss = share;
            self
        }

        fn with_policer(mut self, rate: f64) -> Self {
            self.policer = Some(Policer {
                rate,
                tokens: 0.0,
                at: None,
            });
            self
        }

        fn with_queue_limit(mut self, limit: Duration) -> Self {
            self.queue_limit = Some(limit);
            self
        }

        /// Starts from the window's floor, as after cuts.
        fn at_the_floor(mut self) -> Self {
            self.window.ref_wnd = MIN_REF_WND;
            self.window.ref_wnd_lowest = MIN_REF_WND;
            self.window.publish();
            self
        }

        fn measure_from(&mut self, from: Duration) {
            self.measured = Measured {
                from,
                ..Measured::default()
            };
        }

        fn published(&self) -> f64 {
            bits(self.window.rates.carried()) / 1.2
        }

        /// A fixed pseudo-random sequence (xorshift), evenly between 0 and 1.
        fn chance(&mut self) -> f64 {
            self.random ^= self.random << 13;
            self.random ^= self.random >> 7;
            self.random ^= self.random << 17;
            f64::from(u32::try_from(self.random >> 32).unwrap()) / f64::from(u32::MAX)
        }

        /// The source makes a frame each sixtieth of a second.
        fn run(&mut self, from: Duration, to: Duration) {
            self.next_frame = self.next_frame.max(from);
            self.advance(from, to, true);
        }

        /// The source makes nothing, while what waits still goes.
        fn still(&mut self, from: Duration, to: Duration) {
            self.advance(from, to, false);
        }

        /// The source makes `frame_times` frames' worth of bitrate at once.
        fn burst(&mut self, frame_times: f64) {
            self.make(frame_times * self.published() / 60.0 / 8.0);
        }

        fn make(&mut self, bytes: f64) {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a frame of a few megabytes at most"
            )]
            let bytes = bytes as u64;
            let (full, rest) = (bytes / u64::from(MTU), bytes % u64::from(MTU));
            self.waiting.extend((0..full).map(|_| MTU));
            if rest > 0 {
                self.waiting.push_back(u16::try_from(rest).unwrap());
            }
        }

        fn advance(&mut self, from: Duration, to: Duration, frames: bool) {
            let mut at = from;
            while at < to {
                let now = self.start + at;
                self.acknowledge(now);
                let published = self.published();
                let measuring = at >= self.measured.from;
                if measuring {
                    self.measured.published_sum += published;
                    self.measured.samples += 1;
                    self.measured.published_least = Some(
                        self.measured
                            .published_least
                            .map_or(published, |least| least.min(published)),
                    );
                }
                if frames && at >= self.next_frame {
                    self.make(published / 60.0 / 8.0);
                    self.next_frame += Duration::from_secs(1) / 60;
                }
                self.transmit(now, measuring);
                at += STEP;
            }
        }

        /// Sends what waits, as far as the window and the pacer let it go.
        fn transmit(&mut self, now: Instant, measuring: bool) {
            #[expect(clippy::cast_precision_loss, reason = "rates are small")]
            let rate = self.window.metrics().pacing_rate.unwrap() as f64;
            let mut held = false;
            while let Some(&bytes) = self.waiting.front() {
                if self.in_flight + u64::from(MTU) > self.window.window()
                    || !self.pacer.take(rate, bytes, now)
                {
                    held = true;
                    break;
                }
                self.waiting.pop_front();
                self.window.on_packet_sent(now, bytes, 0);
                self.in_flight += u64::from(bytes);
                self.sent_bits = f64::from(bytes).mul_add(8.0, self.sent_bits);
                let acked = self.carry(now, bytes, measuring);
                if measuring {
                    self.measured.sent += 1;
                    self.measured.lost += u32::from(acked.is_none());
                }
                self.flying.push_back(Flight {
                    number: self.sent_packets,
                    sent: now,
                    bytes,
                    acked,
                });
                self.sent_packets += 1;
            }
            self.app_limited = !held;
        }

        /// When the acknowledgement of a packet sent at `now` arrives, or `None` when the path
        /// drops it.
        fn carry(&mut self, now: Instant, bytes: u16, measuring: bool) -> Option<Instant> {
            let reaches = now + self.round_trip / 4;
            if let Some(policer) = &mut self.policer
                && !policer.passes(bytes, reaches)
            {
                return None;
            }
            if self
                .queue_limit
                .is_some_and(|limit| self.served_until.saturating_duration_since(reaches) > limit)
            {
                return None;
            }
            let served = reaches.max(self.served_until)
                + Duration::from_secs_f64(f64::from(bytes) * 8.0 / self.capacity);
            self.served_until = served;
            if measuring {
                self.measured.queued_most = self
                    .measured
                    .queued_most
                    .max(served.saturating_duration_since(reaches));
            }
            (self.chance() >= self.radio_loss).then(|| served + self.round_trip * 3 / 4)
        }

        fn acknowledge(&mut self, now: Instant) {
            let mut index = 0;
            while let Some(flight) = self.flying.get(index) {
                match flight.acked {
                    None => index += 1,
                    Some(acked) if acked <= now => {
                        let (sent, bytes) = (flight.sent, flight.bytes);
                        self.largest_acked = Some(flight.number);
                        self.flying.remove(index);
                        self.in_flight -= u64::from(bytes);
                        self.smoothed =
                            0.125f64.mul_add((acked - sent).as_secs_f64(), 0.875 * self.smoothed);
                        let smoothed = Duration::from_secs_f64(self.smoothed);
                        self.window
                            .acked_packet(acked, sent, u64::from(bytes), smoothed);
                        self.window
                            .on_end_acks(acked, self.in_flight, self.app_limited, None);
                        self.detect_lost(acked);
                        index = 0;
                    }
                    Some(_) => break,
                }
            }
            self.detect_lost(now);
        }

        /// Every packet before the largest acknowledged one that is still in flight was dropped,
        /// and these come first in `flying`.
        fn detect_lost(&mut self, now: Instant) {
            let Some(largest) = self.largest_acked else {
                return;
            };
            let delay = Duration::from_secs_f64(self.smoothed * 9.0 / 8.0);
            let mut lost: u64 = 0;
            while let Some(flight) = self.flying.front()
                && flight.number < largest
                && (flight.number + 3 <= largest
                    || now.saturating_duration_since(flight.sent) >= delay)
            {
                let bytes = flight.bytes;
                self.flying.pop_front();
                self.in_flight -= u64::from(bytes);
                self.window.on_packet_lost(bytes, 0, now);
                lost += u64::from(bytes);
            }
            if lost > 0 {
                self.window
                    .on_congestion_event(now, now, false, false, lost, 0);
            }
        }
    }

    #[test]
    fn the_bitrate_settles_near_a_bottleneck_with_a_short_queue() {
        let mut path = Path::new(10_000_000.0);
        path.run(Duration::ZERO, Duration::from_secs(10));
        let settled_from = path.sent_bits;
        path.measure_from(Duration::from_secs(15));
        path.run(Duration::from_secs(10), Duration::from_secs(20));
        let sent = (path.sent_bits - settled_from) / 10.0;
        assert!(
            sent > 7_000_000.0,
            "uses most of a 10 Mbps bottleneck: {sent}"
        );
        assert!(
            path.measured.queued_most < Duration::from_millis(60),
            "keeps the queue short: {:?}",
            path.measured.queued_most
        );
    }

    #[test]
    fn a_narrower_bottleneck_lowers_the_bitrate() {
        let mut path = Path::new(10_000_000.0);
        path.run(Duration::ZERO, Duration::from_secs(10));
        path.capacity = 3_000_000.0;
        path.measure_from(Duration::from_secs(15));
        path.run(Duration::from_secs(10), Duration::from_secs(20));
        let published = bits(path.window.rates.carried()) / 1.2;
        assert!(
            published < 4_000_000.0,
            "follows the narrower path: {published}"
        );
        assert!(
            path.measured.queued_most < Duration::from_millis(60),
            "drains the queue: {:?}",
            path.measured.queued_most
        );
    }

    /// A video source at the published bitrate runs dry after each frame, and noq then tells the
    /// controller that it has nothing to send: from the floor, the window still grows to what the
    /// path carries, on a slow link and on a short and a long path.
    #[test]
    fn a_source_at_its_bitrate_grows_from_the_floor_to_what_the_path_carries() {
        for (capacity, round_trip) in [(1e6, 50), (8e6, 20), (20e6, 120)] {
            let mut path = Path::new(capacity)
                .with_round_trip(Duration::from_millis(round_trip))
                .at_the_floor();
            path.measure_from(Duration::from_secs(20));
            path.run(Duration::ZERO, Duration::from_secs(40));
            let mean = path.measured.published_mean();
            assert!(
                mean > 0.8 * capacity,
                "{} Mbit/s, {round_trip} ms: {mean}",
                capacity / 1e6
            );
        }
    }

    /// A screen sends a frame of a few frame times of bitrate at once, which the pacer sends at
    /// 1.5 times the bitrate, and is then still until the next change. Its bursts are its only
    /// probes: the published bitrate must not settle above what the path carries, nor fall far
    /// below it on a path that loses packets by itself, nor rise once the burst's
    /// acknowledgements have come.
    #[test]
    fn a_source_that_sends_in_bursts_publishes_no_more_than_the_path_carries() {
        for (capacity, budgets, loss) in [
            (3e6, 4.0, 0.0),
            (8e6, 4.0, 0.0),
            (8e6, 2.0, 0.0),
            (20e6, 4.0, 0.0),
            (8e6, 4.0, 0.02),
        ] {
            let mut path = Path::new(capacity).with_radio_loss(loss);
            let mut rise: f64 = 1.0;
            let (mut least, mut most) = (f64::MAX, 0.0f64);
            for second in 0..30 {
                let (mut at, end) = (Duration::from_secs(second), Duration::from_secs(second + 1));
                path.burst(budgets);
                while at < end && !(path.waiting.is_empty() && path.flying.is_empty()) {
                    path.still(at, at + STEP);
                    at += STEP;
                }
                let answered = path.published();
                path.still(at, end);
                let still = path.published();
                if second >= 10 {
                    rise = rise.max(still / answered);
                    (least, most) = (least.min(still), most.max(still));
                }
            }
            let case = format!(
                "{} Mbit/s, {budgets} frame times, {}% lost",
                capacity / 1e6,
                loss * 100.0
            );
            assert!(rise < 1.02, "{case}: rises {rise} while still");
            assert!(
                least > 0.4 * capacity && most < 1.35 * capacity,
                "{case}: from {least} to {most}"
            );
        }
    }

    /// A radio loses packets by itself, without a queue, and the video's parity and resends
    /// repair them: the bitrate stays near what the path carries and never falls to the floor.
    #[test]
    fn a_path_that_loses_packets_by_itself_keeps_the_bitrate() {
        for (capacity, round_trip, loss) in [
            (6e6, 60, 0.01),
            (6e6, 60, 0.02),
            (20e6, 120, 0.01),
            (20e6, 120, 0.02),
        ] {
            let mut path = Path::new(capacity)
                .with_round_trip(Duration::from_millis(round_trip))
                .with_radio_loss(loss);
            path.measure_from(Duration::from_secs(10));
            path.run(Duration::ZERO, Duration::from_secs(40));
            let (mean, least) = (
                path.measured.published_mean(),
                path.measured.published_least(),
            );
            let case = format!(
                "{} Mbit/s, {round_trip} ms, {}% lost",
                capacity / 1e6,
                loss * 100.0
            );
            assert!(
                mean > 0.5 * capacity && least > 0.15 * capacity,
                "{case}: {mean} on average, {least} at least"
            );
        }
    }

    /// A policer drops what exceeds its rate at once, without a queue: the bitrate settles just
    /// above that rate and loses little, instead of sawing below it.
    #[test]
    fn a_policer_holds_the_bitrate_near_its_rate() {
        let rate = 3e6;
        let mut path = Path::new(100e6)
            .with_round_trip(Duration::from_millis(60))
            .with_policer(rate);
        path.measure_from(Duration::from_secs(10));
        path.run(Duration::ZERO, Duration::from_secs(40));
        let (least, lost) = (path.measured.published_least(), path.measured.lost_share());
        assert!(
            least > 0.8 * rate && lost < 0.05,
            "{least} at least, {lost} lost"
        );
    }

    /// A short queue that overflows before the queue delay reaches its target: losses with a
    /// queue still cut the window, so they stay rare.
    #[test]
    fn a_queue_that_overflows_early_loses_little() {
        let mut path = Path::new(10e6).with_queue_limit(Duration::from_millis(15));
        path.measure_from(Duration::from_secs(10));
        path.run(Duration::ZERO, Duration::from_secs(40));
        let lost = path.measured.lost_share();
        assert!(lost < 0.01, "{lost} lost");
    }
}
