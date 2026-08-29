use std::{
    future::Future,
    path::PathBuf,
    sync::{Arc, OnceLock},
    time::Duration,
};

use kodosi_session::{KodosiPty, ProcessSnapshot, ProcessTarget, inspect_process_from_snapshot};
use tokio::{
    sync::{Semaphore, mpsc},
    task::JoinHandle,
    time,
};
use tokio_util::sync::CancellationToken;

use crate::session_runtime::events::{LocalCoordinatorOrigin, RuntimeSessionEvent};

const PROCESS_SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(1);
const METADATA_CAPTURE_RETRY_BASE: Duration = Duration::from_millis(250);
const METADATA_CAPTURE_RETRY_MAX: Duration = Duration::from_secs(5);
const METADATA_DELIVERY_TIMEOUT: Duration = Duration::from_secs(1);
const METADATA_WORKER_SHUTDOWN_TIMEOUT: Duration = Duration::from_millis(250);
const METADATA_REQUEST_CAPACITY: usize = 1;

fn capture_permit() -> &'static Arc<Semaphore> {
    static PERMIT: OnceLock<Arc<Semaphore>> = OnceLock::new();
    PERMIT.get_or_init(|| Arc::new(Semaphore::new(1)))
}

async fn capture_snapshot(target: ProcessTarget) -> Option<Arc<ProcessSnapshot>> {
    let permit = Arc::clone(capture_permit()).acquire_owned().await.ok()?;

    let capture = tokio::task::spawn_blocking(move || {
        let snapshot = ProcessSnapshot::capture_for(target);
        drop(permit);
        snapshot
    });
    match time::timeout(PROCESS_SNAPSHOT_TIMEOUT, capture).await {
        Ok(Ok(snapshot)) => Some(Arc::new(snapshot)),
        Ok(Err(error)) => {
            tracing::warn!(%error, "process snapshot task failed");
            None
        }
        Err(_) => {
            tracing::warn!(
                timeout_ms = PROCESS_SNAPSHOT_TIMEOUT.as_millis(),
                "process snapshot timed out; retaining cached metadata",
            );
            None
        }
    }
}

#[derive(Debug, Clone)]
struct MetadataRequest {
    foreground_process_group: Option<u32>,
    cwd_override: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PublishedMetadata {
    working_dir: Option<String>,
    running_command: Option<String>,
    detected_agent: Option<String>,
}

pub(super) struct RuntimeMetadataWorker {
    requests: mpsc::Sender<MetadataRequest>,
    cancellation: CancellationToken,
    join_handle: JoinHandle<()>,
}

impl RuntimeMetadataWorker {
    pub(super) fn spawn(
        origin: LocalCoordinatorOrigin,
        child_pid: u32,
        session_events: mpsc::Sender<RuntimeSessionEvent>,
        parent_cancellation: &CancellationToken,
    ) -> Self {
        let cancellation = parent_cancellation.child_token();
        let task_cancellation = cancellation.clone();
        let (requests, request_rx) = mpsc::channel(METADATA_REQUEST_CAPACITY);
        let join_handle = tokio::spawn(run_worker(
            origin,
            child_pid,
            session_events,
            request_rx,
            task_cancellation,
        ));
        Self {
            requests,
            cancellation,
            join_handle,
        }
    }

    pub(super) fn request(&self, pty: &KodosiPty, cwd_override: Option<&str>) -> bool {
        self.requests
            .try_send(MetadataRequest {
                foreground_process_group: pty.foreground_process_group_id(),
                cwd_override: cwd_override.map(PathBuf::from),
            })
            .is_ok()
    }

    pub(super) async fn shutdown(self) {
        self.cancellation.cancel();
        let mut join_handle = self.join_handle;
        if time::timeout(METADATA_WORKER_SHUTDOWN_TIMEOUT, &mut join_handle)
            .await
            .is_err()
        {
            join_handle.abort();
            drop(join_handle.await);
        }
    }
}

async fn run_worker(
    origin: LocalCoordinatorOrigin,
    child_pid: u32,
    session_events: mpsc::Sender<RuntimeSessionEvent>,
    requests: mpsc::Receiver<MetadataRequest>,
    cancellation: CancellationToken,
) {
    run_worker_with_capture(
        origin,
        child_pid,
        session_events,
        requests,
        cancellation,
        capture_snapshot,
    )
    .await;
}

async fn run_worker_with_capture<C, F>(
    origin: LocalCoordinatorOrigin,
    child_pid: u32,
    session_events: mpsc::Sender<RuntimeSessionEvent>,
    mut requests: mpsc::Receiver<MetadataRequest>,
    cancellation: CancellationToken,
    mut capture: C,
) where
    C: FnMut(ProcessTarget) -> F,
    F: Future<Output = Option<Arc<ProcessSnapshot>>>,
{
    let mut last_published: Option<PublishedMetadata> = None;
    let mut pending: Option<MetadataRequest> = None;
    let mut retry_delay = METADATA_CAPTURE_RETRY_BASE;
    loop {
        let request = if let Some(request) = pending.take() {
            request
        } else {
            let request = tokio::select! {
                biased;
                () = cancellation.cancelled() => break,
                request = requests.recv() => request,
            };
            let Some(request) = request else {
                break;
            };
            request
        };
        let target = ProcessTarget::new(child_pid, request.foreground_process_group);
        let snapshot = tokio::select! {
            biased;
            () = cancellation.cancelled() => break,
            snapshot = capture(target) => snapshot,
        };
        let Some(snapshot) = snapshot else {
            let retry = time::sleep(retry_delay);
            tokio::pin!(retry);
            pending = Some(request);
            loop {
                tokio::select! {
                    biased;
                    () = cancellation.cancelled() => return,
                    request = requests.recv() => {
                        match request {
                            Some(request) => pending = Some(request),
                            None => return,
                        }
                    }
                    () = &mut retry => break,
                }
            }
            retry_delay = retry_delay
                .checked_mul(2)
                .unwrap_or(METADATA_CAPTURE_RETRY_MAX)
                .min(METADATA_CAPTURE_RETRY_MAX);
            continue;
        };
        retry_delay = METADATA_CAPTURE_RETRY_BASE;
        let inspection = inspect_process_from_snapshot(
            &snapshot,
            child_pid,
            request.foreground_process_group,
            request.cwd_override.as_deref(),
        );
        let metadata = PublishedMetadata {
            working_dir: inspection
                .working_dir
                .map(|value| value.to_string_lossy().into_owned()),
            running_command: inspection.running_command,
            detected_agent: inspection.detected_agent,
        };
        if last_published.as_ref() == Some(&metadata) {
            continue;
        }

        let event = RuntimeSessionEvent::RuntimeMetadata {
            origin,
            working_dir: metadata.working_dir.clone(),
            running_command: metadata.running_command.clone(),
            detected_agent: metadata.detected_agent.clone(),
        };
        let delivery = tokio::select! {
            biased;
            () = cancellation.cancelled() => break,
            delivery = time::timeout(METADATA_DELIVERY_TIMEOUT, session_events.send(event)) => delivery,
        };
        match delivery {
            Ok(Ok(())) => last_published = Some(metadata),
            Ok(Err(_)) => break,
            Err(_) => tracing::warn!(
                session_id = %origin.session_id,
                "runtime process metadata delivery timed out",
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    use super::{
        METADATA_CAPTURE_RETRY_BASE, MetadataRequest, PublishedMetadata, capture_permit,
        capture_snapshot, run_worker_with_capture,
    };
    use crate::session_runtime::events::{LocalCoordinatorOrigin, RuntimeSessionEvent};
    use kodosi_domain::ids::SessionId;
    use kodosi_session::ProcessSnapshot;

    fn origin(session_id: SessionId) -> LocalCoordinatorOrigin {
        LocalCoordinatorOrigin {
            session_id,
            local_incarnation_id: uuid::Uuid::now_v7(),
        }
    }

    #[tokio::test]
    async fn admitted_snapshot_waits_for_global_permit_instead_of_disappearing() {
        let held = Arc::clone(capture_permit()).acquire_owned().await.unwrap();
        let target = kodosi_session::ProcessTarget::new(std::process::id(), None);
        let capture = tokio::spawn(capture_snapshot(target));
        tokio::task::yield_now().await;
        assert!(!capture.is_finished());
        drop(held);
        let snapshot = tokio::time::timeout(std::time::Duration::from_secs(3), capture)
            .await
            .expect("capture must finish after permit release")
            .expect("capture task must not panic");
        assert!(snapshot.is_some());
    }

    #[tokio::test(start_paused = true)]
    async fn failed_capture_retries_without_new_pty_output() {
        let session_id = SessionId::new();
        let origin = origin(session_id);
        let (requests_tx, requests_rx) = mpsc::channel(1);
        let (events_tx, mut events_rx) = mpsc::channel(1);
        let cancellation = CancellationToken::new();
        let calls = Arc::new(AtomicUsize::new(0));
        requests_tx
            .send(MetadataRequest {
                foreground_process_group: None,
                cwd_override: Some("/retry-success".into()),
            })
            .await
            .expect("request admitted");
        let worker_calls = Arc::clone(&calls);
        let worker = tokio::spawn(run_worker_with_capture(
            origin,
            std::process::id(),
            events_tx,
            requests_rx,
            cancellation.clone(),
            move |_| {
                let call = worker_calls.fetch_add(1, Ordering::SeqCst);
                std::future::ready((call > 0).then(|| Arc::new(ProcessSnapshot::default())))
            },
        ));

        tokio::task::yield_now().await;
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(events_rx.try_recv().is_err());
        tokio::time::advance(METADATA_CAPTURE_RETRY_BASE).await;
        let event = events_rx.recv().await.expect("retry publishes metadata");
        std::assert_matches!(
            event,
            RuntimeSessionEvent::RuntimeMetadata { origin, working_dir: Some(path), .. }
                if origin.session_id == session_id && path == "/retry-success"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        cancellation.cancel();
        worker.await.expect("worker joins");
    }

    #[tokio::test(start_paused = true)]
    async fn newer_request_replaces_failed_capture_during_backoff() {
        let session_id = SessionId::new();
        let origin = origin(session_id);
        let (requests_tx, requests_rx) = mpsc::channel(1);
        let (events_tx, mut events_rx) = mpsc::channel(1);
        let cancellation = CancellationToken::new();
        let calls = Arc::new(AtomicUsize::new(0));
        requests_tx
            .send(MetadataRequest {
                foreground_process_group: Some(1),
                cwd_override: Some("/stale".into()),
            })
            .await
            .expect("first request admitted");
        let worker_calls = Arc::clone(&calls);
        let worker = tokio::spawn(run_worker_with_capture(
            origin,
            std::process::id(),
            events_tx,
            requests_rx,
            cancellation.clone(),
            move |_| {
                let call = worker_calls.fetch_add(1, Ordering::SeqCst);
                std::future::ready((call > 0).then(|| Arc::new(ProcessSnapshot::default())))
            },
        ));

        tokio::task::yield_now().await;
        requests_tx
            .send(MetadataRequest {
                foreground_process_group: Some(2),
                cwd_override: Some("/latest".into()),
            })
            .await
            .expect("latest request admitted");
        tokio::task::yield_now().await;
        tokio::time::advance(METADATA_CAPTURE_RETRY_BASE).await;

        let event = events_rx.recv().await.expect("latest retry publishes");
        std::assert_matches!(
            event,
            RuntimeSessionEvent::RuntimeMetadata { working_dir: Some(path), .. }
                if path == "/latest"
        );
        cancellation.cancel();
        worker.await.expect("worker joins");
    }

    #[tokio::test(start_paused = true)]
    async fn cancellation_interrupts_capture_retry_backoff() {
        let (requests_tx, requests_rx) = mpsc::channel(1);
        let (events_tx, _events_rx) = mpsc::channel(1);
        let cancellation = CancellationToken::new();
        requests_tx
            .send(MetadataRequest {
                foreground_process_group: None,
                cwd_override: None,
            })
            .await
            .expect("request admitted");
        let worker = tokio::spawn(run_worker_with_capture(
            origin(SessionId::new()),
            std::process::id(),
            events_tx,
            requests_rx,
            cancellation.clone(),
            |_| std::future::ready(None),
        ));

        tokio::task::yield_now().await;
        cancellation.cancel();
        tokio::task::yield_now().await;
        assert!(worker.is_finished());
        worker.await.expect("worker joins");
    }

    #[test]
    fn unchanged_metadata_is_deduplicated_before_publish() {
        let first = PublishedMetadata {
            working_dir: Some("/repo".to_owned()),
            running_command: Some("claude".to_owned()),
            detected_agent: Some("Claude Code".to_owned()),
        };
        let unchanged = first.clone();
        let changed = PublishedMetadata {
            running_command: Some("copilot".to_owned()),
            ..first.clone()
        };

        assert_eq!(Some(&first), Some(&unchanged));
        assert_ne!(Some(&first), Some(&changed));
    }
}
