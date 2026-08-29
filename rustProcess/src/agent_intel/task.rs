use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Duration;

use agent_intel::runtime::io_pool;
use agent_intel::{
    AgentKind, ArtifactWatcher, FsArtifactWatcher, LiveAgentProvider, WatchTarget,
    create_live_provider, global_watch_targets, normalize_workspace_root, workspace_watch_targets,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::session_runtime::events::RuntimeSessionEvent;
use kodosi_domain::ids::SessionId;

const TERMINAL_CHANNEL_CAPACITY: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct WatcherKey {
    agent: AgentKind,
    workspace: Option<PathBuf>,
}

struct SessionWatcher {
    handles: Vec<WatcherLease>,
}

struct WatcherLease {
    key: WatcherKey,
    handle: Arc<FsArtifactWatcher>,
}

struct SessionWatcherReceivers {
    first: Option<tokio::sync::broadcast::Receiver<agent_intel::WatchEvent>>,
    second: Option<tokio::sync::broadcast::Receiver<agent_intel::WatchEvent>>,
}

impl SessionWatcherReceivers {
    async fn recv(&mut self) -> Option<agent_intel::WatchEvent> {
        loop {
            match (&mut self.first, &mut self.second) {
                (Some(first), Some(second)) => {
                    tokio::select! {
                        result = first.recv() => match result {
                            Ok(event) => return Some(event),
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => self.first = None,
                        },
                        result = second.recv() => match result {
                            Ok(event) => return Some(event),
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => self.second = None,
                        },
                    }
                }
                (Some(first), None) => match first.recv().await {
                    Ok(event) => return Some(event),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => self.first = None,
                },
                (None, Some(second)) => match second.recv().await {
                    Ok(event) => return Some(event),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => self.second = None,
                },
                (None, None) => return None,
            }
        }
    }
}

impl SessionWatcher {
    fn subscribe(&self) -> SessionWatcherReceivers {
        let mut receivers = self.handles.iter().map(|lease| lease.handle.subscribe());
        SessionWatcherReceivers {
            first: receivers.next(),
            second: receivers.next(),
        }
    }

    async fn shutdown(self) -> Result<(), String> {
        for lease in self.handles {
            if SharedWatcherRegistry::global().release(&lease.key, &lease.handle) {
                lease
                    .handle
                    .shutdown()
                    .await
                    .map_err(|error| error.to_string())?;
            }
        }
        Ok(())
    }
}

struct SharedWatcherEntry {
    watcher: Weak<FsArtifactWatcher>,
    leases: usize,
}

#[derive(Default)]
struct SharedWatcherRegistry {
    entries: Mutex<HashMap<WatcherKey, SharedWatcherEntry>>,
}

impl SharedWatcherRegistry {
    fn global() -> &'static Self {
        static WATCHERS: OnceLock<SharedWatcherRegistry> = OnceLock::new();
        WATCHERS.get_or_init(Self::default)
    }

    fn acquire(
        &self,
        key: WatcherKey,
        targets: Vec<WatchTarget>,
    ) -> Result<Arc<FsArtifactWatcher>, String> {
        let mut entries = match self.entries.lock() {
            Ok(entries) => entries,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(entry) = entries.get_mut(&key)
            && let Some(existing) = entry.watcher.upgrade()
        {
            entry.leases = entry.leases.saturating_add(1);
            return Ok(existing);
        }

        let spawned = FsArtifactWatcher::spawn(targets).map_err(|error| error.to_string())?;
        entries.insert(
            key,
            SharedWatcherEntry {
                watcher: Arc::downgrade(&spawned),
                leases: 1,
            },
        );
        drop(entries);
        Ok(spawned)
    }

    fn release(&self, key: &WatcherKey, handle: &Arc<FsArtifactWatcher>) -> bool {
        let mut entries = match self.entries.lock() {
            Ok(entries) => entries,
            Err(poisoned) => poisoned.into_inner(),
        };
        let Some(entry) = entries.get_mut(key) else {
            return false;
        };
        if !entry.watcher.ptr_eq(&Arc::downgrade(handle)) || entry.leases == 0 {
            return false;
        }
        entry.leases -= 1;
        if entry.leases != 0 {
            return false;
        }
        entries.remove(key);
        true
    }
}

const IDLE_TICK_INTERVAL: Duration = Duration::from_secs(1);

pub(crate) struct AgentIntelHandle {
    pub(crate) terminal_tx: mpsc::Sender<bytes::Bytes>,
    pub(crate) cancellation: CancellationToken,
    pub(crate) join_handle: tokio::task::JoinHandle<()>,
}

#[expect(
    clippy::too_many_arguments,
    reason = "the task is bound to one session incarnation and its shared permission authorities"
)]
pub(crate) fn try_spawn(
    session_id: SessionId,
    local_incarnation_id: uuid::Uuid,
    generation: uuid::Uuid,
    detected_agent: &str,
    cwd: Option<&str>,
    session_events_tx: mpsc::Sender<RuntimeSessionEvent>,
    parent_cancellation: &CancellationToken,
    permission_decisions: super::permission_decision_registry::PermissionDecisionRegistry,
    permission_timeout: Duration,
) -> Option<AgentIntelHandle> {
    let cwd = normalize_workspace_root(cwd?)
        .to_string_lossy()
        .into_owned();
    let kind = AgentKind::from_banner(detected_agent).or(match detected_agent {
        "claude" => Some(AgentKind::Claude),
        "copilot" => Some(AgentKind::Copilot),
        _ => None,
    })?;
    let detected_agent = detected_agent.to_owned();
    let agent_label = detected_agent.clone();

    let cancellation = parent_cancellation.child_token();

    let (terminal_tx, terminal_rx) = mpsc::channel(TERMINAL_CHANNEL_CAPACITY);
    let cancel = cancellation.clone();
    let join_handle = if kind == AgentKind::Claude {
        tokio::spawn(run_claude_agents_task(
            session_id,
            local_incarnation_id,
            generation,
            cwd,
            terminal_rx,
            session_events_tx,
            cancel,
        ))
    } else {
        let watcher = spawn_watcher_for(kind, &detected_agent, &cwd);
        tokio::spawn(async move {
            let provider_agent = detected_agent.clone();
            let provider_cwd = cwd.clone();
            let provider_session_id =
                agent_intel::domain::ids::SessionId::new(session_id.to_string());
            let provider = match io_pool::spawn_io(move || {
                create_live_provider(&provider_agent, &provider_cwd, &provider_session_id)
            })
            .await
            {
                Ok(Some(provider)) => provider,
                Ok(None) => return,
                Err(error) => {
                    tracing::warn!(session_id = %session_id, %error, "agent intel provider construction failed");
                    return;
                }
            };
            run_agent_intel_task(
                session_id,
                local_incarnation_id,
                generation,
                provider,
                terminal_rx,
                watcher,
                session_events_tx,
                cancel,
                permission_decisions,
                permission_timeout,
            )
            .await;
        })
    };

    tracing::info!(
        session_id = %session_id,
        agent = %agent_label,
        "spawned agent intelligence task"
    );

    Some(AgentIntelHandle {
        terminal_tx,
        cancellation,
        join_handle,
    })
}

#[expect(
    clippy::too_many_lines,
    reason = "one select loop owns command, telemetry, channel, and cancellation ordering"
)]
async fn run_claude_agents_task(
    session_id: SessionId,
    local_incarnation_id: uuid::Uuid,
    generation: uuid::Uuid,
    cwd: String,
    mut terminal_rx: mpsc::Receiver<bytes::Bytes>,
    session_events_tx: mpsc::Sender<RuntimeSessionEvent>,
    cancellation: CancellationToken,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut telemetry_rx = super::telemetry::subscribe();
    let mut adapter_rx = super::telemetry::subscribe_extensions();
    let mut telemetry = super::telemetry::ClaudeTelemetryState::default();
    let mut command_snapshot = None;
    let mut last_published = None;
    let mut turn_open = false;
    let mut last_turn_state = None;
    publish_room_delivery_state(
        session_id,
        local_incarnation_id,
        crate::host_protocol::RoomAgentDeliveryState::WaitingForAdapter,
        None,
        Some(
            "Claude Channel is available only when explicitly enabled and admitted by policy"
                .to_owned(),
        ),
        &session_events_tx,
    )
    .await;
    loop {
        tokio::select! {
            biased;
            () = cancellation.cancelled() => break,
            value = terminal_rx.recv() => {
                let Some(value) = value else {
                    break;
                };
                if !value.is_empty() {
                    turn_open = true;
                    publish_agent_turn_state_if_changed(
                        session_id,
                        local_incarnation_id,
                        crate::session_runtime::events::AgentTurnState::Running,
                        &mut last_turn_state,
                        &session_events_tx,
                    )
                    .await;
                }
            }
            _ = interval.tick() => {
                let snapshot = super::claude_agents::query(&cwd).await;
                let observed_turn_state = if matches!(
                    snapshot.lifecycle,
                    agent_intel::domain::AgentLifecycle::Starting
                        | agent_intel::domain::AgentLifecycle::Working
                        | agent_intel::domain::AgentLifecycle::Waiting
                ) {
                    turn_open = true;
                    Some(crate::session_runtime::events::AgentTurnState::Running)
                } else if snapshot.lifecycle == agent_intel::domain::AgentLifecycle::Idle {
                    turn_open = false;
                    Some(crate::session_runtime::events::AgentTurnState::Idle)
                } else {
                    None
                };
                if let Some(state) = observed_turn_state {
                    publish_agent_turn_state_if_changed(
                        session_id,
                        local_incarnation_id,
                        state,
                        &mut last_turn_state,
                        &session_events_tx,
                    )
                    .await;
                }
                command_snapshot = Some(snapshot.clone());
                let mut snapshot = snapshot;
                telemetry.overlay(&mut snapshot);
                if last_published.as_ref() == Some(&snapshot) {
                    continue;
                }
                if !publish_snapshot(
                    &session_id,
                    local_incarnation_id,
                    generation,
                    &snapshot,
                    &session_events_tx,
                    &cancellation,
                )
                .await
                {
                    break;
                }
                last_published = Some(snapshot);
            }
            event = async {
                match telemetry_rx.as_mut() {
                    Some(receiver) => Some(receiver.recv().await),
                    None => std::future::pending().await,
                }
            } => {
                match event {
                    Some(Ok(observation))
                        if observation.belongs_to(session_id, local_incarnation_id) =>
                    {
                        if observation.opens_turn() {
                            turn_open = true;
                            publish_agent_turn_state_if_changed(
                                session_id,
                                local_incarnation_id,
                                crate::session_runtime::events::AgentTurnState::Running,
                                &mut last_turn_state,
                                &session_events_tx,
                            )
                            .await;
                        }
                        if let Some(boundary) = observation.boundary() {
                            match boundary.kind {
                                super::telemetry::ObservedBoundaryKind::Tool => {
                                    publish_agent_boundary(
                                        session_id,
                                        local_incarnation_id,
                                        boundary,
                                        &session_events_tx,
                                    )
                                    .await;
                                }
                                super::telemetry::ObservedBoundaryKind::Turn if turn_open => {
                                    publish_agent_turn_state_if_changed(
                                        session_id,
                                        local_incarnation_id,
                                        crate::session_runtime::events::AgentTurnState::Idle,
                                        &mut last_turn_state,
                                        &session_events_tx,
                                    )
                                    .await;
                                    turn_open = false;
                                }
                                super::telemetry::ObservedBoundaryKind::Turn => {}
                            }
                        }
                        telemetry.apply(&observation);
                        let Some(mut snapshot) = command_snapshot.clone() else {
                            continue;
                        };
                        telemetry.overlay(&mut snapshot);
                        if last_published.as_ref() == Some(&snapshot) {
                            continue;
                        }
                        if !publish_snapshot(
                            &session_id,
                            local_incarnation_id,
                            generation,
                            &snapshot,
                            &session_events_tx,
                            &cancellation,
                        )
                        .await
                        {
                            break;
                        }
                        last_published = Some(snapshot);
                    }
                    Some(Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) | None => {}
                    Some(Err(tokio::sync::broadcast::error::RecvError::Closed)) => {
                        telemetry_rx = None;
                    }
                }
            }
            event = async {
                match adapter_rx.as_mut() {
                    Some(receiver) => Some(receiver.recv().await),
                    None => std::future::pending().await,
                }
            } => {
                match event {
                    Some(Ok(observation))
                        if observation.belongs_to(session_id, local_incarnation_id) =>
                    {
                        if let Some(boundary) = observation.boundary() {
                            match boundary.kind {
                                super::telemetry::ObservedBoundaryKind::Tool => {
                                    publish_agent_boundary(
                                        session_id,
                                        local_incarnation_id,
                                        boundary,
                                        &session_events_tx,
                                    )
                                    .await;
                                }
                                super::telemetry::ObservedBoundaryKind::Turn => {
                                    publish_agent_turn_state_if_changed(
                                        session_id,
                                        local_incarnation_id,
                                        crate::session_runtime::events::AgentTurnState::Idle,
                                        &mut last_turn_state,
                                        &session_events_tx,
                                    )
                                    .await;
                                    turn_open = false;
                                }
                            }
                        }
                        if let Some(transition) =
                            crate::rooms::delivery::transition_from_extension(&observation)
                        {
                            publish_room_delivery_state(
                                session_id,
                                local_incarnation_id,
                                transition.state,
                                transition.event_id,
                                transition.detail,
                                &session_events_tx,
                            )
                            .await;
                        }
                    }
                    Some(Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) | None => {}
                    Some(Err(tokio::sync::broadcast::error::RecvError::Closed)) => {
                        adapter_rx = None;
                    }
                }
            }
        }
    }
}

#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    reason = "per-session task fans inputs from disjoint channels in a single biased select!; splitting would either obscure cancellation safety (cross-arm break) or require a holder struct that only papered over the same wiring"
)]
async fn run_agent_intel_task(
    session_id: SessionId,
    local_incarnation_id: uuid::Uuid,
    generation: uuid::Uuid,
    provider: Arc<dyn LiveAgentProvider>,
    mut terminal_rx: mpsc::Receiver<bytes::Bytes>,
    watcher: Option<SessionWatcher>,
    session_events_tx: mpsc::Sender<RuntimeSessionEvent>,
    cancellation: CancellationToken,
    permission_decisions: super::permission_decision_registry::PermissionDecisionRegistry,
    permission_timeout: Duration,
) {
    let mut tick_rx = provider.ticks();
    let mut extension_rx = super::telemetry::subscribe_extensions();
    let mut extension_state = super::copilot_extension::CopilotExtensionState::default();
    let mut permission_bridge =
        super::telemetry::register_permission_bridge(session_id, local_incarnation_id);
    let mut last_turn_state = None;

    let mut watcher_rx = watcher.as_ref().map(SessionWatcher::subscribe);

    let mut idle_interval = tokio::time::interval(IDLE_TICK_INTERVAL);
    idle_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    idle_interval.tick().await;

    provider.settle_deferred_io().await;
    publish_current(
        &session_id,
        local_incarnation_id,
        generation,
        &provider,
        &extension_state,
        &session_events_tx,
        &cancellation,
    )
    .await;
    publish_room_delivery_state(
        session_id,
        local_incarnation_id,
        crate::host_protocol::RoomAgentDeliveryState::WaitingForAdapter,
        None,
        Some("Waiting for the session-scoped Copilot extension".to_owned()),
        &session_events_tx,
    )
    .await;

    loop {
        tokio::select! {
            () = cancellation.cancelled() => {
                tracing::debug!(session_id = %session_id, "agent intel task cancelled");
                break;
            }

            Some(bytes) = terminal_rx.recv() => {
                provider.feed_terminal_output(&bytes);
            }

            _ = idle_interval.tick() => {
                provider.tick_idle();
                provider.settle_deferred_io().await;
            }

            Some(watch_event) = async {
                match watcher_rx.as_mut() {
                    Some(rx) => rx.recv().await,
                    None => std::future::pending().await,
                }
            } => {
                provider.notify_artifact_changed(&watch_event.path);
                provider.tick_idle();
                provider.settle_deferred_io().await;
            }

            tick = tick_rx.recv() => {
                match tick {
                    Ok(()) => {
                        if !publish_current(
                            &session_id,
                            local_incarnation_id,
                            generation,
                            &provider,
                            &extension_state,
                            &session_events_tx,
                            &cancellation,
                        )
                        .await
                        {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::debug!(
                            session_id = %session_id,
                            lagged = n,
                            "snapshot bus lagged; coalescing"
                        );
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        tracing::debug!(session_id = %session_id, "snapshot bus closed");
                        break;
                    }
                }
            }

            event = async {
                match extension_rx.as_mut() {
                    Some(receiver) => Some(receiver.recv().await),
                    None => std::future::pending().await,
                }
            } => {
                match event {
                    Some(Ok(observation))
                        if observation.belongs_to(session_id, local_incarnation_id) =>
                    {
                        let observed_turn_state = match observation.event_type.as_str() {
                            "user.message"
                            | "assistant.turn_start"
                            | "tool.execution_start" => {
                                Some(crate::session_runtime::events::AgentTurnState::Running)
                            }
                            "session.idle" => {
                                Some(crate::session_runtime::events::AgentTurnState::Idle)
                            }
                            _ => None,
                        };
                        if let Some(state) = observed_turn_state {
                            publish_agent_turn_state_if_changed(
                                session_id,
                                local_incarnation_id,
                                state,
                                &mut last_turn_state,
                                &session_events_tx,
                            )
                            .await;
                        }
                        if let Some(boundary) = observation.boundary() {
                            publish_agent_boundary(
                                session_id,
                                local_incarnation_id,
                                boundary,
                                &session_events_tx,
                            )
                            .await;
                        }
                        if let Some(transition) =
                            crate::rooms::delivery::transition_from_extension(&observation)
                        {
                            publish_room_delivery_state(
                                session_id,
                                local_incarnation_id,
                                transition.state,
                                transition.event_id,
                                transition.detail,
                                &session_events_tx,
                            )
                            .await;
                        }
                        extension_state.apply(&observation);
                        if !publish_current(
                            &session_id,
                            local_incarnation_id,
                            generation,
                            &provider,
                            &extension_state,
                            &session_events_tx,
                            &cancellation,
                        )
                        .await
                        {
                            break;
                        }
                    }
                    Some(Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) | None => {}
                    Some(Err(tokio::sync::broadcast::error::RecvError::Closed)) => {
                        extension_rx = None;
                    }
                }
            }

            request = async {
                match permission_bridge.as_mut() {
                    Some(bridge) => bridge.recv().await,
                    None => std::future::pending().await,
                }
            } => {
                let Some(request) = request else {
                    permission_bridge = None;
                    continue;
                };
                if request.local_incarnation_id != local_incarnation_id {
                    drop(
                        request
                            .reply
                            .send(super::telemetry::CopilotPermissionDecision::NoResult),
                    );
                    continue;
                }
                tokio::spawn(super::copilot_extension::handle_permission_request(
                    request,
                    session_id,
                    local_incarnation_id,
                    permission_decisions.clone(),
                    permission_timeout,
                    session_events_tx.clone(),
                    cancellation.clone(),
                ));
            }
        }
    }

    if let Some(handle) = watcher
        && let Err(err) = handle.shutdown().await
    {
        tracing::debug!(
            session_id = %session_id,
            %err,
            "artifact watcher shutdown errored; continuing"
        );
    }
}

async fn publish_room_delivery_state(
    session_id: SessionId,
    local_incarnation_id: uuid::Uuid,
    state: crate::host_protocol::RoomAgentDeliveryState,
    event_id: Option<String>,
    detail: Option<String>,
    session_events_tx: &mpsc::Sender<RuntimeSessionEvent>,
) {
    drop(
        tokio::time::timeout(
            Duration::from_secs(1),
            session_events_tx.send(RuntimeSessionEvent::RoomAgentDeliveryState {
                id: session_id,
                local_incarnation_id,
                state,
                event_id,
                detail,
            }),
        )
        .await,
    );
}

async fn publish_agent_boundary(
    session_id: SessionId,
    local_incarnation_id: uuid::Uuid,
    boundary: super::telemetry::ObservedBoundary,
    session_events_tx: &mpsc::Sender<RuntimeSessionEvent>,
) {
    let super::telemetry::ObservedBoundaryKind::Tool = boundary.kind else {
        return;
    };
    drop(
        tokio::time::timeout(
            Duration::from_secs(1),
            session_events_tx.send(RuntimeSessionEvent::AgentBoundary {
                id: session_id,
                local_incarnation_id,
                kind: crate::session_runtime::events::AgentBoundaryKind::Tool,
                tool_use_id: boundary.tool_use_id,
            }),
        )
        .await,
    );
}

async fn publish_agent_turn_state_if_changed(
    session_id: SessionId,
    local_incarnation_id: uuid::Uuid,
    state: crate::session_runtime::events::AgentTurnState,
    last_state: &mut Option<crate::session_runtime::events::AgentTurnState>,
    session_events_tx: &mpsc::Sender<RuntimeSessionEvent>,
) {
    if *last_state == Some(state) {
        return;
    }
    if matches!(
        tokio::time::timeout(
            Duration::from_secs(1),
            session_events_tx.send(RuntimeSessionEvent::AgentTurnStateChanged {
                id: session_id,
                local_incarnation_id,
                state,
            }),
        )
        .await,
        Ok(Ok(()))
    ) {
        *last_state = Some(state);
    }
}

async fn publish_current(
    session_id: &SessionId,
    local_incarnation_id: uuid::Uuid,
    generation: uuid::Uuid,
    provider: &Arc<dyn LiveAgentProvider>,
    extension_state: &super::copilot_extension::CopilotExtensionState,
    session_events_tx: &mpsc::Sender<RuntimeSessionEvent>,
    cancellation: &CancellationToken,
) -> bool {
    let provider_state = provider.current_state();
    let mut snapshot =
        agent_intel::AgentIntelSnapshot::from_provider_state(provider_state.as_ref());
    extension_state.overlay(&mut snapshot);

    publish_snapshot(
        session_id,
        local_incarnation_id,
        generation,
        &snapshot,
        session_events_tx,
        cancellation,
    )
    .await
}

async fn publish_snapshot(
    session_id: &SessionId,
    local_incarnation_id: uuid::Uuid,
    generation: uuid::Uuid,
    snapshot: &agent_intel::AgentIntelSnapshot,
    session_events_tx: &mpsc::Sender<RuntimeSessionEvent>,
    cancellation: &CancellationToken,
) -> bool {
    let Ok(payload) = serde_json::to_value(snapshot) else {
        return true;
    };

    let permit = tokio::select! {
        biased;
        () = cancellation.cancelled() => return false,
        permit = session_events_tx.reserve() => permit,
    };

    permit.map_or_else(
        |_| {
            tracing::debug!(
                session_id = %session_id,
                "session events channel closed, stopping agent intel"
            );
            false
        },
        |permit| {
            permit.send(RuntimeSessionEvent::AgentIntelSnapshot {
                id: *session_id,
                local_incarnation_id,
                generation,
                payload,
            });
            true
        },
    )
}

fn spawn_watcher_for(kind: AgentKind, detected_agent: &str, cwd: &str) -> Option<SessionWatcher> {
    let workspace_root = normalize_workspace_root(cwd);
    let mut handles = Vec::with_capacity(2);

    let global_targets = global_watch_targets(kind);
    if !global_targets.is_empty() {
        let key = WatcherKey {
            agent: kind,
            workspace: None,
        };
        match SharedWatcherRegistry::global().acquire(key.clone(), global_targets) {
            Ok(handle) => handles.push(WatcherLease { key, handle }),
            Err(error) => tracing::warn!(
                agent = detected_agent,
                %error,
                "agent intel global watcher failed to start"
            ),
        }
    }

    let workspace_targets = workspace_watch_targets(kind, &workspace_root);
    if !workspace_targets.is_empty() {
        let key = WatcherKey {
            agent: kind,
            workspace: Some(workspace_root.clone()),
        };
        match SharedWatcherRegistry::global().acquire(key.clone(), workspace_targets) {
            Ok(handle) => handles.push(WatcherLease { key, handle }),
            Err(error) => tracing::warn!(
                agent = detected_agent,
                workspace = %workspace_root.display(),
                %error,
                "agent intel workspace watcher failed to start"
            ),
        }
    }

    (!handles.is_empty()).then_some(SessionWatcher { handles })
}

#[cfg(test)]
mod watcher_tests {
    use super::{SharedWatcherRegistry, WatcherKey};
    use agent_intel::{AgentKind, ArtifactWatcher, WatchTarget};
    use std::{path::PathBuf, sync::Arc, time::Duration};

    fn target(path: PathBuf) -> Vec<WatchTarget> {
        vec![WatchTarget {
            path,
            recursive: true,
        }]
    }

    #[tokio::test]
    async fn same_key_shares_watcher_and_different_workspace_isolated() {
        let first_dir = tempfile::tempdir().expect("first watcher directory");
        let second_dir = tempfile::tempdir().expect("second watcher directory");
        let first_path = std::fs::canonicalize(first_dir.path()).expect("first canonical path");
        let second_path = std::fs::canonicalize(second_dir.path()).expect("second canonical path");
        let registry = SharedWatcherRegistry::default();
        let first_key = WatcherKey {
            agent: AgentKind::Copilot,
            workspace: Some(first_path.clone()),
        };
        let second_key = WatcherKey {
            agent: AgentKind::Copilot,
            workspace: Some(second_path.clone()),
        };

        let first = registry
            .acquire(first_key.clone(), target(first_path.clone()))
            .expect("first watcher");
        let same = registry
            .acquire(first_key.clone(), target(first_path.clone()))
            .expect("shared watcher");
        let different = registry
            .acquire(second_key.clone(), target(second_path))
            .expect("isolated watcher");
        assert!(Arc::ptr_eq(&first, &same));
        assert!(!Arc::ptr_eq(&first, &different));
        assert!(!registry.release(&first_key, &first));
        assert!(registry.release(&first_key, &same));
        assert!(!registry.release(&first_key, &same));
        assert!(registry.release(&second_key, &different));

        first.shutdown().await.expect("first shutdown");
        different.shutdown().await.expect("second shutdown");
    }

    #[tokio::test]
    async fn every_lease_receives_events_from_shared_watcher() {
        let directory = tempfile::tempdir().expect("watcher directory");
        let path = std::fs::canonicalize(directory.path()).expect("canonical path");
        let registry = SharedWatcherRegistry::default();
        let key = WatcherKey {
            agent: AgentKind::Copilot,
            workspace: None,
        };
        let first = registry
            .acquire(key.clone(), target(path.clone()))
            .expect("first watcher");
        let second = registry
            .acquire(key.clone(), target(path.clone()))
            .expect("shared watcher");
        let mut first_rx = first.subscribe();
        let mut second_rx = second.subscribe();
        tokio::time::sleep(Duration::from_millis(100)).await;
        let changed = path.join("session-state/owner/events.jsonl");
        std::fs::create_dir_all(changed.parent().expect("event parent")).expect("event directory");
        std::fs::write(&changed, "{}\n").expect("event fixture");

        let first_event = tokio::time::timeout(Duration::from_secs(3), first_rx.recv())
            .await
            .expect("first event timeout")
            .expect("first event");
        let second_event = tokio::time::timeout(Duration::from_secs(3), second_rx.recv())
            .await
            .expect("second event timeout")
            .expect("second event");
        assert!(first_event.path.starts_with(&path));
        assert!(second_event.path.starts_with(&path));
        assert!(!registry.release(&key, &first));
        assert!(registry.release(&key, &second));
        first.shutdown().await.expect("shared shutdown");
    }
}
