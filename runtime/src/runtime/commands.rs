use super::Reconnect;
use bytes::Bytes;
#[cfg(feature = "host")]
use serde_json::Value;
use serde_json::json;
#[cfg(feature = "host")]
use std::collections::BTreeSet;
#[cfg(feature = "host")]
use std::path::{Path, PathBuf};
use std::time::Duration;
#[cfg(feature = "host")]
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{Completion, Job, MAX_SESSIONS, Opening, Runtime, Scope};
#[cfg(feature = "host")]
use super::{Local, StartedSession};
use crate::{
    Command, Error, Result,
    network::{BackendReply, INPUT_NOT_SENT, RemoteConnection},
    protocol::{ConnectionState, SessionStatus, parse_id},
    terminal::RemoteTerminal,
};
#[cfg(feature = "host")]
use crate::{
    Config,
    agents::{AgentStart, Launcher},
    protocol::{SessionEntry, SessionKind},
    provider::{self, ConversationIdentity},
    terminal::{SessionChange, TerminalSession},
};
#[cfg(feature = "host")]
use std::sync::Arc;

const RUN_ON_HOST: &str = "Start a program on the computer that hosts this terminal.";
#[cfg(not(feature = "host"))]
const NO_DOSI: &str = "Dosi runs on the computer that hosts your terminals.";
const SHARE_ON_HOST: &str = "Change sharing on the computer hosting this session.";

impl Runtime {
    pub(super) fn apply_for_consumer(
        &mut self,
        command: &Command,
        consumer: Uuid,
        lifetime: CancellationToken,
    ) -> Result<()> {
        match command {
            Command::OpenRemote { session_id, .. } => {
                let id = parse_id(session_id)?;
                if self.views.len() >= MAX_SESSIONS && !self.views.contains_key(&id) {
                    return Err(Error::Busy);
                }
                let consumers = self.views.entry(id).or_default();
                consumers.retain(|_, token| !token.is_cancelled());
                if consumers.len() >= 64 && !consumers.contains_key(&consumer) {
                    return Err(Error::Busy);
                }
                consumers.insert(consumer, lifetime);
                if let Err(error) = self.open_remote(id, command.clone()) {
                    self.release_view(id, consumer);
                    return Err(error);
                }
                Ok(())
            }
            Command::DisconnectRemote { session_id, .. } => {
                self.release_view(parse_id(session_id)?, consumer);
                self.result(command);
                Ok(())
            }
            Command::Visible { session_ids, .. } => {
                self.see(consumer, lifetime, session_ids.iter().cloned().collect());
                self.result(command);
                Ok(())
            }
            _ => self.apply(command),
        }
    }

    fn release_view(&mut self, id: Uuid, consumer: Uuid) {
        if let Some(consumers) = self.views.get_mut(&id) {
            consumers.remove(&consumer);
        }
        self.prune_views();
    }

    pub(super) fn prune_views(&mut self) {
        let expired = self
            .views
            .iter_mut()
            .filter_map(|(id, consumers)| {
                consumers.retain(|_, token| !token.is_cancelled());
                consumers.is_empty().then_some(*id)
            })
            .collect::<Vec<_>>();
        for id in expired {
            self.disconnect_remote(id);
        }
    }

    #[expect(clippy::too_many_lines, reason = "one dispatch for every command")]
    pub(super) fn apply(&mut self, command: &Command) -> Result<()> {
        match command {
            Command::ListSessions { .. } => {
                self.publish_catalog();
                if self.scope.user.is_some() {
                    self.job_capacity()?;
                    self.network_command(command.clone());
                } else {
                    self.result(command);
                }
            }
            #[cfg(feature = "host")]
            Command::CreateSession { .. } => self.create(command.clone())?,
            #[cfg(not(feature = "host"))]
            Command::CreateSession { .. } => {
                return Err(Error::Invalid(
                    "This device cannot start terminals.".to_owned(),
                ));
            }
            Command::RenameSession { .. } => self.rename(command)?,
            Command::Input { .. } => self.send_input(command)?,
            Command::Read { .. } => self.read(command)?,
            Command::Show { session_id, .. } => {
                self.target(parse_id(session_id)?)?;
                self.emit(json!({"type":"session.show", "sessionId":session_id}));
                self.result(command);
            }
            Command::ShowMission { mission_id, .. } => {
                self.emit(json!({"type":"mission.show", "missionId":mission_id}));
                self.result(command);
            }
            #[cfg(feature = "host")]
            command if command.is_dosi() => self.dosi_command(command)?,
            #[cfg(not(feature = "host"))]
            command if command.is_dosi() => return Err(Error::Invalid(NO_DOSI.to_owned())),
            Command::SeeMission {
                mission_id,
                sequence,
                ..
            } => {
                self.see_room(mission_id, *sequence)?;
                self.result(command);
            }
            Command::CloseSession { .. } | Command::InterruptSession { .. } => {
                self.end_session(command)?;
            }
            #[cfg(feature = "host")]
            Command::RunInSession { .. } => self.run_in_session(command)?,
            #[cfg(not(feature = "host"))]
            Command::RunInSession { .. } => {
                self.command_session(command)?;
                return Err(Error::Invalid(RUN_ON_HOST.to_owned()));
            }
            Command::OpenRemote { .. }
            | Command::DisconnectRemote { .. }
            | Command::Visible { .. } => {
                return Err(Error::Invalid(
                    "remote view commands require a current client".to_owned(),
                ));
            }
            Command::ShareSession { .. }
            | Command::AttachSession { .. }
            | Command::LeaveSession { .. } => self.change_membership(command)?,
            #[cfg(feature = "host")]
            Command::DiscoverConversations { .. }
            | Command::ReadConversation { .. }
            | Command::InspectProvider { .. }
            | Command::InspectAgents { .. }
            | Command::InspectAgentModels { .. }
            | Command::AddAgentAccount { .. }
            | Command::ChooseAgent { .. } => self.provider(command)?,
            #[cfg(not(feature = "host"))]
            Command::DiscoverConversations { .. }
            | Command::ReadConversation { .. }
            | Command::InspectProvider { .. } => {
                return Err(Error::Invalid(
                    "Open agent conversations on the computer that runs the agent.".to_owned(),
                ));
            }
            #[cfg(not(feature = "host"))]
            Command::InspectAgents { .. }
            | Command::InspectAgentModels { .. }
            | Command::AddAgentAccount { .. }
            | Command::ChooseAgent { .. } => {
                return Err(Error::Invalid(
                    "This device cannot start terminals.".to_owned(),
                ));
            }
            Command::Resize { .. } | Command::Focus { .. } | Command::Blur { .. } => {
                return Err(Error::Invalid(
                    "terminal control requires a current subscription".to_owned(),
                ));
            }
            Command::Login { .. } | Command::Logout { .. } | Command::DeleteAccount { .. } => {
                self.job_capacity()?;
                if self.auth_pending {
                    return Err(Error::Busy);
                }
                self.auth_pending = true;
                self.network_command(command.clone());
            }
            _ => {
                self.job_capacity()?;
                self.network_command(command.clone());
            }
        }
        Ok(())
    }

    #[cfg(feature = "host")]
    fn provider(&mut self, command: &Command) -> Result<()> {
        self.job_capacity()?;
        let launcher = Arc::clone(&self.launcher);
        let command = command.clone();
        if let Command::ChooseAgent { change, .. } = &command {
            launcher.choose(change);
            self.spawn(async move {
                Arc::clone(&launcher).save().await;
                let result = serde_json::to_value(launcher.choice()).map_err(Error::from);
                Completion::Provider { command, result }
            });
        } else {
            let home = self.config.home.clone();
            let shell = shell(&self.config);
            self.spawn(async move {
                let result = provider_reply(&home, &shell, &launcher, &command).await;
                Completion::Provider { command, result }
            });
        }
        Ok(())
    }

    fn read(&mut self, command: &Command) -> Result<()> {
        let Command::Read {
            session_id,
            expected_runtime_incarnation_id,
            history,
            ..
        } = command
        else {
            return Err(Error::Invalid("expected a terminal read".to_owned()));
        };
        let id = parse_id(session_id)?;
        self.check_incarnation(id, parse_id(expected_runtime_incarnation_id)?)?;
        self.job_capacity()?;
        let read = self.target(id)?.read(*history);
        let command = command.clone();
        self.spawn(async move {
            Completion::Read {
                command,
                result: read.await,
            }
        });
        Ok(())
    }

    fn send_input(&mut self, command: &Command) -> Result<()> {
        let Command::Input {
            session_id,
            expected_runtime_incarnation_id,
            text,
            ..
        } = command
        else {
            return Err(Error::Invalid("expected terminal input".to_owned()));
        };
        let id = parse_id(session_id)?;
        self.check_incarnation(id, parse_id(expected_runtime_incarnation_id)?)?;
        self.job_capacity()?;
        let input = self
            .target(id)?
            .send_input(Bytes::from(text.clone().into_bytes()));
        let command = command.clone();
        self.spawn(async move {
            Completion::Terminal {
                command,
                result: input.await,
            }
        });
        Ok(())
    }

    fn command_session(&self, command: &Command) -> Result<Uuid> {
        let (Command::RenameSession {
            session_id: id,
            expected_runtime_incarnation_id: incarnation,
            ..
        }
        | Command::CloseSession {
            session_id: id,
            expected_runtime_incarnation_id: incarnation,
            ..
        }
        | Command::InterruptSession {
            session_id: id,
            expected_runtime_incarnation_id: incarnation,
            ..
        }
        | Command::RunInSession {
            session_id: id,
            expected_runtime_incarnation_id: incarnation,
            ..
        }
        | Command::ShareSession {
            session_id: id,
            expected_runtime_incarnation_id: incarnation,
            ..
        }
        | Command::AttachSession {
            session_id: id,
            expected_runtime_incarnation_id: incarnation,
            ..
        }
        | Command::LeaveSession {
            session_id: id,
            expected_runtime_incarnation_id: incarnation,
            ..
        }) = command
        else {
            return Err(Error::Invalid(
                "expected current session mutation".to_owned(),
            ));
        };
        let id = parse_id(id)?;
        self.check_incarnation(id, parse_id(incarnation)?)?;
        Ok(id)
    }

    fn rename(&mut self, command: &Command) -> Result<()> {
        let id = self.command_session(command)?;
        self.require_owner(id)?;
        #[cfg(feature = "host")]
        if self.local.get(&id).is_some_and(|local| !local.published)
            && !self.publishing.contains(&id)
        {
            if let Command::RenameSession { name, .. } = command {
                self.local
                    .get_mut(&id)
                    .ok_or(Error::NotFound)?
                    .entry
                    .name
                    .clone_from(name);
            }
            self.publish_catalog();
            self.result(command);
            return Ok(());
        }
        self.administer(id, command.clone())
    }

    #[cfg(feature = "host")]
    fn run_in_session(&mut self, command: &Command) -> Result<()> {
        let id = self.command_session(command)?;
        let Command::RunInSession {
            command: line,
            agent,
            ..
        } = command
        else {
            return Err(Error::Invalid("expected a command to run".to_owned()));
        };
        self.job_capacity()?;
        let terminal = self
            .local
            .get(&id)
            .ok_or_else(|| Error::Invalid(RUN_ON_HOST.to_owned()))?
            .terminal
            .clone();
        let launcher = Arc::clone(&self.launcher);
        let (line, agent) = (line.clone(), agent.clone());
        let command = command.clone();
        self.spawn(async move {
            let result = async {
                let line = match (line, agent) {
                    (Some(line), None) => line,
                    (None, Some(agent)) => launcher.line(&agent).await.map_err(Error::Invalid)?,
                    _ => {
                        return Err(Error::Invalid(
                            "Run either a command or an agent.".to_owned(),
                        ));
                    }
                };
                terminal.run(line).await
            }
            .await;
            Completion::Terminal { command, result }
        });
        Ok(())
    }

    fn end_session(&mut self, command: &Command) -> Result<()> {
        let id = self.command_session(command)?;
        if !self.hosts(&id)
            && self
                .connections
                .get(&id)
                .is_none_or(RemoteTerminal::is_closed)
        {
            return self.open_remote(id, command.clone());
        }
        self.job_capacity()?;
        let closing = matches!(command, Command::CloseSession { .. });
        let response = self.target(id)?.end(closing);
        if closing {
            #[cfg(feature = "host")]
            if let Some(local) = self.local.get_mut(&id) {
                local.entry.status = SessionStatus::Closing;
            }
            self.publish_catalog();
        }
        let command = command.clone();
        self.spawn(async move {
            Completion::Terminal {
                command,
                result: response.await,
            }
        });
        Ok(())
    }

    fn change_membership(&mut self, command: &Command) -> Result<()> {
        let id = self.command_session(command)?;
        match command {
            #[cfg(feature = "host")]
            Command::ShareSession {
                expected_user_ids, ..
            } => {
                if let Some(expected) = expected_user_ids
                    && self.local.get(&id).is_some_and(|local| {
                        expected.iter().collect::<BTreeSet<_>>()
                            != local.entry.shared_with.iter().collect::<BTreeSet<_>>()
                    })
                {
                    return Err(Error::Invalid(
                        "Session sharing changed. Reopen sharing before saving.".into(),
                    ));
                }
                if !self.local.contains_key(&id) {
                    return Err(Error::Invalid(SHARE_ON_HOST.to_owned()));
                }
                self.require_publication(id)?;
            }
            #[cfg(not(feature = "host"))]
            Command::ShareSession { .. } => {
                return Err(Error::Invalid(SHARE_ON_HOST.to_owned()));
            }
            Command::AttachSession { .. } => {
                self.require_owner(id)?;
                #[cfg(feature = "host")]
                self.require_publication(id)?;
            }
            Command::LeaveSession { .. }
                if self.hosts(&id) || self.remotes.get(&id).is_some_and(|entry| entry.is_owner) =>
            {
                return Err(Error::Invalid(
                    "The owner cannot leave their own session.".to_owned(),
                ));
            }
            _ => {}
        }
        self.administer(id, command.clone())
    }

    #[cfg(feature = "host")]
    fn require_publication(&mut self, id: Uuid) -> Result<()> {
        if self.local.get(&id).is_some_and(|local| !local.published) {
            self.publish_locals();
            return Err(Error::Invalid(super::NOT_PUBLISHED.to_owned()));
        }
        Ok(())
    }

    fn require_owner(&self, id: Uuid) -> Result<()> {
        if self.hosts(&id) || self.remotes.get(&id).is_some_and(|entry| entry.is_owner) {
            Ok(())
        } else {
            Err(Error::Invalid(
                "Only the owner can change this session.".to_owned(),
            ))
        }
    }

    #[cfg(feature = "host")]
    fn create(&mut self, command: Command) -> Result<()> {
        let Command::CreateSession {
            request_id, resume, ..
        } = &command
        else {
            return Err(Error::Invalid("expected session creation".to_owned()));
        };
        let existing = self.local.iter().find_map(|(id, local)| {
            (local.creation.request_id() == request_id).then_some((*id, &local.creation))
        });
        if let Some((id, existing)) = existing {
            if existing != &command {
                return Err(Error::Invalid(
                    "request ID was reused for another session".to_owned(),
                ));
            }
            self.publish_catalog();
            self.emit(json!({"type":"session.result", "requestId":request_id, "sessionId":id, "operation":"session.create"}));
            return Ok(());
        }
        if self.local.len() + self.creating.len() >= MAX_SESSIONS {
            return Err(Error::Busy);
        }
        self.job_capacity()?;
        if let Some(resume) = resume
            && self.local.values().map(|local| &local.creation).chain(self.creating.values())
                .any(|command| matches!(command, Command::CreateSession { resume: Some(active), .. } if active == resume)) {
                return Err(Error::Invalid("This provider conversation is already running in Kodosi.".to_owned()));
            }
        let id = Uuid::now_v7();
        let config = self.config.clone();
        let changes = self.changes.clone();
        let launcher = Arc::clone(&self.launcher);
        self.creating.insert(id, command.clone());
        self.spawn(async move {
            let result = create_session(id, config, &launcher, &command, changes).await;
            Completion::Created { id, result }
        });
        Ok(())
    }

    pub(super) fn open_remote(&mut self, id: Uuid, command: Command) -> Result<()> {
        if self
            .connections
            .get(&id)
            .is_some_and(RemoteTerminal::is_closed)
        {
            self.connections.remove(&id);
        }
        if self.hosts(&id)
            || self
                .connections
                .get(&id)
                .is_some_and(|connection| !connection.is_interrupted())
        {
            self.result(&command);
            return Ok(());
        }
        if let Some(opening) = self.opening.get_mut(&id) {
            if !opening.commands.iter().any(|waiting| waiting == &command) {
                if opening.commands.len() >= 32 {
                    return Err(Error::Busy);
                }
                opening.commands.push(command);
            }
            return Ok(());
        }
        self.job_capacity()?;
        let attempt = Uuid::now_v7();
        let cancellation = CancellationToken::new();
        self.opening.insert(
            id,
            Opening {
                attempt,
                cancellation: cancellation.clone(),
                commands: vec![command],
            },
        );
        let recovering = self.recovering(id);
        if let Some(entry) = self.remotes.get_mut(&id) {
            if let Ok(incarnation) = Uuid::parse_str(&entry.incarnation_id)
                && self.views.contains_key(&id)
            {
                self.reconnect.entry(id).or_insert_with(|| Reconnect {
                    incarnation,
                    failures: 0,
                    next: tokio::time::Instant::now() + Duration::from_secs(2),
                    connected: None,
                });
            }
            if !recovering {
                entry.connection_state = ConnectionState::Connecting;
            }
            entry.message.take_if(|message| message != INPUT_NOT_SENT);
        }
        self.publish_catalog();
        let network = self.network.clone();
        let generation = self.scope.network_generation;
        self.spawn(async move {
            let result = if network.generation() == generation {
                tokio::select! {
                    () = cancellation.cancelled() => Err(Error::Stale),
                    result = network.connect_remote(id) => result.map_err(Error::from),
                }
            } else {
                Err(Error::Stale)
            };
            Completion::Connected {
                id,
                attempt,
                result,
            }
        });
        Ok(())
    }

    fn disconnect_remote(&mut self, id: Uuid) {
        self.views.remove(&id);
        self.reconnect.remove(&id);
        if let Some(opening) = self.opening.remove(&id) {
            opening.cancellation.cancel();
        }
        if let Some(connection) = self.connections.remove(&id) {
            connection.disconnect();
        }
        if let Some(entry) = self.remotes.get_mut(&id) {
            entry.connection_state = ConnectionState::Offline;
            entry.connected_users.clear();
        }
        self.show_shared_status(id);
        self.publish_catalog();
    }

    fn administer(&mut self, id: Uuid, command: Command) -> Result<()> {
        self.job_capacity()?;
        let busy = self.administering.contains(&id);
        #[cfg(feature = "host")]
        let busy = busy || self.publishing.contains(&id);
        if busy {
            return Err(Error::Busy);
        }
        self.administering.insert(id);
        self.network_command(command);
        Ok(())
    }

    pub(super) fn network_command(&mut self, command: Command) {
        self.backend_command(command, true);
    }

    pub(super) fn refresh(&mut self, command: Command) {
        self.backend_command(command, false);
    }

    fn backend_command(&mut self, command: Command, requested: bool) {
        let network = self.network.clone();
        let generation = self.scope.network_generation;
        self.spawn(async move {
            let result = if network.generation() == generation {
                match serde_json::to_value(&command) {
                    Ok(args) => network
                        .execute(command.operation(), args)
                        .await
                        .map_err(Error::from),
                    Err(error) => Err(Error::from(error)),
                }
            } else {
                Err(Error::Stale)
            };
            Completion::BackendClient {
                command,
                requested,
                generation: network.generation(),
                result,
            }
        });
    }

    pub(super) fn terminal_command(
        &mut self,
        command: &Command,
        connection: Uuid,
        reply: oneshot::Sender<Result<()>>,
    ) {
        match self.terminal_response(command, connection) {
            Ok(response) => self.spawn(async move {
                Completion::HeadlessControl {
                    reply,
                    result: response.await,
                }
            }),
            Err(error) => drop(reply.send(Err(error))),
        }
    }

    fn terminal_response(
        &self,
        command: &Command,
        connection: Uuid,
    ) -> Result<std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send>>> {
        self.job_capacity()?;
        let (session_id, incarnation_id) = match &command {
            Command::Resize {
                session_id,
                identity,
                ..
            } => (session_id, &identity.expected_runtime_incarnation_id),
            Command::Focus {
                session_id,
                expected_runtime_incarnation_id,
                ..
            }
            | Command::Blur {
                session_id,
                expected_runtime_incarnation_id,
                ..
            } => (session_id, expected_runtime_incarnation_id),
            _ => return Err(Error::Invalid("expected terminal control".to_owned())),
        };
        let id = parse_id(session_id)?;
        self.check_incarnation(id, parse_id(incarnation_id)?)?;
        let target = self.target(id)?;
        match command {
            Command::Resize { identity, .. } => {
                let (size, geometry) = identity.geometry()?;
                Ok(target.resize(connection, size, Some(geometry), false))
            }
            Command::Focus { .. } | Command::Blur { .. } => {
                Ok(target.focus(connection, matches!(command, Command::Focus { .. })))
            }
            _ => Err(Error::Invalid("expected terminal control".to_owned())),
        }
    }

    #[expect(clippy::too_many_lines, reason = "one dispatch for every finished job")]
    pub(super) fn complete(&mut self, job: Job) {
        if self.synchronize_account().is_err() {
            return;
        }
        let current = self.in_scope(&job.scope);
        match job.completion {
            Completion::Input { result } => {
                if current && let Err(error) = result {
                    self.emit(json!({"type":"system.error", "message":format!("Terminal input was not confirmed and was not retried: {error}")}));
                }
            }
            Completion::Initialized(result) => {
                self.initializing = false;
                self.emit(self.auth_event());
                if job.scope.network_generation == self.network.generation()
                    && let Err(error) = result
                    && !matches!(error, Error::Stale)
                {
                    self.emit(json!({"type":"system.error", "message":error.to_string()}));
                }
                #[cfg(feature = "host")]
                self.publish_locals();
            }
            #[cfg(feature = "host")]
            Completion::Created { id, result } => self.created(id, result, current),
            Completion::BackendClient {
                command,
                requested,
                generation,
                result,
            } => self.network_completed(&job.scope, &command, requested, generation, result),
            Completion::Connected {
                id,
                attempt,
                result,
            } => self.connected(id, attempt, result, current),
            #[cfg(feature = "host")]
            Completion::Published {
                id,
                incarnation,
                result,
            } => self.published(id, incarnation, result, current),
            Completion::Unpublished { id, result } if current => {
                self.unpublishing.remove(&id);
                match result {
                    Ok(()) => {
                        #[cfg(feature = "host")]
                        self.retiring.remove(&id);
                    }
                    Err(error) => tracing::debug!(%id, %error, "session retirement deferred"),
                }
            }
            Completion::Terminal { command, result } if current => {
                match result {
                    Ok(()) => self.result(&command),
                    Err(error) => self.error(&command, &error),
                }
                if matches!(
                    command,
                    Command::CloseSession { .. } | Command::InterruptSession { .. }
                ) && let Some(id) = command.session_id().and_then(|id| Uuid::parse_str(id).ok())
                    && !self.hosts(&id)
                    && !self.views.contains_key(&id)
                {
                    self.disconnect_remote(id);
                }
            }
            #[cfg(feature = "host")]
            Completion::Provider { command, result } if current => match result {
                Ok(value) => {
                    self.learn_agents(&command, &value);
                    self.emit(json!({"type":"provider.result", "requestId":command.request_id(), "operation":command.operation(), "result":value}));
                }
                Err(error) => self.error(&command, &error),
            },
            Completion::Checkpoint { reply, result } => {
                drop(reply.send(if current { result } else { Err(Error::Stale) }));
            }
            Completion::HeadlessControl { reply, result } => {
                drop(reply.send(if current { result } else { Err(Error::Stale) }));
            }
            Completion::Subscribed {
                target,
                reply,
                result,
            } => match result {
                Ok(subscription) if current => {
                    if let Err(Ok(subscription)) = reply.send(Ok(subscription)) {
                        target.unsubscribe(subscription.connection_id);
                    }
                }
                Ok(subscription) => {
                    target.unsubscribe(subscription.connection_id);
                    drop(reply.send(Err(Error::Stale)));
                }
                Err(error) => {
                    drop(reply.send(Err(error)));
                }
            },
            Completion::Unpublished { .. } => {}
            Completion::Terminal { command, .. } => self.error(&command, &Error::Stale),
            Completion::Read { command, result } if current => match result {
                Ok(text) => {
                    let mut value = super::answer(&command, Ok(()));
                    value["text"] = json!(text);
                    self.emit(value);
                }
                Err(error) => self.error(&command, &error),
            },
            Completion::Read { command, .. } => self.error(&command, &Error::Stale),
            #[cfg(feature = "host")]
            Completion::Provider { command, .. } => self.error(&command, &Error::Stale),
        }
    }

    #[cfg(feature = "host")]
    fn created(&mut self, id: Uuid, result: Result<StartedSession>, current: bool) {
        let Some(command) = self.creating.remove(&id) else {
            return;
        };
        if !current {
            self.error(&command, &Error::Stale);
            return;
        }
        match result {
            Ok(mut started) => {
                let Some(terminal) = started.terminal.take() else {
                    return;
                };
                let Command::CreateSession {
                    request_id,
                    name,
                    mission_id,
                    ..
                } = &command
                else {
                    terminal.cancel();
                    return;
                };
                if terminal.is_closed() {
                    self.error(
                        &command,
                        &Error::Other(
                            "The process exited before its terminal was ready.".to_owned(),
                        ),
                    );
                    return;
                }
                let entry = SessionEntry {
                    attention: false,
                    connected_users: vec![],
                    program: None,
                    prompt: false,
                    repository: started.repository,
                    title: None,
                    program_status: None,
                    context: None,
                    id: id.to_string(),
                    incarnation_id: terminal.incarnation_id.to_string(),
                    kind: SessionKind::Local,
                    name: name.clone(),
                    working_dir: Some(started.directory.to_string_lossy().into_owned()),
                    owner_user_id: self.scope.user.clone(),
                    owner_name: None,
                    host_device_id: None,
                    host_name: None,
                    is_owner: true,
                    status: SessionStatus::Running,
                    connection_state: ConnectionState::Local,
                    message: None,
                    mission_id: mission_id.clone(),
                    mission_name: None,
                    shared_with: vec![],
                };
                let created = json!({"type":"session.result", "requestId":request_id, "sessionId":id, "operation":"session.create"});
                self.local.insert(
                    id,
                    Local {
                        terminal,
                        entry,
                        creation: command,
                        published: false,
                        room_sequence: 0,
                        worktree: started.worktree.take(),
                    },
                );
                self.publish_catalog();
                self.emit(created);
                self.publish_locals();
            }
            Err(error) => self.error(&command, &error),
        }
    }

    pub(super) fn network_completed(
        &mut self,
        scope: &Scope,
        command: &Command,
        requested: bool,
        generation: u64,
        result: Result<BackendReply>,
    ) {
        let changes_account = matches!(
            command,
            Command::Login { .. } | Command::Logout { .. } | Command::DeleteAccount { .. }
        );
        if changes_account {
            self.auth_pending = false;
        }
        let current = self.in_scope(scope);
        let own_retirement = changes_account
            && scope.network_generation.checked_add(1) == Some(generation)
            && generation == self.network.generation();
        if !current && !own_retirement {
            self.error(command, &Error::Stale);
            return;
        }
        if let Some(id) = command.session_id().and_then(|id| Uuid::parse_str(id).ok()) {
            self.administering.remove(&id);
        }
        match result {
            Ok(reply)
                if reply.generation == self.network.generation()
                    && reply.user_id
                        == self.network.identity().map(|identity| identity.user_id) =>
            {
                self.apply_metadata(command);
                let later = reply.answered_later;
                self.network_reply(reply);
                if !later {
                    self.finish(command);
                }
            }
            Ok(_) => self.error(command, &Error::Stale),
            Err(error) if requested => self.error(command, &error),
            Err(Error::Stale | Error::Stopped) => {}
            Err(error) => {
                tracing::warn!(operation = command.operation(), %error, "a refresh of the runtime failed");
            }
        }
    }

    fn connected(
        &mut self,
        id: Uuid,
        attempt: Uuid,
        result: Result<RemoteConnection>,
        current: bool,
    ) {
        if !current
            || self
                .opening
                .get(&id)
                .is_none_or(|opening| opening.attempt != attempt)
        {
            return;
        }
        let Some(opening) = self.opening.remove(&id) else {
            return;
        };
        match result {
            Ok(connection) => {
                if opening.commands.is_empty()
                    && self
                        .reconnect
                        .get(&id)
                        .is_none_or(|retry| retry.incarnation != connection.session.incarnation_id)
                {
                    connection.disconnect();
                    self.reconnect.remove(&id);
                    return;
                }
                if self.views.contains_key(&id) {
                    let failures = self.reconnect.get(&id).map_or(0, |retry| retry.failures);
                    self.reconnect.insert(
                        id,
                        Reconnect {
                            incarnation: connection.session.incarnation_id,
                            failures,
                            next: tokio::time::Instant::now() + Duration::from_secs(2),
                            connected: Some(tokio::time::Instant::now()),
                        },
                    );
                }
                let notice = self
                    .remotes
                    .get(&id)
                    .and_then(|entry| entry.message.clone())
                    .filter(|message| message == INPUT_NOT_SENT);
                let mut entry = self.remote_entry(self.listed_remote(&connection.session));
                entry.connection_state = ConnectionState::Connected;
                entry.status = SessionStatus::Running;
                entry.message = notice;
                let resumed = self.connections.get(&id).is_some_and(|existing| {
                    existing.is_interrupted()
                        && existing.incarnation_id == connection.session.incarnation_id
                });
                if resumed {
                    if let Some(existing) = self.connections.get(&id) {
                        existing.resume(connection);
                    }
                } else {
                    self.connections
                        .insert(id, RemoteTerminal::spawn(connection, self.changes.clone()));
                }
                self.remotes.insert(id, entry);
                self.publish_catalog();
                for command in opening.commands {
                    if matches!(command, Command::OpenRemote { .. }) {
                        self.result(&command);
                    } else if let Err(error) = self.apply(&command) {
                        self.error(&command, &error);
                    }
                }
            }
            Err(error) => {
                tracing::warn!(%id, %error, "remote terminal connection failed");
                if matches!(error, Error::Invalid(_)) {
                    self.reconnect.remove(&id);
                    if let Some(connection) = self.connections.remove(&id) {
                        connection.disconnect();
                    }
                }
                if let Some(retry) = self.reconnect.get_mut(&id) {
                    retry.failures = retry.failures.saturating_add(1);
                    retry.next = tokio::time::Instant::now() + Reconnect::delay(retry.failures);
                    retry.connected = None;
                }
                let recovering = self.reconnect.contains_key(&id) && self.recovering(id);
                if let Some(entry) = self.remotes.get_mut(&id) {
                    if recovering {
                        entry.status = SessionStatus::Reconnecting;
                    } else {
                        entry.connection_state = if matches!(error, Error::Invalid(_)) {
                            ConnectionState::Blocked
                        } else {
                            ConnectionState::Offline
                        };
                        entry.message = Some(error.to_string());
                    }
                }
                self.publish_catalog();
                for command in opening.commands {
                    self.error(&command, &error);
                }
            }
        }
    }

    #[cfg(feature = "host")]
    fn published(&mut self, id: Uuid, incarnation: Uuid, result: Result<()>, current: bool) {
        if !current {
            return;
        }
        self.publishing.remove(&id);
        match result {
            Ok(()) => {
                if let Some(local) = self
                    .local
                    .get_mut(&id)
                    .filter(|local| local.terminal.incarnation_id == incarnation)
                {
                    local.published = true;
                    if let Some(identity) = self.network.identity() {
                        local.entry.owner_user_id = Some(identity.user_id);
                        local.entry.host_device_id = Some(identity.device_id);
                    }
                    self.publish_catalog();
                } else {
                    self.retiring.insert(id);
                }
            }
            Err(error) => {
                tracing::debug!(%id, %error, "local session publication deferred");
            }
        }
    }

    fn apply_metadata(&mut self, command: &Command) {
        let Some(id) = command.session_id().and_then(|id| Uuid::parse_str(id).ok()) else {
            return;
        };
        #[cfg(feature = "host")]
        if let Some(local) = self.local.get_mut(&id) {
            match command {
                Command::RenameSession { name, .. } => local.entry.name.clone_from(name),
                Command::ShareSession { user_ids, .. } => {
                    local.entry.shared_with.clone_from(user_ids);
                }
                Command::AttachSession { mission_id, .. } => {
                    local.entry.mission_id.clone_from(mission_id);
                    local.entry.mission_name = None;
                }
                _ => {}
            }
        }
        if matches!(command, Command::LeaveSession { .. }) {
            self.disconnect_remote(id);
            self.remotes.remove(&id);
        }
        self.publish_catalog();
    }
}

#[cfg(feature = "host")]
pub(super) fn shell(config: &Config) -> PathBuf {
    config
        .initial_shell
        .as_ref()
        .map_or_else(kodosi_os::process::default_shell, PathBuf::from)
}

#[cfg(feature = "host")]
async fn session_program(
    config: &Config,
    directory: &Path,
    resume: Option<&ConversationIdentity>,
) -> Result<(PathBuf, Vec<String>)> {
    let Some(resume) = resume else {
        return Ok((shell(config), Vec::new()));
    };
    let root = provider::state_root(&config.home, resume.provider).map_err(Error::Invalid)?;
    provider::validate_resume(
        &root,
        resume.provider,
        directory
            .to_str()
            .ok_or_else(|| Error::Invalid("working directory is not UTF-8".to_owned()))?,
        &resume.native_conversation_id,
    )
    .await
    .map_err(Error::Invalid)?;
    let executable = crate::agents::executable(&shell(config), resume.provider.executable())
        .await
        .ok_or_else(|| Error::Invalid("The provider CLI is not installed.".to_owned()))?;
    let arguments = match resume.provider {
        provider::Provider::Claude => {
            vec!["--resume".to_owned(), resume.native_conversation_id.clone()]
        }
        provider::Provider::Copilot => {
            vec![format!("--resume={}", resume.native_conversation_id)]
        }
    };
    Ok((executable, arguments))
}

#[cfg(feature = "host")]
async fn start_line(
    config: &Config,
    launcher: &Arc<Launcher>,
    command: Option<&String>,
    agent: Option<&AgentStart>,
    prompt: Option<&String>,
) -> Result<Option<String>> {
    let quoted = prompt.map(|prompt| crate::agents::quote(&shell(config), prompt));
    let Some(agent) = agent else {
        return Ok(command.map(|line| {
            [Some(line.clone()), quoted]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" ")
        }));
    };
    let line = launcher.line(agent).await.map_err(Error::Invalid)?;
    let flag = agent.agent.prompt_flag().map(str::to_owned);
    Ok(Some(
        [Some(line), quoted.as_ref().and(flag), quoted]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" "),
    ))
}

#[cfg(feature = "host")]
async fn create_session(
    id: Uuid,
    config: Config,
    launcher: &Arc<Launcher>,
    command: &Command,
    changes: mpsc::Sender<SessionChange>,
) -> Result<StartedSession> {
    let Command::CreateSession {
        working_dir,
        resume,
        branch,
        worktrees,
        command: start,
        agent,
        prompt,
        ..
    } = command
    else {
        return Err(Error::Invalid("expected session creation".to_owned()));
    };
    let directory = working_dir
        .as_ref()
        .map_or_else(|| config.home.clone(), PathBuf::from);
    let directory = tokio::task::spawn_blocking(move || kodosi_os::fs::canonicalize(&directory))
        .await
        .map_err(std::io::Error::from)??;
    if !tokio::fs::metadata(&directory).await?.is_dir() {
        return Err(Error::Invalid(
            "working directory is not a directory".to_owned(),
        ));
    }
    if branch.is_some() && resume.is_some() {
        return Err(Error::Invalid(
            "A conversation resumes in its own folder, not on a new branch.".to_owned(),
        ));
    }
    if (start.is_some() || agent.is_some()) && resume.is_some() {
        return Err(Error::Invalid(
            "A resumed conversation starts without a command.".to_owned(),
        ));
    }
    if start.is_some() && agent.is_some() {
        return Err(Error::Invalid(
            "Start either a command or an agent.".to_owned(),
        ));
    }
    let start = start_line(
        &config,
        launcher,
        start.as_ref(),
        agent.as_ref(),
        prompt.as_ref(),
    )
    .await?;
    let (program, arguments) = session_program(&config, &directory, resume.as_ref()).await?;
    let worktree = match branch {
        Some(branch) => Some(
            crate::worktree::create(&directory, branch, worktrees.as_deref().map(Path::new))
                .await?,
        ),
        None => None,
    };
    let directory = worktree
        .as_ref()
        .map_or(directory, |worktree| worktree.path.clone());
    let terminal = match TerminalSession::spawn(
        id,
        Uuid::now_v7(),
        program,
        arguments,
        directory.clone(),
        Some(config.data_root.join("shell")),
        changes,
    )
    .await
    {
        Ok(terminal) => terminal,
        Err(error) => {
            if let Some(worktree) = worktree {
                crate::worktree::remove_unchanged(worktree).await;
            }
            return Err(error);
        }
    };
    if let Some(line) = start {
        terminal.start(line)?;
    }
    Ok(StartedSession {
        terminal: Some(terminal),
        repository: worktree.is_some() || crate::worktree::is_repository(&directory),
        directory,
        worktree,
    })
}

#[cfg(feature = "host")]
async fn provider_reply(
    home: &Path,
    shell: &Path,
    launcher: &Launcher,
    command: &Command,
) -> Result<Value> {
    match command {
        Command::InspectAgents { .. } => {
            return Ok(serde_json::to_value(
                launcher.catalog().await.map_err(Error::Invalid)?,
            )?);
        }
        Command::InspectAgentModels { agent, .. } => {
            return Ok(serde_json::to_value(launcher.models(*agent).await)?);
        }
        Command::AddAgentAccount { agent, name, .. } => {
            return Ok(serde_json::to_value(
                launcher
                    .add_account(*agent, name)
                    .await
                    .map_err(Error::Invalid)?,
            )?);
        }
        _ => {}
    }
    let provider = match command {
        Command::DiscoverConversations { provider, .. }
        | Command::ReadConversation { provider, .. }
        | Command::InspectProvider { provider, .. } => *provider,
        _ => return Err(Error::Invalid("expected provider request".to_owned())),
    };
    let root = provider::state_root(home, provider).map_err(Error::Invalid)?;
    let result = match command {
        Command::DiscoverConversations {
            provider,
            working_directory,
            cursor,
            limit,
            max_bytes,
            ..
        } => serde_json::to_value(
            provider::discover_history(
                &root,
                *provider,
                working_directory.as_deref(),
                cursor.as_deref(),
                *limit,
                *max_bytes,
            )
            .await
            .map_err(Error::Invalid)?,
        )?,
        Command::ReadConversation {
            provider,
            working_directory,
            native_conversation_id,
            before_byte,
            limit,
            max_bytes,
            ..
        } => serde_json::to_value(
            provider::read(
                &root,
                *provider,
                working_directory,
                native_conversation_id,
                *before_byte,
                *limit,
                *max_bytes,
            )
            .await
            .map_err(Error::Invalid)?,
        )?,
        Command::InspectProvider {
            provider,
            working_directory,
            ..
        } => serde_json::to_value(
            provider::inspect(
                &root,
                *provider,
                working_directory.as_deref(),
                crate::agents::executable(shell, provider.executable()).await,
            )
            .await
            .map_err(Error::Invalid)?,
        )?,
        _ => return Err(Error::Invalid("expected provider request".to_owned())),
    };
    Ok(result)
}
