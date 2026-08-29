use std::path::PathBuf;

use directories::BaseDirs;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::mpsc;

use ::agent_intel::AgentKind;
use ::agent_intel::claude::subagent_layout;
use ::agent_intel::ops::{
    auto_mode, copilot_repos, custom_agents, diagnostics, external_sessions, import, memory,
    projects, provider_conversations, settings,
};

use super::{AccountAgentIntelEvent, AgentIntelCommand, AgentIntelEvent};

pub(crate) async fn handle_runtime(
    command: AgentIntelCommand,
    local_sessions: &crate::session_runtime::registry::SessionRegistry,
    agent_intel: &crate::agent_intel::AgentIntelRegistry,
    event_tx: mpsc::Sender<AccountAgentIntelEvent>,
    account_user_id: Option<String>,
    account_epoch: u64,
) {
    let request_id = command.request_id().to_owned();
    let reply = match dispatch(command, Some((local_sessions, agent_intel))).await {
        Ok(payload) => AgentIntelEvent::Reply {
            request_id,
            payload,
        },
        Err(message) => AgentIntelEvent::Error {
            request_id,
            message,
        },
    };
    if let Err(error) = event_tx
        .send(AccountAgentIntelEvent::new(
            account_user_id,
            account_epoch,
            reply,
        ))
        .await
    {
        tracing::warn!(%error, "agent-intel reply dropped — event consumer is gone");
    }
}

type LocalTranscriptAuthority<'a> = (
    &'a crate::session_runtime::registry::SessionRegistry,
    &'a crate::agent_intel::AgentIntelRegistry,
);

#[expect(
    clippy::too_many_lines,
    reason = "dispatch keeps every command arm in one match for compile-time exhaustiveness; splitting hides exhaustiveness checks"
)]
async fn dispatch(
    command: AgentIntelCommand,
    local_transcript_authority: Option<LocalTranscriptAuthority<'_>>,
) -> Result<Value, String> {
    match command {
        AgentIntelCommand::ReadSettings { agent, cwd, .. } => {
            let agent = parse_agent(&agent)?;
            into_value(settings::read_settings(agent, cwd.as_deref()).await?)
        }
        AgentIntelCommand::ListClaudeProjects { .. } => {
            let home = home_dir()?;
            into_value(projects::list_claude_projects(&home).await?)
        }
        AgentIntelCommand::ListProjectSessions { slug, .. } => {
            let home = home_dir()?;
            into_value(projects::list_project_sessions(&home, &slug).await?)
        }
        AgentIntelCommand::ListProjectMemories { slug, .. } => {
            let home = home_dir()?;
            into_value(projects::list_project_memories(&home, &slug).await?)
        }
        AgentIntelCommand::ReadProjectMemory { slug, filename, .. } => {
            let home = home_dir()?;
            into_value(projects::read_project_memory(&home, &slug, &filename).await?)
        }
        AgentIntelCommand::ListClaudeMemory { cwd, .. } => {
            let home = home_dir()?;
            into_value(memory::list_memory(&home, &cwd).await?)
        }
        AgentIntelCommand::ReadClaudeMemory { cwd, filename, .. } => {
            let home = home_dir()?;
            into_value(memory::read_memory(&home, &cwd, &filename).await?)
        }
        AgentIntelCommand::ReadSessionConversation {
            agent,
            cwd,
            session_id,
            before_byte,
            max_records,
            max_bytes,
            ..
        } => {
            let agent = parse_agent(&agent)?;
            into_value(
                import::read_session_conversation(
                    agent,
                    &cwd,
                    &session_id,
                    before_byte,
                    max_records,
                    max_bytes,
                )
                .await?,
            )
        }
        AgentIntelCommand::ReadSubagentTranscript {
            agent,
            path,
            before_byte,
            max_records,
            max_bytes,
            ..
        } => {
            let agent = parse_agent(&agent)?;
            into_value(
                import::read_subagent_transcript(agent, &path, before_byte, max_records, max_bytes)
                    .await?,
            )
        }
        AgentIntelCommand::ListCopilotRepositories { .. } => {
            let home = home_dir()?;
            into_value(copilot_repos::list_repositories(&home).await?)
        }
        AgentIntelCommand::ListCopilotRepoSessions { repository, .. } => {
            let home = home_dir()?;
            into_value(copilot_repos::list_repo_sessions(&home, &repository).await?)
        }
        AgentIntelCommand::DiscoverProviderConversations {
            provider,
            working_directory,
            cursor,
            limit,
            max_bytes,
            ..
        } => {
            let home = home_dir()?;
            let provider = parse_agent(&provider)?;
            into_value(
                provider_conversations::discover(
                    &home,
                    provider,
                    &working_directory,
                    cursor.as_deref(),
                    limit,
                    max_bytes,
                )
                .await?,
            )
        }
        AgentIntelCommand::CopyProjectMemory {
            source_slug,
            filename,
            target_slug,
            ..
        } => {
            let home = home_dir()?;
            into_value(
                projects::copy_project_memory(&home, &source_slug, &filename, &target_slug).await?,
            )
        }
        AgentIntelCommand::ResolveActiveSession {
            cwd,
            session_id,
            expected_runtime_incarnation_id,
            ..
        } => {
            let (local_sessions, agent_intel) = local_transcript_authority
                .ok_or_else(|| "local transcript authority is unavailable".to_owned())?;
            let session_id = kodosi_domain::ids::SessionId::parse_field(&session_id, "sessionId")
                .map_err(|error| error.to_string())?;
            let expected_incarnation = uuid::Uuid::parse_str(&expected_runtime_incarnation_id)
                .map_err(|error| format!("invalid expectedRuntimeIncarnationId: {error}"))?;
            let record = local_sessions
                .record(session_id)
                .filter(|record| record.local_incarnation_id == expected_incarnation)
                .ok_or_else(|| "local session incarnation is no longer current".to_owned())?;
            if record.summary.working_dir.as_deref() != Some(cwd.as_str()) {
                return Err("requested cwd does not match the local session".to_owned());
            }
            let native_session_id = agent_intel.agent_session_id(session_id).ok_or_else(|| {
                "the selected session has no observed native transcript".to_owned()
            })?;
            let active_jsonl = ::agent_intel::claude::ClaudeCodeProvider::default()
                .transcript_path(&cwd, native_session_id)
                .map(|path| path.to_string_lossy().into_owned());
            into_value(serde_json::json!({
                "runtimeSessionId": session_id.to_string(),
                "runtimeIncarnationId": expected_incarnation.to_string(),
                "nativeSessionId": native_session_id,
                "activeJsonl": active_jsonl,
            }))
        }
        AgentIntelCommand::ReadClaudeAutoModeRules { .. } => {
            let home = home_dir()?;
            let rules = auto_mode::read_auto_mode_rules(&home).await?;
            into_value(rules)
        }
        AgentIntelCommand::WriteClaudeAutoModeRules {
            environment,
            allow,
            soft_deny,
            hard_deny,
            ..
        } => {
            let home = home_dir()?;
            auto_mode::write_auto_mode_rules(
                &home,
                auto_mode::AutoModeRules {
                    environment,
                    allow,
                    soft_deny,
                    hard_deny,
                },
            )
            .await?;
            Ok(Value::Null)
        }
        AgentIntelCommand::ListCustomAgents { directory, .. } => {
            let dir = PathBuf::from(directory);
            let parsed = tokio::task::spawn_blocking(move || {
                custom_agents::find_custom_agent_files(
                    &dir,
                    custom_agents::AgentFileConvention::ClaudeMarkdown,
                )
                .into_iter()
                .filter_map(|path| custom_agents::CustomAgentFile::parse(&path).ok())
                .collect::<Vec<_>>()
            })
            .await
            .map_err(|error| format!("custom agent scan task join error: {error}"))?;
            into_value(parsed)
        }
        AgentIntelCommand::ListActiveCustomizations { cwd, agent, .. } => {
            let home = home_dir()?;
            let vendor = match agent.as_str() {
                "claude" => ::agent_intel::domain::CustomizationVendor::Claude,
                "copilot" => ::agent_intel::domain::CustomizationVendor::Copilot,
                other => {
                    return Err(format!(
                        "listActiveCustomizations unsupported for agent: {other}"
                    ));
                }
            };
            let report =
                diagnostics::list_active_customizations(&home, &PathBuf::from(cwd), vendor)
                    .await
                    .map_err(|error| format!("diagnostics: {error}"))?;
            into_value(report)
        }
        AgentIntelCommand::DiscoverExternalMcpServers { .. } => {
            let home = home_dir()?;
            let servers = ::agent_intel::mcp::discover_external_mcp_servers(&home);
            into_value(servers)
        }
        AgentIntelCommand::DiscoverExternalSessions { agent, .. } => {
            let home = home_dir()?;
            let filter = match agent.as_deref() {
                Some("claude") => Some(external_sessions::ExternalAgent::Claude),
                Some("copilot") => Some(external_sessions::ExternalAgent::Copilot),
                Some(other) => return Err(format!("unsupported agent filter: {other}")),
                None => None,
            };
            let sessions = external_sessions::discover_external_sessions(&home, filter).await;
            into_value(sessions)
        }
        AgentIntelCommand::ListSubagentTranscripts {
            cwd, session_id, ..
        } => {
            let slug = ::agent_intel::claude::ClaudeCodeProvider::encode_project_path(&cwd);
            let session_id = subagent_layout::TranscriptSessionId::parse(&session_id)
                .map_err(|error| format!("invalid transcript sessionId: {error}"))?;
            let projects_dir = ::agent_intel::runtime::paths::claude_home().join("projects");
            let transcripts =
                subagent_layout::find_subagent_transcripts(&projects_dir, &slug, &session_id);
            let entries: Vec<_> = transcripts
                .into_iter()
                .map(|t| {
                    serde_json::json!({
                        "agentId": t.agent_id,
                        "path": t.path.to_string_lossy(),
                        "workflowRunId": t.workflow_run_id,
                    })
                })
                .collect();
            into_value(serde_json::json!({
                "transcripts": entries,
            }))
        }
    }
}

fn parse_agent(value: &str) -> Result<AgentKind, String> {
    match value {
        "claude" => Ok(AgentKind::Claude),
        "copilot" => Ok(AgentKind::Copilot),
        other => Err(format!("unsupported agent: {other}")),
    }
}

fn home_dir() -> Result<PathBuf, String> {
    BaseDirs::new()
        .map(|dirs| dirs.home_dir().to_path_buf())
        .ok_or_else(|| "could not determine home directory".to_owned())
}

fn into_value<T: Serialize>(value: T) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|error| format!("serialize reply: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn collect_reply(command: AgentIntelCommand) -> AgentIntelEvent {
        let (tx, mut rx) = mpsc::channel(1);
        let local_sessions = crate::session_runtime::registry::SessionRegistry::default();
        let agent_intel = crate::agent_intel::AgentIntelRegistry::default();
        handle_runtime(
            command,
            &local_sessions,
            &agent_intel,
            tx,
            Some("account".to_owned()),
            7,
        )
        .await;
        let envelope = rx
            .recv()
            .await
            .expect("dispatch must always emit one event");
        assert_eq!(envelope.account_user_id.as_deref(), Some("account"));
        assert_eq!(envelope.account_epoch, 7);
        envelope.event
    }

    #[tokio::test]
    async fn resolve_active_session_requires_local_transcript_authority() {
        let temp = tempfile::tempdir().expect("tempdir");
        let event = collect_reply(AgentIntelCommand::ResolveActiveSession {
            request_id: "test-3".to_owned(),
            cwd: temp.path().to_string_lossy().into_owned(),
            session_id: "01900000-0000-7000-8000-000000000001".to_owned(),
            expected_runtime_incarnation_id: "01900000-0000-7000-8000-000000000002".to_owned(),
        })
        .await;
        std::assert_matches!(event, AgentIntelEvent::Error { message, .. } if message.contains("no longer current"));
    }

    #[tokio::test]
    async fn exact_runtime_sessions_sharing_cwd_resolve_distinct_native_transcripts() {
        use kodosi_domain::{ids::SessionId, session::SessionSummary, terminal::TerminalSize};

        let cwd = "/tmp/shared-project";
        let first = SessionId::new();
        let second = SessionId::new();
        let first_incarnation = uuid::Uuid::now_v7();
        let second_incarnation = uuid::Uuid::now_v7();
        let mut local_sessions = crate::session_runtime::registry::SessionRegistry::default();
        for (id, incarnation) in [(first, first_incarnation), (second, second_incarnation)] {
            let mut summary = SessionSummary::new_owned(
                id,
                "Session".to_owned(),
                "shell".to_owned(),
                TerminalSize::new(80, 24).expect("size"),
                None,
            );
            summary.working_dir = Some(cwd.to_owned());
            local_sessions.insert_discovered(
                summary,
                None,
                incarnation,
                kodosi_domain::session::LocalSessionRecoveryState::Live,
            );
        }
        let mut agent_intel = crate::agent_intel::AgentIntelRegistry::default();
        agent_intel.set_agent_session_id(first, Some("native-first"));
        agent_intel.set_agent_session_id(second, Some("native-second"));

        let first_payload = dispatch(
            AgentIntelCommand::ResolveActiveSession {
                request_id: "first".to_owned(),
                cwd: cwd.to_owned(),
                session_id: first.to_string(),
                expected_runtime_incarnation_id: first_incarnation.to_string(),
            },
            Some((&local_sessions, &agent_intel)),
        )
        .await
        .expect("first mapping");
        let second_payload = dispatch(
            AgentIntelCommand::ResolveActiveSession {
                request_id: "second".to_owned(),
                cwd: cwd.to_owned(),
                session_id: second.to_string(),
                expected_runtime_incarnation_id: second_incarnation.to_string(),
            },
            Some((&local_sessions, &agent_intel)),
        )
        .await
        .expect("second mapping");

        assert_eq!(first_payload["nativeSessionId"], "native-first");
        assert_eq!(second_payload["nativeSessionId"], "native-second");
        assert_ne!(first_payload["activeJsonl"], second_payload["activeJsonl"]);
    }

    #[tokio::test]
    async fn replacement_incarnation_rejects_retired_transcript_mapping() {
        use kodosi_domain::{ids::SessionId, session::SessionSummary, terminal::TerminalSize};
        let session_id = SessionId::new();
        let current_incarnation = uuid::Uuid::now_v7();
        let retired_incarnation = uuid::Uuid::now_v7();
        let mut summary = SessionSummary::new_owned(
            session_id,
            "Session".to_owned(),
            "shell".to_owned(),
            TerminalSize::new(80, 24).expect("size"),
            None,
        );
        summary.working_dir = Some("/tmp/project".to_owned());
        let mut local_sessions = crate::session_runtime::registry::SessionRegistry::default();
        local_sessions.insert_discovered(
            summary,
            None,
            current_incarnation,
            kodosi_domain::session::LocalSessionRecoveryState::Live,
        );
        let mut agent_intel = crate::agent_intel::AgentIntelRegistry::default();
        agent_intel.set_agent_session_id(session_id, Some("native-current"));

        let error = dispatch(
            AgentIntelCommand::ResolveActiveSession {
                request_id: "stale".to_owned(),
                cwd: "/tmp/project".to_owned(),
                session_id: session_id.to_string(),
                expected_runtime_incarnation_id: retired_incarnation.to_string(),
            },
            Some((&local_sessions, &agent_intel)),
        )
        .await
        .expect_err("retired incarnation must fail closed");
        assert!(error.contains("no longer current"));
    }

    #[tokio::test]
    async fn list_subagent_transcripts_rejects_non_component_session_id() {
        let error = dispatch(
            AgentIntelCommand::ListSubagentTranscripts {
                request_id: "transcripts".to_owned(),
                cwd: "/tmp/project".to_owned(),
                session_id: "../escape".to_owned(),
            },
            None,
        )
        .await
        .expect_err("path-like transcript identity must fail closed");
        assert!(error.contains("invalid transcript sessionId"));
    }

    #[tokio::test]
    async fn active_customizations_explicit_vendor_selects_adapter() {
        let cwd = tempfile::tempdir().expect("cwd");
        let event = collect_reply(AgentIntelCommand::ListActiveCustomizations {
            request_id: "request-2".to_owned(),
            cwd: cwd.path().to_string_lossy().into_owned(),
            agent: "copilot".to_owned(),
        })
        .await;
        let AgentIntelEvent::Reply { payload, .. } = event else {
            panic!("expected reply");
        };
        assert_eq!(payload["vendor"], "copilot");
    }
}
