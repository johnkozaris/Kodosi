use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::PathBuf,
    pin::Pin,
    sync::Arc,
    time::Duration,
};

use bytes::Bytes;
use futures_util::FutureExt as _;
use serde_json::{Value, json};
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore, broadcast, mpsc, oneshot, watch},
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    Command, CommandEnvelope, Config, Error, Event, Result,
    network::{
        LocalPublication, Network, NetworkConfig, NetworkEvent, NetworkReply, RemoteConnection,
        RemoteSession, TerminalControl,
    },
    protocol::{ConnectionState, SessionEntry, SessionKind, SessionStatus},
    terminal::{
        LocalSession, RemoteTerminal, SessionChange, Subscription, TerminalPixelGeometry,
        TerminalSize,
    },
};

mod commands;
#[cfg(test)]
mod tests;

const MAX_SESSIONS: usize = 64;
const MAX_JOBS: usize = 64;
const MAX_PUBLICATIONS_IN_FLIGHT: usize = 8;
const COMMAND_CAPACITY: usize = 128;
const MAX_QUEUED_INPUT_BYTES: usize = 4 * 1024 * 1024;

type Observation = (Vec<Event>, broadcast::Receiver<Event>);

#[derive(Clone)]
pub struct RuntimeHandle {
    commands: mpsc::Sender<Request>,
    events: broadcast::Sender<Event>,
    stopped: watch::Receiver<bool>,
    input_budget: Arc<Semaphore>,
}

enum Request {
    AdmitInput {
        session: Uuid,
        incarnation: Uuid,
        connection: Uuid,
        bytes: Bytes,
        reply: oneshot::Sender<Result<()>>,
        permit: OwnedSemaphorePermit,
    },
    Command(CommandEnvelope),
    TerminalCommand {
        envelope: CommandEnvelope,
        connection: Uuid,
    },
    Snapshot(oneshot::Sender<Result<Vec<Event>>>),
    Observe(oneshot::Sender<Result<Observation>>),
    Subscribe {
        session: Uuid,
        reply: oneshot::Sender<Result<Subscription>>,
    },
    Checkpoint {
        session: Uuid,
        connection: Uuid,
        reply: oneshot::Sender<Result<crate::network::CheckpointCut>>,
    },
    Unsubscribe {
        session: Uuid,
        connection: Uuid,
    },
    Input {
        session: Uuid,
        incarnation: Uuid,
        connection: Uuid,
        bytes: Bytes,
        permit: OwnedSemaphorePermit,
    },
    Resize {
        session: Uuid,
        incarnation: Uuid,
        connection: Uuid,
        size: TerminalSize,
        reply: oneshot::Sender<Result<()>>,
    },
    Shutdown,
}

impl RuntimeHandle {
    pub fn try_send(&self, command: CommandEnvelope) -> Result<()> {
        command.command.validate()?;
        if matches!(
            command.command,
            Command::Resize { .. } | Command::Focus { .. } | Command::Blur { .. }
        ) {
            return Err(Error::Invalid(
                "terminal control requires a current subscription".to_owned(),
            ));
        }
        self.send(Request::Command(command))
    }

    pub fn try_send_terminal(&self, envelope: CommandEnvelope, connection: Uuid) -> Result<()> {
        envelope.command.validate()?;
        if !matches!(
            envelope.command,
            Command::Resize { .. } | Command::Focus { .. } | Command::Blur { .. }
        ) || connection.is_nil()
        {
            return Err(Error::Invalid(
                "expected a subscribed terminal control".to_owned(),
            ));
        }
        self.send(Request::TerminalCommand {
            envelope,
            connection,
        })
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }

    pub async fn observe(&self) -> Result<Observation> {
        let (reply, response) = oneshot::channel();
        self.send(Request::Observe(reply))?;
        response.await.map_err(|_| Error::Stopped)?
    }

    pub async fn snapshot(&self) -> Result<Vec<Event>> {
        let (reply, response) = oneshot::channel();
        self.send(Request::Snapshot(reply))?;
        response.await.map_err(|_| Error::Stopped)?
    }

    pub async fn subscribe_terminal(&self, session_id: Uuid) -> Result<Subscription> {
        let (reply, response) = oneshot::channel();
        self.send(Request::Subscribe {
            session: session_id,
            reply,
        })?;
        response.await.map_err(|_| Error::Stopped)?
    }

    pub fn input(
        &self,
        session_id: Uuid,
        incarnation_id: Uuid,
        connection_id: Uuid,
        bytes: Bytes,
    ) -> Result<()> {
        if bytes.len() > 1024 * 1024 {
            return Err(Error::Invalid("terminal input exceeds 1 MiB".to_owned()));
        }
        let permits = u32::try_from(bytes.len()).map_err(|_| Error::Busy)?;
        let permit = Arc::clone(&self.input_budget)
            .try_acquire_many_owned(permits)
            .map_err(|_| Error::Busy)?;
        self.send(Request::Input {
            session: session_id,
            incarnation: incarnation_id,
            connection: connection_id,
            bytes,
            permit,
        })
    }

    pub async fn admit_input(
        &self,
        session: Uuid,
        incarnation: Uuid,
        connection: Uuid,
        bytes: Bytes,
    ) -> Result<()> {
        if bytes.len() > 1024 * 1024 {
            return Err(Error::Invalid("terminal input exceeds 1 MiB".into()));
        }
        let permits = u32::try_from(bytes.len()).map_err(|_| Error::Busy)?;
        let permit = Arc::clone(&self.input_budget)
            .try_acquire_many_owned(permits)
            .map_err(|_| Error::Busy)?;
        let (reply, response) = oneshot::channel();
        self.send(Request::AdmitInput {
            session,
            incarnation,
            connection,
            bytes,
            reply,
            permit,
        })?;
        tokio::time::timeout(Duration::from_secs(5), response)
            .await
            .map_err(|_| {
                Error::Other(
                    "Input admission was not confirmed; do not retry automatically.".into(),
                )
            })?
            .map_err(|_| Error::Stopped)?
    }

    pub async fn terminal_checkpoint(
        &self,
        session: Uuid,
        connection: Uuid,
    ) -> Result<crate::network::CheckpointCut> {
        let (reply, response) = oneshot::channel();
        self.send(Request::Checkpoint {
            session,
            connection,
            reply,
        })?;
        response.await.map_err(|_| Error::Stopped)?
    }

    pub async fn unsubscribe_terminal(&self, session_id: Uuid, connection_id: Uuid) {
        drop(
            self.commands
                .send(Request::Unsubscribe {
                    session: session_id,
                    connection: connection_id,
                })
                .await,
        );
    }

    pub async fn resize(
        &self,
        session: Uuid,
        incarnation: Uuid,
        connection: Uuid,
        cols: u16,
        rows: u16,
    ) -> Result<()> {
        let size =
            TerminalSize::new(rows, cols).map_err(|error| Error::Invalid(error.to_string()))?;
        let (reply, response) = oneshot::channel();
        self.send(Request::Resize {
            session,
            incarnation,
            connection,
            size,
            reply,
        })?;
        response.await.map_err(|_| Error::Stopped)?
    }

    pub async fn stopped(&self) {
        let mut stopped = self.stopped.clone();
        drop(stopped.wait_for(|stopped| *stopped).await);
    }

    pub async fn shutdown(&self) {
        let mut stopped = self.stopped.clone();
        if *stopped.borrow() {
            return;
        }
        drop(self.commands.send(Request::Shutdown).await);
        drop(stopped.wait_for(|stopped| *stopped).await);
    }

    fn send(&self, request: Request) -> Result<()> {
        if *self.stopped.borrow() {
            return Err(Error::Stopped);
        }
        self.commands
            .try_send(request)
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => Error::Busy,
                mpsc::error::TrySendError::Closed(_) => Error::Stopped,
            })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Scope {
    user: Option<String>,
    epoch: u64,
    network_generation: u64,
}

struct Local {
    terminal: LocalSession,
    entry: SessionEntry,
    creation: Command,
    published: bool,
}

impl Local {
    fn publication(
        &self,
        id: Uuid,
    ) -> (
        LocalPublication,
        mpsc::Sender<crate::network::HostRequest>,
        broadcast::Receiver<crate::network::PublishedFrame>,
    ) {
        (
            LocalPublication {
                session_id: id,
                incarnation_id: self.terminal.incarnation_id,
                name: self.entry.name.clone(),
                room_id: self
                    .entry
                    .room_id
                    .as_deref()
                    .and_then(|id| Uuid::parse_str(id).ok()),
                shared_with: self.entry.shared_with.iter().cloned().collect(),
            },
            self.terminal.host_requests.clone(),
            self.terminal.output(),
        )
    }
}

struct StartedSession {
    terminal: Option<LocalSession>,
    directory: PathBuf,
}

impl Drop for StartedSession {
    fn drop(&mut self) {
        if let Some(terminal) = &self.terminal {
            terminal.cancel();
        }
    }
}

struct Opening {
    attempt: Uuid,
    cancellation: CancellationToken,
    commands: Vec<Command>,
}

struct Job {
    scope: Scope,
    completion: Completion,
}

enum Completion {
    Input {
        result: Result<()>,
    },
    Initialized(Result<()>),
    Created {
        id: Uuid,
        result: Result<StartedSession>,
    },
    Network {
        command: Command,
        generation: u64,
        result: Result<NetworkReply>,
    },
    Connected {
        id: Uuid,
        attempt: Uuid,
        result: Result<RemoteConnection>,
    },
    Published {
        id: Uuid,
        incarnation: Uuid,
        result: Result<()>,
    },
    Unpublished {
        id: Uuid,
        result: Result<()>,
    },
    Terminal {
        command: Command,
        result: Result<()>,
    },
    Provider {
        command: Command,
        result: Result<Value>,
    },
    Checkpoint {
        reply: oneshot::Sender<Result<crate::network::CheckpointCut>>,
        result: Result<crate::network::CheckpointCut>,
    },
    HeadlessControl {
        reply: oneshot::Sender<Result<()>>,
        result: Result<()>,
    },
    Subscribed {
        target: TerminalTarget,
        reply: oneshot::Sender<Result<Subscription>>,
        result: Result<Subscription>,
    },
}

#[derive(Clone)]
enum TerminalTarget {
    Local(LocalSession),
    Remote(RemoteTerminal),
}

impl TerminalTarget {
    fn unsubscribe(&self, connection: Uuid) {
        match self {
            Self::Local(terminal) => terminal.unsubscribe(connection),
            Self::Remote(terminal) => terminal.unsubscribe(connection),
        }
    }

    fn subscribe(&self) -> Pin<Box<dyn Future<Output = Result<Subscription>> + Send>> {
        match self {
            Self::Local(terminal) => Box::pin(terminal.subscribe()),
            Self::Remote(terminal) => Box::pin(terminal.subscribe()),
        }
    }

    fn checkpoint(
        &self,
        connection: Uuid,
    ) -> Pin<Box<dyn Future<Output = Result<crate::network::CheckpointCut>> + Send>> {
        match self {
            Self::Local(terminal) => Box::pin(terminal.checkpoint(connection)),
            Self::Remote(terminal) => Box::pin(terminal.checkpoint(connection)),
        }
    }

    fn focus(
        &self,
        connection: Uuid,
        focused: bool,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        match self {
            Self::Local(terminal) => Box::pin(terminal.focus(connection, focused)),
            Self::Remote(terminal) => {
                let result = terminal.control(Some(connection), TerminalControl::Focus { focused });
                Box::pin(async move { result.await.map(|_| ()) })
            }
        }
    }

    fn end(&self, stop: bool) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        match self {
            Self::Local(terminal) if stop => Box::pin(terminal.stop()),
            Self::Local(terminal) => Box::pin(terminal.interrupt()),
            Self::Remote(terminal) => {
                let result = terminal.control(
                    None,
                    if stop {
                        TerminalControl::Stop
                    } else {
                        TerminalControl::Interrupt
                    },
                );
                Box::pin(async move { result.await.map(|_| ()) })
            }
        }
    }

    fn resize(
        &self,
        connection: Uuid,
        size: TerminalSize,
        geometry: Option<TerminalPixelGeometry>,
        claim: bool,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        match self {
            Self::Local(terminal) => Box::pin(terminal.resize(connection, size, geometry, claim)),
            Self::Remote(terminal) => {
                let result = terminal.control(
                    Some(connection),
                    TerminalControl::Resize {
                        request_id: Uuid::now_v7().to_string(),
                        rows: size.rows(),
                        cols: size.cols(),
                        width_pixels: geometry.map_or(0, TerminalPixelGeometry::width_pixels),
                        height_pixels: geometry.map_or(0, TerminalPixelGeometry::height_pixels),
                        cell_width_pixels: geometry
                            .map_or(0, TerminalPixelGeometry::cell_width_pixels),
                        cell_height_pixels: geometry
                            .map_or(0, TerminalPixelGeometry::cell_height_pixels),
                        claim,
                    },
                );
                Box::pin(async move { result.await.map(|_| ()) })
            }
        }
    }
}

struct Reconnect {
    incarnation: Uuid,
    failures: u32,
    next: tokio::time::Instant,
}

struct Runtime {
    config: Config,
    network: Network,
    events: broadcast::Sender<Event>,
    scope: Scope,
    local: HashMap<Uuid, Local>,
    creating: HashMap<Uuid, Command>,
    remotes: HashMap<Uuid, SessionEntry>,
    connections: HashMap<Uuid, RemoteTerminal>,
    opening: HashMap<Uuid, Opening>,
    reconnect: HashMap<Uuid, Reconnect>,
    publishing: BTreeSet<Uuid>,
    retiring: BTreeSet<Uuid>,
    unpublishing: BTreeSet<Uuid>,
    administering: BTreeSet<Uuid>,
    jobs: JoinSet<Job>,
    changes: mpsc::Sender<SessionChange>,
    dark: bool,
    initializing: bool,
    auth_pending: bool,
    latest_auth: Option<Value>,
    cached_events: BTreeMap<String, Value>,
}

pub async fn start(config: Config) -> Result<RuntimeHandle> {
    let network = Network::new(NetworkConfig {
        api_url: config.backend_url.clone(),
        issuer: config.oidc_issuer.clone(),
        client_id: config.oidc_client_id.clone(),
        scopes: config.oidc_scopes.clone(),
        audience: config.oidc_audience.clone(),
        data_root: config.data_root.clone(),
        secret_service: config.secret_service.clone(),
        isolated: config.isolated,
    })?;
    let network_events = network.events();
    let scope = Scope {
        user: None,
        epoch: 0,
        network_generation: network.generation(),
    };
    let (commands, receiver) = mpsc::channel(COMMAND_CAPACITY);
    let (events, _) = broadcast::channel(256);
    let (changes, change_rx) = mpsc::channel(128);
    let (stopped, stopped_rx) = watch::channel(false);
    let handle = RuntimeHandle {
        commands,
        events: events.clone(),
        stopped: stopped_rx,
        input_budget: Arc::new(Semaphore::new(MAX_QUEUED_INPUT_BYTES)),
    };
    let server = crate::headless::serve(handle.clone(), config.data_root.clone()).await?;
    let runtime = Runtime {
        config,
        network,
        events,
        scope,
        local: HashMap::new(),
        creating: HashMap::new(),
        remotes: HashMap::new(),
        connections: HashMap::new(),
        opening: HashMap::new(),
        reconnect: HashMap::new(),
        publishing: BTreeSet::new(),
        retiring: BTreeSet::new(),
        unpublishing: BTreeSet::new(),
        administering: BTreeSet::new(),
        jobs: JoinSet::new(),
        changes,
        dark: true,
        initializing: true,
        auth_pending: false,
        latest_auth: None,
        cached_events: BTreeMap::new(),
    };
    tokio::spawn(async move {
        runtime
            .run(receiver, change_rx, network_events, server)
            .await;
        let _ = stopped.send(true);
    });
    Ok(handle)
}

impl Runtime {
    fn event(&self, value: Value) -> Result<Event> {
        Event::new(self.scope.user.clone(), self.scope.epoch, value)
    }

    fn emit(&self, value: Value) {
        match self.event(value) {
            Ok(event) => {
                drop(self.events.send(event));
            }
            Err(error) => tracing::error!(%error, "invalid runtime event"),
        }
    }

    fn entries(&self) -> Vec<SessionEntry> {
        let mut entries = self
            .local
            .values()
            .map(|local| local.entry.clone())
            .chain(self.remotes.values().cloned())
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| left.id.cmp(&right.id));
        entries
    }

    fn publish_catalog(&self) {
        self.emit(json!({"type":"sessions.snapshot", "sessions":self.entries()}));
    }

    fn snapshot_events(&self) -> Result<Vec<Event>> {
        let mut values = vec![
            json!({"type":"system.ready", "protocolVersion":crate::protocol::VERSION}),
            self.auth_event(),
            json!({"type":"sessions.snapshot", "sessions":self.entries()}),
        ];
        values.extend(self.cached_events.values().cloned());
        values.into_iter().map(|value| self.event(value)).collect()
    }

    fn auth_event(&self) -> Value {
        if let Some(event) = &self.latest_auth {
            return event.clone();
        }
        if self.initializing {
            return json!({"type":"auth.finalizing"});
        }
        match self.network.identity() {
            Some(identity) => json!({"type":"auth.ready", "userId":identity.user_id,
                "displayName":identity.display_name, "deviceId":identity.device_id,
                "enrolled":identity.enrolled}),
            None => json!({"type":"auth.required", "reason":"signedOut"}),
        }
    }

    fn in_scope(&self, scope: &Scope) -> bool {
        scope == &self.scope
            && scope.network_generation == self.network.generation()
            && scope.user == self.network.identity().map(|identity| identity.user_id)
    }

    fn job_capacity(&self) -> Result<()> {
        if self.jobs.len() < MAX_JOBS {
            Ok(())
        } else {
            Err(Error::Busy)
        }
    }

    fn spawn(&mut self, future: impl Future<Output = Completion> + Send + 'static) {
        let scope = self.scope.clone();
        self.jobs.spawn(async move {
            Job {
                scope,
                completion: future.await,
            }
        });
    }

    async fn run(
        mut self,
        mut requests: mpsc::Receiver<Request>,
        mut changes: mpsc::Receiver<SessionChange>,
        mut network_events: broadcast::Receiver<NetworkEvent>,
        server: crate::headless::HeadlessServer,
    ) {
        self.emit(json!({"type":"system.ready", "protocolVersion":crate::protocol::VERSION}));
        let network = self.network.clone();
        self.spawn(async move {
            Completion::Initialized(network.initialize().await.map_err(Error::from))
        });
        let mut retry = tokio::time::interval(Duration::from_secs(5));
        retry.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            if self.synchronize_account().is_err() {
                break;
            }
            tokio::select! {
                request = requests.recv() => {
                    if self.synchronize_account().is_err() { break; }
                    match request {
                        None | Some(Request::Shutdown) => break,
                        Some(Request::Command(envelope)) => {
                            if envelope.account_epoch != self.scope.epoch || envelope.account_user_id != self.scope.user {
                                self.error(&envelope.command, &Error::Stale);
                            } else if matches!(envelope.command, Command::Shutdown {}) {
                                break;
                            } else if let Err(error) = self.apply(&envelope.command) {
                                self.error(&envelope.command, &error);
                            }
                        }
                        Some(Request::TerminalCommand { envelope, connection }) => {
                            if envelope.account_epoch != self.scope.epoch || envelope.account_user_id != self.scope.user {
                                self.terminal_result(&envelope.command, Err(Error::Stale));
                            } else if let Err(error) = self.terminal_command(envelope.command.clone(), connection) {
                                self.terminal_result(&envelope.command, Err(error));
                            }
                        }
                        Some(request) => self.request(request),
                    }
                }
                change = changes.recv() => {
                    if let Some(change) = change { self.change(change); }
                }
                event = network_events.recv() => {
                    match event {
                        Ok(event) => self.network_event(event),
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            for command in [Command::RefreshAuth {}, Command::RefreshDevices {}, Command::RefreshFriends {}, Command::ListRooms {}, Command::ListSessions {}] {
                                self.network_command(command);
                            }
                        },
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
                job = self.jobs.join_next(), if !self.jobs.is_empty() => {
                    match job {
                        Some(Ok(job)) => self.complete(job),
                        Some(Err(error)) => {
                            self.emit(json!({"type":"system.error", "message":format!("Runtime operation failed: {error}")}));
                            break;
                        }
                        None => {},
                    }
                }
                _ = retry.tick() => { self.publish_locals(); self.retire_publications(); self.reconnect_views(); }
            }
        }
        requests.close();
        self.jobs.abort_all();
        while let Some(job) = self.jobs.join_next().await {
            if let Ok(Job {
                completion:
                    Completion::Created {
                        result: Ok(started),
                        ..
                    },
                ..
            }) = job
            {
                drop(started);
            }
        }
        for opening in self.opening.values() {
            opening.cancellation.cancel();
        }
        for terminal in self.connections.values() {
            terminal.disconnect();
        }
        self.connections.clear();
        let mut stopping = JoinSet::new();
        for local in self.local.values() {
            let terminal = local.terminal.clone();
            stopping.spawn(async move {
                if terminal.stop().await.is_err() {
                    terminal.cancel();
                }
            });
        }
        while stopping.join_next().await.is_some() {}
        self.network.shutdown().await;
        server.shutdown().await;
    }

    fn admit_input(
        &mut self,
        session: Uuid,
        incarnation: Uuid,
        connection: Uuid,
        bytes: Bytes,
        reply: oneshot::Sender<Result<()>>,
        permit: OwnedSemaphorePermit,
    ) {
        let target = self
            .job_capacity()
            .and_then(|()| self.check_incarnation(session, incarnation))
            .and_then(|()| self.target(session));
        match target {
            Ok(target) => {
                let admission = match target {
                    TerminalTarget::Local(terminal) => {
                        terminal.admit_input(connection, bytes).boxed()
                    }
                    TerminalTarget::Remote(terminal) => {
                        terminal.admit_input(connection, bytes).boxed()
                    }
                };
                self.spawn(async move {
                    let result = admission.await;
                    drop(permit);
                    Completion::HeadlessControl { reply, result }
                });
            }
            Err(error) => {
                drop(reply.send(Err(error)));
            }
        }
    }

    fn input(
        &mut self,
        session: Uuid,
        incarnation: Uuid,
        connection: Uuid,
        bytes: Bytes,
        permit: OwnedSemaphorePermit,
    ) {
        let result = self
            .job_capacity()
            .and_then(|()| self.check_incarnation(session, incarnation))
            .and_then(|()| self.target(session));
        match result {
            Ok(target) => {
                let admission = match target {
                    TerminalTarget::Local(terminal) => {
                        terminal.admit_input(connection, bytes).boxed()
                    }
                    TerminalTarget::Remote(terminal) => {
                        terminal.admit_input(connection, bytes).boxed()
                    }
                };
                self.spawn(async move {
                    let result = admission.await;
                    drop(permit);
                    Completion::Input { result }
                });
            }
            Err(error) => self.emit(json!({"type":"system.error", "message":error.to_string()})),
        }
    }

    fn request(&mut self, request: Request) {
        match request {
            Request::Snapshot(reply) => {
                drop(reply.send(self.snapshot_events()));
            }
            Request::Observe(reply) => {
                let receiver = self.events.subscribe();
                drop(reply.send(self.snapshot_events().map(|events| (events, receiver))));
            }
            Request::Subscribe { session, reply } => {
                let target = self.job_capacity().and_then(|()| self.target(session));
                match target {
                    Ok(target) => {
                        let response = target.subscribe();
                        self.spawn(async move {
                            let result = response.await;
                            Completion::Subscribed {
                                target,
                                reply,
                                result,
                            }
                        });
                    }
                    Err(error) => {
                        drop(reply.send(Err(error)));
                    }
                }
            }
            Request::Checkpoint {
                session,
                connection,
                reply,
            } => match self.job_capacity().and_then(|()| self.target(session)) {
                Ok(target) => {
                    let response = target.checkpoint(connection);
                    self.spawn(async move {
                        Completion::Checkpoint {
                            reply,
                            result: response.await,
                        }
                    });
                }
                Err(error) => {
                    drop(reply.send(Err(error)));
                }
            },
            Request::Unsubscribe {
                session,
                connection,
            } => {
                if let Ok(target) = self.target(session) {
                    target.unsubscribe(connection);
                }
            }
            Request::AdmitInput {
                session,
                incarnation,
                connection,
                bytes,
                reply,
                permit,
            } => {
                self.admit_input(session, incarnation, connection, bytes, reply, permit);
            }
            Request::Input {
                session,
                incarnation,
                connection,
                bytes,
                permit,
            } => self.input(session, incarnation, connection, bytes, permit),
            Request::Resize {
                session,
                incarnation,
                connection,
                size,
                reply,
            } => {
                let target = self
                    .job_capacity()
                    .and_then(|()| self.check_incarnation(session, incarnation))
                    .and_then(|()| self.target(session));
                match target {
                    Ok(target) => {
                        let response = target.resize(connection, size, None, true);
                        self.spawn(async move {
                            Completion::HeadlessControl {
                                reply,
                                result: response.await,
                            }
                        });
                    }
                    Err(error) => {
                        drop(reply.send(Err(error)));
                    }
                }
            }
            Request::Command(_) | Request::TerminalCommand { .. } | Request::Shutdown => {}
        }
    }

    fn target(&self, id: Uuid) -> Result<TerminalTarget> {
        self.local.get(&id).map_or_else(
            || {
                self.connections
                    .get(&id)
                    .filter(|terminal| !terminal.is_closed())
                    .cloned()
                    .map(TerminalTarget::Remote)
                    .ok_or(Error::NotFound)
            },
            |local| Ok(TerminalTarget::Local(local.terminal.clone())),
        )
    }

    fn check_incarnation(&self, id: Uuid, incarnation: Uuid) -> Result<()> {
        let expected = self
            .local
            .get(&id)
            .map(|local| local.terminal.incarnation_id)
            .or_else(|| {
                self.remotes
                    .get(&id)
                    .and_then(|entry| Uuid::parse_str(&entry.incarnation_id).ok())
            })
            .ok_or(Error::NotFound)?;
        if expected == incarnation {
            Ok(())
        } else {
            Err(Error::Stale)
        }
    }

    fn error(&self, command: &Command, error: &Error) {
        let operation = command.operation();
        let kind = match operation.split('.').next() {
            Some("session") => "session.error",
            Some("provider") => "provider.error",
            Some("room") => "room.error",
            Some("friends") => "friends.error",
            Some("devices") => "devices.error",
            Some("auth") => "auth.error",
            _ => "system.error",
        };
        if kind == "system.error" {
            self.emit(json!({"type":kind, "message":error.to_string()}));
            return;
        }
        let mut value = json!({"type":kind, "operation":operation, "message":error.to_string()});
        if let Some(request) = command.request_id() {
            value["requestId"] = json!(request);
        }
        if kind == "session.error"
            && let Some(session) = command.session_id()
        {
            value["sessionId"] = json!(session);
        }
        if kind == "room.error"
            && let Ok(args) = serde_json::to_value(command)
            && let Some(room) = args.get("roomId")
        {
            value["roomId"] = room.clone();
        }
        self.emit(value);
    }

    fn result(&self, command: &Command) {
        self.emit(
            json!({"type":"session.result", "operation":command.operation(),
            "requestId":command.request_id(), "sessionId":command.session_id()}),
        );
    }

    fn synchronize_account(&mut self) -> Result<()> {
        let user = self.network.identity().map(|identity| identity.user_id);
        let generation = self.network.generation();
        if user == self.scope.user && generation == self.scope.network_generation {
            return Ok(());
        }
        self.scope = Scope {
            user: user.clone(),
            network_generation: generation,
            epoch: self.scope.epoch.checked_add(1).ok_or(Error::Stopped)?,
        };
        for connection in self.connections.values() {
            connection.disconnect();
        }
        for opening in self.opening.values() {
            opening.cancellation.cancel();
        }
        self.connections.clear();
        self.opening.clear();
        self.reconnect.clear();
        self.remotes.clear();
        self.cached_events.clear();
        self.latest_auth = None;
        self.administering.clear();
        self.publishing.clear();
        self.retiring.clear();
        self.unpublishing.clear();
        for local in self.local.values_mut() {
            local.published = false;
            local.entry.shared_with.clear();
            local.entry.room_id = None;
            local.entry.room_name = None;
            local.entry.owner_user_id.clone_from(&user);
            local.entry.host_device_id = None;
        }
        self.emit(self.auth_event());
        self.publish_catalog();
        Ok(())
    }

    fn network_event(&mut self, event: NetworkEvent) {
        if event.generation != self.network.generation()
            || event.user_id != self.network.identity().map(|identity| identity.user_id)
        {
            return;
        }
        if self.synchronize_account().is_err() {
            return;
        }
        let kind = event
            .event
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if kind == "network.sharing" {
            let id = event
                .event
                .get("sessionId")
                .and_then(Value::as_str)
                .and_then(|id| Uuid::parse_str(id).ok());
            let incarnation = event
                .event
                .get("incarnationId")
                .and_then(Value::as_str)
                .and_then(|id| Uuid::parse_str(id).ok());
            let users = event
                .event
                .get("sharedWith")
                .cloned()
                .and_then(|users| serde_json::from_value::<Vec<String>>(users).ok());
            if let (Some(id), Some(incarnation), Some(users)) = (id, incarnation, users)
                && let Some(local) = self
                    .local
                    .get_mut(&id)
                    .filter(|local| local.terminal.incarnation_id == incarnation)
            {
                local.entry.shared_with = users;
                self.publish_catalog();
            }
        } else if kind == "network.sessions" {
            let result = event
                .event
                .get("sessions")
                .ok_or_else(|| Error::Invalid("session catalog missing".to_owned()))
                .and_then(|value| {
                    serde_json::from_value::<Vec<RemoteSession>>(value.clone()).map_err(Error::from)
                });
            match result {
                Ok(remotes) => self.replace_remotes(remotes),
                Err(error) => {
                    self.emit(json!({"type":"system.error", "message":error.to_string()}));
                }
            }
        } else {
            match kind {
                "auth.device_code" | "auth.finalizing" => {
                    self.latest_auth = Some(event.event.clone());
                }
                "auth.ready" | "auth.required" | "auth.error" => self.latest_auth = None,
                _ => {}
            }
            if matches!(
                kind,
                "friends.snapshot" | "devices.list" | "devices.link.snapshot" | "rooms.snapshot"
            ) {
                self.cached_events
                    .insert(kind.to_owned(), event.event.clone());
            }
            self.emit(event.event);
        }
    }

    fn network_reply(&mut self, reply: NetworkReply) {
        for event in reply.events {
            self.network_event(NetworkEvent {
                generation: reply.generation,
                user_id: reply.user_id.clone(),
                event,
            });
        }
    }

    fn replace_remotes(&mut self, remotes: Vec<RemoteSession>) {
        for remote in &remotes {
            if let Some(local) = self
                .local
                .get_mut(&remote.id)
                .filter(|local| local.terminal.incarnation_id == remote.incarnation_id)
                && !self.administering.contains(&remote.id)
                && !self.publishing.contains(&remote.id)
            {
                local.entry.name.clone_from(&remote.name);
                local.entry.room_id = remote.room_id.map(|id| id.to_string());
                local.entry.room_name.clone_from(&remote.room_name);
                local
                    .entry
                    .shared_with
                    .retain(|user| remote.shared_with.contains(user));
            }
        }
        let desired = remotes
            .iter()
            .map(|entry| (entry.id, entry.incarnation_id))
            .collect::<HashMap<_, _>>();
        let retired = self
            .reconnect
            .iter()
            .filter(|(id, retry)| desired.get(id) != Some(&retry.incarnation))
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        for id in retired {
            self.reconnect.remove(&id);
            if self
                .opening
                .get(&id)
                .is_some_and(|opening| opening.commands.is_empty())
                && let Some(opening) = self.opening.remove(&id)
            {
                opening.cancellation.cancel();
            }
        }
        self.connections.retain(|id, connection| {
            let keep =
                desired.get(id) == Some(&connection.incarnation_id) && !connection.is_closed();
            if !keep {
                connection.disconnect();
            }
            keep
        });
        self.remotes = remotes
            .into_iter()
            .filter(|entry| !self.local.contains_key(&entry.id))
            .map(|entry| (entry.id, self.remote_entry(entry)))
            .collect();
        self.publish_catalog();
    }

    fn remote_entry(&self, remote: RemoteSession) -> SessionEntry {
        let is_owner = self.scope.user.as_deref() == Some(remote.owner_user_id.as_str());
        let connection_state = if self
            .connections
            .get(&remote.id)
            .is_some_and(|connection| !connection.is_closed())
        {
            ConnectionState::Connected
        } else if self.opening.contains_key(&remote.id) {
            ConnectionState::Connecting
        } else {
            ConnectionState::Offline
        };
        let previous = self
            .remotes
            .get(&remote.id)
            .filter(|entry| entry.incarnation_id == remote.incarnation_id.to_string());
        SessionEntry {
            connected_users: if connection_state == ConnectionState::Connected {
                previous.map_or_else(Vec::new, |entry| entry.connected_users.clone())
            } else {
                vec![]
            },
            program: previous.and_then(|entry| entry.program.clone()),
            title: previous.and_then(|entry| entry.title.clone()),
            id: remote.id.to_string(),
            incarnation_id: remote.incarnation_id.to_string(),
            kind: SessionKind::Remote,
            name: remote.name,
            working_dir: previous.and_then(|entry| entry.working_dir.clone()),
            owner_user_id: Some(remote.owner_user_id),
            owner_name: Some(remote.owner_name),
            host_device_id: Some(remote.host_device_id),
            host_name: Some(remote.host_name),
            is_owner,
            status: if remote.online {
                SessionStatus::Running
            } else {
                SessionStatus::Reconnecting
            },
            connection_state,
            message: self
                .remotes
                .get(&remote.id)
                .and_then(|entry| entry.message.clone()),
            room_id: remote.room_id.map(|id| id.to_string()),
            room_name: remote.room_name,
            shared_with: remote.shared_with,
            create_request_id: None,
        }
    }

    fn change(&mut self, change: SessionChange) {
        match change {
            SessionChange::RemoteEnded { id, instance_id } => {
                if self
                    .connections
                    .get(&id)
                    .is_some_and(|connection| connection.instance_id == instance_id)
                {
                    self.connections.remove(&id);
                    self.reconnect.remove(&id);
                    self.remotes.remove(&id);
                    self.publish_catalog();
                }
            }
            SessionChange::Presence { id, users } => {
                if let Some(local) = self.local.get_mut(&id) {
                    local.entry.connected_users = users;
                    self.publish_catalog();
                }
            }
            SessionChange::Cwd { id, path } => {
                if let Some(local) = self.local.get_mut(&id) {
                    local.entry.working_dir = Some(path.to_string_lossy().into_owned());
                    self.publish_catalog();
                }
            }
            SessionChange::Program { id, program } => {
                if let Some(local) = self.local.get_mut(&id) {
                    local.entry.program = program;
                    self.publish_catalog();
                }
            }
            SessionChange::Title { id, title } => {
                if let Some(local) = self.local.get_mut(&id) {
                    local.entry.title = Some(title.clone());
                    self.publish_catalog();
                }
                self.emit(json!({"type":"term.title", "sessionId":id, "title":title}));
            }
            SessionChange::Bell { id } => self.emit(json!({"type":"term.bell", "sessionId":id})),
            SessionChange::Notification { id, title, body } => self.emit(
                json!({"type":"term.notification", "sessionId":id, "title":title, "body":body}),
            ),
            SessionChange::Ended { id, reason } => {
                if self.local.remove(&id).is_some_and(|local| local.published)
                    || self.publishing.contains(&id)
                {
                    self.retiring.insert(id);
                }
                self.publish_catalog();
                self.retire_publications();
                tracing::info!(%id, %reason, "session ended");
            }
            SessionChange::Metadata {
                id,
                incarnation,
                instance_id,
                metadata,
            } => {
                if self.connections.get(&id).is_some_and(|connection| {
                    connection.incarnation_id == incarnation
                        && connection.instance_id == instance_id
                }) && let Some(entry) = self.remotes.get_mut(&id)
                {
                    entry.connected_users = metadata.connected_users;
                    entry.working_dir = metadata.directory;
                    entry.title = metadata.title;
                    entry.program = metadata.program;
                    self.publish_catalog();
                }
            }
            SessionChange::RemoteClosed {
                id,
                incarnation,
                instance_id,
                reason,
            } => {
                if self.connections.get(&id).is_some_and(|connection| {
                    connection.incarnation_id == incarnation
                        && connection.instance_id == instance_id
                }) {
                    self.connections.remove(&id);
                    if let Some(entry) = self.remotes.get_mut(&id) {
                        entry.connection_state = ConnectionState::Offline;
                        entry.connected_users.clear();
                        entry.message = Some(reason);
                    }
                    self.publish_catalog();
                }
            }
        }
    }

    fn reconnect_views(&mut self) {
        let now = tokio::time::Instant::now();
        let ids = self
            .reconnect
            .iter()
            .filter(|(id, retry)| {
                retry.next <= now
                    && !self.connections.contains_key(id)
                    && !self.opening.contains_key(id)
            })
            .map(|(id, _)| *id)
            .take(4)
            .collect::<Vec<_>>();
        for id in ids {
            let command = Command::OpenRemote {
                request_id: Uuid::now_v7().to_string(),
                session_id: id.to_string(),
            };
            if self.open_remote(id, command).is_ok()
                && let Some(opening) = self.opening.get_mut(&id)
            {
                opening.commands.clear();
            }
        }
    }

    fn refresh_catalog(&mut self) {
        if self.scope.user.is_some() && self.job_capacity().is_ok() {
            self.network_command(Command::ListSessions {});
        }
    }

    fn publish_locals(&mut self) {
        let Some(identity) = self.network.identity() else {
            return;
        };
        if !identity.enrolled {
            return;
        }
        let published = self.local.values().filter(|local| local.published).count();
        let available = MAX_PUBLICATIONS_IN_FLIGHT
            .saturating_sub(self.publishing.len())
            .min(MAX_JOBS.saturating_sub(self.jobs.len()))
            .min(
                MAX_SESSIONS
                    .saturating_sub(published + self.publishing.len() + self.retiring.len()),
            );
        let ids = self
            .local
            .iter()
            .filter(|(id, local)| {
                !local.published && !local.terminal.is_closed() && !self.publishing.contains(id)
            })
            .take(available)
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        for id in ids {
            let (info, host_requests, output) = self.local[&id].publication(id);
            let network = self.network.clone();
            let generation = self.scope.network_generation;
            self.publishing.insert(id);
            self.spawn(async move {
                let incarnation = info.incarnation_id;
                let result = if network.generation() == generation {
                    network
                        .publish(info, host_requests, output)
                        .await
                        .map_err(Error::from)
                } else {
                    Err(Error::Stale)
                };
                Completion::Published {
                    id,
                    incarnation,
                    result,
                }
            });
        }
    }

    fn retire_publications(&mut self) {
        let available = MAX_JOBS.saturating_sub(self.jobs.len());
        let ids = self
            .retiring
            .iter()
            .filter(|id| !self.unpublishing.contains(id))
            .take(available)
            .copied()
            .collect::<Vec<_>>();
        for id in ids {
            let network = self.network.clone();
            let generation = self.scope.network_generation;
            self.unpublishing.insert(id);
            self.spawn(async move {
                let result = if network.generation() == generation {
                    network.unpublish(id).await.map_err(Error::from)
                } else {
                    Err(Error::Stale)
                };
                Completion::Unpublished { id, result }
            });
        }
    }
}
