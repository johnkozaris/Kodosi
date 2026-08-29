use std::sync::{Arc, Mutex};

use tokio::sync::broadcast;

use crate::copilot::CopilotCliProvider;
use crate::domain::AgentProviderState;
use crate::domain::ids::SessionId;
use crate::ports::live_provider::LiveAgentProvider;
use crate::runtime::snapshot_bus::SnapshotBus;
use crate::runtime::snapshot_view::SnapshotView;

pub struct CopilotLiveAgentProvider {
    view: Arc<SnapshotView<AgentProviderState>>,
    bus: SnapshotBus,
    inner: Mutex<CopilotCliProvider>,
}

impl CopilotLiveAgentProvider {
    #[must_use]
    pub fn new(session_id: &SessionId, cwd: &str) -> Self {
        let mut inner = CopilotCliProvider::new();

        inner.set_defer_store_reads(true);
        inner.set_session_context(cwd, session_id.as_str());

        inner.force_full_attach();
        inner.tick();
        let initial = inner.snapshot().clone();

        Self {
            view: Arc::new(SnapshotView::new(initial)),
            bus: SnapshotBus::new(),
            inner: Mutex::new(inner),
        }
    }

    fn republish_locked(&self, guard: &mut CopilotCliProvider) {
        if !guard.tick() {
            return;
        }
        self.view.store(guard.snapshot().clone());
        self.bus.publish();
    }

    async fn drain_pending_store_read(&self) {
        let Some((store, session_id)) = self
            .inner
            .lock()
            .ok()
            .and_then(|mut guard| guard.take_pending_store_read())
        else {
            return;
        };

        let read_session_id = session_id.clone();
        let Ok(state) =
            tokio::task::spawn_blocking(move || store.read_state_with_notice(&read_session_id))
                .await
        else {
            return;
        };

        if let Ok(mut guard) = self.inner.lock() {
            guard.apply_store_read_for_session(&session_id, state);
            self.republish_locked(&mut guard);
        }
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) fn new_with_events_path_for_test(
        _session_id: SessionId,
        events_path: std::path::PathBuf,
    ) -> Self {
        let mut inner = CopilotCliProvider::new();
        inner.set_defer_store_reads(true);
        inner.override_events_path_for_test(events_path);
        inner.force_full_attach();
        inner.tick();
        let initial = inner.snapshot().clone();

        Self {
            view: Arc::new(SnapshotView::new(initial)),
            bus: SnapshotBus::new(),
            inner: Mutex::new(inner),
        }
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) fn new_with_store_for_test(
        _session_id: SessionId,
        events_path: std::path::PathBuf,
        store_path: std::path::PathBuf,
        store_session_id: &str,
    ) -> Self {
        let mut inner = CopilotCliProvider::with_store_path(store_path);
        inner.set_defer_store_reads(true);
        inner.override_events_path_for_test(events_path);
        inner.set_session_context("/tmp/proj", store_session_id);
        inner.force_full_attach();
        inner.tick();
        let initial = inner.snapshot().clone();

        Self {
            view: Arc::new(SnapshotView::new(initial)),
            bus: SnapshotBus::new(),
            inner: Mutex::new(inner),
        }
    }
}

impl LiveAgentProvider for CopilotLiveAgentProvider {
    fn current_state(&self) -> Arc<AgentProviderState> {
        self.view.load()
    }

    fn ticks(&self) -> broadcast::Receiver<()> {
        self.bus.subscribe()
    }

    fn feed_terminal_output(&self, _bytes: &[u8]) {}

    fn tick_idle(&self) {
        if let Ok(mut guard) = self.inner.lock()
            && guard.tick()
        {
            self.view.store(guard.snapshot().clone());
            self.bus.publish();
        }
    }

    fn notify_artifact_changed(&self, path: &std::path::Path) {
        let _ = path;
    }

    fn settle_deferred_io(&self) -> crate::ports::live_provider::BoxFuture<'_, ()> {
        Box::pin(self.drain_pending_store_read())
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code may panic on unexpected failures"
)]
mod tests {
    use super::CopilotLiveAgentProvider;
    use crate::domain::ids::SessionId;
    use crate::ports::live_provider::LiveAgentProvider;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn fresh_home() -> TempDir {
        TempDir::new().expect("tempdir")
    }

    fn write_jsonl(path: &PathBuf, lines: &[&str]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent");
        }
        let mut body = lines.join("\n");
        body.push('\n');
        fs::write(path, body).expect("write events");
    }

    #[tokio::test]
    async fn current_snapshot_returns_initial_value() {
        let home = fresh_home();
        let sid = "11111111-1111-1111-1111-111111111111";
        let events = home
            .path()
            .join(format!("session-state/{sid}/events.jsonl"));
        let provider =
            CopilotLiveAgentProvider::new_with_events_path_for_test(SessionId::new(sid), events);
        let snap = provider.current_state();
        assert_eq!(snap.identity.agent_type, "copilot");
    }

    #[tokio::test]
    async fn force_full_attach_picks_up_pre_seeded_events() {
        let home = fresh_home();
        let sid = "22222222-2222-2222-2222-222222222222";
        let events = home
            .path()
            .join(format!("session-state/{sid}/events.jsonl"));
        write_jsonl(
            &events,
            &[
                r#"{"type":"assistant.message","data":{"timestamp":"2025-01-01T00:00:00.000Z","content":"hi"}}"#,
            ],
        );

        let provider =
            CopilotLiveAgentProvider::new_with_events_path_for_test(SessionId::new(sid), events);

        let snap = provider.current_state();
        assert_eq!(snap.identity.agent_type, "copilot");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn settle_deferred_io_folds_in_the_store_read_without_blocking_the_runtime() {
        let home = fresh_home();
        let sid = "99999999-9999-9999-9999-999999999999";
        let db = home.path().join("session-store.db");
        {
            let conn = rusqlite::Connection::open(&db).expect("open db");
            conn.execute_batch(
                "CREATE TABLE schema_version (version INTEGER NOT NULL); \
                 INSERT INTO schema_version(version) VALUES (2); \
                 CREATE TABLE sessions (\
                    id TEXT PRIMARY KEY, cwd TEXT, repository TEXT, branch TEXT, \
                    summary TEXT, created_at TEXT, updated_at TEXT, host_type TEXT\
                 ); \
                 CREATE TABLE turns (\
                    id INTEGER PRIMARY KEY AUTOINCREMENT, session_id TEXT NOT NULL, \
                    turn_index INTEGER NOT NULL, user_message TEXT, \
                    assistant_response TEXT, timestamp TEXT\
                 );",
            )
            .expect("schema");
            conn.execute(
                "INSERT INTO sessions(id, repository, summary, host_type) VALUES (?,?,?,?)",
                rusqlite::params![sid, "owner/repo", "Deferred summary", "terminal"],
            )
            .expect("seed");
        }

        let events = home
            .path()
            .join(format!("session-state/{sid}/events.jsonl"));
        let provider =
            CopilotLiveAgentProvider::new_with_store_for_test(SessionId::new(sid), events, db, sid);

        assert!(
            provider.current_state().suggested_title.is_none(),
            "construction must not perform the SQLite read on the runtime thread"
        );

        provider.tick_idle();
        provider.settle_deferred_io().await;

        assert_eq!(
            provider.current_state().suggested_title.as_deref(),
            Some("Deferred summary"),
            "the deferred read must land via settle_deferred_io"
        );
    }

    #[tokio::test]
    async fn settle_deferred_io_is_a_noop_when_nothing_is_pending() {
        let home = fresh_home();
        let sid = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
        let events = home
            .path()
            .join(format!("session-state/{sid}/events.jsonl"));
        let provider =
            CopilotLiveAgentProvider::new_with_events_path_for_test(SessionId::new(sid), events);
        provider.settle_deferred_io().await;
        provider.settle_deferred_io().await;
    }
}
