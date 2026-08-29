use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::domain::SubAgentTree;
use crate::jsonl::subagents::{SubAgentSpawn, SubAgentTracker, TaskNotificationStatus};

pub struct JsonlState {
    pub projection_reset: bool,
    pub sub_agents: Option<SubAgentTree>,
    pub suggested_title: Option<String>,
    pub model: Option<String>,
    pub version: Option<String>,
    pub unknown_record_types: Vec<String>,
    pub unknown_system_subtypes: Vec<String>,
    pub unrecognized_envelopes: u32,
}

pub struct JsonlReader {
    tail: crate::jsonl::JsonlTail,
    sub_agents: SubAgentTracker,
    title: Option<String>,
    model: Option<String>,
    version: Option<String>,
    unknown_record_types: BTreeSet<String>,
    unknown_system_subtypes: BTreeSet<String>,
    unrecognized_envelopes: u32,
    projection_reset: bool,
    dirty: bool,
}

const UNKNOWN_TYPE_CAP: usize = 32;

impl Default for JsonlReader {
    fn default() -> Self {
        Self {
            tail: crate::jsonl::JsonlTail::new(),
            sub_agents: SubAgentTracker::default(),
            title: None,
            model: None,
            version: None,
            unknown_record_types: BTreeSet::new(),
            unknown_system_subtypes: BTreeSet::new(),
            unrecognized_envelopes: 0,
            projection_reset: false,
            dirty: false,
        }
    }
}

impl JsonlReader {
    #[must_use]
    pub const fn has_pending_drain(&self) -> bool {
        self.tail.has_pending_drain()
    }

    pub fn set_path(&mut self, path: PathBuf) {
        if self.tail.path() != Some(path.as_path()) {
            self.reset_projection();
            self.tail.set_path(path);
        }
    }

    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.tail.path()
    }

    pub fn read_new(&mut self) -> bool {
        loop {
            match self.tail.read_new() {
                crate::jsonl::TailRead::Missing => return false,
                crate::jsonl::TailRead::Truncated => self.reset_projection(),
                crate::jsonl::TailRead::Progress => {
                    self.dirty = true;
                    return true;
                }
                crate::jsonl::TailRead::Lines(lines) if lines.is_empty() => return false,
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
        let mut progressed = false;
        loop {
            match self.tail.read_new() {
                crate::jsonl::TailRead::Missing => return progressed,
                crate::jsonl::TailRead::Truncated => {
                    self.reset_projection();
                    progressed = true;
                }
                crate::jsonl::TailRead::Progress => {
                    self.dirty = true;
                    progressed = true;
                }
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

    pub fn take_state(&mut self) -> Option<JsonlState> {
        if !std::mem::take(&mut self.dirty) {
            return None;
        }
        Some(JsonlState {
            projection_reset: std::mem::take(&mut self.projection_reset),
            sub_agents: self.sub_agents.take_tree(),
            suggested_title: self.title.clone(),
            model: self.model.clone(),
            version: self.version.clone(),
            unknown_record_types: self.unknown_record_types.iter().cloned().collect(),
            unknown_system_subtypes: self.unknown_system_subtypes.iter().cloned().collect(),
            unrecognized_envelopes: self.unrecognized_envelopes,
        })
    }

    fn reset_projection(&mut self) {
        self.sub_agents.reset();
        self.title = None;
        self.model = None;
        self.version = None;
        self.unknown_record_types.clear();
        self.unknown_system_subtypes.clear();
        self.unrecognized_envelopes = 0;
        self.projection_reset = true;
        self.dirty = true;
    }

    fn process_line(&mut self, line: &str) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            return;
        };
        let record_type = value
            .get("type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        if record_type.is_empty() {
            if value.as_object().is_some_and(|fields| !fields.is_empty()) {
                self.unrecognized_envelopes = self.unrecognized_envelopes.saturating_add(1);
            }
            return;
        }
        match record_type {
            "assistant" => self.process_assistant(&value),
            "user" => {
                if let Some(version) = value.get("version").and_then(serde_json::Value::as_str) {
                    self.version = Some(version.to_owned());
                }
                if let Some(blocks) = value
                    .get("message")
                    .and_then(|message| message.get("content"))
                    .and_then(serde_json::Value::as_array)
                {
                    for block in blocks {
                        if block.get("type").and_then(serde_json::Value::as_str)
                            == Some("tool_result")
                            && let Some(id) =
                                block.get("tool_use_id").and_then(serde_json::Value::as_str)
                        {
                            self.sub_agents.record_completion(id);
                        }
                    }
                }
            }
            "custom-title" => {
                self.title = crate::jsonl::claude_code_metadata::extract_custom_title(&value);
            }
            "agent-name" => {
                self.title = value
                    .get("agentName")
                    .and_then(serde_json::Value::as_str)
                    .filter(|name| !name.is_empty())
                    .map(str::to_owned);
            }
            "ai-title" => {
                self.title = value
                    .get("title")
                    .or_else(|| value.get("aiTitle"))
                    .and_then(serde_json::Value::as_str)
                    .filter(|title| !title.is_empty())
                    .map(str::to_owned);
            }
            "system" => self.process_system(&value),
            "attachment"
            | "file-history-delta"
            | "file-history-snapshot"
            | "last-prompt"
            | "mode"
            | "permission-mode"
            | "queue-operation"
            | "result" => {}
            other if self.unknown_record_types.len() < UNKNOWN_TYPE_CAP => {
                self.unknown_record_types.insert(other.to_owned());
            }
            _ => {}
        }
    }

    fn process_assistant(&mut self, value: &serde_json::Value) {
        let message = value.get("message").cloned().unwrap_or_default();
        let nested = message
            .get("parent_tool_use_id")
            .or_else(|| message.get("parentToolUseId"))
            .or_else(|| value.get("parent_tool_use_id"))
            .or_else(|| value.get("parentToolUseId"))
            .is_some_and(|parent| !parent.is_null());
        if !nested && let Some(model) = message.get("model").and_then(serde_json::Value::as_str) {
            self.model = Some(model.to_owned());
        }
        let Some(blocks) = message.get("content").and_then(serde_json::Value::as_array) else {
            return;
        };
        for block in blocks {
            if block.get("type").and_then(serde_json::Value::as_str) != Some("tool_use") {
                continue;
            }
            let tool = block
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            if matches!(tool, "Task" | "Agent")
                && let Some(id) = block.get("id").and_then(serde_json::Value::as_str)
            {
                self.sub_agents.record_spawn(SubAgentSpawn { agent_id: id });
            }
        }
    }

    fn process_system(&mut self, value: &serde_json::Value) {
        let subtype = value
            .get("subtype")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        match subtype {
            "task_started" => {
                if let Some(task_id) = value.get("task_id").and_then(serde_json::Value::as_str) {
                    self.sub_agents.record_task_started(
                        task_id,
                        value.get("tool_use_id").and_then(serde_json::Value::as_str),
                    );
                }
            }
            "task_notification" => {
                if let (Some(task_id), Some(status)) = (
                    value.get("task_id").and_then(serde_json::Value::as_str),
                    value
                        .get("status")
                        .and_then(serde_json::Value::as_str)
                        .and_then(TaskNotificationStatus::from_str_ci),
                ) {
                    self.sub_agents.record_task_notification(task_id, status);
                }
            }
            "agents_killed" | "away_summary" | "compact_boundary" | "informational"
            | "local_command" | "stop_hook_summary" | "hook_started" | "hook_response"
            | "task_progress" | "turn_duration" => {}
            other if !other.is_empty() && self.unknown_system_subtypes.len() < UNKNOWN_TYPE_CAP => {
                self.unknown_system_subtypes.insert(other.to_owned());
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projects_only_current_identity_title_and_workers() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("session.jsonl");
        std::fs::write(
            &path,
            concat!(
                "{\"type\":\"custom-title\",\"customTitle\":\"Current\"}\n",
                "{\"type\":\"user\",\"version\":\"2.1.200\"}\n",
                "{\"type\":\"assistant\",\"message\":{\"model\":\"claude-opus-5\",",
                "\"content\":[{\"type\":\"tool_use\",\"id\":\"worker-1\",\"name\":\"Agent\"}]}}\n"
            ),
        )
        .expect("write");
        let mut reader = JsonlReader::default();
        reader.set_path(path);
        assert!(reader.read_full());
        let state = reader.take_state().expect("state");
        assert_eq!(state.suggested_title.as_deref(), Some("Current"));
        assert_eq!(state.version.as_deref(), Some("2.1.200"));
        assert_eq!(state.model.as_deref(), Some("claude-opus-5"));
        assert_eq!(state.sub_agents.expect("workers").agents.len(), 1);
    }
}
