use std::{sync::OnceLock, time::Duration};

pub const PER_LAUNCH_STAGGER: Duration = Duration::from_millis(300);

pub const COLD_START_QUIESCE: Duration = Duration::from_secs(1);

static LAST_LAUNCH: OnceLock<tokio::sync::Mutex<Option<tokio::time::Instant>>> = OnceLock::new();

pub struct LaunchPermit {
    guard: tokio::sync::MutexGuard<'static, Option<tokio::time::Instant>>,
}

impl LaunchPermit {
    pub fn record_spawn(mut self) {
        *self.guard = Some(tokio::time::Instant::now());
    }
}

pub async fn acquire_launch_permit() -> LaunchPermit {
    let coordinator = LAST_LAUNCH.get_or_init(|| tokio::sync::Mutex::new(None));
    let last_launch = coordinator.lock().await;
    let delay = last_launch.map_or(COLD_START_QUIESCE, |last| {
        PER_LAUNCH_STAGGER.saturating_sub(last.elapsed())
    });
    if !delay.is_zero() {
        tokio::time::sleep(delay).await;
    }
    LaunchPermit { guard: last_launch }
}
