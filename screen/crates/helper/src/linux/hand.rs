//! The person at the computer comes first: while they move its own pointer, viewers' input waits a
//! moment, so that two hands never fight over it. The desktop lets the helper see only where the
//! pointer is, with the frames, so the pointer showing where the helper did not put it is the
//! person's hand; their keys stay unseen.

use std::{
    collections::VecDeque,
    sync::{Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};

/// How long the person's own input keeps viewers' input away.
const FIRST: Duration = Duration::from_secs(1);
/// How long the desktop may take to show the pointer where the helper put it.
const SHOWING: Duration = Duration::from_secs(1);
/// How far, in points, the desktop may show the pointer from where the helper put it.
const NEAR: f64 = 2.0;

#[derive(Default)]
pub(super) struct Hand(Mutex<Seen>);

/// Positions are in the desktop's points.
#[derive(Default)]
struct Seen {
    /// Where the helper put the pointer lately, the newest last.
    put: VecDeque<(Instant, (f64, f64))>,
    shown: Option<(f64, f64)>,
    /// When the person at the computer last moved the pointer.
    moved: Option<Instant>,
}

impl Hand {
    pub(super) fn put(&self, at: (f64, f64)) {
        let now = Instant::now();
        let mut seen = self.lock();
        while seen
            .put
            .front()
            .is_some_and(|(when, _)| now.duration_since(*when) >= SHOWING)
        {
            seen.put.pop_front();
        }
        seen.put.push_back((now, at));
    }

    /// The desktop shows the pointer `at`.
    pub(super) fn shown(&self, at: (f64, f64)) {
        let now = Instant::now();
        let mut seen = self.lock();
        let moved = seen.shown.replace(at).is_some_and(|before| before != at);
        let put = seen
            .put
            .iter()
            .rev()
            .enumerate()
            .any(|(index, (when, put))| {
                (index == 0 || now.duration_since(*when) < SHOWING) && near(*put, at)
            });
        if moved && !put {
            seen.moved = Some(now);
        }
    }

    /// Whether the desktop last showed the pointer `at`.
    pub(super) fn showed(&self, at: (f64, f64)) -> bool {
        self.lock().shown.is_some_and(|shown| near(shown, at))
    }

    /// True while the person at the computer moves its pointer or just did.
    pub(super) fn busy(&self) -> bool {
        self.lock().moved.is_some_and(|at| at.elapsed() < FIRST)
    }

    fn lock(&self) -> MutexGuard<'_, Seen> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn near(one: (f64, f64), other: (f64, f64)) -> bool {
    (one.0 - other.0).abs() <= NEAR && (one.1 - other.1).abs() <= NEAR
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_pointer_that_the_helper_did_not_put_there_is_the_persons() {
        let hand = Hand::default();
        hand.shown((100.0, 100.0));
        hand.put((300.0, 200.0));
        hand.put((310.0, 205.0));
        hand.shown((300.5, 200.0));
        hand.shown((310.0, 205.0));
        assert!(!hand.busy());
        hand.shown((500.0, 50.0));
        assert!(hand.busy());
    }
}
