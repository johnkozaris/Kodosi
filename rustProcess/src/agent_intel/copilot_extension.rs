use std::time::Duration;

use agent_intel::AgentIntelSnapshot;
use agent_intel::domain::{
    AgentAttention, AgentAttentionKind, AgentExceptionalKind, AgentExceptionalState,
    AgentLifecycle, AgentOutcome, AgentOutcomeKind, AgentSource, AgentSourceKind,
    CurrentAgentActivity, PendingAgentInteraction, PendingInteractionKind,
};
use kodosi_domain::ids::SessionId;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::permission_decision_registry::{
    PendingKey, PendingPermissionMetadata, PermissionDecision, PermissionDecisionRegistry,
};
use super::telemetry::{CopilotPermissionDecision, CopilotPermissionRequest, ExtensionObservation};
use crate::session_runtime::events::RuntimeSessionEvent;

#[derive(Debug, Default)]
pub(crate) struct CopilotExtensionState {
    seen: bool,
    lifecycle: Option<AgentLifecycle>,
    current_activity: Option<CurrentAgentActivity>,
    pending_interaction: Option<PendingAgentInteraction>,
    attention: Option<AgentAttention>,
    exceptional_state: Option<AgentExceptionalState>,
    outcome: Option<AgentOutcome>,
}

impl CopilotExtensionState {
    #[expect(
        clippy::too_many_lines,
        reason = "one exhaustive event reducer keeps every typed Copilot transition visible"
    )]
    pub(crate) fn apply(&mut self, observation: &ExtensionObservation) {
        self.seen = true;
        match observation.event_type.as_str() {
            "kodosi.extension.ready" => {
                self.lifecycle = Some(AgentLifecycle::Starting);
                self.current_activity = Some(activity("Copilot extension connected", observation));
            }
            "user.message" | "assistant.turn_start" => {
                self.clear_resolved_attention();
                self.lifecycle = Some(AgentLifecycle::Working);
                self.current_activity = Some(activity("Processing a user request", observation));
            }
            "assistant.message" => {
                self.clear_resolved_attention();
                self.lifecycle = Some(AgentLifecycle::Working);
                self.current_activity =
                    Some(activity("Produced an assistant response", observation));
            }
            "tool.execution_start" => {
                self.clear_resolved_attention();
                self.lifecycle = Some(AgentLifecycle::Working);
                let tool = tool_label(&observation.data);
                self.current_activity = Some(activity(&format!("Running {tool}"), observation));
            }
            "tool.execution_complete" => {
                let tool = tool_label(&observation.data);
                if observation
                    .data
                    .get("success")
                    .and_then(serde_json::Value::as_bool)
                    == Some(false)
                {
                    let summary = format!("Copilot {tool} tool failed");
                    self.lifecycle = Some(AgentLifecycle::Waiting);
                    self.exceptional_state = Some(AgentExceptionalState {
                        kind: AgentExceptionalKind::Other,
                        summary: summary.clone(),
                        retryable: true,
                    });
                    self.attention = Some(AgentAttention {
                        kind: AgentAttentionKind::Failure,
                        summary,
                        actionable: true,
                    });
                } else {
                    self.clear_resolved_attention();
                    self.lifecycle = Some(AgentLifecycle::Working);
                    self.current_activity =
                        Some(activity(&format!("Completed {tool}"), observation));
                }
            }
            "permission.requested" => {
                let kind = permission_label(&observation.data);
                let summary = format!("Copilot needs {kind} permission");
                self.lifecycle = Some(AgentLifecycle::Waiting);
                self.pending_interaction = Some(PendingAgentInteraction {
                    kind: PendingInteractionKind::Permission,
                    summary: summary.clone(),
                    tool_name: None,
                    can_approve: false,
                    can_deny: false,
                    can_answer: false,
                    can_focus: true,
                });
                self.attention = Some(AgentAttention {
                    kind: AgentAttentionKind::Permission,
                    summary,
                    actionable: true,
                });
            }
            "session.idle" => {
                self.clear_resolved_attention();
                self.lifecycle = Some(AgentLifecycle::Idle);
                self.current_activity = None;
            }
            "session.error" => {
                let (kind, summary) = session_error(&observation.data);
                self.lifecycle = Some(AgentLifecycle::Failed);
                self.exceptional_state = Some(AgentExceptionalState {
                    kind,
                    summary: summary.clone(),
                    retryable: !matches!(kind, AgentExceptionalKind::Quota),
                });
                self.attention = Some(AgentAttention {
                    kind: match kind {
                        AgentExceptionalKind::Authentication => AgentAttentionKind::Authentication,
                        AgentExceptionalKind::RateLimit
                        | AgentExceptionalKind::Quota
                        | AgentExceptionalKind::ContextOverflow => AgentAttentionKind::Limit,
                        _ => AgentAttentionKind::Failure,
                    },
                    summary: summary.clone(),
                    actionable: true,
                });
                self.outcome = Some(AgentOutcome {
                    kind: AgentOutcomeKind::Failed,
                    summary,
                });
            }
            "session.shutdown" => {
                self.clear_resolved_attention();
                self.lifecycle = Some(AgentLifecycle::Stopped);
                self.current_activity = None;
                self.outcome = Some(AgentOutcome {
                    kind: AgentOutcomeKind::Stopped,
                    summary: "Copilot session ended".to_owned(),
                });
            }
            _ => {}
        }
    }

    pub(crate) fn overlay(&self, snapshot: &mut AgentIntelSnapshot) {
        if !self.seen {
            return;
        }
        if let Some(lifecycle) = self.lifecycle {
            snapshot.lifecycle = lifecycle;
        }
        snapshot.current_activity.clone_from(&self.current_activity);
        snapshot
            .pending_interaction
            .clone_from(&self.pending_interaction);
        snapshot.attention.clone_from(&self.attention);
        snapshot
            .exceptional_state
            .clone_from(&self.exceptional_state);
        snapshot.outcome.clone_from(&self.outcome);
        snapshot.source = AgentSource {
            kind: AgentSourceKind::Extension,
            degraded: false,
            detail: Some("Live state from the session-scoped Copilot extension".to_owned()),
        };
    }

    fn clear_resolved_attention(&mut self) {
        self.pending_interaction = None;
        self.attention = None;
        self.exceptional_state = None;
        self.outcome = None;
    }
}

pub(crate) async fn handle_permission_request(
    request: CopilotPermissionRequest,
    session_id: SessionId,
    local_incarnation_id: uuid::Uuid,
    decisions: PermissionDecisionRegistry,
    timeout: Duration,
    session_events: mpsc::Sender<RuntimeSessionEvent>,
    cancellation: CancellationToken,
) {
    let normalized = normalize_permission(&request);
    let key = PendingKey {
        session_id,
        session_incarnation_id: local_incarnation_id,
        tool_use_id: normalized.tool_use_id.clone(),
    };
    let Some(mut decision_receiver) = decisions.park(key.clone()) else {
        drop(request.reply.send(CopilotPermissionDecision::NoResult));
        return;
    };
    let deadline_at_ms = unix_now_ms().saturating_add(duration_to_ms(timeout));
    let risk = crate::agent_intel::risk::classify(&normalized.tool_name, &normalized.tool_input);
    let bounded_tool_input = bounded_tool_input(&normalized.tool_input);
    if !decisions.stage_metadata(
        &key,
        PendingPermissionMetadata {
            tool_name: normalized.tool_name.clone(),
            tool_input: bounded_tool_input,
            deadline_at_ms,
            risk,
        },
    ) {
        decisions.discard_unpublished(&key);
        drop(request.reply.send(CopilotPermissionDecision::NoResult));
        return;
    }
    let pending = RuntimeSessionEvent::PendingPermissionRequest {
        id: session_id,
        local_incarnation_id,
        tool_use_id: normalized.tool_use_id.clone(),
    };
    if !send_supervision_event(&session_events, pending).await {
        decisions.discard_unpublished(&key);
        drop(request.reply.send(CopilotPermissionDecision::NoResult));
        return;
    }

    let decision = wait_for_decision(
        &mut decision_receiver,
        PermissionWaitContext {
            key: &key,
            decisions: &decisions,
            timeout,
            cancellation: &cancellation,
        },
    )
    .await;
    let resolved = RuntimeSessionEvent::PermissionResolved {
        id: session_id,
        local_incarnation_id,
        tool_use_id: normalized.tool_use_id,
    };
    let delivered = send_supervision_event(&session_events, resolved).await;
    let reply = if delivered {
        match decision {
            PermissionDecision::Allow => CopilotPermissionDecision::ApproveOnce,
            PermissionDecision::Deny { reason } => CopilotPermissionDecision::Reject {
                feedback: reason.unwrap_or_else(|| "Denied by the Kodosi supervisor".to_owned()),
            },
        }
    } else {
        CopilotPermissionDecision::NoResult
    };
    drop(request.reply.send(reply));
}

#[expect(
    clippy::struct_field_names,
    reason = "the three fields are the normalized tool identity contract"
)]
struct NormalizedPermission {
    tool_use_id: String,
    tool_name: String,
    tool_input: serde_json::Value,
}

fn normalize_permission(request: &CopilotPermissionRequest) -> NormalizedPermission {
    let permission = &request.permission_request;
    let kind = string_field(permission, "kind").unwrap_or("permission");
    let tool_use_id = string_field(permission, "toolCallId")
        .filter(|value| !value.is_empty())
        .unwrap_or(&request.request_id)
        .to_owned();
    let tool_name = match kind {
        "shell" => "Bash".to_owned(),
        "write" => "Edit".to_owned(),
        "read" => "Read".to_owned(),
        "mcp" => {
            string_field(permission, "toolName").map_or_else(|| "MCP".to_owned(), ToOwned::to_owned)
        }
        "url" => "WebFetch".to_owned(),
        other => format!("Copilot:{other}"),
    };
    NormalizedPermission {
        tool_use_id,
        tool_name,
        tool_input: permission.clone(),
    }
}

struct PermissionWaitContext<'a> {
    key: &'a PendingKey,
    decisions: &'a PermissionDecisionRegistry,
    timeout: Duration,
    cancellation: &'a CancellationToken,
}

async fn wait_for_decision(
    receiver: &mut tokio::sync::oneshot::Receiver<
        super::permission_decision_registry::ResolvedDecision,
    >,
    context: PermissionWaitContext<'_>,
) -> PermissionDecision {
    let outcome = tokio::select! {
        () = context.cancellation.cancelled() => None,
        result = tokio::time::timeout(context.timeout, receiver) => {
            result.ok().and_then(Result::ok).map(|resolved| resolved.decision)
        },
    };
    let Some(outcome) = outcome else {
        context.decisions.seal_sender(context.key);
        return timeout_decision(context.timeout);
    };
    context.decisions.seal_sender(context.key);
    outcome
}

fn timeout_decision(timeout: Duration) -> PermissionDecision {
    PermissionDecision::Deny {
        reason: Some(format!(
            "Kodosi received no decision within {} seconds",
            timeout.as_secs()
        )),
    }
}

async fn send_supervision_event(
    sender: &mpsc::Sender<RuntimeSessionEvent>,
    event: RuntimeSessionEvent,
) -> bool {
    matches!(
        tokio::time::timeout(Duration::from_secs(2), sender.send(event)).await,
        Ok(Ok(()))
    )
}

fn unix_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, duration_to_ms)
}

fn duration_to_ms(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn bounded_tool_input(value: &serde_json::Value) -> serde_json::Value {
    const MAX_BYTES: usize = 8 * 1024;
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    if bytes.len() <= MAX_BYTES {
        return value.clone();
    }
    serde_json::json!({
        "omitted": true,
        "bytes": bytes.len(),
    })
}

fn activity(summary: &str, observation: &ExtensionObservation) -> CurrentAgentActivity {
    CurrentAgentActivity {
        summary: bounded(summary, 240),
        last_progress_at: Some(observation.timestamp.clone()),
    }
}

fn string_field<'a>(value: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(serde_json::Value::as_str)
}

fn tool_label(data: &serde_json::Value) -> &'static str {
    match string_field(data, "toolCategory") {
        Some("shell") => "shell",
        Some("read") => "read",
        Some("write") => "write",
        Some("search") => "search",
        Some("task") => "task",
        Some("web") => "web",
        Some("integration") => "integration",
        _ => "other",
    }
}

fn permission_label(data: &serde_json::Value) -> &'static str {
    match string_field(data, "kind") {
        Some("shell") => "shell",
        Some("write") => "write",
        Some("read") => "read",
        Some("mcp") => "integration",
        Some("url") => "network",
        _ => "additional",
    }
}

fn session_error(data: &serde_json::Value) -> (AgentExceptionalKind, String) {
    match string_field(data, "errorCategory") {
        Some("authentication") => (
            AgentExceptionalKind::Authentication,
            "Copilot authentication failed".to_owned(),
        ),
        Some("rate-limit") => (
            AgentExceptionalKind::RateLimit,
            "Copilot reached a rate limit".to_owned(),
        ),
        Some("quota") => (
            AgentExceptionalKind::Quota,
            "Copilot quota was exhausted".to_owned(),
        ),
        Some("context-limit") => (
            AgentExceptionalKind::ContextOverflow,
            "Copilot reached the context limit".to_owned(),
        ),
        Some("network") => (
            AgentExceptionalKind::Other,
            "Copilot reported a network failure".to_owned(),
        ),
        _ => (
            AgentExceptionalKind::Other,
            "Copilot reported a session error".to_owned(),
        ),
    }
}

fn bounded(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation(event_type: &str, data: serde_json::Value) -> ExtensionObservation {
        ExtensionObservation {
            session_id: SessionId::new().to_string(),
            event_type: event_type.to_owned(),
            timestamp: "2026-08-23T12:00:00Z".to_owned(),
            data,
        }
    }

    #[test]
    fn permission_event_is_attention_but_not_remotely_actionable() {
        let mut state = CopilotExtensionState::default();
        state.apply(&observation(
            "permission.requested",
            serde_json::json!({"requestId":"r1","kind":"shell"}),
        ));
        let mut snapshot = AgentIntelSnapshot::default();
        state.overlay(&mut snapshot);
        assert_eq!(snapshot.lifecycle, AgentLifecycle::Waiting);
        let pending = snapshot.pending_interaction.expect("pending");
        assert!(pending.can_focus);
        assert!(!pending.can_approve);
        assert_eq!(
            snapshot.attention.expect("attention").kind,
            AgentAttentionKind::Permission
        );
    }

    #[test]
    fn permission_timeout_always_denies() {
        std::assert_matches!(
            timeout_decision(Duration::from_secs(30)),
            PermissionDecision::Deny { reason: Some(reason) }
                if reason.contains("30 seconds")
        );
    }

    #[test]
    fn failed_tool_uses_only_safe_categories() {
        let mut state = CopilotExtensionState::default();
        let secret = "/Users/alice/project: command and stderr";
        state.apply(&observation(
            "tool.execution_complete",
            serde_json::json!({
                "toolCategory":"shell",
                "success":false,
                "error":secret,
            }),
        ));
        let mut snapshot = AgentIntelSnapshot::default();
        state.overlay(&mut snapshot);
        let summary = snapshot.exceptional_state.expect("exception").summary;
        assert_eq!(summary, "Copilot shell tool failed");
        assert!(!summary.contains(secret));
    }

    #[test]
    fn session_error_ignores_raw_vendor_message() {
        let mut state = CopilotExtensionState::default();
        let secret = "prompt and command leaked in vendor error";
        state.apply(&observation(
            "session.error",
            serde_json::json!({
                "errorCategory":"authentication",
                "message":secret,
            }),
        ));
        let mut snapshot = AgentIntelSnapshot::default();
        state.overlay(&mut snapshot);
        let exceptional = snapshot.exceptional_state.expect("exception");
        assert_eq!(exceptional.kind, AgentExceptionalKind::Authentication);
        assert_eq!(exceptional.summary, "Copilot authentication failed");
        assert!(!exceptional.summary.contains(secret));
    }
}
