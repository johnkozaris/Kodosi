use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::domain::{
    AgentIdentity, AgentProviderState, AgentStatus, HealthStatus, SettingsFilePath, SettingsScope,
};
use crate::terminal::{errors::ErrorDetector, status::StatusDetector};

use super::jsonl::JsonlReader;

pub struct ClaudeCodeProvider {
    snapshot: AgentProviderState,
    dirty: bool,

    status_detector: StatusDetector,
    error_detector: ErrorDetector,

    jsonl: JsonlReader,
    session_cwd: Option<String>,

    needs_full_jsonl_scan: bool,
    needs_incremental_jsonl_scan: bool,

    attached_session_id: Option<String>,

    last_activity: std::time::Instant,
    terminal_fallback_fields: BTreeSet<String>,
}

impl Default for ClaudeCodeProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ClaudeCodeProvider {
    fn mark_terminal_fallback(&mut self, field: &str) {
        if self.terminal_fallback_fields.insert(field.to_owned()) {
            self.refresh_terminal_fallback_notice();
        }
    }

    fn refresh_terminal_fallback_notice(&mut self) {
        let (unknown_record_types, unrecognized_envelopes, mut notices) =
            self.snapshot.identity.parser_compat.as_ref().map_or_else(
                || (Vec::new(), 0, Vec::new()),
                |notice| {
                    (
                        notice.unknown_record_types.clone(),
                        notice.unrecognized_envelopes,
                        notice
                            .degradation_notices
                            .iter()
                            .filter(|notice| {
                                !matches!(
                                    notice,
                                    crate::domain::DegradationNotice::TerminalFallback { .. }
                                )
                            })
                            .cloned()
                            .collect(),
                    )
                },
            );
        if !self.terminal_fallback_fields.is_empty() {
            notices.push(crate::domain::DegradationNotice::TerminalFallback {
                fields: self.terminal_fallback_fields.iter().cloned().collect(),
            });
        }
        self.snapshot.identity.parser_compat = crate::domain::ParserCompatNotice::from_parts(
            unknown_record_types,
            unrecognized_envelopes,
            notices,
        );
        self.dirty = true;
    }

    fn update_parser_compat(
        &mut self,
        unknown_record_types: Vec<String>,
        unrecognized_envelopes: u32,
        unknown_system_subtypes: Vec<String>,
    ) {
        let mut notices = unknown_system_subtypes
            .into_iter()
            .map(|subtype| crate::domain::DegradationNotice::UnknownClaudeSubtype { subtype })
            .collect::<Vec<_>>();
        if !self.terminal_fallback_fields.is_empty() {
            notices.push(crate::domain::DegradationNotice::TerminalFallback {
                fields: self.terminal_fallback_fields.iter().cloned().collect(),
            });
        }
        self.snapshot.identity.parser_compat = crate::domain::ParserCompatNotice::from_parts(
            unknown_record_types,
            unrecognized_envelopes,
            notices,
        );
    }

    pub fn new() -> Self {
        Self {
            snapshot: AgentProviderState {
                activity: None,
                identity: AgentIdentity {
                    agent_type: "claude".to_owned(),
                    version: None,
                    model: None,
                    parser_compat: None,
                },
                status: AgentStatus::Idle,
                health: HealthStatus::Healthy,
                error: None,
                hitl_prompt: None,
                suggested_title: None,
                sub_agents: None,
                cwd: None,
            },
            dirty: true,
            status_detector: StatusDetector::default(),
            error_detector: ErrorDetector::default(),
            jsonl: JsonlReader::default(),
            session_cwd: None,
            needs_full_jsonl_scan: false,
            needs_incremental_jsonl_scan: false,
            attached_session_id: None,
            last_activity: std::time::Instant::now(),
            terminal_fallback_fields: BTreeSet::new(),
        }
    }

    const STALL_THRESHOLD: std::time::Duration = std::time::Duration::from_secs(30);

    fn record_activity(&mut self) {
        self.last_activity = std::time::Instant::now();
        if self.snapshot.error.take().is_some() {
            self.dirty = true;
        }
        if self.snapshot.health == HealthStatus::Stalled {
            self.snapshot.health = HealthStatus::Healthy;
            self.dirty = true;
        }
    }

    fn check_stall(&mut self) {
        if self.snapshot.health == HealthStatus::Healthy
            && self.snapshot.status == AgentStatus::Running
            && self.last_activity.elapsed() > Self::STALL_THRESHOLD
        {
            self.snapshot.health = HealthStatus::Stalled;
            self.dirty = true;
        }
    }

    fn reset_jsonl_projection(&mut self) {
        self.snapshot.sub_agents = None;
        self.snapshot.suggested_title = None;
        self.snapshot.identity.model = None;
        self.snapshot.identity.version = None;
        self.snapshot.identity.parser_compat = None;
    }

    fn apply_jsonl_state(&mut self) {
        if let Some(state) = self.jsonl.take_state() {
            if state.projection_reset {
                self.reset_jsonl_projection();
            }
            self.snapshot.suggested_title = state.suggested_title;
            self.snapshot.identity.model = state.model;
            self.snapshot.identity.version = state.version;
            self.update_parser_compat(
                state.unknown_record_types,
                state.unrecognized_envelopes,
                state.unknown_system_subtypes,
            );
            if state.projection_reset || state.sub_agents.is_some() {
                self.snapshot.sub_agents = state
                    .sub_agents
                    .and_then(|tree| (!tree.agents.is_empty()).then_some(tree));
            }
            self.dirty = true;
        }
    }

    pub fn encode_project_path(cwd: &str) -> String {
        cwd.replace(['/', '\\', ':'], "-")
    }

    fn claude_home() -> PathBuf {
        crate::runtime::paths::home_dir().join(".claude")
    }

    #[must_use]
    pub const fn snapshot(&self) -> &AgentProviderState {
        &self.snapshot
    }

    pub fn request_full_attach(&mut self) {
        self.needs_full_jsonl_scan = true;
    }

    pub fn request_incremental_jsonl_scan(&mut self) {
        if !self.needs_full_jsonl_scan {
            self.needs_incremental_jsonl_scan = true;
        }
    }

    #[must_use]
    pub fn current_transcript_path(&self) -> Option<&std::path::Path> {
        self.jsonl.path()
    }

    pub fn force_full_attach(&mut self) {
        self.request_full_attach();
        self.settle_deferred_jsonl_io();
    }

    pub fn settle_deferred_jsonl_io(&mut self) {
        if self.needs_full_jsonl_scan {
            self.needs_full_jsonl_scan = false;
            self.needs_incremental_jsonl_scan = false;
            self.jsonl.read_full();
            self.apply_jsonl_state();
        } else if self.needs_incremental_jsonl_scan || self.jsonl.has_pending_drain() {
            self.needs_incremental_jsonl_scan = false;
            if self.jsonl.read_new() {
                self.apply_jsonl_state();
            }
        }
    }

    #[cfg(test)]
    pub fn override_jsonl_path_for_test(&mut self, path: PathBuf) {
        self.jsonl.set_path(path);
    }

    #[cfg(test)]
    pub fn rewind_last_activity_for_test(&mut self, by: std::time::Duration) {
        if let Some(earlier) = self.last_activity.checked_sub(by) {
            self.last_activity = earlier;
        }
    }
}

impl ClaudeCodeProvider {
    pub fn set_session_context(&mut self, cwd: &str, session_id: &str) {
        self.session_cwd = Some(cwd.to_owned());
        if self.snapshot.cwd.as_deref() != Some(cwd) {
            self.snapshot.cwd = Some(cwd.to_owned());
            self.dirty = true;
        }
        if let Some(path) = self.transcript_path(cwd, session_id) {
            self.jsonl.set_path(path);
        }
        if self.attached_session_id.as_deref() != Some(session_id) {
            self.attached_session_id = Some(session_id.to_owned());
        }
    }

    pub fn feed_terminal_output(&mut self, bytes: &[u8]) {
        if !bytes.is_empty() {
            self.record_activity();
        }
        if let Some(status) = self.status_detector.feed(bytes) {
            self.snapshot.status = status;
            self.mark_terminal_fallback("status");
            self.dirty = true;
        }
        self.error_detector.feed(bytes);
        if let Some(error) = self.error_detector.take_error() {
            self.snapshot.error = Some(error);
            self.mark_terminal_fallback("error");
            self.dirty = true;
        }
    }

    pub fn tick(&mut self) -> bool {
        if let Some(status) = self.status_detector.tick() {
            self.snapshot.status = status;
            self.mark_terminal_fallback("status");
            if status == AgentStatus::Idle && self.snapshot.activity.is_some() {
                self.snapshot.activity = None;
            }
            self.dirty = true;
        }
        self.check_stall();
        self.error_detector.tick();

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

impl ClaudeCodeProvider {
    pub fn settings_paths(&self, cwd: &str) -> Vec<SettingsFilePath> {
        let home = Self::claude_home();
        let mut paths = vec![SettingsFilePath {
            scope: SettingsScope::User,
            path: home.join("settings.json"),
        }];
        let project_root = PathBuf::from(cwd);
        paths.push(SettingsFilePath {
            scope: SettingsScope::Project,
            path: project_root.join(".claude/settings.json"),
        });
        paths.push(SettingsFilePath {
            scope: SettingsScope::Local,
            path: project_root.join(".claude/settings.local.json"),
        });
        #[cfg(target_os = "macos")]
        paths.push(SettingsFilePath {
            scope: SettingsScope::Managed,
            path: PathBuf::from("/Library/Application Support/ClaudeCode/managed-settings.json"),
        });
        paths
    }

    pub fn transcript_path(&self, cwd: &str, session_id: &str) -> Option<PathBuf> {
        Some(
            Self::claude_home()
                .join("projects")
                .join(Self::encode_project_path(cwd))
                .join(format!("{session_id}.jsonl")),
        )
    }

    pub fn transcript_root(&self) -> Option<PathBuf> {
        Some(Self::claude_home().join("projects"))
    }

    pub fn transcript_decoder(&self) -> &'static dyn crate::TranscriptDecoder {
        crate::claude::transcript::ClaudeTranscriptDecoder::INSTANCE
    }
}

#[cfg(test)]
mod stall_tests {
    use super::*;

    #[test]
    fn flips_to_stalled_when_running_without_activity() {
        let mut provider = ClaudeCodeProvider::new();

        provider.feed_terminal_output(&[b'x'; 160]);
        let snapshot = provider.poll_snapshot().expect("first snapshot");
        assert_eq!(
            snapshot.get("status").and_then(|v| v.as_str()),
            Some("running")
        );
        assert_eq!(
            snapshot.get("health").and_then(|v| v.as_str()),
            Some("healthy")
        );

        provider.rewind_last_activity_for_test(std::time::Duration::from_secs(31));
        let snapshot = provider.poll_snapshot().expect("stall snapshot");
        assert_eq!(
            snapshot.get("health").and_then(|v| v.as_str()),
            Some("stalled")
        );

        provider.feed_terminal_output(b"recovered");
        let snapshot = provider.poll_snapshot().expect("recovery snapshot");
        assert_eq!(
            snapshot.get("health").and_then(|v| v.as_str()),
            Some("healthy")
        );
    }

    #[test]
    fn terminal_derived_status_exposes_fallback_provenance() {
        let mut provider = ClaudeCodeProvider::new();
        provider.feed_terminal_output(&[b'x'; 160]);
        let snapshot = provider.snapshot();
        let notice = snapshot
            .identity
            .parser_compat
            .as_ref()
            .expect("terminal provenance notice");
        assert!(notice.degradation_notices.iter().any(|notice| matches!(
            notice,
            crate::domain::DegradationNotice::TerminalFallback { fields }
                if fields == &vec!["status".to_owned()]
        )));
    }

    #[test]
    fn replacement_transcript_clears_jsonl_owned_projection() {
        let first_dir = tempfile::tempdir().unwrap();
        let first = first_dir.path().join("first.jsonl");
        std::fs::write(
            &first,
            concat!(
                "{\"type\":\"custom-title\",\"customTitle\":\"Old title\"}\n",
                "{\"type\":\"user\",\"version\":\"2.1.200\",",
                "\"permissionMode\":\"plan\"}\n",
                "{\"type\":\"assistant\",\"message\":{\"id\":\"msg-1\",",
                "\"model\":\"claude-opus-5\",\"content\":[],\"usage\":{",
                "\"input_tokens\":10,\"output_tokens\":5,",
                "\"cache_creation_input_tokens\":0,\"cache_read_input_tokens\":0}}}\n"
            ),
        )
        .unwrap();
        let second_dir = tempfile::tempdir().unwrap();
        let second = second_dir.path().join("second.jsonl");
        std::fs::write(&second, "").unwrap();

        let mut provider = ClaudeCodeProvider::new();
        provider.override_jsonl_path_for_test(first);
        provider.force_full_attach();
        assert_eq!(
            provider.snapshot.identity.model.as_deref(),
            Some("claude-opus-5")
        );
        assert_eq!(
            provider.snapshot.suggested_title.as_deref(),
            Some("Old title")
        );
        provider.override_jsonl_path_for_test(second);
        provider.force_full_attach();

        assert!(provider.snapshot.identity.model.is_none());
        assert!(provider.snapshot.identity.version.is_none());
        assert!(provider.snapshot.suggested_title.is_none());
    }

    #[test]
    fn session_producer_version_populates_identity() {
        let dir = tempfile::tempdir().unwrap();
        let transcript = dir.path().join("session.jsonl");
        std::fs::write(&transcript, "{\"type\":\"user\",\"version\":\"2.1.200\"}\n").unwrap();
        let mut provider = ClaudeCodeProvider::new();
        provider.override_jsonl_path_for_test(transcript);
        provider.force_full_attach();
        drop(provider.poll_snapshot());

        assert_eq!(
            provider.snapshot.identity.version.as_deref(),
            Some("2.1.200")
        );
    }

    #[test]
    fn idle_status_does_not_trigger_stall() {
        let mut provider = ClaudeCodeProvider::new();

        provider.rewind_last_activity_for_test(std::time::Duration::from_mins(2));
        let snapshot = provider.poll_snapshot().expect("snapshot");
        assert_eq!(
            snapshot.get("status").and_then(|v| v.as_str()),
            Some("idle")
        );
        assert_eq!(
            snapshot.get("health").and_then(|v| v.as_str()),
            Some("healthy")
        );
    }

    #[test]
    fn waiting_for_input_does_not_trigger_stall() {
        let mut provider = ClaudeCodeProvider::new();
        provider.feed_terminal_output(b"Approve this? [Y/n] ");
        let snapshot = provider.poll_snapshot().expect("first snapshot");
        assert_eq!(
            snapshot.get("status").and_then(|v| v.as_str()),
            Some("waitingForInput")
        );

        provider.rewind_last_activity_for_test(std::time::Duration::from_mins(2));
        let snapshot = provider.poll_snapshot();
        let health = snapshot
            .as_ref()
            .and_then(|v| v.get("health").and_then(|v| v.as_str()))
            .unwrap_or("healthy");
        assert_eq!(health, "healthy");
    }
}
