use std::time::Duration;

use rand::RngExt;
use tokio::time;
use tokio_util::sync::CancellationToken;

const INITIAL_RECONNECT_BACKOFF_SECS: u64 = 1;
pub(crate) const DEFAULT_MAX_RECONNECT_BACKOFF_SECS: u64 = 30;
const STABLE_CONNECTION_SECS: u64 = 30;

pub(crate) const REAUTH_PARK_SECS: u64 = 300;

#[derive(Debug, Clone)]
pub(crate) struct ReconnectBackoff {
    current_secs: u64,
    max_secs: u64,
    connected_at: Option<time::Instant>,
}

impl ReconnectBackoff {
    pub(crate) const fn capped(max_secs: u64) -> Self {
        Self {
            current_secs: INITIAL_RECONNECT_BACKOFF_SECS,
            max_secs,
            connected_at: None,
        }
    }

    pub(crate) const fn standard() -> Self {
        Self::capped(DEFAULT_MAX_RECONNECT_BACKOFF_SECS)
    }

    pub(crate) fn reset(&mut self) {
        self.current_secs = INITIAL_RECONNECT_BACKOFF_SECS;
        self.connected_at = None;
    }

    pub(crate) fn record_connected(&mut self) {
        self.connected_at = Some(time::Instant::now());
    }

    pub(crate) fn finish_connection(&mut self) {
        if self.connected_at.take().is_some_and(|connected_at| {
            connected_at.elapsed() >= Duration::from_secs(STABLE_CONNECTION_SECS)
        }) {
            self.reset();
        }
    }

    pub(crate) fn record_failure(&mut self) {
        self.current_secs = self.current_secs.saturating_mul(2).min(self.max_secs);
    }

    pub(crate) async fn wait_for_next_retry(&self, cancellation: &CancellationToken) -> bool {
        let jittered = next_equal_jitter_backoff_secs(self.current_secs);
        tokio::select! {
            () = cancellation.cancelled() => false,
            () = time::sleep(Duration::from_secs(jittered)) => true,
        }
    }

    pub(crate) async fn wait_after_failure(&mut self, cancellation: &CancellationToken) -> bool {
        if !self.wait_for_next_retry(cancellation).await {
            return false;
        }
        self.record_failure();
        true
    }
}

pub(crate) async fn wait_for_reauth(cancellation: &CancellationToken) -> bool {
    tokio::select! {
        () = cancellation.cancelled() => false,
        () = time::sleep(Duration::from_secs(REAUTH_PARK_SECS)) => true,
    }
}

#[must_use]
pub(crate) fn next_equal_jitter_backoff_secs(reconnect_backoff_secs: u64) -> u64 {
    let bounded = reconnect_backoff_secs.max(1);
    let half = bounded / 2;
    let jittered = half + rand::rng().random_range(0..=(bounded - half));
    jittered.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floors_to_one_when_backoff_is_one() {
        for _ in 0..10_000 {
            assert_eq!(next_equal_jitter_backoff_secs(1), 1);
        }
    }

    #[test]
    fn lands_in_equal_jitter_range() {
        let backoff = 30u64;
        let half = backoff / 2;
        for _ in 0..10_000 {
            let value = next_equal_jitter_backoff_secs(backoff);
            assert!(value >= half, "expected >= {half}, got {value}");
            assert!(value <= backoff, "expected <= {backoff}, got {value}");
        }
    }

    #[test]
    fn short_connections_preserve_escalation_and_stable_connection_resets() {
        let mut backoff = ReconnectBackoff::capped(30);
        backoff.record_failure();
        backoff.record_failure();
        assert_eq!(backoff.current_secs, 4);

        backoff.record_connected();
        backoff.finish_connection();
        assert_eq!(backoff.current_secs, 4);

        backoff.record_failure();
        assert_eq!(backoff.current_secs, 8);
        backoff.record_connected();
        backoff.connected_at = Some(
            time::Instant::now()
                .checked_sub(Duration::from_secs(STABLE_CONNECTION_SECS))
                .expect("test instant supports stability subtraction"),
        );
        backoff.finish_connection();
        assert_eq!(backoff.current_secs, 1);
    }

    #[test]
    fn reconnect_backoff_caps_and_resets() {
        let mut backoff = ReconnectBackoff::capped(4);
        assert_eq!(backoff.current_secs, 1);

        backoff.record_failure();
        assert_eq!(backoff.current_secs, 2);
        backoff.record_failure();
        assert_eq!(backoff.current_secs, 4);
        backoff.record_failure();
        assert_eq!(backoff.current_secs, 4);

        backoff.reset();
        assert_eq!(backoff.current_secs, 1);
    }
}
