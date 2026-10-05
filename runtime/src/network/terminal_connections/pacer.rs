use std::time::Duration;

use tokio::time::Instant;

use crate::network::{Result, invalid};

const INITIAL_WINDOW: u64 = 16 * 1024;
const MINIMUM_WINDOW: u64 = 4 * 1024;
const MAXIMUM_WINDOW: u64 = 8 * 1024 * 1024;
const TRANSIT_SECONDS: f64 = 0.5;
const SAMPLE: Duration = Duration::from_millis(200);

#[derive(Default)]
enum Sample {
    #[default]
    Idle,
    Armed,
    Running {
        started: Instant,
        bytes: u64,
    },
}

#[derive(Default)]
pub(super) struct Pacer {
    sent: u64,
    acknowledged: u64,
    bulk: u64,
    rate: f64,
    sample: Sample,
    busy: Option<(Instant, u64)>,
}

impl Pacer {
    pub(super) const fn in_transit(&self) -> u64 {
        self.sent - self.acknowledged
    }

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the product is clamped to the window bounds"
    )]
    pub(super) fn window(&self) -> u64 {
        if self.rate == 0.0 {
            INITIAL_WINDOW
        } else {
            ((self.rate * TRANSIT_SECONDS) as u64).clamp(MINIMUM_WINDOW, MAXIMUM_WINDOW)
        }
    }

    pub(super) fn room(&mut self) -> bool {
        if self.in_transit() < self.window() {
            return true;
        }
        self.blocked();
        false
    }

    pub(super) const fn blocked(&mut self) {
        if matches!(self.sample, Sample::Idle) {
            self.sample = Sample::Armed;
        }
    }

    pub(super) fn sent(&mut self, bytes: u64, now: Instant) {
        if self.in_transit() == 0 {
            self.busy = Some((now, self.acknowledged));
        }
        self.sent += bytes;
    }

    pub(super) const fn bulk(&mut self) {
        self.bulk = self.sent;
        self.blocked();
    }

    pub(super) const fn idle(&mut self) {
        if self.acknowledged >= self.bulk {
            self.sample = Sample::Idle;
        }
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "a speed estimate does not need exact byte counts"
    )]
    pub(super) fn acknowledge(&mut self, received: u64, now: Instant) -> Result<()> {
        if received < self.acknowledged || received > self.sent {
            return Err(invalid("Terminal output acknowledgement is out of range."));
        }
        let bytes = received - self.acknowledged;
        self.acknowledged = received;
        if self.in_transit() == 0
            && let Some((started, from)) = self.busy.take()
        {
            let elapsed = now.saturating_duration_since(started).as_secs_f64();
            if elapsed > 0.0 {
                self.rate = self.rate.max((received - from) as f64 / elapsed);
            }
        }
        match self.sample {
            Sample::Idle => {}
            Sample::Armed => {
                self.sample = Sample::Running {
                    started: now,
                    bytes: 0,
                };
            }
            Sample::Running {
                started,
                bytes: total,
            } => {
                let total = total + bytes;
                let elapsed = now.saturating_duration_since(started);
                if elapsed < SAMPLE {
                    self.sample = Sample::Running {
                        started,
                        bytes: total,
                    };
                } else {
                    let measured = total as f64 / elapsed.as_secs_f64();
                    self.rate = if self.rate == 0.0 {
                        measured
                    } else {
                        f64::midpoint(self.rate, measured)
                    };
                    self.sample = Sample::Idle;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(start: Instant, milliseconds: u64) -> Instant {
        start + Duration::from_millis(milliseconds)
    }

    #[test]
    fn the_window_follows_the_speed_that_the_viewer_acknowledges_while_output_waits() {
        let start = Instant::now();
        let mut pacer = Pacer::default();
        assert_eq!(pacer.window(), INITIAL_WINDOW);
        assert!(pacer.room());
        pacer.sent(128 * 1024, start);
        assert!(!pacer.room());
        pacer.acknowledge(16 * 1024, at(start, 100)).unwrap();
        pacer.acknowledge(32 * 1024, at(start, 200)).unwrap();
        assert_eq!(pacer.window(), INITIAL_WINDOW);
        pacer.acknowledge(64 * 1024, at(start, 350)).unwrap();
        assert_eq!(pacer.window(), 48 * 1024 * 4 / 2);
        assert_eq!(pacer.in_transit(), 64 * 1024);
        assert!(pacer.room());

        let mut slow = Pacer::default();
        slow.sent(64 * 1024, start);
        assert!(!slow.room());
        slow.acknowledge(1024, at(start, 1000)).unwrap();
        slow.acknowledge(2048, at(start, 2000)).unwrap();
        assert_eq!(slow.window(), MINIMUM_WINDOW);
    }

    #[test]
    fn a_snapshot_in_transit_shows_the_speed_of_a_slow_link() {
        let start = Instant::now();
        let mut slow = Pacer::default();
        slow.sent(30_000, start);
        slow.bulk();
        slow.idle();
        slow.acknowledge(4000, at(start, 300)).unwrap();
        slow.idle();
        slow.acknowledge(8000, at(start, 550)).unwrap();
        assert_eq!(slow.window(), 8000);
    }

    #[test]
    fn output_that_arrived_in_full_shows_the_least_speed_and_does_not_decrease_the_estimate() {
        let start = Instant::now();
        let mut pacer = Pacer::default();
        pacer.sent(3000, start);
        pacer.idle();
        pacer.acknowledge(3000, at(start, 200)).unwrap();
        assert_eq!(pacer.window(), 7500);
        pacer.sent(100, at(start, 1000));
        pacer.idle();
        pacer.acknowledge(3100, at(start, 1200)).unwrap();
        assert_eq!(pacer.window(), 7500);
        pacer.sent(60_000, at(start, 2000));
        pacer.acknowledge(63_100, at(start, 2100)).unwrap();
        assert_eq!(pacer.window(), 300_000);
    }

    #[test]
    fn a_false_acknowledgement_is_refused() {
        let start = Instant::now();
        let mut pacer = Pacer::default();
        pacer.sent(1000, start);
        pacer.acknowledge(1000, at(start, 1000)).unwrap();
        assert!(pacer.acknowledge(1001, start).is_err());
        assert!(pacer.acknowledge(999, start).is_err());
    }
}
