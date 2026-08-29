use tokio_util::sync::CancellationToken;

use kodosi_backend_client::user_events::UserEventsHandle;

use crate::session_runtime::events::AccountEventOrigin;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelfDeviceLinkIdentity {
    pub(crate) origin: AccountEventOrigin,
    pub(crate) device_id: String,
    pub(crate) device_label: String,
    pub(crate) user_code: String,
    pub(crate) expires_at: String,
}

#[derive(Debug, Default)]
pub(crate) struct AccountRuntimes {
    user_events: Option<UserEventsHandle>,
    self_device_link: Option<SelfDeviceLinkRuntime>,
}

#[derive(Debug)]
pub(crate) struct SelfDeviceLinkRuntime {
    pub(crate) identity: SelfDeviceLinkIdentity,
    pub(crate) cancellation: CancellationToken,
    pub(crate) join_handle: tokio::task::JoinHandle<()>,
}

impl AccountRuntimes {
    pub(crate) fn user_events_healthy(&self) -> bool {
        self.user_events.as_ref().is_some_and(|runtime| {
            !runtime.cancellation.is_cancelled() && !runtime.join_handle.is_finished()
        })
    }

    pub(crate) fn attach_user_events(&mut self, runtime: UserEventsHandle) {
        self.stop_user_events();
        self.user_events = Some(runtime);
    }

    pub(crate) fn stop_user_events(&mut self) -> bool {
        let Some(runtime) = self.user_events.take() else {
            return false;
        };

        runtime.cancellation.cancel();
        if !runtime.join_handle.is_finished() {
            runtime.join_handle.abort();
        }
        true
    }

    pub(crate) fn self_device_link(&self) -> Option<&SelfDeviceLinkRuntime> {
        self.self_device_link.as_ref()
    }

    pub(crate) fn attach_self_device_link(&mut self, runtime: SelfDeviceLinkRuntime) {
        self.stop_self_device_link();
        self.self_device_link = Some(runtime);
    }

    pub(crate) fn retire_self_device_link(&mut self, expected_user_code: &str) -> bool {
        if self
            .self_device_link
            .as_ref()
            .is_none_or(|runtime| runtime.identity.user_code != expected_user_code)
        {
            return false;
        }
        self.self_device_link.take().is_some()
    }

    pub(crate) fn request_self_device_link_cancellation(&self) -> bool {
        let Some(runtime) = self.self_device_link.as_ref() else {
            return false;
        };
        runtime.cancellation.cancel();
        true
    }

    fn stop_self_device_link(&mut self) -> bool {
        let Some(runtime) = self.self_device_link.take() else {
            return false;
        };

        runtime.cancellation.cancel();
        if !runtime.join_handle.is_finished() {
            runtime.join_handle.abort();
        }
        true
    }

    pub(crate) fn shutdown_all(&mut self) {
        self.stop_user_events();
        self.stop_self_device_link();
    }
}

pub(crate) fn shutdown_account_runtimes(
    device_flow: &mut crate::runtime::identity::DeviceFlowRuntime,
    account_runtimes: &mut AccountRuntimes,
    pending_discovery_surfaces: &mut std::collections::BTreeSet<
        crate::session_runtime::events::DiscoverySurface,
    >,
) {
    device_flow.cancel();
    account_runtimes.shutdown_all();
    pending_discovery_surfaces.clear();
}

pub(crate) struct AccountRuntimeShutdown {
    user_events: Option<tokio::task::JoinHandle<()>>,
    self_device_link: Option<tokio::task::JoinHandle<()>>,
}

impl AccountRuntimeShutdown {
    pub(crate) async fn join_before(self, deadline: tokio::time::Instant) {
        let user_events =
            join_account_task("user-events websocket cleanup", self.user_events, deadline);
        let self_device_link = join_account_task(
            "self-device-link backend cleanup",
            self.self_device_link,
            deadline,
        );
        tokio::join!(user_events, self_device_link);
    }
}

pub(crate) fn begin_shutdown_account_runtimes(
    device_flow: &mut crate::runtime::identity::DeviceFlowRuntime,
    account_runtimes: &mut AccountRuntimes,
    pending_discovery_surfaces: &mut std::collections::BTreeSet<
        crate::session_runtime::events::DiscoverySurface,
    >,
) -> AccountRuntimeShutdown {
    device_flow.cancel();
    let user_events = account_runtimes.user_events.take().map(|runtime| {
        runtime.cancellation.cancel();
        runtime.join_handle
    });
    let self_device_link = account_runtimes.self_device_link.take().map(|runtime| {
        runtime.cancellation.cancel();
        runtime.join_handle
    });
    pending_discovery_surfaces.clear();
    AccountRuntimeShutdown {
        user_events,
        self_device_link,
    }
}

async fn join_account_task(
    name: &'static str,
    task: Option<tokio::task::JoinHandle<()>>,
    deadline: tokio::time::Instant,
) {
    let Some(mut task) = task else {
        return;
    };
    match tokio::time::timeout_at(deadline, &mut task).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            tracing::warn!(name, %error, "account runtime cleanup task failed");
        }
        Err(_) => {
            tracing::warn!(
                name,
                "account runtime cleanup exceeded shutdown deadline; aborting task"
            );
            task.abort();
            drop(task.await);
        }
    }
}

impl Drop for AccountRuntimes {
    fn drop(&mut self) {
        self.shutdown_all();
    }
}

#[cfg(test)]
mod tests {
    use tokio_util::sync::CancellationToken;

    use super::{
        AccountRuntimeShutdown, AccountRuntimes, SelfDeviceLinkIdentity, SelfDeviceLinkRuntime,
    };
    use crate::session_runtime::events::{AccountEpoch, AccountEventOrigin};
    use kodosi_backend_client::user_events::UserEventsHandle;

    fn user_events_handle() -> (
        UserEventsHandle,
        CancellationToken,
        tokio::task::AbortHandle,
    ) {
        let cancellation = CancellationToken::new();
        let task_cancellation = cancellation.clone();
        let join_handle = tokio::spawn(async move {
            task_cancellation.cancelled().await;
        });
        let abort_handle = join_handle.abort_handle();
        (
            UserEventsHandle {
                cancellation: cancellation.clone(),
                join_handle,
            },
            cancellation,
            abort_handle,
        )
    }

    fn self_device_link_identity(user_code: &str) -> SelfDeviceLinkIdentity {
        SelfDeviceLinkIdentity {
            origin: AccountEventOrigin {
                account_user_id: "11111111-1111-1111-1111-111111111111".to_owned(),
                epoch: AccountEpoch::for_test(7),
            },
            device_id: "device-local".to_owned(),
            device_label: "Test Mac".to_owned(),
            user_code: user_code.to_owned(),
            expires_at: "2026-08-18T09:00:00Z".to_owned(),
        }
    }

    fn self_device_link_runtime() -> (
        SelfDeviceLinkRuntime,
        CancellationToken,
        tokio::task::AbortHandle,
    ) {
        let cancellation = CancellationToken::new();
        let join_handle = tokio::spawn(std::future::pending::<()>());
        let abort_handle = join_handle.abort_handle();
        (
            SelfDeviceLinkRuntime {
                identity: self_device_link_identity("ABCD-EFGH"),
                cancellation: cancellation.clone(),
                join_handle,
            },
            cancellation,
            abort_handle,
        )
    }

    async fn wait_for_finished(abort_handle: &tokio::task::AbortHandle) {
        for _ in 0..10 {
            if abort_handle.is_finished() {
                return;
            }
            tokio::task::yield_now().await;
        }
        assert!(abort_handle.is_finished());
    }

    #[tokio::test]
    async fn self_link_cancel_does_not_wait_for_a_saturated_event_lane() {
        let mut runtimes = AccountRuntimes::default();
        let cancellation = CancellationToken::new();
        let task_cancellation = cancellation.clone();
        let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(1);
        event_tx.send(()).await.expect("seed full event lane");
        let join_handle = tokio::spawn(async move {
            task_cancellation.cancelled().await;
            event_tx.send(()).await.expect("publish terminal event");
        });
        let abort = join_handle.abort_handle();
        runtimes.attach_self_device_link(SelfDeviceLinkRuntime {
            identity: self_device_link_identity("ABCD-EFGH"),
            cancellation: cancellation.clone(),
            join_handle,
        });

        assert!(runtimes.request_self_device_link_cancellation());
        assert!(cancellation.is_cancelled());
        tokio::task::yield_now().await;
        assert!(
            !abort.is_finished(),
            "cleanup must remain installed while its terminal event awaits capacity"
        );
        assert!(runtimes.self_device_link().is_some());

        assert_eq!(event_rx.recv().await, Some(()));
        for _ in 0..10 {
            if abort.is_finished() {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert!(abort.is_finished());
        assert_eq!(event_rx.recv().await, Some(()));
    }

    #[tokio::test]
    async fn bounded_account_shutdown_joins_user_events_cooperatively() {
        let cancellation = CancellationToken::new();
        let task_cancellation = cancellation.clone();
        let cleaned = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let task_cleaned = std::sync::Arc::clone(&cleaned);
        let join_handle = tokio::spawn(async move {
            task_cancellation.cancelled().await;
            task_cleaned.store(true, std::sync::atomic::Ordering::Release);
        });
        cancellation.cancel();
        let shutdown = AccountRuntimeShutdown {
            user_events: Some(join_handle),
            self_device_link: None,
        };

        shutdown
            .join_before(tokio::time::Instant::now() + std::time::Duration::from_secs(1))
            .await;
        assert!(cleaned.load(std::sync::atomic::Ordering::Acquire));
    }

    #[tokio::test]
    async fn attaching_user_events_stops_prior_handle() {
        let mut runtimes = AccountRuntimes::default();
        let (first, first_cancel, first_abort) = user_events_handle();
        let (second, _second_cancel, second_abort) = user_events_handle();

        runtimes.attach_user_events(first);
        assert!(runtimes.user_events_healthy());

        runtimes.attach_user_events(second);

        assert!(first_cancel.is_cancelled());
        wait_for_finished(&first_abort).await;
        assert!(first_abort.is_finished());
        assert!(runtimes.user_events_healthy());
        second_abort.abort();
    }

    #[tokio::test]
    async fn user_events_health_tracks_cancelled_and_finished_handles() {
        let mut runtimes = AccountRuntimes::default();
        let (runtime, cancellation, abort) = user_events_handle();
        runtimes.attach_user_events(runtime);
        assert!(runtimes.user_events_healthy());

        cancellation.cancel();
        assert!(!runtimes.user_events_healthy());
        runtimes.stop_user_events();
        wait_for_finished(&abort).await;

        let finished = tokio::spawn(async {});
        let finished_abort = finished.abort_handle();
        for _ in 0..10 {
            if finished.is_finished() {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert!(finished.is_finished());
        runtimes.attach_user_events(UserEventsHandle {
            cancellation: CancellationToken::new(),
            join_handle: finished,
        });
        assert!(!runtimes.user_events_healthy());

        finished_abort.abort();
    }

    #[tokio::test]
    async fn requesting_self_device_link_cancellation_keeps_cleanup_task_authoritative() {
        let mut runtimes = AccountRuntimes::default();
        let (runtime, cancellation, abort) = self_device_link_runtime();

        runtimes.attach_self_device_link(runtime);
        assert!(runtimes.self_device_link().is_some());
        assert!(runtimes.request_self_device_link_cancellation());
        assert!(runtimes.request_self_device_link_cancellation());

        assert!(cancellation.is_cancelled());
        assert!(runtimes.self_device_link().is_some());
        assert!(
            !abort.is_finished(),
            "self-link cleanup task must keep running after cancellation"
        );
        abort.abort();
    }

    #[tokio::test]
    async fn stale_self_link_result_does_not_retire_replacement() {
        let mut runtimes = AccountRuntimes::default();
        let (runtime, _cancellation, abort) = self_device_link_runtime();
        runtimes.attach_self_device_link(runtime);

        assert!(!runtimes.retire_self_device_link("OLD1-CODE"));
        assert_eq!(
            runtimes
                .self_device_link()
                .map(|runtime| runtime.identity.user_code.as_str()),
            Some("ABCD-EFGH")
        );
        assert!(runtimes.retire_self_device_link("ABCD-EFGH"));
        abort.abort();
    }

    #[tokio::test]
    async fn shutdown_all_is_idempotent() {
        let mut runtimes = AccountRuntimes::default();
        let (user_events, user_events_cancel, user_events_abort) = user_events_handle();
        let (self_link, self_link_cancel, self_link_abort) = self_device_link_runtime();

        runtimes.attach_user_events(user_events);
        runtimes.attach_self_device_link(self_link);
        runtimes.shutdown_all();
        runtimes.shutdown_all();

        assert!(user_events_cancel.is_cancelled());
        wait_for_finished(&user_events_abort).await;
        assert!(user_events_abort.is_finished());
        assert!(self_link_cancel.is_cancelled());
        self_link_abort.abort();
    }
}
