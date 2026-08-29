use std::sync::{Arc, Mutex};

use tokio::sync::broadcast;

use crate::claude::ClaudeCodeProvider;
use crate::domain::AgentProviderState;
use crate::domain::ids::SessionId;
use crate::ports::live_provider::LiveAgentProvider;
use crate::runtime::snapshot_bus::SnapshotBus;
use crate::runtime::snapshot_view::SnapshotView;

pub struct ClaudeLiveAgentProvider {
    view: Arc<SnapshotView<AgentProviderState>>,
    bus: SnapshotBus,
    inner: Arc<Mutex<ClaudeCodeProvider>>,
}

impl ClaudeLiveAgentProvider {
    #[must_use]
    pub fn new(session_id: &SessionId, cwd: &str) -> Self {
        let mut inner = ClaudeCodeProvider::new();
        inner.set_session_context(cwd, session_id.as_str());
        inner.request_full_attach();

        inner.tick();
        let initial = inner.snapshot().clone();

        Self {
            view: Arc::new(SnapshotView::new(initial)),
            bus: SnapshotBus::new(),
            inner: Arc::new(Mutex::new(inner)),
        }
    }

    fn republish_locked(&self, guard: &mut ClaudeCodeProvider) {
        if !guard.tick() {
            return;
        }
        self.view.store(guard.snapshot().clone());
        self.bus.publish();
    }

    async fn drain_pending_jsonl_io(&self) {
        let inner = Arc::clone(&self.inner);
        let Ok(changed) = crate::runtime::io_pool::spawn_io(move || {
            let Ok(mut guard) = inner.lock() else {
                return false;
            };
            guard.settle_deferred_jsonl_io();
            guard.tick()
        })
        .await
        else {
            return;
        };
        if changed && let Ok(guard) = self.inner.lock() {
            self.view.store(guard.snapshot().clone());
            self.bus.publish();
        }
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) fn new_with_jsonl_path_for_test(
        _session_id: SessionId,
        jsonl_path: std::path::PathBuf,
    ) -> Self {
        let mut inner = ClaudeCodeProvider::new();
        inner.override_jsonl_path_for_test(jsonl_path);
        inner.force_full_attach();
        inner.tick();
        let initial = inner.snapshot().clone();

        Self {
            view: Arc::new(SnapshotView::new(initial)),
            bus: SnapshotBus::new(),
            inner: Arc::new(Mutex::new(inner)),
        }
    }
}

impl LiveAgentProvider for ClaudeLiveAgentProvider {
    fn current_state(&self) -> Arc<AgentProviderState> {
        self.view.load()
    }

    fn ticks(&self) -> broadcast::Receiver<()> {
        self.bus.subscribe()
    }

    fn feed_terminal_output(&self, bytes: &[u8]) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.feed_terminal_output(bytes);
            self.republish_locked(&mut guard);
        }
    }

    fn tick_idle(&self) {
        if let Ok(mut guard) = self.inner.lock()
            && guard.tick()
        {
            self.view.store(guard.snapshot().clone());
            self.bus.publish();
        }
    }

    fn notify_artifact_changed(&self, path: &std::path::Path) {
        if let Ok(mut guard) = self.inner.lock()
            && guard.current_transcript_path() == Some(path)
        {
            guard.request_incremental_jsonl_scan();
        }
    }

    fn settle_deferred_io(&self) -> crate::ports::live_provider::BoxFuture<'_, ()> {
        Box::pin(self.drain_pending_jsonl_io())
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code may panic on unexpected failures"
)]
mod tests {
    use super::ClaudeLiveAgentProvider;
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
        fs::write(path, body).expect("write jsonl");
    }

    #[tokio::test]
    async fn current_snapshot_returns_initial_value() {
        let provider = ClaudeLiveAgentProvider::new(
            &SessionId::new("11111111-1111-1111-1111-111111111111"),
            "/tmp/x",
        );
        let snap = provider.current_state();
        assert_eq!(snap.identity.agent_type, "claude");
    }

    #[tokio::test]
    async fn ticks_subscriber_receives_publish_after_terminal_feed() {
        let provider = ClaudeLiveAgentProvider::new(
            &SessionId::new("22222222-2222-2222-2222-222222222222"),
            "/tmp/y",
        );
        let mut rx = provider.ticks();

        provider.feed_terminal_output(b"\n");
        let _ = tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv()).await;
    }

    #[tokio::test]
    async fn hot_attach_eager_loads_subagents_without_session_start() {
        let home = fresh_home();
        let session_id = "33333333-3333-3333-3333-333333333333";
        let jsonl = home.path().join(format!("{session_id}.jsonl"));

        write_jsonl(
            &jsonl,
            &[
                r#"{"type":"summary","summary":"hot attach test","leafUuid":"l1"}"#,
                r#"{"type":"assistant","sessionId":"33333333-3333-3333-3333-333333333333","uuid":"u1","timestamp":"2025-01-01T00:00:00.000Z","message":{"id":"m1","model":"claude-sonnet-4","content":[{"type":"tool_use","id":"toolu_1","name":"Agent","input":{"description":"sub work","prompt":"do x","subagent_type":"general-purpose"}}],"usage":{"input_tokens":10,"output_tokens":20,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}"#,
            ],
        );

        let provider = ClaudeLiveAgentProvider::new_with_jsonl_path_for_test(
            SessionId::new(session_id),
            jsonl,
        );

        let snap = provider.current_state();
        assert!(
            snap.sub_agents.is_some(),
            "expected sub_agents to be eager-loaded on attach (P0 fix); got None"
        );
        let tree = snap.sub_agents.as_ref().unwrap();
        assert!(
            !tree.agents.is_empty(),
            "expected at least one subagent from the JSONL Task entry"
        );
    }

    #[tokio::test]
    async fn transcript_append_without_hook_reconciles_deferred() {
        use std::io::Write as _;

        let home = fresh_home();
        let session_id = "77777777-7777-7777-7777-777777777777";
        let jsonl = home.path().join(format!("{session_id}.jsonl"));
        write_jsonl(
            &jsonl,
            &[
                r#"{"type":"assistant","message":{"id":"m1","model":"claude-opus-4-6","content":[],"usage":{"input_tokens":10,"output_tokens":5,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}"#,
            ],
        );
        let provider = ClaudeLiveAgentProvider::new_with_jsonl_path_for_test(
            SessionId::new(session_id),
            jsonl.clone(),
        );
        assert_eq!(
            provider.current_state().identity.model.as_deref(),
            Some("claude-opus-4-6")
        );
        let mut ticks = provider.ticks();
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&jsonl)
            .expect("open transcript");
        writeln!(
            file,
            r#"{{"type":"assistant","message":{{"id":"m2","model":"claude-sonnet-5","content":[]}}}}"#
        )
        .expect("append transcript");

        provider.notify_artifact_changed(&jsonl);
        assert_eq!(
            provider.current_state().identity.model.as_deref(),
            Some("claude-opus-4-6"),
            "filesystem notification must not read the transcript inline"
        );
        assert!(ticks.try_recv().is_err());

        provider.settle_deferred_io().await;

        assert_eq!(
            provider.current_state().identity.model.as_deref(),
            Some("claude-sonnet-5")
        );
        ticks.recv().await.expect("reconciliation publishes a tick");
    }

    #[tokio::test]
    async fn sibling_transcript_notification_is_ignored() {
        let home = fresh_home();
        let session_id = "88888888-8888-8888-8888-888888888888";
        let jsonl = home.path().join(format!("{session_id}.jsonl"));
        write_jsonl(&jsonl, &[]);
        let provider = ClaudeLiveAgentProvider::new_with_jsonl_path_for_test(
            SessionId::new(session_id),
            jsonl,
        );
        let mut ticks = provider.ticks();

        provider.notify_artifact_changed(&home.path().join("sibling.jsonl"));
        provider.settle_deferred_io().await;

        assert!(ticks.try_recv().is_err());
    }
}
