use std::time::Duration;

use tokio::time::Instant;

use crate::network::{Result, invalid};

const INITIAL_WINDOW: u64 = 64 * 1024;
const MINIMUM_WINDOW: u64 = 16 * 1024;
const MAXIMUM_WINDOW: u64 = 8 * 1024 * 1024;
const TRANSIT_SECONDS: f64 = 0.5;
const SAMPLE: Duration = Duration::from_millis(200);

#[derive(Default)]
pub(super) struct Pacer {
    sent: u64,
    acknowledged: u64,
    rate: f64,
    pressed: bool,
    sample: Option<(Instant, u64)>,
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

    pub(super) fn room(&mut self, now: Instant) -> bool {
        if self.in_transit() < self.window() {
            return true;
        }
        if !self.pressed {
            self.pressed = true;
            self.sample = Some((now, 0));
        }
        false
    }

    pub(super) const fn sent(&mut self, bytes: u64) {
        self.sent += bytes;
    }

    pub(super) const fn idle(&mut self) {
        self.pressed = false;
        self.sample = None;
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
        let Some((started, total)) = self.sample else {
            return Ok(());
        };
        let total = total + bytes;
        let elapsed = now.saturating_duration_since(started);
        if elapsed < SAMPLE {
            self.sample = Some((started, total));
            return Ok(());
        }
        let measured = total as f64 / elapsed.as_secs_f64();
        self.rate = if self.rate == 0.0 {
            measured
        } else {
            f64::midpoint(self.rate, measured)
        };
        self.pressed = false;
        self.sample = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_follows_the_speed_that_the_viewer_acknowledges() {
        let start = Instant::now();
        let mut pacer = Pacer::default();
        assert_eq!(pacer.window(), INITIAL_WINDOW);
        assert!(pacer.room(start));
        pacer.sent(INITIAL_WINDOW);
        assert!(!pacer.room(start));
        pacer
            .acknowledge(16 * 1024, start + Duration::from_millis(100))
            .unwrap();
        assert_eq!(pacer.window(), INITIAL_WINDOW);
        pacer
            .acknowledge(48 * 1024, start + Duration::from_millis(250))
            .unwrap();
        assert_eq!(pacer.window(), 48 * 1024 * 4 / 2);
        assert_eq!(pacer.in_transit(), 16 * 1024);
        assert!(pacer.room(start));

        let mut slow = Pacer::default();
        slow.sent(INITIAL_WINDOW);
        assert!(!slow.room(start));
        slow.acknowledge(2048, start + Duration::from_secs(1))
            .unwrap();
        assert_eq!(slow.window(), MINIMUM_WINDOW);
    }

    #[test]
    fn output_that_does_not_wait_gives_no_speed_sample_and_a_false_acknowledgement_is_refused() {
        let start = Instant::now();
        let mut pacer = Pacer::default();
        pacer.sent(1000);
        pacer
            .acknowledge(1000, start + Duration::from_secs(1))
            .unwrap();
        assert_eq!(pacer.window(), INITIAL_WINDOW);
        assert!(pacer.acknowledge(1001, start).is_err());
        assert!(pacer.acknowledge(999, start).is_err());
    }
}
