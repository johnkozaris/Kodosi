use std::sync::{Arc, LazyLock};

use tokio::sync::{AcquireError, Semaphore};
use tracing::Instrument;

const IO_POOL_PARALLELISM: usize = 32;

#[derive(Debug, thiserror::Error)]
pub enum IoPoolError {
    #[error("agent-intel io pool is shutting down")]
    Shutdown,
    #[error("agent-intel io task did not complete: {0}")]
    JoinFailure(String),
}

impl From<AcquireError> for IoPoolError {
    fn from(_: AcquireError) -> Self {
        Self::Shutdown
    }
}

static POOL: LazyLock<Arc<Semaphore>> =
    LazyLock::new(|| Arc::new(Semaphore::new(IO_POOL_PARALLELISM)));

pub async fn spawn_io<F, T>(f: F) -> Result<T, IoPoolError>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let permit = Arc::clone(&POOL).acquire_owned().await?;
    let span = tracing::Span::current();
    tokio::task::spawn_blocking(move || {
        let _enter = span.enter();
        let result = f();
        drop(permit);
        result
    })
    .in_current_span()
    .await
    .map_err(|join_err| IoPoolError::JoinFailure(join_err.to_string()))
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests use expect/unwrap for failure clarity"
)]
mod tests {
    use super::{IO_POOL_PARALLELISM, spawn_io};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn spawn_io_runs_blocking_closure_and_returns_value() {
        let value = spawn_io(|| 1 + 2).await.expect("io ok");
        assert_eq!(value, 3);
    }

    #[tokio::test]
    async fn spawn_io_caps_parallelism_at_pool_limit() {
        let inflight = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();
        for _ in 0..(IO_POOL_PARALLELISM * 2) {
            let inflight = Arc::clone(&inflight);
            let peak = Arc::clone(&peak);
            handles.push(tokio::spawn(async move {
                spawn_io(move || {
                    let now = inflight.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(now, Ordering::SeqCst);
                    std::thread::sleep(std::time::Duration::from_millis(20));
                    inflight.fetch_sub(1, Ordering::SeqCst);
                })
                .await
                .expect("io ok");
            }));
        }
        for h in handles {
            h.await.expect("task ok");
        }
        let observed_peak = peak.load(Ordering::SeqCst);
        assert!(
            observed_peak <= IO_POOL_PARALLELISM,
            "peak inflight {observed_peak} exceeded pool limit {IO_POOL_PARALLELISM}"
        );
    }
}
