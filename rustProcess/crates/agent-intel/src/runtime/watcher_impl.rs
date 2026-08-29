use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use notify::{Event as NotifyEvent, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::ports::error::{AgentIntelError, Result};
use crate::ports::watcher::{ArtifactWatcher, WatchEvent};

const DEBOUNCE_WINDOW: Duration = Duration::from_millis(150);
const DEBOUNCE_POLL_INTERVAL: Duration = Duration::from_millis(25);

const BRIDGE_CHANNEL_CAPACITY: usize = 1024;

const BROADCAST_CAPACITY: usize = 256;

#[derive(Debug, Clone)]
pub struct WatchTarget {
    pub path: PathBuf,
    pub recursive: bool,
}

pub struct FsArtifactWatcher {
    publish_tx: broadcast::Sender<WatchEvent>,
    shutdown_tx: tokio::sync::Mutex<Option<oneshot::Sender<()>>>,
    join_handle: tokio::sync::Mutex<Option<JoinHandle<()>>>,
}

impl FsArtifactWatcher {
    pub fn spawn(targets: Vec<WatchTarget>) -> Result<Arc<Self>> {
        let (publish_tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let (bridge_tx, bridge_rx) = mpsc::channel(BRIDGE_CHANNEL_CAPACITY);

        let watcher = notify::recommended_watcher(move |res: notify::Result<NotifyEvent>| {
            if let Ok(event) = res
                && bridge_tx.try_send(event).is_err()
            {
                tracing::warn!("artifact watcher bridge channel full; dropping event");
            }
        })
        .map_err(|err| AgentIntelError::Io {
            path: PathBuf::new(),
            source: std::io::Error::other(err.to_string()),
        })?;

        let publish_tx_for_task = publish_tx.clone();
        let join_handle = tokio::spawn(async move {
            run_dispatcher(
                watcher,
                targets,
                bridge_rx,
                publish_tx_for_task,
                shutdown_rx,
            )
            .await;
        });

        Ok(Arc::new(Self {
            publish_tx,
            shutdown_tx: tokio::sync::Mutex::new(Some(shutdown_tx)),
            join_handle: tokio::sync::Mutex::new(Some(join_handle)),
        }))
    }
}

impl ArtifactWatcher for FsArtifactWatcher {
    fn subscribe(&self) -> broadcast::Receiver<WatchEvent> {
        self.publish_tx.subscribe()
    }

    async fn shutdown(&self) -> Result<()> {
        let maybe_tx = {
            let mut guard = self.shutdown_tx.lock().await;
            guard.take()
        };
        if let Some(tx) = maybe_tx {
            let _ignored = tx.send(());
        }
        let maybe_handle = {
            let mut guard = self.join_handle.lock().await;
            guard.take()
        };
        if let Some(handle) = maybe_handle {
            drop(handle.await);
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct AttachmentLease {
    path: PathBuf,
    mode: RecursiveMode,
}

#[derive(Debug)]
struct TargetState {
    target: WatchTarget,
    attachment: Option<AttachmentLease>,
}

impl TargetState {
    fn new(target: WatchTarget) -> Self {
        Self {
            target,
            attachment: None,
        }
    }

    fn resolve_attach_path(&self) -> Option<PathBuf> {
        let mut candidate: PathBuf = self.target.path.clone();
        loop {
            if candidate.exists() {
                return Some(candidate);
            }
            match candidate.parent() {
                Some(parent) if parent != candidate.as_path() => {
                    candidate = parent.to_path_buf();
                }
                _ => return None,
            }
        }
    }
}

#[derive(Debug, Default)]
struct AttachmentState {
    non_recursive_leases: usize,
    recursive_leases: usize,
    installed_mode: Option<RecursiveMode>,
}

impl AttachmentState {
    const fn effective_mode(&self) -> RecursiveMode {
        if self.recursive_leases > 0 {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        }
    }

    fn acquire(&mut self, mode: RecursiveMode) {
        match mode {
            RecursiveMode::Recursive => self.recursive_leases += 1,
            RecursiveMode::NonRecursive => self.non_recursive_leases += 1,
        }
    }

    fn release(&mut self, mode: RecursiveMode) {
        match mode {
            RecursiveMode::Recursive => self.recursive_leases -= 1,
            RecursiveMode::NonRecursive => self.non_recursive_leases -= 1,
        }
    }

    const fn is_empty(&self) -> bool {
        self.non_recursive_leases == 0 && self.recursive_leases == 0
    }
}

type Attachments = HashMap<PathBuf, AttachmentState>;

async fn run_dispatcher(
    mut watcher: RecommendedWatcher,
    targets: Vec<WatchTarget>,
    mut bridge_rx: mpsc::Receiver<NotifyEvent>,
    publish_tx: broadcast::Sender<WatchEvent>,
    mut shutdown_rx: oneshot::Receiver<()>,
) {
    let mut states: Vec<TargetState> = targets.into_iter().map(TargetState::new).collect();
    let mut attachments = Attachments::new();
    for state in &mut states {
        attach_target(&mut watcher, &mut attachments, state);
    }

    let mut pending: HashMap<PathBuf, Instant> = HashMap::new();
    let mut debounce_tick = tokio::time::interval(DEBOUNCE_POLL_INTERVAL);
    debounce_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            biased;
            _ = &mut shutdown_rx => break,

            Some(event) = bridge_rx.recv() => {
                handle_event(
                    event,
                    &mut watcher,
                    &mut attachments,
                    &mut states,
                    &mut pending,
                );
            }

            _ = debounce_tick.tick() => {
                publish_due_events(&publish_tx, &mut pending, Instant::now());
            }

            else => break,
        }
    }

    for state in states {
        if let Some(lease) = state.attachment {
            release_attachment(&mut watcher, &mut attachments, &lease);
        }
    }
}

trait NativeWatcher {
    fn install(&mut self, path: &std::path::Path, mode: RecursiveMode) -> notify::Result<()>;
    fn remove(&mut self, path: &std::path::Path) -> notify::Result<()>;
}

impl NativeWatcher for RecommendedWatcher {
    fn install(&mut self, path: &std::path::Path, mode: RecursiveMode) -> notify::Result<()> {
        self.watch(path, mode)
    }

    fn remove(&mut self, path: &std::path::Path) -> notify::Result<()> {
        self.unwatch(path)
    }
}

fn attach_target(
    watcher: &mut impl NativeWatcher,
    attachments: &mut Attachments,
    state: &mut TargetState,
) {
    let Some(path_to_watch) = state.resolve_attach_path() else {
        tracing::warn!(
            target = %state.target.path.display(),
            "artifact watcher: no existing ancestor to attach to"
        );
        return;
    };

    let mode = if path_to_watch == state.target.path && state.target.recursive {
        RecursiveMode::Recursive
    } else {
        RecursiveMode::NonRecursive
    };
    if state
        .attachment
        .as_ref()
        .is_some_and(|lease| lease.path == path_to_watch && lease.mode == mode)
    {
        return;
    }

    if let Err(error) = acquire_attachment(watcher, attachments, &path_to_watch, mode) {
        tracing::warn!(
            target = %state.target.path.display(),
            attempted = %path_to_watch.display(),
            %error,
            "artifact watcher attach failed"
        );
        return;
    }

    let previous = state.attachment.replace(AttachmentLease {
        path: path_to_watch.clone(),
        mode,
    });
    if let Some(previous) = previous {
        release_attachment(watcher, attachments, &previous);
    }
    tracing::debug!(
        target = %state.target.path.display(),
        attached_at = %path_to_watch.display(),
        "artifact watcher attached"
    );
}

fn acquire_attachment(
    watcher: &mut impl NativeWatcher,
    attachments: &mut Attachments,
    path: &PathBuf,
    requested: RecursiveMode,
) -> notify::Result<()> {
    let state = attachments.entry(path.clone()).or_default();
    let previous_mode = state.installed_mode;
    state.acquire(requested);
    let next_mode = state.effective_mode();
    if previous_mode == Some(next_mode) {
        return Ok(());
    }
    if previous_mode.is_some() {
        watcher.remove(path)?;
    }
    if let Err(error) = watcher.install(path, next_mode) {
        state.release(requested);
        if let Some(previous_mode) = previous_mode {
            if watcher.install(path, previous_mode).is_ok() {
                state.installed_mode = Some(previous_mode);
            } else {
                state.installed_mode = None;
            }
        } else if state.is_empty() {
            attachments.remove(path);
        }
        return Err(error);
    }
    state.installed_mode = Some(next_mode);
    Ok(())
}

fn release_attachment(
    watcher: &mut impl NativeWatcher,
    attachments: &mut Attachments,
    lease: &AttachmentLease,
) {
    let Some(state) = attachments.get_mut(&lease.path) else {
        return;
    };
    state.release(lease.mode);
    if state.is_empty() {
        let installed = state.installed_mode.take();
        attachments.remove(&lease.path);
        if installed.is_some() {
            drop(watcher.remove(&lease.path));
        }
        return;
    }
    let previous_mode = state.installed_mode;
    let next_mode = state.effective_mode();
    if previous_mode == Some(next_mode) {
        return;
    }
    if previous_mode.is_some() {
        drop(watcher.remove(&lease.path));
    }
    match watcher.install(&lease.path, next_mode) {
        Ok(()) => state.installed_mode = Some(next_mode),
        Err(error) => {
            state.installed_mode = None;
            tracing::warn!(
                path = %lease.path.display(),
                %error,
                "artifact watcher mode downgrade failed"
            );
        }
    }
}

fn handle_event(
    event: NotifyEvent,
    watcher: &mut RecommendedWatcher,
    attachments: &mut Attachments,
    states: &mut [TargetState],
    pending: &mut HashMap<PathBuf, Instant>,
) {
    for path in event.paths {
        for state in states.iter_mut() {
            let attached_at_is_ancestor = state
                .attachment
                .as_ref()
                .is_some_and(|lease| path.starts_with(&lease.path));
            if attached_at_is_ancestor
                && state.attachment.as_ref().map(|lease| lease.path.as_path())
                    != Some(state.target.path.as_path())
                && state.resolve_attach_path().as_deref()
                    != state.attachment.as_ref().map(|lease| lease.path.as_path())
            {
                attach_target(watcher, attachments, state);
            }
        }

        if states
            .iter()
            .any(|state| path.starts_with(&state.target.path))
        {
            pending.insert(path, Instant::now());
        }
    }
}

fn publish_due_events(
    publish_tx: &broadcast::Sender<WatchEvent>,
    pending: &mut HashMap<PathBuf, Instant>,
    now: Instant,
) {
    let due = pending
        .iter()
        .filter(|(_, last_seen)| now.duration_since(**last_seen) >= DEBOUNCE_WINDOW)
        .map(|(path, _)| path.clone())
        .collect::<Vec<_>>();
    for path in due {
        pending.remove(&path);
        drop(publish_tx.send(WatchEvent { path }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;
    use tokio::time::timeout;

    async fn drain(rx: &mut broadcast::Receiver<WatchEvent>, dur: Duration) -> Vec<WatchEvent> {
        let mut out = Vec::new();
        loop {
            match timeout(dur, rx.recv()).await {
                Ok(Ok(ev)) => out.push(ev),
                _ => return out,
            }
        }
    }

    const WATCH_EVENT_TIMEOUT: Duration = Duration::from_secs(3);

    fn canonicalise(p: &Path) -> PathBuf {
        fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
    }

    #[derive(Default)]
    struct RecordingWatcher {
        operations: Vec<(bool, PathBuf, Option<RecursiveMode>)>,
    }

    impl NativeWatcher for RecordingWatcher {
        fn install(&mut self, path: &Path, mode: RecursiveMode) -> notify::Result<()> {
            self.operations.push((true, path.to_path_buf(), Some(mode)));
            Ok(())
        }

        fn remove(&mut self, path: &Path) -> notify::Result<()> {
            self.operations.push((false, path.to_path_buf(), None));
            Ok(())
        }
    }

    #[test]
    fn shared_attachment_unwatches_only_after_last_lease() {
        let path = PathBuf::from("/tmp/shared-ancestor");
        let mut watcher = RecordingWatcher::default();
        let mut attachments = Attachments::new();

        acquire_attachment(
            &mut watcher,
            &mut attachments,
            &path,
            RecursiveMode::NonRecursive,
        )
        .unwrap();
        acquire_attachment(
            &mut watcher,
            &mut attachments,
            &path,
            RecursiveMode::NonRecursive,
        )
        .unwrap();
        assert_eq!(
            watcher.operations.len(),
            1,
            "one native watch for two leases"
        );

        release_attachment(
            &mut watcher,
            &mut attachments,
            &AttachmentLease {
                path: path.clone(),
                mode: RecursiveMode::NonRecursive,
            },
        );
        assert_eq!(
            watcher.operations.len(),
            1,
            "first release keeps native watch"
        );

        release_attachment(
            &mut watcher,
            &mut attachments,
            &AttachmentLease {
                path: path.clone(),
                mode: RecursiveMode::NonRecursive,
            },
        );
        assert_eq!(
            watcher.operations,
            vec![
                (true, path.clone(), Some(RecursiveMode::NonRecursive)),
                (false, path, None),
            ]
        );
    }

    #[test]
    fn recursive_lease_upgrades_then_downgrades_shared_attachment() {
        let path = PathBuf::from("/tmp/shared-mode");
        let mut watcher = RecordingWatcher::default();
        let mut attachments = Attachments::new();

        acquire_attachment(
            &mut watcher,
            &mut attachments,
            &path,
            RecursiveMode::NonRecursive,
        )
        .unwrap();
        acquire_attachment(
            &mut watcher,
            &mut attachments,
            &path,
            RecursiveMode::Recursive,
        )
        .unwrap();
        release_attachment(
            &mut watcher,
            &mut attachments,
            &AttachmentLease {
                path: path.clone(),
                mode: RecursiveMode::Recursive,
            },
        );

        assert_eq!(
            watcher.operations,
            vec![
                (true, path.clone(), Some(RecursiveMode::NonRecursive)),
                (false, path.clone(), None),
                (true, path.clone(), Some(RecursiveMode::Recursive)),
                (false, path.clone(), None),
                (true, path, Some(RecursiveMode::NonRecursive)),
            ]
        );
    }

    #[tokio::test]
    async fn emits_event_on_file_create_under_watched_dir() {
        let temp = TempDir::new().unwrap();
        let dir = canonicalise(temp.path());
        let watcher = FsArtifactWatcher::spawn(vec![WatchTarget {
            path: dir.clone(),
            recursive: false,
        }])
        .unwrap();
        let mut rx = watcher.subscribe();

        tokio::time::sleep(Duration::from_millis(100)).await;

        fs::write(dir.join("hello.txt"), "hi").unwrap();

        let events = drain(&mut rx, WATCH_EVENT_TIMEOUT).await;
        assert!(!events.is_empty(), "expected at least one event");

        watcher.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn walks_up_when_target_does_not_exist() {
        let temp = TempDir::new().unwrap();
        let temp_canon = canonicalise(temp.path());
        let nested = temp_canon.join("not").join("yet").join("there");
        let watcher = FsArtifactWatcher::spawn(vec![WatchTarget {
            path: nested.clone(),
            recursive: false,
        }])
        .unwrap();
        let mut rx = watcher.subscribe();

        tokio::time::sleep(Duration::from_millis(100)).await;

        fs::create_dir_all(&nested).unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;
        fs::write(nested.join("appeared.txt"), "x").unwrap();

        let events = drain(&mut rx, WATCH_EVENT_TIMEOUT).await;
        assert!(
            !events.is_empty(),
            "watcher should have emitted at least one event after walk-up re-attach"
        );

        watcher.shutdown().await.unwrap();
    }

    #[test]
    fn debounce_emits_only_after_the_last_event_in_a_burst() {
        let path = PathBuf::from("/tmp/settings.json");
        let started = Instant::now();
        let mut pending = HashMap::from([(path.clone(), started)]);
        let (tx, mut rx) = broadcast::channel(4);

        publish_due_events(
            &tx,
            &mut pending,
            (started + DEBOUNCE_WINDOW)
                .checked_sub(Duration::from_millis(1))
                .expect("one millisecond is below the debounce window"),
        );
        assert!(rx.try_recv().is_err());

        let final_seen = started + Duration::from_millis(100);
        pending.insert(path.clone(), final_seen);
        publish_due_events(&tx, &mut pending, started + DEBOUNCE_WINDOW);
        assert!(
            rx.try_recv().is_err(),
            "the first deadline must not publish a stale read"
        );

        publish_due_events(&tx, &mut pending, final_seen + DEBOUNCE_WINDOW);
        let event = rx.try_recv().expect("the trailing event must be published");
        assert_eq!(event.path, path);
        assert!(pending.is_empty());
    }

    #[tokio::test]
    async fn debounces_rapid_writes_to_same_path() {
        let temp = TempDir::new().unwrap();
        let dir = canonicalise(temp.path());
        let watcher = FsArtifactWatcher::spawn(vec![WatchTarget {
            path: dir.clone(),
            recursive: false,
        }])
        .unwrap();
        let mut rx = watcher.subscribe();

        tokio::time::sleep(Duration::from_millis(100)).await;

        let path = dir.join("burst.txt");
        for i in 0..5u8 {
            fs::write(&path, [i]).unwrap();
        }

        let events = drain(&mut rx, Duration::from_millis(500)).await;
        let same_path_events = events.iter().filter(|e| e.path == path).count();
        assert!(
            same_path_events <= 2,
            "debounce should coalesce burst writes (got {same_path_events})"
        );

        watcher.shutdown().await.unwrap();
    }
}
