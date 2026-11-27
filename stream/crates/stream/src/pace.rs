//! How often a stream codes a new picture. While a viewer acts, the delay from the act to the
//! picture counts. While people only watch, half as many pictures look the same, cost half the
//! work on both sides and give each frame twice the bits.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Pace {
    /// A viewer acts.
    Acting,
    /// People only watch.
    Watching,
}

impl Pace {
    pub const fn frame_rate(self) -> u32 {
        match self {
            Self::Acting => 60,
            Self::Watching => 30,
        }
    }

    /// The time from one new picture to the next.
    pub fn frame_time(self) -> Duration {
        Duration::from_secs(1) / self.frame_rate()
    }
}

/// Follows what the viewers of one stream do, and tells its pace when the pace changes.
#[derive(Debug)]
pub struct Pacer {
    pace: Pace,
    acted_at: Instant,
}

impl Pacer {
    /// How long a stream keeps the higher pace after a viewer's last act.
    pub const HOLD: Duration = Duration::from_secs(2);

    /// A stream starts as acted on: the person who opened it just did that.
    pub const fn new(now: Instant) -> Self {
        Self {
            pace: Pace::Acting,
            acted_at: now,
        }
    }

    pub const fn pace(&self) -> Pace {
        self.pace
    }

    /// A viewer gave input, a pointer move too, or changed the part of the picture that it looks
    /// at. Returns the higher pace when the stream was at the lower one.
    pub fn acted(&mut self, now: Instant) -> Option<Pace> {
        self.acted_at = self.acted_at.max(now);
        self.change(Pace::Acting)
    }

    /// Returns the lower pace once no viewer acted for `HOLD`.
    pub fn settle(&mut self, now: Instant) -> Option<Pace> {
        if now.saturating_duration_since(self.acted_at) < Self::HOLD {
            return None;
        }
        self.change(Pace::Watching)
    }

    fn change(&mut self, pace: Pace) -> Option<Pace> {
        (std::mem::replace(&mut self.pace, pace) != pace).then_some(pace)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MOMENT: Duration = Duration::from_millis(1);

    #[test]
    fn watching_lowers_the_pace_after_the_hold_and_an_act_raises_it_at_once_each_said_once() {
        let start = Instant::now();
        let mut pacer = Pacer::new(start + MOMENT);
        assert_eq!(pacer.pace(), Pace::Acting);
        assert_eq!(pacer.settle(start + Pacer::HOLD), None);
        assert_eq!(
            pacer.settle(start + MOMENT + Pacer::HOLD),
            Some(Pace::Watching)
        );
        assert_eq!(pacer.settle(start + Pacer::HOLD * 3), None);

        let acted = start + Pacer::HOLD * 4;
        assert_eq!(pacer.acted(acted), Some(Pace::Acting));
        assert_eq!(pacer.acted(acted + MOMENT), None);
        assert_eq!(pacer.settle(acted + Pacer::HOLD), None);
        assert_eq!(
            pacer.settle(acted + MOMENT + Pacer::HOLD),
            Some(Pace::Watching)
        );
        assert_eq!(pacer.pace(), Pace::Watching);
    }
}
