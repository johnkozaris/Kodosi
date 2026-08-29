use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::adapter::CURRENT_ADAPTER;
use crate::domain::{
    AgentError, AgentErrorKind, AgentStatus, HealthStatus, HitlPrompt, HitlPromptType, SubAgentTree,
};
use crate::jsonl::copilot_metadata;
use crate::jsonl::subagents::{SubAgentSpawn, SubAgentTracker};

pub struct CopilotEventsState {
    pub projection_reset: bool,
    pub sub_agents: Option<SubAgentTree>,
    pub model: Option<String>,
    pub version: Option<String>,
    pub cwd: Option<String>,
    pub saw_shutdown: bool,
    pub activity_intent: Option<String>,
    pub status: Option<AgentStatus>,
    pub health: Option<HealthStatus>,
    pub hitl_prompt: Option<Option<HitlPrompt>>,
    pub error: Option<AgentError>,
    pub clear_activity: bool,
    pub unknown_record_types: Vec<String>,
    pub unrecognized_envelopes: u32,
    pub adapter_errors: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum ProjectionDisposition {
    #[default]
    Incremental,
    Reset,
}

#[derive(Default)]
pub struct CopilotEventsReader {
    tail: crate::jsonl::JsonlTail,
    sub_agents: SubAgentTracker,
    model: Option<String>,
    version: Option<String>,
    cwd: Option<String>,
    projection: ProjectionDisposition,
    dirty: bool,
    saw_shutdown: bool,
    activity_intent: Option<String>,
    status: Option<AgentStatus>,
    health: Option<HealthStatus>,
    #[expect(
        clippy::option_option,
        reason = "the reducer distinguishes no update, clearing, and setting the current prompt"
    )]
    hitl_prompt: Option<Option<HitlPrompt>>,
    error: Option<AgentError>,
    clear_activity: bool,
    unknown_record_types: BTreeSet<String>,
    unrecognized_envelopes: u32,
    adapter_errors: BTreeSet<String>,
}

const UNKNOWN_TYPE_CAP: usize = 32;

impl CopilotEventsReader {
    pub fn set_path(&mut self, path: PathBuf) {
        if self.tail.path() != Some(path.as_path()) {
            self.reset_projection();
            self.projection = ProjectionDisposition::Incremental;
            self.dirty = false;
            self.tail.set_path(path);
        }
    }

    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.tail.path()
    }

    pub fn read_new(&mut self) -> bool {
        let mut truncated = false;
        loop {
            match self.tail.read_new() {
                crate::jsonl::TailRead::Missing => return truncated,
                crate::jsonl::TailRead::Truncated => {
                    self.reset_projection();
                    truncated = true;
                }
                crate::jsonl::TailRead::Progress => {
                    self.dirty = true;
                    return true;
                }
                crate::jsonl::TailRead::Lines(lines) if lines.is_empty() => return truncated,
                crate::jsonl::TailRead::Lines(lines) => {
                    for line in lines {
                        self.process_line(&line);
                    }
                    self.dirty = true;
                    return true;
                }
            }
        }
    }

    pub fn read_full(&mut self) -> bool {
        self.tail.reset_offset();
        self.reset_projection();
        let mut progressed = true;
        loop {
            match self.tail.read_new() {
                crate::jsonl::TailRead::Missing => return progressed,
                crate::jsonl::TailRead::Truncated => self.reset_projection(),
                crate::jsonl::TailRead::Progress => self.dirty = true,
                crate::jsonl::TailRead::Lines(lines) if lines.is_empty() => return progressed,
                crate::jsonl::TailRead::Lines(lines) => {
                    for line in lines {
                        self.process_line(&line);
                    }
                    self.dirty = true;
                    progressed = true;
                }
            }
        }
    }

    pub fn take_state(&mut self) -> Option<CopilotEventsState> {
        if !std::mem::take(&mut self.dirty) {
            return None;
        }
        Some(CopilotEventsState {
            projection_reset: std::mem::take(&mut self.projection) == ProjectionDisposition::Reset,
            sub_agents: self.sub_agents.take_tree(),
            model: self.model.clone(),
            version: self.version.clone(),
            cwd: self.cwd.clone(),
            saw_shutdown: std::mem::take(&mut self.saw_shutdown),
            activity_intent: self.activity_intent.take(),
            status: self.status.take(),
            health: self.health.take(),
            hitl_prompt: self.hitl_prompt.take(),
            error: self.error.take(),
            clear_activity: std::mem::take(&mut self.clear_activity),
            unknown_record_types: self.unknown_record_types.iter().cloned().collect(),
            unrecognized_envelopes: self.unrecognized_envelopes,
            adapter_errors: self.adapter_errors.iter().cloned().collect(),
        })
    }

    fn reset_projection(&mut self) {
        self.sub_agents.reset();
        self.model = None;
        self.version = None;
        self.cwd = None;
        self.projection = ProjectionDisposition::Reset;
        self.dirty = true;
        self.saw_shutdown = false;
        self.activity_intent = None;
        self.status = None;
        self.health = None;
        self.hitl_prompt = None;
        self.error = None;
        self.clear_activity = false;
        self.unknown_record_types.clear();
        self.unrecognized_envelopes = 0;
        self.adapter_errors.clear();
    }

    fn process_line(&mut self, line: &str) {
        let event = match CURRENT_ADAPTER.decode_line(line) {
            Ok(event) => event,
            Err(error) => {
                if self.adapter_errors.len() < UNKNOWN_TYPE_CAP {
                    self.adapter_errors.insert(error.to_string());
                }
                return;
            }
        };
        if event.event_type.is_empty() {
            if event.non_empty_envelope {
                self.unrecognized_envelopes = self.unrecognized_envelopes.saturating_add(1);
            }
            return;
        }
        let timestamp = event.timestamp.as_deref().unwrap_or_default();
        let data = event.data;
        match event.event_type.as_str() {
            "session.start" | "session.resume" => {
                self.status = Some(AgentStatus::Idle);
                self.health = Some(HealthStatus::Healthy);
                self.read_session_identity(&data);
            }
            "session.context_changed" => {
                self.cwd = copilot_metadata::extract_cwd(&data);
            }
            "session.model_change" => {
                self.model = copilot_metadata::extract_new_model(&data);
            }
            "session.idle" | "session.task_complete" | "assistant.turn_end" => {
                self.status = Some(AgentStatus::Idle);
                self.clear_activity = true;
            }
            "assistant.turn_start" => {
                self.status = Some(AgentStatus::Running);
                self.health = Some(HealthStatus::Healthy);
            }
            "session.shutdown" => {
                self.saw_shutdown = true;
                self.status = Some(AgentStatus::Idle);
                self.health = Some(HealthStatus::Dead);
                self.clear_activity = true;
            }
            "session.error" => {
                self.error = Some(session_error(&data, timestamp));
            }
            "subagent.started" => {
                if let Some(id) = subagent_id(&data) {
                    self.sub_agents.record_spawn(SubAgentSpawn { agent_id: id });
                }
            }
            "subagent.completed" => {
                if let Some(id) = subagent_id(&data) {
                    self.sub_agents.record_completion(id);
                }
            }
            "subagent.failed" => {
                if let Some(id) = subagent_id(&data) {
                    self.sub_agents.record_failure(id);
                }
            }
            "permission.requested" | "tool.approval_requested" => {
                let request = data.get("permissionRequest").unwrap_or(&data);
                let prompt = data.get("promptRequest").unwrap_or(&data);
                self.status = Some(AgentStatus::WaitingForInput);
                self.hitl_prompt = Some(Some(HitlPrompt {
                    prompt_type: HitlPromptType::AllowDeny,
                    tool_name: request
                        .get("toolName")
                        .or_else(|| request.get("kind"))
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned),
                    description: prompt
                        .get("intention")
                        .or_else(|| prompt.get("fullCommandText"))
                        .or_else(|| data.get("message"))
                        .or_else(|| data.get("description"))
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned),
                }));
            }
            "permission.completed" | "tool.approval_decision" => {
                self.status = Some(AgentStatus::Running);
                self.hitl_prompt = Some(None);
            }
            "assistant.intent" => {
                self.activity_intent = data
                    .get("message")
                    .and_then(serde_json::Value::as_str)
                    .filter(|message| !message.is_empty())
                    .map(|message| crate::jsonl::subagents::truncate_chars(message, 200));
            }
            "session.context_summary"
            | "session.truncation"
            | "session.handoff"
            | "session.remote_steerable_changed"
            | "session.permissions_changed"
            | "session.autopilot_objective_changed"
            | "session.binary_asset"
            | "session.schedule_created"
            | "session.schedule_cancelled"
            | "session.info"
            | "session.warning"
            | "session.mode_changed"
            | "session.workspace_file_changed"
            | "session.usage_checkpoint"
            | "session.compaction_start"
            | "session.compaction_complete"
            | "session.plan_changed"
            | "user.message"
            | "assistant.message"
            | "assistant.reasoning"
            | "tool.execution_start"
            | "tool.execution_complete"
            | "abort"
            | "agent_idle"
            | "agent_completed"
            | "shell_completed"
            | "shell_detached_completed"
            | "instruction_discovered"
            | "tool.user_requested"
            | "subagent.selected"
            | "subagent.deselected"
            | "hook.start"
            | "hook.end"
            | "skill.invoked"
            | "system.message"
            | "system.notification" => {}
            other if self.unknown_record_types.len() < UNKNOWN_TYPE_CAP => {
                self.unknown_record_types.insert(other.to_owned());
            }
            _ => {}
        }
    }

    fn read_session_identity(&mut self, data: &serde_json::Value) {
        self.version = copilot_metadata::extract_copilot_version(data);
        self.model = copilot_metadata::extract_start_model(data);
        self.cwd = copilot_metadata::extract_cwd(data);
    }
}

fn subagent_id(data: &serde_json::Value) -> Option<&str> {
    data.get("transcriptPath")
        .or_else(|| data.get("transcript_path"))
        .or_else(|| data.get("agentId"))
        .or_else(|| data.get("agent_id"))
        .or_else(|| data.get("toolCallId"))
        .and_then(serde_json::Value::as_str)
        .filter(|id| !id.is_empty())
}

fn session_error(data: &serde_json::Value, timestamp: &str) -> AgentError {
    let (kind, message) = match data
        .get("errorType")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "auth" | "authentication" => (AgentErrorKind::AuthFailure, "Copilot authentication failed"),
        "rate_limit" | "ratelimit" => (AgentErrorKind::RateLimit, "Copilot is rate limited"),
        "quota" => (AgentErrorKind::QuotaExceeded, "Copilot quota was exceeded"),
        "context" | "context_overflow" => (
            AgentErrorKind::ContextOverflow,
            "Copilot reached its context limit",
        ),
        _ => (AgentErrorKind::Other, "Copilot reported a session error"),
    };
    AgentError {
        kind,
        message: message.to_owned(),
        timestamp: timestamp.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projects_current_identity_lifecycle_and_permission() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("events.jsonl");
        std::fs::write(
            &path,
            concat!(
                "{\"type\":\"session.start\",\"data\":{\"copilotVersion\":\"1.0.99\",",
                "\"selectedModel\":\"gpt-5.6-sol\",\"context\":{\"cwd\":\"/repo\"}}}\n",
                "{\"type\":\"permission.requested\",\"data\":{\"permissionRequest\":",
                "{\"toolName\":\"shell\"},\"message\":\"Approve\"}}\n"
            ),
        )
        .expect("write");
        let mut reader = CopilotEventsReader::default();
        reader.set_path(path);
        assert!(reader.read_full());
        let state = reader.take_state().expect("state");
        assert_eq!(state.version.as_deref(), Some("1.0.99"));
        assert_eq!(state.model.as_deref(), Some("gpt-5.6-sol"));
        assert_eq!(state.cwd.as_deref(), Some("/repo"));
        assert_eq!(state.status, Some(AgentStatus::WaitingForInput));
        assert!(matches!(state.hitl_prompt, Some(Some(_))));
    }
}
