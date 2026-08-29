use std::path::PathBuf;
use std::time::Instant;

use super::jsonl::CopilotEventsReader;
use super::session_store_db::{CopilotSessionStore, SessionStoreRead, SessionStoreState};
use crate::domain::{
    AgentIdentity, AgentProviderState, AgentStatus, HealthStatus, SettingsFilePath, SettingsScope,
};

pub struct CopilotCliProvider {
    snapshot: AgentProviderState,
    dirty: bool,
    reader: CopilotEventsReader,
    attached_session_id: Option<String>,
    store: CopilotSessionStore,
    last_store_read_at: Option<Instant>,
    session_store_state: Option<SessionStoreState>,
    last_scanned_cwd: Option<PathBuf>,

    defer_store_reads: bool,
    pending_store_read: bool,
}

impl Default for CopilotCliProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl CopilotCliProvider {
    #[must_use]
    pub fn new() -> Self {
        let snapshot = AgentProviderState {
            identity: AgentIdentity {
                agent_type: "copilot".to_owned(),
                version: None,
                model: None,
                parser_compat: None,
            },
            status: AgentStatus::Idle,
            health: HealthStatus::Unknown,
            ..AgentProviderState::default()
        };
        Self {
            snapshot,
            dirty: true,
            reader: CopilotEventsReader::default(),
            attached_session_id: None,
            store: CopilotSessionStore::new(super::filesystem::session_store_db_path()),
            last_store_read_at: None,
            session_store_state: None,
            last_scanned_cwd: None,
            defer_store_reads: false,
            pending_store_read: false,
        }
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) fn with_store_path(store_path: PathBuf) -> Self {
        let mut p = Self::new();
        p.store = CopilotSessionStore::new(store_path);
        p
    }

    #[cfg(test)]
    pub(crate) fn override_events_path_for_test(&mut self, path: PathBuf) {
        self.reader.set_path(path);
    }

    #[must_use]
    pub const fn snapshot(&self) -> &AgentProviderState {
        &self.snapshot
    }

    pub fn force_full_attach(&mut self) {
        if self.reader.read_full() {
            self.apply_events_state();
        }
    }

    const STORE_DEBOUNCE_SECS: u64 = 30;

    pub(crate) const fn set_defer_store_reads(&mut self, defer: bool) {
        self.defer_store_reads = defer;
    }

    pub(crate) fn take_pending_store_read(&mut self) -> Option<(CopilotSessionStore, String)> {
        if !std::mem::take(&mut self.pending_store_read) {
            return None;
        }
        let session_id = self.attached_session_id.clone()?;
        Some((self.store.clone(), session_id))
    }

    fn refresh_session_store(&mut self, force: bool) {
        let Some(session_id) = self.attached_session_id.clone() else {
            return;
        };

        if !force
            && self
                .last_store_read_at
                .is_some_and(|t| t.elapsed().as_secs() < Self::STORE_DEBOUNCE_SECS)
        {
            return;
        }

        self.last_store_read_at = Some(Instant::now());

        if self.defer_store_reads {
            self.pending_store_read = true;
            return;
        }

        let new_state = self.store.read_state_with_notice(&session_id);
        self.apply_store_read(new_state);
    }

    pub(crate) fn apply_store_read(&mut self, read: SessionStoreRead) {
        let new_state = match read {
            SessionStoreRead::Missing => {
                let store_changed = self.session_store_state.take().is_some();
                let title_changed = self.snapshot.suggested_title.take().is_some();
                let changed = store_changed || title_changed;
                if changed {
                    self.dirty = true;
                }
                return;
            }
            SessionStoreRead::Degraded(notice) => {
                self.session_store_state = None;
                self.snapshot.suggested_title = None;
                tracing::debug!(?notice, "Copilot session store projection degraded");
                self.dirty = true;
                return;
            }
            SessionStoreRead::State(state) => state,
        };

        if self.session_store_state.as_ref() == Some(&new_state) {
            return;
        }

        let store_summary = new_state.current_summary.clone();
        if self.snapshot.suggested_title != store_summary {
            self.snapshot.suggested_title = store_summary;
        }

        self.session_store_state = Some(new_state);
        self.dirty = true;
    }

    pub(crate) fn apply_store_read_for_session(
        &mut self,
        session_id: &str,
        read: SessionStoreRead,
    ) {
        if self.attached_session_id.as_deref() == Some(session_id) {
            self.apply_store_read(read);
        }
    }

    fn clear_events_derived_state(&mut self) {
        self.reset_events_projection();
        self.snapshot.error = None;
        self.snapshot.suggested_title = None;
        self.session_store_state = None;
        self.last_store_read_at = None;
        self.pending_store_read = false;
        self.dirty = true;
    }

    fn reset_events_projection(&mut self) {
        self.snapshot.sub_agents = None;
        self.snapshot.identity.model = None;
        self.snapshot.identity.version = None;
        self.snapshot.identity.parser_compat = None;
        self.snapshot.status = AgentStatus::Idle;
        self.snapshot.health = HealthStatus::Unknown;
        self.snapshot.hitl_prompt = None;
        self.snapshot.activity = None;
    }

    fn apply_events_state(&mut self) {
        let Some(state) = self.reader.take_state() else {
            return;
        };
        let saw_shutdown = state.saw_shutdown;
        if state.projection_reset {
            self.reset_events_projection();
        }

        if state.projection_reset || state.sub_agents.is_some() {
            self.snapshot.sub_agents = state
                .sub_agents
                .and_then(|tree| (!tree.agents.is_empty()).then_some(tree));
        }
        self.snapshot.identity.model = state.model;
        self.snapshot.identity.version = state.version;
        let clears_error = matches!(state.health, Some(HealthStatus::Healthy))
            || matches!(state.status, Some(AgentStatus::Running | AgentStatus::Idle));
        if let Some(cwd) = state.cwd
            && self.snapshot.cwd.as_deref() != Some(cwd.as_str())
        {
            self.snapshot.cwd = Some(cwd);
        }
        if let Some(intent) = state.activity_intent
            && self.snapshot.activity.as_deref() != Some(intent.as_str())
        {
            self.snapshot.activity = Some(intent);
        }
        if state.clear_activity {
            self.snapshot.activity = None;
        }
        if let Some(status) = state.status {
            self.snapshot.status = status;
        }
        if let Some(health) = state.health {
            self.snapshot.health = health;
        }
        if clears_error {
            self.snapshot.error = None;
        }
        if let Some(hitl_prompt) = state.hitl_prompt {
            self.snapshot.hitl_prompt = hitl_prompt;
        }
        if let Some(error) = state.error {
            self.snapshot.error = Some(error);
        }
        let notices = state
            .adapter_errors
            .into_iter()
            .map(|message| crate::domain::DegradationNotice::MalformedCopilotEvent { message })
            .collect();
        self.snapshot.identity.parser_compat = crate::domain::ParserCompatNotice::from_parts(
            state.unknown_record_types,
            state.unrecognized_envelopes,
            notices,
        );
        self.dirty = true;

        if saw_shutdown {
            self.refresh_session_store(true);
        }
    }
}

impl CopilotCliProvider {
    pub fn set_session_context(&mut self, cwd: &str, session_id: &str) {
        if self.snapshot.cwd.as_deref() != Some(cwd) {
            self.snapshot.cwd = Some(cwd.to_owned());
            self.dirty = true;
        }

        if let Some(path) = self.transcript_path(cwd, session_id) {
            self.reader.set_path(path);
        }

        let session_changed = self.attached_session_id.as_deref() != Some(session_id);
        if session_changed {
            self.attached_session_id = Some(session_id.to_owned());
            self.clear_events_derived_state();
            self.reader.read_full();
            self.apply_events_state();
        }

        let effective_cwd = self
            .snapshot
            .cwd
            .clone()
            .map_or_else(|| PathBuf::from(cwd), PathBuf::from);

        let cwd_changed = self.last_scanned_cwd.as_deref() != Some(effective_cwd.as_path());
        if session_changed || cwd_changed {
            self.last_scanned_cwd = Some(effective_cwd);
        }

        if session_changed {
            self.refresh_session_store(true);
        }
    }

    pub fn tick(&mut self) -> bool {
        if self.reader.read_new() {
            self.apply_events_state();
        }
        self.refresh_session_store(false);

        let was_dirty = self.dirty;
        self.dirty = false;
        was_dirty
    }

    #[cfg(test)]
    fn poll_snapshot(&mut self) -> Option<serde_json::Value> {
        self.tick()
            .then(|| serde_json::to_value(&self.snapshot).expect("snapshot must serialize"))
    }
}

impl CopilotCliProvider {
    pub fn settings_paths(&self, cwd: &str) -> Vec<SettingsFilePath> {
        let home = super::filesystem::copilot_home();
        let project_root = PathBuf::from(cwd);
        let mut paths = vec![SettingsFilePath {
            scope: SettingsScope::User,
            path: home.join("settings.json"),
        }];
        paths.push(SettingsFilePath {
            scope: SettingsScope::Project,
            path: project_root.join(".github/copilot/settings.json"),
        });
        paths.push(SettingsFilePath {
            scope: SettingsScope::Local,
            path: project_root.join(".github/copilot/settings.local.json"),
        });
        paths
    }

    pub fn transcript_path(&self, _cwd: &str, session_id: &str) -> Option<PathBuf> {
        Some(super::filesystem::events_jsonl_path(session_id))
    }

    pub fn transcript_root(&self) -> Option<PathBuf> {
        Some(super::filesystem::copilot_home().join("session-state"))
    }

    pub fn transcript_decoder(&self) -> &'static dyn crate::TranscriptDecoder {
        crate::copilot::transcript::CopilotTranscriptDecoder::INSTANCE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_matches_expected() {
        let provider = CopilotCliProvider::new();
        assert_eq!(provider.snapshot().identity.agent_type, "copilot");
    }

    #[test]
    fn malformed_jsonl_record_surfaces_typed_parser_degradation() {
        let dir = tempfile::tempdir().unwrap();
        let events = dir.path().join("events.jsonl");
        std::fs::write(
            &events,
            "not-json\n{\"type\":\"assistant.message\",\"data\":{\"outputTokens\":7}}\n",
        )
        .unwrap();
        let mut provider = CopilotCliProvider::new();
        provider.override_events_path_for_test(events);
        provider.force_full_attach();

        let notice = provider
            .snapshot()
            .identity
            .parser_compat
            .as_ref()
            .expect("malformed event must badge snapshot");
        assert!(notice.degradation_notices.iter().any(|notice| matches!(
            notice,
            crate::domain::DegradationNotice::MalformedCopilotEvent { message }
                if message.contains("invalid JSON")
        )));
    }

    #[test]
    fn first_poll_emits_snapshot() {
        let mut provider = CopilotCliProvider::new();
        let snap = provider.poll_snapshot();
        assert!(
            snap.is_some(),
            "first poll must emit identity-bearing snapshot"
        );
        let value = snap.unwrap();
        assert_eq!(
            value
                .get("identity")
                .and_then(|v| v.get("agentType"))
                .and_then(|v| v.as_str()),
            Some("copilot")
        );
    }

    #[test]
    fn session_producer_version_populates_identity_only() {
        let dir = tempfile::tempdir().unwrap();
        let events = dir.path().join("events.jsonl");
        std::fs::write(
            &events,
            "{\"type\":\"session.start\",\"data\":{\"copilotVersion\":\"1.0.99\"}}\n",
        )
        .unwrap();
        let mut provider = CopilotCliProvider::new();
        provider.override_events_path_for_test(events);
        provider.force_full_attach();

        assert_eq!(
            provider.snapshot.identity.version.as_deref(),
            Some("1.0.99")
        );
    }

    #[test]
    fn second_poll_is_silent() {
        let mut provider = CopilotCliProvider::new();
        drop(provider.poll_snapshot());
        assert!(
            provider.poll_snapshot().is_none(),
            "no state change ⇒ no snapshot emission"
        );
    }

    #[test]
    fn cwd_change_marks_dirty() {
        let mut provider = CopilotCliProvider::new();
        drop(provider.poll_snapshot());
        provider.set_session_context("/tmp/proj", "abc-123");
        assert!(
            provider.poll_snapshot().is_some(),
            "cwd transition should re-emit"
        );
    }

    #[test]
    fn settings_paths_are_resolved() {
        let provider = CopilotCliProvider::new();
        let paths = provider.settings_paths("/tmp/proj");
        assert_eq!(paths.len(), 3);
        std::assert_matches!(paths[0].scope, SettingsScope::User);
        assert!(paths[0].path.ends_with(".copilot/settings.json"));
        std::assert_matches!(paths[1].scope, SettingsScope::Project);
        assert!(paths[1].path.ends_with(".github/copilot/settings.json"));
    }

    #[test]
    fn transcript_path_uses_session_state() {
        let provider = CopilotCliProvider::new();
        let path = provider
            .transcript_path("/tmp/proj", "abc-123")
            .expect("transcript path");
        assert!(path.ends_with("session-state/abc-123/events.jsonl"));
    }

    fn seed_store_db(path: &std::path::Path, session_id: &str) {
        use rusqlite::Connection;
        let conn = Connection::open(path).unwrap();
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
        .unwrap();
        conn.execute(
            "INSERT INTO sessions(id, repository, summary, host_type, created_at) \
             VALUES (?, ?, ?, ?, ?)",
            rusqlite::params![
                session_id,
                "owner/repo",
                "My summary",
                "terminal",
                "2026-04-25T00:00:00Z",
            ],
        )
        .unwrap();
    }

    #[test]
    fn attach_reads_store_and_surfaces_summary_only() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("session-store.db");
        seed_store_db(&db, "session-a");

        let mut provider = CopilotCliProvider::with_store_path(db);
        provider.set_session_context("/tmp/proj", "session-a");
        let value = provider.poll_snapshot().expect("snapshot after attach");

        assert_eq!(
            value.get("suggestedTitle").and_then(|v| v.as_str()),
            Some("My summary"),
            "store summary flows into top-level suggestedTitle"
        );
    }

    #[test]
    fn missing_store_db_does_not_block_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let mut provider = CopilotCliProvider::with_store_path(dir.path().join("absent.db"));
        provider.set_session_context("/tmp/proj", "session-a");
        let value = provider.poll_snapshot().expect("snapshot after attach");

        assert!(
            value
                .get("suggestedTitle")
                .and_then(|v| v.as_str())
                .is_none()
        );
    }

    #[test]
    fn session_id_change_clears_prior_store_state() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("session-store.db");
        seed_store_db(&db, "session-a");

        let mut provider = CopilotCliProvider::with_store_path(db);
        provider.set_session_context("/tmp/proj", "session-a");
        drop(provider.poll_snapshot());

        provider.set_session_context("/tmp/proj", "session-ghost");
        let value = provider.poll_snapshot().expect("snapshot after switch");
        assert!(
            value
                .get("suggestedTitle")
                .and_then(|v| v.as_str())
                .is_none(),
            "prior session summary must not leak"
        );
    }

    #[test]
    fn stale_deferred_store_read_cannot_repopulate_new_session() {
        use std::collections::BTreeMap;

        let dir = tempfile::tempdir().unwrap();
        let mut provider =
            CopilotCliProvider::with_store_path(dir.path().join("missing-session-store.db"));
        provider.set_session_context("/tmp/proj", "session-a");
        provider.set_session_context("/tmp/proj", "session-b");
        provider.apply_store_read_for_session(
            "session-a",
            SessionStoreRead::State(SessionStoreState {
                schema_version: 1,
                current_summary: Some("old summary".to_owned()),
                current_host_type: None,
                current_created_at: None,
                current_repository: None,
                repo_total_sessions: 1,
                repo_total_turns: 1,
                additional_fields: BTreeMap::new(),
            }),
        );

        assert!(provider.snapshot().suggested_title.is_none());
        assert!(provider.session_store_state.is_none());
    }

    #[test]
    fn shutdown_event_bypasses_debounce_and_refreshes_summary() {
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("session-store.db");
        let session_id = "session-a";
        seed_store_db(&db, session_id);

        let events_path = dir.path().join("events.jsonl");
        std::fs::write(&events_path, b"").unwrap();

        let mut provider = CopilotCliProvider::with_store_path(db.clone());
        provider.set_session_context("/tmp/proj", session_id);
        provider.override_events_path_for_test(events_path.clone());

        let v = provider.poll_snapshot().expect("first snapshot");
        assert_eq!(
            v.get("suggestedTitle").and_then(|v| v.as_str()),
            Some("My summary"),
            "attach must have read the DB"
        );

        {
            let conn = rusqlite::Connection::open(&db).unwrap();
            conn.execute(
                "UPDATE sessions SET summary = ? WHERE id = ?",
                rusqlite::params!["Final summary", session_id],
            )
            .unwrap();
        }

        let v = provider.poll_snapshot();
        if let Some(v) = v {
            assert_eq!(
                v.get("suggestedTitle").and_then(|v| v.as_str()),
                Some("My summary"),
                "debounce must suppress re-read inside 30s window"
            );
        }

        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&events_path)
            .unwrap();
        writeln!(
            f,
            r#"{{"type":"session.shutdown","data":{{"totalPremiumRequests":1}}}}"#
        )
        .unwrap();
        drop(f);

        let v = provider
            .poll_snapshot()
            .expect("shutdown must force re-read and re-emit");
        assert_eq!(
            v.get("suggestedTitle").and_then(|v| v.as_str()),
            Some("Final summary"),
            "session.shutdown must bypass the 30s debounce"
        );
    }

    #[test]
    fn missing_store_db_debounces_further_reads() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("session-store.db");

        let mut provider = CopilotCliProvider::with_store_path(db.clone());
        provider.set_session_context("/tmp/proj", "session-a");
        drop(provider.poll_snapshot());

        seed_store_db(&db, "session-a");
        drop(provider.poll_snapshot());

        assert!(
            provider.snapshot().suggested_title.is_none(),
            "a miss must arm the debounce, not re-read SQLite on every tick"
        );
    }

    #[test]
    fn deferred_store_reads_are_handed_out_instead_of_run_inline() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("session-store.db");
        seed_store_db(&db, "session-a");

        let mut provider = CopilotCliProvider::with_store_path(db);
        provider.set_defer_store_reads(true);
        provider.set_session_context("/tmp/proj", "session-a");

        assert!(
            provider.snapshot().suggested_title.is_none(),
            "deferred mode must not touch SQLite inline"
        );

        let (store, session_id) = provider
            .take_pending_store_read()
            .expect("attach must raise a deferred read request");
        assert_eq!(session_id, "session-a");
        assert!(
            provider.take_pending_store_read().is_none(),
            "the request is claimed exactly once"
        );

        provider.apply_store_read(store.read_state_with_notice(&session_id));
        assert_eq!(
            provider.snapshot().suggested_title.as_deref(),
            Some("My summary")
        );
    }

    #[test]
    fn degraded_store_read_clears_stale_projection_and_surfaces_notice() {
        let mut provider = CopilotCliProvider::new();
        provider.apply_store_read(SessionStoreRead::State(SessionStoreState {
            schema_version: 3,
            current_summary: Some("stale summary".to_owned()),
            current_host_type: Some("terminal".to_owned()),
            current_created_at: None,
            current_repository: Some("owner/repo".to_owned()),
            repo_total_sessions: 1,
            repo_total_turns: 2,
            additional_fields: std::collections::BTreeMap::new(),
        }));
        assert_eq!(
            provider.snapshot().suggested_title.as_deref(),
            Some("stale summary")
        );

        provider.apply_store_read(SessionStoreRead::Degraded(
            crate::domain::DegradationNotice::CopilotSessionStoreFailure {
                failure: crate::domain::CopilotSessionStoreFailureKind::Query,
                message: "no such table: sessions".to_owned(),
            },
        ));

        assert!(provider.snapshot().suggested_title.is_none());
    }

    #[test]
    fn missing_store_clears_store_and_title_independently() {
        let mut provider = CopilotCliProvider::new();
        provider.apply_store_read(SessionStoreRead::State(SessionStoreState {
            schema_version: 3,
            current_summary: Some("stale".to_owned()),
            current_host_type: None,
            current_created_at: None,
            current_repository: None,
            repo_total_sessions: 0,
            repo_total_turns: 0,
            additional_fields: std::collections::BTreeMap::new(),
        }));
        provider.apply_store_read(SessionStoreRead::Missing);

        assert!(provider.session_store_state.is_none());
        assert!(provider.snapshot.suggested_title.is_none());
    }

    #[test]
    fn missing_store_clears_each_projection_when_the_other_is_already_empty() {
        let state = SessionStoreState {
            schema_version: 3,
            current_summary: None,
            current_host_type: None,
            current_created_at: None,
            current_repository: None,
            repo_total_sessions: 1,
            repo_total_turns: 0,
            additional_fields: std::collections::BTreeMap::new(),
        };

        let mut store_only = CopilotCliProvider::new();
        store_only.session_store_state = Some(state);
        store_only.snapshot.suggested_title = None;
        store_only.apply_store_read(SessionStoreRead::Missing);
        assert!(store_only.session_store_state.is_none());

        let mut title_only = CopilotCliProvider::new();
        title_only.session_store_state = None;
        title_only.snapshot.suggested_title = Some("stale".to_owned());
        title_only.apply_store_read(SessionStoreRead::Missing);
        assert!(title_only.snapshot.suggested_title.is_none());
    }

    #[test]
    fn truncation_clears_stale_subagent_projection() {
        let dir = tempfile::tempdir().unwrap();
        let events = dir.path().join("events.jsonl");
        std::fs::write(
            &events,
            b"{\"type\":\"subagent.started\",\"data\":{\"toolCallId\":\"s1\",\"agentName\":\"explore\",\"agentDescription\":\"Research\"}}\n",
        )
        .unwrap();

        let mut provider = CopilotCliProvider::with_store_path(dir.path().join("absent.db"));
        provider.set_session_context("/tmp/proj", "session-a");
        provider.override_events_path_for_test(events.clone());
        provider.force_full_attach();
        drop(provider.poll_snapshot());
        assert!(
            provider.snapshot().sub_agents.is_some(),
            "fixture must seed a subagent before truncation"
        );

        std::fs::write(&events, b"{\"type\":\"user.message\",\"data\":{}}\n").unwrap();
        drop(provider.poll_snapshot());

        assert!(
            provider.snapshot().sub_agents.is_none(),
            "projection reset must clear a subagent absent from the rewritten JSONL"
        );
    }
}
