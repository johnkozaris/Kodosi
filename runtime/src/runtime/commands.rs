use super::Reconnect;
use bytes::Bytes;
use serde_json::{Value, json};
#[cfg(feature = "host")]
use std::collections::BTreeSet;
#[cfg(feature = "host")]
use std::path::{Path, PathBuf};
use std::{borrow::Cow, collections::HashSet, time::Duration};
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
    network::{BackendReply, RemoteConnection, input_not_sent},
    protocol::{AppLifecycle, ConnectionState, SessionStatus, parse_id},
    terminal::RemoteTerminal,
};
#[cfg(feature = "host")]
use crate::{
    Config,
    agents::{AgentStart, Launch, Launcher, RunningAgent},
    protocol::{SessionEntry, SessionKind},
    provider::{self, ConversationIdentity},
    terminal::{SessionChange, TerminalSession},
};
#[cfg(feature = "host")]
use std::sync::Arc;

/// How long a terminal's chat takes at most to give its conversation.
const CONVERSATION_WAIT: Duration = Duration::from_secs(10);
/// What an agent hears when someone lets it go on with the task that a restart cut off.
pub(super) const GO_ON: &str = "Continue";

fn run_on_host() -> crate::text::Text {
    text!("Start a program on the computer that hosts this terminal.")
}
#[cfg(not(feature = "host"))]
pub(super) fn no_dosi() -> crate::text::Text {
    text!("Dosi runs on the computer that hosts your terminals.")
}

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
                if !self.shown(id) {
                    self.follow_view(id);
                    self.result(command);
                    return Ok(());
                }
                if let Err(error) = self.open_remote(id, command.clone()) {
                    self.release_view(id, consumer);
                    return Err(error);
                }
                self.follow_view(id);
                Ok(())
            }
            Command::DocumentOpen { .. } => self.open_document(command, consumer, lifetime),
            Command::DisconnectRemote { session_id, .. } => {
                self.release_view(parse_id(session_id)?, consumer);
                self.result(command);
                Ok(())
            }
            Command::Visible {
                session_ids,
                watching,
                ..
            } => {
                self.see(
                    consumer,
                    lifetime,
                    session_ids.iter().cloned().collect(),
                    *watching,
                );
                self.result(command);
                Ok(())
            }
            Command::Presence { state, .. } => {
                self.attention.present(consumer, *state, lifetime);
                self.attend();
                self.result(command);
                Ok(())
            }
            Command::Resync { .. } => {
                for mut event in self.snapshot_events()? {
                    event.recipient = Some(consumer);
                    self.send_event(event);
                }
                self.show_approvals(consumer);
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
        self.follow_views();
    }

    #[expect(clippy::too_many_lines, reason = "one dispatch for every command")]
    pub(super) fn apply(&mut self, command: &Command) -> Result<()> {
        match command {
            Command::ListSessions { .. } => {
                self.publish_asked_catalog();
                if self.scope.user.is_some() {
                    self.job_capacity()?;
                    self.network_command(command.clone());
                } else {
                    self.result(command);
                }
            }
            #[cfg(feature = "host")]
            Command::CreateSession { .. } => self.create(command.clone())?,
            #[cfg(feature = "host")]
            Command::RestoreSessions { .. } => {
                self.restore_saved();
                self.result(command);
            }
            #[cfg(not(feature = "host"))]
            Command::RestoreSessions { .. } => self.result(command),
            Command::ListSessionFolders { .. } => {
                let mut value = super::answer(command, Ok(()));
                value["folders"] = json!(self.session_folders());
                self.emit(value);
            }
            Command::SuggestBranch { .. } => {
                let mut value = super::answer(command, Ok(()));
                value["branch"] = json!(crate::names::new_branch(&self.terminal_names()));
                self.emit(value);
            }
            #[cfg(not(feature = "host"))]
            Command::CreateSession { .. } => {
                return Err(Error::Refused(text!("This device cannot start terminals.")));
            }
            Command::RenameSession { .. } => self.rename(command)?,
            Command::Input { .. } => self.send_input(command)?,
            Command::ChangeAgent { .. } => self.change_agent(command)?,
            Command::Picture { .. } => self.paste_picture(command)?,
            #[cfg(feature = "host")]
            Command::Message { .. } => self.send_message(command)?,
            #[cfg(not(feature = "host"))]
            Command::Message { .. } => {
                return Err(Error::Refused(text!(
                    "This device sends no messages to agents."
                )));
            }
            Command::Read { .. } => self.read(command)?,
            Command::CanvasAsk { .. } => self.ask_canvas(command)?,
            #[cfg(feature = "host")]
            Command::BrowserStatus { .. }
            | Command::BrowserCopy { .. }
            | Command::BrowserForget { .. }
            | Command::BrowserClear { .. }
            | Command::BrowserGet { .. }
            | Command::BrowserSet { .. } => self.use_browser_settings(command)?,
            #[cfg(not(feature = "host"))]
            Command::BrowserStatus { .. }
            | Command::BrowserCopy { .. }
            | Command::BrowserForget { .. }
            | Command::BrowserClear { .. }
            | Command::BrowserGet { .. }
            | Command::BrowserSet { .. } => {
                return Err(Error::Refused(text!(
                    "The browsers of a computer are in the app on that computer."
                )));
            }
            #[cfg(feature = "host")]
            Command::ScreenStatus { .. } | Command::ScreenAllow { .. } => {
                self.use_screen(command)?;
            }
            #[cfg(not(feature = "host"))]
            Command::ScreenStatus { .. } | Command::ScreenAllow { .. } => {
                return Err(Error::Refused(text!(
                    "Only the app on a computer shares its screen."
                )));
            }
            Command::DocumentSend { .. } => self.send_to_document(command)?,
            Command::DocumentClose { .. } => self.close_document(command)?,
            #[cfg(feature = "host")]
            Command::CanvasShow { .. }
            | Command::CanvasBrowse { .. }
            | Command::CanvasDocument { .. } => {
                self.use_canvas(command)?;
            }
            #[cfg(not(feature = "host"))]
            Command::CanvasShow { .. }
            | Command::CanvasBrowse { .. }
            | Command::CanvasDocument { .. } => {
                return Err(Error::Invalid(
                    "Agents show files on the canvas of their own terminal.".to_owned(),
                ));
            }
            Command::Show { session_id, .. } => {
                if !self.listed(&parse_id(session_id)?) {
                    return Err(Error::NotFound);
                }
                self.emit(json!({"type":"session.show", "sessionId":session_id}));
                self.result(command);
            }
            Command::ShowProject { project_id, .. } => {
                self.emit(json!({"type":"project.show", "projectId":project_id}));
                self.result(command);
            }
            Command::RetryConnection { .. } => {
                self.network.retry_now();
                self.retry_views_now();
                self.result(command);
            }
            Command::Lifecycle {
                state: AppLifecycle::Background,
                ..
            } => self.go_to_background(command),
            Command::Lifecycle {
                state: AppLifecycle::Foreground,
                ..
            } => {
                if let Some(pausing) = self.pausing.take() {
                    pausing.cancel();
                }
                self.network.resume();
                self.retry_views_now();
                self.result(command);
            }
            Command::NetworkChanged { .. } => {
                self.network.network_changed();
                self.retry_views_now();
                self.result(command);
            }
            #[cfg(feature = "host")]
            Command::Look { look, .. } => {
                if self.look.change(*look) {
                    for local in self.local.values_mut() {
                        local.entry.look = *look;
                    }
                    self.publish_catalog();
                }
                self.result(command);
            }
            #[cfg(not(feature = "host"))]
            Command::Look { .. } => self.result(command),
            command if command.is_dosi() => self.dosi_command(command)?,
            Command::SeeProject {
                project_id,
                position,
                ..
            } => {
                self.see_project(project_id, *position)?;
                self.result(command);
            }
            Command::CloseSession { .. } | Command::InterruptSession { .. } => {
                self.end_session(command)?;
            }
            Command::ContinueSession { .. } => self.continue_session(command)?,
            Command::ShowChanges { .. } => self.show_changes(command)?,
            #[cfg(feature = "host")]
            Command::RunInSession { .. } => self.run_in_session(command)?,
            #[cfg(not(feature = "host"))]
            Command::RunInSession { .. } => {
                self.command_session(command)?;
                return Err(Error::Text(run_on_host()));
            }
            Command::OpenRemote { .. }
            | Command::DisconnectRemote { .. }
            | Command::Visible { .. }
            | Command::Presence { .. }
            | Command::Resync { .. } => {
                return Err(Error::Invalid(
                    "remote view commands require a current client".to_owned(),
                ));
            }
            Command::ShareSession { .. }
            | Command::AttachSession { .. }
            | Command::LeaveSession { .. }
            | Command::HideSession { .. } => self.change_membership(command)?,
            #[cfg(feature = "host")]
            Command::DiscoverConversations { .. }
            | Command::ReadConversation { .. }
            | Command::InspectProvider { .. }
            | Command::InspectAgents { .. }
            | Command::InspectAgentModels { .. }
            | Command::InspectUsage { .. }
            | Command::AddAgentAccount { .. }
            | Command::ChooseAgent { .. } => self.provider(command)?,
            #[cfg(not(feature = "host"))]
            Command::DiscoverConversations { .. }
            | Command::ReadConversation { .. }
            | Command::InspectProvider { .. } => {
                return Err(Error::Refused(text!(
                    "Open agent conversations on the computer that runs the agent."
                )));
            }
            #[cfg(not(feature = "host"))]
            Command::ListRepositories { .. } => {
                return Err(Error::Refused(text!("Link repositories from a computer.")));
            }
            #[cfg(feature = "host")]
            Command::ListRepositories { project_id, .. } => {
                self.job_capacity()?;
                let folders = self.repository_folders(project_id.as_deref());
                let client = self.network.inner.http.client.clone();
                let command = command.clone();
                self.spawn(async move {
                    let result = crate::projects::providers::choices(&client, folders)
                        .await
                        .map_err(Error::from);
                    Completion::Repositories { command, result }
                });
            }
            #[cfg(not(feature = "host"))]
            Command::InspectAgents { .. }
            | Command::InspectAgentModels { .. }
            | Command::InspectUsage { .. }
            | Command::AddAgentAccount { .. }
            | Command::ChooseAgent { .. }
            | Command::ListFolders { .. } => {
                return Err(Error::Refused(text!("This device cannot start terminals.")));
            }
            #[cfg(feature = "host")]
            Command::Sleep { .. } => self.sleep(command)?,
            #[cfg(feature = "host")]
            Command::KeepAwake { keep_awake, .. } => self.keep_awake(command, *keep_awake),
            Command::Wake { device_id, .. } => self.wake(command, device_id)?,
            Command::TestWake { device_id, .. } => self.test_wake(command, device_id)?,
            #[cfg(feature = "host")]
            Command::DescribeWake { .. } => self.describe_wake(command)?,
            #[cfg(feature = "host")]
            Command::SignalWake { sleeper, .. } => self.signal_wake(command, sleeper)?,
            #[cfg(feature = "host")]
            Command::RecordWakeTest { hardware, woke, .. } => {
                let test = crate::protocol::WakeTest {
                    hardware: hardware.clone(),
                    woke: *woke,
                };
                self.record_wake_test(command, test)?;
            }
            #[cfg(not(feature = "host"))]
            Command::Sleep { .. }
            | Command::KeepAwake { .. }
            | Command::DescribeWake { .. }
            | Command::SignalWake { .. }
            | Command::RecordWakeTest { .. } => {
                return Err(Error::Invalid(
                    "A power command goes to a computer.".to_owned(),
                ));
            }
            #[cfg(feature = "host")]
            Command::ListFolders { path, .. } => {
                self.job_capacity()?;
                let home = self.config.home.clone();
                let recent = self.recent.with(self.running_folders());
                let (command, path) = (command.clone(), path.clone());
                self.spawn(async move {
                    let result = crate::folders::list(home, path, recent).await;
                    Completion::Folders { command, result }
                });
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

    fn go_to_background(&mut self, command: &Command) {
        let pausing = CancellationToken::new();
        if let Some(earlier) = self.pausing.replace(pausing.clone()) {
            earlier.cancel();
        }
        let command = command.clone();
        self.spawn(async move {
            tokio::select! {
                () = pausing.cancelled() => {}
                () = tokio::time::sleep(super::BACKGROUND_GRACE) => {}
            }
            Completion::BackgroundDue { command, pausing }
        });
    }

    #[cfg(feature = "host")]
    fn repository_folders(&self, project_id: Option<&str>) -> Vec<String> {
        let mut terminals = self
            .local
            .values()
            .filter_map(|local| {
                Some((
                    local.entry.project_id.as_deref() != project_id,
                    local.entry.working_dir.clone()?,
                ))
            })
            .collect::<Vec<_>>();
        terminals.sort_by_key(|(other, _)| *other);
        let mut folders: Vec<String> = Vec::new();
        for folder in terminals
            .into_iter()
            .map(|(_, folder)| folder)
            .chain(self.recent.with([]))
        {
            if !folders.contains(&folder) {
                folders.push(folder);
            }
        }
        folders
    }

    #[cfg(feature = "host")]
    fn running_folders(&self) -> Vec<String> {
        let mut running = self
            .local
            .iter()
            .filter_map(|(id, local)| Some((*id, local.entry.working_dir.clone()?)))
            .collect::<Vec<_>>();
        running.sort_by_key(|(id, _)| std::cmp::Reverse(*id));
        running.into_iter().map(|(_, folder)| folder).collect()
    }

    /// A terminal for another computer gets its name here too, so that the command between
    /// computers always carries one.
    pub(super) fn named<'a>(&self, command: &'a Command) -> Cow<'a, Command> {
        match command {
            Command::CreateSession { name, .. } if name.is_empty() => {
                let mut named = command.clone();
                if let Command::CreateSession { name, .. } = &mut named {
                    *name = crate::names::new_terminal(&self.terminal_names());
                }
                Cow::Owned(named)
            }
            _ => Cow::Borrowed(command),
        }
    }

    fn terminal_names(&self) -> HashSet<&str> {
        let names = self.remotes.values().map(|entry| entry.name.as_str());
        #[cfg(feature = "host")]
        let names = names
            .chain(self.local.values().map(|local| local.entry.name.as_str()))
            .chain(self.creating.values().filter_map(|command| match command {
                Command::CreateSession { name, .. } => Some(name.as_str()),
                _ => None,
            }));
        names.collect()
    }

    pub(super) fn ask_computer(&mut self, command: &Command, computer: String) -> Result<()> {
        if !command.runs_on_other_computers() {
            return Err(Error::Refused(text!("Do this on the computer itself.")));
        }
        self.job_capacity()?;
        let request = serde_json::to_value(command)?;
        let network = self.network.clone();
        let command = command.clone();
        self.spawn(async move {
            let result = network
                .ask_computer(&computer, request)
                .await
                .map_err(Error::from);
            Completion::Asked {
                command,
                computer,
                result,
            }
        });
        Ok(())
    }

    pub(super) fn asked(&mut self, command: &Command, computer: &str, answer: Value) -> Result<()> {
        let event = self
            .event(answer.clone())
            .map_err(|_| Error::Text(crate::protocol::other_version()))?;
        if event.event.request_id() != Some(command.request_id()) || !event.event.is_answer() {
            return Err(Error::Text(crate::protocol::other_version()));
        }
        if let (Command::KeepAwake { keep_awake, .. }, crate::EventBody::PowerResult { .. }) =
            (command, &event.event)
        {
            self.kept_awake(computer, *keep_awake);
        }
        let started = match (command, &event.event) {
            (
                Command::CreateSession { .. },
                crate::EventBody::SessionResult {
                    session_id: Some(id),
                    ..
                },
            ) => Uuid::parse_str(id).ok(),
            _ => None,
        };
        match started {
            Some(id) => {
                self.starting.insert(
                    id,
                    super::Starting {
                        answer,
                        until: tokio::time::Instant::now() + super::STARTING,
                    },
                );
                self.started_remotes();
            }
            None => self.emit(answer),
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
            conversation,
            ..
        } = command
        else {
            return Err(Error::Invalid("expected a terminal read".to_owned()));
        };
        let id = parse_id(session_id)?;
        self.check_incarnation(id, parse_id(expected_runtime_incarnation_id)?)?;
        self.reachable(id)?;
        self.job_capacity()?;
        let target = self.target(id)?;
        let read: std::pin::Pin<Box<dyn Future<Output = Result<String>> + Send>> = if *conversation
        {
            let chat = target.document(
                Uuid::now_v7(),
                crate::chat::CHAT_PIPE.to_owned(),
                self.person.clone().unwrap_or_else(|| "Someone".to_owned()),
                self.scope.user.clone(),
                Uuid::now_v7(),
            );
            Box::pin(read_conversation(chat))
        } else {
            target.read(*history)
        };
        let command = command.clone();
        self.spawn(async move {
            Completion::Read {
                command,
                result: read.await,
            }
        });
        Ok(())
    }

    fn ask_canvas(&mut self, command: &Command) -> Result<()> {
        let Command::CanvasAsk {
            session_id,
            request,
            ..
        } = command
        else {
            return Err(Error::Invalid("expected a canvas request".to_owned()));
        };
        let id = parse_id(session_id)?;
        self.reachable(id)?;
        self.job_capacity()?;
        let answer = self.target(id)?.canvas(request.clone());
        let command = command.clone();
        self.spawn(async move {
            Completion::Canvas {
                command,
                result: answer.await,
            }
        });
        Ok(())
    }

    #[cfg(feature = "host")]
    fn use_browser_settings(&mut self, command: &Command) -> Result<()> {
        self.job_capacity()?;
        let browsers = self.services.browsers.clone();
        let command = command.clone();
        self.spawn(async move {
            let result = match &command {
                Command::BrowserCopy { name, .. } => browsers
                    .copy_sign_ins(name)
                    .await
                    .map(|sites| json!({ "sites": sites })),
                Command::BrowserForget { .. } => browsers
                    .forget_sign_ins()
                    .await
                    .map(|sites| json!({ "sites": sites })),
                Command::BrowserClear { .. } => browsers.clear().await.map(|()| json!({})),
                Command::BrowserGet { .. } => browsers.get().await.map(|()| json!({})),
                Command::BrowserSet {
                    form_predictions,
                    cross_site_cookies,
                    ..
                } => browsers
                    .use_privacy(shared_browser::Privacy {
                        form_predictions: *form_predictions,
                        cross_site_cookies: *cross_site_cookies,
                    })
                    .await
                    .map(|()| json!({})),
                _ => {
                    let status = browsers.status().await;
                    let sources: Vec<Value> = status
                        .sources
                        .iter()
                        .map(|source| {
                            json!({
                                "name": source.name,
                                "browser": source.browser,
                                "protected": source.protected,
                            })
                        })
                        .collect();
                    Ok(json!({
                        "sources": sources,
                        "sites": status.sites,
                        "engine": status.engine,
                        "getting": status.getting,
                        "formPredictions": status.privacy.form_predictions,
                        "crossSiteCookies": status.privacy.cross_site_cookies,
                    }))
                }
            };
            Completion::Settings { command, result }
        });
        Ok(())
    }

    #[cfg(feature = "host")]
    fn use_screen(&mut self, command: &Command) -> Result<()> {
        let screen = self.services.screen.clone();
        let mut question = None;
        match command {
            Command::ScreenAllow {
                allowed: true,
                vouch,
                ..
            } => {
                screen.offered().map_err(Error::Text)?;
                let account = self.network.account().unwrap_or_default();
                let server = self.config.foreign_server();
                question = Some((
                    crate::canvas::screen_question(&account, server.as_deref()),
                    vouch.clone(),
                ));
            }
            Command::ScreenAllow { allowed: false, .. } => screen.turn_off(),
            _ => {}
        }
        self.job_capacity()?;
        let command = command.clone();
        self.spawn(async move {
            if let Some((question, vouch)) = question {
                screen.turn_on(question, vouch).await;
            }
            let access = screen.access().await;
            let result = Ok(json!({
                "allowed": *screen.allowed().borrow(),
                "available": screen.available(),
                "access": access,
            }));
            Completion::Settings { command, result }
        });
        Ok(())
    }

    #[cfg(feature = "host")]
    fn use_canvas(&mut self, command: &Command) -> Result<()> {
        let id = parse_id(command.session_id().unwrap_or_default())?;
        let (canvas, terminal, agent) = self
            .local
            .get(&id)
            .map(|local| {
                let agent = local
                    .entry
                    .program
                    .as_deref()
                    .and_then(crate::agents::Agent::named)
                    .map(|agent| agent.label().to_owned());
                (
                    local.terminal.canvas.clone(),
                    local.entry.name.clone(),
                    agent,
                )
            })
            .ok_or_else(|| {
                Error::Invalid("Use the canvas of a terminal on this computer.".to_owned())
            })?;
        self.job_capacity()?;
        let command = command.clone();
        self.spawn(async move {
            let result = match &command {
                Command::CanvasShow {
                    paths,
                    address,
                    browser,
                    screen,
                    folder,
                    ..
                } => {
                    canvas
                        .show(crate::canvas::Show {
                            paths: paths.iter().map(PathBuf::from).collect(),
                            address: address.clone(),
                            browser_as_it_is: *browser,
                            screen: screen.clone(),
                            folder: PathBuf::from(folder),
                            terminal,
                            agent,
                        })
                        .await
                }
                Command::CanvasBrowse {
                    folder, arguments, ..
                } => {
                    canvas
                        .browse(PathBuf::from(folder), arguments.clone(), terminal, agent)
                        .await
                }
                Command::CanvasDocument {
                    folder, arguments, ..
                } => Ok(canvas
                    .document_command(agent, terminal, PathBuf::from(folder), arguments.clone())
                    .await),
                _ => Err(crate::text::Text::plain("expected a canvas command")),
            };
            Completion::Canvas { command, result }
        });
        Ok(())
    }

    fn send_input(&mut self, command: &Command) -> Result<()> {
        let Command::Input {
            session_id,
            expected_runtime_incarnation_id,
            text,
            enter,
            ..
        } = command
        else {
            return Err(Error::Invalid("expected terminal input".to_owned()));
        };
        let id = parse_id(session_id)?;
        self.check_incarnation(id, parse_id(expected_runtime_incarnation_id)?)?;
        self.reachable(id)?;
        self.job_capacity()?;
        let target = self.target(id)?;
        let input = if *enter {
            target.type_line(text.clone(), false)
        } else {
            target.send_input(Bytes::from(text.clone().into_bytes()))
        };
        let command = command.clone();
        self.spawn(async move {
            Completion::Terminal {
                command,
                result: input.await,
            }
        });
        Ok(())
    }

    fn change_agent(&mut self, command: &Command) -> Result<()> {
        let Command::ChangeAgent {
            session_id,
            expected_runtime_incarnation_id,
            change,
            ..
        } = command
        else {
            return Err(Error::Invalid("expected a change of the agent".to_owned()));
        };
        let id = parse_id(session_id)?;
        self.check_incarnation(id, parse_id(expected_runtime_incarnation_id)?)?;
        self.reachable(id)?;
        #[cfg(feature = "host")]
        if self.hosts(&id) {
            self.change_agent_here(id, change)?;
            self.result(command);
            return Ok(());
        }
        self.job_capacity()?;
        let change = self.target(id)?.change_agent(change.clone());
        let command = command.clone();
        self.spawn(async move {
            Completion::Terminal {
                command,
                result: change.await,
            }
        });
        Ok(())
    }

    fn paste_picture(&mut self, command: &Command) -> Result<()> {
        let Command::Picture {
            session_id,
            expected_runtime_incarnation_id,
            path,
            ..
        } = command
        else {
            return Err(Error::Invalid("expected a picture".to_owned()));
        };
        let id = parse_id(session_id)?;
        self.check_incarnation(id, parse_id(expected_runtime_incarnation_id)?)?;
        self.reachable(id)?;
        self.job_capacity()?;
        let target = self.target(id)?;
        let path = path.clone();
        let command = command.clone();
        self.spawn(async move {
            let result = match crate::terminal::Picture::read(path).await {
                Ok(picture) => target.paste_picture(picture).await,
                Err(error) => Err(error),
            };
            Completion::Terminal { command, result }
        });
        Ok(())
    }

    #[cfg(feature = "host")]
    fn send_message(&mut self, command: &Command) -> Result<()> {
        let Command::Message {
            session_id,
            expected_runtime_incarnation_id,
            text,
            from_session_id,
            agent,
            ..
        } = command
        else {
            return Err(Error::Invalid("expected a message".to_owned()));
        };
        let id = parse_id(session_id)?;
        self.check_incarnation(id, parse_id(expected_runtime_incarnation_id)?)?;
        self.reachable(id)?;
        self.job_capacity()?;
        let from = from_session_id.as_deref().map(parse_id).transpose()?;
        if from == Some(id) {
            return Err(Error::Invalid(
                "This is your own terminal. Send the message to another terminal.".to_owned(),
            ));
        }
        let line = super::messages::message_line(&self.sender(from, agent.as_deref()), text);
        let delivery = self.target(id)?.type_line(line, true);
        let command = command.clone();
        self.spawn(async move {
            Completion::Terminal {
                command,
                result: delivery.await,
            }
        });
        Ok(())
    }

    pub(super) fn command_session(&self, command: &Command) -> Result<Uuid> {
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
        | Command::ContinueSession {
            session_id: id,
            expected_runtime_incarnation_id: incarnation,
            ..
        }
        | Command::ShowChanges {
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
        }
        | Command::HideSession {
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
        let (terminal, folder) = self
            .local
            .get(&id)
            .map(|local| {
                (
                    local.terminal.clone(),
                    local.entry.working_dir.clone().map(PathBuf::from),
                )
            })
            .ok_or_else(|| Error::Text(run_on_host()))?;
        let launcher = Arc::clone(&self.launcher);
        let antigravity_mount = self.config.manages_home().then(|| {
            (
                self.config.home.clone(),
                crate::agents::notes_folder(&self.config.data_root),
            )
        });
        let (line, agent) = (line.clone(), agent.clone());
        let command = command.clone();
        self.spawn(async move {
            let result = async {
                let (line, quiet_settings) = match (line, agent) {
                    (Some(line), None) => (line, None),
                    (None, Some(agent)) => {
                        if agent.agent == crate::agents::Agent::Antigravity
                            && let Some((home, notes)) = antigravity_mount
                        {
                            mount_antigravity_plugin(home, notes).await;
                        }
                        let launch = launcher.line(&agent).await.map_err(Error::Invalid)?;
                        if let (Some(home), Some(folder)) = (launch.copilot_home, folder) {
                            allow_copilot_link(home, folder).await;
                        }
                        (launch.line, launch.quiet_settings)
                    }
                    _ => {
                        return Err(Error::Invalid(
                            "Run either a command or an agent.".to_owned(),
                        ));
                    }
                };
                let held = match quiet_settings.clone() {
                    Some(settings) => launcher.hold_quiet(id, settings).await,
                    None => false,
                };
                let ran = terminal.run(line).await;
                if ran.is_err()
                    && held
                    && let Some(settings) = quiet_settings
                {
                    launcher.release_quiet_hold(id, settings).await;
                }
                ran
            }
            .await;
            Completion::Terminal { command, result }
        });
        Ok(())
    }

    /// A close of a terminal on another computer did its work when the list no longer has the
    /// terminal, even when the end of the terminal came before the host's answer.
    pub(super) fn closed_and_gone(&self, command: &Command) -> bool {
        matches!(command, Command::CloseSession { .. })
            && command
                .session_id()
                .and_then(|id| Uuid::parse_str(id).ok())
                .is_some_and(|id| !self.hosts(&id) && !self.listed.contains_key(&id))
    }

    fn end_session(&mut self, command: &Command) -> Result<()> {
        let id = self.command_session(command)?;
        let closing = matches!(command, Command::CloseSession { .. });
        if closing && !self.hosts(&id) && !self.host_online(id) {
            return self.administer(id, command.clone());
        }
        self.reachable(id)?;
        if !self.hosts(&id)
            && self
                .connections
                .get(&id)
                .is_none_or(RemoteTerminal::is_closed)
        {
            return self.open_remote(id, command.clone());
        }
        self.job_capacity()?;
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

    fn continue_session(&mut self, command: &Command) -> Result<()> {
        let id = self.command_session(command)?;
        self.reachable(id)?;
        if !self.hosts(&id)
            && self
                .connections
                .get(&id)
                .is_none_or(RemoteTerminal::is_closed)
        {
            return self.open_remote(id, command.clone());
        }
        self.job_capacity()?;
        let typed = self.target(id)?.type_line(GO_ON.to_owned(), true);
        let command = command.clone();
        self.spawn(async move {
            Completion::Terminal {
                command,
                result: typed.await,
            }
        });
        Ok(())
    }

    fn show_changes(&mut self, command: &Command) -> Result<()> {
        let id = self.command_session(command)?;
        self.reachable(id)?;
        self.job_capacity()?;
        #[cfg(feature = "host")]
        if self.hosts(&id) {
            let shown = self.changes_shown(id)?;
            let command = command.clone();
            self.spawn(async move {
                Completion::Terminal {
                    command,
                    result: shown.await,
                }
            });
            return Ok(());
        }
        if self
            .connections
            .get(&id)
            .is_none_or(RemoteTerminal::is_closed)
        {
            return self.open_remote(id, command.clone());
        }
        let asked = self.target(id)?.ask_changes();
        let command = command.clone();
        self.spawn(async move {
            Completion::Terminal {
                command,
                result: asked.await,
            }
        });
        Ok(())
    }

    /// The checks of a change to the people of a terminal that this computer makes before it asks
    /// the owner, and again before it runs the change.
    #[cfg_attr(
        not(feature = "host"),
        expect(
            clippy::needless_pass_by_ref_mut,
            reason = "only a computer that hosts terminals publishes them"
        )
    )]
    pub(super) fn membership_checks(&mut self, command: &Command) -> Result<Uuid> {
        let id = self.command_session(command)?;
        match command {
            #[cfg(feature = "host")]
            Command::ShareSession {
                expected_user_ids, ..
            } if self.local.contains_key(&id) => {
                if let Some(expected) = expected_user_ids
                    && self.local.get(&id).is_some_and(|local| {
                        expected.iter().collect::<BTreeSet<_>>()
                            != local.entry.shared_with.iter().collect::<BTreeSet<_>>()
                    })
                {
                    return Err(Error::Refused(super::sharing_changed()));
                }
                self.require_publication(id)?;
            }
            Command::ShareSession { .. } => {
                return Err(Error::Refused(super::hosted_elsewhere()));
            }
            Command::AttachSession { .. } => {
                self.require_owner(id)?;
                #[cfg(feature = "host")]
                self.require_publication(id)?;
            }
            _ => {}
        }
        Ok(id)
    }

    fn change_membership(&mut self, command: &Command) -> Result<()> {
        let id = self.membership_checks(command)?;
        match command {
            Command::LeaveSession { .. }
                if self.hosts(&id) || self.remotes.get(&id).is_some_and(|entry| entry.is_owner) =>
            {
                return Err(Error::Refused(text!("You cannot leave your own terminal.")));
            }
            Command::HideSession { .. } if self.hosts(&id) => {
                return Err(Error::Refused(text!(
                    "This terminal runs on this computer."
                )));
            }
            Command::HideSession { .. } if self.host_online(id) => {
                return Err(Error::Text(self.computer_is(id, true)));
            }
            _ => {}
        }
        self.administer(id, command.clone())
    }

    #[cfg(feature = "host")]
    fn require_publication(&mut self, id: Uuid) -> Result<()> {
        if self.local.get(&id).is_some_and(|local| !local.published) {
            self.publish_locals();
            return Err(Error::Refused(super::not_published()));
        }
        Ok(())
    }

    fn require_owner(&self, id: Uuid) -> Result<()> {
        (self.hosts(&id) || self.remotes.get(&id).is_some_and(|entry| entry.is_owner))
            .ok_or_else(|| Error::Refused(text!("Only the owner can change this terminal.")))
    }

    #[cfg(feature = "host")]
    fn create(&mut self, command: Command) -> Result<()> {
        let Command::CreateSession { request_id, .. } = &command else {
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
        let like = self.can_start(&command)?;
        self.begin_creation(command, like, None, None).map(drop)
    }

    /// The checks of a new terminal that this computer makes before it asks the owner, and
    /// again before it starts the terminal.
    #[cfg(feature = "host")]
    pub(super) fn can_start(&self, command: &Command) -> Result<Option<RunningAgent>> {
        let Command::CreateSession { resume, like, .. } = command else {
            return Err(Error::Invalid("expected session creation".to_owned()));
        };
        if let Some(resume) = resume
            && self.local.values().map(|local| &local.creation).chain(self.creating.values())
                .any(|command| matches!(command, Command::CreateSession { resume: Some(active), .. } if active == resume)) {
                return Err(Error::Refused(text!("This provider conversation is already running in Kodosi.")));
            }
        let like = match like {
            Some(terminal) => self.running_agent(terminal)?,
            None => None,
        };
        self.room_for_terminal()?;
        Ok(like)
    }

    #[cfg(feature = "host")]
    fn room_for_terminal(&self) -> Result<()> {
        if self.local.len() + self.creating.len() >= MAX_SESSIONS {
            return Err(Error::Busy);
        }
        self.job_capacity()
    }

    /// Whether a terminal of this computer started for this request already.
    #[cfg(feature = "host")]
    pub(super) fn started_by(&self, request: &str) -> bool {
        self.local
            .values()
            .map(|local| &local.creation)
            .chain(self.creating.values())
            .any(|command| command.request_id() == request)
    }

    #[cfg(not(feature = "host"))]
    #[expect(clippy::unused_self, reason = "a viewer starts no terminals")]
    pub(super) const fn started_by(&self, _request: &str) -> bool {
        false
    }

    #[cfg(feature = "host")]
    fn running_agent(&self, terminal: &str) -> Result<Option<RunningAgent>> {
        let local = self
            .local
            .get(&parse_id(terminal)?)
            .filter(|local| local.comes_back())
            .ok_or(Error::NotFound)?;
        Ok(local
            .entry
            .program
            .as_deref()
            .and_then(crate::agents::Agent::named)
            .map(|agent| RunningAgent {
                agent,
                account_folder: local.account_folder.clone(),
                settings: local.entry.program_settings.clone(),
                pending: local.entry.program_pending.clone(),
            }))
    }

    #[cfg(feature = "host")]
    pub(super) fn begin_creation(
        &mut self,
        command: Command,
        like: Option<RunningAgent>,
        screen: Option<PathBuf>,
        quiet_settings: Option<PathBuf>,
    ) -> Result<Uuid> {
        self.room_for_terminal()?;
        let id = Uuid::now_v7();
        let config = self.config.clone();
        let changes = self.changes.clone();
        let launcher = Arc::clone(&self.launcher);
        let services = self.services.clone();
        let chat = self.links.room(id);
        let look = self.look.watch();
        self.creating.insert(id, command.clone());
        self.spawn(async move {
            let result = create_session(
                id,
                config,
                services,
                chat,
                look,
                &launcher,
                &command,
                like,
                screen,
                quiet_settings,
                changes,
            )
            .await;
            Completion::Created { id, result }
        });
        Ok(id)
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
        if !self.host_online(id) {
            return self.await_host(id, &command);
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
        let keeps_screen = self.keeps_screen(id);
        if let Some(entry) = self.remotes.get_mut(&id) {
            if let Ok(incarnation) = Uuid::parse_str(&entry.incarnation_id)
                && self.views.contains_key(&id)
            {
                self.reconnect.entry(id).or_insert_with(|| Reconnect {
                    incarnation,
                    failures: 0,
                    next: tokio::time::Instant::now() + Duration::from_secs(2),
                    connected: None,
                    failure: crate::diagnostics::Repeats::new(),
                });
            }
            if !keeps_screen {
                entry.connection_state = ConnectionState::Connecting;
            }
            entry
                .message
                .take_if(|message| *message != input_not_sent());
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

    fn await_host(&mut self, id: Uuid, command: &Command) -> Result<()> {
        if !matches!(command, Command::OpenRemote { .. }) {
            return Err(self.offline(id));
        }
        if self.views.contains_key(&id)
            && let Some(incarnation) = self
                .remotes
                .get(&id)
                .and_then(|entry| Uuid::parse_str(&entry.incarnation_id).ok())
        {
            self.reconnect.entry(id).or_insert_with(|| Reconnect {
                incarnation,
                failures: 0,
                next: tokio::time::Instant::now(),
                connected: None,
                failure: crate::diagnostics::Repeats::new(),
            });
        }
        self.result(command);
        Ok(())
    }

    pub(super) fn reachable(&self, id: Uuid) -> Result<()> {
        self.host_online(id).ok_or_else(|| self.offline(id))
    }

    pub(super) fn offline(&self, id: Uuid) -> Error {
        Error::Text(self.computer_is(id, false))
    }

    fn computer_is(&self, id: Uuid, online: bool) -> crate::text::Text {
        let host = self
            .remotes
            .get(&id)
            .and_then(|entry| entry.host_name.as_deref());
        match (host, online) {
            (Some(host), true) => text!("{computer} is online.", computer = host),
            (Some(host), false) => text!("{computer} is offline.", computer = host),
            (None, true) => text!("Its computer is online."),
            (None, false) => text!("Its computer is offline."),
        }
    }

    fn disconnect_remote(&mut self, id: Uuid) {
        self.views.remove(&id);
        self.reconnect.remove(&id);
        let waiting = self.opening.remove(&id).map_or_else(Vec::new, |opening| {
            opening.cancellation.cancel();
            opening.commands
        });
        if let Some(connection) = self.connections.remove(&id) {
            connection.disconnect();
        }
        if let Some(entry) = self.remotes.get_mut(&id) {
            entry.connection_state = ConnectionState::Offline;
            entry.connected_users.clear();
        }
        self.show_shared_status(id);
        self.publish_catalog();
        for command in waiting {
            if matches!(command, Command::OpenRemote { .. }) {
                self.result(&command);
            } else if let Err(error) = self.apply(&command) {
                self.error(&command, &error);
            }
        }
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
        self.backend_command(command, true, None);
    }

    pub(super) fn refresh(&mut self, command: Command) {
        self.backend_command(command, false, None);
    }

    /// A friend request goes to the account that this device found before it asked the owner, so
    /// that the server does not look up its user name again.
    pub(super) fn request_friend(&mut self, command: Command, account: String) -> Result<()> {
        self.job_capacity()?;
        self.backend_command(command, true, Some(account));
        Ok(())
    }

    fn backend_command(&mut self, command: Command, requested: bool, account: Option<String>) {
        let network = self.network.clone();
        let generation = self.scope.network_generation;
        self.spawn(async move {
            let result = if network.generation() == generation {
                match serde_json::to_value(&command) {
                    Ok(mut args) => {
                        if let Some(account) = account {
                            args["userId"] = json!(account);
                        }
                        network
                            .execute(command.operation(), args)
                            .await
                            .map_err(Error::from)
                    }
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
            Command::Focus { returning, .. } => Ok(target.focus(connection, true, *returning)),
            Command::Blur { .. } => Ok(target.focus(connection, false, false)),
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
            #[cfg(feature = "host")]
            Completion::Input { result } => {
                if current
                    && let Err(error) = result
                    && !matches!(error, Error::Stopped | Error::NotFound)
                {
                    tracing::warn!(%error, "an answer of Dosi did not reach its terminal");
                }
            }
            Completion::Typed {
                session,
                number,
                reply,
                result,
            } => {
                if let Some(waiting) = self.typing.get_mut(&session) {
                    *waiting = waiting.saturating_sub(1);
                    if *waiting == 0 {
                        self.typing.remove(&session);
                    }
                }
                if current {
                    match &result {
                        Ok(()) => {
                            if self.unsent.get(&session).is_none_or(|last| number > *last) {
                                self.clear_input_notice(session);
                            }
                        }
                        Err(Error::Unsent(_)) => self.note_unsent_input(session),
                        Err(_) => {}
                    }
                }
                match reply {
                    Some(reply) => {
                        drop(reply.send(if current { result } else { Err(Error::Stale) }));
                    }
                    None => {
                        if current
                            && let Err(error) = result
                            && !matches!(error, Error::Stopped | Error::NotFound | Error::Unsent(_))
                        {
                            tracing::warn!(%error, "input did not reach its terminal");
                        }
                    }
                }
            }
            Completion::Initialized(result) => {
                self.initializing = false;
                self.emit(self.auth_event());
                if job.scope.network_generation == self.network.generation()
                    && let Err(error) = result
                    && !matches!(error, Error::Stale)
                {
                    tracing::warn!(%error, "the account did not start");
                }
                #[cfg(feature = "host")]
                self.publish_locals();
            }
            #[cfg(feature = "host")]
            Completion::Created { id, result } => {
                self.created(id, result, current);
                if !self.local.contains_key(&id) {
                    self.launcher.release_quiet(id);
                }
            }
            #[cfg(feature = "host")]
            Completion::Account {
                id,
                group,
                account,
                folder,
                agent,
                outside,
                note,
                settings,
            } => {
                if let Some(local) = self.local.get_mut(&id) {
                    drop(local.terminal.set_account(group, account));
                    let same_program = local.entry.program.as_deref() == Some(agent.name());
                    if same_program {
                        local.account_folder = folder;
                    }
                    let settings = settings.changed(&local.changed);
                    if !settings.is_empty() {
                        drop(local.terminal.set_settings(group, settings));
                    }
                    let outside = outside && same_program;
                    let note = note.filter(|_| same_program && local.entry.face.is_none());
                    if local.entry.program_outside != outside || local.entry.program_note != note {
                        local.entry.program_outside = outside;
                        local.entry.program_note = note;
                        self.publish_catalog();
                    }
                }
            }
            #[cfg(feature = "host")]
            Completion::Conversation { id, program, line } => {
                if let Some(local) = self.local.get_mut(&id) {
                    let current = local.entry.program.as_deref() == Some(program.as_str());
                    local.conversation.found(line.filter(|_| current));
                }
            }
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
                refused,
                result,
            } => self.published(id, incarnation, refused, result, current),
            #[cfg(feature = "host")]
            Completion::Told {
                id,
                mentions,
                result,
            } => self.told(id, mentions, result, current),
            #[cfg(feature = "host")]
            Completion::WentOn { id, result } => {
                if current {
                    self.went_on(id, result);
                }
            }
            #[cfg(feature = "host")]
            Completion::Counted { id, changes } => self.counted(id, changes),
            #[cfg(feature = "host")]
            Completion::ChangesShown { result } => {
                if let Err(error) = result {
                    tracing::debug!(%error, "the diff of a terminal was not shown");
                }
            }
            #[cfg(feature = "host")]
            Completion::AgentTyped { id, line, result } => {
                self.agent_typed(id, line, result, current);
            }
            #[cfg(feature = "host")]
            Completion::AgentChangeDue => {
                if current {
                    self.type_agent_changes();
                }
            }
            #[cfg(feature = "host")]
            Completion::Mentioned { mentions, later } => {
                if current {
                    for (terminal, mention) in mentions {
                        if self.local.contains_key(&terminal) {
                            self.mentions.keep(terminal, mention);
                        }
                    }
                    for unchecked in later {
                        if self.local.contains_key(&unchecked.terminal) {
                            self.mentions.wait_for_check(unchecked);
                        }
                    }
                    self.tell_mentions();
                }
            }
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
            Completion::BackgroundDue { command, pausing } => {
                if pausing.is_cancelled() {
                    self.result(&command);
                } else {
                    self.pausing = None;
                    let paused = self.network.pause();
                    self.spawn(async move {
                        paused.await;
                        Completion::Paused { command }
                    });
                }
            }
            Completion::Paused { command } => self.result(&command),
            Completion::Terminal { command, result } if current => {
                match result {
                    Ok(()) => self.result(&command),
                    Err(_) if self.closed_and_gone(&command) => self.result(&command),
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
            #[cfg(feature = "host")]
            Completion::Repositories { command, result } if current => match result {
                Ok(repositories) => {
                    self.emit(json!({"type":"repositories.result", "requestId":command.request_id(), "operation":command.operation(), "repositories":repositories}));
                }
                Err(error) => self.error(&command, &error),
            },
            #[cfg(feature = "host")]
            Completion::SleepChecked { command, result } if current => {
                self.sleep_checked(&command, result);
            }
            #[cfg(feature = "host")]
            Completion::Slept { result, slept } => self.slept(&result, slept),
            #[cfg(feature = "host")]
            Completion::Folders { command, result } if current => match result {
                Ok(folders) => {
                    self.emit(json!({"type":"folders.result", "requestId":command.request_id(), "operation":command.operation(), "folders":folders}));
                }
                Err(error) => self.error(&command, &error),
            },
            Completion::Asked {
                command,
                computer,
                result,
            } if current => {
                if let Err(error) =
                    result.and_then(|answer| self.asked(&command, &computer, answer))
                {
                    self.error(&command, &error);
                }
            }
            Completion::Checked { held, facts } => self.checked(*held, &facts, current),
            Completion::WakeDescribed {
                device,
                number,
                reported,
                result,
            } if current => self.wake_described(device, number, reported.as_ref(), result),
            Completion::OwnNetwork { own, connects } => self.own_network_read(own, connects),
            Completion::Roused {
                device,
                details,
                roused,
            } if current => self.roused(&device, &details, &roused),
            Completion::WakeTested {
                device,
                details,
                tested,
            } if current => self.wake_tested(&device, &details, tested),
            Completion::WakeDescribed { .. }
            | Completion::Roused { .. }
            | Completion::WakeTested { .. } => {}
            #[cfg(feature = "host")]
            Completion::WakeRead { command, own } if current => self.wake_read(&command, own),
            #[cfg(feature = "host")]
            Completion::WakeSignaled { command, signal } if current => {
                self.wake_signaled(&command, signal);
            }
            Completion::TerminalFolders {
                device,
                asked,
                result,
            } if current => self.folders_answered(&device, asked, result),
            Completion::TerminalFolders { .. } => {}
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
            Completion::Canvas { command, result } if current => match result {
                Ok(answer) => {
                    #[cfg(feature = "host")]
                    self.sync_canvas(command.session_id().unwrap_or_default());
                    let mut value = super::answer(&command, Ok(()));
                    value["answer"] = json!(answer);
                    self.emit(value);
                }
                Err(message) => self.error(&command, &Error::Text(message)),
            },
            Completion::Canvas { command, .. } => self.error(&command, &Error::Stale),
            #[cfg(feature = "host")]
            Completion::Settings { command, result } => match result {
                Ok(fields) => {
                    let mut value = super::answer(&command, Ok(()));
                    if let (Some(value), Value::Object(fields)) = (value.as_object_mut(), fields) {
                        value.extend(fields);
                    }
                    self.emit(value);
                }
                Err(message) => self.error(&command, &Error::Text(message)),
            },
            Completion::Document {
                pipe,
                attempt,
                result,
            } => self.document_opened(pipe, attempt, result, current),
            #[cfg(feature = "host")]
            Completion::Provider { command, .. }
            | Completion::Folders { command, .. }
            | Completion::Repositories { command, .. }
            | Completion::SleepChecked { command, .. }
            | Completion::WakeRead { command, .. }
            | Completion::WakeSignaled { command, .. } => {
                self.error(&command, &Error::Stale);
            }
            Completion::Asked { command, .. } => self.error(&command, &Error::Stale),
        }
    }

    #[cfg(feature = "host")]
    #[expect(clippy::too_many_lines, reason = "the parts of one new terminal")]
    fn created(&mut self, id: Uuid, result: Result<StartedSession>, current: bool) {
        let Some(command) = self.creating.remove(&id) else {
            return;
        };
        let restored = self.restore.starting.remove(&id);
        if let Some(saved) = &restored {
            self.restore.forget_screen(saved.id);
        }
        let unfinished = restored
            .as_ref()
            .and_then(|saved| self.restore.unfinished(saved));
        let result = if current || restored.is_some() {
            result
        } else {
            Err(Error::Stale)
        };
        if let (Some(saved), Err(error)) = (&restored, &result) {
            tracing::warn!(%error, name = %saved.name, "a terminal did not come back");
        }
        match result {
            Ok(mut started) => {
                let Some(terminal) = started.terminal.take() else {
                    return;
                };
                let Command::CreateSession {
                    request_id,
                    name,
                    project_id,
                    ..
                } = &command
                else {
                    terminal.cancel();
                    return;
                };
                if terminal.is_closed() {
                    self.error(
                        &command,
                        &Error::Text(text!("The process exited before its terminal was ready.")),
                    );
                    return;
                }
                let entry = SessionEntry {
                    attention: false,
                    connected_users: vec![],
                    program: None,
                    program_account: None,
                    program_outside: false,
                    program_note: None,
                    program_settings: None,
                    program_live: None,
                    program_pending: None,
                    prompt: false,
                    repository: started.repository,
                    title: None,
                    title_program: None,
                    program_status: None,
                    working: false,
                    waiting: None,
                    activity: None,
                    context: None,
                    canvas: None,
                    face: None,
                    helpers: 0,
                    look: self.look.now(),
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
                    stalled: false,
                    direct: false,
                    hidden: false,
                    message: None,
                    project_id: project_id.clone(),
                    project_name: None,
                    shared_with: vec![],
                    restored_from: restored.as_ref().map(|saved| saved.id.to_string()),
                    stopped: false,
                    changes: None,
                };
                let created = json!({"type":"session.result", "requestId":request_id, "sessionId":id, "operation":"session.create"});
                if let Some(folder) = &entry.working_dir {
                    self.recent.remember(folder);
                }
                if let Some(saved) = &restored {
                    terminal.canvas.restore(
                        saved.canvas.iter().map(PathBuf::from).collect(),
                        saved.browser.clone(),
                        saved.screen,
                        saved.id,
                    );
                }
                let (kept, wanted) = restored.map_or((None, None), |saved| {
                    (
                        saved
                            .worktree
                            .filter(|worktree| started.directory.starts_with(&worktree.path)),
                        saved.sharing,
                    )
                });
                self.local.insert(
                    id,
                    Local {
                        terminal,
                        entry,
                        creation: command,
                        published: false,
                        project_sequence: 0,
                        worktree: started.worktree.take().or(kept),
                        wanted,
                        conversation: super::restore::Conversation::default(),
                        changed: crate::agents::AgentSettings::default(),
                        account_folder: None,
                        marked: tokio::sync::watch::channel(None).0,
                        counted: None,
                        counting: false,
                    },
                );
                match unfinished {
                    Some(super::restore::Unfinished::Continue) => {
                        self.restart
                            .continue_when_ready(id, tokio::time::Instant::now());
                    }
                    Some(super::restore::Unfinished::Offer) => self.offer_continue(id),
                    None => {}
                }
                self.count_branch(id);
                self.apply_wanted();
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
        let own_retirement = (changes_account || matches!(command, Command::CancelLogin { .. }))
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
                if matches!(command, Command::RefreshDevices { .. })
                    && let Some(hosts) = self.cached_events.get("devices.hosts").cloned()
                {
                    self.emit(hosts);
                }
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
                            failure: crate::diagnostics::Repeats::new(),
                        },
                    );
                }
                let notice = self
                    .remotes
                    .get(&id)
                    .and_then(|entry| entry.message.clone())
                    .filter(|message| *message == input_not_sent());
                let mut entry = self.remote_entry(self.listed_remote(&connection.session));
                entry.connection_state = ConnectionState::Connected;
                entry.status = SessionStatus::Running;
                entry.stalled = false;
                entry.direct = false;
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
                if let Some(connection) = self.connections.get(&id) {
                    connection.show(!self.views.contains_key(&id) || self.shown(id));
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
            Err(error) => self.connection_failed(id, opening.commands, error),
        }
    }

    fn connection_failed(&mut self, id: Uuid, commands: Vec<Command>, error: Error) {
        if self
            .reconnect
            .get_mut(&id)
            .is_none_or(|retry| retry.failure.new_failure(&error))
        {
            tracing::warn!(%id, %error, "remote terminal connection failed");
        }
        let blocked = matches!(error, Error::Invalid(_) | Error::Refused(_));
        if blocked {
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
        let kept = self.reconnect.contains_key(&id)
            && self.views.contains_key(&id)
            && self.keeps_screen(id);
        let online = self.host_online(id);
        let status = self.remote_status(id, online);
        if let Some(entry) = self.remotes.get_mut(&id) {
            if kept {
                entry.status = status;
            } else {
                entry.connection_state = if blocked {
                    ConnectionState::Blocked
                } else {
                    ConnectionState::Offline
                };
                entry.message = (blocked || online).then(|| error.text());
            }
        }
        self.publish_catalog();
        let error = if blocked || online {
            error
        } else {
            self.offline(id)
        };
        for command in commands {
            if !blocked && !online && matches!(command, Command::OpenRemote { .. }) {
                self.result(&command);
            } else {
                self.error(&command, &error);
            }
        }
    }

    #[cfg(feature = "host")]
    pub(super) fn published(
        &mut self,
        id: Uuid,
        incarnation: Uuid,
        refused: bool,
        result: Result<()>,
        current: bool,
    ) {
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
                if let Some(local) = self.local.get_mut(&id).filter(|local| {
                    refused
                        && (local.entry.project_id.is_some() || !local.entry.shared_with.is_empty())
                }) {
                    tracing::warn!(%id, %error, "the terminal is private now: the server refused its project or people");
                    local.entry.project_id = None;
                    local.entry.project_name = None;
                    local.entry.shared_with.clear();
                    self.publish_catalog();
                }
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
                    local.wanted = None;
                }
                Command::AttachSession { project_id, .. } => {
                    local.entry.project_id.clone_from(project_id);
                    local.entry.project_name = None;
                    local.wanted = None;
                }
                _ => {}
            }
        }
        match command {
            Command::LeaveSession { .. } | Command::CloseSession { .. } => {
                self.remotes.remove(&id);
                self.disconnect_remote(id);
            }
            Command::HideSession { .. } => {
                if let Some(entry) = self.remotes.get_mut(&id) {
                    entry.hidden = true;
                }
            }
            _ => {}
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
        .ok_or_else(|| Error::Refused(text!("The provider CLI is not installed.")))?;
    let arguments = super::restore::resume_words(
        resume.provider.agent(),
        Some(&resume.native_conversation_id),
    )
    .ok_or_else(|| Error::Invalid("This agent continues no conversation.".to_owned()))?;
    Ok((executable, arguments))
}

#[cfg(feature = "host")]
async fn start_line(
    config: &Config,
    launcher: &Arc<Launcher>,
    command: Option<&String>,
    agent: Option<&AgentStart>,
    prompt: Option<&String>,
) -> Result<Option<Launch>> {
    let quoted = prompt.map(|prompt| crate::agents::quote(&shell(config), prompt.trim()));
    let Some(agent) = agent else {
        return Ok(command.map(|line| Launch {
            line: [Some(line.clone()), quoted]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" "),
            quiet_settings: None,
            copilot_home: None,
        }));
    };
    if config.manages_home() && agent.agent == crate::agents::Agent::Antigravity {
        mount_antigravity_plugin(
            config.home.clone(),
            crate::agents::notes_folder(&config.data_root),
        )
        .await;
    }
    let launch = launcher.line(agent).await.map_err(Error::Invalid)?;
    let flag = agent.agent.prompt_flag().map(str::to_owned);
    Ok(Some(Launch {
        line: [Some(launch.line), quoted.as_ref().and(flag), quoted]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" "),
        quiet_settings: launch.quiet_settings,
        copilot_home: launch.copilot_home,
    }))
}

#[cfg(feature = "host")]
async fn start_folder(
    config: &Config,
    working_dir: Option<&str>,
    worktrees: Option<&str>,
) -> Result<PathBuf> {
    let directory = working_dir.map_or_else(|| config.home.clone(), PathBuf::from);
    if !directory.is_absolute() || worktrees.is_some_and(|place| !Path::new(place).is_absolute()) {
        return Err(Error::Refused(text!(
            "Choose a folder by its full path on that computer."
        )));
    }
    let shown = directory.display().to_string();
    let directory = tokio::task::spawn_blocking(move || kodosi_os::fs::canonicalize(&directory))
        .await
        .map_err(std::io::Error::from)?
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => {
                Error::Refused(text!("There is no folder {folder}.", folder = &shown))
            }
            _ => Error::Io(error),
        })?;
    if !tokio::fs::metadata(&directory).await?.is_dir() {
        return Err(Error::Invalid(
            "working directory is not a directory".to_owned(),
        ));
    }
    Ok(directory)
}

#[cfg(feature = "host")]
#[expect(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    reason = "the parts of one new terminal"
)]
async fn create_session(
    id: Uuid,
    config: Config,
    services: crate::canvas::CanvasServices,
    chat: crate::chat::TerminalChat,
    look: tokio::sync::watch::Receiver<crate::terminal::TerminalLook>,
    launcher: &Arc<Launcher>,
    command: &Command,
    like: Option<RunningAgent>,
    screen: Option<PathBuf>,
    quiet_settings: Option<PathBuf>,
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
        like: like_terminal,
        size,
        ..
    } = command
    else {
        return Err(Error::Invalid("expected session creation".to_owned()));
    };
    let grid = size
        .map(crate::protocol::StartSize::geometry)
        .transpose()?
        .unwrap_or_default();
    let directory = start_folder(&config, working_dir.as_deref(), worktrees.as_deref()).await?;
    if like_terminal.is_some()
        && (start.is_some() || agent.is_some() || resume.is_some() || prompt.is_some())
    {
        return Err(Error::Invalid(
            "A terminal like another starts without a command, an agent, a conversation or a prompt."
                .to_owned(),
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
    let agent = match like {
        Some(running) => Some(launcher.start_like(running).await),
        None => agent.clone(),
    };
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
    let screen = match screen {
        Some(file) => tokio::task::spawn_blocking(move || super::restore::earlier_screen(&file))
            .await
            .unwrap_or_default(),
        None => Vec::new(),
    };
    let terminal = match TerminalSession::spawn(
        id,
        Uuid::now_v7(),
        program,
        arguments,
        directory.clone(),
        Some(config.data_root.join("shell")),
        Some(crate::agents::notes_folder(&config.data_root)),
        screen,
        grid,
        crate::canvas::CanvasHandle::spawn(id, &config.data_root, services),
        chat,
        look,
        crate::terminal::pictures_root(&config.data_root).join(id.to_string()),
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
    if let Some(start) = start {
        if let Some(settings) = start.quiet_settings.or(quiet_settings) {
            launcher.hold_quiet(id, settings).await;
        }
        if let Some(home) = start.copilot_home {
            allow_copilot_link(home, directory.clone()).await;
        }
        terminal.start(start.line)?;
    }
    Ok(StartedSession {
        terminal: Some(terminal),
        repository: worktree.is_some() || crate::worktree::is_repository(&directory),
        directory,
        worktree,
    })
}

/// Without the answer in Copilot's file, Copilot asks the person once in the terminal.
#[cfg(feature = "host")]
pub(super) async fn allow_copilot_link(home: PathBuf, folder: PathBuf) {
    let allowed =
        tokio::task::spawn_blocking(move || crate::agents::allow_copilot_link(&home, &folder))
            .await;
    if let Ok(Err(error)) = allowed {
        tracing::warn!(%error, "Copilot will ask before its chat can answer its questions");
    }
}

#[cfg(feature = "host")]
async fn mount_antigravity_plugin(home: PathBuf, notes: PathBuf) {
    let mounted = tokio::task::spawn_blocking(move || {
        crate::agents::mount_antigravity_plugin(&home, &notes, true)
    })
    .await;
    if let Ok(Err(error)) = mounted {
        tracing::warn!(%error, "Kodosi's plugin for Antigravity CLI was not linked");
    }
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
        Command::InspectUsage { .. } => {
            return Ok(serde_json::to_value(
                launcher.usage().await.map_err(Error::Invalid)?,
            )?);
        }
        Command::AddAgentAccount { agent, name, .. } => {
            return Ok(serde_json::to_value(
                launcher
                    .add_account(*agent, name)
                    .await
                    .map_err(Error::Text)?,
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

/// Reads the conversation of a terminal's chat from the first frame of its pipe, which closes
/// again: the chat's agent gets nothing.
async fn read_conversation(
    chat: impl Future<Output = Result<crate::canvas::PipeEnds>>,
) -> Result<String> {
    let mut ends = chat.await?;
    let frame = tokio::time::timeout(CONVERSATION_WAIT, ends.receiver.recv())
        .await
        .map_err(|_| Error::Busy)?
        .ok_or(Error::Stopped)?;
    match serde_json::from_slice(&frame) {
        Ok(crate::chat::ChatFrame::Snapshot(chat)) => crate::chat::transcript(&chat),
        _ => None,
    }
    .ok_or_else(|| {
        Error::Invalid(
            "The agent in this terminal reports no conversation to Kodosi. Read its screen without --conversation."
                .to_owned(),
        )
    })
}
