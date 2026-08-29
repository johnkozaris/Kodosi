use std::time::Duration;

use tokio::{task::JoinHandle, time};

const SESSION_DRAIN_SECS: u64 = 10;
const TASK_DRAIN_SECS: u64 = 5;
const SHUTDOWN_COORDINATION_SECS: u64 = 1;

pub(crate) const SESSION_DRAIN_BUDGET: Duration = Duration::from_secs(SESSION_DRAIN_SECS);

pub(crate) const TASK_DRAIN_BUDGET: Duration = Duration::from_secs(TASK_DRAIN_SECS);

pub(crate) const HOST_SHUTDOWN_BUDGET: Duration =
    Duration::from_secs(SESSION_DRAIN_SECS + TASK_DRAIN_SECS + SHUTDOWN_COORDINATION_SECS);

#[cfg(feature = "cli")]
pub(crate) const HOST_STOP_SLACK: Duration = Duration::from_secs(5);

pub(crate) async fn join_within(label: &str, handle: JoinHandle<()>, deadline: time::Instant) {
    let abort = handle.abort_handle();
    match time::timeout_at(deadline, handle).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) if error.is_panic() => {
            tracing::warn!(label, %error, "task panicked during shutdown");
        }
        Ok(Err(error)) => {
            tracing::debug!(label, %error, "task ended before it could be joined");
        }
        Err(_elapsed) => {
            tracing::warn!(label, "task overran the shutdown budget; aborting it");
            abort.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{HOST_SHUTDOWN_BUDGET, SESSION_DRAIN_BUDGET, TASK_DRAIN_BUDGET, join_within};

    #[test]
    fn the_host_budget_reserves_time_for_coordination() {
        assert!(HOST_SHUTDOWN_BUDGET > SESSION_DRAIN_BUDGET + TASK_DRAIN_BUDGET,);
    }

    #[tokio::test(start_paused = true)]
    async fn a_wedged_task_is_aborted_at_the_deadline_instead_of_awaited() {
        let handle = tokio::spawn(std::future::pending::<()>());
        let abort = handle.abort_handle();
        let deadline = tokio::time::Instant::now() + TASK_DRAIN_BUDGET;

        join_within("wedged", handle, deadline).await;

        for _ in 0..16 {
            if abort.is_finished() {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert!(abort.is_finished(), "the overrunning task must be aborted");
    }

    #[tokio::test(start_paused = true)]
    async fn a_task_that_finishes_in_budget_is_joined_not_aborted() {
        let handle = tokio::spawn(async {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        });
        let deadline = tokio::time::Instant::now() + TASK_DRAIN_BUDGET;

        join_within("prompt", handle, deadline).await;
    }
}
