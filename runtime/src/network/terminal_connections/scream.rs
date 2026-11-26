//! `SCReAMv2` (draft-ietf-ccwg-rfc8298bis-screamv2-03) as the congestion controller of the QUIC
//! connections between devices. It keeps the queue it builds short, which is what interactive video
//! needs, and tells the video senders the bitrate that the path carries.
//!
//! Queue delay comes from round trips: each acknowledged packet's round trip minus the base round
//! trip, the least one in the last half minute. Round trips need no clock in common, so the draft's
//! clock drift remedy (section 4.5.1) does not apply: it would take a standing queue for a longer
//! route. Classic ECN marks count as congestion; L4S marking needs per-acknowledgement mark counts
//! that noq does not report.

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
const LOSS_RATE_THRESHOLD: f64 = 0.01;
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

/// Builds one controller for each path of each connection.
#[derive(Debug, Default)]
pub(crate) struct Scream;

impl ControllerFactory for Scream {
    fn build(self: Arc<Self>, now: Instant, current_mtu: u16) -> Box<dyn Controller> {
        Box::new(Window::new(now, current_mtu))
    }
}

/// The bitrate that a path carries for its video, in bits per second, before each sender takes off
/// what its own queue and frame sizes need (draft section 4.4). Shared by a controller and its
/// clones, so a sender reads the live value from the clone that `Connection::congestion_state`
/// returns.
#[derive(Debug, Default)]
pub(crate) struct Carried(AtomicU64);

impl Carried {
    #[cfg(any(feature = "host", test))]
    pub(crate) fn get(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}

/// Finds the bitrate that a controller built by [`Scream`] publishes.
#[cfg(feature = "host")]
pub(crate) fn carried(controller: Box<dyn Controller>) -> Option<Arc<Carried>> {
    controller
        .into_any()
        .downcast::<Window>()
        .ok()
        .map(|window| window.carried)
}

/// Congestion signals since the window last reacted.
#[derive(Debug, Clone, Copy, Default)]
struct Seen {
    loss: bool,
    ecn: bool,
}

#[derive(Debug, Clone)]
struct Window {
    carried: Arc<Carried>,
    mss: f64,
    ref_wnd: f64,
    ref_wnd_i: f64,
    ref_wnd_i_update_allowed: bool,
    ref_wnd_i_update_time: Option<Instant>,
    ref_wnd_lowest: f64,
    ref_wnd_overhead: f64,
    s_rtt: f64,
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
            carried: Arc::default(),
            mss,
            ref_wnd,
            ref_wnd_i: 1.0,
            ref_wnd_i_update_allowed: true,
            ref_wnd_i_update_time: None,
            ref_wnd_lowest: ref_wnd,
            ref_wnd_overhead: REF_WND_OVERHEAD_MAX,
            s_rtt: 0.1,
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
        if self.seen.loss || self.seen.ecn {
            self.last_congestion_detected = Some(now);
        }
        let mut is_loss = false;
        let mut is_ce = false;
        let mut virtual_alpha = None;
        if Self::since(self.last_reaction, now) >= self.s_rtt {
            if self.seen.loss
                && (self.loss_rate > LOSS_RATE_THRESHOLD
                    || self.qdelay_avg > self.qdelay_target / 4.0)
            {
                is_loss = true;
            } else if self.seen.ecn {
                is_ce = true;
            } else if self.qdelay_avg > self.qdelay_target / 2.0
                && self.qdelay_avg >= self.qdelay_avg_prev
            {
                virtual_alpha = Some(
                    ((self.qdelay_avg - self.qdelay_target / 2.0) / (self.qdelay_target / 2.0))
                        .clamp(0.0, 1.0),
                );
                self.last_congestion_detected = Some(now);
            }
            self.loss_event_rate =
                0.99f64.mul_add(self.loss_event_rate, if is_loss { 0.01 } else { 0.0 });
            self.seen.loss = false;
            self.seen.ecn = false;
        }
        self.qdelay_avg_prev = self.qdelay_avg;
        let reacts = is_loss || is_ce || virtual_alpha.is_some();
        if reacts {
            self.congested = true;
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
        if is_loss {
            self.ref_wnd *= BETA_LOSS;
        }
        if is_ce {
            self.ref_wnd *= BETA_ECN;
        }
        if let Some(alpha) = virtual_alpha {
            let backoff = soften(alpha / 2.0, self);
            self.ref_wnd *= 1.0 - backoff;
        }
        self.ref_wnd = self.ref_wnd.max(MIN_REF_WND);
        self.ref_wnd_lowest = self.ref_wnd;
        if reacts {
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
            reason = "a finite positive bitrate below the ceiling"
        )]
        let carried = carried.min(TARGET_BITRATE_MAX * 1.2) as u64;
        self.carried.0.store(carried, Ordering::Relaxed);
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
        self.s_rtt = smoothed.as_secs_f64().max(SMALLEST_RTT);
        self.sample_base(now, round);
        self.qdelay = (round - self.base_rtt().unwrap_or(round)).max(0.0);
        self.bytes_newly_acked += bytes;
        self.acked.push_back((now, bytes));
        let alpha = (LOSS_RATE_THRESHOLD / 4.0).min(self.mss / self.ref_wnd / 2.0);
        self.loss_rate *= 1.0 - alpha;
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
        _app_limited: bool,
        _largest_packet_num_acked: Option<u64>,
    ) {
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
        let alpha = (LOSS_RATE_THRESHOLD / 4.0).min(self.mss / self.ref_wnd / 2.0);
        self.loss_rate = (1.0 - alpha).mul_add(self.loss_rate, alpha);
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

    /// A sender that follows the published bitrate, through a bottleneck that serves `capacity`
    /// bits per second from one queue, on a path with a 20 ms round trip without queue.
    struct Path {
        window: Window,
        start: Instant,
        capacity: f64,
        credit: f64,
        served_until: Instant,
        flying: VecDeque<(Instant, Instant)>,
        smoothed: f64,
        queued_most: Duration,
        sent_bits: f64,
    }

    impl Path {
        fn new(capacity: f64) -> Self {
            let start = Instant::now();
            Self {
                window: Window::new(start, MTU),
                start,
                capacity,
                credit: 0.0,
                served_until: start,
                flying: VecDeque::new(),
                smoothed: 0.02,
                queued_most: Duration::ZERO,
                sent_bits: 0.0,
            }
        }

        fn run(&mut self, from: Duration, to: Duration) {
            let mut at = from;
            while at < to {
                let now = self.start + at;
                while let Some(&(sent, acked)) = self.flying.front()
                    && acked <= now
                {
                    self.flying.pop_front();
                    self.smoothed =
                        0.125f64.mul_add((acked - sent).as_secs_f64(), 0.875 * self.smoothed);
                    let smoothed = Duration::from_secs_f64(self.smoothed);
                    self.window
                        .acked_packet(acked, sent, u64::from(MTU), smoothed);
                    let in_flight = (self.flying.len() * usize::from(MTU)) as u64;
                    self.window.on_end_acks(acked, in_flight, false, None);
                }
                let rate = (bits(self.window.carried.get()) / 1.2).max(500_000.0);
                self.credit += rate * STEP.as_secs_f64() / 8.0;
                let window = usize::try_from(self.window.window()).unwrap();
                while self.credit >= f64::from(MTU)
                    && (self.flying.len() + 1) * usize::from(MTU) <= window
                {
                    self.credit -= f64::from(MTU);
                    self.window.on_packet_sent(now, MTU, 0);
                    self.sent_bits = f64::from(MTU).mul_add(8.0, self.sent_bits);
                    let reaches = now + Duration::from_millis(5);
                    let served = reaches.max(self.served_until)
                        + Duration::from_secs_f64(f64::from(MTU) * 8.0 / self.capacity);
                    self.served_until = served;
                    if at > from + to.saturating_sub(from) / 2 {
                        self.queued_most = self
                            .queued_most
                            .max(served.saturating_duration_since(reaches));
                    }
                    self.flying
                        .push_back((now, served + Duration::from_millis(15)));
                }
                self.credit = self.credit.min(4.0 * f64::from(MTU));
                at += STEP;
            }
        }
    }

    #[test]
    fn the_bitrate_settles_near_a_bottleneck_with_a_short_queue() {
        let mut path = Path::new(10_000_000.0);
        path.run(Duration::ZERO, Duration::from_secs(10));
        let settled_from = path.sent_bits;
        path.run(Duration::from_secs(10), Duration::from_secs(20));
        let sent = (path.sent_bits - settled_from) / 10.0;
        assert!(
            sent > 7_000_000.0,
            "uses most of a 10 Mbps bottleneck: {sent}"
        );
        assert!(
            path.queued_most < Duration::from_millis(60),
            "keeps the queue short: {:?}",
            path.queued_most
        );
    }

    #[test]
    fn a_narrower_bottleneck_lowers_the_bitrate() {
        let mut path = Path::new(10_000_000.0);
        path.run(Duration::ZERO, Duration::from_secs(10));
        path.capacity = 3_000_000.0;
        path.queued_most = Duration::ZERO;
        path.run(Duration::from_secs(10), Duration::from_secs(20));
        let published = bits(path.window.carried.get()) / 1.2;
        assert!(
            published < 4_000_000.0,
            "follows the narrower path: {published}"
        );
        assert!(
            path.queued_most < Duration::from_millis(60),
            "drains the queue: {:?}",
            path.queued_most
        );
    }

    /// Wi-Fi loses packets now and then without any queue; only losses with a queue mean
    /// congestion.
    #[test]
    fn a_rare_loss_without_a_queue_keeps_the_window() {
        let mut path = Path::new(1_000_000_000.0);
        path.run(Duration::ZERO, Duration::from_secs(5));
        let before = path.window.ref_wnd;
        let now = path.start + Duration::from_secs(5);
        path.window.on_packet_lost(MTU, 0, now);
        path.window
            .on_congestion_event(now, now, false, false, u64::from(MTU), 0);
        path.window.react(now + Duration::from_secs(1));
        assert!(
            path.window.ref_wnd >= before * 0.99,
            "{before} -> {}",
            path.window.ref_wnd
        );
    }
}
