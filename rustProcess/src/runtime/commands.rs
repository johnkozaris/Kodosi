use super::Reconnect;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{Completion, Job, Local, MAX_SESSIONS, Opening, Runtime, Scope, StartedSession};
use crate::{
    Command, Config, Error, Result,
    network::{NetworkReply, RemoteConnection},
    protocol::{ConnectionState, SessionEntry, SessionKind, SessionStatus, parse_id},
    provider,
    terminal::{LocalSession, RemoteTerminal, SessionChange, TerminalPixelGeometry, TerminalSize},
};

impl Runtime {
    pub(super) fn apply(&mut self, command: &Command) -> Result<()> {
        match command {
            Command::ListSessions {} => {
                self.publish_catalog();
                self.refresh_catalog();
            }
            Command::CreateSession { .. } => self.create(command.clone())?,
            Command::RenameSession { .. } => self.rename(command)?,
            Command::StopSession { .. } | Command::InterruptSession { .. } => {
                self.end_session(command)?;
            }
            Command::OpenRemote { session_id, .. } => {
                self.open_remote(parse_id(session_id)?, command.clone())?;
            }
            Command::DisconnectRemote { session_id } => {
                self.disconnect_remote(parse_id(session_id)?);
            }
            Command::ShareSession { .. }
            | Command::AttachSession { .. }
            | Command::LeaveSession { .. } => self.change_membership(command)?,
            Command::DiscoverConversations { .. }
            | Command::ReadConversation { .. }
            | Command::InspectProvider { .. } => {
                self.job_capacity()?;
                let home = self.config.home.clone();
                let command = command.clone();
                self.spawn(async move {
                    let result = provider_reply(&home, &command).await;
                    Completion::Provider { command, result }
                });
            }
            Command::Theme { dark } => {
                self.dark = *dark;
                for local in self.local.values() {
                    local.terminal.theme(*dark);
                }
            }
            Command::Resize { .. } | Command::Focus { .. } | Command::Blur { .. } => {
                return Err(Error::Invalid(
                    "terminal control requires a current subscription".to_owned(),
                ));
            }
            Command::Login {} | Command::Logout {} => {
                self.job_capacity()?;
                if self.auth_pending {
                    return Err(Error::Busy);
                }
                self.auth_pending = true;
                self.network_command(command.clone());
            }
            Command::Shutdown {} => {}
            _ => {
                self.job_capacity()?;
                self.network_command(command.clone());
            }
        }
        Ok(())
    }

    fn command_session(&self, command: &Command) -> Result<Uuid> {
        let (Command::RenameSession {
            session_id: id,
            expected_runtime_incarnation_id: incarnation,
            ..
        }
        | Command::StopSession {
            session_id: id,
            expected_runtime_incarnation_id: incarnation,
            ..
        }
        | Command::InterruptSession {
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
            Ok(())
        } else {
            self.administer(id, command.clone())
        }
    }

    fn end_session(&mut self, command: &Command) -> Result<()> {
        let id = self.command_session(command)?;
        if !self.local.contains_key(&id)
            && self
                .connections
                .get(&id)
                .is_none_or(RemoteTerminal::is_closed)
        {
            return self.open_remote(id, command.clone());
        }
        self.job_capacity()?;
        let stop = matches!(command, Command::StopSession { .. });
        let response = self.target(id)?.end(stop);
        if stop {
            if let Some(local) = self.local.get_mut(&id) {
                local.entry.status = SessionStatus::Stopping;
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
                    return Err(Error::Invalid(
                        "Change sharing on the computer hosting this session.".to_owned(),
                    ));
                }
                self.require_publication(id)?;
            }
            Command::AttachSession { .. } => {
                self.require_owner(id)?;
                self.require_publication(id)?;
            }
            Command::LeaveSession { .. }
                if self.local.contains_key(&id)
                    || self.remotes.get(&id).is_some_and(|entry| entry.is_owner) =>
            {
                return Err(Error::Invalid(
                    "The owner cannot leave their own session.".to_owned(),
                ));
            }
            _ => {}
        }
        self.administer(id, command.clone())
    }

    fn require_publication(&mut self, id: Uuid) -> Result<()> {
        if self.local.get(&id).is_some_and(|local| !local.published) {
            self.publish_locals();
            return Err(Error::Invalid("Remote access is still connecting. Try again when this device is online and approved.".to_owned()));
        }
        Ok(())
    }

    fn require_owner(&self, id: Uuid) -> Result<()> {
        if self.local.contains_key(&id) || self.remotes.get(&id).is_some_and(|entry| entry.is_owner)
        {
            Ok(())
        } else {
            Err(Error::Invalid(
                "Only the owner can change this session.".to_owned(),
            ))
        }
    }

    fn create(&mut self, command: Command) -> Result<()> {
        let Command::CreateSession {
            request_id, resume, ..
        } = &command
        else {
            return Err(Error::Invalid("expected session creation".to_owned()));
        };
        let existing = self.local.iter().find_map(|(id, local)| {
            (local.creation.request_id() == Some(request_id)).then_some((*id, &local.creation))
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
        if let Some(existing) = self
            .creating
            .values()
            .find(|pending| pending.request_id() == Some(request_id))
        {
            return if existing == &command {
                Ok(())
            } else {
                Err(Error::Invalid(
                    "request ID was reused for another session".to_owned(),
                ))
            };
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
        let dark = self.dark;
        self.creating.insert(id, command.clone());
        self.spawn(async move {
            let result = create_session(id, config, &command, dark, changes).await;
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
        if self.local.contains_key(&id) || self.connections.contains_key(&id) {
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
        if let Some(entry) = self.remotes.get_mut(&id) {
            if let Ok(incarnation) = Uuid::parse_str(&entry.incarnation_id) {
                self.reconnect.entry(id).or_insert_with(|| Reconnect {
                    incarnation,
                    failures: 0,
                    next: tokio::time::Instant::now() + Duration::from_secs(2),
                });
            }
            entry.connection_state = ConnectionState::Connecting;
            entry.message = None;
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
        self.publish_catalog();
    }

    fn administer(&mut self, id: Uuid, command: Command) -> Result<()> {
        self.job_capacity()?;
        if self.administering.contains(&id) || self.publishing.contains(&id) {
            return Err(Error::Busy);
        }
        self.administering.insert(id);
        self.network_command(command);
        Ok(())
    }

    pub(super) fn network_command(&mut self, command: Command) {
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
            Completion::Network {
                command,
                generation: network.generation(),
                result,
            }
        });
    }

    pub(super) fn terminal_command(&mut self, command: Command, connection: Uuid) -> Result<()> {
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
        let response = match &command {
            Command::Resize {
                identity, claim, ..
            } => {
                let (size, geometry) = geometry(identity)?;
                target.resize(connection, size, Some(geometry), *claim)
            }
            Command::Focus { .. } | Command::Blur { .. } => {
                target.focus(connection, matches!(command, Command::Focus { .. }))
            }
            _ => return Err(Error::Invalid("expected terminal control".to_owned())),
        };
        self.spawn(async move {
            Completion::Terminal {
                command,
                result: response.await,
            }
        });
        Ok(())
    }

    pub(super) fn terminal_result(&self, command: &Command, result: Result<()>) {
        match command {
            Command::Resize {
                session_id,
                identity,
                ..
            } => {
                if let Ok(mut value) = serde_json::to_value(identity) {
                    value["type"] = json!(if result.is_ok() {
                        "term.resizeApplied"
                    } else {
                        "term.resizeRejected"
                    });
                    value["sessionId"] = json!(session_id);
                    value["runtimeIncarnationId"] = json!(identity.expected_runtime_incarnation_id);
                    if let Err(error) = result {
                        value["reason"] = json!(error.to_string());
                    }
                    self.emit(value);
                }
            }
            Command::Focus {
                request_id,
                session_id,
                expected_runtime_incarnation_id,
                client_id,
                subscription_generation,
            } => {
                self.emit(json!({"type":if result.is_ok() {"term.focusApplied"} else {"term.focusRejected"},
                    "sessionId":session_id, "requestId":request_id,
                    "runtimeIncarnationId":expected_runtime_incarnation_id,
                    "subscriptionId":client_id, "subscriptionGeneration":subscription_generation,
                    "reason":result.err().map(|error| error.to_string())}));
            }
            Command::Blur { .. } => {}
            _ => match result {
                Ok(()) => self.result(command),
                Err(error) => self.error(command, &error),
            },
        }
    }

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
                if job.scope.network_generation == self.network.generation() && let Err(error) = result && !matches!(error, Error::Stale) { self.error(&Command::RefreshAuth {}, &error); }
                self.publish_locals();
            }
            Completion::Created { id, result } => self.created(id, result, current),
            Completion::Network { command, generation, result } => self.network_completed(&job.scope, &command, generation, result),
            Completion::Connected { id, attempt, result } => self.connected(id, attempt, result, current),
            Completion::Published { id, incarnation, result } => self.published(id, incarnation, result, current),
            Completion::Unpublished { id, result } if current => {
                self.unpublishing.remove(&id);
                match result {
                    Ok(()) => { self.retiring.remove(&id); }
                    Err(error) => tracing::debug!(%id, %error, "session retirement deferred"),
                }
            }
            Completion::Terminal { command, result } if current => self.terminal_result(&command, result),
            Completion::Provider { command, result } if current => match result {
                Ok(value) => self.emit(json!({"type":"provider.reply", "requestId":command.request_id(), "operation":command.operation(), "result":value})),
                Err(error) => self.error(&command, &error),
            },
            Completion::Checkpoint { reply, result } => { drop(reply.send(if current { result } else { Err(Error::Stale) })); }
            Completion::HeadlessControl { reply, result } => { drop(reply.send(if current { result } else { Err(Error::Stale) })); }
            Completion::Subscribed { target, reply, result } => match result {
                Ok(subscription) if current => {
                    if let Err(Ok(subscription)) = reply.send(Ok(subscription)) { target.unsubscribe(subscription.connection_id); }
                }
                Ok(subscription) => { target.unsubscribe(subscription.connection_id); drop(reply.send(Err(Error::Stale))); }
                Err(error) => { drop(reply.send(Err(error))); }
            },
            Completion::Unpublished { .. } | Completion::Terminal { .. } | Completion::Provider { .. } => {},
        }
    }

    fn created(&mut self, id: Uuid, result: Result<StartedSession>, current: bool) {
        let Some(command) = self.creating.remove(&id) else {
            return;
        };
        if !current {
            return;
        }
        match result {
            Ok(mut started) => {
                let Some(terminal) = started.terminal.take() else {
                    return;
                };
                let Command::CreateSession {
                    request_id, name, ..
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
                    connected_users: vec![],
                    program: None,
                    title: None,
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
                    room_id: None,
                    room_name: None,
                    shared_with: vec![],
                    create_request_id: Some(request_id.clone()),
                };
                let created = json!({"type":"session.result", "requestId":request_id, "sessionId":id, "operation":"session.create"});
                self.local.insert(
                    id,
                    Local {
                        terminal,
                        entry,
                        creation: command,
                        published: false,
                    },
                );
                self.publish_catalog();
                self.emit(created);
                self.publish_locals();
            }
            Err(error) => self.error(&command, &error),
        }
    }

    fn network_completed(
        &mut self,
        scope: &Scope,
        command: &Command,
        generation: u64,
        result: Result<NetworkReply>,
    ) {
        let changes_account = matches!(command, Command::Login {} | Command::Logout {});
        if changes_account {
            self.auth_pending = false;
        }
        let current = self.in_scope(scope);
        let own_retirement = changes_account
            && scope.network_generation.checked_add(1) == Some(generation)
            && generation == self.network.generation();
        if !current && !own_retirement {
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
                self.network_reply(reply);
            }
            Err(error) => {
                self.error(command, &error);
            }
            Ok(_) => {}
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
                self.reconnect.insert(
                    id,
                    Reconnect {
                        incarnation: connection.session.incarnation_id,
                        failures: 0,
                        next: tokio::time::Instant::now() + Duration::from_secs(2),
                    },
                );
                let mut entry = self.remote_entry(connection.session.clone());
                entry.connection_state = ConnectionState::Connected;
                entry.message = None;
                self.connections
                    .insert(id, RemoteTerminal::spawn(connection, self.changes.clone()));
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
                if matches!(error, Error::Invalid(_)) {
                    self.reconnect.remove(&id);
                }
                if let Some(retry) = self.reconnect.get_mut(&id) {
                    retry.failures = retry.failures.saturating_add(1);
                    retry.next = tokio::time::Instant::now()
                        + Duration::from_secs(2_u64.pow(retry.failures.min(5)));
                }
                if let Some(entry) = self.remotes.get_mut(&id) {
                    entry.connection_state = if matches!(error, Error::Invalid(_)) {
                        ConnectionState::Blocked
                    } else {
                        ConnectionState::Offline
                    };
                    entry.message = Some(error.to_string());
                }
                self.publish_catalog();
                for command in opening.commands {
                    self.error(&command, &error);
                }
            }
        }
    }

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
        if let Some(local) = self.local.get_mut(&id) {
            match command {
                Command::RenameSession { name, .. } => local.entry.name.clone_from(name),
                Command::ShareSession { user_ids, .. } => {
                    local.entry.shared_with.clone_from(user_ids);
                }
                Command::AttachSession { room_id, .. } => {
                    local.entry.room_id.clone_from(room_id);
                    local.entry.room_name = None;
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

fn geometry(
    identity: &crate::protocol::ResizeIdentity,
) -> Result<(TerminalSize, TerminalPixelGeometry)> {
    let size = TerminalSize::new(identity.rows, identity.cols)
        .map_err(|error| Error::Invalid(error.to_string()))?;
    let geometry = TerminalPixelGeometry::new(
        identity.width_pixels,
        identity.height_pixels,
        identity.cell_width_pixels,
        identity.cell_height_pixels,
    )
    .and_then(|geometry| geometry.validate_for_size(size))
    .map_err(|error| Error::Invalid(error.to_string()))?;
    Ok((size, geometry))
}

async fn create_session(
    id: Uuid,
    config: Config,
    command: &Command,
    dark: bool,
    changes: mpsc::Sender<SessionChange>,
) -> Result<StartedSession> {
    let Command::CreateSession {
        working_dir,
        resume,
        ..
    } = command
    else {
        return Err(Error::Invalid("expected session creation".to_owned()));
    };
    let directory = working_dir
        .as_ref()
        .map_or_else(|| config.home.clone(), PathBuf::from);
    let directory = tokio::fs::canonicalize(directory).await?;
    if !tokio::fs::metadata(&directory).await?.is_dir() {
        return Err(Error::Invalid(
            "working directory is not a directory".to_owned(),
        ));
    }
    let (program, arguments) = if let Some(resume) = resume {
        provider::validate_resume(
            &config.home,
            resume.provider,
            directory
                .to_str()
                .ok_or_else(|| Error::Invalid("working directory is not UTF-8".to_owned()))?,
            &resume.native_conversation_id,
        )
        .await
        .map_err(Error::Invalid)?;
        let executable = provider::resolve_executable(resume.provider)
            .ok_or_else(|| Error::Invalid("The provider CLI is not installed.".to_owned()))?;
        let arguments = match resume.provider {
            provider::Provider::Claude => {
                vec!["--resume".to_owned(), resume.native_conversation_id.clone()]
            }
            provider::Provider::Copilot => {
                vec![format!("--resume={}", resume.native_conversation_id)]
            }
        };
        (executable, arguments)
    } else {
        (
            config
                .initial_shell
                .as_ref()
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("SHELL").map(PathBuf::from))
                .unwrap_or_else(|| PathBuf::from("/bin/sh")),
            vec![],
        )
    };
    let terminal = LocalSession::spawn(
        id,
        Uuid::now_v7(),
        program,
        arguments,
        directory.clone(),
        dark,
        changes,
    )
    .await?;
    Ok(StartedSession {
        terminal: Some(terminal),
        directory,
    })
}

async fn provider_reply(home: &Path, command: &Command) -> Result<Value> {
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
                home,
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
                home,
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
            provider::inspect(home, *provider, working_directory.as_deref())
                .await
                .map_err(Error::Invalid)?,
        )?,
        _ => return Err(Error::Invalid("expected provider request".to_owned())),
    };
    Ok(result)
}
