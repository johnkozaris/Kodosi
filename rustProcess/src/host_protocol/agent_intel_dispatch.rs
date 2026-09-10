use std::path::PathBuf;

use directories::BaseDirs;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::mpsc;

use ::agent_intel::AgentKind;
use ::agent_intel::claude::subagent_layout;
use ::agent_intel::ops::{
    auto_mode,
    bound_custom_agents::StagedCustomAgentCapabilities,
    bound_memory::{MemorySelectionScope, StagedMemoryCapabilities},
    bound_projects::{
        ActiveProjectSource, BoundProjectAgentSettings, BoundProjectCustomAgentSummary,
        BoundProjectCustomizationSummary, BoundProjectMemorySummary, BoundProjectSessionSummary,
        BoundProjectSnapshot, ProjectSourceLocator,
    },
    copilot_repos, custom_agents, diagnostics, external_sessions, import, memory, projects,
    provider_conversations, settings,
    settings_tree::AgentSettingsTreeBundle,
};

use super::{AccountAgentIntelEvent, AgentIntelCommand, AgentIntelEvent};

pub(crate) async fn handle_runtime(
    command: AgentIntelCommand,
    local_sessions: &crate::session_runtime::registry::SessionRegistry,
    agent_intel: &mut crate::agent_intel::AgentIntelState,
    event_tx: mpsc::Sender<AccountAgentIntelEvent>,
    account_user_id: Option<String>,
    account_epoch: u64,
) {
    let request_id = command.request_id().to_owned();
    let mutation_id = command.mutation_id().map(str::to_owned);
    let may_have_ambiguous_delivery = command.may_have_ambiguous_mutation_delivery();
    let selection_scope = MemorySelectionScope {
        account_user_id: account_user_id.clone(),
        account_epoch,
    };
    let reply = match dispatch(
        command,
        Some((local_sessions, agent_intel)),
        &selection_scope,
    )
    .await
    {
        Ok(payload) => AgentIntelEvent::Reply {
            request_id,
            payload,
        },
        Err(message) => {
            let failure_kind = classify_failure(may_have_ambiguous_delivery, &message);
            let delivery_ambiguous =
                failure_kind == super::AgentIntelFailureKind::DeliveryAmbiguous;
            AgentIntelEvent::Error {
                request_id,
                message,
                failure_kind,
                mutation_id,
                reconciliation_required: delivery_ambiguous,
            }
        }
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

fn classify_failure(
    may_have_ambiguous_delivery: bool,
    message: &str,
) -> super::AgentIntelFailureKind {
    if may_have_ambiguous_delivery
        && (message.contains("task join") || message.contains("ledger is unavailable"))
    {
        super::AgentIntelFailureKind::DeliveryAmbiguous
    } else {
        super::AgentIntelFailureKind::Deterministic
    }
}

type LocalTranscriptAuthority<'a> = (
    &'a crate::session_runtime::registry::SessionRegistry,
    &'a mut crate::agent_intel::AgentIntelState,
);

struct StagedProjectSnapshot {
    snapshot: BoundProjectSnapshot,
    memory: Option<StagedMemoryCapabilities>,
    custom_agents: Vec<StagedCustomAgentCapabilities>,
}

async fn dispatch(
    command: AgentIntelCommand,
    local_transcript_authority: Option<LocalTranscriptAuthority<'_>>,
    selection_scope: &MemorySelectionScope,
) -> Result<Value, String> {
    match command {
        AgentIntelCommand::ListProjectSourcesBound {
            cursor,
            limit,
            max_bytes,
            ..
        } => {
            let (local_sessions, agent_intel) = local_transcript_authority
                .ok_or_else(|| "bound project source authority is unavailable".to_owned())?;
            let mut counts = std::collections::BTreeMap::<String, (u64, Vec<String>)>::new();
            for id in local_sessions.ids() {
                let Some(record) = local_sessions.record(*id) else {
                    continue;
                };
                let Some(cwd) = record.summary.working_dir.as_ref() else {
                    continue;
                };
                let entry = counts.entry(cwd.clone()).or_default();
                entry.0 += 1;
                entry.1.push(record.summary.id.to_string());
            }
            let active_sources = counts
                .into_iter()
                .map(
                    |(working_directory, (session_count, session_ids))| ActiveProjectSource {
                        working_directory,
                        session_count,
                        session_ids,
                    },
                )
                .collect();
            let home = home_dir()?;
            into_value(
                agent_intel
                    .bound_projects
                    .list(
                        &home,
                        active_sources,
                        cursor.as_deref(),
                        limit,
                        max_bytes,
                        selection_scope.clone(),
                    )
                    .await?,
            )
        }
        AgentIntelCommand::InspectProjectSourceBound {
            selection_token, ..
        } => {
            let (local_sessions, agent_intel) = local_transcript_authority
                .ok_or_else(|| "bound project source authority is unavailable".to_owned())?;
            let resolution = agent_intel
                .bound_projects
                .stage_resolve(&selection_token, selection_scope)?;
            let staged = project_snapshot(
                local_sessions,
                agent_intel,
                &resolution.resolved,
                selection_scope,
            )
            .await?;
            let payload = into_value(&staged.snapshot)?;
            if let Some(memory) = staged.memory.as_ref() {
                agent_intel
                    .bound_memory
                    .preflight_staged_capabilities(memory)?;
                agent_intel
                    .bound_memory
                    .validate_staged_capabilities(memory)?;
            }
            let custom_agent_stages = staged.custom_agents.iter().collect::<Vec<_>>();
            agent_intel
                .bound_custom_agents
                .preflight_staged_capabilities(&custom_agent_stages)?;
            agent_intel
                .bound_custom_agents
                .validate_staged_capabilities(&custom_agent_stages)?;
            resolution.resolved.verify_current()?;
            if let Some(memory) = staged.memory {
                agent_intel.bound_memory.commit_staged_capabilities(memory);
            }
            agent_intel
                .bound_custom_agents
                .commit_staged_capabilities(staged.custom_agents);
            agent_intel.bound_projects.commit_resolution(&resolution);
            Ok(payload)
        }
        AgentIntelCommand::ReadSettings { agent, cwd, .. } => {
            let agent = parse_agent(&agent)?;
            into_value(settings::read_settings(agent, cwd.as_deref()).await?)
        }
        other => {
            return dispatch_remaining(other, local_transcript_authority, selection_scope).await;
        }
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "the project snapshot assembles one security-fenced cross-source projection"
)]
async fn project_snapshot(
    local_sessions: &crate::session_runtime::registry::SessionRegistry,
    agent_intel: &crate::agent_intel::AgentIntelState,
    source: &::agent_intel::ops::bound_projects::ResolvedProjectSource,
    selection_scope: &MemorySelectionScope,
) -> Result<StagedProjectSnapshot, String> {
    let home = home_dir()?;
    source.verify_current()?;
    let mut sessions = Vec::new();
    let mut memories = Vec::new();
    let mut custom_agents = Vec::new();
    let mut customizations = Vec::new();
    let mut projected_settings = Vec::new();
    let mut staged_memory = None;
    let mut staged_custom_agents = Vec::new();
    match &source.locator {
        ProjectSourceLocator::Active { canonical_cwd } => {
            let cwd = canonical_cwd.to_string_lossy().into_owned();
            let projection_root = source.held_projection_path()?;
            let held_project = source.held_directory()?;
            for id in local_sessions.ids() {
                let Some(record) = local_sessions.record(*id) else {
                    continue;
                };
                if record.summary.working_dir.as_deref() != Some(cwd.as_str()) {
                    continue;
                }
                sessions.push(BoundProjectSessionSummary {
                    session_id: record.summary.id.to_string(),
                    runtime_incarnation_id: Some(record.local_incarnation_id.to_string()),
                    title: bounded_text(&record.summary.title, 512),
                    agent: bounded_text(
                        record
                            .summary
                            .detected_agent
                            .as_deref()
                            .unwrap_or(&record.summary.runtime_name),
                        128,
                    ),
                    status: format!("{:?}", record.summary.state).to_ascii_lowercase(),
                    mode: Some(format!("{:?}", record.summary.mode).to_ascii_lowercase()),
                    started_at: Some(record.summary.created_at.to_string()),
                    updated_at: Some(record.summary.last_update.to_string()),
                    size_bytes: None,
                    host_type: None,
                    transcript_available: agent_intel
                        .registry
                        .agent_session_id(record.summary.id)
                        .is_some(),
                });
            }
            sessions.sort_by(|left, right| {
                right
                    .updated_at
                    .cmp(&left.updated_at)
                    .then(left.title.cmp(&right.title))
                    .then(left.session_id.cmp(&right.session_id))
            });
            source.verify_current()?;
            let memory_stage = agent_intel
                .bound_memory
                .stage_capabilities_from_root(
                    &home,
                    canonical_cwd,
                    &held_project,
                    selection_scope.clone(),
                )
                .await?;
            memories = memory_stage
                .items()
                .iter()
                .cloned()
                .map(|item| BoundProjectMemorySummary {
                    read_selection_token: item.read_selection_token,
                    open_selection_token: item.open_selection_token,
                    copy_selection_token: item.copy_selection_token,
                    filename: item.filename,
                    memory_type: item.memory_type,
                })
                .collect();
            staged_memory = Some(memory_stage);
            source.verify_current()?;
            for (components, target) in [
                (
                    [".claude", "agents"],
                    ::agent_intel::ops::custom_agents::AgentTarget::Claude,
                ),
                (
                    [".github", "agents"],
                    ::agent_intel::ops::custom_agents::AgentTarget::VsCode,
                ),
            ] {
                source.verify_current()?;
                let remaining =
                    ::agent_intel::ops::bound_custom_agents::MAX_BOUND_PROJECT_CUSTOM_AGENTS
                        .saturating_sub(custom_agents.len());
                if remaining == 0 {
                    break;
                }
                let custom_agent_stage = agent_intel
                    .bound_custom_agents
                    .stage_capabilities_from_root(
                        &held_project,
                        &components,
                        target,
                        selection_scope.clone(),
                        remaining,
                    )
                    .await?;
                custom_agents.extend(custom_agent_stage.items().iter().cloned().map(|item| {
                    let target = match item.target {
                        ::agent_intel::ops::custom_agents::AgentTarget::Claude => "claude",
                        ::agent_intel::ops::custom_agents::AgentTarget::VsCode => "vsCode",
                        ::agent_intel::ops::custom_agents::AgentTarget::Unknown => "unknown",
                    };
                    BoundProjectCustomAgentSummary {
                        detail_selection_token: item.detail_selection_token,
                        open_selection_token: item.open_selection_token,
                        target: target.to_owned(),
                        name: item.name,
                        description: item.description,
                        model: item.model,
                        tools: item.tools,
                        error_count: item.error_count,
                    }
                }));
                staged_custom_agents.push(custom_agent_stage);
                source.verify_current()?;
            }
            custom_agents.sort_by(|left, right| {
                left.name
                    .to_ascii_lowercase()
                    .cmp(&right.name.to_ascii_lowercase())
                    .then(left.name.cmp(&right.name))
            });
            for (agent_name, agent_kind, vendor) in [
                (
                    "claude",
                    AgentKind::Claude,
                    ::agent_intel::domain::CustomizationVendor::Claude,
                ),
                (
                    "copilot",
                    AgentKind::Copilot,
                    ::agent_intel::domain::CustomizationVendor::Copilot,
                ),
            ] {
                source.verify_current()?;
                let bundle = settings::read_settings_from_root(agent_kind, &held_project).await?;
                source.verify_current()?;
                projected_settings.push(BoundProjectAgentSettings {
                    agent: agent_name.to_owned(),
                    settings: AgentSettingsTreeBundle::project(&bundle)?,
                });
                source.verify_current()?;
                let report = diagnostics::list_active_customizations_from_root(
                    &home,
                    &held_project,
                    &projection_root,
                    vendor,
                )
                .await
                .map_err(|error| format!("project customizations: {error}"))?;
                source.verify_current()?;
                customizations.extend(
                    report
                        .skills
                        .into_iter()
                        .chain(report.plugins)
                        .chain(report.mcp_servers)
                        .chain(report.instructions)
                        .map(sanitize_customization),
                );
            }
        }
        ProjectSourceLocator::ClaudeArchive { project_slug } => {
            source.verify_current()?;
            let projection_root = source.held_projection_path()?;
            sessions = projects::list_project_sessions_at(&projection_root)
                .await?
                .into_iter()
                .map(|session| BoundProjectSessionSummary {
                    title: session.id.clone(),
                    session_id: session.id,
                    runtime_incarnation_id: None,
                    agent: "claude".to_owned(),
                    status: "archived".to_owned(),
                    mode: None,
                    started_at: session.started_at,
                    updated_at: session.modified_at,
                    size_bytes: Some(session.size_bytes),
                    host_type: None,
                    transcript_available: false,
                })
                .collect();
            source.verify_current()?;
            let memory_stage = agent_intel
                .bound_memory
                .stage_archive_capabilities(&home, project_slug, selection_scope.clone())
                .await?;
            memories = memory_stage
                .items()
                .iter()
                .cloned()
                .map(|item| BoundProjectMemorySummary {
                    read_selection_token: item.read_selection_token,
                    open_selection_token: item.open_selection_token,
                    copy_selection_token: item.copy_selection_token,
                    filename: item.filename,
                    memory_type: item.memory_type,
                })
                .collect();
            staged_memory = Some(memory_stage);
            source.verify_current()?;
        }
        ProjectSourceLocator::CopilotArchive { repository } => {
            source.verify_current()?;
            let projection_database = source.held_projection_path()?;
            sessions = copilot_repos::list_repo_sessions_at(&projection_database, repository)
                .await?
                .into_iter()
                .map(|session| BoundProjectSessionSummary {
                    title: bounded_text(session.summary.as_deref().unwrap_or(&session.id), 1024),
                    session_id: session.id,
                    runtime_incarnation_id: None,
                    agent: "copilot".to_owned(),
                    status: "archived".to_owned(),
                    mode: None,
                    started_at: session.created_at,
                    updated_at: session.updated_at,
                    size_bytes: None,
                    host_type: session.host_type.map(|value| bounded_text(&value, 128)),
                    transcript_available: false,
                })
                .collect();
            source.verify_current()?;
        }
    }
    sessions.truncate(500);
    memories.truncate(512);
    custom_agents.truncate(128);
    customizations.truncate(512);
    source.verify_current()?;
    let snapshot = BoundProjectSnapshot {
        source_selection_token: source.next_selection_token.clone(),
        source_kind: source.source_kind,
        agent: source.agent.clone(),
        label: source.label.clone(),
        sessions,
        memories,
        custom_agents,
        customizations,
        settings: projected_settings,
    };
    let bytes = serde_json::to_vec(&snapshot)
        .map_err(|error| format!("encode project snapshot: {error}"))?
        .len();
    if bytes > 8 * 1024 * 1024 {
        return Err("project snapshot exceeds 8388608 byte limit".to_owned());
    }
    Ok(StagedProjectSnapshot {
        snapshot,
        memory: staged_memory,
        custom_agents: staged_custom_agents,
    })
}

fn sanitize_customization(
    entry: ::agent_intel::domain::CustomizationEntry,
) -> BoundProjectCustomizationSummary {
    let scope = match entry.scope {
        ::agent_intel::domain::Scope::User => "user".to_owned(),
        ::agent_intel::domain::Scope::Workspace { .. } => "project".to_owned(),
        ::agent_intel::domain::Scope::Plugin { marketplace, .. } => {
            format!("plugin:{marketplace}")
        }
        ::agent_intel::domain::Scope::BuiltIn => "builtIn".to_owned(),
        ::agent_intel::domain::Scope::Managed => "managed".to_owned(),
        ::agent_intel::domain::Scope::Extension { id } => {
            id.map_or_else(|| "extension".to_owned(), |id| format!("extension:{id}"))
        }
    };
    BoundProjectCustomizationSummary {
        kind: format!("{:?}", entry.kind).to_ascii_lowercase(),
        name: bounded_text(&entry.name, 256),
        scope: bounded_text(&scope, 256),
        enabled: entry.enabled,
        status: format!("{:?}", entry.status).to_ascii_lowercase(),
        description: entry.description.map(|value| bounded_text(&value, 2048)),
        status_message: entry.status_message.map(|value| bounded_text(&value, 2048)),
    }
}

fn bounded_text(value: &str, maximum_bytes: usize) -> String {
    if value.len() <= maximum_bytes {
        return value.to_owned();
    }
    value[..value.floor_char_boundary(maximum_bytes)].to_owned()
}

#[expect(
    clippy::too_many_lines,
    reason = "one exhaustive match retains compile-time coverage of the legacy command family"
)]
async fn dispatch_remaining(
    command: AgentIntelCommand,
    local_transcript_authority: Option<LocalTranscriptAuthority<'_>>,
    selection_scope: &MemorySelectionScope,
) -> Result<Value, String> {
    match command {
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
        AgentIntelCommand::ListClaudeMemoryBound { cwd, .. } => {
            let home = home_dir()?;
            let (_, agent_intel) = local_transcript_authority
                .ok_or_else(|| "bound memory authority is unavailable".to_owned())?;
            into_value(
                agent_intel
                    .bound_memory
                    .list(&home, &cwd, selection_scope.clone())
                    .await?,
            )
        }
        AgentIntelCommand::ReadClaudeMemoryBound {
            selection_token, ..
        } => {
            let (_, agent_intel) = local_transcript_authority
                .ok_or_else(|| "bound memory authority is unavailable".to_owned())?;
            into_value(
                agent_intel
                    .bound_memory
                    .read(&selection_token, selection_scope)
                    .await?,
            )
        }
        AgentIntelCommand::OpenProjectMemoryBound {
            selection_token, ..
        } => {
            let (_, agent_intel) = local_transcript_authority
                .ok_or_else(|| "bound memory authority is unavailable".to_owned())?;
            let (file, display_name) = agent_intel
                .bound_memory
                .open(&selection_token, selection_scope)
                .await?;
            into_value(agent_intel.open_handoffs.prepare(file, display_name)?)
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
        AgentIntelCommand::CopyProjectMemoryBound {
            source_selection_token,
            destination_selection_token,
            mutation_id,
            ..
        } => {
            let (_, agent_intel) = local_transcript_authority.ok_or_else(|| {
                "bound project memory mutation authority is unavailable".to_owned()
            })?;
            if let Some(receipt) = agent_intel.project_mutations.existing(&mutation_id)? {
                return into_value(receipt);
            }
            let destination = agent_intel
                .bound_projects
                .stage_resolve(&destination_selection_token, selection_scope)?;
            let target_label = destination.resolved.label.clone();
            let memory_destination = destination.resolved.memory_destination()?;
            let receipt = agent_intel
                .bound_memory
                .copy_to_project(
                    &source_selection_token,
                    memory_destination,
                    &mutation_id,
                    target_label,
                    agent_intel.project_mutations.clone(),
                    selection_scope,
                )
                .await?;
            agent_intel.bound_projects.consume_resolution(&destination);
            into_value(receipt)
        }
        AgentIntelCommand::ReconcileProjectMemoryCopy { mutation_id, .. } => {
            let (_, agent_intel) = local_transcript_authority.ok_or_else(|| {
                "bound project memory mutation authority is unavailable".to_owned()
            })?;
            into_value(agent_intel.project_mutations.reconcile(&mutation_id)?)
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
            let native_session_id = agent_intel
                .registry
                .agent_session_id(session_id)
                .ok_or_else(|| {
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
        AgentIntelCommand::ReadClaudeAutoModeRulesBound { .. } => {
            let (_, agent_intel) = local_transcript_authority
                .ok_or_else(|| "bound auto-mode authority is unavailable".to_owned())?;
            let home = home_dir()?;
            into_value(
                agent_intel
                    .bound_auto_mode
                    .read(&home, selection_scope.clone())
                    .await?,
            )
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
        AgentIntelCommand::WriteClaudeAutoModeRulesBound {
            target_token,
            expected_revision,
            mutation_id,
            environment,
            allow,
            soft_deny,
            hard_deny,
            ..
        } => {
            let (_, agent_intel) = local_transcript_authority
                .ok_or_else(|| "bound auto-mode authority is unavailable".to_owned())?;
            let home = home_dir()?;
            into_value(
                agent_intel
                    .bound_auto_mode
                    .write(
                        &home,
                        &target_token,
                        &expected_revision,
                        &mutation_id,
                        auto_mode::AutoModeRules {
                            environment,
                            allow,
                            soft_deny,
                            hard_deny,
                        },
                        selection_scope,
                    )
                    .await?,
            )
        }
        AgentIntelCommand::ReconcileClaudeAutoModeRulesWrite { mutation_id, .. } => {
            let (_, agent_intel) = local_transcript_authority
                .ok_or_else(|| "bound auto-mode authority is unavailable".to_owned())?;
            into_value(agent_intel.bound_auto_mode.reconcile(&mutation_id)?)
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
        AgentIntelCommand::ListCustomAgentsBound { directory, .. } => {
            let (_, agent_intel) = local_transcript_authority
                .ok_or_else(|| "bound custom-agent authority is unavailable".to_owned())?;
            into_value(
                agent_intel
                    .bound_custom_agents
                    .list(&directory, selection_scope.clone())
                    .await?,
            )
        }
        AgentIntelCommand::ReadCustomAgentBound {
            selection_token, ..
        } => {
            let (_, agent_intel) = local_transcript_authority
                .ok_or_else(|| "bound custom-agent authority is unavailable".to_owned())?;
            into_value(
                agent_intel
                    .bound_custom_agents
                    .read(&selection_token, selection_scope)
                    .await?,
            )
        }
        AgentIntelCommand::OpenCustomAgentBound {
            selection_token, ..
        } => {
            let (_, agent_intel) = local_transcript_authority
                .ok_or_else(|| "bound custom-agent authority is unavailable".to_owned())?;
            let (file, display_name) = agent_intel
                .bound_custom_agents
                .open(&selection_token, selection_scope)
                .await?;
            into_value(agent_intel.open_handoffs.prepare(file, display_name)?)
        }
        AgentIntelCommand::ReleaseOpenHandoff { handoff_id, .. } => {
            let (_, agent_intel) = local_transcript_authority
                .ok_or_else(|| "native open handoff authority is unavailable".to_owned())?;
            agent_intel.open_handoffs.release(&handoff_id)?;
            Ok(Value::Null)
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
        AgentIntelCommand::DiscoverExternalBound { .. } => {
            let (_, agent_intel) = local_transcript_authority
                .ok_or_else(|| "bound external discovery authority is unavailable".to_owned())?;
            let home = home_dir()?;
            into_value(
                agent_intel
                    .bound_external
                    .discover(&home, selection_scope.clone())
                    .await?,
            )
        }
        AgentIntelCommand::ExternalSourceActionBound {
            selection_token,
            action,
            ..
        } => {
            let (_, agent_intel) = local_transcript_authority
                .ok_or_else(|| "bound external discovery authority is unavailable".to_owned())?;
            let selected = agent_intel
                .bound_external
                .take(&selection_token, selection_scope)?;
            match action.as_str() {
                "copyPath" => into_value(
                    ::agent_intel::ops::bound_external::BoundExternalAction::CopyPath {
                        source_path: selected.path.to_string_lossy().into_owned(),
                        handoff_id: None,
                        handoff_path: None,
                        display_name: selected.display_name,
                    },
                ),
                "reveal" => into_value(
                    ::agent_intel::ops::bound_external::BoundExternalAction::Reveal {
                        source_path: selected.path.to_string_lossy().into_owned(),
                        handoff_id: None,
                        handoff_path: None,
                        display_name: selected.display_name,
                    },
                ),
                "open" => {
                    let handoff = agent_intel
                        .open_handoffs
                        .prepare(selected.file, selected.display_name)?;
                    into_value(
                        ::agent_intel::ops::bound_external::BoundExternalAction::Open {
                            source_path: None,
                            handoff_id: handoff.handoff_id,
                            handoff_path: handoff.handoff_path,
                            display_name: handoff.display_name,
                        },
                    )
                }
                _ => Err("unsupported external source action".to_owned()),
            }
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
        AgentIntelCommand::ListProjectSourcesBound { .. }
        | AgentIntelCommand::InspectProjectSourceBound { .. }
        | AgentIntelCommand::ReadSettings { .. } => {
            Err("agent-intel command reached the wrong dispatcher".to_owned())
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
        let mut agent_intel = crate::agent_intel::AgentIntelState::default();
        handle_runtime(
            command,
            &local_sessions,
            &mut agent_intel,
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
        std::assert_matches!(
            event,
            AgentIntelEvent::Error {
                message,
                failure_kind: crate::host_protocol::AgentIntelFailureKind::Deterministic,
                mutation_id: None,
                reconciliation_required: false,
                ..
            } if message.contains("no longer current")
        );
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
        let mut agent_intel = crate::agent_intel::AgentIntelState::default();
        agent_intel
            .registry
            .set_agent_session_id(first, Some("native-first"));
        agent_intel
            .registry
            .set_agent_session_id(second, Some("native-second"));
        let scope = MemorySelectionScope {
            account_user_id: Some("account".to_owned()),
            account_epoch: 7,
        };

        let first_payload = dispatch(
            AgentIntelCommand::ResolveActiveSession {
                request_id: "first".to_owned(),
                cwd: cwd.to_owned(),
                session_id: first.to_string(),
                expected_runtime_incarnation_id: first_incarnation.to_string(),
            },
            Some((&local_sessions, &mut agent_intel)),
            &scope,
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
            Some((&local_sessions, &mut agent_intel)),
            &scope,
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
        let mut agent_intel = crate::agent_intel::AgentIntelState::default();
        agent_intel
            .registry
            .set_agent_session_id(session_id, Some("native-current"));
        let scope = MemorySelectionScope {
            account_user_id: Some("account".to_owned()),
            account_epoch: 7,
        };

        let error = dispatch(
            AgentIntelCommand::ResolveActiveSession {
                request_id: "stale".to_owned(),
                cwd: "/tmp/project".to_owned(),
                session_id: session_id.to_string(),
                expected_runtime_incarnation_id: retired_incarnation.to_string(),
            },
            Some((&local_sessions, &mut agent_intel)),
            &scope,
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
            &MemorySelectionScope {
                account_user_id: None,
                account_epoch: 0,
            },
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

    #[test]
    fn mutation_failure_taxonomy_only_reconciles_delivery_ambiguity() {
        for deterministic in [
            "auto-mode target is stale or already consumed",
            "auto-mode rules changed; reload before saving",
            "autoMode.allow[0] exceeds byte cap",
            "operation cancelled before commit",
        ] {
            assert_eq!(
                classify_failure(true, deterministic),
                crate::host_protocol::AgentIntelFailureKind::Deterministic
            );
        }
        assert_eq!(
            classify_failure(true, "bound auto-mode write task join: cancelled"),
            crate::host_protocol::AgentIntelFailureKind::DeliveryAmbiguous
        );
        assert_eq!(
            classify_failure(false, "background read task join: cancelled"),
            crate::host_protocol::AgentIntelFailureKind::Deterministic
        );
    }
}
