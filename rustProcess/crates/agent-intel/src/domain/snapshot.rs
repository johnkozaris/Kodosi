use serde::{Deserialize, Serialize};

use super::{
    AgentErrorKind, AgentProviderState, AgentStatus, HealthStatus, HitlPromptType, SubAgentStatus,
};

const IDENTITY_TEXT_LIMIT: usize = 1_024;
const SUMMARY_TEXT_LIMIT: usize = 240;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentIntelSnapshot {
    pub identity: LiveAgentIdentity,
    pub lifecycle: AgentLifecycle,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attention: Option<AgentAttention>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_interaction: Option<PendingAgentInteraction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_activity: Option<CurrentAgentActivity>,
    pub workers: ChildAgentSummary,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<AgentOutcome>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exceptional_state: Option<AgentExceptionalState>,
    pub source: AgentSource,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LiveAgentIdentity {
    pub agent_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vendor_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_id: Option<u32>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum AgentLifecycle {
    Starting,
    Working,
    #[default]
    Idle,
    Waiting,
    Completed,
    Failed,
    Stopped,
    Offline,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentAttention {
    pub kind: AgentAttentionKind,
    pub summary: String,
    pub actionable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum AgentAttentionKind {
    Permission,
    Question,
    Failure,
    Stalled,
    Limit,
    Authentication,
    Compaction,
    Worker,
    Review,
    Delivery,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[expect(
    clippy::struct_excessive_bools,
    reason = "the bounded interaction contract exposes independent action capabilities"
)]
pub struct PendingAgentInteraction {
    pub kind: PendingInteractionKind,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    pub can_approve: bool,
    pub can_deny: bool,
    pub can_answer: bool,
    pub can_focus: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum PendingInteractionKind {
    Permission,
    Question,
    Focus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CurrentAgentActivity {
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_progress_at: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChildAgentSummary {
    pub active: u32,
    pub blocked: u32,
    pub failed: u32,
    pub completed: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentOutcome {
    pub kind: AgentOutcomeKind,
    pub summary: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum AgentOutcomeKind {
    Completed,
    Failed,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentExceptionalState {
    pub kind: AgentExceptionalKind,
    pub summary: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum AgentExceptionalKind {
    RateLimit,
    Authentication,
    Quota,
    ContextOverflow,
    Stalled,
    Crash,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentSource {
    pub kind: AgentSourceKind,
    pub degraded: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl Default for AgentSource {
    fn default() -> Self {
        Self {
            kind: AgentSourceKind::Transcript,
            degraded: true,
            detail: Some("Waiting for a supported live vendor source".to_owned()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum AgentSourceKind {
    Runtime,
    Sdk,
    Extension,
    Telemetry,
    Command,
    Transcript,
    Terminal,
}

impl AgentIntelSnapshot {
    #[must_use]
    pub fn from_provider_state(state: &AgentProviderState) -> Self {
        let exceptional_state = state
            .error
            .as_ref()
            .map(exceptional_from_error)
            .or_else(|| {
                (state.health == HealthStatus::Stalled).then(|| AgentExceptionalState {
                    kind: AgentExceptionalKind::Stalled,
                    summary: "The agent has stopped making observable progress".to_owned(),
                    retryable: true,
                })
            })
            .or_else(|| {
                (state.health == HealthStatus::Dead).then(|| AgentExceptionalState {
                    kind: AgentExceptionalKind::Crash,
                    summary: "The agent process or vendor session ended unexpectedly".to_owned(),
                    retryable: true,
                })
            });

        let pending_interaction = state.hitl_prompt.as_ref().map(|prompt| {
            let summary = prompt
                .description
                .as_deref()
                .or(prompt.tool_name.as_deref())
                .unwrap_or("The agent needs input");
            let permission = prompt.prompt_type == HitlPromptType::AllowDeny;
            PendingAgentInteraction {
                kind: if permission {
                    PendingInteractionKind::Permission
                } else {
                    PendingInteractionKind::Question
                },
                summary: bounded(summary, SUMMARY_TEXT_LIMIT),
                tool_name: bounded_option(prompt.tool_name.as_deref(), SUMMARY_TEXT_LIMIT),
                can_approve: permission,
                can_deny: permission,
                can_answer: !permission,
                can_focus: true,
            }
        });

        let attention = pending_interaction
            .as_ref()
            .map(|interaction| AgentAttention {
                kind: match interaction.kind {
                    PendingInteractionKind::Permission => AgentAttentionKind::Permission,
                    PendingInteractionKind::Question => AgentAttentionKind::Question,
                    PendingInteractionKind::Focus => AgentAttentionKind::Review,
                },
                summary: interaction.summary.clone(),
                actionable: true,
            })
            .or_else(|| {
                exceptional_state.as_ref().map(|exception| AgentAttention {
                    kind: attention_kind(exception.kind),
                    summary: exception.summary.clone(),
                    actionable: exception.retryable,
                })
            });

        let lifecycle = if state.health == HealthStatus::Dead {
            AgentLifecycle::Failed
        } else {
            match state.status {
                AgentStatus::Running => AgentLifecycle::Working,
                AgentStatus::Idle => AgentLifecycle::Idle,
                AgentStatus::WaitingForInput => AgentLifecycle::Waiting,
            }
        };
        let outcome = (lifecycle == AgentLifecycle::Failed).then(|| AgentOutcome {
            kind: AgentOutcomeKind::Failed,
            summary: exceptional_state.as_ref().map_or_else(
                || "The agent ended unexpectedly".to_owned(),
                |exception| exception.summary.clone(),
            ),
        });

        let source_degraded = state.identity.parser_compat.is_some();
        Self {
            identity: LiveAgentIdentity {
                agent_type: bounded(&state.identity.agent_type, SUMMARY_TEXT_LIMIT),
                version: bounded_option(state.identity.version.as_deref(), SUMMARY_TEXT_LIMIT),
                model: bounded_option(state.identity.model.as_deref(), SUMMARY_TEXT_LIMIT),
                title: bounded_option(state.suggested_title.as_deref(), SUMMARY_TEXT_LIMIT),
                cwd: bounded_option(state.cwd.as_deref(), IDENTITY_TEXT_LIMIT),
                vendor_session_id: None,
                process_id: None,
            },
            lifecycle,
            attention,
            pending_interaction,
            current_activity: state
                .activity
                .as_deref()
                .map(|activity| CurrentAgentActivity {
                    summary: bounded(activity, SUMMARY_TEXT_LIMIT),
                    last_progress_at: None,
                }),
            workers: summarize_workers(state),
            outcome,
            exceptional_state,
            source: AgentSource {
                kind: AgentSourceKind::Transcript,
                degraded: source_degraded,
                detail: source_degraded.then(|| {
                    "The vendor transcript schema contains unsupported records".to_owned()
                }),
            },
        }
    }
}

fn summarize_workers(state: &AgentProviderState) -> ChildAgentSummary {
    let mut summary = ChildAgentSummary::default();
    for worker in state
        .sub_agents
        .as_ref()
        .into_iter()
        .flat_map(|tree| &tree.agents)
    {
        match worker.status {
            SubAgentStatus::Running => summary.active = summary.active.saturating_add(1),
            SubAgentStatus::Failed => summary.failed = summary.failed.saturating_add(1),
            SubAgentStatus::Completed | SubAgentStatus::Stopped => {
                summary.completed = summary.completed.saturating_add(1);
            }
        }
    }
    summary
}

fn exceptional_from_error(error: &super::AgentError) -> AgentExceptionalState {
    let (kind, retryable) = match error.kind {
        AgentErrorKind::RateLimit => (AgentExceptionalKind::RateLimit, true),
        AgentErrorKind::AuthFailure => (AgentExceptionalKind::Authentication, true),
        AgentErrorKind::QuotaExceeded => (AgentExceptionalKind::Quota, false),
        AgentErrorKind::ContextOverflow => (AgentExceptionalKind::ContextOverflow, true),
        AgentErrorKind::Other => (AgentExceptionalKind::Other, true),
    };
    AgentExceptionalState {
        kind,
        summary: bounded(&error.message, SUMMARY_TEXT_LIMIT),
        retryable,
    }
}

const fn attention_kind(kind: AgentExceptionalKind) -> AgentAttentionKind {
    match kind {
        AgentExceptionalKind::RateLimit
        | AgentExceptionalKind::Quota
        | AgentExceptionalKind::ContextOverflow => AgentAttentionKind::Limit,
        AgentExceptionalKind::Authentication => AgentAttentionKind::Authentication,
        AgentExceptionalKind::Stalled => AgentAttentionKind::Stalled,
        AgentExceptionalKind::Crash | AgentExceptionalKind::Other => AgentAttentionKind::Failure,
    }
}

fn bounded_option(value: Option<&str>, max_chars: usize) -> Option<String> {
    value
        .filter(|text| !text.is_empty())
        .map(|text| bounded(text, max_chars))
}

fn bounded(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

pub fn wire_authority_sample() -> AgentIntelSnapshot {
    AgentIntelSnapshot {
        identity: LiveAgentIdentity {
            agent_type: "claude".to_owned(),
            version: Some("2.1.88".to_owned()),
            model: Some("claude-opus-4-6".to_owned()),
            title: Some("Refactor Agent Intel".to_owned()),
            cwd: Some("/home/user/proj".to_owned()),
            vendor_session_id: Some("vendor-session".to_owned()),
            process_id: Some(42),
        },
        lifecycle: AgentLifecycle::Waiting,
        attention: Some(AgentAttention {
            kind: AgentAttentionKind::Permission,
            summary: "Approve the Bash command".to_owned(),
            actionable: true,
        }),
        pending_interaction: Some(PendingAgentInteraction {
            kind: PendingInteractionKind::Permission,
            summary: "Approve the Bash command".to_owned(),
            tool_name: Some("Bash".to_owned()),
            can_approve: true,
            can_deny: true,
            can_answer: false,
            can_focus: true,
        }),
        current_activity: Some(CurrentAgentActivity {
            summary: "Reviewing runtime changes".to_owned(),
            last_progress_at: Some("2026-08-23T12:00:00Z".to_owned()),
        }),
        workers: ChildAgentSummary {
            active: 2,
            blocked: 1,
            failed: 0,
            completed: 3,
        },
        outcome: None,
        exceptional_state: None,
        source: AgentSource {
            kind: AgentSourceKind::Extension,
            degraded: false,
            detail: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{AgentError, AgentIdentity, HitlPrompt, SubAgentInfo, SubAgentTree};

    #[test]
    fn provider_projection_is_bounded_and_attention_first() {
        let state = AgentProviderState {
            identity: AgentIdentity {
                agent_type: "claude".repeat(200),
                ..AgentIdentity::default()
            },
            status: AgentStatus::WaitingForInput,
            hitl_prompt: Some(HitlPrompt {
                prompt_type: HitlPromptType::AllowDeny,
                tool_name: Some("Bash".to_owned()),
                description: Some("x".repeat(10_000)),
            }),
            error: Some(AgentError {
                kind: AgentErrorKind::RateLimit,
                message: "slow down".to_owned(),
                timestamp: "now".to_owned(),
            }),
            sub_agents: Some(SubAgentTree {
                agents: vec![SubAgentInfo {
                    status: SubAgentStatus::Running,
                }],
            }),
            ..AgentProviderState::default()
        };

        let live = AgentIntelSnapshot::from_provider_state(&state);

        assert_eq!(live.identity.agent_type.chars().count(), SUMMARY_TEXT_LIMIT);
        assert_eq!(
            live.pending_interaction
                .as_ref()
                .expect("pending")
                .summary
                .chars()
                .count(),
            SUMMARY_TEXT_LIMIT
        );
        assert_eq!(
            live.attention.expect("attention").kind,
            AgentAttentionKind::Permission
        );
        assert_eq!(live.workers.active, 1);
    }

    #[test]
    fn wire_shape_has_no_history_collections() {
        let value = serde_json::to_value(wire_authority_sample()).expect("serialize");
        for removed in [
            "tasks",
            "errors",
            "fileChanges",
            "toolActivity",
            "hookLifecycle",
            "recentSlashCommands",
            "permissionEvents",
            "commands",
        ] {
            assert!(value.get(removed).is_none(), "{removed} must not be live");
        }
    }
}
